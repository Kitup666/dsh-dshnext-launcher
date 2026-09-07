//! 桌面窗口模式：用已装浏览器的 `--app` 启动参数把 WebUI 开成无地址栏、
//! 无标签栏的独立窗口。**不是套壳**——启动器不嵌内核、零新增依赖，只是
//! 换一种方式喊起浏览器；窗口进程全部记在浏览器名下，启动器仍单进程。
//!
//! 只有 Chromium 系（Chrome/Edge/Brave/Vivaldi/Opera/Arc）懂 `--app`，
//! Firefox 没有这个模式。解析顺序：
//!
//!   1. 默认浏览器（HKCU `UrlAssociations\http\UserChoice` → ProgId →
//!      `shell\open\command` 命令模板抽 exe）
//!   2. 是 Chromium 系就用它；不是（Firefox 等）静默改用 Edge
//!   3. Edge 兜底（HKLM `App Paths\msedge.exe`，Win10/11 必装）
//!   4. 都不行 → Err，上层回退系统浏览器标签页
//!
//! 与 platform.rs 同款纪律：`reg` 子进程读注册表，不引 winreg。

use std::path::PathBuf;
use std::process::Command;

/// `--app` 支持者名单（exe 文件名，小写）。
const CHROMIUMS: &[&str] = &[
    "chrome.exe",
    "msedge.exe",
    "brave.exe",
    "vivaldi.exe",
    "opera.exe",
    "arc.exe",
];

fn reg(args: &[&str]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let out = Command::new("reg")
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(oem_to_string(&out.stdout))
    } else {
        Err(oem_to_string(&out.stderr))
    }
}

/// reg.exe 管道输出按系统 OEM 代码页编码（中文系统 = GBK 936），不是 UTF-8——
/// 值名「(默认)」和含中文的安装路径都会带非 ASCII 字节，`from_utf8_lossy`
/// 会把路径搅成替换符。UTF-8 先试（ASCII 与启用 UTF-8 代码页的系统走快路），
/// 失败再走 MultiByteToWideChar（kernel32，仓库零依赖纪律下的手写 FFI）。
fn oem_to_string(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    const CP_OEMCP: u32 = 1;
    unsafe extern "system" {
        fn MultiByteToWideChar(
            cp: u32,
            flags: u32,
            src: *const u8,
            srclen: i32,
            dst: *mut u16,
            dstlen: i32,
        ) -> i32;
    }
    let len = unsafe {
        MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), bytes.len() as i32, std::ptr::null_mut(), 0)
    };
    if len <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut buf = vec![0u16; len as usize];
    let n = unsafe {
        MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), bytes.len() as i32, buf.as_mut_ptr(), len)
    };
    if n <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

/// reg query 输出里取 REG_SZ 值（REG_SZ 之后的那段，路径含空格要拼回来）。
/// **不按值名匹配**：值名随系统语言变（英文 `(Default)` / 中文「(默认)」），
/// `/ve`、`/v ProgId` 的输出里 REG_SZ 行各只有一行，按行找才稳。
pub fn reg_sz_value(out: &str) -> Option<String> {
    out.lines()
        .find(|l| l.contains("REG_SZ"))
        .map(|l| {
            l.split_whitespace()
                .skip_while(|t| !t.contains("REG_SZ"))
                .skip(1)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty())
}

/// exe 文件名（小写）是否在 Chromium 系名单里。
pub fn is_chromium(exe: &PathBuf) -> bool {
    exe.file_name()
        .and_then(|n| n.to_str())
        .map(|n| CHROMIUMS.contains(&n.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// 从 `shell\open\command` 的命令模板里抽出真实 exe 路径。模板形态：
///
///   `"C:\...\chrome.exe" --single-argument %1`   （带引号，Chrome/Edge 官方形态）
///   `C:\Program Files\...\brave.exe --single-argument %1`（无引号，路径含空格）
///   `"C:\...\firefox.exe" -osint -url "%1"`      （Firefox，抽出来也白搭）
///
/// 带引号取第一对引号之间；无引号把 token 往后拼、停在第一个开关（`-`/`%`）
/// 上——路径带空格就靠这个拼回来。剩下的 `%1`、`-osint` 尾巴全不要：我们直起
/// 进程自己传参，不用模板。
pub fn extract_exe_from_cmd(cmd: &str) -> Option<PathBuf> {
    let s = cmd.trim();
    if s.is_empty() {
        return None;
    }
    let exe = if let Some(rest) = s.strip_prefix('"') {
        // 带引号：引号对之间是 exe；模板里引号内不会出现 `"`
        let end = rest.find('"')?;
        rest[..end].to_string()
    } else {
        // 无引号：拼 token 直到遇到开关
        let mut acc = String::new();
        for tok in s.split_whitespace() {
            if tok.starts_with('-') || tok.starts_with('%') {
                break;
            }
            if !acc.is_empty() {
                acc.push(' ');
            }
            acc.push_str(tok);
        }
        acc
    };
    let exe = PathBuf::from(exe);
    // spawn 前必须能绝对定位；顺带挡掉空串
    (exe.is_absolute()).then_some(exe)
}

/// 从 reg 输出里按 ProgId 解析默认浏览器 exe（http 优先，https 兜底）。
fn default_browser_exe() -> Option<PathBuf> {
    let user_choice = |scheme: &str| {
        reg(&[
            "query",
            &format!(r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\{scheme}\UserChoice"),
            "/v",
            "ProgId",
        ])
        .ok()
        .and_then(|s| reg_sz_value(&s))
    };
    let prog_id = user_choice("http").or_else(|| user_choice("https"))?;
    let cmd = reg(&[
        "query",
        &format!(r"HKEY_CLASSES_ROOT\{prog_id}\shell\open\command"),
        "/ve",
    ])
    .ok()
    .and_then(|s| reg_sz_value(&s))
    .or_else(|| {
        reg(&[
            "query",
            &format!(r"HKCU\Software\Classes\{prog_id}\shell\open\command"),
            "/ve",
        ])
        .ok()
        .and_then(|s| reg_sz_value(&s))
    })?;
    extract_exe_from_cmd(&cmd)
}

/// Edge 兜底：HKLM `App Paths\msedge.exe`（Win10/11 必装）。
fn edge_fallback() -> Option<PathBuf> {
    let out = reg(&[
        "query",
        r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\msedge.exe",
        "/ve",
    ])
    .ok()?;
    let p = reg_sz_value(&out)?;
    let p = if PathBuf::from(&p).is_absolute() {
        PathBuf::from(p)
    } else {
        // App Paths 偶有存裸文件名的（依赖 PATH 查找），没法定位就不赌
        return None;
    };
    Some(p)
}

/// 解析桌面窗口该用的浏览器：默认浏览器是 Chromium 系就跟它，不是/读不到
/// 则退 Edge。全部失败返回 None。
pub fn resolve_chromium() -> Option<PathBuf> {
    match default_browser_exe() {
        Some(exe) if is_chromium(&exe) => Some(exe),
        // Firefox 等不认识 --app 的：静默改用 Edge，谁也不打扰
        Some(_) => {
            log::debug!("默认浏览器不支持 --app，回退 Edge");
            edge_fallback()
        }
        None => edge_fallback(),
    }
}

/// 以桌面窗口打开 `url`（带 token 的 WebUI 地址）。直起浏览器进程不经
/// shell，token 作为单个参数传递，没有转义问题。成功返回 Ok(url)。
/// 解析不到可用浏览器或 spawn 失败 → Err(url)，上层回退系统浏览器。
pub fn open(url: String) -> Result<String, String> {
    let Some(exe) = resolve_chromium() else {
        return Err(url);
    };
    let spawn = || {
        use std::os::windows::process::CommandExt;
        Command::new(&exe)
            .arg(format!("--app={url}"))
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW，spawn 不弹黑框
            .spawn()
    };
    match spawn() {
        Ok(_) => Ok(url),
        Err(e) => {
            log::warn!("桌面窗口启动失败（{}）：{e}", exe.display());
            Err(url)
        }
    }
}
