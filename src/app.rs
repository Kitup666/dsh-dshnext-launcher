//! 顶层 State / Message。update 在 `update.rs`，view 在 `pages/`。
//!
//! 阶段 3：六个页面全部移植。状态形状对照上一代 `App.tsx` 的 `Shared` props，
//! 页面 view 统一签名 `fn view(app: &Dshnext) -> Element<'_, Message>`——比逐个
//! 传十几个参数清楚。

use crate::core::envres::EnvStatus;
use crate::core::event::{CoreEvent, LogStream};
use crate::core::plugins::{MarketItem, PluginInfo};
use crate::core::procman::ProcStatus;
use crate::core::profiles::ProfileInfo;
use crate::core::store::Config;
use crate::pages::Page;
use crate::theme::Palette;
use crate::ui::anim::{self, AnimState};
use crate::ui::modal::Dialog;
use crate::ui::widgets::{Toast, ToastKind};
use iced::window;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

/// 真实绘制次数（阶段 0 的测法：自定义 widget 在 draw() 里自增，不走 Message，
/// 否则「订阅出帧」会自己触发下一帧，测出来的空闲是假的）。
pub static DRAWS: AtomicU64 = AtomicU64::new(0);

/// 日志环形缓冲上限（DESIGN.md §8 第 4 条）。
pub const LOG_CAP: usize = 2000;
/// toast 存活时长。
pub const TOAST_TTL: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

/// `--shot 路径 --after 毫秒`：开窗后自截图退出，便于无人值守取证。
#[derive(Clone)]
pub struct Shot {
    pub path: String,
    pub after: u64,
}

/// 一条日志。`ts` 是毫秒时间戳，展示时只取时分秒。
pub struct LogLine {
    pub profile: String,
    pub stream: LogStream,
    pub line: String,
    pub ts: i64,
}

/// 插件页的两个分页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginTab {
    Installed,
    Market,
}

pub struct Dshnext {
    // ---- 窗口与视觉 ----
    pub mode: Mode,
    pub anim: AnimState,
    pub window: Option<window::Id>,
    pub shot: Option<Shot>,
    /// `--switch-to <页> --switch-at <毫秒>`：开窗后自动切一次页。
    /// 存在的理由是**动画取证**：`--shot --after` 截的永远是落定态，配上这个才能
    /// 把快门卡在过渡中间（`--switch-at` 与 `--after` 差多少，就截到第几毫秒）。
    pub switch: Option<(Page, u64)>,
    pub autotest: bool,
    pub e2e: bool,
    pub maximized: bool,
    /// 窗口几何跟踪（逻辑 px）：`window::events` 的 Moved/Resized 持续更新，
    /// 关窗时写进 `config.window` 供下次恢复。最大化期间不更新——保存的是
    /// 正常态几何，还原尺寸才对。iced 没有关窗时的 get_position 任务，
    /// 只能这样全程跟踪（iced_runtime 0.14 全文无 get_position/get_size）。
    pub win_pos: Option<iced::Point>,
    pub win_size: Option<iced::Size>,

    // ---- 导航与浮层 ----
    pub page: Page,
    /// 切页前的那一页，只为侧边栏交叉淡入服务（新旧两项同时插值，见 `pages::nav_item`）。
    /// 落定后不清空——`anim::NAV` 归零时它就不再被读了，清空反而要多一条消息。
    pub prev_page: Option<Page>,
    pub dialog: Option<Dialog>,
    /// 模态输入框的草稿值。
    pub draft: String,
    pub toasts: Vec<Toast>,

    // ---- 后端数据 ----
    pub config: Config,
    pub env: Option<EnvStatus>,
    pub profiles: Vec<ProfileInfo>,
    pub procs: Vec<ProcStatus>,
    pub logs: VecDeque<LogLine>,
    /// profile → WebUI 地址（从 dsh 输出里解析到的，比 config.port 准）。
    pub urls: HashMap<String, String>,
    /// 当前选中的 profile，首页与插件页共用。
    pub selected: String,
    /// 全局忙提示。非 None 时相关按钮禁用。
    pub busy: Option<String>,

    // ---- 环境页 ----
    pub dsh_versions: Vec<String>,
    pub dsh_latest: Option<String>,
    pub dsh_pick: String,
    pub node_versions: Vec<String>,
    pub node_pick: String,
    pub include_rc: bool,
    /// 版本列表是否正在请求中。只看 `dsh_versions.is_empty()` 不够：
    /// 请求飞在半路时它还是空的，反复进环境页会重复发请求。
    pub versions_loading: bool,

    // ---- 插件页 ----
    pub plugins: Vec<PluginInfo>,
    pub market: Vec<MarketItem>,
    pub market_loaded: bool,
    pub plugin_tab: PluginTab,
    pub query: String,

    // ---- 控制台页 ----
    /// `pages::console::ALL` 或某个 profile 名。
    pub log_filter: String,
    pub auto_scroll: bool,

    // ---- 设置页 ----
    /// 编辑草稿；点保存才写回 config 并落盘（上一代同样的 dirty 机制）。
    pub cfg_draft: Config,
    pub show_key: bool,
    /// 端口输入框的原始文本：允许中间态（空串、非法值），保存时才解析。
    pub port_text: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    // 窗口
    Opened(window::Id),
    Shoot,
    Shot(window::Screenshot),
    Tick(Instant),
    HoverEnter(anim::Key),
    HoverExit(anim::Key),
    DragWindow,
    Minimize,
    ToggleMaximize,
    MaximizedChanged(bool),
    Close,
    Resize(window::Direction),
    /// 全量窗口事件（Moved/Resized/CloseRequested…）：几何跟踪 + 关窗保存靠它。
    /// 单窗口应用，Id 不值得携带。
    WindowEvent(window::Event),

    // 导航与浮层
    Goto(Page),
    Select(String),
    /// 选中某个 profile 并跳到插件页（版本管理页的「插件」按钮）。
    GotoPlugins(String),
    /// 设置页主题三档（"light" | "dark" | "system"），即时生效并落盘。
    SetTheme(&'static str),
    /// "system" 的注册表探测结果（true = 系统偏好浅色）。
    SystemThemeResolved(bool),
    OpenDialog(Dialog),
    CloseDialog,
    DialogInput(String),
    DialogConfirm,
    ToastTick(Instant),
    Notify(ToastKind, String),
    /// 什么都不做。给「按钮在位但当前无动作」的场合用。
    Noop,

    // 后端事件流
    Core(CoreEvent),

    // 环境
    RefreshEnv,
    EnvLoaded(EnvStatus),
    LoadVersions,
    /// (dsh 版本列表, dsh latest, node 版本列表)
    VersionsLoaded(Result<(Vec<String>, Option<String>, Vec<String>), String>),
    PickDsh(String),
    PickNode(String),
    ToggleRc(bool),
    InstallNode,
    InstallDsh,
    InstallPnpm,
    /// 安装/卸载类操作的统一回调：(动作名, 结果)。
    OpDone(&'static str, Result<(), String>),

    // profile
    RefreshProfiles,
    ProfilesLoaded(Result<Vec<ProfileInfo>, String>),
    Start(String),
    Started(Result<(String, String), String>),
    Stop(String),
    Stopped(Result<String, String>),
    PollProcs,
    ProcsLoaded(Vec<ProcStatus>),
    OpenPath(String),
    OpenUi(String),

    // 插件
    RefreshPlugins,
    PluginsLoaded(Result<Vec<PluginInfo>, String>),
    LoadMarket,
    MarketLoaded(Result<Vec<MarketItem>, String>),
    SetPluginTab(PluginTab),
    Query(String),
    InstallPlugin(String),

    // 控制台
    SetLogFilter(String),
    ToggleAutoScroll(bool),
    ClearLogs,
    CopyLogs,

    // 设置
    CfgApiKey(String),
    CfgPort(String),
    CfgAutoOpen(bool),
    CfgAutoStart(bool),
    CfgNodeMirror(String),
    CfgNpmRegistry(String),
    CfgCatalog(String),
    ToggleShowKey,
    SaveConfig,
    ConfigSaved(Result<(), String>),

    /// e2e 的第二拍：启动成功若干秒后自动停止。
    E2eStop,
}

impl Dshnext {
    pub fn new(mode: Mode, shot: Option<Shot>, autotest: bool, e2e: bool) -> Self {
        // 配置是同步读的（一个小 JSON），不值得为它开 Task。
        // 自启开关以注册表实际状态为准：config 只存意图，用户可能手动删过键。
        let mut config = crate::core::store::load();
        config.autostart = crate::core::platform::autostart_enabled();
        // 主题跟随 config；命令行 --theme 已在 main 里定过，这里以命令行为准。
        Self {
            mode,
            anim: AnimState::default(),
            window: None,
            shot,
            switch: None,
            autotest,
            e2e,
            maximized: false,
            win_pos: None,
            win_size: None,

            page: Page::Home,
            prev_page: None,
            dialog: None,
            draft: String::new(),
            toasts: Vec::new(),

            port_text: config.port.to_string(),
            cfg_draft: config.clone(),
            config,
            env: None,
            profiles: Vec::new(),
            procs: Vec::new(),
            logs: VecDeque::new(),
            urls: HashMap::new(),
            selected: String::new(),
            busy: None,

            dsh_versions: Vec::new(),
            dsh_latest: None,
            dsh_pick: "latest".into(),
            node_versions: Vec::new(),
            node_pick: crate::core::envres::DEFAULT_NODE_VERSION.into(),
            include_rc: true,
            versions_loading: false,

            plugins: Vec::new(),
            market: Vec::new(),
            market_loaded: false,
            plugin_tab: PluginTab::Installed,
            query: String::new(),

            log_filter: crate::pages::console::ALL.to_string(),
            auto_scroll: true,

            show_key: false,
        }
    }

    pub fn palette(&self) -> &'static Palette {
        match self.mode {
            Mode::Dark => &Palette::DARK,
            Mode::Light => &Palette::LIGHT,
        }
    }

    /// 环境是否可用（能启动 harness）。
    pub fn env_ready(&self) -> bool {
        self.env.as_ref().is_some_and(|e| e.dsh_version.is_some())
    }

    pub fn running(&self, profile: &str) -> Option<&ProcStatus> {
        self.procs.iter().find(|p| p.profile == profile)
    }

    /// 某 profile 的 WebUI 地址：优先用解析到的，退回进程记录。
    pub fn url_of(&self, profile: &str) -> Option<String> {
        self.urls
            .get(profile)
            .cloned()
            .or_else(|| self.running(profile).map(|p| p.url.clone()))
    }

    /// 设置页是否有未保存改动（上一代靠 JSON 全量比较，这里逐字段比更省）。
    pub fn cfg_dirty(&self) -> bool {
        let a = &self.cfg_draft;
        let b = &self.config;
        a.api_key != b.api_key
            || a.port != b.port
            || a.auto_open != b.auto_open
            || a.autostart != b.autostart
            || a.node_mirror != b.node_mirror
            || a.npm_registry != b.npm_registry
            || a.plugin_catalog_url != b.plugin_catalog_url
    }

    pub fn push_log(&mut self, profile: String, stream: LogStream, line: String, ts: i64) {
        if self.logs.len() >= LOG_CAP {
            self.logs.pop_front();
        }
        self.logs.push_back(LogLine {
            profile,
            stream,
            line,
            ts,
        });
    }

    pub fn notify(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toasts.push(Toast {
            kind,
            text: text.into(),
            until: Instant::now() + TOAST_TTL,
        });
    }

    /// 系统提示同时进日志，方便事后追溯。
    pub fn sys_log(&mut self, profile: &str, line: impl Into<String>) {
        self.push_log(
            profile.to_string(),
            LogStream::System,
            line.into(),
            crate::core::event::now_millis(),
        );
    }
}
