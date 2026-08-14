use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, handlers};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(handlers::api::health_handle))
        .route("/ping", get(handlers::api::ping_handle))
        .route(
            "/webhook/release",
            post(handlers::api::release_webhook_handle),
        )
}
