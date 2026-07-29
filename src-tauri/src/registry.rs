//! Registry 管理模块
//!
//! 解析/校验 registry.json，管理 agent 定义和人格元数据。
//! 对应设计文档「registry.json 结构」「锁定文件机制」「预置默认 agent 定义」章节。

use crate::error::{AppError, AppResult};
use crate::types::AgentConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// registry.json 顶层结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    /// 数据格式版本（SemVer），如 "1.0.0"
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    /// agent 定义表，key 为 agent id
    pub agents: HashMap<String, AgentConfigEntry>,
}

/// registry.json 中的 agent 配置条目
///
/// 与 AgentConfig 的差异：registry.json 不存 id（id 是 HashMap 的 key）。
/// 其余字段（display_name/config_dir/sync_files/exclude_files/accent_color）一一对应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigEntry {
    pub display_name: String,
    pub config_dir: String,
    pub sync_files: Vec<String>,
    #[serde(default)]
    pub exclude_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
}

impl AgentConfigEntry {
    /// 转换为 AgentConfig（补上 id）
    pub fn to_agent_config(&self, id: &str) -> AgentConfig {
        AgentConfig {
            id: id.to_string(),
            display_name: self.display_name.clone(),
            config_dir: self.config_dir.clone(),
            sync_files: self.sync_files.clone(),
            exclude_files: self.exclude_files.clone(),
            accent_color: self.accent_color.clone(),
        }
    }
}

impl Registry {
    /// 从文件加载 registry.json
    pub fn load(path: &Path) -> AppResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AppError::Registry(format!("读取 registry.json 失败: {}", e)))?;
        let registry: Registry = serde_json::from_str(&content)?;
        registry.validate()?;
        Ok(registry)
    }

    /// 保存到 registry.json（先写临时文件再 rename，保证原子写入）
    pub fn save(&self, path: &Path) -> AppResult<()> {
        self.validate()?;
        let content = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// 校验 registry 完整性
    pub fn validate(&self) -> AppResult<()> {
        // schemaVersion 必填
        if self.schema_version.is_empty() {
            return Err(AppError::Registry("schemaVersion 不能为空".into()));
        }

        // 校验每个 agent 条目
        for (id, entry) in &self.agents {
            if id.is_empty() {
                return Err(AppError::Registry("agent id 不能为空".into()));
            }
            if entry.display_name.is_empty() {
                return Err(AppError::Registry(format!(
                    "agent '{}' 的 displayName 不能为空",
                    id
                )));
            }
            if entry.config_dir.is_empty() {
                return Err(AppError::Registry(format!(
                    "agent '{}' 的 configDir 不能为空",
                    id
                )));
            }
            if entry.sync_files.is_empty() {
                return Err(AppError::Registry(format!(
                    "agent '{}' 的 syncFiles 不能为空",
                    id
                )));
            }
            // 校验 glob 模式合法性
            for pattern in &entry.sync_files {
                validate_glob(pattern).map_err(|e| {
                    AppError::Registry(format!("agent '{}' 的 syncFiles 模式 '{}' 非法: {}", id, pattern, e))
                })?;
            }
            for pattern in &entry.exclude_files {
                validate_glob(pattern).map_err(|e| {
                    AppError::Registry(format!("agent '{}' 的 excludeFiles 模式 '{}' 非法: {}", id, pattern, e))
                })?;
            }
        }
        Ok(())
    }

    /// 获取所有 agent 配置列表
    pub fn list_agents(&self) -> Vec<AgentConfig> {
        self.agents
            .iter()
            .map(|(id, entry)| entry.to_agent_config(id))
            .collect()
    }

    /// 获取单个 agent 配置
    pub fn get_agent(&self, id: &str) -> Option<AgentConfig> {
        self.agents.get(id).map(|e| e.to_agent_config(id))
    }

    /// 添加或更新 agent
    pub fn upsert_agent(&mut self, config: AgentConfig) {
        let entry = AgentConfigEntry {
            display_name: config.display_name,
            config_dir: config.config_dir,
            sync_files: config.sync_files,
            exclude_files: config.exclude_files,
            accent_color: config.accent_color,
        };
        self.agents.insert(config.id, entry);
    }

    /// 删除 agent
    pub fn remove_agent(&mut self, id: &str) -> bool {
        self.agents.remove(id).is_some()
    }

    /// 创建空 registry（仅含 schemaVersion）
    pub fn new_empty() -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            agents: HashMap::new(),
        }
    }

    /// 创建包含预置默认 agent 的 registry
    ///
    /// 对应设计文档「预置默认 agent 定义」。
    /// 用户在 onboarding 时可勾选预置。
    pub fn new_with_presets(preset_ids: &[&str]) -> Self {
        let mut registry = Self::new_empty();
        let presets = default_presets();
        for id in preset_ids {
            if let Some(config) = presets.iter().find(|c| c.id == *id) {
                registry.upsert_agent(config.clone());
            }
        }
        registry
    }
}

/// 校验 glob 模式合法性（用 globset 尝试编译）
fn validate_glob(pattern: &str) -> Result<(), String> {
    // 结尾 / 等价于 /**，先规范化再校验
    let normalized = if pattern.ends_with('/') {
        format!("{}**", pattern)
    } else {
        pattern.to_string()
    };
    globset::Glob::new(&normalized).map_err(|e| e.to_string())?;
    Ok(())
}

/// 预置默认 agent 列表
///
/// 对应设计文档「预置默认 agent 定义」章节的 JSON。
fn default_presets() -> Vec<AgentConfig> {
    vec![
        AgentConfig {
            id: "workbuddy".into(),
            display_name: "WorkBuddy".into(),
            config_dir: "~/.workbuddy".into(),
            sync_files: vec![
                "SOUL.md".into(),
                "IDENTITY.md".into(),
                "USER.md".into(),
                "memory/**".into(),
            ],
            exclude_files: vec![
                "memory/cache/".into(),
                "memory/tmp/".into(),
                "*.lock".into(),
                "*.tmp".into(),
                "*.log".into(),
                "*.bak".into(),
            ],
            accent_color: Some("#42b883".into()),
        },
        AgentConfig {
            id: "claude-code".into(),
            display_name: "Claude Code".into(),
            config_dir: "~/.claude".into(),
            sync_files: vec!["CLAUDE.md".into(), "settings.json".into()],
            exclude_files: vec!["*.log".into()],
            accent_color: Some("#d97706".into()),
        },
        AgentConfig {
            id: "cursor".into(),
            display_name: "Cursor".into(),
            config_dir: "~/.cursor".into(),
            sync_files: vec!["rules/**".into()],
            exclude_files: vec![],
            accent_color: Some("#6366f1".into()),
        },
        AgentConfig {
            id: "codex".into(),
            display_name: "Codex".into(),
            config_dir: "~/.codex".into(),
            sync_files: vec!["config.toml".into()],
            exclude_files: vec![
                "*.sqlite".into(),
                "*.sqlite-shm".into(),
                "*.sqlite-wal".into(),
                "logs_*.sqlite".into(),
                "installation_id".into(),
                "tmp/".into(),
                ".tmp/".into(),
                "sqlite/".into(),
                "vendor_imports/".into(),
                "skills/".into(),
            ],
            accent_color: Some("#10b981".into()),
        },
        AgentConfig {
            id: "zcode".into(),
            display_name: "ZCode".into(),
            config_dir: "~/.zcode".into(),
            sync_files: vec!["AGENTS.md".into()],
            exclude_files: vec!["cli/".into(), "plugin-workspace/".into(), "v2/".into(), "agents/".into(), "skills/".into()],
            accent_color: Some("#8b5cf6".into()),
        },
        AgentConfig {
            id: "qoder".into(),
            display_name: "Qoder".into(),
            config_dir: "~/.qoderworkcn".into(),
            sync_files: vec!["commands/**".into()],
            exclude_files: vec![
                "bin/".into(),
                "cache/".into(),
                "logs/".into(),
                "machine-id".into(),
                "skills/".into(),
            ],
            accent_color: Some("#f59e0b".into()),
        },
        AgentConfig {
            id: "openclaw".into(),
            display_name: "OpenClaw".into(),
            config_dir: "~/.openclaw".into(),
            sync_files: vec!["identity/**".into()],
            exclude_files: vec![
                "exec-approvals.json".into(),
                "*.sock".into(),
            ],
            accent_color: Some("#ef4444".into()),
        },
        AgentConfig {
            id: "qwenpaw".into(),
            display_name: "QwenPaw".into(),
            config_dir: "~/.qwenpaw".into(),
            sync_files: vec![
                "HEARTBEAT.md".into(),
                "config.json".into(),
                "settings.json".into(),
            ],
            exclude_files: vec![
                "qwenpaw.log".into(),
                "token_usage.json".into(),
                "workspaces/".into(),
                "skill_pool/".into(),
            ],
            accent_color: Some("#3b82f6".into()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parse_valid() {
        let json = r#"{
            "schemaVersion": "1.0.0",
            "agents": {
                "workbuddy": {
                    "displayName": "WorkBuddy",
                    "configDir": "~/.workbuddy",
                    "syncFiles": ["SOUL.md", "memory/**"],
                    "excludeFiles": ["*.lock"]
                }
            }
        }"#;
        let registry: Registry = serde_json::from_str(json).unwrap();
        registry.validate().unwrap();
        assert_eq!(registry.agents.len(), 1);
        assert_eq!(registry.get_agent("workbuddy").unwrap().display_name, "WorkBuddy");
    }

    #[test]
    fn registry_parse_missing_field() {
        let json = r#"{
            "schemaVersion": "1.0.0",
            "agents": {
                "bad": {
                    "displayName": "",
                    "configDir": "~/.bad",
                    "syncFiles": ["*.md"]
                }
            }
        }"#;
        let registry: Registry = serde_json::from_str(json).unwrap();
        let err = registry.validate().unwrap_err();
        assert!(err.to_string().contains("displayName"));
    }

    #[test]
    fn registry_parse_invalid_glob() {
        let json = r#"{
            "schemaVersion": "1.0.0",
            "agents": {
                "bad": {
                    "displayName": "Bad",
                    "configDir": "~/.bad",
                    "syncFiles": ["[unclosed"]
                }
            }
        }"#;
        let registry: Registry = serde_json::from_str(json).unwrap();
        let err = registry.validate().unwrap_err();
        assert!(err.to_string().contains("非法"));
    }

    #[test]
    fn registry_presets() {
        let registry = Registry::new_with_presets(&["workbuddy", "cursor"]);
        assert_eq!(registry.agents.len(), 2);
        assert!(registry.get_agent("workbuddy").is_some());
        assert!(registry.get_agent("cursor").is_some());
        assert!(registry.get_agent("claude-code").is_none());
    }

    #[test]
    fn registry_save_and_load() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let registry = Registry::new_with_presets(&["workbuddy"]);
        registry.save(tmp.path()).unwrap();
        let loaded = Registry::load(tmp.path()).unwrap();
        assert_eq!(loaded.agents.len(), 1);
    }
}
