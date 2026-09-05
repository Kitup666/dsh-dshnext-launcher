//! `update` / `subscription`：所有状态迁移。与 view 分开是因为这文件本身就长，
//! 混在一起找东西费劲。

use crate::app::{Dshnext, Message, Mode, PluginTab, TOAST_TTL};
use crate::bridge;
use crate::core::event::{CoreEvent, LogStream};
use crate::pages::{self, Page};
use crate::ui::anim::{self, HOVER_DUR};
use crate::ui::modal::Dialog;
use crate::ui::widgets::ToastKind;
use iced::{Subscription, Task, window};
use std::time::{Duration, Instant};

impl Dshnext {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // 交互链路取证：--e2e / 手动点击时用 RUST_LOG=dshnext=debug 看消息是否到达。
        // Tick 与 ToastTick 是高频的，排除掉免得把日志冲掉。
        if !matches!(message, Message::Tick(_) | Message::ToastTick(_)) {
            log::debug!("msg {message:?}");
        }
        match message {
            // ---------- 窗口 ----------
            Message::Opened(id) => {
                self.window = Some(id);
                let mut tasks = vec![
                    Task::done(Message::RefreshEnv),
                    Task::done(Message::RefreshProfiles),
                    Task::done(Message::PollProcs),
                ];
                if let Some(s) = &self.shot {
                    let after = s.after;
                    tasks.push(Task::perform(
                        async move { tokio::time::sleep(Duration::from_millis(after)).await },
                        |_| Message::Shoot,
                    ));
                }
                // --switch-to：定时切一次页，让 --shot 能截在过渡中间。
                if let Some((page, at)) = self.switch {
                    tasks.push(Task::perform(
                        async move { tokio::time::sleep(Duration::from_millis(at)).await },
                        move |_| Message::Goto(page),
                    ));
                }
                if self.autotest {
                    tasks.push(Task::done(Message::HoverEnter("nav.versions")));
                    tasks.push(Task::perform(
                        async { tokio::time::sleep(Duration::from_millis(600)).await },
                        |_| Message::HoverExit("nav.versions"),
                    ));
                }
                Task::batch(tasks)
            }
            Message::Shoot => match self.window {
                Some(id) => window::screenshot(id).map(Message::Shot),
                None => Task::none(),
            },
            Message::Shot(shot) => {
                let path = self
                    .shot
                    .as_ref()
                    .map(|s| s.path.clone())
                    .unwrap_or_default();
                crate::write_png(&path, &shot);
                log::info!(
                    "已写出 {path}（{}x{} @{}x），draws={}",
                    shot.size.width,
                    shot.size.height,
                    shot.scale_factor,
                    crate::app::DRAWS.load(std::sync::atomic::Ordering::Relaxed)
                );
                iced::exit()
            }
            Message::Tick(now) => {
                self.anim.tick(now);
                Task::none()
            }
            Message::HoverEnter(key) => {
                self.anim.animate_to(key, 1.0, HOVER_DUR, Instant::now());
                Task::none()
            }
            Message::HoverExit(key) => {
                self.anim.animate_to(key, 0.0, HOVER_DUR, Instant::now());
                Task::none()
            }
            Message::DragWindow => match self.window {
                // 交给系统（WM_NCLBUTTONDOWN + HTCAPTION），比自算 delta 跟手。
                Some(id) => window::drag(id),
                None => Task::none(),
            },
            Message::Minimize => match self.window {
                Some(id) => window::minimize(id, true),
                None => Task::none(),
            },
            Message::ToggleMaximize => match self.window {
                // toggle 完回读真实状态：本地布尔翻转会和现实脱节。
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

            // ---------- 导航与浮层 ----------
            Message::Goto(page) => {
                if self.page != page {
                    self.prev_page = Some(self.page);
                    self.page = page;
                    self.start_page_anim();
                }
                // 进插件页时按需拉一次已装列表（上一代是 useEffect 依赖 selected）。
                if page == Page::Plugins && !self.selected.is_empty() {
                    return Task::done(Message::RefreshPlugins);
                }
                // 进环境页时拉版本列表，只拉一次（在途请求也算已拉）。
                if page == Page::Env && self.dsh_versions.is_empty() && !self.versions_loading {
                    return Task::done(Message::LoadVersions);
                }
                Task::none()
            }
            Message::GotoPlugins(name) => {
                self.selected = name;
                if self.page != Page::Plugins {
                    self.prev_page = Some(self.page);
                    self.page = Page::Plugins;
                    self.start_page_anim();
                }
                Task::done(Message::RefreshPlugins)
            }
            Message::Select(name) => {
                let changed = self.selected != name;
                self.selected = name;
                if changed && self.page == Page::Plugins {
                    return Task::done(Message::RefreshPlugins);
                }
                Task::none()
            }
            Message::ToggleTheme => {
                self.mode = match self.mode {
                    Mode::Dark => Mode::Light,
                    Mode::Light => Mode::Dark,
                };
                // 主题即时落盘，不跟设置页的 dirty 机制搅在一起（与上一代一致）。
                let theme = match self.mode {
                    Mode::Dark => "dark",
                    Mode::Light => "light",
                };
                self.config.theme = theme.into();
                self.cfg_draft.theme = theme.into();
                let cfg = self.config.clone();
                Task::perform(async move { crate::core::store::save(&cfg) }, |r| {
                    Message::ConfigSaved(r)
                })
            }
            Message::OpenDialog(d) => {
                self.draft = d.initial();
                self.dialog = Some(d);
                // 模态淡入。
                self.anim.animate_to("modal", 1.0, HOVER_DUR, Instant::now());
                Task::none()
            }
            Message::CloseDialog => {
                self.dialog = None;
                self.draft.clear();
                self.anim.animate_to("modal", 0.0, HOVER_DUR, Instant::now());
                Task::none()
            }
            Message::DialogInput(v) => {
                self.draft = v;
                Task::none()
            }
            Message::DialogConfirm => self.confirm_dialog(),
            Message::ToastTick(now) => {
                self.toasts.retain(|t| t.until > now);
                Task::none()
            }
            Message::Notify(kind, text) => {
                self.notify(kind, text);
                Task::none()
            }
            Message::Noop => Task::none(),

            // ---------- 后端事件流 ----------
            Message::Core(event) => self.on_core_event(event),

            // ---------- 环境 ----------
            Message::RefreshEnv => {
                let cfg = self.config.clone();
                // status() 跑三次 `xx --version`，每次最多 20s，必须异步。
                Task::perform(
                    async move { crate::core::envres::status(&cfg).await },
                    Message::EnvLoaded,
                )
            }
            Message::EnvLoaded(env) => {
                self.env = Some(env);
                Task::none()
            }
            Message::LoadVersions => {
                self.versions_loading = true;
                self.busy = Some("正在查询可用版本…".into());
                let cfg = self.config.clone();
                let rc = self.include_rc;
                Task::perform(
                    async move {
                        // Node 列表失败不应拖垮 dsh 列表：分别处理。
                        let dsh = crate::core::envres::fetch_dsh_versions(&cfg, rc).await?;
                        let nodes = crate::core::envres::fetch_node_versions(&cfg)
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|n| n.version)
                            .collect();
                        Ok((dsh.versions, dsh.latest, nodes))
                    },
                    Message::VersionsLoaded,
                )
            }
            Message::VersionsLoaded(result) => {
                self.busy = None;
                self.versions_loading = false;
                match result {
                    Ok((versions, latest, nodes)) => {
                        self.dsh_versions = versions;
                        self.dsh_latest = latest;
                        if !nodes.is_empty() && !nodes.contains(&self.node_pick) {
                            self.node_pick = nodes[0].clone();
                        }
                        self.node_versions = nodes;
                    }
                    Err(e) => self.notify(ToastKind::Err, format!("查询版本失败：{e}")),
                }
                Task::none()
            }
            Message::PickDsh(v) => {
                self.dsh_pick = v;
                Task::none()
            }
            Message::PickNode(v) => {
                self.node_pick = v;
                Task::none()
            }
            Message::ToggleRc(v) => {
                self.include_rc = v;
                Task::done(Message::LoadVersions)
            }
            Message::InstallNode => {
                let label = format!("正在下载安装便携版 Node.js {}", self.node_pick);
                self.busy = Some(label.clone());
                self.sys_log("env", label);
                let tx = bridge::sink();
                let cfg = self.config.clone();
                let ver = self.node_pick.clone();
                Task::perform(
                    async move { crate::core::installs::install_node(tx, cfg, ver).await },
                    |r| Message::OpDone("安装 Node.js", r),
                )
            }
            Message::InstallDsh => {
                let label = format!("正在安装 dsh {}", self.dsh_pick);
                self.busy = Some(label.clone());
                self.sys_log("env", label);
                let tx = bridge::sink();
                let cfg = self.config.clone();
                let ver = self.dsh_pick.clone();
                Task::perform(
                    async move { crate::core::installs::install_dsh(tx, cfg, ver).await },
                    |r| Message::OpDone("安装 dsh", r),
                )
            }
            Message::InstallPnpm => {
                self.busy = Some("正在安装 pnpm".into());
                self.sys_log("env", "正在安装 pnpm");
                let tx = bridge::sink();
                let cfg = self.config.clone();
                Task::perform(
                    async move { crate::core::installs::install_pnpm(tx, cfg).await },
                    |r| Message::OpDone("安装 pnpm", r),
                )
            }
            Message::OpDone(what, result) => {
                self.busy = None;
                match result {
                    Ok(()) => {
                        self.notify(ToastKind::Ok, format!("{what}完成"));
                        // 装完立刻重新探测，界面上的版本号才会变。
                        Task::batch([
                            Task::done(Message::RefreshEnv),
                            Task::done(Message::RefreshProfiles),
                        ])
                    }
                    Err(e) => {
                        self.notify(ToastKind::Err, format!("{what}失败：{e}（详情见控制台）"));
                        Task::none()
                    }
                }
            }

            // ---------- profile ----------
            Message::RefreshProfiles => Task::perform(
                // 目录遍历是同步的，扔阻塞线程池免得卡事件循环。
                async {
                    tokio::task::spawn_blocking(crate::core::profiles::list)
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::ProfilesLoaded,
            ),
            Message::ProfilesLoaded(result) => {
                match result {
                    Ok(list) => {
                        // selected 失效时落到第一项（与上一代同逻辑）。
                        if self.selected.is_empty() || !list.iter().any(|p| p.name == self.selected)
                        {
                            self.selected = list.first().map(|p| p.name.clone()).unwrap_or_default();
                        }
                        self.profiles = list;
                    }
                    Err(e) => self.notify(ToastKind::Err, format!("读取版本列表失败：{e}")),
                }
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
            Message::Start(profile) => {
                self.busy = Some(format!("正在启动 {profile}…"));
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
                match result {
                    Ok((profile, url)) => {
                        self.urls.insert(profile.clone(), url.clone());
                        self.notify(ToastKind::Info, format!("正在启动 {profile}…"));
                        let mut tasks = vec![Task::done(Message::PollProcs)];
                        let is_e2e = std::env::args().any(|a| a == "--e2e");
                        if self.config.auto_open && !is_e2e {
                            tasks.push(Task::done(Message::OpenPath(url)));
                        }
                        if is_e2e {
                            tasks.push(Task::perform(
                                async { tokio::time::sleep(Duration::from_secs(8)).await },
                                |_| Message::E2eStop,
                            ));
                        }
                        Task::batch(tasks)
                    }
                    Err(e) => {
                        self.notify(ToastKind::Err, format!("启动失败：{e}"));
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
                        self.urls.remove(&profile);
                        self.notify(ToastKind::Ok, format!("已停止 {profile}"));
                        Task::done(Message::PollProcs)
                    }
                    Err(e) => {
                        self.notify(ToastKind::Err, format!("停止失败：{e}"));
                        Task::none()
                    }
                }
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
                // 替代 tauri-plugin-opener：目录交资源管理器，http 交浏览器。
                if let Err(e) = open::that_detached(&path) {
                    self.notify(ToastKind::Err, format!("打开失败：{e}"));
                }
                Task::none()
            }
            Message::OpenUi(profile) => match self.url_of(&profile) {
                Some(url) => Task::done(Message::OpenPath(url)),
                None => {
                    self.notify(ToastKind::Err, "还没拿到 WebUI 地址，请稍等启动完成");
                    Task::none()
                }
            },

            // ---------- 插件 ----------
            Message::RefreshPlugins => {
                if self.selected.is_empty() {
                    self.plugins.clear();
                    return Task::none();
                }
                let profile = self.selected.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::core::plugins::list(&profile))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::PluginsLoaded,
                )
            }
            Message::PluginsLoaded(result) => {
                match result {
                    Ok(list) => self.plugins = list,
                    Err(e) => {
                        self.plugins.clear();
                        self.notify(ToastKind::Err, e);
                    }
                }
                Task::none()
            }
            Message::LoadMarket => {
                self.busy = Some("正在拉取插件列表…".into());
                let cfg = self.config.clone();
                Task::perform(
                    async move { crate::core::plugins::market(&cfg).await },
                    Message::MarketLoaded,
                )
            }
            Message::MarketLoaded(result) => {
                self.busy = None;
                match result {
                    Ok(items) => {
                        self.market = items;
                        self.market_loaded = true;
                    }
                    Err(e) => self.notify(ToastKind::Err, format!("拉取插件市场失败：{e}")),
                }
                Task::none()
            }
            Message::SetPluginTab(tab) => {
                self.plugin_tab = tab;
                if tab == PluginTab::Market && !self.market_loaded {
                    return Task::done(Message::LoadMarket);
                }
                Task::none()
            }
            Message::Query(q) => {
                self.query = q;
                Task::none()
            }
            Message::InstallPlugin(source) => {
                if self.selected.is_empty() {
                    self.notify(ToastKind::Err, "请先选择一个版本");
                    return Task::none();
                }
                self.busy = Some(format!("正在安装 {source}…"));
                let profile = self.selected.clone();
                self.sys_log(&profile.clone(), format!("开始安装插件 {source}"));
                let tx = bridge::sink();
                let cfg = self.config.clone();
                Task::perform(
                    async move { crate::core::plugins::add(tx, &cfg, &profile, &source).await },
                    |r| Message::OpDone("安装插件", r),
                )
                .chain(Task::done(Message::RefreshPlugins))
            }

            // ---------- 控制台 ----------
            Message::SetLogFilter(f) => {
                self.log_filter = f;
                Task::none()
            }
            Message::ToggleAutoScroll(v) => {
                self.auto_scroll = v;
                Task::none()
            }
            Message::ClearLogs => {
                self.logs.clear();
                Task::none()
            }
            Message::CopyLogs => {
                let text = pages::console::visible_text(self);
                let n = text.lines().count();
                iced::clipboard::write::<Message>(text)
                    .chain(Task::done(Message::Notify(
                        ToastKind::Ok,
                        format!("已复制 {n} 行日志"),
                    )))
            }

            // ---------- 设置 ----------
            Message::CfgApiKey(v) => {
                self.cfg_draft.api_key = v;
                Task::none()
            }
            Message::CfgPort(v) => {
                // 只留数字，允许空串（正在删改）。解析成功才写进 draft。
                self.port_text = v.chars().filter(|c| c.is_ascii_digit()).take(5).collect();
                if let Ok(p) = self.port_text.parse::<u16>() {
                    if p > 0 {
                        self.cfg_draft.port = p;
                    }
                }
                Task::none()
            }
            Message::CfgAutoOpen(v) => {
                self.cfg_draft.auto_open = v;
                Task::none()
            }
            Message::CfgNodeMirror(v) => {
                self.cfg_draft.node_mirror = v;
                Task::none()
            }
            Message::CfgNpmRegistry(v) => {
                self.cfg_draft.npm_registry = v;
                Task::none()
            }
            Message::CfgCatalog(v) => {
                self.cfg_draft.plugin_catalog_url = v;
                Task::none()
            }
            Message::ToggleShowKey => {
                self.show_key = !self.show_key;
                Task::none()
            }
            Message::SaveConfig => {
                self.config = self.cfg_draft.clone();
                let cfg = self.config.clone();
                Task::perform(async move { crate::core::store::save(&cfg) }, |r| {
                    Message::ConfigSaved(r)
                })
            }
            Message::ConfigSaved(result) => {
                match result {
                    Ok(()) => self.notify(ToastKind::Ok, "设置已保存"),
                    Err(e) => self.notify(ToastKind::Err, format!("保存失败：{e}")),
                }
                Task::none()
            }

            Message::E2eStop => {
                let names: Vec<String> = self.procs.iter().map(|p| p.profile.clone()).collect();
                log::info!("e2e: 停止 {names:?}");
                Task::batch(names.into_iter().map(|n| Task::done(Message::Stop(n))))
            }
        }
    }

    /// 起一次切页入场补间。**只在页面真的变了时调**（`Goto` 到当前页是常事：
    /// 侧边栏点两下、Ctrl+N 按重复），否则会闪一下。
    ///
    /// `PAGE` 从 1 跑到 0（1 = 刚切过来）。为什么不是 0→1：`AnimState::value`
    /// 对没有记录的 key 返回 0.0，而 0 正好是落定态，于是冷启动与 `--page`
    /// 出图都不必预置初值，也不会有第一帧的突兀位移。
    ///
    /// `NAV` 同帧起跑：侧边栏选中指示条从上一个位置滑到新位置，靠的是同一个
    /// 补间值（`pages::mod` 里按它插值 y 偏移），不是各自计时——两个动画不同步
    /// 会看出「内容先到、条子后到」。
    fn start_page_anim(&mut self) {
        let now = Instant::now();
        self.anim.restart(anim::PAGE, 1.0, 0.0, anim::PAGE_DUR, now);
        self.anim.restart(anim::NAV, 1.0, 0.0, anim::PAGE_DUR, now);
    }

    /// 模态确认：按 Dialog 变体分派到对应后端调用。
    fn confirm_dialog(&mut self) -> Task<Message> {
        let Some(dialog) = self.dialog.clone() else {
            return Task::none();
        };
        let value = self.draft.trim().to_string();
        // 需要输入的对话框，空值不放行（按钮也是禁用的，这里是双保险）。
        if dialog.needs_input() && value.is_empty() {
            return Task::none();
        }
        self.dialog = None;
        self.draft.clear();
        self.anim.animate_to("modal", 0.0, HOVER_DUR, Instant::now());

        match dialog {
            Dialog::CreateProfile => {
                let v = value.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::core::profiles::create(&v))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| Message::OpDone("新建版本", r),
                )
            }
            Dialog::RenameProfile(from) => {
                let to = value.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::core::profiles::rename(&from, &to)
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| Message::OpDone("重命名版本", r),
                )
            }
            Dialog::CopyProfile(from) => {
                let to = value.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::core::profiles::copy(&from, &to))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| Message::OpDone("复制版本", r),
                )
            }
            Dialog::DeleteProfile(name) => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::core::profiles::delete(&name))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                |r| Message::OpDone("删除版本", r),
            ),
            Dialog::ManualPlugin => Task::done(Message::InstallPlugin(value)),
            Dialog::RemovePlugin(name) => {
                if self.selected.is_empty() {
                    return Task::none();
                }
                self.busy = Some(format!("正在卸载 {name}…"));
                let tx = bridge::sink();
                let cfg = self.config.clone();
                let profile = self.selected.clone();
                Task::perform(
                    async move { crate::core::plugins::remove(tx, &cfg, &profile, &name).await },
                    |r| Message::OpDone("卸载插件", r),
                )
                .chain(Task::done(Message::RefreshPlugins))
            }
            Dialog::RemoveNode => {
                self.busy = Some("正在删除托管 Node.js".into());
                Task::perform(
                    async {
                        tokio::task::spawn_blocking(crate::core::installs::remove_node)
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| Message::OpDone("删除托管 Node.js", r),
                )
            }
            Dialog::RemoveDsh => {
                self.busy = Some("正在卸载 dsh".into());
                Task::perform(
                    async {
                        tokio::task::spawn_blocking(crate::core::installs::remove_dsh)
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| Message::OpDone("卸载 dsh", r),
                )
            }
        }
    }

    fn on_core_event(&mut self, event: CoreEvent) -> Task<Message> {
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
                self.urls.insert(profile.clone(), url.clone());
                self.sys_log(&profile, format!("WebUI 地址：{url}"));
            }
            CoreEvent::Exit { profile, code } => {
                self.sys_log(&profile, format!("进程已退出（退出码 {code}）"));
                self.procs.retain(|p| p.profile != profile);
                self.urls.remove(&profile);
            }
        }
        Task::none()
    }

    /// 空闲零订阅（DESIGN.md §8 第 1 条）：
    /// - `frames()` 只在动画期间挂
    /// - 进程轮询只在有实例在跑时挂
    /// - toast 清理只在有 toast 时挂
    /// - core 事件流是被动的，后端不发就不产消息
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            window::open_events().map(Message::Opened),
            bridge::events().map(Message::Core),
            iced::event::listen_with(|event, _status, _id| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key, modifiers, ..
                }) = event
                else {
                    return None;
                };
                // ESC 关模态（模态是覆盖层，拿不到键盘焦点，只能走全局监听）
                if matches!(
                    key,
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                ) {
                    return Some(Message::CloseDialog);
                }
                // Ctrl+1..6 切页（与上一代一致）
                if modifiers.control() && !modifiers.alt() && !modifiers.shift() {
                    if let iced::keyboard::Key::Character(c) = &key {
                        if let Some(p) = Page::from_digit(c.as_str()) {
                            return Some(Message::Goto(p));
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
            subs.push(iced::time::every(Duration::from_secs(2)).map(|_| Message::PollProcs));
        }
        if !self.toasts.is_empty() {
            // toast 到期检查：500ms 够了，不需要逐帧。
            subs.push(
                iced::time::every(TOAST_TTL / 8).map(|_| Message::ToastTick(Instant::now())),
            );
        }
        Subscription::batch(subs)
    }
}
