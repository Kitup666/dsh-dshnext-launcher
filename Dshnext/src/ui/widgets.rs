//! 页面通用小件：标签、列表行、空状态、忙提示、输入框、复选框、下拉、分段控件。
//! 逐条对应上一代 `styles.css` 里的 `.tag` / `.list-row` / `.empty` / `.input` …

use crate::theme::{self, Palette, R_CTL, R_PILL};
use crate::ui::anim::AnimState;
use crate::ui::{FONT_MONO, FONT_SANS, mono, txt, txt_bold};
use iced::widget::text::IntoFragment;
use iced::widget::{
    Column, Row, checkbox, column, container, mouse_area, pick_list, row, space, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding, Shadow, Theme};

/// 标签色调（对应 `.tag-ok` / `.tag-warn` / `.tag-bad` / `.tag-accent` / `.tag-teal`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Ok,
    Warn,
    Bad,
    Accent,
    /// teal 目前没有使用点（上一代给「已装 bundle」用过），令牌留着备用。
    #[allow(dead_code)]
    Teal,
}

impl Tone {
    fn colors(self, pal: &Palette) -> (Color, Color) {
        match self {
            Tone::Neutral => (pal.surface_2, pal.text_2),
            Tone::Ok => (pal.ok_soft, pal.ok),
            Tone::Warn => (pal.warn_soft, pal.warn),
            Tone::Bad => (pal.bad_soft, pal.bad),
            Tone::Accent => (pal.accent_soft, pal.accent),
            Tone::Teal => (pal.teal_soft, pal.teal),
        }
    }
}

/// 药丸标签（CSS `.tag`：0.71rem/600，圆角 999）。
pub fn tag<'a, Message: 'a>(
    label: impl IntoFragment<'a>,
    tone: Tone,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let (bg, fg) = tone.colors(pal);
    container(txt_bold(label).size(10).color(fg))
        .padding(Padding::from([2, 9]))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(fg),
            background: Some(bg.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: R_PILL.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 状态圆点（CSS `.dot`）。`live` 时用 ok 色——脉冲动画不做，静态点已足够。
pub fn dot<'a, Message: 'a>(color: Color) -> Element<'a, Message> {
    container(space::Space::new())
        .width(7.0)
        .height(7.0)
        .style(move |_theme: &Theme| container::Style {
            text_color: None,
            background: Some(color.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: R_PILL.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 图标徽章（CSS `.icon-badge`：42×42 圆角方块）。
pub fn icon_badge<'a, Message: 'a>(
    inner: Element<'a, Message>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(inner)
        .width(38.0)
        .height(38.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text_2),
            background: Some(pal.surface_2.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 11.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 列表行（CSS `.list-row`）：左图标 + 主内容 + 右操作，行间用 hover 高亮。
/// `key` 为 None 时不做 hover 过渡（列表行数量随数据变，静态 key 不够用）。
pub fn list_row<'a, Message: Clone + 'a>(
    badge: Option<Element<'a, Message>>,
    main: Column<'a, Message>,
    actions: Row<'a, Message>,
    selected: bool,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let mut r = Row::new().spacing(15).align_y(Alignment::Center);
    if let Some(b) = badge {
        r = r.push(b);
    }
    r = r
        .push(container(main.spacing(3)).width(Fill))
        .push(actions.spacing(7).align_y(Alignment::Center));

    container(r)
        .width(Fill)
        .padding(Padding::from([13, 0]))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text),
            background: Some(if selected {
                pal.row_selected.into()
            } else {
                Background::Color(Color::TRANSPARENT)
            }),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: R_CTL.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 行间分隔线（CSS `.list-row + .list-row { box-shadow: 0 -1px 0 var(--border) }`）。
pub fn divider<'a, Message: 'a>(pal: &'static Palette) -> Element<'a, Message> {
    container(space::Space::new())
        .width(Fill)
        .height(1.0)
        .style(move |_theme: &Theme| container::Style {
            text_color: None,
            background: Some(pal.border.into()),
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 安静空状态（CSS `.empty`：无虚线框、无图标圈）。
pub fn empty<'a, Message: 'a>(
    text_: impl IntoFragment<'a>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(txt(text_).size(12).color(pal.text_3))
        .width(Fill)
        .padding(Padding::from([46, 24]))
        .align_x(Alignment::Center)
        .into()
}

/// 忙提示（CSS `.spinner` + 文案）。转圈动画需要常驻订阅，这里用静态圆点替代
/// ——空闲零占用的纪律比一个转圈重要（DESIGN.md §8 第 1 条）。
pub fn busy<'a, Message: 'a>(
    text_: impl IntoFragment<'a>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    row![
        dot(pal.accent),
        txt(text_).size(11.5).color(pal.text_2),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// 表单字段：标签 + 控件 + 提示（CSS `.field` / `.field-label` / `.field-hint`）。
pub fn field<'a, Message: 'a>(
    label: impl IntoFragment<'a>,
    control: Element<'a, Message>,
    hint: Option<&'a str>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let mut col = column![
        txt_bold(label).size(11).color(pal.text_2),
        control,
    ]
    .spacing(6);
    if let Some(h) = hint {
        col = col.push(txt(h).size(10.5).color(pal.text_3));
    }
    col.width(Fill).into()
}

/// 键值行（CSS `.kv`）。
pub fn kv<'a, Message: 'a>(
    key: &'static str,
    value: impl IntoFragment<'a>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    row![
        container(txt(key).size(11).color(pal.text_2)).width(Length::Fixed(96.0)),
        mono(value).size(11).color(pal.text_3),
    ]
    .spacing(12)
    .into()
}

/// 单行输入框（CSS `.input`）。`is_mono` 给路径、API key 之类用等宽。
pub fn input<'a, Message: Clone + 'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    is_mono: bool,
    pal: &'static Palette,
) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .padding(Padding::from([9, 12]))
        .size(11.5)
        .font(if is_mono { FONT_MONO } else { FONT_SANS })
        .style(move |_theme: &Theme, status: text_input::Status| {
            let focused = matches!(status, text_input::Status::Focused { .. });
            let hovered = matches!(status, text_input::Status::Hovered);
            text_input::Style {
                background: pal.input_bg.into(),
                border: Border {
                    color: if focused {
                        pal.accent
                    } else if hovered {
                        pal.border_hi
                    } else {
                        pal.border_mid
                    },
                    width: 1.0,
                    radius: R_CTL.into(),
                },
                icon: pal.text_3,
                placeholder: pal.text_3,
                value: pal.text,
                selection: pal.accent_soft,
            }
        })
        .into()
}

/// 密码输入（API key）。`secure` 时显示圆点。
pub fn secret_input<'a, Message: Clone + 'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    secure: bool,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let mut ti = text_input(placeholder, value)
        .on_input(on_input)
        .padding(Padding::from([9, 12]))
        .size(11.5)
        .font(FONT_MONO)
        .style(move |_theme: &Theme, status: text_input::Status| {
            let focused = matches!(status, text_input::Status::Focused { .. });
            text_input::Style {
                background: pal.input_bg.into(),
                border: Border {
                    color: if focused { pal.accent } else { pal.border_mid },
                    width: 1.0,
                    radius: R_CTL.into(),
                },
                icon: pal.text_3,
                placeholder: pal.text_3,
                value: pal.text,
                selection: pal.accent_soft,
            }
        });
    if secure {
        ti = ti.secure(true);
    }
    ti.into()
}

/// 复选框（CSS `.checkbox`）。
pub fn check<'a, Message: Clone + 'a>(
    label: &'a str,
    value: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
    pal: &'static Palette,
) -> Element<'a, Message> {
    // 0.14 的 checkbox() 只收 is_checked，标签走 .label()（与 0.13 不同）
    checkbox(value)
        .label(label)
        .on_toggle(on_toggle)
        .size(15.0)
        .text_size(11.5)
        .font(FONT_SANS)
        .spacing(9)
        .style(move |_theme: &Theme, status: checkbox::Status| {
            let on = matches!(
                status,
                checkbox::Status::Active { is_checked: true }
                    | checkbox::Status::Hovered { is_checked: true }
            );
            checkbox::Style {
                background: if on { pal.accent.into() } else { pal.input_bg.into() },
                icon_color: pal.on_accent,
                border: Border {
                    color: if on { pal.accent } else { pal.border_mid },
                    width: 1.0,
                    radius: 5.0.into(),
                },
                text_color: Some(pal.text_2),
            }
        })
        .into()
}

/// 下拉选择（CSS `.select`）。选项收 owned `Vec<T>`——`pick_list` 的
/// `L: Borrow<[T]>` 允许 Vec，省得调用方为了凑 `&'a [T]` 去 leak。
/// `vpad` 是纵向内边距：pick_list 没有 `.height()`，高度只能靠 padding 撑。
/// 上一代基础 `.select` 高 36px（vpad 9），首页 hero 的 `.select` 覆盖成 48px
/// 以与 `.btn-hero` 等高（vpad 15）——两者不同高会让下拉顶边比按钮低、看着「陷下去」。
pub fn dropdown<'a, T, Message>(
    options: Vec<T>,
    selected: Option<T>,
    on_select: impl Fn(T) -> Message + 'a,
    width: f32,
    vpad: u16,
    pal: &'static Palette,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    Message: Clone + 'a,
{
    pick_list(options, selected, on_select)
        .width(Length::Fixed(width))
        .padding(Padding::from([vpad, 12]))
        .text_size(11.5)
        .font(FONT_SANS)
        .style(move |_theme: &Theme, status: pick_list::Status| {
            let hovered = matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. });
            pick_list::Style {
                text_color: pal.text,
                placeholder_color: pal.text_3,
                handle_color: pal.text_3,
                background: pal.input_bg.into(),
                border: Border {
                    color: if hovered { pal.border_hi } else { pal.border_mid },
                    width: 1.0,
                    radius: R_CTL.into(),
                },
            }
        })
        .menu_style(move |_theme: &Theme| iced::widget::overlay::menu::Style {
            background: pal.surface_1.into(),
            border: Border {
                color: pal.border_mid,
                width: 1.0,
                radius: R_CTL.into(),
            },
            text_color: pal.text,
            selected_text_color: pal.on_accent,
            selected_background: pal.accent.into(),
            shadow: pal.shadow_pop,
        })
        .into()
}

/// 分段控件（CSS `.segmented`）：一组互斥选项，选中项浮起。
pub fn segmented<'a, Message: Clone + 'a>(
    items: Vec<(&'a str, bool, Message)>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let mut r = Row::new().spacing(2);
    for (label, on, msg) in items {
        let seg = container(
            txt_bold(label)
                .size(11)
                .color(if on { pal.text } else { pal.text_2 }),
        )
        .padding(Padding::from([6, 14]))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(if on { pal.text } else { pal.text_2 }),
            background: Some(if on {
                pal.surface_1.into()
            } else {
                Background::Color(Color::TRANSPARENT)
            }),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 8.0.into(),
            },
            shadow: if on { pal.shadow_ctl } else { Shadow::default() },
            snap: true,
        });
        r = r.push(mouse_area(seg).on_press(msg));
    }
    container(r)
        .padding(3)
        .style(move |_theme: &Theme| container::Style {
            text_color: None,
            background: Some(pal.surface_2.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 11.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 页头（CSS `.page-head`）：标题 + 描述 + 右侧操作。
pub fn page_head<'a, Message: 'a>(
    title: &'a str,
    desc: &'a str,
    actions: Option<Element<'a, Message>>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    let left = column![
        txt_bold(title).size(19).color(pal.text),
        txt(desc).size(12).color(pal.text_3),
    ]
    .spacing(5);
    let mut r = row![left, space::horizontal()]
        .width(Fill)
        .align_y(Alignment::Center);
    if let Some(a) = actions {
        r = r.push(a);
    }
    r.into()
}

/// 提示条（CSS `.strip`）：卡片内的次级操作条。
pub fn strip<'a, Message: 'a>(
    content: Row<'a, Message>,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(content.spacing(12).align_y(Alignment::Center))
        .width(Fill)
        .padding(Padding::from([11, 14]))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text_2),
            background: Some(pal.surface_2.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: R_CTL.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 通知（CSS `.toast`）：右下角浮出，左侧一道色条表示类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Ok,
    Err,
    Info,
}

pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    /// 到期时刻（用 tick 推进；见 app.rs 的 ToastTick）。
    pub until: std::time::Instant,
}

pub fn toast_host<'a, Message: 'a>(
    toasts: &'a [Toast],
    pal: &'static Palette,
) -> Element<'a, Message> {
    if toasts.is_empty() {
        return space::Space::new().into();
    }
    let mut col = Column::new().spacing(10).align_x(Alignment::End);
    for t in toasts {
        let accent = match t.kind {
            ToastKind::Ok => pal.ok,
            ToastKind::Err => pal.bad,
            ToastKind::Info => pal.accent,
        };
        let body = row![
            // 左侧色条替代 CSS 的 inset box-shadow（iced 的 Shadow 无 inset）
            container(space::Space::new())
                .width(3.0)
                .height(Length::Fixed(18.0))
                .style(move |_theme: &Theme| container::Style {
                    text_color: None,
                    background: Some(accent.into()),
                    border: Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: R_PILL.into(),
                    },
                    shadow: Shadow::default(),
                    snap: true,
                }),
            txt(t.text.as_str()).size(11.5).color(pal.text),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        col = col.push(
            container(body)
                .padding(Padding::from([11, 15]))
                .max_width(400.0)
                .style(move |_theme: &Theme| container::Style {
                    text_color: Some(pal.text),
                    background: Some(pal.surface_1.into()),
                    border: Border {
                        color: pal.card_border,
                        width: 1.0,
                        radius: 12.0.into(),
                    },
                    shadow: pal.shadow_pop,
                    snap: true,
                }),
        );
    }
    // 靠右下：外层撑满 + 末端对齐，再留边距。
    container(col)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .padding(24)
        .into()
}

/// 长路径省略：iced 不做 `word-break`，超长路径会把行撑爆。
/// 保留尾部（文件名/目录名比盘符有用），前面用省略号。
pub fn ellipsize(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let tail: String = chars[chars.len() - (max - 1)..].iter().collect();
    format!("…{tail}")
}

/// hover 高亮的可点击行：给列表行加交互。`key` 唯一时才做过渡。
pub fn hoverable<'a, Message: Clone + 'a>(
    content: Element<'a, Message>,
    key: Option<crate::ui::anim::Key>,
    anim: &AnimState,
    pal: &'static Palette,
    on_press: Option<Message>,
    on_enter: Option<Message>,
    on_exit: Option<Message>,
) -> Element<'a, Message> {
    let t = key.map(|k| anim.value(k)).unwrap_or(0.0);
    let wrapped = container(content)
        .width(Fill)
        .style(move |_theme: &Theme| container::Style {
            text_color: None,
            background: Some(theme::lerp(Color::TRANSPARENT, pal.hover, t).into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: R_CTL.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        });
    let mut ma = mouse_area(wrapped);
    if let Some(m) = on_press {
        ma = ma.on_press(m);
    }
    if let (Some(a), Some(b)) = (on_enter, on_exit) {
        ma = ma.on_enter(a).on_exit(b);
    }
    ma.into()
}
