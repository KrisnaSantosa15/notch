//! Drawing tools: single-frame canvas, primitive shapes, and icons. All coordinates in dp.

use crate::text::Fonts;
use tiny_skia as sk;

pub const BLACK: [u8; 3] = [0, 0, 0];
pub const WHITE: [u8; 3] = [245, 245, 247];
pub const GRAY: [u8; 3] = [142, 142, 147];
pub const ORANGE: [u8; 3] = [255, 159, 10];
pub const GREEN: [u8; 3] = [48, 209, 88];
pub const RED: [u8; 3] = [255, 69, 58];

pub enum Icon {
    Play,
    Pause,
    Prev,
    Next,
    Reset,
    /// 0 = muted, 1..3 = number of sound waves
    Speaker(u8),
    Caps(bool),
    /// fill (0..1), is_charging
    Battery(f32, bool),
    Note,
    Lyrics,
    Tray,
    Check,
    Close,
    Plus,
}

/// Secondary text on glass: translucent white instead of grey, ensuring readability
/// on both light and dark backgrounds.
pub const SOFT: f32 = 0.58;

/// Single-frame canvas. Contents clipped to notch shape via `mask`.
pub struct Frame<'a> {
    pub pm: sk::Pixmap,
    pub mask: sk::Mask,
    pub s: f32,
    pub fonts: &'a mut Fonts,
}

impl Frame<'_> {
    fn ts(&self) -> sk::Transform {
        sk::Transform::from_scale(self.s, self.s)
    }

    pub fn paint(&mut self, path: &sk::Path, paint: &sk::Paint) {
        let ts = self.ts();
        self.pm.fill_path(path, paint, sk::FillRule::Winding, ts, Some(&self.mask));
    }

    pub fn fill(&mut self, path: Option<sk::Path>, rgb: [u8; 3], a: f32) {
        if let Some(p) = path {
            self.paint(&p, &solid(rgb, a));
        }
    }

    pub fn stroke(&mut self, path: Option<sk::Path>, width: f32, rgb: [u8; 3], a: f32) {
        let Some(p) = path else { return };
        let stroke = sk::Stroke {
            width,
            line_cap: sk::LineCap::Round,
            line_join: sk::LineJoin::Round,
            ..Default::default()
        };
        let ts = self.ts();
        self.pm.stroke_path(&p, &solid(rgb, a), &stroke, ts, Some(&self.mask));
    }

    pub fn dashed(&mut self, path: Option<sk::Path>, width: f32, rgb: [u8; 3], a: f32) {
        let Some(p) = path else { return };
        let stroke = sk::Stroke {
            width,
            line_cap: sk::LineCap::Round,
            dash: sk::StrokeDash::new(vec![5.0, 5.0], 0.0),
            ..Default::default()
        };
        let ts = self.ts();
        self.pm.stroke_path(&p, &solid(rgb, a), &stroke, ts, Some(&self.mask));
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, rgb: [u8; 3], a: f32) {
        self.fill(sk::PathBuilder::from_circle(cx, cy, r), rgb, a);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, x: f32, baseline: f32, size: f32, bold: bool, rgb: [u8; 3], a: f32, t: &str) {
        let s = self.s;
        self.fonts.draw(&mut self.pm, &self.mask, x * s, baseline * s, size * s, bold, rgb, a, t);
    }

    /// Right-aligned text at `right`; returns its left edge.
    #[allow(clippy::too_many_arguments)]
    pub fn text_right(&mut self, right: f32, baseline: f32, size: f32, bold: bool, rgb: [u8; 3], a: f32, t: &str) -> f32 {
        let x = right - self.width(t, size, bold);
        self.text(x, baseline, size, bold, rgb, a, t);
        x
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text_center(&mut self, cx: f32, baseline: f32, size: f32, bold: bool, rgb: [u8; 3], a: f32, t: &str) {
        let x = cx - self.width(t, size, bold) / 2.0;
        self.text(x, baseline, size, bold, rgb, a, t);
    }

    pub fn width(&mut self, t: &str, size: f32, bold: bool) -> f32 {
        self.fonts.measure(t, size * self.s, bold) / self.s
    }

    /// Truncates text with "…" if wider than `max`.
    pub fn fit(&mut self, t: &str, size: f32, bold: bool, max: f32) -> String {
        if self.width(t, size, bold) <= max {
            return t.to_string();
        }
        let mut chars: Vec<char> = t.chars().collect();
        while chars.pop().is_some() {
            let s = chars.iter().collect::<String>().trim_end().to_string() + "…";
            if self.width(&s, size, bold) <= max {
                return s;
            }
        }
        "…".into()
    }

    /// Progress ring: full dim track, then colored arc starting from 12 o'clock.
    #[allow(clippy::too_many_arguments)]
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, width: f32, frac: f32, rgb: [u8; 3], a: f32) {
        self.stroke(arc(cx, cy, r, -90.0, 360.0), width, WHITE, 0.12 * a);
        if frac > 0.002 {
            self.stroke(arc(cx, cy, r, -90.0, 360.0 * frac.min(1.0)), width, rgb, a);
        }
    }

    /// Pair of glowing eyes, mascot when idle.
    #[allow(clippy::too_many_arguments)]
    pub fn eyes(&mut self, cx: f32, cy: f32, r: f32, gap: f32, blink: bool, a: f32) {
        self.animated_eyes(cx, cy, r, gap, blink, 0.0, a);
    }

    /// Pair of glowing eyes with horizontal look direction offset.
    #[allow(clippy::too_many_arguments)]
    pub fn animated_eyes(&mut self, cx: f32, cy: f32, r: f32, gap: f32, blink: bool, look_x: f32, a: f32) {
        for ex in [cx - gap, cx + gap] {
            self.circle(ex, cy, r * 2.2, [24, 28, 40], 0.85 * a);
            self.circle(ex, cy, r * 2.2, [90, 150, 255], 0.25 * a);
            let ry = if blink { (r * 0.15).max(0.6) } else { r };
            let oval = sk::Rect::from_xywh(ex - r + look_x, cy - ry, r * 2.0, ry * 2.0).and_then(sk::PathBuilder::from_oval);
            self.fill(oval, [255, 255, 255], a);
        }
    }

    /// Draws a square (e.g. album artwork) with rounded corners.
    #[allow(clippy::too_many_arguments)]
    pub fn image(&mut self, img: &sk::Pixmap, x: f32, y: f32, size: f32, r: f32, a: f32) {
        let k = size / img.width() as f32;
        let shader = sk::Pattern::new(
            img.as_ref(),
            sk::SpreadMode::Pad,
            sk::FilterQuality::Bicubic,
            a,
            sk::Transform::from_row(k, 0.0, 0.0, k, x, y),
        );
        let paint = sk::Paint { shader, anti_alias: true, ..Default::default() };
        if let Some(p) = rrect(x, y, size, size, r) {
            self.paint(&p, &paint);
        }
    }

    /// Draws an arbitrary aspect ratio image, fitted into a `size` box centered at (cx, cy).
    #[allow(clippy::too_many_arguments)]
    pub fn image_fit(&mut self, img: &sk::Pixmap, cx: f32, cy: f32, size: f32, r: f32, a: f32) {
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let k = size / iw.max(ih);
        let (w, h) = (iw * k, ih * k);
        let (x, y) = (cx - w / 2.0, cy - h / 2.0);
        let shader = sk::Pattern::new(
            img.as_ref(),
            sk::SpreadMode::Pad,
            sk::FilterQuality::Bicubic,
            a,
            sk::Transform::from_row(k, 0.0, 0.0, k, x, y),
        );
        let paint = sk::Paint { shader, anti_alias: true, ..Default::default() };
        if let Some(p) = rrect(x, y, w, h, r) {
            self.paint(&p, &paint);
        }
    }

    /// Artwork placeholder: dark grey box with musical note.
    pub fn art_placeholder(&mut self, x: f32, y: f32, size: f32, r: f32, a: f32) {
        self.fill(rrect(x, y, size, size, r), [44, 44, 46], a);
        self.icon(Icon::Note, x + size / 2.0, y + size / 2.0, size * 0.42, GRAY, a);
    }

    pub fn icon(&mut self, icon: Icon, cx: f32, cy: f32, z: f32, rgb: [u8; 3], a: f32) {
        match icon {
            Icon::Play => {
                let p = poly(&[(cx - z * 0.34, cy - z * 0.48), (cx + z * 0.5, cy), (cx - z * 0.34, cy + z * 0.48)]);
                self.stroke(p.clone(), z * 0.12, rgb, a); // slightly rounded corners
                self.fill(p, rgb, a);
            }
            Icon::Pause => {
                self.fill(rrect(cx - z * 0.36, cy - z * 0.48, z * 0.26, z * 0.96, z * 0.06), rgb, a);
                self.fill(rrect(cx + z * 0.1, cy - z * 0.48, z * 0.26, z * 0.96, z * 0.06), rgb, a);
            }
            Icon::Next | Icon::Prev => {
                let d = if matches!(icon, Icon::Next) { 1.0 } else { -1.0 };
                let p = poly(&[
                    (cx - d * z * 0.48, cy - z * 0.42),
                    (cx + d * z * 0.22, cy),
                    (cx - d * z * 0.48, cy + z * 0.42),
                ]);
                self.stroke(p.clone(), z * 0.1, rgb, a);
                self.fill(p, rgb, a);
                let bx = if d > 0.0 { cx + z * 0.28 } else { cx - z * 0.42 };
                self.fill(rrect(bx, cy - z * 0.42, z * 0.14, z * 0.84, z * 0.05), rgb, a);
            }
            Icon::Reset => {
                let r = z * 0.4;
                self.stroke(arc(cx, cy, r, -60.0, 290.0), z * 0.12, rgb, a);
                // arrow tip at the start of the arc, pointing counter-clockwise
                let t = (-60f32).to_radians();
                let (px, py) = (cx + r * t.cos(), cy + r * t.sin());
                let (dx, dy) = (t.sin(), -t.cos());
                let (nx, ny) = (-dy, dx);
                self.fill(
                    poly(&[
                        (px + dx * z * 0.2, py + dy * z * 0.2),
                        (px - dx * z * 0.06 + nx * z * 0.18, py - dy * z * 0.06 + ny * z * 0.18),
                        (px - dx * z * 0.06 - nx * z * 0.18, py - dy * z * 0.06 - ny * z * 0.18),
                    ]),
                    rgb,
                    a,
                );
            }
            Icon::Speaker(level) => {
                let x = cx - z * 0.22;
                let body = poly(&[
                    (x - z * 0.3, cy - z * 0.16),
                    (x - z * 0.1, cy - z * 0.16),
                    (x + z * 0.16, cy - z * 0.4),
                    (x + z * 0.16, cy + z * 0.4),
                    (x - z * 0.1, cy + z * 0.16),
                    (x - z * 0.3, cy + z * 0.16),
                ]);
                self.stroke(body.clone(), z * 0.06, rgb, a);
                self.fill(body, rgb, a);
                if level == 0 {
                    let (ax, s) = (x + z * 0.5, z * 0.15);
                    self.stroke(line(ax - s, cy - s, ax + s, cy + s), z * 0.09, rgb, a);
                    self.stroke(line(ax - s, cy + s, ax + s, cy - s), z * 0.09, rgb, a);
                }
                for i in 1..=level.min(3) {
                    let r = z * (0.12 + 0.16 * i as f32);
                    self.stroke(arc(x + z * 0.16, cy, r, -48.0, 96.0), z * 0.08, rgb, a);
                }
            }
            Icon::Caps(on) => {
                let arrow = poly(&[
                    (cx, cy - z * 0.48),
                    (cx + z * 0.44, cy - z * 0.04),
                    (cx + z * 0.2, cy - z * 0.04),
                    (cx + z * 0.2, cy + z * 0.2),
                    (cx - z * 0.2, cy + z * 0.2),
                    (cx - z * 0.2, cy - z * 0.04),
                    (cx - z * 0.44, cy - z * 0.04),
                ]);
                self.stroke(arrow.clone(), z * 0.08, rgb, a);
                if on {
                    self.fill(arrow, rgb, a);
                }
                self.fill(rrect(cx - z * 0.2, cy + z * 0.34, z * 0.4, z * 0.1, z * 0.03), rgb, a);
            }
            Icon::Battery(frac, charging) => {
                let (w, h) = (z * 1.05, z * 0.52);
                let (x, y) = (cx - w / 2.0 - z * 0.04, cy - h / 2.0);
                self.stroke(rrect(x, y, w, h, h * 0.3), z * 0.06, GRAY, a);
                self.fill(rrect(x + w + z * 0.04, cy - h * 0.2, z * 0.06, h * 0.4, z * 0.03), GRAY, a);
                let color = if charging {
                    GREEN
                } else if frac < 0.2 {
                    RED
                } else {
                    rgb
                };
                let inner = (w - z * 0.16) * frac.clamp(0.06, 1.0);
                self.fill(rrect(x + z * 0.08, y + z * 0.08, inner, h - z * 0.16, h * 0.18), color, a);
            }
            Icon::Note => {
                self.circle(cx - z * 0.14, cy + z * 0.28, z * 0.18, rgb, a);
                self.fill(rrect(cx - z * 0.02, cy - z * 0.46, z * 0.1, z * 0.76, z * 0.03), rgb, a);
                self.fill(
                    poly(&[(cx + z * 0.06, cy - z * 0.46), (cx + z * 0.38, cy - z * 0.3), (cx + z * 0.38, cy - z * 0.12), (cx + z * 0.06, cy - z * 0.26)]),
                    rgb,
                    a,
                );
            }
            Icon::Lyrics => {
                // quote bubble: three lines of text with varying lengths
                for (i, len) in [1.0, 0.72, 0.86].iter().enumerate() {
                    let y = cy - z * 0.32 + i as f32 * z * 0.32;
                    self.fill(rrect(cx - z * 0.45, y - z * 0.06, z * 0.9 * len, z * 0.12, z * 0.06), rgb, a);
                }
            }
            Icon::Tray => {
                // downward arrow into tray
                self.stroke(line(cx, cy - z * 0.5, cx, cy + z * 0.05), z * 0.09, rgb, a);
                self.stroke(
                    poly_open(&[(cx - z * 0.2, cy - z * 0.14), (cx, cy + z * 0.06), (cx + z * 0.2, cy - z * 0.14)]),
                    z * 0.09,
                    rgb,
                    a,
                );
                self.stroke(
                    poly_open(&[
                        (cx - z * 0.5, cy - z * 0.02),
                        (cx - z * 0.5, cy + z * 0.42),
                        (cx + z * 0.5, cy + z * 0.42),
                        (cx + z * 0.5, cy - z * 0.02),
                    ]),
                    z * 0.09,
                    rgb,
                    a,
                );
            }
            Icon::Check => self.stroke(
                poly_open(&[(cx - z * 0.36, cy + z * 0.02), (cx - z * 0.1, cy + z * 0.28), (cx + z * 0.4, cy - z * 0.26)]),
                z * 0.16,
                rgb,
                a,
            ),
            Icon::Close => {
                let s = z * 0.32;
                self.stroke(line(cx - s, cy - s, cx + s, cy + s), z * 0.13, rgb, a);
                self.stroke(line(cx - s, cy + s, cx + s, cy - s), z * 0.13, rgb, a);
            }
            Icon::Plus => {
                let s = z * 0.38;
                self.stroke(line(cx - s, cy, cx + s, cy), z * 0.12, rgb, a);
                self.stroke(line(cx, cy - s, cx, cy + s), z * 0.12, rgb, a);
            }
        }
    }

    /// Bell swung by `angle` degrees around its top, so it can ring.
    pub fn bell(&mut self, cx: f32, cy: f32, z: f32, angle: f32, rgb: [u8; 3], a: f32) {
        let swing = sk::Transform::from_rotate_at(angle, cx, cy - z * 0.45);
        let mut pb = sk::PathBuilder::new();
        pb.move_to(cx - z * 0.42, cy + z * 0.24);
        pb.quad_to(cx - z * 0.3, cy + z * 0.1, cx - z * 0.3, cy - z * 0.08);
        pb.cubic_to(cx - z * 0.3, cy - z * 0.5, cx + z * 0.3, cy - z * 0.5, cx + z * 0.3, cy - z * 0.08);
        pb.quad_to(cx + z * 0.3, cy + z * 0.1, cx + z * 0.42, cy + z * 0.24);
        pb.close();
        self.fill(pb.finish().and_then(|p| p.transform(swing)), rgb, a);
        let clapper = sk::PathBuilder::from_circle(cx, cy + z * 0.36, z * 0.1).and_then(|p| p.transform(swing));
        self.fill(clapper, rgb, a);
    }
}

fn poly_open(points: &[(f32, f32)]) -> Option<sk::Path> {
    let mut pb = sk::PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for p in &points[1..] {
        pb.line_to(p.0, p.1);
    }
    pb.finish()
}

pub fn solid(rgb: [u8; 3], a: f32) -> sk::Paint<'static> {
    let mut p = sk::Paint::default();
    p.set_color_rgba8(rgb[0], rgb[1], rgb[2], (a.clamp(0.0, 1.0) * 255.0) as u8);
    p.anti_alias = true;
    p
}

const KAPPA: f32 = 0.5523; // Bezier control point offset for quarter circles

/// Notch shape: flat top against screen edge, two concave "ears" at top corners,
/// and rounded bottom corners.
pub fn notch_path(x0: f32, x1: f32, h: f32, r: f32, ear: f32) -> sk::Path {
    let k = r * (1.0 - KAPPA);
    let mut pb = sk::PathBuilder::new();
    pb.move_to(x0 - ear, 0.0);
    pb.quad_to(x0, 0.0, x0, ear);
    pb.line_to(x0, h - r);
    pb.cubic_to(x0, h - k, x0 + k, h, x0 + r, h);
    pb.line_to(x1 - r, h);
    pb.cubic_to(x1 - k, h, x1, h - k, x1, h - r);
    pb.line_to(x1, ear);
    pb.quad_to(x1, 0.0, x1 + ear, 0.0);
    pb.close();
    pb.finish().unwrap()
}

pub fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<sk::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = r * (1.0 - KAPPA);
    let mut pb = sk::PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - k, y, x + w, y + k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - k, x + w - k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + k, y + h, x, y + h - k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + k, x + k, y, x + r, y);
    pb.close();
    pb.finish()
}

pub fn arc(cx: f32, cy: f32, r: f32, start_deg: f32, sweep_deg: f32) -> Option<sk::Path> {
    let n = ((sweep_deg.abs() / 4.0).ceil() as usize).max(2);
    let mut pb = sk::PathBuilder::new();
    for i in 0..=n {
        let t = (start_deg + sweep_deg * i as f32 / n as f32).to_radians();
        let (x, y) = (cx + r * t.cos(), cy + r * t.sin());
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.finish()
}

fn poly(points: &[(f32, f32)]) -> Option<sk::Path> {
    let mut pb = sk::PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for p in &points[1..] {
        pb.line_to(p.0, p.1);
    }
    pb.close();
    pb.finish()
}

fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> Option<sk::Path> {
    let mut pb = sk::PathBuilder::new();
    pb.move_to(x0, y0);
    pb.line_to(x1, y1);
    pb.finish()
}
