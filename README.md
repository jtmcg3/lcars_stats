# lcars_stats

`lcars_stats` is a macOS terminal system monitor with an LCARS-inspired interface. It is currently designed for Apple Silicon Macs and uses `macmon` for Apple-specific hardware metrics.

## Platform Support

This project is currently **macOS only**.

| Platform | Status | Notes |
| --- | --- | --- |
| macOS on Apple Silicon | Supported | Full intended target. Uses `macmon` for GPU, power, thermal, and CPU cluster metrics. |
| macOS on Intel | Not tested | Basic `sysinfo` metrics may work, but Apple Silicon/macmon panels are not expected to provide full data. |
| Linux | Not supported | Linux/NVIDIA support is not implemented. |
| Windows | Not supported | Windows support is not implemented. |

## Requirements

- macOS
- Apple Silicon Mac recommended
- Rust toolchain with `cargo`
- `macmon` installed and available on `PATH`
- A terminal with Unicode/block character support

The app can still start without `macmon`, but Apple Silicon-specific panels such as GPU, power, thermal, ANE, and P/E core metrics will show unavailable or fallback data.

## Install Dependencies

### Rust

Install Rust with `rustup`:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After installation, restart your shell or source the cargo environment file as instructed by `rustup`.

Verify Rust is available:

```sh
cargo --version
```

### macmon

Install `macmon` using the installation method recommended by the `macmon` project, then verify the binary is available:

```sh
macmon --help
```

`lcars_stats` expects to run:

```sh
macmon pipe -i 500
```

If that command does not work, the Apple Silicon-specific panels will not have live hardware data.

## Build

Clone the repository and build with Cargo:

```sh
git clone <repo-url>
cd lcars_stats
cargo build --release
```

The release binary will be created at:

```sh
target/release/lcars_stats
```

## Run

From the repository:

```sh
cargo run --release
```

Or run the compiled binary directly:

```sh
./target/release/lcars_stats
```

## Controls

| Key | Action |
| --- | --- |
| `q` | Quit |
| `Esc` | Quit |

## Troubleshooting

### Apple-specific panels show unavailable data

Confirm `macmon` is installed and available on `PATH`:

```sh
which macmon
macmon pipe -i 500
```

If `macmon pipe -i 500` fails, fix the `macmon` installation before running `lcars_stats`.

### Cargo cannot compile the project

Make sure Rust is installed and up to date:

```sh
rustup update
cargo --version
```

On a fresh macOS install, you may also need Apple's command line developer tools:

```sh
xcode-select --install
```

### The display looks misaligned

Use a terminal and font with Unicode/block character support. The interface uses box drawing and block characters for the LCARS-style layout.

