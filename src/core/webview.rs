//! 桌面窗口模式：WebView2 独立窗口（无边框 + macOS 风交通灯角标 + 顶部悬浮横条）。
//!
//! 自己的顶层窗口：进程 DeepseekHarness.exe、挂 DSH.ico、无原生标题栏。webview 子窗口
//! 铺满客户区（四周内缩 GRIP 宽——这圈是父窗口自己的客户区，鼠标落在上面父窗口才收
//! 得到 WM_NCHITTEST，边缘缩放全靠它；环刻意一个像素都不画，保持 alpha=0 透出桌面 =
//! 透明外圈）。操作件两类，都是 owned top-level 悬浮层：
//!
//! - **上两角 L 形角括号**（hover 停留 220ms 浮现）：右上红=关闭、左上黄=最小化。
//!   一横一竖两笔胶囊、圆头端帽 + 肘点倒角、半透明、同色外发光，悬停加亮。
//! - **顶部居中横条**（默认隐藏，光标靠近顶部中央 ~80ms 浮现，带下滑+淡入浮动动画）：
//!   半透明白胶囊；拖动=移动窗口（转发 WM_NCLBUTTONDOWN/HTCAPTION 进系统模态移动循环）、
//!   双击=最大化/还原、单击=锁置顶（再点解锁，锁定时胶囊变蓝）。单/双击用「松手起
//!   双击间隔定时器、期间收到双击则取消」区分；按下拖出阈值先进拖拽并吃掉那次 UP。
//!
//! 渲染走 UpdateLayeredWindow + 软件 SDF 抗锯齿（预乘 BGRA 位图）：色键透明做不了
//! 半透明和逐帧动画，ULW 可以；小图逐像素 float 混合，每帧几千像素，开销可忽略。
//! 显示/隐藏/动画由主窗口 30ms 定时器统一驱动（轮询光标位置——webview 子窗口会吃掉
//! 它下面的 WM_MOUSEMOVE，定时轮询是唯一稳妥的 hover 检测）；WM_MOVE/WM_SIZE 实时
//! 重摆悬浮层，拖动/缩放全程跟手。置顶态运行时记录（PINNED），窗口隐藏重开不丢。
//!
//! 为什么手写原生 Win32 而不用 tao/wry 的窗口层：tao 是 winit 分支，与 iced 主线程的
//! winit 同进程抢全局状态（窗口类/DPI），副线程建 tao 窗口会原生崩溃。所以用 `windows`
//! crate 手写窗口 + 消息泵，只借 wry 做 WebView2（raw-window-handle 递 HWND，且必须
//! build_as_child 才能用 set_bounds）。
//!
//! 进程模型（2026-09-09 二次改）：桌面窗口跑在**独立二进制** `DeepseekHarness.exe`
//! （workspace 成员 webui/，与本模块同一份源码）里。此前用「dshnext.exe 硬链接换皮 +
//! 中继自展」伪造身份，但硬链接与启动器同字节——dock/任务管理器读到的内嵌图标与
//! 版本描述永远和启动器一样，改名不改皮。独立 bin 才有真身份：自己的文件名、自己的
//! 鲸鱼图标（webui/build.rs）、任务管理器天然独立成行（不再需要中继）。宿主自带消息
//! 泵 + CoInitializeEx(STA)；启动器经管道递活：stdin 每行一个 URL（token 不上命令行），
//! 宿主 stdout 回 `closed` 行，父进程退出 → EOF → 自毁。双击独立打开时 stdin 不是
//! 管道：改从数据目录 `webui-url.txt` 读最近 URL（token 落盘是独立打开的必要代价，
//! 文件在用户级 LOCALAPPDATA）。单窗口复用，关窗=隐藏。

use std::cell::{Cell, RefCell};
use std::num::NonZero;
use std::sync::OnceLock;

use raw_window_handle::{
    DisplayHandle, HasDisplayHandle, HasWindowHandle, HandleError, RawDisplayHandle,
    RawWindowHandle, Win32WindowHandle, WindowHandle, WindowsDisplayHandle,
};
use wry::{Rect, WebView, WebViewBuilder};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, GetDoubleClickTime, ReleaseCapture, SetCapture,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// 跨线程投递「打开/导航到 URL」。lParam = Box<String> 裸指针。
const WM_APP_OPEN: u32 = WM_USER + 1;
/// stdin EOF（父进程已退出）→ 自毁。
const WM_APP_QUIT: u32 = WM_USER + 2;

/// 窗口名即程序名：dock/任务栏/Alt+Tab 显示的都是它。
const WEBUI_TITLE: &str = "DeepseekHarness";
const CLASS_NAME: &str = "DshnextWebuiWindow";
const BADGE_CLASS: &str = "DshnextWebuiBadge";
const PILL_CLASS: &str = "DshnextWebuiPill";

/// 边缘缩放手柄宽度（逻辑像素）。webview 子窗口四周内缩 GRIP，这圈是父窗口自己的
/// 客户区——鼠标落在上面 WM_NCHITTEST 才到父窗口，边缘缩放才有得收。
const GRIP: f64 = 6.0;
/// 角括号触发区边长（逻辑像素，窗口上两角的方形热区）。别调大——太大就把
/// 角落缩放热区盖住了，拖不了边。
const ZONE: f64 = 24.0;
/// L 形角括号：肘点离角 / 臂长 / 笔画粗细 / 悬浮层窗口边长（逻辑像素）。
const INSET: f64 = 10.0;
const ARM: f64 = 20.0;
const THK: f64 = 2.5;
const BADGE_WIN: f64 = 46.0;
/// 内容圆角半径（逻辑像素）。原先取 16−GRIP=10，用户嫌大，减半。
const CONTENT_RADIUS: f64 = 5.0;
/// 桌面窗口最小尺寸（逻辑像素，宽=高）。改这一个数即可。
const MIN_TRACK: f64 = 300.0;
/// 顶部横条：胶囊可见长宽 / 窗口高 / 窗口离顶边 / 触发带半宽半高（逻辑像素）。
const PILL_W: f64 = 160.0;
const PILL_H: f64 = 8.0;
const PILL_HIT: f64 = 22.0;
const PILL_TOP: f64 = 8.0;
const PILL_ZONE_W: f64 = 130.0;
const PILL_ZONE_H: f64 = 72.0;
/// 悬停停留门槛：交通灯 220ms（防路过误弹）、横条 80ms（它是拖动手柄要灵敏）。
const BADGE_DWELL: u128 = 220;
const PILL_DWELL: u128 = 80;
/// 浮入/浮出时长（秒）。
const IN_DUR: f32 = 0.18;
const OUT_DUR: f32 = 0.14;
/// hover 轮询定时器 id + 间隔（ms）。
const HOVER_TIMER: usize = 0xB7;
const HOVER_INTERVAL: u32 = 30;
/// 横条「单击 vs 双击」区分定时器 id。
const PILL_CLICK_TIMER: usize = 0xC1;

/// 交通灯色（sRGB 浮点）：红 #FF5F57=关闭 / 黄 #FEBC2E=最小化（macOS 规格）。
const CLOSE_RGB: [f32; 3] = [1.0, 0.373, 0.341];
const MIN_RGB: [f32; 3] = [0.996, 0.737, 0.180];
/// 横条/角标统一的品牌蓝 #5B76FF（置顶态更强）。
const PIN_RGB: [f32; 3] = [0.357, 0.463, 1.0];
/// 环底色（深色近似）：类背景刷用（失焦时系统擦 NC 区，防白条）。
const RING_BG: u32 = 0x001A1A1C;

/// 宿主进程（DeepseekHarness.exe）身份标志：决定 notify_closed 走 stdout 还是
/// 进程内 channel。独立双击打开时 stdout 无效，写入自然失败、忽略即可。
static IS_HOST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// 主进程侧：桌面窗口宿主子进程（stdin 递 URL、stdout 收 closed）。
struct Host {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
}
static HOST: std::sync::Mutex<Option<Host>> = std::sync::Mutex::new(None);

/// 桌面窗口被关（红角标 / Alt+F4）→ 通知主程序按 close_stops 停服务。主进程里
/// 走全局 channel（订阅 builder 是裸 fn 捕获不了状态，同 bridge/tray）；宿主
/// 子进程里写 stdout 一行 `closed`，由主进程的读线程转进同一个 channel。
static CLOSE_TX: OnceLock<tokio::sync::mpsc::UnboundedSender<()>> = OnceLock::new();
static CLOSE_RX: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<()>>> =
    std::sync::Mutex::new(None);

fn notify_closed() {
    if IS_HOST.load(std::sync::atomic::Ordering::Relaxed) {
        use std::io::Write;
        let mut so = std::io::stdout();
        let _ = so.write_all(b"closed\n");
        let _ = so.flush();
        return;
    }
    let tx = CLOSE_TX.get_or_init(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        *CLOSE_RX.lock().expect("CLOSE_RX 锁") = Some(rx);
        tx
    });
    let _ = tx.send(());
}

/// 关闭事件流（主程序侧订阅，映射成 Message::WebviewClosed）。
pub fn close_events() -> iced::Subscription<()> {
    iced::Subscription::run(|| {
        iced::stream::channel(4, async move |mut out| {
            use futures_util::SinkExt;
            loop {
                let got = {
                    let mut g = CLOSE_RX.lock().expect("CLOSE_RX 锁");
                    g.as_mut().and_then(|rx| rx.try_recv().ok())
                };
                if got.is_some() {
                    let _ = out.send(()).await;
                } else {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        })
    })
}

/// 给窗口挂独立任务栏身份（per-window AUMID）。不设的话，同进程的桌面窗口
/// 会并进启动器那个任务栏组，没有可单独点选的项。走属性存储通路
/// （`SHGetPropertyStoreForWindow` + `PKEY_AppUserModel_ID`），不是进程级的
/// `SetCurrentProcessExplicitAppUserModelID`（那个会连主窗口一起改名）。
fn set_window_aumid(hwnd: HWND, aumid: &str) {
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Variant::VT_BSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    unsafe {
        let Ok(ps) = SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd) else {
            log::warn!("取窗口属性存储失败，任务栏身份未设");
            return;
        };
        let mut pv = PROPVARIANT::default();
        // 联合体里的 ManuallyDrop 字段不自动 DerefMut，显式解一层再写。
        (*pv.Anonymous.Anonymous).vt = VT_BSTR;
        (*pv.Anonymous.Anonymous).Anonymous.bstrVal =
            std::mem::ManuallyDrop::new(windows::core::BSTR::from(aumid));
        match ps.SetValue(&PKEY_AppUserModel_ID, &pv) {
            Ok(()) => {
                let _ = ps.Commit();
                // 读回验证。**窗口属性存储读回来是 VT_VECTOR|VT_UI1（字节序列化的
                // UTF-16），不是我们写进去的 VT_BSTR**——按 BSTR 解释直接段错误
                // （2026-09-09 踩实）。先验类型再取。
                match ps.GetValue(&PKEY_AppUserModel_ID) {
                    Ok(got) => {
                        let inner = &*got.Anonymous.Anonymous;
                        if inner.vt == VT_BSTR {
                            let s = inner.Anonymous.bstrVal.to_string();
                            log::info!("桌面窗口 AUMID 已设并读回：{s:?}");
                        } else {
                            log::info!("桌面窗口 AUMID 已设（读回类型 {:?}）", inner.vt);
                        }
                    }
                    Err(e) => log::warn!("AUMID 读回失败：{e}"),
                }
            }
            Err(e) => log::warn!("AUMID SetValue 失败：{e}"),
        }
        std::mem::ManuallyDrop::drop(&mut (*pv.Anonymous.Anonymous).Anonymous.bstrVal);
    }
}

thread_local! {
    static WV: RefCell<Option<WebView>> = const { RefCell::new(None) };
    /// 两个交通灯 + 一个横条的悬浮层窗口。
    static BADGES: RefCell<[HWND; 2]> = const {
        RefCell::new([HWND(std::ptr::null_mut()), HWND(std::ptr::null_mut())])
    };
    static PILL: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
    /// 动画进度 0..1（3=横条位）。
    static BADGE_T: RefCell<[f32; 2]> = const { RefCell::new([0.0; 2]) };
    static PILL_T: Cell<f32> = const { Cell::new(0.0) };
    /// 上一帧时间戳（算 dt）。
    static LAST_POLL: Cell<u128> = const { Cell::new(0) };
    /// 当前触发区（-1 无 / 0 关 / 1 最小 / 2 横条）+ 进入时刻。
    static ZONE_ID: Cell<i32> = const { Cell::new(-1) };
    static ZONE_SINCE: Cell<u128> = const { Cell::new(0) };
    /// 上次渲染签名（x,y,t*1000,hover[,pinned]），没变就跳过 ULW。
    static BADGE_SIG: RefCell<[(i64, i64, i64, i64); 2]> = const { RefCell::new([(0, 0, 0, 0); 2]) };
    /// 角标底图（纯贴图）：按 (dpi, 边长) 烘一次 [[idle;hover];角标] 预乘 BGRA，
    /// 每帧只剩逐字节乘 alpha，不再跑 SDF + 指数衰减。
    static BADGE_MASTER: RefCell<Option<(u32, usize, [[Vec<u8>; 2]; 2])>> =
        const { RefCell::new(None) };
    static BADGE_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static PILL_SIG: Cell<(i64, i64, i64, i64, i64)> = const { Cell::new((0, 0, 0, 0, 0)) };
    /// 横条底图：同角标的纯贴图方案，按 (dpi, 宽, 高) 烘 [[未置顶;置顶];idle;hover]。
    static PILL_MASTER: RefCell<Option<(u32, usize, usize, [[Vec<u8>; 2]; 2])>> =
        const { RefCell::new(None) };
    static PILL_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// 横条按下/拖拽/双击状态机。
    static PILL_DOWN: Cell<(i32, i32)> = const { Cell::new((0, 0)) };
    static PILL_DRAG: Cell<bool> = const { Cell::new(false) };
    static PILL_DBL: Cell<bool> = const { Cell::new(false) };
    /// 置顶（HWND_TOPMOST）态。关窗=隐藏不销毁，重开要保持。
    static PINNED: Cell<bool> = const { Cell::new(false) };
}

/// 内嵌的 DSH.ico（WebUI 窗口图标）。
const DSH_ICO: &[u8] = include_bytes!("../../assets/icons/dsh.ico");

/// 以 WebView2 窗口打开 `url`：确保宿主进程活着，把 URL 写进它的 stdin。
/// 无运行时 → Err（不回退）。写失败（宿主被杀/崩）→ 收尸重展一次再试。
/// 若已有独立打开的宿主窗口（非本进程管道拉起），前置复用、不再起第二个。
pub fn open(url: String) -> Result<(), String> {
    if !runtime_present() {
        return Err("未检测到 WebView2 运行时（请安装 Microsoft Edge WebView2 Runtime）".into());
    }
    let mine = HOST.lock().map_or(false, |g| g.is_some());
    if !mine && crate::win32::raise_if_exists(WEBUI_TITLE) {
        log::info!("已有独立打开的桌面窗口，前置复用");
        return Ok(());
    }
    let line = format!("{url}\n");
    use std::io::Write;
    for attempt in 0..2 {
        ensure_host()?;
        let r = HOST.lock().map_or_else(
            |_| Err("HOST 锁中毒".to_string()),
            |mut g| match g.as_mut() {
                Some(h) => h
                    .stdin
                    .write_all(line.as_bytes())
                    .and_then(|_| h.stdin.flush())
                    .map_err(|e| e.to_string()),
                None => Err("宿主进程未就绪".to_string()),
            },
        );
        if r.is_ok() {
            return Ok(());
        }
        log::warn!("写 webview 宿主进程失败（第 {attempt} 次），重展：{}", r.unwrap_err());
        if let Ok(mut g) = HOST.lock() {
            if let Some(mut h) = g.take() {
                let _ = h.child.kill();
                let _ = h.child.wait();
            }
        }
    }
    Err("WebView2 宿主进程启动失败".into())
}

fn ensure_host() -> Result<(), String> {
    let mut g = HOST.lock().map_err(|_| "HOST 锁中毒".to_string())?;
    if g.is_some() {
        // 存活与否不在此判；open() 写失败即视为宿主没了（收尸重展）。
        return Ok(());
    }
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // 宿主是独立构建的同目录兄弟 exe（webui/ 成员，产物 DeepseekHarness.exe）：
    // 自己的文件名/图标/版本信息，dock 与任务管理器天然独立成行。
    let host = exe
        .parent()
        .ok_or("current_exe 没有父目录")?
        .join("DeepseekHarness.exe");
    if !host.exists() {
        return Err(format!(
            "缺少桌面窗口程序：{}（请更新启动器或重装安装包）",
            host.display()
        ));
    }
    let mut child = std::process::Command::new(host)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| format!("启动 WebView2 宿主进程失败：{e}"))?;
    let stdin = child.stdin.take().ok_or("拿不到宿主进程 stdin")?;
    let stdout = child.stdout.take();
    *g = Some(Host { child, stdin });
    // 宿主 stdout 读线程：一行 `closed` = 用户关了桌面窗口 → 转进进程内 channel，
    // 主程序订阅（close_events）收到后按 close_stops 停服务。
    if let Some(so) = stdout {
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(so).lines().map_while(Result::ok) {
                if line.trim() == "closed" {
                    notify_closed();
                }
            }
        });
    }
    Ok(())
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 单调毫秒（hover 停留计时用）。
fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 光标是否落在某窗口的窗口矩形里（悬浮层热区语义）。
fn in_rect(pt: POINT, w: HWND) -> bool {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(w, &mut r);
    }
    pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom
}

/// stdin 是否为管道：管道 = 启动器拉起（URL 走 stdin）；否则（双击，GUI 子系统
/// 无控制台，句柄无效）= 独立模式，URL 从数据目录文件读。
fn stdin_is_pipe() -> bool {
    use windows::Win32::Storage::FileSystem::GetFileType;
    use windows::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE};
    unsafe {
        match GetStdHandle(STD_INPUT_HANDLE) {
            Ok(h) if !h.is_invalid() => GetFileType(h) == windows::Win32::Storage::FileSystem::FILE_TYPE_PIPE,
            _ => false,
        }
    }
}

/// 解析 `webui-url.txt`：取首个非空行，必须是 http(s)。纯函数供测试。
fn parse_url_file(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && (l.starts_with("http://") || l.starts_with("https://")))
        .map(str::to_string)
}

/// 独立打开模式：读启动器最近一次落盘的 WebUI 地址（含 token）。
fn standalone_url() -> Option<String> {
    let dir = crate::core::store::init_data_dir();
    let s = std::fs::read_to_string(dir.join("webui-url.txt")).ok()?;
    parse_url_file(&s)
}

/// 宿主入口（DeepseekHarness.exe 的 main 调，永不返回）。双模式见模块文档。
pub fn host_main() -> ! {
    IS_HOST.store(true, std::sync::atomic::Ordering::Relaxed);
    // DPI 感知必须在任何 HWND 之前设：宿主不跑 winit（启动器的 PMv2 是它设的），
    // 不设的话系统按 96 DPI 虚拟化渲染再位图拉伸 1.25x，整窗文字发糊（2026-09-10
    // 用户实测「比网页端糊很多」）。manifest 声明会 14001（见 webui/build.rs），
    // 这里走 winit 同款运行时 API。失败（极少）退回 system aware 兜底。
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            let _ = SetProcessDPIAware();
        }
    }
    let piped = stdin_is_pipe();
    if !piped {
        // 独立模式：已有桌面窗口（启动器拉的或另一个独立实例）→ 前置复用后退出。
        if crate::win32::raise_if_exists(WEBUI_TITLE) {
            std::process::exit(0);
        }
    }
    let hwnd = build_window();
    let ptr = hwnd.0 as isize;
    let h = HWND(ptr as *mut core::ffi::c_void);
    let post_url = |url: String| {
        let boxed = Box::into_raw(Box::new(url)) as isize;
        if unsafe { PostMessageW(Some(h), WM_APP_OPEN, WPARAM(0), LPARAM(boxed)) }.is_err() {
            unsafe { drop(Box::from_raw(boxed as *mut String)) };
        }
    };
    if piped {
        // stdin 每行一个 URL → WM_APP_OPEN；EOF（父进程没了）→ WM_APP_QUIT 自毁。
        std::thread::spawn(move || {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            for line in stdin.lock().lines().map_while(Result::ok) {
                let url = line.trim().to_string();
                if url.is_empty() {
                    continue;
                }
                let boxed = Box::into_raw(Box::new(url)) as isize;
                let h = HWND(ptr as *mut core::ffi::c_void);
                let posted =
                    unsafe { PostMessageW(Some(h), WM_APP_OPEN, WPARAM(0), LPARAM(boxed)) };
                if posted.is_err() {
                    unsafe { drop(Box::from_raw(boxed as *mut String)) };
                }
            }
            let h = HWND(ptr as *mut core::ffi::c_void);
            let _ = unsafe { PostMessageW(Some(h), WM_APP_QUIT, WPARAM(0), LPARAM(0)) };
        });
    } else {
        match standalone_url() {
            Some(u) => post_url(u),
            None => {
                let msg = to_wide("还没拿到 WebUI 地址：请先在 DshDesk 启动器里启动服务（或点首页「打开界面」），再双击本程序。");
                let cap = to_wide(WEBUI_TITLE);
                unsafe {
                    MessageBoxW(None, PCWSTR(msg.as_ptr()), PCWSTR(cap.as_ptr()), MB_OK);
                }
                std::process::exit(0);
            }
        }
    }
    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    std::process::exit(0);
}

/// 建窗口 + 悬浮层 + webview 子窗口 + hover 定时器（宿主子进程主线程）。失败直接退进程。
fn build_window() -> HWND {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let hinst = match unsafe { GetModuleHandleW(None) } {
        Ok(h) => h,
        Err(e) => {
            log::error!("GetModuleHandleW 失败：{e}");
            std::process::exit(1);
        }
    };
    let class_w = to_wide(CLASS_NAME);
    let icon = load_dsh_icon();
    // 类背景刷设深色：窗口失焦时系统会用它擦除非客户区，留 null 会擦成白条。
    let bg_brush = unsafe { CreateSolidBrush(COLORREF(RING_BG)) };
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        hIcon: icon.unwrap_or_default(),
        hIconSm: icon.unwrap_or_default(),
        hbrBackground: bg_brush,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: PCWSTR(class_w.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&wc) } == 0 {
        log::error!("注册 WebView2 窗口类失败");
        std::process::exit(1);
    }
    let title_w = to_wide(WEBUI_TITLE);
    // 无边框：WS_POPUP 天生无 caption；配 WS_THICKFRAME 保留缩放、显式组合避免
    // bitflags 取反不清位的坑。WM_NCCALCSIZE 返回 0 去掉厚边框的非客户区。
    let style = WINDOW_STYLE(
        WS_POPUP.0
            | WS_THICKFRAME.0
            | WS_MINIMIZEBOX.0
            | WS_MAXIMIZEBOX.0
            | WS_SYSMENU.0
            | WS_CLIPCHILDREN.0,
    );
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            PCWSTR(class_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1200,
            800,
            None,
            None,
            Some(windows::Win32::Foundation::HINSTANCE(hinst.0)),
            None,
        )
    };
    let hwnd = match hwnd {
        Ok(h) => h,
        Err(e) => {
            log::error!("创建 WebView2 窗口失败：{e}");
            std::process::exit(1);
        }
    };

    // 无边框窗口默认是直角，显式要 DWM 圆角（Win11）。
    unsafe {
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&pref) as u32,
        );
    }
    set_window_aumid(hwnd, "DeepseekHarness");

    // PMv2 之后 CreateWindowEx 的宽高是物理像素；1200×800 是逻辑默认值，按窗口
    // 所在显示器的 DPI 放大，保持用户熟悉的默认大小（此前系统替我们拉伸）。
    unsafe {
        let s = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            (1200.0 * s).round() as i32,
            (800.0 * s).round() as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    // 悬浮层窗口类：ULW 内容不走 WM_PAINT，类只是挂 wndproc。
    let badge_class = to_wide(BADGE_CLASS);
    let wc_b = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        // CS_DBLCLKS：没有它系统永远不发 WM_LBUTTONDBLCLK（双击=两次单击）
        style: CS_DBLCLKS,
        lpfnWndProc: Some(badge_wndproc),
        hInstance: hinst.into(),
        hbrBackground: bg_brush,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: PCWSTR(badge_class.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&wc_b) } == 0 {
        log::error!("注册角标窗口类失败");
        std::process::exit(1);
    }
    let pill_class = to_wide(PILL_CLASS);
    let wc_p = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_DBLCLKS,
        lpfnWndProc: Some(pill_wndproc),
        hInstance: hinst.into(),
        hbrBackground: bg_brush,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: PCWSTR(pill_class.as_ptr()),
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&wc_p) } == 0 {
        log::error!("注册横条窗口类失败");
        std::process::exit(1);
    }
    // owned top-level → 盖在 webview 的 DWM 合成内容之上；LAYERED 走 ULW 半透明。
    let make_popup = |cls: PCWSTR, id: isize| -> Option<HWND> {
        let h = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
                cls,
                PCWSTR(to_wide("").as_ptr()),
                WINDOW_STYLE(WS_POPUP.0),
                0,
                0,
                0,
                0,
                Some(hwnd),
                None,
                Some(windows::Win32::Foundation::HINSTANCE(hinst.0)),
                None,
            )
        }
        .ok()?;
        unsafe {
            let _ = SetWindowLongPtrW(h, GWLP_USERDATA, id);
        }
        Some(h)
    };
    BADGES.with(|bs| {
        let mut arr = bs.borrow_mut();
        for id in 0..2usize {
            match make_popup(PCWSTR(badge_class.as_ptr()), id as isize) {
                Some(h) => arr[id] = h,
                None => log::error!("创建角标窗口 {id} 失败"),
            }
        }
    });
    if let Some(p) = make_popup(PCWSTR(pill_class.as_ptr()), 0) {
        PILL.with(|c| c.set(p));
    } else {
        log::error!("创建横条窗口失败");
    }

    // webview 作为子窗口铺满客户区（四周内缩 GRIP）。
    match WebViewBuilder::new()
        .with_bounds(initial_rect(hwnd))
        .build_as_child(&HwndWindow(hwnd.0 as isize, hinst.0 as isize))
    {
        Ok(wv) => WV.with(|c| *c.borrow_mut() = Some(wv)),
        Err(e) => {
            log::error!("创建 WebView2 失败：{e}");
            std::process::exit(1);
        }
    }

    // 角标 hover 轮询定时器（webview 子窗口吃掉了 WM_MOUSEMOVE，只能定时查光标）。
    unsafe {
        SetTimer(Some(hwnd), HOVER_TIMER, HOVER_INTERVAL, None);
    }
    hwnd
}

/// 客户区 → webview 子窗口 Rect：四周内缩 GRIP，把边缘留给父窗口做缩放手柄——
/// 否则鼠标落在 webview 子窗口上，父的 WM_NCHITTEST 收不到。
fn client_rect(hwnd: HWND) -> Rect {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64;
    let scale = dpi / 96.0;
    let g = (GRIP * scale).round() as i32;
    let w = (r.right - r.left - 2 * g).max(0) as u32;
    let h = (r.bottom - r.top - 2 * g).max(0) as u32;
    Rect {
        position: wry::dpi::PhysicalPosition::new(g, g).into(),
        size: wry::dpi::PhysicalSize::new(w, h).into(),
    }
}
fn initial_rect(hwnd: HWND) -> Rect {
    client_rect(hwnd)
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_OPEN => {
            let url = unsafe { *Box::from_raw(lparam.0 as *mut String) };
            WV.with(|c| {
                if let Some(wv) = &*c.borrow() {
                    let _ = wv.load_url(&url);
                    let _ = wv.set_bounds(client_rect(hwnd));
                }
            });
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            LRESULT(0)
        }
        // 父进程已退出（stdin EOF）：销毁窗口 → WM_DESTROY → PostQuitMessage → 泵尽自退。
        WM_APP_QUIT => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        // 无边框窗口：阻止 DWM 在激活/失活时画非客户边框（否则失焦顶部出现白条）。
        WM_NCACTIVATE => LRESULT(1),
        // 去掉原生标题栏/边框：客户区 = 整个窗口。缩放/贴靠靠下面的 WM_NCHITTEST。
        WM_NCCALCSIZE => {
            if wparam.0 != 0 {
                // 返回 0 = 不裁客户区（无边框）。最大化时裁掉超出工作区的部分由系统处理。
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_NCHITTEST => {
            let (sx, sy) = lp_point(lparam);
            let mut pt = POINT { x: sx, y: sy };
            let _ = ScreenToClient(hwnd, &mut pt);
            let mut cr = RECT::default();
            let _ = GetClientRect(hwnd, &mut cr);
            let dpi = GetDpiForWindow(hwnd).max(96) as f64;
            let scale = dpi / 96.0;
            let grip = (GRIP * scale) as i32;
            // 用客户区尺寸（与 pt 同一坐标系）；GetWindowRect 含 WS_THICKFRAME
            // 的不可见边框会偏大，导致右/下边缘永远判不到。
            let w = cr.right - cr.left;
            let h = cr.bottom - cr.top;
            // 边缘环 → 缩放手柄
            let left = pt.x < grip;
            let right = pt.x >= w - grip;
            let top = pt.y < grip;
            let bottom = pt.y >= h - grip;
            let hit = match (left, right, top, bottom) {
                (true, _, true, _) => Some(HTTOPLEFT),
                (_, true, true, _) => Some(HTTOPRIGHT),
                (true, _, _, true) => Some(HTBOTTOMLEFT),
                (_, true, _, true) => Some(HTBOTTOMRIGHT),
                (true, _, _, _) => Some(HTLEFT),
                (_, true, _, _) => Some(HTRIGHT),
                (_, _, true, _) => Some(HTTOP),
                (_, _, _, true) => Some(HTBOTTOM),
                _ => None,
            };
            if let Some(h) = hit {
                LRESULT(h as isize)
            } else {
                LRESULT(HTCLIENT as isize)
            }
        }
        // 手柄环刻意一个像素都不画：返回 1 阻止系统拿类刷擦成不透明，环带保持
        // alpha=0 → 透出桌面（透明外圈，用户确认设计如此——2026-09-09 那次「白圈」
        // 是截图时桌面本来就白，误诊）。WM_PAINT 交给 DefWindowProc 只验证不画。
        WM_ERASEBKGND => LRESULT(1),
        // 窗口一动（拖动/贴靠/最大化）悬浮层必须跟上：owned 弹窗位置不随主窗走。
        WM_MOVE => {
            place_overlays(hwnd);
            LRESULT(0)
        }
        // 无边框窗口最大化默认会盖住任务栏，显式给工作区尺寸。
        WM_GETMINMAXINFO => {
            let mmi = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            unsafe {
                let _ = GetMonitorInfoW(mon, &mut mi);
            }
            let (work, full) = (mi.rcWork, mi.rcMonitor);
            mmi.ptMaxPosition.x = work.left - full.left;
            mmi.ptMaxPosition.y = work.top - full.top;
            mmi.ptMaxSize.x = work.right - work.left;
            mmi.ptMaxSize.y = work.bottom - work.top;
            // 最小尺寸：MIN_TRACK 是逻辑像素，按 DPI 换算成物理值。
            let s = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
            mmi.ptMinTrackSize.x = (MIN_TRACK * s).round() as i32;
            mmi.ptMinTrackSize.y = (MIN_TRACK * s).round() as i32;
            LRESULT(0)
        }
        // 跨显示器拖动（DPI 不同）：按系统建议矩形缩放，内容比例/清晰度才对。
        WM_DPICHANGED => {
            let sug = unsafe { &*(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    sug.left,
                    sug.top,
                    sug.right - sug.left,
                    sug.bottom - sug.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == HOVER_TIMER {
                poll_badges(hwnd);
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_SIZE => {
            let mut cr = RECT::default();
            let _ = GetClientRect(hwnd, &mut cr);
            let scale = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
            let g = (GRIP * scale).round() as i32;
            let cw = (cr.right - cr.left - 2 * g).max(1);
            let chh = (cr.bottom - cr.top - 2 * g).max(1);
            WV.with(|c| {
                if let Some(wv) = &*c.borrow() {
                    let _ = wv.set_bounds(client_rect(hwnd));
                }
            });
            // webview 子窗口套圆角区域让内容四角跟窗口的 DWM 圆角呼应；最大化时
            // 去圆角贴工作区（DWM 侧 DONOTROUND + 清掉子窗口区域）。
            let child = FindWindowExW(Some(hwnd), None, None, None).unwrap_or_default();
            if !child.is_invalid() {
                let maxd = is_maximized(hwnd);
                let pref = if maxd { DWMWCP_DONOTROUND } else { DWMWCP_ROUND };
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &pref as *const _ as *const core::ffi::c_void,
                    std::mem::size_of_val(&pref) as u32,
                );
                if maxd {
                    let _ = SetWindowRgn(child, None, true);
                } else {
                    // 内容圆角半径（原为 16−GRIP=10，用户嫌大改 CONTENT_RADIUS）。
                    let rad = (CONTENT_RADIUS * scale).round() as i32;
                    // SetWindowRgn 会复制形状，HRGN 用完即删（旧代码每次 WM_SIZE 漏一个）。
                    if let Some(rgn) = rounded_region(cw, chh, rad, 16) {
                        let _ = SetWindowRgn(child, Some(rgn), true);
                        // SetWindowRgn 已复制形状；HRGN 用完即删（旧实现每次 WM_SIZE 漏一个）。
                        unsafe {
                            let _ = DeleteObject(HGDIOBJ(rgn.0));
                        }
                    }
                }
            }
            place_overlays(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE); // 拦截关闭按钮/Alt+F4 → 隐藏
            notify_closed();
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 悬浮层目标位置（屏幕坐标）：交通灯贴两上角、横条顶部居中。
fn overlay_rects(main: HWND) -> [(i32, i32, i32, i32); 3] {
    unsafe {
        let mut wr = RECT::default();
        let _ = GetWindowRect(main, &mut wr);
        let scale = GetDpiForWindow(main).max(96) as f64 / 96.0;
        let bw = (BADGE_WIN * scale).round() as i32;
        let pw = (PILL_W * scale).round() as i32;
        let ph = (PILL_HIT * scale).round() as i32;
        let pt = (PILL_TOP * scale).round() as i32;
        let w = wr.right - wr.left;
        [
            (wr.right - bw, wr.top, bw, bw), // 0 右上：关闭
            (wr.left, wr.top, bw, bw),      // 1 左上：最小化
            (wr.left + (w - pw) / 2, wr.top + pt, pw, ph), // 2 顶部居中：横条
        ]
    }
}

/// 内容四角的圆角区域。**不能直接用 `CreateRoundRectRgn`**：它是硬边二值遮罩，
/// 5px 半径的圆弧只有 3~4 个采样点，用户看到的就是阶梯（「倒角像素感很重」）。
/// 这里四角各画 `STEPS` 段圆弧，步长约 0.5px，肉眼平滑——抗锯齿靠细分不靠 alpha
/// （子窗口没透明通道可用，形状只能是 0/1）。
fn rounded_region(w: i32, h: i32, r: i32, steps: u32) -> Option<HRGN> {
    let r = r.max(1).min(w / 2).min(h / 2) as f64;
    // 中心按顺时针：左上→右上→右下→左下。
    let centers = [
        (r, r),
        ((w as f64 - r), r),
        ((w as f64 - r), (h as f64 - r)),
        (r, (h as f64 - r)),
    ];
    let mut pts = Vec::with_capacity(steps as usize + 1);
    for k in 0..steps * 4 {
        // 从左边中点起、顺时针扫一整圈：θ=π 时恰是 (0, r)。
        let theta = std::f64::consts::PI + k as f64 * (std::f64::consts::PI / (2.0 * steps as f64));
        let (cx, cy) = centers[(k / steps) as usize % 4];
        pts.push(POINT {
            x: (cx + r * theta.cos()).round() as i32,
            y: (cy + r * theta.sin()).round() as i32,
        });
    }
    let rgn = unsafe { CreatePolygonRgn(&pts, ALTERNATE) };
    if rgn.is_invalid() {
        None
    } else {
        Some(rgn)
    }
}

/// 只挪窗口不动内容（拖拽模态循环里 WM_MOVE 走这条，30ms 轮询被阻塞也要跟手）。
fn place_overlays(main: HWND) {
    let rects = overlay_rects(main);
    unsafe {
        for (i, b) in BADGES.with(|bs| *bs.borrow()).iter().enumerate() {
            if !b.is_invalid() && IsWindowVisible(*b).as_bool() {
                let (x, y, w, h) = rects[i];
                let _ = SetWindowPos(*b, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        let pill = PILL.get();
        if !pill.is_invalid() && IsWindowVisible(pill).as_bool() {
            let (x, y, w, h) = rects[2];
            let _ = SetWindowPos(pill, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}

/// 30ms 心跳：hover 判定 → 目标 → 动画步进 → ULW 渲染。
fn poll_badges(hwnd: HWND) {
    unsafe {
        let now = now_ms();
        let dt = if LAST_POLL.get() == 0 {
            0.03
        } else {
            now.saturating_sub(LAST_POLL.get()).min(200) as f32 / 1000.0
        };
        LAST_POLL.set(now);

        let mut tgt = [0.0f32; 3]; // 0 关 / 1 最小 / 2 横条
        let mut hov_badge = false;
        let mut hov_pill = false;
        let active = IsWindowVisible(hwnd).as_bool()
            && !IsIconic(hwnd).as_bool()
            && GetForegroundWindow() == hwnd;
        if active {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let mut wr = RECT::default();
            let _ = GetWindowRect(hwnd, &mut wr);
            let scale = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
            let w = wr.right - wr.left;
            let h = wr.bottom - wr.top;
            let px = pt.x - wr.left;
            let py = pt.y - wr.top;
            // 悬在已显示的悬浮层上 → 保持显示（等点或移走）
            let bs = BADGES.with(|b| *b.borrow());
            let hold0 = !bs[0].is_invalid() && IsWindowVisible(bs[0]).as_bool() && in_rect(pt, bs[0]);
            let hold1 = !bs[1].is_invalid() && IsWindowVisible(bs[1]).as_bool() && in_rect(pt, bs[1]);
            let pill = PILL.get();
            let holdp = !pill.is_invalid() && IsWindowVisible(pill).as_bool() && in_rect(pt, pill);
            hov_badge = hold0 || hold1;
            hov_pill = holdp;
            // 触发区：上两角=交通灯；顶部中央带=横条（角落优先，避免和交通灯打架）
            let zone = (ZONE * scale).round() as i32;
            let mut z = -1;
            if px >= 0 && py >= 0 && px < w && py < h {
                let tl = px <= zone && py <= zone;
                let tr = px >= w - zone && py <= zone;
                let in_pill = py <= (PILL_ZONE_H * scale).round() as i32
                    && (px - w / 2).abs() <= (PILL_ZONE_W * scale).round() as i32;
                z = match (tl, tr, in_pill) {
                    (true, false, _) => 1,
                    (false, true, _) => 0,
                    (false, false, true) => 2,
                    _ => -1,
                };
            }
            if z != ZONE_ID.get() {
                ZONE_ID.set(z);
                ZONE_SINCE.set(now);
            }
            let dwell = if z == 2 { PILL_DWELL } else { BADGE_DWELL };
            if z >= 0 && now.saturating_sub(ZONE_SINCE.get()) >= dwell {
                tgt[z as usize] = 1.0;
            }
            if hold0 {
                tgt[0] = 1.0;
            }
            if hold1 {
                tgt[1] = 1.0;
            }
            if holdp {
                tgt[2] = 1.0;
            }
        }

        // 动画步进 + 渲染
        let rects = overlay_rects(hwnd);
        let pinned = PINNED.get();
        let mut ts = BADGE_T.with(|t| *t.borrow());
        let mut sigs = BADGE_SIG.with(|s| *s.borrow());
        for i in 0..2usize {
            let nt = approach(ts[i], tgt[i], dt, if tgt[i] > ts[i] { IN_DUR } else { OUT_DUR});
            ts[i] = nt;
            let b = BADGES.with(|bs| bs.borrow()[i]);
            if b.is_invalid() {
                continue;
            }
            if nt <= 0.003 {
                ts[i] = 0.0;
                if IsWindowVisible(b).as_bool() {
                    let _ = ShowWindow(b, SW_HIDE);
                }
                sigs[i] = (0, 0, 0, 0);
                continue;
            }
            let (x, y, w, h) = rects[i];
            let hovered = hov_badge && in_rect_now(b);
            let sig = (x as i64, y as i64, (nt * 1000.0) as i64, hovered as i64);
            // 签名没变就整段跳过：绝不每 tick 无条件 SetWindowPos（会持续弄脏
            // DWM 合成，盖在 webview 上就是掉帧来源）。
            if sig != sigs[i] {
                sigs[i] = sig;
                let _ = SetWindowPos(b, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
                if !IsWindowVisible(b).as_bool() {
                    let _ = ShowWindow(b, SW_SHOWNA);
                }
                render_badge(b, i, nt, hovered, w as usize, h as usize);
            }
        }
        BADGE_T.with(|t| *t.borrow_mut() = ts);
        BADGE_SIG.with(|s| *s.borrow_mut() = sigs);

        let pill = PILL.get();
        if !pill.is_invalid() {
            let pt_v = PILL_T.get();
            let nt = approach(pt_v, tgt[2], dt, if tgt[2] > pt_v { IN_DUR } else { OUT_DUR });
            PILL_T.set(nt);
            if nt <= 0.003 {
                PILL_T.set(0.0);
                if IsWindowVisible(pill).as_bool() {
                    let _ = ShowWindow(pill, SW_HIDE);
                }
                PILL_SIG.set((0, 0, 0, 0, 0));
            } else {
                let (x, y, w, h) = rects[2];
                let sig = (x as i64, y as i64, (nt * 1000.0) as i64, hov_pill as i64, pinned as i64);
                if sig != PILL_SIG.get() {
                    PILL_SIG.set(sig);
                    let _ = SetWindowPos(pill, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
                    if !IsWindowVisible(pill).as_bool() {
                        let _ = ShowWindow(pill, SW_SHOWNA);
                    }
                    render_pill(pill, nt, hov_pill, pinned, w as usize, h as usize);
                }
            }
        }
    }
}

fn in_rect_now(b: HWND) -> bool {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        in_rect(pt, b)
    }
}

fn approach(cur: f32, target: f32, dt: f32, dur: f32) -> f32 {
    let step = dt / dur;
    if target >= cur {
        (cur + step).min(target)
    } else {
        (cur - step).max(target)
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// 烘一版角标底图（满 alpha、无滑入）：L 形两笔胶囊（圆头端帽，肘点自然汇成
/// 倒角）+ 同色外发光。SDF 的指数衰减只在 (dpi,尺寸,变体) 首次出现时跑一次。
fn badge_master(dpi: u32, w: usize, i: usize, hov: usize) -> Vec<u8> {
    let scale = dpi.max(96) as f64 / 96.0;
    let mut img = Img::new(w, w);
    let r = (THK * scale / 2.0) as f32;
    let arm = (ARM * scale) as f32;
    let ins = (INSET * scale) as f32;
    let col = if i == 0 { CLOSE_RGB } else { MIN_RGB };
    // 肘点：右上贴角内 (w-ins, ins)，横臂向左；左上 (ins, ins)，横臂向右。竖臂向下。
    let (ex, dx) = if i == 0 { (w as f32 - ins, -1.0f32) } else { (ins, 1.0) };
    let (a, glow) = if hov == 1 { (0.95f32, 0.42f32) } else { (0.72f32, 0.28f32) };
    let soft = 3.0 * scale as f32;
    img.glow_segment(ex, ins, ex + dx * arm, ins, r, soft, col, glow);
    img.glow_segment(ex, ins, ex, ins + arm, r, soft, col, glow);
    img.segment(ex, ins, ex + dx * arm, ins, r, col, a);
    img.segment(ex, ins, ex, ins + arm, r, col, a);
    img.to_bgra()
}

/// 渲染角标 = 取贴图 + 逐字节乘淡入系数；滑入走 pptDst 偏移（不重画）。
fn render_badge(hwnd: HWND, i: usize, t: f32, hovered: bool, w: usize, h: usize) {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let scale = dpi as f64 / 96.0;
    BADGE_MASTER.with(|c| {
        let mut g = c.borrow_mut();
        if !matches!(&*g, Some((d, s, _)) if *d == dpi && *s == w) {
            *g = Some((
                dpi,
                w,
                [
                    [badge_master(dpi, w, 0, 0), badge_master(dpi, w, 0, 1)],
                    [badge_master(dpi, w, 1, 0), badge_master(dpi, w, 1, 1)],
                ],
            ));
        }
    });
    let e = (ease_out(t.clamp(0.0, 1.0)) * 255.0) as u32;
    BADGE_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        BADGE_MASTER.with(|c| {
            let g = c.borrow();
            let (_, _, m) = g.as_ref().unwrap();
            let src = &m[i][hovered as usize];
            buf.resize(src.len(), 0);
            for (o, v) in buf.iter_mut().zip(src.iter()) {
                *o = (*v as u32 * e / 255) as u8; // 预乘 alpha 直接整体缩放
            }
        });
        let slide = -(((1.0 - ease_out(t)) * 4.0 * scale as f32).round() as i32);
        push_overlay_bgra(hwnd, &buf, w as i32, h as i32, 0, slide);
    });
}

/// 渲染顶部横条：半透明胶囊，滑入（从上方 8px）+ 淡入；悬停更亮、置顶变蓝。
/// 烘一版横条底图（满 alpha、无滑入）：和角标同款的胶囊笔画 + 同色外发光，
/// 统一品牌蓝；置顶比悬停更亮、发光更强，**另铺一圈深蓝内描边**（先画更粗的
/// 深蓝胶囊再叠正常内胆，露出的环就是描边）——单靠亮度/发光区分不出置顶态
/// （2026-09-10 用户反馈），要一眼可见的形状差异。
fn pill_master(dpi: u32, w: usize, h: usize, hov: usize, pinned: usize) -> Vec<u8> {
    let scale = dpi.max(96) as f64 / 96.0;
    let mut img = Img::new(w, h);
    let r = (PILL_H * scale / 2.0) as f32;
    let cy = h as f32 / 2.0;
    let (a, glow) = match (hov, pinned) {
        (0, 0) => (0.72f32, 0.22f32),
        (1, 0) => (0.95, 0.32),
        (0, 1) => (0.85, 0.28),
        _ => (1.0, 0.38),
    };
    // 发光半径原先 soft=3（可见晕约 9px）把 overlay 左右裁成平头；收窄并给
    // 圆头+光晕留 pad，胶囊不再贴 overlay 窗口边。
    let soft = 1.35 * scale as f32;
    let ax = r + soft * 3.0 + 1.0;
    let bx = (w as f32 - ax).max(ax + 1.0);
    img.glow_segment(ax, cy, bx, cy, r, soft, PIN_RGB, glow);
    if pinned != 0 {
        // 深蓝内描边：先铺满一圈深蓝底胶囊（#1B2A8F 级别），正常内胆半径缩进
        // 2px 叠上去，四周留出的深蓝环就是状态标记（往外加会超出悬浮窗被裁）。
        let outline = [0.106, 0.165, 0.561];
        let t = 2.0 * scale as f32;
        img.segment(ax, cy, bx, cy, r, outline, 0.95);
        img.segment(ax, cy, bx, cy, (r - t).max(1.0), PIN_RGB, a);
    } else {
        img.segment(ax, cy, bx, cy, r, PIN_RGB, a);
    }
    img.to_bgra()
}

/// 渲染横条 = 取贴图 + 逐字节乘淡入系数；滑入走 pptDst 偏移（同角标）。
fn render_pill(hwnd: HWND, t: f32, hovered: bool, pinned: bool, w: usize, h: usize) {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let scale = dpi as f64 / 96.0;
    PILL_MASTER.with(|c| {
        let mut g = c.borrow_mut();
        if !matches!(&*g, Some((d, mw, mh, _)) if *d == dpi && *mw == w && *mh == h) {
            *g = Some((
                dpi,
                w,
                h,
                [
                    [pill_master(dpi, w, h, 0, 0), pill_master(dpi, w, h, 0, 1)],
                    [pill_master(dpi, w, h, 1, 0), pill_master(dpi, w, h, 1, 1)],
                ],
            ));
        }
    });
    let e = (ease_out(t.clamp(0.0, 1.0)) * 255.0) as u32;
    PILL_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        PILL_MASTER.with(|c| {
            let g = c.borrow();
            let (_, _, _, m) = g.as_ref().unwrap();
            let src = &m[hovered as usize][pinned as usize];
            buf.resize(src.len(), 0);
            for (o, v) in buf.iter_mut().zip(src.iter()) {
                *o = (*v as u32 * e / 255) as u8;
            }
        });
        let slide = -(((1.0 - ease_out(t)) * 8.0 * scale as f32).round() as i32);
        push_overlay_bgra(hwnd, &buf, w as i32, h as i32, 0, slide);
    });
}

/// 预乘 RGBA 浮点小图 + SDF 抗锯齿画笔（圆 / 胶囊线段），逐像素 src-over。
struct Img {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Img {
    fn new(w: usize, h: usize) -> Self {
        Img { w, h, px: vec![[0.0; 4]; w * h] }
    }
    fn blend(&mut self, x: usize, y: usize, col: [f32; 3], a: f32) {
        if a <= 0.0 {
            return;
        }
        let sa = a.min(1.0);
        let p = &mut self.px[y * self.w + x];
        p[0] = col[0] * sa + p[0] * (1.0 - sa);
        p[1] = col[1] * sa + p[1] * (1.0 - sa);
        p[2] = col[2] * sa + p[2] * (1.0 - sa);
        p[3] = sa + p[3] * (1.0 - sa);
    }
    /// 线段外发光：形状外按距离指数衰减（soft=衰减半径），内部全亮。
    fn glow_segment(
        &mut self,
        ax: f32,
        ay: f32,
        bx: f32,
        by: f32,
        r: f32,
        soft: f32,
        col: [f32; 3],
        a: f32,
    ) {
        if a <= 0.0 {
            return;
        }
        let m = r + soft * 3.0;
        let x0 = (ax.min(bx) - m).max(0.0) as usize;
        let x1 = (ax.max(bx) + m).min(self.w as f32 - 1.0) as usize;
        let y0 = (ay.min(by) - m).max(0.0) as usize;
        let y1 = (ay.max(by) + m).min(self.h as f32 - 1.0) as usize;
        let vx = bx - ax;
        let vy = by - ay;
        let l2 = (vx * vx + vy * vy).max(1e-6);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let px = x as f32 + 0.5 - ax;
                let py = y as f32 + 0.5 - ay;
                let tt = ((px * vx + py * vy) / l2).clamp(0.0, 1.0);
                let dx = px - tt * vx;
                let dy = py - tt * vy;
                let d = (dx * dx + dy * dy).sqrt() - r;
                let g = if d <= 0.0 { 1.0 } else { (-d / soft).exp() };
                self.blend(x, y, col, a * g);
            }
        }
    }
    fn segment(&mut self, ax: f32, ay: f32, bx: f32, by: f32, r: f32, col: [f32; 3], a: f32) {
        if a <= 0.0 {
            return;
        }
        let x0 = (ax.min(bx) - r - 1.0).max(0.0) as usize;
        let x1 = (ax.max(bx) + r + 1.0).min(self.w as f32 - 1.0) as usize;
        let y0 = (ay.min(by) - r - 1.0).max(0.0) as usize;
        let y1 = (ay.max(by) + r + 1.0).min(self.h as f32 - 1.0) as usize;
        let vx = bx - ax;
        let vy = by - ay;
        let l2 = (vx * vx + vy * vy).max(1e-6);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let px = x as f32 + 0.5 - ax;
                let py = y as f32 + 0.5 - ay;
                let tt = ((px * vx + py * vy) / l2).clamp(0.0, 1.0);
                let dx = px - tt * vx;
                let dy = py - tt * vy;
                let d = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, col, a * cov);
                }
            }
        }
    }
    fn to_bgra(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.w * self.h * 4);
        for p in &self.px {
            v.push((p[2] * 255.0).round().clamp(0.0, 255.0) as u8);
            v.push((p[1] * 255.0).round().clamp(0.0, 255.0) as u8);
            v.push((p[0] * 255.0).round().clamp(0.0, 255.0) as u8);
            v.push((p[3] * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        v
    }
}

/// 把预乘 BGRA 缓冲经 UpdateLayeredWindow 贴到悬浮层窗口。当前窗口位置 +
/// (dx,dy) 作为目标点——淡入滑入借这个偏移做，不用重画像素。
fn push_overlay_bgra(hwnd: HWND, buf: &[u8], w: i32, h: i32, dx: i32, dy: i32) {
    unsafe {
        let mut wr = RECT::default();
        let _ = GetWindowRect(hwnd, &mut wr);
        let dst = POINT { x: wr.left + dx, y: wr.top + dy };
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // 负值 = 自上而下
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        if let Ok(bmp) = CreateDIBSection(Some(screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            if !bits.is_null() {
                std::ptr::copy_nonoverlapping(buf.as_ptr(), bits as *mut u8, buf.len().min(w as usize * h as usize * 4));
                let old = SelectObject(mem, bmp.into());
                let size = SIZE { cx: w, cy: h };
                let src = POINT { x: 0, y: 0 };
                let bf = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let _ = UpdateLayeredWindow(
                    hwnd,
                    Some(screen),
                    Some(&dst),
                    Some(&size),
                    Some(mem),
                    Some(&src),
                    COLORREF(0),
                    Some(&bf),
                    ULW_ALPHA,
                );
                SelectObject(mem, old);
                let _ = DeleteObject(bmp.into());
            }
        }
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(None, screen);
    }
}

/// 角标可点框（悬浮层窗口内坐标）：罩住 L 形两臂的方形带；外缘收在环带之后
/// （GRIP+1 以内全穿透），角落缩放才不被挡。
fn badge_box(i: usize, bw: i32, scale: f64) -> (i32, i32, i32, i32) {
    let lo = ((GRIP + 1.0) * scale) as i32; // 外缘：贴着窗口角的一圈留给缩放
    let hi = ((INSET + ARM + 4.0) * scale) as i32; // 内缘：盖到臂端外 4px
    let (x0, x1) = if i == 0 { (bw - hi, bw - lo) } else { (lo, hi) };
    (x0, lo, x1, hi)
}

fn in_badge_box(hwnd: HWND, i: usize, pt: POINT) -> bool {
    unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        let s = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
        let (x0, y0, x1, y1) = badge_box(i, r.right - r.left, s);
        let (lx, ly) = (pt.x - r.left, pt.y - r.top);
        lx >= x0 && lx <= x1 && ly >= y0 && ly <= y1
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "system" fn badge_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // 点角标不抢激活（webview 里正输入着别把焦点弄丢）。
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        // 只有 L 形本体那块可点；框外一律 HTTRANSPARENT 穿透给下层（主窗口
        // 边缘缩放热区/内容），不然悬浮层会把角落的拖边动作整个吃掉。
        WM_NCHITTEST => {
            let id = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize;
            let pt = POINT {
                x: (lparam.0 & 0xFFFF) as u16 as i16 as i32,
                y: ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
            };
            LRESULT(if in_badge_box(hwnd, id, pt) { HTCLIENT as isize } else { HTTRANSPARENT as isize })
        }
        WM_LBUTTONDOWN => {
            // 抬起时再判范围：按下就 capture，框外抬起只取消不算点击。
            let _ = SetCapture(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            let id = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as usize;
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            if !in_badge_box(hwnd, id, pt) {
                return LRESULT(0);
            }
            let main = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
            match id {
                0 => {
                    let _ = ShowWindow(main, SW_HIDE); // 关闭=隐藏复用
                    notify_closed();
                }
                _ => {
                    let _ = ShowWindow(main, SW_MINIMIZE);
                }
            }
            // 立刻收掉这个角标（动画清零，下轮询重算）
            BADGE_T.with(|t| t.borrow_mut()[id.min(1)] = 0.0);
            BADGE_SIG.with(|s| s.borrow_mut()[id.min(1)] = (0, 0, 0, 0));
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "system" fn pill_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let main = unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or_default();
    match msg {
        // 点横条不抢激活。
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        // 同角标：贴着窗口顶边的 GRIP 环带穿透给主窗口做缩放，其余可点。
        WM_NCHITTEST => {
            let pt = POINT {
                x: (lparam.0 & 0xFFFF) as u16 as i16 as i32,
                y: ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
            };
            let mut r = RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let s = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
            let lo = ((GRIP + 1.0) * s) as i32;
            let hit = pt.y >= r.top + lo && pt.y < r.bottom && pt.x >= r.left && pt.x < r.right;
            LRESULT(if hit { HTCLIENT as isize } else { HTTRANSPARENT as isize })
        }
        WM_LBUTTONDOWN => {
            let _ = SetCapture(hwnd);
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            PILL_DOWN.set((pt.x, pt.y));
            PILL_DRAG.set(false);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            // 按下后拖出阈值 → 转系统模态移动循环（阻塞到松手；期间主窗 WM_MOVE
            // 持续来，横条/交通灯跟着重摆）。
            if GetCapture() == hwnd && (wparam.0 & 0x0001) != 0 && !PILL_DRAG.get() {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let (dx, dy) = PILL_DOWN.get();
                let thr = (4.0 * GetDpiForWindow(hwnd).max(96) as f64 / 96.0) as i32;
                if (pt.x - dx).abs() + (pt.y - dy).abs() > thr {
                    PILL_DRAG.set(true);
                    PILL_T.set(1.0); // 拖拽中保持全亮
                    PILL_SIG.set((0, 0, 0, 0, 0));
                    let _ = ReleaseCapture();
                    SendMessageW(
                        main,
                        WM_NCLBUTTONDOWN,
                        Some(WPARAM(HTCAPTION as usize)),
                        Some(LPARAM(0)),
                    );
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            if PILL_DRAG.get() {
                PILL_DRAG.set(false); // 拖完松手：不吃单击
            } else if PILL_DBL.get() {
                PILL_DBL.set(false); // 双击的第二个 UP：不吃单击
            } else {
                // 可能是单击，等一个双击间隔；期间没等到双击才执行「锁置顶」
                SetTimer(Some(hwnd), PILL_CLICK_TIMER, GetDoubleClickTime().max(200), None);
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            let _ = KillTimer(Some(hwnd), PILL_CLICK_TIMER);
            PILL_DBL.set(true);
            let _ = ShowWindow(main, if is_maximized(main) { SW_RESTORE } else { SW_MAXIMIZE });
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == PILL_CLICK_TIMER {
                let _ = KillTimer(Some(hwnd), PILL_CLICK_TIMER);
                toggle_pin(main);
                PILL_SIG.set((0, 0, 0, 0, 0)); // 强制重渲染（颜色变了）
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// 切置顶：HWND_TOPMOST ↔ HWND_NOTOPMOST。
fn toggle_pin(main: HWND) {
    let pinned = PINNED.with(|p| {
        let new = !p.get();
        p.set(new);
        new
    });
    let after = if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST };
    unsafe {
        let _ = SetWindowPos(main, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

fn is_maximized(hwnd: HWND) -> bool {
    unsafe { IsZoomed(hwnd).as_bool() }
}

/// WM_NCHITTEST 的 lparam 解屏幕坐标（带符号扩展）。
fn lp_point(lparam: LPARAM) -> (i32, i32) {
    let v = lparam.0 as u32;
    let x = (v as i16) as i32;
    let y = ((v >> 16) as i16) as i32;
    (x, y)
}

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
struct HwndWindow(isize, isize);

/// 从内嵌 .ico 解析最大图 → HICON。
fn load_dsh_icon() -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, LR_DEFAULTSIZE};
    let b = DSH_ICO;
    if b.len() < 6 || u16::from_le_bytes([b[0], b[1]]) != 1 {
        return None;
    }
    let count = u16::from_le_bytes([b[4], b[5]]) as usize;
    let mut best: Option<(u64, usize, usize)> = None;
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
    unsafe { CreateIconFromResourceEx(&b[off..off + len], true, 0x0003_0000, 0, 0, LR_DEFAULTSIZE).ok() }
}

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
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::parse_url_file;

    #[test]
    fn url_file_picks_first_http_line() {
        assert_eq!(
            parse_url_file("http://127.0.0.1:3080/?token=abc\n"),
            Some("http://127.0.0.1:3080/?token=abc".into())
        );
        // 前导空行容忍；只认 http(s) 开头的行。
        assert_eq!(
            parse_url_file("\n  https://x/y  \n垃圾行"),
            Some("https://x/y".into())
        );
        assert_eq!(parse_url_file(""), None);
        assert_eq!(parse_url_file("not a url"), None);
        // http 前缀必须带协议分隔符，防住 "httpfoo" 这类误判。
        assert_eq!(parse_url_file("httpfoo://x"), None);
    }
}
