use crate::core::envres::{child_path, dsh_prefix, node_dir, node_dist_base, DSH_ALLOW_SCRIPTS, DEFAULT_NODE_VERSION};
use crate::core::event::{progress, EventSink};
use crate::core::store::Config;
use futures_util::StreamExt;
use std::os::windows::process::CommandExt;

/// 把一段文本按行转发为 EnvProgress 事件
fn emit_lines(tx: &EventSink, task: &str, buf: &str) {
    for line in buf.lines() {
        if !line.trim().is_empty() {
            progress(tx, task, line);
        }
    }
}

/// 阻塞式收集子进程输出并按行发事件（放到 spawn_blocking 里跑）
fn run_streaming(tx: EventSink, task: String, mut cmd: std::process::Command) -> Result<(), String> {
    use std::io::BufRead;
    cmd.creation_flags(0x0800_0000);
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动子进程失败: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let t2 = task.clone();
    let tx2 = tx.clone();
    let err_thread = std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            progress(&tx2, &t2, line);
        }
    });
    let reader = std::io::BufReader::new(stdout);
    for line in reader.lines().map_while(Result::ok) {
        progress(&tx, &task, line);
    }
    err_thread.join().map_err(|_| "stderr 线程异常".to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("命令退出码：{}", status.code().unwrap_or(-1)))
    }
}

fn npm_cmd(cfg: &Config) -> std::process::Command {
    // npm 用系统的还是便携的？便携 node 安装后包含 npm.cmd；否则找系统 npm
    let mut cmd = if node_dir().join("npm.cmd").exists() {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &node_dir().join("npm.cmd").to_string_lossy()]);
        c
    } else {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "npm"]);
        c
    };
    cmd.env("PATH", child_path());
    if !cfg.npm_registry.trim().is_empty() {
        cmd.arg(format!("--registry {}", cfg.npm_registry.trim()));
    }
    cmd
}

/// 下载并解压便携版 Node.js（strip 掉 zip 的顶层目录）
pub async fn install_node(tx: EventSink, cfg: Config, version: String) -> Result<(), String> {
    let version = if version.trim().is_empty() {
        DEFAULT_NODE_VERSION.to_string()
    } else {
        version.trim().trim_start_matches('v').to_string()
    };
    let arch = if std::env::consts::ARCH == "aarch64" {
        "win-arm64"
    } else {
        "win-x64"
    };
    let file = format!("node-v{version}-{arch}.zip");
    let url = format!("{}/v{version}/{file}", node_dist_base(&cfg));
    let target = node_dir();
    if target.join("node.exe").exists() {
        return Err("便携 Node 已存在，请先删除 runtime/node 再重装".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let _ = progress(&tx, "node", format!("下载 {url}"));
    let resp = client
        .get(&url)
        .header("User-Agent", "DshDesk")
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败：HTTP {}（可在设置里换 Node 镜像）", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let zip_path = std::env::temp_dir().join(&file);
    {
        let mut stream = resp.bytes_stream();
        let mut out = tokio::fs::File::create(&zip_path)
            .await
            .map_err(|e| e.to_string())?;
        use tokio::io::AsyncWriteExt;
        let mut got: u64 = 0;
        let mut next_report = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("下载中断: {e}"))?;
            out.write_all(&chunk).await.map_err(|e| e.to_string())?;
            got += chunk.len() as u64;
            if got >= next_report {
                next_report = got + (2 << 20);
                progress(
                    &tx,
                    "node",
                    format!(
                        "已下载 {:.1} / {:.1} MB",
                        got as f64 / 1048576.0,
                        total as f64 / 1048576.0
                    ),
                );
            }
        }
        out.flush().await.map_err(|e| e.to_string())?;
    }

    // 解压放阻塞线程
    let tx2 = tx.clone();
    let task = "node".to_string();
    let target2 = target.clone();
    let result = tokio::task::spawn_blocking(move || {
        extract_node_zip(&zip_path, &target2, &tx2, &task).map(|_| {
            let _ = std::fs::remove_file(&zip_path);
        })
    })
    .await
    .map_err(|e| e.to_string())?;
    result?;
    if !target.join("node.exe").exists() {
        return Err("解压完成但未找到 node.exe".into());
    }
    Ok(())
}

/// 解压便携 Node zip（strip 首层目录），下载版与离线版共用。
fn extract_node_zip(
    zip_path: &std::path::Path,
    target: &std::path::Path,
    tx: &EventSink,
    task: &str,
) -> Result<(), String> {
    let f = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("zip 打开失败: {e}"))?;
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        // strip 首层目录 node-vXX-win-x64/
        let rel = match name.split_once('/') {
            Some((_, rest)) if !rest.is_empty() => rest.to_string(),
            _ => continue,
        };
        let dest = target.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    emit_lines(tx, task, "Node.js 解压完成");
    Ok(())
}

// ---- 离线安装（roadmap #9）：包由用户放进 store::offline_dir()，断网可装 ----

/// 离线包扫描结果：三类包各至多一个（同名取字典序最大——新版优先）。
#[derive(Debug, Clone, Default)]
pub struct OfflinePacks {
    pub node: Option<std::path::PathBuf>,
    pub dsh: Option<std::path::PathBuf>,
    pub pnpm: Option<std::path::PathBuf>,
}

impl OfflinePacks {
    pub fn any(&self) -> bool {
        self.node.is_some() || self.dsh.is_some() || self.pnpm.is_some()
    }
}

/// 扫描离线包目录。命名约定：`node-*.zip` / `dsh*.tgz` / `pnpm*.tgz`。
pub fn scan_offline() -> OfflinePacks {
    let mut packs = OfflinePacks::default();
    let Ok(rd) = std::fs::read_dir(crate::core::store::offline_dir()) else {
        return packs;
    };
    let mut names: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    names.sort();
    for p in names {
        let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
            continue;
        };
        if name.starts_with("node-") && name.ends_with(".zip") {
            packs.node = Some(p);
        } else if name.starts_with("dsh") && (name.ends_with(".tgz") || name.ends_with(".tar.gz")) {
            packs.dsh = Some(p);
        } else if name.starts_with("pnpm") && (name.ends_with(".tgz") || name.ends_with(".tar.gz")) {
            packs.pnpm = Some(p);
        }
    }
    packs
}

/// 从离线 zip 装便携 Node（跳过下载）。
pub async fn install_node_offline(tx: EventSink, zip: std::path::PathBuf) -> Result<(), String> {
    let target = node_dir();
    if target.join("node.exe").exists() {
        return Err("便携 Node 已存在，请先删除 runtime/node 再安装".into());
    }
    if !zip.exists() {
        return Err("离线包文件不存在（被移动或删除？）".into());
    }
    let tx2 = tx.clone();
    let task = "node".to_string();
    let target2 = target.clone();
    let zip2 = zip.clone();
    let result = tokio::task::spawn_blocking(move || {
        extract_node_zip(&zip2, &target2, &tx2, &task)
    })
    .await
    .map_err(|e| e.to_string())?;
    result?;
    if !target.join("node.exe").exists() {
        return Err("解压完成但未找到 node.exe".into());
    }
    emit_lines(&tx, "node", &format!("离线安装自 {}", zip.display()));
    Ok(())
}

/// 从离线 tgz 装 dsh（npm install -g 本地包，跳过 registry）。
pub async fn install_dsh_offline(
    tx: EventSink,
    cfg: Config,
    tgz: std::path::PathBuf,
) -> Result<(), String> {
    if !tgz.exists() {
        return Err("离线包文件不存在（被移动或删除？）".into());
    }
    let mut cmd = npm_cmd(&cfg);
    cmd.arg("install").arg("-g").arg("--prefix").arg(dsh_prefix());
    cmd.arg(format!("--allow-scripts={DSH_ALLOW_SCRIPTS}"));
    cmd.arg(&tgz);
    std::fs::create_dir_all(dsh_prefix()).map_err(|e| e.to_string())?;
    let task = "dsh".to_string();
    tokio::task::spawn_blocking(move || run_streaming(tx, task, cmd))
        .await
        .map_err(|e| e.to_string())?
}

/// 从离线 tgz 装 pnpm。
pub async fn install_pnpm_offline(
    tx: EventSink,
    cfg: Config,
    tgz: std::path::PathBuf,
) -> Result<(), String> {
    if !tgz.exists() {
        return Err("离线包文件不存在（被移动或删除？）".into());
    }
    let mut cmd = npm_cmd(&cfg);
    cmd.arg("install").arg("-g").arg("--prefix").arg(dsh_prefix());
    cmd.arg(&tgz);
    let task = "pnpm".to_string();
    tokio::task::spawn_blocking(move || run_streaming(tx, task, cmd))
        .await
        .map_err(|e| e.to_string())?
}

/// 把 dsh 安装/更新到托管前缀（version 传 "latest" 或精确版本号）
pub async fn install_dsh(tx: EventSink, cfg: Config, version: String) -> Result<(), String> {
    let ver = version.trim();
    let ver = if ver.is_empty() || ver == "latest" {
        "@deepseek-ai/dsh@latest".to_string()
    } else {
        format!("@deepseek-ai/dsh@{ver}")
    };
    let mut cmd = npm_cmd(&cfg);
    cmd.arg("install").arg("-g").arg("--prefix").arg(dsh_prefix());
    cmd.arg(format!("--allow-scripts={DSH_ALLOW_SCRIPTS}"));
    cmd.arg(&ver);
    std::fs::create_dir_all(dsh_prefix()).map_err(|e| e.to_string())?;
    let task = "dsh".to_string();
    tokio::task::spawn_blocking(move || run_streaming(tx, task, cmd))
        .await
        .map_err(|e| e.to_string())?
}

/// 安装 pnpm 到托管前缀（dsh plugin 依赖它）
pub async fn install_pnpm(tx: EventSink, cfg: Config) -> Result<(), String> {
    let mut cmd = npm_cmd(&cfg);
    cmd.arg("install").arg("-g").arg("--prefix").arg(dsh_prefix());
    cmd.arg("pnpm");
    let task = "pnpm".to_string();
    tokio::task::spawn_blocking(move || run_streaming(tx, task, cmd))
        .await
        .map_err(|e| e.to_string())?
}

/// npm registry 上可用的 dsh 版本列表
pub async fn dsh_versions(cfg: Config, include_rc: bool) -> Result<crate::core::envres::DshVersions, String> {
    crate::core::envres::fetch_dsh_versions(&cfg, include_rc).await
}

/// 删除托管 Node（供「重装」前清理）
pub fn remove_node() -> Result<(), String> {
    let dir = node_dir();
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 删除托管 dsh（供「重装」前清理）
pub fn remove_dsh() -> Result<(), String> {
    let dir = dsh_prefix();
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}
