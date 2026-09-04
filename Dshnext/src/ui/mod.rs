//! 自造通用组件 + 文本纪律入口。
//!
//! 全项目文本必须走这里的 `txt`/`txt_bold`/`mono`，**禁止直接用 `iced::widget::text`**：
//! 封装里钉死了内嵌字体与 `Shaping::Advanced`（DESIGN.md §6 的双保险，
//! 漏一处就是满屏豆腐块）。

pub mod anim;
pub mod button;
pub mod card;
pub mod icon;

use iced::widget::text::{self, IntoFragment, Text};
use iced::{Font, font};
use std::borrow::Cow;

pub const SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-Regular.subset.ttf");
pub const SANS_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-SemiBold.subset.ttf");
pub const MONO: &[u8] = include_bytes!("../../assets/fonts/CascadiaMono.subset.ttf");

pub const FONT_SANS: Font = Font::with_name("Noto Sans SC");
/// 子集字体已写 name ID 16/17，SemiBold 注册在同一家族下，这条才拿得到（§6 坑 3）。
pub const FONT_SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..FONT_SANS
};
pub const FONT_MONO: Font = Font::with_name("Cascadia Mono");

pub fn load_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![SANS.into(), SANS_SEMIBOLD.into(), MONO.into()]
}

/// 正文文本：内嵌 Noto Sans SC + Advanced shaping。
/// 入参用 `IntoFragment<'a>`（`text()` 的真实约束）。**不能钉死 `'static`**：
/// `Text<'a>` 对 `'a` 不变（invariant），钉死后塞不进短生命周期的 `Column`。
pub fn txt<'a>(content: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(content)
        .font(FONT_SANS)
        .shaping(text::Shaping::Advanced)
}

/// 标题/按钮文本：SemiBold 字重。
pub fn txt_bold<'a>(content: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(content)
        .font(FONT_SEMIBOLD)
        .shaping(text::Shaping::Advanced)
}

/// 等宽文本：路径、版本号、日志。Cascadia Mono 数字天然等宽（§6 坑 1）。
pub fn mono<'a>(content: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(content)
        .font(FONT_MONO)
        .shaping(text::Shaping::Advanced)
}
