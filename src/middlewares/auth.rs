use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header::CONTENT_TYPE},
    middleware::Next,
    response::Response,
};

use crate::AppState;

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
            let (name, val) = part.trim().split_once('=')?;
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

pub async fn auth(
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

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
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert_eq!(provided_key(&req), "");
    }
}
