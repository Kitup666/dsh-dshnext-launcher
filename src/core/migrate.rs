//! 目录迁移（设置页「修改目录」/ `--migrate-to`）：把启动器数据目录 /
//! dsh-home 的现有内容搬到新位置。
//!
//! **两阶段纪律**（2026-09-07 用户实测翻车后的重写）：
//! 1. **先复制**：所有条目复制到目标（同卷也不走 rename——rename 半路失败
//!    没有回滚，复制是幂等可重试的）。任何一个文件复制失败 → 整体 Err，
//!    此时 pointer / redirect / 删除都**还没发生**，旧位置完整可启动可重试。
//!    单文件复制带 3 次退避重试——杀软/索引器的瞬时共享锁是实测翻车原因。
//! 2. **后删除**：复制全部成功才删源。删除失败（文件被占用）不回滚、
//!    不报错，只收集进 warnings——数据已在双份，用户事后手删即可。
//!
//! config.json 参与复制但**删除排在最后**（pointer 已写、redirect 已发生，
//! 新位置接管之后旧位置才清场）。

use crate::core::envres;
use crate::core::store;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 一次性完成「改启动器目录 + 改 dsh-home」的落盘部分（阻塞线程里跑）。
/// 返回 warnings：删除阶段没删掉的旧文件（数据已双份，属非致命）。
///
/// `home_custom` = 用户是否显式填了 dsh-home（false = 跟随 `<启动器目录>/home`）。
pub fn relocate(
    launcher_new: PathBuf,
    home_new: PathBuf,
    home_custom: bool,
) -> Result<Vec<String>, String> {
    let old_data = store::data_dir();
    // 「当前生效的 home」必须在改道前抓取：默认 home 跟着数据目录走，
    // redirect 之后取会拿到（可能已不存在的）旧覆盖路径。
    let old_home = envres::home_dir();
    let launcher_moved = launcher_new != old_data;
    let mut warnings: Vec<String> = Vec::new();

    // ---- 1. 启动器数据目录：先复制，全成才改道 ----
    if launcher_moved {
        fs::create_dir_all(&launcher_new)
            .map_err(|e| format!("创建 {} 失败：{e}", launcher_new.display()))?;
        // config.json 排除在批量复制外，单独最后处理（见模块注释）。
        // remap：树里的 junction（npm 依赖结构大量使用）目标若是绝对路径
        // 指向旧数据目录，必须改写前缀——否则删了旧位置链接全断。
        let errs = copy_tree(&old_data, &launcher_new, &[launcher_new.clone()],
                             Some((&old_data, &launcher_new)));
        if !errs.is_empty() {
            // 复制阶段失败：pointer/redirect/删除都未发生，原地完整可重试。
            return Err(format!(
                "复制中断（旧位置未动，可重试）：{} 项失败，首个：{}",
                errs.len(),
                errs[0]
            ));
        }
    }

    // ---- 2. dsh-home：复制也排在删除之前——旧 home 往往就在旧数据目录里
    //      （默认 `<数据>/home`），清场顺序不对会把自己要复制的源删掉。
    //      home 在数据目录内且目标就是它的新家时，内容已随数据复制到位，跳过。
    if home_new != old_home {
        let already_moved = launcher_moved
            && old_home
                .strip_prefix(&old_data)
                .map(|rel| home_new == launcher_new.join(rel))
                .unwrap_or(false);
        if !already_moved && old_home.exists() {
            let remap = launcher_moved.then(|| (old_data.as_path(), launcher_new.as_path()));
            let errs = copy_tree(&old_home, &home_new, &[], remap);
            if !errs.is_empty() {
                return Err(format!(
                    "dsh-home 复制中断（原 home 未动，可重试）：{} 项失败，首个：{}",
                    errs.len(),
                    errs[0]
                ));
            }
        }
        if old_home.exists() {
            if let Err(e) = fs::remove_dir_all(&old_home) {
                warnings.push(format!("{}（{e}）", old_home.display()));
            }
        }
        envres::set_home_dir(home_new.clone());
    }

    // ---- 3. 启动器数据目录：写 pointer、改道、config 单独接取 ----
    if launcher_moved {
        store::write_pointer(&launcher_new)?;
        store::redirect_data_dir(launcher_new.clone());

        let old_cfg = old_data.join("config.json");
        if old_cfg.exists() {
            copy_file_retry(&old_cfg, &launcher_new.join("config.json"))?;
        }
    }

    // ---- 4. 删除阶段：旧位置清场（pointer 文件除外——它必须留在锚定目录）。
    //      删除失败不回滚、不报错，只收集进 warnings（数据已双份）。
    if launcher_moved {
        let pointer = store::pointer_path();
        if let Ok(entries) = fs::read_dir(&old_data) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p == pointer {
                    continue;
                }
                let r = if p.is_dir() {
                    fs::remove_dir_all(&p)
                } else {
                    fs::remove_file(&p)
                };
                if let Err(e) = r {
                    warnings.push(format!("{}（{e}）", p.display()));
                }
            }
        }
        if old_data != store::boot_dir() {
            // 锚定目录本身留着（pointer 的家）；非锚定的旧数据目录删壳。
            let _ = fs::remove_dir(&old_data);
        }
    }

    // ---- 3. config 的 dsh_home 字段（从可能已改道的位置重读）----
    let mut cfg = store::load();
    cfg.dsh_home = if home_custom {
        home_new.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    store::save(&cfg)?;
    Ok(warnings)
}

/// 递归复制 `src` 下全部条目到 `dst`（dst 会被创建）。`skip` 里的路径原样跳过
/// （嵌套目标、config 这类要特殊排序的）。`remap` = (旧根, 新根)：树里的
/// junction/symlink 目标若落在旧根下，重建时改写到新根。
/// 单文件复制失败**不中断**，继续搬其余条目，失败的路径收集返回——中止决策
/// 归调用方（不同阶段中止代价不同）。
fn copy_tree(
    src: &Path,
    dst: &Path,
    skip: &[PathBuf],
    remap: Option<(&Path, &Path)>,
) -> Vec<String> {
    let mut errs = Vec::new();
    if let Err(e) = fs::create_dir_all(dst) {
        return vec![format!("创建 {} 失败：{e}", dst.display())];
    }
    let entries = match fs::read_dir(src) {
        Ok(e) => e,
        Err(e) => return vec![format!("读取 {} 失败：{e}", src.display())],
    };
    for entry in entries.flatten() {
        let from = entry.path();
        if skip.contains(&from) {
            continue;
        }
        let to = dst.join(entry.file_name());
        // reparse point（junction/symlink）：不能 fs::copy（目录链接直接
        // 拒绝访问——实测翻车点），重建链接并按 remap 改写目标前缀。
        let is_link = fs::symlink_metadata(&from)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if is_link {
            if let Err(e) = recreate_link(&from, &to, remap) {
                errs.push(e);
            }
        } else if from.is_dir() {
            errs.extend(copy_tree(&from, &to, &[], remap));
        } else if let Err(e) = copy_file_retry(&from, &to) {
            errs.push(e);
        }
    }
    errs
}

/// 在 `to` 处重建 `from` 的 junction/symlink，目标经 `remap` 前缀改写。
fn recreate_link(from: &Path, to: &Path, remap: Option<(&Path, &Path)>) -> Result<(), String> {
    let target = fs::read_link(from).map_err(|e| format!("读取链接 {} 失败：{e}", from.display()))?;
    let retargeted = match remap {
        Some((old_root, new_root)) => retarget(&target, old_root, new_root),
        None => target,
    };
    // 半截残留（上次失败的中断现场）先清掉；junction 用 remove_dir 摘除。
    let _ = fs::remove_dir(to);
    let _ = fs::remove_file(to);
    // junction 不需要特权（symlink_dir 要管理员/开发者模式），优先用它。
    #[cfg(windows)]
    {
        junction::create(&retargeted, to)
            .map_err(|e| format!("重建链接 {} → {} 失败：{e}", to.display(), retargeted.display()))
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(&retargeted, to)
            .map_err(|e| format!("重建链接 {} 失败：{e}", to.display()))
    }
}

/// junction 目标常带 `\\?\` 前缀（readlink 原样返回）：剥掉再比对，落在
/// 旧根下就接到新根，外面的目标原样保留。
fn retarget(target: &Path, old_root: &Path, new_root: &Path) -> PathBuf {
    let s = target.to_string_lossy();
    let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let p = PathBuf::from(stripped);
    match p.strip_prefix(old_root) {
        Ok(rel) => new_root.join(rel),
        Err(_) => p,
    }
}

/// 单文件复制，3 次退避重试。杀软/Windows Search 的瞬时共享锁是实测的
/// 中断源（home/profiles/node_modules 半截），重试能把它们吃掉。
fn copy_file_retry(from: &Path, to: &Path) -> Result<(), String> {
    let mut last = String::new();
    for i in 0..3 {
        match fs::copy(from, to) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(250 * (i + 1)));
            }
        }
    }
    Err(format!("复制 {} 失败：{last}", from.display()))
}
