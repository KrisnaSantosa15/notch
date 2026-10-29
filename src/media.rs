//! Reads the currently playing track from Windows (SMTC) on a background thread.
//! SMTC is the same Windows media transport control shown when adjusting volume,
//! so Spotify, YouTube in a browser, Apple Music, etc. are automatically detected.

use crate::lyrics::{self, Lyrics};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tiny_skia as sk;
use windows::core::Interface;
use windows::Foundation::TimeSpan;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapBounds, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat,
    BitmapTransform, ColorManagementMode, ExifOrientationMode,
};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Props,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::IRandomAccessStream;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Scaled-down album artwork, plus an accent color extracted from it.
pub struct Art {
    pub image: sk::Pixmap,
    pub accent: [u8; 3],
}

#[derive(Clone, Default)]
pub struct MediaInfo {
    pub has_session: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    /// Timeline start (seconds); needed for seeking.
    pub start: f64,
    pub duration: f64,
    pub position: f64,
    /// Time (unix seconds) when `position` was sampled, to estimate current position.
    pub position_at: f64,
    pub art: Option<Arc<Art>>,
}

impl MediaInfo {
    pub fn position_now(&self) -> f64 {
        let mut p = self.position;
        if self.playing {
            p += (unix_now() - self.position_at).max(0.0);
        }
        if self.duration > 0.0 {
            p = p.min(self.duration);
        }
        p.max(0.0)
    }

    pub fn key(&self) -> String {
        format!("{}\u{1}{}", self.title, self.artist)
    }
}

#[derive(Clone)]
pub enum LyricsState {
    Loading,
    Found(Arc<Lyrics>),
    Missing,
}

/// Lyrics for track `key`. Populated by the search thread, read by the UI.
#[derive(Clone)]
pub struct LyricsSlot {
    pub key: String,
    pub state: LyricsState,
}

pub enum Cmd {
    Toggle,
    Next,
    Prev,
    /// Seek to the n-th second of the track.
    Seek(f64),
    /// Re-read immediately (e.g. after enabling lyrics).
    Refresh,
}

pub struct Handle {
    pub state: Arc<Mutex<MediaInfo>>,
    pub lyrics: Arc<Mutex<LyricsSlot>>,
    lyrics_on: Arc<AtomicBool>,
    tx: mpsc::Sender<Cmd>,
}

impl Handle {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn set_lyrics(&self, on: bool) {
        self.lyrics_on.store(on, Ordering::Relaxed);
        self.send(Cmd::Refresh);
    }
}

#[derive(Default)]
struct ArtCache {
    key: String,
    art: Option<Arc<Art>>,
    tries: u8,
}

/// Starts the media thread. Sends `msg` to `hwnd` on any meaningful change.
pub fn start(hwnd: isize, msg: u32, lyrics_on: bool) -> Handle {
    let state = Arc::new(Mutex::new(MediaInfo::default()));
    let slot = Arc::new(Mutex::new(LyricsSlot { key: String::new(), state: LyricsState::Missing }));
    let lyrics_on = Arc::new(AtomicBool::new(lyrics_on));
    let (tx, rx) = mpsc::channel();
    let handle = Handle { state: state.clone(), lyrics: slot.clone(), lyrics_on: lyrics_on.clone(), tx };
    let notify = move || unsafe {
        let _ = PostMessageW(HWND(hwnd as _), msg, WPARAM(0), LPARAM(0));
    };

    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut manager: Option<Manager> = None;
        let mut cache = ArtCache::default();
        loop {
            let cmd = rx.recv_timeout(Duration::from_secs(1));
            if let Err(mpsc::RecvTimeoutError::Disconnected) = cmd {
                break;
            }
            if manager.is_none() {
                manager = Manager::RequestAsync().and_then(|op| op.get()).ok();
            }
            let Some(m) = &manager else { continue };

            if let (Ok(cmd), Ok(session)) = (cmd, m.GetCurrentSession()) {
                let start = state.lock().unwrap().start;
                if run(&session, cmd, start).unwrap_or(false) {
                    // give media player app time to update its status
                    std::thread::sleep(Duration::from_millis(250));
                }
            }

            let info = fake_media().unwrap_or_else(|| read(m, &mut cache).unwrap_or_default());
            let changed = changed(&state.lock().unwrap(), &info);

            // Lyrics are queried once per track on a separate thread to keep SMTC responsive.
            if lyrics_on.load(Ordering::Relaxed) && info.has_session {
                let key = info.key();
                let mut s = slot.lock().unwrap();
                if s.key != key {
                    *s = LyricsSlot { key: key.clone(), state: LyricsState::Loading };
                    let (slot, title, artist, duration) = (slot.clone(), info.title.clone(), info.artist.clone(), info.duration);
                    std::thread::spawn(move || {
                        let found = lyrics::fetch(&title, &artist, duration);
                        let mut s = slot.lock().unwrap();
                        if s.key == key {
                            s.state = found.map_or(LyricsState::Missing, |l| LyricsState::Found(Arc::new(l)));
                            drop(s);
                            notify();
                        }
                    });
                    drop(s);
                    notify();
                }
            }

            *state.lock().unwrap() = info;
            if changed {
                notify();
            }
        }
    });

    handle
}

/// debug: NOTCH_FAKE_MEDIA="title|artist|duration|position" simulates a playing track
/// (to test music and lyrics UI without playing audio).
fn fake_media() -> Option<MediaInfo> {
    static STARTED: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    let spec = std::env::var("NOTCH_FAKE_MEDIA").ok()?;
    let parts: Vec<&str> = spec.split('|').collect();
    let num = |i: usize| parts.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    Some(MediaInfo {
        has_session: true,
        playing: true,
        title: parts.first()?.to_string(),
        artist: parts.get(1).unwrap_or(&"").to_string(),
        duration: num(2),
        position: num(3),
        position_at: *STARTED.get_or_init(unix_now),
        ..Default::default()
    })
}

/// Executes command; returns `true` if the player needs time to respond.
fn run(s: &Session, cmd: Cmd, start: f64) -> windows::core::Result<bool> {
    let op = match cmd {
        Cmd::Toggle => s.TryTogglePlayPauseAsync()?,
        Cmd::Next => s.TrySkipNextAsync()?,
        Cmd::Prev => s.TrySkipPreviousAsync()?,
        Cmd::Seek(t) => s.TryChangePlaybackPositionAsync(((start + t) * 1e7) as i64)?,
        Cmd::Refresh => return Ok(false),
    };
    op.get()?;
    Ok(true)
}

fn read(m: &Manager, cache: &mut ArtCache) -> windows::core::Result<MediaInfo> {
    let s = m.GetCurrentSession()?;
    let props = s.TryGetMediaPropertiesAsync()?.get()?;
    let playback = s.GetPlaybackInfo()?;
    let tl = s.GetTimelineProperties()?;

    let secs = |t: TimeSpan| t.Duration as f64 / 1e7;
    let start = secs(tl.StartTime()?);
    let end = secs(tl.EndTime()?);
    // Windows DateTime = number of 100-ns intervals since 1601
    let updated = tl.LastUpdatedTime()?.UniversalTime as f64 / 1e7 - 11_644_473_600.0;
    let now = unix_now();
    let title = props.Title()?.to_string();
    let artist = props.Artist()?.to_string();

    // Artwork is fetched once per track; retried a few times because players
    // often publish metadata first and artwork slightly later.
    let key = format!("{title}\u{1}{artist}");
    if key != cache.key {
        *cache = ArtCache { key, ..Default::default() };
    }
    if cache.art.is_none() && cache.tries < 4 {
        cache.tries += 1;
        cache.art = fetch_art(&props).ok().map(Arc::new);
    }

    Ok(MediaInfo {
        has_session: true,
        playing: playback.PlaybackStatus()? == Status::Playing,
        title,
        artist,
        start,
        duration: (end - start).max(0.0),
        position: secs(tl.Position()?) - start,
        position_at: if updated > 0.0 && updated <= now { updated } else { now },
        art: cache.art.clone(),
    })
}

/// Decodes artwork using Windows built-in decoder, cropped to a 96 px square.
fn fetch_art(props: &Props) -> windows::core::Result<Art> {
    const S: u32 = 96;
    let stream = props.Thumbnail()?.OpenReadAsync()?.get()?;
    let decoder = BitmapDecoder::CreateAsync(&stream.cast::<IRandomAccessStream>()?)?.get()?;
    let (w, h) = (decoder.PixelWidth()?.max(1), decoder.PixelHeight()?.max(1));
    let scale = S as f32 / w.min(h) as f32;
    let sw = ((w as f32 * scale).round() as u32).max(S);
    let sh = ((h as f32 * scale).round() as u32).max(S);

    let t = BitmapTransform::new()?;
    t.SetScaledWidth(sw)?;
    t.SetScaledHeight(sh)?;
    t.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    t.SetBounds(BitmapBounds { X: (sw - S) / 2, Y: (sh - S) / 2, Width: S, Height: S })?;
    let pixels = decoder
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            &t,
            ExifOrientationMode::RespectExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )?
        .get()?
        .DetachPixelData()?;

    let mut data = pixels.to_vec();
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2); // BGRA -> RGBA
    }
    let accent = accent_of(&data);
    let image = sk::IntSize::from_wh(S, S)
        .and_then(|size| sk::Pixmap::from_vec(data, size))
        .ok_or_else(windows::core::Error::empty)?;
    Ok(Art { image, accent })
}

/// Most vibrant color from artwork, brightened for readability on dark backgrounds.
fn accent_of(rgba: &[u8]) -> [u8; 3] {
    let (mut sum, mut total) = ([0f32; 3], 0f32);
    for px in rgba.as_chunks::<4>().0 {
        let [r, g, b] = [px[0] as f32, px[1] as f32, px[2] as f32];
        let sat = r.max(g).max(b) - r.min(g).min(b);
        let weight = sat * sat + 1.0;
        sum[0] += r * weight;
        sum[1] += g * weight;
        sum[2] += b * weight;
        total += weight;
    }
    let c = sum.map(|v| v / total);
    let max = c[0].max(c[1]).max(c[2]).max(1.0);
    let min = c[0].min(c[1]).min(c[2]);
    if (max - min) / max < 0.18 {
        return [245, 245, 247]; // nearly greyscale artwork: fall back to white
    }
    let k = 235.0 / max;
    c.map(|v| (v * k).min(255.0) as u8)
}

/// Only wakes the UI if something actually changed (not just time ticking).
fn changed(old: &MediaInfo, new: &MediaInfo) -> bool {
    let art_changed = match (&old.art, &new.art) {
        (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
        (None, None) => false,
        _ => true,
    };
    art_changed
        || old.has_session != new.has_session
        || old.playing != new.playing
        || old.title != new.title
        || old.artist != new.artist
        || (old.duration - new.duration).abs() > 0.5
        || (old.position_now() - new.position_now()).abs() > 1.5
}

pub fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
