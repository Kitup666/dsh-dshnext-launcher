//! 系统托盘（roadmap #6）：tray-icon 0.24，菜单「显示 / 退出」+ 左键点击显示窗口。
//!
//! 平台事实（查过源码/踩过才敢写）：
//! - **iced 0.14 没有 window::hide/show**（iced_runtime 全文只有 minimize），所以
//!   「关闭到托盘」直接走 Win32 `ShowWindow`（`win32.rs`），hwnd 按标题找。
//! - **TrayIcon 不是 Send**（内部 Rc<RefCell>）：托盘对象放 thread_local，
//!   只从主线程碰——`ensure()`/`remove()` 只在 update（Message::Opened / 保存
//!   设置）里调，事件订阅流（跑在 iced 执行器的 worker 线程）**只读** crate
//!   的全局事件 channel，不碰托盘对象。
//! - tray-icon 在 Windows 上自建消息窗口收事件，要求创建线程有消息泵——winit
//!   主线程即满足；菜单/点击事件从全局 channel 出来，轮询模式同 `bridge.rs`。
//! - `SetForegroundWindow` 在调用方非前台时静默失败（AGENTS.md 坑 1），
//!   `win32.rs` 里先 AttachThreadInput 再前置。

use std::cell::RefCell;
use tray_icon::{menu, TrayIcon, TrayIconBuilder};

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

/// 托盘事件（经 Subscription 转成 Message::Tray）。
#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    Show,
    Exit,
}

/// 幂等创建托盘（主线程；失败只记日志——托盘是增强，不能为它崩主程序）。
pub fn ensure() {
    TRAY.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        let item_show = menu::MenuItem::with_id("dsh.show", "显示主窗口", true, None);
        let item_exit = menu::MenuItem::with_id("dsh.exit", "退出", true, None);
        let menu = menu::Menu::new();
        if let Err(e) = menu.append_items(&[&item_show, &item_exit]) {
            log::warn!("托盘菜单创建失败: {e}");
            return;
        }
        let rgba = include_bytes!("../assets/icons/window-64.rgba");
        let icon = tray_icon::Icon::from_rgba(rgba.to_vec(), 64, 64).expect("托盘图标尺寸");
        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("DshDesk — DeepSeek Harness 启动器")
            .with_icon(icon)
            .build()
        {
            Ok(t) => *slot.borrow_mut() = Some(t),
            Err(e) => log::warn!("托盘创建失败: {e}"),
        }
    });
}

/// 移除托盘（Drop 撤图标）。非主线程调用是 no-op（thread_local 为空）。
pub fn remove() {
    TRAY.with(|slot| *slot.borrow_mut() = None);
}

/// 托盘事件流：轮询全局 channel（菜单 + 托盘点击）。只在开托盘时挂
/// （subscription 按 config.tray 门控），无事件时 50ms 醒一次、零帧产出。
pub fn events() -> iced::Subscription<TrayEvent> {
    iced::Subscription::run(|| {
        use futures_util::SinkExt;
        iced::stream::channel(16, async |mut out| {
            loop {
                while let Ok(ev) = menu::MenuEvent::receiver().try_recv() {
                    if ev.id.0 == "dsh.show" {
                        let _ = out.send(TrayEvent::Show).await;
                    } else if ev.id.0 == "dsh.exit" {
                        let _ = out.send(TrayEvent::Exit).await;
                    }
                }
                while let Ok(ev) = tray_icon::TrayIconEvent::receiver().try_recv() {
                    // 左键单击当「显示」；右键菜单是 tray-icon 自己弹的。
                    if matches!(
                        ev,
                        tray_icon::TrayIconEvent::Click {
                            button: tray_icon::MouseButton::Left,
                            ..
                        }
                    ) {
                        let _ = out.send(TrayEvent::Show).await;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
    })
}

/// 显示并前置主窗口（win32.rs 按标题找 hwnd）。
pub fn show_window() {
    crate::win32::show_main_window(crate::app::WINDOW_TITLE);
}

/// 隐藏主窗口（关闭到托盘）。
pub fn hide_window() {
    crate::win32::hide_main_window(crate::app::WINDOW_TITLE);
}
