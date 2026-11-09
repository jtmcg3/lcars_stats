//! Custom LCARS-styled widgets for ratatui
//!
//! This module provides authentic Star Trek LCARS visual components including:
//! - Rounded corner panels
//! - Pill-shaped gauges
//! - Sidebar button panels
//! - LCARS color palette

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use std::sync::RwLock;

/// LCARS colour roles. The defaults are the authentic TNG palette; a theme can
/// replace them at runtime (see `crate::palette`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Frame, header and the CPU panel (TNG orange).
    pub primary: Color,
    /// Secondary frame colour and "healthy" readings (TNG blue).
    pub secondary: Color,
    /// Third frame colour (TNG purple).
    pub tertiary: Color,
    /// Soft accent for labels and notes (TNG peach).
    pub soft: Color,
    /// Earthy accent (TNG tan).
    pub warm: Color,
    /// Quiet accent (TNG lavender).
    pub quiet: Color,
    /// Numeric readouts (TNG yellow).
    pub value: Color,
    /// Usage/temperature band 2 (TNG yellow).
    pub caution: Color,
    /// Usage/temperature band 3 (TNG peach).
    pub warning: Color,
    /// Critical band and errors (TNG red).
    pub alert: Color,
}

impl Palette {
    pub const TNG: Palette = Palette {
        primary: Color::Rgb(255, 153, 0),
        secondary: Color::Rgb(153, 153, 255),
        tertiary: Color::Rgb(204, 153, 255),
        soft: Color::Rgb(255, 204, 153),
        warm: Color::Rgb(204, 153, 102),
        quiet: Color::Rgb(153, 153, 204),
        value: Color::Rgb(255, 255, 153),
        caution: Color::Rgb(255, 255, 153),
        warning: Color::Rgb(255, 204, 153),
        alert: Color::Rgb(204, 102, 102),
    };
}

static PALETTE: RwLock<Palette> = RwLock::new(Palette::TNG);

/// The active palette.
pub fn pal() -> Palette {
    *PALETTE.read().unwrap_or_else(|e| e.into_inner())
}

/// Replace the active palette (e.g. after the desktop theme changes).
pub fn set_palette(p: Palette) {
    *PALETTE.write().unwrap_or_else(|e| e.into_inner()) = p;
}

pub const LCARS_BG: Color = Color::Reset;

/// A sidebar button for the LCARS interface
#[derive(Debug, Clone)]
pub struct LcarsSidebarButton {
    pub label: String,
    pub color: Color,
    pub height: u16,
}

impl LcarsSidebarButton {
    pub fn new(label: &str, color: Color) -> Self {
        Self {
            label: label.to_uppercase(),
            color,
            height: 3,
        }
    }

    pub fn height(mut self, height: u16) -> Self {
        self.height = height;
        self
    }
}

impl Widget for LcarsSidebarButton {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 1 {
            return;
        }

        let h = area.height;

        // For pill shape, we draw rounded top and bottom
        // Top row: rounded corners
        if h >= 1 {
            let y = area.y;
            // Top-left rounded corner
            buf[(area.x, y)].set_char('╭').set_fg(self.color);
            // Top edge
            for x in 1..area.width.saturating_sub(1) {
                buf[(area.x + x, y)].set_char('─').set_fg(self.color);
            }
            // Top-right rounded corner
            if area.width > 1 {
                buf[(area.x + area.width - 1, y)].set_char('╮').set_fg(self.color);
            }
        }

        // Middle rows: filled with color
        for y_off in 1..h.saturating_sub(1) {
            let y = area.y + y_off;
            // Left edge
            buf[(area.x, y)].set_char('│').set_fg(self.color);
            // Fill middle
            for x in 1..area.width.saturating_sub(1) {
                buf[(area.x + x, y)].set_bg(self.color);
            }
            // Right edge
            if area.width > 1 {
                buf[(area.x + area.width - 1, y)].set_char('│').set_fg(self.color);
            }
        }

        // Bottom row: rounded corners
        if h >= 2 {
            let y = area.y + h - 1;
            // Bottom-left rounded corner
            buf[(area.x, y)].set_char('╰').set_fg(self.color);
            // Bottom edge
            for x in 1..area.width.saturating_sub(1) {
                buf[(area.x + x, y)].set_char('─').set_fg(self.color);
            }
            // Bottom-right rounded corner
            if area.width > 1 {
                buf[(area.x + area.width - 1, y)].set_char('╯').set_fg(self.color);
            }
        }

        // Draw the label centered
        if h >= 2 {
            let label_y = area.y + h / 2;
            let max_len = (area.width.saturating_sub(2)) as usize;
            let label: String = self.label.chars().take(max_len).collect();
            // Center the label
            let label_x = area.x + (area.width.saturating_sub(label.len() as u16)) / 2;

            if label_y > area.y && label_y < area.y + h - 1 {
                buf.set_string(
                    label_x,
                    label_y,
                    &label,
                    Style::default().fg(self.color).add_modifier(Modifier::BOLD),
                );
            }
        }
    }
}

/// A pill-shaped gauge for LCARS displays
#[derive(Debug, Clone)]
pub struct LcarsGauge {
    pub ratio: f64,
    pub label: String,
    pub color: Color,
    pub show_percentage: bool,
}

impl LcarsGauge {
    pub fn new(ratio: f64, color: Color) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
            label: String::new(),
            color,
            show_percentage: true,
        }
    }

    pub fn label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    pub fn show_percentage(mut self, show: bool) -> Self {
        self.show_percentage = show;
        self
    }
}

impl Widget for LcarsGauge {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 1 {
            return;
        }

        let gauge_width = area.width.saturating_sub(2);
        let filled_width = ((gauge_width as f64) * self.ratio) as u16;

        // Draw rounded left cap
        let y = area.y;
        buf[(area.x, y)]
            .set_char('(')
            .set_fg(self.color);

        // Draw filled portion
        for x in 1..=filled_width {
            if area.x + x < area.x + area.width - 1 {
                buf[(area.x + x, y)]
                    .set_char('█')
                    .set_fg(self.color);
            }
        }

        // Draw empty portion
        for x in (filled_width + 1)..gauge_width {
            if area.x + x < area.x + area.width - 1 {
                buf[(area.x + x, y)]
                    .set_char('░')
                    .set_fg(self.color);
            }
        }

        // Draw rounded right cap
        buf[(area.x + area.width - 1, y)]
            .set_char(')')
            .set_fg(self.color);

        // Draw label in center if provided
        if !self.label.is_empty() && area.width > 10 {
            let label_text = if self.show_percentage {
                format!("{} {:.0}%", self.label, self.ratio * 100.0)
            } else {
                self.label.clone()
            };
            let label_x = area.x + (area.width.saturating_sub(label_text.len() as u16)) / 2;
            buf.set_string(
                label_x,
                y,
                &label_text,
                Style::default().add_modifier(Modifier::BOLD),
            );
        }
    }
}

/// Draw LCARS-style header bar
pub fn draw_lcars_header(buf: &mut Buffer, area: Rect, title: &str, stardate: &str) {
    if area.height < 1 || area.width < 2 {
        return;
    }

    // Top border with rounded corners
    let y = area.y;
    buf[(area.x, y)].set_char('╭').set_fg(pal().primary);
    for x in 1..area.width - 1 {
        buf[(area.x + x, y)].set_char('─').set_fg(pal().primary);
    }
    buf[(area.x + area.width - 1, y)].set_char('╮').set_fg(pal().primary);

    if area.height < 2 {
        return;
    }

    // Content line with pill buttons
    let y = area.y + 1;
    buf[(area.x, y)].set_char('│').set_fg(pal().primary);

    // Orange pill
    for x in 1..4 {
        if area.x + x < area.x + area.width - 1 {
            buf[(area.x + x, y)]
                .set_char('█')
                .set_fg(pal().primary)
                .set_bg(pal().primary);
        }
    }

    // Title
    let title_start = 5;
    buf.set_string(
        area.x + title_start,
        y,
        title,
        Style::default().fg(pal().primary).add_modifier(Modifier::BOLD),
    );

    // Blue pill
    let blue_start = title_start + title.len() as u16 + 2;
    for x in 0..3 {
        if area.x + blue_start + x < area.x + area.width - 1 {
            buf[(area.x + blue_start + x, y)]
                .set_char('█')
                .set_fg(pal().secondary)
                .set_bg(pal().secondary);
        }
    }

    // Stardate
    let stardate_label = format!("  STARDATE {}", stardate);
    let stardate_start = blue_start + 2;
    if area.x + stardate_start + (stardate_label.len() as u16) < area.x + area.width {
        buf.set_string(
            area.x + stardate_start,
            y,
            &stardate_label,
            Style::default().fg(pal().secondary),
        );
    }

    buf[(area.x + area.width - 1, y)].set_char('│').set_fg(pal().primary);

    if area.height < 3 {
        return;
    }

    // Bottom border
    let y = area.y + 2;
    buf[(area.x, y)].set_char('├').set_fg(pal().primary);
    for x in 1..area.width - 1 {
        buf[(area.x + x, y)].set_char('─').set_fg(pal().primary);
    }
    buf[(area.x + area.width - 1, y)].set_char('┤').set_fg(pal().primary);
}

/// Draw LCARS-style footer bar
pub fn draw_lcars_footer(buf: &mut Buffer, area: Rect, items: &[(&str, Color)], status: &str) {
    if area.height < 1 || area.width < 2 {
        return;
    }

    let y = area.y;
    buf[(area.x, y)].set_char('├').set_fg(pal().primary);
    for x in 1..area.width - 1 {
        buf[(area.x + x, y)].set_char('─').set_fg(pal().primary);
    }
    buf[(area.x + area.width - 1, y)].set_char('┤').set_fg(pal().primary);

    if area.height < 2 {
        return;
    }

    let y = area.y + 1;
    buf[(area.x, y)].set_char('╰').set_fg(pal().primary);

    let mut x_pos = area.x + 1;
    for (label, color) in items {
        // Pill indicator
        buf[(x_pos, y)].set_char('█').set_fg(*color).set_bg(*color);
        x_pos += 1;

        // Label
        let label_str = format!(" {} ", label);
        buf.set_string(x_pos, y, &label_str, Style::default().fg(*color));
        x_pos += label_str.len() as u16;

        // Spacer
        x_pos += 1;
    }

    // Status on the right
    let status_text = format!("STATUS: {status}");
    let status_x = area
        .x
        .saturating_add(area.width)
        .saturating_sub(status_text.len() as u16)
        .saturating_sub(5);
    if status_x > x_pos {
        // Status pill
        for i in 0..3 {
            buf[(status_x - 4 + i, y)]
                .set_char('█')
                .set_fg(pal().secondary)
                .set_bg(pal().secondary);
        }
        buf.set_string(
            status_x,
            y,
            &status_text,
            Style::default().fg(pal().value).add_modifier(Modifier::BOLD),
        );
    }

    // Fill remaining with dashes
    for x in x_pos..status_x.saturating_sub(5) {
        buf[(x, y)].set_char('─').set_fg(pal().primary);
    }

    buf[(area.x + area.width - 1, y)].set_char('╯').set_fg(pal().primary);
}

/// Create a section title line in LCARS style
pub fn lcars_section_title(title: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled("◆ ", Style::default().fg(color)),
        Span::styled(
            title.to_uppercase(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ◆", Style::default().fg(color)),
    ])
}

/// Get color based on usage percentage
pub fn get_usage_color(percent: f64) -> Color {
    let p = pal();
    match percent as u16 {
        0..=50 => p.secondary,
        51..=75 => p.caution,
        76..=90 => p.warning,
        _ => p.alert,
    }
}

/// Get color for temperature
pub fn get_temp_color(temp_c: f64) -> Color {
    let p = pal();
    if temp_c > 95.0 {
        p.alert
    } else if temp_c > 85.0 {
        p.warning
    } else if temp_c > 70.0 {
        p.caution
    } else {
        p.secondary
    }
}
