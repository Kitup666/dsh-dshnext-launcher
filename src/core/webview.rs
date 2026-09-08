//! 桌面窗口模式：WebView2 独立窗口。
//!
//! 与 `--app`（借 Edge 的窗口）不同，这是**我们自己的顶层窗口**——进程是
//! dshnext.exe、挂鲸鱼图标、标题「DshDesk — WebUI」，所以 MyDockFinder 里不再归到
//! Edge 名下。代价是多一棵 WebView2 进程树（~150-200MB，同内核）。
//!
//! 为什么手写 Win32 窗口而不用 tao/wry 的窗口层：tao 是 winit 的分支，而 iced 主线程
//! 已经跑着一个 winit 事件循环——同进程两个 winit 系循环在 Windows 上抢全局状态
//! （窗口类/DPI），副线程建 tao 窗口会原生崩溃。所以这里用 `windows` crate 手写一个
//! 原生窗口 + 消息泵，只借 wry 做 WebView2 那层（wry 通过 raw-window-handle 拿我们的 HWND）。
//!
//! 线程模型（照 tray.rs 的约束）：WebView2 的 COM 对象 + 窗口必须建在自带消息泵的
//! 线程上。主线程通过 `PostMessageW(WM_APP_OPEN, Box<url>)` 跨线程投递命令（PostMessage
//! 本身线程安全，消息进目标线程队列由其泵处理）。单窗口复用：关窗=隐藏，再开发货导航。

use std::cell::RefCell;
use std::num::NonZero;
use std::sync::mpsc;
use std::sync::OnceLock;

use raw_window_handle::{
    DisplayHandle, HasDisplayHandle, HasWindowHandle, HandleError, RawDisplayHandle,
    RawWindowHandle, Win32WindowHandle, WindowHandle, WindowsDisplayHandle,
};
use wry::{WebView, WebViewBuilder};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    MSG, SW_HIDE, SW_SHOW, WM_APP, WM_CLOSE, WM_DESTROY, WS_OVERLAPPEDWINDOW,
    WNDCLASSEXW,
};

/// 跨线程投递「打开/导航到 URL」的自定义消息。lParam = Box<String> 的裸指针。
const WM_APP_OPEN: u32 = WM_APP + 1;

const WEBUI_TITLE: &str = "DshDesk — WebUI";
const CLASS_NAME: &str = "DshnextWebuiWindow";

/// webview 线程活到进程结束；HWND 就绪后填这里，供主线程 PostMessage。
static HWND_GLOBAL: OnceLock<isize> = OnceLock::new();
static INIT: OnceLock<()> = OnceLock::new();

thread_local! {
    /// 本线程持有的 WebView2（WndProc 里 load_url 用）。
    static WV: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

/// 以 WebView2 窗口打开 `url`（带 token 的 WebUI 地址）。无运行时 → Err（不回退）。
pub fn open(url: String) -> Result<(), String> {
    if !runtime_present() {
        return Err("未检测到 WebView2 运行时（请安装 Microsoft Edge WebView2 Runtime）".into());
    }
    ensure_started()?;
    let hwnd = HWND_GLOBAL.get().copied().ok_or("webview 窗口未就绪")?;
    let boxed = Box::into_raw(Box::new(url)) as isize;
    let posted = unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_APP_OPEN,
            WPARAM(0),
            LPARAM(boxed),
        )
    };
    if posted.is_ok() {
        Ok(())
    } else {
        // 投递失败：回收 Box 防泄漏，报错。
        unsafe {
            drop(Box::from_raw(boxed as *mut String));
        }
        Err("WebView2 窗口已关闭".into())
    }
}

fn ensure_started() -> Result<(), String> {
    INIT.get_or_init(|| {
        let (ack_tx, ack_rx) = mpsc::channel::<isize>();
        let spawned = std::thread::Builder::new()
            .name("dshnext-webview".into())
            .spawn(move || run_thread(ack_tx));
        if spawned.is_ok() {
            if let Ok(hwnd) = ack_rx.recv() {
                let _ = HWND_GLOBAL.set(hwnd);
            }
        }
    });
    if HWND_GLOBAL.get().is_some() {
        Ok(())
    } else {
        Err("WebView2 线程启动失败".into())
    }
}

/// UTF-16 空结尾（PCWSTR 要的缓冲）。
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// webview 线程主体：建原生窗口 + WebView2，跑消息泵。永不返回。
fn run_thread(ack_tx: mpsc::Sender<isize>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let hinst = match unsafe { GetModuleHandleW(None) } {
        Ok(h) => h,
        Err(e) => {
            log::error!("GetModuleHandleW 失败：{e}");
            return;
        }
    };

    let class_w = to_wide(CLASS_NAME);
    // 窗口图标：直接用用户指定的 DSH.ico（内嵌进 exe，运行时解析 ICO 目录取
    // 最大那张图给 CreateIconFromResourceEx）。加载失败留默认图标，不致命。
    let icon = load_dsh_icon();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        hIcon: icon.unwrap_or_default(),
        hIconSm: icon.unwrap_or_default(),
        hCursor: unsafe {
            windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            )
        }
        .unwrap_or_default(),
        lpszClassName: PCWSTR(class_w.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&wc) } == 0 {
        log::error!("注册 WebView2 窗口类失败");
        return;
    }

    let title_w = to_wide(WEBUI_TITLE);
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
            1200,
            800,
            None,
            None,
            Some(HINSTANCE(hinst.0)),
            None,
        )
    };
    let hwnd = match hwnd {
        Ok(h) => h,
        Err(e) => {
            log::error!("创建 WebView2 窗口失败：{e}");
            return;
        }
    };

    // 建 WebView2 挂到这个 HWND（wry 内部会跑嵌套消息泵等异步环境创建完成）。
    match WebViewBuilder::new().build(&HwndWindow(hwnd.0 as isize, hinst.0 as isize)) {
        Ok(wv) => WV.with(|c| *c.borrow_mut() = Some(wv)),
        Err(e) => {
            log::error!("创建 WebView2 失败：{e}");
            return;
        }
    }

    let _ = ack_tx.send(hwnd.0 as isize);

    // 消息泵。
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_APP_OPEN => {
            let url = unsafe { *Box::from_raw(lparam.0 as *mut String) };
            WV.with(|c| {
                if let Some(wv) = &*c.borrow() {
                    let _ = wv.load_url(&url);
                }
            });
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
            }
            LRESULT(0)
        }
        // 关闭 = 隐藏复用，不销毁（下次 Open 秒显）。
        WM_CLOSE => {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// 把原生 HWND 适配成 raw-window-handle，喂给 wry。
struct HwndWindow(isize, isize); // (hwnd, hinstance)

impl HasWindowHandle for HwndWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let mut h = Win32WindowHandle::new(NonZero::new(self.0).ok_or(HandleError::Unavailable)?);
        h.hinstance = NonZero::new(self.1);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(h)) })
    }
}

impl HasDisplayHandle for HwndWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe {
            DisplayHandle::borrow_raw(RawDisplayHandle::Windows(WindowsDisplayHandle::new()))
        })
    }
}

/// 内嵌的 DSH.ico（用户指定的 WebUI 窗口图标）。
const DSH_ICO: &[u8] = include_bytes!("../../assets/icons/dsh.ico");

/// 从内嵌 .ico 解析出最大尺寸那张图，CreateIconFromResourceEx 生成 HICON。
/// .ico 结构 = ICONDIR(6B) + N 个 ICONDIRENTRY(16B) + 图像数据（BITMAPINFOHEADER
/// +DIB 或 PNG，Vista 起 CreateIconFromResourceEx 也认 PNG）。
fn load_dsh_icon() -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, LR_DEFAULTSIZE};
    let b = DSH_ICO;
    if b.len() < 6 || u16::from_le_bytes([b[0], b[1]]) != 1 {
        return None; // 不是图标文件（type 字段应为 1）
    }
    let count = u16::from_le_bytes([b[4], b[5]]) as usize;
    // 选尺寸最大的那张（目录里的 width/height 字节 0 = 256）。
    let mut best: Option<(u64, usize, usize)> = None; // (像素面积, offset, len)
    for i in 0..count {
        let e = 6 + i * 16;
        if e + 16 > b.len() {
            break;
        }
        let dim = |x: u8| u64::from(if x == 0 { 256u16 } else { u16::from(x) });
        let size = dim(b[e]) * dim(b[e + 1]);
        let len = u32::from_le_bytes(b[e + 8..e + 12].try_into().ok()?) as usize;
        let off = u32::from_le_bytes(b[e + 12..e + 16].try_into().ok()?) as usize;
        if off + len > b.len() {
            continue;
        }
        if best.map_or(true, |(bs, _, _)| size > bs) {
            best = Some((size, off, len));
        }
    }
    let (_, off, len) = best?;
    unsafe {
        CreateIconFromResourceEx(&b[off..off + len], true, 0x0003_0000, 0, 0, LR_DEFAULTSIZE).ok()
    }
}

/// WebView2 运行时是否在场：查 EdgeUpdate 客户端注册表键（存在即装了 Evergreen runtime）。
/// 走 `reg` 子进程（同 platform.rs 纪律），只看退出码不解析值。
fn runtime_present() -> bool {
    use std::os::windows::process::CommandExt;
    const GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let keys = [
        format!(r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{GUID}"),
        format!(r"HKLM\SOFTWARE\Microsoft\EdgeUpdate\Clients\{GUID}"),
        format!(r"HKCU\Software\Microsoft\EdgeUpdate\Clients\{GUID}"),
    ];
    keys.iter().any(|k| {
        std::process::Command::new("reg")
            .args(["query", k.as_str()])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}
