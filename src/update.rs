//! `update` / `subscription`：所有状态迁移。与 view 分开是因为这文件本身就长，
//! 混在一起找东西费劲。

use crate::app::{Dshnext, Message, Mode, Onboarding, PickField, PluginTab, RuntimeOp, TOAST_TTL};
use crate::bridge;
use crate::core::event::{CoreEvent, LogStream};
use crate::pages::{self, Page};
use crate::ui::anim::{self, HOVER_DUR};
use crate::ui::modal::Dialog;
use crate::ui::widgets::ToastKind;
use iced::{Subscription, Task, window};
use std::time::{Duration, Instant};

/// 目录表单路由：设置页编辑优先于首启引导（两者不会同开，防御性排序）。
fn dir_form_mut(app: &mut Dshnext) -> Option<&mut Onboarding> {
    app.dirs_edit.as_mut().or(app.onboarding.as_mut())
}
impl Dshnext {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        // 交互链路取证：--e2e / 手动点击时用 RUST_LOG=dshnext=debug 看消息是否到达。
        // Tick 与 ToastTick 是高频的，排除掉免得把日志冲掉。
        if !matches!(
            message,
            Message::Tick(_) | Message::ToastTick(_) | Message::MigrateTick | Message::WindowEvent(_)
        ) {
            log::debug!("msg {message:?}");
        }
        let task = self.update_inner(message);
        // --migrate-go：boot 闭包里发不出 Task（运行时还没接手，返回值被丢），
        // 挂标记在这里补发第一次确认。
        if self.boot_migrate {
            self.boot_migrate = false;
            let go = self.confirm_dirs_edit();
            return Task::batch([task, go]);
        }
        task
    }

    fn update_inner(&mut self, message: Message) -> Task<Message> {
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
                // 托盘要在主线程建（TrayIcon !Send），Opened 正是主线程 update。
                // --minimized 静默自启时强制挂：窗口不可见又没托盘就找不回来了。
                if self.config.tray || self.minimized_start {
                    crate::tray::ensure();
                }
                // 首启引导的淡入（复用 "modal" 补间；同一时间只有一层浮层）。
                if self.onboarding.is_some() {
                    self.anim
                        .animate_to("modal", 1.0, Duration::from_millis(220), Instant::now());
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
                // 双击最大化走 titlebar 的原生 on_double_click（→ ToggleMaximize），
                // 不在这里做计时——window::drag 的模态循环会吞掉第二击。
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
            Message::Close => self.close_or_tray(),
            Message::WindowEvent(window::Event::Opened { position, size }) => {
                self.win_pos = position;
                self.win_size = Some(size);
                self.narrow_layout = size.width < 1150.0;
                // 置顶要等窗口真出来了才能设（main 里设得太早，FindWindowW 落空）。
                if self.config.always_on_top {
                    crate::win32::set_topmost(crate::app::WINDOW_TITLE, true);
                }
                Task::none()
            }
            // 最大化期间不更新几何：保存的必须是还原态尺寸（见 app.win_pos 注释）。
            Message::WindowEvent(window::Event::Moved(p)) => {
                if !self.maximized {
                    self.win_pos = Some(p);
                }
                Task::none()
            }
            Message::WindowEvent(window::Event::Resized(s)) => {
                // 迟滞换档：进窄档要更窄（<1120），出窄档要更宽（>1180），
                // 中间 60px 是抖动缓冲，布局在这个带里不翻转。
                // 最大化时也要换档（宽屏必然是宽档），只是几何别覆盖还原态。
                self.narrow_layout = if self.narrow_layout {
                    s.width < 1180.0
                } else {
                    s.width < 1120.0
                };
                if !self.maximized {
                    self.win_size = Some(s);
                }
                // 拖动期间开逐帧重画（见 resize_pump_until 注释）。
                self.resize_pump_until = Some(Instant::now() + Duration::from_millis(250));
                Task::none()
            }
            // OS 级关闭（Alt+F4 / 任务栏）：exit_on_close_request=false，
            // 关窗权在我们，先存几何；开托盘则隐藏驻留。
            Message::WindowEvent(window::Event::CloseRequested) => self.close_or_tray(),
            Message::WindowEvent(_) => Task::none(),
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
                // 进环境页时拉版本列表，只拉一次（在途请求也算已拉）；
                // 顺带扫离线包目录（便宜：一次 read_dir）。
                if page == Page::Env {
                    let mut t = vec![Task::done(Message::ScanOffline)];
                    if self.dsh_versions.is_empty() && !self.versions_loading {
                        t.push(Task::done(Message::LoadVersions));
                    }
                    return Task::batch(t);
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
            Message::SetTheme(t) => match t {
                // "system" 要查注册表（reg.exe 子进程几十 ms），别在 update 主线程
                // 同步等——丢到阻塞线程池，结果回来再落模式。
                "system" => Task::perform(
                    async {
                        tokio::task::spawn_blocking(crate::core::platform::system_prefers_light)
                            .await
                            .unwrap_or(false)
                    },
                    Message::SystemThemeResolved,
                ),
                t => {
                    self.mode = match t {
                        "light" => Mode::Light,
                        _ => Mode::Dark,
                    };
                    self.config.theme = t.into();
                    self.cfg_draft.theme = t.into();
                    let cfg = self.config.clone();
                    Task::perform(async move { crate::core::store::save(&cfg) }, |r| {
                        Message::ConfigSaved(r)
                    })
                }
            },
            Message::SystemThemeResolved(light) => {
                // mode 落探测结果，落盘值仍是 "system"（下次启动继续跟随）。
                self.mode = if light { Mode::Light } else { Mode::Dark };
                self.config.theme = "system".into();
                self.cfg_draft.theme = "system".into();
                let cfg = self.config.clone();
                Task::perform(async move { crate::core::store::save(&cfg) }, |r| {
                    Message::ConfigSaved(r)
                })
            }
            // 状态栏图钉：立即生效、立即落盘（不走设置页草稿——那是延迟保存的语义）。
            Message::ToggleTopmost => {
                self.config.always_on_top = !self.config.always_on_top;
                crate::win32::set_topmost(crate::app::WINDOW_TITLE, self.config.always_on_top);
                self.cfg_draft.always_on_top = self.config.always_on_top;
                if let Err(e) = crate::core::store::save(&self.config) {
                    log::warn!("保存置顶开关失败: {e}");
                }
                Task::none()
            }
            Message::CfgAutoStart(v) => {
                self.cfg_draft.autostart = v;
                Task::none()
            }
            Message::CfgTray(v) => {
                self.cfg_draft.tray = v;
                Task::none()
            }
            Message::CfgUpdateUrl(v) => {
                self.cfg_draft.update_url = v;
                Task::none()
            }
            Message::CfgAutoRestart(v) => {
                self.cfg_draft.auto_restart = v;
                Task::none()
            }
            Message::CfgCloseStops(v) => {
                self.cfg_draft.close_stops = v;
                Task::none()
            }
            Message::CheckUpdate => {
                let url = self.config.effective_update_url().to_string();
                Task::perform(crate::core::selfupdate::latest(url), Message::UpdateChecked)
            }
            Message::UpdateChecked(result) => match result {
                Ok(info) => {
                    let newer = crate::core::selfupdate::newer(&info.version, env!("CARGO_PKG_VERSION"));
                    let webui_missing = crate::core::selfupdate::webui_sibling()
                        .map_or(true, |p| !p.exists());
                    // 版本更新要下；版本已最新但宿主 exe 缺失、源里又有 → 补下载
                    // （0.1.10 的旧启动器只换 dshnext.exe，重启后靠这条补齐）。
                    if newer || (webui_missing && info.webui_url.is_some()) {
                        self.busy = Some(format!("正在下载更新 v{}…", info.version));
                        self.update_busy = true;
                        Task::perform(
                            crate::core::selfupdate::update_step(info),
                            Message::UpdateReady,
                        )
                    } else {
                        self.notify(
                            ToastKind::Ok,
                            format!("已是最新版本 v{}", env!("CARGO_PKG_VERSION")),
                        );
                        Task::none()
                    }
                }
                Err(e) => {
                    self.notify(ToastKind::Err, format!("检查更新失败：{e}"));
                    Task::none()
                }
            },
            Message::UpdateReady(result) => {
                self.busy = None;
                self.update_busy = false;
                match result {
                    Ok((version, main, webui)) => {
                        // 换身是本地文件操作，同步做掉（快），成败都明确告知。
                        match crate::core::selfupdate::apply_swap(main.as_deref(), webui.as_deref())
                        {
                            Ok(()) if main.is_some() => self.notify(
                                ToastKind::Ok,
                                format!("已更新到 v{version}，重启启动器后生效"),
                            ),
                            Ok(()) => self.notify(
                                ToastKind::Ok,
                                "桌面窗口程序已补齐，无需重启",
                            ),
                            Err(e) => self.notify(ToastKind::Err, format!("更新落位失败：{e}")),
                        }
                        Task::none()
                    }
                    Err(e) => {
                        self.notify(ToastKind::Err, format!("更新失败：{e}"));
                        Task::none()
                    }
                }
            }
            Message::UpdateTick => Task::none(),
            Message::Tray(crate::tray::TrayEvent::Show) => {
                crate::tray::show_window();
                Task::none()
            }
            Message::Tray(crate::tray::TrayEvent::Exit) => self.close_window(),
            Message::OpenDialog(d) => {
                self.draft = d.initial();
                self.dialog = Some(d);
                // 模态淡入 220ms（motion-designer：modal open 200–300 ease-out，
                // 比 hover 慢才有「浮层」的重量感）。
                self.anim
                    .animate_to("modal", 1.0, Duration::from_millis(220), Instant::now());
                Task::none()
            }
            Message::CloseDialog => {
                self.dialog = None;
                self.draft.clear();
                // 「停止并继续」被取消：清掉待续跑操作，别让下次轮询意外触发。
                self.resume_runtime_op = None;
                // 关闭比打开快（150ms vs 220ms）——退场拖沓会让界面显得粘手。
                self.anim
                    .animate_to("modal", 0.0, Duration::from_millis(150), Instant::now());
                Task::none()
            }
            Message::DialogInput(v) => {
                self.draft = v;
                Task::none()
            }
            Message::DialogConfirm => self.confirm_dialog(),

            // ---------- 目录表单（首启引导 / 设置页修改共用） ----------
            Message::ObEdit(f, v) => {
                if let Some(ob) = dir_form_mut(self) {
                    match f {
                        PickField::LauncherDir => {
                            ob.launcher_dir = v;
                            ob.home_hint = Onboarding::home_hint_for(&ob.launcher_dir);
                        }
                        PickField::DshHome => ob.dsh_home = v,
                    }
                }
                Task::none()
            }
            Message::ObBrowse(f) => {
                if let Some(ob) = dir_form_mut(self) {
                    ob.picking = Some(f);
                }
                // rfd 的 IFileDialog 在阻塞线程上跑（COM 初始化由它自己管），
                // 不堵 iced 的主线程。
                Task::perform(
                    tokio::task::spawn_blocking(move || {
                        rfd::FileDialog::new()
                            .set_title("选择目录")
                            .pick_folder()
                            .map(|p| p.to_string_lossy().into_owned())
                    }),
                    move |res| Message::ObPicked(f, res.unwrap_or(None)),
                )
            }
            Message::ObPicked(f, path) => {
                if let Some(ob) = dir_form_mut(self) {
                    ob.picking = None;
                    if let Some(p) = path {
                        match f {
                            PickField::LauncherDir => {
                                ob.launcher_dir = p;
                                ob.home_hint = Onboarding::home_hint_for(&ob.launcher_dir);
                            }
                            PickField::DshHome => ob.dsh_home = p,
                        }
                    }
                }
                Task::none()
            }
            Message::ObDefaults => {
                if let Some(ob) = dir_form_mut(self) {
                    ob.launcher_dir.clear();
                    ob.dsh_home.clear();
                    ob.home_hint = Onboarding::home_hint_for("");
                }
                Task::none()
            }
            Message::ObConfirm => {
                if self.dirs_edit.is_some() {
                    self.confirm_dirs_edit()
                } else {
                    self.confirm_onboarding()
                }
            }
            Message::MigrateDone(result) => self.migrate_done(result),
            Message::MigrateTick => {
                self.migrate_prog = crate::core::migrate::progress();
                Task::none()
            }
            Message::OpenDirsEdit => {
                // 与首启引导互斥（正常情况引导早关了；防御一下）。
                if self.onboarding.is_none() {
                    self.dirs_edit = Some(Onboarding::for_edit(&self.config.dsh_home));
                    crate::app::DIRS_EDIT_OPEN.store(true, std::sync::atomic::Ordering::Relaxed);
                    self.anim
                        .animate_to("modal", 1.0, Duration::from_millis(220), Instant::now());
                }
                Task::none()
            }
            Message::ObClose => {
                if self.dirs_edit.take().is_some() {
                    crate::app::DIRS_EDIT_OPEN.store(false, std::sync::atomic::Ordering::Relaxed);
                    self.anim
                        .animate_to("modal", 0.0, Duration::from_millis(150), Instant::now());
                }
                Task::none()
            }
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
                // 顺带扫离线包目录（--page env 不走 Goto，这里兜底）。
                Task::perform(
                    async move { crate::core::envres::status(&cfg).await },
                    Message::EnvLoaded,
                )
                .chain(Task::done(Message::ScanOffline))
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
                if let Some(t) = self.require_stopped(RuntimeOp::InstallNode) {
                    return t;
                }
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
                if let Some(t) = self.require_stopped(RuntimeOp::InstallDsh) {
                    return t;
                }
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
            Message::InstallNodeOffline(zip) => {
                if let Some(t) = self.require_stopped(RuntimeOp::InstallNodeOffline(zip.clone())) {
                    return t;
                }
                self.busy = Some("正在离线安装便携版 Node.js".into());
                self.sys_log("env", format!("离线安装 Node：{}", zip.display()));
                let tx = bridge::sink();
                Task::perform(
                    async move { crate::core::installs::install_node_offline(tx, zip).await },
                    |r| Message::OpDone("离线安装 Node.js", r),
                )
            }
            Message::InstallDshOffline(tgz) => {
                if let Some(t) = self.require_stopped(RuntimeOp::InstallDshOffline(tgz.clone())) {
                    return t;
                }
                self.busy = Some("正在离线安装 dsh".into());
                self.sys_log("env", format!("离线安装 dsh：{}", tgz.display()));
                let tx = bridge::sink();
                let cfg = self.config.clone();
                Task::perform(
                    async move { crate::core::installs::install_dsh_offline(tx, cfg, tgz).await },
                    |r| Message::OpDone("离线安装 dsh", r),
                )
            }
            Message::InstallPnpmOffline(tgz) => {
                if let Some(t) = self.require_stopped(RuntimeOp::InstallPnpmOffline(tgz.clone())) {
                    return t;
                }
                self.busy = Some("正在离线安装 pnpm".into());
                self.sys_log("env", format!("离线安装 pnpm：{}", tgz.display()));
                let tx = bridge::sink();
                let cfg = self.config.clone();
                Task::perform(
                    async move { crate::core::installs::install_pnpm_offline(tx, cfg, tgz).await },
                    |r| Message::OpDone("离线安装 pnpm", r),
                )
            }
            Message::ScanOffline => {
                Task::perform(
                    async {
                        tokio::task::spawn_blocking(crate::core::installs::scan_offline)
                            .await
                            .unwrap_or_default()
                    },
                    Message::OfflineScanned,
                )
            }
            Message::OfflineScanned(packs) => {
                self.offline = Some(packs);
                Task::none()
            }
            Message::InstallPnpm => {
                if let Some(t) = self.require_stopped(RuntimeOp::InstallPnpm) {
                    return t;
                }
                self.busy = Some("正在安装 pnpm".into());
                self.sys_log("env", "正在安装 pnpm");
                let tx = bridge::sink();
                let cfg = self.config.clone();
                Task::perform(
                    async move { crate::core::installs::install_pnpm(tx, cfg).await },
                    |r| Message::OpDone("安装 pnpm", r),
                )
            }
            Message::RemoveDshNow => {
                // 守卫放这里而不是 Dialog 确认臂：两条入口（用户确认卸载、
                // 「停止并继续」续跑）都汇到这条消息，守卫天然复用。
                if let Some(t) = self.require_stopped(RuntimeOp::RemoveDsh) {
                    return t;
                }
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
            Message::RemoveNodeNow => {
                if let Some(t) = self.require_stopped(RuntimeOp::RemoveNode) {
                    return t;
                }
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
            Message::OpDone(what, result) => {
                self.busy = None;
                match result {
                    Ok(()) => {
                        self.notify(ToastKind::Ok, format!("{what}完成"));
                        // 装完立刻重新探测，界面上的版本号才会变。
                        Task::batch([
                            Task::done(Message::RefreshEnv),
                            Task::done(Message::RefreshProfiles),
                            Task::done(Message::ScanOffline),
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
                // 用户手动启动 = 接管了局面，之前挂在「停止并继续」上的
                // 待续跑操作作废（否则下次实例清空时会意外触发）。
                self.resume_runtime_op = None;
                // 先探测端口再 spawn：活动 web 服务可复用、半死进程提前报错，
                // 别等 dsh 绑定失败再翻日志（对齐 WEP-56 的复用策略）。
                let port = self.config.port;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::core::platform::probe_port(port)
                        })
                        .await
                        .unwrap_or(crate::core::platform::PortProbe::Other)
                    },
                    move |probe| Message::StartProbed(profile, probe),
                )
            }
            Message::StartProbed(profile, probe) => {
                let port = self.config.port;
                // 端口上的服务是不是我们自己跑的实例（按 URL 里的端口认）。
                let own = self
                    .procs
                    .iter()
                    .any(|p| p.url.contains(&format!(":{port}")));
                // 这次 Start 是自动重启在途？端口被崩掉的实例残留占用时别误开
                // 「现有服务」，改走退避重试（计数并入崩溃序列，连 3 次封顶）。
                let via_restart = self.restarting.contains(&profile);
                match probe {
                    crate::core::platform::PortProbe::Free => {
                        // 接管成功（或本来就干净）：清掉一次性标记，别把名字留到
                        // 下回真冲突时误判成「接管后仍占用」。
                        self.takeover.remove(&profile);
                        self.busy = Some(format!("正在启动 {profile}…"));
                        let tx = bridge::sink();
                        let procs = bridge::procs();
                        let cfg = self.config.clone();
                        Task::perform(
                            async move {
                                crate::core::procman::start(tx, &procs, &cfg, &profile, port)
                                    .await
                                    .map(|url| (profile, url))
                            },
                            Message::Started,
                        )
                    }
                    crate::core::platform::PortProbe::Http if !own && !via_restart => {
                        // 端口被残留的 dsh（上次没停干净的 node 树）占着：token 只在
                        // 它启动时的 stdout 里，拿不到就没法开它的界面。与其让用户去
                        // 任务管理器杀 node，直接结束端口属主进程再走一遍启动。
                        // 只接管一次（takeover 集合防循环），第二次仍占用才回落到提示。
                        if self.takeover.contains(&profile) {
                            self.takeover.remove(&profile);
                            self.notify(
                                ToastKind::Warn,
                                format!("端口 {port} 接管后仍被占用，请手动结束占用进程或改端口"),
                            );
                            return Task::none();
                        }
                        self.takeover.insert(profile.clone());
                        self.notify(
                            ToastKind::Info,
                            format!("端口 {port} 被残留实例占用，正在结束旧进程并接管…"),
                        );
                        let p = profile.clone();
                        Task::perform(
                            async move {
                                let _ = tokio::task::spawn_blocking(move || {
                                    crate::core::platform::kill_port_owner(port)
                                })
                                .await;
                                tokio::time::sleep(Duration::from_millis(800)).await;
                            },
                            move |_| Message::Start(p),
                        )
                    }
                    crate::core::platform::PortProbe::Http if via_restart => {
                        // 崩掉的实例残留子进程还占着口：退避重试，计数并入序列。
                        self.restarting.insert(profile.clone());
                        self.notify(
                            ToastKind::Info,
                            format!("端口 {port} 尚未释放，5s 后重试自动重启"),
                        );
                        let e = self
                            .crash_restarts
                            .entry(profile.clone())
                            .or_insert((0, Instant::now()));
                        let (count, _) = *e;
                        *e = (count + 1, Instant::now());
                        if count + 1 >= 3 {
                            self.restarting.remove(&profile);
                            self.notify(ToastKind::Err, format!("{profile} 端口持续未释放，停止自动重启"));
                            Task::none()
                        } else {
                            Task::perform(
                                async move { tokio::time::sleep(Duration::from_secs(5)).await },
                                move |_| Message::Start(profile),
                            )
                        }
                    }
                    _ => {
                        self.restarting.remove(&profile);
                        let msg = if own {
                            format!("端口 {port} 已被正在运行的实例占用，改端口或先停止它")
                        } else {
                            format!("端口 {port} 被其他程序占用且不是 Web 服务，改端口后再启动")
                        };
                        self.notify(ToastKind::Err, msg);
                        Task::none()
                    }
                }
            }
            Message::Started(result) => {
                self.busy = None;
                match result {
                    Ok((profile, url)) => {
                        self.crash_restarts.remove(&profile);
                        // 启动耗时锚点：Started（进程已 spawn）→ Url（WebUI 就绪）。
                        self.boot_at.insert(profile.clone(), Instant::now());
                        self.notify(ToastKind::Info, format!("正在启动 {profile}…"));
                        let mut tasks = vec![Task::done(Message::PollProcs)];
                        let is_e2e = std::env::args().any(|a| a == "--e2e");
                        if self.config.auto_open && !is_e2e {
                            // start() 返回的是裸地址（无 token，开出来 404）；
                            // 等 Url 事件带 token 的地址来了再开（pending_open）。
                            self.pending_open.insert(
                                profile.clone(),
                                Instant::now() + Duration::from_secs(30),
                            );
                        }
                        let _ = url; // 裸地址不能用来打开（404），展示走 procs 的 url 字段
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
                self.stopping.insert(profile.clone());
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
                    }
                    Err(e) => {
                        self.notify(ToastKind::Err, format!("停止失败：{e}"));
                    }
                }
                // 退出收尾：等最后一个 Stopped（成功或失败都算）落定再真关窗，
                // 否则退出会卡在「还在停」的状态。
                if self.exit_after_stop && self.procs.is_empty() {
                    self.exit_after_stop = false;
                    return self.close_window();
                }
                Task::done(Message::PollProcs)
            }
            Message::StopAll => {
                let names: Vec<String> = self.procs.iter().map(|p| p.profile.clone()).collect();
                if names.is_empty() {
                    return Task::none();
                }
                Task::batch(names.into_iter().map(|n| Task::done(Message::Stop(n))))
            }
            Message::WebviewClosed => {
                // 桌面窗口关了：按配置顺带停服务（不停则 dsh 继续跑，重开窗口复用）。
                if self.config.close_stops && !self.procs.is_empty() {
                    Task::done(Message::StopAll)
                } else {
                    Task::none()
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
                // 「停止并继续」的续跑点：实例全停干净了，取回暂存的操作重投。
                // 有残留实例（停止失败/又开了一个）就继续等下一次轮询。
                if let Some(op) = self.resume_runtime_op.clone() {
                    if self.procs.is_empty() {
                        self.resume_runtime_op = None;
                        self.notify(ToastKind::Info, format!("实例已停止，继续{}", op.label()));
                        return Task::done(op.message());
                    }
                }
                // 轮询只在有实例时挂——正好用来清过期的自动打开等待（30s）。
                let now = Instant::now();
                let expired: Vec<String> = self
                    .pending_open
                    .iter()
                    .filter(|(_, d)| **d <= now)
                    .map(|(p, _)| p.clone())
                    .collect();
                for profile in &expired {
                    self.pending_open.remove(profile);
                    self.notify(
                        ToastKind::Warn,
                        format!("{profile}：等了 30 秒也没拿到 WebUI 地址，自动打开已取消"),
                    );
                }
                Task::none()
            }
            Message::OpenPath(path) => {
                // 替代 tauri-plugin-opener：目录交资源管理器，http 交浏览器。
                if let Err(e) = open::that_detached(&path) {
                    self.notify(ToastKind::Err, format!("打开失败：{e}"));
                }
                Task::none()
            }
            Message::OpenWebUi(url) => {
                // 带 token 的 WebUI 地址按 app_window 分流：桌面窗口模式走
                // WebView2 独立窗口（core::webview），否则系统浏览器标签页。
                if self.config.app_window {
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || crate::core::webview::open(url))
                                .await
                                .unwrap_or_else(|e| Err(e.to_string()))
                        },
                        |r: Result<(), String>| match r {
                            Ok(()) => Message::Noop,
                            Err(e) => Message::Notify(ToastKind::Err, e),
                        },
                    )
                } else {
                    if let Err(e) = open::that_detached(&url) {
                        self.notify(ToastKind::Err, format!("打开失败：{e}"));
                    }
                    Task::none()
                }
            }
            Message::AppWindowSaved(Ok(())) => Task::none(),
            Message::AppWindowSaved(Err(e)) => {
                self.notify(ToastKind::Err, format!("保存打开方式失败：{e}"));
                Task::none()
            }
            Message::OpenUi(profile) => match self.url_of(&profile) {
                Some(url) => Task::done(Message::OpenWebUi(url)),
                None if self.running(&profile).is_some() => {
                    // 实例在跑但带 token 的地址还没从 stdout 里解析出来：
                    // 挂起等待，地址一到自动开（绝不拿裸地址开 404）。
                    self.pending_open.insert(
                        profile.clone(),
                        Instant::now() + Duration::from_secs(30),
                    );
                    self.notify(ToastKind::Info, "WebUI 地址就绪后自动打开…");
                    Task::none()
                }
                None => {
                    self.notify(ToastKind::Err, "还没拿到 WebUI 地址，请稍等启动完成");
                    Task::none()
                }
            },
            Message::SetAppWindow(on) => {
                // 启动页「打开方式」：切换即生效。config 与 draft 必须同写，
                // 否则设置页「保存」会拿旧 draft 把开关悄悄打回去。
                self.config.app_window = on;
                self.cfg_draft.app_window = on;
                let cfg = self.config.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::core::store::save(&cfg))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r: Result<(), String>| match r {
                        Ok(()) => Message::AppWindowSaved(Ok(())),
                        Err(e) => Message::AppWindowSaved(Err(e)),
                    },
                )
            }

            // ---------- 插件 ----------
            Message::RefreshPlugins => {
                self.problems.clear();
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
                        self.market_page = 0;
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
            Message::SetMarketSort(sort) => {
                self.market_sort = sort;
                self.market_page = 0;
                Task::none()
            }
            Message::SetMarketPage(page) => {
                self.market_page = page;
                Task::none()
            }
            Message::TogglePlugin(name, disabled) => {
                if self.selected.is_empty() {
                    return Task::none();
                }
                self.busy = Some(format!(
                    "正在{} {name}…",
                    if disabled { "禁用" } else { "启用" }
                ));
                let profile = self.selected.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::core::patch::set_disabled(&profile, &name, disabled)
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    |r| Message::OpDone("切换插件", r),
                )
                .chain(Task::done(Message::RefreshPlugins))
            }
            Message::DiagnosePlugins => {
                if self.selected.is_empty() {
                    self.notify(ToastKind::Err, "请先选择一个版本");
                    return Task::none();
                }
                self.busy = Some("正在诊断插件…".into());
                let profile = self.selected.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || crate::core::plugins::diagnose(&profile))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::PluginsDiagnosed,
                )
            }
            Message::PluginsDiagnosed(result) => {
                self.busy = None;
                match result {
                    Ok(problems) => {
                        let errors = problems.iter().filter(|p| p.severity == "error").count();
                        self.problems = problems;
                        if self.problems.is_empty() {
                            self.notify(ToastKind::Ok, "未发现会导致启动失败的插件");
                        } else {
                            self.notify(
                                ToastKind::Warn,
                                format!("发现 {} 处问题（{} 处会导致启动失败）", self.problems.len(), errors),
                            );
                        }
                    }
                    Err(e) => {
                        self.problems.clear();
                        self.notify(ToastKind::Err, format!("诊断失败：{e}"));
                    }
                }
                Task::none()
            }
            Message::Query(q) => {
                self.query = q;
                self.market_page = 0;
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
            Message::ToggleErrorsOnly(v) => {
                self.errors_only = v;
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
            Message::CopyWebUrl(url) => iced::clipboard::write::<Message>(url)
                .chain(Task::done(Message::Notify(
                    ToastKind::Ok,
                    "已复制 WebUI 地址（含 token）".to_string(),
                ))),

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
            Message::ExportDiag => {
                // 日志尾部在这里预格式化（LogLine 属于 app 层，diag 保持不依赖它）。
                let log_tail: Vec<String> = self
                    .logs
                    .iter()
                    .rev()
                    .take(200)
                    .rev()
                    .map(|l| {
                        let t = l.ts / 1000;
                        format!(
                            "{:02?}:{:02?}:{:02?} [{:?}] {}",
                            (t / 3600) % 24,
                            (t / 60) % 60,
                            t % 60,
                            l.stream,
                            l.line
                        )
                    })
                    .collect();
                let text = crate::core::diag::render(
                    &self.config,
                    self.env.as_ref(),
                    &self.profiles,
                    &self.procs,
                    &log_tail,
                    env!("CARGO_PKG_VERSION"),
                    &crate::core::platform::os_pretty(),
                );
                let path = crate::core::diag::target_path();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            std::fs::write(&path, &text)
                                .map(|_| path.clone())
                                .map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    Message::DiagExported,
                )
            }
            Message::DiagExported(result) => match result {
                Ok(path) => {
                    self.notify(
                        ToastKind::Ok,
                        format!("诊断已导出：{}", path.display()),
                    );
                    let dir = path.parent().map(|d| d.to_path_buf());
                    dir.map(|d| Task::done(Message::OpenPath(d.display().to_string())))
                        .unwrap_or_else(Task::none)
                }
                Err(e) => {
                    self.notify(ToastKind::Err, format!("诊断导出失败：{e}"));
                    Task::none()
                }
            },
            Message::ToggleShowKey => {
                self.show_key = !self.show_key;
                Task::none()
            }
            Message::SaveConfig => {
                self.config = self.cfg_draft.clone();
                let cfg = self.config.clone();
                // 自启注册表随保存一起应用（幂等）：开关状态以注册表为准，
                // config 只存意图。
                if cfg.tray {
                    crate::tray::ensure();
                } else {
                    crate::tray::remove();
                }
                Task::perform(
                    async move {
                        crate::core::store::save(&cfg)?;
                        crate::core::platform::set_autostart(cfg.autostart)?;
                        Ok(())
                    },
                    |r: Result<(), String>| Message::ConfigSaved(r),
                )
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
        // 确认也是关闭：走快速退场（同 CloseDialog）。
        self.anim
            .animate_to("modal", 0.0, Duration::from_millis(150), Instant::now());

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
            Dialog::RemoveNode => Task::done(Message::RemoveNodeNow),
            Dialog::RemoveDsh => Task::done(Message::RemoveDshNow),
            Dialog::StopAndContinue(_) => {
                // 「停止并继续」：停掉全部实例，ProcsLoaded 清空时续跑
                // resume_runtime_op。取消路径走 CloseDialog（resume 残留
                // 由 CloseDialog/手动启动清掉）。
                let names: Vec<String> = self.procs.iter().map(|p| p.profile.clone()).collect();
                if names.is_empty() {
                    // 弹框到确认之间实例自己退了：直接续跑。
                    self.resume_runtime_op
                        .take()
                        .map(|op| Task::done(op.message()))
                        .unwrap_or_else(Task::none)
                } else {
                    Task::batch(
                        names
                            .iter()
                            .map(|n| Task::done(Message::Stop(n.clone())))
                            .chain(std::iter::once(Task::done(Message::PollProcs))),
                    )
                }
            }
        }
    }

    /// 首次启动引导「开始使用」：建目录 → 改道 → pointer → 落盘 → 重新探测。
    /// 失败时引导留着（toast 报错），用户改路径重试。
    fn confirm_onboarding(&mut self) -> Task<Message> {
        let Some(ob) = &self.onboarding else {
            return Task::none();
        };
        let launcher = if ob.launcher_dir.trim().is_empty() {
            crate::core::store::boot_dir()
        } else {
            std::path::PathBuf::from(ob.launcher_dir.trim())
        };
        // 默认 dsh-home 跟着（可能已改道的）启动器目录走：<launcher>/home。
        let custom_home = !ob.dsh_home.trim().is_empty();
        let home = if custom_home {
            std::path::PathBuf::from(ob.dsh_home.trim())
        } else {
            launcher.join("home")
        };

        let applied = (|| -> Result<(), String> {
            // 手输的可能是相对路径：先转绝对（不碰 \\?\ verbatim 前缀，
            // 那个在环境页/配置里看着像乱码）。
            let launcher = std::path::absolute(&launcher).unwrap_or(launcher);
            let home = std::path::absolute(&home).unwrap_or(home);
            std::fs::create_dir_all(&launcher)
                .map_err(|e| format!("创建启动器目录失败：{e}"))?;
            std::fs::create_dir_all(&home).map_err(|e| format!("创建 dsh-home 失败：{e}"))?;
            crate::core::store::redirect_data_dir(launcher.clone());
            crate::core::envres::set_home_dir(home.clone());
            crate::core::store::write_pointer(&launcher)?;
            let mut cfg = self.config.clone();
            // 默认存空串——将来数据目录再搬家，空串仍解析到新的默认位置；
            // 自定义路径存绝对路径，搬家不受影响。
            cfg.dsh_home = if custom_home {
                home.to_string_lossy().into_owned()
            } else {
                String::new()
            };
            crate::core::store::save(&cfg)?;
            self.config = cfg.clone();
            self.cfg_draft = cfg;
            Ok(())
        })();

        match applied {
            Ok(()) => {
                self.onboarding = None;
                self.notify(ToastKind::Ok, "目录已就绪，可以开始使用了");
                Task::batch(vec![
                    Task::done(Message::RefreshEnv),
                    Task::done(Message::RefreshProfiles),
                    Task::done(Message::ScanOffline),
                ])
            }
            Err(e) => {
                self.notify(ToastKind::Err, e);
                Task::none()
            }
        }
    }

    /// 设置页「修改目录」确认：先守门（实例在跑/有别的忙活不行），然后
    /// 阻塞线程里跑 `core::migrate::relocate`（搬文件 + pointer + redirect +
    /// config 落盘），OpDone(Ok) 负责收尾（关表单 + 重探环境）。
    /// 改动托管 runtime 的操作（装/卸 dsh、Node、pnpm）都会覆盖或删除正被
    /// harness 进程使用的文件——node 惰性加载模块树，装到一半实例会吃到
    /// 半新半旧的模块；删运行中的 node.exe 更会半途失败留残局。**不直接
    /// 拒绝**：弹「停止并继续」确认框，用户同意则先停全部实例再续跑
    /// （`resume_runtime_op` 暂存，ProcsLoaded 清空时取回）。返回
    /// Some(task) = 已弹框/已拦下，调用方直接 return。
    fn require_stopped(&mut self, op: crate::app::RuntimeOp) -> Option<Task<Message>> {
        if self.procs.is_empty() {
            return None;
        }
        self.resume_runtime_op = Some(op.clone());
        self.draft.clear();
        self.dialog = Some(Dialog::StopAndContinue(op.label().into()));
        self.anim
            .animate_to("modal", 1.0, Duration::from_millis(220), Instant::now());
        Some(Task::none())
    }

    fn confirm_dirs_edit(&mut self) -> Task<Message> {
        let Some(ob) = &self.dirs_edit else {
            return Task::none();
        };
        // 进程的工作目录 / DSH_HOME 指着旧位置，跑着的时候搬不动也不该搬。
        if !self.procs.is_empty() {
            self.notify(ToastKind::Warn, "先停止所有运行中的实例，再修改目录");
            return Task::none();
        }
        if self.busy.is_some() {
            return Task::none();
        }

        let launcher = if ob.launcher_dir.trim().is_empty() {
            crate::core::store::boot_dir()
        } else {
            std::path::PathBuf::from(ob.launcher_dir.trim())
        };
        let custom_home = !ob.dsh_home.trim().is_empty();
        let home = if custom_home {
            std::path::PathBuf::from(ob.dsh_home.trim())
        } else {
            launcher.join("home")
        };
        let launcher = std::path::absolute(&launcher).unwrap_or(launcher);
        let home = std::path::absolute(&home).unwrap_or(home);

        let from = crate::core::store::data_dir();
        self.sys_log(
            "env",
            format!(
                "目录转移：{} → {}（dsh-home → {}）",
                from.display(),
                launcher.display(),
                home.display()
            ),
        );
        self.busy = Some("正在转移目录…".into());
        self.migrating = true;
        self.migrate_prog = (0, 0);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    crate::core::migrate::relocate(launcher, home, custom_home)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            },
            |r| Message::MigrateDone(r),
        )
    }

    /// 目录转移收尾。Ok 的参数是删除阶段没删掉的旧文件（数据已双份，
    /// 非致命）——toast 概括、全文进控制台页。
    fn migrate_done(&mut self, result: Result<Vec<String>, String>) -> Task<Message> {
        self.busy = None;
        self.migrating = false;
        self.migrate_prog = (0, 0);
        if self.dirs_edit.take().is_some() {
            crate::app::DIRS_EDIT_OPEN.store(false, std::sync::atomic::Ordering::Relaxed);
            self.anim
                .animate_to("modal", 0.0, Duration::from_millis(150), Instant::now());
        }
        match result {
            Ok(w) if w.is_empty() => {
                self.notify(ToastKind::Ok, "目录转移完成");
            }
            Ok(w) => {
                for line in &w {
                    self.sys_log("env", format!("旧位置残留：{line}"));
                }
                self.notify(
                    ToastKind::Warn,
                    format!("目录转移完成，{} 项留在旧位置（详情见控制台）", w.len()),
                );
            }
            Err(e) => {
                // 失败原因必须可追溯：toast 只活 4 秒，全文进控制台页。
                self.sys_log("env", format!("目录转移失败：{e}"));
                self.notify(ToastKind::Err, format!("目录转移失败：{e}"));
            }
        }
        // 成功失败都要重探：成功是路径全变了，失败也可能是改道后的半程状态。
        Task::batch([
            Task::done(Message::RefreshEnv),
            Task::done(Message::RefreshProfiles),
            Task::done(Message::ScanOffline),
        ])
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
                // 落盘最近地址：桌面窗口（DeepseekHarness.exe）双击独立打开靠它。
                crate::core::store::save_webui_url(&url);
                self.sys_log(&profile, format!("WebUI 地址：{url}"));
                let mut tasks: Vec<Task<Message>> = Vec::new();
                // 启动耗时落表：Started→Url 的毫秒差写进 config，首页 meta 上墙。
                // 静默保存（失败不值得打扰用户）。
                if let Some(t0) = self.boot_at.remove(&profile) {
                    self.config.last_boot_ms = Some(t0.elapsed().as_millis() as u64);
                    let cfg = self.config.clone();
                    tasks.push(Task::perform(
                        async move {
                            let _ = tokio::task::spawn_blocking(move || {
                                crate::core::store::save(&cfg)
                            })
                            .await;
                        },
                        |_| Message::Noop,
                    ));
                }
                // 带 token 的地址到了，兑现等着的自动打开。
                if self.pending_open.remove(&profile).is_some() {
                    tasks.push(Task::done(Message::OpenWebUi(url)));
                }
                return Task::batch(tasks);
            }
            CoreEvent::Exit { profile, code } => {
                self.sys_log(&profile, format!("进程已退出（退出码 {code}）"));
                // 稳定跑了 2 分钟以上的算「新的崩溃序列」——重启计数清零。
                let uptime = self
                    .procs
                    .iter()
                    .find(|p| p.profile == profile)
                    .map(|p| p.uptime_secs)
                    .unwrap_or(0);
                if uptime > 120 {
                    self.crash_restarts.remove(&profile);
                }
                self.procs.retain(|p| p.profile != profile);
                self.urls.remove(&profile);
                if self.stopping.remove(&profile) {
                    // 主动停止，走完收尾。
                } else if code != 0 && self.config.auto_restart {
                    // 崩溃自愈：指数退避 2^n 秒，连 3 次封顶。
                    const MAX: u32 = 3;
                    let e = self
                        .crash_restarts
                        .entry(profile.clone())
                        .or_insert((0, Instant::now()));
                    let (count, _) = *e;
                    if count >= MAX {
                        self.notify(
                            ToastKind::Err,
                            format!("{profile} 已连续崩溃 {MAX} 次，停止自动重启"),
                        );
                    } else {
                        *e = (count + 1, Instant::now());
                        let delay = 2u64.pow(count + 1);
                        self.notify(
                            ToastKind::Info,
                            format!("{profile} 异常退出，{delay}s 后自动重启（第 {}/{MAX} 次）", count + 1),
                        );
                        self.restarting.insert(profile.clone());
                        return Task::perform(
                            async move { tokio::time::sleep(Duration::from_secs(delay)).await },
                            move |_| Message::Start(profile),
                        );
                    }
                }
            }
        }
        Task::none()
    }

    /// 关窗：先把几何写进 config.json 再关。**必须同步写**——走 Task 的话，
    /// 进程可能在落盘前就随窗口一起退出了。
    fn close_window(&mut self) -> Task<Message> {
        self.save_geom();
        // close_stops：先异步停完所有实例，Stopped 里再回到这里真关窗
        // （exit_after_stop 已置位）。taskkill 是子进程级，不能阻塞 update。
        if self.config.close_stops && !self.procs.is_empty() && !self.exit_after_stop {
            self.exit_after_stop = true;
            return Task::done(Message::StopAll);
        }
        match self.window {
            Some(id) => window::close(id),
            None => iced::exit(),
        }
    }

    /// 开托盘 = 关到托盘（dsh 实例继续跑，托盘常驻模式）；没开 = 真退出。
    fn close_or_tray(&mut self) -> Task<Message> {
        if self.config.tray {
            self.save_geom();
            crate::tray::hide_window();
            Task::none()
        } else {
            self.close_window()
        }
    }

    /// 窗口几何落盘（同步，理由见 close_window）。
    fn save_geom(&mut self) {
        if let (Some(p), Some(s)) = (self.win_pos, self.win_size) {
            self.config.window = Some(crate::core::store::WindowGeom {
                x: p.x,
                y: p.y,
                w: s.width,
                h: s.height,
            });
            self.cfg_draft = self.config.clone();
            if let Err(e) = crate::core::store::save(&self.config) {
                log::warn!("保存窗口几何失败: {e}");
            }
        }
    }

    /// 空闲零订阅（DESIGN.md §8 第 1 条）：
    /// - `frames()` 只在动画期间挂
    /// - 进程轮询只在有实例在跑时挂
    /// - toast 清理只在有 toast 时挂
    /// - core 事件流是被动的，后端不发就不产消息
    /// - `window::events()` 同理被动：不拖动/缩放/关窗就没有事件
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            window::open_events().map(Message::Opened),
            // 几何跟踪与关窗保存的数据源（Moved/Resized/CloseRequested）。
            window::events().map(|(_, e)| Message::WindowEvent(e)),
            bridge::events().map(Message::Core),
            iced::event::listen_with(|event, _status, _id| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key, modifiers, ..
                }) = event
                else {
                    return None;
                };
                // ESC 关模态（模态是覆盖层，拿不到键盘焦点，只能走全局监听）。
                // 目录编辑表单也是浮层：ESC 关它而不是下层模态（开合状态走
                // 全局原子量——这个订阅只收裸 fn 指针，捕获不了状态）。
                if matches!(
                    key,
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                ) {
                    return Some(if crate::app::DIRS_EDIT_OPEN.load(std::sync::atomic::Ordering::Relaxed) {
                        Message::ObClose
                    } else {
                        Message::CloseDialog
                    });
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
        if self.anim.is_animating()
            || self
                .resize_pump_until
                .is_some_and(|t| Instant::now() < t)
        {
            subs.push(window::frames().map(Message::Tick));
        }
        if self.config.tray || self.minimized_start {
            subs.push(crate::tray::events().map(Message::Tray));
        }
        if self.config.close_stops && self.config.app_window {
            // 桌面窗口被关 → WebviewClosed（按 close_stops 停服务）。只有真用
            // 桌面窗口的用户才挂这条，空闲零订阅纪律不变。
            subs.push(crate::core::webview::close_events().map(|_| Message::WebviewClosed));
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
        if self.migrating {
            // 目录转移进度心跳。复制是大批量小文件，120ms 刷新一次足够顺滑，
            // 也不至于把 update 日志/重绘打爆。
            subs.push(iced::time::every(Duration::from_millis(120)).map(|_| Message::MigrateTick));
        }
        if self.update_busy {
            // 更新下载心跳：120ms 读一次进度全局刷进度条（同迁移的节流）。
            subs.push(iced::time::every(Duration::from_millis(120)).map(|_| Message::UpdateTick));
        }
        // 环境光漂移的心跳（AmbientTick）已随背景冻结一起拆掉（2026-09-06）：
        // 用户看了几天说「反正也看不出来在动」，空闲零出帧纪律恢复。
        Subscription::batch(subs)
    }
}
