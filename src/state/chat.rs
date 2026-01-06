use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// A single chat message entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEntry {
    pub uid: u32,
    pub content: String,
    pub stamp: f64,
    #[serde(rename = "pub")]
    pub is_publisher: bool,
}

impl ChatEntry {
    pub fn new(uid: u32, content: String, stamp: f64, is_publisher: bool) -> Self {
        Self {
            uid,
            content,
            stamp,
            is_publisher,
        }
    }
}

/// Client identity mapping (ip, rid) -> (uid, name)
#[derive(Debug, Clone, Serialize)]
pub struct ClientIdentity {
    pub uid: u32,
    pub name: Option<String>,
}

/// Chat database managing messages and user mappings
#[derive(Debug)]
pub struct ChatDatabaseInner {
    pub messages: Vec<ChatEntry>,
    pub name_map: HashSet<String>,           // names already in use
    pub uid_map: HashMap<u32, String>,       // uid -> name
    pub client_map: HashMap<String, HashMap<String, ClientIdentity>>, // ip -> rid -> (uid, name)
    pub ip_map: HashMap<u32, String>,        // uid -> ip
    pub next_uid: u32,
    pub dump_path: PathBuf,
}

impl ChatDatabaseInner {
    pub fn new(dump_path: PathBuf) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            messages: Vec::new(),
            name_map: HashSet::new(),
            uid_map: HashMap::new(),
            client_map: HashMap::new(),
            ip_map: HashMap::new(),
            next_uid: rng.gen_range(114514..1919810),
            dump_path,
        }
    }

    /// Reset the chat database (called when stream starts)
    pub fn reset(&mut self) {
        let mut rng = rand::thread_rng();
        self.messages.clear();
        self.name_map.clear();
        self.uid_map.clear();
        self.client_map.clear();
        self.ip_map.clear();
        self.next_uid = rng.gen_range(114514..1919810);
    }

    /// Add a chat entry
    pub fn add_entry(&mut self, ip: String, rid: String, content: String, is_publisher: bool) {
        let stamp = Utc::now().timestamp_millis() as f64 / 1000.0;

        let uid = if let Some(client) = self.client_map.get(&ip).and_then(|m| m.get(&rid)) {
            client.uid
        } else {
            // Create new anonymous user
            self.next_uid += 1;
            let uid = self.next_uid;
            self.client_map
                .entry(ip.clone())
                .or_insert_with(HashMap::new)
                .insert(rid.clone(), ClientIdentity { uid, name: None });
            self.ip_map.insert(uid, ip);
            uid
        };

        let entry = ChatEntry::new(uid, content, stamp, is_publisher);

        // Insert maintaining sorted order by timestamp
        let pos = self
            .messages
            .partition_point(|e| e.stamp <= stamp);
        self.messages.insert(pos, entry);
    }

    /// Set or change client's display name
    pub fn set_client_name(&mut self, ip: &str, rid: &str, name: String) -> bool {
        // Check if name is already taken
        if self.name_map.contains(&name) {
            return false;
        }

        let uid = if let Some(client) = self.client_map.get(ip).and_then(|m| m.get(rid)) {
            // Existing client, update name
            if client.name.is_some() {
                return false; // Already has a name
            }
            client.uid
        } else {
            // New client with name
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

        // Update all mappings
        self.name_map.insert(name.clone());
        self.uid_map.insert(uid, name.clone());

        // Update client_map
        if let Some(client) = self.client_map.get_mut(ip).and_then(|m| m.get_mut(rid)) {
            client.name = Some(name);
        }

        true
    }

    /// Get client's display name
    pub fn get_client_name(&self, ip: &str, rid: &str) -> Option<String> {
        self.client_map
            .get(ip)?
            .get(rid)
            .and_then(|c| c.name.clone())
    }

    /// Get chat entries starting from a timestamp
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

                // Add name if available
                if let Some(name) = self.uid_map.get(&entry.uid) {
                    obj["name"] = serde_json::json!(name);
                } else if let Some(ip) = self.ip_map.get(&entry.uid) {
                    obj["ip"] = serde_json::json!(ip);
                }

                obj
            })
            .collect()
    }

    /// Get raw entries from a timestamp
    fn get_entries_from(&self, stamp: f64, prev: bool) -> Vec<ChatEntry> {
        if stamp < 0.0 {
            // Client has no messages yet, return last 10
            if self.messages.len() < 10 {
                return self.messages.clone();
            } else {
                return self.messages[self.messages.len() - 10..].to_vec();
            }
        }

        let idx = self.messages.partition_point(|e| e.stamp <= stamp);

        if prev {
            let start = if idx >= 10 { idx - 10 } else { 0 };
            self.messages[start..idx].to_vec()
        } else {
            if idx < self.messages.len() {
                self.messages[idx + 1..].to_vec()
            } else {
                Vec::new()
            }
        }
    }

    /// Get total number of unique users
    pub fn size(&self) -> usize {
        self.ip_map.len()
    }

    /// Dump full chat history to file
    pub fn dump_full(&self) {
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

        let dump_data = serde_json::json!({
            "umap": self.uid_map,
            "cmap": self.client_map,
            "records": records,
        });

        let filename = self.dump_path.join(format!(
            "live-{}.dump",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        ));

        if let Some(parent) = filename.parent() {
            fs::create_dir_all(parent).ok();
        }

        if let Ok(content) = serde_json::to_string_pretty(&dump_data) {
            fs::write(filename, content).ok();
        }
    }

    /// Dump brief chat history to file
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

/// Wrapper for streaming info (audience count from SRS)
pub struct StreamingInfo {
    pub audiences: i32,
    pub active: Arc<RwLock<bool>>,
}

impl StreamingInfo {
    pub fn new() -> Self {
        Self {
            audiences: -1,
            active: Arc::new(RwLock::new(true)),
        }
    }

    pub fn get_audiences(&self) -> i32 {
        self.audiences
    }

    /// Fetch audience count from SRS API
    pub async fn tick(&mut self, srs_api_url: &str) {
        let api_url = format!("{}/api/v1/clients/", srs_api_url);

        match reqwest::get(&api_url).await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(clients) = json.get("clients").and_then(|c| c.as_array()) {
                            // Subtract 1 to exclude the publisher
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

    /// Start background task
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
