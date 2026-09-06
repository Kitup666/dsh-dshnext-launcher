use crate::core::store::{data_dir, Config};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;
use tokio::process::Command;

/// 官方 dsh 包里需要放行安装脚本的原生模块清单
pub const DSH_ALLOW_SCRIPTS: &str =
    "@deepseek-ai/dsh-subprocess-local,koffi,node-pty,@google/genai,protobufjs";
pub const DEFAULT_NODE_VERSION: &str = "24.19.0";

pub fn node_dir() -> PathBuf {
    data_dir().join("runtime").join("node")
}

/// npm 全局前缀：dsh.cmd / pnpm.cmd / node_modules 都在这层
pub fn dsh_prefix() -> PathBuf {
    data_dir().join("runtime").join("dsh")
}

pub fn dsh_cmd() -> PathBuf {
    dsh_prefix().join("dsh.cmd")
}

/// dsh-home 覆盖（config.dsh_home，首次启动引导设置；None = 默认 `<数据>/home`）。
/// 进程级单次设定：启动时从 config 读一次，引导确认时改一次。
static HOME_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_home_dir(p: PathBuf) {
    *HOME_OVERRIDE.write().expect("HOME_OVERRIDE 锁") = Some(p);
}

/// 从 config 的 dsh_home 字段初始化覆盖（main 早期调用）。
pub fn init_home_from_config(cfg_dsh_home: &str) {
    let s = cfg_dsh_home.trim();
    if !s.is_empty() {
        set_home_dir(PathBuf::from(s));
    }
}

pub fn home_dir() -> PathBuf {
    if let Ok(g) = HOME_OVERRIDE.read() {
        if let Some(p) = g.as_ref() {
            return p.clone();
        }
    }
    data_dir().join("home")
}

pub fn profiles_root() -> PathBuf {
    home_dir().join("profiles")
}

/// 便携 node + 托管 dsh 前置到 PATH 的子进程环境
pub fn child_path() -> String {
    let mut parts: Vec<String> = Vec::new();
    if node_dir().join("node.exe").exists() {
        parts.push(node_dir().to_string_lossy().to_string());
    }
    if dsh_prefix().exists() {
        parts.push(dsh_prefix().to_string_lossy().to_string());
    }
    let system = std::env::var("PATH").unwrap_or_default();
    parts.push(system);
    parts.join(";")
}

/// 跑一个只读的 `xx --version` 探测，返回首行输出
async fn probe(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", program])
        .args(args)
        .env("PATH", child_path())
        .creation_flags(0x0800_0000);
    let out = tokio::time::timeout(Duration::from_secs(20), cmd.output()).await;
    match out {
        Ok(Ok(o)) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let line = text.lines().next().unwrap_or("").trim().to_string();
            if line.is_empty() {
                None
            } else {
                Some(line)
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvStatus {
    pub node_version: Option<String>,
    pub node_path: Option<String>,
    pub node_managed: bool,
    pub pnpm_version: Option<String>,
    pub dsh_version: Option<String>,
    pub dsh_path: Option<String>,
    pub data_dir: String,
    pub home_dir: String,
}

pub async fn status(_cfg: &Config) -> EnvStatus {
    let managed_node = node_dir().join("node.exe");
    let dsh = dsh_cmd();

    let node_version = probe("node", &["--version"]).await;
    let node_path = if node_version.is_some() {
        Some(if managed_node.exists() {
            managed_node.to_string_lossy().to_string()
        } else {
            "（系统 PATH）".into()
        })
    } else {
        None
    };
    let node_managed = managed_node.exists();

    let pnpm_version = probe("pnpm.cmd", &["--version"]).await;
    let dsh_version = if dsh.exists() {
        probe(
            &dsh.to_string_lossy(),
            &["--version"],
        )
        .await
    } else {
        None
    };

    EnvStatus {
        node_version,
        node_path,
        node_managed,
        pnpm_version,
        dsh_version: dsh_version.clone(),
        dsh_path: if dsh.exists() {
            Some(dsh.to_string_lossy().to_string())
        } else {
            None
        },
        data_dir: data_dir().to_string_lossy().to_string(),
        home_dir: home_dir().to_string_lossy().to_string(),
    }
}

pub fn registry_url(cfg: &Config) -> String {
    let r = cfg.npm_registry.trim().trim_end_matches('/').to_string();
    if r.is_empty() {
        "https://registry.npmjs.org".into()
    } else {
        r
    }
}

pub fn node_dist_base(cfg: &Config) -> String {
    let m = cfg.node_mirror.trim().trim_end_matches('/').to_string();
    if m.is_empty() {
        "https://nodejs.org/dist".into()
    } else {
        m
    }
}

/// 版本号比较：按数字段降序，预发布（-rc.x）排在同版本正式版之后展示时靠后
pub fn cmp_version_desc(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(v: &str) -> Vec<(u64, String)> {
        let (main, pre) = match v.split_once('-') {
            Some((m, p)) => (m, Some(p)),
            None => (v, None),
        };
        let mut k: Vec<(u64, String)> = main
            .split('.')
            .map(|s| (s.parse::<u64>().unwrap_or(0), String::new()))
            .collect();
        // 正式版 > 预发布：给正式版加一个尾段 (1,"")
        k.push(match pre {
            None => (1, String::new()),
            Some(p) => (0, p.to_string()),
        });
        k
    }
    key(b).cmp(&key(a))
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeRelease {
    pub version: String,
    pub lts: Option<String>,
}

/// Node 官方 dist 的 index.json：取每条大版本线的最新一版（含 LTS 标记）
pub async fn fetch_node_versions(cfg: &Config) -> Result<Vec<NodeRelease>, String> {
    let url = format!("{}/index.json", node_dist_base(cfg));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .header("User-Agent", "DshDesk")
        .send()
        .await
        .map_err(|e| format!("请求 Node 版本列表失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Node 版本列表返回 {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let arr = body.as_array().cloned().unwrap_or_default();
    let mut out: Vec<NodeRelease> = Vec::new();
    let mut seen_major: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for item in &arr {
        let version = item["version"].as_str().unwrap_or("").trim_start_matches('v');
        if version.is_empty() {
            continue;
        }
        // 只保留提供 Windows 二进制的版本
        let has_win = item["files"]
            .as_array()
            .map(|f| f.iter().any(|x| x.as_str().is_some_and(|s| s.starts_with("win-x64"))))
            .unwrap_or(false);
        if !has_win {
            continue;
        }
        let major: u64 = version.split('.').next().unwrap_or("0").parse().unwrap_or(0);
        if major < 20 || !seen_major.insert(major) {
            continue;
        }
        out.push(NodeRelease {
            version: version.to_string(),
            lts: item["lts"].as_str().map(|s| s.to_string()),
        });
    }
    out.sort_by(|a, b| cmp_version_desc(&a.version, &b.version));
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct DshVersions {
    pub latest: Option<String>,
    pub versions: Vec<String>,
}

pub async fn fetch_dsh_versions(cfg: &Config, include_rc: bool) -> Result<DshVersions, String> {
    let url = format!("{}/@deepseek-ai%2Fdsh", registry_url(cfg));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&url)
        .header("User-Agent", "DshDesk")
        .send()
        .await
        .map_err(|e| format!("请求 npm registry 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("npm registry 返回 {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let latest = body["dist-tags"]["latest"]
        .as_str()
        .map(|s| s.to_string());
    let mut versions: Vec<String> = body["versions"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    if !include_rc {
        versions.retain(|v| !v.contains('-'));
    }
    versions.sort_by(|a, b| cmp_version_desc(a, b));
    Ok(DshVersions { latest, versions })
}
