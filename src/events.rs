//! State and input: hover, clicks, file drags, typing, timers, lyrics, and reminders.

use crate::*;
use std::mem::discriminant;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

const SHELF_VISIBLE: usize = 5;
const TASKS_VISIBLE: usize = 4;

impl App {
    fn current_live(&self) -> Live {
        if let Some(hud) = self.hud {
            Live::Hud(hud)
        } else if self.alert {
            Live::Alert
        } else if self.reminder.is_some() {
            Live::Reminder
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
            Live::Timer | Live::Media => 236.0,
            Live::Lyric => {
                let text = self.lyric_text().unwrap_or("").to_string();
                let s = self.scale;
                let w = self.fonts.as_mut().map_or(0.0, |f| f.measure(&text, 13.0 * s, true) / s);
                (w + 108.0).clamp(236.0, 470.0)
            }
            Live::Idle => 150.0,
        }
    }

    /// Recalculate notch target dimensions and start its animation.
    pub(crate) fn retarget(&mut self) {
        let live = self.current_live();
        if discriminant(&live) != discriminant(&self.live) {
            self.live_since = Instant::now(); // new content fades in
        }
        self.live = live;
        let (w, h, r) = if self.open { OPEN } else { (self.live_width(live), BAR_H, 12.0) };
        self.w.to = w;
        self.h.to = h;
        self.r.to = r;
        self.start_animation();
        self.update_live_timer();
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
            self.w.v += 240.0;
            self.retarget();
        }
    }

    /// Equalizer only animates when music is playing and the notch is closed.
    fn update_live_timer(&mut self) {
        let want = !self.open && !self.hidden && matches!(self.live, Live::Media | Live::Lyric) && self.info.playing;
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

    fn animate(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        // opening = slightly bouncy, closing = calmer
        let zeta = if self.open { 0.62 } else { 0.75 };
        let steps = (dt * 240.0).ceil().max(1.0);
        for _ in 0..steps as u32 {
            for s in [&mut self.w, &mut self.h, &mut self.r] {
                s.step(dt / steps, zeta);
            }
            self.scroll.step(dt / steps, 0.9);
        }
        let springs = [self.w, self.h, self.r, self.scroll];
        if springs.iter().all(Spring::done) && self.content_alpha() >= 1.0 {
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
        }
        if !self.open {
            self.open = true;
            if self.alert {
                self.tab = Tab::Focus;
            }
            if self.reminder.is_some() {
                self.tab = Tab::Tasks;
            }
            self.alert = false;
            self.reminder = None;
            self.retarget();
        }

        if let (true, Some((Action::ShelfItem(i), px, py))) = (left_down, self.press) {
            if (x - px).hypot(y - py) > 5.0 {
                self.press = None;
                return self.shelf.get(i).map(|item| vec![item.path.clone()]);
            }
        }

        let hover = self.hit_rect(x, y).map(|(a, _)| a);
        if hover != self.hover {
            self.hover = hover;
            self.redraw();
        }
        None
    }

    pub(crate) fn on_mouse_leave(&mut self) {
        self.hovering = false;
        self.hover = None;
        self.press = None;
        // small delay so the notch doesn't flicker when the mouse grazes the edge
        unsafe { SetTimer(self.hwnd, TIMER_CLOSE, 260, None) };
        self.redraw();
    }

    pub(crate) fn on_press(&mut self, x: f32, y: f32) {
        let (x, y) = (x / self.scale, y / self.scale);
        self.press = self.hit_rect(x, y).map(|(a, _)| (a, x, y));
    }

    pub(crate) fn on_click(&mut self, x: f32, y: f32) {
        let (x, y) = (x / self.scale, y / self.scale);
        let pressed = self.press.take();
        let Some((action, rect)) = self.hit_rect(x, y) else {
            self.stop_editing();
            return;
        };
        if pressed.map(|p| p.0) != Some(action) {
            return; // button was pressed in one spot and released in another
        }
        if action != Action::TaskInput {
            self.stop_editing();
        }
        match action {
            Action::Tab(tab) => self.tab = tab,
            Action::Prev => self.media.send(media::Cmd::Prev),
            Action::Next => self.media.send(media::Cmd::Next),
            Action::Toggle => {
                self.media.send(media::Cmd::Toggle);
                self.info.playing = !self.info.playing; // update immediately, corrected by media thread
            }
            Action::LyricsToggle => {
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
            Action::FocusToggle => self.focus.toggle(),
            Action::FocusReset => self.focus.reset(),
            Action::Preset(m) => {
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
                    self.shelf.remove(i);
                    self.clamp_scroll();
                    self.save();
                }
            }
            Action::ShelfClear => {
                self.shelf.clear();
                self.shelf_scroll = 0;
                self.save();
            }
            Action::TaskToggle(i) => {
                if let Some(t) = self.tasks.get_mut(i) {
                    t.done = !t.done;
                    self.save();
                }
            }
            Action::TaskRemove(i) => {
                if i < self.tasks.len() {
                    self.tasks.remove(i);
                    self.clamp_scroll();
                    self.save();
                }
            }
            Action::TaskInput => self.start_editing(),
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
        if self.open && matches!(self.tab, Tab::Shelf | Tab::Tasks) {
            if self.tab == Tab::Shelf {
                self.shelf_scroll = self.shelf_scroll.saturating_add_signed(step);
            } else {
                self.task_scroll = self.task_scroll.saturating_add_signed(step);
            }
            self.clamp_scroll();
            self.redraw();
            return;
        }
        if let Some((level, muted)) = system::nudge_volume(delta as f32 / 120.0 * 0.04) {
            self.show_hud(Hud::Volume(level, muted));
        }
    }

    fn clamp_scroll(&mut self) {
        self.shelf_scroll = self.shelf_scroll.min(self.shelf.len().saturating_sub(SHELF_VISIBLE));
        self.task_scroll = self.task_scroll.min(self.tasks.len().saturating_sub(TASKS_VISIBLE));
    }

    // ---- Shelf: drag and drop in / out ---------------------------------------------------

    pub(crate) fn on_drag_enter(&mut self) {
        self.drag_in = true;
        self.open = true;
        self.tab = Tab::Shelf;
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_CLOSE);
        }
        self.retarget();
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
        self.schedule_close(400);
        self.redraw();
    }

    fn schedule_close(&self, ms: u32) {
        if !self.hovering {
            unsafe { SetTimer(self.hwnd, TIMER_CLOSE, ms, None) };
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
            c if c >= 0x20 && c != 0x7F => {
                if let Some(ch) = char::from_u32(c) {
                    self.draft.push(ch);
                }
            }
            _ => {}
        }
        self.draft = self.draft.chars().take(120).collect();
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
                    TIMER_CLOSE if self.editing || self.drag_in => return,
                    TIMER_CLOSE => {
                        self.open = false;
                        self.hover = None;
                    }
                    TIMER_HUD => self.hud = None,
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
                    TIMER_CAPS => self.show_hud(Hud::Caps(system::caps_lock())),
                    _ => {}
                }
                self.retarget();
            }
        }
    }

    fn notify(&mut self) {
        unsafe {
            let _ = PlaySoundW(windows::core::w!("SystemNotification"), None, SND_ALIAS | SND_ASYNC);
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

        if self.focus.running() {
            dirty = true;
            if self.focus.remaining() <= 0.0 {
                self.focus.reset();
                self.alert = !self.open;
                self.notify();
            }
        }

        // task reminder
        let now = media::unix_now();
        if let Some(task) = self.tasks.iter_mut().find(|t| !t.done && !t.notified && t.due.is_some_and(|d| d <= now)) {
            task.notified = true;
            self.reminder = (!self.open).then(|| task.text.clone());
            self.notify();
            dirty = true;
        }

        if self.open && self.info.playing {
            dirty = true; // progress bar moves
        }
        if self.live == Live::Idle && self.tick % 5 == 0 {
            self.blink = true;
            unsafe { SetTimer(self.hwnd, TIMER_BLINK, 140, None) };
            dirty = true;
        }
        if self.tick % 30 == 0 {
            self.battery = system::battery();
        }

        // keyboard layout changed within the same window (not from switching apps)
        let keyboard = system::keyboard_layout();
        if keyboard.0 == self.keyboard.0 && keyboard.1 != self.keyboard.1 && keyboard.1 != 0 {
            self.show_hud(Hud::Layout(keyboard.1));
        }
        self.keyboard = keyboard;

        // step aside during games, fullscreen video, or presentations
        let hide = system::fullscreen();
        if hide != self.hidden {
            self.hidden = hide;
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
        store::save(self.lyrics_on, self.focus.minutes, &files, &self.tasks);
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
            self.pw = (CANVAS_W * self.scale).ceil() as u32;
            self.ph = (CANVAS_H * self.scale).ceil() as u32;

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

            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            self.pos = POINT { x: (screen_w - self.pw as i32) / 2, y: 0 };
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
        for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
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
