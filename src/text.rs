//! Custom text rendering from Windows Segoe UI font (via fontdue).

use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use tiny_skia::{Mask, Pixmap};

pub struct Fonts {
    regular: Font,
    bold: Font,
    cache: HashMap<(char, u32, bool), (Metrics, Vec<u8>)>,
}

impl Fonts {
    pub fn load() -> Fonts {
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\Fonts\\";
        let load = |names: &[&str]| {
            let bytes = names
                .iter()
                .find_map(|n| std::fs::read(format!("{dir}{n}")).ok())
                .expect("Windows font not found");
            Font::from_bytes(bytes, FontSettings::default()).expect("corrupt font")
        };
        Fonts {
            regular: load(&["segoeui.ttf", "arial.ttf"]),
            bold: load(&["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf"]),
            cache: HashMap::new(),
        }
    }

    fn glyph(&mut self, c: char, px: f32, bold: bool) -> &(Metrics, Vec<u8>) {
        let font = if bold { &self.bold } else { &self.regular };
        self.cache
            .entry((c, (px * 4.0).round() as u32, bold))
            .or_insert_with(|| font.rasterize(c, px))
    }

    pub fn measure(&mut self, text: &str, px: f32, bold: bool) -> f32 {
        text.chars().map(|c| self.glyph(c, px, bold).0.advance_width).sum()
    }

    /// Wraps text by word to fit within `max` pixels, up to `rows` lines.
    pub fn wrap(&mut self, text: &str, px: f32, bold: bool, max: f32, rows: usize) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in text.split_whitespace() {
            let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if self.measure(&next, px, bold) <= max || cur.is_empty() {
                cur = next;
            } else {
                lines.push(std::mem::replace(&mut cur, word.to_string()));
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        if lines.len() > rows {
            let rest = lines.split_off(rows - 1).join(" ");
            lines.push(rest);
        }
        lines
    }

    /// Draws `text` with baseline at (x, baseline), all in pixels.
    /// Pixels outside `mask` (notch shape) are clipped.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        pm: &mut Pixmap,
        mask: &Mask,
        x: f32,
        baseline: f32,
        px: f32,
        bold: bool,
        rgb: [u8; 3],
        alpha: f32,
        text: &str,
    ) {
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        let clip = mask.data();
        let data = pm.data_mut();
        let mut pen = x;
        for c in text.chars() {
            let g = self.glyph(c, px, bold);
            let m = g.0;
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let cov = g.1[row * m.width + col];
                    let (x, y) = (gx + col as i32, gy + row as i32);
                    if cov == 0 || x < 0 || y < 0 || x >= w || y >= h {
                        continue;
                    }
                    let i = (y * w + x) as usize;
                    let a = cov as f32 / 255.0 * alpha * clip[i] as f32 / 255.0;
                    if a <= 0.0 {
                        continue;
                    }
                    // premultiplied RGBA: result = color * a + dst * (1 - a)
                    let d = &mut data[i * 4..i * 4 + 4];
                    for k in 0..3 {
                        d[k] = (rgb[k] as f32 * a + d[k] as f32 * (1.0 - a)).round() as u8;
                    }
                    d[3] = (255.0 * a + d[3] as f32 * (1.0 - a)).round() as u8;
                }
            }
            pen += m.advance_width;
        }
    }
}
