//! Win32 窗口显隐：托盘「关闭到托盘 / 显示」的后半截。
//!
//! iced 0.14 的 window 任务集里没有 hide/show（只有 minimize，收进托盘会闪
//! 任务栏最小化动画，不对），所以直接 ShowWindow。窗口句柄按标题 FindWindowW——
//! 与 AGENTS.md 里整套自动化脚本（PrintWindow/PostMessage）同一套找法。

/// 显示并前置主窗口。
pub fn show_main_window(title: &str) {
    unsafe {
        let Some(hwnd) = find_window(title) else {
            log::warn!("托盘显示：找不到主窗口（标题 {title:?}）");
            return;
        };
        ShowWindow(hwnd, SW_SHOW);
        // SetForegroundWindow 在调用方非前台时静默失败（AGENTS.md 坑 1）：
        // 借当前前台线程的输入队列再前置。
        let fg = GetForegroundWindow();
        let this_tid = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        let fg_tid = GetWindowThreadProcessId(fg, std::ptr::null_mut());
        if fg != 0 && fg_tid != this_tid {
            AttachThreadInput(fg_tid, this_tid, 1);
            SetForegroundWindow(hwnd);
            AttachThreadInput(fg_tid, this_tid, 0);
        } else {
            SetForegroundWindow(hwnd);
        }
    }
}

/// 隐藏主窗口。
pub fn hide_main_window(title: &str) {
    unsafe {
        if let Some(hwnd) = find_window(title) {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

unsafe fn find_window(title: &str) -> Option<isize> {
    let mut wide: Vec<u16> = title.encode_utf16().collect();
    wide.push(0);
    // edition 2024：unsafe fn 体里调 unsafe 操作也要显式 unsafe 块。
    let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
    if hwnd == 0 {
        None
    } else {
        Some(hwnd)
    }
}

const SW_HIDE: i32 = 0;
const SW_SHOW: i32 = 9;

unsafe extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn GetForegroundWindow() -> isize;
    fn AttachThreadInput(a: u32, b: u32, attach: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
}
