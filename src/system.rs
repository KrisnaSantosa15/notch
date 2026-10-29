//! Small Windows bridge: volume, Caps Lock, battery, fullscreen detection, and autostart.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, AUDIO_VOLUME_NOTIFICATION_DATA, IMMDeviceEnumerator, MMDeviceEnumerator,
    PlaySoundW, SND_ASYNC, SND_MEMORY,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const KEY_VOLUME: usize = 1;

static FADING_VOLUME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn endpoint() -> Option<IAudioEndpointVolume> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        device.Activate(CLSCTX_ALL, None).ok()
    }
}

/// Master volume (0..1) and mute status.
pub fn volume() -> Option<(f32, bool)> {
    let v = endpoint()?;
    unsafe { Some((v.GetMasterVolumeLevelScalar().ok()?, v.GetMute().ok()?.as_bool())) }
}

pub fn nudge_volume(delta: f32) -> Option<(f32, bool)> {
    let v = endpoint()?;
    unsafe {
        let level = (v.GetMasterVolumeLevelScalar().ok()? + delta).clamp(0.0, 1.0);
        v.SetMasterVolumeLevelScalar(level, std::ptr::null()).ok()?;
        if delta > 0.0 {
            let _ = v.SetMute(false, std::ptr::null());
        }
    }
    volume()
}

pub(crate) fn fade_out_curve(t: f32) -> f32 {
    let u = (1.0 - t).clamp(0.0, 1.0);
    u * u
}

pub(crate) fn fade_in_curve(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct FadeGuard<'a>(&'a IAudioEndpointVolume, f32);

impl Drop for FadeGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.SetMasterVolumeLevelScalar(self.1, std::ptr::null());
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
        FADING_VOLUME.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Smoothly fades master audio out (~160 ms) before pausing, or fades in (~200 ms) after starting playback,
/// always restoring the user's original volume level afterward without triggering the Volume HUD.
pub fn fade_toggle<F: FnOnce() -> windows::core::Result<()>>(
    currently_playing: bool,
    toggle: F,
) -> windows::core::Result<()> {
    let Some(ep) = endpoint() else {
        return toggle();
    };
    let (orig, muted) = unsafe {
        let v = ep.GetMasterVolumeLevelScalar().unwrap_or(0.0);
        let m = ep.GetMute().map(|b| b.as_bool()).unwrap_or(false);
        (v, m)
    };
    if muted || orig <= 0.02 {
        return toggle();
    }

    FADING_VOLUME.store(true, std::sync::atomic::Ordering::Relaxed);
    let _guard = FadeGuard(&ep, orig);

    if currently_playing {
        // Fade out gently over ~160 ms before pausing
        const STEPS: u32 = 8;
        for step in 1..=STEPS {
            let level = orig * fade_out_curve(step as f32 / STEPS as f32);
            unsafe {
                let _ = ep.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        toggle()?;
        std::thread::sleep(std::time::Duration::from_millis(55));
    } else {
        // Start at 0 volume and swell up smoothly over ~200 ms
        unsafe {
            let _ = ep.SetMasterVolumeLevelScalar(0.0, std::ptr::null());
        }
        toggle()?;
        std::thread::sleep(std::time::Duration::from_millis(25));
        const STEPS: u32 = 10;
        for step in 1..=STEPS {
            let level = orig * fade_in_curve(step as f32 / STEPS as f32);
            unsafe {
                let _ = ep.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    Ok(())
}

pub fn caps_lock() -> bool {
    unsafe { GetKeyState(VK_CAPITAL.0 as i32) & 1 == 1 }
}

/// (is_charging, battery_percentage). `None` if the system has no battery.
pub fn battery() -> Option<(bool, u8)> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some((s.ACLineStatus == 1, s.BatteryLifePercent))
}

/// True when a game, fullscreen video, or presentation is active: notch steps aside.
pub fn fullscreen(ignore1: HWND, ignore2: HWND) -> bool {
    let state = unsafe { SHQueryUserNotificationState() };
    if let Ok(s) = state {
        if s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE {
            return true;
        }
    }
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null()
            || fg == GetDesktopWindow()
            || fg == GetShellWindow()
            || fg == ignore1
            || fg == ignore2
        {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let hmon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        // only a fullscreen app on the notch's own monitor should hide it
        if hmon.0.is_null() || hmon != MonitorFromWindow(ignore1, MONITOR_DEFAULTTOPRIMARY) {
            return false;
        }
        let mut minfo = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut minfo).as_bool() {
            return false;
        }
        let mr = minfo.rcMonitor;
        let style = GetWindowLongW(fg, GWL_STYLE) as u32;
        let has_caption = style & WS_CAPTION.0 == WS_CAPTION.0;
        r.left <= mr.left && r.top <= mr.top && r.right >= mr.right && r.bottom >= mr.bottom && !has_caption
    }
}

/// True when the active foreground window on the notch's monitor is maximized (e.g. browser with top tabs).
pub fn maximized_on_monitor(ignore1: HWND, ignore2: HWND) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null()
            || fg == GetDesktopWindow()
            || fg == GetShellWindow()
            || fg == ignore1
            || fg == ignore2
        {
            return false;
        }
        if !IsZoomed(fg).as_bool() {
            return false;
        }
        let hmon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        !hmon.0.is_null() && hmon == MonitorFromWindow(ignore1, MONITOR_DEFAULTTOPRIMARY)
    }
}

/// Returns the monitor bounds (`rcMonitor`) for `"primary"`, `"active"` (foreground window), or `"cursor"`.
pub fn target_monitor_rect(hwnd: HWND, mode: &str) -> Option<RECT> {
    unsafe {
        let hmon = if mode.eq_ignore_ascii_case("cursor") {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY)
        } else if mode.eq_ignore_ascii_case("active") {
            let fg = GetForegroundWindow();
            if !fg.0.is_null() && fg != hwnd {
                MonitorFromWindow(fg, MONITOR_DEFAULTTOPRIMARY)
            } else {
                MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)
            }
        } else {
            MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)
        };
        let mut minfo = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(hmon, &mut minfo).as_bool().then_some(minfo.rcMonitor)
    }
}

// ---- Volume notifications via Windows Core Audio (no keyboard hooks needed) -----------

#[windows::core::implement(IAudioEndpointVolumeCallback)]
struct VolumeNotification(HWND, u32);

impl IAudioEndpointVolumeCallback_Impl for VolumeNotification_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if FADING_VOLUME.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        unsafe {
            let _ = PostMessageW(self.0, self.1, WPARAM(KEY_VOLUME), LPARAM(0));
        }
        Ok(())
    }
}

pub fn register_volume_notify(hwnd: HWND, msg: u32) {
    if let Some(ep) = endpoint() {
        let cb: IAudioEndpointVolumeCallback = VolumeNotification(hwnd, msg).into();
        unsafe {
            let _ = ep.RegisterControlChangeNotify(&cb);
        }
    }
}

// ---- Glass: native DWM blur on the backdrop window ---------------------------------------
// Note: Modern Windows DWM ignores GDI regions (SetWindowRgn) when ACCENT_ENABLE_BLURBEHIND
// is active, which causes unclipped sharp rectangular frosted grey halos on rounded notch corners.
// To ensure perfectly smooth, anti-aliased curves without rectangular artifacts, the backdrop
// window is kept hidden, and Notch renders its deep dark glass via per-pixel alpha on the main window.

pub fn enable_blur(_hwnd: HWND) {}

/// Blur area in screen pixels: (left, top, width, height, bottom corner radius). `None` = hide.
/// Kept hidden to prevent rectangular DWM composition halos from protruding past curved corners.
pub fn set_blur_shape(hwnd: HWND, _shape: Option<(i32, i32, i32, i32, i32)>) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

// ---- Keyboard layout --------------------------------------------------------------------

/// (active window, keyboard layout id)
pub fn keyboard_layout() -> (isize, u16) {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        let hkl = GetKeyboardLayout(thread);
        (fg.0 as isize, (hkl.0 as usize & 0xffff) as u16)
    }
}

/// ("EN", "English") for a language id.
pub fn language_name(lang: u16) -> (String, String) {
    use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName, LOCALE_SENGLISHLANGUAGENAME};
    unsafe {
        let mut name = [0u16; 85];
        let n = LCIDToLocaleName(lang as u32, Some(&mut name), 0);
        if n <= 1 {
            return ("??".into(), "Keyboard".into());
        }
        let locale = String::from_utf16_lossy(&name[..n as usize - 1]);
        let code = locale.split('-').next().unwrap_or("").to_uppercase();
        let mut buf = [0u16; 85];
        let m = GetLocaleInfoEx(PCWSTR(name.as_ptr()), LOCALE_SENGLISHLANGUAGENAME, Some(&mut buf));
        let full = if m > 1 { String::from_utf16_lossy(&buf[..m as usize - 1]) } else { locale.clone() };
        (code, full)
    }
}

// ---- Clipboard (for Ctrl+V in task input) -----------------------------------------------

pub fn clipboard_text(hwnd: HWND) -> Option<String> {
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    unsafe {
        OpenClipboard(hwnd).ok()?;
        let text = GetClipboardData(CF_UNICODETEXT.0 as u32).ok().and_then(|h| {
            let global = windows::Win32::Foundation::HGLOBAL(h.0);
            let max_u16 = GlobalSize(global) / 2;
            if max_u16 == 0 {
                return None;
            }
            let ptr = GlobalLock(global) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let slice = std::slice::from_raw_parts(ptr, max_u16);
            let len = slice.iter().position(|&c| c == 0).unwrap_or(max_u16);
            let s = String::from_utf16_lossy(&slice[..len]);
            let _ = GlobalUnlock(global);
            Some(s)
        });
        let _ = CloseClipboard();
        text
    }
}

pub fn set_clipboard_text(hwnd: HWND, text: &str) -> bool {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    unsafe {
        if OpenClipboard(hwnd).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let ok = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2).ok().is_some_and(|h| {
            let ptr = GlobalLock(h) as *mut u16;
            if ptr.is_null() {
                return false;
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
            let _ = GlobalUnlock(h);
            SetClipboardData(CF_UNICODETEXT.0 as u32, HANDLE(h.0)).is_ok()
        });
        let _ = CloseClipboard();
        ok
    }
}

pub fn set_clipboard_listener(hwnd: HWND, enable: bool) {
    use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener};
    unsafe {
        if enable {
            let _ = AddClipboardFormatListener(hwnd);
        } else {
            let _ = RemoveClipboardFormatListener(hwnd);
        }
    }
}

// ---- System Stats (CPU & RAM) ------------------------------------------------------------

pub fn ram_usage() -> u8 {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut status).is_ok() {
            status.dwMemoryLoad.min(100) as u8
        } else {
            0
        }
    }
}

pub fn cpu_usage() -> u8 {
    use std::sync::Mutex;
    use windows::Win32::System::Threading::GetSystemTimes;
    static PREV_TIMES: Mutex<Option<(u64, u64, u64)>> = Mutex::new(None);

    unsafe {
        let mut idle = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_err() {
            return 0;
        }

        let to_u64 = |ft: FILETIME| ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
        let cur_idle = to_u64(idle);
        let cur_kernel = to_u64(kernel);
        let cur_user = to_u64(user);

        let mut lock = PREV_TIMES.lock().unwrap();
        if let Some((prev_idle, prev_kernel, prev_user)) = *lock {
            let d_idle = cur_idle.saturating_sub(prev_idle);
            let d_kernel = cur_kernel.saturating_sub(prev_kernel);
            let d_user = cur_user.saturating_sub(prev_user);
            let total = d_kernel + d_user;

            *lock = Some((cur_idle, cur_kernel, cur_user));
            if total > 0 && d_idle <= total {
                let usage = ((total - d_idle) as f64 / total as f64 * 100.0).round();
                return (usage as u8).min(100);
            }
        } else {
            *lock = Some((cur_idle, cur_kernel, cur_user));
        }
        0
    }
}

// ---- Privacy Indicator (Camera & Microphone In-Use) --------------------------------------

fn is_device_in_use(device: &str) -> bool {
    unsafe {
        let base_path = format!("Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{device}");
        let base_w: Vec<u16> = base_path.encode_utf16().chain([0]).collect();
        let mut base_key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(base_w.as_ptr()), 0, KEY_READ, &mut base_key).is_err() {
            return false;
        }

        let mut in_use = check_key_in_use(base_key);
        if !in_use {
            // Also check NonPackaged subkey for Win32 apps
            let mut non_packaged = HKEY::default();
            if RegOpenKeyExW(base_key, windows::core::w!("NonPackaged"), 0, KEY_READ, &mut non_packaged).is_ok() {
                in_use = check_key_in_use(non_packaged);
                let _ = RegCloseKey(non_packaged);
            }
        }
        let _ = RegCloseKey(base_key);
        in_use
    }
}

unsafe fn check_key_in_use(key: HKEY) -> bool {
    use windows::core::PWSTR;
    let mut index = 0;
    let mut name_buf = [0u16; 256];
    loop {
        let mut name_len = name_buf.len() as u32;
        if RegEnumKeyExW(
            key,
            index,
            PWSTR(name_buf.as_mut_ptr()),
            &mut name_len,
            None,
            PWSTR::null(),
            None,
            None,
        )
        .is_err()
        {
            break;
        }
        index += 1;
        name_buf[name_len as usize] = 0;
        let sub_name = PCWSTR(name_buf.as_ptr());
        let mut stop: u64 = 1;
        let mut start: u64 = 0;
        let mut size = std::mem::size_of::<u64>() as u32;

        let res_stop = RegGetValueW(
            key,
            sub_name,
            windows::core::w!("LastUsedTimeStop"),
            RRF_RT_REG_QWORD,
            None,
            Some(&mut stop as *mut _ as *mut _),
            Some(&mut size),
        );
        let mut size_start = std::mem::size_of::<u64>() as u32;
        let res_start = RegGetValueW(
            key,
            sub_name,
            windows::core::w!("LastUsedTimeStart"),
            RRF_RT_REG_QWORD,
            None,
            Some(&mut start as *mut _ as *mut _),
            Some(&mut size_start),
        );

        // An app is currently using the device if it has started and either never stopped (stop == 0)
        // or the start timestamp is newer than the stop timestamp (start > stop).
        if res_start.is_ok() && start > 0 && (res_stop.is_err() || stop == 0 || start > stop) {
            return true;
        }
    }
    false
}

/// Returns (camera_in_use, microphone_in_use)
pub fn privacy_status() -> (bool, bool) {
    (is_device_in_use("webcam"), is_device_in_use("microphone"))
}

// ---- Autostart with Windows --------------------------------------------------------------

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: PCWSTR = w!("Notch");

pub fn autostart() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME, RRF_RT_REG_SZ, None, None, None).is_ok() }
}

pub fn set_autostart(on: bool) {
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME);
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let value: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain([0]).collect();
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_NAME,
            REG_SZ.0,
            Some(value.as_ptr() as *const _),
            (value.len() * 2) as u32,
        );
    }
}

// ---- Tactile sound effects (in-memory WAV via PlaySoundW) --------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    Pop,
    Chime,
    Drop,
    Tick,
}

static POP_WAV: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
static CHIME_WAV: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
static DROP_WAV: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
static TICK_WAV: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

fn generate_wav<F: Fn(f32) -> f32>(duration_ms: u32, sample_rate: u32, generator: F) -> Vec<u8> {
    let num_samples = (sample_rate * duration_ms / 1000) as usize;
    let byte_rate = sample_rate * 2;
    let data_len = (num_samples * 2) as u32;
    let file_len = 36 + data_len;
    let mut data = Vec::with_capacity(44 + num_samples * 2);

    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&file_len.to_le_bytes());
    data.extend_from_slice(b"WAVE");

    data.extend_from_slice(b"fmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&sample_rate.to_le_bytes());
    data.extend_from_slice(&byte_rate.to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());

    data.extend_from_slice(b"data");
    data.extend_from_slice(&data_len.to_le_bytes());

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let s = generator(t).clamp(-1.0, 1.0);
        let sample = (s * 32767.0) as i16;
        data.extend_from_slice(&sample.to_le_bytes());
    }
    data
}

pub fn play_sound(sound: Sound) {
    let wav = match sound {
        Sound::Pop => POP_WAV.get_or_init(|| {
            generate_wav(22, 22050, |t| {
                let env = (-t * 190.0).exp();
                (t * (2400.0 - t * 40000.0) * std::f32::consts::TAU).sin() * env * 0.45
            })
        }),
        Sound::Chime => CHIME_WAV.get_or_init(|| {
            generate_wav(120, 22050, |t| {
                let env = (-t * 30.0).exp();
                ((t * 880.0 * std::f32::consts::TAU).sin() * 0.6
                    + (t * 1760.0 * std::f32::consts::TAU).sin() * 0.4)
                    * env
                    * 0.4
            })
        }),
        Sound::Drop => DROP_WAV.get_or_init(|| {
            generate_wav(35, 22050, |t| {
                let env = (-t * 110.0).exp();
                (t * 220.0 * std::f32::consts::TAU).sin() * env * 0.5
            })
        }),
        Sound::Tick => TICK_WAV.get_or_init(|| {
            generate_wav(12, 22050, |t| {
                let env = (-t * 360.0).exp();
                (t * 2800.0 * std::f32::consts::TAU).sin() * env * 0.35
            })
        }),
    };
    unsafe {
        let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_ASYNC | SND_MEMORY);
    }
}

/// GET a URL as text, giving up after 10 s so a stalled connection can't hang the caller forever.
pub fn http_get(client: &windows::Web::Http::HttpClient, url: &str) -> Option<String> {
    use windows::Foundation::{AsyncStatus, Uri};
    let uri = Uri::CreateUri(&windows::core::HSTRING::from(url)).ok()?;
    let op = client.GetStringAsync(&uri).ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while op.Status().ok()? == AsyncStatus::Started {
        if std::time::Instant::now() > deadline {
            let _ = op.Cancel();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Some(op.GetResults().ok()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wav_generation() {
        let wav = generate_wav(20, 22050, |_t| 0.0);
        assert!(wav.len() >= 44);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn test_fade_curves() {
        assert!((fade_out_curve(0.0) - 1.0).abs() < 1e-6);
        assert!(fade_out_curve(0.5) < fade_out_curve(0.0));
        assert!(fade_out_curve(1.0).abs() < 1e-6);

        assert!(fade_in_curve(0.0).abs() < 1e-6);
        assert!(fade_in_curve(0.5) > 0.0 && fade_in_curve(0.5) < 1.0);
        assert!((fade_in_curve(1.0) - 1.0).abs() < 1e-6);
    }
}
