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

/// 主窗口标题（tray/win32 按 FindWindowW 找窗口用，与 main.rs .title() 必须同源）。
pub const WINDOW_TITLE: &str = "DshDesk — DeepSeek Harness 启动器";

/// 真实绘制次数（阶段 0 的测法：自定义 widget 在 draw() 里自增，不走 Message，
/// 否则「订阅出帧」会自己触发下一帧，测出来的空闲是假的）。
pub static DRAWS: AtomicU64 = AtomicU64::new(0);

/// 目录编辑表单是否开着。ESC 的全局键盘订阅只能收裸 fn 指针（捕获不了
/// 状态，同 `bridge` 的全局 channel 一族），去向放这里。
pub static DIRS_EDIT_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

/// 首次启动引导里可「浏览」选择的两个目录字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickField {
    LauncherDir,
    DshHome,
}

/// 目录表单（首次启动引导与设置页「修改目录」共用同一套字段/消息/浮层，
/// `first_run` 区分两者：引导不可关闭；编辑模式可取消、确认会**转移现有文件**）。
/// 两个目录**空串 = 用默认**——输入框的 placeholder 显示默认路径，用户看得到
/// 自己将得到什么。
pub struct Onboarding {
    pub first_run: bool,
    pub launcher_dir: String,
    pub dsh_home: String,
    /// 目录选择对话框为哪个字段打开（打开期间避免重复点「浏览」）。
    pub picking: Option<PickField>,
    /// placeholder 用的默认路径（启动器锚定目录）。`&'static`：view 借不走
    /// 局部 String，创建时泄漏一次（浮层生命周期内只有一次）。
    pub launcher_default: &'static str,
    /// dsh-home 的 placeholder：跟随 launcher 草稿（留空 = <启动器目录>/home）。
    /// 存在状态里 view 才能借（ObEdit(LauncherDir) 时同步更新）。
    pub home_hint: String,
}

impl Onboarding {
    pub fn new() -> Self {
        let launcher_default = crate::core::store::boot_dir()
            .to_string_lossy()
            .into_owned();
        Self {
            first_run: true,
            launcher_dir: String::new(),
            dsh_home: String::new(),
            picking: None,
            launcher_default: Box::leak(launcher_default.into_boxed_str()),
            home_hint: Self::home_hint_for(""),
        }
    }

    /// 设置页「修改目录」用的表单：预填当前实际值（非默认才填，保持
    /// 「空串 = 默认」的约定）。
    pub fn for_edit(current_dsh_home: &str) -> Self {
        let mut ob = Self::new();
        ob.first_run = false;
        let data = crate::core::store::data_dir();
        if data != crate::core::store::boot_dir() {
            ob.launcher_dir = data.to_string_lossy().into_owned();
        }
        ob.dsh_home = current_dsh_home.trim().to_string();
        ob.home_hint = Self::home_hint_for(&ob.launcher_dir);
        ob
    }

    /// dsh-home 的 placeholder：留空 = <启动器目录>/home（用户自定义了
    /// launcher 还留空 home 时，默认值落在他选的目录下）。
    pub fn home_hint_for(launcher_draft: &str) -> String {
        let base = launcher_draft.trim();
        if base.is_empty() {
            crate::core::store::boot_dir()
                .join("home")
                .to_string_lossy()
                .into_owned()
        } else {
            std::path::PathBuf::from(base)
                .join("home")
                .to_string_lossy()
                .into_owned()
        }
    }
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
    /// `--migrate-go`：boot 闭包发不出 Task（运行时未接手），标记后在第一次
    /// update 里补发 ObConfirm 的转移任务。
    pub boot_migrate: bool,
    pub autotest: bool,
    pub e2e: bool,
    pub maximized: bool,
    /// 窗口几何跟踪（逻辑 px）：`window::events` 的 Moved/Resized 持续更新，
    /// 关窗时写进 `config.window` 供下次恢复。最大化期间不更新——保存的是
    /// 正常态几何，还原尺寸才对。iced 没有关窗时的 get_position 任务，
    /// 只能这样全程跟踪（iced_runtime 0.14 全文无 get_position/get_size）。
    pub win_pos: Option<iced::Point>,
    pub win_size: Option<iced::Size>,
    /// 窄布局档的迟滞状态（进入 <1120，退出 >1180）
    pub narrow_layout: bool,
    /// 逐帧重画泵的截止时刻：拖动调窗时 DWM 会把上一帧拉伸到新窗口尺寸
    /// （内容「放大再缩小」的残影），重画一快拉伸帧就停留不住。
    /// 每次 Resized 续 250ms，停手后自然熄火——空闲零出帧纪律不破。
    pub resize_pump_until: Option<Instant>,

    // ---- 导航与浮层 ----
    pub page: Page,
    /// 切页前的那一页，只为侧边栏交叉淡入服务（新旧两项同时插值，见 `pages::nav_item`）。
    /// 落定后不清空——`anim::NAV` 归零时它就不再被读了，清空反而要多一条消息。
    pub prev_page: Option<Page>,
    pub dialog: Option<Dialog>,
    /// 模态输入框的草稿值。
    pub draft: String,
    /// 首次启动目录引导（None = 已配置过 / 自动化运行跳过）。非 None 时盖住
    /// 整个界面且不可关闭——目录是它唯一的前置问题。
    pub onboarding: Option<Onboarding>,
    /// 设置页「修改目录」表单（与 onboarding 共用浮层与 Ob* 消息；
    /// 两者不会同时开着——Ob* 的路由是 dirs_edit 优先）。
    pub dirs_edit: Option<Onboarding>,
    /// 目录转移进行中（confirm_dirs_edit 到 MigrateDone 之间）。挂 MigrateTick
    /// 心跳订阅、浮层显示进度条并禁用按钮。
    pub migrating: bool,
    /// 「停止并继续」待续跑的 runtime 变更操作：守卫拦下时暂存，用户确认停
    /// 实例后由 ProcsLoaded（空）取回续投。取消/用户手动接管时清空。
    pub resume_runtime_op: Option<RuntimeOp>,
    /// 最近一次读到的复制进度（MigrateTick 从 core::migrate::progress() 抄来；
    /// 存字段是为了 view 有值可画）。
    pub migrate_prog: (u64, u64),
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

    // ---- 环境页离线包：进环境页时扫描 offline 目录 ----
    pub offline: Option<crate::core::installs::OfflinePacks>,

    // ---- 插件页 ----
    pub plugins: Vec<PluginInfo>,
    pub market: Vec<MarketItem>,
    pub market_loaded: bool,
    pub plugin_tab: PluginTab,
    pub query: String,

    // ---- 运行守护 ----
    /// 用户点了「停止」的 profile：Exit 事件到达时据此区分主动停 vs 崩溃。
    pub stopping: std::collections::HashSet<String>,
    /// 自动重启在途的 profile：StartProbed 探到端口未释放时走重试而非误开外部服务。
    pub restarting: std::collections::HashSet<String>,
    /// 等 WebUI 地址（带 token 的 Url 事件）就绪后自动打开浏览器的 profile，
    /// 值是放弃时刻——裸地址开出来是 404（dsh 的 UI 只挂在 token 路径上），
    /// 绝不能提前拿 `http://127.0.0.1:{port}` 去开。
    pub pending_open: std::collections::HashMap<String, Instant>,
    /// 每个 profile 的 (连续崩溃次数, 上次自动重启时刻)——指数退避与上限用。
    pub crash_restarts: std::collections::HashMap<String, (u32, Instant)>,

    // ---- 控制台页 ----
    /// `pages::console::ALL` 或某个 profile 名。
    pub log_filter: String,
    pub auto_scroll: bool,
    /// 「只看错误」开关：过滤掉非错误行（见 console::is_errorish）。
    pub errors_only: bool,

    // ---- 启动耗时 ----
    /// profile → Started 时刻；CoreEvent::Url 到达时算差值写进 config。
    pub boot_at: HashMap<String, Instant>,
    /// --minimized 静默自启：建窗不可见 + 强制挂托盘（否则窗口找不回来）。
    pub minimized_start: bool,
    /// 正在下载更新：驱动设置页进度条与心跳订阅（进度字节见 selfupdate::download_progress）。
    pub update_busy: bool,

    // ---- 设置页 ----
    /// 编辑草稿；点保存才写回 config 并落盘（上一代同样的 dirty 机制）。
    pub cfg_draft: Config,
    pub show_key: bool,
    /// 端口输入框的原始文本：允许中间态（空串、非法值），保存时才解析。
    pub port_text: String,
}

/// 会被「实例运行中」守卫拦下的托管 runtime 变更操作。守卫弹确认框，用户
/// 同意后先停全部实例，停干净（ProcsLoaded 空）再按 `message()` 续跑原操作。
#[derive(Debug, Clone)]
pub enum RuntimeOp {
    InstallDsh,
    InstallNode,
    InstallPnpm,
    InstallDshOffline(std::path::PathBuf),
    InstallNodeOffline(std::path::PathBuf),
    InstallPnpmOffline(std::path::PathBuf),
    RemoveDsh,
    RemoveNode,
}

impl RuntimeOp {
    /// 续跑时重新投递的消息（重新进各自的 update 臂，守卫此时已放行）。
    pub fn message(&self) -> Message {
        match self {
            RuntimeOp::InstallDsh => Message::InstallDsh,
            RuntimeOp::InstallNode => Message::InstallNode,
            RuntimeOp::InstallPnpm => Message::InstallPnpm,
            RuntimeOp::InstallDshOffline(p) => Message::InstallDshOffline(p.clone()),
            RuntimeOp::InstallNodeOffline(p) => Message::InstallNodeOffline(p.clone()),
            RuntimeOp::InstallPnpmOffline(p) => Message::InstallPnpmOffline(p.clone()),
            RuntimeOp::RemoveDsh => Message::RemoveDshNow,
            RuntimeOp::RemoveNode => Message::RemoveNodeNow,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RuntimeOp::InstallDsh | RuntimeOp::InstallDshOffline(_) => "安装 dsh",
            RuntimeOp::InstallNode | RuntimeOp::InstallNodeOffline(_) => "安装 Node.js",
            RuntimeOp::InstallPnpm | RuntimeOp::InstallPnpmOffline(_) => "安装 pnpm",
            RuntimeOp::RemoveDsh => "卸载 dsh",
            RuntimeOp::RemoveNode => "删除托管 Node.js",
        }
    }
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

    // 首次启动目录引导
    ObEdit(PickField, String),
    /// 打开系统目录选择对话框。
    ObBrowse(PickField),
    /// 选择结果（None = 用户取消）。
    ObPicked(PickField, Option<String>),
    /// 两个目录全部回到默认（草稿清空，placeholder 显示默认路径）。
    ObDefaults,
    /// 「开始使用」：建目录、写配置与 pointer、重新探测。
    ObConfirm,
    /// 环境页「修改」：打开编辑模式的目录表单（预填当前值）。
    OpenDirsEdit,
    /// 编辑模式的取消（首启引导没有这个出口）。
    ObClose,
    /// 目录转移结束：Ok 里是删除阶段没删掉的旧文件清单（非致命）。
    MigrateDone(Result<Vec<String>, String>),
    /// 转移进行中的心跳（~120ms 一次）：从 core::migrate 抄进度给 view 画。
    MigrateTick,
    /// 卸载 dsh / 删除托管 Node 的实际执行（从 Dialog 确认臂拆出来，
    /// 守卫放行或「停止并继续」续跑都投递这个）。
    RemoveDshNow,
    RemoveNodeNow,
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
    /// 离线安装（包在 data/offline/），参数是包文件路径。
    InstallNodeOffline(std::path::PathBuf),
    InstallDshOffline(std::path::PathBuf),
    InstallPnpmOffline(std::path::PathBuf),
    ScanOffline,
    OfflineScanned(crate::core::installs::OfflinePacks),
    /// 安装/卸载类操作的统一回调：(动作名, 结果)。
    OpDone(&'static str, Result<(), String>),

    // profile
    RefreshProfiles,
    ProfilesLoaded(Result<Vec<ProfileInfo>, String>),
    Start(String),
    /// 启动前的端口探测结果：Free 才真正 spawn，Http 复用打开，Other 警告。
    StartProbed(String, crate::core::platform::PortProbe),
    Started(Result<(String, String), String>),
    Stop(String),
    Stopped(Result<String, String>),
    PollProcs,
    ProcsLoaded(Vec<ProcStatus>),
    OpenPath(String),
    OpenUi(String),
    /// 打开带 token 的 WebUI 地址：按 app_window 配置分流——浏览器标签页或
    /// 浏览器 --app 独立窗口（桌面窗口模式，见 core::appwin）。
    OpenWebUi(String),
    /// 桌面窗口打开的落地/失败回调。Err(url) = 没解析到可用浏览器或启动失败，
    /// url 供回退走系统浏览器。
    AppWinOpened(Result<String, String>),
    /// 启动页「打开方式」分段控件：切换即写 config+draft 并落盘。
    SetAppWindow(bool),
    /// SetAppWindow 落盘回调。失败只 toast 不回滚（开关意图下轮仍生效）。
    AppWindowSaved(Result<(), String>),

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
    ToggleErrorsOnly(bool),
    ClearLogs,
    CopyLogs,
    /// 首页点 WEB 地址：复制带 token 的完整链接（没有则复制裸地址）。
    CopyWebUrl(String),

    // 设置
    /// 托盘事件（显示/退出）。
    Tray(crate::tray::TrayEvent),
    /// 自更新：检查 → 下载校验 → 换身。
    CheckUpdate,
    UpdateChecked(Result<crate::core::selfupdate::UpdateInfo, String>),
    UpdateReady(Result<(String, std::path::PathBuf), String>),
    /// 下载期心跳：只为触发重绘读进度全局，本身无副作用。
    UpdateTick,
    /// 导出脱敏诊断文件（设置页「关于」卡）。
    ExportDiag,
    DiagExported(Result<std::path::PathBuf, String>),
    CfgApiKey(String),
    CfgPort(String),
    CfgAutoOpen(bool),
    CfgAutoStart(bool),
    CfgTray(bool),
    ToggleTopmost,
    CfgUpdateUrl(String),
    CfgAutoRestart(bool),
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
        // 首次启动（config 不存在）弹目录引导；--shot/--e2e/--autotest 的
        // 无人值守运行跳过——自动化要的是可预期的界面，不该被浮层挡住。
        let onboarding = if crate::core::store::config_exists() || shot.is_some() || autotest || e2e
        {
            None
        } else {
            Some(Onboarding::new())
        };
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
            narrow_layout: false,
            resize_pump_until: None,

            page: Page::Home,
            prev_page: None,
            dialog: None,
            draft: String::new(),
            onboarding,
            dirs_edit: None,
            migrating: false,
            resume_runtime_op: None,
            migrate_prog: (0, 0),
            boot_migrate: false,
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

            offline: None,

            plugins: Vec::new(),
            market: Vec::new(),
            market_loaded: false,
            plugin_tab: PluginTab::Installed,
            query: String::new(),

            stopping: std::collections::HashSet::new(),
            restarting: std::collections::HashSet::new(),
            pending_open: std::collections::HashMap::new(),
            crash_restarts: std::collections::HashMap::new(),

            log_filter: crate::pages::console::ALL.to_string(),
            auto_scroll: true,
            errors_only: false,
            boot_at: HashMap::new(),
            minimized_start: false,
            update_busy: false,

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

    /// 窗口是否落入「窄」档——各页响应式布局共用的唯一开关。
    /// 带迟滞（1120 进 / 1180 出，见 Resized 分支）：拖边经过阈值附近时
    /// 手一抖宽度就在边界来回，无迟滞则整页布局跟着反复翻转，看起来
    /// 是整页在颤。
    pub fn narrow(&self) -> bool {
        self.narrow_layout
    }

    /// 某 profile 可打开的 WebUI 地址：只认 stdout 里解析出来的
    /// （带 token）那条；进程记录里的裸地址开出来是 404，绝不退回它。
    pub fn url_of(&self, profile: &str) -> Option<String> {
        self.urls.get(profile).cloned()
    }

    /// 设置页是否有未保存改动（上一代靠 JSON 全量比较，这里逐字段比更省）。
    pub fn cfg_dirty(&self) -> bool {
        let a = &self.cfg_draft;
        let b = &self.config;
        a.api_key != b.api_key
            || a.port != b.port
            || a.auto_open != b.auto_open
            || a.app_window != b.app_window
            || a.always_on_top != b.always_on_top
            || a.autostart != b.autostart
            || a.tray != b.tray
            || a.update_url != b.update_url
            || a.auto_restart != b.auto_restart
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
