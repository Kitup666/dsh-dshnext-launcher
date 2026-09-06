//! 诊断导出：把排障所需的状态快照成一份**脱敏**文本文件。
//!
//! 铁律：API Key 只出现长度，不出现内容——这份文件是给用户拿去发 issue 的，
//! 不能变成泄密通道（与设置页「不会上传到任何地方」的承诺同一条纪律）。

use crate::core::store::Config;

/// 拼诊断报告。`log_tail` 由调用方（update，LogLine 所在层）预格式化好传进来，
/// 本模块保持不依赖 ui/app 层。
pub fn render(
    cfg: &Config,
    env: Option<&crate::core::envres::EnvStatus>,
    profiles: &[crate::core::profiles::ProfileInfo],
    procs: &[crate::core::procman::ProcStatus],
    log_tail: &[String],
    version: &str,
    os: &str,
) -> String {
    let mut s = String::new();
    s.push_str("DshDesk 诊断信息（API Key 已脱敏）\n");
    s.push_str("=====================================\n\n");
    s.push_str(&format!("启动器版本：{version}\n"));
    s.push_str(&format!("系统：{os}\n"));
    s.push_str(&format!(
        "数据目录：{}\n\n",
        crate::core::store::data_dir().display()
    ));

    s.push_str("[配置]\n");
    s.push_str(&format!("  端口：{}\n", cfg.port));
    s.push_str(&format!("  主题：{}\n", cfg.theme));
    s.push_str(&format!("  自动打开 WebUI：{}\n", cfg.auto_open));
    s.push_str(&format!("  开机自启：{}\n", cfg.autostart));
    s.push_str(&format!(
        "  API Key：{}\n",
        if cfg.api_key.is_empty() {
            "未设置".to_string()
        } else {
            format!("已设置（{} 字符，内容已隐藏）", cfg.api_key.chars().count())
        }
    ));
    s.push_str(&format!("  Node 镜像：{}\n", cfg.node_mirror));
    s.push_str(&format!("  npm registry：{}\n", cfg.npm_registry));
    s.push_str(&format!(
        "  插件目录：{}\n\n",
        if cfg.plugin_catalog_url.is_empty() {
            "未设置".to_string()
        } else {
            cfg.plugin_catalog_url.clone()
        }
    ));

    s.push_str("[环境]\n");
    match env {
        Some(e) => {
            s.push_str(&format!("  Node：{}\n", opt(&e.node_version)));
            s.push_str(&format!("  pnpm：{}\n", opt(&e.pnpm_version)));
            s.push_str(&format!("  dsh：{}\n", opt(&e.dsh_version)));
            s.push_str(&format!("  dsh 路径：{}\n", opt(&e.dsh_path)));
            s.push_str(&format!("  DSH_HOME：{}\n", e.home_dir));
        }
        None => s.push_str("  尚未探测\n"),
    }
    s.push('\n');

    s.push_str(&format!("[版本] 共 {} 个\n", profiles.len()));
    for p in profiles {
        s.push_str(&format!("  {}（{}）\n", p.name, p.path));
    }
    s.push('\n');

    s.push_str(&format!("[运行中实例] 共 {} 个\n", procs.len()));
    for p in procs {
        s.push_str(&format!(
            "  {} pid={} port={} 运行 {}s（{}）\n",
            p.profile, p.pid, p.port, p.uptime_secs, p.url
        ));
    }
    s.push('\n');

    s.push_str(&format!("[日志尾部] 最后 {} 条\n", log_tail.len()));
    for l in log_tail {
        s.push_str(l);
        s.push('\n');
    }
    s
}

fn opt(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| "未安装".into())
}

/// 导出目标：桌面优先（用户最容易找到），失败退回家目录再退数据目录。
pub fn target_path() -> std::path::PathBuf {
    let stamp = chrono_stamp();
    let name = format!("DshDesk-诊断-{stamp}.txt");
    if let Some(dir) = dirs::document_dir() {
        return dir.join(name);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(name);
    }
    crate::core::store::data_dir().join(name)
}

/// 本地时间戳（无 chrono 依赖，用系统本地时间）。
fn chrono_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 就用 UTC：文件名唯一性是目的，时区无所谓。
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days as i64);
    let rem = secs % 86400;
    let (hh, mm, ss) = (rem / 3600, rem % 3600 / 60, rem % 60);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// 天数 → (年, 月, 日)，Howard Hinnant 的算法，免引 chrono。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
