// ============================================================================
// 流信息结构体
// ============================================================================

use std::sync::Arc;

use parking_lot::{RwLock};
use tokio::task::JoinHandle;

/// 流信息统计
///
/// 用于从 SRS API 获取观众人数信息
#[derive(Clone)]
pub struct StreamingInfoInner {
    /// 当前观众人数（-1 表示未知）
    pub audiences_num: i32,
}

impl StreamingInfoInner {
    /// 创建新的流信息对象
    pub fn new() -> Self {
        Self { audiences_num: 0 }
    }

    /// 获取当前观众人数
    pub fn get_audiences_num(&self) -> i32 {
        self.audiences_num
    }

    pub fn set_audiences_num(&mut self, num: i32) {
        self.audiences_num = num;
    }
}

#[derive(Clone)]
pub struct StreamingInfo {
    pub inner: Arc<RwLock<StreamingInfoInner>>,
}

impl StreamingInfo {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StreamingInfoInner::new())),
        }
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
    pub fn tick(self, srs_api_url: String) -> JoinHandle<()> {
        let api_url = format!("http://{}/api/v1/clients/", srs_api_url);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

            // 禁用代理，避免本地请求被系统代理拦截
            let client = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap();

            loop {
                interval.tick().await;

                let mut new_num = -1;
                match client.get(&api_url).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                if let Some(clients) =
                                    json.get("clients").and_then(|c| c.as_array())
                                {
                                    // 减去 1 排除推流端
                                    new_num = clients.len().saturating_sub(1) as i32;
                                } else {
                                    tracing::warn!(
                                    "GET from {}, received response and status is success, but response has no key\"clients\"",
                                    api_url
                                );
                                }
                            }
                        } else {
                            tracing::warn!(
                                "GET from {}, received response but status is not success",
                                api_url
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!("GET from {} error: {}", api_url, e);
                    }
                };

                let mut inner = self.inner.write();
                inner.set_audiences_num(new_num);
            }
        })
    }
}
