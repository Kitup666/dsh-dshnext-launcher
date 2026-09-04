//! 卡片：无边框，靠软阴影浮起（DESIGN.md §6「去框化」）。
//! 对应 CSS `.card { background: surface-1; border: none; border-radius: 16px; box-shadow: var(--shadow-card) }`。

use crate::theme::{Palette, R_CARD};
use crate::ui::{txt, txt_bold};
use iced::widget::{Column, container};
use iced::{Border, Color, Element, Fill, Padding, Theme};
use iced::widget::text::IntoFragment;

/// 标准卡片容器：padding 24/26，圆角 16，shadow_card。
pub fn card<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(content)
        .width(Fill)
        .padding(Padding::from([24, 26]))
        .style(card_style(pal))
        .into()
}

#[allow(dead_code)] // 阶段 3 列表页用
/// 贴边卡片（CSS `.card.flush`）：横向 padding 交给行自己管，列表页用。
pub fn card_flush<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(content)
        .width(Fill)
        .padding(Padding::from([8, 0]))
        .style(card_style(pal))
        .into()
}

/// 闭包捕获 `&'static Palette`（Copy、'static）——widget 的 `.style()` 闭包
/// 没有 HRTB 问题（那是 program builder `.theme()` 的坑），阶段 0 已验证。
pub fn card_style(
    pal: &'static Palette,
) -> impl Fn(&Theme) -> container::Style + Copy + 'static {
    move |_theme: &Theme| container::Style {
        text_color: Some(pal.text),
        background: Some(pal.surface_1.into()),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: R_CARD.into(),
        },
        shadow: pal.shadow_card,
        snap: true,
    }
}

/// 卡片标题（CSS `.card-title`：0.95rem / 640 → 13.5px SemiBold）。
/// 颜色不显式设，继承 container style 的 `text_color`。
pub fn card_title<'a, Message: 'a>(s: impl IntoFragment<'a>) -> Element<'a, Message> {
    txt_bold(s).size(13.5).into()
}

/// 卡片副标题（CSS `.card-sub`：0.8rem / text-3）。
pub fn card_sub<'a, Message: 'a>(
    s: impl IntoFragment<'a>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    txt(s).size(11.5).color(pal.text_3).into()
}

#[allow(dead_code)] // 阶段 3 表单页用
/// 卡片区块：标题 + 副标题 + 内容，纵向排好间距（CSS `.section`）。
pub fn section<'a, Message: 'a>(
    title: impl IntoFragment<'a>,
    sub: impl IntoFragment<'a>,
    body: Column<'a, Message>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(
        Column::new()
            .push(card_title(title))
            .push(card_sub(sub, pal))
            .spacing(3)
            .push(body.spacing(14)),
    )
    .width(Fill)
    .into()
}
