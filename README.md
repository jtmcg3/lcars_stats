# lcars_stats

Star Trek LCARS-themed system monitor TUI written in Rust.

On Apple Silicon Macs with [`macmon`](https://github.com/vladkens/macmon) installed, it shows P/E-cluster CPU, GPU, Neural Engine, power, and thermal metrics. Without `macmon` (or on other platforms), it falls back to aggregate CPU, memory, network, disk I/O, and top processes from [`sysinfo`](https://crates.io/crates/sysinfo).

## Requirements

- **Rust 1.88+** (`edition = "2024"` and `if`/`let` chains)
- A color-capable terminal
- Optional: `macmon` on PATH (macOS / Apple Silicon) for GPU, ANE, power, and temperature panels

## Build and run

```bash
cargo run --release
```

Quit with `q` or Esc.

## Layout

| Panel | Source |
| --- | --- |
| Warp Nacelles (CPU clusters) | `macmon` P/E clusters, or sysinfo aggregate CPU |
| Plasma Conduit (GPU) | `macmon` |
| Neural Matrix (ANE) | `macmon` |
| Reactor Output (power) | `macmon` |
| Environmental (thermal) | `macmon` |
| Shield Capacity (memory) | sysinfo |
| Subspace Array (network) | sysinfo |
| Cargo Transport (disk I/O) | sysinfo process I/O totals |
| Sensor Array (top processes) | sysinfo |

When `macmon` is missing, Apple-specific panels show a “require macmon” message and the header/footer status is `UNAVAILABLE`.
