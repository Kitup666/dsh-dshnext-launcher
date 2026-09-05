//! 卡片：暗色靠 1px 白描边 + 色阶差浮起（借鉴 orevx glass-dark），
//! 亮色靠软阴影浮起（上一代设计）。card_border 令牌区分两者。
//!
//! 假高斯模糊（2026-09-06）：暗色卡面 = 半透明 `pal.glass` + 卡内颗粒层
//! （`ui::frosted::Frosted`，布局跟随内容的自定义容器——stack 拼法会被
//! Fill 撑爆，颗粒漏到卡片下面）。颗粒**只属于卡片**：全窗铺的教训是
//! 颗粒直接铺在背景上（用户抓的），磨砂是玻璃的属性，只能属于玻璃本身。
//! 亮色 `pal.grain = 0`，Frosted 只画不透明白卡，颗粒零采样。
//!
//! 高度分两档（`frontend-design` 的 elevation 纪律：**一套**阴影阶梯按层级取用，
//! 不是每个盒子自己挑一个）：
//! - `card()`：普通内容容器，R_CARD 16 + shadow_card
//! - `card_hero()`：页面英雄区，圆角与阴影都升一档（R_HERO 20 + shadow_hero）。
//!   首页 hero 用它——两者视觉重量相同的话，hero 和它下面的列表卡分不出主次。

use crate::theme::{FS_SMALL, FS_TITLE, Palette, R_CARD, R_HERO};
use crate::ui::{txt, txt_bold};
use iced::widget::text::IntoFragment;
use iced::widget::{Column, container};
use iced::{Border, Element, Fill, Padding, Shadow, Theme};

/// 标准卡片容器：padding 24，圆角 16，shadow_card，磨砂（glass + 卡内颗粒）。
pub fn card<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    frosted(content, pal, Padding::from(24), R_CARD, pal.shadow_card)
}

/// 英雄卡：比 `card()` 高一档（更大圆角 + 更宽内边距 + 更深阴影）。
pub fn card_hero<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    frosted(content, pal, Padding::from(32), R_HERO, pal.shadow_hero)
}

/// 磨砂卡：Frosted 自定义容器（glass 底 + 卡内颗粒 + 内容在颗粒之上）。
fn frosted<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
    padding: Padding,
    radius: f32,
    shadow: Shadow,
) -> Element<'a, Message> {
    // 伪造 backdrop 的光球只在暗色给（grain>0 即暗色）；亮色 None。
    let backdrop = if pal.grain > 0.0 {
        Some(crate::ui::glow_mesh::ambient_specs(pal))
    } else {
        None
    };
    crate::ui::frosted::Frosted::new(
        content,
        surface_style(pal, radius, shadow),
        padding,
        pal.grain,
        backdrop,
    )
    .into()
}

/// 卡面样式：**不透明** surface_1 底——磨砂的「透」由 Frosted 画进卡内的
/// 柔化光球承担，真半透明底下没东西可透只剩噪声（2026-09-06 教训）。
/// + 1px 白描边勾轮廓（黑底上黑阴影不可见，借鉴 orevx）；亮色白卡+阴影。
fn surface_style(pal: &'static Palette, radius: f32, shadow: Shadow) -> container::Style {
    container::Style {
        text_color: Some(pal.text),
        background: Some(pal.surface_1.into()),
        border: Border {
            color: pal.card_border,
            width: 0.3,
            radius: radius.into(),
        },
        shadow,
        snap: true,
    }
}

/// 闭包捕获 `&'static Palette`（Copy、'static）——widget 的 `.style()` 闭包
/// 没有 HRTB 问题（那是 program builder `.theme()` 的坑），阶段 0 已验证。
/// 控制台视口等非磨砂容器也用它（暗色拿到同一块 glass 底，无颗粒）。
pub fn card_style(
    pal: &'static Palette,
) -> impl Fn(&Theme) -> container::Style + Copy + 'static {
    move |_theme: &Theme| surface_style(pal, R_CARD, pal.shadow_card)
}

/// 卡片标题（CSS `.card-title`：0.95rem / 640 → 13.5px SemiBold）。
/// 颜色不显式设，继承 container style 的 `text_color`。
pub fn card_title<'a, Message: 'a>(s: impl IntoFragment<'a>) -> Element<'a, Message> {
    txt_bold(s).size(FS_TITLE).into()
}

/// 卡片副标题（CSS `.card-sub`：0.8rem / text-3）。
pub fn card_sub<'a, Message: 'a>(
    s: impl IntoFragment<'a>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    txt(s).size(FS_SMALL).color(pal.text_3).into()
}

#[allow(dead_code)] // 阶段 3 列表页用
/// 贴边卡片（CSS `.card.flush`）：横向 padding 交给行自己管，列表页用。
/// 旧结构（container + style），不做磨砂——列表行自带底色，颗粒会打架。
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
