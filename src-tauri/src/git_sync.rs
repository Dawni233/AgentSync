//! Git 同步模块
//!
//! 封装 git2 crate，处理 pull/commit/push/clone 和冲突检测。
//! 对应设计文档「git2 实现要点」章节。
//!
//! 关键规格（必须遵守）：
//! - PAT 认证：用 credential callback，用户名 "x-token"，PAT 作为密码
//! - **不能**把 PAT 拼到 URL（会被写入 .git/config 明文存盘）
//! - pull --rebase：rebase 冲突时 abort，返回 PullOutcome::L1Conflict
//! - 默认分支固定 main（MVP 不探测默认分支）
//! - 网络错误分类：Auth=阻断，Net/Ssl=重试，Repository+NotFound=阻断

use crate::error::{AppError, AppResult};
use git2::{Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository};
use std::path::Path;

/// 默认分支名（MVP 固定 main）
pub const DEFAULT_BRANCH: &str = "main";

/// `pull --rebase` 的结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullOutcome {
    /// rebase 无冲突，pull 成功
    Clean,
    /// 检测到 L1 git 冲突（本地未推送 commit 与远程 commit 改同一文件）
    L1Conflict,
}

/// 构建 PAT 认证的 callbacks
///
/// PAT 作为密码传入。用户名规则：
/// - GitHub：用户名任意，可用 "x-token"
/// - Gitee：用户名必须是实际账户名（从仓库 URL 解析）
fn make_callbacks(username: &str, pat: &str) -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    let pat_owned = pat.to_string();
    let username_owned = username.to_string();
    callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext(&username_owned, &pat_owned)
    });
    callbacks
}

/// 构建 PAT 认证的 fetch options
fn make_fetch_options(username: &str, pat: &str) -> FetchOptions<'static> {
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(make_callbacks(username, pat));
    fo
}

/// 构建 PAT 认证的 push options
fn make_push_options(username: &str, pat: &str) -> PushOptions<'static> {
    let mut po = PushOptions::new();
    po.remote_callbacks(make_callbacks(username, pat));
    po
}

/// 从仓库 URL 解析用户名
///
/// URL 格式：https://gitee.com/{username}/{repo}.git
/// GitHub 用户名任意，Gitee 必须是实际账户名。
/// 统一从 URL 提取，兼容两个平台。
pub fn extract_username_from_url(url: &str) -> String {
    let without_proto = url.split("://").nth(1).unwrap_or(url);
    let path = without_proto.split('/').nth(1).unwrap_or("");
    let path = path.trim_start_matches('/');
    let username = path.split('/').next().unwrap_or("");
    if username.is_empty() {
        "x-token".to_string()
    } else {
        username.to_string()
    }
}

/// clone 远程仓库到本地
pub fn clone_repo(url: &str, local_path: &Path, pat: &str) -> AppResult<Repository> {
    let username = extract_username_from_url(url);
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(make_fetch_options(&username, pat));
    let repo = builder
        .clone(url, local_path)
        .map_err(|e| AppError::Git(format!("clone 失败: {}", e)))?;
    Ok(repo)
}

/// pull --rebase，返回 PullOutcome 区分无冲突/L1 冲突
///
/// 流程：
/// 1. fetch 远程 main 分支
/// 2. 拿到远程 HEAD oid
/// 3. 启动 rebase（本地 HEAD onto 远程）
/// 4. 遍历 rebase 操作，遇到冲突则 abort 返回 L1Conflict
/// 5. 无冲突则 finish rebase
pub fn pull_rebase(repo: &Repository, pat: &str) -> AppResult<PullOutcome> {
    // 从 origin URL 解析用户名
    let url = repo
        .find_remote("origin")?
        .url()
        .unwrap_or("")
        .to_string();
    let username = extract_username_from_url(&url);

    // 1. fetch 远程
    let mut remote = repo.find_remote("origin")?;
    let refspec = format!("refs/heads/{}:refs/remotes/origin/{}", DEFAULT_BRANCH, DEFAULT_BRANCH);
    remote.fetch(&[&refspec], Some(&mut make_fetch_options(&username, pat)), None)?;

    // 2. 拿到远程 HEAD oid
    let remote_ref_name = format!("refs/remotes/origin/{}", DEFAULT_BRANCH);
    let remote_ref = repo.find_reference(&remote_ref_name)?;
    let remote_oid = remote_ref
        .target()
        .ok_or_else(|| AppError::Git("远程引用无 target".into()))?;

    // 3. 检查是否需要更新
    let head = repo.head()?;
    let local_oid = head
        .target()
        .ok_or_else(|| AppError::Git("本地 HEAD 无 target".into()))?;

    // 如果本地已经是远程的最新，无需操作
    if local_oid == remote_oid {
        return Ok(PullOutcome::Clean);
    }

    // 如果远程是本地的祖先（本地领先远程），无需操作
    let mb = repo.merge_base(local_oid, remote_oid)?;
    if mb == remote_oid {
        return Ok(PullOutcome::Clean);
    }

    // 如果本地是远程的祖先（fast-forward 场景），直接 fast-forward
    if mb == local_oid {
        let remote_commit = repo.find_commit(remote_oid)?;
        repo.reset(remote_commit.as_object(), git2::ResetType::Hard, None)?;
        return Ok(PullOutcome::Clean);
    }

    // 4. 启动 rebase（本地 HEAD onto 远程，处理本地有未推送 commit 的情况）
    let annotated_local = repo.find_annotated_commit(local_oid)?;
    let annotated_remote = repo.find_annotated_commit(remote_oid)?;

    let mut rebase = repo.rebase(
        Some(&annotated_local),
        None,
        Some(&annotated_remote),
        Some(&mut git2::RebaseOptions::default()),
    )?;

    // 5. 遍历 rebase 操作
    while let Some(op_result) = rebase.next() {
        match op_result {
            Ok(op) => {
                // op 仅用于获取信息，避免 unused 警告
                let _ = op.kind();
                // 尝试 commit 这个操作
                let sig = repo.signature()?;
                match rebase.commit(None, &sig, None) {
                    Ok(_) => continue,
                    Err(e) => {
                        // rebase.commit 返回 Applied 表示有冲突
                        let code = e.code();
                        rebase.abort()?;
                        if code == git2::ErrorCode::Applied {
                            return Ok(PullOutcome::L1Conflict);
                        }
                        return Err(AppError::Git(format!("rebase commit 失败: {}", e)));
                    }
                }
            }
            Err(e) => {
                rebase.abort()?;
                return Err(AppError::Git(format!("rebase 操作失败: {}", e)));
            }
        }
    }

    rebase.finish(None)?;
    Ok(PullOutcome::Clean)
}

/// abort rebase（L1 冲突时调用，恢复到 pull 前状态）
pub fn abort_rebase(repo: &Repository) -> AppResult<()> {
    // git2 没有直接判断是否在 rebase 中的 API，尝试 abort，失败则忽略
    if let Ok(mut rebase) = repo.open_rebase(None) {
        rebase.abort()?;
    }
    Ok(())
}

/// git add 全部变更（git add -A 语义）
pub fn add_all(repo: &Repository) -> AppResult<()> {
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    Ok(())
}

/// git commit
///
/// 如果没有变更返回 false（调用方据此决定是否 push）。
pub fn commit(repo: &Repository, message: &str) -> AppResult<bool> {
    add_all(repo)?;

    let tree_oid = {
        let mut index = repo.index()?;
        index.write_tree()?
    };
    let tree = repo.find_tree(tree_oid)?;

    // HEAD 可能是 unborn（空仓库首次 commit）或已有 commit
    let head_result = repo.head();
    match head_result {
        Ok(head) => {
            let parent = repo.find_commit(head.target().unwrap())?;
            // 检查是否有实际变更（tree 与 parent 的 tree 相同则跳过）
            if parent.tree_id() == tree_oid {
                return Ok(false);
            }
            let sig = repo.signature()?;
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                message,
                &tree,
                &[&parent],
            )?;
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // 空仓库首次 commit，无 parent
            let sig = repo.signature()?;
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                message,
                &tree,
                &[],
            )?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(true)
}

/// push 到远程 main 分支
pub fn push(repo: &Repository, pat: &str) -> AppResult<()> {
    let url = repo
        .find_remote("origin")?
        .url()
        .unwrap_or("")
        .to_string();
    let username = extract_username_from_url(&url);
    let mut remote = repo.find_remote("origin")?;
    let push_refspec = format!(
        "refs/heads/{}:refs/heads/{}",
        DEFAULT_BRANCH, DEFAULT_BRANCH
    );
    remote.push(&[&push_refspec], Some(&mut make_push_options(&username, pat)))?;
    Ok(())
}

/// 检查本地是否有未推送 commit
///
/// 比较 local HEAD 与 origin/main 的 oid。
pub fn has_unpushed_commits(repo: &Repository) -> AppResult<bool> {
    let head = repo.head()?;
    let local_oid = head
        .target()
        .ok_or_else(|| AppError::Git("本地 HEAD 无 target".into()))?;

    let remote_ref_name = format!("refs/remotes/origin/{}", DEFAULT_BRANCH);
    match repo.find_reference(&remote_ref_name) {
        Ok(remote_ref) => {
            let remote_oid = remote_ref
                .target()
                .ok_or_else(|| AppError::Git("远程引用无 target".into()))?;
            Ok(local_oid != remote_oid)
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => {
            // 远程引用不存在，视为有未推送
            Ok(true)
        }
        Err(e) => Err(e.into()),
    }
}

/// commit + push 组合
///
/// 返回 (committed, pushed)：
/// - committed：是否产生了新 commit
/// - pushed：push 是否成功（push 失败不阻断，调用方按 S_END_PARTIAL 处理）
pub fn commit_and_push(repo: &Repository, pat: &str, message: &str) -> AppResult<(bool, bool)> {
    let committed = commit(repo, message)?;
    if !committed {
        // 无新变更，但仍可能需要 push 未推送的 commit
        if !has_unpushed_commits(repo)? {
            return Ok((false, true));
        }
    }
    match push(repo, pat) {
        Ok(()) => Ok((committed, true)),
        Err(e) => {
            // push 失败：本地 commit 已生效，仅远程未更新
            // 按 S_END_PARTIAL 处理，不视为错误
            log::warn!("push 失败（本地 commit 已生效）: {}", e);
            Ok((committed, false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use tempfile::TempDir;

    /// 构造测试仓库：bare 仓库模拟远程 + clone 到本地
    fn setup_test_repo() -> (TempDir, TempDir, Repository) {
        let remote_dir = TempDir::new().unwrap();
        let _remote = Repository::init_bare(remote_dir.path()).unwrap();

        let local_dir = TempDir::new().unwrap();
        let repo = Repository::clone(
            remote_dir.path().to_str().unwrap(),
            local_dir.path(),
        )
        .unwrap();

        // 初始 commit（让 main 分支存在）
        let sig = repo.signature().unwrap();
        let tree_oid = {
            let mut index = repo.index().unwrap();
            // 写一个初始文件让 tree 非空
            std::fs::write(local_dir.path().join("init.txt"), "init").unwrap();
            index.add_path(Path::new("init.txt")).unwrap();
            index.write().unwrap();
            index.write_tree().unwrap()
        };
        {
            let tree = repo.find_tree(tree_oid).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();
        }

        // clone 已自动配置 origin 远程，无需重复添加
        // 确保 main 分支存在
        {
            let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch(DEFAULT_BRANCH, &head_commit, true).ok();
        }

        (remote_dir, local_dir, repo)
    }

    #[test]
    fn commit_with_no_changes_returns_false() {
        let (_remote, _local, repo) = setup_test_repo();
        let committed = commit(&repo, "empty commit").unwrap();
        assert!(!committed);
    }

    #[test]
    fn commit_with_changes_returns_true() {
        let (_remote, local_dir, repo) = setup_test_repo();
        std::fs::write(local_dir.path().join("new.txt"), "new content").unwrap();
        let committed = commit(&repo, "add new file").unwrap();
        assert!(committed);
    }

    #[test]
    fn has_unpushed_commits_after_commit() {
        let (_remote, local_dir, repo) = setup_test_repo();
        std::fs::write(local_dir.path().join("new.txt"), "new").unwrap();
        commit(&repo, "new").unwrap();
        assert!(has_unpushed_commits(&repo).unwrap());
    }

    #[test]
    fn pull_rebase_clean_when_remote_unchanged() {
        let (_remote, _local, repo) = setup_test_repo();
        // 先 push 建立 origin/main 引用
        // 使用本地文件协议 push（无需 PAT）
        let mut remote = repo.find_remote("origin").unwrap();
        remote
            .push(
                &[&format!("refs/heads/{}:refs/heads/{}", DEFAULT_BRANCH, DEFAULT_BRANCH)],
                None,
            )
            .unwrap();
        // 远程无变化，pull 应返回 Clean
        let outcome = pull_rebase(&repo, "fake_pat").unwrap();
        assert_eq!(outcome, PullOutcome::Clean);
    }
}
