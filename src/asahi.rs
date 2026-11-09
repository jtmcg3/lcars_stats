//! Apple Silicon metrics on Linux (Asahi), read straight from sysfs.
//!
//! - CPU clusters: cpufreq policies. The policy with the lowest max frequency is
//!   the E-cluster, the highest is the P-cluster (M1: policy0 = cpu0-3 at 2.06 GHz,
//!   policy4 = cpu4-7 at 3.2 GHz). Usage comes from the caller's per-CPU numbers.
//! - Power: `macsmc_hwmon` exposes "Total System Power" and "Heatpipe Power" (SoC).
//! - Temperatures: every `temp*_label` on `macsmc_hwmon` plus the NVMe composite.
//! - Battery: `power_supply/macsmc-battery/{power_now,status}`.
//!
//! The Asahi GPU driver exposes no utilisation/frequency counters and there is no
//! Neural Engine driver, so those stay unavailable rather than showing zeros.

use crate::apple_silicon::{thermal_status, AppleSiliconStats, Source};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct Cluster {
    cpus: Vec<usize>,
    cur_freq_file: PathBuf,
}

#[derive(Debug, Clone)]
struct Sensor {
    label: String,
    input: PathBuf,
}

/// Reads Asahi sysfs metrics. Paths are discovered once; values are read per sample.
#[derive(Debug, Clone)]
pub struct AsahiCollector {
    e_cluster: Cluster,
    p_cluster: Cluster,
    sys_power: Option<PathBuf>,
    soc_power: Option<PathBuf>,
    temps: Vec<Sensor>,
    battery: Option<PathBuf>,
}

impl AsahiCollector {
    /// Detect on the live system.
    pub fn detect() -> Option<Self> {
        Self::detect_at(Path::new("/sys"))
    }

    /// Detect under a sysfs root. Needs two CPU clusters with different max
    /// frequencies and the Apple SMC hardware monitor.
    pub fn detect_at(sys: &Path) -> Option<Self> {
        let (e_cluster, p_cluster) = find_clusters(sys)?;
        let smc = find_hwmon(sys, "macsmc_hwmon")?;

        let mut sys_power = None;
        let mut soc_power = None;
        let mut temps = Vec::new();
        for (kind, label, input) in labelled_inputs(&smc) {
            match (kind.as_str(), label.as_str()) {
                ("power", "Total System Power") => sys_power = Some(input),
                ("power", "Heatpipe Power") => soc_power = Some(input),
                ("temp", _) => temps.push(Sensor { label: short_label(&label), input }),
                _ => {}
            }
        }
        if let Some(nvme) = find_hwmon(sys, "nvme") {
            for (kind, _label, input) in labelled_inputs(&nvme) {
                if kind == "temp" {
                    temps.push(Sensor { label: "NVME".into(), input });
                    break;
                }
            }
        }

        let battery = sys.join("class/power_supply/macsmc-battery");
        let battery = battery.join("power_now").is_file().then_some(battery);

        Some(Self { e_cluster, p_cluster, sys_power, soc_power, temps, battery })
    }

    /// Take a sample. `cpu_usage[i]` is the busy percentage of `cpu<i>`.
    pub fn sample(&self, cpu_usage: &[f32]) -> AppleSiliconStats {
        let sensors: Vec<(String, f64)> = self
            .temps
            .iter()
            .filter_map(|s| Some((s.label.clone(), read_num(&s.input)? / 1000.0)))
            .collect();
        let hottest = sensors.iter().map(|(_, t)| *t).fold(0.0, f64::max);

        let (battery_w, battery_status) = match &self.battery {
            Some(dir) => (
                read_num(&dir.join("power_now")).map(|uw| uw / 1_000_000.0),
                fs::read_to_string(dir.join("status")).map(|s| s.trim().to_uppercase()).unwrap_or_default(),
            ),
            None => (None, String::new()),
        };

        AppleSiliconStats {
            available: true,
            source: Source::Asahi,
            ecpu_freq_mhz: read_num(&self.e_cluster.cur_freq_file).unwrap_or(0.0) / 1000.0,
            ecpu_usage_percent: mean_usage(cpu_usage, &self.e_cluster.cpus),
            pcpu_freq_mhz: read_num(&self.p_cluster.cur_freq_file).unwrap_or(0.0) / 1000.0,
            pcpu_usage_percent: mean_usage(cpu_usage, &self.p_cluster.cpus),
            sys_power_w: self.sys_power.as_deref().and_then(read_num).unwrap_or(0.0) / 1_000_000.0,
            soc_power_w: self.soc_power.as_deref().and_then(read_num).unwrap_or(0.0) / 1_000_000.0,
            battery_w,
            battery_status,
            thermal_status: thermal_status(hottest),
            sensors,
            ..Default::default()
        }
    }
}

/// (E, P) clusters from cpufreq policies, or None on a single-cluster CPU.
fn find_clusters(sys: &Path) -> Option<(Cluster, Cluster)> {
    let mut policies: Vec<(u64, Cluster)> = fs::read_dir(sys.join("devices/system/cpu/cpufreq"))
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("policy"))
        .filter_map(|e| {
            let dir = e.path();
            let max = read_num(&dir.join("cpuinfo_max_freq"))? as u64;
            let cpus = fs::read_to_string(dir.join("related_cpus"))
                .ok()?
                .split_whitespace()
                .filter_map(|c| c.parse().ok())
                .collect::<Vec<usize>>();
            (!cpus.is_empty()).then(|| (max, Cluster { cpus, cur_freq_file: dir.join("scaling_cur_freq") }))
        })
        .collect();
    policies.sort_by_key(|(max, c)| (*max, c.cpus[0]));
    let (lo_max, _) = policies.first()?;
    let (hi_max, _) = policies.last()?;
    if lo_max == hi_max {
        return None;
    }
    let p = policies.pop()?.1;
    let e = policies.swap_remove(0).1;
    Some((e, p))
}

fn find_hwmon(sys: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(sys.join("class/hwmon"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|dir| fs::read_to_string(dir.join("name")).is_ok_and(|n| n.trim() == name))
}

/// (kind, label, input path) for every `<kind><n>_label` in a hwmon directory.
fn labelled_inputs(dir: &Path) -> Vec<(String, String, PathBuf)> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().to_string();
            let stem = file.strip_suffix("_label")?;
            let kind = stem.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
            let label = fs::read_to_string(e.path()).ok()?.trim().to_string();
            let input = dir.join(format!("{stem}_input"));
            input.is_file().then_some((kind, label, input))
        })
        .collect();
    out.sort_by(|a, b| a.2.cmp(&b.2));
    out
}

/// Compact LCARS-style names for the M1 SMC sensors.
fn short_label(label: &str) -> String {
    match label {
        "NAND Flash Temperature" => "NAND".into(),
        "Battery Hotspot" => "BATTERY".into(),
        "Charge Regulator Temp" => "CHARGER".into(),
        "WiFi/BT Module Temp" => "WIFI".into(),
        other => other.to_uppercase().replace(" TEMPERATURE", "").replace(" TEMP", ""),
    }
}

fn read_num(path: &Path) -> Option<f64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn mean_usage(usage: &[f32], cpus: &[usize]) -> f64 {
    let vals: Vec<f64> = cpus.iter().filter_map(|&i| usage.get(i)).map(|&u| u as f64).collect();
    if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake sysfs tree shaped like an M1 MacBook Air under Asahi.
    fn m1_tree(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lcars-asahi-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let w = |p: &str, v: &str| {
            let f = root.join(p);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, format!("{v}\n")).unwrap();
        };
        let cf = "devices/system/cpu/cpufreq";
        w(&format!("{cf}/policy0/related_cpus"), "0 1 2 3");
        w(&format!("{cf}/policy0/cpuinfo_max_freq"), "2064000");
        w(&format!("{cf}/policy0/scaling_cur_freq"), "972000");
        w(&format!("{cf}/policy4/related_cpus"), "4 5 6 7");
        w(&format!("{cf}/policy4/cpuinfo_max_freq"), "3204000");
        w(&format!("{cf}/policy4/scaling_cur_freq"), "3204000");
        w("class/hwmon/hwmon3/name", "macsmc_hwmon");
        w("class/hwmon/hwmon3/power1_label", "Total System Power");
        w("class/hwmon/hwmon3/power1_input", "5102535");
        w("class/hwmon/hwmon3/power3_label", "Heatpipe Power");
        w("class/hwmon/hwmon3/power3_input", "1592752");
        w("class/hwmon/hwmon3/temp1_label", "NAND Flash Temperature");
        w("class/hwmon/hwmon3/temp1_input", "34372");
        w("class/hwmon/hwmon3/temp4_label", "WiFi/BT Module Temp");
        w("class/hwmon/hwmon3/temp4_input", "71500");
        w("class/hwmon/hwmon0/name", "nvme");
        w("class/hwmon/hwmon0/temp1_label", "Composite");
        w("class/hwmon/hwmon0/temp1_input", "32850");
        w("class/power_supply/macsmc-battery/power_now", "-5187000");
        w("class/power_supply/macsmc-battery/status", "Discharging");
        root
    }

    #[test]
    fn detects_m1_clusters_power_and_sensors() {
        let root = m1_tree("detect");
        let c = AsahiCollector::detect_at(&root).expect("M1 tree should be detected");
        let usage = [10.0, 20.0, 30.0, 40.0, 90.0, 70.0, 0.0, 0.0];
        let s = c.sample(&usage);
        assert_eq!(s.source, Source::Asahi);
        assert!(s.available);
        assert_eq!(s.ecpu_freq_mhz, 972.0);
        assert_eq!(s.pcpu_freq_mhz, 3204.0);
        assert!((s.ecpu_usage_percent - 25.0).abs() < 1e-9);
        assert!((s.pcpu_usage_percent - 40.0).abs() < 1e-9);
        assert!((s.sys_power_w - 5.102535).abs() < 1e-9);
        assert!((s.soc_power_w - 1.592752).abs() < 1e-9);
        assert_eq!(s.battery_w, Some(-5.187));
        assert_eq!(s.battery_status, "DISCHARGING");
        let labels: Vec<&str> = s.sensors.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["NAND", "WIFI", "NVME"]);
        // Hottest sensor is 71.5 °C -> ELEVATED.
        assert_eq!(s.thermal_status, "ELEVATED");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn not_detected_without_smc_or_with_one_cluster() {
        let root = m1_tree("nosmc");
        fs::remove_dir_all(root.join("class/hwmon/hwmon3")).unwrap();
        assert!(AsahiCollector::detect_at(&root).is_none(), "no macsmc_hwmon");
        fs::remove_dir_all(&root).unwrap();

        let root = m1_tree("onecluster");
        fs::write(root.join("devices/system/cpu/cpufreq/policy4/cpuinfo_max_freq"), "2064000\n").unwrap();
        assert!(AsahiCollector::detect_at(&root).is_none(), "equal max freqs = no big.LITTLE split");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_values_read_as_zero_not_panic() {
        let root = m1_tree("missing");
        let c = AsahiCollector::detect_at(&root).unwrap();
        fs::remove_file(root.join("class/hwmon/hwmon3/power1_input")).unwrap();
        fs::remove_file(root.join("devices/system/cpu/cpufreq/policy0/scaling_cur_freq")).unwrap();
        let s = c.sample(&[]);
        assert_eq!(s.sys_power_w, 0.0);
        assert_eq!(s.ecpu_freq_mhz, 0.0);
        assert_eq!(s.ecpu_usage_percent, 0.0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn short_labels() {
        assert_eq!(short_label("Battery Hotspot"), "BATTERY");
        assert_eq!(short_label("Charge Regulator Temp"), "CHARGER");
        assert_eq!(short_label("Some Die Temperature"), "SOME DIE");
    }
}
