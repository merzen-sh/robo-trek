use std::sync::Arc;

use axum::{Router, middleware, routing::get};

use crate::{AppState, handlers, middlewares::auth::auth};

mod api;
mod dashboard;

pub fn router(state: Arc<AppState>) -> Router {
    let protected = Router::<Arc<AppState>>::new()
        .route("/", get(handlers::home::dashboard_handle))
        .nest("/api", api::router())
        .nest("/dashboard", dashboard::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth));

    // The Prometheus scrape endpoint stays outside the auth layer so the
    // scraper can collect it without an API key.
    Router::<Arc<AppState>>::new()
        .route(
            "/prometheus/metrics",
            get(handlers::home::prometheus_scrape_handle),
        )
        .merge(protected)
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
        let app = router(test_state("api-auth").await.0);
        let resp = app.clone().oneshot(req("/api/ping")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let html = body_str(resp).await;
        assert!(html.contains("Enter API key"));
    }

    #[tokio::test]
    async fn api_group_accepts_header_key() {
        let app = router(test_state("api-header").await.0);
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
        let app = router(test_state("dash-cookie").await.0);
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
        let app = router(test_state("home-auth").await.0);
        let resp = app.clone().oneshot(req("/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn home_accepts_cookie_key() {
        let app = router(test_state("home-cookie").await.0);
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

    #[tokio::test]
    async fn tickets_page_requires_auth() {
        let app = router(test_state("tickets-auth").await.0);
        let resp = app
            .clone()
            .oneshot(req("/dashboard/tickets"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn tickets_page_accepts_cookie_key() {
        let app = router(test_state("tickets-cookie").await.0);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard/tickets")
                    .header("cookie", "api_key=secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_str(resp).await.contains("Tickets"));
    }

    #[tokio::test]
    async fn prometheus_scrape_endpoint_is_public() {
        let app = router(test_state("prom-scrape").await.0);
        let resp = app.oneshot(req("/prometheus/metrics")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_str(resp).await;
        assert!(body.contains("robo_trek_cpu_percent"));
        assert!(body.contains("robo_trek_memory_percent"));
    }
}
