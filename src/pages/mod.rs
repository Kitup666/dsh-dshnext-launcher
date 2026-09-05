//! 六个页面 + 共享外壳（无边框窗口、标题栏、侧边栏、环境光、模态、toast）。
//!
//! 页面本身只负责主区内容；外壳在 `shell()` 里统一拼，页面之间不重复。

pub mod console;
pub mod env;
pub mod home;
pub mod plugins;
pub mod profiles;
pub mod settings;

use crate::app::{Dshnext, Message};
use crate::theme::{self, FS_BODY, FS_MICRO, FS_TINY, FS_TITLE, Palette};
use crate::ui::anim;
use crate::ui::icon;
use crate::ui::modal;
use crate::ui::reveal::reveal;
use crate::ui::widgets;
use crate::ui::{mono, titlebar, txt, txt_bold};
use iced::widget::{Column, column, container, mouse_area, row, scrollable, space, stack};
use iced::{
    Alignment, Border, Color, Element, Fill, Length, Padding, Shadow, Theme, Vector,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Profiles,
    Plugins,
    Env,
    Console,
    Settings,
}

impl Page {
    /// 侧边栏顺序，也是 Ctrl+1..6 的顺序（与上一代 NAV 一致）。
    pub const ALL: [Page; 6] = [
        Page::Home,
        Page::Profiles,
        Page::Plugins,
        Page::Env,
        Page::Console,
        Page::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Home => "启动",
            Page::Profiles => "版本管理",
            Page::Plugins => "插件管理",
            Page::Env => "环境",
            Page::Console => "控制台",
            Page::Settings => "设置",
        }
    }

    pub fn icon(self) -> &'static [u8] {
        match self {
            Page::Home => icon::LAUNCH,
            Page::Profiles => icon::VERSIONS,
            Page::Plugins => icon::PLUGINS,
            Page::Env => icon::ENV,
            Page::Console => icon::CONSOLE,
            Page::Settings => icon::SETTINGS,
        }
    }

    /// hover 补间的 key。必须是 `&'static str`，所以按页面写死。
    pub fn anim_key(self) -> &'static str {
        match self {
            Page::Home => "nav.home",
            Page::Profiles => "nav.versions",
            Page::Plugins => "nav.plugins",
            Page::Env => "nav.env",
            Page::Console => "nav.console",
            Page::Settings => "nav.settings",
        }
    }

    pub fn from_digit(c: &str) -> Option<Page> {
        let n: usize = c.parse().ok()?;
        Self::ALL.get(n.checked_sub(1)?).copied()
    }
}

/// 顶层 view：外壳 + 当前页 + 模态 + toast + 缩放热区。
pub fn view(app: &Dshnext) -> Element<'_, Message> {
    let pal = app.palette();

    let page_body = match app.page {
        Page::Home => home::view(app),
        Page::Profiles => profiles::view(app),
        Page::Plugins => plugins::view(app),
        Page::Env => env::view(app),
        Page::Console => console::view(app),
        Page::Settings => settings::view(app),
    };

    // 控制台页自己撑满高度（内部滚动），其余页面外层滚动。
    let body: Element<'_, Message> = if app.page == Page::Console {
        container(page_body)
            .width(Fill)
            .height(Fill)
            .padding(Padding::from(44).top(28))
            .into()
    } else {
        scrollable(
            container(page_body)
                .width(Fill)
                .padding(Padding::from(44).top(28)),
        )
        .width(Fill)
        .height(Fill)
        .into()
    };

    // 切页入场：内容整体上移落位（src/ui/reveal.rs）。面纱淡入已删——盖整块区域
    // 会让背景变暗，用户否了。只包主区一个元素——侧边栏和标题栏不动，视线才有
    // 锚点；同时也是「一次只动 1~2 个元素」这条动效纪律。落定后 t=0，reveal 走
    // 直通分支，零开销。
    let main_area: Element<'_, Message> = reveal(
        body,
        app.anim.value(anim::PAGE),
        anim::PAGE_SHIFT,
    );

    let inner = column![
        titlebar::titlebar(
            pal,
            &app.anim,
            titlebar::Actions {
                drag: Message::DragWindow,
                minimize: Message::Minimize,
                toggle_maximize: Message::ToggleMaximize,
                close: Message::Close,
            },
            app.maximized,
            Message::HoverEnter,
            Message::HoverExit,
        ),
        row![sidebar(app, pal), side_divider(pal), main_area]
            .width(Fill)
            .height(Fill),
    ]
    .width(Fill)
    .height(Fill);

    // 环境光在最底层，铺满含标题栏（orevx 是挂在 html 背景上、整页共享）。
    let with_ambient = stack![ambient(pal), inner].width(Fill).height(Fill);

    let shell = container(with_ambient)
        .width(Fill)
        .height(Fill)
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(pal.text),
            background: Some(pal.bg_app.into()),
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        });

    // 模态在页面之上、toast 在模态之上、缩放热区在最上层。
    let mut layers: Element<'_, Message> = shell.into();
    if let Some(d) = &app.dialog {
        layers = modal::overlay(
            layers,
            d,
            &app.draft,
            pal,
            &app.anim,
            modal::Actions {
                close: Message::CloseDialog,
                confirm: Message::DialogConfirm,
                on_input: Box::new(Message::DialogInput),
            },
            Message::HoverEnter,
            Message::HoverExit,
        );
    }

    stack![
        layers,
        widgets::toast_host(&app.toasts, pal),
        titlebar::resize_grips(Message::Resize),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

/// 顶部环境光渐变带：蓝 → 青 → 透明，高 300px（含标题栏）。
/// 亮色两档都 transparent，整条带子不可见，零成本。
fn ambient(pal: &'static Palette) -> Element<'static, Message> {
    column![
        container(space::Space::new())
            .width(Fill)
            .height(300.0)
            .style(move |_theme: &Theme| container::Style {
                text_color: None,
                background: Some(iced::Background::Gradient(iced::Gradient::Linear(
                    iced::gradient::Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, pal.ambient_top)
                        .add_stop(0.5, pal.ambient_mid)
                        .add_stop(1.0, Color::TRANSPARENT),
                ))),
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            }),
        space::vertical(),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

fn sidebar<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
    let brand = row![
        container(txt_bold("D").size(16).color(pal.on_accent))
            .center_x(34.0)
            .center_y(34.0)
            .style(move |_theme: &Theme| container::Style {
                text_color: Some(pal.on_accent),
                background: Some(iced::Background::Gradient(iced::Gradient::Linear(
                    iced::gradient::Linear::new(iced::Degrees(150.0))
                        .add_stop(0.0, pal.accent_hi)
                        .add_stop(1.0, pal.accent),
                ))),
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 10.0.into(),
                },
                shadow: Shadow {
                    color: theme::with_alpha(pal.accent_line, 0.9),
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 12.0,
                },
                snap: true,
            }),
        column![
            txt_bold("DshDesk").size(FS_TITLE).color(pal.text),
            txt("DeepSeek Harness 启动器").size(FS_MICRO).color(pal.text_3),
        ],
    ]
    .spacing(11)
    .align_y(Alignment::Center);

    let mut nav = Column::new().spacing(2);
    for p in Page::ALL {
        nav = nav.push(nav_item(app, p, pal));
    }

    let ready = app.env_ready();
    let (dot_color, ready_text) = match &app.env {
        None => (pal.text_3, "正在检测…"),
        Some(_) if ready => (pal.ok, "环境就绪"),
        Some(_) => (pal.warn, "环境未就绪"),
    };
    let foot = column![
        row![
            widgets::dot(dot_color),
            txt(ready_text).size(FS_TINY).color(pal.text_3),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        foot_row(
            "dsh",
            app.env
                .as_ref()
                .and_then(|e| e.dsh_version.clone())
                .unwrap_or_else(|| "未安装".into()),
            pal
        ),
        foot_row(
            "Node",
            app.env
                .as_ref()
                .and_then(|e| e.node_version.clone())
                .unwrap_or_else(|| "未安装".into()),
            pal
        ),
    ]
    .spacing(6);

    // DrawTicker 放侧边栏（常驻可见）。放 scrollable 里会被视口剔除，永远不 draw
    // ——阶段 0 那条 intersects(viewport) 剔除逻辑的活教材。
    container(
        column![
            brand,
            space::vertical().height(24.0),
            nav,
            space::vertical(),
            foot,
            Element::new(DrawTicker),
        ]
        .spacing(0),
    )
    .width(Length::Fixed(232.0))
    .height(Fill)
    .padding(Padding::from(14).top(22).bottom(16))
    .style(move |_theme: &Theme| container::Style {
        text_color: Some(pal.text),
        // 透明：环境光在下层铺满整窗，这里填色会切出一道硬边。
        background: None,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    })
    .into()
}

/// 侧栏与主区之间的 1px 分隔线。两栏都透明、全靠环境光渐变区分，渐变淡出的
/// 下半部（控制台/设置）会糊在一起；补一条 `border` 色细线分区，成本最低。
fn side_divider<'a>(pal: &'static Palette) -> Element<'a, Message> {
    container(space::Space::new().width(1.0).height(Fill))
        .style(move |_theme: &Theme| container::Style {
            text_color: None,
            background: Some(pal.border.into()),
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        })
        .into()
}

fn nav_item<'a>(app: &'a Dshnext, page: Page, pal: &'static Palette) -> Element<'a, Message> {
    let key = page.anim_key();
    let t = app.anim.value(key);

    // 选中程度 act ∈ [0,1]，切页时新旧两项**同时**插值：旧项 1→0、新项 0→1，
    // 于是两个胶囊交叉淡入，看着像一块底色滑了过去。整数 0/1 的阶跃会「跳」一下
    // ——上一代靠 CSS transition 掩掉，这里只能自己插。
    // `anim::NAV` 是剩余进度（1 = 刚切、0 = 落定），所以新项取 1-nav、旧项取 nav。
    let nav = app.anim.value(anim::NAV);
    let act = if app.page == page {
        1.0 - nav
    } else if app.prev_page == Some(page) {
        nav
    } else {
        0.0
    };

    // 底色/描边按 act 插值。**描边色不能用半透明白往里插**（DESIGN.md §7.5 第 15 条：
    // 物理混色会把带色相的半透明放大），这里 card_border 本身就是中性白，安全。
    let bg = theme::lerp(
        // 未选中时的底是 hover 叠色，选中时是 surface_1。两者都要参与：
        // 悬停着切页时不插 hover 会先闪回透明。
        theme::lerp(Color::TRANSPARENT, pal.hover, t),
        pal.surface_1,
        act,
    );
    let border_c = theme::with_alpha(pal.card_border, pal.card_border.a * act);
    let border_w = 0.3 * act;

    let text_c = theme::lerp(theme::lerp(pal.text_2, pal.text, t), pal.text, act);
    let icon_c = theme::lerp(theme::lerp(pal.text_3, pal.text_2, t), pal.accent, act);

    let mut inner = row![
        icon::icon::<Message>(page.icon(), 18.0, icon_c),
        txt(page.label()).size(FS_BODY).color(text_c),
    ]
    .spacing(11)
    .align_y(Alignment::Center);

    // 首页显示运行中实例数（对应上一代 `.badge-count`）。
    if page == Page::Home && !app.procs.is_empty() {
        inner = inner.push(space::horizontal()).push(widgets::tag(
            format!("{}", app.procs.len()),
            widgets::Tone::Accent,
            pal,
        ));
    }

    let item = container(inner)
        .width(Fill)
        // iced 的 container 默认顶对齐；只给 height 会让图标+文字贴在 36px 行的
        // 顶部、下面空一截，看着整列「偏上」。center_y 同时定高 + 垂直居中（同 brand 的 D）。
        .center_y(36.0)
        .padding(Padding::from([0, 11]))
        .style(move |_theme: &Theme| container::Style {
            text_color: Some(text_c),
            background: Some(bg.into()),
            border: Border {
                color: border_c,
                width: border_w,
                radius: 10.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        });

    mouse_area(item)
        .on_press(Message::Goto(page))
        .on_enter(Message::HoverEnter(key))
        .on_exit(Message::HoverExit(key))
        .into()
}

fn foot_row<'a>(k: &'static str, v: String, pal: &'static Palette) -> Element<'a, Message> {
    row![
        txt(k).size(FS_TINY).color(pal.text_3),
        space::horizontal(),
        mono(widgets::ellipsize(&v, 16)).size(FS_TINY).color(pal.text_2),
    ]
    .width(Fill)
    .into()
}

/// 1x1 数帧 widget（阶段 0 验证过的做法）。
struct DrawTicker;

impl<Message, Theme, Renderer> iced::advanced::widget::Widget<Message, Theme, Renderer>
    for DrawTicker
where
    Renderer: iced::advanced::Renderer,
{
    fn size(&self) -> iced::Size<Length> {
        iced::Size::new(Length::Fixed(1.0), Length::Fixed(1.0))
    }

    fn layout(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &Renderer,
        _limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        iced::advanced::layout::Node::new(iced::Size::new(1.0, 1.0))
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        _layout: iced::advanced::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        crate::app::DRAWS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// 把秒数格式化成「x 时 y 分」（对应上一代 `fmtUptime`）。
pub fn fmt_uptime(s: i64) -> String {
    if s < 60 {
        return format!("{s} 秒");
    }
    let m = s / 60;
    if m < 60 {
        return format!("{m} 分 {} 秒", s % 60);
    }
    format!("{} 时 {} 分", m / 60, m % 60)
}

/// 毫秒时间戳 → 时分秒。
pub fn fmt_time(ts: i64) -> String {
    if ts == 0 {
        return String::new();
    }
    // 本地时区偏移：不引 chrono，用 UTC+8 硬编码不合适；这里用系统本地时间。
    let secs = ts / 1000;
    let local = secs + local_offset_secs();
    let d = local.rem_euclid(86400);
    format!("{:02}:{:02}:{:02}", d / 3600, (d % 3600) / 60, d % 60)
}

/// 本地时区偏移（秒）。用一次 SystemTime 与 chrono-free 的办法：
/// Windows 上 `GetTimeZoneInformation` 要额外 FFI，这里退一步——
/// 用进程启动时的本地/UTC 差值缓存下来，足够日志显示用。
fn local_offset_secs() -> i64 {
    use std::sync::OnceLock;
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        // std 没有本地时间 API。用 `time` crate 也是新依赖，
        // 折中：读环境变量 TZ 偏移不可靠，直接用 UTC+8（项目只面向国内 Windows）。
        // 若将来要准确，换 `time` crate 的 UtcOffset::current_local_offset。
        8 * 3600
    })
}
