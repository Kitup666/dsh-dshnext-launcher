use crate::envres::{child_path, dsh_cmd, home_dir};
use crate::store::Config;
use serde::Serialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

pub struct ProcMeta {
    pub pid: u32,
    pub profile: String,
    pub port: u16,
    pub started_at: i64,
    pub url: String,
}

#[derive(Default)]
pub struct ProcMap(pub tokio::sync::Mutex<HashMap<String, ProcMeta>>);

#[derive(Debug, Clone, Serialize)]
pub struct ProcStatus {
    pub profile: String,
    pub port: u16,
    pub url: String,
    pub pid: u32,
    pub uptime_secs: i64,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub async fn status(map: &ProcMap) -> Vec<ProcStatus> {
    let guard = map.0.lock().await;
    let now = now_millis();
    guard
        .values()
        .map(|m| ProcStatus {
            profile: m.profile.clone(),
            port: m.port,
            url: m.url.clone(),
            pid: m.pid,
            uptime_secs: (now - m.started_at) / 1000,
        })
        .collect()
}

pub async fn is_running(map: &ProcMap, profile: &str) -> bool {
    map.0.lock().await.contains_key(profile)
}

/// 启动 `dsh --profile <name> --port <port> --no-open`，转发日志并解析 WebUI 地址
pub async fn start(
    app: AppHandle,
    map: &ProcMap,
    cfg: &Config,
    profile: &str,
    port: u16,
) -> Result<String, String> {
    {
        let guard = map.0.lock().await;
        if guard.contains_key(profile) {
            return Err(format!("版本 {profile} 正在运行"));
        }
    }
    let dsh = dsh_cmd();
    if !dsh.exists() {
        return Err("dsh 未安装，请先到「环境」页安装".into());
    }
    let cwd = home_dir().join("profiles").join(profile);
    if !cwd.exists() {
        return Err(format!("版本 {profile} 目录不存在"));
    }

    let mut cmd = Command::new("cmd");
    cmd.args([
        "/C",
        &dsh.to_string_lossy(),
        "--profile",
        profile,
        "--port",
        &port.to_string(),
        "--no-open",
    ])
    .current_dir(&cwd)
    .env("PATH", child_path())
    .env("DSH_HOME", home_dir())
    .creation_flags(0x0800_0000);
    if !cfg.api_key.trim().is_empty() {
        cmd.env("DEEPSEEK_API_KEY", cfg.api_key.trim());
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(false);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动 dsh 失败: {e}"))?;
    let pid = child.id().unwrap_or(0);
    let url = format!("http://127.0.0.1:{port}");
    let started_at = now_millis();

    map.0.lock().await.insert(
        profile.to_string(),
        ProcMeta {
            pid,
            profile: profile.to_string(),
            port,
            started_at,
            url: url.clone(),
        },
    );

    // stdout 转发 + 解析 WebUI 地址
    let app2 = app.clone();
    let profile2 = profile.to_string();
    if let Some(mut out) = child.stdout.take() {
        tauri::async_runtime::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(&mut out);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(pos) = line.find("http://") {
                    let rest = &line[pos..];
                    let end = rest
                        .find(|c: char| c.is_whitespace())
                        .unwrap_or(rest.len());
                    let found = rest[..end].to_string();
                    let _ = app2.emit(
                        "dsh-url",
                        serde_json::json!({ "profile": profile2, "url": found }),
                    );
                }
                let _ = app2.emit(
                    "dsh-log",
                    serde_json::json!({ "profile": profile2, "stream": "stdout", "line": line, "ts": now_millis() }),
                );
            }
        });
    }
    // stderr 转发
    let app3 = app.clone();
    let profile3 = profile.to_string();
    if let Some(mut err) = child.stderr.take() {
        tauri::async_runtime::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(&mut err);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app3.emit(
                    "dsh-log",
                    serde_json::json!({ "profile": profile3, "stream": "stderr", "line": line, "ts": now_millis() }),
                );
            }
        });
    }
    // 退出监视：通知前端并关掉该 profile 的 WebUI 窗口
    let app4 = app.clone();
    let profile4 = profile.to_string();
    tauri::async_runtime::spawn(async move {
        let code = match child.wait().await {
            Ok(s) => s.code().unwrap_or(-1),
            Err(_) => -1,
        };
        use tauri::Manager;
        if let Some(w) = app4.get_webview_window(&format!("webui-{profile4}")) {
            let _ = w.close();
        }
        let _ = app4.emit(
            "dsh-exit",
            serde_json::json!({ "profile": profile4, "code": code }),
        );
    });

    Ok(url)
}

/// taskkill 杀整棵进程树
pub async fn stop(app: AppHandle, map: &ProcMap, profile: &str) -> Result<(), String> {
    let meta = map.0.lock().await.remove(profile);
    match meta {
        Some(m) => {
            let pid = m.pid;
            let mut kill = Command::new("taskkill");
            kill.args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000);
            let _ = kill.output().await;
            let _ = app.emit(
                "dsh-log",
                serde_json::json!({ "profile": profile, "stream": "system", "line": format!("已发送停止指令 (PID {pid})"), "ts": now_millis() }),
            );
            Ok(())
        }
        None => Err(format!("版本 {profile} 未在运行")),
    }
}
