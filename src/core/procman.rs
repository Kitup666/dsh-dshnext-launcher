use crate::core::envres::{child_path, dsh_cmd, home_dir};
use crate::core::store::Config;
use crate::core::event::{log, CoreEvent, EventSink, LogStream};
use serde::Serialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
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
    tx: EventSink,
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
    let tx2 = tx.clone();
    let profile2 = profile.to_string();
    if let Some(mut out) = child.stdout.take() {
        tokio::spawn(async move {
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
                    let _ = tx2.send(CoreEvent::Url {
                        profile: profile2.clone(),
                        url: found,
                    });
                }
                log(&tx2, &profile2, LogStream::Stdout, line);
            }
        });
    }
    // stderr 转发
    let tx3 = tx.clone();
    let profile3 = profile.to_string();
    if let Some(mut err) = child.stderr.take() {
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(&mut err);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log(&tx3, &profile3, LogStream::Stderr, line);
            }
        });
    }
    // 退出监视。上一代在这里还要关掉 profile 对应的 WebUI 窗口，Dshnext 改用
    // 系统浏览器（DESIGN.md §5），浏览器标签由用户自己管，这段逻辑随之删除。
    let tx4 = tx.clone();
    let profile4 = profile.to_string();
    tokio::spawn(async move {
        let code = match child.wait().await {
            Ok(s) => s.code().unwrap_or(-1),
            Err(_) => -1,
        };
        let _ = tx4.send(CoreEvent::Exit {
            profile: profile4,
            code,
        });
    });

    Ok(url)
}

/// taskkill 杀整棵进程树
pub async fn stop(tx: &EventSink, map: &ProcMap, profile: &str) -> Result<(), String> {
    let meta = map.0.lock().await.remove(profile);
    match meta {
        Some(m) => {
            let pid = m.pid;
            let mut kill = Command::new("taskkill");
            kill.args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000);
            let _ = kill.output().await;
            log(
                tx,
                profile,
                LogStream::System,
                format!("已发送停止指令 (PID {pid})"),
            );
            Ok(())
        }
        None => Err(format!("版本 {profile} 未在运行")),
    }
}
