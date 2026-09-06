//! 自造通用组件 + 文本纪律入口。
//!
//! 全项目文本必须走这里的 `txt`/`txt_bold`/`mono`，**禁止直接用 `iced::widget::text`**：
//! 封装里钉死了内嵌字体与 `Shaping::Advanced`（DESIGN.md §6 的双保险，
//! 漏一处就是满屏豆腐块）。

pub mod anim;
pub mod button;
pub mod card;
pub mod frosted;
pub mod glass_pipeline;
pub mod glow_mesh;
pub mod icon;
pub mod modal;
pub mod onboarding;
pub mod reveal;
pub mod titlebar;
pub mod widgets;

pub use reveal::reveal_at;

use iced::widget::text::{self, IntoFragment, Text};
use iced::{Font, font};
use std::borrow::Cow;

pub const SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-Regular.subset.ttf");
pub const SANS_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-SemiBold.subset.ttf");
pub const MONO: &[u8] = include_bytes!("../../assets/fonts/CascadiaMono.subset.ttf");
/// Display 层（品牌名 / hero / overline / 统计数字）：Martian Mono SemiExpanded，
/// 只子集了 ASCII——中文字符自动回落 Noto，混排文本（如 hero 的中文 profile 名）安全。
pub const DISPLAY: &[u8] = include_bytes!("../../assets/fonts/MartianMono-Regular.subset.ttf");
pub const DISPLAY_BOLD: &[u8] = include_bytes!("../../assets/fonts/MartianMono-Bold.subset.ttf");

pub const FONT_SANS: Font = Font::with_name("Noto Sans SC");
/// 子集字体已写 name ID 16/17，SemiBold 注册在同一家族下，这条才拿得到（§6 坑 3）。
pub const FONT_SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..FONT_SANS
};
pub const FONT_MONO: Font = Font::with_name("Cascadia Mono");
pub const FONT_DISPLAY: Font = Font::with_name("Martian Mono");
pub const FONT_DISPLAY_BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..FONT_DISPLAY
};

pub fn load_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        SANS.into(),
        SANS_SEMIBOLD.into(),
        MONO.into(),
        DISPLAY.into(),
        DISPLAY_BOLD.into(),
    ]
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

/// 机器声 display：品牌名、overline、hero、统计值。Martian Mono 只盖 ASCII，
/// 中文自动回落 Noto（fontdb per-script fallback）。
pub fn disp<'a>(content: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(content)
        .font(FONT_DISPLAY)
        .shaping(text::Shaping::Advanced)
}

/// display 加粗（hero、品牌名）。
pub fn disp_bold<'a>(content: impl IntoFragment<'a>) -> Text<'a> {
    iced::widget::text(content)
        .font(FONT_DISPLAY_BOLD)
        .shaping(text::Shaping::Advanced)
}
