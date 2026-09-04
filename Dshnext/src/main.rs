//! Dshnext 入口。
//!
//! 用法：
//! ```text
//! dshnext [--theme dark|light] [--shot 路径 [--after 毫秒]]
//!         [--page home|profiles|plugins|env|console|settings]
//!         [--drawlog] [--autotest] [--e2e] [--all-backends]
//! ```
//!
//! 阶段 3 已完成：六个页面全部移植，后端真实接通。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bridge;
mod core;
mod pages;
mod theme;
mod ui;
mod update;

#[cfg(test)]
mod tests;

use app::{Dshnext, Mode, Shot};
use pages::Page;
use std::time::Duration;

fn main() -> iced::Result {
    env_logger::Builder::from_env(
        // iced_wgpu 默认 info 会把整个适配器列表打出来；这台机器枚举出 6 个重复的
        // RTX 4060，光格式化+写这坨就几百 ms（GUI 进程写 stderr 更慢）。诊断后端选择
        // 时临时 RUST_LOG=iced_wgpu=info 再打开。
        env_logger::Env::default().default_filter_or("dshnext=info,iced_wgpu=warn,wgpu_core=warn"),
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

    // core 事件通道：发送端进全局供后端用，接收端等 subscription 取走。
    // 必须在 application 之前——view 第一次跑就可能要 sink()。
    bridge::init();

    // --page：直接开在某一页，出图验收时省得点。
    let start_page = match opt("--page").as_deref() {
        Some("profiles") => Page::Profiles,
        Some("plugins") => Page::Plugins,
        Some("env") => Page::Env,
        Some("console") => Page::Console,
        Some("settings") => Page::Settings,
        _ => Page::Home,
    };

    let e2e = flag("--e2e");
    let mut application = iced::application(
        move || {
            let mut app = Dshnext::new(mode, shot.clone(), autotest, e2e);
            app.page = start_page;
            app
        },
        Dshnext::update,
        pages::view,
    )
    .title("DshDesk — DeepSeek Harness 启动器")
    .window(iced::window::Settings {
        // --tall：出图验收用，把长页面（设置页）一屏截完
        size: if flag("--tall") {
            iced::Size::new(1280.0, 1400.0)
        } else {
            iced::Size::new(1280.0, 860.0)
        },
        // 去掉系统标题栏与缩放边框，改自绘（src/ui/titlebar.rs）。
        decorations: false,
        platform_specific: iced::window::settings::PlatformSpecific {
            // 阴影必须留着：无边框 + 无阴影时窗口和桌面糊成一片，边界看不出来。
            // 代价是顶部会多出 1px 线（iced 文档明说），可以接受。
            undecorated_shadow: true,
            // 圆角交给 DWM 做（Win11 22000+）。自己在容器上画圆角边框会和方形的
            // 窗口表面对不齐，角上露出直角——让系统裁，内容只管填满。
            corner_preference: iced::window::settings::platform::CornerPreference::Round,
            ..Default::default()
        },
        min_size: Some(iced::Size::new(880.0, 560.0)),
        ..Default::default()
    })
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

/// 把 `window::screenshot` 的结果写成 PNG。`--shot` 用。
pub fn write_png(path: &str, shot: &iced::window::Screenshot) {
    let file = match std::fs::File::create(path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("创建 {path} 失败：{e}");
            return;
        }
    };
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        shot.size.width,
        shot.size.height,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Err(e) = enc
        .write_header()
        .and_then(|mut w| w.write_image_data(&shot.rgba))
    {
        log::error!("写 PNG 失败：{e}");
    }
}
