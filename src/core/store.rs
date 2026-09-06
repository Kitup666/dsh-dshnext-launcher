use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

/// 窗口几何（逻辑 px）。关窗时写入 config.json，开窗恢复；None = 从未保存。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowGeom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

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
    /// 界面主题："light" | "dark" | "system"（跟随系统，启动时探测注册表）
    pub theme: String,
    /// 开机自启（HKCU Run 键，用户级无需管理员）
    pub autostart: bool,
    /// 关闭到托盘（托盘常驻模式）；关 = 关窗即退出
    pub tray: bool,
    /// 启动器更新源（GitHub Releases API 地址），空 = 不检查更新
    pub update_url: String,
    /// 上次关闭时的窗口几何，开窗恢复
    pub window: Option<WindowGeom>,
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
            autostart: false,
            tray: false,
            update_url: String::new(),
            window: None,
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
