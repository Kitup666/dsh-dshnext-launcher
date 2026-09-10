//! 通过 profile 的用户补丁层 (`cordis.patch.yml`) 热禁用/启用插件。
//!
//! 机制与 dsh-market 的 `src/patch.ts` 对齐，写入格式一致，因此两边写同一份
//! 文件可以互通：
//!
//! ```yaml
//! - id: <rowId>
//!   disabled: true
//! ```
//!
//! `disabled: true` 停掉该 loader entry；`disabled: false` 强制启用一个被更低
//! 层禁用的 entry。harness 的 HMR 会在保存后约 1 秒重组，且每次启动都重新应用，
//! 所以选择能持久化。
//!
//! 这里做的是行级扫描，不引 YAML 依赖：pitch 文件可能含本 crate 无法解析的结构，
//! 但 `- id: X` + `disabled: true|false` 这一对形态足够判断。

use std::collections::HashSet;
use std::path::Path;

/// 用户补丁层当前对各类行的声明。
#[derive(Debug, Default, Clone)]
pub struct PatchState {
    /// `disabled: true` 的行 id。
    pub disables: HashSet<String>,
    /// `disabled: false`（强制启用）的行 id。
    pub forced: HashSet<String>,
}

/// 允许写入补丁层的行 id：纯 YAML 标量，且与 dsh-market 的 `ROW_ID_RE` 一致。
fn valid_row_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// 顶层 `- id: X` 行提取行 id（列 0，无前导空格）。
fn top_level_row_id(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("- id: ")?;
    let id = rest.trim();
    valid_row_id(id).then_some(id)
}

/// 行级扫描补丁文件，读取 disable/force 行。
pub fn read_state(patch_path: &Path) -> PatchState {
    let mut state = PatchState::default();
    let Ok(text) = std::fs::read_to_string(patch_path) else {
        return state;
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut in_insert = false;
    for (index, raw) in lines.iter().enumerate() {
        let line = raw.trim_end();
        if line == "- insert:" {
            in_insert = true;
            continue;
        }
        if line.starts_with("- ") {
            in_insert = false;
        }
        if in_insert {
            continue;
        }
        let Some(id) = top_level_row_id(line) else {
            continue;
        };
        let next = lines.get(index + 1).map(|l| l.trim()).unwrap_or("");
        if next == "disabled: true" {
            state.disables.insert(id.to_string());
        } else if next == "disabled: false" {
            state.forced.insert(id.to_string());
        }
    }
    state
}

/// 一个包在用户补丁层里“拥有”的行 id：bundle patch 的 insert 行，加上包根目录
/// 约定的 `cordis.patch.yml` 的 insert 行。与 dsh-market 的 `rowIdsForPackage`
/// 同样的口径——只碰这个包自己插入的行，不碰它只是重新配置别人的行（#147）。
pub fn row_ids_for_package(profile_dir: &Path, package: &str) -> Vec<String> {
    let pdir = profile_dir.join("node_modules").join(package);
    let mut ids: Vec<String> = Vec::new();
    if let Some(patch) = crate::core::plugins::bundle_patch_path(&pdir) {
        if let Ok(text) = std::fs::read_to_string(&patch) {
            ids.extend(crate::core::plugins::insert_entry_ids(&text));
        }
    }
    if let Ok(text) = std::fs::read_to_string(pdir.join("cordis.patch.yml")) {
        ids.extend(crate::core::plugins::insert_entry_ids(&text));
    }
    ids.sort();
    ids.dedup();
    ids
}

/// 一行 disable/force 块。
fn row_block(row_id: &str, disabled: bool) -> String {
    format!(
        "- id: {row_id}\n  disabled: {}\n",
        if disabled { "true" } else { "false" }
    )
}

/// 删除指定 id + 值的两行块；返回新文本（未命中返回 None）。
fn remove_block(text: &str, row_id: &str, value: bool) -> Option<String> {
    let want = if value { "disabled: true" } else { "disabled: false" };
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut removed = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        if !removed
            && top_level_row_id(line) == Some(row_id)
            && lines.get(i + 1).map(|l| l.trim()) == Some(want)
        {
            i += 2;
            removed = true;
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }
    removed.then(|| out.join("\n"))
}

/// 去掉行尾注释后，文件是否只剩 `[]` 占位。空/注释文件返回 ""。
fn content_without_comments(text: &str) -> String {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 删除最后一个块后，把空的 `[]` 占位补回去。
///
/// 追加第一行时会把模板的 `[]` 注释掉；删掉最后一行就会留下一个纯注释文件——
/// 那不是顶层数组，dsh 会拒绝启动整个 profile。所以要还原占位。
fn with_placeholder_restored(text: &str) -> String {
    if !content_without_comments(text).is_empty() {
        return text.to_string();
    }
    // 还原被注释掉的占位 `# []`
    let mut restored: Option<String> = None;
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('#') && line.contains("[]") {
            let indent = &line[..line.len() - line.trim_start().len()];
            let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            new_lines[i] = format!("{indent}[]");
            restored = Some(new_lines.join("\n"));
            break;
        }
    }
    if let Some(r) = restored {
        return r;
    }
    if text.is_empty() || text.ends_with('\n') {
        format!("{text}[]\n")
    } else {
        format!("{text}\n[]\n")
    }
}

/// 追加一条顶层 patch 条目；文件不是合法条目列表时拒绝，绝不让它更糟。
fn append_patch_entry(patch_path: &Path, block: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(patch_path).unwrap_or_default();
    let core = text.trim();
    if core.is_empty() {
        std::fs::write(patch_path, block).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let without_comments = content_without_comments(&text);
    if without_comments.is_empty() {
        let next = if text.ends_with('\n') { text } else { format!("{text}\n") };
        std::fs::write(patch_path, format!("{next}{block}")).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if without_comments == "[]" || without_comments == "[ ]" {
        // 把模板的空列表占位注释掉，再追加（否则一个文档里出现两个顶层元素）。
        let mut done = false;
        let lines: Vec<String> = text
            .lines()
            .map(|l| {
                if !done && l.trim() == "[]" {
                    done = true;
                    let indent = &l[..l.len() - l.trim_start().len()];
                    format!("{indent}# []")
                } else {
                    l.to_string()
                }
            })
            .collect();
        let next = lines.join("\n");
        let next = if next.ends_with('\n') { next } else { format!("{next}\n") };
        std::fs::write(patch_path, format!("{next}{block}")).map_err(|e| e.to_string())?;
        return Ok(());
    }
    // 以顶层流式结构（`[...]` / `{...}`）结尾时无法安全追加。
    let last_content = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .last()
        .unwrap_or("");
    if last_content.starts_with('[') || last_content.starts_with('{') {
        return Err("补丁层以顶层流式结构结尾，拒绝自动追加；请先整理成条目列表".into());
    }
    let next = if text.ends_with('\n') { text } else { format!("{text}\n") };
    std::fs::write(patch_path, format!("{next}{block}")).map_err(|e| e.to_string())?;
    Ok(())
}

/// 禁用一个行：追加 `- id: X` + `disabled: true`（幂等）。
pub fn disable_row(patch_path: &Path, row_id: &str) -> Result<(), String> {
    if !valid_row_id(row_id) {
        return Err(format!("行 id `{row_id}` 含特殊字符，不支持写入补丁层"));
    }
    if read_state(patch_path).disables.contains(row_id) {
        return Ok(());
    }
    append_patch_entry(patch_path, &row_block(row_id, true))
}

/// 启用一个行：删除 `disabled: true` 块；若被更低层按住则写 `disabled: false`。
pub fn enable_row(patch_path: &Path, row_id: &str) -> Result<(), String> {
    if !valid_row_id(row_id) {
        return Err(format!("行 id `{row_id}` 含特殊字符，不支持写入补丁层"));
    }
    let state = read_state(patch_path);
    if let Ok(text) = std::fs::read_to_string(patch_path) {
        if let Some(next) = remove_block(&text, row_id, true) {
            std::fs::write(patch_path, with_placeholder_restored(&next)).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    if state.forced.contains(row_id) {
        return Ok(());
    }
    append_patch_entry(patch_path, &row_block(row_id, false))
}

/// 切换一个插件的禁用状态：对它拥有的每个 entry 行写入或移除禁用块。
pub fn set_disabled(profile: &str, package: &str, disabled: bool) -> Result<(), String> {
    let dir = crate::core::plugins::profile_dir(profile);
    if !dir.join("package.json").exists() {
        return Err(format!("版本 {profile} 不存在或缺少 package.json"));
    }
    let patch_path = dir.join("cordis.patch.yml");
    let ids = row_ids_for_package(&dir, package);
    if ids.is_empty() {
        return Err(format!(
            "{package} 没有可切换的补丁行（纯客户端插件或未安装）"
        ));
    }
    for id in &ids {
        if disabled {
            disable_row(&patch_path, id)?;
        } else {
            enable_row(&patch_path, id)?;
        }
    }
    Ok(())
}

/// 卸载清理：移除这些行 id 的 disable/force 块，避免遗留孤儿行
/// （对齐 dsh-market 的 removeRowBlocks）。
pub fn remove_rows(patch_path: &Path, row_ids: &[String]) {
    let Ok(text) = std::fs::read_to_string(patch_path) else {
        return;
    };
    let mut next = text.clone();
    for id in row_ids {
        if let Some(t) = remove_block(&next, id, true) {
            next = t;
        }
        if let Some(t) = remove_block(&next, id, false) {
            next = t;
        }
    }
    if next != text {
        let _ = std::fs::write(patch_path, with_placeholder_restored(&next));
    }
}

#[cfg(test)]
mod tests {
    use super::{disable_row, enable_row, read_state, remove_block};
    use std::path::PathBuf;

    fn temp_patch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("dshnext-patch-test-{name}.yml"));
        std::fs::write(&p, "# header\n[]\n").unwrap();
        p
    }

    #[test]
    fn disable_and_enable_round_trip_restores_placeholder() {
        let path = temp_patch("roundtrip");
        disable_row(&path, "web-search").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("- id: web-search\n  disabled: true\n"), "{text}");
        assert!(read_state(&path).disables.contains("web-search"));
        assert!(!text.contains("\n[]\n") || text.contains("# []"), "{text}");

        enable_row(&path, "web-search").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("web-search"), "{text}");
        // 删掉最后一行后必须还原成合法顶层数组，否则 profile 起不来。
        assert_eq!(super::content_without_comments(&text), "[]", "{text}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn remove_block_only_matches_the_requested_value() {
        let text = "- id: x\n  disabled: false\n- id: x\n  disabled: true\n";
        let removed = remove_block(text, "x", true).unwrap();
        assert!(removed.contains("disabled: false"));
        assert!(!removed.contains("disabled: true"));
    }
}
