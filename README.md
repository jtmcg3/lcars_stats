# lcars_stats

Star Trek LCARS-themed system monitor TUI written in Rust.

On Apple Silicon Macs with [`macmon`](https://github.com/vladkens/macmon) installed, it shows P/E-cluster CPU, GPU, Neural Engine, power, and thermal metrics. On Apple Silicon running Linux ([Asahi](https://asahilinux.org/)), it reads P/E clusters, power, battery, and temperatures from sysfs instead. Elsewhere it falls back to aggregate CPU, memory, network, disk I/O, and top processes from [`sysinfo`](https://crates.io/crates/sysinfo).

## Supported platforms

| Platform | Apple Silicon panels | Colours |
| --- | --- | --- |
| macOS on Apple M-series | Full, via `macmon` | Classic TNG |
| **Omarchy on Apple M-series Macs** (Asahi Linux) | P/E clusters, power, battery, temperatures from sysfs | Follows the Omarchy theme |
| Other Linux / platforms | Aggregate CPU only | Follows the Omarchy theme if present, else TNG |

Tested on Omarchy on an M1 MacBook Air.

## Requirements

- **Rust 1.88+** (`edition = "2024"` and `if`/`let` chains)
- A color-capable terminal
- Optional: `macmon` on PATH (macOS / Apple Silicon) for GPU, ANE, power, and temperature panels
- Asahi Linux needs nothing extra: cpufreq and the `macsmc_hwmon` driver are read directly

## Build and run

```bash
cargo run --release
```

Quit with `q` or Esc.

## Layout

| Panel | Source |
| --- | --- |
| Warp Nacelles (CPU clusters) | `macmon` or Asahi cpufreq P/E clusters, else sysinfo aggregate CPU |
| Plasma Conduit (GPU) | `macmon` (no Linux telemetry on Asahi) |
| Neural Matrix (ANE) | `macmon` (no Linux driver on Asahi) |
| Reactor Output (power) | `macmon`; Asahi: SoC + system power, battery flow |
| Environmental (thermal) | `macmon`; Asahi: SMC and NVMe temperature sensors |
| Shield Capacity (memory) | sysinfo |
| Subspace Array (network) | sysinfo |
| Cargo Transport (disk I/O) | sysinfo process I/O totals |
| Sensor Array (top processes) | sysinfo |

When neither `macmon` nor Asahi sysfs is available, Apple-specific panels show a “require macmon” message and the header/footer status is `UNAVAILABLE`.

## Colours

The default palette is classic TNG LCARS. On [Omarchy](https://omarchy.org/), lcars_stats follows the active desktop theme and recolours live when the theme changes:

1. `<theme>/lcars.toml` sets LCARS roles directly (`primary`, `secondary`, `tertiary`, `soft`, `warm`, `quiet`, `value`, `caution`, `warning`, `alert`), each as `"#rrggbb"`.
2. Otherwise roles are derived from the theme's `colors.toml`.

`<theme>` is `~/.local/state/omarchy/current/theme` (or `$LCARS_THEME_DIR`). Set `LCARS_PALETTE=tng` to always use the classic colours.
