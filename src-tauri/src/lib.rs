pub mod auto_sync;
pub mod db;
pub mod error;
pub mod file_mapper;
pub mod git_sync;
pub mod onboarding;
pub mod persona;
pub mod registry;
pub mod sync_engine;
pub mod types;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Listener, Manager};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use auto_sync::AutoSyncManager;
use db::Database;
use sync_engine::{ConflictResolution, SyncContext, SyncEngine};
use types::{Agent, AgentConfig, Persona, PersonaFileContent, Settings, SyncResult, SyncStatus};

/// 应用全局状态
struct AppState {
    db: Database,
    app_data_dir: PathBuf,
    repo_path: PathBuf,
    /// 当前进行中的同步上下文（冲突时保存，resolve 后清除）
    pending_sync: Mutex<Option<(String, SyncContext)>>,
    /// 自动同步管理器
    auto_sync: Arc<AutoSyncManager>,
}

/// IPC hello world 命令 -- 验证前后端通信
#[tauri::command]
fn ping(name: &str) -> String {
    format!("Hello, {}! AgentSync backend is alive.", name)
}

/// 返回应用数据目录路径
#[tauri::command]
fn get_app_data_dir(app: tauri::AppHandle) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取 app_data_dir 失败: {}", e))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 获取 settings
#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    state.db.get_settings().map_err(|e| e.to_string())
}

/// 保存 settings
#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, settings: Settings) -> Result<(), String> {
    state.db.save_settings(&settings).map_err(|e| e.to_string())
}

/// 检查仓库是否已初始化（供前端判断是否需要 onboarding）
#[tauri::command]
fn is_repo_initialized(state: tauri::State<'_, AppState>) -> bool {
    onboarding::is_repo_initialized(&state.app_data_dir)
}

/// 触发 onboarding 全流程
#[tauri::command]
fn init_app(
    state: tauri::State<'_, AppState>,
    repo_url: String,
    platform: String,
    pat_token: String,
    preset_agent_ids: Vec<String>,
    import_strategy: onboarding::ImportStrategyOption,
) -> Result<onboarding::InitAppResult, String> {
    // 保存 settings（含 PAT，供后续 sync 使用）
    let mut settings = state.db.get_settings().map_err(|e| e.to_string())?;
    settings.repo_url = repo_url.clone();
    settings.platform = match platform.as_str() {
        "github" => types::Platform::GitHub,
        _ => types::Platform::Gitee,
    };
    settings.pat_token = pat_token.clone();
    state.db.save_settings(&settings).map_err(|e| e.to_string())?;

    let params = onboarding::InitAppParams {
        repo_url,
        pat_token,
        preset_agent_ids,
        import_strategy,
    };
    let result = onboarding::init_app(&params, &state.app_data_dir).map_err(|e| e.to_string())?;

    // init_app 成功后，把 registry 里的 agent 配置写入 SQLite agent_cache 表
    // 否则主界面 get_agents 返回空列表
    if result.success {
        let registry_path = state.app_data_dir.join("repo").join("registry.json");
        if let Ok(registry) = registry::Registry::load(&registry_path) {
            for config in registry.list_agents() {
                if let Err(e) = state.db.upsert_agent(&config) {
                    log::warn!("写入 agent_cache 失败 {}: {}", config.id, e);
                }
            }
        }
    }

    Ok(result)
}

/// 测试 git 凭据
#[tauri::command]
fn test_git_auth(url: String, token: String) -> Result<bool, String> {
    // 从仓库 URL 解析用户名（Gitee/GitHub 的 URL 格式：https://gitee.com/{username}/{repo}.git）
    // Gitee 的 PAT 认证要求用户名是实际账户名，不能用任意值（与 GitHub 不同）
    let username = git_sync::extract_username_from_url(&url);

    let mut callbacks = git2::RemoteCallbacks::new();
    let token_owned = token.to_string();
    let username_owned = username.to_string();
    callbacks.credentials(move |_u, _username, _allowed| {
        git2::Cred::userpass_plaintext(&username_owned, &token_owned)
    });
    let mut remote = git2::Remote::create_detached(url.as_str()).map_err(|e| e.to_string())?;
    remote
        .connect_auth(git2::Direction::Fetch, Some(callbacks), None)
        .map_err(|e| e.to_string())?;
    remote.disconnect().ok();
    Ok(true)
}

/// 获取所有 agent
#[tauri::command]
fn get_agents(state: tauri::State<'_, AppState>) -> Result<Vec<Agent>, String> {
    let configs = state.db.list_agents().map_err(|e| e.to_string())?;
    let mut agents = Vec::new();
    for c in configs {
        // 计算实际跟踪文件数
        let current_dir = state.repo_path.join(&c.id).join("_current");
        let file_count = if current_dir.exists() {
            match file_mapper::build_matcher(&c.sync_files, &c.exclude_files) {
                Ok((inc, exc)) => {
                    file_mapper::list_syncable_files(&current_dir, &inc, &exc)
                        .unwrap_or_default()
                        .len() as u32
                }
                Err(_) => 0,
            }
        } else {
            0
        };
        // 读取运行时状态（含 current_persona）
        let (status_str, last_sync_at, current_persona) = state
            .db
            .get_agent_status(&c.id)
            .unwrap_or(("idle".to_string(), None, None));
        let sync_status = match status_str.as_str() {
            "syncing" => SyncStatus::Syncing,
            "pending" => SyncStatus::Pending,
            "conflict" => SyncStatus::Conflict,
            "error" => SyncStatus::Error,
            _ => SyncStatus::Idle,
        };
        agents.push(Agent {
            config: c,
            current_persona,
            sync_status,
            last_sync_at,
            tracked_file_count: file_count,
        });
    }
    Ok(agents)
}

/// 跟踪文件信息（供前端文件列表展示）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackedFileInfo {
    name: String,
    size_bytes: u64,
    modified_at: Option<i64>,
}

/// 列出 agent 的跟踪文件（基于 _current/ 目录 + syncFiles/excludeFiles 规则）
#[tauri::command]
fn list_tracked_files(
    state: tauri::State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<TrackedFileInfo>, String> {
    let config = state
        .db
        .list_agents()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == agent_id)
        .ok_or_else(|| format!("agent '{}' 未注册", agent_id))?;

    let current_dir = state.repo_path.join(&agent_id).join("_current");
    if !current_dir.exists() {
        return Ok(vec![]);
    }

    let (inc, exc) =
        file_mapper::build_matcher(&config.sync_files, &config.exclude_files).map_err(|e| e.to_string())?;
    let files = file_mapper::list_syncable_files(&current_dir, &inc, &exc)
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for rel in files {
        let path = current_dir.join(&rel);
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let modified_at = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        result.push(TrackedFileInfo {
            name: rel,
            size_bytes: size,
            modified_at,
        });
    }
    Ok(result)
}

/// 添加 agent
///
/// async 执行：写 SQLite + 更新 registry.json + 创建 _current/ + 首次导入 + commit/push
/// git 操作放后台线程，避免阻塞 UI
#[tauri::command]
async fn add_agent(
    state: tauri::State<'_, AppState>,
    config: AgentConfig,
) -> Result<(), String> {
    // 1. 写 SQLite（快速操作，直接执行）
    state.db.upsert_agent(&config).map_err(|e| e.to_string())?;

    // 2. 更新 registry.json
    let registry_path = state.repo_path.join("registry.json");
    let mut registry = registry::Registry::load(&registry_path)
        .unwrap_or_else(|_| registry::Registry::new_empty());
    registry.upsert_agent(config.clone());
    registry.save(&registry_path).map_err(|e| e.to_string())?;

    // 3. 创建 _current/ 并首次导入本地配置（文件操作 + git 操作放后台线程）
    let repo_path = state.repo_path.clone();
    let config_dir = config.config_dir.clone();
    let sync_files = config.sync_files.clone();
    let exclude_files = config.exclude_files.clone();
    let agent_id = config.id.clone();
    let pat = state.db.get_settings().map_err(|e| e.to_string())?.pat_token;

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let current_dir = repo_path.join(&agent_id).join("_current");
        std::fs::create_dir_all(&current_dir).map_err(|e| e.to_string())?;

        let local_config_dir = file_mapper::expand_tilde(&config_dir).map_err(|e| e.to_string())?;
        let local_config_path = std::path::PathBuf::from(&local_config_dir);

        if local_config_path.exists() {
            file_mapper::copy_local_to_current(
                &local_config_path,
                &current_dir,
                &sync_files,
                &exclude_files,
            )
            .map_err(|e| e.to_string())?;
        }

        // commit + push
        let repo = git2::Repository::open(&repo_path).map_err(|e| e.to_string())?;
        git_sync::commit(&repo, &format!("add agent: {}", agent_id))
            .map_err(|e| e.to_string())?;
        let _ = git_sync::push(&repo, &pat);

        Ok(())
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))?
}

/// 删除 agent
///
/// async 执行：删 SQLite + 更新 registry.json + 删仓库目录 + commit/push
#[tauri::command]
async fn remove_agent(state: tauri::State<'_, AppState>, agent_id: String) -> Result<(), String> {
    // 1. 删 SQLite
    state
        .db
        .remove_agent(&agent_id)
        .map_err(|e| e.to_string())?;

    // 2. 更新 registry.json
    let registry_path = state.repo_path.join("registry.json");
    if let Ok(mut registry) = registry::Registry::load(&registry_path) {
        registry.remove_agent(&agent_id);
        let _ = registry.save(&registry_path);
    }

    // 3. 删仓库目录 + commit/push（后台线程）
    let repo_path = state.repo_path.clone();
    let pat = state.db.get_settings().map_err(|e| e.to_string())?.pat_token;
    let id = agent_id.clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let agent_dir = repo_path.join(&id);
        if agent_dir.exists() {
            std::fs::remove_dir_all(&agent_dir).map_err(|e| e.to_string())?;
        }
        if let Ok(repo) = git2::Repository::open(&repo_path) {
            let _ = git_sync::commit(&repo, &format!("remove agent: {}", id));
            let _ = git_sync::push(&repo, &pat);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))?
}

/// 同步单个 agent
///
/// async 执行：git 操作放后台线程，避免阻塞 UI
#[tauri::command]
async fn sync_agent(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    agent_id: String,
) -> Result<SyncResult, String> {
    let config = state
        .db
        .list_agents()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == agent_id)
        .ok_or_else(|| format!("agent '{}' 未注册", agent_id))?;

    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    let local_config_dir = PathBuf::from(
        file_mapper::expand_tilde(&config.config_dir).map_err(|e| e.to_string())?,
    );

    let _ = app.emit("sync:started", serde_json::json!({ "agentId": &agent_id }));

    // git 操作放后台线程
    let repo_path = state.repo_path.clone();
    let app_data_dir = state.app_data_dir.clone();
    let pat = settings.pat_token;
    let sync_files = config.sync_files.clone();
    let exclude_files = config.exclude_files.clone();
    let id = agent_id.clone();

    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(SyncResult, Option<SyncContext>), String> {
        let engine = SyncEngine::new(repo_path, app_data_dir, pat);
        engine
            .sync_agent(&id, &local_config_dir, &sync_files, &exclude_files)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))??;

    let (sync_result, ctx) = result;

    if sync_result.status == types::SyncResultStatus::Conflict {
        if let Some(ctx) = ctx {
            *state
                .pending_sync
                .lock()
                .map_err(|e| e.to_string())? = Some((agent_id.clone(), ctx));
            let _ = app.emit(
                "conflict:detected",
                serde_json::json!({
                    "agentId": &agent_id,
                    "conflictType": if sync_result.error_message.as_deref()
                        .map(|s| s.contains("L1"))
                        .unwrap_or(false) { "L1" } else { "L2" },
                    "conflictFiles": sync_result.conflict_files,
                }),
            );
        }
    } else {
        let _ = app.emit(
            "sync:completed",
            serde_json::json!({ "result": &sync_result }),
        );
        if sync_result.status == types::SyncResultStatus::Success {
            let now = chrono::Utc::now().timestamp_millis();
            let _ = state.db.update_sync_status(&agent_id, &SyncStatus::Idle, Some(now));
        }
    }
    sync_engine::cleanup_tmp(&state.app_data_dir, &agent_id)
        .map_err(|e| e.to_string())?;
    Ok(sync_result)
}

/// 同步所有 agent
///
/// async 执行：每个 agent 的 git 操作放后台线程
#[tauri::command]
async fn sync_all(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SyncResult>, String> {
    let configs = state.db.list_agents().map_err(|e| e.to_string())?;
    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    let repo_path = state.repo_path.clone();
    let app_data_dir = state.app_data_dir.clone();
    let pat = settings.pat_token;

    let mut results = Vec::new();
    for config in configs {
        let local_config_dir = PathBuf::from(
            file_mapper::expand_tilde(&config.config_dir).map_err(|e| e.to_string())?,
        );
        let _ = app.emit(
            "sync:started",
            serde_json::json!({ "agentId": &config.id }),
        );

        let repo_path = repo_path.clone();
        let app_data_dir = app_data_dir.clone();
        let pat = pat.clone();
        let sync_files = config.sync_files.clone();
        let exclude_files = config.exclude_files.clone();
        let id = config.id.clone();

        let sync_outcome = tauri::async_runtime::spawn_blocking(move || -> Result<(SyncResult, Option<SyncContext>), String> {
            let engine = SyncEngine::new(repo_path, app_data_dir, pat);
            engine
                .sync_agent(&id, &local_config_dir, &sync_files, &exclude_files)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("后台任务失败: {}", e));

        match sync_outcome {
            Ok(Ok((result, ctx))) => {
                if result.status == types::SyncResultStatus::Conflict {
                    if let Some(ctx) = ctx {
                        *state
                            .pending_sync
                            .lock()
                            .map_err(|e| e.to_string())? = Some((config.id.clone(), ctx));
                    }
                    let _ = app.emit(
                        "conflict:detected",
                        serde_json::json!({
                            "agentId": &config.id,
                            "conflictType": if result.error_message.as_deref()
                                .map(|s| s.contains("L1"))
                                .unwrap_or(false) { "L1" } else { "L2" },
                            "conflictFiles": result.conflict_files,
                        }),
                    );
                } else {
                    let _ = app.emit(
                        "sync:completed",
                        serde_json::json!({ "result": &result }),
                    );
                    if result.status == types::SyncResultStatus::Success {
                        let now = chrono::Utc::now().timestamp_millis();
                        let _ = state
                            .db
                            .update_sync_status(&config.id, &SyncStatus::Idle, Some(now));
                    }
                }
                sync_engine::cleanup_tmp(&state.app_data_dir, &config.id)
                    .map_err(|e| e.to_string())?;
                results.push(result);
            }
            Ok(Err(e)) => {
                let _ = app.emit(
                    "sync:error",
                    serde_json::json!({ "agentId": &config.id, "errorMessage": &e }),
                );
                results.push(SyncResult {
                    agent_id: config.id,
                    status: types::SyncResultStatus::Error,
                    pulled_files: vec![],
                    pushed_files: vec![],
                    conflict_files: vec![],
                    error_message: Some(e),
                    duration_ms: 0,
                });
            }
            Err(e) => {
                let _ = app.emit(
                    "sync:error",
                    serde_json::json!({ "agentId": &config.id, "errorMessage": &e }),
                );
                results.push(SyncResult {
                    agent_id: config.id,
                    status: types::SyncResultStatus::Error,
                    pulled_files: vec![],
                    pushed_files: vec![],
                    conflict_files: vec![],
                    error_message: Some(e),
                    duration_ms: 0,
                });
            }
        }
    }
    Ok(results)
}

/// 解决冲突（前端用户决策后调用）
#[tauri::command]
fn resolve_conflict(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    resolution: serde_json::Value,
) -> Result<SyncResult, String> {
    let resolution: ConflictResolution =
        serde_json::from_value(resolution).map_err(|e| format!("解析 resolution 失败: {}", e))?;
    let pending = state
        .pending_sync
        .lock()
        .map_err(|e| e.to_string())?
        .take();
    let (agent_id, ctx) = pending.ok_or_else(|| "没有待处理的冲突".to_string())?;

    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    let engine = SyncEngine::new(
        state.repo_path.clone(),
        state.app_data_dir.clone(),
        settings.pat_token,
    );

    match engine.resolve_conflict(&ctx, &resolution) {
        Ok(result) => {
            let _ = app.emit(
                "sync:completed",
                serde_json::json!({ "result": &result }),
            );
            sync_engine::cleanup_tmp(&state.app_data_dir, &agent_id)
                .map_err(|e| e.to_string())?;
            Ok(result)
        }
        Err(e) => {
            let _ = app.emit(
                "sync:error",
                serde_json::json!({ "agentId": &agent_id, "errorMessage": e.to_string() }),
            );
            Err(e.to_string())
        }
    }
}

/* ------------------------------------------------------------------ */
/* 人格管理 IPC 命令                                                   */
/* ------------------------------------------------------------------ */

/// 列出 agent 的所有人格
#[tauri::command]
fn list_personalities(state: tauri::State<'_, AppState>, agent_id: String) -> Result<Vec<Persona>, String> {
    persona::list_personalities(&state.repo_path, &agent_id).map_err(|e| e.to_string())
}

/// 读取人格文件内容及其对应的本地文件内容（用于 Personalities 视图预览）
#[tauri::command]
async fn read_persona_file(
    state: tauri::State<'_, AppState>,
    agent_id: String,
    persona_name: String,
    file_path: String,
) -> Result<PersonaFileContent, String> {
    let config = state
        .db
        .list_agents()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == agent_id)
        .ok_or_else(|| format!("agent '{}' 未注册", agent_id))?;
    let repo_path = state.repo_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        persona::read_persona_file(&repo_path, &config, &persona_name, &file_path)
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))?
    .map_err(|e| e.to_string())
}

/// 保存当前为人格
#[tauri::command]
fn save_personality(
    state: tauri::State<'_, AppState>,
    agent_id: String,
    name: String,
) -> Result<(), String> {
    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    persona::save_personality(&state.repo_path, &agent_id, &name, &settings.pat_token)
        .map_err(|e| e.to_string())
}

/// 切换到指定人格
#[tauri::command]
fn switch_personality(
    state: tauri::State<'_, AppState>,
    agent_id: String,
    name: String,
) -> Result<(), String> {
    let config = state
        .db
        .list_agents()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == agent_id)
        .ok_or_else(|| format!("agent '{}' 未注册", agent_id))?;
    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    persona::switch_personality(
        &state.repo_path,
        &state.app_data_dir,
        &agent_id,
        &name,
        &config.config_dir,
        &config.sync_files,
        &config.exclude_files,
        &settings.pat_token,
    )
    .map_err(|e| e.to_string())?;
    state
        .db
        .update_current_persona(&agent_id, Some(&name))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除人格
#[tauri::command]
fn delete_personality(
    state: tauri::State<'_, AppState>,
    agent_id: String,
    name: String,
) -> Result<(), String> {
    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    persona::delete_personality(&state.repo_path, &agent_id, &name, &settings.pat_token)
        .map_err(|e| e.to_string())?;
    // 删除的若是当前激活人格，清空 current_persona 避免残留指向
    let (_, _, current_persona) = state
        .db
        .get_agent_status(&agent_id)
        .unwrap_or(("idle".to_string(), None, None));
    if current_persona.as_deref() == Some(name.as_str()) {
        state
            .db
            .update_current_persona(&agent_id, None)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 导出人格包
#[tauri::command]
fn export_personalities(
    state: tauri::State<'_, AppState>,
    agent_id: String,
    names: Vec<String>,
    output_path: String,
) -> Result<String, String> {
    persona::export_personalities(&state.repo_path, &agent_id, &names, std::path::Path::new(&output_path))
        .map_err(|e| e.to_string())
}

/// 导入人格包 -- 预览 diff
#[tauri::command]
fn preview_import_personalities(
    state: tauri::State<'_, AppState>,
    zip_path: String,
    agent_id: String,
) -> Result<Vec<persona::PersonaDiffPreview>, String> {
    persona::preview_import_personalities(
        std::path::Path::new(&zip_path),
        &state.repo_path,
        &agent_id,
    )
    .map_err(|e| e.to_string())
}

/// 导入人格包 -- 确认后解压
#[tauri::command]
fn import_personalities(
    state: tauri::State<'_, AppState>,
    zip_path: String,
    agent_id: String,
) -> Result<(), String> {
    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    persona::import_personalities(
        std::path::Path::new(&zip_path),
        &state.repo_path,
        &agent_id,
        &settings.pat_token,
    )
    .map_err(|e| e.to_string())
}

/* ------------------------------------------------------------------ */
/* 自动同步 IPC 命令                                                   */
/* ------------------------------------------------------------------ */

/// 启动自动同步
#[tauri::command]
async fn start_auto_sync(state: tauri::State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let settings = state.db.get_settings().map_err(|e| e.to_string())?;
    state
        .auto_sync
        .start(app, settings.auto_sync_interval_min)
        .await
        .map_err(|e| e.to_string())
}

/// 停止自动同步
#[tauri::command]
async fn stop_auto_sync(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.auto_sync.stop().await.map_err(|e| e.to_string())
}

/// 设置开机自启动
#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            log::info!("应用数据目录: {}", data_dir.display());

            let repo_path = data_dir.join("repo");
            let db_path = data_dir.join("agentsync.db");
            let database = Database::open(&db_path)?;

            // 启动时同步 registry.json -> SQLite agent_cache
            // 覆盖存量情况：已 onboarding 但 agent 未写入 SQLite
            let registry_path = repo_path.join("registry.json");
            if registry_path.exists() {
                if let Ok(registry) = registry::Registry::load(&registry_path) {
                    for config in registry.list_agents() {
                        let _ = database.upsert_agent(&config);
                    }
                    log::info!("已从 registry.json 同步 agent 配置到 SQLite");
                }
            }

            let auto_sync = Arc::new(AutoSyncManager::new());

            app.manage(AppState {
                db: database,
                app_data_dir: data_dir,
                repo_path,
                pending_sync: Mutex::new(None),
                auto_sync: auto_sync.clone(),
            });

            // 系统托盘（对应设计文档 Phase 5 -> 系统托盘）
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let sync_now = tauri::menu::MenuItem::with_id(app, "sync_now", "立即同步", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&sync_now, &quit])?;

            tauri::tray::TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("AgentSync - 已就绪")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "sync_now" => {
                        let _ = app.emit("tray:sync_now", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // 托盘状态随同步事件切换 tooltip（对应设计文档 Phase 5 -> 托盘图标随状态切换）
            // 图标切换需要多套图片资源，MVP 用 tooltip 文字反映状态
            let app_handle_for_tray = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let app_clone = app_handle_for_tray.clone();
                let _ = app_clone.listen("sync:started", {
                    let app = app_handle_for_tray.clone();
                    move |event| {
                        let payload: serde_json::Value = serde_json::from_str(event.payload()).unwrap_or_default();
                        let agent_id = payload["agentId"].as_str().unwrap_or("");
                        if let Some(tray) = app.tray_by_id("main-tray") {
                            let _ = tray.set_tooltip(Some(&format!("AgentSync - 同步中: {}", agent_id)));
                        }
                    }
                });
                let app_clone = app_handle_for_tray.clone();
                let _ = app_clone.listen("sync:completed", {
                    let app = app_handle_for_tray.clone();
                    move |_event| {
                        if let Some(tray) = app.tray_by_id("main-tray") {
                            let _ = tray.set_tooltip(Some("AgentSync - 已同步"));
                        }
                    }
                });
                let app_clone = app_handle_for_tray.clone();
                let _ = app_clone.listen("sync:error", {
                    let app = app_handle_for_tray.clone();
                    move |_event| {
                        if let Some(tray) = app.tray_by_id("main-tray") {
                            let _ = tray.set_tooltip(Some("AgentSync - 同步错误"));
                        }
                    }
                });
                let app_clone = app_handle_for_tray.clone();
                let _ = app_clone.listen("conflict:detected", {
                    let app = app_handle_for_tray.clone();
                    move |_event| {
                        if let Some(tray) = app.tray_by_id("main-tray") {
                            let _ = tray.set_tooltip(Some("AgentSync - 有冲突待解决"));
                        }
                    }
                });
            });

            // 根据设置初始化开机自启动
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<AppState>();
                if let Ok(settings) = state.db.get_settings() {
                    if settings.launch_at_login {
                        let _ = app_handle.autolaunch().enable();
                    }
                    // 启动定时同步（如果启用）
                    if settings.auto_sync_enabled {
                        let _ = state
                            .auto_sync
                            .start(app_handle.clone(), settings.auto_sync_interval_min)
                            .await;
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            get_app_data_dir,
            get_settings,
            save_settings,
            test_git_auth,
            is_repo_initialized,
            init_app,
            get_agents,
            list_tracked_files,
            add_agent,
            remove_agent,
            sync_agent,
            sync_all,
            resolve_conflict,
            // 人格管理
            list_personalities,
            read_persona_file,
            save_personality,
            switch_personality,
            delete_personality,
            export_personalities,
            preview_import_personalities,
            import_personalities,
            // 自动同步
            start_auto_sync,
            stop_auto_sync,
            set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
