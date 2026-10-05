//! 数据库管理模块

use crate::types::{AppError, AppResult};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub mod migrations;
pub mod models;
pub mod queries;
pub mod runtime;

/// 数据库管理器
#[derive(Debug, Clone)]
pub struct DatabaseManager {
    connection: Arc<Mutex<Connection>>,
}

impl DatabaseManager {
    /// A degraded session never writes to or deletes the failed database.
    pub fn in_memory() -> AppResult<Self> {
        let manager = Self {
            connection: Arc::new(Mutex::new(Connection::open_in_memory()?)),
        };
        manager
            .connection
            .lock()
            .map_err(|error| AppError::Database(format!("数据库连接锁失败: {error}")))?
            .execute("PRAGMA foreign_keys = ON", [])?;
        manager.initialize()?;
        Ok(manager)
    }

    /// All command/event clones share this connection, including after recovery.
    pub fn replace_connection(&self, replacement: Self) -> AppResult<()> {
        let mut current = self
            .connection
            .lock()
            .map_err(|error| AppError::Database(format!("数据库连接锁失败: {error}")))?;
        let mut next = replacement
            .connection
            .lock()
            .map_err(|error| AppError::Database(format!("恢复数据库连接锁失败: {error}")))?;
        std::mem::swap(&mut *current, &mut *next);
        Ok(())
    }
    /// 创建新的数据库管理器实例
    pub fn new<P: AsRef<Path>>(database_path: P) -> AppResult<Self> {
        let connection = Connection::open(database_path)?;

        // 启用外键约束
        connection.execute("PRAGMA foreign_keys = ON", [])?;

        Ok(DatabaseManager {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// 获取数据库连接引用
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.connection)
    }

    /// 初始化数据库（运行迁移）
    pub fn initialize(&self) -> AppResult<()> {
        let mut conn = self
            .connection
            .lock()
            .map_err(|error| AppError::Database(format!("数据库连接锁失败: {error}")))?;
        let transaction = conn.transaction()?;
        migrations::run_migrations(&transaction)?;
        transaction.commit()?;
        Ok(())
    }
}
