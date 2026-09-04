//! 描边图标（DESIGN.md §7.5）。
//!
//! 上一代是内联 SVG（lucide 风格，1.7px 描边、单色）。这里把同样的路径存成
//! `assets/icons/*.svg` 用 `include_bytes!` 编进 exe。
//!
//! **动态换色已验证**（阶段 1）：`svg::Style { color: Some(c) }` 走的是渲染后
//! 像素级 RGB 替换（保留 alpha），对单色描边图标等价于 `currentColor`。
//! 多色图标不适用——本套图标全是单色。

use iced::widget::svg::{self, Handle};
use iced::{Color, Element, Length, Theme};

pub const LAUNCH: &[u8] = include_bytes!("../../assets/icons/launch.svg");
pub const VERSIONS: &[u8] = include_bytes!("../../assets/icons/versions.svg");
pub const PLUGINS: &[u8] = include_bytes!("../../assets/icons/plugins.svg");
pub const ENV: &[u8] = include_bytes!("../../assets/icons/env.svg");
pub const CONSOLE: &[u8] = include_bytes!("../../assets/icons/console.svg");
pub const SETTINGS: &[u8] = include_bytes!("../../assets/icons/settings.svg");

/// 以指定颜色绘制一个图标。
pub fn icon<'a, Message: 'a>(data: &'static [u8], size: impl Into<Length>, color: Color) -> Element<'a, Message> {
    let size: Length = size.into();
    svg::Svg::new(Handle::from_memory(data))
        .width(size)
        .height(size)
        .style(move |_theme: &Theme, _status: svg::Status| svg::Style {
            color: Some(color),
        })
        .into()
}
