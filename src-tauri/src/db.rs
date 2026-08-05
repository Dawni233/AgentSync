//! 数据库模块
//!
//! 应用配置存储（SQLite）。对应设计文档「本地存储布局」：
//! agentsync.db 存 PAT/设置/agent 注册表缓存。
//!
//! 单行 settings 表 + agent_cache 表。

use crate::error::{AppError, AppResult};
use crate::types::{AgentConfig, Settings};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

/// 应用数据库（线程安全的 SQLite 连接）
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// 打开/创建数据库
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                repo_url TEXT NOT NULL DEFAULT '',
                platform TEXT NOT NULL DEFAULT 'gitee',
                pat_token TEXT NOT NULL DEFAULT '',
                auto_sync_enabled INTEGER NOT NULL DEFAULT 0,
                auto_sync_interval_min INTEGER NOT NULL DEFAULT 15,
                launch_at_login INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS agent_cache (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                config_dir TEXT NOT NULL,
                sync_files TEXT NOT NULL,
                exclude_files TEXT NOT NULL,
                accent_color TEXT,
                current_persona TEXT,
                sync_status TEXT NOT NULL DEFAULT 'idle',
                last_sync_at INTEGER,
                tracked_file_count INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 读取 settings（不存在则返回默认值）
    pub fn get_settings(&self) -> AppResult<Settings> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT repo_url, platform, pat_token, auto_sync_enabled, auto_sync_interval_min, launch_at_login FROM settings WHERE id = 1",
        )?;
        let result = stmt.query_row([], |row| {
            let platform_str: String = row.get(1)?;
            let platform = match platform_str.as_str() {
                "github" => crate::types::Platform::GitHub,
                _ => crate::types::Platform::Gitee,
            };
            Ok(Settings {
                repo_url: row.get(0)?,
                platform,
                pat_token: row.get(2)?,
                auto_sync_enabled: row.get::<_, i64>(3)? != 0,
                auto_sync_interval_min: row.get::<_, i64>(4)? as u32,
                launch_at_login: row.get::<_, i64>(5)? != 0,
            })
        });
        match result {
            Ok(s) => Ok(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Settings::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// 保存 settings
    pub fn save_settings(&self, settings: &Settings) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        let platform_str = match settings.platform {
            crate::types::Platform::GitHub => "github",
            crate::types::Platform::Gitee => "gitee",
        };
        conn.execute(
            "INSERT OR REPLACE INTO settings (id, repo_url, platform, pat_token, auto_sync_enabled, auto_sync_interval_min, launch_at_login)
             VALUES (1, ?, ?, ?, ?, ?, ?)",
            params![
                settings.repo_url,
                platform_str,
                settings.pat_token,
                settings.auto_sync_enabled as i64,
                settings.auto_sync_interval_min as i64,
                settings.launch_at_login as i64,
            ],
        )?;
        Ok(())
    }

    /// 获取所有缓存的 agent 配置
    pub fn list_agents(&self) -> AppResult<Vec<AgentConfig>> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, display_name, config_dir, sync_files, exclude_files, accent_color FROM agent_cache",
        )?;
        let agents = stmt
            .query_map([], |row| {
                let sync_files_json: String = row.get(3)?;
                let exclude_files_json: String = row.get(4)?;
                Ok(AgentConfig {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    config_dir: row.get(2)?,
                    sync_files: serde_json::from_str(&sync_files_json).unwrap_or_default(),
                    exclude_files: serde_json::from_str(&exclude_files_json).unwrap_or_default(),
                    accent_color: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(agents)
    }

    /// 获取 agent 的运行时状态（sync_status + last_sync_at + current_persona）
    pub fn get_agent_status(
        &self,
        agent_id: &str,
    ) -> AppResult<(String, Option<i64>, Option<String>)> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        let result = conn.query_row(
            "SELECT sync_status, last_sync_at, current_persona FROM agent_cache WHERE id = ?",
            rusqlite::params![agent_id],
            |row| {
                let status: String = row.get(0)?;
                let last_sync: Option<i64> = row.get(1).ok();
                let current_persona: Option<String> = row.get(2).ok();
                Ok((status, last_sync, current_persona))
            },
        );
        match result {
            Ok(s) => Ok(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(("idle".to_string(), None, None)),
            Err(e) => Err(e.into()),
        }
    }

    /// 添加或更新 agent 缓存
    pub fn upsert_agent(&self, config: &AgentConfig) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO agent_cache (id, display_name, config_dir, sync_files, exclude_files, accent_color)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                config.id,
                config.display_name,
                config.config_dir,
                serde_json::to_string(&config.sync_files)?,
                serde_json::to_string(&config.exclude_files)?,
                config.accent_color,
            ],
        )?;
        Ok(())
    }

    /// 删除 agent 缓存
    pub fn remove_agent(&self, agent_id: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute("DELETE FROM agent_cache WHERE id = ?", params![agent_id])?;
        Ok(())
    }

    /// 更新 agent 同步状态
    pub fn update_sync_status(
        &self,
        agent_id: &str,
        status: &crate::types::SyncStatus,
        last_sync_at: Option<i64>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        let status_str = match status {
            crate::types::SyncStatus::Idle => "idle",
            crate::types::SyncStatus::Syncing => "syncing",
            crate::types::SyncStatus::Pending => "pending",
            crate::types::SyncStatus::Conflict => "conflict",
            crate::types::SyncStatus::Error => "error",
        };
        conn.execute(
            "UPDATE agent_cache SET sync_status = ?, last_sync_at = ? WHERE id = ?",
            params![status_str, last_sync_at, agent_id],
        )?;
        Ok(())
    }

    /// 更新 agent 当前激活的人格（切换/删除人格时调用）
    pub fn update_current_persona(&self, agent_id: &str, persona: Option<&str>) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute(
            "UPDATE agent_cache SET current_persona = ? WHERE id = ?",
            params![persona, agent_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Platform;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, Database) {
        let tmp = TempDir::new().unwrap();
        let db = Database::open(&tmp.path().join("test.db")).unwrap();
        (tmp, db)
    }

    #[test]
    fn settings_default_when_empty() {
        let (_tmp, db) = test_db();
        let s = db.get_settings().unwrap();
        assert_eq!(s.platform, Platform::Gitee);
        assert!(!s.auto_sync_enabled);
        assert_eq!(s.auto_sync_interval_min, 15);
    }

    #[test]
    fn settings_save_and_load() {
        let (_tmp, db) = test_db();
        let s = Settings {
            repo_url: "https://gitee.com/test/repo.git".into(),
            platform: Platform::GitHub,
            pat_token: "token123".into(),
            auto_sync_enabled: true,
            auto_sync_interval_min: 30,
            launch_at_login: true,
        };
        db.save_settings(&s).unwrap();
        let loaded = db.get_settings().unwrap();
        assert_eq!(loaded.repo_url, s.repo_url);
        assert_eq!(loaded.platform, Platform::GitHub);
        assert_eq!(loaded.pat_token, "token123");
        assert!(loaded.auto_sync_enabled);
        assert_eq!(loaded.auto_sync_interval_min, 30);
    }

    #[test]
    fn agent_cache_upsert_and_list() {
        let (_tmp, db) = test_db();
        let config = AgentConfig {
            id: "workbuddy".into(),
            display_name: "WorkBuddy".into(),
            config_dir: "~/.workbuddy".into(),
            sync_files: vec!["SOUL.md".into()],
            exclude_files: vec!["*.lock".into()],
            accent_color: Some("#42b883".into()),
        };
        db.upsert_agent(&config).unwrap();
        let agents = db.list_agents().unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "workbuddy");
        assert_eq!(agents[0].sync_files, vec!["SOUL.md".to_string()]);
    }

    #[test]
    fn agent_cache_remove() {
        let (_tmp, db) = test_db();
        let config = AgentConfig {
            id: "test".into(),
            display_name: "Test".into(),
            config_dir: "~/.test".into(),
            sync_files: vec!["*.md".into()],
            exclude_files: vec![],
            accent_color: None,
        };
        db.upsert_agent(&config).unwrap();
        db.remove_agent("test").unwrap();
        assert_eq!(db.list_agents().unwrap().len(), 0);
    }
}
