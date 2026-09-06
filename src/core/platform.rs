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
            &format!("\"{}\"", exe.display()),
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
