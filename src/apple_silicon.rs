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

impl AppleSiliconStats {
    /// Create stats from macmon metrics
    pub fn from_macmon(metrics: &MacmonMetrics) -> Self {
        let cpu_temp = metrics.temp.cpu_temp_avg;
        let gpu_temp = metrics.temp.gpu_temp_avg;

        // Determine thermal status
        let max_temp = cpu_temp.max(gpu_temp);
        let thermal_status = if max_temp > 95.0 {
            "CRITICAL".to_string()
        } else if max_temp > 85.0 {
            "THROTTLING".to_string()
        } else if max_temp > 70.0 {
            "ELEVATED".to_string()
        } else {
            "NOMINAL".to_string()
        };

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
