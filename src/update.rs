//! Checks GitHub Releases for new Notch updates.
//! Uses native Windows WinRT HTTP & JSON APIs without extra dependencies.

use std::sync::RwLock;
use windows::core::HSTRING;
use windows::Data::Json::JsonObject;
use windows::Foundation::Uri;
use windows::Web::Http::HttpClient;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub tag: String,
    pub url: String,
}

pub static UPDATE_AVAILABLE: RwLock<Option<UpdateInfo>> = RwLock::new(None);

pub fn check_latest_release() -> Option<UpdateInfo> {
    let client = HttpClient::new().ok()?;
    if let Ok(headers) = client.DefaultRequestHeaders() {
        let _ = headers.UserAgent().and_then(|ua| ua.TryParseAdd(&HSTRING::from("Notch-App")));
    }
    let uri = Uri::CreateUri(&HSTRING::from(
        "https://api.github.com/repos/KrisnaSantosa15/notch/releases/latest",
    ))
    .ok()?;
    let body = client.GetStringAsync(&uri).ok()?.get().ok()?.to_string();
    let json = JsonObject::Parse(&HSTRING::from(body)).ok()?;
    let tag = json.GetNamedString(&HSTRING::from("tag_name")).ok()?.to_string();
    let url = json.GetNamedString(&HSTRING::from("html_url")).ok()?.to_string();

    let clean_tag = tag.trim_start_matches('v');
    let clean_current = CURRENT_VERSION.trim_start_matches('v');

    if is_newer(clean_tag, clean_current) {
        Some(UpdateInfo { tag, url })
    } else {
        None
    }
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse_ver = |v: &str| -> Vec<u32> {
        v.split('.')
            .map(|s| s.split('-').next().unwrap_or("0"))
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    let l = parse_ver(latest);
    let c = parse_ver(current);
    l > c
}

pub fn check_in_background(on_found: impl FnOnce(UpdateInfo) + Send + 'static) {
    std::thread::spawn(move || {
        // Wait 3 seconds after startup before checking to keep launch instant
        std::thread::sleep(std::time::Duration::from_secs(3));
        if let Some(info) = check_latest_release() {
            if let Ok(mut lock) = UPDATE_AVAILABLE.write() {
                *lock = Some(info.clone());
            }
            on_found(info);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.4.0", "0.3.0"));
        assert!(is_newer("0.3.1", "0.3.0"));
        assert!(is_newer("1.0.0", "0.3.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
    }
}
