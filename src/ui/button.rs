//! 按钮（DESIGN.md §7.1 的落地件）。
//!
//! 对应 CSS `.btn` 一族：四类变体 × 三档尺寸 × 禁用态。
//! hover 过渡不读 `button::Status::Hovered`（那是阶跃的），而是读 `AnimState`
//! 里当前 key 的补间值 t∈[0,1]，颜色用 `theme::lerp` 插值——这就是 CSS
//! `transition: background .14s` 的等价物。Pressed/Disabled 仍走 Status。

use crate::theme::{self, FS_SMALL, FS_TINY, FS_TITLE, Palette, R_CTL};
use crate::ui::anim::{self, AnimState};
use crate::ui::txt_bold;
use iced::gradient::Linear;
use iced::widget::{button, mouse_area};
use iced::{Background, Border, Color, Element, Gradient, Padding, Shadow, Theme};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// `.btn-primary`：accent 竖向渐变 + 彩色投影。**一屏只许有一个**
    /// （`frontend-design` 的 CTA 纪律）：渐变+投影是最重的强调手段，列表里每行
    /// 来一个的话，谁都不显眼，只显得花。行内的正向动作用 `Accent`。
    Primary,
    /// 行内正向动作：surface 底 + accent 描边 + accent 字。比 Secondary 显眼、
    /// 比 Primary 安静，一屏出现多次也不会打架（版本管理每行的「启动」、
    /// 环境页三行的「安装」、插件市场每行的「安装」都是它）。
    Accent,
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
        // 走统一字号阶梯（theme.rs）。上一代按 rem 直译出 11 / 11.6 / 13.7 三个
        // 邻近怪值，差 0.6px 谁也看不出，只是让阶梯多出三档。
        match self {
            Size::Small => FS_TINY,
            Size::Medium => FS_SMALL,
            Size::Hero => FS_TITLE,
        }
    }
    /// 圆角随层级走（`frontend-design` 的 elevation 纪律）：小控件贴合内容用小圆角，
    /// hero 是页面主 CTA，圆角要和它上面的卡片（R_CARD 16）成比例，不能和小按钮同值。
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
            // 按压反馈（motion-designer 微交互 100–200ms 一档）：底色沉到
            // surface_3、描边到最亮——即时阶跃即可，压着的那一下要「有手感」。
            background: Some(
                theme::lerp(pal.surface_1, pal.surface_2, t).into(),
            ),
            text_color: pal.text,
            border: Border {
                color: if pressed {
                    pal.border_hi
                } else {
                    theme::lerp(pal.border_mid, pal.border_hi, t)
                },
                width: 0.3,
                radius,
            },
            shadow: pal.shadow_ctl,
            snap: true,
        },
        Variant::Accent => button::Style {
            // 底色只在 surface 之间插值（中性），accent 只出现在描边和文字上
            // ——§7.5 第 15 条：带色相的半透明叠色在物理混色下会被放大。
            background: Some(
                if pressed {
                    pal.surface_3
                } else {
                    theme::lerp(pal.surface_1, pal.surface_2, t)
                }
                .into(),
            ),
            text_color: theme::lerp(pal.accent, pal.accent_hi, t),
            border: Border {
                color: theme::lerp(pal.accent_line, pal.accent, t),
                width: 0.8,
                radius,
            },
            shadow: Shadow::default(),
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
            background: Some(
                if pressed {
                    pal.bad_soft
                } else {
                    theme::lerp(Color::TRANSPARENT, pal.bad_soft, t)
                }
                .into(),
            ),
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
            background: Some(
                if pressed {
                    // 按住时 hover 叠色加深一倍（4% → 8%）。
                    theme::with_alpha(pal.hover, (pal.hover.a * 2.0).min(1.0))
                } else {
                    theme::lerp(Color::TRANSPARENT, pal.hover, t)
                }
                .into(),
            ),
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
