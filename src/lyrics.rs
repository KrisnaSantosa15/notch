//! Synced lyrics from LRCLIB (lrclib.net), free and without API keys.
//! HTTP and JSON use native Windows APIs (WinRT), adding no extra dependencies.

use windows::core::HSTRING;
use windows::Data::Json::{JsonArray, JsonObject, JsonValueType};
use windows::Web::Http::HttpClient;

pub struct Lyrics {
    /// (seconds, text); empty text = instrumental pause
    pub lines: Vec<(f64, String)>,
}

impl Lyrics {
    /// Line currently being sung at second `t`.
    pub fn index_at(&self, t: f64) -> Option<usize> {
        self.lines.partition_point(|(s, _)| *s <= t).checked_sub(1)
    }
}

pub fn fetch(title: &str, artist: &str, duration: f64) -> Option<Lyrics> {
    let (track, artist) = clean(title, artist);
    if track.is_empty() {
        return None;
    }
    let client = HttpClient::new().ok()?;
    if let Ok(headers) = client.DefaultRequestHeaders() {
        let _ = headers.UserAgent().and_then(|ua| ua.TryParseAdd(&HSTRING::from("Notch/0.3")));
    }
    let get = |url: String| crate::system::http_get(&client, &url);

    let base = "https://lrclib.net/api";
    let mut exact = format!("{base}/get?track_name={}&artist_name={}", enc(&track), enc(&artist));
    if duration > 0.0 {
        exact += &format!("&duration={}", duration.round());
    }
    if let Some(lrc) = get(exact).and_then(|body| synced(&JsonObject::Parse(&HSTRING::from(body)).ok()?)) {
        return Some(parse(&lrc));
    }

    // Fuzzy search: match duration if known, avoiding wrong songs.
    let searches = [
        format!("{base}/search?track_name={}&artist_name={}", enc(&track), enc(&artist)),
        format!("{base}/search?q={}", enc(&format!("{track} {artist}"))),
    ];
    for url in searches {
        let Some(body) = get(url) else { continue };
        let Ok(list) = JsonArray::Parse(&HSTRING::from(body)) else { continue };
        for i in 0..list.Size().unwrap_or(0) {
            let Ok(item) = list.GetObjectAt(i) else { continue };
            let d = item.GetNamedNumber(&HSTRING::from("duration")).unwrap_or(0.0);
            if duration > 0.0 && d > 0.0 && (d - duration).abs() > 6.0 {
                continue;
            }
            if let Some(lrc) = synced(&item) {
                return Some(parse(&lrc));
            }
        }
    }
    None
}

fn synced(obj: &JsonObject) -> Option<String> {
    let v = obj.GetNamedValue(&HSTRING::from("syncedLyrics")).ok()?;
    if v.ValueType().ok()? != JsonValueType::String {
        return None;
    }
    let s = v.GetString().ok()?.to_string();
    (!s.trim().is_empty()).then_some(s)
}

/// "[01:23.45]text" -> (83.45, "text"). A single line may have multiple timestamps.
pub fn parse(lrc: &str) -> Lyrics {
    let mut lines = Vec::new();
    for raw in lrc.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let Some(t) = parse_time(&stripped[..end]) else { break };
            times.push(t);
            rest = &stripped[end + 1..];
        }
        for t in times {
            lines.push((t, rest.trim().to_string()));
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    Lyrics { lines }
}

fn parse_time(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    Some(m.trim().parse::<f64>().ok()? * 60.0 + s.trim().parse::<f64>().ok()?)
}

/// Clean up titles from YouTube etc.: strip "(Official Video)", "[MV]", "| ...", "feat. ...",
/// and split "Artist - Title" if the artist is just a channel name.
fn clean(title: &str, artist: &str) -> (String, String) {
    let mut t = title.to_string();
    for (open, close) in [('(', ')'), ('[', ']'), ('【', '】')] {
        while let (Some(a), Some(b)) = (t.find(open), t.find(close)) {
            if b < a {
                break;
            }
            t.replace_range(a..b + close.len_utf8(), "");
        }
    }
    if let Some(i) = t.find('|') {
        t.truncate(i);
    }
    let mut artist = artist.trim_end_matches(" - Topic").replace("VEVO", "").trim().to_string();
    if let Some((left, right)) = t.split_once(" - ") {
        artist = left.trim().to_string();
        t = right.to_string();
    }
    let lower = t.to_lowercase();
    for cut in [" feat.", " feat ", " ft.", " ft ", " official", " lyrics", " lyric video", " audio"] {
        if let Some(i) = lower.find(cut) {
            t.truncate(i);
            break;
        }
    }
    (t.trim().to_string(), artist)
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out += &format!("%{b:02X}"),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lrc() {
        let l = parse("[ar:Someone]\n[00:12.50]Hello\n[00:10.00][00:20.00]Again\n[00:30.00]");
        let times: Vec<f64> = l.lines.iter().map(|x| x.0).collect();
        assert_eq!(times, vec![10.0, 12.5, 20.0, 30.0]);
        assert_eq!(l.lines[1].1, "Hello");
        assert_eq!(l.index_at(11.0), Some(0));
        assert_eq!(l.index_at(5.0), None);
    }

    /// Requires internet: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn fetches_from_lrclib() {
        let l = fetch("Tulus - Hati-Hati di Jalan (Official Music Video)", "TULUS", 0.0).expect("lyrics found");
        assert!(l.lines.len() > 10);
        println!("{} lines, sample: {:?}", l.lines.len(), &l.lines[3]);
    }

    #[test]
    fn cleans_youtube_titles() {
        assert_eq!(
            clean("Tulus - Hati-Hati di Jalan (Official Music Video)", "TULUS"),
            ("Hati-Hati di Jalan".into(), "Tulus".into())
        );
        assert_eq!(clean("Someone Like You | Live", "Adele - Topic"), ("Someone Like You".into(), "Adele".into()));
        assert_eq!(clean("Song feat. Other", "A"), ("Song".into(), "A".into()));
    }
}
