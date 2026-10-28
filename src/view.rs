//! Notch layouts: closed state (live activity) and open state (tabs).

use crate::ui::*;
use crate::*;

const PRESETS: [u32; 4] = [5, 15, 25, 45];
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const SHELF_VISIBLE: usize = 5;
const TASKS_VISIBLE: usize = 3;
const LYRIC_ROW: f32 = 19.0;

impl App {
    pub(crate) fn render(&mut self) {
        let s = self.scale;
        let cx = (self.pw as f32 / s) / 2.0;
        let w = self.w.x.max(60.0);
        let h = self.h.x.max(EAR + 6.0);
        let r = self.r.x.clamp(0.0, (h - EAR).min(w / 2.0));
        let (x0, x1) = (cx - w / 2.0, cx + w / 2.0);

        // How open the notch is (0..1), used for glass opacity and content cross-fading.
        let (open_h, closed_h) = (self.config.notch.open_height, self.config.notch.closed_height);
        let t = ((h - closed_h) / (open_h - closed_h).max(1.0)).clamp(0.0, 1.0);
        self.update_blur(x0, x1, h, r, t);

        let mut pm = sk::Pixmap::new(self.pw, self.ph).unwrap();
        let ts = sk::Transform::from_scale(s, s);
        let shape = notch_path(x0, x1, h, r, EAR);

        let has_satellite = !self.open && self.focus.running() && self.info.has_session;
        let sat_w = 32.0;
        let sat_h = self.config.notch.closed_height;
        let sat_x = x1 + EAR + 8.0;
        let sat_r = 16.0;
        let sat_path = has_satellite.then(|| rrect(sat_x, 0.0, sat_w, sat_h, sat_r)).flatten();

        // soft shadow beneath open panel
        if t > 0.0 {
            for i in 1..=7 {
                let g = i as f32 * 2.6;
                if let Some(p) = rrect(x0 - g, -r, w + g * 2.0, h + r + g + 4.0, r + g) {
                    pm.fill_path(&p, &solid(BLACK, 0.05 * t * (1.0 - i as f32 / 8.0)), sk::FillRule::Winding, ts, None);
                }
            }
        }

        // Feature 1B: Ambient backdrop underglow during music playback
        if self.info.playing {
            let accent = self.accent();
            let pulse = (self.started.elapsed().as_secs_f32() * 2.2).sin() * 0.15 + 0.85;
            for i in 1..=4 {
                let spread = i as f32 * 3.2;
                let alpha = (0.040 / i as f32) * pulse;
                if let Some(p) = rrect(x0 - spread, -r, w + spread * 2.0, h + r + spread + 2.0, r + spread) {
                    pm.fill_path(&p, &solid(accent, alpha), sk::FillRule::Winding, ts, None);
                }
            }
        }

        // Shelf drag-in: ripples radiate outward from the notch outline (drawn unmasked, outside the body)
        if self.drag_in {
            let accent = self.accent();
            let time = self.started.elapsed().as_secs_f32();
            let stroke = sk::Stroke { width: 1.6, ..Default::default() };
            for i in 0..3 {
                let phase = (time * 1.7 + i as f32 / 3.0).fract();
                let e = phase * 28.0;
                let ring = notch_path(x0 - e, x1 + e, h + e, r + e * 0.6, EAR);
                let alpha = (phase * std::f32::consts::PI).sin() * 0.55 * t;
                pm.stroke_path(&ring, &solid(accent, alpha), &stroke, ts, None);
            }
        }

        // body: solid black when closed (blends into screen bezel), dark glass when open
        let mix = |a: [u8; 3]| a.map(|v| (v as f32 * t) as u8);
        let alpha = 1.0 - 0.04 * t;
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
        if let Some(ref sp) = sat_path {
            pm.fill_path(sp, &body, sk::FillRule::Winding, ts, None);
        }

        let mut mask = sk::Mask::new(self.pw, self.ph).unwrap();
        mask.fill_path(&shape, sk::FillRule::Winding, true, ts);
        if let Some(ref sp) = sat_path {
            mask.fill_path(sp, sk::FillRule::Winding, true, ts);
        }

        let closed_a = (1.0 - t * 3.0).clamp(0.0, 1.0) * self.content_alpha();
        let open_a = ((t - 0.55) / 0.45).clamp(0.0, 1.0);

        let mut fonts = self.fonts.take().unwrap();
        let mut f = Frame { pm, mask, s, fonts: &mut fonts };
        // Specular glass rim highlight: ambient reflection + dynamic cursor glint
        if t > 0.0 {
            f.stroke(Some(shape.clone()), 1.2, WHITE, 0.11 * t);

            if let Some((mx, _my)) = self.mouse_pos {
                let glint_w = 95.0;
                let gx0 = (mx - glint_w).max(x0 - EAR);
                let gx1 = (mx + glint_w).min(x1 + EAR);
                if gx1 > gx0 {
                    let grad = sk::LinearGradient::new(
                        sk::Point::from_xy(gx0, 0.0),
                        sk::Point::from_xy(gx1, 0.0),
                        vec![
                            sk::GradientStop::new(0.0, sk::Color::from_rgba8(255, 255, 255, 0)),
                            sk::GradientStop::new(0.5, sk::Color::from_rgba8(255, 255, 255, (0.32 * t * 255.0) as u8)),
                            sk::GradientStop::new(1.0, sk::Color::from_rgba8(255, 255, 255, 0)),
                        ],
                        sk::SpreadMode::Pad,
                        sk::Transform::identity(),
                    );
                    if let Some(shader) = grad {
                        let stroke_paint = sk::Paint { shader, anti_alias: true, ..Default::default() };
                        let stroke_style = sk::Stroke { width: 1.6, ..Default::default() };
                        f.pm.stroke_path(&shape, &stroke_paint, &stroke_style, ts, None);
                    }
                }
            }
        }

        let mut hits = Vec::new();
        if closed_a > 0.0 {
            self.draw_live(&mut f, &mut hits, x0, x1, cx, closed_a);
            if has_satellite {
                let sat_cy = sat_h / 2.0;
                match &self.info.art {
                    Some(art) => f.image(&art.image, sat_x + 6.0, 6.0, 20.0, 6.0, closed_a),
                    None => {
                        self.draw_equalizer(&mut f, sat_x + sat_w - 6.0, sat_cy, closed_a);
                    }
                }
                if let Some(rect) = sk::Rect::from_xywh(sat_x, 0.0, sat_w, sat_h) {
                    hits.push((Action::Tab(Tab::Music), rect));
                }
            }

            // Bottom-rim ambient progress line for active Pomodoro
            let progress = if self.focus.running() {
                Some(self.focus.progress())
            } else {
                None
            };
            if let Some(p) = progress {
                let track_x0 = x0 + 16.0;
                let track_x1 = x1 - 16.0;
                let track_w = (track_x1 - track_x0).max(10.0);
                let track_y = h - 2.5;
                let bar_w = track_w * p.clamp(0.0, 1.0);
                let accent = self.accent();

                // Subtle track guide
                f.fill(rrect(track_x0, track_y, track_w, 1.8, 1.0), WHITE, 0.10 * closed_a);

                // Luminous progress indicator
                if bar_w > 1.0 {
                    f.fill(rrect(track_x0, track_y - 0.5, bar_w, 2.8, 1.4), accent, 0.35 * closed_a);
                    f.fill(rrect(track_x0, track_y, bar_w, 1.8, 1.0), accent, 0.90 * closed_a);
                }
            }
        }
        if open_a > 0.0 {
            // buttons only become clickable once sufficiently visible
            let hits = if open_a > 0.6 { Some(&mut hits) } else { None };
            let mut c = Ctx { f: &mut f, hits, hover: self.hover, a: open_a };
            if !self.is_tab_enabled(self.tab) {
                self.tab = self.first_enabled_tab();
            }
            self.draw_header(&mut c, x0, x1);
            match self.tab {
                Tab::Music => self.draw_music(&mut c, x0, x1, cx),
                Tab::Focus => self.draw_focus(&mut c, x0, x1),
                Tab::Shelf => self.draw_shelf(&mut c, x0, x1, cx),
                Tab::Tasks => self.draw_tasks(&mut c, x0, x1, cx),
                Tab::Settings => self.draw_settings(&mut c, x0, x1, cx),
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
    fn draw_live(&self, f: &mut Frame, hits: &mut Vec<(Action, sk::Rect)>, x0: f32, x1: f32, cx: f32, a: f32) {
        let cy = self.config.notch.closed_height / 2.0;
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
                f.fill(rrect(bx, cy - 2.0, fill, 4.0, 2.0), self.accent(), a);
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
                let color = self.accent();
                f.ring(left, cy, 7.5, 2.4, 1.0, color, a);
                f.text_right(right, 21.0, 12.0, true, color, a, "Time's up");
            }
            Live::Reminder => {
                // the bell rings (decaying swing) as the reminder lands; its time sits on the right
                let accent = self.accent();
                let e = self.live_since.elapsed().as_secs_f32();
                let swing = (e * 30.0).sin() * 20.0 * (-e * 4.5).exp();
                f.bell(left, cy, 15.0, swing, accent, a);
                let (text, label) = self.reminder.clone().unwrap_or_default();
                let label_x = if label.is_empty() { right } else { f.text_right(right, 21.0, 12.0, true, accent, a, &label) - 10.0 };
                let text = f.fit(&text, 12.0, true, label_x - (left + 16.0));
                f.text(left + 16.0, 21.0, 12.0, true, WHITE, a, &text);
            }
            Live::Welcome => {
                let t = self.started.elapsed().as_secs_f32();

                // Natural, expressive eye gaze cycle (Center -> Look Right -> Look Left -> Center)
                let cycle = t % 7.2;
                let look = if cycle < 2.0 {
                    0.0 // Center
                } else if cycle < 2.5 {
                    let p = (cycle - 2.0) / 0.5;
                    p * 2.4 // Smooth glance to the right
                } else if cycle < 4.4 {
                    2.4 // Hold looking right
                } else if cycle < 4.9 {
                    let p = (cycle - 4.4) / 0.5;
                    2.4 - p * 4.8 // Smooth glance to the left
                } else if cycle < 6.7 {
                    -2.4 // Hold looking left
                } else {
                    let p = (cycle - 6.7) / 0.5;
                    -2.4 + p * 2.4 // Smooth return to center
                };

                // Natural blink once every 3.8 seconds for ~150ms
                let blink = (t % 3.8) < 0.15;
                f.animated_eyes(left, cy, 3.2, 8.0, blink, look, a);

                let tips = [
                    "Welcome to Notch",
                    "Hover or click to open",
                    "Drag files here to Shelf",
                    "Press Win+Alt+N",
                ];
                let idx = ((t / 3.0) as usize) % tips.len();
                f.text_center(cx, 21.0, 12.0, true, WHITE, a, tips[idx]);

                let color = self.accent();
                for i in 0..3 {
                    let wave = ((t * 5.5 + i as f32 * 1.8).sin() * 0.5 + 0.5) * 8.0 + 3.0;
                    let bar_x = right - (2 - i) as f32 * 5.5;
                    f.fill(rrect(bar_x, cy - wave / 2.0, 2.8, wave, 1.4), color, a * 0.9);
                }
            }
            Live::Timer => {
                let color = self.accent();
                f.ring(left, cy, 7.5, 2.4, self.focus.progress(), color, a);
                f.text_right(right, 21.0, 13.0, true, color, a, &mmss(self.focus.remaining().ceil()));
            }
            Live::Media | Live::Lyric => {
                self.mini_art(f, x0, a);
                if let Some(r) = sk::Rect::from_xywh(x0, 0.0, 42.0, self.config.notch.closed_height) {
                    hits.push((Action::Tab(Tab::Music), r));
                }

                if self.hovering && self.info.has_session {
                    let avail_w = (x1 - x0 - 130.0).max(40.0);
                    let title = if self.info.title.is_empty() {
                        "Playing Media"
                    } else {
                        &self.info.title
                    };
                    let fit_title = f.fit(title, 11.5, true, avail_w);
                    f.text(x0 + 40.0, 20.5, 11.5, true, WHITE, a, &fit_title);
                    if let Some(r) = sk::Rect::from_xywh(x0 + 40.0, 0.0, avail_w, self.config.notch.closed_height) {
                        hits.push((Action::Tab(Tab::Music), r));
                    }

                    let p_x = x1 - 66.0;
                    let t_x = x1 - 44.0;
                    let n_x = x1 - 22.0;

                    if self.hover == Some(Action::Prev) {
                        f.circle(p_x, cy, 9.0, WHITE, 0.18 * a);
                    }
                    f.icon(Icon::Prev, p_x, cy, 10.0, WHITE, 0.9 * a);
                    if let Some(r) = sk::Rect::from_xywh(p_x - 10.0, cy - 10.0, 20.0, 20.0) {
                        hits.push((Action::Prev, r));
                    }

                    let play_icon = if self.info.playing { Icon::Pause } else { Icon::Play };
                    if self.hover == Some(Action::Toggle) {
                        f.circle(t_x, cy, 11.0, WHITE, 0.22 * a);
                    }
                    f.icon(play_icon, t_x, cy, 12.0, WHITE, a);
                    if let Some(r) = sk::Rect::from_xywh(t_x - 11.0, cy - 11.0, 22.0, 22.0) {
                        hits.push((Action::Toggle, r));
                    }

                    if self.hover == Some(Action::Next) {
                        f.circle(n_x, cy, 9.0, WHITE, 0.18 * a);
                    }
                    f.icon(Icon::Next, n_x, cy, 10.0, WHITE, 0.9 * a);
                    if let Some(r) = sk::Rect::from_xywh(n_x - 10.0, cy - 10.0, 20.0, 20.0) {
                        hits.push((Action::Next, r));
                    }
                } else {
                    if self.live == Live::Lyric {
                        let text = f.fit(self.lyric_text().unwrap_or(""), 13.0, true, x1 - x0 - 108.0);
                        f.text_center(cx, 21.0, 13.0, true, WHITE, a, &text);
                    }
                    self.draw_equalizer(f, right, cy, a);
                }
            }
            Live::Idle => f.eyes(cx, 15.0, 3.2, 9.0, self.blink, a),
        }

        // Privacy indicator dots (green for camera, orange for microphone)
        let (cam, mic) = self.privacy;
        if cam || mic {
            let mut dot_x = x1 - 20.0;
            if cam {
                f.circle(dot_x, cy, 6.0, GREEN, 0.28 * a);
                f.circle(dot_x, cy, 3.5, GREEN, a);
                dot_x -= 12.0;
            }
            if mic {
                f.circle(dot_x, cy, 6.0, ORANGE, 0.28 * a);
                f.circle(dot_x, cy, 3.5, ORANGE, a);
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
        if self.config.theme.accent_mode.eq_ignore_ascii_case("custom") {
            if let Some(rgb) = config::parse_hex_color(&self.config.theme.custom_accent) {
                return rgb;
            }
        }
        self.info
            .art
            .as_ref()
            .map(|art| art.accent)
            .or_else(|| config::parse_hex_color(&self.config.theme.custom_accent))
            .unwrap_or(ORANGE)
    }

    // ---- Open notch -------------------------------------------------------------------------

    /// Top header: tabs on the left, battery (or date) and clock on the right.
    fn draw_header(&mut self, c: &mut Ctx, x0: f32, x1: f32) {
        let open_tasks = self.tasks.iter().filter(|t| !t.done).count();
        let mut tabs = Vec::new();
        if self.config.tabs.media {
            tabs.push((Tab::Music, "Music", 0));
        }
        if self.config.tabs.focus {
            tabs.push((Tab::Focus, "Focus", 0));
        }
        if self.config.tabs.shelf {
            tabs.push((Tab::Shelf, "Shelf", self.shelf.len()));
        }
        if self.config.tabs.tasks {
            tabs.push((Tab::Tasks, "Tasks", open_tasks));
        }
        tabs.push((Tab::Settings, "Settings", 0));
        let mut x = x0 + 22.0;
        for (tab, label, count) in tabs {
            let badge = (count > 0).then(|| count.to_string());
            x += c.pill(Action::Tab(tab), x, 12.0, label, badge.as_deref(), self.tab == tab, false) + 4.0;
        }

        if let Some(info) = update::UPDATE_AVAILABLE.read().ok().and_then(|u| u.clone()) {
            let label = format!("Update {}", info.tag);
            x += c.pill(Action::OpenUpdate, x, 12.0, &label, None, false, false) + 4.0;
        }

        let a = c.a;
        if let Some(Hud::Volume(level, muted)) = self.hud {
            let accent = self.accent();
            let waves = if muted || level <= 0.0 { 0 } else { 1 + (level * 2.99) as u8 };
            let vol_txt = if muted { "Muted".to_string() } else { format!("{}%", (level * 100.0).round()) };
            let pill_w = 124.0;
            let pill_h = 24.0;
            let pill_x = x1 - 26.0 - pill_w;
            let pill_y = 12.0;

            // Sleek volume capsule
            c.f.fill(rrect(pill_x, pill_y, pill_w, pill_h, 12.0), WHITE, 0.12 * a);
            c.f.stroke(rrect(pill_x, pill_y, pill_w, pill_h, 12.0), 1.0, accent, 0.35 * a);

            // Speaker icon
            c.f.icon(Icon::Speaker(waves), pill_x + 14.0, pill_y + 12.0, 14.0, WHITE, a);

            // Mini level bar
            let bar_x = pill_x + 28.0;
            let bar_w = 50.0;
            let bar_y = pill_y + 10.0;
            c.f.fill(rrect(bar_x, bar_y, bar_w, 4.0, 2.0), WHITE, 0.20 * a);
            let fill_w = if muted { 0.0 } else { bar_w * level };
            if fill_w > 0.0 {
                c.f.fill(rrect(bar_x, bar_y, fill_w, 4.0, 2.0), accent, a);
            }

            // Percentage text
            c.f.text_right(pill_x + pill_w - 8.0, pill_y + 16.5, 11.0, true, WHITE, a, &vol_txt);
        } else {
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
        }

        let (cam, mic) = self.privacy;
        if cam || mic {
            let mut dot_x = x1 - 16.0;
            if cam {
                c.f.circle(dot_x, 18.0, 5.0, GREEN, 0.28 * a);
                c.f.circle(dot_x, 18.0, 3.0, GREEN, a);
                dot_x -= 10.0;
            }
            if mic {
                c.f.circle(dot_x, 18.0, 5.0, ORANGE, 0.28 * a);
                c.f.circle(dot_x, 18.0, 3.0, ORANGE, a);
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
        let fresh = !self.lyric_layout.as_ref().is_some_and(|l| Arc::ptr_eq(&l.source, lyrics));
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
            self.lyric_layout = Some(LyricLayout { source: lyrics.clone(), rows });
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
        let accent = self.accent();
        let focus = &self.focus;
        let (rx, ry, rr) = (x0 + 60.0, 94.0, 32.0);

        // progress ring also acts as play/pause toggle
        if c.hover == Some(Action::FocusToggle) {
            c.f.circle(rx, ry, rr - 4.0, WHITE, 0.08 * a);
        }
        c.f.ring(rx, ry, rr, 5.0, if focus.active() { focus.progress() } else { 0.0 }, accent, a);
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
            let accent = self.accent();

            if active {
                // ripples live outside the notch (see render); inside, the tray icon bobs gently
                let bob = (self.started.elapsed().as_secs_f32() * 9.0).sin() * 1.5;
                c.f.icon(Icon::Tray, cx, 84.0 + bob, 26.0, accent, a);
                c.f.text_center(cx, 124.0, 13.5, true, WHITE, a, "Release to add to Shelf");
                c.f.text_center(cx, 143.0, 11.0, false, accent, 0.85 * a, "Drop anywhere to store");
            } else {
                c.f.fill(zone.clone(), WHITE, 0.04 * a);
                c.f.dashed(zone, 1.4, WHITE, 0.28 * a);
                c.f.icon(Icon::Tray, cx, 84.0, 24.0, WHITE, 0.8 * a);
                c.f.text_center(cx, 124.0, 13.0, true, WHITE, a, "Drop files here");
                c.f.text_center(cx, 143.0, 11.0, false, WHITE, a * SOFT, "Files stay where they are. Drag them out anytime.");
            }
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
            c.f.text_center(cx, 68.0, 14.0, true, WHITE, a, "Nothing to do");
            c.f.text_center(cx, 89.0, 12.0, false, WHITE, a * SOFT, "End a task with a time, like 15:30, to get a reminder.");
        }

        for (slot, &i) in order.iter().skip(self.task_scroll).take(TASKS_VISIBLE).enumerate() {
            let task = &self.tasks[i];
            let y = 42.0 + slot as f32 * 26.0;
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

        // add task input bar comfortably above bottom curve
        let (fx, fy, fw) = (x0 + 32.0, 124.0, x1 - x0 - 64.0);
        let bg = if self.editing {
            0.14
        } else if c.hover == Some(Action::TaskInput) {
            0.10
        } else {
            0.06
        };
        c.f.fill(rrect(fx, fy, fw, 26.0, 13.0), WHITE, bg * a);
        let accent = self.accent();
        if self.editing {
            c.f.stroke(rrect(fx, fy, fw, 26.0, 13.0), 1.0, accent, 0.55 * a);
        }
        // once the draft ends with a time it becomes a reminder: the + turns into a bell that pops in
        let parsed = tasks::Task::parse(&self.draft);
        match self.draft_time_since.filter(|_| parsed.as_ref().is_some_and(|t| t.due.is_some())) {
            Some(since) => {
                let x = (since.elapsed().as_secs_f32() / 0.3).min(1.0) - 1.0;
                let pop = 1.0 + 2.7 * x * x * x + 1.7 * x * x; // ease-out-back
                c.f.bell(fx + 14.0, fy + 13.0, 13.0 * pop, 0.0, accent, a);
            }
            None => c.f.icon(Icon::Plus, fx + 14.0, fy + 13.0, 10.0, WHITE, a * SOFT),
        }
        let submit = self.editing && parsed.is_some();
        let text_x = fx + 28.0;
        let max_w = fw - 44.0 - if submit { 20.0 } else { 0.0 };
        if self.draft.is_empty() {
            let hint = if self.editing { "Type and press Enter" } else { "Add a task… try “Stand-up 09:30”" };
            c.f.text(text_x, fy + 17.5, 12.0, false, WHITE, a * SOFT * 0.85, hint);
        } else {
            // long text: show trailing portion where user is typing
            let mut shown: String = self.draft.clone();
            while c.f.width(&shown, 12.0, false) > max_w && !shown.is_empty() {
                shown.remove(0);
            }
            // the recognised time is tinted with the accent
            let trimmed = self.draft.trim_end();
            let time_at = match &parsed {
                Some(t) if t.due.is_some() => trimmed.rfind(char::is_whitespace).map_or(0, |i| i + 1),
                _ => self.draft.len(),
            };
            let split = time_at.saturating_sub(self.draft.len() - shown.len());
            let (head, time) = shown.split_at(split);
            c.f.text(text_x, fy + 17.5, 12.0, false, WHITE, a, head);
            let head_w = c.f.width(head, 12.0, false);
            c.f.text(text_x + head_w, fy + 17.5, 12.0, false, accent, a, time);
        }
        if submit {
            // return button, same as pressing Enter
            let (bx, by) = (fx + fw - 15.0, fy + 13.0);
            c.f.circle(bx, by, 9.0, accent, a);
            let mut pb = sk::PathBuilder::new();
            pb.move_to(bx + 3.2, by - 3.4);
            pb.line_to(bx + 3.2, by + 1.2);
            pb.line_to(bx - 3.4, by + 1.2);
            pb.move_to(bx - 1.2, by - 1.1);
            pb.line_to(bx - 3.5, by + 1.2);
            pb.line_to(bx - 1.2, by + 3.5);
            c.f.stroke(pb.finish(), 1.5, BLACK, 0.85 * a);
            c.hit(Action::TaskSubmit, bx - 11.0, fy, 22.0, 26.0);
        }
        if self.editing && self.caret {
            let shown_w = c.f.width(&self.draft, 12.0, false).min(max_w);
            let cx = if self.draft.is_empty() { text_x } else { text_x + shown_w + 1.0 };
            c.f.fill(rrect(cx, fy + 6.0, 1.5, 14.0, 0.75), WHITE, a);
        }
        c.hit(Action::TaskInput, fx, fy, fw, 26.0);
    }

    fn draw_settings(&mut self, c: &mut Ctx, x0: f32, x1: f32, _cx: f32) {
        let a = c.a;
        let accent = self.accent();
        let left = x0 + 26.0;
        let align_x = left + 68.0;

        // Row 1 (y = 50..74): ACCENT (swatches) & config.toml link (right)
        c.f.text(left, 65.0, 10.5, true, WHITE, 0.45 * a, "ACCENT");

        let accents: [(&str, [u8; 3]); 6] = [
            ("Auto", [180, 180, 190]),
            ("Blue", [0x3B, 0x82, 0xF6]),
            ("Green", [0x10, 0xB9, 0x81]),
            ("Purple", [0x8B, 0x5C, 0xF6]),
            ("Orange", [0xF9, 0x73, 0x16]),
            ("Red", [0xEF, 0x44, 0x44]),
        ];

        let mut sx = align_x;
        let sy = 52.0;
        let is_auto = self.config.theme.accent_mode.eq_ignore_ascii_case("album_art");
        for (i, (name, color)) in accents.iter().enumerate() {
            let is_sel = if i == 0 {
                is_auto
            } else {
                !is_auto && self.config.theme.custom_accent.to_uppercase().contains(match i {
                    1 => "3B82F6",
                    2 => "10B981",
                    3 => "8B5CF6",
                    4 => "F97316",
                    _ => "EF4444",
                })
            };

            let sw_w = if i == 0 { 42.0 } else { 20.0 };
            let sw_h = 20.0;

            if is_sel {
                c.f.fill(rrect(sx - 2.5, sy - 2.5, sw_w + 5.0, sw_h + 5.0, 7.0), WHITE, 0.9 * a);
            }
            if i == 0 {
                let bg = if is_sel { 0.9 } else { 0.16 };
                let txt_color = if is_sel { BLACK } else { WHITE };
                c.f.fill(rrect(sx, sy, sw_w, sw_h, 4.5), WHITE, bg * a);
                c.f.text_center(sx + sw_w / 2.0, sy + 14.0, 10.5, true, txt_color, a, name);
            } else {
                c.f.fill(rrect(sx, sy, sw_w, sw_h, 4.5), *color, a);
            }

            c.hit(Action::SettingsAccent(i), sx, sy, sw_w, sw_h);
            sx += sw_w + 8.0;
        }

        // Clean links on the right side of Row 1: Reset to default & config.toml
        let cfg_label = "config.toml";
        let cfg_w = c.f.width(cfg_label, 11.0, true) + 20.0;
        let cfg_x = x1 - 26.0 - cfg_w;
        let cfg_hover = c.hover == Some(Action::SettingsOpenConfig);
        c.f.fill(rrect(cfg_x, 50.0, cfg_w, 24.0, 12.0), WHITE, if cfg_hover { 0.12 } else { 0.05 } * a);
        c.f.text(cfg_x + 10.0, 66.0, 11.0, true, WHITE, if cfg_hover { 0.95 } else { 0.50 } * a, cfg_label);
        c.hit(Action::SettingsOpenConfig, cfg_x, 50.0, cfg_w, 24.0);

        let rst_label = "Reset";
        let rst_w = c.f.width(rst_label, 11.0, true) + 20.0;
        let rst_x = cfg_x - 8.0 - rst_w;
        let rst_hover = c.hover == Some(Action::SettingsResetDefaults);
        c.f.fill(rrect(rst_x, 50.0, rst_w, 24.0, 12.0), WHITE, if rst_hover { 0.12 } else { 0.05 } * a);
        c.f.text(rst_x + 10.0, 66.0, 11.0, true, WHITE, if rst_hover { 0.95 } else { 0.50 } * a, rst_label);
        c.hit(Action::SettingsResetDefaults, rst_x, 50.0, rst_w, 24.0);

        // Row 2 (y = 82..106): MODULES (TABS)
        c.f.text(left, 97.0, 10.5, true, WHITE, 0.45 * a, "TABS");
        let tabs = [
            ("Music", self.config.tabs.media),
            ("Focus", self.config.tabs.focus),
            ("Shelf", self.config.tabs.shelf),
            ("Tasks", self.config.tabs.tasks),
        ];
        let mut tx = align_x;
        for (idx, (t_name, on)) in tabs.iter().enumerate() {
            let pw = c.chip(Action::SettingsToggleTab(idx), tx, 82.0, t_name, *on, accent);
            tx += pw + 8.0;
        }

        // Row 3 (y = 114..138): BEHAVIOR & SYSTEM
        c.f.text(left, 129.0, 10.5, true, WHITE, 0.45 * a, "SYSTEM");
        let behaviors = [
            (Action::SettingsToggleSound, "Sound", self.config.behavior.sound_effects),
            (Action::SettingsToggleHover, "Hover Open", self.config.behavior.hover_to_open),
            (Action::SettingsToggleFullscreen, "Fullscreen Hide", self.config.behavior.auto_hide_fullscreen),
            (Action::SettingsToggleAutostart, "Autostart", system::autostart()),
        ];
        let mut bx = align_x;
        for (action, label, on) in behaviors {
            let pw = c.chip(action, bx, 114.0, label, on, accent);
            bx += pw + 8.0;
        }
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

    /// Compact toggle chip with active accent tint and status indicator dot.
    fn chip(&mut self, action: Action, x: f32, y: f32, label: &str, active: bool, accent: [u8; 3]) -> f32 {
        let a = self.a;
        let label_w = self.f.width(label, 11.5, true);
        let dot_r = 2.5;
        let dot_x = x + 10.0 + dot_r;
        let dot_y = y + 12.0;
        let w = label_w + 20.0 + dot_r * 2.0 + 4.0;
        let h = 24.0;
        let r = 12.0;

        let hovered = self.hover == Some(action);
        let bg_color = if active { accent } else { WHITE };
        let bg_alpha = if active {
            if hovered { 0.22 } else { 0.15 }
        } else if hovered {
            0.10
        } else {
            0.05
        };
        self.f.fill(rrect(x, y, w, h, r), bg_color, bg_alpha * a);

        if active {
            self.f.stroke(rrect(x, y, w, h, r), 1.0, accent, 0.35 * a);
            self.f.circle(dot_x, dot_y, dot_r, accent, a);
        } else {
            self.f.circle(dot_x, dot_y, dot_r, WHITE, 0.22 * a);
        }

        let text_alpha = if active { 0.95 } else if hovered { 0.75 } else { 0.45 };
        self.f.text(dot_x + dot_r + 6.0, y + 16.0, 11.5, true, WHITE, text_alpha * a, label);

        self.hit(action, x, y, w, h);
        w
    }
}

fn mmss(sec: f64) -> String {
    let s = sec.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmss_formatting() {
        assert_eq!(mmss(0.0), "0:00");
        assert_eq!(mmss(65.0), "1:05");
        assert_eq!(mmss(1500.0), "25:00");
    }

    #[test]
    fn test_notch_and_satellite_paths() {
        let notch = notch_path(100.0, 300.0, 32.0, 12.0, 9.0);
        assert!(!notch.is_empty());
        let sat = rrect(310.0, 0.0, 32.0, 32.0, 16.0);
        assert!(sat.is_some());
    }
}
