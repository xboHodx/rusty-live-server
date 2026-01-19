use axum::{Json, response::Response};
use crate::state::AppState;
use axum::{
    extract::State,
    response::{IntoResponse},
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
struct StreamingInfoReasponse {
    /// 观众数
    audiences_num: i32,
}

impl StreamingInfoReasponse {
    pub fn new() -> Self {
        Self {
            audiences_num: 0,
        }
    }

    pub fn with_stream_name(mut self, num: i32) -> Self {
        self.audiences_num = num;
        self
    }
}

pub async fn streaming_info_handler(
    State(state): State<Arc<AppState>>
) -> Response {
    let response = StreamingInfoReasponse::new();

    let streaming_info = state.streaming_info.clone();
    let streaming_info_guard = streaming_info.inner.read();
    let response = response.with_stream_name(streaming_info_guard.get_audiences_num());

    return Json(response).into_response();
}

