//! MacBook-like notch for Windows.
//!
//! Two stacked windows at the top center of the screen:
//! - backdrop window: native DWM blur (glass effect), shaped using a window region;
//! - main window: per-pixel transparent (layered window), containing everything drawn by
//!   tiny-skia. Transparent pixels pass clicks through, so only the notch is "present".
//!
//! `events.rs` handles input and state, `view.rs` handles drawing.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod events;
mod lyrics;
mod media;
mod shelf;
mod store;
mod system;
mod tasks;
mod text;
mod ui;
mod view;
pub(crate) mod config;
pub(crate) mod update;

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia as sk;
use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Console::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// All dimensions in dp (1 dp = 1 px at 100% scale).
const CANVAS_W: f32 = 580.0; // includes room for shadow
const CANVAS_H: f32 = 214.0;
const EAR: f32 = 9.0; // "ear" curve blending into the screen edge

const WM_APP_MEDIA: u32 = WM_APP + 1;
const WM_APP_KEY: u32 = WM_APP + 2;
const WM_APP_TRAY: u32 = WM_APP + 3;
const WM_APP_UPDATE: u32 = WM_APP + 4;
const WM_MOUSELEAVE: u32 = 0x02A3; // not exported by WindowsAndMessaging module
const PBT_APMPOWERSTATUSCHANGE: usize = 0x0A;
const HOTKEY_TOGGLE: i32 = 100;

const TIMER_ANIM: usize = 1;
const TIMER_TICK: usize = 2;
const TIMER_BLINK: usize = 3;
const TIMER_CLOSE: usize = 4;
const TIMER_HUD: usize = 5;
const TIMER_ALERT: usize = 6;
const TIMER_LIVE: usize = 7;
const TIMER_VOLUME: usize = 8;
const TIMER_CAPS: usize = 9;
const TIMER_LYRIC: usize = 10;
const TIMER_CARET: usize = 11;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Music,
    Focus,
    Shelf,
    Tasks,
}

/// Everything clickable in the open notch.
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Tab(Tab),
    Prev,
    Toggle,
    Next,
    LyricsToggle,
    SeekBar,
    SeekLine(usize),
    FocusToggle,
    FocusReset,
    Preset(u32),
    ShelfItem(usize),
    ShelfRemove(usize),
    ShelfClear,
    TaskToggle(usize),
    TaskRemove(usize),
    TaskInput,
    OpenUpdate,
}

/// Brief HUD message that appears for ~2 seconds then disappears.
#[derive(Clone, Copy, PartialEq)]
enum Hud {
    Volume(f32, bool),
    Caps(bool),
    Power(bool, u8),
    Layout(u16),
}

/// What the closed notch displays, ordered by priority.
#[derive(Clone, Copy, PartialEq)]
enum Live {
    Hud(Hud),
    Alert,
    Reminder,
    Timer,
    Lyric,
    Media,
    Idle,
}

/// Simple Pomodoro timer: countdown from preset minutes.
struct Focus {
    minutes: u32,
    left: f64,
    ends_at: Option<Instant>,
}

impl Focus {
    fn new(minutes: u32) -> Focus {
        Focus { minutes, left: minutes as f64 * 60.0, ends_at: None }
    }
    fn total(&self) -> f64 {
        self.minutes as f64 * 60.0
    }
    fn running(&self) -> bool {
        self.ends_at.is_some()
    }
    fn remaining(&self) -> f64 {
        match self.ends_at {
            Some(end) => end.saturating_duration_since(Instant::now()).as_secs_f64(),
            None => self.left,
        }
    }
    fn progress(&self) -> f32 {
        (1.0 - self.remaining() / self.total()) as f32
    }
    /// Running or paused mid-way.
    fn active(&self) -> bool {
        self.running() || self.left < self.total()
    }
    fn toggle(&mut self) {
        match self.ends_at {
            Some(_) => {
                self.left = self.remaining();
                self.ends_at = None;
            }
            None => self.ends_at = Some(Instant::now() + Duration::from_secs_f64(self.left)),
        }
    }
    fn reset(&mut self) {
        self.ends_at = None;
        self.left = self.total();
    }
}

/// Damped spring: drives the bouncy motion when the notch resizes.
#[derive(Clone, Copy)]
struct Spring {
    x: f32,
    v: f32,
    to: f32,
}

impl Spring {
    fn new(x: f32) -> Self {
        Self { x, v: 0.0, to: x }
    }
    fn step(&mut self, dt: f32, zeta: f32) {
        const K: f32 = 230.0;
        let a = K * (self.to - self.x) - 2.0 * K.sqrt() * zeta * self.v;
        self.v += a * dt;
        self.x += self.v * dt;
    }
    fn done(&self) -> bool {
        (self.to - self.x).abs() < 0.1 && self.v.abs() < 0.5
    }
    fn snap(&mut self) {
        self.x = self.to;
        self.v = 0.0;
    }
}

/// Position of each lyric line in the lyrics column (already wrapped to screen lines).
struct LyricLayout {
    source: usize, // Lyrics address, to detect when recalculation is needed
    rows: Vec<(f32, Vec<String>)>,
}

struct App {
    hwnd: HWND,
    backdrop: HWND,
    scale: f32,
    pos: POINT,
    pw: u32,
    ph: u32,
    mem_dc: HDC,
    bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    bits: *mut u8,
    blur_shape: Option<(i32, i32, i32, i32, i32)>,

    w: Spring,
    h: Spring,
    r: Spring,
    scroll: Spring, // lyrics column scroll
    open: bool,
    hovering: bool,
    hidden: bool,
    animating: bool,
    live_timer: bool,
    last_frame: Instant,
    started: Instant,
    live: Live,
    live_since: Instant,
    tick: u32,
    blink: bool,
    minute: u16,
    keyboard: (isize, u16),

    tab: Tab,
    focus: Focus,
    hud: Option<Hud>,
    alert: bool,
    reminder: Option<String>,
    battery: Option<(bool, u8)>,
    privacy: (bool, bool), // (camera_in_use, microphone_in_use)
    stats: (u8, u8),       // (cpu_percent, ram_percent)
    caps: bool,
    hover: Option<Action>,
    hits: Vec<(Action, sk::Rect)>,
    press: Option<(Action, f32, f32)>,

    media: media::Handle,
    info: media::MediaInfo,
    lyrics_on: bool,
    lyrics: media::LyricsSlot,
    lyric_line: Option<usize>,
    lyric_layout: Option<LyricLayout>,

    shelf: Vec<shelf::Item>,
    shelf_scroll: usize,
    drag_in: bool,

    tasks: Vec<tasks::Task>,
    task_scroll: usize,
    editing: bool,
    draft: String,
    caret: bool,

    fonts: Option<text::Fonts>,
    pub(crate) config: config::Config,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    if ctrl_type == CTRL_C_EVENT || ctrl_type == CTRL_BREAK_EVENT || ctrl_type == CTRL_CLOSE_EVENT {
        let hwnd = HWND(MAIN_HWND.load(Ordering::Relaxed) as _);
        if !hwnd.0.is_null() {
            let _ = PostMessageW(hwnd, WM_DESTROY, WPARAM(0), LPARAM(0));
            return BOOL(1);
        }
    }
    BOOL(0)
}

impl App {
    pub(crate) fn is_tab_enabled(&self, tab: Tab) -> bool {
        match tab {
            Tab::Music => self.config.tabs.media,
            Tab::Focus => self.config.tabs.focus,
            Tab::Shelf => self.config.tabs.shelf,
            Tab::Tasks => self.config.tabs.tasks,
        }
    }

    pub(crate) fn first_enabled_tab(&self) -> Tab {
        if self.config.tabs.media {
            Tab::Music
        } else if self.config.tabs.focus {
            Tab::Focus
        } else if self.config.tabs.shelf {
            Tab::Shelf
        } else if self.config.tabs.tasks {
            Tab::Tasks
        } else {
            Tab::Music
        }
    }
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

/// Right-click context menu. Called outside `with_app` because the menu runs its own message loop.
fn show_menu(hwnd: HWND) {
    const TOGGLE: usize = 1;
    const AUTOSTART: usize = 2;
    const EDIT_CONFIG: usize = 3;
    const ABOUT: usize = 4;
    const UPDATE: usize = 5;
    const QUIT: usize = 6;
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let _ = AppendMenuW(menu, MF_STRING, TOGGLE, w!("Toggle Open / Close"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let check = if system::autostart() { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check, AUTOSTART, w!("Start with Windows"));
        let _ = AppendMenuW(menu, MF_STRING, EDIT_CONFIG, w!("Edit config.toml"));

        let update_info = update::UPDATE_AVAILABLE.read().ok().and_then(|u| u.clone());
        if let Some(ref info) = update_info {
            let label = format!("Update Available: {} (Download)", info.tag);
            let label_w: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            let _ = AppendMenuW(menu, MF_STRING, UPDATE, windows::core::PCWSTR(label_w.as_ptr()));
        }

        let _ = AppendMenuW(menu, MF_STRING, ABOUT, w!("About Notch"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, QUIT, w!("Quit Notch"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_TOPALIGN, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
        match cmd.0 as usize {
            TOGGLE => {
                with_app(|a| a.toggle_open());
            }
            AUTOSTART => system::set_autostart(!system::autostart()),
            EDIT_CONFIG => {
                if let Some(path) = config::path() {
                    shelf::open(&path.to_string_lossy());
                }
            }
            ABOUT => {
                let msg = format!("Notch v{}\nCreated by Krisna Santosa\n\nMacBook-like dynamic notch for Windows 10/11.\nLicensed under GNU AGPLv3.", env!("CARGO_PKG_VERSION"));
                let msg_w: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                let caption = w!("About Notch");
                let _ = MessageBoxW(hwnd, windows::core::PCWSTR(msg_w.as_ptr()), caption, MB_OK | MB_ICONINFORMATION);
            }
            UPDATE => {
                if let Some(info) = update_info {
                    shelf::open(&info.url);
                }
            }
            QUIT => {
                let _ = DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let x = (lp.0 & 0xffff) as i16 as f32;
    let y = ((lp.0 >> 16) & 0xffff) as i16 as f32;
    match msg {
        WM_MOUSEMOVE => {
            let left_down = wp.0 & 1 != 0;
            // dragging files out runs outside with_app: its OLE loop calls back into us
            if let Some(Some(paths)) = with_app(|a| a.on_mouse_move(x, y, left_down)) {
                shelf::drag_out(hwnd, &paths);
                with_app(|a| a.after_drag_out());
            }
        }
        WM_MOUSELEAVE => {
            with_app(|a| a.on_mouse_leave());
        }
        WM_LBUTTONDOWN => {
            with_app(|a| a.on_press(x, y));
        }
        WM_LBUTTONUP => {
            with_app(|a| a.on_click(x, y));
        }
        WM_MOUSEWHEEL => {
            with_app(|a| a.on_wheel((wp.0 >> 16) as i16));
        }
        WM_RBUTTONUP => show_menu(hwnd),
        WM_CHAR => {
            with_app(|a| a.on_char(wp.0 as u32));
        }
        WM_KILLFOCUS => {
            with_app(|a| a.stop_editing());
        }
        WM_SETCURSOR => {
            let cursor = with_app(|a| match a.hover {
                Some(Action::TaskInput) => Some(IDC_IBEAM),
                Some(_) => Some(IDC_HAND),
                None => None,
            });
            if let Some(Some(id)) = cursor {
                if let Ok(c) = LoadCursorW(None, id) {
                    SetCursor(c);
                }
                return LRESULT(1);
            }
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        // clicking the notch must not steal focus from the active window
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER => {
            with_app(|a| a.on_timer(wp.0));
        }
        WM_APP_MEDIA => {
            with_app(|a| a.on_media());
        }
        WM_APP_UPDATE => {
            with_app(|a| {
                a.redraw();
            });
        }
        WM_APP_TRAY => {
            match lp.0 as u32 {
                WM_LBUTTONUP => {
                    with_app(|a| a.toggle_open());
                }
                WM_RBUTTONUP => {
                    show_menu(hwnd);
                }
                _ => {}
            }
        }
        WM_APP_KEY => {
            SetTimer(hwnd, TIMER_VOLUME, 30, None);
        }
        WM_POWERBROADCAST => {
            if wp.0 == PBT_APMPOWERSTATUSCHANGE {
                with_app(|a| a.on_power());
            }
            return LRESULT(1);
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            with_app(|a| a.relayout());
        }
        WM_HOTKEY => {
            if wp.0 as i32 == HOTKEY_TOGGLE {
                with_app(|a| a.toggle_open());
            }
        }
        WM_DESTROY => {
            let nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            let _ = UnregisterHotKey(hwnd, HOTKEY_TOGGLE);
            PostQuitMessage(0);
        }
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    LRESULT(0)
}

/// Backdrop glass window: receives no input at all.
unsafe extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;

        let backdrop_class = w!("NotchBackdrop");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(backdrop_proc),
            hInstance: instance.into(),
            lpszClassName: backdrop_class,
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        });
        let backdrop = CreateWindowExW(ex, backdrop_class, w!(""), WS_POPUP, 0, 0, 1, 1, None, None, instance, None)?;
        system::enable_blur(backdrop);
        system::set_blur_shape(backdrop, None);

        let class = w!("NotchPrototype");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        // owned by the backdrop window, so it always stays right above it
        let hwnd = CreateWindowExW(ex | WS_EX_LAYERED, class, w!("Notch"), WS_POPUP, 0, 0, 1, 1, backdrop, None, instance, None)?;
        MAIN_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        let _ = SetConsoleCtrlHandler(Some(console_ctrl_handler), true);

        shelf::register(hwnd); // also initializes COM for thumbnails and volume
        let cfg = config::load();
        let saved = store::load();
        let px = (48.0 * GetDpiForWindow(hwnd) as f32 / 96.0) as u32;
        let now = Instant::now();
        let initial_tab = if cfg.tabs.media {
            Tab::Music
        } else if cfg.tabs.focus {
            Tab::Focus
        } else if cfg.tabs.shelf {
            Tab::Shelf
        } else if cfg.tabs.tasks {
            Tab::Tasks
        } else {
            Tab::Music
        };
        let app = App {
            hwnd,
            backdrop,
            scale: 1.0,
            pos: POINT::default(),
            pw: 0,
            ph: 0,
            mem_dc: CreateCompatibleDC(None),
            bmp: HBITMAP::default(),
            old_bmp: HGDIOBJ::default(),
            bits: std::ptr::null_mut(),
            blur_shape: None,
            w: Spring::new(cfg.notch.closed_width),
            h: Spring::new(cfg.notch.closed_height),
            r: Spring::new(cfg.notch.corner_radius),
            scroll: Spring::new(0.0),
            open: false,
            hovering: false,
            hidden: false,
            animating: false,
            live_timer: false,
            last_frame: now,
            started: now,
            live: Live::Idle,
            live_since: now - Duration::from_secs(1),
            tick: 0,
            blink: false,
            minute: GetLocalTime().wMinute,
            keyboard: system::keyboard_layout(),
            tab: initial_tab,
            focus: Focus::new(saved.preset),
            hud: None,
            alert: false,
            reminder: None,
            battery: system::battery(),
            privacy: (false, false),
            stats: (0, system::ram_usage()),
            caps: system::caps_lock(),
            hover: None,
            hits: Vec::new(),
            press: None,
            media: media::start(hwnd.0 as isize, WM_APP_MEDIA, saved.lyrics),
            info: media::MediaInfo::default(),
            lyrics_on: saved.lyrics,
            lyrics: media::LyricsSlot { key: String::new(), state: media::LyricsState::Missing },
            lyric_line: None,
            lyric_layout: None,
            shelf: saved.files.into_iter().map(|p| shelf::Item::new(p, px)).collect(),
            shelf_scroll: 0,
            drag_in: false,
            tasks: saved.tasks,
            task_scroll: 0,
            editing: false,
            draft: String::new(),
            caret: true,
            fonts: Some(text::Fonts::load()),
            config: cfg,
        };
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        with_app(|a| a.relayout());

        // debug: NOTCH_OPEN=music|focus|shelf|tasks opens the notch without hover (for screenshots)
        if let Ok(tab) = std::env::var("NOTCH_OPEN") {
            with_app(|a| {
                a.open = true;
                a.tab = match tab.as_str() {
                    "focus" => Tab::Focus,
                    "shelf" => Tab::Shelf,
                    "tasks" => Tab::Tasks,
                    _ => Tab::Music,
                };
                if tab == "focus" {
                    a.focus.toggle();
                }
                a.retarget();
            });
        }

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE); // glass window appears automatically when the notch opens
        SetTimer(hwnd, TIMER_TICK, 1000, None);
        system::register_volume_notify(hwnd, WM_APP_KEY);
        let _ = RegisterHotKey(hwnd, HOTKEY_TOGGLE, MOD_WIN | MOD_ALT | MOD_NOREPEAT, 'N' as u32);

        let update_hwnd = hwnd.0 as isize;
        update::check_in_background(move |_info| {
            let _ = PostMessageW(HWND(update_hwnd as _), WM_APP_UPDATE, WPARAM(0), LPARAM(0));
        });

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_APP_TRAY,
            hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            szTip: [0; 128],
            ..Default::default()
        };
        let tip = "Notch by Krisna Santosa\0".encode_utf16().collect::<Vec<u16>>();
        nid.szTip[..tip.len()].copy_from_slice(&tip);
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// Used by `events.rs` and `view.rs`.
fn lyrics_of(slot: &media::LyricsSlot) -> Option<&Arc<lyrics::Lyrics>> {
    match &slot.state {
        media::LyricsState::Found(l) => Some(l),
        _ => None,
    }
}
