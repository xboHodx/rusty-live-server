//! # 应用状态模块
//!
//! 定义了应用程序的全局状态和数据结构。
//! 包括：
//! - `srs` - SRS 客户端和主播状态管理
//! - `chat` - 聊天室消息和用户管理
//! - `banner` - 答题题库管理

// 子模块声明
pub mod srs;    // SRS 相关状态管理
pub mod chat;   // 聊天室状态管理
pub mod banner; // 题库状态管理

// 导出公共类型，供其他模块使用
pub use srs::{ClientStatus};  // SRS 数据库和状态枚举
pub use banner::BannerDatabase;                              // 题库数据库

// 导入依赖
use std::sync::Arc;
use parking_lot::RwLock;
use crate::config::Config;

/// 全局应用状态
///
/// 此结构体在所有处理器之间共享，包含了应用程序运行所需的所有状态数据。
/// 使用 `Arc<RwLock<>>` 包装以实现线程安全的读写访问。
///
/// ### 字段说明
/// - `srs_db`: SRS 客户端和主播状态数据库
/// - `chat_db`: 聊天室消息和用户映射数据库
/// - `banner_db`: 题库数据库（只读，使用 Arc 共享）
/// - `config`: 应用配置信息
#[derive(Clone)]
pub struct AppState {
    /// SRS 数据库 - 管理客户端连接、主播状态、答题验证等
    pub srs_db: Arc<RwLock<srs::SrsDatabaseInner>>,
    /// 聊天室数据库 - 管理聊天消息、用户昵称、UID 映射等
    pub chat_db: Arc<RwLock<chat::ChatDatabaseInner>>,
    /// 题库数据库 - 管理答题问题，只读访问
    pub banner_db: Arc<BannerDatabase>,
    /// 应用配置 - 包含端口、路径等配置信息
    pub config: Config,
}

impl AppState {
    /// 创建新的应用状态实例
    ///
    /// ### 参数
    /// - `config`: 应用配置对象
    ///
    /// ### 返回
    /// 成功返回 `AppState` 实例，失败返回错误信息
    ///
    /// ### 初始化过程
    /// 1. 加载题库数据库
    /// 2. 初始化 SRS 数据库（需要密钥文件路径）
    /// 3. 初始化聊天室数据库（需要转储路径）
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        // 初始化题库数据库
        let banner_db = Arc::new(BannerDatabase::new(&config.banner_db_path)?);
        let secret_path = config.secret_path.clone();
        let dump_path = config.dump_path.clone();

        Ok(Self {
            srs_db: Arc::new(RwLock::new(srs::SrsDatabaseInner::new(secret_path)?)),
            chat_db: Arc::new(RwLock::new(chat::ChatDatabaseInner::new(dump_path))),
            banner_db,
            config,
        })
    }
}
