//! Configuration management for Notch via %APPDATA%\Notch\config.toml.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub notch: NotchConfig,
    pub tabs: TabsConfig,
    pub theme: ThemeConfig,
    pub behavior: BehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotchConfig {
    /// Height of notch when closed in dp (default: 32.0)
    pub closed_height: f32,
    /// Width of notch when idle/closed in dp (default: 160.0)
    pub closed_width: f32,
    /// Width of notch when open in dp (default: 500.0)
    pub open_width: f32,
    /// Height of notch when open in dp (default: 180.0)
    pub open_height: f32,
    /// Corner radius when closed in dp (default: 12.0)
    pub corner_radius: f32,
    /// Corner radius when open in dp (default: 36.0)
    pub open_radius: f32,
}

impl Default for NotchConfig {
    fn default() -> Self {
        Self {
            closed_height: 32.0,
            closed_width: 160.0,
            open_width: 500.0,
            open_height: 180.0,
            corner_radius: 12.0,
            open_radius: 36.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TabsConfig {
    pub media: bool,
    pub focus: bool,
    pub shelf: bool,
    pub tasks: bool,
}

impl Default for TabsConfig {
    fn default() -> Self {
        Self {
            media: true,
            focus: true,
            shelf: true,
            tasks: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Accent color mode: "album_art" or "custom"
    pub accent_mode: String,
    /// Custom accent hex color (e.g. "#3B82F6" or "auto")
    pub custom_accent: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            accent_mode: "album_art".to_string(),
            custom_accent: "#3B82F6".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Auto-close delay in milliseconds when cursor leaves notch (default: 260)
    pub auto_close_delay_ms: u32,
    /// Hover delay in milliseconds before opening the closed notch (default: 120)
    pub hover_delay_ms: u32,
    /// Whether hovering mouse cursor over closed notch automatically opens it (default: true)
    pub hover_to_open: bool,
    /// Whether to automatically hide notch during fullscreen apps/games/videos (default: true)
    pub auto_hide_fullscreen: bool,
    /// Whether to play subtle tactile sound effects on interactions (default: true)
    pub sound_effects: bool,
    /// Global shortcut to toggle the notch open/closed (default: "Win+Alt+N")
    pub toggle_hotkey: String,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            auto_close_delay_ms: 260,
            hover_delay_ms: 120,
            hover_to_open: true,
            auto_hide_fullscreen: true,
            sound_effects: true,
            toggle_hotkey: "Win+Alt+N".to_string(),
        }
    }
}

pub fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("APPDATA").ok()?).join("Notch").join("config.toml"))
}

/// Parses a shortcut string like "Win+Alt+N" or "Ctrl+Space" into Win32 `(MOD_* | MOD_NOREPEAT, VK_*)`.
pub fn parse_hotkey(spec: &str) -> Option<(u32, u32)> {
    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_WIN: u32 = 0x0008;
    const MOD_NOREPEAT: u32 = 0x4000;

    let mut mods = 0u32;
    let mut vk = None;
    for part in spec.split('+').map(str::trim).filter(|s| !s.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "win" | "super" | "meta" => mods |= MOD_WIN,
            "alt" => mods |= MOD_ALT,
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "shift" => mods |= MOD_SHIFT,
            "space" => vk = Some(0x20),
            "tab" => vk = Some(0x09),
            k if k.len() == 1 => {
                let b = k.as_bytes()[0];
                if b.is_ascii_alphabetic() {
                    vk = Some(b.to_ascii_uppercase() as u32);
                } else if b.is_ascii_digit() {
                    vk = Some(b as u32);
                } else {
                    return None;
                }
            }
            k if k.starts_with('f') => {
                let n: u32 = k[1..].parse().ok()?;
                if (1..=12).contains(&n) {
                    vk = Some(0x70 + n - 1);
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if mods == 0 {
        return None;
    }
    Some((mods | MOD_NOREPEAT, vk?))
}

/// Parses hex color strings like "#3B82F6", "3B82F6", "#FFF", or "FFF".
pub fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let s = hex.trim().trim_start_matches('#');
    if !s.is_ascii() {
        return None; // byte slicing below would split a multi-byte char
    }
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r, g, b])
    } else if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&s[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&s[2..3], 16).ok()? * 17;
        Some([r, g, b])
    } else {
        None
    }
}

const DEFAULT_CONFIG_TEMPLATE: &str = r##"# Notch Configuration File (config.toml)
# Edit values and restart Notch or toggle to apply changes.

[notch]
# Height of the notch when closed (dp)
closed_height = 32.0

# Width of the notch when closed and idle (dp)
closed_width = 160.0

# Width of the notch when opened (dp)
open_width = 500.0

# Height of the notch when opened (dp)
open_height = 180.0

# Corner curvature radius when closed (dp)
corner_radius = 12.0

# Corner curvature radius when opened (dp)
open_radius = 36.0

[tabs]
# Enable or disable individual tabs in the open notch
media = true
focus = true
shelf = true
tasks = true

[theme]
# Accent color mode: "album_art" (adapts to music album art) or "custom"
accent_mode = "album_art"

# Custom accent hex color when accent_mode is "custom" (e.g. "#3B82F6", "#10B981", "#8B5CF6")
custom_accent = "#3B82F6"

[behavior]
# Duration in milliseconds to wait before closing notch when cursor leaves
auto_close_delay_ms = 260

# Duration in milliseconds cursor must rest on the closed notch before it opens
hover_delay_ms = 120

# Whether hovering mouse cursor over closed notch automatically opens it
hover_to_open = true

# Whether to automatically hide notch during fullscreen apps, games, or videos
auto_hide_fullscreen = true

# Whether to play subtle tactile sound effects on interactions (e.g. Pomodoro, shelf, tasks)
sound_effects = true

# Global keyboard shortcut to toggle the notch open or closed
toggle_hotkey = "Win+Alt+N"
"##;

pub fn load() -> Config {
    let Some(path) = path() else {
        return Config::default();
    };

    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, DEFAULT_CONFIG_TEMPLATE);
        return Config::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match toml::from_str::<Config>(&content) {
            Ok(mut cfg) => {
                cfg.sanitize();
                cfg
            }
            Err(e) => {
                // keep the broken file, otherwise the next in-app setting change would overwrite it
                eprintln!("Failed to parse config.toml: {e}. Using defaults, kept as config.toml.bad.");
                let _ = std::fs::rename(&path, path.with_extension("toml.bad"));
                Config::default()
            }
        },
        Err(_) => Config::default(),
    }
}

impl Config {
    /// Clamps hand-edited values to sane ranges (0, negative, or huge sizes break rendering).
    pub fn sanitize(&mut self) {
        let d = NotchConfig::default();
        let fix = |v: f32, def: f32, min: f32, max: f32| if v.is_finite() { v.clamp(min, max) } else { def };
        let n = &mut self.notch;
        n.closed_height = fix(n.closed_height, d.closed_height, 20.0, 80.0);
        n.closed_width = fix(n.closed_width, d.closed_width, 80.0, 600.0);
        n.open_width = fix(n.open_width, d.open_width, 300.0, 1000.0);
        n.open_height = fix(n.open_height, d.open_height, 120.0, 400.0);
        n.corner_radius = fix(n.corner_radius, d.corner_radius, 0.0, n.closed_height / 2.0);
        n.open_radius = fix(n.open_radius, d.open_radius, 0.0, 80.0);
        self.behavior.auto_close_delay_ms = self.behavior.auto_close_delay_ms.min(10_000);
        self.behavior.hover_delay_ms = self.behavior.hover_delay_ms.min(2_000);
    }

    pub fn save(&self) {
        if let Some(path) = path() {
            if let Ok(content) = toml::to_string_pretty(self) {
                crate::store::write_atomic(&path, &content);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_template() {
        let cfg: Config = toml::from_str(DEFAULT_CONFIG_TEMPLATE).expect("Template must be valid toml");
        assert_eq!(cfg.notch.closed_height, 32.0);
        assert_eq!(cfg.notch.open_width, 500.0);
        assert!(cfg.tabs.media);
        assert_eq!(cfg.theme.accent_mode, "album_art");
        assert_eq!(cfg.behavior.auto_close_delay_ms, 260);
        assert_eq!(cfg.behavior.hover_delay_ms, 120);
        assert!(cfg.behavior.auto_hide_fullscreen);
        assert!(cfg.behavior.sound_effects);
        assert_eq!(cfg.behavior.toggle_hotkey, "Win+Alt+N");
    }

    #[test]
    fn parses_hotkey_strings() {
        assert_eq!(parse_hotkey("Win+Alt+N"), Some((0x4009, b'N' as u32)));
        assert_eq!(parse_hotkey("ctrl + space"), Some((0x4002, 0x20)));
        assert_eq!(parse_hotkey("Alt+Shift+F1"), Some((0x4005, 0x70)));
        assert_eq!(parse_hotkey("N"), None);
        assert_eq!(parse_hotkey("Win+"), None);
    }

    #[test]
    fn disabled_tabs_can_be_configured() {
        let toml_str = r#"
            [tabs]
            media = false
            focus = false
            shelf = true
            tasks = true
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert!(!cfg.tabs.media);
        assert!(!cfg.tabs.focus);
        assert!(cfg.tabs.shelf);
        assert!(cfg.tabs.tasks);
    }

    #[test]
    fn parses_hex_colors() {
        assert_eq!(parse_hex_color("#3B82F6"), Some([0x3B, 0x82, 0xF6]));
        assert_eq!(parse_hex_color("3b82f6"), Some([0x3B, 0x82, 0xF6]));
        assert_eq!(parse_hex_color("#fff"), Some([255, 255, 255]));
        assert_eq!(parse_hex_color("invalid"), None);
        assert_eq!(parse_hex_color("aéé1"), None);
    }

    #[test]
    fn sanitize_clamps_sizes() {
        let mut cfg = Config::default();
        cfg.notch.open_width = 0.0;
        cfg.notch.open_height = f32::NAN;
        cfg.notch.closed_width = 1e9;
        cfg.sanitize();
        assert_eq!(cfg.notch.open_width, 300.0);
        assert_eq!(cfg.notch.open_height, 180.0);
        assert_eq!(cfg.notch.closed_width, 600.0);
    }

    #[test]
    fn config_serializes_and_deserializes() {
        let mut cfg = Config::default();
        cfg.theme.custom_accent = "#10B981".to_string();
        let s = toml::to_string_pretty(&cfg).expect("Serialization must succeed");
        let parsed: Config = toml::from_str(&s).expect("Deserialization must succeed");
        assert_eq!(parsed.theme.custom_accent, "#10B981");
    }
}
