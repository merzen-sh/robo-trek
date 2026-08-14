use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::Html,
};
use serde::Deserialize;

use crate::{AppState, metrics, render};

use super::{ErrorResponse, internal};

pub async fn dashboard_handle(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    let data = current_metrics("home", &state).map_err(internal)?;
    let html = render::render(render::template::HOME, &data).map_err(internal)?;
    Ok(Html(html))
}

pub async fn metrics_handle(
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, (StatusCode, Json<ErrorResponse>)> {
    let data = current_metrics("home", &state).map_err(internal)?;
    let html = render::render(render::template::METRICS, &data).map_err(internal)?;
    Ok(Html(html))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub range: Option<String>,
}

pub async fn history_handle(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let range = query.range.as_deref().unwrap_or("2m");
    let data = history_data(range, &state).await.map_err(internal)?;
    Ok(Json(data))
}

fn current_metrics(active: &str, state: &AppState) -> Result<serde_json::Value, String> {
    let history = state.metrics_history.lock().unwrap_or_else(|p| p.into_inner());
    let latest = history.latest().cloned();
    let snapshot = history.snapshot();

    let (cpu_percent, mem_percent, mem_used_kb, mem_total_kb) = match latest {
        Some(s) => (s.cpu, s.mem_percent, s.mem_used_kb, s.mem_total_kb),
        None => (0.0, 0.0, 0, 0),
    };

    let history_json = serde_json::json!({
        "labels": snapshot.labels,
        "cpu": snapshot.cpu,
        "mem": snapshot.mem,
    });

    Ok(serde_json::json!({
        "active": active,
        "metrics": {
            "cpu_percent": format!("{cpu_percent:.1}"),
            "mem_percent": format!("{mem_percent:.1}"),
            "mem_used_mb": (mem_used_kb / 1024).to_string(),
            "mem_total_mb": (mem_total_kb / 1024).to_string(),
        },
        "history_json": serde_json::to_string(&history_json).unwrap_or_default(),
    }))
}

async fn history_data(range: &str, state: &AppState) -> Result<serde_json::Value, String> {
    match range {
        "1h" | "6h" | "24h" => {
            let range_secs: u64 = match range {
                "1h" => 3600,
                "6h" => 21_600,
                _ => 86_400,
            };
            let since = metrics::now_secs().saturating_sub(range_secs);
            let rows = state.db.get_metrics_since(since).await?;
            let mut labels = Vec::with_capacity(rows.len());
            let mut cpu = Vec::with_capacity(rows.len());
            let mut mem = Vec::with_capacity(rows.len());
            for (ts, c, m) in rows {
                labels.push(metrics::format_label(ts));
                cpu.push(c);
                mem.push(m);
            }
            Ok(serde_json::json!({ "labels": labels, "cpu": cpu, "mem": mem }))
        }
        _ => {
            let snapshot = state
                .metrics_history
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .snapshot();
            Ok(serde_json::json!({
                "labels": snapshot.labels,
                "cpu": snapshot.cpu,
                "mem": snapshot.mem,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_state;

    #[tokio::test]
    async fn metrics_handle_returns_fragment() {
        let (state, _rx) = test_state("metrics");
        let Html(html) = metrics_handle(State(state)).await.unwrap();
        assert!(html.contains("CPU"));
        assert!(html.contains("Memory"));
        assert!(html.contains("metrics-history"));
    }

    #[tokio::test]
    async fn dashboard_handle_renders_home() {
        let (state, _rx) = test_state("home");
        let Html(html) = dashboard_handle(State(state)).await.unwrap();
        assert!(html.contains("Home"));
        assert!(html.contains("htmx"));
        assert!(html.contains("metrics-chart"));
        assert!(html.contains("CPU"));
        assert!(html.contains("Memory"));
    }

    #[tokio::test]
    async fn history_handle_returns_series() {
        let (state, _rx) = test_state("history");
        let Json(value) = history_handle(
            State(state.clone()),
            Query(HistoryQuery {
                range: Some("1h".into()),
            }),
        )
        .await
        .unwrap();
        assert!(value["labels"].is_array());
        assert!(value["cpu"].is_array());
        assert!(value["mem"].is_array());

        state
            .metrics_history
            .lock()
            .unwrap()
            .push(crate::metrics::Sample {
                ts: 100,
                cpu: 12.0,
                mem_percent: 34.0,
                mem_used_kb: 1024,
                mem_total_kb: 4096,
            });
        let Json(value) = history_handle(
            State(state),
            Query(HistoryQuery { range: Some("2m".into()) }),
        )
        .await
        .unwrap();
        assert_eq!(value["cpu"].as_array().unwrap().len(), 1);
    }
}