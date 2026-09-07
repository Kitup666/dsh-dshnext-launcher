//! 启动器自更新。
//!
//! 更新源是 **config.update_url**——空 = 用内置默认源（`store::DEFAULT_UPDATE_URL`，
//! 即本仓库的 `.../releases/latest`）；填了自定义地址（fork/镜像）就优先用。支持
//! 格式：GitHub Releases API `.../releases/latest` 的 JSON（`tag_name` + assets
//! 里 `dshnext.exe` 与可选的 `dshnext.exe.sha256`）。
//!
//! 流程：检查（比版本）→ 下载到数据目录 → SHA256 校验（fail-closed，用
//! PowerShell Get-FileHash，零新依赖）→ 换身（运行中的 exe 改名 .old、新 exe
//! 落到原位）→ 提示重启。下次启动顺手删 .old。

use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};

/// 下载进度（字节）：流式下载时逐块更新，UI 轮询显示进度条。
static DL_DONE: AtomicU64 = AtomicU64::new(0);
static DL_TOTAL: AtomicU64 = AtomicU64::new(0);

/// (已下载, 总大小)。总大小 0 = 服务端没给 content-length（进度条走不定长态）。
pub fn download_progress() -> (u64, u64) {
    (DL_DONE.load(Ordering::Relaxed), DL_TOTAL.load(Ordering::Relaxed))
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub exe_url: String,
    pub sha256: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// 查最新版。找不到 exe 资产、网络失败都返回 Err。
pub async fn latest(api_url: String) -> Result<UpdateInfo, String> {
    // reqwest 默认不发 User-Agent，GitHub API 对无 UA 请求直接 403。
    let client = reqwest::Client::builder()
        .user_agent("DshDesk")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("请求更新源失败：{e}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        // 匿名限额 60 次/小时按**出口 IP** 计——共享出口（代理/机场/NAT）
        // 的机器自己没发几个请求也可能被别人打满。等一小时窗口重置即可。
        return Err(format!(
            "GitHub 暂时限制了更新检查（{status}，匿名配额按出口 IP 共享已耗尽）：稍等几分钟到一小时再点「检查更新」即可，不影响其他功能"
        ));
    }
    let rel: Release = resp
        .json()
        .await
        .map_err(|e| format!("更新源不是合法的 Releases JSON：{e}"))?;
    let exe = rel
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("dshnext.exe"))
        .ok_or("Release 里没有 dshnext.exe 资产")?;
    let sha = rel
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("dshnext.exe.sha256"))
        .map(|a| a.browser_download_url.clone());
    Ok(UpdateInfo {
        version: rel.tag_name.trim_start_matches('v').to_string(),
        exe_url: exe.browser_download_url.clone(),
        sha256: sha,
    })
}

/// 下载到 `dest`，流式逐块上报字节进度（供 UI 进度条）。
pub async fn download_to(url: &str, dest: &std::path::Path) -> Result<(), String> {
    use futures_util::StreamExt;
    let client = reqwest::Client::builder()
        .user_agent("DshDesk")
        // 22 MB 走代理可能十几秒，给足；卡死由这个超时兜底。
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("下载失败：{e}"))?;
    DL_TOTAL.store(resp.content_length().unwrap_or(0), Ordering::Relaxed);
    DL_DONE.store(0, Ordering::Relaxed);
    let mut buf: Vec<u8> = Vec::with_capacity(DL_TOTAL.load(Ordering::Relaxed) as usize);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载失败：{e}"))?;
        buf.extend_from_slice(&chunk);
        DL_DONE.store(buf.len() as u64, Ordering::Relaxed);
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(dest, &buf).map_err(|e| e.to_string())
}

/// 下载文本（.sha256 资产用）。
pub async fn download_string(url: &str) -> Result<String, String> {
    reqwest::get(url)
        .await
        .map_err(|e| format!("下载 sha256 失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("下载 sha256 失败：{e}"))?
        .text()
        .await
        .map_err(|e| format!("下载 sha256 失败：{e}"))
}

/// 文件 SHA256（小写 hex）。走 PowerShell Get-FileHash——零新依赖（sha2 会拖
/// 一串 crypto 编译，值不上一分钟一次的调用）。
pub fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "(Get-FileHash -Algorithm SHA256 -LiteralPath '{}').Hash.ToLower()",
                path.display()
            ),
        ])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && s.len() == 64 {
        Ok(s)
    } else {
        Err(format!("哈希计算失败：{}", String::from_utf8_lossy(&out.stderr)))
    }
}

/// 下载的字节是否可信。**fail-closed**：期望哈希是 Some 就必须精确相等，
/// None（更新源没提供 .sha256 资产）才放行。期望值容忍整份 .sha256 文本
/// （trim + 取首个空白分隔字段），裸 hash 与「hash  文件名」两种格式都接。
pub fn verify_hash(downloaded: &std::path::Path, expected: Option<&str>) -> Result<(), String> {
    let Some(raw) = expected else {
        return Ok(());
    };
    let expected = raw.trim().split_whitespace().next().unwrap_or("");
    if expected.len() != 64 {
        return Err(format!(".sha256 内容不合法（{raw:?}），已拒绝安装"));
    }
    let actual = sha256_file(downloaded)?;
    if actual == expected.to_lowercase() {
        Ok(())
    } else {
        Err(format!("SHA256 不符（期望 {expected}，实际 {actual}），已拒绝安装"))
    }
}

/// 版本比较：按 '.' 分段数值比，段数不齐补零。非数字段当 0。
pub fn newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| p.trim().parse().unwrap_or(0))
            .collect()
    };
    let (r, l) = (parse(remote), parse(local));
    for i in 0..r.len().max(l.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if rv != lv {
            return rv > lv;
        }
    }
    false
}

/// 换身：当前 exe → .old，下载好的新 exe → 原位。返回提示（重启生效）。
pub fn apply_swap(new_exe: &std::path::Path) -> Result<(), String> {
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = cur.with_extension("exe.old");
    // 上次更新遗留的 .old 还在（上次没删成）会挡 rename——先清。
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&cur, &old).map_err(|e| format!("改名旧 exe 失败：{e}"))?;
    std::fs::copy(new_exe, &cur).map_err(|e| format!("落位新 exe 失败：{e}"))?;
    let _ = std::fs::remove_file(new_exe);
    Ok(())
}

/// 启动早期清掉上次更新遗留的 .old（此时旧 exe 一定没人用了）。
pub fn cleanup_old() {
    if let Ok(cur) = std::env::current_exe() {
        let _ = std::fs::remove_file(cur.with_extension("exe.old"));
    }
}
