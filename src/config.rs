//! # 配置模块
//!
//! 定义应用程序的配置结构体和加载逻辑。
//!
//! ## 配置项说明
//! - API 服务地址和端口（3484）
//! - 聊天服务地址和端口（3614）
//! - SRS 回调服务地址和端口（8848）
//! - 文件路径（题库、密钥、转储目录）
//! - SRS API 地址

use std::net::IpAddr;
use std::path::PathBuf;

/// 应用配置结构体
///
/// 包含所有运行时配置参数
#[derive(Debug, Clone)]
pub struct Config {
    /// API 服务监听地址
    pub api_host: IpAddr,
    /// API 服务监听端口
    pub api_port: u16,
    /// 聊天服务监听地址
    pub chat_host: IpAddr,
    /// 聊天服务监听端口
    pub chat_port: u16,
    /// SRS 回调服务监听地址
    pub srs_host: IpAddr,
    /// SRS 回调服务监听端口
    pub srs_port: u16,
    /// 基础路径（所有其他路径的根目录）
    pub base_path: PathBuf,
    /// 题库数据库文件路径
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
    /// ### 默认值
    /// - API 地址: 0.0.0.0:3484
    /// - 聊天地址: 0.0.0.0:3614
    /// - SRS 地址: 0.0.0.0:8848
    /// - SRS API: 0.0.0.0:1985
    /// - 基础路径: `/home/xbohodx02/work/rusty-live-server`
    ///
    /// ### 注意事项
    /// 目前 `base_path` 是硬编码的，实际部署时需要修改或改为从环境变量读取
    pub fn from_env() -> Self {
        // 基础路径（当前硬编码）
        let base_path = PathBuf::from("/home/xbohodx02/work/rusty-live-server");

        Self {
            api_host: "0.0.0.0".parse().unwrap(),
            api_port: 3484,
            chat_host: "0.0.0.0".parse().unwrap(),
            chat_port: 3614,
            srs_host: "0.0.0.0".parse().unwrap(),
            srs_port: 8848,
            base_path: base_path.clone(),
            banner_db_path: base_path.join("config/bannerdb"),
            dump_path: base_path.join("dumps"),
            secret_path: base_path.join("secrets/secret.txt"),
            srs_api_host: "0.0.0.0".to_string(),
            srs_api_port: 1985,
        }
    }

    /// 获取 API 服务地址（host:port 格式）
    pub fn api_addr(&self) -> String {
        format!("{}:{}", self.api_host, self.api_port)
    }

    /// 获取聊天服务地址（host:port 格式）
    pub fn chat_addr(&self) -> String {
        format!("{}:{}", self.chat_host, self.chat_port)
    }

    /// 获取 SRS 回调服务地址（host:port 格式）
    pub fn srs_addr(&self) -> String {
        format!("{}:{}", self.srs_host, self.srs_port)
    }

    /// 获取 SRS API URL（http://host:port 格式）
    pub fn srs_api_url(&self) -> String {
        format!("http://{}:{}", self.srs_api_host, self.srs_api_port)
    }
}
