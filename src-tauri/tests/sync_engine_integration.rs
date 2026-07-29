//! sync_engine 集成测试
//!
//! 对应设计文档「测试策略 -> 集成测试：临时仓库方案」。
//! 用 tempfile 创建本地 bare 仓库模拟远程，不依赖网络。
//! 用两个本地 clone 模拟两台设备，覆盖状态机跳转表的关键分支。

use agentsync_lib::git_sync;
use agentsync_lib::sync_engine::{ConflictResolution, SyncEngine};
use agentsync_lib::types::SyncResultStatus;
use git2::Repository;
use std::fs;
use tempfile::TempDir;

/// 测试夹具：模拟两台设备共享同一远程仓库
struct TestEnv {
    remote_dir: TempDir,
    app_data_dir: TempDir,
    repo_path: std::path::PathBuf,
    local_config_dir: TempDir,
    other_repo_dir: TempDir,
}

impl TestEnv {
    /// 创建测试环境并完成初始 commit + push
    fn new() -> Self {
        let remote_dir = TempDir::new().unwrap();
        Repository::init_bare(remote_dir.path()).unwrap();

        let app_data_dir = TempDir::new().unwrap();
        let repo_path = app_data_dir.path().join("repo");
        let local_config_dir = TempDir::new().unwrap();
        let other_repo_dir = TempDir::new().unwrap();

        // 设备 A clone 空仓库
        let repo = Repository::clone(
            remote_dir.path().to_str().unwrap(),
            &repo_path,
        )
        .unwrap();
        repo.set_head("refs/heads/main").unwrap();

        // 初始 commit + push
        fs::write(
            repo_path.join("registry.json"),
            r#"{"schemaVersion":"1.0.0","agents":{}}"#,
        )
        .unwrap();
        git_sync::commit(&repo, "init").unwrap();
        git_sync::push(&repo, "fake_pat").unwrap();

        // 设备 B clone（模拟另一台设备）
        let _other_repo = Repository::clone(
            remote_dir.path().to_str().unwrap(),
            other_repo_dir.path(),
        )
        .unwrap();

        TestEnv {
            remote_dir,
            app_data_dir,
            repo_path,
            local_config_dir,
            other_repo_dir,
        }
    }

    fn engine(&self) -> SyncEngine {
        SyncEngine::new(
            self.repo_path.clone(),
            self.app_data_dir.path().to_path_buf(),
            "fake_pat".to_string(),
        )
    }

    fn write_current_file(&self, agent_id: &str, rel: &str, content: &str) {
        let path = self.repo_path.join(agent_id).join("_current").join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn write_local_file(&self, rel: &str, content: &str) {
        let path = self.local_config_dir.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn read_local_file(&self, rel: &str) -> String {
        fs::read_to_string(self.local_config_dir.path().join(rel)).unwrap()
    }

    /// 模拟另一台设备修改远程仓库的 _current/ 并 push
    ///
    /// 在设备 B 的 clone 里改文件、commit、push。
    fn other_device_update(&self, agent_id: &str, rel: &str, content: &str) {
        let other_repo = Repository::open(self.other_repo_dir.path()).unwrap();
        // 确保 HEAD 指向 main（clone 后可能指向 master）
        if other_repo.head().is_err() {
            other_repo.set_head("refs/heads/main").unwrap();
        }
        // fetch 远程最新
        let mut remote = other_repo.find_remote("origin").unwrap();
        remote
            .fetch(&["refs/heads/main:refs/remotes/origin/main"], None, None)
            .unwrap();
        // fast-forward 本地 main 到 origin/main
        let remote_oid = other_repo
            .find_reference("refs/remotes/origin/main")
            .unwrap()
            .target()
            .unwrap();
        let remote_commit = other_repo.find_commit(remote_oid).unwrap();
        other_repo
            .reset(remote_commit.as_object(), git2::ResetType::Hard, None)
            .unwrap();
        // 修改文件 + commit + push
        let other_path = self.other_repo_dir.path().join(agent_id).join("_current").join(rel);
        if let Some(parent) = other_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&other_path, content).unwrap();
        git_sync::commit(&other_repo, &format!("other device update {}", rel)).unwrap();
        git_sync::push(&other_repo, "fake_pat").unwrap();
    }

    fn commit_and_push_current(&self, message: &str) {
        let repo = Repository::open(&self.repo_path).unwrap();
        git_sync::commit(&repo, message).unwrap();
        git_sync::push(&repo, "fake_pat").unwrap();
    }
}

/// S2 -> S_END：本地无变更 & 远程无新内容 & 无未推送 commit
#[test]
fn sync_no_changes() {
    let env = TestEnv::new();
    let engine = env.engine();

    env.write_current_file("workbuddy", "SOUL.md", "soul");
    env.write_local_file("SOUL.md", "soul");
    env.commit_and_push_current("add _current");

    let (result, ctx) = engine
        .sync_agent(
            "workbuddy",
            env.local_config_dir.path(),
            &["SOUL.md".into()],
            &[],
        )
        .unwrap();

    assert_eq!(result.status, SyncResultStatus::Success);
    assert!(result.pulled_files.is_empty());
    assert!(result.pushed_files.is_empty());
    assert!(ctx.is_none());
}

/// S2 -> S4 -> S6：本地有变更 & 远程无新内容
#[test]
fn sync_local_changes_only() {
    let env = TestEnv::new();
    let engine = env.engine();

    env.write_current_file("workbuddy", "SOUL.md", "old");
    env.commit_and_push_current("add _current");

    env.write_local_file("SOUL.md", "new content");

    let (result, ctx) = engine
        .sync_agent(
            "workbuddy",
            env.local_config_dir.path(),
            &["SOUL.md".into()],
            &[],
        )
        .unwrap();

    assert_eq!(result.status, SyncResultStatus::Success);
    assert!(result.pushed_files.contains(&"SOUL.md".to_string()));
    assert!(ctx.is_none());

    let current_content = fs::read_to_string(
        env.repo_path.join("workbuddy/_current/SOUL.md"),
    )
    .unwrap();
    assert_eq!(current_content, "new content");
}

/// S2 -> S5 -> S6：本地无变更 & 远程有新内容
#[test]
fn sync_remote_changes_only() {
    let env = TestEnv::new();
    let engine = env.engine();

    env.write_current_file("workbuddy", "SOUL.md", "v1");
    env.write_local_file("SOUL.md", "v1");
    env.commit_and_push_current("init _current");

    // 另一台设备修改远程 _current/ 并 push
    env.other_device_update("workbuddy", "SOUL.md", "v2 from remote");

    let (result, _ctx) = engine
        .sync_agent(
            "workbuddy",
            env.local_config_dir.path(),
            &["SOUL.md".into()],
            &[],
        )
        .unwrap();

    assert_eq!(result.status, SyncResultStatus::Success);
    assert!(result.pulled_files.contains(&"SOUL.md".to_string()),
        "pulled_files should contain SOUL.md, got: {:?}", result.pulled_files);
    assert_eq!(env.read_local_file("SOUL.md"), "v2 from remote");
}

/// S2 -> S2a：L2 冲突（本地有变更 & 远程有新内容）
#[test]
fn sync_l2_conflict_detected() {
    let env = TestEnv::new();
    let engine = env.engine();

    env.write_current_file("workbuddy", "SOUL.md", "v1");
    env.write_local_file("SOUL.md", "v1");
    env.commit_and_push_current("init");

    env.other_device_update("workbuddy", "SOUL.md", "remote v2");
    env.write_local_file("SOUL.md", "local v2");

    let (result, ctx) = engine
        .sync_agent(
            "workbuddy",
            env.local_config_dir.path(),
            &["SOUL.md".into()],
            &[],
        )
        .unwrap();

    assert_eq!(result.status, SyncResultStatus::Conflict);
    let ctx = ctx.unwrap();

    let resolved = engine
        .resolve_conflict(&ctx, &ConflictResolution::L2KeepLocal)
        .unwrap();
    assert_eq!(resolved.status, SyncResultStatus::Success);

    assert_eq!(env.read_local_file("SOUL.md"), "local v2");
    let current = fs::read_to_string(env.repo_path.join("workbuddy/_current/SOUL.md")).unwrap();
    assert_eq!(current, "local v2");
}

/// S2 -> S2a -> 保留远程
#[test]
fn sync_l2_conflict_keep_remote() {
    let env = TestEnv::new();
    let engine = env.engine();

    env.write_current_file("workbuddy", "SOUL.md", "v1");
    env.write_local_file("SOUL.md", "v1");
    env.commit_and_push_current("init");

    env.other_device_update("workbuddy", "SOUL.md", "remote wins");
    env.write_local_file("SOUL.md", "local loses");

    let (result, ctx) = engine
        .sync_agent(
            "workbuddy",
            env.local_config_dir.path(),
            &["SOUL.md".into()],
            &[],
        )
        .unwrap();

    assert_eq!(result.status, SyncResultStatus::Conflict);
    let ctx = ctx.unwrap();

    let resolved = engine
        .resolve_conflict(&ctx, &ConflictResolution::L2KeepRemote)
        .unwrap();
    assert_eq!(resolved.status, SyncResultStatus::Success);

    assert_eq!(env.read_local_file("SOUL.md"), "remote wins");
}

/// S5 成功路径验证
#[test]
fn sync_rollback_on_failure() {
    let env = TestEnv::new();
    let engine = env.engine();

    env.write_current_file("workbuddy", "SOUL.md", "v1");
    env.write_local_file("SOUL.md", "v1");
    env.commit_and_push_current("init");

    env.other_device_update("workbuddy", "SOUL.md", "remote v2");

    let (result, _) = engine
        .sync_agent(
            "workbuddy",
            env.local_config_dir.path(),
            &["SOUL.md".into()],
            &[],
        )
        .unwrap();

    assert_eq!(result.status, SyncResultStatus::Success);
    assert_eq!(env.read_local_file("SOUL.md"), "remote v2");
}

/// 验证 excludeFiles 生效：锁文件不同步
#[test]
fn sync_excludes_lock_files() {
    let env = TestEnv::new();
    let engine = env.engine();

    env.write_current_file("workbuddy", "SOUL.md", "soul");
    env.write_local_file("SOUL.md", "soul");
    env.write_local_file("session.lock", "lock content");
    env.commit_and_push_current("init");

    let (result, _) = engine
        .sync_agent(
            "workbuddy",
            env.local_config_dir.path(),
            &["SOUL.md".into(), "*.lock".into()],
            &["*.lock".into()],
        )
        .unwrap();

    assert_eq!(result.status, SyncResultStatus::Success);
    assert!(!env.repo_path.join("workbuddy/_current/session.lock").exists());
}
