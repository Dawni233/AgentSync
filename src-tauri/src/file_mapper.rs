//! 文件映射模块
//!
//! `_current/` 与本地配置目录之间的双向拷贝，人格切换时执行文件覆盖。
//! 对应设计文档「路径与 glob 解析」「原子性设计」章节。
//!
//! 关键规格（必须遵守）：
//! - glob 语义：`memory/**` 匹配子目录所有文件；`*.lock` 匹配任意层级；
//!   `memory/cache/` 结尾 / 等价于 `memory/cache/**`
//! - 匹配基准：相对于 agent 目录的路径（如 `memory/chat_2024.md`）
//! - L1 文件级原子性：先写临时文件再 rename
//! - L2 agent 级原子性：全部文件先写暂存区，全部成功才覆盖目标

use crate::error::{AppError, AppResult};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// 构建包含/排除两个 GlobSet
///
/// 返回 (include_set, exclude_set)。
/// 最终同步文件集 = include 匹配结果 - exclude 匹配结果。
pub fn build_matcher(
    sync_files: &[String],
    exclude_files: &[String],
) -> AppResult<(GlobSet, GlobSet)> {
    let include = build_globset(sync_files)?;
    let exclude = build_globset(exclude_files)?;
    Ok((include, exclude))
}

/// 构建 GlobSet，处理结尾 `/` 等价于 `/**` 的约定
///
/// glob 语义约定（对应设计文档「glob 语义约定」表）：
/// - 含 `/` 的模式（如 `memory/*`、`memory/**`）用 literal_separator(true)：
///   `*` 不跨目录，`**` 跨目录
/// - 不含 `/` 的模式（如 `*.lock`、`*.tmp`）用 literal_separator(false)：
///   匹配任意层级的 basename（`*.lock` 匹配 `memory/session.lock`）
fn build_globset(patterns: &[String]) -> AppResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        // 文档约定：结尾 / 等价于 /**（匹配目录下所有内容）
        let normalized = if p.ends_with('/') {
            format!("{}**", p)
        } else {
            p.clone()
        };
        // 不含 / 的纯 basename 模式匹配任意层级；含 / 的模式 * 不跨目录
        let literal_sep = normalized.contains('/');
        let glob = GlobBuilder::new(&normalized)
            .literal_separator(literal_sep)
            .build()
            .map_err(|e| AppError::Config(format!("glob 模式 '{}' 非法: {}", p, e)))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| AppError::Config(format!("构建 GlobSet 失败: {}", e)))
}

/// 展开 `~` 为用户目录
///
/// 使用 shellexpand crate，跨平台支持。
pub fn expand_tilde(path: &str) -> AppResult<String> {
    Ok(shellexpand::tilde(path).to_string())
}

/// 判断文件是否应同步（在 include 集合内且不在 exclude 集合内）
pub fn should_sync(rel_path: &str, include: &GlobSet, exclude: &GlobSet) -> bool {
    include.is_match(rel_path) && !exclude.is_match(rel_path)
}

/// 收集目录下所有应同步的文件相对路径
///
/// 扫描 `base_dir`，返回相对路径列表（如 `memory/chat_2024.md`）。
/// 跳过 exclude 文件和目录。
pub fn list_syncable_files(
    base_dir: &Path,
    include: &GlobSet,
    exclude: &GlobSet,
) -> AppResult<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(base_dir)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            // 过滤目录：如果目录路径命中 exclude，整个目录跳过
            if e.file_type().is_dir() {
                let rel = e
                    .path()
                    .strip_prefix(base_dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                // 目录路径加 / 后判断（与 `memory/cache/` 模式一致）
                !exclude.is_match(format!("{}/", rel)) && !exclude.is_match(&rel)
            } else {
                true
            }
        })
    {
        let entry = entry.map_err(|e| AppError::Io(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(base_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if should_sync(&rel, include, exclude) {
            files.push(rel);
        }
    }
    files.sort();
    Ok(files)
}

/// 本地配置目录 -> 仓库 `_current/`
///
/// 返回拷贝的文件相对路径列表。
/// 单文件写入走 L1 文件级原子性（先写 .tmp 再 rename）。
pub fn copy_local_to_current(
    local_dir: &Path,
    current_dir: &Path,
    sync_files: &[String],
    exclude_files: &[String],
) -> AppResult<Vec<String>> {
    let (include, exclude) = build_matcher(sync_files, exclude_files)?;
    let files = list_syncable_files(local_dir, &include, &exclude)?;
    let mut copied = Vec::new();
    for rel in &files {
        let src = local_dir.join(rel);
        let dst = current_dir.join(rel);
        copy_file_atomic(&src, &dst)?;
        copied.push(rel.clone());
    }
    Ok(copied)
}

/// 仓库 `_current/` -> 本地配置目录（L2 agent 级原子写入）
///
/// 全部文件先写入暂存区 `staging_dir`，全部成功后再统一覆盖本地配置目录。
/// 任一文件失败则不覆盖本地，返回错误（调用方从快照恢复）。
///
/// 返回拷贝的文件相对路径列表。
pub fn copy_current_to_local_atomic(
    current_dir: &Path,
    local_dir: &Path,
    staging_dir: &Path,
    sync_files: &[String],
    exclude_files: &[String],
) -> AppResult<Vec<String>> {
    let (include, exclude) = build_matcher(sync_files, exclude_files)?;
    let files = list_syncable_files(current_dir, &include, &exclude)?;

    // 阶段 1：全部文件拷到暂存区
    fs::create_dir_all(staging_dir)?;
    for rel in &files {
        let src = current_dir.join(rel);
        let dst = staging_dir.join(rel);
        copy_file_atomic(&src, &dst)?;
    }

    // 阶段 2：全部成功后，从暂存区覆盖到本地配置目录
    let mut copied = Vec::new();
    for rel in &files {
        let src = staging_dir.join(rel);
        let dst = local_dir.join(rel);
        copy_file_atomic(&src, &dst)?;
        copied.push(rel.clone());
    }

    // 清理暂存区
    let _ = fs::remove_dir_all(staging_dir);
    Ok(copied)
}

/// 文件级原子拷贝：先写 `.tmp` 临时文件，再 rename 覆盖目标
fn copy_file_atomic(src: &Path, dst: &Path) -> AppResult<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dst.with_extension("tmp_sync");
    fs::copy(src, &tmp)?;
    fs::rename(&tmp, dst)?;
    Ok(())
}

/// 创建本地配置目录的快照（用于 L2 agent 级回滚）
///
/// 把 local_dir 下所有应同步文件复制到 snapshot_dir。
pub fn create_snapshot(
    local_dir: &Path,
    snapshot_dir: &Path,
    sync_files: &[String],
    exclude_files: &[String],
) -> AppResult<()> {
    let (include, exclude) = build_matcher(sync_files, exclude_files)?;
    let files = list_syncable_files(local_dir, &include, &exclude)?;
    fs::create_dir_all(snapshot_dir)?;
    for rel in &files {
        let src = local_dir.join(rel);
        let dst = snapshot_dir.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, &dst)?;
    }
    Ok(())
}

/// 从快照恢复本地配置目录
///
/// 把 snapshot_dir 下的文件覆盖回 local_dir。
pub fn restore_snapshot(snapshot_dir: &Path, local_dir: &Path) -> AppResult<()> {
    if !snapshot_dir.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(snapshot_dir).min_depth(1) {
        let entry = entry.map_err(|e| AppError::Io(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(snapshot_dir)
            .unwrap()
            .to_path_buf();
        let dst = local_dir.join(&rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &dst)?;
    }
    Ok(())
}

/// 删除快照目录
pub fn remove_snapshot(snapshot_dir: &Path) -> AppResult<()> {
    if snapshot_dir.exists() {
        fs::remove_dir_all(snapshot_dir)?;
    }
    Ok(())
}

/// 计算两个目录间应同步文件的差异
///
/// 返回 (local_only, current_only, modified)：
/// - local_only：本地有、`_current/` 没有的文件（本地新增）
/// - current_only：`_current/` 有、本地没有的文件（远程新增）
/// - modified：两边都有但内容不同的文件
pub fn diff_dirs(
    local_dir: &Path,
    current_dir: &Path,
    sync_files: &[String],
    exclude_files: &[String],
) -> AppResult<(Vec<String>, Vec<String>, Vec<String>)> {
    let (include, exclude) = build_matcher(sync_files, exclude_files)?;
    let local_files = list_syncable_files(local_dir, &include, &exclude)?;
    let current_files = list_syncable_files(current_dir, &include, &exclude)?;

    let local_set: std::collections::HashSet<_> = local_files.iter().collect();
    let current_set: std::collections::HashSet<_> = current_files.iter().collect();

    let mut local_only = Vec::new();
    let mut current_only = Vec::new();
    let mut modified = Vec::new();

    for f in &local_files {
        if !current_set.contains(f) {
            local_only.push(f.clone());
        } else {
            // 两边都有，比较内容
            let local_content = fs::read(local_dir.join(f))?;
            let current_content = fs::read(current_dir.join(f))?;
            if local_content != current_content {
                modified.push(f.clone());
            }
        }
    }
    for f in &current_files {
        if !local_set.contains(f) {
            current_only.push(f.clone());
        }
    }

    local_only.sort();
    current_only.sort();
    modified.sort();
    Ok((local_only, current_only, modified))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(base: &Path, rel: &str, content: &str) {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn glob_include_exclude() {
        let (inc, exc) = build_matcher(
            &["SOUL.md".into(), "memory/**".into()],
            &["*.lock".into(), "memory/cache/".into()],
        )
        .unwrap();
        assert!(should_sync("SOUL.md", &inc, &exc));
        assert!(should_sync("memory/chat_2024.md", &inc, &exc));
        assert!(!should_sync("IDENTITY.md", &inc, &exc));
        assert!(!should_sync("memory/session.lock", &inc, &exc));
        assert!(!should_sync("memory/cache/a.txt", &inc, &exc));
    }

    #[test]
    fn glob_memory_star_vs_starstar() {
        let empty = _build_empty();
        let (inc, _) = build_matcher(&["memory/*".into()], &[]).unwrap();
        assert!(should_sync("memory/a.txt", &inc, &empty));
        assert!(!should_sync("memory/sub/a.txt", &inc, &empty));

        let (inc, _) = build_matcher(&["memory/**".into()], &[]).unwrap();
        assert!(should_sync("memory/sub/a.txt", &inc, &empty));
    }

    fn _build_empty() -> GlobSet {
        GlobSetBuilder::new().build().unwrap()
    }

    #[test]
    fn tilde_expand() {
        let p = expand_tilde("~/.workbuddy").unwrap();
        assert!(p.ends_with(".workbuddy"));
        assert!(!p.contains('~'));
    }

    #[test]
    fn copy_local_to_current_basic() {
        let local = TempDir::new().unwrap();
        let current = TempDir::new().unwrap();
        write_file(local.path(), "SOUL.md", "soul content");
        write_file(local.path(), "memory/chat.md", "chat");
        write_file(local.path(), "memory/cache/tmp.txt", "cache");

        let copied = copy_local_to_current(
            local.path(),
            current.path(),
            &["SOUL.md".into(), "memory/**".into()],
            &["memory/cache/".into()],
        )
        .unwrap();

        assert!(copied.contains(&"SOUL.md".to_string()));
        assert!(copied.contains(&"memory/chat.md".to_string()));
        assert!(!copied.contains(&"memory/cache/tmp.txt".to_string()));
        assert_eq!(fs::read_to_string(current.path().join("SOUL.md")).unwrap(), "soul content");
    }

    #[test]
    fn diff_dirs_detects_changes() {
        let local = TempDir::new().unwrap();
        let current = TempDir::new().unwrap();
        write_file(local.path(), "SOUL.md", "local version");
        write_file(local.path(), "new.md", "new file");
        write_file(current.path(), "SOUL.md", "old version");
        write_file(current.path(), "remote_only.md", "remote");

        let (local_only, current_only, modified) = diff_dirs(
            local.path(),
            current.path(),
            &["**".into()],
            &[],
        )
        .unwrap();

        assert!(local_only.contains(&"new.md".to_string()));
        assert!(current_only.contains(&"remote_only.md".to_string()));
        assert!(modified.contains(&"SOUL.md".to_string()));
    }

    #[test]
    fn snapshot_create_and_restore() {
        let local = TempDir::new().unwrap();
        let snapshot = TempDir::new().unwrap();
        write_file(local.path(), "SOUL.md", "original");

        create_snapshot(local.path(), snapshot.path(), &["SOUL.md".into()], &[]).unwrap();

        // 修改本地
        write_file(local.path(), "SOUL.md", "modified");

        // 从快照恢复
        restore_snapshot(snapshot.path(), local.path()).unwrap();

        assert_eq!(fs::read_to_string(local.path().join("SOUL.md")).unwrap(), "original");
    }
}
