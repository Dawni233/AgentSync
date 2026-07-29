//! 自动同步模块
//!
//! 对应设计文档「Phase 5: 自动同步 -> 后台定时器」。
//! 用 tokio::spawn + tokio::time::interval 实现定时同步。

use crate::error::AppResult;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

/// 自动同步管理器
pub struct AutoSyncManager {
    /// 当前定时任务句柄
    task: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    /// 运行标志（用于任务内部检查是否应继续）
    running: Arc<AtomicBool>,
}

impl AutoSyncManager {
    pub fn new() -> Self {
        Self {
            task: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 启动定时同步
    ///
    /// 按 settings.autoSyncIntervalMin 间隔触发 sync_all。
    /// 若已在运行，先停止再启动。
    pub async fn start(&self, app: AppHandle, interval_min: u32) -> AppResult<()> {
        self.stop().await?;

        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);

        let task = tauri::async_runtime::spawn(async move {
            let interval_secs = (interval_min as u64) * 60;
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            ticker.tick().await; // 跳过首次立即触发

            while running.load(Ordering::SeqCst) {
                ticker.tick().await;
                if !running.load(Ordering::SeqCst) {
                    break;
                }

                // 检查是否启用
                let should_sync = {
                    let state = app.state::<crate::AppState>();
                    match state.db.get_settings() {
                        Ok(s) => s.auto_sync_enabled,
                        Err(_) => false,
                    }
                };

                if should_sync {
                    log::info!("[auto_sync] 定时触发 sync_all");
                    // 通过 emit 事件让前端触发，统一处理 UI 状态
                    let _ = app.emit("auto_sync:trigger", ());
                }
            }
            log::info!("[auto_sync] 定时任务已停止");
        });

        *self.task.lock().await = Some(task);
        log::info!("[auto_sync] 定时同步已启动，间隔 {} 分钟", interval_min);
        Ok(())
    }

    /// 停止定时同步
    pub async fn stop(&self) -> AppResult<()> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.task.lock().await.take() {
            handle.abort();
        }
        Ok(())
    }

    /// 重启定时同步（设置变更后调用）
    pub async fn restart(&self, app: AppHandle, interval_min: u32) -> AppResult<()> {
        self.start(app, interval_min).await
    }
}

impl Default for AutoSyncManager {
    fn default() -> Self {
        Self::new()
    }
}
