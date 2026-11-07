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

/// Processed Apple Silicon metrics for display
#[derive(Debug, Clone, Default)]
pub struct AppleSiliconStats {
    /// Whether macmon is available
    pub available: bool,
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

/// Parse one `macmon pipe` JSON line.
pub(crate) fn parse_macmon_line(line: &str) -> Result<MacmonMetrics, serde_json::Error> {
    serde_json::from_str(line)
}

/// Thermal band from the hotter of CPU/GPU average temps.
pub(crate) fn thermal_status(max_temp_c: f64) -> String {
    if max_temp_c > 95.0 {
        "CRITICAL".to_string()
    } else if max_temp_c > 85.0 {
        "THROTTLING".to_string()
    } else if max_temp_c > 70.0 {
        "ELEVATED".to_string()
    } else {
        "NOMINAL".to_string()
    }
}

impl AppleSiliconStats {
    /// Create stats from macmon metrics
    pub fn from_macmon(metrics: &MacmonMetrics) -> Self {
        let cpu_temp = metrics.temp.cpu_temp_avg;
        let gpu_temp = metrics.temp.gpu_temp_avg;
        let thermal_status = thermal_status(cpu_temp.max(gpu_temp));

        Self {
            available: true,
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

    /// Create unavailable stats (when macmon is not installed)
    pub fn unavailable() -> Self {
        Self {
            available: false,
            thermal_status: "UNAVAILABLE".to_string(),
            ..Default::default()
        }
    }
}

/// Manages the macmon subprocess for continuous metrics collection
pub struct MacmonCollector {
    receiver: Receiver<MacmonMetrics>,
    _child: Option<Child>,
    latest: MacmonMetrics,
}

impl MacmonCollector {
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
                if let Ok(metrics) = parse_macmon_line(&line)
                    && tx.send(metrics).is_err()
                {
                    break;
                }
            }
        });

        Some(Self {
            receiver: rx,
            _child: Some(child),
            latest: MacmonMetrics::default(),
        })
    }

    /// Get the latest metrics, updating from the collector
    pub fn get_latest(&mut self) -> &MacmonMetrics {
        // Drain all pending messages, keep the latest
        while let Ok(metrics) = self.receiver.try_recv() {
            self.latest = metrics;
        }
        &self.latest
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

    /// Sample from macmon 0.7 docs (`ecpu_usage` / `pcpu_usage` / `gpu_usage` tuples).
    const LEGACY_PIPE_JSON: &str = r#"{
        "timestamp": "2025-02-24T20:38:15.427569+00:00",
        "temp": {"cpu_temp_avg": 43.73614, "gpu_temp_avg": 36.95167},
        "memory": {
            "ram_total": 25769803776,
            "ram_usage": 20985479168,
            "swap_total": 4294967296,
            "swap_usage": 2602434560
        },
        "ecpu_usage": [1181, 0.082656614],
        "pcpu_usage": [1974, 0.015181795],
        "gpu_usage": [461, 0.021497859],
        "cpu_power": 0.20486385,
        "gpu_power": 0.017451683,
        "ane_power": 0.0,
        "all_power": 0.22231553,
        "sys_power": 5.876533
    }"#;

    /// Current macmon pipe schema: `cpu_tiers` / `ecpu_freq_mhz`, no `*_usage` tuples.
    const CURRENT_PIPE_JSON: &str = r#"{
        "timestamp": "2026-03-29T20:59:59.337500+00:00",
        "temp": {"cpu_temp_avg": 48.95563888549805, "gpu_temp_avg": 44.01784133911133},
        "cpu_tiers": [{"label": "E", "freq_mhz": 1100, "scaled_ratio": 0.082656614, "active_ratio": 0.18}],
        "ecpu_freq_mhz": 1100,
        "ecpu_scaled_ratio": 0.082656614,
        "pcpu_freq_mhz": 1800,
        "pcpu_scaled_ratio": 0.015181795,
        "gpu_freq_mhz": 461,
        "gpu_scaled_ratio": 0.021497859,
        "cpu_power": 0.20486385,
        "gpu_power": 0.017451683,
        "ane_power": 0.0,
        "all_power": 0.22231553,
        "sys_power": 5.876533
    }"#;

    #[test]
    fn parse_legacy_macmon_pipe_json() {
        let metrics = parse_macmon_line(LEGACY_PIPE_JSON).expect("legacy pipe JSON should parse");
        assert_eq!(metrics.ecpu_usage, (1181.0, 0.082656614));
        assert_eq!(metrics.pcpu_usage, (1974.0, 0.015181795));
        assert_eq!(metrics.gpu_usage, (461.0, 0.021497859));
        assert!((metrics.temp.cpu_temp_avg - 43.73614).abs() < 1e-5);
        assert!((metrics.cpu_power - 0.20486385).abs() < 1e-9);
    }

    #[test]
    fn parse_empty_object_as_all_zero_defaults() {
        let metrics = parse_macmon_line("{}").expect("empty object uses serde defaults");
        assert_eq!(metrics.ecpu_usage, (0.0, 0.0));
        assert_eq!(metrics.pcpu_usage, (0.0, 0.0));
        assert_eq!(metrics.gpu_usage, (0.0, 0.0));
        assert_eq!(metrics.cpu_power, 0.0);
        assert_eq!(metrics.temp.cpu_temp_avg, 0.0);
        assert_eq!(metrics.temp.gpu_temp_avg, 0.0);
    }

    #[test]
    fn current_macmon_schema_drops_cluster_usage_to_zero() {
        // Unknown fields are ignored; missing `ecpu_usage` becomes Default.
        let metrics = parse_macmon_line(CURRENT_PIPE_JSON).expect("current schema still deserializes");
        assert_eq!(
            metrics.ecpu_usage,
            (0.0, 0.0),
            "current macmon JSON has no ecpu_usage tuple"
        );
        assert_eq!(metrics.pcpu_usage, (0.0, 0.0));
        assert_eq!(metrics.gpu_usage, (0.0, 0.0));
        // Power/temp keys still match, so the sample looks "valid" except clusters.
        assert!((metrics.cpu_power - 0.20486385).abs() < 1e-9);
        assert!(metrics.temp.cpu_temp_avg > 0.0);
    }

    #[test]
    fn null_gpu_temp_rejects_the_line() {
        // macmon documents gpu_temp_avg as null when no sensor has a valid reading.
        // #[serde(default)] does not treat JSON null as missing, so the line is dropped.
        let json = r#"{"temp":{"cpu_temp_avg":40.0,"gpu_temp_avg":null},"ecpu_usage":[1000,0.1]}"#;
        assert!(parse_macmon_line(json).is_err());
    }

    #[test]
    fn from_macmon_scales_usage_ratio_to_percent() {
        let metrics = parse_macmon_line(LEGACY_PIPE_JSON).unwrap();
        let stats = AppleSiliconStats::from_macmon(&metrics);
        assert!((stats.ecpu_usage_percent - 8.2656614).abs() < 1e-6);
        assert!((stats.pcpu_usage_percent - 1.5181795).abs() < 1e-6);
        assert!((stats.gpu_usage_percent - 2.1497859).abs() < 1e-6);
        assert_eq!(stats.ecpu_freq_mhz, 1181.0);
        assert_eq!(stats.thermal_status, "NOMINAL");
        assert!(stats.available);
    }

    #[test]
    fn thermal_status_thresholds_are_exclusive() {
        assert_eq!(thermal_status(70.0), "NOMINAL");
        assert_eq!(thermal_status(70.1), "ELEVATED");
        assert_eq!(thermal_status(85.0), "ELEVATED");
        assert_eq!(thermal_status(85.1), "THROTTLING");
        assert_eq!(thermal_status(95.0), "THROTTLING");
        assert_eq!(thermal_status(95.1), "CRITICAL");
    }

    #[test]
    fn from_macmon_uses_hotter_of_cpu_and_gpu() {
        let json = r#"{"temp":{"cpu_temp_avg":60.0,"gpu_temp_avg":90.0}}"#;
        let stats = AppleSiliconStats::from_macmon(&parse_macmon_line(json).unwrap());
        assert_eq!(stats.thermal_status, "THROTTLING");
    }

    #[test]
    fn collector_default_metrics_look_like_nominal_idle() {
        // MacmonCollector::get_latest starts at MacmonMetrics::default() until a line arrives.
        let stats = AppleSiliconStats::from_macmon(&MacmonMetrics::default());
        assert_eq!(stats.thermal_status, "NOMINAL");
        assert_eq!(stats.ecpu_usage_percent, 0.0);
        assert_eq!(stats.cpu_power_w, 0.0);
        assert!(
            stats.available,
            "current code marks any deserialized sample available, including zeros"
        );
    }

    // Intended behavior from issue #6: empty/zero/schema-mismatch must not look live.
    #[ignore = "issue #6: from_macmon always sets available=true, even for empty/zero JSON"]
    #[test]
    fn empty_or_zero_metrics_should_not_be_marked_available() {
        let empty = AppleSiliconStats::from_macmon(&parse_macmon_line("{}").unwrap());
        assert!(
            !empty.available,
            "empty JSON is indistinguishable from a real idle sample"
        );

        let schema_mismatch =
            AppleSiliconStats::from_macmon(&parse_macmon_line(CURRENT_PIPE_JSON).unwrap());
        assert!(
            !schema_mismatch.available
                || schema_mismatch.ecpu_freq_mhz > 0.0
                || schema_mismatch.ecpu_usage_percent > 0.0,
            "current macmon schema has no *_usage tuples; should not present fake 0% clusters as live"
        );
    }
}
