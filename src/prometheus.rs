use ::prometheus::{Encoder, Gauge, Registry, TextEncoder};

/// Prometheus registry and gauges backing long-term host metrics. Samples are
/// recorded by the metrics sampler and scraped by a Prometheus server, which
/// stores the history for Grafana.
pub struct Metrics {
    registry: Registry,
    cpu_percent: Gauge,
    mem_percent: Gauge,
    mem_used_bytes: Gauge,
    mem_total_bytes: Gauge,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        let registry = Registry::new();
        let cpu_percent =
            Gauge::new("robo_trek_cpu_percent", "Host CPU usage percent").expect("gauge");
        let mem_percent =
            Gauge::new("robo_trek_memory_percent", "Host memory usage percent").expect("gauge");
        let mem_used_bytes =
            Gauge::new("robo_trek_memory_used_bytes", "Host memory in use in bytes")
                .expect("gauge");
        let mem_total_bytes =
            Gauge::new("robo_trek_memory_total_bytes", "Host total memory in bytes")
                .expect("gauge");
        for gauge in [
            Box::new(cpu_percent.clone()),
            Box::new(mem_percent.clone()),
            Box::new(mem_used_bytes.clone()),
            Box::new(mem_total_bytes.clone()),
        ] {
            registry.register(gauge).expect("register gauge");
        }
        Self {
            registry,
            cpu_percent,
            mem_percent,
            mem_used_bytes,
            mem_total_bytes,
        }
    }

    /// Records the latest sample into the gauges; Prometheus scrapes whatever
    /// is current, so no per-minute aggregation is needed.
    pub fn record(&self, cpu: f64, mem: f64, mem_used_kb: u64, mem_total_kb: u64) {
        self.cpu_percent.set(cpu);
        self.mem_percent.set(mem);
        self.mem_used_bytes.set(mem_used_kb as f64 * 1024.0);
        self.mem_total_bytes.set(mem_total_kb as f64 * 1024.0);
    }

    /// Renders the Prometheus text exposition format for scraping.
    pub fn render(&self) -> String {
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        if encoder
            .encode(&self.registry.gather(), &mut buffer)
            .is_err()
        {
            return String::new();
        }
        String::from_utf8(buffer).unwrap_or_default()
    }
}
