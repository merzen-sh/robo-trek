use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::AppState;

use super::{ErrorResponse, internal};

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
}

#[derive(Serialize)]
pub struct PingResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct WebhookRelease {
    pub version: String,
}

pub async fn health_handle() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "robo-trek".to_string(),
    })
}

pub async fn ping_handle() -> Json<PingResponse> {
    Json(PingResponse {
        message: "pong".to_string(),
    })
}

pub async fn release_webhook_handle(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WebhookRelease>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    state
        .release_tx
        .send(req.version)
        .await
        .map_err(|e| internal(format!("failed to queue release: {e}")))?;

    Ok(Json(serde_json::json!({"status": "ok"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;

    #[tokio::test]
    async fn health_handle_reports_ok() {
        let resp = health_handle().await;
        assert_eq!(resp.0.status, "ok");
        assert_eq!(resp.0.service, "robo-trek");
    }

    #[tokio::test]
    async fn ping_handle_reports_pong() {
        let resp = ping_handle().await;
        assert_eq!(resp.0.message, "pong");
    }

    #[tokio::test]
    async fn release_webhook_handle_queues_version() {
        let (state, mut rx) = test_state("webhook");
        let resp = release_webhook_handle(
            State(state),
            Json(WebhookRelease {
                version: "v3.0.0".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(resp.0, serde_json::json!({"status": "ok"}));
        assert_eq!(rx.try_recv().unwrap(), "v3.0.0");
    }
}
