use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{Html, Response},
};

use crate::{AppState, render};

use super::{ErrorResponse, internal};

pub async fn releases_page_handle(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    let versions = state.db.list_releases().await.map_err(internal)?;
    let data = serde_json::json!({ "active": "releases", "releases": versions });
    let html = render::render(render::template::RELEASES, &data).map_err(internal)?;
    Ok(Html(html))
}

pub async fn release_image_handle(
    State(state): State<Arc<AppState>>,
    Path(version): Path<String>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    match state.db.get_release(&version).await {
        Ok(Some(png)) => Response::builder()
            .header(CONTENT_TYPE, "image/png")
            .body(Body::from(png))
            .map_err(|e| internal(e.to_string())),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("release {version} not found"),
            }),
        )),
        Err(e) => Err(internal(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;

    #[tokio::test]
    async fn releases_page_handle_lists_cached_versions() {
        let (state, _rx) = test_state("page");
        state.db.put_release("v1.0.0", b"png").await.unwrap();
        let Html(html) = releases_page_handle(State(state)).await.unwrap();
        assert!(html.contains("v1.0.0"));
    }

    #[tokio::test]
    async fn releases_page_handle_empty_state() {
        let (state, _rx) = test_state("page-empty");
        let Html(html) = releases_page_handle(State(state)).await.unwrap();
        assert!(html.contains("No releases yet"));
    }

    #[tokio::test]
    async fn release_image_handle_serves_png() {
        let (state, _rx) = test_state("img");
        state.db.put_release("v1.0.0", b"png-data").await.unwrap();
        let resp = release_image_handle(State(state), Path("v1.0.0".into()))
            .await
            .unwrap();
        assert_eq!(resp.headers()[CONTENT_TYPE], "image/png");
    }

    #[tokio::test]
    async fn release_image_handle_not_found() {
        let (state, _rx) = test_state("img-missing");
        let (status, _json) = release_image_handle(State(state), Path("v9.9.9".into()))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
