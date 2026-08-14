use std::sync::Arc;

use axum::{Router, routing::get};

use crate::{AppState, handlers};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/releases", get(handlers::dashboard::releases_page_handle))
        .route("/tickets", get(handlers::dashboard::tickets_page_handle))
        .route(
            "/tickets/fragment",
            get(handlers::dashboard::tickets_fragment_handle),
        )
        .route(
            "/releases/:version",
            get(handlers::dashboard::release_image_handle),
        )
}
