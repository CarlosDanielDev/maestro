use std::time::Instant;

/// A snapshot of current process resource usage.
#[derive(Debug, Clone, Copy)]
pub struct ResourceSnapshot {
    /// Process RSS memory in bytes.
    pub rss_bytes: u64,
    /// Total system memory in bytes.
    pub total_memory_bytes: u64,
    /// Optional TurboQuant baseline RSS (before compression).
    pub tq_baseline_bytes: Option<u64>,
}

impl ResourceSnapshot {
    /// RSS in megabytes.
    pub fn rss_mb(&self) -> f64 {
        self.rss_bytes as f64 / (1024.0 * 1024.0)
    }

    /// RSS as percentage of total system memory.
    pub fn memory_pct(&self) -> f64 {
        if self.total_memory_bytes == 0 {
            return 0.0;
        }
        (self.rss_bytes as f64 / self.total_memory_bytes as f64) * 100.0
    }

    /// TurboQuant delta as percentage reduction, if baseline is set.
    pub fn tq_delta_pct(&self) -> Option<f64> {
        self.tq_baseline_bytes.map(|baseline| {
            if baseline == 0 {
                return 0.0;
            }
            let saved = baseline.saturating_sub(self.rss_bytes);
            (saved as f64 / baseline as f64) * 100.0
        })
    }
}

/// Trait for sampling system resources. Mockable for testing.
pub trait ResourceMonitor: Send + Sync {
    /// Take a snapshot of current resource usage.
    fn snapshot(&self) -> ResourceSnapshot;
}

/// Production implementation using the `sysinfo` crate.
/// Caches samples to avoid calling OS every TUI frame.
pub struct SysInfoMonitor {
    pid: sysinfo::Pid,
    total_memory: u64,
    cache: std::sync::Mutex<CachedSnapshot>,
    staleness_ms: u64,
    tq_baseline: std::sync::Mutex<Option<u64>>,
}

struct CachedSnapshot {
    snapshot: ResourceSnapshot,
    sampled_at: Instant,
}

impl SysInfoMonitor {
    /// Create a new monitor for the current process.
    /// `staleness_ms` controls how often the OS is actually sampled (default 1000ms).
    pub fn new(staleness_ms: u64) -> Self {
        let pid = sysinfo::Pid::from_u32(std::process::id());
        let sys = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_memory(sysinfo::MemoryRefreshKind::everything()),
        );
        let total_memory = sys.total_memory();

        let initial = ResourceSnapshot {
            rss_bytes: 0,
            total_memory_bytes: total_memory,
            tq_baseline_bytes: None,
        };

        Self {
            pid,
            total_memory,
            cache: std::sync::Mutex::new(CachedSnapshot {
                snapshot: initial,
                sampled_at: Instant::now()
                    .checked_sub(std::time::Duration::from_secs(60))
                    .unwrap_or_else(Instant::now),
            }),
            staleness_ms,
            tq_baseline: std::sync::Mutex::new(None),
        }
    }

    /// Set the TurboQuant baseline RSS for delta display.
    #[allow(dead_code)] // Reason: wired when TurboQuant adapter lands compression metrics
    pub fn set_tq_baseline(&self, baseline_bytes: u64) {
        *self.tq_baseline.lock().unwrap() = Some(baseline_bytes);
    }

    /// Clear the TurboQuant baseline (when TQ is disabled).
    #[allow(dead_code)] // Reason: wired when TurboQuant toggle clears baseline
    pub fn clear_tq_baseline(&self) {
        *self.tq_baseline.lock().unwrap() = None;
    }
}

impl ResourceMonitor for SysInfoMonitor {
    fn snapshot(&self) -> ResourceSnapshot {
        // Check staleness without holding the lock during syscall
        let needs_refresh = {
            let cache = self.cache.lock().unwrap();
            cache.sampled_at.elapsed().as_millis() as u64 >= self.staleness_ms
        };

        if needs_refresh {
            // Syscall outside lock to avoid stalling the render thread
            let mut sys = sysinfo::System::new();
            sys.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&[self.pid]),
                true,
                sysinfo::ProcessRefreshKind::nothing().with_memory(),
            );
            let rss = sys.process(self.pid).map(|p| p.memory()).unwrap_or(0);
            let tq_baseline = *self.tq_baseline.lock().unwrap();

            let mut cache = self.cache.lock().unwrap();
            cache.snapshot = ResourceSnapshot {
                rss_bytes: rss,
                total_memory_bytes: self.total_memory,
                tq_baseline_bytes: tq_baseline,
            };
            cache.sampled_at = Instant::now();
        }

        self.cache.lock().unwrap().snapshot
    }
}

/// Mock implementation for tests.
#[allow(dead_code)] // Reason: used in tests and future screen tests
pub struct MockResourceMonitor {
    snapshot: ResourceSnapshot,
}

impl MockResourceMonitor {
    #[allow(dead_code)] // Reason: used in tests and future integration tests
    pub fn new(rss_bytes: u64, total_memory_bytes: u64) -> Self {
        Self {
            snapshot: ResourceSnapshot {
                rss_bytes,
                total_memory_bytes,
                tq_baseline_bytes: None,
            },
        }
    }

    #[allow(dead_code)] // Reason: used in tests and future integration tests
    pub fn with_tq_baseline(mut self, baseline: u64) -> Self {
        self.snapshot.tq_baseline_bytes = Some(baseline);
        self
    }
}

impl ResourceMonitor for MockResourceMonitor {
    fn snapshot(&self) -> ResourceSnapshot {
        self.snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ResourceSnapshot --

    #[test]
    fn snapshot_rss_mb_converts_correctly() {
        let snap = ResourceSnapshot {
            rss_bytes: 150 * 1024 * 1024, // 150 MB
            total_memory_bytes: 16 * 1024 * 1024 * 1024,
            tq_baseline_bytes: None,
        };
        assert!((snap.rss_mb() - 150.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_memory_pct_calculates_correctly() {
        let snap = ResourceSnapshot {
            rss_bytes: 8 * 1024 * 1024 * 1024,           // 8 GB
            total_memory_bytes: 16 * 1024 * 1024 * 1024, // 16 GB
            tq_baseline_bytes: None,
        };
        assert!((snap.memory_pct() - 50.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_memory_pct_zero_total_returns_zero() {
        let snap = ResourceSnapshot {
            rss_bytes: 100,
            total_memory_bytes: 0,
            tq_baseline_bytes: None,
        };
        assert_eq!(snap.memory_pct(), 0.0);
    }

    #[test]
    fn snapshot_tq_delta_pct_none_when_no_baseline() {
        let snap = ResourceSnapshot {
            rss_bytes: 100,
            total_memory_bytes: 1000,
            tq_baseline_bytes: None,
        };
        assert!(snap.tq_delta_pct().is_none());
    }

    #[test]
    fn snapshot_tq_delta_pct_calculates_reduction() {
        let snap = ResourceSnapshot {
            rss_bytes: 62 * 1024 * 1024, // 62 MB (after TQ)
            total_memory_bytes: 16 * 1024 * 1024 * 1024,
            tq_baseline_bytes: Some(100 * 1024 * 1024), // 100 MB baseline
        };
        let delta = snap.tq_delta_pct().unwrap();
        assert!((delta - 38.0).abs() < 0.1);
    }

    #[test]
    fn snapshot_tq_delta_pct_zero_baseline_returns_zero() {
        let snap = ResourceSnapshot {
            rss_bytes: 100,
            total_memory_bytes: 1000,
            tq_baseline_bytes: Some(0),
        };
        assert_eq!(snap.tq_delta_pct().unwrap(), 0.0);
    }

    // -- MockResourceMonitor --

    #[test]
    fn mock_monitor_returns_configured_snapshot() {
        let monitor = MockResourceMonitor::new(100 * 1024 * 1024, 16 * 1024 * 1024 * 1024);
        let snap = monitor.snapshot();
        assert_eq!(snap.rss_bytes, 100 * 1024 * 1024);
        assert_eq!(snap.total_memory_bytes, 16 * 1024 * 1024 * 1024);
        assert!(snap.tq_baseline_bytes.is_none());
    }

    #[test]
    fn mock_monitor_with_tq_baseline() {
        let monitor = MockResourceMonitor::new(62 * 1024 * 1024, 16 * 1024 * 1024 * 1024)
            .with_tq_baseline(100 * 1024 * 1024);
        let snap = monitor.snapshot();
        assert_eq!(snap.tq_baseline_bytes, Some(100 * 1024 * 1024));
    }

    // -- SysInfoMonitor integration --

    #[test]
    fn sysinfo_monitor_returns_sane_values() {
        let monitor = SysInfoMonitor::new(0); // no caching for test
        let snap = monitor.snapshot();
        // Process must be using some memory
        assert!(snap.rss_bytes > 0, "RSS must be > 0");
        // Total system memory must be > 0
        assert!(snap.total_memory_bytes > 0, "total memory must be > 0");
        // RSS must be less than total
        assert!(
            snap.rss_bytes < snap.total_memory_bytes,
            "RSS ({}) must be < total ({})",
            snap.rss_bytes,
            snap.total_memory_bytes
        );
    }

    #[test]
    fn sysinfo_monitor_caches_samples() {
        let monitor = SysInfoMonitor::new(60_000); // 60s cache
        let snap1 = monitor.snapshot();
        let snap2 = monitor.snapshot();
        // Second call should return cached value (same rss)
        assert_eq!(snap1.rss_bytes, snap2.rss_bytes);
    }

    #[test]
    fn sysinfo_monitor_tq_baseline_propagates() {
        let monitor = SysInfoMonitor::new(0);
        monitor.set_tq_baseline(200 * 1024 * 1024);
        let snap = monitor.snapshot();
        assert_eq!(snap.tq_baseline_bytes, Some(200 * 1024 * 1024));

        monitor.clear_tq_baseline();
        let snap2 = monitor.snapshot();
        assert!(snap2.tq_baseline_bytes.is_none());
    }
}
