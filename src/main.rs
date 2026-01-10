use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;
use sysinfo::{Networks, System};

// LCARS color palette
const LCARS_ORANGE: Color = Color::Rgb(255, 153, 0);
const LCARS_BLUE: Color = Color::Rgb(153, 153, 255);
const LCARS_PURPLE: Color = Color::Rgb(204, 153, 255);
const LCARS_PEACH: Color = Color::Rgb(255, 204, 153);
const LCARS_RED: Color = Color::Rgb(204, 102, 102);
const LCARS_YELLOW: Color = Color::Rgb(255, 255, 153);
const LCARS_BG: Color = Color::Black;

struct App {
    sys: System,
    networks: Networks,
    should_quit: bool,
    network_stats: NetworkStats,
}

struct NetworkStats {
    rx_bytes: u64,
    tx_bytes: u64,
    rx_speed: f64,
    tx_speed: f64,
}

impl App {
    fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();

        Self {
            sys,
            networks,
            should_quit: false,
            network_stats: NetworkStats {
                rx_bytes: 0,
                tx_bytes: 0,
                rx_speed: 0.0,
                tx_speed: 0.0,
            },
        }
    }

    fn update(&mut self) {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();

        // Calculate network stats
        let old_rx = self.network_stats.rx_bytes;
        let old_tx = self.network_stats.tx_bytes;

        self.networks.refresh();
        let mut total_rx = 0;
        let mut total_tx = 0;

        for (_name, network) in &self.networks {
            total_rx += network.total_received();
            total_tx += network.total_transmitted();
        }

        self.network_stats.rx_bytes = total_rx;
        self.network_stats.tx_bytes = total_tx;

        // Calculate speeds (bytes per second)
        if old_rx > 0 {
            self.network_stats.rx_speed = (total_rx.saturating_sub(old_rx)) as f64;
            self.network_stats.tx_speed = (total_tx.saturating_sub(old_tx)) as f64;
        }
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
) -> io::Result<()> {
    loop {
        app.update();
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(1000))? {
            if let Event::Key(key) = event::read()? {
                app.on_key(key.code);
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Main content
            Constraint::Length(2),  // Footer
        ])
        .split(size);

    // Header
    render_header(f, chunks[0]);

    // Main content area
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),  // CPU cores
            Constraint::Percentage(20),  // RAM
            Constraint::Percentage(20),  // Swap
            Constraint::Percentage(20),  // Network
        ])
        .split(chunks[1]);

    // Render each section
    render_warp_cores(f, content_chunks[0], app);
    render_shields(f, content_chunks[1], app);
    render_cargo_bay(f, content_chunks[2], app);
    render_subspace(f, content_chunks[3], app);

    // Footer
    render_footer(f, chunks[2]);
}

fn render_header(f: &mut Frame, area: Rect) {
    let stardate = format!("{:.2}", chrono::Local::now().timestamp() as f64 / 86400.0 + 2451545.0);
    let header_text = vec![
        Line::from(vec![
            Span::styled("╔═══════════════════════════════════════════════════════════════════════════════╗", Style::default().fg(LCARS_ORANGE)),
        ]),
        Line::from(vec![
            Span::styled("║  ", Style::default().fg(LCARS_ORANGE)),
            Span::styled("█████", Style::default().fg(LCARS_ORANGE).bg(LCARS_ORANGE)),
            Span::styled("  LCARS SYSTEM MONITOR  ", Style::default().fg(LCARS_ORANGE).add_modifier(Modifier::BOLD)),
            Span::styled("█████", Style::default().fg(LCARS_BLUE).bg(LCARS_BLUE)),
            Span::styled("  STARDATE ", Style::default().fg(LCARS_BLUE)),
            Span::styled(stardate, Style::default().fg(LCARS_YELLOW)),
            Span::styled("  ║", Style::default().fg(LCARS_ORANGE)),
        ]),
        Line::from(vec![
            Span::styled("╚═══════════════════════════════════════════════════════════════════════════════╝", Style::default().fg(LCARS_ORANGE)),
        ]),
    ];

    let header = Paragraph::new(header_text).style(Style::default().bg(LCARS_BG));
    f.render_widget(header, area);
}

fn render_warp_cores(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(LCARS_ORANGE))
        .title(Span::styled(" ◆ WARP CORES ◆ ", Style::default().fg(LCARS_ORANGE).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(LCARS_BG));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let cpus = app.sys.cpus();
    let cpu_count = cpus.len();

    if cpu_count == 0 {
        return;
    }

    // Calculate layout for CPU cores
    let cores_per_row = 4;
    let rows = cpu_count.div_ceil(cores_per_row);

    let mut constraints = vec![];
    for _ in 0..rows {
        constraints.push(Constraint::Length(3));
    }
    if rows < (inner_area.height / 3) as usize {
        constraints.push(Constraint::Min(0));
    }

    let row_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (row_idx, row_area) in row_chunks.iter().enumerate().take(rows) {
        let start_idx = row_idx * cores_per_row;
        let end_idx = (start_idx + cores_per_row).min(cpu_count);
        let cores_in_row = end_idx - start_idx;

        let col_constraints: Vec<Constraint> = (0..cores_in_row)
            .map(|_| Constraint::Percentage((100 / cores_in_row) as u16))
            .collect();

        let col_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);

        for (col_idx, col_area) in col_chunks.iter().enumerate().take(cores_in_row) {
            let cpu_idx = start_idx + col_idx;
            if let Some(cpu) = cpus.get(cpu_idx) {
                let usage = cpu.cpu_usage();
                let color = get_usage_color(usage as u16);

                let gauge = Gauge::default()
                    .block(Block::default())
                    .gauge_style(Style::default().fg(color).bg(LCARS_BG))
                    .label(format!("C{} {:.0}%", cpu_idx, usage))
                    .ratio(f64::from(usage) / 100.0);

                f.render_widget(gauge, *col_area);
            }
        }
    }
}

fn render_shields(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(LCARS_BLUE))
        .title(Span::styled(" ◆ SHIELD CAPACITY ◆ ", Style::default().fg(LCARS_BLUE).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(LCARS_BG));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let total_mem = app.sys.total_memory();
    let used_mem = app.sys.used_memory();
    let percent = if total_mem > 0 {
        (used_mem as f64 / total_mem as f64) * 100.0
    } else {
        0.0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner_area);

    let color = get_usage_color(percent as u16);
    let gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(color).bg(LCARS_BG))
        .label(format!("{:.1}% ({:.2} GB / {:.2} GB)",
            percent,
            used_mem as f64 / 1_073_741_824.0,
            total_mem as f64 / 1_073_741_824.0))
        .ratio(percent / 100.0);

    f.render_widget(gauge, chunks[0]);
}

fn render_cargo_bay(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(LCARS_PURPLE))
        .title(Span::styled(" ◆ CARGO BAY ◆ ", Style::default().fg(LCARS_PURPLE).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(LCARS_BG));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let total_swap = app.sys.total_swap();
    let used_swap = app.sys.used_swap();
    let percent = if total_swap > 0 {
        (used_swap as f64 / total_swap as f64) * 100.0
    } else {
        0.0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner_area);

    let color = get_usage_color(percent as u16);
    let gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(color).bg(LCARS_BG))
        .label(format!("{:.1}% ({:.2} GB / {:.2} GB)",
            percent,
            used_swap as f64 / 1_073_741_824.0,
            total_swap as f64 / 1_073_741_824.0))
        .ratio(percent / 100.0);

    f.render_widget(gauge, chunks[0]);
}

fn render_subspace(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(LCARS_PEACH))
        .title(Span::styled(" ◆ SUBSPACE ARRAY ◆ ", Style::default().fg(LCARS_PEACH).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(LCARS_BG));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let rx_mb = app.network_stats.rx_bytes as f64 / 1_048_576.0;
    let tx_mb = app.network_stats.tx_bytes as f64 / 1_048_576.0;
    let rx_speed_kb = app.network_stats.rx_speed / 1024.0;
    let tx_speed_kb = app.network_stats.tx_speed / 1024.0;

    let text = vec![
        Line::from(vec![
            Span::styled("▼ RX: ", Style::default().fg(LCARS_YELLOW)),
            Span::styled(format!("{:.2} MB", rx_mb), Style::default().fg(LCARS_BLUE)),
            Span::styled("  [", Style::default().fg(LCARS_PEACH)),
            Span::styled(format!("{:.1} KB/s", rx_speed_kb), Style::default().fg(LCARS_PURPLE)),
            Span::styled("]", Style::default().fg(LCARS_PEACH)),
        ]),
        Line::from(vec![
            Span::styled("▲ TX: ", Style::default().fg(LCARS_YELLOW)),
            Span::styled(format!("{:.2} MB", tx_mb), Style::default().fg(LCARS_BLUE)),
            Span::styled("  [", Style::default().fg(LCARS_PEACH)),
            Span::styled(format!("{:.1} KB/s", tx_speed_kb), Style::default().fg(LCARS_PURPLE)),
            Span::styled("]", Style::default().fg(LCARS_PEACH)),
        ]),
    ];

    let paragraph = Paragraph::new(text).style(Style::default().bg(LCARS_BG));
    f.render_widget(paragraph, inner_area);
}

fn render_footer(f: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("█", Style::default().fg(LCARS_ORANGE).bg(LCARS_ORANGE)),
        Span::styled(" [Q/ESC] ", Style::default().fg(LCARS_ORANGE)),
        Span::styled("TERMINATE  ", Style::default().fg(LCARS_YELLOW)),
        Span::styled("█", Style::default().fg(LCARS_BLUE).bg(LCARS_BLUE)),
        Span::styled(" SYSTEM STATUS: ", Style::default().fg(LCARS_BLUE)),
        Span::styled("NOMINAL", Style::default().fg(LCARS_YELLOW).add_modifier(Modifier::BOLD)),
    ]))
    .style(Style::default().bg(LCARS_BG));

    f.render_widget(footer, area);
}

fn get_usage_color(percent: u16) -> Color {
    match percent {
        0..=50 => LCARS_BLUE,
        51..=75 => LCARS_YELLOW,
        76..=90 => LCARS_PEACH,
        _ => LCARS_RED,
    }
}
