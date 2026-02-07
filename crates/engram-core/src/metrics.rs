//! Metrics and observability for Engram daemon.
//!
//! Provides request tracking, latency measurement, and memory monitoring.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Atomic metrics for daemon performance tracking.
pub struct Metrics {
    /// Total number of requests processed
    pub requests_total: AtomicU64,
    /// Sum of all request latencies in microseconds
    pub requests_latency_us: AtomicU64,
    /// Number of cache hits
    pub cache_hits: AtomicU64,
    /// Number of cache misses
    pub cache_misses: AtomicU64,
    /// Number of projects currently loaded
    pub projects_loaded: AtomicU64,
    /// Current memory usage in bytes (approximate)
    pub memory_bytes: AtomicUsize,
    /// Daemon start time
    start_time: Instant,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// Create a new metrics instance.
    pub fn new() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            requests_latency_us: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            projects_loaded: AtomicU64::new(0),
            memory_bytes: AtomicUsize::new(0),
            start_time: Instant::now(),
        }
    }

    /// Record a completed request.
    pub fn record_request(&self, latency: Duration) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_latency_us
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
    }

    /// Record a cache hit.
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss.
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Get uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get cache hit rate (0.0 - 1.0).
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get average request latency.
    pub fn avg_latency(&self) -> Duration {
        let total = self.requests_total.load(Ordering::Relaxed);
        let latency_us = self.requests_latency_us.load(Ordering::Relaxed);
        if total == 0 {
            Duration::ZERO
        } else {
            Duration::from_micros(latency_us / total)
        }
    }
}

/// Tracks latency samples for percentile calculation.
pub struct LatencyTracker {
    samples: RwLock<VecDeque<(String, Duration)>>,
    max_samples: usize,
}

impl Default for LatencyTracker {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl LatencyTracker {
    /// Create a new latency tracker with given capacity.
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: RwLock::new(VecDeque::with_capacity(max_samples)),
            max_samples,
        }
    }

    /// Record a latency sample for an operation.
    pub fn record(&self, operation: &str, duration: Duration) {
        let mut samples = self.samples.write().unwrap();
        samples.push_back((operation.to_string(), duration));

        while samples.len() > self.max_samples {
            samples.pop_front();
        }
    }

    /// Get P50 latency for an operation.
    pub fn p50(&self, operation: &str) -> Duration {
        self.percentile(operation, 0.50)
    }

    /// Get P99 latency for an operation.
    pub fn p99(&self, operation: &str) -> Duration {
        self.percentile(operation, 0.99)
    }

    /// Get specific percentile for an operation.
    pub fn percentile(&self, operation: &str, p: f64) -> Duration {
        let samples = self.samples.read().unwrap();
        let mut durations: Vec<_> = samples
            .iter()
            .filter(|(op, _)| op == operation)
            .map(|(_, d)| *d)
            .collect();

        if durations.is_empty() {
            return Duration::ZERO;
        }

        durations.sort();
        let idx = ((durations.len() as f64 * p) as usize).min(durations.len() - 1);
        durations[idx]
    }

    /// Get sample count for an operation.
    pub fn sample_count(&self, operation: &str) -> usize {
        let samples = self.samples.read().unwrap();
        samples.iter().filter(|(op, _)| op == operation).count()
    }
}

/// Memory pressure levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    /// Normal operation (<70% of limit)
    Normal,
    /// Warning level (70-90% of limit)
    Warning,
    /// Critical level (>90% of limit)
    Critical,
}

/// Monitors memory usage and pressure.
pub struct MemoryMonitor {
    /// Memory limit in bytes
    limit: usize,
    /// Current usage (tracked externally)
    current: AtomicUsize,
}

impl MemoryMonitor {
    /// Create a new memory monitor with given limit.
    pub fn new(limit_bytes: usize) -> Self {
        Self {
            limit: limit_bytes,
            current: AtomicUsize::new(0),
        }
    }

    /// Create a monitor with 100MB limit.
    pub fn default_limit() -> Self {
        Self::new(100 * 1024 * 1024)
    }

    /// Update current memory usage.
    pub fn update(&self, bytes: usize) {
        self.current.store(bytes, Ordering::Relaxed);
    }

    /// Add to current memory usage.
    pub fn add(&self, bytes: usize) {
        self.current.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Subtract from current memory usage.
    pub fn sub(&self, bytes: usize) {
        self.current.fetch_sub(bytes, Ordering::Relaxed);
    }

    /// Get current usage in bytes.
    pub fn current(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    /// Get memory limit in bytes.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Get usage ratio (0.0 - 1.0+).
    pub fn usage_ratio(&self) -> f64 {
        self.current() as f64 / self.limit as f64
    }

    /// Check current memory pressure level.
    pub fn check_pressure(&self) -> MemoryPressure {
        let ratio = self.usage_ratio();
        match ratio {
            r if r < 0.7 => MemoryPressure::Normal,
            r if r < 0.9 => MemoryPressure::Warning,
            _ => MemoryPressure::Critical,
        }
    }

    /// Check if eviction is needed.
    pub fn should_evict(&self) -> bool {
        matches!(
            self.check_pressure(),
            MemoryPressure::Warning | MemoryPressure::Critical
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_request_recording() {
        let metrics = Metrics::new();
        metrics.record_request(Duration::from_millis(10));
        metrics.record_request(Duration::from_millis(20));

        assert_eq!(metrics.requests_total.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.avg_latency(), Duration::from_millis(15));
    }

    #[test]
    fn test_metrics_cache_hit_rate() {
        let metrics = Metrics::new();
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        assert!((metrics.cache_hit_rate() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_latency_tracker_percentiles() {
        let tracker = LatencyTracker::new(100);

        for i in 1..=100 {
            tracker.record("test", Duration::from_millis(i));
        }

        // With 100 samples (1-100ms), idx=50 gives value at position 50 = 51ms
        // idx=99 gives value at position 99 = 100ms
        let p50 = tracker.p50("test");
        let p99 = tracker.p99("test");

        // P50 should be around median (50-51ms)
        assert!(p50 >= Duration::from_millis(50) && p50 <= Duration::from_millis(51));
        // P99 should be near top (99-100ms)
        assert!(p99 >= Duration::from_millis(99) && p99 <= Duration::from_millis(100));
    }

    #[test]
    fn test_latency_tracker_empty() {
        let tracker = LatencyTracker::new(100);
        assert_eq!(tracker.p99("nonexistent"), Duration::ZERO);
    }

    #[test]
    fn test_memory_monitor_pressure() {
        let monitor = MemoryMonitor::new(100);

        monitor.update(50);
        assert_eq!(monitor.check_pressure(), MemoryPressure::Normal);

        monitor.update(75);
        assert_eq!(monitor.check_pressure(), MemoryPressure::Warning);

        monitor.update(95);
        assert_eq!(monitor.check_pressure(), MemoryPressure::Critical);
    }

    #[test]
    fn test_memory_monitor_add_sub() {
        let monitor = MemoryMonitor::new(100);

        monitor.add(30);
        monitor.add(20);
        assert_eq!(monitor.current(), 50);

        monitor.sub(10);
        assert_eq!(monitor.current(), 40);
    }
}
