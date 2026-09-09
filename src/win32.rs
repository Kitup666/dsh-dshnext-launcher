//! Win32 窗口显隐：托盘「关闭到托盘 / 显示」的后半截。
//!
//! iced 0.14 的 window 任务集里没有 hide/show（只有 minimize，收进托盘会闪
//! 任务栏最小化动画，不对），所以直接 ShowWindow。窗口句柄按标题 FindWindowW——
//! 与 AGENTS.md 里整套自动化脚本（PrintWindow/PostMessage）同一套找法。

/// 显示并前置主窗口。
pub fn show_main_window(title: &str) {
    if !raise_if_exists(title) {
        log::warn!("托盘显示：找不到主窗口（标题 {title:?}）");
    }
}

/// 窗口存在则显示并前置，返回是否存在。桌面窗口宿主用它做「已有窗口→复用」
/// （启动器 open() 与独立双击两条路都靠它去重）。
pub fn raise_if_exists(title: &str) -> bool {
    unsafe {
        let Some(hwnd) = find_window(title) else {
            return false;
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
        true
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

/// 置顶开关：HWND_TOPMOST / HWND_NOTOPMOST 只改 Z 序，不动位置尺寸、不抢焦点。
pub fn set_topmost(title: &str, on: bool) {
    unsafe {
        if let Some(hwnd) = find_window(title) {
            let insert = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
            SetWindowPos(hwnd, insert, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// 单实例互斥：进程级资源（ProcMap、托盘、config 写入、自更新 rename）
/// 都按「一个启动器」设计，双开会互相看不见对方的子进程、互踩 config。
///
/// - `Ok(())` = 拿到互斥，本实例是第一个（句柄存全局，进程退出 OS 自动释放，
///   崩溃也不会留死锁——这正是选互斥对象而不是指针文件的原因）。
/// - `Err(())` = 已有实例在跑，已尝试把它的窗口恢复并前置，调用方直接退出。
pub fn acquire_single_instance(title: &str) -> Result<(), ()> {
    // 互斥句柄必须一直攥着（一放下互斥就失效），进程内没人用它但得活着。
    static KEEP: std::sync::Mutex<Option<isize>> = std::sync::Mutex::new(None);
    unsafe {
        let name: Vec<u16> = "Local\\Dshnext.SingleInstance"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle == 0 {
            // 创建失败（极罕见）：宁可不拦，别把正常启动拦死。
            log::warn!("单实例互斥创建失败，放行");
            return Ok(());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            log::info!("已有实例在运行，前置其窗口后退出");
            show_main_window(title);
            return Err(());
        }
        *KEEP.lock().expect("单实例句柄锁") = Some(handle);
        Ok(())
    }
}

const HWND_TOPMOST: isize = -1;
const HWND_NOTOPMOST: isize = -2;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;

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
    #[allow(non_snake_case)]
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, w: i32, h: i32, flags: u32) -> bool;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn CreateMutexW(attr: *const u32, initial: i32, name: *const u16) -> isize;
    fn GetLastError() -> u32;
}

const ERROR_ALREADY_EXISTS: u32 = 183;
