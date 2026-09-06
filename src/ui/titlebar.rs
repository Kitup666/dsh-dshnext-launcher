//! 自绘标题栏 + 无边框窗口的边缘缩放热区。
//!
//! `decorations: false` 之后系统标题栏和四周的缩放边框一起消失，两样都得自己补：
//! - 标题栏：`mouse_area(...).on_press(Message::DragWindow)` → `window::drag(id)`
//!   把后续拖动交给系统（不是自己算 delta 再 move_to，那样会掉帧、跟手差）。
//! - 缩放：窗口四边/四角各铺一条 6px 的透明热区，按下时 `window::drag_resize(id, dir)`。
//!
//! 纪律：热区必须**在最上层**（`stack!` 的最后一个孩子），否则被卡片吃掉点击；
//! 但它们只在边缘 6px，正常内容区不受影响。

use crate::theme::{self, FS_SMALL, Palette};
use crate::ui::anim::{self, AnimState};
use crate::ui::txt_bold;
use iced::widget::{Row, container, mouse_area, row, space};
use iced::window::Direction;
use iced::{Alignment, Border, Color, Element, Fill, Length, Padding, Shadow, Theme};
/// 窗口条高度（逻辑像素）。**只**承担拖动 + 窗口按钮，横跨主区上方——
/// 40 的细条（用户反馈：64 太宽，是被品牌块撑高的）。品牌块是侧边栏
/// 自己的独立栏位（BRAND_H），高度与此互不牵连。
pub const TITLEBAR_H: f32 = 40.0;
/// 品牌头部单元高度：贴窗口左上角、宽 = 侧边栏，独立于窗口条。
pub const BRAND_H: f32 = 64.0;
/// 侧边栏宽度——品牌头部单元与它对齐（pages/mod.rs 的 sidebar 同款值）。
pub const SIDEBAR_W: f32 = 232.0;
/// 边缘缩放热区宽度。6px 是 Windows 原生无边框应用的常用值：够点中，又不至于误触。
const GRIP: f32 = 6.0;

/// 标题栏需要的三个动作，由 app 层给出对应的 Message。
pub struct Actions<Message> {
    pub drag: Message,
    pub minimize: Message,
    pub toggle_maximize: Message,
    pub close: Message,
}

/// 品牌头部单元：**侧边栏的独立栏位**，贴窗口左上角、宽 = 侧边栏、
/// 高 = BRAND_H（不受窗口条高度牵连）。内容左对齐（与侧边栏 14px 内距
/// 同款），整块是拖动热区（非交互元素，按下即拖窗，资源管理器惯例）。
pub fn brand_cell<'a, Message: Clone + 'a>(
    brand: Element<'a, Message>,
    drag: Message,
) -> Element<'a, Message> {
    mouse_area(
        container(brand)
            .width(Length::Fixed(SIDEBAR_W))
            .height(Length::Fixed(BRAND_H))
            .padding(Padding::from(0.0).left(14.0))
            .align_y(Alignment::Center),
    )
    .on_press(drag)
    .into()
}

/// 窗口条：主区上方的细拖动条 + 三个窗口按钮。宽度随主区（它住在主区
/// 列里，侧边栏不在其间）。
pub fn titlebar<'a, Message: Clone + 'a>(
    pal: &'static Palette,
    anim: &AnimState,
    actions: Actions<Message>,
    maximized: bool,
    hover: impl Fn(anim::Key) -> Message + Copy + 'a,
    unhover: impl Fn(anim::Key) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    let Actions {
        drag,
        minimize,
        toggle_maximize,
        close,
    } = actions;

    // 可拖动区：整条空白都能抓。
    let grab = mouse_area(
        container(space::Space::new())
            .height(Fill)
            .width(Fill),
    )
    .on_press(drag);

    let buttons = row![
        win_btn("win.min", Glyph::Minimize, pal, anim, minimize, hover, unhover, false),
        win_btn(
            "win.max",
            if maximized { Glyph::Restore } else { Glyph::Maximize },
            pal,
            anim,
            toggle_maximize,
            hover,
            unhover,
            false,
        ),
        win_btn("win.close", Glyph::Close, pal, anim, close, hover, unhover, true),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    container(row![grab, buttons].align_y(Alignment::Center))
        .width(Fill)
        .height(Length::Fixed(TITLEBAR_H))
        .padding(Padding::from(0.0).right(6.0))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text_2),
            // 透明：环境光渐变在下层铺满整窗，标题栏填色会在这里切出一道硬边。
            background: None,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

/// 三个窗口按钮的字形。用文本而不是 svg：`─ □ ❐ ✕` 都在子集字体的覆盖范围内
/// （ASCII + 标点 + 制表符号），省一次 svg 解析，也省三个文件。
#[derive(Clone, Copy)]
enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Glyph {
    fn text(self) -> &'static str {
        match self {
            Glyph::Minimize => "\u{2500}",  // ─ BOX DRAWINGS LIGHT HORIZONTAL
            Glyph::Maximize => "\u{25a1}",  // □ WHITE SQUARE
            Glyph::Restore => "\u{2750}",   // ❐ UPPER RIGHT DROP-SHADOWED WHITE SQUARE
            Glyph::Close => "\u{2715}",     // ✕ MULTIPLICATION X
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn win_btn<'a, Message: Clone + 'a>(
    key: anim::Key,
    glyph: Glyph,
    pal: &'static Palette,
    anim: &AnimState,
    on_press: Message,
    hover: impl Fn(anim::Key) -> Message + 'a,
    unhover: impl Fn(anim::Key) -> Message + 'a,
    danger: bool,
) -> Element<'a, Message> {
    let t = anim.value(key);
    // 关闭按钮 hover 变红（Windows 惯例），其余变浅色叠加。
    let bg = if danger {
        theme::lerp(Color::TRANSPARENT, theme::with_alpha(pal.bad, 0.9), t)
    } else {
        theme::lerp(Color::TRANSPARENT, pal.hover, t)
    };
    let fg = if danger {
        theme::lerp(pal.text_3, Color::WHITE, t)
    } else {
        theme::lerp(pal.text_3, pal.text, t)
    };

    let body = container(txt_bold(glyph.text()).size(FS_SMALL).color(fg))
        .width(Length::Fixed(34.0))
        .height(Length::Fixed(26.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(fg),
            background: Some(bg.into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 7.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        });

    mouse_area(body)
        .on_press(on_press)
        .on_enter(hover(key))
        .on_exit(unhover(key))
        .into()
}

/// 八向缩放热区，铺满窗口边缘。放在 `stack!` 最上层。
/// 中间那块必须是**没有 mouse_area 的 Space**，否则会挡住整个内容区的点击。
pub fn resize_grips<'a, Message: Clone + 'a>(
    on_resize: impl Fn(Direction) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    let grip = |dir: Direction, w: Length, h: Length| -> Element<'a, Message> {
        mouse_area(space::Space::new().width(w).height(h))
            .on_press(on_resize(dir))
            .interaction(cursor_for(dir))
            .into()
    };

    let band = |left: Direction, mid: Option<Direction>, right: Direction, h: Length| -> Row<'a, Message> {
        let mut r = row![grip(left, Length::Fixed(GRIP), h)];
        r = match mid {
            Some(d) => r.push(grip(d, Fill, h)),
            // 中间行留空：不能包 mouse_area，否则内容区点不到。
            None => r.push(space::Space::new().width(Fill).height(h)),
        };
        r.push(grip(right, Length::Fixed(GRIP), h))
    };

    iced::widget::column![
        band(
            Direction::NorthWest,
            Some(Direction::North),
            Direction::NorthEast,
            Length::Fixed(GRIP)
        ),
        band(Direction::West, None, Direction::East, Fill),
        band(
            Direction::SouthWest,
            Some(Direction::South),
            Direction::SouthEast,
            Length::Fixed(GRIP)
        ),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

fn cursor_for(dir: Direction) -> iced::mouse::Interaction {
    use iced::mouse::Interaction as I;
    match dir {
        Direction::North | Direction::South => I::ResizingVertically,
        Direction::East | Direction::West => I::ResizingHorizontally,
        Direction::NorthWest | Direction::SouthEast => I::ResizingDiagonallyDown,
        Direction::NorthEast | Direction::SouthWest => I::ResizingDiagonallyUp,
    }
}

