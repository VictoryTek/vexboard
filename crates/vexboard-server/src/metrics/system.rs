use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;

/// A snapshot of system-level metrics.
#[derive(Debug, Clone, Serialize)]
pub struct SystemSnapshot {
    pub cpu_percent: f64,
    pub memory_total_kb: u64,
    pub memory_used_kb: u64,
    pub memory_percent: f64,
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub disk_free_kb: u64,
    pub disk_total_kb: u64,
}

/// Read a single snapshot of current system metrics from /proc.
pub async fn read_snapshot() -> anyhow::Result<SystemSnapshot> {
    let cpu = read_cpu_percent().await?;
    let (mem_total, mem_used) = read_memory().await?;
    let (net_rx, net_tx) = read_network().await?;
    let (disk_r, disk_w) = read_disk().await?;
    let (disk_free, disk_total) = read_disk_space();

    let mem_percent = if mem_total > 0 {
        (mem_used as f64 / mem_total as f64) * 100.0
    } else {
        0.0
    };

    Ok(SystemSnapshot {
        cpu_percent: cpu,
        memory_total_kb: mem_total,
        memory_used_kb: mem_used,
        memory_percent: mem_percent,
        net_rx_bytes: net_rx,
        net_tx_bytes: net_tx,
        disk_read_bytes: disk_r,
        disk_write_bytes: disk_w,
        disk_free_kb: disk_free,
        disk_total_kb: disk_total,
    })
}

/// Read available and total disk space for the root filesystem via statvfs.
#[cfg(unix)]
fn read_disk_space() -> (u64, u64) {
    use std::ffi::CString;
    let Ok(path) = CString::new("/") else {
        return (0, 0);
    };
    // SAFETY: path is a valid C string, stat is fully initialised via zeroed().
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if ret != 0 {
        return (0, 0);
    }
    let block_size = if stat.f_frsize > 0 {
        stat.f_frsize as u64
    } else {
        stat.f_bsize as u64
    };
    let free_kb = (stat.f_bavail as u64).saturating_mul(block_size) / 1024;
    let total_kb = (stat.f_blocks as u64).saturating_mul(block_size) / 1024;
    (free_kb, total_kb)
}

#[cfg(not(unix))]
fn read_disk_space() -> (u64, u64) {
    (0, 0)
}

/// Background loop that reads system metrics and broadcasts them at the configured interval.
#[tracing::instrument(skip_all)]
pub async fn metrics_loop(tx: broadcast::Sender<SystemSnapshot>, interval_ms: u64) {
    let interval = Duration::from_millis(interval_ms);
    tracing::info!(?interval, "Starting system metrics broadcast loop");

    loop {
        match read_snapshot().await {
            Ok(snapshot) => {
                let _ = tx.send(snapshot);
            }
            Err(e) => {
                tracing::warn!("Failed to read system metrics: {e}");
            }
        }
        tokio::time::sleep(interval).await;
    }
}

/// Read CPU utilization by sampling /proc/stat twice with a 1-second delay.
async fn read_cpu_percent() -> anyhow::Result<f64> {
    let stat1 = tokio::fs::read_to_string("/proc/stat").await?;
    let (idle1, total1) = parse_cpu_line(&stat1)?;

    tokio::time::sleep(Duration::from_millis(250)).await;

    let stat2 = tokio::fs::read_to_string("/proc/stat").await?;
    let (idle2, total2) = parse_cpu_line(&stat2)?;

    let idle_delta = idle2.saturating_sub(idle1) as f64;
    let total_delta = total2.saturating_sub(total1) as f64;

    if total_delta == 0.0 {
        return Ok(0.0);
    }

    Ok(((total_delta - idle_delta) / total_delta) * 100.0)
}

fn parse_cpu_line(stat: &str) -> anyhow::Result<(u64, u64)> {
    let line = stat
        .lines()
        .find(|l| l.starts_with("cpu "))
        .ok_or_else(|| anyhow::anyhow!("No cpu line in /proc/stat"))?;

    let parts: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();

    if parts.len() < 4 {
        return Err(anyhow::anyhow!("Unexpected /proc/stat format"));
    }

    let idle = parts[3]; // idle is the 4th field
    let total: u64 = parts.iter().sum();

    Ok((idle, total))
}

/// Read memory info from /proc/meminfo.
async fn read_memory() -> anyhow::Result<(u64, u64)> {
    let content = tokio::fs::read_to_string("/proc/meminfo").await?;

    let mut total = 0u64;
    let mut available = 0u64;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = parse_meminfo_value(line)?;
        } else if line.starts_with("MemAvailable:") {
            available = parse_meminfo_value(line)?;
        }
    }

    let used = total.saturating_sub(available);
    Ok((total, used))
}

fn parse_meminfo_value(line: &str) -> anyhow::Result<u64> {
    let val = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Invalid meminfo line: {line}"))?
        .parse::<u64>()?;
    Ok(val)
}

/// Read network I/O totals from /proc/net/dev.
async fn read_network() -> anyhow::Result<(u64, u64)> {
    let content = tokio::fs::read_to_string("/proc/net/dev").await?;
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;

    for line in content.lines().skip(2) {
        // Skip header lines
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            continue;
        }
        // Skip loopback
        if parts[0].starts_with("lo:") || parts[0] == "lo:" {
            continue;
        }
        if let (Ok(rx), Ok(tx)) = (parts[1].parse::<u64>(), parts[9].parse::<u64>()) {
            rx_total += rx;
            tx_total += tx;
        }
    }

    Ok((rx_total, tx_total))
}

/// Read disk I/O from /proc/diskstats.
async fn read_disk() -> anyhow::Result<(u64, u64)> {
    let content = tokio::fs::read_to_string("/proc/diskstats").await?;
    let mut read_sectors = 0u64;
    let mut write_sectors = 0u64;

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 14 {
            continue;
        }
        // Only count whole disk devices (e.g., sda, nvme0n1), skip partitions
        let name = parts[2];
        if name.chars().last().is_some_and(|c| c.is_ascii_digit())
            && !name.contains("nvme")
            && !name.starts_with("sd")
        {
            continue;
        }
        // For sd* and nvme* devices without partition suffix
        let is_whole_disk = (name.starts_with("sd") && name.len() == 3)
            || (name.contains("nvme") && name.ends_with("n1") && !name.contains('p'));

        if !is_whole_disk {
            continue;
        }

        if let (Ok(r), Ok(w)) = (parts[5].parse::<u64>(), parts[9].parse::<u64>()) {
            read_sectors += r;
            write_sectors += w;
        }
    }

    // Sectors are typically 512 bytes
    Ok((read_sectors * 512, write_sectors * 512))
}
