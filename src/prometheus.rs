use ::prometheus::{Encoder, Error, Gauge, Registry, TextEncoder};

/// Host metrics exposed to Prometheus for scraping.
pub struct Metrics {
    registry: Registry,
    cpu_percent: Gauge,
    mem_percent: Gauge,
    mem_used_bytes: Gauge,
    mem_total_bytes: Gauge,
}

/// A single CPU/memory sample recorded into the gauges.
pub struct HostSample {
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub mem_used_kb: u64,
    pub mem_total_kb: u64,
}

impl Metrics {
    pub fn new() -> Result<Self, Error> {
        let registry = Registry::new();
        let cpu_percent = Gauge::new("robo_trek_cpu_percent", "Host CPU usage percent")?;
        let mem_percent = Gauge::new("robo_trek_memory_percent", "Host memory usage percent")?;
        let mem_used_bytes =
            Gauge::new("robo_trek_memory_used_bytes", "Host memory in use in bytes")?;
        let mem_total_bytes =
            Gauge::new("robo_trek_memory_total_bytes", "Host total memory in bytes")?;
        for gauge in [
            Box::new(cpu_percent.clone()),
            Box::new(mem_percent.clone()),
            Box::new(mem_used_bytes.clone()),
            Box::new(mem_total_bytes.clone()),
        ] {
            registry.register(gauge)?;
        }
        Ok(Self {
            registry,
            cpu_percent,
            mem_percent,
            mem_used_bytes,
            mem_total_bytes,
        })
    }

    /// Records the latest sample; Prometheus scrapes whatever is current, so
    /// no per-minute aggregation is needed.
    pub fn record(&self, sample: &HostSample) {
        self.cpu_percent.set(sample.cpu_percent);
        self.mem_percent.set(sample.mem_percent);
        self.mem_used_bytes.set(sample.mem_used_kb as f64 * 1024.0);
        self.mem_total_bytes
            .set(sample.mem_total_kb as f64 * 1024.0);
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
