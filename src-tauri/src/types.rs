//! 共享类型定义
//!
//! 对应设计文档「类型定义」章节的 Rust 端结构体。
//! 前端 src/types/index.ts 一一对应。

use serde::{Deserialize, Serialize};

/* ------------------------------------------------------------------ */
/* Agent                                                               */
/* ------------------------------------------------------------------ */

/// 用户新增 agent 时填写的表单数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    /// agent 标识符，如 "workbuddy"，作为仓库存放目录名
    pub id: String,
    /// UI 显示名，如 "WorkBuddy"
    pub display_name: String,
    /// 本地配置目录绝对路径或 ~ 开头，如 "~/.workbuddy"
    pub config_dir: String,
    /// 包含 glob 规则
    pub sync_files: Vec<String>,
    /// 排除 glob 规则
    pub exclude_files: Vec<String>,
    /// UI 标识色（可选），如 "#42b883"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
}

/// 注册后含运行时状态的完整 agent 对象
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    #[serde(flatten)]
    pub config: AgentConfig,
    /// 当前激活的人格名，未激活则为 null
    pub current_persona: Option<String>,
    /// 运行时同步状态
    pub sync_status: SyncStatus,
    /// 最近一次同步时间戳（Unix 毫秒），未同步为 null
    pub last_sync_at: Option<i64>,
    /// 跟踪文件数（syncFiles 匹配 - excludeFiles 匹配）
    pub tracked_file_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncStatus {
    Idle,
    Syncing,
    Pending,
    Conflict,
    Error,
}

/* ------------------------------------------------------------------ */
/* Settings                                                            */
/* ------------------------------------------------------------------ */

/// 应用全局设置，存 SQLite settings 表（单行）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 远程仓库 URL，如 https://gitee.com/user/workbuddy-sync.git
    pub repo_url: String,
    pub platform: Platform,
    /// PAT，明文存（MVP），未来迁移 keychain
    pub pat_token: String,
    pub auto_sync_enabled: bool,
    /// 自动同步间隔（分钟），可选值 5/15/30/60
    pub auto_sync_interval_min: u32,
    /// 开机自启动
    pub launch_at_login: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitee")]
    Gitee,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repo_url: String::new(),
            platform: Platform::Gitee,
            pat_token: String::new(),
            auto_sync_enabled: false,
            auto_sync_interval_min: 15,
            launch_at_login: false,
        }
    }
}

/* ------------------------------------------------------------------ */
/* SyncResult                                                          */
/* ------------------------------------------------------------------ */

/// 单次 sync_agent 的返回值；sync_all 返回 Vec<SyncResult>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub agent_id: String,
    pub status: SyncResultStatus,
    /// 从远程拉取并写回本地的文件（相对 agent 目录路径）
    pub pulled_files: Vec<String>,
    /// 本地变更推送到远程的文件
    pub pushed_files: Vec<String>,
    /// 冲突文件（status='conflict' 时填充）
    pub conflict_files: Vec<String>,
    /// status='error' 时填充
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// 同步耗时（ms）
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncResultStatus {
    Success,
    Conflict,
    Error,
    Skipped,
}

/* ------------------------------------------------------------------ */
/* Persona                                                             */
/* ------------------------------------------------------------------ */

/// 人格（角色）元数据，从 agent 文件夹扫描得到
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Persona {
    pub agent_id: String,
    /// 文件夹名，如 "work-mode"
    pub name: String,
    /// 从 manifest 或文件夹名生成
    pub display_name: String,
    /// 包含的文件列表（相对 agent 目录）
    pub files: Vec<String>,
    /// 总字节数
    pub size_bytes: u64,
    /// 是否当前激活
    pub is_current: bool,
    /// 导入来源标记，导入时填充
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<i64>,
}
