use std::sync::Arc;

use axum::{Router, middleware, routing::get};

use crate::{AppState, handlers, middlewares::auth::auth};

mod api;
mod dashboard;

pub fn router(state: Arc<AppState>) -> Router {
    Router::<Arc<AppState>>::new()
        .route("/", get(handlers::home::dashboard_handle))
        .route("/metrics", get(handlers::home::metrics_handle))
        .route("/metrics/history", get(handlers::home::history_handle))
        .nest("/api", api::router())
        .nest("/dashboard", dashboard::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::Response,
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn req(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    async fn body_str(resp: Response) -> String {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn api_group_requires_auth() {
        let app = router(test_state("api-auth").0);
        let resp = app.clone().oneshot(req("/api/ping")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let html = body_str(resp).await;
        assert!(html.contains("Enter API key"));
    }

    #[tokio::test]
    async fn api_group_accepts_header_key() {
        let app = router(test_state("api-header").0);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/ping")
                    .header("x-api-key", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_str(resp).await, "{\"message\":\"pong\"}");
    }

    #[tokio::test]
    async fn dashboard_group_accepts_cookie_key() {
        let app = router(test_state("dash-cookie").0);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard/releases")
                    .header("cookie", "api_key=secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_str(resp).await.contains("Releases"));
    }

    #[tokio::test]
    async fn home_requires_auth() {
        let app = router(test_state("home-auth").0);
        let resp = app.clone().oneshot(req("/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn home_accepts_cookie_key() {
        let app = router(test_state("home-cookie").0);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("cookie", "api_key=secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_str(resp).await.contains("Home"));
    }
}
