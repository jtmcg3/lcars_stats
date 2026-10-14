mod apple_silicon;
mod lcars_widgets;

use apple_silicon::{AppleSiliconStats, MacmonCollector, MacmonSampleState};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lcars_widgets::*;
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};

/// Network statistics tracking
struct NetworkStats {
    rx_bytes: u64,
    tx_bytes: u64,
    rx_speed: f64,
    tx_speed: f64,
}

/// Disk I/O statistics
struct DiskStats {
    read_bytes: u64,
    write_bytes: u64,
    read_speed: f64,
    write_speed: f64,
}

/// Top process information
#[derive(Clone)]
struct ProcessInfo {
    name: String,
    cpu_percent: f32,
    memory_bytes: u64,
}

/// Main application state
struct App {
    sys: System,
    networks: Networks,
    disks: Disks,
    should_quit: bool,
    network_stats: NetworkStats,
    disk_stats: DiskStats,
    macmon: Option<MacmonCollector>,
    apple_stats: AppleSiliconStats,
    top_processes: Vec<ProcessInfo>,
}

impl App {
    fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();

        // Try to start macmon collector
        let macmon = MacmonCollector::new();
        let apple_stats = if macmon.is_some() {
            AppleSiliconStats::no_data()
        } else {
            AppleSiliconStats::unavailable()
        };

        Self {
            sys,
            networks,
            disks,
            should_quit: false,
            network_stats: NetworkStats {
                rx_bytes: 0,
                tx_bytes: 0,
                rx_speed: 0.0,
                tx_speed: 0.0,
            },
            disk_stats: DiskStats {
                read_bytes: 0,
                write_bytes: 0,
                read_speed: 0.0,
                write_speed: 0.0,
            },
            macmon,
            apple_stats,
            top_processes: Vec::new(),
        }
    }

    fn update(&mut self) {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

        // Update network stats
        let old_rx = self.network_stats.rx_bytes;
        let old_tx = self.network_stats.tx_bytes;
        self.networks.refresh(true);

        let mut total_rx = 0;
        let mut total_tx = 0;
        for (_name, network) in &self.networks {
            total_rx += network.total_received();
            total_tx += network.total_transmitted();
        }

        self.network_stats.rx_bytes = total_rx;
        self.network_stats.tx_bytes = total_tx;
        if old_rx > 0 {
            self.network_stats.rx_speed = (total_rx.saturating_sub(old_rx)) as f64;
            self.network_stats.tx_speed = (total_tx.saturating_sub(old_tx)) as f64;
        }

        // Update disk stats
        let old_read = self.disk_stats.read_bytes;
        let old_write = self.disk_stats.write_bytes;
        self.disks.refresh(true);

        let mut total_read = 0u64;
        let mut total_write = 0u64;
        // Note: sysinfo doesn't provide per-disk I/O rates directly
        // We'll estimate from process I/O if available
        for process in self.sys.processes().values() {
            let disk_usage = process.disk_usage();
            total_read = total_read.saturating_add(disk_usage.total_read_bytes);
            total_write = total_write.saturating_add(disk_usage.total_written_bytes);
        }

        self.disk_stats.read_bytes = total_read;
        self.disk_stats.write_bytes = total_write;
        if old_read > 0 {
            self.disk_stats.read_speed = (total_read.saturating_sub(old_read)) as f64;
            self.disk_stats.write_speed = (total_write.saturating_sub(old_write)) as f64;
        }

        if let Some(ref mut collector) = self.macmon {
            self.apple_stats = collector.stats();
        }

        // Update top processes
        self.update_top_processes();
    }

    fn update_top_processes(&mut self) {
        let mut processes: Vec<ProcessInfo> = self
            .sys
            .processes()
            .values()
            .map(|p| ProcessInfo {
                name: p.name().to_string_lossy().to_string(),
                cpu_percent: p.cpu_usage(),
                memory_bytes: p.memory(),
            })
            .collect();

        // Sort by CPU usage descending
        processes.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));

        // Keep top 5
        self.top_processes = processes.into_iter().take(5).collect();
    }

    fn on_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            _ => {}
        }
    }
}

fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {err:?}");
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), B::Error>
where
    B::Error: From<io::Error>,
{
    loop {
        app.update();
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(1000))?
            && let Event::Key(key) = event::read()?
        {
            app.on_key(key.code);
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Main layout: header, content, footer
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Content
            Constraint::Length(2),  // Footer
        ])
        .split(size);

    // Render header
    let stardate = format!("{:.2}", chrono::Local::now().timestamp() as f64 / 86400.0 + 2451545.0);
    let header = LcarsHeader::new("ENGINEERING", &stardate);
    f.render_widget(header, main_chunks[0]);

    // Content area: sidebar + main panels
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),   // Sidebar
            Constraint::Min(0),      // Main content
        ])
        .split(main_chunks[1]);

    // Render sidebar
    render_sidebar(f, content_chunks[0], app);

    // Main content panels
    let panel_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),   // CPU clusters (WARP NACELLES)
            Constraint::Length(5),   // GPU (PLASMA CONDUIT)
            Constraint::Length(4),   // ANE (NEURAL MATRIX)
            Constraint::Length(5),   // Power (REACTOR OUTPUT)
            Constraint::Length(4),   // Thermal (ENVIRONMENTAL)
            Constraint::Length(4),   // Memory (SHIELD CAPACITY)
            Constraint::Length(5),   // Network (SUBSPACE ARRAY)
            Constraint::Length(4),   // Disk (CARGO TRANSPORT)
            Constraint::Min(0),      // Processes (SENSOR ARRAY)
        ])
        .split(content_chunks[1]);

    // Render each panel
    render_warp_nacelles(f, panel_chunks[0], app);
    render_plasma_conduit(f, panel_chunks[1], app);
    render_neural_matrix(f, panel_chunks[2], app);
    render_reactor_output(f, panel_chunks[3], app);
    render_environmental(f, panel_chunks[4], app);
    render_shield_capacity(f, panel_chunks[5], app);
    render_subspace_array(f, panel_chunks[6], app);
    render_cargo_transport(f, panel_chunks[7], app);
    render_sensor_array(f, panel_chunks[8], app);

    // Render footer
    let footer = LcarsFooter::new(
        &[("[Q]", "TERMINATE", LCARS_ORANGE), ("[R]", "REFRESH", LCARS_BLUE)],
        &app.apple_stats.thermal_status,
    );
    f.render_widget(footer, main_chunks[2]);
}

/// LCARS Header widget
struct LcarsHeader {
    title: String,
    stardate: String,
}

impl LcarsHeader {
    fn new(title: &str, stardate: &str) -> Self {
        Self {
            title: title.to_string(),
            stardate: stardate.to_string(),
        }
    }
}

impl Widget for LcarsHeader {
    fn render(self, area: Rect, buf: &mut Buffer) {
        draw_lcars_header(buf, area, &self.title, &self.stardate);
    }
}

/// LCARS Footer widget
struct LcarsFooter {
    items: Vec<(String, String, ratatui::style::Color)>,
    status: String,
}

impl LcarsFooter {
    fn new(items: &[(&str, &str, ratatui::style::Color)], status: &str) -> Self {
        Self {
            items: items.iter().map(|(k, v, c)| (k.to_string(), v.to_string(), *c)).collect(),
            status: status.to_string(),
        }
    }
}

impl Widget for LcarsFooter {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let items: Vec<(&str, ratatui::style::Color)> = self.items
            .iter()
            .map(|(k, v, c)| {
                let combined = format!("{} {}", k, v);
                // We need to leak this string to get a static lifetime
                // This is a hack but works for our use case
                (Box::leak(combined.into_boxed_str()) as &str, *c)
            })
            .collect();
        draw_lcars_footer(buf, area, &items, &self.status);
    }
}

fn render_sidebar(f: &mut Frame, area: Rect, _app: &App) {
    // Define sidebar buttons aligned with content panel heights
    // Heights must match: 5, 5, 4, 5, 4, 4, 5, 4, Min(remaining)
    let buttons = [
        ("CPU", LCARS_ORANGE, 5u16),     // WARP NACELLES
        ("GPU", LCARS_BLUE, 5u16),       // PLASMA CONDUIT
        ("ANE", LCARS_PURPLE, 4u16),     // NEURAL MATRIX
        ("PWR", LCARS_PEACH, 5u16),      // REACTOR OUTPUT
        ("TMP", LCARS_YELLOW, 4u16),     // ENVIRONMENTAL
        ("MEM", LCARS_BLUE, 4u16),       // SHIELD CAPACITY
        ("NET", LCARS_PEACH, 5u16),      // SUBSPACE ARRAY
        ("DSK", LCARS_PURPLE, 4u16),     // CARGO TRANSPORT
        ("PRC", LCARS_TAN, 0u16),        // SENSOR ARRAY (fills remaining)
    ];

    let mut y_offset = 0u16;

    for (label, color, height) in &buttons {
        let btn_height = if *height == 0 {
            // Last button fills remaining space
            area.height.saturating_sub(y_offset)
        } else {
            *height
        };

        if y_offset + btn_height <= area.height {
            let button_area = Rect {
                x: area.x,
                y: area.y + y_offset,
                width: area.width,
                height: btn_height,
            };
            let button = LcarsSidebarButton::new(label, *color);
            f.render_widget(button, button_area);
        }
        y_offset += btn_height;
    }
}

fn line_with_stale(state: MacmonSampleState, mut spans: Vec<Span<'static>>) -> Line<'static> {
    if state == MacmonSampleState::Stale {
        spans.push(Span::styled(
            "  STALE",
            Style::default().fg(LCARS_RED).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn render_warp_nacelles(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("WARP NACELLES (CPU CLUSTERS)", LCARS_ORANGE);

    let ecpu_pct = app.apple_stats.ecpu_usage_percent;
    let pcpu_pct = app.apple_stats.pcpu_usage_percent;
    let ecpu_freq = app.apple_stats.ecpu_freq_mhz;
    let pcpu_freq = app.apple_stats.pcpu_freq_mhz;

    let content = match app.apple_stats.sample_state {
        state @ (MacmonSampleState::Live | MacmonSampleState::Stale) => vec![
            Line::from(""), // Align with pill interior
            title,
            Line::from(vec![
                Span::styled("P-CLUSTER: ", Style::default().fg(LCARS_ORANGE)),
                Span::styled(format!("{:.0}%", pcpu_pct), Style::default().fg(get_usage_color(pcpu_pct))),
                Span::styled(format!("  @ {:.0} MHz", pcpu_freq), Style::default().fg(LCARS_YELLOW)),
            ]),
            line_with_stale(state, vec![
                Span::styled("E-CLUSTER: ", Style::default().fg(LCARS_ORANGE)),
                Span::styled(format!("{:.0}%", ecpu_pct), Style::default().fg(get_usage_color(ecpu_pct))),
                Span::styled(format!("  @ {:.0} MHz", ecpu_freq), Style::default().fg(LCARS_YELLOW)),
            ]),
        ],
        MacmonSampleState::NoData => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("NO DATA", Style::default().fg(LCARS_PEACH))),
        ],
        MacmonSampleState::NotInstalled => {
            let cpu_usage: f32 = app.sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / app.sys.cpus().len() as f32;
            vec![
                Line::from(""),
                title,
                Line::from(vec![
                    Span::styled("AGGREGATE: ", Style::default().fg(LCARS_ORANGE)),
                    Span::styled(format!("{:.0}%", cpu_usage), Style::default().fg(get_usage_color(cpu_usage as f64))),
                    Span::styled(" (macmon unavailable)", Style::default().fg(LCARS_PEACH)),
                ]),
            ]
        }
    };

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_plasma_conduit(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("PLASMA CONDUIT (GPU)", LCARS_BLUE);

    let content = match app.apple_stats.sample_state {
        state @ (MacmonSampleState::Live | MacmonSampleState::Stale) => {
            let gpu_pct = app.apple_stats.gpu_usage_percent;
            let gpu_freq = app.apple_stats.gpu_freq_mhz;
            let gpu_power = app.apple_stats.gpu_power_w;
            vec![
                Line::from(""),
                title,
                Line::from(vec![
                    Span::styled("UTILIZATION: ", Style::default().fg(LCARS_BLUE)),
                    Span::styled(format!("{:.0}%", gpu_pct), Style::default().fg(get_usage_color(gpu_pct))),
                ]),
                line_with_stale(state, vec![
                    Span::styled(format!("FREQ: {:.0} MHz  POWER: {:.1}W", gpu_freq, gpu_power), Style::default().fg(LCARS_YELLOW)),
                ]),
            ]
        }
        MacmonSampleState::NoData => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("NO DATA", Style::default().fg(LCARS_PEACH))),
        ],
        MacmonSampleState::NotInstalled => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("GPU metrics require macmon", Style::default().fg(LCARS_PEACH))),
        ],
    };

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_neural_matrix(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("NEURAL MATRIX (ANE)", LCARS_PURPLE);

    let content = match app.apple_stats.sample_state {
        state @ (MacmonSampleState::Live | MacmonSampleState::Stale) => {
            let ane_power = app.apple_stats.ane_power_w;
            let status = if ane_power > 0.1 { "ACTIVE" } else { "STANDBY" };
            vec![
                Line::from(""),
                title,
                line_with_stale(state, vec![
                    Span::styled("POWER: ", Style::default().fg(LCARS_PURPLE)),
                    Span::styled(format!("{:.2}W", ane_power), Style::default().fg(LCARS_YELLOW)),
                    Span::styled(format!("   STATUS: {}", status), Style::default().fg(if ane_power > 0.1 { LCARS_BLUE } else { LCARS_PEACH })),
                ]),
            ]
        }
        MacmonSampleState::NoData => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("NO DATA", Style::default().fg(LCARS_PEACH))),
        ],
        MacmonSampleState::NotInstalled => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("ANE metrics require macmon", Style::default().fg(LCARS_PEACH))),
        ],
    };

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_reactor_output(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("REACTOR OUTPUT (POWER)", LCARS_PEACH);

    let content = match app.apple_stats.sample_state {
        state @ (MacmonSampleState::Live | MacmonSampleState::Stale) => vec![
            Line::from(""),
            title,
            Line::from(vec![
                Span::styled("CPU: ", Style::default().fg(LCARS_ORANGE)),
                Span::styled(format!("{:.1}W", app.apple_stats.cpu_power_w), Style::default().fg(LCARS_YELLOW)),
                Span::styled("  GPU: ", Style::default().fg(LCARS_BLUE)),
                Span::styled(format!("{:.1}W", app.apple_stats.gpu_power_w), Style::default().fg(LCARS_YELLOW)),
            ]),
            line_with_stale(state, vec![
                Span::styled("ANE: ", Style::default().fg(LCARS_PURPLE)),
                Span::styled(format!("{:.1}W", app.apple_stats.ane_power_w), Style::default().fg(LCARS_YELLOW)),
                Span::styled("  TOTAL: ", Style::default().fg(LCARS_PEACH)),
                Span::styled(format!("{:.1}W", app.apple_stats.sys_power_w), Style::default().fg(LCARS_YELLOW).add_modifier(Modifier::BOLD)),
            ]),
        ],
        MacmonSampleState::NoData => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("NO DATA", Style::default().fg(LCARS_PEACH))),
        ],
        MacmonSampleState::NotInstalled => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("Power metrics require macmon", Style::default().fg(LCARS_PEACH))),
        ],
    };

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_environmental(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("ENVIRONMENTAL CONTROLS (THERMAL)", LCARS_YELLOW);

    let content = match app.apple_stats.sample_state {
        MacmonSampleState::Live | MacmonSampleState::Stale => {
            let cpu_temp = app.apple_stats.cpu_temp_c;
            let gpu_temp = app.apple_stats.gpu_temp_c;
            let status_color = match app.apple_stats.thermal_status.as_str() {
                "NOMINAL" => LCARS_BLUE,
                "ELEVATED" => LCARS_YELLOW,
                "STALE" => LCARS_PEACH,
                _ => LCARS_RED,
            };
            vec![
                Line::from(""),
                title,
                Line::from(vec![
                    Span::styled("CPU: ", Style::default().fg(LCARS_ORANGE)),
                    Span::styled(format!("{:.0}°C", cpu_temp), Style::default().fg(get_temp_color(cpu_temp))),
                    Span::styled("  GPU: ", Style::default().fg(LCARS_BLUE)),
                    Span::styled(format!("{:.0}°C", gpu_temp), Style::default().fg(get_temp_color(gpu_temp))),
                    Span::styled("  STATUS: ", Style::default().fg(LCARS_PEACH)),
                    Span::styled(&app.apple_stats.thermal_status, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                ]),
            ]
        }
        MacmonSampleState::NoData => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("NO DATA", Style::default().fg(LCARS_PEACH))),
        ],
        MacmonSampleState::NotInstalled => vec![
            Line::from(""),
            title,
            Line::from(Span::styled("Temperature metrics require macmon", Style::default().fg(LCARS_PEACH))),
        ],
    };

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_shield_capacity(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("SHIELD CAPACITY (MEMORY)", LCARS_BLUE);

    let total_mem = app.sys.total_memory();
    let used_mem = app.sys.used_memory();
    let percent = if total_mem > 0 {
        (used_mem as f64 / total_mem as f64) * 100.0
    } else {
        0.0
    };

    let content = vec![
        Line::from(""), // Align with pill interior
        title,
        Line::from(vec![
            Span::styled(format!("{:.1}%", percent), Style::default().fg(get_usage_color(percent))),
            Span::styled(format!("  ({:.2} GB / {:.2} GB)",
                used_mem as f64 / 1_073_741_824.0,
                total_mem as f64 / 1_073_741_824.0), Style::default().fg(LCARS_YELLOW)),
        ]),
    ];

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_subspace_array(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("SUBSPACE ARRAY (NETWORK)", LCARS_PEACH);

    let rx_gb = app.network_stats.rx_bytes as f64 / 1_073_741_824.0;
    let tx_gb = app.network_stats.tx_bytes as f64 / 1_073_741_824.0;
    let rx_speed = app.network_stats.rx_speed / 1_048_576.0; // MB/s
    let tx_speed = app.network_stats.tx_speed / 1_048_576.0;

    let content = vec![
        Line::from(""), // Align with pill interior
        title,
        Line::from(vec![
            Span::styled("▼ RX: ", Style::default().fg(LCARS_YELLOW)),
            Span::styled(format!("{:.2} GB", rx_gb), Style::default().fg(LCARS_BLUE)),
            Span::styled(format!(" [{:.1} MB/s]", rx_speed), Style::default().fg(LCARS_PURPLE)),
        ]),
        Line::from(vec![
            Span::styled("▲ TX: ", Style::default().fg(LCARS_YELLOW)),
            Span::styled(format!("{:.2} GB", tx_gb), Style::default().fg(LCARS_BLUE)),
            Span::styled(format!(" [{:.1} MB/s]", tx_speed), Style::default().fg(LCARS_PURPLE)),

        ])
    ];

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_cargo_transport(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("CARGO TRANSPORT (DISK I/O)", LCARS_PURPLE);

    let read_speed = app.disk_stats.read_speed / 1_048_576.0; // MB/s
    let write_speed = app.disk_stats.write_speed / 1_048_576.0;

    let content = vec![
        Line::from(""), // Align with pill interior
        title,
        Line::from(vec![
            Span::styled("READ: ", Style::default().fg(LCARS_YELLOW)),
            Span::styled(format!("{:.1} MB/s", read_speed), Style::default().fg(LCARS_BLUE)),
            Span::styled("  WRITE: ", Style::default().fg(LCARS_YELLOW)),
            Span::styled(format!("{:.1} MB/s", write_speed), Style::default().fg(LCARS_BLUE)),
        ]),
    ];

    let para = Paragraph::new(content).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

fn render_sensor_array(f: &mut Frame, area: Rect, app: &App) {
    let title = lcars_section_title("SENSOR ARRAY (TOP PROCESSES)", LCARS_TAN);

    let mut lines = vec![
        Line::from(""), // Align with pill interior
        title,
    ];

    for proc in &app.top_processes {
        let name: String = proc.name.chars().take(15).collect();
        let mem_mb = proc.memory_bytes as f64 / 1_048_576.0;
        lines.push(Line::from(vec![
            Span::styled(format!("{:<15}", name), Style::default().fg(LCARS_YELLOW)),
            Span::styled(format!(" CPU: {:>5.1}%", proc.cpu_percent), Style::default().fg(get_usage_color(proc.cpu_percent as f64))),
            Span::styled(format!("  MEM: {:>7.1} MB", mem_mb), Style::default().fg(LCARS_BLUE)),
        ]));
    }

    let para = Paragraph::new(lines).style(Style::default().bg(LCARS_BG));
    f.render_widget(para, area);
}

#[cfg(test)]
mod panel_state {
    use super::apple_silicon::{AppleSiliconStats, MacmonMetrics, MacmonSampleState, TempMetrics};
    use super::{
        render_environmental, render_neural_matrix, render_plasma_conduit, render_reactor_output,
        render_warp_nacelles, App,
    };
    use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Frame, Terminal};

    fn reading() -> AppleSiliconStats {
        AppleSiliconStats::from_macmon(&MacmonMetrics {
            ecpu_usage: (1200.0, 0.25),
            pcpu_usage: (3200.0, 0.5),
            gpu_usage: (800.0, 0.125),
            cpu_power: 3.5,
            gpu_power: 1.5,
            ane_power: 0.25,
            all_power: 6.0,
            sys_power: 8.0,
            temp: TempMetrics {
                cpu_temp_avg: 45.0,
                gpu_temp_avg: 40.0,
            },
            ..Default::default()
        })
    }

    fn mark_stale(mut stats: AppleSiliconStats) -> AppleSiliconStats {
        stats.sample_state = MacmonSampleState::Stale;
        stats.thermal_status = "STALE".to_string();
        stats
    }

    fn paint(app: &App, render: fn(&mut Frame, Rect, &App)) -> String {
        let backend = TestBackend::new(100, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), app))
            .unwrap();
        buffer_text(terminal.backend().buffer())
    }

    fn buffer_text(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn no_data_panels_do_not_fall_back_to_sysinfo_or_zeros() {
        let mut app = App::new();
        app.apple_stats = AppleSiliconStats::no_data();
        let cpu = paint(&app, render_warp_nacelles);
        assert!(cpu.contains("NO DATA"), "{cpu}");
        assert!(!cpu.contains("AGGREGATE"), "{cpu}");
        assert!(!cpu.contains("macmon unavailable"), "{cpu}");
        assert!(!cpu.contains("P-CLUSTER"), "{cpu}");

        let gpu = paint(&app, render_plasma_conduit);
        assert!(gpu.contains("NO DATA"), "{gpu}");
        assert!(!gpu.contains("require macmon"), "{gpu}");

        let thermal = paint(&app, render_environmental);
        assert!(thermal.contains("NO DATA"), "{thermal}");
        assert!(!thermal.contains("NOMINAL"), "{thermal}");
        assert!(!thermal.contains("require macmon"), "{thermal}");
    }

    #[test]
    fn not_installed_panels_keep_the_existing_fallback() {
        let mut app = App::new();
        app.apple_stats = AppleSiliconStats::unavailable();
        let cpu = paint(&app, render_warp_nacelles);
        assert!(cpu.contains("AGGREGATE"), "{cpu}");
        assert!(cpu.contains("macmon unavailable"), "{cpu}");
        assert!(!cpu.contains("NO DATA"), "{cpu}");

        let gpu = paint(&app, render_plasma_conduit);
        assert!(gpu.contains("GPU metrics require macmon"), "{gpu}");
        let ane = paint(&app, render_neural_matrix);
        assert!(ane.contains("ANE metrics require macmon"), "{ane}");
        let power = paint(&app, render_reactor_output);
        assert!(power.contains("Power metrics require macmon"), "{power}");
        let thermal = paint(&app, render_environmental);
        assert!(thermal.contains("Temperature metrics require macmon"), "{thermal}");
    }

    #[test]
    fn live_panels_show_readings_without_a_stale_mark() {
        let mut app = App::new();
        app.apple_stats = reading();
        let cpu = paint(&app, render_warp_nacelles);
        assert!(cpu.contains("3200"), "{cpu}");
        assert!(cpu.contains("50%"), "{cpu}");
        assert!(cpu.contains("1200"), "{cpu}");
        assert!(!cpu.contains("STALE"), "{cpu}");
        assert!(!cpu.contains("NO DATA"), "{cpu}");

        let gpu = paint(&app, render_plasma_conduit);
        assert!(gpu.contains("800"), "{gpu}");
        assert!(gpu.contains("1.5W"), "{gpu}");
        assert!(!gpu.contains("STALE"), "{gpu}");
    }

    #[test]
    fn stale_panels_keep_last_numbers_and_show_stale() {
        let mut app = App::new();
        app.apple_stats = mark_stale(reading());
        let cpu = paint(&app, render_warp_nacelles);
        assert!(cpu.contains("3200"), "{cpu}");
        assert!(cpu.contains("50%"), "{cpu}");
        assert!(cpu.contains("1200"), "{cpu}");
        assert!(cpu.contains("STALE"), "{cpu}");
        assert!(!cpu.contains("NO DATA"), "{cpu}");
        assert!(!cpu.contains("AGGREGATE"), "{cpu}");

        let gpu = paint(&app, render_plasma_conduit);
        assert!(gpu.contains("800"), "{gpu}");
        assert!(gpu.contains("1.5W"), "{gpu}");
        assert!(gpu.contains("STALE"), "{gpu}");

        let ane = paint(&app, render_neural_matrix);
        assert!(ane.contains("0.25W"), "{ane}");
        assert!(ane.contains("STALE"), "{ane}");

        let power = paint(&app, render_reactor_output);
        assert!(power.contains("3.5W"), "{power}");
        assert!(power.contains("8.0W"), "{power}");
        assert!(power.contains("STALE"), "{power}");

        let thermal = paint(&app, render_environmental);
        assert!(thermal.contains("45"), "{thermal}");
        assert!(thermal.contains("40"), "{thermal}");
        assert!(thermal.contains("STALE"), "{thermal}");
        assert!(!thermal.contains("NOMINAL"), "{thermal}");
    }
}
