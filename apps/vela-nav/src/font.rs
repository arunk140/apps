//! Antialiased text in the app's own fonts (`fonts.rs`, Noto Sans), drawn as runs of equal coverage blended
//! over a background colour the caller names, as every `cmf-wp2-ui` shape is.

use cmf_wp2_ui::color::blend565;
use cmf_wp2_ui::{ONE, Surface};

pub struct Glyph {
    pub c: char,
    pub adv: u8,
    /// Ink box from the pen position on the baseline.
    pub x: i8,
    pub y: i8,
    pub w: u8,
    pub h: u8,
    /// Byte offset of the 4-bit coverage, two pixels a byte, rows packed.
    pub off: u16,
}

pub struct Font {
    pub ascent: i32,
    pub descent: i32,
    /// Height of a capital above the baseline.
    pub cap: i32,
    pub glyphs: &'static [Glyph],
    pub bits: &'static [u8],
}

impl Font {
    fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.c == c)
    }

    /// Every character of `t` is in the font.
    pub fn covers(&self, t: &str) -> bool {
        t.chars().all(|c| self.glyph(c).is_some())
    }

    pub fn width(&self, t: &str) -> i32 {
        t.chars().map(|c| self.glyph(c).map_or(0, |g| g.adv as i32)).sum()
    }

    /// Draws `t` with its baseline at `y`, from `x`; returns the pen position after it. Characters the font
    /// lacks are skipped.
    pub fn draw(&self, s: &mut impl Surface, x: i32, y: i32, t: &str, fg: u16, bg: u16) -> i32 {
        let mut shade = [0u16; 16];
        for (a, c) in shade.iter_mut().enumerate() {
            *c = blend565(bg, fg, a as i32 * ONE / 15);
        }
        let mut pen = x;
        for ch in t.chars() {
            let Some(g) = self.glyph(ch) else { continue };
            let (gx, gy, w) = (pen + g.x as i32, y + g.y as i32, g.w as usize);
            let at = |i: usize| {
                let b = self.bits[g.off as usize + i / 2];
                if i.is_multiple_of(2) { b >> 4 } else { b & 15 }
            };
            for row in 0..g.h as usize {
                let base = row * w;
                let mut col = 0;
                while col < w {
                    let a = at(base + col);
                    let start = col;
                    while col < w && at(base + col) == a {
                        col += 1;
                    }
                    if a != 0 {
                        s.rect(gx + start as i32, gy + row as i32, (col - start) as i32, 1, shade[a as usize]);
                    }
                }
            }
            pen += g.adv as i32;
        }
        pen
    }

    /// A black outline `r` px (≤ 4) around the ink of `t`, for text drawn over the map: draw it, then the text over
    /// it with a black background. Each glyph's ink is a bit mask a row, widened with shifts, so it is cheap.
    pub fn halo(&self, s: &mut impl Surface, x: i32, y: i32, t: &str, r: i32) {
        const ROWS: usize = 80;
        let r = r.clamp(0, 4);
        let mut pen = x;
        for ch in t.chars() {
            let Some(g) = self.glyph(ch) else { continue };
            let (w, h) = (g.w as i32, g.h as i32);
            let (gx, gy) = (pen + g.x as i32 - r, y + g.y as i32 - r);
            pen += g.adv as i32;
            if w == 0 || w + 2 * r > 64 || (h + 2 * r) as usize > ROWS {
                continue;
            }
            let mut ink = [0u64; ROWS];
            for row in 0..h {
                let mut bits = 0u64;
                for col in 0..w {
                    let i = (row * w + col) as usize;
                    let b = self.bits[g.off as usize + i / 2];
                    let a = if i.is_multiple_of(2) { b >> 4 } else { b & 15 };
                    if a >= 4 {
                        bits |= 1 << (col + r);
                    }
                }
                ink[(row + r) as usize] = bits;
            }
            let n = (h + 2 * r) as usize;
            for oy in 0..n {
                let mut out = 0u64;
                for dy in -r..=r {
                    let sy = oy as i32 + dy;
                    if sy < 0 || sy >= n as i32 || ink[sy as usize] == 0 {
                        continue;
                    }
                    let m = ink[sy as usize];
                    // a round pen: the reach across shrinks with the distance down
                    let reach = [[0, 0, 0, 0, 0], [1, 0, 0, 0, 0], [2, 2, 1, 0, 0], [3, 3, 2, 1, 0], [4, 4, 3, 3, 1]]
                        [r as usize][dy.unsigned_abs() as usize];
                    let mut d = m;
                    for i in 1..=reach {
                        d |= m << i | m >> i;
                    }
                    out |= d;
                }
                while out != 0 {
                    let start = out.trailing_zeros();
                    let len = (!(out >> start)).trailing_zeros().min(64 - start);
                    s.rect(gx + start as i32, gy + oy as i32, len as i32, 1, 0);
                    let mask = if len >= 64 { u64::MAX } else { ((1u64 << len) - 1) << start };
                    out &= !mask;
                }
            }
        }
    }

    /// The longest start of `t` that fits `max` px, and whether it was cut (then an ellipsis follows it).
    pub fn fit<'a>(&self, t: &'a str, max: i32) -> (&'a str, bool) {
        if self.width(t) <= max {
            return (t, false);
        }
        let room = max - self.width("…");
        let mut w = 0;
        for (i, c) in t.char_indices() {
            w += self.glyph(c).map_or(0, |g| g.adv as i32);
            if w > room {
                return (t[..i].trim_end(), true);
            }
        }
        (t, false)
    }

    /// `t` centred on `cx`, cut with an ellipsis to `max` px.
    #[allow(clippy::too_many_arguments)]
    pub fn centre(&self, s: &mut impl Surface, cx: i32, y: i32, t: &str, max: i32, fg: u16, bg: u16) {
        let (t, cut) = self.fit(t, max);
        let w = self.width(t) + if cut { self.width("…") } else { 0 };
        let end = self.draw(s, cx - w / 2, y, t, fg, bg);
        if cut {
            self.draw(s, end, y, "…", fg, bg);
        }
    }
}

/// A filled polygon (any shape, even-odd), antialiased: exact horizontal coverage on 4 sub-rows a pixel.
/// `paint(x)` picks the colour per column, so a shape can have two flat faces.
pub fn poly(s: &mut impl Surface, pts: &[(i32, i32)], bg: u16, paint: impl Fn(i32) -> u16) {
    const SUB: i32 = 4; // sub-rows a pixel
    const FX: i32 = 16; // sub-pixel units a pixel, across
    let y0 = pts.iter().map(|p| p.1).min().unwrap_or(0);
    let y1 = pts.iter().map(|p| p.1).max().unwrap_or(0);
    let x0 = pts.iter().map(|p| p.0).min().unwrap_or(0);
    let x1 = pts.iter().map(|p| p.0).max().unwrap_or(0);
    let wide = (x1 - x0 + 2) as usize;
    if wide > 128 || pts.len() > 8 {
        return; // icons and the car only
    }
    for y in y0..y1 {
        let mut cov = [0i32; 128];
        for k in 0..SUB {
            // sample at the sub-row's middle, in 1/(2*SUB) px
            let sy2 = y * 2 * SUB + 2 * k + 1;
            let mut xs = [0i32; 8];
            let mut n = 0;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                let (ay, by) = (a.1 * 2 * SUB, b.1 * 2 * SUB);
                if (ay <= sy2) != (by <= sy2) {
                    xs[n] = (a.0 * FX) + (b.0 - a.0) * FX * (sy2 - ay) / (by - ay);
                    n += 1;
                }
            }
            xs[..n].sort_unstable();
            for span in xs[..n].chunks(2) {
                let [l, r] = span else { continue };
                for px in (l.div_euclid(FX))..=(r.div_euclid(FX)) {
                    let lo = (px * FX).max(*l);
                    let hi = ((px + 1) * FX).min(*r);
                    let i = px - x0 + 1;
                    if hi > lo && i >= 0 && (i as usize) < wide {
                        cov[i as usize] += hi - lo;
                    }
                }
            }
        }
        // runs of equal colour
        let full = FX * SUB;
        let mut i = 0;
        while i < wide {
            let c = cov[i];
            let x = x0 - 1 + i as i32;
            let col = if c == 0 { None } else { Some(blend565(bg, paint(x), c * ONE / full)) };
            let start = i;
            while i < wide && cov[i] == c && paint(x0 - 1 + i as i32) == paint(x) {
                i += 1;
            }
            if let Some(col) = col {
                s.rect(x, y, (i - start) as i32, 1, col);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{L, S, XL};
    use cmf_wp2_ui::{Fb, SIZE};
    use std::vec;

    #[test]
    fn fonts_have_what_the_screens_need() {
        assert!(XL.covers("0123456789."));
        assert!(L.covers("12:05 pm km mi ft Vela Nav Arrived Off route"));
        assert!(S.covers("Bannerghatta Main Road · 4.7 km · 12 min Rerouting…"));
        assert!(!S.covers("ಹೊಸೂರು ರಸ್ತೆ"));
    }

    #[test]
    fn text_is_antialiased_and_fits() {
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        let end = XL.draw(&mut fb, 100, 200, "300", 0xFFFF, 0);
        assert_eq!(end, 100 + XL.width("300"));
        let mut shades = std::collections::BTreeSet::new();
        for y in 100..220 {
            for x in 90..end + 10 {
                shades.insert(fb.at(x, y));
            }
        }
        assert!(shades.len() > 8, "{}", shades.len());
        let (t, cut) = S.fit("Bannerghatta Main Road", 120);
        assert!(cut && S.width(t) + S.width("…") <= 120, "{t}");
        assert_eq!(S.fit("Road", 120), ("Road", false));
    }

    #[test]
    fn a_polygon_has_soft_edges_and_a_solid_middle() {
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        poly(&mut fb, &[(100, 100), (140, 160), (60, 160)], 0, |_| 0xFFFF);
        assert_eq!(fb.at(100, 150), 0xFFFF);
        assert_eq!(fb.at(100, 90), 0);
        let edge = fb.at(79, 130); // the left edge crosses it at x 79.7
        assert!(edge != 0 && edge != 0xFFFF, "{edge:x}");
    }
}
