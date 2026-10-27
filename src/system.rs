//! Small Windows bridge: volume, Caps Lock, battery, fullscreen detection, and autostart.

use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const KEY_VOLUME: usize = 1;
pub const KEY_CAPS: usize = 2;

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
pub fn fullscreen() -> bool {
    let state = unsafe { SHQueryUserNotificationState() };
    if let Ok(s) = state {
        if s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE {
            return true;
        }
    }
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || fg == GetDesktopWindow() || fg == GetShellWindow() {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let hmon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        if hmon.0.is_null() {
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

// ---- Volume & Caps Lock keys: low-level keyboard hook -----------------------------------
// The hook only forwards messages to the window; values are read afterwards,
// once Windows finishes applying the key state.

static TARGET: AtomicIsize = AtomicIsize::new(0);
static MESSAGE: AtomicIsize = AtomicIsize::new(0);

pub fn install_key_hook(hwnd: HWND, msg: u32) {
    TARGET.store(hwnd.0 as isize, Ordering::Relaxed);
    MESSAGE.store(msg as isize, Ordering::Relaxed);
    unsafe {
        if let Ok(module) = GetModuleHandleW(None) {
            let _ = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook), module, 0);
        }
    }
}

unsafe extern "system" fn key_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let key = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let vk = VIRTUAL_KEY(key.vkCode as u16);
        let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let kind = match vk {
            VK_VOLUME_UP | VK_VOLUME_DOWN | VK_VOLUME_MUTE if down => KEY_VOLUME,
            VK_CAPITAL if !down => KEY_CAPS,
            _ => 0,
        };
        if kind != 0 {
            let hwnd = HWND(TARGET.load(Ordering::Relaxed) as _);
            let _ = PostMessageW(hwnd, MESSAGE.load(Ordering::Relaxed) as u32, WPARAM(kind), LPARAM(0));
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

// ---- Glass: native DWM blur on the backdrop window ---------------------------------------
// Windows blurs the screen content behind the notch, incurring zero CPU cost for us.
// Shaped using a window region; anti-aliased edges are drawn by the main window on top.

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient: u32,
    animation: u32,
}

#[repr(C)]
struct CompositionData {
    attribute: u32,
    data: *mut std::ffi::c_void,
    size: usize,
}

pub fn enable_blur(hwnd: HWND) {
    const WCA_ACCENT_POLICY: u32 = 19;
    const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
    type SetComposition = unsafe extern "system" fn(HWND, *mut CompositionData) -> BOOL;
    unsafe {
        let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else { return };
        let Some(func) = windows::Win32::System::LibraryLoader::GetProcAddress(
            user32,
            windows::core::s!("SetWindowCompositionAttribute"),
        ) else {
            return;
        };
        let set: SetComposition = std::mem::transmute(func);
        let mut accent = AccentPolicy { state: ACCENT_ENABLE_BLURBEHIND, flags: 0, gradient: 0, animation: 0 };
        let mut data = CompositionData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut accent as *mut _ as *mut _,
            size: std::mem::size_of::<AccentPolicy>(),
        };
        let _ = set(hwnd, &mut data);
    }
}

/// Blur area in screen pixels: (left, top, width, height, bottom corner radius). `None` = hide.
/// DWM blurs the entire window rect, so the window itself is resized;
/// the region is only used to round the bottom corners.
pub fn set_blur_shape(hwnd: HWND, shape: Option<(i32, i32, i32, i32, i32)>) {
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
    unsafe {
        let Some((x, y, w, h, rad)) = shape else {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        };
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW);
        // top corners are placed outside the window, so only bottom corners round
        SetWindowRgn(hwnd, CreateRoundRectRgn(0, -2 * rad, w + 1, h + 1, rad * 2, rad * 2), true);
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
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    unsafe {
        OpenClipboard(hwnd).ok()?;
        let text = GetClipboardData(CF_UNICODETEXT.0 as u32).ok().and_then(|h| {
            let global = windows::Win32::Foundation::HGLOBAL(h.0);
            let ptr = GlobalLock(global) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(global);
            Some(s)
        });
        let _ = CloseClipboard();
        text
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
