//! 目录迁移（设置页「修改目录」）：把启动器数据目录 / dsh-home 的现有内容
//! 搬到新位置。同卷 `fs::rename` 一步到位；跨卷 rename 会失败（EXDEV），
//! 退回递归复制 + 删源——托管 runtime 可能几个 GB，跨盘转移耗时就耗在这。
//!
//! 顺序纪律（`relocate`）：**config.json 最后搬**。内容搬移中途失败（文件被
//! 占用等）时 pointer/redirect 都还没发生，旧位置仍然完整可启动，重试是
//! 安全的（已搬走的条目源已不存在，跳过）。

use crate::core::envres;
use crate::core::store;
use std::fs;
use std::path::{Path, PathBuf};

/// 把 `src` 目录下的全部条目搬进 `dst`（dst 会被创建）。`skip` 用于嵌套场景
/// （新目录在旧目录里面时跳过它自己）。返回搬动的条目数。
pub fn move_dir_contents(src: &Path, dst: &Path, skip: Option<&Path>) -> Result<usize, String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建 {} 失败：{e}", dst.display()))?;
    let mut n = 0;
    let entries =
        fs::read_dir(src).map_err(|e| format!("读取 {} 失败：{e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        if Some(from.as_path()) == skip || !from.exists() {
            continue;
        }
        move_entry(&from, &dst.join(entry.file_name()))?;
        n += 1;
    }
    Ok(n)
}

fn move_entry(from: &Path, to: &Path) -> Result<(), String> {
    // 同卷 rename 一步到位；失败（跨卷/被占用后的跨卷余项）退回复制+删源。
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, to)?;
        fs::remove_dir_all(from)
            .map_err(|e| format!("删除源 {} 失败：{e}", from.display()))?;
    } else {
        fs::copy(from, to).map_err(|e| format!("复制 {} 失败：{e}", from.display()))?;
        fs::remove_file(from).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let f = entry.path();
        let t = to.join(entry.file_name());
        if f.is_dir() {
            copy_dir(&f, &t)?;
        } else {
            fs::copy(&f, &t).map_err(|e| format!("复制 {} 失败：{e}", f.display()))?;
        }
    }
    Ok(())
}

/// 一次性完成「改启动器目录 + 改 dsh-home」的落盘部分（阻塞线程里跑）：
///
/// 1. 启动器目录变了：搬全部条目（config.json 除外）→ 写 pointer → 会话内
///    redirect → 搬 config.json；
/// 2. dsh-home 变了（`home_new` ≠ 当前生效值）：搬内容 → 删空的旧 home →
///    设会话覆盖；
/// 3. 重读 config、更新 dsh_home 字段（自定义存绝对路径，默认存空串）并落盘。
///
/// `home_custom` = 用户是否显式填了 dsh-home（false = 跟随 `<启动器目录>/home`）。
pub fn relocate(launcher_new: PathBuf, home_new: PathBuf, home_custom: bool) -> Result<(), String> {
    let old_data = store::data_dir();

    // ---- 1. 启动器数据目录 ----
    if launcher_new != old_data {
        move_dir_contents(&old_data, &launcher_new, Some(&launcher_new))
            .map_err(|e| format!("转移数据目录失败：{e}"))?;
        store::write_pointer(&launcher_new)?;
        store::redirect_data_dir(launcher_new.clone());
        // config 最后搬：它一搬走，旧位置就只剩空壳。
        let old_cfg = old_data.join("config.json");
        if old_cfg.exists() {
            move_entry(&old_cfg, &launcher_new.join("config.json"))?;
        }
    }

    // ---- 2. dsh-home（redirect 之后取「当前生效值」才对：默认 home 跟着
    //      数据目录走，上面刚改了道）----
    let current_home = envres::home_dir();
    if home_new != current_home {
        // 当前 home 不存在（全新机器/已被清掉）就没有东西可搬，直接建新的。
        if current_home.exists() {
            move_dir_contents(&current_home, &home_new, None)
                .map_err(|e| format!("转移 dsh-home 失败：{e}"))?;
            // 空了才删得掉；删不掉（有用户文件）也无碍。
            let _ = fs::remove_dir(&current_home);
        }
        envres::set_home_dir(home_new.clone());
    }

    // ---- 3. config 的 dsh_home 字段（从可能已改道的位置重读）----
    let mut cfg = store::load();
    cfg.dsh_home = if home_custom {
        home_new.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    store::save(&cfg)?;
    Ok(())
}
