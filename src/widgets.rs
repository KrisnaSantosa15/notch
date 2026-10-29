//! "Notchlets" lightweight custom widget engine (%APPDATA%\Notch\widgets\*.toml).
//! Supports Open-Meteo weather (no API key), JSON price/metric tickers, event countdowns,
//! and local status/webhook badges with zero extra dependencies.

use crate::config::parse_hex_color;
use crate::media::unix_now;
use crate::system::http_get;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Data::Json::{JsonObject, JsonValueType};
use windows::Web::Http::HttpClient;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

#[derive(Clone, Debug, PartialEq)]
pub struct WidgetBadge {
    pub label: String,
    pub value: String,
    pub color: Option<[u8; 3]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetSpec {
    pub enabled: bool,
    /// "weather" | "ticker" | "countdown" | "static"
    pub kind: String,
    pub label: String,
    pub color: String,
    pub refresh_secs: u64,
    // weather (Open-Meteo)
    pub lat: f64,
    pub lon: f64,
    // ticker (any public JSON endpoint)
    pub url: String,
    pub json_path: String,
    pub prefix: String,
    pub suffix: String,
    pub decimals: Option<usize>,
    // countdown (target Unix timestamp in seconds)
    pub target_unix: i64,
    // static / local webhook value
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct WidgetFile {
    pub widget: Vec<WidgetSpec>,
}

const SAMPLE_WIDGETS_TEMPLATE: &str = r##"# Notchlets — Custom Widgets for Notch
# Rename this file to `my-widgets.toml` (ending in `.toml`) to activate!
# You can also drop multiple `.toml` files into this folder.

[[widget]]
enabled = true
kind = "weather"
label = "JKT"
lat = -6.2088
lon = 106.8456
color = "#3B82F6"
refresh_secs = 600

[[widget]]
enabled = false
kind = "ticker"
label = "BTC"
url = "https://api.coinbase.com/v2/prices/BTC-USD/spot"
json_path = "data.amount"
prefix = "$"
decimals = 0
color = "#F97316"
refresh_secs = 120

[[widget]]
enabled = false
kind = "countdown"
label = "F1 GP"
target_unix = 1793545200
color = "#EF4444"
refresh_secs = 60
"##;

pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("APPDATA").ok()?).join("Notch").join("widgets"))
}

pub fn ensure_dir_and_sample() -> Option<PathBuf> {
    let d = dir()?;
    if !d.exists() {
        let _ = std::fs::create_dir_all(&d);
    }
    let sample = d.join("README-examples.toml.sample");
    if !sample.exists() {
        let _ = std::fs::write(&sample, SAMPLE_WIDGETS_TEMPLATE);
    }
    Some(d)
}

pub fn parse_widget_toml(content: &str) -> Vec<WidgetSpec> {
    if let Ok(multi) = toml::from_str::<WidgetFile>(content) {
        if !multi.widget.is_empty() {
            return multi.widget.into_iter().filter(|w| w.enabled).collect();
        }
    }
    if let Ok(single) = toml::from_str::<WidgetSpec>(content) {
        if single.enabled && !single.kind.is_empty() {
            return vec![single];
        }
    }
    Vec::new()
}

fn load_specs() -> Vec<WidgetSpec> {
    let Some(d) = ensure_dir_and_sample() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(d) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("toml")))
        .collect();
    paths.sort();

    let mut specs = Vec::new();
    for p in paths {
        if let Ok(text) = std::fs::read_to_string(&p) {
            specs.extend(parse_widget_toml(&text));
        }
    }
    specs.truncate(3);
    specs
}

pub fn format_countdown(now_unix: i64, target_unix: i64) -> String {
    let diff = target_unix - now_unix;
    if diff <= 0 {
        return "Now".to_string();
    }
    let days = diff / 86_400;
    let hours = (diff % 86_400) / 3_600;
    let mins = (diff % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours:02}h")
    } else if hours > 0 {
        format!("{hours}h {mins:02}m")
    } else {
        format!("{mins}m")
    }
}

pub fn extract_json_path(json_text: &str, dot_path: &str) -> Option<String> {
    let mut obj = JsonObject::Parse(&windows::core::HSTRING::from(json_text)).ok()?;
    let parts: Vec<&str> = dot_path.split('.').filter(|s| !s.is_empty()).collect();
    for (idx, &key) in parts.iter().enumerate() {
        let hkey = windows::core::HSTRING::from(key);
        if !obj.HasKey(&hkey).ok()? {
            return None;
        }
        let val = obj.GetNamedValue(&hkey).ok()?;
        if idx + 1 == parts.len() {
            return match val.ValueType().ok()? {
                JsonValueType::String => Some(val.GetString().ok()?.to_string()),
                JsonValueType::Number => Some(format!("{}", val.GetNumber().ok()?)),
                JsonValueType::Boolean => Some(val.GetBoolean().ok()?.to_string()),
                _ => None,
            };
        } else {
            obj = val.GetObject().ok()?;
        }
    }
    None
}

fn format_number(raw: &str, decimals: Option<usize>) -> String {
    let Ok(num) = raw.trim().parse::<f64>() else {
        return raw.trim().to_string();
    };
    if let Some(d) = decimals {
        format!("{num:.d$}")
    } else if num.abs() >= 10_000.0 {
        format!("{:.1}K", num / 1_000.0)
    } else if (num - num.round()).abs() < 0.05 {
        format!("{:.0}", num)
    } else {
        format!("{num:.1}")
    }
}

fn evaluate(spec: &WidgetSpec, client: &HttpClient) -> Option<String> {
    match spec.kind.to_ascii_lowercase().as_str() {
        "static" => Some(spec.value.trim().to_string()).filter(|s| !s.is_empty()),
        "countdown" => Some(format_countdown(unix_now() as i64, spec.target_unix)),
        "weather" => {
            let url = format!(
                "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}&current_weather=true",
                spec.lat, spec.lon
            );
            let body = http_get(client, &url)?;
            let temp = extract_json_path(&body, "current_weather.temperature")?;
            let formatted = format_number(&temp, Some(0));
            Some(format!("{formatted}°"))
        }
        "ticker" => {
            if spec.url.is_empty() || spec.json_path.is_empty() {
                return None;
            }
            let body = http_get(client, &spec.url)?;
            let raw = extract_json_path(&body, &spec.json_path)?;
            let formatted = format_number(&raw, spec.decimals);
            Some(format!("{}{}{}", spec.prefix, formatted, spec.suffix))
        }
        _ => None,
    }
}

pub fn start(hwnd: isize, msg: u32) -> Arc<Mutex<Vec<WidgetBadge>>> {
    let slot = Arc::new(Mutex::new(Vec::new()));
    let slot_bg = slot.clone();
    std::thread::spawn(move || {
        let client = HttpClient::new().ok();
        let mut cache: HashMap<String, (Instant, String)> = HashMap::new();
        loop {
            let specs = load_specs();
            let mut badges = Vec::new();
            let now = Instant::now();
            for spec in &specs {
                let key = format!(
                    "{}:{}:{}:{}:{}:{}:{}",
                    spec.kind, spec.label, spec.lat, spec.lon, spec.url, spec.json_path, spec.value
                );
                let ttl = Duration::from_secs(spec.refresh_secs.clamp(15, 3600));
                let val = if let Some((fetched_at, cached)) = cache.get(&key) {
                    if now.duration_since(*fetched_at) < ttl {
                        Some(cached.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                let val = val.or_else(|| {
                    let c = client.as_ref()?;
                    let fresh = evaluate(spec, c)?;
                    cache.insert(key, (now, fresh.clone()));
                    Some(fresh)
                });
                if let Some(value) = val {
                    badges.push(WidgetBadge {
                        label: spec.label.clone(),
                        value,
                        color: parse_hex_color(&spec.color),
                    });
                }
            }

            let mut guard = slot_bg.lock().unwrap();
            if *guard != badges {
                *guard = badges;
                drop(guard);
                unsafe {
                    let _ = PostMessageW(HWND(hwnd as _), msg, WPARAM(0), LPARAM(0));
                }
            } else {
                drop(guard);
            }

            std::thread::sleep(Duration::from_secs(15));
        }
    });
    slot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_widget_toml() {
        let specs = parse_widget_toml(SAMPLE_WIDGETS_TEMPLATE);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].kind, "weather");
        assert_eq!(specs[0].label, "JKT");
    }

    #[test]
    fn test_format_countdown() {
        assert_eq!(format_countdown(1000, 900), "Now");
        assert_eq!(format_countdown(1000, 1000 + 3600 * 5 + 60 * 12), "5h 12m");
        assert_eq!(format_countdown(1000, 1000 + 86400 * 2 + 3600 * 4), "2d 04h");
    }

    #[test]
    fn test_extract_json_path() {
        let json = r#"{"data":{"amount":"68420.50"},"current_weather":{"temperature":28.4}}"#;
        assert_eq!(extract_json_path(json, "data.amount").as_deref(), Some("68420.50"));
        assert_eq!(extract_json_path(json, "current_weather.temperature").as_deref(), Some("28.4"));
        assert_eq!(extract_json_path(json, "missing.key"), None);
    }
}
