//! 顶层 State / Message / update / view / subscription。
//!
//! 阶段 1 的产出是视觉地基（卡片/按钮/过渡/图标），阶段 2 在此之上把后端接进来：
//! `core/` 的异步函数经 `bridge` 变成 `Task`，事件流经 `Subscription` 进 `update`。
//! 当前页面是「环境」页的最小可用版——真实读配置、真实探测 node/dsh/pnpm、
//! 真实列 profile，进度与日志走 CoreEvent。
//!
//! 页面骨架（侧边栏 232px + 主区）照抄上一代 `.app` 的 grid。

use crate::bridge;
use crate::core::envres::EnvStatus;
use crate::core::event::{CoreEvent, LogStream};
use crate::core::procman::ProcStatus;
use crate::core::profiles::ProfileInfo;
use crate::core::store::Config;
use crate::theme::{self, HERO_NUM_SIZE, Palette, R_PILL};
use crate::ui::anim::{self, AnimState, HOVER_DUR};
use crate::ui::button::{self, Size as BtnSize, Spec, Variant};
use crate::ui::titlebar;
use crate::ui::{card, icon, mono, txt, txt_bold};
use iced::widget::{Column, Row, column, container, mouse_area, row, scrollable, space, stack};
use iced::{
    Alignment, Border, Color, Element, Fill, Length, Padding, Shadow, Subscription, Task, Theme,
    Vector, window,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 真实绘制次数（阶段 0 的测法：自定义 widget 在 draw() 里自增，不走 Message，
/// 否则「订阅出帧」会自己触发下一帧，测出来的空闲是假的）。
pub static DRAWS: AtomicU64 = AtomicU64::new(0);

/// 日志环形缓冲上限（DESIGN.md §8 第 4 条）。
const LOG_CAP: usize = 2000;

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

/// 一条日志。`ts` 是毫秒时间戳，展示时只取时分秒。
pub struct LogLine {
    /// 归属的 profile（或 EnvProgress 的 task 名）。阶段 3 的控制台页要按它过滤。
    #[allow(dead_code)]
    pub profile: String,
    pub stream: LogStream,
    pub line: String,
    pub ts: i64,
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
    /// 最大化状态。无边框后系统不再管这个，标题栏按钮字形要跟着变。
    pub maximized: bool,
    /// `--e2e`：开窗后自动跑一遍「启动第一个 profile → 等 8s → 停止」，
    /// 用日志证明后端链路（spawn / stdout 转发 / URL 解析 / taskkill）真的通。
    pub e2e: bool,

    // ---- 阶段 2：真实后端状态 ----
    pub config: Config,
    pub env: Option<EnvStatus>,
    pub profiles: Vec<ProfileInfo>,
    pub procs: Vec<ProcStatus>,
    pub logs: VecDeque<LogLine>,
    /// 全局忙提示（正在探测/安装/启动…）。非 None 时相关按钮禁用。
    pub busy: Option<String>,
    /// 最近一次错误，显示在页面上。
    pub error: Option<String>,
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
    /// 自绘标题栏：拖动窗口（交给系统，不自己算 delta）。
    DragWindow,
    Minimize,
    ToggleMaximize,
    /// 最大化状态回读，用来切标题栏按钮字形。
    MaximizedChanged(bool),
    Close,
    /// 边缘热区：开始缩放。
    Resize(window::Direction),

    // ---- 阶段 2：后端链路 ----
    /// 后端事件流（进度/日志/URL/退出）。
    Core(CoreEvent),
    /// 重新探测环境（node/pnpm/dsh 版本）。
    RefreshEnv,
    EnvLoaded(EnvStatus),
    /// 重新扫 profile 列表。
    RefreshProfiles,
    ProfilesLoaded(Result<Vec<ProfileInfo>, String>),
    /// 有实例在跑时才起的 2s 轮询（DESIGN.md §8 第 1 条）。
    PollProcs,
    ProcsLoaded(Vec<ProcStatus>),
    /// 打开数据目录 / WebUI 地址。
    OpenPath(String),
    /// 启动某个 profile 的 `dsh web`。
    Start(String),
    /// 启动结果：(profile, url) 或错误。
    Started(Result<(String, String), String>),
    /// 停止某个 profile（taskkill 整棵进程树）。
    Stop(String),
    Stopped(Result<String, String>),
    /// 后端操作失败。
    Failed(String),
    /// `--e2e` 的第二拍：启动成功若干秒后自动停止，验证完整链路。
    E2eStop,
}

impl Dshnext {
    pub fn new(mode: Mode, shot: Option<Shot>, autotest: bool, e2e: bool) -> Self {
        // 配置是同步读的（一个小 JSON 文件），不值得为它开 Task。
        let config = crate::core::store::load();
        // 主题跟随 config（与上一代 config.json 完全兼容），命令行 --theme 优先。
        Self {
            mode,
            anim: AnimState::default(),
            window: None,
            shot,
            last_action: "（还没有）",
            autotest,
            maximized: false,
            e2e,
            config,
            env: None,
            profiles: Vec::new(),
            procs: Vec::new(),
            logs: VecDeque::new(),
            busy: None,
            error: None,
        }
    }

    pub fn palette(&self) -> &'static Palette {
        match self.mode {
            Mode::Dark => &Palette::DARK,
            Mode::Light => &Palette::LIGHT,
        }
    }

    /// 追加一条日志，超上限从头弹出（DESIGN.md §8 第 4 条）。
    fn push_log(&mut self, profile: String, stream: LogStream, line: String, ts: i64) {
        if self.logs.len() >= LOG_CAP {
            self.logs.pop_front();
        }
        self.logs.push_back(LogLine {
            profile,
            stream,
            line,
            ts,
        });
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Opened(id) => {
                self.window = Some(id);
                let mut tasks = Vec::new();
                // 开窗即打通两条最简链路：环境探测 + profile 列表（DESIGN.md §10 步骤 10）。
                tasks.push(Task::done(Message::RefreshEnv));
                tasks.push(Task::done(Message::RefreshProfiles));
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
                // 主题存进 config.json（与上一代同一个文件、同一个字段）。
                self.config.theme = match self.mode {
                    Mode::Dark => "dark".into(),
                    Mode::Light => "light".into(),
                };
                let cfg = self.config.clone();
                Task::perform(
                    async move { crate::core::store::save(&cfg) },
                    |r| match r {
                        Ok(()) => Message::Pressed("主题已保存"),
                        Err(e) => Message::Failed(format!("保存配置失败：{e}")),
                    },
                )
            }
            Message::Pressed(label) => {
                log::info!("pressed {label}");
                self.last_action = label;
                Task::none()
            }
            Message::DragWindow => match self.window {
                // window::drag 把后续拖动交给系统（WM_NCLBUTTONDOWN + HTCAPTION），
                // 比自己算 delta 再 move_to 跟手得多，也不吃帧。
                Some(id) => window::drag(id),
                None => Task::none(),
            },
            Message::Minimize => match self.window {
                Some(id) => window::minimize(id, true),
                None => Task::none(),
            },
            Message::ToggleMaximize => match self.window {
                // toggle 完再回读一次真实状态：万一系统拒绝（如已全屏），
                // 本地布尔翻转就和现实脱节，按钮字形会错。
                Some(id) => window::toggle_maximize::<Message>(id)
                    .chain(window::is_maximized(id).map(Message::MaximizedChanged)),
                None => Task::none(),
            },
            Message::MaximizedChanged(v) => {
                self.maximized = v;
                Task::none()
            }
            Message::Close => match self.window {
                Some(id) => window::close(id),
                None => iced::exit(),
            },
            Message::Resize(dir) => match self.window {
                Some(id) => window::drag_resize(id, dir),
                None => Task::none(),
            },

            // ---- 阶段 2：后端链路 ----
            Message::Core(event) => {
                log::debug!("core event: {event:?}");
                match event {
                    CoreEvent::EnvProgress { task, line } => {
                        self.push_log(
                            task,
                            LogStream::System,
                            line,
                            crate::core::event::now_millis(),
                        );
                    }
                    CoreEvent::Log {
                        profile,
                        stream,
                        line,
                        ts,
                    } => self.push_log(profile, stream, line, ts),
                    CoreEvent::Url { profile, url } => {
                        self.push_log(
                            profile,
                            LogStream::System,
                            format!("WebUI 地址：{url}"),
                            crate::core::event::now_millis(),
                        );
                    }
                    CoreEvent::Exit { profile, code } => {
                        self.push_log(
                            profile.clone(),
                            LogStream::System,
                            format!("进程退出，退出码 {code}"),
                            crate::core::event::now_millis(),
                        );
                        self.procs.retain(|p| p.profile != profile);
                    }
                }
                Task::none()
            }
            Message::RefreshEnv => {
                self.busy = Some("正在探测环境…".into());
                let cfg = self.config.clone();
                // envres::status 会跑三次 `xx --version`，每次最多 20s 超时，
                // 必须走 Task 异步，不能在 update 里 block_on。
                Task::perform(
                    async move { crate::core::envres::status(&cfg).await },
                    Message::EnvLoaded,
                )
            }
            Message::EnvLoaded(env) => {
                self.busy = None;
                self.env = Some(env);
                Task::none()
            }
            Message::RefreshProfiles => Task::perform(
                // 目录遍历是同步的，扔到阻塞线程池免得卡住事件循环。
                async move {
                    tokio::task::spawn_blocking(crate::core::profiles::list)
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::ProfilesLoaded,
            ),
            Message::ProfilesLoaded(result) => {
                match result {
                    Ok(list) => {
                        self.profiles = list;
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
                // e2e 要等 profile 列表到手才知道启动谁；只跑一次。
                if self.e2e {
                    self.e2e = false;
                    if let Some(first) = self.profiles.first().map(|p| p.name.clone()) {
                        log::info!("e2e: 启动 {first}");
                        return Task::done(Message::Start(first));
                    }
                    log::warn!("e2e: 没有 profile 可启动");
                }
                Task::none()
            }
            Message::E2eStop => {
                let names: Vec<String> = self.procs.iter().map(|p| p.profile.clone()).collect();
                log::info!("e2e: 停止 {names:?}");
                Task::batch(names.into_iter().map(|n| Task::done(Message::Stop(n))))
            }
            Message::PollProcs => {
                let procs = bridge::procs();
                Task::perform(
                    async move { crate::core::procman::status(&procs).await },
                    Message::ProcsLoaded,
                )
            }
            Message::ProcsLoaded(list) => {
                self.procs = list;
                Task::none()
            }
            Message::OpenPath(path) => {
                // 替代 tauri-plugin-opener：目录交给资源管理器，http 交给浏览器。
                if let Err(e) = open::that_detached(&path) {
                    self.error = Some(format!("打开失败：{e}"));
                }
                Task::none()
            }
            Message::Start(profile) => {
                self.busy = Some(format!("正在启动 {profile}…"));
                self.error = None;
                let tx = bridge::sink();
                let procs = bridge::procs();
                let cfg = self.config.clone();
                let port = self.config.port;
                Task::perform(
                    async move {
                        crate::core::procman::start(tx, &procs, &cfg, &profile, port)
                            .await
                            .map(|url| (profile, url))
                    },
                    Message::Started,
                )
            }
            Message::Started(result) => {
                self.busy = None;
                // e2e 标志在 ProfilesLoaded 里已被置 false，这里用 shot.is_none() 之外
                // 的独立标记会更清楚——但简单起见：只要命令行给了 --e2e 就走自动停止。
                let was_e2e = std::env::args().any(|a| a == "--e2e");
                match result {
                    Ok((profile, url)) => {
                        self.push_log(
                            profile,
                            LogStream::System,
                            format!("已启动，WebUI {url}"),
                            crate::core::event::now_millis(),
                        );
                        // 起进程后立刻拉一次状态：轮询订阅要靠 procs 非空才挂上。
                        let mut tasks = vec![Task::done(Message::PollProcs)];
                        if self.config.auto_open && !was_e2e {
                            tasks.push(Task::done(Message::OpenPath(url)));
                        }
                        if was_e2e {
                            tasks.push(Task::perform(
                                async { tokio::time::sleep(Duration::from_secs(8)).await },
                                |_| Message::E2eStop,
                            ));
                        }
                        Task::batch(tasks)
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            Message::Stop(profile) => {
                self.busy = Some(format!("正在停止 {profile}…"));
                let tx = bridge::sink();
                let procs = bridge::procs();
                Task::perform(
                    async move {
                        crate::core::procman::stop(&tx, &procs, &profile)
                            .await
                            .map(|()| profile)
                    },
                    Message::Stopped,
                )
            }
            Message::Stopped(result) => {
                self.busy = None;
                match result {
                    Ok(profile) => {
                        self.procs.retain(|p| p.profile != profile);
                        Task::done(Message::PollProcs)
                    }
                    Err(e) => {
                        self.error = Some(e);
                        Task::none()
                    }
                }
            }
            Message::Failed(e) => {
                log::warn!("操作失败：{e}");
                self.busy = None;
                self.error = Some(e);
                Task::none()
            }
        }
    }

    /// 空闲零订阅（DESIGN.md §8 第 1 条）：
    /// - `frames()` 只在动画期间挂
    /// - 进程轮询只在**有实例在跑**时挂（上一代是无条件 setInterval(2000)）
    /// - core 事件流是被动的：后端不发东西就不产消息，不造帧
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            window::open_events().map(Message::Opened),
            bridge::events().map(Message::Core),
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
        if !self.procs.is_empty() {
            subs.push(
                iced::time::every(Duration::from_secs(2)).map(|_| Message::PollProcs),
            );
        }
        Subscription::batch(subs)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let pal = self.palette();

        let main = column![
            self.page_head(pal),
            self.card_env(pal),
            self.card_profiles(pal),
            self.card_logs(pal),
            self.card_buttons(pal),
            self.card_shadows(pal),
            self.card_icons(pal),
            self.card_anim(pal),
        ]
        .spacing(18)
        .padding(Padding::from(44).top(28));

        // 环境光层固定在顶部（不随内容滚动），卡片在其上滚过——
        // 对应 orevx 把渐变挂在 html 背景、内容滚过它的效果。
        let main_area = scrollable(main).width(Fill).height(Fill);

        // 无边框窗口：标题栏在最上，内容在下，整体套一层圆角边框容器，
        // 最后叠一层边缘缩放热区。渐变要盖到标题栏，所以 ambient 在最外层 stack。
        let body = stack![
            self.ambient(pal),
            column![
                titlebar::titlebar(
                    "DshDesk — DeepSeek Harness 启动器",
                    pal,
                    &self.anim,
                    titlebar::Actions {
                        drag: Message::DragWindow,
                        minimize: Message::Minimize,
                        toggle_maximize: Message::ToggleMaximize,
                        close: Message::Close,
                    },
                    self.maximized,
                    Message::HoverEnter,
                    Message::HoverExit,
                ),
                row![self.sidebar(pal), main_area].width(Fill).height(Fill),
            ]
            .width(Fill)
            .height(Fill),
        ]
        .width(Fill)
        .height(Fill);

        // 窗口圆角交给 DWM（main.rs 的 corner_preference: Round）——自己画圆角
        // 会和方形的窗口表面对不齐、角上露直角。这层只负责底色与外描边。
        let shell = container(body)
            .width(Fill)
            .height(Fill)
            .style(move |_theme: &Theme| container::Style {
                text_color: Some(pal.text),
                background: Some(pal.bg_app.into()),
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            });

        stack![shell, titlebar::resize_grips(Message::Resize)]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// 顶部环境光渐变带：蓝 → 青 → 透明，高 300px（含标题栏）。
    /// 亮色两档都 transparent，整条带子不可见，零成本。
    fn ambient(&self, pal: &'static Palette) -> Element<'_, Message> {
        column![
            container(space::Space::new())
                .width(Fill)
                .height(300.0)
                .style(ambient_style(pal)),
            space::vertical(),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// 环境卡：真实探测结果（`core::envres::status`）+ hero 数字排版。
    /// 这是阶段 2「端到端打通」的第一条链路——数字是真的从磁盘和子进程读来的。
    fn card_env(&self, pal: &'static Palette) -> Element<'_, Message> {
        let mut col = Column::new()
            .push(card::card_title("环境 · 真实探测结果"))
            .push(card::card_sub(
                "core::envres::status() 跑三次 `xx --version` 探测托管运行时；配置读自 %LOCALAPPDATA%\\DshDesk\\config.json，与上一代同一个文件。",
                pal,
            ))
            .spacing(14);

        col = match &self.env {
            None => col.push(
                txt(match &self.busy {
                    Some(msg) => msg.clone(),
                    None => "尚未探测".to_string(),
                })
                .size(12)
                .color(pal.text_2),
            ),
            Some(env) => {
                // hero 数字：profile 数量（借鉴 orevx 的 48px/600 排版）
                let hero = column![
                    txt("已安装版本").size(10.5).color(pal.text_3),
                    txt_bold(format!("{}", self.profiles.len()))
                        .size(HERO_NUM_SIZE)
                        .color(pal.text),
                ]
                .spacing(2);

                let meta = row![
                    meta_cell("DSH", env.dsh_version.as_deref().unwrap_or("未安装"), pal),
                    meta_cell("NODE", env.node_version.as_deref().unwrap_or("未安装"), pal),
                    meta_cell("PNPM", env.pnpm_version.as_deref().unwrap_or("未安装"), pal),
                    meta_cell(
                        "托管 NODE",
                        if env.node_managed { "是" } else { "否（系统 PATH）" },
                        pal,
                    ),
                ]
                .spacing(30);

                col.push(row![hero, space::horizontal(), meta].align_y(Alignment::End))
                    .push(kv("数据目录", &env.data_dir, pal))
                    .push(kv("DSH_HOME", &env.home_dir, pal))
            }
        };

        let refresh_disabled = self.busy.is_some();
        col = col.push(
            row![
                mk_btn(
                    "b.env.refresh",
                    "重新探测",
                    Variant::Secondary,
                    BtnSize::Small,
                    refresh_disabled,
                    pal,
                    &self.anim,
                )
                .with_press(Message::RefreshEnv),
                mk_btn(
                    "b.env.open",
                    "打开数据目录",
                    Variant::Secondary,
                    BtnSize::Small,
                    self.env.is_none(),
                    pal,
                    &self.anim,
                )
                .with_press(Message::OpenPath(
                    self.env
                        .as_ref()
                        .map(|e| e.data_dir.clone())
                        .unwrap_or_default()
                )),
            ]
            .spacing(10),
        );

        if let Some(err) = &self.error {
            col = col.push(txt(err.clone()).size(11.5).color(pal.bad));
        }

        card::card(col, pal)
    }

    /// 版本（profile）列表：真实扫 `$DSH_HOME/profiles/`。
    fn card_profiles(&self, pal: &'static Palette) -> Element<'_, Message> {
        let mut col = Column::new()
            .push(card::card_title(format!(
                "版本管理 · {} 个 profile",
                self.profiles.len()
            )))
            .push(card::card_sub(
                "core::profiles::list() 读每个目录的 package.json，取 dsh.profile.bundles 与 dependencies。",
                pal,
            ))
            .spacing(14);

        if self.profiles.is_empty() {
            col = col.push(txt("还没有版本。到上一代或用 dsh 新建一个即可。").size(12).color(pal.text_3));
        } else {
            for p in &self.profiles {
                let running = self.procs.iter().find(|s| s.profile == p.name);
                col = col.push(self.profile_row(p, running, pal));
            }
        }

        col = col.push(
            mk_btn(
                "b.prof.refresh",
                "重新扫描",
                Variant::Secondary,
                BtnSize::Small,
                false,
                pal,
                &self.anim,
            )
            .with_press(Message::RefreshProfiles),
        );

        card::card(col, pal)
    }

    /// profile 一行：名字 + 运行状态 + 元信息 + 启动/停止/打开目录。
    /// 写成方法而不是自由函数，因为按钮要读 `self.anim` 的 hover 补间值。
    fn profile_row<'a>(
        &'a self,
        p: &'a ProfileInfo,
        running: Option<&'a ProcStatus>,
        pal: &'static Palette,
    ) -> Element<'a, Message> {
        let status: Element<'_, Message> = match running {
            Some(s) => row![
                container(space::Space::new())
                    .width(7.0)
                    .height(7.0)
                    .style(dot_style(pal, pal.ok)),
                txt(format!("运行中 · PID {} · {}s", s.pid, s.uptime_secs))
                    .size(10.5)
                    .color(pal.ok),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
            None => txt("空闲").size(10.5).color(pal.text_3).into(),
        };

        // hover 补间的 key 必须每行唯一，但 anim::Key 是 &'static str，
        // profile 名是运行期字符串——用固定前缀 + 索引不可靠（列表会变）。
        // 折中：这一组按钮不做 hover 过渡（传 None），点击仍然正常。
        let actions: Element<'_, Message> = match running {
            Some(s) => row![
                button::btn(
                    Spec::new("row.open", "打开界面", Variant::Secondary).size(BtnSize::Small),
                    pal,
                    &self.anim,
                    Some(Message::OpenPath(s.url.clone())),
                    None,
                    None,
                ),
                button::btn(
                    Spec::new("row.stop", "停止", Variant::QuietDanger).size(BtnSize::Small),
                    pal,
                    &self.anim,
                    Some(Message::Stop(p.name.clone())),
                    None,
                    None,
                ),
            ]
            .spacing(8)
            .into(),
            None => row![
                button::btn(
                    Spec::new("row.start", "启动", Variant::Primary).size(BtnSize::Small),
                    pal,
                    &self.anim,
                    Some(Message::Start(p.name.clone())),
                    None,
                    None,
                ),
                button::btn(
                    Spec::new("row.dir", "打开目录", Variant::Secondary).size(BtnSize::Small),
                    pal,
                    &self.anim,
                    Some(Message::OpenPath(p.path.clone())),
                    None,
                    None,
                ),
            ]
            .spacing(8)
            .into(),
        };

        row![
            column![
                row![txt_bold(p.name.clone()).size(13).color(pal.text), status]
                    .spacing(10)
                    .align_y(Alignment::Center),
                mono(format!(
                    "{} 个 bundle · {} 个依赖",
                    p.bundles.len(),
                    p.dependencies.len()
                ))
                .size(10.5)
                .color(pal.text_3),
            ]
            .spacing(2),
            space::horizontal(),
            actions,
        ]
        .width(Fill)
        .align_y(Alignment::Center)
        .spacing(12)
        .into()
    }

    /// 日志卡：CoreEvent 流的落点。空的时候说明还没有后端活动。
    fn card_logs(&self, pal: &'static Palette) -> Element<'_, Message> {
        let mut col = Column::new()
            .push(card::card_title(format!("控制台 · {} 行", self.logs.len())))
            .push(card::card_sub(
                "后端事件经 tokio channel → Subscription::run → update()，按 stream 着色。环形缓冲上限 2000 行。",
                pal,
            ))
            .spacing(6);

        if self.logs.is_empty() {
            col = col.push(txt("（暂无输出）").size(11.5).color(pal.text_3));
        } else {
            // 只渲染最后 30 行：整份 2000 行的虚拟滚动是阶段 3 的事（DESIGN.md §7.3）。
            for l in self.logs.iter().rev().take(30).collect::<Vec<_>>().into_iter().rev() {
                col = col.push(log_row(l, pal));
            }
        }

        card::card(col, pal)
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

        // 底部三行改成真实值：环境是否就绪由探测结果决定，不再是写死的文案。
        let ready = self
            .env
            .as_ref()
            .is_some_and(|e| e.dsh_version.is_some() && e.node_version.is_some());
        let (dot_color, ready_text) = match &self.env {
            None => (pal.text_3, "未探测"),
            Some(_) if ready => (pal.ok, "环境就绪"),
            Some(_) => (pal.warn, "环境不完整"),
        };
        let foot = column![
            row![
                container(space::Space::new())
                    .width(7.0)
                    .height(7.0)
                    .style(dot_style(pal, dot_color)),
                txt(ready_text).size(10.5).color(pal.text_3),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            foot_row(
                "dsh",
                self.env
                    .as_ref()
                    .and_then(|e| e.dsh_version.clone())
                    .unwrap_or_else(|| "—".into()),
                pal
            ),
            foot_row(
                "Node",
                self.env
                    .as_ref()
                    .and_then(|e| e.node_version.clone())
                    .unwrap_or_else(|| "—".into()),
                pal
            ),
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

fn foot_row<'a>(k: &'static str, v: String, pal: &'static Palette) -> Element<'a, Message> {
    row![
        txt(k).size(10.5).color(pal.text_3),
        space::horizontal(),
        mono(v).size(10.5).color(pal.text_2),
    ]
    .width(Fill)
    .into()
}

/// meta 信息带的一格（对应上一代 CSS `.meta`）。
fn meta_cell<'a>(label: &'static str, value: &str, pal: &'static Palette) -> Element<'a, Message> {
    column![
        txt(label).size(9.5).color(pal.text_3),
        mono(value.to_string()).size(12.5).color(pal.text),
    ]
    .spacing(3)
    .into()
}

/// 键值行（对应上一代 CSS `.kv`）。
fn kv<'a>(key: &'static str, value: &str, pal: &'static Palette) -> Element<'a, Message> {
    row![
        container(txt(key).size(11).color(pal.text_2)).width(Length::Fixed(90.0)),
        mono(value.to_string()).size(11).color(pal.text_3),
    ]
    .spacing(12)
    .into()
}


/// 日志一行：时间 + 来源着色（对应上一代 `.log-stdout/.log-stderr/...`）。
fn log_row<'a>(l: &LogLine, pal: &'static Palette) -> Element<'a, Message> {
    let color = match l.stream {
        LogStream::Stdout => pal.text,
        LogStream::Stderr => pal.bad,
        LogStream::System => pal.accent,
        LogStream::Plugin => pal.teal,
    };
    // 只显示时分秒：日志列很窄，完整时间戳挤掉正文。
    let secs = (l.ts / 1000) % 86400;
    let hms = format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    );
    row![
        mono(hms).size(10).color(pal.text_3),
        mono(l.line.clone()).size(10.5).color(color),
    ]
    .spacing(9)
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

fn dot_style(
    _pal: &'static Palette,
    color: Color,
) -> impl Fn(&Theme) -> iced::widget::container::Style + Copy + 'static {
    move |_theme: &Theme| iced::widget::container::Style {
        text_color: None,
        background: Some(color.into()),
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
        // 背景透明：环境光渐变在下层铺满整窗，侧边栏若填色会切出一道硬边。
        background: None,
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
