//! Dshnext 入口。
//!
//! 用法：`dshnext [--shot 路径] [--after 毫秒] [--theme dark|light] [--all-backends]`
//!
//! 阶段 1 只到「视觉地基」：没有后端、没有页面路由，整个程序就是 app.rs 的 demo 页。
//! 后端接入在阶段 2（core/ 解耦 + EventSink），页面移植在阶段 3。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod theme;
mod ui;

use app::{Dshnext, Mode, Shot};
use std::time::Duration;

fn main() -> iced::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("dshnext=info,iced_wgpu=info,wgpu_core=warn"),
    )
    .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    let mode = match opt("--theme").as_deref() {
        Some("light") => Mode::Light,
        _ => Mode::Dark,
    };
    let shot = opt("--shot").map(|path| Shot {
        path,
        after: opt("--after")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1600),
    });

    // 性能纪律 §8-3：限定 GPU 后端为 DX12，白省 38 MB（阶段 0 实测 117.8 → 79.8）。
    // wgpu 在 compositor 创建时才读这个变量，晚于 main，所以进程内设置有效。
    // 外部已设 WGPU_BACKEND 或 ICED_BACKEND=tiny-skia 时不覆盖；
    // --all-backends 留给复现阶段 0 的内存归因。
    if !flag("--all-backends") && std::env::var_os("WGPU_BACKEND").is_none() {
        // SAFETY: 单线程启动阶段，尚未创建窗口或后台线程。
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }
    log::info!(
        "mode={mode:?} WGPU_BACKEND={:?} ICED_BACKEND={:?} shot={:?}",
        std::env::var("WGPU_BACKEND").ok(),
        std::env::var("ICED_BACKEND").ok(),
        shot.as_ref().map(|s| (s.path.clone(), s.after)),
    );

    // 每 5s 报一次真实绘制次数（独立线程，不进 iced 事件循环，不造帧）。
    let autotest = flag("--autotest");
    if autotest || flag("--drawlog") {
        std::thread::spawn(|| {
            let mut last = 0u64;
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let now = app::DRAWS.load(std::sync::atomic::Ordering::Relaxed);
                log::info!("draws total={now} delta={} (过去 5s)", now - last);
                last = now;
            }
        });
    }

    let mut application = iced::application(
        move || Dshnext::new(mode, shot.clone(), autotest),
        Dshnext::update,
        Dshnext::view,
    )
    .title("DshDesk — DeepSeek Harness 启动器")
    .window_size((1280.0, 860.0))
    .theme(theme_of)
    .style(app_background)
    .subscription(Dshnext::subscription)
    .default_font(ui::FONT_SANS);

    for font in ui::load_fonts() {
        application = application.font(font);
    }

    application.run()
}

/// 具名函数：program builder 的 `.theme()` 传闭包会撞 HRTB（阶段 0 坑 2）。
fn theme_of(state: &Dshnext) -> iced::Theme {
    match state.mode {
        Mode::Dark => iced::Theme::Dark,
        Mode::Light => iced::Theme::Light,
    }
}

fn app_background(state: &Dshnext, _theme: &iced::Theme) -> iced::theme::Style {
    let pal = state.palette();
    iced::theme::Style {
        background_color: pal.bg_app,
        text_color: pal.text,
    }
}
