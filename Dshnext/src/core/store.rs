use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api_key: String,
    pub port: u16,
    /// Node.js 便携版下载镜像，空串 = 官方 https://nodejs.org/dist
    pub node_mirror: String,
    /// npm registry，空串 = 官方 https://registry.npmjs.org
    pub npm_registry: String,
    /// DSH 插件商店 catalog.json 地址，空串 = 不启用
    pub plugin_catalog_url: String,
    /// 启动后 WebUI 呈现方式："window" | "browser"
    pub open_mode: String,
    /// 启动成功后是否自动打开 WebUI
    pub auto_open: bool,
    /// 界面主题："light" | "dark"
    pub theme: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            port: 3080,
            node_mirror: "https://npmmirror.com/mirrors/node".into(),
            npm_registry: String::new(),
            plugin_catalog_url: String::new(),
            open_mode: "window".into(),
            auto_open: true,
            theme: "light".into(),
        }
    }
}

/// 启动器私有数据目录：%LOCALAPPDATA%\DshDesk（可用 DSHDESK_DATA_DIR 覆盖）
pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DSHDESK_DATA_DIR") {
        if !d.trim().is_empty() {
            return PathBuf::from(d);
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DshDesk")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

pub fn load() -> Config {
    fs::read(config_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Config) -> Result<(), String> {
    fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(config_path(), json).map_err(|e| e.to_string())
}
