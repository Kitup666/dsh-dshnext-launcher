use crate::core::envres::{child_path, dsh_cmd, dsh_prefix, profiles_root};
use crate::core::event::{log, EventSink, LogStream};
use crate::core::store::Config;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    /// 是否已在用户补丁层 (`cordis.patch.yml`) 被禁用。
    pub disabled: bool,
}

/// 随附的 in-box bundle：由 dsh 安装本体提供，不是用户可装卸的社区插件。
/// 与 dsh-market 的 `INBOX_BUNDLES` 保持一致（/dsh-market/src/profile.ts）。
pub const INBOX_BUNDLES: &[&str] = &[
    "@deepseek-ai/dsh-base",
    "@deepseek-ai/dsh-web-app",
    "@deepseek-ai/dsh-headless",
];

/// 默认策展插件目录：awesome-dsh-plugin 的 plugins.json（与 dsh-market 同源）。
pub const DEFAULT_CATALOG_URL: &str = "https://awesome-dsh-plugin.com/plugins.json";

/// 离线诊断出的一处可能导致 harness 启动失败的问题。
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    /// "error" = 下次启动会失败；"warn" = 可疑但未必失败。
    pub severity: String,
    /// 关联的包名（卸载按钮据此定位）。
    pub package: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketItem {
    pub name: String,
    pub source: String,
    pub description: String,
    pub stars: i64,
    pub version: String,
    pub origin: String, // "npm" | "catalog"
    /// 浏览器里打开的插件页面（策展页优先，否则 npm / GitHub）。
    pub page: String,
}

pub fn list(profile: &str) -> Result<Vec<PluginInfo>, String> {
    let pkg_path = profiles_root().join(profile).join("package.json");
    let raw = std::fs::read(&pkg_path)
        .map_err(|_| format!("版本 {profile} 不存在或缺少 package.json"))?;
    let pkg: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let dir = profile_dir(profile);
    let patch_state = crate::core::patch::read_state(&dir.join("cordis.patch.yml"));
    if let Some(deps) = pkg["dependencies"].as_object() {
        for (name, ver) in deps {
            // in-box bundle 由 dsh 安装本体提供，不算用户插件（对齐 dsh-market）。
            if INBOX_BUNDLES.contains(&name.as_str()) {
                continue;
            }
            let disabled = crate::core::patch::row_ids_for_package(&dir, name)
                .iter()
                .any(|id| patch_state.disables.contains(id));
            out.push(PluginInfo {
                name: name.clone(),
                version: ver.as_str().unwrap_or("").to_string(),
                disabled,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn profile_dir(profile: &str) -> PathBuf {
    profiles_root().join(profile)
}

/// 解析 dsh 安装目录里的包目录（in-box bundle 在这里，不在 profile node_modules）。
/// 兼容两种 pnpm 布局：提升到安装根，或嵌在 `@deepseek-ai/dsh` 之下。
fn install_package_dir(name: &str) -> Option<PathBuf> {
    let root = dsh_prefix().join("node_modules");
    let mut candidates = vec![root.join(name)];
    if let Some((scope, _)) = name.split_once('/') {
        candidates.push(root.join(scope).join("dsh").join("node_modules").join(name));
    }
    candidates.into_iter().find(|p| p.join("package.json").exists())
}

fn read_package_json(dir: &Path) -> Option<serde_json::Value> {
    let raw = std::fs::read(dir.join("package.json")).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// 包是否声明了 dsh 相关信息：任何 `dsh` 字段，或自带 `cordis.patch.yml`。
/// 宽松判定，只用于诊断里的“可能不是插件”警告——卸载判断另用 bundle 归属。
fn has_dsh_surface(dir: &Path) -> bool {
    if dir.join("cordis.patch.yml").exists() {
        return true;
    }
    read_package_json(dir)
        .map(|v| v.get("dsh").is_some())
        .unwrap_or(false)
}

/// profile 的 `dsh.profile.bundles` 列表。
fn profile_bundle_names(dir: &Path) -> Vec<String> {
    read_package_json(dir)
        .and_then(|v| {
            v.get("dsh")
                .and_then(|d| d.get("profile"))
                .and_then(|p| p.get("bundles"))
                .and_then(|b| b.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|b| b.as_str().map(str::to_string))
                        .collect()
                })
        })
        .unwrap_or_default()
}

/// 包声明的 bundle patch 文件（存在才返回）。
pub(crate) fn bundle_patch_path(dir: &Path) -> Option<PathBuf> {
    let v = read_package_json(dir)?;
    let rel = v.get("dsh")?.get("bundle")?.get("patch")?.as_str()?;
    let path = dir.join(rel);
    path.exists().then_some(path)
}

/// 已声明的依赖名集合（读 profile package.json）。
fn installed_names(profile: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Some(v) = read_package_json(&profile_dir(profile)) {
        if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
            for key in deps.keys() {
                set.insert(key.clone());
            }
        }
    }
    set
}

/// 从一个 patch 文件里抽出 `insert:` 块中定义的 loader entry id。
///
/// 只统计缩进在 `insert:` 之下的列表项，顶层 `- id:` 是 patch 行（覆盖已有条目），
/// 不算新条目。这是够用的行级扫描，不引 YAML 依赖；嵌套 group 的 config 子项也会计入。
pub(crate) fn insert_entry_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut insert_indents: Vec<usize> = Vec::new();
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        while let Some(&top) = insert_indents.last() {
            if indent <= top {
                insert_indents.pop();
            } else {
                break;
            }
        }
        let trimmed = raw.trim_start();
        let content = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let content_indent = indent + (trimmed.len() - content.len());
        if content.starts_with("insert:") {
            insert_indents.push(content_indent);
            continue;
        }
        if insert_indents.is_empty() {
            continue;
        }
        if let Some(value) = content.strip_prefix("id:") {
            let id = value.trim().trim_matches(|c| c == '\'' || c == '"');
            if !id.is_empty() {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// 收集一个包 patch 里 insert 的 entry id，记到对应层名下（按包名去重）。
fn add_package_ids(
    by_id: &mut HashMap<String, Vec<String>>,
    seen: &mut HashSet<String>,
    name: &str,
    pdir: &Path,
) {
    if !seen.insert(name.to_string()) {
        return;
    }
    if let Some(patch) = bundle_patch_path(pdir) {
        if let Ok(text) = std::fs::read_to_string(&patch) {
            for id in insert_entry_ids(&text) {
                by_id.entry(id).or_default().push(name.to_string());
            }
        }
    }
}

/// 每个 entry id 分别由哪些层（bundle 包名 / "user patch"）定义。
/// bundle 既解析 profile node_modules（社区），也解析 dsh 安装目录（in-box），
/// 这样社区插件与核心（如 `storage`）的 id 冲突也能被抓到。
fn entry_id_layers(dir: &Path) -> HashMap<String, Vec<String>> {
    let mut by_id: HashMap<String, Vec<String>> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    if let Some(v) = read_package_json(dir) {
        if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
            for name in deps.keys() {
                let pdir = dir.join("node_modules").join(name);
                if pdir.join("package.json").exists() {
                    add_package_ids(&mut by_id, &mut seen, name, &pdir);
                } else if let Some(installed) = install_package_dir(name) {
                    add_package_ids(&mut by_id, &mut seen, name, &installed);
                }
            }
        }
    }
    for name in profile_bundle_names(dir) {
        let pdir = dir.join("node_modules").join(&name);
        if pdir.join("package.json").exists() {
            add_package_ids(&mut by_id, &mut seen, &name, &pdir);
        } else if let Some(installed) = install_package_dir(&name) {
            add_package_ids(&mut by_id, &mut seen, &name, &installed);
        }
    }

    if let Ok(text) = std::fs::read_to_string(dir.join("cordis.patch.yml")) {
        for id in insert_entry_ids(&text) {
            by_id.entry(id).or_default().push("user patch".to_string());
        }
    }
    by_id
}

/// 离线扫描一个 profile，找出会导致 harness 下次启动失败的问题。
/// 不需要 harness 在跑——这正是它在崩溃打不开时还能用的原因。
pub fn diagnose(profile: &str) -> Result<Vec<Problem>, String> {
    let dir = profile_dir(profile);
    if !dir.join("package.json").exists() {
        return Err(format!("版本 {profile} 不存在或缺少 package.json"));
    }
    let mut out: Vec<Problem> = Vec::new();

    // 1) 重复 loader entry id：cordis 会直接拒绝加载整棵树。
    for (id, layers) in entry_id_layers(&dir) {
        if layers.len() < 2 {
            continue;
        }
        let mut uniq: Vec<String> = Vec::new();
        for layer in &layers {
            if !uniq.contains(layer) {
                uniq.push(layer.clone());
            }
        }
        out.push(Problem {
            severity: "error".into(),
            package: uniq.first().cloned().unwrap_or_default(),
            message: format!(
                "loader entry id `{id}` 被重复定义（{}）——重复 id 会让 harness 下次启动直接失败",
                uniq.join("、")
            ),
        });
    }

    // 2) 声明了但装不出来 / 不是插件的依赖；3) bundle 行指向未安装的包。
    if let Some(v) = read_package_json(&dir) {
        if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
            for name in deps.keys() {
                if INBOX_BUNDLES.contains(&name.as_str()) {
                    continue;
                }
                let pdir = dir.join("node_modules").join(name);
                if !pdir.exists() {
                    out.push(Problem {
                        severity: "error".into(),
                        package: name.clone(),
                        message: format!(
                            "依赖 {name} 已声明但未安装——bundle 行无法解析，harness 会启动失败；请重装或卸载"
                        ),
                    });
                } else if !has_dsh_surface(&pdir) {
                    out.push(Problem {
                        severity: "warn".into(),
                        package: name.clone(),
                        message: format!(
                            "{name} 没有 dsh 插件声明（dsh.bundle / dsh.client / cordis.patch.yml），可能不是可加载插件"
                        ),
                    });
                }
            }
        }
        if let Some(bundles) = v
            .get("dsh")
            .and_then(|d| d.get("profile"))
            .and_then(|p| p.get("bundles"))
            .and_then(|b| b.as_array())
        {
            for b in bundles {
                let Some(name) = b.as_str() else { continue };
                if INBOX_BUNDLES.contains(&name) {
                    continue;
                }
                if !dir.join("node_modules").join(name).exists() {
                    out.push(Problem {
                        severity: "error".into(),
                        package: name.to_string(),
                        message: format!(
                            "bundle {name} 列在 dsh.profile.bundles 里但未安装——loader 会在这一行失败"
                        ),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// 一次 `dsh plugin` 运行的结果。
struct OpRun {
    ok: bool,
    output: String,
}

/// 跑 `dsh plugin --profile <name> <args...>`，输出按行转发为 Log 事件并汇总返回。
async fn run_plugin(tx: EventSink, profile: &str, args: &[String]) -> Result<OpRun, String> {
    let dsh = dsh_cmd();
    if !dsh.exists() {
        return Err("dsh 未安装，请先到「环境」页安装".into());
    }
    let cwd = profiles_root().join(profile);
    if !cwd.exists() {
        return Err(format!("版本 {profile} 目录不存在"));
    }
    let mut cmd = Command::new("cmd");
    cmd.arg("/C")
        .arg(&dsh)
        .arg("plugin")
        .arg("--profile")
        .arg(profile);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(&cwd)
        .env("PATH", child_path())
        .env("DSH_HOME", crate::core::envres::home_dir())
        // pnpm 无 TTY 时会中止删除 modules 目录，必须显式 CI。
        .env("CI", "true")
        // 固定 store 到软件数据目录（pnpm 默认按项目所在盘选，会跑到 D:\.pnpm-store）。
        .env("npm_config_store_dir", crate::core::envres::pnpm_store_dir())
        .creation_flags(0x0800_0000)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("启动 dsh plugin 失败: {e}"))?;
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();

    let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    let tx2 = tx.clone();
    let profile_out = profile.to_string();
    let cap_out = captured.clone();
    let out_task = tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(&mut out);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(mut buf) = cap_out.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
            log(&tx2, &profile_out, LogStream::Plugin, line);
        }
    });
    let profile_err = profile.to_string();
    let cap_err = captured.clone();
    let err_task = tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let reader = tokio::io::BufReader::new(&mut err);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(mut buf) = cap_err.lock() {
                buf.push_str(&line);
                buf.push('\n');
            }
            log(&tx, &profile_err, LogStream::Plugin, line);
        }
    });
    let _ = out_task.await;
    let _ = err_task.await;
    let status = child.wait().await.map_err(|e| e.to_string())?;
    let output = captured
        .lock()
        .map(|buf| buf.clone())
        .unwrap_or_default();
    Ok(OpRun {
        ok: status.success(),
        output,
    })
}

/// 输出尾部若干行，供 toast 显示。
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(6);
    lines[start..].join("\n")
}

/// 跑一次插件操作；遇到 pnpm store 位置不一致时，自动 `pnpm install` 重链一次再重试。
async fn plugin_op(tx: EventSink, profile: &str, args: &[String]) -> Result<(), String> {
    let mut run = run_plugin(tx.clone(), profile, args).await?;
    if !run.ok && run.output.contains("ERR_PNPM_UNEXPECTED_STORE") {
        log(
            &tx,
            profile,
            LogStream::Plugin,
            "pnpm store 位置不一致，重链一次后重试",
        );
        let relink = run_plugin(
            tx.clone(),
            profile,
            &["install".to_string(), "--no-frozen-lockfile".to_string()],
        )
        .await?;
        if relink.ok {
            run = run_plugin(tx.clone(), profile, args).await?;
        } else {
            return Err(format!("pnpm install 重链失败：{}", tail(&relink.output)));
        }
    }
    if run.ok {
        Ok(())
    } else {
        Err(format!("dsh plugin 失败：{}", tail(&run.output)))
    }
}

pub async fn add(tx: EventSink, _cfg: &Config, profile: &str, source: &str) -> Result<(), String> {
    if source.trim().is_empty() {
        return Err("插件源不能为空".into());
    }
    // 装完立刻校验，坏插件当场回滚——这正是「装插件把 harness 装崩」的预防。
    let before = installed_names(profile);
    plugin_op(
        tx.clone(),
        profile,
        &["add".to_string(), source.trim().to_string()],
    )
    .await?;
    let removed = validate_added(tx, profile, &before).await;
    if !removed.is_empty() {
        return Err(format!(
            "插件会破坏 harness 启动，已自动回滚：{}",
            removed.join(", ")
        ));
    }
    Ok(())
}

pub async fn remove(tx: EventSink, _cfg: &Config, profile: &str, name: &str) -> Result<(), String> {
    // 卸载前先记下自己拥有的补丁行 id（包删掉后就查不到了），成功后清理，
    // 避免用户在补丁层的禁用/启用块变成孤儿行。
    let dir = profile_dir(profile);
    let ids = crate::core::patch::row_ids_for_package(&dir, name.trim());
    plugin_op(
        tx,
        profile,
        &["remove".to_string(), name.trim().to_string()],
    )
    .await?;
    crate::core::patch::remove_rows(&dir.join("cordis.patch.yml"), &ids);
    Ok(())
}

/// 校验本次 `add` 新增的包（假成功守卫，对齐 dsh-market 的 validateAddedPlugins）：
/// - 被加进 `dsh.profile.bundles`、却拿不出可加载 bundle patch 的包 —— 下次启动会在这一行失败，当场卸载；
/// - 引入了重复 loader entry id 的包 —— 下次启动直接崩，当场卸载。
/// 不在 bundle 列表里的纯函数插件保持原样（inert，不会让启动失败）。
/// @returns 被回滚的包名，空表示全通过。
async fn validate_added(tx: EventSink, profile: &str, before: &HashSet<String>) -> Vec<String> {
    let dir = profile_dir(profile);
    let added: Vec<String> = installed_names(profile)
        .into_iter()
        .filter(|name| !before.contains(name))
        .collect();
    let bundles = profile_bundle_names(&dir);
    let mut removed: Vec<String> = Vec::new();

    for name in &added {
        if !bundles.iter().any(|b| b == name) {
            continue; // 不是 bundle，不参与启动，无需回滚
        }
        let pdir = dir.join("node_modules").join(name);
        if !pdir.exists() || bundle_patch_path(&pdir).is_none() {
            log(
                &tx,
                profile,
                LogStream::Plugin,
                format!("{name} 被列为 bundle 但没有可加载的 bundle patch，回滚"),
            );
            if plugin_op(
                tx.clone(),
                profile,
                &["remove".to_string(), name.clone()],
            )
            .await
            .is_ok()
            {
                removed.push(name.clone());
            }
        }
    }

    let added_set: HashSet<String> = added.iter().cloned().collect();
    for (_id, layers) in entry_id_layers(&dir) {
        if layers.len() < 2 {
            continue;
        }
        for name in layers.iter().filter(|l| added_set.contains(*l)) {
            if removed.contains(name) {
                continue;
            }
            log(
                &tx,
                profile,
                LogStream::Plugin,
                format!("{name} 引入了重复 loader entry id，回滚（否则下次启动会失败）"),
            );
            if plugin_op(
                tx.clone(),
                profile,
                &["remove".to_string(), name.clone()],
            )
            .await
            .is_ok()
            {
                removed.push(name.clone());
            }
        }
    }
    removed
}

/// npm registry 的 keywords:dsh-plugin 搜索。比 GitHub topic 精确得多：
/// 结果就是能直接 `dsh plugin add` 的包，而 topic 搜索会被只打了标签的大仓库淹没。
///
/// 配置的镜像（npmmirror 等）对 `keywords:dsh-plugin` 的搜索索引可能为空——此时
/// 回落官方 npm 重试一次。市场检索用官方源、插件安装仍用配置的源，互不影响。
async fn npm_search_items(cfg: &Config) -> Result<Vec<MarketItem>, String> {
    let base = crate::core::envres::registry_url(cfg);
    let first = npm_search_at(&base).await;
    if needs_official_fallback(&base, &first) {
        return npm_search_at(crate::core::installs::OFFICIAL_REGISTRY).await;
    }
    first
}

/// 该源的结果是否不足以代表市场（请求失败，或搜到 0 条）且值得回落官方重试。
fn needs_official_fallback(base: &str, result: &Result<Vec<MarketItem>, String>) -> bool {
    base != crate::core::installs::OFFICIAL_REGISTRY
        && !matches!(result, Ok(items) if !items.is_empty())
}

/// 对单个 registry base 跑一次 keywords:dsh-plugin 搜索。
async fn npm_search_at(base: &str) -> Result<Vec<MarketItem>, String> {
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
            out.push(MarketItem {
                source: name.clone(),
                page: default_page(&name),
                name,
                description: pkg["description"].as_str().unwrap_or("").to_string(),
                // npm 搜索没有星星数据；星星只来自策展目录。
                stars: 0,
                version: pkg["version"].as_str().unwrap_or("").to_string(),
                origin: "npm".into(),
            });
        }
    }
    Ok(out)
}

/// 合法 npm 包名（粗判，交给 pnpm 最终校验）。
fn valid_npm_name(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(|c| c.is_whitespace() || matches!(c, ':' | '#'))
}

/// 安装目标对应的可浏览页面：GitHub 源用仓库页，其余当 npm 包名用 npm 页。
fn default_page(source: &str) -> String {
    if let Some(rest) = source.strip_prefix("github:") {
        let repo = rest.split('#').next().unwrap_or(rest);
        format!("https://github.com/{repo}")
    } else {
        format!("https://www.npmjs.com/package/{source}")
    }
}

/// 从 GitHub URL 解析安装目标 `github:owner/repo[#path:/sub]`。
/// 与 dsh-market 的 parseSourceUrl 同口径（支持 `/tree/<branch>/<subpath>`）。
fn parse_github_source(url: &str) -> Option<String> {
    let rest = url.trim().strip_prefix("https://github.com/")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let segs: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        return None;
    }
    let owner = segs[0];
    let repo = segs[1].trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    if segs.len() > 3 && segs[2] == "tree" {
        let sub = segs[4..].join("/");
        if !sub.is_empty() && !sub.split('/').any(|s| s == ".." || s == "." || s.is_empty()) {
            return Some(format!("github:{owner}/{repo}#path:/{sub}"));
        }
    }
    Some(format!("github:{owner}/{repo}"))
}

/// 作者提供的 GitHub Release tarball，必须绑定到条目自己的 repo（防名字抢注）。
fn release_tarball_target(value: &str, repo: &str) -> Option<String> {
    let rest = value.trim().strip_prefix("https://github.com/")?;
    let path = rest.split(['?', '#']).next().unwrap_or(rest);
    if !(path.ends_with(".tgz") || path.ends_with(".tar.gz")) {
        return None;
    }
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 4 || segs[2] != "releases" {
        return None;
    }
    let bound = format!("{}/{}", segs[0].to_lowercase(), segs[1].to_lowercase());
    (bound == repo.to_lowercase()).then(|| value.trim().to_string())
}

/// 目录条目的描述，中文优先。
fn pick_description(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if v.is_object() {
        for key in ["zh", "en"] {
            if let Some(s) = v[key].as_str() {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    String::new()
}

/// 策展条目 → 安装目标：npm 包 > repo 绑定的 Release tarball > GitHub 源码。
/// 对齐 dsh-market 的 installTargetFor。
fn curated_install_target(it: &serde_json::Value) -> Option<String> {
    if let Some(npm) = it["npm"].as_str() {
        if valid_npm_name(npm) {
            return Some(npm.to_string());
        }
    }
    let source = parse_github_source(it["url"].as_str()?)?;
    if let Some(tarball) = it["tarball"].as_str() {
        let repo = source
            .trim_start_matches("github:")
            .split('#')
            .next()
            .unwrap_or("");
        if let Some(target) = release_tarball_target(tarball, repo) {
            return Some(target);
        }
    }
    Some(source)
}

/// 可选的插件商店 catalog.json：同时支持策展目录（awesome-dsh-plugin 形状，
/// 条目带 `url`/`npm`/`tarball`）与旧格式（`repo`/`github`）。
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
        let Some(name) = it["name"].as_str() else { continue };
        if name.is_empty() {
            continue;
        }
        // 已废弃条目不展示（对齐 dsh-market 的 deprecated 处理）。
        if it["deprecated"].as_bool() == Some(true) {
            continue;
        }
        // 策展形状优先。
        if let Some(source) = curated_install_target(it) {
            let page = it["page"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| default_page(&source));
            out.push(MarketItem {
                name: name.to_string(),
                source,
                description: pick_description(&it["description"]),
                stars: it["stars"].as_i64().unwrap_or(0),
                version: it["version"].as_str().unwrap_or("").to_string(),
                origin: "catalog".into(),
                page,
            });
            continue;
        }
        // 旧格式：name/repo/github。
        let repo = it["repo"]
            .as_str()
            .or_else(|| it["github"].as_str())
            .unwrap_or(name);
        let source = if repo.starts_with("github:") || !repo.contains('/') {
            repo.to_string()
        } else {
            format!("github:{repo}")
        };
        let page = it["page"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_page(&source));
        out.push(MarketItem {
            source,
            name: name.to_string(),
            description: it["description"].as_str().unwrap_or("").to_string(),
            stars: it["stars"].as_i64().unwrap_or(0),
            version: it["version"].as_str().unwrap_or("").to_string(),
            origin: "catalog".into(),
            page,
        });
    }
    Ok(out)
}

pub async fn market(cfg: &Config) -> Result<Vec<MarketItem>, String> {
    let mut items: Vec<MarketItem> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    // 没配自定义目录就用策展默认源（对齐 dsh-market）。
    let custom = cfg.plugin_catalog_url.trim();
    let catalog_url = if custom.is_empty() {
        DEFAULT_CATALOG_URL
    } else {
        custom
    };
    match catalog_items(catalog_url).await {
        Ok(v) => items.extend(v),
        Err(e) => errors.push(format!("插件目录: {e}")),
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

#[cfg(test)]
mod tests {
    use super::{
        curated_install_target, insert_entry_ids, needs_official_fallback, parse_github_source,
        MarketItem,
    };

    #[test]
    fn curated_entries_map_to_install_targets() {
        let npm = serde_json::json!({ "npm": "dshmarket", "url": "https://github.com/x/y" });
        assert_eq!(curated_install_target(&npm).as_deref(), Some("dshmarket"));
        let gh = serde_json::json!({
            "npm": null,
            "url": "https://github.com/CAI-MH/dsh-quality-review"
        });
        assert_eq!(
            curated_install_target(&gh).as_deref(),
            Some("github:CAI-MH/dsh-quality-review")
        );
        assert_eq!(
            parse_github_source("https://github.com/o/r/tree/main/packages/p").as_deref(),
            Some("github:o/r#path:/packages/p")
        );
    }

    #[test]
    fn insert_entry_ids_only_counts_insert_blocks() {
        let patch = "\
- id: web
  config:
    searchProvider: deepseek-official
- insert:
    - id: alpha
      name: '@x/alpha'
    - id: beta
      name: '@x/beta'
- id: tools
  config:
    mode: native
- insert:
    - id: gamma
      name: '@x/gamma'
      group: true
      config:
        - id: child
          name: '@x/child'
";
        let ids = insert_entry_ids(patch);
        // 顶层 `- id: web` / `- id: tools` 是 patch 行，不算；insert 块内及 group 子项算。
        assert_eq!(ids, vec!["alpha", "beta", "gamma", "child"]);
    }

    fn item() -> MarketItem {
        MarketItem {
            name: "x".into(),
            source: "x".into(),
            description: String::new(),
            stars: 0,
            version: String::new(),
            origin: "npm".into(),
            page: String::new(),
        }
    }

    #[test]
    fn mirror_yielding_nothing_falls_back_to_official() {
        let mirror = "https://registry.npmmirror.com";
        let empty: Result<Vec<MarketItem>, String> = Ok(Vec::new());
        let failed: Result<Vec<MarketItem>, String> = Err("boom".into());
        assert!(needs_official_fallback(mirror, &empty));
        assert!(needs_official_fallback(mirror, &failed));
        assert!(!needs_official_fallback(mirror, &Ok(vec![item()])));
        // 官方源本身就是终点，不再回落
        assert!(!needs_official_fallback(
            crate::core::installs::OFFICIAL_REGISTRY,
            &empty
        ));
    }
}
