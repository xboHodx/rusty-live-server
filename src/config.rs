//! # 配置模块
//!
//! 定义应用程序的配置结构体和加载逻辑。
//!
//! ## 配置项说明
//! - 服务监听地址和端口（8848）
//! - 文件路径（题库、密钥、转储目录）

use std::env;
use std::net::IpAddr;
use std::path::PathBuf;

/// 应用配置结构体
///
/// 包含所有运行时配置参数
#[derive(Debug, Clone)]
pub struct Config {
    /// 服务监听地址
    pub host: IpAddr,
    /// 服务监听端口
    pub port: u16,
    /// 基础路径（所有其他路径的根目录）
    pub base_path: PathBuf,
    /// 题库数据库目录路径
    pub banner_db_path: PathBuf,
    /// 聊天记录转储目录
    pub dump_path: PathBuf,
    /// 密钥文件路径
    pub secret_path: PathBuf,
    /// SRS API 主机地址
    pub srs_api_host: String,
    /// SRS API 端口
    pub srs_api_port: u16,
}

impl Config {
    /// 从环境变量或默认值创建配置
    ///
    /// ### 环境变量
    /// - `LIVE_SERVER_BASE_PATH` - 基础路径（默认：当前工作目录）
    ///
    /// ### 默认值
    /// - 服务地址: 0.0.0.0:8848
    /// - 基础路径: 当前工作目录
    pub fn from_env() -> Self {
        // 基础路径：优先使用环境变量，否则使用当前工作目录
        let base_path = if let Ok(path) = env::var("LIVE_SERVER_BASE_PATH") {
            PathBuf::from(path)
        } else {
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        Self {
            host: "0.0.0.0".parse().unwrap(),
            port: 8848,
            base_path: base_path.clone(),
            banner_db_path: base_path.join("config/bannerdb"),
            dump_path: base_path.join("dumps"),
            secret_path: base_path.join("secrets/secret.txt"),
            srs_api_host: "127.0.0.1".to_string(),
            srs_api_port: 1985,
        }
    }

    /// 获取服务地址（host:port 格式）
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn srs_api_addr(&self) -> String {
        format!("{}:{}", self.srs_api_host, self.srs_api_port)
    }
}
