//! State and input: hover, clicks, file drags, typing, timers, lyrics, and reminders.

use crate::*;
use std::mem::discriminant;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const SHELF_VISIBLE: usize = 5;
const TASKS_VISIBLE: usize = 3;
const CLIPS_VISIBLE: usize = 3;
const MAX_CLIPS: usize = 30;

impl App {
    fn current_live(&self) -> Live {
        if let Some(hud) = self.hud {
            Live::Hud(hud)
        } else if self.alert {
            Live::Alert
        } else if self.reminder.is_some() {
            Live::Reminder
        } else if self.welcome_tour {
            Live::Welcome
        } else if self.focus.running() {
            Live::Timer
        } else if self.lyric_text().is_some_and(|t| !t.is_empty()) && self.info.playing {
            Live::Lyric
        } else if self.info.has_session {
            Live::Media
        } else {
            Live::Idle
        }
    }

    /// Lyric line currently being sung (if lyrics are enabled and found).
    pub(crate) fn lyric_text(&self) -> Option<&str> {
        let lyrics = lyrics_of(&self.lyrics).filter(|_| self.lyrics_on)?;
        Some(lyrics.lines.get(self.lyric_line?)?.1.as_str())
    }

    fn live_width(&mut self, live: Live) -> f32 {
        match live {
            Live::Hud(Hud::Caps(_) | Hud::Layout(_)) => 280.0,
            Live::Hud(_) => 300.0,
            Live::Alert => 270.0,
            Live::Reminder => 340.0,
            Live::Welcome => 340.0,
            Live::Timer => 236.0,
            Live::Media | Live::Lyric => {
                if self.hover_ready && !self.open && self.info.has_session {
                    330.0
                } else if live == Live::Lyric {
                    let text = self.lyric_text().unwrap_or("").to_string();
                    let s = self.scale;
                    let w = self.fonts.as_mut().map_or(0.0, |f| f.measure(&text, 13.0 * s, true) / s);
                    (w + 108.0).clamp(236.0, 470.0)
                } else {
                    236.0
                }
            }
            Live::Idle => self.config.notch.closed_width,
        }
    }

    pub(crate) fn play_sound(&self, sound: system::Sound) {
        if self.config.behavior.sound_effects {
            system::play_sound(sound);
        }
    }

    /// Toggle notch between open and closed state (e.g. from hotkey or tray).
    pub(crate) fn toggle_open(&mut self) {
        self.open = !self.open;
        if self.open {
            // hovering stays false so the next mouse move arms WM_MOUSELEAVE and the notch can auto-close
            if self.alert && self.config.tabs.focus {
                self.tab = Tab::Focus;
            }
            if self.reminder.is_some() && self.config.tabs.tasks {
                self.tab = Tab::Tasks;
            }
            self.alert = false;
            self.reminder = None;
        } else {
            self.pinned = false;
            self.hovering = false;
            self.hover_ready = false;
            self.hover = None;
            self.press = None;
            self.stop_editing();
        }
        self.retarget();
    }

    pub(crate) fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
        self.play_sound(system::Sound::Pop);
        if self.pinned && !self.open {
            self.open = true;
            self.retarget();
        } else {
            self.redraw();
        }
    }

    pub(crate) fn on_escape(&mut self) {
        if self.editing {
            self.draft.clear();
            self.stop_editing();
            return;
        }
        if self.open {
            self.open = false;
            self.pinned = false;
            self.hovering = false;
            self.hover_ready = false;
            self.hover = None;
            self.press = None;
            self.retarget();
        }
    }

    pub(crate) fn show_welcome(&mut self) {
        self.welcome_tour = true;
        self.welcome_since = Instant::now();
        self.open = false;
        self.pinned = false;
        self.hovering = false;
        self.hover_ready = false;
        self.play_sound(system::Sound::Chime);
        self.update_live_timer();
        self.retarget();
        self.redraw();
    }

    /// Recalculate notch target dimensions and start its animation.
    pub(crate) fn retarget(&mut self) {
        let live = self.current_live();
        if discriminant(&live) != discriminant(&self.live) {
            self.live_since = Instant::now(); // new content fades in
        }
        self.live = live;
        let (w, h, r) = if self.open {
            (self.config.notch.open_width, self.config.notch.open_height, self.config.notch.open_radius)
        } else {
            (self.live_width(live), self.config.notch.closed_height, self.config.notch.corner_radius)
        };
        self.w.to = w;
        self.h.to = h;
        self.r.to = r;
        self.start_animation();
        self.update_live_timer();
        if self.open != self.esc_hotkey {
            self.esc_hotkey = self.open;
            unsafe {
                if self.open {
                    let _ = RegisterHotKey(self.hwnd, HOTKEY_ESC, MOD_NOREPEAT, VK_ESCAPE.0 as u32);
                } else {
                    let _ = UnregisterHotKey(self.hwnd, HOTKEY_ESC);
                }
            }
        }
    }

    pub(crate) fn start_animation(&mut self) {
        if !self.animating {
            self.animating = true;
            self.last_frame = Instant::now();
            unsafe { SetTimer(self.hwnd, TIMER_ANIM, 15, None) };
        }
    }

    /// Small outward pulse, indicating updated content.
    fn bump(&mut self) {
        if !self.open {
            self.w.v += 60.0;
            self.retarget();
        }
    }

    /// Equalizer only animates when music is playing and the notch is closed.
    fn update_live_timer(&mut self) {
        let want = !self.open && !self.hidden && (self.welcome_tour || ((matches!(self.live, Live::Media | Live::Lyric) || (self.focus.running() && self.info.has_session)) && self.info.playing));
        if want != self.live_timer {
            self.live_timer = want;
            unsafe {
                if want {
                    SetTimer(self.hwnd, TIMER_LIVE, 33, None);
                } else {
                    let _ = KillTimer(self.hwnd, TIMER_LIVE);
                }
            }
        }
    }

    pub(crate) fn content_alpha(&self) -> f32 {
        (self.live_since.elapsed().as_secs_f32() / 0.22).min(1.0)
    }

    /// Motion that outlives the springs: drag-in ripples, the ringing reminder bell, the task bell popping in.
    fn still_moving(&self) -> bool {
        let ringing = self.live == Live::Reminder && self.live_since.elapsed().as_secs_f32() < 1.2;
        let popping = self.draft_time_since.is_some_and(|t| t.elapsed().as_secs_f32() < 0.35);
        self.drag_in || ringing || popping
    }

    fn animate(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        // Snappy, solid Apple-grade spring: high stiffness, critically damped (zero jelly wobbling)
        let zeta = if self.open { 0.88 } else { 0.94 };
        let steps = (dt * 240.0).ceil().max(1.0);
        for _ in 0..steps as u32 {
            for s in [&mut self.w, &mut self.h, &mut self.r] {
                s.step(dt / steps, zeta);
            }
            self.scroll.step(dt / steps, 0.92);
        }
        let springs = [self.w, self.h, self.r, self.scroll];
        if springs.iter().all(Spring::done) && self.content_alpha() >= 1.0 && !self.still_moving() {
            for s in [&mut self.w, &mut self.h, &mut self.r, &mut self.scroll] {
                s.snap();
            }
            self.animating = false;
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_ANIM);
            }
        }
        self.render();
    }

    fn hit_rect(&self, x: f32, y: f32) -> Option<(Action, sk::Rect)> {
        self.hits
            .iter()
            .find(|(_, r)| x >= r.left() && x <= r.right() && y >= r.top() && y <= r.bottom())
            .copied()
    }

    // ---- Mouse ---------------------------------------------------------------------------

    /// Returns the list of files if the user starts dragging a Shelf item out.
    pub(crate) fn on_mouse_move(&mut self, x: f32, y: f32, left_down: bool) -> Option<Vec<String>> {
        let (x, y) = (x / self.scale, y / self.scale);
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_CLOSE);
        }
        if !self.hovering {
            self.hovering = true;
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
            }
            if !self.open && self.config.behavior.hover_to_open {
                let delay = self.config.behavior.hover_delay_ms;
                if delay == 0 {
                    self.open_on_hover();
                } else {
                    unsafe {
                        SetTimer(self.hwnd, TIMER_HOVER, delay, None);
                    }
                }
            }
        }

        if let (true, Some((Action::ShelfItem(i), px, py))) = (left_down, self.press) {
            if (x - px).hypot(y - py) > 5.0 {
                self.press = None;
                return self.shelf.get(i).map(|item| vec![item.path.clone()]);
            }
        }

        let moved = self.mouse_pos.is_none_or(|(px, py)| (px - x).hypot(py - y) > 3.0);
        if moved {
            self.mouse_pos = Some((x, y));
            self.last_active = Instant::now();
            if self.open || self.live == Live::Idle {
                self.redraw();
            }
        }

        let hover = self.hit_rect(x, y).map(|(a, _)| a);
        if hover != self.hover {
            self.hover = hover;
            self.redraw();
        }
        None
    }

    fn open_on_hover(&mut self) {
        if !self.hovering || self.open || !self.config.behavior.hover_to_open {
            return;
        }
        self.hover_ready = true;
        if self.welcome_tour {
            self.welcome_tour = false;
            self.welcomed = true;
            self.save();
        }
        if self.info.has_session {
            self.retarget();
        } else {
            self.open = true;
            if self.alert && self.config.tabs.focus {
                self.tab = Tab::Focus;
            }
            if self.reminder.is_some() && self.config.tabs.tasks {
                self.tab = Tab::Tasks;
            }
            self.alert = false;
            self.reminder = None;
            self.retarget();
        }
    }

    pub(crate) fn on_mouse_leave(&mut self) {
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_HOVER);
        }
        self.hovering = false;
        self.hover_ready = false;
        self.mouse_pos = None;
        self.hover = None;
        self.press = None;
        if !self.open {
            self.retarget();
        }
        if !self.pinned {
            // small delay so the notch doesn't flicker when the mouse grazes the edge
            unsafe { SetTimer(self.hwnd, TIMER_CLOSE, self.config.behavior.auto_close_delay_ms, None) };
        }
        self.redraw();
    }

    pub(crate) fn on_press(&mut self, x: f32, y: f32) {
        let (x, y) = (x / self.scale, y / self.scale);
        self.press = self.hit_rect(x, y).map(|(a, _)| (a, x, y));
    }

    pub(crate) fn on_middle_click(&mut self) {
        if !self.open && self.info.has_session {
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_HOVER);
            }
            self.play_sound(system::Sound::Pop);
            self.media.send(media::Cmd::Toggle);
            self.info.playing = !self.info.playing;
            self.update_live_timer();
            self.redraw();
        }
    }

    pub(crate) fn on_click(&mut self, x: f32, y: f32) {
        if self.welcome_tour {
            self.welcome_tour = false;
            self.welcomed = true;
            self.save();
        }
        let (x, y) = (x / self.scale, y / self.scale);
        let pressed = self.press.take();
        let Some((action, rect)) = self.hit_rect(x, y) else {
            if !self.open {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_HOVER);
                }
                self.open = true;
                self.hovering = true;
                self.hover_ready = true;
                if self.info.has_session && self.config.tabs.media {
                    self.tab = Tab::Music;
                }
                self.retarget();
            }
            self.stop_editing();
            return;
        };
        if pressed.map(|p| p.0) != Some(action) {
            return; // button was pressed in one spot and released in another
        }
        if !matches!(action, Action::TaskInput | Action::TaskSubmit) {
            self.stop_editing();
        }
        match action {
            Action::Tab(tab) => {
                self.play_sound(system::Sound::Pop);
                if tab == Tab::Settings && self.open && self.tab == Tab::Settings {
                    self.tab = self.first_enabled_tab();
                } else {
                    self.tab = tab;
                }
                if !self.open {
                    self.open = true;
                    self.hovering = true;
                    self.hover_ready = true;
                }
            }
            Action::Prev => self.media.send(media::Cmd::Prev),
            Action::Next => self.media.send(media::Cmd::Next),
            Action::Toggle => {
                self.play_sound(system::Sound::Pop);
                self.media.send(media::Cmd::Toggle);
                self.info.playing = !self.info.playing; // update immediately, corrected by media thread
            }
            Action::LyricsToggle => {
                self.play_sound(system::Sound::Pop);
                self.lyrics_on = !self.lyrics_on;
                self.media.set_lyrics(self.lyrics_on);
                self.lyric_layout = None;
                self.save();
            }
            Action::SeekBar => {
                let frac = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
                self.seek(frac * self.info.duration);
            }
            Action::SeekLine(i) => {
                if let Some(t) = lyrics_of(&self.lyrics).and_then(|l| l.lines.get(i)).map(|l| l.0) {
                    self.seek(t);
                }
            }
            Action::FocusToggle => {
                self.play_sound(system::Sound::Pop);
                self.focus.toggle();
            }
            Action::FocusReset => {
                self.play_sound(system::Sound::Pop);
                self.focus.reset();
            }
            Action::Preset(m) => {
                self.play_sound(system::Sound::Pop);
                self.focus = Focus::new(m);
                self.save();
            }
            Action::ShelfItem(i) => {
                if let Some(item) = self.shelf.get(i) {
                    shelf::open(&item.path);
                }
            }
            Action::ShelfRemove(i) => {
                if i < self.shelf.len() {
                    self.play_sound(system::Sound::Pop);
                    self.shelf.remove(i);
                    self.clamp_scroll();
                    self.save();
                }
            }
            Action::ShelfClear => {
                self.play_sound(system::Sound::Pop);
                self.shelf.clear();
                self.shelf_scroll = 0;
                self.save();
            }
            Action::TaskToggle(i) => {
                let mut toggled = None;
                if let Some(t) = self.tasks.get_mut(i) {
                    t.done = !t.done;
                    toggled = Some(t.done);
                }
                if let Some(done) = toggled {
                    if done {
                        self.happy_until = Some(Instant::now() + Duration::from_millis(2500));
                    }
                    self.save();
                    self.play_sound(if done {
                        system::Sound::Chime
                    } else {
                        system::Sound::Pop
                    });
                }
            }
            Action::TaskRemove(i) => {
                if i < self.tasks.len() {
                    self.play_sound(system::Sound::Pop);
                    self.tasks.remove(i);
                    self.clamp_scroll();
                    self.save();
                }
            }
            Action::TaskInput => self.start_editing(),
            Action::TaskSubmit => self.on_char(0x0D),
            Action::ClipCopy(i) => {
                if let Some(text) = self.clips.get(i).cloned() {
                    if system::set_clipboard_text(self.hwnd, &text) {
                        self.play_sound(system::Sound::Pop);
                        self.clip_copied = Some((0, Instant::now()));
                    }
                }
            }
            Action::ClipRemove(i) => {
                if i < self.clips.len() {
                    self.play_sound(system::Sound::Pop);
                    self.clips.remove(i);
                    self.clamp_scroll();
                }
            }
            Action::ClipClear => {
                self.play_sound(system::Sound::Pop);
                self.clips.clear();
                self.clip_scroll = 0;
            }
            Action::OpenUpdate => {
                if let Some(info) = update::UPDATE_AVAILABLE.read().ok().and_then(|u| u.clone()) {
                    shelf::open(&info.url);
                }
            }
            Action::TogglePin => self.toggle_pin(),
            Action::SettingsAccent(i) => {
                match i {
                    0 => self.config.theme.accent_mode = "album_art".to_string(),
                    1 => {
                        self.config.theme.accent_mode = "custom".to_string();
                        self.config.theme.custom_accent = "#3B82F6".to_string();
                    }
                    2 => {
                        self.config.theme.accent_mode = "custom".to_string();
                        self.config.theme.custom_accent = "#10B981".to_string();
                    }
                    3 => {
                        self.config.theme.accent_mode = "custom".to_string();
                        self.config.theme.custom_accent = "#8B5CF6".to_string();
                    }
                    4 => {
                        self.config.theme.accent_mode = "custom".to_string();
                        self.config.theme.custom_accent = "#F97316".to_string();
                    }
                    _ => {
                        self.config.theme.accent_mode = "custom".to_string();
                        self.config.theme.custom_accent = "#EF4444".to_string();
                    }
                }
                self.config.save();
                self.play_sound(system::Sound::Pop);
                self.redraw();
            }
            Action::SettingsToggleSound => {
                self.config.behavior.sound_effects = !self.config.behavior.sound_effects;
                self.config.save();
                self.play_sound(system::Sound::Pop);
                self.redraw();
            }
            Action::SettingsToggleFullscreen => {
                self.config.behavior.auto_hide_fullscreen = !self.config.behavior.auto_hide_fullscreen;
                self.config.save();
                self.play_sound(system::Sound::Pop);
                self.redraw();
            }
            Action::SettingsToggleHover => {
                self.config.behavior.hover_to_open = !self.config.behavior.hover_to_open;
                self.config.save();
                self.play_sound(system::Sound::Pop);
                self.redraw();
            }
            Action::SettingsToggleAutostart => {
                let on = system::autostart();
                system::set_autostart(!on);
                self.play_sound(system::Sound::Pop);
                self.redraw();
            }
            Action::SettingsToggleTab(i) => {
                match i {
                    0 => self.config.tabs.media = !self.config.tabs.media,
                    1 => self.config.tabs.focus = !self.config.tabs.focus,
                    2 => self.config.tabs.shelf = !self.config.tabs.shelf,
                    3 => self.config.tabs.tasks = !self.config.tabs.tasks,
                    _ => {
                        self.config.tabs.clipboard = !self.config.tabs.clipboard;
                        self.sync_clipboard_listener();
                    }
                }
                self.config.save();
                self.play_sound(system::Sound::Pop);
                self.redraw();
            }
            Action::SettingsResetDefaults => {
                self.config = config::Config::default();
                self.config.save();
                self.sync_clipboard_listener();
                self.play_sound(system::Sound::Pop);
                self.relayout();
                self.retarget();
                self.start_animation();
                self.redraw();
            }
            Action::SettingsOpenConfig => {
                if let Some(path) = config::path() {
                    shelf::open(&path.to_string_lossy());
                }
                self.play_sound(system::Sound::Pop);
            }
        }
        self.sync_lyric();
        self.retarget();
    }

    fn seek(&mut self, t: f64) {
        self.media.send(media::Cmd::Seek(t));
        // update immediately; media thread will correct if the player rejects it
        self.info.position = t;
        self.info.position_at = media::unix_now();
    }

    pub(crate) fn on_wheel(&mut self, delta: i16) {
        let step = if delta > 0 { -1 } else { 1 };
        if self.open && matches!(self.tab, Tab::Shelf | Tab::Tasks | Tab::Clipboard) {
            match self.tab {
                Tab::Shelf => self.shelf_scroll = self.shelf_scroll.saturating_add_signed(step),
                Tab::Tasks => self.task_scroll = self.task_scroll.saturating_add_signed(step),
                Tab::Clipboard => self.clip_scroll = self.clip_scroll.saturating_add_signed(step),
                _ => {}
            }
            self.clamp_scroll();
            self.redraw();
            return;
        }

        // Shift + Wheel scrubs currently playing media (±5s)
        let is_shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        if is_shift && self.info.has_session && self.info.duration > 0.0 {
            let scrub = if delta > 0 { 5.0 } else { -5.0 };
            let target = (self.info.position + scrub).clamp(0.0, self.info.duration);
            self.seek(target);
            self.play_sound(system::Sound::Tick);
            return;
        }

        // Wheel over notch adjusts master volume with haptic feedback
        if let Some((level, muted)) = system::nudge_volume(delta as f32 / 120.0 * 0.04) {
            self.play_sound(system::Sound::Tick);
            self.show_hud(Hud::Volume(level, muted));
            self.redraw();
        }
    }

    fn clamp_scroll(&mut self) {
        self.shelf_scroll = self.shelf_scroll.min(self.shelf.len().saturating_sub(SHELF_VISIBLE));
        self.task_scroll = self.task_scroll.min(self.tasks.len().saturating_sub(TASKS_VISIBLE));
        self.clip_scroll = self.clip_scroll.min(self.clips.len().saturating_sub(CLIPS_VISIBLE));
    }

    pub(crate) fn sync_clipboard_listener(&mut self) {
        if self.config.tabs.clipboard != self.clip_listening {
            self.clip_listening = self.config.tabs.clipboard;
            system::set_clipboard_listener(self.hwnd, self.clip_listening);
            if self.clip_listening {
                self.on_clipboard_update();
            }
        }
    }

    pub(crate) fn on_clipboard_update(&mut self) {
        if !self.config.tabs.clipboard {
            return;
        }
        let Some(text) = system::clipboard_text(self.hwnd) else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        self.clips.retain(|c| c != &text);
        self.clips.insert(0, text);
        self.clips.truncate(MAX_CLIPS);
        self.clamp_scroll();
        if self.open && self.tab == Tab::Clipboard {
            self.redraw();
        }
    }

    // ---- Shelf: drag and drop in / out ---------------------------------------------------

    pub(crate) fn on_drag_enter(&mut self) {
        self.drag_in = true;
        self.open = true;
        if self.config.tabs.shelf {
            self.tab = Tab::Shelf;
        }
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_CLOSE);
        }
        self.play_sound(system::Sound::Pop);
        self.retarget();
        self.start_animation();
    }

    pub(crate) fn on_drag_leave(&mut self) {
        self.drag_in = false;
        self.schedule_close(260);
        self.redraw();
    }

    pub(crate) fn on_drop(&mut self, paths: Vec<String>) {
        self.drag_in = false;
        let px = (48.0 * self.scale) as u32;
        for path in paths {
            if !self.shelf.iter().any(|i| i.path == path) {
                self.shelf.push(shelf::Item::new(path, px));
            }
        }
        self.shelf_scroll = self.shelf.len().saturating_sub(SHELF_VISIBLE);
        self.play_sound(system::Sound::Drop);
        self.save();
        self.schedule_close(1400); // let the result stay visible for a moment
        self.retarget();
    }

    pub(crate) fn after_drag_out(&mut self) {
        // files that were moved (not copied) no longer exist at their original path
        self.shelf.retain(|i| std::path::Path::new(&i.path).exists());
        self.clamp_scroll();
        self.save();
        self.hovering = false;
        self.hover_ready = false;
        self.schedule_close(400);
        self.redraw();
    }

    fn schedule_close(&self, ms: u32) {
        if !self.hovering && !self.pinned {
            unsafe { SetTimer(self.hwnd, TIMER_CLOSE, ms.max(self.config.behavior.auto_close_delay_ms), None) };
        }
    }

    // ---- Typing new tasks ----------------------------------------------------------------

    fn start_editing(&mut self) {
        self.editing = true;
        self.caret = true;
        unsafe {
            // the notch is normally never active; take keyboard focus specifically for typing
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(self.hwnd);
            SetTimer(self.hwnd, TIMER_CARET, 530, None);
        }
    }

    pub(crate) fn stop_editing(&mut self) {
        if !self.editing {
            return;
        }
        self.editing = false;
        self.surrogate = None;
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_CARET);
        }
        self.schedule_close(260);
        self.redraw();
    }

    pub(crate) fn on_char(&mut self, c: u32) {
        if !self.editing {
            return;
        }
        if (0xD800..=0xDBFF).contains(&c) {
            self.surrogate = Some(c as u16);
            return;
        }
        let prev_surrogate = self.surrogate.take();
        match c {
            0x08 => {
                self.draft.pop();
            }
            0x0D => {
                if let Some(task) = tasks::Task::parse(&self.draft) {
                    self.tasks.push(task);
                    self.draft.clear();
                    self.task_scroll = self.tasks.len().saturating_sub(TASKS_VISIBLE);
                    self.save();
                }
            }
            0x1B => {
                self.draft.clear();
                self.stop_editing();
                return;
            }
            0x16 => {
                if let Some(text) = system::clipboard_text(self.hwnd) {
                    self.draft += text.lines().next().unwrap_or("");
                }
            }
            0xDC00..=0xDFFF => {
                if let Some(hi) = prev_surrogate {
                    if let Some(Ok(ch)) = char::decode_utf16([hi, c as u16]).next() {
                        self.draft.push(ch);
                    }
                }
            }
            c if c >= 0x20 && c != 0x7F => {
                if let Some(ch) = char::from_u32(c) {
                    self.draft.push(ch);
                }
            }
            _ => {}
        }
        self.draft = self.draft.chars().take(120).collect();
        let has_time = tasks::Task::parse(&self.draft).is_some_and(|t| t.due.is_some());
        if has_time != self.draft_time_since.is_some() {
            self.draft_time_since = has_time.then(Instant::now);
            if has_time {
                self.start_animation();
            }
        }
        self.caret = true;
        unsafe { SetTimer(self.hwnd, TIMER_CARET, 530, None) };
        self.redraw();
    }

    // ---- HUD, power, timers --------------------------------------------------------------

    fn show_hud(&mut self, hud: Hud) {
        let fresh = self.hud.is_none();
        self.hud = Some(hud);
        unsafe { SetTimer(self.hwnd, TIMER_HUD, 1800, None) };
        if fresh {
            self.bump();
        }
        self.retarget();
    }

    pub(crate) fn on_power(&mut self) {
        let now = system::battery();
        if let (Some((was, _)), Some((ac, pct))) = (self.battery, now) {
            if was != ac {
                self.show_hud(Hud::Power(ac, pct));
            }
        }
        self.battery = now;
    }

    pub(crate) fn on_timer(&mut self, id: usize) {
        match id {
            TIMER_ANIM => self.animate(),
            TIMER_LIVE => self.redraw(),
            TIMER_TICK => self.on_tick(),
            TIMER_CARET => {
                self.caret = !self.caret;
                self.redraw();
            }
            TIMER_LYRIC => self.sync_lyric(),
            _ => {
                unsafe {
                    let _ = KillTimer(self.hwnd, id);
                }
                match id {
                    TIMER_CLOSE if self.editing || self.drag_in || self.pinned => return,
                    TIMER_CLOSE => {
                        self.open = false;
                        self.hover_ready = false;
                        self.hover = None;
                    }
                    TIMER_HUD => {
                        self.hud = None;
                        self.redraw();
                    }
                    TIMER_ALERT => {
                        self.alert = false;
                        self.reminder = None;
                    }
                    TIMER_BLINK => self.blink = false,
                    TIMER_VOLUME => {
                        if let Some((level, muted)) = system::volume() {
                            self.show_hud(Hud::Volume(level, muted));
                        }
                    }
                    TIMER_HOVER => {
                        self.open_on_hover();
                        return;
                    }
                    TIMER_CAPS => self.show_hud(Hud::Caps(system::caps_lock())),
                    _ => {}
                }
                self.retarget();
            }
        }
    }

    fn notify(&mut self) {
        unsafe {
            SetTimer(self.hwnd, TIMER_ALERT, 8000, None);
        }
        self.bump();
        self.retarget();
    }

    fn on_tick(&mut self) {
        self.tick += 1;
        let minute = unsafe { GetLocalTime() }.wMinute;
        let mut dirty = minute != self.minute;
        self.minute = minute;

        let caps = system::caps_lock();
        if self.tick > 1 && caps != self.caps {
            self.caps = caps;
            self.show_hud(Hud::Caps(caps));
            dirty = true;
        }

        if self.focus.running() {
            dirty = true;
            if self.focus.remaining() <= 0.0 {
                self.focus.reset();
                self.alert = !self.open;
                self.happy_until = Some(Instant::now() + Duration::from_millis(4000));
                self.play_sound(system::Sound::Chime);
                self.notify();
            }
        }
        if self.happy_until.is_some_and(|t| Instant::now() >= t) {
            self.happy_until = None;
            dirty = true;
        }
        if self.clip_copied.is_some_and(|(_, t)| t.elapsed().as_secs_f32() >= 1.4) {
            self.clip_copied = None;
            dirty = true;
        }

        // task reminder
        let now = media::unix_now();
        if let Some(task) = self.tasks.iter_mut().find(|t| !t.done && !t.notified && t.due.is_some_and(|d| d <= now)) {
            task.notified = true;
            self.reminder = (!self.open).then(|| (task.text.clone(), task.label.clone()));
            self.play_sound(system::Sound::Chime);
            self.notify();
            dirty = true;
        }

        if self.open && self.info.playing {
            dirty = true; // progress bar moves
        }
        if self.live == Live::Idle && self.tick.is_multiple_of(5) {
            self.blink = true;
            unsafe { SetTimer(self.hwnd, TIMER_BLINK, 140, None) };
            dirty = true;
        }
        if self.tick.is_multiple_of(30) {
            self.battery = system::battery();
        }

        // walks the camera/mic consent registry trees: every 3 s is plenty for an indicator
        if self.tick.is_multiple_of(3) {
            let privacy = system::privacy_status();
            if privacy != self.privacy {
                self.privacy = privacy;
                self.bump();
                dirty = true;
            }
        }

        // CPU/RAM are only shown in the open header
        if self.open && self.tick.is_multiple_of(2) {
            let stats = (system::cpu_usage(), system::ram_usage());
            if stats != self.stats {
                self.stats = stats;
                dirty = true;
            }
        }

        // keyboard layout changed within the same window (not from switching apps)
        let keyboard = system::keyboard_layout();
        if keyboard.0 == self.keyboard.0 && keyboard.1 != self.keyboard.1 && keyboard.1 != 0 {
            self.show_hud(Hud::Layout(keyboard.1));
        }
        self.keyboard = keyboard;

        // step aside during games, fullscreen video, or presentations
        let hide = if self.config.behavior.auto_hide_fullscreen {
            system::fullscreen(self.hwnd, self.backdrop)
        } else {
            false
        };
        if hide != self.hidden {
            self.hidden = hide;
            if hide && self.open {
                self.open = false;
                self.pinned = false;
                self.hovering = false;
                self.hover_ready = false;
                self.retarget();
            }
            unsafe {
                let _ = ShowWindow(self.hwnd, if hide { SW_HIDE } else { SW_SHOWNOACTIVATE });
            }
            self.blur_shape = None;
            system::set_blur_shape(self.backdrop, None); // reappears on the next render
            self.update_live_timer();
            dirty = true;
        }

        self.sync_lyric();
        if dirty {
            self.redraw();
        }
    }

    pub(crate) fn on_media(&mut self) {
        let info = self.media.state.lock().unwrap().clone();
        let new_track = info.has_session && (info.title != self.info.title || !self.info.has_session);
        self.info = info;
        // lyrics column layout is recalculated automatically by view.rs if lyrics change
        self.lyrics = self.media.lyrics.lock().unwrap().clone();
        if new_track {
            self.lyric_line = None;
            self.bump();
        }
        self.sync_lyric();
        self.retarget();
    }

    /// Match current lyric line with track position, then set a timer for the next line.
    /// This keeps lyrics synced without continuous repainting.
    pub(crate) fn sync_lyric(&mut self) {
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_LYRIC);
        }
        let Some(lyrics) = lyrics_of(&self.lyrics).filter(|_| self.lyrics_on && self.info.has_session).cloned() else {
            if self.lyric_line.take().is_some() {
                self.retarget();
            }
            return;
        };
        let pos = self.info.position_now() + 0.2;
        let line = lyrics.index_at(pos);
        if line != self.lyric_line {
            self.lyric_line = line;
            if self.live == Live::Lyric {
                self.live_since = Instant::now(); // new line fades in
            }
            self.retarget();
        }
        if self.info.playing {
            let next = line.map_or(0, |i| i + 1);
            if let Some((t, _)) = lyrics.lines.get(next) {
                let ms = ((t - pos) * 1000.0).clamp(30.0, 60_000.0) as u32;
                unsafe { SetTimer(self.hwnd, TIMER_LYRIC, ms, None) };
            }
        }
    }

    fn save(&self) {
        let files: Vec<String> = self.shelf.iter().map(|i| i.path.clone()).collect();
        store::save(self.welcomed, self.lyrics_on, self.focus.minutes, &files, &self.tasks);
    }

    /// Redraw only when not animating (animation already draws every frame).
    pub(crate) fn redraw(&mut self) {
        if !self.animating && !self.hidden {
            self.render();
        }
    }

    /// Calculate DPI scale, prepare bitmap, and center the window at the top of the main screen.
    pub(crate) fn relayout(&mut self) {
        unsafe {
            self.scale = GetDpiForWindow(self.hwnd) as f32 / 96.0;
            let canvas_w = (self.config.notch.open_width + 80.0).max(CANVAS_W);
            let canvas_h = (self.config.notch.open_height + 40.0).max(CANVAS_H);
            self.pw = (canvas_w * self.scale).ceil() as u32;
            self.ph = (canvas_h * self.scale).ceil() as u32;

            if !self.bmp.is_invalid() {
                SelectObject(self.mem_dc, self.old_bmp);
                let _ = DeleteObject(self.bmp);
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.pw as i32,
                    biHeight: -(self.ph as i32), // negative = top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            self.bmp = CreateDIBSection(self.mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .expect("CreateDIBSection failed");
            self.bits = bits as *mut u8;
            self.old_bmp = SelectObject(self.mem_dc, self.bmp);

            let hmon = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTOPRIMARY);
            let mut minfo = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(hmon, &mut minfo).as_bool() {
                let mon_w = minfo.rcMonitor.right - minfo.rcMonitor.left;
                self.pos = POINT {
                    x: minfo.rcMonitor.left + (mon_w - self.pw as i32) / 2,
                    y: minfo.rcMonitor.top,
                };
            } else {
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                self.pos = POINT { x: (screen_w - self.pw as i32) / 2, y: 0 };
            }
        }
        self.blur_shape = None;
        system::set_blur_shape(self.backdrop, None);
        self.lyric_layout = None;
        self.render();
    }

    /// Glass blur shape tracks the notch. Only sent to Windows when changed.
    pub(crate) fn update_blur(&mut self, x0: f32, x1: f32, h: f32, r: f32, open: f32) {
        let s = self.scale;
        // slightly smaller than the notch: smooth edges are drawn by the main window
        let shape = (open > 0.02 && !self.hidden).then(|| {
            let left = (x0 * s).round() as i32 + 1;
            let right = (x1 * s).round() as i32 - 1;
            (self.pos.x + left, self.pos.y, right - left, (h * s).round() as i32 - 1, (r * s).round() as i32)
        });
        if shape != self.blur_shape {
            self.blur_shape = shape;
            system::set_blur_shape(self.backdrop, shape);
        }
    }

    /// Copy pixels (premultiplied RGBA) to Windows bitmap (BGRA) and present.
    pub(crate) fn present(&self, pm: &sk::Pixmap) {
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, (self.pw * self.ph * 4) as usize) };
        for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(pm.data().as_chunks::<4>().0) {
            *d = [s[2], s[1], s[0], s[3]];
        }
        let size = SIZE { cx: self.pw as i32, cy: self.ph as i32 };
        let origin = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&self.pos),
                Some(&size),
                self.mem_dc,
                Some(&origin),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }
}
