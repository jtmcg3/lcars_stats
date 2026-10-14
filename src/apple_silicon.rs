//! Apple Silicon metrics collection using macmon
//!
//! This module provides access to Apple M1/M2/M3 specific metrics including:
//! - GPU usage and power
//! - Neural Engine (ANE) power
//! - CPU cluster breakdown (E-cores vs P-cores)
//! - Power consumption
//! - Temperature readings

#![allow(dead_code)]

use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// How a macmon sample should be shown. Collector process up is not the same as a useful sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacmonSampleState {
    #[default]
    NoData,
    NotInstalled,
    Live,
    Stale,
}

/// Temperature readings from Apple Silicon sensors
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub struct TempMetrics {
    #[serde(default)]
    pub cpu_temp_avg: f64,
    #[serde(default)]
    pub gpu_temp_avg: f64,
}

/// Memory statistics
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub struct MemoryMetrics {
    #[serde(default)]
    pub ram_total: u64,
    #[serde(default)]
    pub ram_usage: u64,
    #[serde(default)]
    pub swap_total: u64,
    #[serde(default)]
    pub swap_usage: u64,
}

/// Raw metrics from macmon JSON output
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub struct MacmonMetrics {
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub temp: TempMetrics,
    #[serde(default)]
    pub memory: MemoryMetrics,
    /// E-cluster CPU usage: [frequency_mhz, usage_ratio]
    #[serde(default)]
    pub ecpu_usage: (f64, f64),
    /// P-cluster CPU usage: [frequency_mhz, usage_ratio]
    #[serde(default)]
    pub pcpu_usage: (f64, f64),
    /// GPU usage: [frequency_mhz, usage_ratio]
    #[serde(default)]
    pub gpu_usage: (f64, f64),
    #[serde(default)]
    pub cpu_power: f64,
    #[serde(default)]
    pub gpu_power: f64,
    #[serde(default)]
    pub ane_power: f64,
    #[serde(default)]
    pub all_power: f64,
    #[serde(default)]
    pub sys_power: f64,
}

impl MacmonMetrics {
    /// True only for a real sensor signal: power, cluster or GPU frequency, usage, or temperature.
    pub fn is_useful(&self) -> bool {
        self.cpu_power != 0.0
            || self.gpu_power != 0.0
            || self.ane_power != 0.0
            || self.all_power != 0.0
            || self.sys_power != 0.0
            || self.ecpu_usage.0 != 0.0
            || self.ecpu_usage.1 != 0.0
            || self.pcpu_usage.0 != 0.0
            || self.pcpu_usage.1 != 0.0
            || self.gpu_usage.0 != 0.0
            || self.gpu_usage.1 != 0.0
            || self.temp.cpu_temp_avg != 0.0
            || self.temp.gpu_temp_avg != 0.0
    }
}

/// Processed Apple Silicon metrics for display
#[derive(Debug, Clone, Default)]
pub struct AppleSiliconStats {
    pub sample_state: MacmonSampleState,
    /// E-cluster (efficiency cores) frequency in MHz
    pub ecpu_freq_mhz: f64,
    /// E-cluster usage as percentage (0-100)
    pub ecpu_usage_percent: f64,
    /// P-cluster (performance cores) frequency in MHz
    pub pcpu_freq_mhz: f64,
    /// P-cluster usage as percentage (0-100)
    pub pcpu_usage_percent: f64,
    /// GPU frequency in MHz
    pub gpu_freq_mhz: f64,
    /// GPU usage as percentage (0-100)
    pub gpu_usage_percent: f64,
    /// CPU power consumption in Watts
    pub cpu_power_w: f64,
    /// GPU power consumption in Watts
    pub gpu_power_w: f64,
    /// Apple Neural Engine power in Watts
    pub ane_power_w: f64,
    /// Total compute power in Watts
    pub total_power_w: f64,
    /// System power in Watts
    pub sys_power_w: f64,
    /// Average CPU temperature in Celsius
    pub cpu_temp_c: f64,
    /// Average GPU temperature in Celsius
    pub gpu_temp_c: f64,
    /// Thermal status string
    pub thermal_status: String,
}

impl AppleSiliconStats {
    /// Create stats from macmon metrics
    pub fn from_macmon(metrics: &MacmonMetrics) -> Self {
        if !metrics.is_useful() {
            return Self::no_data();
        }
        Self::from_useful(metrics, MacmonSampleState::Live)
    }

    fn from_useful(metrics: &MacmonMetrics, sample_state: MacmonSampleState) -> Self {
        let cpu_temp = metrics.temp.cpu_temp_avg;
        let gpu_temp = metrics.temp.gpu_temp_avg;

        let max_temp = cpu_temp.max(gpu_temp);
        let thermal_status = if sample_state == MacmonSampleState::Stale {
            "STALE".to_string()
        } else if max_temp > 95.0 {
            "CRITICAL".to_string()
        } else if max_temp > 85.0 {
            "THROTTLING".to_string()
        } else if max_temp > 70.0 {
            "ELEVATED".to_string()
        } else {
            "NOMINAL".to_string()
        };

        Self {
            sample_state,
            ecpu_freq_mhz: metrics.ecpu_usage.0,
            ecpu_usage_percent: metrics.ecpu_usage.1 * 100.0,
            pcpu_freq_mhz: metrics.pcpu_usage.0,
            pcpu_usage_percent: metrics.pcpu_usage.1 * 100.0,
            gpu_freq_mhz: metrics.gpu_usage.0,
            gpu_usage_percent: metrics.gpu_usage.1 * 100.0,
            cpu_power_w: metrics.cpu_power,
            gpu_power_w: metrics.gpu_power,
            ane_power_w: metrics.ane_power,
            total_power_w: metrics.all_power,
            sys_power_w: metrics.sys_power,
            cpu_temp_c: cpu_temp,
            gpu_temp_c: gpu_temp,
            thermal_status,
        }
    }

    pub fn no_data() -> Self {
        Self {
            sample_state: MacmonSampleState::NoData,
            thermal_status: "NO DATA".to_string(),
            ..Default::default()
        }
    }

    /// Live and stale samples still have macmon numbers. NoData and NotInstalled do not.
    pub fn available(&self) -> bool {
        matches!(
            self.sample_state,
            MacmonSampleState::Live | MacmonSampleState::Stale
        )
    }

    /// Create unavailable stats (when macmon is not installed)
    pub fn unavailable() -> Self {
        Self {
            sample_state: MacmonSampleState::NotInstalled,
            thermal_status: "UNAVAILABLE".to_string(),
            ..Default::default()
        }
    }
}

/// Manages the macmon subprocess for continuous metrics collection
pub struct MacmonCollector {
    receiver: Receiver<MacmonMetrics>,
    _child: Option<Child>,
    latest_useful: Option<MacmonMetrics>,
    last_useful_at: Option<Instant>,
    stale_after: Duration,
}

impl MacmonCollector {
    const STALE_AFTER: Duration = Duration::from_secs(2);

    /// Start a new macmon collector
    ///
    /// Returns None if macmon is not installed
    pub fn new() -> Option<Self> {
        // Check if macmon is available
        let check = Command::new("which")
            .arg("macmon")
            .output()
            .ok()?;

        if !check.status.success() {
            return None;
        }

        // Start macmon pipe with 500ms interval
        let mut child = Command::new("macmon")
            .args(["pipe", "-i", "500"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdout = child.stdout.take()?;
        let (tx, rx): (Sender<MacmonMetrics>, Receiver<MacmonMetrics>) = mpsc::channel();

        // Spawn thread to read macmon output
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(metrics) = serde_json::from_str::<MacmonMetrics>(&line)
                    && tx.send(metrics).is_err()
                {
                    break;
                }
            }
        });

        Some(Self {
            receiver: rx,
            _child: Some(child),
            latest_useful: None,
            last_useful_at: None,
            stale_after: Self::STALE_AFTER,
        })
    }

    /// Drain pending lines and map them to display stats at `now`.
    pub fn stats_at(&mut self, now: Instant) -> AppleSiliconStats {
        while let Ok(metrics) = self.receiver.try_recv() {
            if metrics.is_useful() {
                self.latest_useful = Some(metrics);
                self.last_useful_at = Some(now);
            }
        }

        match (&self.latest_useful, self.last_useful_at) {
            (Some(metrics), Some(seen)) => {
                let age = now.saturating_duration_since(seen);
                if age > self.stale_after {
                    AppleSiliconStats::from_useful(metrics, MacmonSampleState::Stale)
                } else {
                    AppleSiliconStats::from_useful(metrics, MacmonSampleState::Live)
                }
            }
            _ => AppleSiliconStats::no_data(),
        }
    }

    pub fn stats(&mut self) -> AppleSiliconStats {
        self.stats_at(Instant::now())
    }
}

impl Drop for MacmonCollector {
    fn drop(&mut self) {
        if let Some(ref mut child) = self._child {
            let _ = child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSIC_PIPE_JSON: &str = r#"{
        "timestamp": "2026-03-29T20:59:59.337500+00:00",
        "temp": { "cpu_temp_avg": 48.9, "gpu_temp_avg": 44.0 },
        "ecpu_usage": [1902, 0.64],
        "pcpu_usage": [2044, 0.02],
        "gpu_usage": [338, 0.06],
        "cpu_power": 0.89,
        "gpu_power": 0.29,
        "ane_power": 0.0,
        "all_power": 1.19,
        "sys_power": 16.4
    }"#;

    const NESTED_SCHEMA_JSON: &str = r#"{
        "gpu_usage": {"GPUPH": {"units": 20, "freq_mhz": 338, "usage": 0.29}},
        "power": {"cpu": 0.73, "gpu": 0.37, "ane": 0.1},
        "temp": {"cpu_avg": 52.6, "gpu_avg": 49.5}
    }"#;

    fn classic_metrics() -> MacmonMetrics {
        serde_json::from_str(CLASSIC_PIPE_JSON).expect("classic macmon JSON")
    }

    fn test_collector(stale_after: Duration) -> (MacmonCollector, Sender<MacmonMetrics>) {
        let (tx, rx) = mpsc::channel();
        let collector = MacmonCollector {
            receiver: rx,
            _child: None,
            latest_useful: None,
            last_useful_at: None,
            stale_after,
        };
        (collector, tx)
    }

    #[test]
    fn default_metrics_are_not_available() {
        let stats = AppleSiliconStats::from_macmon(&MacmonMetrics::default());
        assert!(!stats.available());
        assert_eq!(stats.sample_state, MacmonSampleState::NoData);
        assert_eq!(stats.thermal_status, "NO DATA");
    }

    #[test]
    fn empty_json_deserializes_to_zeros_and_is_not_available() {
        let metrics: MacmonMetrics =
            serde_json::from_str("{}").expect("empty object should deserialize");
        assert_eq!(metrics.cpu_power, 0.0);
        assert_eq!(metrics.ecpu_usage, (0.0, 0.0));
        let stats = AppleSiliconStats::from_macmon(&metrics);
        assert!(!stats.available());
        assert_eq!(stats.thermal_status, "NO DATA");
    }

    #[test]
    fn collector_seed_default_is_not_live() {
        let stats = AppleSiliconStats::from_macmon(&MacmonMetrics::default());
        assert!(!stats.available());
        assert_eq!(stats.thermal_status, "NO DATA");
    }

    #[test]
    fn classic_pipe_json_is_live_with_sample_values() {
        let stats = AppleSiliconStats::from_macmon(&classic_metrics());
        assert_eq!(stats.sample_state, MacmonSampleState::Live);
        assert_eq!(stats.ecpu_freq_mhz, 1902.0);
        assert!((stats.ecpu_usage_percent - 64.0).abs() < 0.01);
        assert_eq!(stats.pcpu_freq_mhz, 2044.0);
        assert!((stats.pcpu_usage_percent - 2.0).abs() < 0.01);
        assert_eq!(stats.gpu_freq_mhz, 338.0);
        assert!((stats.gpu_usage_percent - 6.0).abs() < 0.01);
        assert!((stats.cpu_power_w - 0.89).abs() < 0.001);
        assert!((stats.gpu_power_w - 0.29).abs() < 0.001);
        assert!((stats.sys_power_w - 16.4).abs() < 0.001);
        assert!((stats.cpu_temp_c - 48.9).abs() < 0.01);
        assert!((stats.gpu_temp_c - 44.0).abs() < 0.01);
        assert_eq!(stats.thermal_status, "NOMINAL");
    }

    #[test]
    fn nested_schema_json_is_no_data_not_nominal() {
        let (mut collector, tx) = test_collector(Duration::from_secs(2));
        if let Ok(metrics) = serde_json::from_str::<MacmonMetrics>(NESTED_SCHEMA_JSON) {
            tx.send(metrics).unwrap();
        }
        let stats = collector.stats_at(Instant::now());
        assert_eq!(stats.sample_state, MacmonSampleState::NoData);
        assert_eq!(stats.thermal_status, "NO DATA");

        let mismatched: MacmonMetrics = serde_json::from_str(
            r#"{"cpu":{"ecpu_usage":[1902,0.64],"power":4.2},"gpu":{"freq_mhz":900,"usage":0.4},"temp":{"cpu_avg":88.0}}"#,
        )
        .unwrap();
        let stats = AppleSiliconStats::from_macmon(&mismatched);
        assert_eq!(stats.sample_state, MacmonSampleState::NoData);
        assert_eq!(stats.thermal_status, "NO DATA");
        assert_eq!(stats.cpu_power_w, 0.0);
        assert_eq!(stats.ecpu_freq_mhz, 0.0);
        assert_eq!(stats.cpu_temp_c, 0.0);
    }

    #[test]
    fn memory_only_json_is_no_data_not_nominal() {
        let metrics: MacmonMetrics = serde_json::from_str(
            r#"{"memory":{"ram_total":17179869184,"ram_usage":8589934592,"swap_total":1,"swap_usage":1}}"#,
        )
        .unwrap();
        assert!(metrics.memory.ram_total > 0);
        let stats = AppleSiliconStats::from_macmon(&metrics);
        assert_eq!(stats.sample_state, MacmonSampleState::NoData);
        assert_eq!(stats.thermal_status, "NO DATA");
        assert_eq!(stats.cpu_power_w, 0.0);
        assert_eq!(stats.cpu_temp_c, 0.0);
    }

    #[test]
    fn hung_collector_stays_no_data() {
        let (mut collector, _tx) = test_collector(Duration::from_secs(2));
        let t0 = Instant::now();
        let stats = collector.stats_at(t0);
        assert_eq!(stats.sample_state, MacmonSampleState::NoData);
        assert!(!stats.available());
        assert_eq!(stats.thermal_status, "NO DATA");
    }

    #[test]
    fn zero_payload_does_not_become_live() {
        let (mut collector, tx) = test_collector(Duration::from_secs(2));
        tx.send(MacmonMetrics::default()).unwrap();
        let stats = collector.stats_at(Instant::now());
        assert_eq!(stats.sample_state, MacmonSampleState::NoData);
        assert_eq!(stats.thermal_status, "NO DATA");
    }

    #[test]
    fn useful_sample_then_silence_becomes_stale() {
        let (mut collector, tx) = test_collector(Duration::from_secs(2));
        tx.send(classic_metrics()).unwrap();
        let t0 = Instant::now();
        let live = collector.stats_at(t0);
        assert_eq!(live.sample_state, MacmonSampleState::Live);
        assert!(live.available());
        assert_eq!(live.thermal_status, "NOMINAL");

        let still_live = collector.stats_at(t0 + Duration::from_secs(2));
        assert_eq!(still_live.sample_state, MacmonSampleState::Live);
        assert_eq!(still_live.thermal_status, "NOMINAL");

        let stale = collector.stats_at(t0 + Duration::from_secs(2) + Duration::from_millis(1));
        assert_eq!(stale.sample_state, MacmonSampleState::Stale);
        assert!(stale.available());
        assert_eq!(stale.thermal_status, "STALE");
        assert_eq!(stale.ecpu_freq_mhz, 1902.0);
        assert!((stale.ecpu_usage_percent - 64.0).abs() < 0.01);
        assert!((stale.cpu_power_w - 0.89).abs() < 0.001);
        assert!((stale.sys_power_w - 16.4).abs() < 0.001);
        assert!((stale.cpu_temp_c - 48.9).abs() < 0.01);
        assert!((stale.gpu_temp_c - 44.0).abs() < 0.01);
    }

    #[test]
    fn empty_sample_does_not_refresh_a_stale_reading() {
        let (mut collector, tx) = test_collector(Duration::from_secs(2));
        tx.send(classic_metrics()).unwrap();
        let t0 = Instant::now();
        assert_eq!(
            collector.stats_at(t0).sample_state,
            MacmonSampleState::Live
        );
        tx.send(MacmonMetrics::default()).unwrap();
        let stale = collector.stats_at(t0 + Duration::from_secs(3));
        assert_eq!(stale.sample_state, MacmonSampleState::Stale);
        assert_eq!(stale.thermal_status, "STALE");
        assert_eq!(stale.ecpu_freq_mhz, 1902.0);
        assert!((stale.cpu_power_w - 0.89).abs() < 0.001);
    }
}
