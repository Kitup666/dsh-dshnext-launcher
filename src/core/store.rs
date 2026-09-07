use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::RwLock};

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
    /// WebUI 以「桌面窗口」打开（浏览器 --app 独立窗口，无地址栏无标签栏）。
    /// 关 = 系统浏览器标签页。仅 Chromium 系支持；非 Chromium 默认浏览器自动
    /// 回退 Edge，都不可用时回退系统浏览器。
    pub app_window: bool,
    /// 界面主题："light" | "dark" | "system"（跟随系统，启动时探测注册表）
    pub theme: String,
    /// 开机自启（HKCU Run 键，用户级无需管理员）
    pub autostart: bool,
    /// 关闭到托盘（托盘常驻模式）；关 = 关窗即退出
    pub tray: bool,
    /// 启动器窗口置顶（状态栏的图钉开关，立即生效并保存）
    pub always_on_top: bool,
    /// 启动器更新源（GitHub Releases API 地址）。空 = 用内置默认源
    /// （DEFAULT_UPDATE_URL）；检查只在用户点「检查更新」时发生，无后台流量。
    pub update_url: String,
    /// harness 异常退出时自动重启（指数退避，最多连 3 次）
    pub auto_restart: bool,
    /// dsh-home（DSH_HOME，profiles 与 harness 数据的根）。空串 = 默认
    /// `<数据目录>/home`。首次启动引导里设置；自定义时存绝对路径。
    pub dsh_home: String,
    /// 上次关闭时的窗口几何，开窗恢复
    pub window: Option<WindowGeom>,
}

/// 本仓库的 Releases API 地址，内置更新源。发布 Release 时附上 dshnext.exe
/// （可选 dshnext.exe.sha256）即可被自更新拉到。
pub const DEFAULT_UPDATE_URL: &str =
    "https://api.github.com/repos/Kitup666/dsh-dshnext-launcher/releases/latest";

impl Config {
    /// 实际生效的更新源：用户填了就用用户的（fork/镜像），没填用内置。
    pub fn effective_update_url(&self) -> &str {
        if self.update_url.trim().is_empty() {
            DEFAULT_UPDATE_URL
        } else {
            &self.update_url
        }
    }
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
            app_window: false,
            theme: "light".into(),
            autostart: false,
            tray: false,
            always_on_top: false,
            update_url: String::new(),
            auto_restart: false,
            dsh_home: String::new(),
            window: None,
        }
    }
}

// ---- 数据目录解析：env > pointer 文件 > 默认 ----
//
// 首次启动引导允许把「启动器数据目录」改到别处。config.json 就住在数据目录里，
// 它自己不能当钥匙（换了目录后下一次启动得先知道去哪找 config）——所以**锚定
// 目录里放一个 pointer 文件**，config 跟着数据目录走：
//
//   锚定 boot_dir = DSHDESK_DATA_DIR（env，开发/测试逃生口）> %LOCALAPPDATA%\DshDesk
//   数据目录     = boot_dir/launcher-dir.txt 指向的绝对路径 > boot_dir
//
// env 设了就直接用 env、pointer 被无视——自动化测试的确定性靠它。

/// 锚定目录：pointer 的家，自身永不搬。
pub fn boot_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DSHDESK_DATA_DIR") {
        if !d.trim().is_empty() {
            return PathBuf::from(d);
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DshDesk")
}

/// pointer 文件名。内容是一个绝对路径（utf-8）。
pub const POINTER_FILE: &str = "launcher-dir.txt";

pub fn pointer_path() -> PathBuf {
    boot_dir().join(POINTER_FILE)
}

static DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// main 早期调用（必须先于任何 config/路径读取）：读 pointer、定下本次会话的
/// 数据目录。返回实际生效的数据目录。
pub fn init_data_dir() -> PathBuf {
    let boot = boot_dir();
    let redirected = std::env::var("DSHDESK_DATA_DIR").is_err().then(|| {
        fs::read_to_string(boot.join(POINTER_FILE))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && *p != boot)
    });
    let resolved = redirected.flatten().unwrap_or(boot);
    *DATA_DIR.write().expect("DATA_DIR 锁") = Some(resolved.clone());
    resolved
}

/// 启动器私有数据目录：%LOCALAPPDATA%\DshDesk（可被 pointer 重定向；
/// DSHDESK_DATA_DIR env 仍是最优先）。未 init（如测试早期路径）时回落 boot_dir。
pub fn data_dir() -> PathBuf {
    if let Ok(g) = DATA_DIR.read() {
        if let Some(p) = g.as_ref() {
            return p.clone();
        }
    }
    boot_dir()
}

/// 首次启动引导确认：本次会话即刻改道数据目录。
pub fn redirect_data_dir(p: PathBuf) {
    *DATA_DIR.write().expect("DATA_DIR 锁") = Some(p);
}

/// 把数据目录指到 `target`（非锚定目录时写 pointer；回到锚定目录时清掉）。
pub fn write_pointer(target: &PathBuf) -> Result<(), String> {
    if *target == boot_dir() {
        let _ = fs::remove_file(pointer_path());
        return Ok(());
    }
    fs::create_dir_all(boot_dir()).map_err(|e| e.to_string())?;
    fs::write(pointer_path(), target.to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())
}

/// 是否从未完成过首次配置（config.json 不存在）——首启引导的触发条件。
pub fn config_exists() -> bool {
    config_path().exists()
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

/// 离线安装包目录约定：`<data_dir>/offline/`。放 `node-*.zip`、`dsh*.tgz`、
/// `pnpm*.tgz`，环境页检测到就提供「离线安装」（断网可装）。
pub fn offline_dir() -> PathBuf {
    data_dir().join("offline")
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
