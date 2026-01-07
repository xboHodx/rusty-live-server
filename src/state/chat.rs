//! # 聊天室状态管理模块
//!
//! 管理聊天室的消息记录、用户身份映射、昵称设置等功能。
//! 支持消息的时间戳排序、用户去重、聊天记录转储等。

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

// ============================================================================
// 数据结构定义
// ============================================================================

/// 单条聊天消息记录
///
/// 存储一条聊天消息的完整信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEntry {
    /// 发送者用户 ID
    pub uid: u32,
    /// 消息内容
    pub content: String,
    /// 消息时间戳（Unix 时间戳，秒级精度）
    pub stamp: f64,
    /// 是否为主播发送的消息（序列化时重命名为 "pub"）
    #[serde(rename = "pub")]
    pub is_publisher: bool,
}

impl ChatEntry {
    /// 创建新的聊天消息条目
    ///
    /// ### 参数
    /// - `uid`: 发送者用户 ID
    /// - `content`: 消息内容
    /// - `stamp`: 消息时间戳
    /// - `is_publisher`: 是否为主播消息
    pub fn new(uid: u32, content: String, stamp: f64, is_publisher: bool) -> Self {
        Self {
            uid,
            content,
            stamp,
            is_publisher,
        }
    }
}

/// 客户端身份信息
///
/// 存储一个 (IP, rid) 对应的用户身份
#[derive(Debug, Clone, Serialize)]
pub struct ClientIdentity {
    /// 用户 ID
    pub uid: u32,
    /// 用户昵称（如果已设置）
    pub name: Option<String>,
}

/// 聊天室数据库
///
/// 管理聊天室的所有状态，包括消息记录、用户映射等。
///
/// ### 数据结构说明
/// - `messages`: 按时间戳排序的消息列表
/// - `name_map`: 已被占用的昵称集合（用于防止昵称重复）
/// - `uid_map`: UID -> 昵称 的映射
/// - `client_map`: 二层 HashMap，IP -> rid -> ClientIdentity
/// - `ip_map`: UID -> IP 的映射（用于消息归属）
/// - `next_uid`: 下一个可用的 UID 起始值
/// - `dump_path`: 聊天记录转储目录路径
#[derive(Debug)]
pub struct ChatDatabaseInner {
    /// 消息列表（按时间戳排序）
    pub messages: Vec<ChatEntry>,
    /// 已被占用的昵称集合
    pub name_map: HashSet<String>,
    /// UID -> 昵称 映射
    pub uid_map: HashMap<u32, String>,
    /// 客户端映射：IP -> rid -> 身份信息
    pub client_map: HashMap<String, HashMap<String, ClientIdentity>>,
    /// UID -> IP 映射（用于显示消息来源）
    pub ip_map: HashMap<u32, String>,
    /// 下一个可用的 UID
    pub next_uid: u32,
    /// 聊天记录转储目录
    pub dump_path: PathBuf,
}

impl ChatDatabaseInner {
    /// 创建新的聊天室数据库
    ///
    /// ### 参数
    /// - `dump_path`: 聊天记录转储目录路径
    ///
    /// ### 注意事项
    /// UID 从 114514~1919810 范围内随机开始，这是一个彩蛋值
    pub fn new(dump_path: PathBuf) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            messages: Vec::new(),
            name_map: HashSet::new(),
            uid_map: HashMap::new(),
            client_map: HashMap::new(),
            ip_map: HashMap::new(),
            // 彩蛋：UID 从随机位置开始
            next_uid: rng.gen_range(114514..1919810),
            dump_path,
        }
    }

    /// 重置聊天室数据库
    ///
    /// 在新直播开始时调用，清空所有消息和用户信息
    pub fn reset(&mut self) {
        let mut rng = rand::thread_rng();
        self.messages.clear();
        self.name_map.clear();
        self.uid_map.clear();
        self.client_map.clear();
        self.ip_map.clear();
        self.next_uid = rng.gen_range(114514..1919810);
    }

    /// 添加聊天消息
    ///
    /// ### 参数
    /// - `ip`: 发送者 IP 地址
    /// - `rid`: 发送者请求 ID（会话 ID）
    /// - `content`: 消息内容
    /// - `is_publisher`: 是否为主播发送
    ///
    /// ### 行为说明
    /// 1. 如果客户端不存在，自动创建匿名用户
    /// 2. 消息按时间戳插入到正确位置（保持有序）
    pub fn add_entry(&mut self, ip: String, rid: String, content: String, is_publisher: bool) {
        // 获取当前时间戳（秒级精度）
        let stamp = Utc::now().timestamp_millis() as f64 / 1000.0;

        // 获取或创建客户端 UID
        let uid = if let Some(client) = self.client_map.get(&ip).and_then(|m| m.get(&rid)) {
            // 客户端已存在，使用其 UID
            client.uid
        } else {
            // 创建新的匿名用户
            self.next_uid += 1;
            let uid = self.next_uid;
            self.client_map
                .entry(ip.clone())
                .or_insert_with(HashMap::new)
                .insert(rid.clone(), ClientIdentity { uid, name: None });
            self.ip_map.insert(uid, ip);
            uid
        };

        // 创建消息条目
        let entry = ChatEntry::new(uid, content, stamp, is_publisher);

        // 使用 partition_point 找到插入位置（保持时间戳有序）
        let pos = self
            .messages
            .partition_point(|e| e.stamp <= stamp);
        self.messages.insert(pos, entry);
    }

    /// 设置或更改客户端昵称
    ///
    /// ### 参数
    /// - `ip`: 客户端 IP 地址
    /// - `rid`: 客户端请求 ID
    /// - `name`: 要设置的昵称
    ///
    /// ### 返回值
    /// - `true`: 昵称设置成功
    /// - `false`: 昵称已被占用或客户端已有昵称
    pub fn set_client_name(&mut self, ip: &str, rid: &str, name: String) -> bool {
        // 检查昵称是否已被占用
        if self.name_map.contains(&name) {
            return false;
        }

        // 获取或创建客户端 UID
        let uid = if let Some(client) = self.client_map.get(ip).and_then(|m| m.get(rid)) {
            // 已存在的客户端
            if client.name.is_some() {
                return false; // 已经有昵称了
            }
            client.uid
        } else {
            // 新客户端，带昵称注册
            self.next_uid += 1;
            let uid = self.next_uid;
            self.client_map
                .entry(ip.to_string())
                .or_insert_with(HashMap::new)
                .insert(rid.to_string(), ClientIdentity {
                    uid,
                    name: Some(name.clone()),
                });
            self.ip_map.insert(uid, ip.to_string());
            uid
        };

        // 更新所有映射
        self.name_map.insert(name.clone());
        self.uid_map.insert(uid, name.clone());

        // 更新 client_map 中的昵称
        if let Some(client) = self.client_map.get_mut(ip).and_then(|m| m.get_mut(rid)) {
            client.name = Some(name);
        }

        true
    }

    /// 获取客户端昵称
    ///
    /// ### 参数
    /// - `ip`: 客户端 IP 地址
    /// - `rid`: 客户端请求 ID
    ///
    /// ### 返回值
    /// 返回客户端昵称（如果已设置）
    pub fn get_client_name(&self, ip: &str, rid: &str) -> Option<String> {
        self.client_map
            .get(ip)?
            .get(rid)
            .and_then(|c| c.name.clone())
    }

    /// 获取指定时间戳之后的聊天消息
    ///
    /// ### 参数
    /// - `stamp`: 起始时间戳
    /// - `prev`: 是否获取之前的消息（true）还是之后的消息（false）
    ///
    /// ### 返回值
    /// 返回符合条件消息的 JSON 数组
    pub fn get_chat_from(&self, stamp: f64, prev: bool) -> Vec<serde_json::Value> {
        let entries = self.get_entries_from(stamp, prev);

        entries
            .into_iter()
            .map(|entry| {
                let mut obj = serde_json::json!({
                    "content": entry.content,
                    "stamp": entry.stamp,
                    "pub": entry.is_publisher,
                });

                // 优先显示昵称，其次显示 IP
                if let Some(name) = self.uid_map.get(&entry.uid) {
                    obj["name"] = serde_json::json!(name);
                } else if let Some(ip) = self.ip_map.get(&entry.uid) {
                    obj["ip"] = serde_json::json!(ip);
                }

                obj
            })
            .collect()
    }

    /// 获取指定时间戳的原始消息条目
    ///
    /// ### 参数
    /// - `stamp`: 起始时间戳
    /// - `prev`: 是否获取之前的消息
    ///
    /// ### 返回值
    /// 返回符合条件的消息条目列表
    fn get_entries_from(&self, stamp: f64, prev: bool) -> Vec<ChatEntry> {
        if stamp < 0.0 {
            // 客户端没有消息记录，返回最近 10 条
            if self.messages.len() < 10 {
                return self.messages.clone();
            } else {
                return self.messages[self.messages.len() - 10..].to_vec();
            }
        }

        // 找到时间戳位置
        let idx = self.messages.partition_point(|e| e.stamp <= stamp);

        if prev {
            // 获取之前的 10 条消息
            let start = if idx >= 10 { idx - 10 } else { 0 };
            self.messages[start..idx].to_vec()
        } else {
            // 获取之后的所有消息
            if idx < self.messages.len() {
                self.messages[idx + 1..].to_vec()
            } else {
                Vec::new()
            }
        }
    }

    /// 获取唯一用户数量
    ///
    /// ### 返回值
    /// 返回当前聊天室中的唯一用户数
    pub fn size(&self) -> usize {
        self.ip_map.len()
    }

    /// 转储完整聊天记录到文件
    ///
    /// 包含完整的用户映射、客户端映射和消息记录
    pub fn dump_full(&self) {
        // 构建消息记录（包含用户信息）
        let records: Vec<serde_json::Value> = self
            .messages
            .iter()
            .map(|m| {
                let mut obj = serde_json::json!({
                    "uid": m.uid,
                    "name": self.uid_map.get(&m.uid),
                    "ip": self.ip_map.get(&m.uid),
                    "content": m.content,
                    "date": format!("{:?}", DateTime::<Utc>::from_timestamp(m.stamp as i64, 0).unwrap_or_default()),
                });
                obj
            })
            .collect();

        // 构建完整转储数据
        let dump_data = serde_json::json!({
            "umap": self.uid_map,
            "cmap": self.client_map,
            "records": records,
        });

        // 生成文件名：live-YYYY-MM-DD HH:MM:SS.dump
        let filename = self.dump_path.join(format!(
            "live-{}.dump",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        ));

        // 确保目录存在
        if let Some(parent) = filename.parent() {
            fs::create_dir_all(parent).ok();
        }

        // 写入文件
        if let Ok(content) = serde_json::to_string_pretty(&dump_data) {
            fs::write(filename, content).ok();
        }
    }

    /// 转储精简聊天记录到文件
    ///
    /// 仅包含消息记录，不包含用户映射
    pub fn dump_brief(&self) {
        let records: Vec<serde_json::Value> = self
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "uid": m.uid,
                    "name": self.uid_map.get(&m.uid),
                    "ip": self.ip_map.get(&m.uid),
                    "content": m.content,
                    "date": format!("{:?}", DateTime::<Utc>::from_timestamp(m.stamp as i64, 0).unwrap_or_default()),
                })
            })
            .collect();

        let filename = self.dump_path.join(format!(
            "live-{}.dump",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        ));

        if let Some(parent) = filename.parent() {
            fs::create_dir_all(parent).ok();
        }

        if let Ok(content) = serde_json::to_string_pretty(&records) {
            fs::write(filename, content).ok();
        }
    }
}

// ============================================================================
// 流信息结构体
// ============================================================================

/// 流信息统计
///
/// 用于从 SRS API 获取观众人数信息
pub struct StreamingInfo {
    /// 当前观众人数（-1 表示未知）
    pub audiences: i32,
    /// 后台任务是否活跃
    pub active: Arc<RwLock<bool>>,
}

impl StreamingInfo {
    /// 创建新的流信息对象
    pub fn new() -> Self {
        Self {
            audiences: -1,
            active: Arc::new(RwLock::new(true)),
        }
    }

    /// 获取当前观众人数
    pub fn get_audiences(&self) -> i32 {
        self.audiences
    }

    /// 从 SRS API 获取观众人数
    ///
    /// ### 参数
    /// - `srs_api_url`: SRS API 地址（如 http://localhost:1985）
    ///
    /// ### 行为说明
    /// 1. 请求 SRS 的 `/api/v1/clients/` 接口
    /// 2. 获取当前连接的客户端数量
    /// 3. 减去 1（排除推流端）得到观众人数
    pub async fn tick(&mut self, srs_api_url: &str) {
        let api_url = format!("{}/api/v1/clients/", srs_api_url);

        match reqwest::get(&api_url).await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(clients) = json.get("clients").and_then(|c| c.as_array()) {
                            // 减去 1 排除推流端
                            self.audiences = clients.len().saturating_sub(1) as i32;
                        }
                    }
                } else {
                    self.audiences = -1;
                }
            }
            Err(_) => {
                tracing::debug!("unable to fetch audiences at this time");
                self.audiences = -1;
            }
        }
    }

    /// 启动后台统计任务
    ///
    /// ### 参数
    /// - `srs_api_url`: SRS API 地址
    ///
    /// ### 行为说明
    /// 每 3 秒从 SRS API 获取一次观众人数
    pub async fn spin(&mut self, srs_api_url: String) {
        let active = self.active.clone();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick(&srs_api_url).await;
                }
                _ = tokio::signal::ctrl_c() => {
                    break;
                }
            }
        }
    }
}
