use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{StatusCode, header::CONTENT_TYPE},
    response::{Html, Response},
};

use crate::{AppState, render};

use super::{ErrorResponse, internal};

pub async fn dashboard_handle(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    let data = home_stats(&state).await.map_err(internal)?;
    let html = render::render(render::template::HOME, &data).map_err(internal)?;
    Ok(Html(html))
}

/// Serves the Prometheus text exposition format. Intentionally kept outside
/// the API-key auth layer so the Prometheus scraper can collect it.
pub async fn prometheus_scrape_handle(State(state): State<Arc<AppState>>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")
        .body(Body::from(state.prometheus.render()))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap()
        })
}

/// Builds the home page "box stats": ticket counts by status plus the number
/// of cached releases, each linking to the relevant dashboard page.
async fn home_stats(state: &AppState) -> Result<serde_json::Value, String> {
    use crate::storages::tickets::TicketFilters;

    async fn count_tickets(state: &AppState, status: Option<&str>) -> Result<i64, String> {
        state
            .tickets
            .count_tickets(&TicketFilters {
                status: status.map(str::to_string),
                search: None,
            })
            .await
            .map_err(|e| format!("failed to count tickets: {e}"))
    }

    let (open, in_progress, closed, total) = tokio::join!(
        count_tickets(state, Some("open")),
        count_tickets(state, Some("in_progress")),
        count_tickets(state, Some("closed")),
        count_tickets(state, None),
    );
    let open = open?;
    let in_progress = in_progress?;
    let closed = closed?;
    let total = total?;

    let releases = state
        .releases
        .list_releases()
        .await
        .map_err(|e| format!("failed to list releases: {e}"))?
        .len();

    let stats = serde_json::json!([
        {
            "label": "Open Tickets",
            "value": open,
            "href": "/dashboard/tickets?status=open",
        },
        {
            "label": "In Progress",
            "value": in_progress,
            "href": "/dashboard/tickets?status=in_progress",
        },
        {
            "label": "Closed Tickets",
            "value": closed,
            "href": "/dashboard/tickets?status=closed",
        },
        { "label": "Total Tickets", "value": total, "href": "/dashboard/tickets" },
        { "label": "Releases", "value": releases, "href": "/dashboard/releases" },
    ]);

    Ok(serde_json::json!({ "active": "home", "stats": stats }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;

    #[tokio::test]
    async fn dashboard_handle_renders_stat_boxes() {
        let (state, _rx) = test_state("home").await;
        let Html(html) = dashboard_handle(State(state)).await.unwrap();
        assert!(html.contains("Home"));
        assert!(html.contains("Open Tickets"));
        assert!(html.contains("In Progress"));
        assert!(html.contains("Closed Tickets"));
        assert!(html.contains("Total Tickets"));
        assert!(html.contains("Releases"));
    }

    #[tokio::test]
    async fn dashboard_handle_shows_counts() {
        let (state, _rx) = test_state("home-stats").await;
        state
            .tickets
            .create_ticket("g", "u1", "mark", "Server down", "")
            .await
            .unwrap();
        state
            .tickets
            .create_ticket("g", "u2", "jane", "UI bug", "")
            .await
            .unwrap();
        let third = state
            .tickets
            .create_ticket("g", "u3", "bob", "Deploy", "")
            .await
            .unwrap();
        state.tickets.close_ticket(third.id, "staff").await.unwrap();
        state.releases.put_release("v1.0.0", b"png").await.unwrap();
        state.releases.put_release("v2.0.0", b"png").await.unwrap();

        let Html(html) = dashboard_handle(State(state)).await.unwrap();
        assert!(html.contains(">2</div>")); // open tickets
        assert!(html.contains(">1</div>")); // closed tickets
        assert!(html.contains(">3</div>")); // total tickets
        assert!(html.contains(">2</div>")); // releases
    }
}
