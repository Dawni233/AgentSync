//! 人格（角色）管理模块
//!
//! 对应设计文档「人格管理」章节的 4 种操作：
//! 1. 保存当前为人格：_current/ -> 新人格文件夹 + commit
//! 2. 切换到指定人格：人格文件夹 -> 覆盖 _current/ + 本地配置目录 + commit
//! 3. 导出人格包：打包为 .zip（含 manifest.json）
//! 4. 导入人格包：校验 + diff 预览 + 强制确认 + 解压
//!
//! 原子操作（对应「原子操作」章节）：
//! - 切换走临时目录 + rename + 失败回滚

use crate::error::{AppError, AppResult};
use crate::file_mapper;
use crate::git_sync;
use crate::types::{AgentConfig, Persona, PersonaFileContent};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 列出 agent 的所有人格
///
/// 扫描 agent 目录下除 `_current/` 外的子目录（每个人格一个文件夹）。
pub fn list_personalities(repo_path: &Path, agent_id: &str) -> AppResult<Vec<Persona>> {
    let agent_dir = repo_path.join(agent_id);
    if !agent_dir.exists() {
        return Ok(vec![]);
    }

    // 当前人格从 _current/ 推断（无法可靠知道，返回 null，由前端根据 sync 状态判断）
    let mut personas = Vec::new();
    for entry in fs::read_dir(&agent_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // 跳过 _current/ 和临时目录
        if name.starts_with('_') {
            continue;
        }

        let persona_dir = entry.path();
        let files = list_files_recursive(&persona_dir)?;
        let size_bytes: u64 = files
            .iter()
            .filter_map(|f| fs::metadata(persona_dir.join(f)).ok().map(|m| m.len()))
            .sum();

        personas.push(Persona {
            agent_id: agent_id.to_string(),
            display_name: name.replace('-', " "),
            name,
            files,
            size_bytes,
            is_current: false, // 由前端根据 currentPersona 字段判断
            imported_at: None,
        });
    }

    personas.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(personas)
}

/// 读取人格文件内容及其对应的本地文件内容
///
/// 用于 Personalities 视图文件预览。`file_path` 为相对 agent 目录的路径
/// （如 "SOUL.md"、"memory/chat.md"），人格目录与本地 config_dir 同构，
/// 直接 join 相对路径即可定位本地文件。
///
/// - 二进制文件（含 0x00 字节）：`is_binary=true`，两 content 均为 None
/// - 任一文件不存在/编码异常：对应 content 为 None，不影响另一个
/// - 路径含 `..`：返回 Err（防目录穿越）
pub fn read_persona_file(
    repo_path: &Path,
    agent_config: &AgentConfig,
    persona_name: &str,
    file_path: &str,
) -> AppResult<PersonaFileContent> {
    // 路径安全：拒绝含 .. 的路径（防目录穿越）
    if file_path.split('/').any(|c| c == "..") || file_path.split('\\').any(|c| c == "..") {
        return Err(AppError::Config(format!(
            "非法文件路径 '{}': 含 .. 组件",
            file_path
        )));
    }

    let persona_path = repo_path
        .join(&agent_config.id)
        .join(persona_name)
        .join(file_path);
    let local_dir = file_mapper::expand_tilde(&agent_config.config_dir)?;
    let local_path = PathBuf::from(&local_dir).join(file_path);

    // 读取并检测二进制
    let read_text = |path: &Path| -> Option<String> {
        let bytes = fs::read(path).ok()?;
        if bytes.contains(&0u8) {
            return None; // 二进制
        }
        String::from_utf8(bytes).ok()
    };

    // 先读 bytes 判断二进制（任一为二进制则整体标记）
    let persona_bytes = fs::read(&persona_path).unwrap_or_default();
    let local_bytes = fs::read(&local_path).unwrap_or_default();
    let is_binary = persona_bytes.contains(&0u8) || local_bytes.contains(&0u8);

    if is_binary {
        return Ok(PersonaFileContent {
            persona_content: None,
            local_content: None,
            is_binary: true,
        });
    }

    Ok(PersonaFileContent {
        persona_content: read_text(&persona_path),
        local_content: read_text(&local_path),
        is_binary: false,
    })
}

/// 1. 保存当前为人格
///
/// 对应文档：
/// 1. 输入人格名称
/// 2. 拷贝 _current/ -> 新人格文件夹
/// 3. git commit + push
pub fn save_personality(
    repo_path: &Path,
    agent_id: &str,
    name: &str,
    pat: &str,
) -> AppResult<()> {
    let agent_dir = repo_path.join(agent_id);
    let current_dir = agent_dir.join("_current");
    let persona_dir = agent_dir.join(name);

    if persona_dir.exists() {
        return Err(AppError::Config(format!(
            "人格 '{}' 已存在",
            name
        )));
    }
    if !current_dir.exists() {
        return Err(AppError::Config(format!(
            "agent '{}' 的 _current/ 不存在，无法保存",
            agent_id
        )));
    }

    // 拷贝 _current/ -> 新人格文件夹
    copy_dir_recursive(&current_dir, &persona_dir)?;

    // commit + push
    let repo = git2::Repository::open(repo_path)?;
    let message = format!("save persona: {}/{}", agent_id, name);
    git_sync::commit(&repo, &message)?;
    let _ = git_sync::push(&repo, pat);

    Ok(())
}

/// 2. 切换到指定人格
///
/// 对应文档：
/// 1. 人格文件夹 -> 覆盖 _current/（先备份当前状态）
/// 2. _current/ -> 本地配置目录
/// 3. git add _current/ && commit
///
/// 原子操作：先写临时目录，成功后 rename，失败回滚
pub fn switch_personality(
    repo_path: &Path,
    app_data_dir: &Path,
    agent_id: &str,
    name: &str,
    config_dir: &str,
    sync_files: &[String],
    exclude_files: &[String],
    pat: &str,
) -> AppResult<()> {
    let agent_dir = repo_path.join(agent_id);
    let persona_dir = agent_dir.join(name);
    let current_dir = agent_dir.join("_current");
    let local_config_dir = PathBuf::from(file_mapper::expand_tilde(config_dir)?);

    if !persona_dir.exists() {
        return Err(AppError::Config(format!(
            "人格 '{}' 不存在",
            name
        )));
    }

    // 切换前快照（用于失败回滚 + 撤销）
    let timestamp = chrono::Utc::now().timestamp_millis();
    let snapshot_dir = app_data_dir
        .join("snapshots")
        .join(format!("switch_{}_{}", agent_id, timestamp));

    // 1. 备份当前 _current/ 到快照
    if current_dir.exists() {
        copy_dir_recursive(&current_dir, &snapshot_dir)?;
    }

    // 2. 原子覆盖 _current/（先写临时目录再 rename）
    let tmp_dir = agent_dir.join("_tmp_switch");
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir)?;
    }
    copy_dir_recursive(&persona_dir, &tmp_dir)?;

    // 校验临时目录完整性
    if !verify_dir_complete(&tmp_dir)? {
        // 校验失败，回滚
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(AppError::Io("切换校验失败：临时目录不完整".into()));
    }

    // 替换 _current/
    let old_current = agent_dir.join("_current_old");
    if current_dir.exists() {
        fs::rename(&current_dir, &old_current)?;
    }
    if let Err(e) = fs::rename(&tmp_dir, &current_dir) {
        // rename 失败，恢复
        if old_current.exists() {
            let _ = fs::rename(&old_current, &current_dir);
        }
        return Err(AppError::Io(format!("切换 rename 失败: {}", e)));
    }
    // 清理 old
    if old_current.exists() {
        let _ = fs::remove_dir_all(&old_current);
    }

    // 3. _current/ -> 本地配置目录（原子写入）
    let staging_dir = app_data_dir
        .join("tmp")
        .join(format!("switch_{}_{}", agent_id, timestamp));
    if let Err(e) = file_mapper::copy_current_to_local_atomic(
        &current_dir,
        &local_config_dir,
        &staging_dir,
        sync_files,
        exclude_files,
    ) {
        // 写回本地失败，从快照恢复 _current/
        log::error!("切换后写回本地失败，回滚 _current/: {}", e);
        if snapshot_dir.exists() {
            let _ = fs::remove_dir_all(&current_dir);
            let _ = copy_dir_recursive(&snapshot_dir, &current_dir);
        }
        return Err(e);
    }

    // 4. commit（让工作区恢复干净）
    let repo = git2::Repository::open(repo_path)?;
    let message = format!("switch persona: {}/{}", agent_id, name);
    git_sync::commit(&repo, &message)?;
    let _ = git_sync::push(&repo, pat);

    // 5. 清理快照（成功后删除，失败时保留供撤销）
    let _ = file_mapper::remove_snapshot(&snapshot_dir);

    Ok(())
}

/// 3. 删除人格
pub fn delete_personality(repo_path: &Path, agent_id: &str, name: &str, pat: &str) -> AppResult<()> {
    let persona_dir = repo_path.join(agent_id).join(name);
    if !persona_dir.exists() {
        return Err(AppError::Config(format!("人格 '{}' 不存在", name)));
    }

    fs::remove_dir_all(&persona_dir)?;

    let repo = git2::Repository::open(repo_path)?;
    let message = format!("delete persona: {}/{}", agent_id, name);
    git_sync::commit(&repo, &message)?;
    let _ = git_sync::push(&repo, pat);

    Ok(())
}

/// 4. 导出人格包
///
/// 打包为 .zip（含 manifest.json）。
/// 对应文档 manifest.json 结构。
pub fn export_personalities(
    repo_path: &Path,
    agent_id: &str,
    names: &[String],
    output_path: &Path,
) -> AppResult<String> {
    let file = fs::File::create(output_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 写 manifest.json
    let manifest = serde_json::json!({
        "format": "agentsync-persona",
        "version": "1.0",
        "agentType": agent_id,
        "personalities": names.iter().map(|name| {
            let persona_dir = repo_path.join(agent_id).join(name);
            let files = list_files_recursive(&persona_dir).unwrap_or_default();
            serde_json::json!({
                "name": name,
                "displayName": name.replace('-', " "),
                "files": files
            })
        }).collect::<Vec<_>>()
    });
    zip.start_file("manifest.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    // 写每个人格的文件
    for name in names {
        let persona_dir = repo_path.join(agent_id).join(name);
        if !persona_dir.exists() {
            continue;
        }
        let files = list_files_recursive(&persona_dir)?;
        for rel in files {
            let src = persona_dir.join(&rel);
            let zip_path = format!("{}/{}", name, rel.replace('\\', "/"));
            zip.start_file(&zip_path, options)?;
            let mut f = fs::File::open(&src)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
    }

    zip.finish()?;
    Ok(output_path.to_string_lossy().to_string())
}

/// 4. 导入人格包 -- 预览 diff
///
/// 解压 zip 但不写入，返回 diff 预览供前端展示。
pub fn preview_import_personalities(
    zip_path: &Path,
    repo_path: &Path,
    agent_id: &str,
) -> AppResult<Vec<PersonaDiffPreview>> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // 读 manifest（index 0）
    let manifest_str = {
        let mut manifest_entry = archive.by_index(0)?;
        if manifest_entry.name() != "manifest.json" {
            return Err(AppError::Config("人格包缺少 manifest.json".into()));
        }
        let mut s = String::new();
        manifest_entry.read_to_string(&mut s)?;
        s
    };
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str)?;

    let manifest_agent_type = manifest["agentType"].as_str().unwrap_or("");
    if manifest_agent_type != agent_id {
        return Err(AppError::Config(format!(
            "agent 类型不兼容：包是 '{}'，当前 agent 是 '{}'",
            manifest_agent_type, agent_id
        )));
    }

    // 先把所有 zip 内文件读到内存（避免借用冲突）
    // key: "persona_name/relative_path" -> content
    let mut zip_files: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for i in 1..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
        zip_files.insert(entry_name, content);
    }

    // 逐人格对比
    let mut previews = Vec::new();
    let personalities = manifest["personalities"].as_array().cloned().unwrap_or_default();
    for p in personalities {
        let name = p["name"].as_str().unwrap_or("").to_string();
        let persona_dir = repo_path.join(agent_id).join(&name);
        let mut files_diff = Vec::new();

        for (entry_name, zip_content) in &zip_files {
            let prefix = format!("{}/", name);
            if !entry_name.starts_with(&prefix) {
                continue;
            }
            let rel = entry_name.strip_prefix(&prefix).unwrap().to_string();
            let local_path = persona_dir.join(&rel);
            let action = if !local_path.exists() {
                "added".to_string()
            } else {
                let local_content = fs::read(&local_path).unwrap_or_default();
                if local_content == *zip_content {
                    "unchanged".to_string()
                } else {
                    "modified".to_string()
                }
            };
            files_diff.push(FileDiff {
                path: rel,
                action,
            });
        }

        files_diff.sort_by(|a, b| a.path.cmp(&b.path));
        previews.push(PersonaDiffPreview {
            name: name.clone(),
            display_name: p["displayName"].as_str().unwrap_or(&name).to_string(),
            files: files_diff,
        });
    }

    Ok(previews)
}

/// 4. 导入人格包 -- 确认后解压
///
/// 用户确认后调用，实际写入文件。
pub fn import_personalities(
    zip_path: &Path,
    repo_path: &Path,
    agent_id: &str,
    pat: &str,
) -> AppResult<()> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // 跳过 manifest.json（index 0）
    for i in 1..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        let dest = repo_path.join(agent_id).join(&entry_name);
        if entry.is_dir() {
            fs::create_dir_all(&dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut f)?;
    }

    // commit + push
    let repo = git2::Repository::open(repo_path)?;
    git_sync::commit(&repo, &format!("import personas: {}", agent_id))?;
    let _ = git_sync::push(&repo, pat);

    Ok(())
}

/* ------------------------------------------------------------------ */
/* 辅助函数                                                            */
/* ------------------------------------------------------------------ */

/// 递归拷贝目录
fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// 列出目录下所有文件的相对路径
fn list_files_recursive(dir: &Path) -> AppResult<Vec<String>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir).min_depth(1) {
        let entry = entry.map_err(|e| AppError::Io(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        files.push(rel);
    }
    files.sort();
    Ok(files)
}

/// 校验目录完整性（非空且可读）
fn verify_dir_complete(dir: &Path) -> AppResult<bool> {
    if !dir.exists() {
        return Ok(false);
    }
    // 尝试列出文件，验证可读
    match fs::read_dir(dir) {
        Ok(mut entries) => Ok(entries.next().is_some()),
        Err(_) => Ok(false),
    }
}

/* ------------------------------------------------------------------ */
/* 导入预览类型                                                        */
/* ------------------------------------------------------------------ */

#[derive(serde::Serialize)]
pub struct PersonaDiffPreview {
    pub name: String,
    pub display_name: String,
    pub files: Vec<FileDiff>,
}

#[derive(serde::Serialize)]
pub struct FileDiff {
    pub path: String,
    pub action: String, // added / modified / unchanged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentConfig;
    use tempfile::TempDir;

    fn make_config(config_dir: &str) -> AgentConfig {
        AgentConfig {
            id: "test-agent".into(),
            display_name: "Test".into(),
            config_dir: config_dir.into(),
            sync_files: vec!["SOUL.md".into(), "memory/**".into()],
            exclude_files: vec![],
            accent_color: Some("#5B4FE9".into()),
        }
    }

    fn write_file(base: &Path, rel: &str, content: &str) {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn read_persona_file_both_exist() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        // 仓库结构: repo/test-agent/work-mode/SOUL.md
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        write_file(repo.path(), "test-agent/work-mode/SOUL.md", "persona version");

        // 本地结构: local/SOUL.md
        write_file(local.path(), "SOUL.md", "local version");

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "SOUL.md",
        )
        .unwrap();

        assert!(!result.is_binary);
        assert_eq!(result.persona_content.as_deref(), Some("persona version"));
        assert_eq!(result.local_content.as_deref(), Some("local version"));
    }

    #[test]
    fn read_persona_file_local_missing() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        write_file(repo.path(), "test-agent/work-mode/SOUL.md", "only in persona");

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "SOUL.md",
        )
        .unwrap();

        assert!(!result.is_binary);
        assert_eq!(result.persona_content.as_deref(), Some("only in persona"));
        assert_eq!(result.local_content, None);
    }

    #[test]
    fn read_persona_file_binary_detected() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        // 写入含 0x00 字节的二进制内容
        std::fs::write(
            repo.path().join("test-agent/work-mode/blob.bin"),
            [0x42, 0x00, 0x43],
        )
        .unwrap();

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "blob.bin",
        )
        .unwrap();

        assert!(result.is_binary);
        assert_eq!(result.persona_content, None);
        assert_eq!(result.local_content, None);
    }

    #[test]
    fn read_persona_file_rejects_path_traversal() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let config = make_config(local.path().to_str().unwrap());

        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "../../../etc/passwd",
        );

        assert!(result.is_err());
    }

    #[test]
    fn read_persona_file_rejects_backslash_traversal() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let config = make_config(local.path().to_str().unwrap());

        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "..\\..\\..\\etc\\passwd",
        );

        assert!(result.is_err());
    }

    #[test]
    fn read_persona_file_persona_missing() {
        let repo = TempDir::new().unwrap();
        let local = TempDir::new().unwrap();
        let persona_dir = repo.path().join("test-agent").join("work-mode");
        std::fs::create_dir_all(&persona_dir).unwrap();
        // 人格目录存在但文件不存在；本地有文件
        write_file(local.path(), "SOUL.md", "local only");

        let config = make_config(local.path().to_str().unwrap());
        let result = read_persona_file(
            repo.path(),
            &config,
            "work-mode",
            "SOUL.md",
        )
        .unwrap();

        assert!(!result.is_binary);
        assert_eq!(result.persona_content, None);
        assert_eq!(result.local_content.as_deref(), Some("local only"));
    }
}
