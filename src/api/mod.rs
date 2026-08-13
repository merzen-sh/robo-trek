use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    service: String,
}

#[derive(Serialize)]
struct PingResponse {
    message: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Deserialize)]
struct WebhookRelease {
    version: String,
}

async fn auth(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let header = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if header != state.config.api_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "robo-trek".to_string(),
    })
}

async fn ping() -> Json<PingResponse> {
    Json(PingResponse {
        message: "pong".to_string(),
    })
}

async fn release_webhook(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WebhookRelease>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    state
        .release_tx
        .send(req.version)
        .map_err(|e| internal(format!("failed to queue release: {e}")))?;

    Ok(Json(serde_json::json!({"status": "ok"})))
}

fn internal(msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { error: msg }),
    )
}

pub async fn serve(state: Arc<AppState>) {
    let app = Router::new()
        .route("/", get(health))
        .route("/ping", get(ping))
        .route("/webhook/release", post(release_webhook))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], state.config.api_port));
    println!("API server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
