use crate::envres::profiles_root;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct ProfileInfo {
    pub name: String,
    pub bundles: Vec<String>,
    pub dependencies: serde_json::Map<String, serde_json::Value>,
    pub path: String,
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 32 {
        return Err("版本名长度需在 1-32 之间".into());
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("版本名只能包含英文字母、数字、- 和 _".into());
    }
    if n.eq_ignore_ascii_case("node_modules") {
        return Err("该名称不可用".into());
    }
    Ok(())
}

fn profile_dir(name: &str) -> PathBuf {
    profiles_root().join(name)
}

pub fn read_profile(dir: &PathBuf) -> Option<ProfileInfo> {
    let pkg_path = dir.join("package.json");
    let raw = std::fs::read(&pkg_path).ok()?;
    let pkg: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let bundles = pkg["dsh"]["profile"]["bundles"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let dependencies = pkg["dependencies"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    Some(ProfileInfo {
        name: dir.file_name()?.to_string_lossy().to_string(),
        bundles,
        dependencies,
        path: dir.to_string_lossy().to_string(),
    })
}

pub fn list() -> Result<Vec<ProfileInfo>, String> {
    let root = profiles_root();
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            if let Some(name) = p.file_name().map(|s| s.to_string_lossy().to_string()) {
                if name == "node_modules" || name.starts_with('.') {
                    continue;
                }
                if let Some(info) = read_profile(&p) {
                    out.push(info);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn write_file(path: &PathBuf, content: &str) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| e.to_string())
}

/// 新建 profile：web 应用模板（dsh-base + dsh-web-app），依赖在首次启动/插件操作时由 dsh 自动初始化
pub fn create(name: &str) -> Result<(), String> {
    validate_name(name)?;
    let dir = profile_dir(name.trim());
    if dir.exists() {
        return Err(format!("版本 {name} 已存在"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let pkg = serde_json::json!({
        "name": format!("dsh-profile-{}", name.trim()),
        "private": true,
        "dependencies": {},
        "dsh": {
            "profile": {
                "bundles": ["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"]
            }
        }
    });
    write_file(
        &dir.join("package.json"),
        &serde_json::to_string_pretty(&pkg).map_err(|e| e.to_string())?,
    )?;
    write_file(&dir.join("cordis.yml"), "[]\n")?;
    write_file(
        &dir.join("cordis.patch.yml"),
        "# 你的补丁层：dsh 会把它应用在所有 bundle 层之后。\n[]\n",
    )?;
    write_file(
        &dir.join("pnpm-workspace.yaml"),
        "packages:\n  - .\nnodeLinker: hoisted\nautoInstallPeers: false\n",
    )?;
    Ok(())
}

pub fn copy(src: &str, dst: &str) -> Result<(), String> {
    validate_name(dst)?;
    let from = profile_dir(src);
    let to = profile_dir(dst.trim());
    if !from.exists() {
        return Err(format!("版本 {src} 不存在"));
    }
    if to.exists() {
        return Err(format!("版本 {dst} 已存在"));
    }
    copy_dir_recursive(&from, &to)?;
    Ok(())
}

pub fn rename(src: &str, dst: &str) -> Result<(), String> {
    validate_name(dst)?;
    let from = profile_dir(src);
    let to = profile_dir(dst.trim());
    if !from.exists() {
        return Err(format!("版本 {src} 不存在"));
    }
    if to.exists() {
        return Err(format!("版本 {dst} 已存在"));
    }
    std::fs::rename(&from, &to).map_err(|e| e.to_string())?;
    // package.json 里的 name 同步改掉
    let pkg_path = to.join("package.json");
    if let Ok(raw) = std::fs::read(&pkg_path) {
        if let Ok(mut pkg) = serde_json::from_slice::<serde_json::Value>(&raw) {
            pkg["name"] = serde_json::Value::String(format!("dsh-profile-{}", dst.trim()));
            let _ = std::fs::write(
                &pkg_path,
                serde_json::to_string_pretty(&pkg).unwrap_or_else(|_| "{}".into()),
            );
        }
    }
    Ok(())
}

pub fn delete(name: &str) -> Result<(), String> {
    let dir = profile_dir(name);
    if !dir.exists() {
        return Err(format!("版本 {name} 不存在"));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

fn copy_dir_recursive(from: &PathBuf, to: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        let dest = to.join(&name);
        if p.is_dir() {
            // profile 里的 node_modules 不复制（pnpm 依赖树，重装即可）
            if name == "node_modules" {
                continue;
            }
            copy_dir_recursive(&p, &dest)?;
        } else {
            std::fs::copy(&p, &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
