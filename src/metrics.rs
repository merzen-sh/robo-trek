use std::{sync::Arc, time::Duration};
use tracing::error;

use crate::prometheus::{HostSample, Metrics};

#[derive(Clone)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

pub struct Mem {
    pub total_kb: u64,
    pub available_kb: u64,
}

pub fn read_cpu_times() -> Result<CpuTimes, String> {
    let contents = std::fs::read_to_string("/proc/stat")
        .map_err(|e| format!("failed to read /proc/stat: {e}"))?;
    parse_cpu_times(&contents).ok_or_else(|| "failed to parse /proc/stat".to_string())
}

fn parse_cpu_times(contents: &str) -> Option<CpuTimes> {
    // Single pass over the token stream: sum every field for the total while
    // capturing the idle fields (indexes 3 and 4) by position. No heap
    // allocation, unlike collecting into a Vec first.
    let (total, idle) = contents
        .lines()
        .next()?
        .split_whitespace()
        .skip(1)
        .filter_map(|f| f.parse::<u64>().ok())
        .enumerate()
        .fold((0u64, 0u64), |(total, idle), (i, v)| {
            (total + v, if i == 3 || i == 4 { idle + v } else { idle })
        });
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

async fn sample_cpu_percent() -> Result<f64, String> {
    let prev = read_cpu_times()?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let now = read_cpu_times()?;
    Ok(cpu_percent(&prev, &now))
}

async fn sample() -> Result<HostSample, String> {
    let cpu = sample_cpu_percent().await?;
    let mem = read_mem()?;
    Ok(HostSample {
        cpu_percent: cpu,
        mem_percent: mem_percent(&mem),
        mem_used_kb: mem.total_kb.saturating_sub(mem.available_kb),
        mem_total_kb: mem.total_kb,
    })
}

const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// Continuously samples CPU/memory into the Prometheus gauges.
pub fn spawn_sampler(metrics: Arc<Metrics>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match sample().await {
                Ok(sample) => metrics.record(&sample),
                Err(e) => error!("metrics sampler error: {e}"),
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
}
