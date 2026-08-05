//! 同步引擎
//!
//! 实现设计文档「状态机跳转表」的完整同步流程（S0-S_END）。
//!
//! 状态机说明：
//! - S0/S1/S2/S4/S5/S6/S_ROLLBACK/S_END：非交互分支，本模块直接处理
//! - S1a（L1 冲突弹窗）/ S2a（L2 冲突弹窗）/ S2b（手动合并）：交互分支
//!   MVP 策略：检测到冲突时返回 SyncResult{status:'conflict'}，
//!   前端弹窗收集用户决策后，调用 resolve_conflict 命令传回决策继续流程。
//!   冲突期间本地配置目录与 `_current/` 保持原状（不变量）。

use crate::error::AppResult;
use crate::file_mapper;
use crate::git_sync;
use crate::types::{SyncResult, SyncResultStatus};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 同步引擎上下文
pub struct SyncEngine {
    /// 仓库本地路径（app_data_dir/repo/）
    pub repo_path: PathBuf,
    /// 应用数据目录（app_data_dir/）
    pub app_data_dir: PathBuf,
    /// PAT token
    pub pat: String,
}

/// 单次同步的内部状态（用于冲突恢复后继续）
#[derive(Debug, Clone)]
pub struct SyncContext {
    pub agent_id: String,
    pub local_config_dir: PathBuf,
    pub current_dir: PathBuf,
    pub snapshot_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub sync_files: Vec<String>,
    pub exclude_files: Vec<String>,
    pub remote_has_new: bool,
    pub local_only: Vec<String>,
    pub current_only: Vec<String>,
    pub modified: Vec<String>,
}

/// L1/L2 冲突的用户决策
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ConflictResolution {
    /// L1 冲突：导出 patch 文件
    L1ExportPatch,
    /// L1 冲突：放弃本地未推送 commit
    L1DiscardLocal,
    /// L1 冲突：取消同步
    L1Cancel,
    /// L2 冲突：保留本地版本
    L2KeepLocal,
    /// L2 冲突：保留远程版本
    L2KeepRemote,
    /// L2 冲突：手动合并完成（合并结果已写入指定路径）
    L2ManualMerge { merged_files: Vec<String> },
    /// L2 冲突：取消
    L2Cancel,
}

impl SyncEngine {
    pub fn new(repo_path: PathBuf, app_data_dir: PathBuf, pat: String) -> Self {
        Self {
            repo_path,
            app_data_dir,
            pat,
        }
    }

    /// 执行单个 agent 的同步流程（S0 -> S_END）
    ///
    /// 对应状态机跳转表的非交互分支。
    /// 遇到冲突（S1a/S2a）时返回 `SyncResult{status:'conflict'}`，
    /// 调用方需保存 SyncContext 供 resolve_conflict 使用。
    pub fn sync_agent(
        &self,
        agent_id: &str,
        local_config_dir: &Path,
        sync_files: &[String],
        exclude_files: &[String],
    ) -> AppResult<(SyncResult, Option<SyncContext>)> {
        let start = Instant::now();
        let agent_dir = self.repo_path.join(agent_id);
        let current_dir = agent_dir.join("_current");
        let timestamp = chrono::Utc::now().timestamp_millis();
        let snapshot_dir = self
            .app_data_dir
            .join("snapshots")
            .join(format!("{}_{}", agent_id, timestamp));
        let staging_dir = self
            .app_data_dir
            .join("tmp")
            .join(format!("{}_{}", agent_id, timestamp));

        // S0：创建本地快照（L2 agent 级原子性回滚用）
        file_mapper::create_snapshot(local_config_dir, &snapshot_dir, sync_files, exclude_files)?;

        let repo = git2::Repository::open(&self.repo_path)?;

        // S2（提前）：diff 本地配置目录 vs _current/（基于 pull 前的基线）
        // 必须在 pull 之前做，否则 pull 更新 _current 后会把远程变更误判为本地变更
        let (local_only, current_only, modified) =
            file_mapper::diff_dirs(local_config_dir, &current_dir, sync_files, exclude_files)?;
        let local_has_changes = !local_only.is_empty() || !modified.is_empty();

        // S1：记录 pull 前的 local HEAD，用于判断 pull 是否拉到了新内容
        let pre_pull_oid = repo.head().ok().and_then(|h| h.target());

        // S1：git pull --rebase
        let pull_outcome = git_sync::pull_rebase(&repo, &self.pat)?;
        if pull_outcome == git_sync::PullOutcome::L1Conflict {
            // S1a：L1 冲突，返回 conflict 让前端弹窗
            let ctx = SyncContext {
                agent_id: agent_id.to_string(),
                local_config_dir: local_config_dir.to_path_buf(),
                current_dir: current_dir.clone(),
                snapshot_dir: snapshot_dir.clone(),
                staging_dir: staging_dir.clone(),
                sync_files: sync_files.to_vec(),
                exclude_files: exclude_files.to_vec(),
                remote_has_new: false,
                local_only: vec![],
                current_only: vec![],
                modified: vec![],
            };
            let _ = file_mapper::remove_snapshot(&snapshot_dir);
            return Ok((
                SyncResult {
                    agent_id: agent_id.to_string(),
                    status: SyncResultStatus::Conflict,
                    pulled_files: vec![],
                    pushed_files: vec![],
                    conflict_files: vec![],
                    error_message: Some("L1 git 冲突，等待用户决策".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Some(ctx),
            ));
        }

        // 判断远程是否有新内容：pull 后 HEAD 变化说明拉到了新 commit
        let post_pull_oid = repo.head().ok().and_then(|h| h.target());
        let remote_has_new = match (pre_pull_oid, post_pull_oid) {
            (Some(pre), Some(post)) => pre != post,
            _ => false,
        };

        // S2 分支判断
        if local_has_changes && remote_has_new {
            // S2 -> S2a：L2 冲突
            let conflict_files: Vec<String> = local_only
                .iter()
                .chain(current_only.iter())
                .chain(modified.iter())
                .cloned()
                .collect();
            let ctx = SyncContext {
                agent_id: agent_id.to_string(),
                local_config_dir: local_config_dir.to_path_buf(),
                current_dir: current_dir.clone(),
                snapshot_dir: snapshot_dir.clone(),
                staging_dir: staging_dir.clone(),
                sync_files: sync_files.to_vec(),
                exclude_files: exclude_files.to_vec(),
                remote_has_new,
                local_only,
                current_only,
                modified,
            };
            Ok((
                SyncResult {
                    agent_id: agent_id.to_string(),
                    status: SyncResultStatus::Conflict,
                    pulled_files: vec![],
                    pushed_files: vec![],
                    conflict_files,
                    error_message: Some("L2 应用冲突，等待用户决策".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                Some(ctx),
            ))
        } else if !local_has_changes && !remote_has_new {
            // 本地无变更 & 远程无新内容
            if git_sync::has_unpushed_commits(&repo)? {
                // S2 -> S6：有未推送 commit（切换 commit 等场景）
                let (committed, pushed) =
                    git_sync::commit_and_push(&repo, &self.pat, &format!("sync: {}", agent_id))?;
                let _ = file_mapper::remove_snapshot(&snapshot_dir);
                Ok((
                    self.build_result(agent_id, vec![], vec![], committed, pushed, start),
                    None,
                ))
            } else {
                // S2 -> S_END：真无变化
                let _ = file_mapper::remove_snapshot(&snapshot_dir);
                Ok((
                    SyncResult {
                        agent_id: agent_id.to_string(),
                        status: SyncResultStatus::Success,
                        pulled_files: vec![],
                        pushed_files: vec![],
                        conflict_files: vec![],
                        error_message: None,
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                    None,
                ))
            }
        } else if !local_has_changes && remote_has_new {
            // S2 -> S5：仅远程有新内容，写回本地
            self.run_s5(
                agent_id,
                &current_dir,
                local_config_dir,
                &staging_dir,
                &snapshot_dir,
                sync_files,
                exclude_files,
                &repo,
                start,
            )
        } else {
            // local_has_changes && !remote_has_new
            // S2 -> S4：合并本地变更到 _current/
            self.run_s4_then_s6(
                agent_id,
                local_config_dir,
                &current_dir,
                &snapshot_dir,
                sync_files,
                exclude_files,
                &repo,
                start,
            )
        }
    }

    /// S4：合并本地变更到 `_current/`，然后 S6
    #[allow(clippy::too_many_arguments)]
    fn run_s4_then_s6(
        &self,
        agent_id: &str,
        local_config_dir: &Path,
        current_dir: &Path,
        snapshot_dir: &Path,
        sync_files: &[String],
        exclude_files: &[String],
        repo: &git2::Repository,
        start: Instant,
    ) -> AppResult<(SyncResult, Option<SyncContext>)> {
        // S4：本地配置目录 -> _current/
        let pushed = match file_mapper::copy_local_to_current(
            local_config_dir,
            current_dir,
            sync_files,
            exclude_files,
        ) {
            Ok(files) => files,
            Err(e) => {
                // S4 -> S_ROLLBACK
                let _ = file_mapper::restore_snapshot(snapshot_dir, local_config_dir);
                let _ = file_mapper::remove_snapshot(snapshot_dir);
                return Ok((
                    SyncResult {
                        agent_id: agent_id.to_string(),
                        status: SyncResultStatus::Error,
                        pulled_files: vec![],
                        pushed_files: vec![],
                        conflict_files: vec![],
                        error_message: Some(format!("S4 合并失败: {}", e)),
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                    None,
                ));
            }
        };

        // S6：commit + push
        let (committed, push_ok) = git_sync::commit_and_push(
            repo,
            &self.pat,
            &format!("sync: {} (local changes)", agent_id),
        )?;
        let _ = file_mapper::remove_snapshot(snapshot_dir);

        Ok((
            self.build_result(agent_id, vec![], pushed, committed, push_ok, start),
            None,
        ))
    }

    /// S5：仓库 `_current/` -> 本地配置目录（原子写入），然后 S6
    #[allow(clippy::too_many_arguments)]
    fn run_s5(
        &self,
        agent_id: &str,
        current_dir: &Path,
        local_config_dir: &Path,
        staging_dir: &Path,
        snapshot_dir: &Path,
        sync_files: &[String],
        exclude_files: &[String],
        repo: &git2::Repository,
        start: Instant,
    ) -> AppResult<(SyncResult, Option<SyncContext>)> {
        // S5：原子写入本地配置目录
        let pulled = match file_mapper::copy_current_to_local_atomic(
            current_dir,
            local_config_dir,
            staging_dir,
            sync_files,
            exclude_files,
        ) {
            Ok(files) => files,
            Err(e) => {
                // S5 -> S_ROLLBACK
                let _ = file_mapper::restore_snapshot(snapshot_dir, local_config_dir);
                let _ = file_mapper::remove_snapshot(snapshot_dir);
                return Ok((
                    SyncResult {
                        agent_id: agent_id.to_string(),
                        status: SyncResultStatus::Error,
                        pulled_files: vec![],
                        pushed_files: vec![],
                        conflict_files: vec![],
                        error_message: Some(format!("S5 写回本地失败: {}", e)),
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                    None,
                ));
            }
        };

        // S5 成功后删除快照（本地配置目录已是新内容）
        let _ = file_mapper::remove_snapshot(snapshot_dir);

        // S6：commit + push（远程内容已 pull，本地可能无新 commit）
        let (committed, push_ok) = git_sync::commit_and_push(
            repo,
            &self.pat,
            &format!("sync: {} (pulled remote)", agent_id),
        )?;

        Ok((
            self.build_result(agent_id, pulled, vec![], committed, push_ok, start),
            None,
        ))
    }

    /// 解决冲突后继续同步流程
    ///
    /// 接收用户的 ConflictResolution 决策，从 S1a/S2a/S2b 继续。
    pub fn resolve_conflict(
        &self,
        ctx: &SyncContext,
        resolution: &ConflictResolution,
    ) -> AppResult<SyncResult> {
        let start = Instant::now();
        let repo = git2::Repository::open(&self.repo_path)?;

        match resolution {
            ConflictResolution::L1ExportPatch => {
                // S1a -> S_END：导出 patch
                let patch_path = self.app_data_dir.join(format!(
                    "patches/{}_{}.patch",
                    ctx.agent_id,
                    chrono::Utc::now().timestamp_millis()
                ));
                self.export_patch(&repo, &patch_path)?;
                let _ = file_mapper::remove_snapshot(&ctx.snapshot_dir);
                Ok(SyncResult {
                    agent_id: ctx.agent_id.clone(),
                    status: SyncResultStatus::Skipped,
                    pulled_files: vec![],
                    pushed_files: vec![],
                    conflict_files: vec![],
                    error_message: Some(format!("patch 已导出到 {}", patch_path.display())),
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            }
            ConflictResolution::L1DiscardLocal => {
                // S1a -> S2：abort rebase + reset 到远程 HEAD
                git_sync::abort_rebase(&repo)?;
                let remote_ref = repo
                    .find_reference(&format!("refs/remotes/origin/{}", git_sync::DEFAULT_BRANCH))?;
                let remote_oid = remote_ref.target().unwrap();
                let remote_commit = repo.find_commit(remote_oid)?;
                repo.reset(remote_commit.as_object(), git2::ResetType::Hard, None)?;
                // 继续走 S2 流程（这里简化为重新 sync_agent）
                let (result, _) = self.sync_agent(
                    &ctx.agent_id,
                    &ctx.local_config_dir,
                    &ctx.sync_files,
                    &ctx.exclude_files,
                )?;
                Ok(result)
            }
            ConflictResolution::L1Cancel => {
                // S1a -> S_END：取消
                git_sync::abort_rebase(&repo)?;
                let _ = file_mapper::remove_snapshot(&ctx.snapshot_dir);
                Ok(SyncResult {
                    agent_id: ctx.agent_id.clone(),
                    status: SyncResultStatus::Skipped,
                    pulled_files: vec![],
                    pushed_files: vec![],
                    conflict_files: vec![],
                    error_message: Some("用户取消同步".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            }
            ConflictResolution::L2KeepLocal => {
                // S2a -> S4：保留本地，本地覆盖 _current/
                self.run_s4_then_s6(
                    &ctx.agent_id,
                    &ctx.local_config_dir,
                    &ctx.current_dir,
                    &ctx.snapshot_dir,
                    &ctx.sync_files,
                    &ctx.exclude_files,
                    &repo,
                    start,
                )
                .map(|(r, _)| r)
            }
            ConflictResolution::L2KeepRemote => {
                // S2a -> S5：保留远程，_current/ 写回本地
                self.run_s5(
                    &ctx.agent_id,
                    &ctx.current_dir,
                    &ctx.local_config_dir,
                    &ctx.staging_dir,
                    &ctx.snapshot_dir,
                    &ctx.sync_files,
                    &ctx.exclude_files,
                    &repo,
                    start,
                )
                .map(|(r, _)| r)
            }
            ConflictResolution::L2ManualMerge { merged_files: _ } => {
                // S2b -> S4：合并结果已写入本地，走 S4 拷贝到 _current/
                self.run_s4_then_s6(
                    &ctx.agent_id,
                    &ctx.local_config_dir,
                    &ctx.current_dir,
                    &ctx.snapshot_dir,
                    &ctx.sync_files,
                    &ctx.exclude_files,
                    &repo,
                    start,
                )
                .map(|(r, _)| r)
            }
            ConflictResolution::L2Cancel => {
                // S2b -> S_END：取消
                let _ = file_mapper::remove_snapshot(&ctx.snapshot_dir);
                Ok(SyncResult {
                    agent_id: ctx.agent_id.clone(),
                    status: SyncResultStatus::Skipped,
                    pulled_files: vec![],
                    pushed_files: vec![],
                    conflict_files: vec![],
                    error_message: Some("用户取消合并".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            }
        }
    }

    /// 导出未推送 commit 到 patch 文件
    fn export_patch(&self, repo: &git2::Repository, patch_path: &Path) -> AppResult<()> {
        if let Some(parent) = patch_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let remote_ref =
            repo.find_reference(&format!("refs/remotes/origin/{}", git_sync::DEFAULT_BRANCH))?;
        let remote_oid = remote_ref.target().unwrap();
        let remote_commit = repo.find_annotated_commit(remote_oid)?;
        let head = repo.head()?.peel_to_commit()?;

        let remote_tree = repo.find_commit(remote_commit.id())?.tree()?;
        let head_tree = head.tree()?;
        let diff = repo.diff_tree_to_tree(Some(&remote_tree), Some(&head_tree), None)?;
        let mut file = std::fs::File::create(patch_path)?;
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            use std::io::Write;
            let _ = file.write_all(line.content());
            true
        })?;
        Ok(())
    }

    /// 构建 SyncResult
    fn build_result(
        &self,
        agent_id: &str,
        pulled: Vec<String>,
        pushed: Vec<String>,
        _committed: bool,
        push_ok: bool,
        start: Instant,
    ) -> SyncResult {
        if push_ok {
            SyncResult {
                agent_id: agent_id.to_string(),
                status: SyncResultStatus::Success,
                pulled_files: pulled,
                pushed_files: pushed,
                conflict_files: vec![],
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
            }
        } else {
            // S_END_PARTIAL：push 失败，本地 commit 已生效
            SyncResult {
                agent_id: agent_id.to_string(),
                status: SyncResultStatus::Success,
                pulled_files: pulled,
                pushed_files: pushed,
                conflict_files: vec![],
                error_message: Some("push 失败，下次同步重试".into()),
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
    }
}

/// 清理 agent 的临时目录（S_END 副作用）
pub fn cleanup_tmp(app_data_dir: &Path, agent_id: &str) -> AppResult<()> {
    let tmp_dir = app_data_dir.join("tmp");
    if !tmp_dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&tmp_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(agent_id) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}
