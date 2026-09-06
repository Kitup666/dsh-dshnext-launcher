//! `iced_test` 用例（DESIGN.md §9「功能对等」行）。
//!
//! **测的是 view↔update 契约**，不是真实子进程：`simulator` 只跑 `view` 并把交互
//! 产生的 `Message` 交回来，`update` 返回的 `Task`（真正 spawn dsh、发网络请求的那些）
//! 在这里被丢弃、不执行。所以本文件覆盖的是「点这个控件会不会发出那条消息」和
//! 「喂进那条消息后状态对不对」；真实的启停链路在阶段 2 用 `--e2e` 实测过。
//!
//! 选控件用 `&str`（`iced_selector` 的按文本精确匹配，见 `content == *self`），
//! 文案取自各页 `view`。同文本歧义靠「只在对应页面上点」规避。

// update 返回的 Task 在测试里本就该丢弃（不驱动运行时），逐条 `let _ =` 太吵。
#![allow(unused_must_use)]

use crate::app::{Dshnext, Message, Mode, Onboarding, PickField, PluginTab};
use crate::core::envres::EnvStatus;
use crate::core::event::{CoreEvent, LogStream};
use crate::core::plugins::PluginInfo;
use crate::core::profiles::ProfileInfo;
use crate::pages::Page;
use crate::ui::modal::Dialog;
use iced_test::simulator;
use std::sync::OnceLock;

/// 把数据目录指到临时位置，保证 `Dshnext::new` 里的 `store::load()` 读到默认配置，
/// 不碰用户真实的 `%LOCALAPPDATA%\DshDesk\config.json`。同时初始化 core bridge——
/// `Message::Start/Stop` 在 `update` 里会同步取 `bridge::sink()`，不 init 会 panic。
fn isolate_data_dir() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let dir = std::env::temp_dir().join("dshnext-tests");
        // SAFETY: 测试进程启动早期、单线程 OnceLock 初始化里设置，无并发读取。
        unsafe { std::env::set_var("DSHDESK_DATA_DIR", &dir) };
        crate::bridge::init();
    });
}

fn app() -> Dshnext {
    isolate_data_dir();
    let mut a = Dshnext::new(Mode::Dark, None, false, false);
    // config.json 是否存在是进程共享的磁盘状态，目录迁移用例会搬它——并行
    // 测试里「引导盖住界面」会随机挡掉点击。除引导专项用例，一律关掉。
    a.onboarding = None;
    a
}

/// 造一个「环境就绪 + 有一个版本」的 app，让各页有内容可渲染、按钮可点。
fn seeded_app() -> Dshnext {
    let mut a = app();
    a.env = Some(EnvStatus {
        node_version: Some("v24.19.0".into()),
        node_path: Some(r"C:\fake\node.exe".into()),
        node_managed: true,
        pnpm_version: Some("10.23.0".into()),
        dsh_version: Some("0.1.1-rc.2".into()),
        dsh_path: Some(r"C:\fake\dsh".into()),
        data_dir: r"C:\fake\DshDesk".into(),
        home_dir: r"C:\fake\DSH_HOME".into(),
    });
    a.profiles = vec![profile("demo", &["cordis".into()])];
    a.selected = "demo".into();
    a
}

fn profile(name: &str, bundles: &[String]) -> ProfileInfo {
    ProfileInfo {
        name: name.into(),
        bundles: bundles.to_vec(),
        dependencies: serde_json::Map::new(),
        path: format!(r"C:\fake\DSH_HOME\profiles\{name}"),
    }
}

/// 在当前 view 里点中文案为 `text` 的控件，返回它产生的消息。
fn click(app: &Dshnext, text: &str) -> Vec<Message> {
    let mut ui = simulator(crate::pages::view(app));
    ui.click(text)
        .unwrap_or_else(|e| panic!("点击 {text:?} 失败：{e}"));
    ui.into_messages().collect()
}

/// 当前 view 里是否存在文案精确等于 `text` 的控件。
fn has_text(app: &Dshnext, text: &str) -> bool {
    simulator(crate::pages::view(app)).find(text).is_ok()
}

/// 断言 `click` 产生的消息里含 `pred` 命中的那条，返回它。
fn one<T: std::fmt::Debug>(msgs: Vec<T>, pred: impl Fn(&T) -> bool, what: &str) -> T {
    msgs.into_iter()
        .find(pred)
        .unwrap_or_else(|| panic!("未产生 {what}"))
}

// ---------------------------------------------------------------- 冒烟

#[test]
fn headless_renders_home() {
    let a = seeded_app();
    assert!(has_text(&a, "DEEPSEEK HARNESS"), "首页 eyebrow 应在");
    assert!(has_text(&a, "启动 harness"), "首页主按钮应在");
}

// ---------------------------------------------------------------- 导航

#[test]
fn sidebar_click_navigates() {
    let mut a = seeded_app();
    let msg = one(
        click(&a, "版本管理"),
        |m| matches!(m, Message::Goto(Page::Profiles)),
        "Goto(Profiles)",
    );
    a.update(msg);
    assert_eq!(a.page, Page::Profiles);
    // 换页后应渲染出版本管理页的专属文案。
    assert!(has_text(&a, "+ 新建版本"), "版本管理页应显示新建按钮");
}

#[test]
fn every_page_renders() {
    for page in Page::ALL {
        let mut a = seeded_app();
        a.page = page;
        // 只要不 panic、能构造出 view 就算过；再抽查一个该页专属文案。
        let signature = match page {
            Page::Home => "运行中的实例",
            Page::Profiles => "+ 新建版本",
            Page::Plugins => "插件管理",
            Page::Env => "检测结果",
            Page::Console => "自动滚动",
            Page::Settings => "模型访问",
        };
        assert!(has_text(&a, signature), "{page:?} 页应含 {signature:?}");
    }
}

// ---------------------------------------------------------------- 模态全流程

#[test]
fn create_profile_dialog_flow() {
    let mut a = seeded_app();
    a.page = Page::Profiles;

    // 1. 点「新建版本」→ OpenDialog
    let msg = one(
        click(&a, "+ 新建版本"),
        |m| matches!(m, Message::OpenDialog(Dialog::CreateProfile)),
        "OpenDialog(CreateProfile)",
    );
    a.update(msg);
    assert_eq!(a.dialog, Some(Dialog::CreateProfile), "模态应打开");

    // 2. 输入框反映草稿值；空草稿时「创建」按钮禁用——点它不产生 DialogConfirm
    //    （点击会冒泡到遮罩触发 CloseDialog，所以只能断言「没有 DialogConfirm」，
    //    不能断言「没有任何消息」）。
    assert!(has_text(&a, "新建版本"), "模态标题应在");
    let empty_click = click(&a, "创建");
    assert!(
        !empty_click.iter().any(|m| matches!(m, Message::DialogConfirm)),
        "草稿为空时点创建不应触发 DialogConfirm"
    );

    // 3. 喂进输入 → 草稿非空 → 创建按钮可点。
    a.update(Message::DialogInput("e2e-new".into()));
    assert!(has_text(&a, "e2e-new"), "输入框应回显草稿");
    let msg = one(
        click(&a, "创建"),
        |m| matches!(m, Message::DialogConfirm),
        "DialogConfirm",
    );
    a.update(msg);
    assert!(a.dialog.is_none(), "确认后模态应关闭");
}

#[test]
fn esc_message_closes_dialog() {
    // ESC 的键盘订阅在 subscription() 里，simulator 不跑订阅；
    // 这里直接喂 CloseDialog（键盘监听最终就是发这条）验证状态迁移。
    let mut a = seeded_app();
    a.update(Message::OpenDialog(Dialog::DeleteProfile("demo".into())));
    assert!(a.dialog.is_some());
    a.update(Message::CloseDialog);
    assert!(a.dialog.is_none(), "CloseDialog 应关模态");
}

// ---------------------------------------------------------------- 主题

#[test]
fn theme_toggle_flips_mode() {
    let mut a = seeded_app();
    assert_eq!(a.mode, Mode::Dark);
    a.update(Message::SetTheme("light")); // 落盘在返回的 Task 里，测试不执行它
    assert_eq!(a.mode, Mode::Light);
    assert_eq!(a.config.theme, "light");
    // 换主题后 view 仍应正常构造。
    assert!(has_text(&a, "DEEPSEEK HARNESS"));
}

// ---------------------------------------------------------------- 插件页

#[test]
fn plugin_tab_switch() {
    let mut a = seeded_app();
    a.page = Page::Plugins;
    let msg = one(
        click(&a, "插件市场"),
        |m| matches!(m, Message::SetPluginTab(PluginTab::Market)),
        "SetPluginTab(Market)",
    );
    a.update(msg);
    assert_eq!(a.plugin_tab, PluginTab::Market);
}

#[test]
fn installed_plugin_row_uninstall_opens_dialog() {
    let mut a = seeded_app();
    a.page = Page::Plugins;
    a.plugins = vec![PluginInfo {
        name: "@scope/dsh-foo".into(),
        version: "1.2.3".into(),
    }];
    let msg = one(
        click(&a, "卸载"),
        |m| matches!(m, Message::OpenDialog(Dialog::RemovePlugin(n)) if n == "@scope/dsh-foo"),
        "OpenDialog(RemovePlugin)",
    );
    a.update(msg);
    assert!(matches!(a.dialog, Some(Dialog::RemovePlugin(_))));
}

// ---------------------------------------------------------------- 控制台

#[test]
fn console_clear_and_empty_state() {
    let mut a = seeded_app();
    a.page = Page::Console;
    a.update(Message::Core(CoreEvent::Log {
        profile: "demo".into(),
        stream: LogStream::Stdout,
        line: "hello from dsh".into(),
        ts: 0,
    }));
    assert_eq!(a.logs.len(), 1);
    assert!(has_text(&a, "hello from dsh"), "日志行应渲染");

    a.update(Message::ClearLogs);
    assert!(a.logs.is_empty());
    // 空态改成图标圈 + 标题/说明两行（widgets::empty_state）。
    assert!(has_text(&a, "暂无输出"), "清空后应显示空状态标题");
    assert!(
        has_text(&a, "启动 harness 或安装插件后，日志会实时出现在这里。"),
        "清空后应显示空状态说明"
    );
}

#[test]
fn core_url_event_lands_in_logs() {
    let mut a = seeded_app();
    a.update(Message::Core(CoreEvent::Url {
        profile: "demo".into(),
        url: "http://127.0.0.1:3080".into(),
    }));
    assert_eq!(a.urls.get("demo").map(String::as_str), Some("http://127.0.0.1:3080"));
    assert!(a.logs.iter().any(|l| l.line.contains("3080")), "地址应进系统日志");
}

// ---------------------------------------------------------------- 设置页

#[test]
fn settings_dirty_tag() {
    let mut a = seeded_app();
    a.page = Page::Settings;
    assert!(!has_text(&a, "有未保存的修改"), "初始无改动不应显脏");

    a.update(Message::CfgApiKey("sk-test-123".into()));
    assert!(a.cfg_dirty());
    assert!(has_text(&a, "有未保存的修改"), "改过应显脏标");
    // 保存按钮此时才可点。
    let msg = one(
        click(&a, "保存"),
        |m| matches!(m, Message::SaveConfig),
        "SaveConfig",
    );
    // 喂进去会返回写盘 Task（测试不执行），只验证消息本身。
    drop(a.update(msg));
}

#[test]
fn show_key_toggles_label() {
    let mut a = seeded_app();
    a.page = Page::Settings;
    assert!(has_text(&a, "显示"), "默认应显示「显示」按钮");
    a.update(Message::ToggleShowKey);
    assert!(a.show_key);
    assert!(has_text(&a, "隐藏"), "切换后按钮文案应变「隐藏」");
}

// ---------------------------------------------------------------- 首页启动/停止

#[test]
fn selfupdate_version_compare() {
    let n = crate::core::selfupdate::newer;
    assert!(n("0.2.0", "0.1.9"));
    assert!(n("v1.0", "0.9.9"));
    assert!(n("0.1.0.1", "0.1.0"), "段数不齐补零比");
    assert!(!n("0.1.0", "0.1.0"));
    assert!(!n("0.1.0", "0.1.1"));
    assert!(!n("0.1", "0.1.0"), "补零后相等不算新");
}

#[test]
fn diag_export_redacts_api_key() {
    let mut cfg = crate::core::store::Config::default();
    cfg.api_key = "sk-very-secret-1234567890abcdef".into();
    let text =
        crate::core::diag::render(&cfg, None, &[], &[], &[], env!("CARGO_PKG_VERSION"), "test-os");
    assert!(!text.contains("sk-very-secret"), "诊断报告绝不能包含 Key 内容");
    assert!(text.contains("已设置（31 字符"), "应只出现长度：{text}");
}

#[test]
fn home_start_emits_start_message() {
    let mut a = seeded_app();
    a.page = Page::Home;
    let msg = one(
        click(&a, "启动 harness"),
        |m| matches!(m, Message::Start(p) if p == "demo"),
        "Start(demo)",
    );
    // 启动是两阶段：Start 只发端口探测，StartProbed(Free) 才置忙 spawn。
    // 测试不执行 Task（不真探测/不真起进程），手动喂 Free 走到置忙那步。
    a.update(msg);
    assert!(a.busy.is_none(), "探测阶段还不应置忙");
    a.update(Message::StartProbed(
        "demo".into(),
        crate::core::platform::PortProbe::Free,
    ));
    assert!(a.busy.is_some(), "探测通过后应置忙");
}

#[test]
fn auto_open_waits_for_tokened_url() {
    // 裸地址（无 token）开出来是 404：自动打开必须等 Url 事件带 token 的地址。
    let mut a = seeded_app();
    a.config.auto_open = true;
    a.update(Message::Started(Ok(("demo".into(), "http://127.0.0.1:3080".into()))));
    assert!(
        a.pending_open.contains_key("demo"),
        "auto_open 时应登记等待带 token 的地址"
    );
    let task = a.update(Message::Core(crate::core::event::CoreEvent::Url {
        profile: "demo".into(),
        url: "http://127.0.0.1:3080/?token=abc".into(),
    }));
    assert!(!a.pending_open.contains_key("demo"), "地址到了应兑现等待");
    let _ = task; // Task::done(OpenPath(tokened))，测试环境不执行
}

#[test]
fn open_ui_without_url_parks_pending() {
    let mut a = seeded_app();
    a.procs = vec![crate::core::procman::ProcStatus {
        profile: "demo".into(),
        port: 3080,
        url: "http://127.0.0.1:3080".into(),
        pid: 4321,
        uptime_secs: 75,
    }];
    drop(a.update(Message::OpenUi("demo".into())));
    assert!(a.pending_open.contains_key("demo"), "实例在跑但地址未到：应挂起等待而非报错");
    drop(a.update(Message::Core(crate::core::event::CoreEvent::Url {
        profile: "demo".into(),
        url: "http://127.0.0.1:3080/?token=x".into(),
    })));
    assert!(!a.pending_open.contains_key("demo"));
}

#[test]
fn narrow_layout_has_hysteresis() {
    let mut a = seeded_app();
    use iced::window;
    // 起始宽 960 → 窄档
    drop(a.update(Message::WindowEvent(window::Event::Opened {
        position: Some(iced::Point::ORIGIN),
        size: iced::Size::new(960.0, 608.0),
    })));
    assert!(a.narrow());
    // 1160：旧逻辑已经翻回宽档；迟滞下仍在窄档（要 >1180 才出）
    drop(a.update(Message::WindowEvent(window::Event::Resized(iced::Size::new(
        1160.0, 608.0,
    )))));
    assert!(a.narrow(), "1160 应留在窄档（抖动缓冲）");
    // 1185 出窄档
    drop(a.update(Message::WindowEvent(window::Event::Resized(iced::Size::new(
        1185.0, 608.0,
    )))));
    assert!(!a.narrow());
    // 1140：旧逻辑立刻进窄档；迟滞下要 <1120 才进
    drop(a.update(Message::WindowEvent(window::Event::Resized(iced::Size::new(
        1140.0, 608.0,
    )))));
    assert!(!a.narrow(), "1140 应留在宽档（抖动缓冲）");
    drop(a.update(Message::WindowEvent(window::Event::Resized(iced::Size::new(
        1100.0, 608.0,
    )))));
    assert!(a.narrow());
}

#[test]
fn toggle_topmost_flips_config_and_draft() {
    let mut a = seeded_app();
    assert!(!a.config.always_on_top);
    // 无真实窗口时 find_window 落空，win32 调用安全成 no-op，逻辑照走。
    drop(a.update(Message::ToggleTopmost));
    assert!(a.config.always_on_top);
    assert!(a.cfg_draft.always_on_top, "草稿要同步，设置页不能因此显示未保存");
    drop(a.update(Message::ToggleTopmost));
    assert!(!a.config.always_on_top);
}

#[test]
fn running_instance_shows_stop() {
    let mut a = seeded_app();
    a.page = Page::Home;
    a.procs = vec![crate::core::procman::ProcStatus {
        profile: "demo".into(),
        port: 3080,
        url: "http://127.0.0.1:3080".into(),
        pid: 4321,
        uptime_secs: 75,
    }];
    assert!(has_text(&a, "停止运行"), "运行中应显示停止按钮");
    let msg = one(
        click(&a, "停止运行"),
        |m| matches!(m, Message::Stop(p) if p == "demo"),
        "Stop(demo)",
    );
    drop(a.update(msg));
}

// ------------------------------------------------ 首次启动目录引导

// 引导确认会改进程级全局（DATA_DIR / pointer / 磁盘），两个用例必须串行，
// 结束时把全局拨回测试锚定目录，不污染其它用例。
static OB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn restore_default_data_dir() {
    let _ = crate::core::store::write_pointer(&crate::core::store::boot_dir());
    crate::core::store::redirect_data_dir(crate::core::store::boot_dir());
}

#[test]
fn onboarding_confirm_defaults() {
    let _g = OB_LOCK.lock().unwrap();
    let mut a = app();
    a.onboarding = Some(Onboarding::new());
    a.update(Message::ObConfirm);
    assert!(a.onboarding.is_none(), "确认后引导应退场");
    assert_eq!(a.config.dsh_home, "", "默认 dsh-home 存空串（跟着数据目录走）");
    assert!(
        crate::core::store::config_exists(),
        "确认应写出 config.json"
    );
    assert!(
        !crate::core::store::pointer_path().exists() || {
            let p = std::fs::read_to_string(crate::core::store::pointer_path()).unwrap();
            p.trim() == crate::core::store::boot_dir().to_string_lossy()
        },
        "默认目录不应留下指向别处的 pointer"
    );
    restore_default_data_dir();
}

#[test]
fn onboarding_confirm_custom_dirs() {
    let _g = OB_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join("dshnext-tests-onboard");
    let _ = std::fs::remove_dir_all(&root);
    let launcher = root.join("data");
    let home = root.join("harness-home");

    let mut a = app();
    a.onboarding = Some(Onboarding::new());
    a.update(Message::ObEdit(
        PickField::LauncherDir,
        launcher.to_string_lossy().into_owned(),
    ));
    a.update(Message::ObEdit(
        PickField::DshHome,
        home.to_string_lossy().into_owned(),
    ));
    a.update(Message::ObConfirm);

    assert!(a.onboarding.is_none(), "确认后引导应退场");
    assert!(launcher.join("config.json").exists(), "config 应写进新数据目录");
    assert!(home.exists(), "dsh-home 应被创建");
    assert_eq!(
        a.config.dsh_home,
        home.to_string_lossy(),
        "自定义 dsh-home 应存绝对路径"
    );
    let ptr = std::fs::read_to_string(crate::core::store::pointer_path())
        .expect("自定义目录必须写 pointer");
    assert_eq!(ptr.trim(), launcher.to_string_lossy());
    // 会话内的路径全局应已改道
    assert_eq!(crate::core::store::data_dir(), launcher);

    // 清场：回到测试锚定目录，删 pointer 与临时树；home 覆盖也拨回去
    // （confirm 走了 set_home_dir，不拨会残留给其它用例）。
    restore_default_data_dir();
    crate::core::envres::set_home_dir(crate::core::store::boot_dir().join("home"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn onboarding_defaults_message_clears_drafts() {
    let mut a = app();
    a.onboarding = Some(Onboarding::new());
    a.update(Message::ObEdit(PickField::LauncherDir, r"D:\somewhere".into()));
    assert_eq!(a.onboarding.as_ref().unwrap().launcher_dir, r"D:\somewhere");
    a.update(Message::ObDefaults);
    let ob = a.onboarding.as_ref().unwrap();
    assert!(ob.launcher_dir.is_empty() && ob.dsh_home.is_empty());
}

#[test]
fn dirs_edit_migrates_everything() {
    let _g = OB_LOCK.lock().unwrap();
    let boot = crate::core::store::boot_dir();
    restore_default_data_dir();
    // 造内容：数据目录里一个散文件 + home/profiles 里一个文件
    std::fs::create_dir_all(boot.join("home").join("profiles"));
    std::fs::write(boot.join("probe.txt"), b"x").unwrap();
    std::fs::write(boot.join("home").join("profiles").join("p.txt"), b"y").unwrap();

    let root = std::env::temp_dir().join("dshnext-tests-migrate");
    let _ = std::fs::remove_dir_all(&root);
    let launcher = root.join("newdata");
    let home = root.join("newhome");

    crate::core::migrate::relocate(launcher.clone(), home.clone(), true).unwrap();

    assert_eq!(crate::core::store::data_dir(), launcher, "会话内应已改道");
    assert!(launcher.join("probe.txt").exists(), "散文件应搬走");
    assert!(!boot.join("probe.txt").exists(), "旧位置不应残留");
    assert!(home.join("profiles").join("p.txt").exists(), "home 内容应搬走");
    assert!(
        launcher.join("config.json").exists(),
        "config.json 应最后搬到新目录"
    );
    assert_eq!(
        crate::core::store::load().dsh_home,
        home.to_string_lossy(),
        "自定义 dsh-home 应落盘"
    );
    let ptr = std::fs::read_to_string(crate::core::store::pointer_path())
        .expect("改道必须写 pointer");
    assert_eq!(ptr.trim(), launcher.to_string_lossy());
    assert_eq!(crate::core::envres::home_dir(), home, "会话内 home 覆盖生效");

    // 清场：路径全局拨回锚定目录，home 覆盖也拨回去
    restore_default_data_dir();
    crate::core::envres::set_home_dir(boot.join("home"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dirs_edit_prefill_close_and_guard() {
    let _g = OB_LOCK.lock().unwrap();
    restore_default_data_dir();
    let mut a = app();
    a.onboarding = None; // OpenDirsEdit 与引导互斥，测试环境固定掉
    drop(a.update(Message::OpenDirsEdit));
    assert!(a.dirs_edit.as_ref().unwrap().first_run == false);
    assert!(has_text(&a, "保存并转移"), "编辑模式应显示转移确认按钮");
    // 实例在跑时确认被拦下（表单留着、不进入转移）
    a.procs = vec![crate::core::procman::ProcStatus {
        profile: "demo".into(),
        port: 3080,
        url: "http://127.0.0.1:3080".into(),
        pid: 1,
        uptime_secs: 1,
    }];
    drop(a.update(Message::ObConfirm));
    assert!(a.dirs_edit.is_some(), "有实例运行时确认应被拒绝");
    assert!(a.busy.is_none(), "被拦下不应进入忙状态");
    drop(a.update(Message::ObClose));
    assert!(a.dirs_edit.is_none(), "取消应关表单");
}
