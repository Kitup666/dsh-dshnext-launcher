//! 阶段 0 可行性探针 —— 不是产品代码。
//!
//! 一个二进制回答 DESIGN.md §10 的四个问题：
//!   render  中文渲染（内嵌字体 + advanced-shaping + 等宽数字对齐）+ 软阴影卡片
//!   ime     中文输入法（微软拼音 preedit / 候选框位置）
//!   idle    空闲占用（挂着不动，看还出不出帧）
//!   软件回退走环境变量 ICED_BACKEND=tiny-skia，与场景正交
//!
//! 用法：`phase0 [render|ime|idle] [--shot 路径]`
//! `--shot` 让程序自己截图后退出，便于无人值守取证。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Layout, layout, mouse, renderer};
use iced::widget::{Column, column, container, row, scrollable, space, text, text_input};
use iced::widget::operation;
use iced::{
    Background, Border, Color, Element, Fill, Font, Length, Rectangle, Shadow, Size, Subscription,
    Task, Theme, Vector, window,
};

mod theme;
use theme::Palette;

const SANS: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-Regular.subset.ttf");
const SANS_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-SemiBold.subset.ttf");
const MONO: &[u8] = include_bytes!("../../assets/fonts/CascadiaMono.subset.ttf");

const FONT_SANS: Font = Font::with_name("Noto Sans SC");
const FONT_SEMIBOLD: Font = Font {
    weight: iced::font::Weight::Semibold,
    ..Font::with_name("Noto Sans SC")
};
const FONT_MONO: Font = Font::with_name("Cascadia Mono");

/// 真实绘制次数。由自定义 widget 在 `draw()` 里自增，**不经过 Message**——
/// 走 Message 会让「订阅出帧」自己触发下一帧，测出来的空闲占用是假的。
static DRAWS: AtomicU64 = AtomicU64::new(0);

const INPUT_ID: &str = "probe-input";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scene {
    Render,
    Ime,
    Idle,
}

#[derive(Debug, Clone)]
enum Message {
    Input(String),
    /// 原始运行时事件，只用于打日志。没有可访问性树时，这是唯一能确认
    /// 「按键到底有没有进到窗口」的手段。
    Raw(iced::Event),
    Opened(window::Id),
    Shoot,
    Shot(window::Screenshot),
}

struct Probe {
    scene: Scene,
    input: String,
    started: Instant,
    shot_path: Option<String>,
    shot_after: u64,
    window: Option<window::Id>,
}

fn main() -> iced::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("phase0=info,iced_wgpu=info,wgpu_core=warn"),
    )
    .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let scene = match args.first().map(String::as_str) {
        Some("ime") => Scene::Ime,
        Some("idle") => Scene::Idle,
        _ => Scene::Render,
    };
    let shot_path = args
        .iter()
        .position(|a| a == "--shot")
        .and_then(|i| args.get(i + 1))
        .cloned();
    // 截图延时：IME 场景要留出外部脚本敲键的时间。
    let shot_after = args
        .iter()
        .position(|a| a == "--after")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1600);

    // 内存归因开关：分别关掉抗锯齿和内嵌字体，看那 100+ MB 是谁的。
    let no_aa = args.iter().any(|a| a == "--no-aa");
    let no_fonts = args.iter().any(|a| a == "--no-fonts");

    // iced 没有暴露 wgpu 的 backends 设置，默认是 Backends::all()：Vulkan、DX12、
    // GL 三套加载器全初始化再挑一个，光这份浪费就有 30 MB 常驻。wgpu 会读
    // WGPU_BACKEND，而它是在 compositor 创建时读的（晚于 main），所以进程内自己
    // 设上就能生效——这是产品里限定 DX12 的可行做法。
    if args.iter().any(|a| a == "--dx12") {
        // SAFETY: 单线程启动阶段，尚未创建任何窗口或后台线程。
        unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
    }
    log::info!(
        "scene={scene:?} ICED_BACKEND={:?} shot={shot_path:?} no_aa={no_aa} no_fonts={no_fonts}",
        std::env::var("ICED_BACKEND").ok()
    );

    // 每 5s 报一次真实绘制次数。独立线程，不参与 iced 的事件循环，
    // 因此它自己不会制造帧——这是空闲测量能成立的前提。
    std::thread::spawn(|| {
        let mut last = 0u64;
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let now = DRAWS.load(Ordering::Relaxed);
            log::info!("draws total={now} delta={} (过去 5s)", now - last);
            last = now;
        }
    });

    let mut app = iced::application(
        move || Probe {
            scene,
            input: String::new(),
            started: Instant::now(),
            shot_path: shot_path.clone(),
            shot_after,
            window: None,
        },
        Probe::update,
        Probe::view,
    )
    .title("DshDesk Phase 0 探针")
    .window_size((1180.0, 820.0))
    .antialiasing(!no_aa)
    .theme(dark_theme)
    .style(app_style)
    .subscription(Probe::subscription);

    if !no_fonts {
        app = app
            .default_font(FONT_SANS)
            .font(SANS)
            .font(SANS_SEMIBOLD)
            .font(MONO);
    }

    app.run()
}

fn dark_theme(_state: &Probe) -> Theme {
    Theme::Dark
}

fn app_style(_state: &Probe, _theme: &Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Palette::DARK.bg_app,
        text_color: Palette::DARK.text,
    }
}

impl Probe {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Input(v) => {
                // 打到控制台：iced 窗口没有可访问性树（UIA descendants=0），
                // 外部脚本读不到控件值，只能靠这行日志验证 IME 提交内容。
                log::info!("input = {v:?}（{} 字符）", v.chars().count());
                self.input = v;
                Task::none()
            }
            Message::Raw(event) => {
                match &event {
                    iced::Event::Keyboard(k) => log::info!("keyboard {k:?}"),
                    iced::Event::InputMethod(i) => log::info!("ime {i:?}"),
                    iced::Event::Window(w) => log::info!("window {w:?}"),
                    _ => {}
                }
                Task::none()
            }
            Message::Opened(id) => {
                self.window = Some(id);
                // 输入框自动聚焦：没有可访问性树意味着外部脚本无法「点中」控件，
                // 靠坐标点击太脆。聚焦交给程序自己，脚本只管敲键。
                let focus = if self.scene == Scene::Ime {
                    operation::focus(INPUT_ID)
                } else {
                    Task::none()
                };
                if self.shot_path.is_some() {
                    // 给渲染器一点时间稳定，否则可能抓到还没画完的 surface。
                    let delay = self.shot_after;
                    focus.chain(Task::perform(
                        async move { tokio::time::sleep(Duration::from_millis(delay)).await },
                        |_| Message::Shoot,
                    ))
                } else {
                    focus
                }
            }
            Message::Shoot => match self.window {
                Some(id) => window::screenshot(id).map(Message::Shot),
                None => Task::none(),
            },
            Message::Shot(shot) => {
                let path = self.shot_path.clone().unwrap_or_default();
                write_png(&path, &shot);
                log::info!(
                    "已写出 {path}（{}x{} @{}x），draws={}",
                    shot.size.width,
                    shot.size.height,
                    shot.scale_factor,
                    DRAWS.load(Ordering::Relaxed)
                );
                iced::exit()
            }
        }
    }

    /// 只订阅窗口打开事件（一次性）。空闲场景刻意不订阅任何周期性来源，
    /// 这样「还在不在出帧」才是运行时自己的行为，不是被我们逼出来的。
    fn subscription(&self) -> Subscription<Message> {
        let opened = window::open_events().map(Message::Opened);
        match self.scene {
            // IME 场景才监听原始事件：listen_raw 会让每个事件都走一遍 update，
            // 空闲场景开了就测不准了。
            Scene::Ime => Subscription::batch([
                opened,
                iced::event::listen_raw(|event, _status, _window| match event {
                    iced::Event::Mouse(_) | iced::Event::Touch(_) => None,
                    other => Some(Message::Raw(other)),
                }),
            ]),
            _ => opened,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let p = Palette::DARK;
        let body = match self.scene {
            Scene::Render => self.view_render(p),
            Scene::Ime => self.view_ime(p),
            Scene::Idle => self.view_idle(p),
        };
        container(column![body, Element::new(DrawTicker)])
            .padding(28)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn view_render(&self, p: Palette) -> Element<'_, Message> {
        let cjk = card(
            p,
            column![
                heading("中文渲染与字重对比", p),
                dim("常规字重（wght 400）：深度求索启动器，插件与版本隔离管理。", p),
                text("语义字重（wght 600）：环境全托管，与系统隔离。")
                    .font(FONT_SEMIBOLD)
                    .size(15)
                    .color(p.text),
                dim("标点与全角：「引号」、（括号）、《书名》——省略号…箭头→勾√", p),
                dim("生僻与扩展：龘㸻鑫燚囍　日本語カナ　한글 조합", p),
                dim("混排：DshDesk 启动 dsh web 于 127.0.0.1:3080（PID 18324）", p),
            ]
            .spacing(9),
        );

        // 等宽数字：故意让每行数字不同。字体不是 tabular 的话，右边界会参差。
        let nums = card(
            p,
            column![
                heading("等宽数字对齐（tabular）", p),
                mono("1234567890  内存 128.4 MB", p),
                mono("1111111111  内存   8.0 MB", p),
                mono("0000000000  内存 999.9 MB", p),
                dim("三行末尾若不齐，说明字体缺 tnum；iced 侧没有 CSS 那种开关。", p),
            ]
            .spacing(7),
        );

        let shadows = row![
            shadow_demo(p, "blur 18 / dy 6", 18.0, 6.0),
            shadow_demo(p, "blur 30 / dy 14", 30.0, 14.0),
            shadow_demo(p, "blur 60 / dy 24", 60.0, 24.0),
        ]
        .spacing(18);

        scrollable(
            column![
                title("阶段 0 · 中文渲染 / 等宽数字 / 软阴影", p),
                cjk,
                nums,
                heading("软阴影三档（对照上一代 CSS box-shadow）", p),
                shadows,
            ]
            .spacing(20),
        )
        .height(Fill)
        .into()
    }

    fn view_ime(&self, p: Palette) -> Element<'_, Message> {
        let input = text_input("在此用微软拼音输入中文…", &self.input)
            .id(INPUT_ID)
            .on_input(Message::Input)
            .padding([12, 14])
            .size(15)
            .font(FONT_SANS)
            .style(move |_, _| iced::widget::text_input::Style {
                background: Background::Color(p.input_bg),
                border: Border {
                    color: p.border_mid,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                icon: p.text_3,
                placeholder: p.text_3,
                value: p.text,
                selection: p.accent_soft,
            });

        card(
            p,
            column![
                title("阶段 0 · 中文输入法", p),
                dim(
                    "检查项：候选框是否贴着光标、preedit 是否就地显示、回删是否按整字。",
                    p
                ),
                input,
                dim(&format!("当前值（{} 字符）：", self.input.chars().count()), p),
                text(if self.input.is_empty() {
                    "（空）".to_string()
                } else {
                    self.input.clone()
                })
                .font(FONT_MONO)
                .size(14)
                .color(p.accent),
            ]
            .spacing(14),
        )
    }

    fn view_idle(&self, p: Palette) -> Element<'_, Message> {
        card(
            p,
            column![
                title("阶段 0 · 空闲出帧", p),
                dim("窗口就这么挂着，别碰鼠标键盘。绘制次数由控制台每 5s 打印。", p),
                mono(&format!("draws   {}", DRAWS.load(Ordering::Relaxed)), p),
                mono(
                    &format!("elapsed {:.0}s", self.started.elapsed().as_secs_f32()),
                    p
                ),
                dim("界面上的数字不会自己刷新——正因为没有帧，这本身就是结论。", p),
            ]
            .spacing(10),
        )
    }
}

/// 1x1 widget，唯一作用是在每次真实绘制时给 `DRAWS` 加一。
/// 不能做成零尺寸：`Column::draw` 会用 `bounds().intersects(viewport)` 剔除子元素，
/// 零面积矩形永远不相交，于是永远不被 draw（这条剔除逻辑本身是 §7.3 虚拟滚动的好消息）。
struct DrawTicker;

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for DrawTicker
where
    Renderer: iced::advanced::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(1.0), Length::Fixed(1.0))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(Size::new(1.0, 1.0))
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        DRAWS.fetch_add(1, Ordering::Relaxed);
    }
}

// ---- 视觉零件：只为看效果，产品代码会进 ui/ ----

fn card<'a>(p: Palette, content: Column<'a, Message>) -> Element<'a, Message> {
    container(content)
        .padding(22)
        .width(Fill)
        .style(move |_| iced::widget::container::Style {
            background: Some(Background::Color(p.surface_1)),
            border: Border {
                color: p.border,
                width: 0.0,
                radius: 16.0.into(),
            },
            shadow: p.shadow_card,
            ..Default::default()
        })
        .into()
}

fn shadow_demo<'a>(p: Palette, label: &'a str, blur: f32, dy: f32) -> Element<'a, Message> {
    container(
        column![
            text(label).size(13).color(p.text_2),
            text("圆角 16 / 软阴影").size(14).color(p.text),
        ]
        .spacing(6),
    )
    .padding(18)
    .width(Length::Fixed(220.0))
    .style(move |_| iced::widget::container::Style {
        background: Some(Background::Color(p.surface_2)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 16.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.75),
            offset: Vector::new(0.0, dy),
            blur_radius: blur,
        },
        ..Default::default()
    })
    .into()
}

fn title<'a>(s: &'a str, p: Palette) -> Element<'a, Message> {
    text(s).size(22).font(FONT_SEMIBOLD).color(p.text).into()
}

fn heading<'a>(s: &'a str, p: Palette) -> Element<'a, Message> {
    text(s).size(15).font(FONT_SEMIBOLD).color(p.text).into()
}

fn dim<'a>(s: &str, p: Palette) -> Element<'a, Message> {
    text(s.to_string()).size(14).color(p.text_2).into()
}

fn mono<'a>(s: &str, p: Palette) -> Element<'a, Message> {
    row![
        text(s.to_string()).size(14).font(FONT_MONO).color(p.text),
        space::horizontal(),
    ]
    .into()
}

fn write_png(path: &str, shot: &window::Screenshot) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        shot.size.width,
        shot.size.height,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("png header")
        .write_image_data(&shot.rgba)
        .expect("png data");
}
