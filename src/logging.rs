use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config;

/// Configures global tracing. The level comes from `LOG_LEVEL` (or `RUST_LOG`
/// if set); every emitted event is also counted into the Prometheus registry.
pub fn init_tracing(config: &config::Config) {
    let default = config
        .log_level
        .parse::<tracing::Level>()
        .unwrap_or(tracing::Level::INFO)
        .to_string();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
