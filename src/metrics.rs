use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::kv;

#[derive(Clone)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

pub struct Mem {
    pub total_kb: u64,
    pub available_kb: u64,
}

#[derive(Clone)]
pub struct Sample {
    pub ts: u64,
    pub cpu: f64,
    pub mem_percent: f64,
    pub mem_used_kb: u64,
    pub mem_total_kb: u64,
}

pub struct MetricsHistory {
    capacity: usize,
    samples: VecDeque<Sample>,
}

pub struct Snapshot {
    pub labels: Vec<String>,
    pub cpu: Vec<f64>,
    pub mem: Vec<f64>,
}

impl MetricsHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            samples: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, sample: Sample) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut labels = Vec::with_capacity(self.samples.len());
        let mut cpu = Vec::with_capacity(self.samples.len());
        let mut mem = Vec::with_capacity(self.samples.len());
        for sample in &self.samples {
            labels.push(format_label(sample.ts));
            cpu.push(sample.cpu);
            mem.push(sample.mem_percent);
        }
        Snapshot { labels, cpu, mem }
    }

    pub fn latest(&self) -> Option<&Sample> {
        self.samples.back()
    }
}

pub fn read_cpu_times() -> Result<CpuTimes, String> {
    let contents = std::fs::read_to_string("/proc/stat")
        .map_err(|e| format!("failed to read /proc/stat: {e}"))?;
    parse_cpu_times(&contents).ok_or_else(|| "failed to parse /proc/stat".to_string())
}

fn parse_cpu_times(contents: &str) -> Option<CpuTimes> {
    let fields: Vec<u64> = contents
        .lines()
        .next()?
        .split_whitespace()
        .skip(1)
        .filter_map(|f| f.parse().ok())
        .collect();
    let total = fields.iter().sum::<u64>();
    let idle = fields.get(3).copied().unwrap_or(0) + fields.get(4).copied().unwrap_or(0);
    Some(CpuTimes { total, idle })
}

pub fn cpu_percent(prev: &CpuTimes, now: &CpuTimes) -> f64 {
    let total_delta = now.total.saturating_sub(prev.total);
    let idle_delta = now.idle.saturating_sub(prev.idle);
    if total_delta == 0 {
        return 0.0;
    }
    ((1.0 - idle_delta as f64 / total_delta as f64) * 100.0).clamp(0.0, 100.0)
}

pub fn read_mem() -> Result<Mem, String> {
    let contents = std::fs::read_to_string("/proc/meminfo")
        .map_err(|e| format!("failed to read /proc/meminfo: {e}"))?;
    parse_mem(&contents).ok_or_else(|| "failed to parse /proc/meminfo".to_string())
}

fn parse_mem(contents: &str) -> Option<Mem> {
    let mut total = None;
    let mut available = None;
    for line in contents.lines() {
        let mut fields = line.split_whitespace();
        match (fields.next(), fields.next()) {
            (Some("MemTotal:"), Some(v)) => total = v.parse().ok(),
            (Some("MemAvailable:"), Some(v)) => available = v.parse().ok(),
            _ => {}
        }
    }
    Some(Mem {
        total_kb: total?,
        available_kb: available?,
    })
}

pub fn mem_percent(mem: &Mem) -> f64 {
    let used_kb = mem.total_kb.saturating_sub(mem.available_kb);
    if mem.total_kb == 0 {
        0.0
    } else {
        (used_kb as f64 / mem.total_kb as f64 * 100.0).clamp(0.0, 100.0)
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn format_label(secs: u64) -> String {
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

async fn sample_cpu_percent() -> Result<f64, String> {
    let prev = read_cpu_times()?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let now = read_cpu_times()?;
    Ok(cpu_percent(&prev, &now))
}

async fn sample() -> Result<Sample, String> {
    let cpu = sample_cpu_percent().await?;
    let mem = read_mem()?;
    Ok(Sample {
        ts: now_secs(),
        cpu,
        mem_percent: mem_percent(&mem),
        mem_used_kb: mem.total_kb.saturating_sub(mem.available_kb),
        mem_total_kb: mem.total_kb,
    })
}

const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
const SAMPLES_PER_FLUSH: u32 = 30;

/// Continuously samples CPU/memory, feeds the in-memory ring buffer (for the
/// live chart), and persists a per-minute aggregate to redb for long-term
/// history. Keeps history warm even when the dashboard is closed.
pub fn spawn_sampler(
    kv: kv::Kv,
    history: Arc<Mutex<MetricsHistory>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut count = 0u32;
        let mut cpu_sum = 0.0f64;
        let mut mem_sum = 0.0f64;
        loop {
            match sample().await {
                Ok(sample) => {
                    let cpu = sample.cpu;
                    let mem_percent = sample.mem_percent;
                    {
                        let mut h = history.lock().unwrap_or_else(|p| p.into_inner());
                        h.push(sample);
                    }

                    cpu_sum += cpu;
                    mem_sum += mem_percent;
                    count += 1;
                    if count >= SAMPLES_PER_FLUSH {
                        if let Err(e) = kv
                            .put_metrics(
                                now_secs(),
                                cpu_sum / f64::from(count),
                                mem_sum / f64::from(count),
                            )
                            .await
                        {
                            eprintln!("failed to persist metrics: {e}");
                        }
                        cpu_sum = 0.0;
                        mem_sum = 0.0;
                        count = 0;
                    }
                }
                Err(e) => eprintln!("metrics sampler error: {e}"),
            }
            tokio::time::sleep(SAMPLE_INTERVAL).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  1234 5 678 9000 100 20 30 0 0 0\ncpu0 100 0 50 500 0 0 0 0 0 0\n";

    #[test]
    fn parses_cpu_times() {
        let times = parse_cpu_times(STAT).unwrap();
        assert_eq!(times.idle, 9000 + 100);
        assert_eq!(times.total, 1234 + 5 + 678 + 9000 + 100 + 20 + 30);
    }

    #[test]
    fn cpu_percent_uses_delta_between_samples() {
        let prev = parse_cpu_times(STAT).unwrap();
        let mut now = prev.clone();
        now.total += 1000;
        now.idle += 100;
        assert!((cpu_percent(&prev, &now) - 90.0).abs() < 0.001);
    }

    #[test]
    fn cpu_percent_zero_when_no_delta() {
        let prev = parse_cpu_times(STAT).unwrap();
        assert_eq!(cpu_percent(&prev, &prev), 0.0);
    }

    #[test]
    fn parses_meminfo() {
        let contents =
            "MemTotal:        8192000 kB\nMemAvailable:    4096000 kB\nSwapTotal:       0 kB\n";
        let mem = parse_mem(contents).unwrap();
        assert_eq!(mem.total_kb, 8192000);
        assert_eq!(mem.available_kb, 4096000);
        assert!((mem_percent(&mem) - 50.0).abs() < 0.001);
    }

    #[test]
    fn history_keeps_most_recent_samples() {
        let mut history = MetricsHistory::new(3);
        history.push(sample_at(1, 10.0, 20.0));
        history.push(sample_at(2, 30.0, 40.0));
        history.push(sample_at(3, 50.0, 60.0));
        history.push(sample_at(4, 70.0, 80.0));

        let snapshot = history.snapshot();
        assert_eq!(snapshot.cpu, vec![30.0, 50.0, 70.0]);
        assert_eq!(snapshot.mem, vec![40.0, 60.0, 80.0]);
        assert_eq!(snapshot.labels.len(), 3);
        assert!(snapshot.labels[0].len() == 8);
        assert_eq!(history.latest().map(|s| s.cpu), Some(70.0));
    }

    fn sample_at(ts: u64, cpu: f64, mem: f64) -> Sample {
        Sample {
            ts,
            cpu,
            mem_percent: mem,
            mem_used_kb: 100,
            mem_total_kb: 200,
        }
    }

    #[test]
    fn formats_timestamp_as_hh_mm_ss() {
        assert_eq!(format_label(0), "00:00:00");
        assert_eq!(format_label(86399), "23:59:59");
        assert_eq!(format_label(3661), "01:01:01");
    }
}
