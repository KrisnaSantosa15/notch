//! Notch layouts: closed state (live activity) and open state (tabs).

use crate::ui::*;
use crate::*;

const PRESETS: [u32; 4] = [5, 15, 25, 45];
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const SHELF_VISIBLE: usize = 5;
const TASKS_VISIBLE: usize = 4;
const LYRIC_ROW: f32 = 19.0;

impl App {
    pub(crate) fn render(&mut self) {
        let s = self.scale;
        let cx = CANVAS_W / 2.0;
        let w = self.w.x.max(60.0);
        let h = self.h.x.max(EAR + 6.0);
        let r = self.r.x.clamp(0.0, (h - EAR).min(w / 2.0));
        let (x0, x1) = (cx - w / 2.0, cx + w / 2.0);

        // How open the notch is (0..1), used for glass opacity and content cross-fading.
        let t = ((h - BAR_H) / (OPEN.1 - BAR_H)).clamp(0.0, 1.0);
        self.update_blur(x0, x1, h, r, t);

        let mut pm = sk::Pixmap::new(self.pw, self.ph).unwrap();
        let ts = sk::Transform::from_scale(s, s);
        let shape = notch_path(x0, x1, h, r, EAR);

        // soft shadow beneath open panel
        if t > 0.0 {
            for i in 1..=7 {
                let g = i as f32 * 2.6;
                if let Some(p) = rrect(x0 - g, -r, w + g * 2.0, h + r + g + 4.0, r + g) {
                    pm.fill_path(&p, &solid(BLACK, 0.05 * t * (1.0 - i as f32 / 8.0)), sk::FillRule::Winding, ts, None);
                }
            }
        }

        // body: solid black when closed (blends into screen bezel), dark glass when open
        let mix = |a: [u8; 3]| a.map(|v| (v as f32 * t) as u8);
        let alpha = 1.0 - 0.24 * t;
        let c = |rgb: [u8; 3]| sk::Color::from_rgba8(rgb[0], rgb[1], rgb[2], (alpha * 255.0) as u8);
        let body = sk::LinearGradient::new(
            sk::Point::from_xy(0.0, 0.0),
            sk::Point::from_xy(0.0, h),
            vec![sk::GradientStop::new(0.0, c(mix([34, 34, 40]))), sk::GradientStop::new(1.0, c(mix([14, 14, 18])))],
            sk::SpreadMode::Pad,
            sk::Transform::identity(),
        )
        .unwrap_or(sk::Shader::SolidColor(c(BLACK)));
        let body = sk::Paint { shader: body, anti_alias: true, ..Default::default() };
        pm.fill_path(&shape, &body, sk::FillRule::Winding, ts, None);

        let mut mask = sk::Mask::new(self.pw, self.ph).unwrap();
        mask.fill_path(&shape, sk::FillRule::Winding, true, ts);

        let closed_a = (1.0 - t * 3.0).clamp(0.0, 1.0) * self.content_alpha();
        let open_a = ((t - 0.55) / 0.45).clamp(0.0, 1.0);

        let mut fonts = self.fonts.take().unwrap();
        let mut f = Frame { pm, mask, s, fonts: &mut fonts };
        // thin reflective border highlight, characteristic of glass
        if t > 0.0 {
            f.stroke(Some(shape.clone()), 1.4, WHITE, 0.13 * t);
        }

        let mut hits = Vec::new();
        if closed_a > 0.0 {
            self.draw_live(&mut f, x0, x1, cx, closed_a);
        }
        if open_a > 0.0 {
            // buttons only become clickable once sufficiently visible
            let hits = if open_a > 0.6 { Some(&mut hits) } else { None };
            let mut c = Ctx { f: &mut f, hits, hover: self.hover, a: open_a };
            self.draw_header(&mut c, x0, x1);
            match self.tab {
                Tab::Music => self.draw_music(&mut c, x0, x1, cx),
                Tab::Focus => self.draw_focus(&mut c, x0, x1),
                Tab::Shelf => self.draw_shelf(&mut c, x0, x1, cx),
                Tab::Tasks => self.draw_tasks(&mut c, x0, x1, cx),
            }
        }

        let pm = f.pm;
        self.fonts = Some(fonts);
        self.hits = hits;
        // debug: NOTCH_DUMP=path.png dumps the last frame
        if let Ok(path) = std::env::var("NOTCH_DUMP") {
            let _ = pm.save_png(path);
        }
        self.present(&pm);
    }

    // ---- Closed notch -----------------------------------------------------------------------

    /// Single row 32 dp tall: icon on the left, label on the right.
    fn draw_live(&self, f: &mut Frame, x0: f32, x1: f32, cx: f32, a: f32) {
        let cy = BAR_H / 2.0;
        let (left, right) = (x0 + 22.0, x1 - 16.0);
        match self.live {
            Live::Hud(Hud::Volume(level, muted)) => {
                let waves = if muted || level <= 0.0 { 0 } else { 1 + (level * 2.99) as u8 };
                f.icon(Icon::Speaker(waves), left, cy, 16.0, WHITE, a);
                let label = if muted { "Muted".to_string() } else { format!("{}", (level * 100.0).round()) };
                f.text_right(right, 21.0, 12.0, true, WHITE, a, &label);
                let (bw, bx) = (128.0, right - 34.0 - 128.0);
                f.fill(rrect(bx, cy - 2.0, bw, 4.0, 2.0), WHITE, 0.18 * a);
                let fill = if muted { 0.0 } else { bw * level };
                f.fill(rrect(bx, cy - 2.0, fill, 4.0, 2.0), WHITE, a);
            }
            Live::Hud(Hud::Caps(on)) => {
                f.icon(Icon::Caps(on), left, cy, 15.0, WHITE, a);
                let (label, alpha) = if on { ("Caps Lock on", 1.0) } else { ("Caps Lock off", SOFT) };
                f.text_right(right, 21.0, 12.0, true, WHITE, a * alpha, label);
            }
            Live::Hud(Hud::Power(charging, pct)) => {
                f.icon(Icon::Battery(pct as f32 / 100.0, charging), left + 2.0, cy, 18.0, WHITE, a);
                let x = f.text_right(right, 21.0, 12.0, true, WHITE, a, &format!("{pct}%"));
                if charging {
                    f.text_right(x - 6.0, 21.0, 12.0, true, GREEN, a, "Charging");
                } else {
                    f.text_right(x - 6.0, 21.0, 12.0, true, WHITE, a * SOFT, "On battery");
                }
            }
            Live::Hud(Hud::Layout(lang)) => {
                let (code, name) = system::language_name(lang);
                f.stroke(rrect(left - 13.0, cy - 8.0, 26.0, 16.0, 4.5), 1.2, WHITE, 0.8 * a);
                f.text_center(left, cy + 3.5, 9.5, true, WHITE, a, &code);
                f.text_right(right, 21.0, 12.0, true, WHITE, a, &name);
            }
            Live::Alert => {
                f.ring(left, cy, 7.5, 2.4, 1.0, ORANGE, a);
                f.text_right(right, 21.0, 12.0, true, ORANGE, a, "Time's up");
            }
            Live::Reminder => {
                f.icon(Icon::Bell, left, cy, 15.0, ORANGE, a);
                let text = self.reminder.clone().unwrap_or_default();
                let text = f.fit(&text, 12.0, true, x1 - x0 - 70.0);
                f.text(left + 16.0, 21.0, 12.0, true, WHITE, a, &text);
            }
            Live::Timer => {
                f.ring(left, cy, 7.5, 2.4, self.focus.progress(), ORANGE, a);
                f.text_right(right, 21.0, 13.0, true, ORANGE, a, &mmss(self.focus.remaining().ceil()));
            }
            Live::Lyric => {
                self.mini_art(f, x0, a);
                let text = f.fit(self.lyric_text().unwrap_or(""), 13.0, true, x1 - x0 - 108.0);
                f.text_center(cx, 21.0, 13.0, true, WHITE, a, &text);
                self.draw_equalizer(f, right, cy, a);
            }
            Live::Media => {
                self.mini_art(f, x0, a);
                self.draw_equalizer(f, right, cy, a);
            }
            Live::Idle => f.eyes(cx, 15.0, 3.2, 9.0, self.blink, a),
        }

        // Privacy indicator dots (green for camera, orange for microphone)
        let (cam, mic) = self.privacy;
        if cam || mic {
            let mut dot_x = x1 - 8.0;
            if cam {
                f.circle(dot_x, cy, 2.5, GREEN, a);
                dot_x -= 7.0;
            }
            if mic {
                f.circle(dot_x, cy, 2.5, ORANGE, a);
            }
        }
    }

    fn mini_art(&self, f: &mut Frame, x0: f32, a: f32) {
        match &self.info.art {
            Some(art) => f.image(&art.image, x0 + 12.0, 6.0, 20.0, 6.0, a),
            None => f.art_placeholder(x0 + 12.0, 6.0, 20.0, 6.0, a),
        }
    }

    /// Four dancing equalizer bars when music plays, resting low when paused.
    fn draw_equalizer(&self, f: &mut Frame, right: f32, cy: f32, a: f32) {
        const SPEED: [f32; 4] = [7.3, 9.1, 6.2, 8.4];
        const PHASE: [f32; 4] = [0.0, 1.9, 3.3, 4.6];
        let t = self.started.elapsed().as_secs_f32();
        let color = self.accent();
        for i in 0..4 {
            let amp = if self.info.playing {
                let v = (t * SPEED[i] + PHASE[i]).sin() * 0.6 + (t * SPEED[i] * 1.7 + PHASE[i] * 2.0).sin() * 0.4;
                0.3 + 0.7 * (v * 0.5 + 0.5)
            } else {
                0.22
            };
            let h = 3.0 + 11.0 * amp;
            let x = right - 3.0 - (3 - i) as f32 * 5.5;
            f.fill(rrect(x, cy - h / 2.0, 3.0, h, 1.5), color, a);
        }
    }

    fn accent(&self) -> [u8; 3] {
        self.info.art.as_ref().map_or(WHITE, |art| art.accent)
    }

    // ---- Open notch -------------------------------------------------------------------------

    /// Top header: tabs on the left, battery (or date) and clock on the right.
    fn draw_header(&mut self, c: &mut Ctx, x0: f32, x1: f32) {
        let open_tasks = self.tasks.iter().filter(|t| !t.done).count();
        let tabs = [
            (Tab::Music, "Music", 0),
            (Tab::Focus, "Focus", 0),
            (Tab::Shelf, "Shelf", self.shelf.len()),
            (Tab::Tasks, "Tasks", open_tasks),
        ];
        let mut x = x0 + 22.0;
        for (tab, label, count) in tabs {
            let badge = (count > 0).then(|| count.to_string());
            x += c.pill(Action::Tab(tab), x, 12.0, label, badge.as_deref(), self.tab == tab, false) + 4.0;
        }

        let a = c.a;
        let now = unsafe { GetLocalTime() };
        let clock = format!("{:02}:{:02}", now.wHour, now.wMinute);
        let right = c.f.text_right(x1 - 26.0, 29.0, 13.0, true, WHITE, a, &clock);
        let left_info = match self.battery {
            Some((charging, pct)) => {
                c.f.icon(Icon::Battery(pct as f32 / 100.0, charging), right - 20.0, 24.5, 16.0, WHITE, a);
                c.f.text_right(right - 33.0, 29.0, 12.0, false, WHITE, a * SOFT, &format!("{pct}%"))
            }
            None => {
                let day = format!("{} {}", DAYS[now.wDayOfWeek as usize % 7], now.wDay);
                c.f.text_right(right - 8.0, 29.0, 13.0, false, WHITE, a * SOFT, &day)
            }
        };

        // System stats (CPU & RAM) displayed subtly in the header gap
        if self.stats.1 > 0 {
            let stats_text = format!("CPU {}%  RAM {}%", self.stats.0, self.stats.1);
            let stats_w = c.f.width(&stats_text, 11.0, false);
            if left_info - stats_w - 16.0 > x {
                c.f.text_right(left_info - 16.0, 29.0, 11.0, false, WHITE, a * SOFT * 0.75, &stats_text);
            }
        }

        let (cam, mic) = self.privacy;
        if cam || mic {
            let mut dot_x = x1 - 12.0;
            if cam {
                c.f.circle(dot_x, 18.0, 2.5, GREEN, a);
                dot_x -= 7.0;
            }
            if mic {
                c.f.circle(dot_x, 18.0, 2.5, ORANGE, a);
            }
        }
    }

    fn draw_music(&mut self, c: &mut Ctx, x0: f32, x1: f32, cx: f32) {
        let a = c.a;
        if !self.info.has_session {
            c.f.eyes(cx, 88.0, 6.0, 15.0, self.blink, a);
            c.f.text_center(cx, 134.0, 13.0, true, WHITE, a, "Nothing playing");
            c.f.text_center(cx, 154.0, 12.0, false, WHITE, a * SOFT, "Play something in Spotify or your browser");
            return;
        }
        if self.lyrics_on {
            self.draw_music_lyrics(c, x0, x1);
        } else {
            self.draw_music_full(c, x0, x1);
        }
    }

    fn art(&self, c: &mut Ctx, x: f32, y: f32, size: f32, r: f32) {
        match &self.info.art {
            Some(art) => c.f.image(&art.image, x, y, size, r, c.a),
            None => c.f.art_placeholder(x, y, size, r, c.a),
        }
    }

    fn controls(&self, c: &mut Ctx, xs: [f32; 4], cy: f32) {
        c.toggle(Action::LyricsToggle, xs[0], cy, 16.0, Icon::Lyrics, 14.0, self.lyrics_on, self.accent());
        c.button(Action::Prev, xs[1], cy, 17.0, Icon::Prev, 13.0);
        let play = if self.info.playing { Icon::Pause } else { Icon::Play };
        c.button(Action::Toggle, xs[2], cy, 20.0, play, 19.0);
        c.button(Action::Next, xs[3], cy, 17.0, Icon::Next, 13.0);
    }

    /// Clickable progress bar for seeking.
    fn progress(&self, c: &mut Ctx, x: f32, y: f32, w: f32, thick: f32) {
        let info = &self.info;
        if info.duration <= 0.0 {
            return;
        }
        let a = c.a;
        let hovered = c.hover == Some(Action::SeekBar);
        let thick = if hovered { thick + 2.0 } else { thick };
        let frac = (info.position_now() / info.duration) as f32;
        c.f.fill(rrect(x, y - thick / 2.0, w, thick, thick / 2.0), WHITE, 0.18 * a);
        c.f.fill(rrect(x, y - thick / 2.0, w * frac, thick, thick / 2.0), self.accent(), a);
        c.hit(Action::SeekBar, x, y - 8.0, w, 16.0);
    }

    fn draw_music_full(&mut self, c: &mut Ctx, x0: f32, x1: f32) {
        let a = c.a;
        let (ax, ay, size) = (x0 + 24.0, 50.0, 72.0);
        self.art(c, ax, ay, size, 14.0);

        let text_x = ax + size + 16.0;
        let max_w = (x1 - 180.0) - text_x;
        let title = if self.info.title.is_empty() { "Unknown title" } else { &self.info.title };
        let title = c.f.fit(title, 15.0, true, max_w);
        c.f.text(text_x, 80.0, 15.0, true, WHITE, a, &title);
        let artist = c.f.fit(&self.info.artist, 13.0, false, max_w);
        c.f.text(text_x, 100.0, 13.0, false, WHITE, a * SOFT, &artist);

        self.controls(c, [x1 - 156.0, x1 - 116.0, x1 - 78.0, x1 - 40.0], 86.0);

        if self.info.duration > 0.0 {
            let (bx, bw) = (x0 + 24.0, x1 - x0 - 48.0);
            self.progress(c, bx, 144.0, bw, 4.0);
            let pos = self.info.position_now();
            c.f.text(bx, 166.0, 11.0, false, WHITE, a * SOFT, &mmss(pos));
            let rest = format!("-{}", mmss(self.info.duration - pos));
            c.f.text_right(bx + bw, 166.0, 11.0, false, WHITE, a * SOFT, &rest);
        }
    }

    /// Lyrics mode: track info and controls on the left, scrolling lyrics on the right.
    fn draw_music_lyrics(&mut self, c: &mut Ctx, x0: f32, x1: f32) {
        let a = c.a;
        self.art(c, x0 + 24.0, 48.0, 56.0, 12.0);
        let (text_x, max_w) = (x0 + 92.0, 136.0);
        let title = if self.info.title.is_empty() { "Unknown title" } else { &self.info.title };
        let title = c.f.fit(title, 14.0, true, max_w);
        c.f.text(text_x, 70.0, 14.0, true, WHITE, a, &title);
        let artist = c.f.fit(&self.info.artist, 12.0, false, max_w);
        c.f.text(text_x, 88.0, 12.0, false, WHITE, a * SOFT, &artist);
        self.controls(c, [x0 + 40.0, x0 + 86.0, x0 + 130.0, x0 + 174.0], 126.0);
        self.progress(c, x0 + 24.0, 158.0, 204.0, 3.0);

        let (col_x, col_w) = (x0 + 252.0, x1 - 24.0 - (x0 + 252.0));
        let col_cx = col_x + col_w / 2.0;
        match &self.lyrics.state {
            _ if self.lyrics.key != self.info.key() => {
                c.f.text_center(col_cx, 108.0, 13.0, false, WHITE, a * SOFT, "Finding lyrics…");
            }
            media::LyricsState::Loading => {
                c.f.text_center(col_cx, 108.0, 13.0, false, WHITE, a * SOFT, "Finding lyrics…");
            }
            media::LyricsState::Missing => {
                c.f.text_center(col_cx, 100.0, 13.0, true, WHITE, a * 0.8, "No synced lyrics");
                c.f.text_center(col_cx, 119.0, 12.0, false, WHITE, a * SOFT, "for this song yet");
            }
            media::LyricsState::Found(lyrics) => {
                let lyrics = lyrics.clone();
                self.draw_lyric_column(c, &lyrics, col_x, col_w);
            }
        }
    }

    fn draw_lyric_column(&mut self, c: &mut Ctx, lyrics: &Arc<lyrics::Lyrics>, col_x: f32, col_w: f32) {
        let a = c.a;
        // wrap lines once per track, store their positions
        let source = Arc::as_ptr(lyrics) as usize;
        let fresh = self.lyric_layout.as_ref().map(|l| l.source) != Some(source);
        if fresh {
            let mut y = 0.0;
            let mut rows = Vec::new();
            for (_, text) in &lyrics.lines {
                let text = if text.is_empty() { "•  •  •" } else { text };
                let wrapped = c.f.fonts.wrap(text, 14.0 * c.f.s, true, col_w * c.f.s, 2);
                let wrapped: Vec<String> = wrapped.iter().map(|l| c.f.fit(l, 14.0, true, col_w)).collect();
                let height = wrapped.len() as f32 * LYRIC_ROW;
                rows.push((y, wrapped));
                y += height + 9.0;
            }
            self.lyric_layout = Some(LyricLayout { source, rows });
        }
        let layout = self.lyric_layout.take().unwrap();

        let current = self.lyric_line;
        let focus = current.unwrap_or(0);
        let target = layout.rows.get(focus).map_or(0.0, |r| r.0);
        if (self.scroll.to - target).abs() > 0.1 || fresh {
            self.scroll.to = target;
            if fresh {
                self.scroll.snap();
            } else {
                self.start_animation(); // smooth scroll to the next line
            }
        }

        for (i, (y, lines)) in layout.rows.iter().enumerate() {
            let top = 74.0 + y - self.scroll.x;
            let bottom = top + lines.len() as f32 * LYRIC_ROW;
            if bottom < 40.0 || top > 180.0 {
                continue;
            }
            let dist = (i as f32 - focus as f32).abs();
            let mut alpha = if current == Some(i) { 1.0 } else { (0.46 - dist * 0.1).max(0.16) };
            if c.hover == Some(Action::SeekLine(i)) {
                alpha = alpha.max(0.8);
            }
            // fade out at top (near tabs) and bottom edges
            let mid = (top + bottom) / 2.0;
            let fade = ((mid - 46.0) / 22.0).clamp(0.0, 1.0) * ((178.0 - mid) / 22.0).clamp(0.0, 1.0);
            for (k, row) in lines.iter().enumerate() {
                let baseline = top + 14.0 + k as f32 * LYRIC_ROW;
                c.f.text(col_x, baseline, 14.0, true, WHITE, a * alpha * fade, row);
            }
            if fade > 0.5 {
                c.hit(Action::SeekLine(i), col_x - 6.0, top - 3.0, col_w + 12.0, bottom - top + 6.0);
            }
        }
        self.lyric_layout = Some(layout);
    }

    fn draw_focus(&mut self, c: &mut Ctx, x0: f32, x1: f32) {
        let a = c.a;
        let focus = &self.focus;
        let (rx, ry, rr) = (x0 + 60.0, 94.0, 32.0);

        // progress ring also acts as play/pause toggle
        if c.hover == Some(Action::FocusToggle) {
            c.f.circle(rx, ry, rr - 4.0, WHITE, 0.08 * a);
        }
        c.f.ring(rx, ry, rr, 5.0, if focus.active() { focus.progress() } else { 0.0 }, ORANGE, a);
        let icon = if focus.running() { Icon::Pause } else { Icon::Play };
        c.f.icon(icon, rx + if focus.running() { 0.0 } else { 2.0 }, ry, 20.0, WHITE, a);
        c.hit(Action::FocusToggle, rx - rr, ry - rr, rr * 2.0, rr * 2.0);

        let text_x = x0 + 112.0;
        c.f.text(text_x, 102.0, 34.0, true, WHITE, a, &mmss(focus.remaining().ceil()));
        let status = if focus.running() {
            // completion time, calculated from current time + remaining duration
            let now = unsafe { GetLocalTime() };
            let end = (now.wHour as u32 * 60 + now.wMinute as u32 + (focus.remaining() / 60.0).ceil() as u32) % 1440;
            format!("Focusing · ends {:02}:{:02}", end / 60, end % 60)
        } else if focus.active() {
            "Paused".to_string()
        } else {
            "Ready when you are".to_string()
        };
        c.f.text(text_x, 124.0, 12.0, false, WHITE, a * SOFT, &status);

        if focus.active() {
            c.button(Action::FocusReset, x1 - 44.0, ry, 17.0, Icon::Reset, 16.0);
        }

        // duration presets; cannot be changed while running
        let running = focus.running();
        let minutes = focus.minutes;
        let mut x = x0 + 24.0;
        for m in PRESETS {
            let label = format!("{m} min");
            x += c.pill(Action::Preset(m), x, 144.0, &label, None, minutes == m, running) + 6.0;
        }
    }

    fn draw_shelf(&mut self, c: &mut Ctx, x0: f32, x1: f32, cx: f32) {
        let a = c.a;
        if self.shelf.is_empty() || self.drag_in {
            let active = self.drag_in;
            let zone = rrect(x0 + 24.0, 48.0, x1 - x0 - 48.0, 112.0, 18.0);
            c.f.fill(zone.clone(), WHITE, if active { 0.10 } else { 0.04 } * a);
            c.f.dashed(zone, 1.4, WHITE, if active { 0.75 } else { 0.28 } * a);
            c.f.icon(Icon::Tray, cx, 84.0, 24.0, WHITE, if active { 1.0 } else { 0.8 } * a);
            let title = if active { "Release to add" } else { "Drop files here" };
            c.f.text_center(cx, 124.0, 13.0, true, WHITE, a, title);
            c.f.text_center(cx, 143.0, 11.0, false, WHITE, a * SOFT, "Files stay where they are. Drag them out anytime.");
            return;
        }

        let (tile_w, tile_h, gap, y) = (76.0, 92.0, 8.0, 46.0);
        let len = self.shelf.len();
        for (slot, i) in (self.shelf_scroll..len).take(SHELF_VISIBLE).enumerate() {
            let tx = x0 + 24.0 + slot as f32 * (tile_w + gap);
            let hovered = matches!(c.hover, Some(Action::ShelfItem(j) | Action::ShelfRemove(j)) if j == i);
            if hovered {
                c.f.fill(rrect(tx, y, tile_w, tile_h, 14.0), WHITE, 0.08 * a);
            }
            let item = &self.shelf[i];
            match &item.thumb {
                Some(img) => c.f.image_fit(img, tx + tile_w / 2.0, y + 34.0, 48.0, 6.0, a),
                None => c.f.icon(Icon::Note, tx + tile_w / 2.0, y + 34.0, 30.0, WHITE, a * SOFT),
            }
            let name = c.f.fit(&item.name, 11.0, false, tile_w - 10.0);
            c.f.text_center(tx + tile_w / 2.0, y + 78.0, 11.0, false, WHITE, a * 0.85, &name);

            let (bx, by) = (tx + tile_w - 11.0, y + 11.0);
            if hovered {
                let over = c.hover == Some(Action::ShelfRemove(i));
                c.f.circle(bx, by, 9.0, if over { [90, 90, 96] } else { [62, 62, 68] }, a);
                c.f.icon(Icon::Close, bx, by, 8.0, WHITE, a);
            }
            c.hit(Action::ShelfRemove(i), bx - 9.0, by - 9.0, 18.0, 18.0);
            c.hit(Action::ShelfItem(i), tx, y, tile_w, tile_h);
        }

        let hint = if len > SHELF_VISIBLE {
            let first = self.shelf_scroll + 1;
            format!("{first}–{} of {len} · scroll for more", (first + SHELF_VISIBLE - 1).min(len))
        } else {
            "Drag out anytime · click to open".to_string()
        };
        c.f.text(x0 + 26.0, 162.0, 11.0, false, WHITE, a * SOFT, &hint);
        let clear_w = c.f.width("Clear", 12.0, true) + 24.0;
        c.pill(Action::ShelfClear, x1 - 24.0 - clear_w, 145.0, "Clear", None, false, false);
    }

    fn draw_tasks(&mut self, c: &mut Ctx, x0: f32, x1: f32, cx: f32) {
        let a = c.a;
        // pending tasks on top (nearest due time first), completed tasks at the bottom
        let mut order: Vec<usize> = (0..self.tasks.len()).collect();
        order.sort_by(|&i, &j| {
            let (ti, tj) = (&self.tasks[i], &self.tasks[j]);
            ti.done
                .cmp(&tj.done)
                .then(ti.due.unwrap_or(f64::MAX).total_cmp(&tj.due.unwrap_or(f64::MAX)))
        });

        if self.tasks.is_empty() {
            c.f.text_center(cx, 90.0, 14.0, true, WHITE, a, "Nothing to do");
            c.f.text_center(cx, 111.0, 12.0, false, WHITE, a * SOFT, "End a task with a time, like 15:30, to get a reminder.");
        }

        for (slot, &i) in order.iter().skip(self.task_scroll).take(TASKS_VISIBLE).enumerate() {
            let task = &self.tasks[i];
            let y = 44.0 + slot as f32 * 26.0;
            let hovered = matches!(c.hover, Some(Action::TaskToggle(j) | Action::TaskRemove(j)) if j == i);
            if hovered {
                c.f.fill(rrect(x0 + 16.0, y, x1 - x0 - 32.0, 26.0, 9.0), WHITE, 0.06 * a);
            }
            let (bx, by) = (x0 + 34.0, y + 13.0);
            if task.done {
                c.f.circle(bx, by, 8.0, GREEN, a);
                c.f.icon(Icon::Check, bx, by, 9.0, BLACK, a);
            } else {
                c.f.stroke(sk::PathBuilder::from_circle(bx, by, 7.5), 1.4, WHITE, 0.5 * a);
            }

            let text = c.f.fit(&task.text, 13.0, false, (x1 - 96.0) - (x0 + 52.0));
            let ta = if task.done { a * SOFT * 0.8 } else { a };
            c.f.text(x0 + 52.0, y + 18.0, 13.0, false, WHITE, ta, &text);
            if task.done {
                let tw = c.f.width(&text, 13.0, false);
                c.f.fill(rrect(x0 + 52.0, y + 13.0, tw, 1.2, 0.6), WHITE, ta);
            }

            if hovered {
                c.button(Action::TaskRemove(i), x1 - 36.0, by, 10.0, Icon::Close, 8.0);
            } else if !task.label.is_empty() {
                let (rgb, la) = if task.overdue() { (ORANGE, a) } else { (WHITE, a * SOFT) };
                c.f.text_right(x1 - 28.0, y + 18.0, 12.0, false, rgb, la, &task.label);
            }
            c.hit(Action::TaskRemove(i), x1 - 46.0, y, 20.0, 26.0);
            c.hit(Action::TaskToggle(i), x0 + 16.0, y, x1 - x0 - 66.0, 26.0);
        }

        // add task input bar
        let (fx, fy, fw) = (x0 + 24.0, 150.0, x1 - x0 - 48.0);
        let bg = if self.editing {
            0.13
        } else if c.hover == Some(Action::TaskInput) {
            0.10
        } else {
            0.06
        };
        c.f.fill(rrect(fx, fy, fw, 24.0, 12.0), WHITE, bg * a);
        if self.editing {
            c.f.stroke(rrect(fx, fy, fw, 24.0, 12.0), 1.0, WHITE, 0.3 * a);
        }
        c.f.icon(Icon::Plus, fx + 14.0, fy + 12.0, 10.0, WHITE, a * SOFT);
        let text_x = fx + 28.0;
        let max_w = fw - 44.0;
        if self.draft.is_empty() {
            let hint = if self.editing { "Type and press Enter" } else { "Add a task… try “Stand-up 09:30”" };
            c.f.text(text_x, fy + 16.5, 12.0, false, WHITE, a * SOFT * 0.85, hint);
        } else {
            // long text: show trailing portion where user is typing
            let mut shown: String = self.draft.clone();
            while c.f.width(&shown, 12.0, false) > max_w && !shown.is_empty() {
                shown.remove(0);
            }
            c.f.text(text_x, fy + 16.5, 12.0, false, WHITE, a, &shown);
        }
        if self.editing && self.caret {
            let shown_w = c.f.width(&self.draft, 12.0, false).min(max_w);
            let cx = if self.draft.is_empty() { text_x } else { text_x + shown_w + 1.0 };
            c.f.fill(rrect(cx, fy + 5.0, 1.5, 14.0, 0.75), WHITE, a);
        }
        c.hit(Action::TaskInput, fx, fy, fw, 24.0);
    }
}

/// Open notch rendering context: canvas, hit test regions, and hovered action.
struct Ctx<'f, 'a> {
    f: &'f mut Frame<'a>,
    hits: Option<&'f mut Vec<(Action, sk::Rect)>>,
    hover: Option<Action>,
    a: f32,
}

impl Ctx<'_, '_> {
    fn hit(&mut self, action: Action, x: f32, y: f32, w: f32, h: f32) {
        if let (Some(hits), Some(rect)) = (self.hits.as_mut(), sk::Rect::from_xywh(x, y, w, h)) {
            hits.push((action, rect));
        }
    }

    /// Circular icon button with subtle hover background.
    fn button(&mut self, action: Action, cx: f32, cy: f32, r: f32, icon: Icon, size: f32) {
        if self.hover == Some(action) {
            self.f.circle(cx, cy, r, WHITE, 0.12 * self.a);
        }
        self.f.icon(icon, cx, cy, size, WHITE, self.a);
        self.hit(action, cx - r, cy - r, r * 2.0, r * 2.0);
    }

    /// Toggle button: highlighted with accent color when active.
    #[allow(clippy::too_many_arguments)]
    fn toggle(&mut self, action: Action, cx: f32, cy: f32, r: f32, icon: Icon, size: f32, on: bool, accent: [u8; 3]) {
        let a = self.a;
        if on {
            self.f.circle(cx, cy, r, accent, 0.26 * a);
        } else if self.hover == Some(action) {
            self.f.circle(cx, cy, r, WHITE, 0.12 * a);
        }
        let color = if on { accent } else { WHITE };
        self.f.icon(icon, cx, cy, size, color, if on { a } else { a * 0.75 });
        self.hit(action, cx - r, cy - r, r * 2.0, r * 2.0);
    }

    /// Text pill (tab or preset), with optional small badge. Returns its width.
    #[allow(clippy::too_many_arguments)]
    fn pill(&mut self, action: Action, x: f32, y: f32, label: &str, badge: Option<&str>, selected: bool, disabled: bool) -> f32 {
        let a = self.a;
        let label_w = self.f.width(label, 12.0, true);
        let badge_w = badge.map_or(0.0, |b| self.f.width(b, 11.0, true) + 5.0);
        let w = label_w + badge_w + 24.0;
        let bg = if selected {
            0.16
        } else if self.hover == Some(action) && !disabled {
            0.08
        } else {
            0.0
        };
        if bg > 0.0 {
            self.f.fill(rrect(x, y, w, 24.0, 12.0), WHITE, bg * a);
        }
        let alpha = if selected {
            1.0
        } else if disabled {
            0.3
        } else {
            SOFT
        };
        self.f.text(x + 12.0, y + 16.5, 12.0, true, WHITE, a * alpha, label);
        if let Some(b) = badge {
            self.f.text(x + 17.0 + label_w, y + 16.5, 11.0, true, WHITE, a * alpha * 0.6, b);
        }
        if !disabled {
            self.hit(action, x, y, w, 24.0);
        }
        w
    }
}

fn mmss(sec: f64) -> String {
    let s = sec.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
