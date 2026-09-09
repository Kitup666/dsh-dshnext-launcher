//! Windows 平台杂项：开机自启（Run 键）、系统主题探测。
//!
//! 走 `reg` 命令而不是注册表 API：零依赖纪律（不引 winreg），这些操作一天
//! 最多执行几次，子进程开销可忽略。`CREATE_NO_WINDOW` 防止 cmd 闪黑框
//! （与 procman 同款标志）。

use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Dshnext";
const THEME_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

fn reg(args: &[&str]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let out = Command::new("reg")
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// 写/清开机自启。清一个不存在的值不算错（reg delete 返回非零是常态）。
pub fn set_autostart(on: bool) -> Result<(), String> {
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        reg(&[
            "add",
            RUN_KEY,
            "/v",
            RUN_VALUE,
            "/t",
            "REG_SZ",
            "/d",
            // 开机自启静默进托盘（--minimized），不弹主窗挡用户干活。
            &format!("\"{}\" --minimized", exe.display()),
            "/f",
        ])
        .map(|_| ())
    } else {
        reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]).map(|_| ()).or(Ok(()))
    }
}

/// 回读 Run 键的真实状态——UI 显示以它为准，不信 config（用户可能手动删过）。
pub fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", RUN_VALUE])
        .map(|s| s.contains(RUN_VALUE))
        .unwrap_or(false)
}

/// 系统偏好浅色主题？（设置 → 个性化 → 颜色）。读不到（被策略禁用等）按深色算。
pub fn system_prefers_light() -> bool {
    reg(&["query", THEME_KEY, "/v", "AppsUseLightTheme"])
        .ok()
        .is_some_and(|s| s.contains("0x1"))
}

/// 人类可读的系统名（ProductName + DisplayVersion），诊断报告用。
pub fn os_pretty() -> String {
    let cv = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let val = |v: &str| {
        reg(&["query", cv, "/v", v])
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.contains(v))
                    .and_then(|l| l.split_whitespace().next_back().map(String::from))
            })
            .unwrap_or_default()
    };
    format!("{} {}", val("ProductName"), val("DisplayVersion"))
        .trim()
        .to_string()
}

/// 端口探测结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortProbe {    /// 没人监听，可以启动。
    Free,
    /// 有 HTTP 应答——端口上跑着活 web 服务（很可能是用户自己起的 dsh）。
    Http,
    /// 被占但不应答 HTTP（半死进程或非 web 程序）。
    Other,
}

/// 区分「端口空闲 / 活 web 服务 / 其他占用」。发一个最小 HTTP 请求看应答头，
/// 800ms 读超时兜底；连不上即时返回 Free（localhost 对关闭端口连接秒拒）。
pub fn probe_port(port: u16) -> PortProbe {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return PortProbe::Free;
    };
    let _ = s.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n");
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    let mut head = [0u8; 12];
    match s.read(&mut head) {
        Ok(n) if n >= 5 && head[..5].eq_ignore_ascii_case(b"HTTP/") => PortProbe::Http,
        _ => PortProbe::Other,
    }
}

/// 从 `netstat -ano` 文本里找监听 `port` 的属主 PID。
/// 行形如 `TCP  127.0.0.1:3080  0.0.0.0:0  LISTENING  12345`——列序是
/// 协议/本地/远端/状态/PID，状态在 f[3]（写成 f[2] 永远匹配不上，
/// 2026-09-09 实测踩过，测试焊死）。
fn parse_netstat_pid(text: &str, port: u16) -> Option<u32> {
    let suffix = format!(":{port}");
    text.lines().find_map(|line| {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 5 && f[0] == "TCP" && f[3] == "LISTENING" && f[1].ends_with(&suffix) {
            f[4].parse().ok()
        } else {
            None
        }
    })
}

/// 结束监听 `port` 的进程树（上次崩溃/退出残留的 dsh node 树），返回被杀的 PID。
///
/// 用 `netstat -ano` 找 LISTENING 行的属主 PID，再 `taskkill /T /F` 杀整棵树——
/// 与 procman::stop 同款手段，零新依赖。找不到属主返回 None（可能刚好已退出）。
pub fn kill_port_owner(port: u16) -> Option<u32> {
    use std::os::windows::process::CommandExt;
    let out = Command::new("netstat")
        .args(["-ano", "-p", "TCP"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let pid = parse_netstat_pid(&text, port)?;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000)
        .output();
    Some(pid)
}

#[cfg(test)]
mod tests {
    use super::parse_netstat_pid;

    #[test]
    fn netstat_pid_picks_listening_owner() {
        let text = "
  Proto  Local Address          Foreign Address        State           PID
  TCP    127.0.0.1:3080         0.0.0.0:0              LISTENING       18620
  TCP    127.0.0.1:3080         127.0.0.1:52111        ESTABLISHED     18620
  TCP    127.0.0.1:52111        127.0.0.1:3080         ESTABLISHED     400
  TCP    [::]:135               [::]:0                 LISTENING       900
";
        assert_eq!(parse_netstat_pid(text, 3080), Some(18620));
        assert_eq!(parse_netstat_pid(text, 135), Some(900));
        assert_eq!(parse_netstat_pid(text, 3081), None);
        // 端口号后缀不能误配（13080 不该命中 :3080）
        assert_eq!(parse_netstat_pid("  TCP    0.0.0.0:13080    0.0.0.0:0    LISTENING    7\n", 3080), None);
    }
}
