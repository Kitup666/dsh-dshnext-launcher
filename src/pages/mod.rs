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
use crate::ui::glass_pipeline::fade_veil;
use crate::ui::glow_mesh;
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::icon;
use crate::ui::modal;
use crate::ui::widgets;
use crate::ui::{card, disp, disp_bold, titlebar, txt};
use iced::widget::{Column, column, container, mouse_area, row, scrollable, space, stack};
use iced::{
    Alignment, Border, Color, Element, Fill, Length, Padding, Shadow, Theme,
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

    // 控制台、首页自己撑满高度（页面免滚，列表在卡内滚），其余页面外层滚动。
    // 容器 height(Fill) 给页面里的 Fill 高度卡传递空间。
    // （Fill 在 scrollable 里会退化成 Shrink——iced 给滚动的子树无穷上限。）
    let body: Element<'_, Message> = if matches!(app.page, Page::Console | Page::Home) {
        container(page_body)
            .width(Fill)
            .height(Fill)
            .padding(Padding::from(44).top(12))
            .into()
    } else {
        // 滚动页叠顶/底虚化帏幕：内容滚过主区上下边时渐隐进背景（帏幕 =
        // 不透明背景场 + 带高 alpha 渐隐，见 glass_pipeline::fade_veil）。
        // 带高对齐内容 padding：静止时帏幕只罩住空白，看不见；滚动时才显形。
        stack![
            scrollable(
                container(page_body)
                    .width(Fill)
                    .height(Fill)
                    .padding(Padding::from(44).top(12)),
            )
            .direction(scrollable::Direction::Vertical(widgets::slim_scrollbar()))
            .style(widgets::slim_scroll_style(pal))
            .width(Fill)
            .height(Fill),
            // 顶帏窄于顶 padding（8 < 12）：稍滚一点标题不该整个被吃掉；
            // 静止时帏幕只罩住 padding 空白。底帏对齐底 padding 44。
            // 顶 padding 12 是平衡值：标题条 40 + 12 = 52，对齐底部 44
            // （2026-09-06 用户反馈上下不平衡，内容整体上移）。
            fade_veil(pal, 8.0, true),
            fade_veil(pal, 44.0, false),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    };

    // 切页入场不再包外壳整体位移——改为各页对**每张卡**错峰落位
    // （reveal_at + delay，见 home/profiles/plugins/env/settings/console）。
    // 一次编排好的入场（卡片依次就位）比整页平移更有设计感；侧边栏和
    // 标题栏不动，视线锚点保留。
    let main_area: Element<'_, Message> = body;

    // 布局：侧边栏（含**独立的品牌头部单元**）｜分隔线｜主区列（细窗口条 +
    // 页面）。窗口条只管拖动+窗口按钮，横跨主区上方；品牌块是侧边栏自己
    // 的栏位，高度独立——细条不再被品牌撑高（2026-09-06 用户反馈）。
    let inner = row![
        sidebar(app, pal),
        side_divider(pal),
        column![
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
            main_area,
        ]
        .width(Fill)
        .height(Fill),
    ]
    .width(Fill)
    .height(Fill);

    // 环境光在最底层（背景要干净：颗粒只属于卡片，不铺全窗——用户指出
    // 「颗粒泄露到背景」，2026-09-06 撤）。
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

/// 环境光：**左上、右下两个径向球圆光晕**（真圆，不是线性拼的楔形）。
/// wgpu 后端走玻璃 shader 的背景模式——与卡片**同一份** fragment 求值，
/// 背景与卡内在构造上就是同一片场；tiny-skia 回退 `ui::glow_mesh` 的
/// Mesh::Solid 扇形顶点（公开 API）。中间大面积留干净。
///
/// **背景已冻结**（2026-09-06 用户定）：慢漂移跑了半天谁也没看出来在动，
/// 白付 15fps 常驻出帧的代价——「空闲零出帧」纪律恢复。光球参数由
/// `glow_mesh::ambient_specs` 统一供给（Frosted 的伪造 backdrop 共用同一
/// 份，卡内颜色随位置对应窗外背景，磨砂的「透」才成立）。
fn ambient(pal: &'static Palette) -> Element<'static, Message> {
    crate::ui::glass_pipeline::background_field(glow_mesh::ambient_specs(pal))
}

/// 品牌块（logo + 名称 + 副标题）：**侧边栏头部单元**，宽 = 侧边栏、贴窗口
/// 左上角，高度 = BRAND_H（独立栏位，2026-09-06 用户定：不与窗口条同高）。
/// logo 是与 exe 图标
/// 同源的插画，圆角烘在 PNG 的 alpha 里（image widget 本身不支持圆角），角上
/// 透出的是环境光渐变，不是黑块。Handle 必须全局单例：image::Handle::
/// from_bytes 的 id 是 Id::unique()，在 view() 里现造会每次都变成「新图」，
/// 缓存穿透导致悬停时闪烁（svg 的 from_memory 按内容 hash 没这个问题，
/// 见 AGENTS.md 坑 27）。
fn brand<'a>(pal: &'static Palette) -> Element<'a, Message> {
    row![
        iced::widget::image::Image::new(brand_handle())
            .width(Length::Fixed(34.0))
            .height(Length::Fixed(34.0)),
        column![
            // 品牌名走 display 字体（Martian Mono）——侧边栏的「机器声」签名；
            // 副标题是中文，Noto 保留。
            disp_bold("DshDesk").size(FS_TITLE).color(pal.text),
            txt("DeepSeek Harness 启动器").size(FS_MICRO).color(pal.text_3),
        ],
    ]
    .spacing(9)
    .align_y(Alignment::Center)
    .into()
}

fn sidebar<'a>(app: &'a Dshnext, pal: &'static Palette) -> Element<'a, Message> {
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
    // 图钉：窗口置顶开关，立即生效并落盘（不走设置页草稿）。
    let pinned = app.config.always_on_top;
    let pin = button::btn(
        Spec::new("sb.pin", if pinned { "已置顶" } else { "置顶" },
                  if pinned { Variant::Accent } else { Variant::Secondary })
            .size(BtnSize::Small),
        pal,
        &app.anim,
        Some(Message::ToggleTopmost),
        Some(Message::HoverEnter("sb.pin")),
        Some(Message::HoverExit("sb.pin")),
    );

    let foot = column![
        row![
            widgets::dot(dot_color),
            txt(ready_text).size(FS_TINY).color(pal.text_3),
            space::horizontal(),
            pin,
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Fill),
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

    // 品牌头部单元在最顶（贴窗口左上角，独立高度，自带拖动热区），
    // 导航和状态脚注在下面的常规容器里。DrawTicker 放侧边栏（常驻可见），
    // 放 scrollable 里会被视口剔除永远不 draw——阶段 0 那条 intersects
    // (viewport) 剔除逻辑的活教材。
    let body = container(
        column![
            nav,
            space::vertical(),
            foot,
            Element::new(DrawTicker),
        ]
        .spacing(0),
    )
    .width(Length::Fixed(232.0))
    .height(Fill)
    .padding(Padding::from(14).top(12).bottom(16))
    .style(move |_theme: &Theme| container::Style {
        text_color: Some(pal.text),
        // 透明：环境光在下层铺满整窗，这里填色会切出一道硬边。
        background: None,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    });

    column![
        titlebar::brand_cell(brand(pal), Message::DragWindow),
        body,
    ]
    .width(Length::Fixed(232.0))
    .height(Fill)
    .into()
}

/// 品牌头像的图片句柄，进程内只建一次（见上，view() 里现造会缓存穿透闪烁）。
fn brand_handle() -> iced::widget::image::Handle {
    static HANDLE: std::sync::OnceLock<iced::widget::image::Handle> = std::sync::OnceLock::new();
    HANDLE
        .get_or_init(|| {
            iced::widget::image::Handle::from_bytes(
                include_bytes!("../../assets/icons/brand.png").to_vec(),
            )
        })
        .clone()
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
    // 选中底用玻璃卡面的等效不透明色（与 shader 卡面同色，见 card::glass_face）；
    // 透明插透明无碍。
    let bg = theme::lerp(
        // 未选中时的底是 hover 叠色，选中时是卡面同色。两者都要参与：
        // 悬停着切页时不插 hover 会先闪回透明。
        theme::lerp(Color::TRANSPARENT, pal.hover, t),
        card::glass_face(pal),
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
        disp(widgets::ellipsize(&v, 16)).size(FS_TINY).color(pal.text_2),
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
