//! Load the LCARS palette from the active Omarchy desktop theme.
//!
//! Lookup order:
//! 1. `LCARS_PALETTE=tng` forces the built-in TNG colours.
//! 2. `<theme>/lcars.toml`: explicit LCARS roles (`primary = "#4fa3c7"`, ...).
//! 3. `<theme>/colors.toml`: roles derived from the theme's terminal palette.
//! 4. The built-in TNG palette.
//!
//! `<theme>` is the current Omarchy theme directory. Missing or malformed keys
//! fall back to the TNG colour for that role, so a partial file is fine.

use crate::lcars_widgets::Palette;
use ratatui::style::Color;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Where Omarchy keeps the applied theme (newest layout first).
fn theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(dir) = std::env::var("LCARS_THEME_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/state/omarchy/current/theme"));
        dirs.push(home.join(".config/omarchy/current/theme"));
    }
    dirs
}

/// The theme directory in use, if any.
pub fn current_theme_dir() -> Option<PathBuf> {
    theme_dirs().into_iter().find(|d| d.join("colors.toml").is_file())
}

/// Modification time of the theme's colour file; changes when the theme is switched.
pub fn theme_stamp(dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(dir.join("colors.toml")).and_then(|m| m.modified()).ok()
}

/// Resolve the palette for the current environment.
pub fn load() -> Palette {
    if std::env::var("LCARS_PALETTE").is_ok_and(|v| v.eq_ignore_ascii_case("tng")) {
        return Palette::TNG;
    }
    match current_theme_dir() {
        Some(dir) => from_theme_dir(&dir),
        None => Palette::TNG,
    }
}

/// Build a palette from a theme directory.
pub fn from_theme_dir(dir: &Path) -> Palette {
    if let Ok(text) = std::fs::read_to_string(dir.join("lcars.toml")) {
        return from_lcars_toml(&text);
    }
    match std::fs::read_to_string(dir.join("colors.toml")) {
        Ok(text) => from_colors_toml(&text),
        Err(_) => Palette::TNG,
    }
}

/// Explicit roles; each key matches a `Palette` field name.
pub fn from_lcars_toml(text: &str) -> Palette {
    let kv = parse_colors(text);
    let get = |k: &str, fallback: Color| kv.get(k).copied().unwrap_or(fallback);
    let t = Palette::TNG;
    Palette {
        primary: get("primary", t.primary),
        secondary: get("secondary", t.secondary),
        tertiary: get("tertiary", t.tertiary),
        soft: get("soft", t.soft),
        warm: get("warm", t.warm),
        quiet: get("quiet", t.quiet),
        value: get("value", t.value),
        caution: get("caution", t.caution),
        warning: get("warning", t.warning),
        alert: get("alert", t.alert),
    }
}

/// Derive roles from an Omarchy `colors.toml` (terminal palette).
pub fn from_colors_toml(text: &str) -> Palette {
    let kv = parse_colors(text);
    let pick = |keys: &[&str], fallback: Color| {
        keys.iter().find_map(|k| kv.get(*k).copied()).unwrap_or(fallback)
    };
    let t = Palette::TNG;
    Palette {
        primary: pick(&["accent", "orange"], t.primary),
        secondary: pick(&["blue"], t.secondary),
        tertiary: pick(&["magenta"], t.tertiary),
        soft: pick(&["foreground"], t.soft),
        warm: pick(&["cyan", "brown"], t.warm),
        quiet: pick(&["muted"], t.quiet),
        value: pick(&["light_foreground", "bright_foreground"], t.value),
        caution: pick(&["yellow"], t.caution),
        warning: pick(&["orange"], t.warning),
        alert: pick(&["red"], t.alert),
    }
}

/// Collect `key = "#rrggbb"` lines; anything else (comments, tables, other values) is skipped.
fn parse_colors(text: &str) -> HashMap<String, Color> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
            Some((key.trim().to_string(), parse_hex(value)?))
        })
        .collect()
}

fn parse_hex(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_rejects_junk() {
        assert_eq!(parse_hex("#4fa3c7"), Some(Color::Rgb(0x4f, 0xa3, 0xc7)));
        assert_eq!(parse_hex("4fa3c7"), None);
        assert_eq!(parse_hex("#4fa3c"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
        assert_eq!(parse_hex("rgba(586a6eaa)"), None);
    }

    #[test]
    fn derives_roles_from_omarchy_colors_toml() {
        let text = r##"
mode = "dark"
accent = "#4fc3e0"
foreground = "#c8ecf5"
light_foreground = "#d8f2f8"
muted = "#6e8287"
red = "#ee7766"
yellow = "#e8d080"
orange = "#ee9955"
cyan = "#4fc3e0"
blue = "#5b8fd5"
magenta = "#a0a0ee"
hyprland_active_border = "rgb(4fc3e0) rgb(6cc0b0) 45deg"
"##;
        let p = from_colors_toml(text);
        assert_eq!(p.primary, Color::Rgb(0x4f, 0xc3, 0xe0));
        assert_eq!(p.secondary, Color::Rgb(0x5b, 0x8f, 0xd5));
        assert_eq!(p.caution, Color::Rgb(0xe8, 0xd0, 0x80));
        assert_eq!(p.warning, Color::Rgb(0xee, 0x99, 0x55));
        assert_eq!(p.alert, Color::Rgb(0xee, 0x77, 0x66));
        assert_eq!(p.value, Color::Rgb(0xd8, 0xf2, 0xf8));
    }

    #[test]
    fn lcars_toml_overrides_and_missing_keys_fall_back_to_tng() {
        let p = from_lcars_toml("primary = \"#4fa3c7\"\n# a comment\nalert = \"not-a-colour\"\n");
        assert_eq!(p.primary, Color::Rgb(0x4f, 0xa3, 0xc7));
        assert_eq!(p.alert, Palette::TNG.alert);
        assert_eq!(p.secondary, Palette::TNG.secondary);
    }

    #[test]
    fn theme_dir_prefers_lcars_toml_over_colors_toml() {
        let dir = std::env::temp_dir().join(format!("lcars-palette-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("colors.toml"), "accent = \"#111111\"\n").unwrap();
        assert_eq!(from_theme_dir(&dir).primary, Color::Rgb(0x11, 0x11, 0x11));
        std::fs::write(dir.join("lcars.toml"), "primary = \"#222222\"\n").unwrap();
        assert_eq!(from_theme_dir(&dir).primary, Color::Rgb(0x22, 0x22, 0x22));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
