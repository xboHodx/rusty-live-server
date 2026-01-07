//! # SRS 客户端和主播状态管理模块
//!
//! 管理与 SRS（Simple Realtime Server）的交互，包括：
//! - 观众客户端状态追踪
//! - 主播推流状态管理
//! - 基于答题的观众鉴权
//! - 密钥验证

use chrono::{DateTime, Utc, Duration};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

// ============================================================================
// 枚举定义
// ============================================================================

/// 客户端状态枚举
///
/// 定义观众在系统中的可能状态，每个状态有不同的过期时间
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    /// 等待答题 - 新用户进入，尚未通过验证
    /// 过期时间：60 秒
    Pending = 0,
    /// 已授权 - 答题通过，可以拉流
    /// 过期时间：3600 秒（1 小时）
    Legal = 1,
    /// 被封禁 - 答题错误
    /// 过期时间：60 秒
    Nil = 2,
    /// 观看中 - 正在播放流
    /// 过期时间：永不过期
    Playing = 3,
    /// 暂离 - 暂时离开（可能回来）
    /// 过期时间：7200 秒（2 小时）
    Resting = 4,
}

impl ClientStatus {
    /// 将状态转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Legal => "legal",
            Self::Nil => "nil",
            Self::Playing => "playing",
            Self::Resting => "resting",
        }
    }

    /// 从字符串解析状态
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "legal" => Some(Self::Legal),
            "nil" => Some(Self::Nil),
            "playing" => Some(Self::Playing),
            "resting" => Some(Self::Resting),
            _ => None,
        }
    }

    /// 判断客户端是否已授权（可以拉流）
    pub fn is_authorized(&self) -> bool {
        matches!(self, Self::Legal | Self::Playing | Self::Resting)
    }

    /// 获取状态的过期时间
    ///
    /// ### 返回值
    /// - `Some(duration)`: 状态会在指定时间后过期
    /// - `None`: 状态永不过期（如 Playing）
    pub fn expiration_duration(&self) -> Option<Duration> {
        match self {
            Self::Pending => Some(Duration::seconds(60)),
            Self::Legal => Some(Duration::seconds(3600)),
            Self::Nil => Some(Duration::seconds(60)),
            Self::Playing => None, // 观看时永不过期
            Self::Resting => Some(Duration::seconds(7200)),
        }
    }
}

/// 主播状态枚举
///
/// 定义主播在系统中的可能状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamerStatus {
    /// 待机 - 未开始推流
    /// 过期时间：180 秒（3 分钟）
    Standby = 0,
    /// 推流中 - 正在直播
    /// 过期时间：永不过期
    Streaming = 1,
    /// 暂停 - 推流暂时中断（如网络问题）
    /// 过期时间：600 秒（10 分钟）
    Pausing = 2,
}

impl StreamerStatus {
    /// 将状态转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standby => "standby",
            Self::Streaming => "streaming",
            Self::Pausing => "pausing",
        }
    }

    /// 获取状态的过期时间
    pub fn expiration_duration(&self) -> Option<Duration> {
        match self {
            Self::Standby => Some(Duration::seconds(180)),
            Self::Streaming => None, // 推流时永不过期
            Self::Pausing => Some(Duration::seconds(600)),
        }
    }
}

// ============================================================================
// 数据结构定义
// ============================================================================

/// 客户端记录
///
/// 存储单个观众客户端的所有信息
#[derive(Debug, Clone)]
pub struct ClientRecord {
    /// 客户端 IP 地址
    pub ip: String,
    /// 会话 ID
    pub session_id: String,
    /// 分配的问题
    pub question: String,
    /// 正确答案
    pub answer: String,
    /// 显示昵称（可选）
    pub display_name: Option<String>,
    /// 是否为主播
    pub is_publisher: bool,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 当前状态
    pub status: ClientStatus,
    /// 最后活动时间
    pub last_activity: DateTime<Utc>,
}

impl ClientRecord {
    /// 创建新的客户端记录
    ///
    /// ### 参数
    /// - `ip`: 客户端 IP 地址
    /// - `session_id`: 会话 ID
    pub fn new(ip: String, session_id: String) -> Self {
        let now = Utc::now();
        Self {
            ip,
            session_id,
            question: String::new(),
            answer: String::new(),
            display_name: None,
            is_publisher: false,
            created_at: now,
            status: ClientStatus::Pending,
            last_activity: now,
        }
    }

    /// 判断客户端是否已过期
    ///
    /// 根据当前状态和最后活动时间判断
    pub fn is_expired(&self) -> bool {
        if let Some(duration) = self.status.expiration_duration() {
            Utc::now().signed_duration_since(self.last_activity) > duration
        } else {
            false
        }
    }
}

/// 主播记录
///
/// 存储当前主播的状态信息
#[derive(Debug, Clone)]
pub struct StreamerRecord {
    /// 主播 IP 地址
    pub ip: Option<String>,
    /// 推流密钥
    pub secret: Option<String>,
    /// 主播的会话 ID
    pub session_id: Option<String>,
    /// 流 URI（格式：app=xxx&stream=xxx）
    pub stream_uri: Option<String>,
    /// 直播间名称
    pub stream_name: Option<String>,
    /// 当前状态
    pub status: StreamerStatus,
    /// 最后活动时间
    pub last_activity: DateTime<Utc>,
}

impl StreamerRecord {
    /// 创建新的主播记录（初始化状态）
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            ip: None,
            secret: None,
            session_id: None,
            stream_uri: None,
            stream_name: None,
            status: StreamerStatus::Standby,
            last_activity: now,
        }
    }

    /// 判断主播是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(duration) = self.status.expiration_duration() {
            Utc::now().signed_duration_since(self.last_activity) > duration
        } else {
            false
        }
    }
}

/// 主播密钥验证器
///
/// 从密钥文件读取有效密钥，验证推流权限
pub struct StreamerVerifier {
    /// 密钥文件路径
    secret_path: PathBuf,
}

impl StreamerVerifier {
    /// 创建新的验证器
    ///
    /// ### 参数
    /// - `secret_path`: 密钥文件路径
    pub fn new(secret_path: PathBuf) -> Self {
        Self { secret_path }
    }

    /// 验证密钥是否有效
    ///
    /// ### 参数
    /// - `secret`: 要验证的密钥
    ///
    /// ### 返回值
    /// - `true`: 密钥有效
    /// - `false`: 密钥无效
    ///
    /// ### 密钥文件格式
    /// 每行一个密钥，以 `secret_` 开头
    pub fn authorize(&self, secret: &str) -> bool {
        match fs::read_to_string(&self.secret_path) {
            Ok(content) => {
                // 按空白字符分割，获取所有密钥
                let known_secrets: Vec<&str> = content
                    .split_whitespace()
                    .collect();
                known_secrets.iter().any(|s| *s == secret)
            }
            Err(_) => false,
        }
    }
}

// ============================================================================
// SRS 数据库
// ============================================================================

/// SRS 数据库内部结构
///
/// 管理所有客户端和主播的状态
pub struct SrsDatabaseInner {
    /// 客户端映射：IP -> session_id -> ClientRecord
    pub clients: HashMap<String, HashMap<String, ClientRecord>>,
    /// 主播记录
    pub streamer: StreamerRecord,
    /// 密钥验证器
    pub verifier: StreamerVerifier,
    /// 是否为公开模式（无需答题）
    pub public_stream: bool,
}

impl SrsDatabaseInner {
    /// 创建新的 SRS 数据库
    ///
    /// ### 参数
    /// - `secret_path`: 密钥文件路径
    pub fn new(secret_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            clients: HashMap::new(),
            streamer: StreamerRecord::new(),
            verifier: StreamerVerifier::new(secret_path),
            public_stream: false,
        })
    }

    /// 重置数据库
    ///
    /// 清除所有客户端和主播数据
    pub fn reset(&mut self) {
        self.clients.clear();
        self.streamer = StreamerRecord::new();
        self.public_stream = false;
    }

    // ========================================================================
    // 客户端操作
    // ========================================================================

    /// 检查客户端是否存在
    pub fn has_client(&self, ip: &str, session_id: &str) -> bool {
        self.clients
            .get(ip)
            .and_then(|m| m.get(session_id))
            .is_some()
    }

    /// 检查客户端是否已授权（可以拉流）
    pub fn has_authorized_client(&self, ip: &str, session_id: &str) -> bool {
        self.clients
            .get(ip)
            .and_then(|m| m.get(session_id))
            .map(|r| r.status.is_authorized())
            .unwrap_or(false)
    }

    /// 添加新客户端
    pub fn add_client(&mut self, ip: String, session_id: String) {
        self.clients
            .entry(ip.clone())
            .or_insert_with(HashMap::new)
            .insert(session_id.clone(), ClientRecord::new(ip, session_id));
    }

    /// 获取客户端记录（只读）
    pub fn get_client(&self, ip: &str, session_id: &str) -> Option<&ClientRecord> {
        self.clients.get(ip)?.get(session_id)
    }

    /// 获取客户端记录（可变）
    pub fn get_client_mut(&mut self, ip: &str, session_id: &str) -> Option<&mut ClientRecord> {
        self.clients.get_mut(ip)?.get_mut(session_id)
    }

    /// 移除客户端
    pub fn remove_client(&mut self, ip: &str, session_id: &str) -> Option<ClientRecord> {
        self.clients.get_mut(ip)?.remove(session_id)
    }

    /// 获取客户端的问题和答案
    pub fn get_client_qa(&self, ip: &str, session_id: &str) -> Option<(&str, &str)> {
        self.get_client(ip, session_id)
            .map(|r| (r.question.as_str(), r.answer.as_str()))
    }

    /// 设置客户端的问题和答案
    pub fn set_client_qa(&mut self, ip: &str, session_id: &str, q: String, a: String) {
        if let Some(client) = self.get_client_mut(ip, session_id) {
            client.question = q;
            client.answer = a;
        }
    }

    /// 获取客户端显示名称
    pub fn get_client_display_name(&self, ip: &str, session_id: &str) -> Option<&str> {
        self.get_client(ip, session_id)?.display_name.as_deref()
    }

    /// 设置客户端显示名称
    pub fn set_client_display_name(&mut self, ip: &str, session_id: &str, name: String) {
        if let Some(client) = self.get_client_mut(ip, session_id) {
            client.display_name = Some(name);
        }
    }

    /// 获取客户端状态
    pub fn get_client_status(&self, ip: &str, session_id: &str) -> Option<ClientStatus> {
        self.get_client(ip, session_id).map(|r| r.status)
    }

    /// 更新客户端活动和状态
    ///
    /// ### 返回值
    /// - `true`: 更新成功
    /// - `false`: 客户端不存在
    pub fn update_client_activity(&mut self, ip: &str, session_id: &str, status: ClientStatus) -> bool {
        if let Some(client) = self.get_client_mut(ip, session_id) {
            client.status = status;
            client.last_activity = Utc::now();
            true
        } else {
            false
        }
    }

    /// 设置客户端为主播
    pub fn set_client_publisher(&mut self, ip: &str, session_id: &str) {
        if let Some(client) = self.get_client_mut(ip, session_id) {
            client.is_publisher = true;
        }
    }

    /// 检查客户端是否为主播
    pub fn client_is_publisher(&self, ip: &str, session_id: &str) -> bool {
        self.get_client(ip, session_id)
            .map(|r| r.is_publisher)
            .unwrap_or(false)
    }

    // ========================================================================
    // 主播操作
    // ========================================================================

    /// 检查是否正在推流
    pub fn is_streaming(&self) -> bool {
        self.streamer.status != StreamerStatus::Standby
    }

    /// 检查是否正在活跃推流（非暂停状态）
    pub fn is_actively_streaming(&self) -> bool {
        self.streamer.status == StreamerStatus::Streaming
    }

    /// 获取流 URI
    pub fn get_stream_uri(&self) -> Option<&str> {
        self.streamer.stream_uri.as_deref()
    }

    /// 获取直播间名称
    pub fn get_stream_name(&self) -> Option<&str> {
        self.streamer.stream_name.as_deref()
    }

    /// 设置直播间名称
    pub fn set_stream_name(&mut self, name: String) {
        self.streamer.stream_name = Some(name);
    }

    /// 验证主播密钥
    pub fn verify_streamer(&self, secret: &str) -> bool {
        self.verifier.authorize(secret)
    }

    /// 注册主播（新推流开始）
    pub fn register_streamer(
        &mut self,
        ip: String,
        secret: String,
        app: String,
        stream: String,
    ) {
        self.streamer.ip = Some(ip);
        self.streamer.secret = Some(secret.clone());
        self.streamer.stream_uri = Some(format!("app={}&stream={}", app, stream));
        self.streamer.status = StreamerStatus::Streaming;
        self.streamer.last_activity = Utc::now();
    }

    /// 连接主播（通过 API 回答问题）
    ///
    /// ### 返回值
    /// - `true`: 密钥匹配，连接成功
    /// - `false`: 密钥不匹配
    pub fn connect_streamer(&mut self, session_id: String, secret: &str) -> bool {
        if self.streamer.secret.as_deref() == Some(secret) {
            self.streamer.session_id = Some(session_id);
            true
        } else {
            false
        }
    }

    /// 暂停推流（on_unpublish 回调）
    pub fn pause_streaming(&mut self) {
        if self.streamer.status == StreamerStatus::Streaming {
            self.streamer.status = StreamerStatus::Pausing;
            self.streamer.last_activity = Utc::now();
        }
    }

    /// 恢复推流
    ///
    /// ### 返回值
    /// - `true`: 密钥匹配，恢复成功
    /// - `false`: 密钥不匹配
    pub fn resume_streaming(
        &mut self,
        ip: String,
        secret: &str,
        app: String,
        stream: String,
    ) -> bool {
        if self.streamer.secret.as_deref() == Some(secret) {
            self.streamer.ip = Some(ip);
            self.streamer.stream_uri = Some(format!("app={}&stream={}", app, stream));
            self.streamer.status = StreamerStatus::Streaming;
            self.streamer.last_activity = Utc::now();
            true
        } else {
            false
        }
    }

    /// 结束推流
    ///
    /// ### 参数
    /// - `session_id`: 主播的会话 ID（可选，用于验证）
    ///
    /// ### 返回值
    /// - `true`: 结束成功
    /// - `false`: session_id 不匹配
    pub fn end_streaming(&mut self, session_id: Option<&str>) -> bool {
        if session_id.is_some() && self.streamer.session_id.as_deref() == session_id {
            self.streamer = StreamerRecord::new();
            true
        } else {
            false
        }
    }

    /// 设置公开模式
    pub fn set_public(&mut self, public: bool) {
        self.public_stream = public;
    }

    /// 检查是否为公开模式
    pub fn is_public(&self) -> bool {
        self.public_stream
    }
}

// ============================================================================
// SRS 数据库包装器
// ============================================================================

/// SRS 数据库包装器
///
/// 提供后台任务支持
pub struct SrsDatabase {
    /// 内部数据库
    pub inner: Arc<RwLock<SrsDatabaseInner>>,
    /// 后台任务是否活跃
    pub active: Arc<RwLock<bool>>,
}

impl SrsDatabase {
    /// 创建新的 SRS 数据库
    pub fn new(secret_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            inner: Arc::new(RwLock::new(SrsDatabaseInner::new(secret_path)?)),
            active: Arc::new(RwLock::new(true)),
        })
    }

    /// 清理过期记录（定期调用）
    pub fn tick(&self) {
        let mut db = self.inner.write();

        // 先检查主播是否过期
        if db.streamer.is_expired() {
            tracing::debug!("srs_db.tick(): 主播已过期，清除所有数据");
            db.reset();
            return;
        }

        // 清理过期的客户端
        let mut clients_to_remove = Vec::new();
        for (ip, clients) in db.clients.iter_mut() {
            let mut session_ids_to_remove = Vec::new();
            for (session_id, client) in clients.iter() {
                if client.is_expired() {
                    tracing::debug!(
                        "srs_db.tick(): 移除过期客户端: (ip={}, session_id={})",
                        ip,
                        session_id
                    );
                    session_ids_to_remove.push(session_id.clone());
                }
            }
            for session_id in session_ids_to_remove {
                clients.remove(&session_id);
            }
            if clients.is_empty() {
                clients_to_remove.push(ip.clone());
            }
        }
        for ip in clients_to_remove {
            db.clients.remove(&ip);
        }
    }

    /// 启动后台 tick 任务
    pub async fn spin(&self) {
        let active = self.active.clone();
        let inner = self.inner.clone();

        tokio::spawn(async move {
            while *active.read() {
                {
                    let db = inner.read();
                    // 释放读锁
                    drop(db);
                }
                // 实际的 tick 操作通过 inner.write() 完成
                // 这里是简化版本，实际 tick 在 main.rs 中实现
            }
        });
    }
}
