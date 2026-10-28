//! Configuration management for Notch via %APPDATA%\Notch\config.toml.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub notch: NotchConfig,
    pub tabs: TabsConfig,
    pub theme: ThemeConfig,
    pub behavior: BehaviorConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notch: NotchConfig::default(),
            tabs: TabsConfig::default(),
            theme: ThemeConfig::default(),
            behavior: BehaviorConfig::default(),
        }
    }
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
    /// Whether hovering mouse cursor over closed notch automatically opens it (default: true)
    pub hover_to_open: bool,
    /// Whether to automatically hide notch during fullscreen apps/games/videos (default: true)
    pub auto_hide_fullscreen: bool,
    /// Whether to play subtle tactile sound effects on interactions (default: true)
    pub sound_effects: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            auto_close_delay_ms: 260,
            hover_to_open: true,
            auto_hide_fullscreen: true,
            sound_effects: true,
        }
    }
}

pub fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("APPDATA").ok()?).join("Notch").join("config.toml"))
}

/// Parses hex color strings like "#3B82F6", "3B82F6", "#FFF", or "FFF".
pub fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let s = hex.trim().trim_start_matches('#');
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

# Whether hovering mouse cursor over closed notch automatically opens it
hover_to_open = true

# Whether to automatically hide notch during fullscreen apps, games, or videos
auto_hide_fullscreen = true

# Whether to play subtle tactile sound effects on interactions (e.g. Pomodoro, shelf, tasks)
sound_effects = true
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
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to parse config.toml: {e}. Using defaults.");
                Config::default()
            }
        },
        Err(_) => Config::default(),
    }
}

impl Config {
    pub fn save(&self) {
        if let Some(path) = path() {
            if let Ok(content) = toml::to_string_pretty(self) {
                let _ = std::fs::write(path, content);
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
        assert!(cfg.behavior.auto_hide_fullscreen);
        assert!(cfg.behavior.sound_effects);
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
