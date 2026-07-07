use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Response},
    routing::{get, post},
    Json, Router,
};
use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AppState {
    pub release_tx: Sender<String>,
}

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

async fn auth(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let api_key = std::env::var("API_KEY").expect("API_KEY must be set");
    let header = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if header != api_key {
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
        .layer(middleware::from_fn(auth))
        .with_state(state);

    let port = std::env::var("API_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .expect("API_PORT must be a valid port number");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("API server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
