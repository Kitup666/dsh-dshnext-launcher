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

use crate::theme::{self, FS_SMALL, FS_TITLE, Palette, R_CARD, R_HERO};
use crate::ui::{txt, txt_bold};
use iced::widget::text::IntoFragment;
use iced::widget::{Column, container};
use iced::{Border, Color, Element, Fill, Length, Padding, Shadow, Theme};

/// 标准卡片容器：padding 24，圆角 16，shadow_card，磨砂（glass + 卡内颗粒）。
pub fn card<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    frosted(content, pal, Padding::from(24), R_CARD, pal.shadow_card, Length::Shrink)
}

/// 满高卡片：控制台等要占满剩余高度的容器（Frosted 默认 Shrink 跟随内容）。
pub fn card_fill<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    frosted(content, pal, Padding::from(24), R_CARD, pal.shadow_card, Length::Fill)
}

/// 玻璃卡面的**等效不透明色**：shader 里 base 烤 navy 6% 再过 overlay 修正
/// 的结果（远离光球处的卡面平区就是这个色）。给不进 shader 的小玻璃面
/// （侧边栏选中胶囊）保持同色。亮色不修正，返回 glass 原色。
pub fn glass_face(pal: &Palette) -> Color {
    if pal.grain <= 0.0 {
        return pal.glass;
    }
    let base = theme::lerp(pal.surface_1, GLASS_HUE_BOT, 0.06);
    let gray = 0.5
        + (crate::ui::glass_pipeline::OVERLAY_GRAY - 0.5)
            * crate::ui::glass_pipeline::OVERLAY_STRENGTH;
    let f = |c: f32| {
        if c < 0.5 { 2.0 * c * gray } else { 1.0 - 2.0 * (1.0 - c) * (1.0 - gray) }
    };
    Color { r: f(base.r), g: f(base.g), b: f(base.b), a: 1.0 }
}

/// 英雄卡：比 `card()` 高一档（更大圆角 + 更宽内边距 + 更深阴影）。
pub fn card_hero<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    frosted(content, pal, Padding::from(32), R_HERO, pal.shadow_hero, Length::Shrink)
}

/// 磨砂卡：Frosted 自定义容器（glass 底 + 卡内颗粒 + 内容在颗粒之上）。
fn frosted<'a, Message: 'static>(
    content: impl Into<Element<'a, Message>>,
    pal: &'static Palette,
    padding: Padding,
    radius: f32,
    shadow: Shadow,
    height: Length,
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
    .height(height)
    .into()
}

/// 卡面样式：**不透明** surface_1 底——磨砂的「透」由 Frosted 画进卡内的
/// 同参光球承担，真半透明底下没东西可透只剩噪声（2026-09-06 教训）。
/// 底色带 160° sheen 渐变（左上混 4% 白 → 落回本色）：光打在玻璃上的
/// 高光，玻璃感的主要来源之一。+ 1px 白描边勾轮廓（借鉴 orevx）。
///
/// 颜色修正层（Apple/Fluent 的做法，zhihu p/657181578）：暗色毛玻璃在
/// 模糊层之上还有一道 overlay 混合，恢复饱和度避免「死灰死灰的」。iced
/// 没有混合模式，等价近似是把修正烤进底色：卡面向环境光的色相偏 6%、
/// sheen 用冷白——卡面像是被同一片环境光照着，而不是中性灰盖在渐变上。
const GLASS_HUE_BOT: Color = Color::from_rgb8(0x1e, 0x3a, 0x8a); // ambient_bot 色相
const GLASS_HUE_TOP: Color = Color::from_rgb8(0x4f, 0x46, 0xe5); // ambient_top 色相
fn surface_style(pal: &'static Palette, radius: f32, shadow: Shadow) -> container::Style {
    let dark = pal.grain > 0.0;
    let base = if dark {
        theme::lerp(pal.surface_1, GLASS_HUE_BOT, 0.06)
    } else {
        pal.surface_1
    };
    // sheen 白里掺 10% 环境顶色相 → 冷白高光
    let cool_white = theme::lerp(Color::WHITE, GLASS_HUE_TOP, 0.10);
    let sheen = theme::lerp(base, cool_white, 0.045);
    container::Style {
        text_color: Some(pal.text),
        background: Some(iced::Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(iced::Degrees(160.0))
                .add_stop(0.0, sheen)
                .add_stop(0.4, base),
        ))),
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
