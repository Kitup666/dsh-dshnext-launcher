//! 按钮（DESIGN.md §7.1 的落地件）。
//!
//! 对应 CSS `.btn` 一族：四类变体 × 三档尺寸 × 禁用态。
//! hover 过渡不读 `button::Status::Hovered`（那是阶跃的），而是读 `AnimState`
//! 里当前 key 的补间值 t∈[0,1]，颜色用 `theme::lerp` 插值——这就是 CSS
//! `transition: background .14s` 的等价物。Pressed/Disabled 仍走 Status。

use crate::theme::{self, Palette, R_CTL};
use crate::ui::anim::{self, AnimState};
use crate::ui::txt_bold;
use iced::gradient::Linear;
use iced::widget::{button, mouse_area};
use iced::{Background, Border, Color, Element, Gradient, Padding, Shadow, Theme};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// `.btn-primary`：accent 竖向渐变 + 彩色投影。
    Primary,
    /// `.btn`：surface_1 底 + 描边。
    Secondary,
    /// `.btn-danger`：纯 bad 底。
    Danger,
    /// `.btn-teal`：teal 渐变 + 深绿字。
    Teal,
    /// `.btn-quiet-danger`：透明底，hover 才变红。行内删除用。
    QuietDanger,
    /// `.btn-ghost`：完全透明，hover 浮出 hover 色。
    Ghost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// `.btn-sm`：30px。
    Small,
    /// `.btn`：36px。
    Medium,
    /// `.btn-hero`：48px。
    Hero,
}

impl Size {
    fn text_size(self) -> f32 {
        match self {
            // CSS 根字号 14px：.79rem≈11 / .83rem≈11.6 / .98rem≈13.7
            Size::Small => 11.0,
            Size::Medium => 11.6,
            Size::Hero => 13.7,
        }
    }
    fn radius(self) -> f32 {
        match self {
            Size::Small => 8.0,
            Size::Medium => R_CTL,
            Size::Hero => 12.0,
        }
    }
    /// 纵向 padding：目标总高 30/36/48，文字行高约 17/18/21。
    fn vpad(self) -> u16 {
        match self {
            Size::Small => 6,
            Size::Medium => 9,
            Size::Hero => 13,
        }
    }
    fn hpad(self) -> u16 {
        match self {
            Size::Small => 12,
            Size::Medium => 16,
            Size::Hero => 34,
        }
    }
}

/// 一个按钮的完整描述。`key` 是 hover 补间的标识，必须在同一窗口内唯一。
/// label 限 `'static`：界面文案全是字面量，省掉一层生命周期传染。
pub struct Spec {
    pub key: anim::Key,
    pub label: Cow<'static, str>,
    pub variant: Variant,
    pub size: Size,
    pub enabled: bool,
}

impl Spec {
    pub fn new(key: anim::Key, label: impl Into<Cow<'static, str>>, variant: Variant) -> Self {
        Self {
            key,
            label: label.into(),
            variant,
            size: Size::Medium,
            enabled: true,
        }
    }
    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.enabled = !d;
        self
    }
}

/// 构造按钮。`on_enter`/`on_exit` 由 app 层给出（通常是 `Message::HoverEnter(key)`）；
/// 传 `None` 或按钮禁用时不包 mouse_area——CSS 的 `:hover:not(:disabled)` 同理。
/// （`Message: Clone` 是 mouse_area 的要求，iced 所有交互控件都逃不掉。）
pub fn btn<'a, Message: Clone + 'a + 'static>(
    spec: Spec,
    pal: &'static Palette,
    anim: &AnimState,
    on_press: Option<Message>,
    on_enter: Option<Message>,
    on_exit: Option<Message>,
) -> Element<'a, Message> {
    let t = anim.value(spec.key);
    let enabled = spec.enabled;

    let inner = button(txt_bold(spec.label).size(spec.size.text_size()))
        .padding(Padding::from([spec.size.vpad(), spec.size.hpad()]))
        .on_press_maybe(on_press.filter(|_| enabled))
        .style(move |_theme: &Theme, status: button::Status| {
            style_for(pal, spec.variant, spec.size, enabled, t, status)
        });

    match (enabled, on_enter, on_exit) {
        (true, Some(enter), Some(exit)) => mouse_area(inner).on_enter(enter).on_exit(exit).into(),
        _ => inner.into(),
    }
}

fn style_for(
    pal: &'static Palette,
    variant: Variant,
    size: Size,
    enabled: bool,
    t: f32,
    status: button::Status,
) -> button::Style {
    let pressed = matches!(status, button::Status::Pressed);
    let radius = size.radius().into();
    let no_border = Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius,
    };

    let mut style = match variant {
        Variant::Secondary => button::Style {
            background: Some(theme::lerp(pal.surface_1, pal.surface_2, t).into()),
            text_color: pal.text,
            border: Border {
                color: theme::lerp(pal.border_mid, pal.border_hi, t),
                width: 0.3,
                radius,
            },
            shadow: pal.shadow_ctl,
            snap: true,
        },
        Variant::Primary => {
            // CSS: linear-gradient(180deg, accent-hi, accent)；hover brightness(1.06)。
            // iced 角度 π = 自上而下（Radians::to_distance 里 angle-90°，y 轴向下）。
            let f = if pressed { 0.97 } else { 1.0 + 0.06 * t };
            button::Style {
                background: Some(Background::Gradient(Gradient::Linear(
                    Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, theme::brighten(pal.accent_hi, f))
                        .add_stop(1.0, theme::brighten(pal.accent, f)),
                ))),
                text_color: pal.on_accent,
                border: no_border,
                shadow: pal.shadow_btn,
                snap: true,
            }
        }
        Variant::Danger => button::Style {
            background: Some(
                theme::brighten(pal.bad, if pressed { 0.94 } else { 1.0 + 0.08 * t }).into(),
            ),
            text_color: pal.on_accent,
            border: no_border,
            shadow: pal.shadow_ctl,
            snap: true,
        },
        Variant::Teal => {
            // CSS: linear-gradient(180deg, color-mix(teal 88%, white), teal)
            let f = if pressed { 0.96 } else { 1.0 + 0.05 * t };
            button::Style {
                background: Some(Background::Gradient(Gradient::Linear(
                    Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, theme::brighten(theme::mix_white(pal.teal, 0.12), f))
                        .add_stop(1.0, theme::brighten(pal.teal, f)),
                ))),
                text_color: pal.on_teal,
                border: no_border,
                shadow: Shadow {
                    color: theme::with_alpha(pal.teal, 0.5),
                    offset: iced::Vector::new(0.0, 6.0),
                    blur_radius: 16.0,
                },
                snap: true,
            }
        }
        Variant::QuietDanger => button::Style {
            background: Some(theme::lerp(Color::TRANSPARENT, pal.bad_soft, t).into()),
            text_color: theme::lerp(pal.text_2, pal.bad, t),
            border: Border {
                color: theme::lerp(pal.border_mid, pal.bad, t),
                width: 0.3,
                radius,
            },
            shadow: Shadow::default(),
            snap: true,
        },
        Variant::Ghost => button::Style {
            background: Some(theme::lerp(Color::TRANSPARENT, pal.hover, t).into()),
            text_color: theme::lerp(pal.text_2, pal.text, t),
            border: no_border,
            shadow: Shadow::default(),
            snap: true,
        },
    };

    // CSS `.btn:disabled { opacity: .4; box-shadow: none }`
    if !enabled {
        style.background = style.background.map(|bg| bg.scale_alpha(0.4));
        style.text_color = theme::with_alpha(style.text_color, 0.4);
        style.border.color = theme::with_alpha(style.border.color, 0.4);
        style.shadow = Shadow::default();
    }

    style
}
