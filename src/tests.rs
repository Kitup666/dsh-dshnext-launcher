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

use crate::app::{Dshnext, Message, Mode, PluginTab};
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
    Dshnext::new(Mode::Dark, None, false, false)
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
    assert!(
        has_text(&a, "暂无输出。启动 harness 或安装插件后，日志会实时出现在这里。"),
        "清空后应显示空状态"
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
