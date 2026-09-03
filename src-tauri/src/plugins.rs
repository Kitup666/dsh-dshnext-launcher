use crate::envres::{child_path, dsh_cmd, profiles_root};
use crate::store::Config;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketItem {
    pub name: String,
    pub source: String,
    pub description: String,
    pub stars: i64,
    pub version: String,
    pub origin: String, // "npm" | "catalog"
}

pub fn list(profile: &str) -> Result<Vec<PluginInfo>, String> {
    let pkg_path = profiles_root().join(profile).join("package.json");
    let raw = std::fs::read(&pkg_path)
        .map_err(|_| format!("版本 {profile} 不存在或缺少 package.json"))?;
    let pkg: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(deps) = pkg["dependencies"].as_object() {
        for (name, ver) in deps {
            out.push(PluginInfo {
                name: name.clone(),
                version: ver.as_str().unwrap_or("").to_string(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 跑 `dsh plugin --profile <name> <op> <source>`，输出按行转发为 dsh-log 事件（stream="plugin"）
async fn plugin_op(app: AppHandle, profile: &str, op: &str, source: &str) -> Result<(), String> {
    let dsh = dsh_cmd();
    if !dsh.exists() {
        return Err("dsh 未安装，请先到「环境」页安装".into());
    }
    let cwd = profiles_root().join(profile);
    if !cwd.exists() {
        return Err(format!("版本 {profile} 目录不存在"));
    }
    let mut cmd = Command::new("cmd");
    cmd.args([
        "/C",
        &dsh.to_string_lossy(),
        "plugin",
        "--profile",
        profile,
        op,
        source,
    ])
    .current_dir(&cwd)
    .env("PATH", child_path())
    .env("DSH_HOME", crate::envres::home_dir())
    .creation_flags(0x0800_0000)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("启动 dsh plugin 失败: {e}"))?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();

    let app2 = app.clone();
    let profile_out = profile.to_string();
    let out_task = tauri::async_runtime::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(&mut out);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app2.emit(
                "dsh-log",
                serde_json::json!({ "profile": profile_out, "stream": "plugin", "line": line, "ts": 0 }),
            );
        }
    });
    let profile_err = profile.to_string();
    let err_task = tauri::async_runtime::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(&mut err);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app.emit(
                "dsh-log",
                serde_json::json!({ "profile": profile_err, "stream": "plugin", "line": line, "ts": 0 }),
            );
        }
    });
    let _ = out_task.await;
    let _ = err_task.await;
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("dsh plugin {op} 失败（退出码 {}）", status.code().unwrap_or(-1)))
    }
}

pub async fn add(app: AppHandle, _cfg: &Config, profile: &str, source: &str) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("插件源不能为空".into());
    }
    plugin_op(app, profile, "add", source.trim()).await
}

pub async fn remove(app: AppHandle, _cfg: &Config, profile: &str, name: &str) -> Result<(), String> {
    plugin_op(app, profile, "remove", name.trim()).await
}

/// npm registry 的 keywords:dsh-plugin 搜索。比 GitHub topic 精确得多：
/// 结果就是能直接 `dsh plugin add` 的包，而 topic 搜索会被只打了标签的大仓库淹没。
async fn npm_search_items(cfg: &Config) -> Result<Vec<MarketItem>, String> {
    let base = crate::envres::registry_url(cfg);
    let url = format!("{base}/-/v1/search?text=keywords:dsh-plugin&size=250");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .header("User-Agent", "DshDesk")
        .send()
        .await
        .map_err(|e| format!("请求 npm 搜索失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("npm 搜索返回 {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(objs) = body["objects"].as_array() {
        for o in objs {
            let pkg = &o["package"];
            let name = pkg["name"].as_str().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            // 搜索会带回只是提到关键词的包，这里只留真的声明了 dsh-plugin 关键词的
            let tagged = pkg["keywords"]
                .as_array()
                .map(|k| k.iter().any(|x| x.as_str() == Some("dsh-plugin")))
                .unwrap_or(false);
            if !tagged {
                continue;
            }
            // 官方 bundle 自身不作为可安装插件展示
            if name.starts_with("@deepseek-ai/dsh-") {
                continue;
            }
            let score = (o["score"]["final"].as_f64().unwrap_or(0.0) * 1000.0) as i64;
            out.push(MarketItem {
                source: name.clone(),
                name,
                description: pkg["description"].as_str().unwrap_or("").to_string(),
                stars: score,
                version: pkg["version"].as_str().unwrap_or("").to_string(),
                origin: "npm".into(),
            });
        }
    }
    Ok(out)
}

/// 可选的插件商店 catalog.json：宽松解析（root 数组或 {plugins|items:[...]})
async fn catalog_items(url: &str) -> Result<Vec<MarketItem>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .header("User-Agent", "DshDesk-Launcher")
        .send()
        .await
        .map_err(|e| format!("请求插件商店失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("插件商店返回 {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let arr = body
        .as_array()
        .cloned()
        .or_else(|| body["plugins"].as_array().cloned())
        .or_else(|| body["items"].as_array().cloned())
        .unwrap_or_default();
    let mut out = Vec::new();
    for it in &arr {
        let name = it["name"]
            .as_str()
            .or_else(|| it["repo"].as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let repo = it["repo"]
            .as_str()
            .or_else(|| it["github"].as_str())
            .unwrap_or(&name);
        let source = if repo.starts_with("github:") || !repo.contains('/') {
            repo.to_string()
        } else {
            format!("github:{repo}")
        };
        out.push(MarketItem {
            source,
            name,
            description: it["description"].as_str().unwrap_or("").to_string(),
            stars: it["stars"].as_i64().unwrap_or(0),
            version: it["version"].as_str().unwrap_or("").to_string(),
            origin: "catalog".into(),
        });
    }
    Ok(out)
}

pub async fn market(cfg: &Config) -> Result<Vec<MarketItem>, String> {
    let mut items: Vec<MarketItem> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    if !cfg.plugin_catalog_url.trim().is_empty() {
        match catalog_items(cfg.plugin_catalog_url.trim()).await {
            Ok(v) => items.extend(v),
            Err(e) => errors.push(e),
        }
    }
    match npm_search_items(cfg).await {
        Ok(v) => items.extend(v),
        Err(e) => errors.push(e),
    }
    if items.is_empty() && !errors.is_empty() {
        return Err(errors.join("；"));
    }
    // 去重（按 source）
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.source.clone()));
    items.sort_by(|a, b| b.stars.cmp(&a.stars));
    Ok(items)
}
