//! 顶层 State / Message / update / view / subscription。
//!
//! 阶段 1 的产出就是这个 demo 页（DESIGN.md §10 步骤 5–7）：四类按钮、
//! 三档软阴影、svg 动态着色、hover 补间 + 主题切换。它同时是验收工具——
//! `--shot` 出图与上一代截图并排比，`--drawlog` 证明动画结束后出帧归零。
//!
//! 页面骨架（侧边栏 232px + 主区）照抄上一代 `.app` 的 grid，
//! 让对比图落在同一个视觉坐标系里。

use crate::theme::{self, HERO_NUM_SIZE, Palette, R_PILL};
use crate::ui::anim::{self, AnimState, HOVER_DUR};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::{card, icon, mono, txt, txt_bold};
use iced::widget::{Column, Row, column, container, mouse_area, row, scrollable, space, stack};
use iced::{
    Alignment, Border, Color, Element, Fill, Length, Padding, Shadow, Subscription, Task, Theme,
    Vector, window,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 真实绘制次数（阶段 0 的测法：自定义 widget 在 draw() 里自增，不走 Message，
/// 否则「订阅出帧」会自己触发下一帧，测出来的空闲是假的）。
pub static DRAWS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

/// `--shot 路径 --after 毫秒`：开窗后自截图退出，便于无人值守取证。
#[derive(Clone)]
pub struct Shot {
    pub path: String,
    pub after: u64,
}

pub struct Dshnext {
    pub mode: Mode,
    pub anim: AnimState,
    pub window: Option<window::Id>,
    pub shot: Option<Shot>,
    /// 最近一次点击的按钮文案，验证 on_press 链路。
    pub last_action: &'static str,
    /// `--autotest`：程序自己触发一次 hover 进/出，用出帧日志证明
    /// 「动画结束 → frames() 撤订 → 出帧归零」，不依赖真实鼠标。
    pub autotest: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Opened(window::Id),
    Shoot,
    Shot(window::Screenshot),
    /// 动画帧。只在 `anim.is_animating()` 期间被订阅。
    Tick(Instant),
    HoverEnter(anim::Key),
    HoverExit(anim::Key),
    ToggleTheme,
    Pressed(&'static str),
    /// autotest 的第二拍：延迟后触发 hover 退出。
    AutoExit,
}

impl Dshnext {
    pub fn new(mode: Mode, shot: Option<Shot>, autotest: bool) -> Self {
        Self {
            mode,
            anim: AnimState::default(),
            window: None,
            shot,
            last_action: "（还没有）",
            autotest,
        }
    }

    pub fn palette(&self) -> &'static Palette {
        match self.mode {
            Mode::Dark => &Palette::DARK,
            Mode::Light => &Palette::LIGHT,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Opened(id) => {
                self.window = Some(id);
                let mut tasks = Vec::new();
                if let Some(s) = &self.shot {
                    let after = s.after;
                    tasks.push(Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(after)).await
                        },
                        |_| Message::Shoot,
                    ));
                }
                if self.autotest {
                    // 程序自己走一遍 hover 进→停 600ms→出，全程用出帧日志取证。
                    tasks.push(Task::done(Message::HoverEnter("b.anim")));
                    tasks.push(Task::perform(
                        async move { tokio::time::sleep(Duration::from_millis(600)).await },
                        |_| Message::AutoExit,
                    ));
                }
                Task::batch(tasks)
            }
            Message::AutoExit => {
                log::info!(
                    "autotest: hover 退出，此前应已因补间产生若干帧；退出后 frames() 撤订，出帧应归零"
                );
                self.anim.animate_to("b.anim", 0.0, HOVER_DUR, Instant::now());
                Task::none()
            }
            Message::Shoot => match self.window {
                Some(id) => window::screenshot(id).map(Message::Shot),
                None => Task::none(),
            },
            Message::Shot(shot) => {
                let path = self.shot.as_ref().map(|s| s.path.clone()).unwrap_or_default();
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
            Message::Tick(now) => {
                // 推进补间；若全部完成，下一次 subscription() 就不再挂 frames()。
                let before = self.anim.tween_count();
                let animating = self.anim.tick(now);
                if before != self.anim.tween_count() {
                    log::info!("tick: 补间 {before} -> {} (animating={animating})", self.anim.tween_count());
                }
                Task::none()
            }
            Message::HoverEnter(key) => {
                log::info!("hover enter {key}");
                self.anim.animate_to(key, 1.0, HOVER_DUR, Instant::now());
                Task::none()
            }
            Message::HoverExit(key) => {
                log::info!("hover exit {key}");
                self.anim.animate_to(key, 0.0, HOVER_DUR, Instant::now());
                Task::none()
            }
            Message::ToggleTheme => {
                self.mode = match self.mode {
                    Mode::Dark => Mode::Light,
                    Mode::Light => Mode::Dark,
                };
                Task::none()
            }
            Message::Pressed(label) => {
                self.last_action = label;
                Task::none()
            }
        }
    }

    /// 空闲零订阅（DESIGN.md §8 第 1 条）：无动画时不挂 frames()。
    /// open_events 是一次性的；listen_with 只在按键时产消息，不造帧。
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            window::open_events().map(Message::Opened),
            iced::event::listen_with(|event, _status, _id| {
                if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                {
                    // Named 没有字母键变体（winit 的 Named 不含 KeyT），字母走 Character。
                    if let iced::keyboard::Key::Character(c) = &key {
                        if c == "t" || c == "T" {
                            return Some(Message::ToggleTheme);
                        }
                    }
                }
                None
            }),
        ];
        if self.anim.is_animating() {
            subs.push(window::frames().map(Message::Tick));
        }
        Subscription::batch(subs)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let pal = self.palette();

        let main = column![
            self.page_head(pal),
            self.card_hero(pal),
            self.card_buttons(pal),
            self.card_shadows(pal),
            self.card_icons(pal),
            self.card_anim(pal),
        ]
        .spacing(18)
        .padding(Padding::from(44).top(34));

        // 环境光层固定在顶部（不随内容滚动），卡片在其上滚过——
        // 对应 orevx 把渐变挂在 html 背景、内容滚过它的效果。
        let main_area = stack![self.ambient(pal), scrollable(main).width(Fill).height(Fill)]
            .width(Fill)
            .height(Fill);

        row![self.sidebar(pal), main_area]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// 顶部环境光渐变带：蓝 → 青 → 透明，高 260px。亮色两档都 transparent，等于无。
    fn ambient(&self, pal: &'static Palette) -> Element<'_, Message> {
        column![
            container(space::Space::new())
                .width(Fill)
                .height(260.0)
                .style(ambient_style(pal)),
            space::vertical(),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// hero 大数字演示卡（借鉴 orevx 的 48px/600 排版）。
    fn card_hero(&self, pal: &'static Palette) -> Element<'_, Message> {
        card::card(
            Column::new()
                .push(txt("DEEPSEEK HARNESS").size(10.5).color(pal.text_3))
                .push(txt_bold("92.717").size(HERO_NUM_SIZE).color(pal.text))
                .push(
                    txt("hero 数字排版 48px / 600。CSS 的 −1.2px 负字距 iced 没有对应 API（同 tnum 一类限制）。")
                        .size(11.5)
                        .color(pal.text_2),
                )
                .spacing(6),
            pal,
        )
    }

    // ---- 侧边栏：hover 过渡的主战场（CSS .nav-item） ----

    fn sidebar(&self, pal: &'static Palette) -> Element<'_, Message> {
        let brand = row![
            container(txt_bold("D").size(16).color(pal.on_accent))
                .center_x(34.0)
                .center_y(34.0)
                .style(brand_style(pal)),
            column![
                txt_bold("DshDesk").size(13.5).color(pal.text),
                txt("DeepSeek Harness 启动器").size(9.5).color(pal.text_3),
            ]
            .spacing(0),
        ]
        .spacing(11)
        .align_y(Alignment::Center);

        let nav = Column::new()
            .spacing(2)
            .push(self.nav_item("启动", "nav.launch", icon::LAUNCH, true, pal))
            .push(self.nav_item("版本管理", "nav.versions", icon::VERSIONS, false, pal))
            .push(self.nav_item("插件管理", "nav.plugins", icon::PLUGINS, false, pal))
            .push(self.nav_item("环境", "nav.env", icon::ENV, false, pal))
            .push(self.nav_item("控制台", "nav.console", icon::CONSOLE, false, pal))
            .push(self.nav_item("设置", "nav.settings", icon::SETTINGS, false, pal));

        let foot = column![
            row![
                container(space::Space::new())
                    .width(7.0)
                    .height(7.0)
                    .style(dot_style(pal)),
                txt("环境就绪").size(10.5).color(pal.text_3),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            foot_row("dsh", "0.1.1-rc.2", pal),
            foot_row("Node", "v24.19.0", pal),
        ]
        .spacing(6);

        // DrawTicker 放侧边栏（常驻可见）。放 scrollable 里会被视口剔除，
        // 永远不 draw——阶段 0 那条 intersects(viewport) 剔除逻辑的活教材。
        container(column![brand, nav, space::vertical(), foot, Element::new(DrawTicker)].spacing(0))
            .width(Length::Fixed(232.0))
            .height(Fill)
            .padding(Padding::from(14).top(22).bottom(16))
            .style(side_style(pal))
            .into()
    }

    fn nav_item(
        &self,
        label: &'static str,
        key: anim::Key,
        ico: &'static [u8],
        active: bool,
        pal: &'static Palette,
    ) -> Element<'_, Message> {
        let t = self.anim.value(key);
        // active：色块 + 1px 描边，无阴影（借鉴 orevx 的导航选中态）；
        // 非 active：hover 补间叠加色。
        let (bg, border_c, border_w) = if active {
            (pal.surface_1.into(), pal.card_border, 1.0)
        } else {
            (
                theme::lerp(Color::TRANSPARENT, pal.hover, t).into(),
                Color::TRANSPARENT,
                0.0,
            )
        };
        let text_c = if active {
            pal.text
        } else {
            theme::lerp(pal.text_2, pal.text, t)
        };
        let icon_c = if active {
            pal.accent
        } else {
            theme::lerp(pal.text_3, pal.text_2, t)
        };

        let item = container(
            row![
                icon::icon::<Message>(ico, 18.0, icon_c),
                txt(label).size(12).color(text_c),
            ]
            .spacing(11)
            .align_y(Alignment::Center),
        )
        .width(Fill)
        .height(36.0)
        .padding(Padding::from([0, 11]))
        .style(move |_theme: &Theme| iced::widget::container::Style {
            text_color: Some(text_c),
            background: Some(bg),
            border: Border {
                color: border_c,
                width: border_w,
                radius: 10.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        });

        mouse_area(item)
            .on_enter(Message::HoverEnter(key))
            .on_exit(Message::HoverExit(key))
            .into()
    }

    // ---- 页面头部 ----

    fn page_head(&self, pal: &'static Palette) -> Element<'_, Message> {
        let toggle_label = match self.mode {
            Mode::Dark => "切到浅色",
            Mode::Light => "切到暗色",
        };
        row![
            column![
                txt_bold("视觉地基").size(19).color(pal.text),
                txt("阶段 1 · 卡片 / 按钮 / 过渡动画 —— 与上一代 styles.css 逐条对照")
                    .size(12)
                    .color(pal.text_3),
            ]
            .spacing(5),
            space::horizontal(),
            mk_btn(
                "b.theme",
                toggle_label,
                Variant::Secondary,
                BtnSize::Medium,
                false,
                pal,
                &self.anim,
            )
            .with_press(Message::ToggleTheme),
        ]
        .width(Fill)
        .align_y(Alignment::Center)
        .into()
    }

    // ---- 验收卡 1：四类按钮 ----

    fn card_buttons(&self, pal: &'static Palette) -> Element<'_, Message> {
        let row_md = row![
            mk_btn("b.primary", "启动 harness", Variant::Primary, BtnSize::Medium, false, pal, &self.anim),
            mk_btn("b.secondary", "打开目录", Variant::Secondary, BtnSize::Medium, false, pal, &self.anim),
            mk_btn("b.danger", "删除", Variant::Danger, BtnSize::Medium, false, pal, &self.anim),
            mk_btn("b.teal", "安装", Variant::Teal, BtnSize::Medium, false, pal, &self.anim),
            mk_btn("b.quiet", "卸载", Variant::QuietDanger, BtnSize::Medium, false, pal, &self.anim),
            mk_btn("b.ghost", "取消", Variant::Ghost, BtnSize::Medium, false, pal, &self.anim),
        ]
        .spacing(10);

        let row_sm = row![
            mk_btn("b.sm.primary", "启动", Variant::Primary, BtnSize::Small, false, pal, &self.anim),
            mk_btn("b.sm.secondary", "打开目录", Variant::Secondary, BtnSize::Small, false, pal, &self.anim),
            mk_btn("b.sm.danger", "删除", Variant::Danger, BtnSize::Small, false, pal, &self.anim),
            mk_btn("b.sm.teal", "安装", Variant::Teal, BtnSize::Small, false, pal, &self.anim),
            mk_btn("b.sm.quiet", "卸载", Variant::QuietDanger, BtnSize::Small, false, pal, &self.anim),
            mk_btn("b.sm.ghost", "取消", Variant::Ghost, BtnSize::Small, false, pal, &self.anim),
        ]
        .spacing(10);

        let row_state = row![
            mk_btn("b.hero", "启动", Variant::Primary, BtnSize::Hero, false, pal, &self.anim),
            mk_btn("b.dis.primary", "启动（禁用）", Variant::Primary, BtnSize::Medium, true, pal, &self.anim),
            mk_btn("b.dis.secondary", "打开目录（禁用）", Variant::Secondary, BtnSize::Medium, true, pal, &self.anim),
            mk_btn("b.dis.danger", "删除（禁用）", Variant::Danger, BtnSize::Medium, true, pal, &self.anim),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        card::card(
            Column::new()
                .push(card::card_title("按钮 · 六类变体 × 三档尺寸 × 禁用态"))
                .push(card::card_sub(
                    "对照 CSS .btn / .btn-primary / .btn-danger / .btn-teal / .btn-quiet-danger / .btn-ghost。主按钮与 teal 按钮是 180° 竖向渐变（Background::Gradient，阶段 1 验证项）。",
                    pal,
                ))
                .push(row_md)
                .push(row_sm)
                .push(row_state)
                .push(
                    txt(format!("上次点击：{}", self.last_action))
                        .size(11.5)
                        .color(pal.text_3),
                )
                .spacing(14),
            pal,
        )
    }

    // ---- 验收卡 2：三档软阴影 ----

    fn card_shadows(&self, pal: &'static Palette) -> Element<'_, Message> {
        card::card(
            Column::new()
                .push(card::card_title("卡片 · 三档软阴影"))
                .push(card::card_sub(
                    "Shadow 无 spread，负 spread 的收缩靠调小 blur 近似（阶段 0 结论）。",
                    pal,
                ))
                .push(
                    row![
                        shadow_demo("shadow_card", "blur 30 / dy 14 —— 卡片浮起", pal.shadow_card, pal),
                        shadow_demo("shadow_pop", "blur 48 / dy 18 —— 弹层", pal.shadow_pop, pal),
                        shadow_demo("shadow_btn", "blur 14 / dy 5 —— 主按钮彩色投影", pal.shadow_btn, pal),
                    ]
                    .spacing(18),
                )
                .spacing(14),
            pal,
        )
    }

    // ---- 验收卡 3：svg 动态着色 ----

    fn card_icons(&self, pal: &'static Palette) -> Element<'_, Message> {
        let all = [
            icon::LAUNCH,
            icon::VERSIONS,
            icon::PLUGINS,
            icon::ENV,
            icon::CONSOLE,
            icon::SETTINGS,
        ];
        let muted: Row<'_, Message> = all.iter().fold(row![], |r, data| {
            r.push(icon::icon::<Message>(*data, 20.0, pal.text_3)).spacing(14)
        });
        let tinted: Row<'_, Message> = all.iter().enumerate().fold(row![], |r, (i, data)| {
            let c = match i % 3 {
                0 => pal.accent,
                1 => pal.teal,
                _ => pal.bad,
            };
            r.push(icon::icon::<Message>(*data, 20.0, c)).spacing(14)
        });

        card::card(
            Column::new()
                .push(card::card_title("图标 · svg 动态换色（阶段 1 验证项）"))
                .push(card::card_sub(
                    "同一批 include_bytes! 的 SVG，靠 svg::Style{color} 做像素级 RGB 替换（保留 alpha）。上一代是内联 SVG + currentColor，等价。",
                    pal,
                ))
                .push(muted)
                .push(tinted)
                .spacing(14),
            pal,
        )
    }

    // ---- 验收卡 4：hover 补间 + 订阅生命周期 ----

    fn card_anim(&self, pal: &'static Palette) -> Element<'_, Message> {
        let t = self.anim.value("b.anim");
        let readout = format!(
            "活跃补间 {} 个 · t = {:.2} · 累计出帧 {} —— 鼠标移开后补间归零，frames() 订阅即撤",
            self.anim.tween_count(),
            t,
            DRAWS.load(Ordering::Relaxed)
        );

        card::card(
            Column::new()
                .push(card::card_title("过渡 · hover 补间（CSS transition .14s 的等价物）"))
                .push(card::card_sub(
                    "把鼠标移到下面按钮上：背景色从静止值补间到 hover 值。动画期间才订阅 window::frames()，结束立刻撤订——空闲 CPU 归零的前提（DESIGN.md §8 第 1 条）。按 T 键切换主题。",
                    pal,
                ))
                .push(
                    row![
                        mk_btn("b.anim", "把鼠标移上来", Variant::Primary, BtnSize::Hero, false, pal, &self.anim),
                        mk_btn("b.anim2", "这个也是", Variant::Secondary, BtnSize::Medium, false, pal, &self.anim),
                    ]
                    .spacing(10),
                )
                .push(mono(readout).size(11.5).color(pal.text_2))
                .spacing(14),
            pal,
        )
    }
}

/// 按钮快捷构造。写成泛型函数而不是闭包：闭包无法推断 `Element<'a>` 的 'a。
fn mk_btn<'a>(
    key: anim::Key,
    label: &'static str,
    variant: Variant,
    size: BtnSize,
    disabled: bool,
    pal: &'static Palette,
    anim: &'a AnimState,
) -> ButtonHandle<'a> {
    ButtonHandle {
        spec: Spec::new(key, label, variant).size(size).disabled(disabled),
        pal,
        anim,
        press: Some(Message::Pressed(label)),
    }
}

/// 延迟构造：让 page_head 能把 on_press 换成 ToggleTheme。
struct ButtonHandle<'a> {
    spec: Spec,
    pal: &'static Palette,
    anim: &'a AnimState,
    press: Option<Message>,
}

impl<'a> ButtonHandle<'a> {
    fn with_press(mut self, msg: Message) -> Self {
        self.press = Some(msg);
        self
    }
}

impl<'a> From<ButtonHandle<'a>> for Element<'a, Message> {
    fn from(h: ButtonHandle<'a>) -> Self {
        let key = h.spec.key;
        button::btn(
            h.spec,
            h.pal,
            h.anim,
            h.press,
            Some(Message::HoverEnter(key)),
            Some(Message::HoverExit(key)),
        )
    }
}

fn foot_row<'a>(k: &'static str, v: &'static str, pal: &'static Palette) -> Element<'a, Message> {
    row![
        txt(k).size(10.5).color(pal.text_3),
        space::horizontal(),
        mono(v).size(10.5).color(pal.text_2),
    ]
    .width(Fill)
    .into()
}

fn shadow_demo<'a>(
    label: &'static str,
    sub: &'static str,
    shadow: Shadow,
    pal: &'static Palette,
) -> Element<'a, Message> {
    container(
        column![
            txt_bold(label).size(13).color(pal.text),
            txt(sub).size(10.5).color(pal.text_3),
        ]
        .spacing(4),
    )
    .width(Length::Fixed(220.0))
    .padding(18)
    .style(move |_theme: &Theme| iced::widget::container::Style {
        text_color: Some(pal.text),
        background: Some(pal.surface_2.into()),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 16.0.into(),
        },
        shadow,
        snap: true,
    })
    .into()
}

/// 品牌方块：CSS `linear-gradient(150deg, accent-hi, accent)` + 彩色投影。
/// iced 的角度与 CSS 同向（Radians::to_distance 里 angle−90°，y 轴向下，
/// 180° 即自上而下——与 CSS linear-gradient(180deg) 一致）。
fn brand_style(pal: &'static Palette) -> impl Fn(&Theme) -> iced::widget::container::Style + Copy + 'static {
    move |_theme: &Theme| iced::widget::container::Style {
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
    }
}

fn dot_style(pal: &'static Palette) -> impl Fn(&Theme) -> iced::widget::container::Style + Copy + 'static {
    move |_theme: &Theme| iced::widget::container::Style {
        text_color: None,
        background: Some(pal.ok.into()),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: R_PILL.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn side_style(pal: &'static Palette) -> impl Fn(&Theme) -> iced::widget::container::Style + Copy + 'static {
    move |_theme: &Theme| iced::widget::container::Style {
        text_color: Some(pal.text),
        background: Some(pal.bg_side.into()),
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// 环境光渐变：自上而下（角度 π），蓝 → 青 → 透明。
/// 亮色两档都 transparent，整条带子不可见，零成本。
fn ambient_style(pal: &'static Palette) -> impl Fn(&Theme) -> iced::widget::container::Style + Copy + 'static {
    move |_theme: &Theme| iced::widget::container::Style {
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
    }
}

/// 1x1 数帧 widget（阶段 0 验证过的做法，注释见 phase0/src/main.rs）。
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
        DRAWS.fetch_add(1, Ordering::Relaxed);
    }
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
