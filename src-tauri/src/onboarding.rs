//! 首次初始化模块
//!
//! 对应设计文档「首次初始化流程」章节。
//! 处理 onboarding 状态机：clone + bootstrap_registry + 首次本地配置导入。

use crate::error::AppResult;
use crate::file_mapper;
use crate::git_sync;
use crate::registry::Registry;
use crate::types::AgentConfig;
use std::path::{Path, PathBuf};

/// onboarding 输入参数
pub struct InitAppParams {
    pub repo_url: String,
    pub pat_token: String,
    /// 用户勾选预置的 agent id 列表
    pub preset_agent_ids: Vec<String>,
    /// 导入策略（用户在 onboarding 步骤 2 选择）
    pub import_strategy: ImportStrategyOption,
}

/// 导入策略选项（用户可选）
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum ImportStrategyOption {
    /// 自动判断（_current 空用本地，本地空用远程，都有时默认本地）
    #[default]
    Auto,
    /// 始终优先本地（本地覆盖远程）
    PreferLocal,
    /// 始终优先远程（远程覆盖本地）
    PreferRemote,
}

/// onboarding 结果
#[derive(serde::Serialize)]
pub struct InitAppResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// 导入的 agent 列表（含首次本地配置导入情况）
    pub imported_agents: Vec<AgentImportInfo>,
}

/// 单个 agent 的首次导入情况
#[derive(serde::Serialize)]
pub struct AgentImportInfo {
    pub agent_id: String,
    /// 本地配置目录是否存在
    pub local_config_exists: bool,
    /// 仓库 _current/ 是否有内容
    pub current_has_content: bool,
    /// 导入策略
    pub strategy: ImportStrategy,
}

#[derive(serde::Serialize, Clone, Copy)]
pub enum ImportStrategy {
    /// 本地 -> _current（首次上传）
    LocalToCurrent,
    /// _current -> 本地（第二台机器拉取）
    CurrentToLocal,
    /// 创建空 _current（本地和远程都无配置）
    Empty,
    /// 跳过（本地配置目录不存在）
    Skipped,
}

/// 执行 onboarding 全流程
///
/// 对应设计文档 onboarding 状态机：
/// 1. clone 仓库到 app_data_dir/repo/
/// 2. 检查仓库状态（空 / 有 registry.json / 无 registry.json）
/// 3. bootstrap_registry（写默认 registry + 预置 agent）
/// 4. 首次本地配置导入（4 场景）
/// 5. commit + push
pub fn init_app(params: &InitAppParams, app_data_dir: &Path) -> AppResult<InitAppResult> {
    let repo_path = app_data_dir.join("repo");

    // 如果 repo 已存在，先清理（避免重复 onboarding 残留）
    if repo_path.exists() {
        std::fs::remove_dir_all(&repo_path)?;
    }

    // 1. clone 仓库
    let repo = git_sync::clone_repo(&params.repo_url, &repo_path, &params.pat_token)?;
    // 确保 HEAD 指向 main
    let _ = repo.set_head(&format!("refs/heads/{}", git_sync::DEFAULT_BRANCH));

    // 2. 检查仓库状态
    let registry_path = repo_path.join("registry.json");
    let has_registry = registry_path.exists();
    let repo_is_empty = repo.head().is_err(); // unborn branch = 空仓库

    if !repo_is_empty && !has_registry {
        // 仓库非空但无 registry.json -> 非 AgentSync 仓库
        return Ok(InitAppResult {
            success: false,
            error_message: Some("远程仓库非空且无 registry.json，不是 AgentSync 仓库".into()),
            imported_agents: vec![],
        });
    }

    // 3. bootstrap_registry
    let registry = if repo_is_empty || !has_registry {
        // 空仓库或无 registry：创建带预置 agent 的 registry
        let preset_refs: Vec<&str> = params.preset_agent_ids.iter().map(|s| s.as_str()).collect();
        Registry::new_with_presets(&preset_refs)
    } else {
        // 已有 registry：加载现有
        Registry::load(&registry_path)?
    };

    // 4. 首次本地配置导入（对每个预置 agent）
    let mut imported_agents = Vec::new();
    for agent_id in &params.preset_agent_ids {
        let config = match registry.get_agent(agent_id) {
            Some(c) => c,
            None => continue,
        };
        let info = import_agent_config(&config, &repo_path, params.import_strategy)?;
        imported_agents.push(info);
    }

    // 5. 保存 registry + commit + push
    registry.save(&registry_path)?;
    git_sync::commit(&repo, "init: bootstrap registry")?;
    let _ = git_sync::push(&repo, &params.pat_token);

    Ok(InitAppResult {
        success: true,
        error_message: None,
        imported_agents,
    })
}

/// 单个 agent 的首次本地配置导入
///
/// 对应设计文档「首次本地配置导入」4 场景表。
/// 用户可选导入策略：Auto（自动）/ PreferLocal / PreferRemote
fn import_agent_config(
    config: &AgentConfig,
    repo_path: &Path,
    strategy_option: ImportStrategyOption,
) -> AppResult<AgentImportInfo> {
    let agent_id = &config.id;
    let current_dir = repo_path.join(agent_id).join("_current");
    let local_config_dir = PathBuf::from(file_mapper::expand_tilde(&config.config_dir)?);

    let local_config_exists = local_config_dir.exists() && has_files(&local_config_dir);
    let current_has_content = current_dir.exists() && has_files(&current_dir);

    let strategy = match (current_has_content, local_config_exists) {
        (false, true) => {
            // 仓库 _current/ 为空 + 本地有文件 -> 本地 -> _current
            std::fs::create_dir_all(&current_dir)?;
            file_mapper::copy_local_to_current(
                &local_config_dir,
                &current_dir,
                &config.sync_files,
                &config.exclude_files,
            )?;
            ImportStrategy::LocalToCurrent
        }
        (true, false) => {
            // 仓库 _current/ 有内容 + 本地无文件 -> _current -> 本地
            std::fs::create_dir_all(&local_config_dir)?;
            let staging = repo_path.join("_tmp_staging");
            file_mapper::copy_current_to_local_atomic(
                &current_dir,
                &local_config_dir,
                &staging,
                &config.sync_files,
                &config.exclude_files,
            )?;
            let _ = std::fs::remove_dir_all(&staging);
            ImportStrategy::CurrentToLocal
        }
        (false, false) => {
            // 都为空 -> 创建空 _current
            std::fs::create_dir_all(&current_dir)?;
            ImportStrategy::Empty
        }
        (true, true) => {
            // 都有内容 -> 按用户选择的策略处理
            match strategy_option {
                ImportStrategyOption::PreferRemote => {
                    // 远程优先：_current -> 本地
                    let staging = repo_path.join("_tmp_staging");
                    file_mapper::copy_current_to_local_atomic(
                        &current_dir,
                        &local_config_dir,
                        &staging,
                        &config.sync_files,
                        &config.exclude_files,
                    )?;
                    let _ = std::fs::remove_dir_all(&staging);
                    ImportStrategy::CurrentToLocal
                }
                ImportStrategyOption::Auto | ImportStrategyOption::PreferLocal => {
                    // 默认/本地优先：本地 -> _current
                    file_mapper::copy_local_to_current(
                        &local_config_dir,
                        &current_dir,
                        &config.sync_files,
                        &config.exclude_files,
                    )?;
                    ImportStrategy::LocalToCurrent
                }
            }
        }
    };

    Ok(AgentImportInfo {
        agent_id: agent_id.clone(),
        local_config_exists,
        current_has_content,
        strategy,
    })
}

/// 判断目录是否有文件（含子目录）
fn has_files(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_some(),
        Err(_) => false,
    }
}

/// 检查仓库是否已初始化（供前端判断是否需要 onboarding）
pub fn is_repo_initialized(app_data_dir: &Path) -> bool {
    let repo_path = app_data_dir.join("repo");
    if !repo_path.exists() {
        return false;
    }
    // 检查是否是 git 仓库且有 registry.json
    let git_dir = repo_path.join(".git");
    if !git_dir.exists() {
        return false;
    }
    repo_path.join("registry.json").exists()
}
