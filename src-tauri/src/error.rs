//! 错误类型定义
//!
//! 统一的应用错误类型，供所有后端模块使用。
//! 对应设计文档「错误处理」章节的 7 类场景。

use thiserror::Error;

/// 应用统一错误类型
#[derive(Debug, Error)]
pub enum AppError {
    /// Git 操作错误（认证失败、网络问题、rebase 冲突等）
    #[error("Git 错误: {0}")]
    Git(String),

    /// 文件 I/O 错误
    #[error("IO 错误: {0}")]
    Io(String),

    /// registry.json 解析或校验错误
    #[error("Registry 错误: {0}")]
    Registry(String),

    /// 配置错误（缺少必填项、格式非法等）
    #[error("配置错误: {0}")]
    Config(String),

    /// 数据库错误
    #[error("数据库错误: {0}")]
    Db(String),

    /// 同步冲突（L1/L2）
    #[error("同步冲突: {0}")]
    Conflict(String),

    /// 其他未分类错误
    #[error("{0}")]
    Other(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<git2::Error> for AppError {
    fn from(e: git2::Error) -> Self {
        // 按 git2 错误类别细分，便于上层做重试/阻断决策
        match e.class() {
            git2::ErrorClass::Net | git2::ErrorClass::Ssl => {
                AppError::Git(format!("网络错误: {}", e))
            }
            git2::ErrorClass::Http => {
                // 401/403 归为认证失败
                AppError::Git(format!("认证失败: {}", e))
            }
            _ => AppError::Git(e.to_string()),
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Registry(format!("JSON 解析失败: {}", e))
    }
}

impl From<zip::result::ZipError> for AppError {
    fn from(e: zip::result::ZipError) -> Self {
        AppError::Io(format!("Zip 操作失败: {}", e))
    }
}

// 让 AppError 能作为 Tauri command 的返回错误（自动序列化为字符串给前端）
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// 便捷 Result 别名
pub type AppResult<T> = Result<T, AppError>;
