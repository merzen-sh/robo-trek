use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{Html, IntoResponse, Redirect, Response},
};

use crate::storages::tickets::TicketStore;
use crate::{AppState, render};

use super::{ErrorResponse, internal};

pub async fn releases_page_handle(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    let versions = state
        .releases
        .list_releases()
        .await
        .map_err(|e| internal(e.to_string()))?;
    let data = serde_json::json!({ "active": "releases", "releases": versions });
    let html = render::render(render::template::RELEASES, &data).map_err(internal)?;
    Ok(Html(html))
}

#[derive(serde::Deserialize)]
pub struct TicketQuery {
    page: Option<i64>,
    q: Option<String>,
    status: Option<String>,
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn tickets_data(
    state: &AppState,
    query: &TicketQuery,
) -> Result<serde_json::Value, (StatusCode, Json<ErrorResponse>)> {
    use crate::storages::tickets::TicketFilters;

    let filters = TicketFilters {
        status: query.status.clone().filter(|s| !s.is_empty()),
        search: query
            .q
            .clone()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };
    let has_filter = filters.status.is_some() || filters.search.is_some();

    let total = state
        .tickets
        .count_tickets(&filters)
        .await
        .map_err(|e| internal(e.to_string()))?;
    let total_pages = ((total + TicketStore::PAGE_SIZE - 1) / TicketStore::PAGE_SIZE).max(1);
    let page = query.page.unwrap_or(1).clamp(1, total_pages);

    let tickets = state
        .tickets
        .list_tickets_page(page, &filters)
        .await
        .map_err(|e| internal(e.to_string()))?;
    let rows: Vec<serde_json::Value> = tickets
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "username": t.username,
                "subject": t.subject,
                "status": t.status,
                "opened_label": t.opened_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            })
        })
        .collect();

    let mut qstr = String::new();
    if let Some(q) = &filters.search {
        qstr.push_str("&q=");
        qstr.push_str(&percent_encode(q));
    }
    if let Some(s) = &filters.status {
        qstr.push_str("&status=");
        qstr.push_str(&percent_encode(s));
    }

    let mut pages = Vec::new();
    let start = (page - 2).max(1);
    let end = (page + 2).min(total_pages);
    for n in start..=end {
        pages.push(serde_json::json!({ "n": n, "current": n == page }));
    }

    Ok(serde_json::json!({
        "active": "tickets",
        "tickets": rows,
        "total": total,
        "page": page,
        "total_pages": total_pages,
        "has_prev": page > 1,
        "has_next": page < total_pages,
        "prev_page": (page - 1).max(1),
        "next_page": (page + 1).min(total_pages),
        "has_pagination": total_pages > 1,
        "pages": pages,
        "q": filters.search.clone().unwrap_or_default(),
        "status": filters.status.clone().unwrap_or_default(),
        "qstr": qstr,
        "has_filter": has_filter,
    }))
}

pub async fn tickets_page_handle(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TicketQuery>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    let data = tickets_data(&state, &query).await?;
    let html = render::render(render::template::TICKETS, &data).map_err(internal)?;
    Ok(Html(html))
}

pub async fn tickets_fragment_handle(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TicketQuery>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if !headers.contains_key("hx-request") {
        let mut parts = Vec::new();
        if let Some(page) = query.page.filter(|p| *p > 1) {
            parts.push(format!("page={page}"));
        }
        if let Some(q) = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            parts.push(format!("q={}", percent_encode(q)));
        }
        if let Some(s) = query.status.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("status={}", percent_encode(s)));
        }
        let mut target = "/dashboard/tickets".to_string();
        if !parts.is_empty() {
            target.push('?');
            target.push_str(&parts.join("&"));
        }
        return Ok(Redirect::to(&target).into_response());
    }
    let data = tickets_data(&state, &query).await?;
    let html = render::render(render::template::TICKETS_LIST, &data).map_err(internal)?;
    Ok(Html(html).into_response())
}

pub async fn release_image_handle(
    State(state): State<Arc<AppState>>,
    Path(version): Path<String>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    match state.releases.get_release(&version).await {
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
        Err(e) => Err(internal(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;

    fn hx_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("hx-request", "true".parse().unwrap());
        headers
    }

    async fn fragment_body(state: Arc<AppState>, query: TicketQuery) -> String {
        let resp = tickets_fragment_handle(State(state), Query(query), hx_headers())
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn releases_page_handle_lists_cached_versions() {
        let (state, _rx) = test_state("page").await;
        state.releases.put_release("v1.0.0", b"png").await.unwrap();
        let Html(html) = releases_page_handle(State(state)).await.unwrap();
        assert!(html.contains("v1.0.0"));
    }

    #[tokio::test]
    async fn releases_page_handle_empty_state() {
        let (state, _rx) = test_state("page-empty").await;
        let Html(html) = releases_page_handle(State(state)).await.unwrap();
        assert!(html.contains("No releases yet"));
    }

    #[tokio::test]
    async fn release_image_handle_serves_png() {
        let (state, _rx) = test_state("img").await;
        state
            .releases
            .put_release("v1.0.0", b"png-data")
            .await
            .unwrap();
        let resp = release_image_handle(State(state), Path("v1.0.0".into()))
            .await
            .unwrap();
        assert_eq!(resp.headers()[CONTENT_TYPE], "image/png");
    }

    #[tokio::test]
    async fn release_image_handle_not_found() {
        let (state, _rx) = test_state("img-missing").await;
        let (status, _json) = release_image_handle(State(state), Path("v9.9.9".into()))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn tickets_page_handle_lists_tickets() {
        let (state, _rx) = test_state("tickets").await;
        state
            .tickets
            .create_ticket("g", "u1", "mark", "Broken thing", "It broke")
            .await
            .unwrap();
        let Html(html) = tickets_page_handle(
            State(state),
            Query(TicketQuery {
                page: None,
                q: None,
                status: None,
            }),
        )
        .await
        .unwrap();
        assert!(html.contains("Broken thing"));
        assert!(html.contains("mark"));
    }

    #[tokio::test]
    async fn tickets_page_handle_empty_state() {
        let (state, _rx) = test_state("tickets-empty").await;
        let Html(html) = tickets_page_handle(
            State(state),
            Query(TicketQuery {
                page: None,
                q: None,
                status: None,
            }),
        )
        .await
        .unwrap();
        assert!(html.contains("No tickets yet"));
    }

    #[tokio::test]
    async fn tickets_page_handle_paginates() {
        let (state, _rx) = test_state("tickets-page").await;
        for i in 0..25 {
            state
                .tickets
                .create_ticket("g", "u1", "mark", &format!("Ticket {i}"), "")
                .await
                .unwrap();
        }
        let Html(page1) = tickets_page_handle(
            State(state.clone()),
            Query(TicketQuery {
                page: None,
                q: None,
                status: None,
            }),
        )
        .await
        .unwrap();
        let Html(page2) = tickets_page_handle(
            State(state.clone()),
            Query(TicketQuery {
                page: Some(2),
                q: None,
                status: None,
            }),
        )
        .await
        .unwrap();
        assert!(page1.contains("Ticket 24"));
        assert!(!page1.contains("Ticket 9"));
        assert!(page1.contains(r#"href="/dashboard/tickets?page=2""#));
        assert!(page1.contains("Page 1 of 3"));
        assert!(page2.contains("Ticket 9"));
        assert!(page2.contains("Page 2 of 3"));
    }

    #[tokio::test]
    async fn tickets_page_handle_clamps_out_of_range_page() {
        let (state, _rx) = test_state("tickets-clamp").await;
        for i in 0..25 {
            state
                .tickets
                .create_ticket("g", "u1", "mark", &format!("Ticket {i}"), "")
                .await
                .unwrap();
        }
        let Html(html) = tickets_page_handle(
            State(state),
            Query(TicketQuery {
                page: Some(999),
                q: None,
                status: None,
            }),
        )
        .await
        .unwrap();
        assert!(html.contains("Ticket 0"));
        assert!(html.contains("Page 3 of 3"));
    }

    #[tokio::test]
    async fn tickets_fragment_handle_returns_partial_only() {
        let (state, _rx) = test_state("tickets-fragment").await;
        for i in 0..25 {
            state
                .tickets
                .create_ticket("g", "u1", "mark", &format!("Ticket {i}"), "")
                .await
                .unwrap();
        }
        let html = fragment_body(
            state,
            TicketQuery {
                page: Some(2),
                q: None,
                status: None,
            },
        )
        .await;
        assert!(html.contains("Ticket 9"));
        assert!(!html.contains("<!DOCTYPE html>"));
        assert!(html.contains(r#"hx-get="/dashboard/tickets/fragment?page=3""#));
        assert!(html.contains(r##"hx-target="#tickets-list""##));
    }

    #[tokio::test]
    async fn tickets_fragment_handle_searches_and_filters() {
        let (state, _rx) = test_state("tickets-search").await;
        for i in 0..12 {
            state
                .tickets
                .create_ticket("g", "u1", "mark", &format!("Server down {i}"), "outage")
                .await
                .unwrap();
        }
        for i in 0..12 {
            let t = state
                .tickets
                .create_ticket("g", "u2", "jane", &format!("UI bug {i}"), "styling")
                .await
                .unwrap();
            state.tickets.close_ticket(t.id, "staff").await.unwrap();
        }

        let search = fragment_body(
            state.clone(),
            TicketQuery {
                page: None,
                q: Some("Server".into()),
                status: None,
            },
        )
        .await;
        assert!(search.contains("Server down 11"));
        assert!(!search.contains("Server down 0"));
        assert!(!search.contains("UI bug"));
        assert!(search.contains(r#"href="/dashboard/tickets?page=1&q=Server""#));
        assert!(search.contains(r#"hx-get="/dashboard/tickets/fragment?page=2&q=Server""#));

        let closed_only = fragment_body(
            state.clone(),
            TicketQuery {
                page: None,
                q: None,
                status: Some("closed".into()),
            },
        )
        .await;
        assert!(closed_only.contains("UI bug 11"));
        assert!(!closed_only.contains("UI bug 0"));
        assert!(!closed_only.contains("Server down"));
        assert!(closed_only.contains(r#"href="/dashboard/tickets?page=1&status=closed""#));
        assert!(
            closed_only.contains(r#"hx-get="/dashboard/tickets/fragment?page=2&status=closed""#)
        );
    }

    #[tokio::test]
    async fn tickets_fragment_handle_redirects_browser_navigation() {
        let (state, _rx) = test_state("tickets-fragment-direct").await;
        state
            .tickets
            .create_ticket("g", "u1", "mark", "Server down", "")
            .await
            .unwrap();
        let resp = tickets_fragment_handle(
            State(state),
            Query(TicketQuery {
                page: Some(2),
                q: Some("Server down".into()),
                status: Some("open".into()),
            }),
            HeaderMap::new(),
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            resp.headers()["location"],
            "/dashboard/tickets?page=2&q=Server%20down&status=open"
        );
    }

    #[tokio::test]
    async fn tickets_fragment_handle_empty_filter_results() {
        let (state, _rx) = test_state("tickets-nomatch").await;
        state
            .tickets
            .create_ticket("g", "u1", "mark", "Server down", "")
            .await
            .unwrap();
        let html = fragment_body(
            state,
            TicketQuery {
                page: None,
                q: Some("zzz-not-found".into()),
                status: None,
            },
        )
        .await;
        assert!(html.contains("No tickets match your filters."));
    }
}
