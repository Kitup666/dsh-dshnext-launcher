//! Dshnext 库入口：启动器 bin（src/main.rs）与桌面窗口宿主 bin
//! （webui/，产物 DeepseekHarness.exe）共享同一份代码。宿主只需要
//! `core::webview::host_main`，但链接的是整个 iced 应用（磁盘 +~20 MB，
//! 运行时不碰 GPU）。

pub mod app;
pub mod bridge;
pub mod core;
pub mod pages;
pub mod theme;
pub mod tray;
pub mod ui;
pub mod update;
pub mod win32;

#[cfg(test)]
mod tests;

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
