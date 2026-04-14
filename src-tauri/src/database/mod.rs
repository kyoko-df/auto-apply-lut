//! 数据库管理模块

use crate::types::AppResult;
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
        let conn = self.connection.lock().unwrap();
        migrations::run_migrations(&*conn)?;
        Ok(())
    }
}
