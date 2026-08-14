use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode, header::CONTENT_TYPE},
    middleware::{self, Next},
    response::Response,
};

use crate::AppState;

mod api;
mod dashboard;

fn provided_key(req: &Request<Body>) -> &str {
    let header_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let cookie_key = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .filter_map(|part| {
            let mut it = part.trim().splitn(2, '=');
            let name = it.next()?;
            let val = it.next()?;
            (name == "api_key").then_some(val)
        })
        .next()
        .unwrap_or("");

    if !header_key.is_empty() {
        header_key
    } else {
        cookie_key
    }
}

const PROMPT_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>API key required</title>
</head>
<body>
  <script>
    var key = prompt("Enter API key");
    if (key) {
      document.cookie = "api_key=" + encodeURIComponent(key) + "; path=/; max-age=86400; SameSite=Lax";
      location.reload();
    } else {
      history.back();
    }
  </script>
  <p>API key required. If scripts are disabled, reload with the key via the browser's JS prompt.</p>
</body>
</html>"#;

async fn auth(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if provided_key(&req) == state.config.api_key {
        return Ok(next.run(req).await);
    }

    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(CONTENT_TYPE, "text/html")
        .body(Body::from(PROMPT_HTML))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::<Arc<AppState>>::new()
        .nest("/api", api::router())
        .nest("/dashboard", dashboard::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn req(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    async fn body_str(resp: Response) -> String {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn provided_key_prefers_header_over_cookie() {
        let req = Request::builder()
            .uri("/")
            .header("x-api-key", "header-key")
            .header("cookie", "api_key=cookie-key")
            .body(Body::empty())
            .unwrap();
        assert_eq!(provided_key(&req), "header-key");
    }

    #[test]
    fn provided_key_parses_cookie() {
        let req = Request::builder()
            .uri("/")
            .header("cookie", "other=1; api_key=secret; foo=2")
            .body(Body::empty())
            .unwrap();
        assert_eq!(provided_key(&req), "secret");
    }

    #[test]
    fn provided_key_empty_when_absent() {
        assert_eq!(provided_key(&req("/")), "");
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
}
