// Drawing, through cmf-wp2-ui's Surface: the watch draws with `&Api`, the tests with `Fb`. One buffer and no
// double buffering, so each frame repaints only what moved and never paints a pixel twice in different colours
// where it can help it (section 2 of the app guide). The sprites (src/sprites.rs) are lists of rects: the Surface
// has no blit.

use crate::game::{C, GATE_W, GROUND_Y, Game, Gate, HOP_X, SIZE, State};
use crate::sprites::{self, PIPE_BODY, PIPE_CAP, PIPE_CAP_H, Sprite};
use cmf_wp2_ui::Rgb;
use cmf_wp2_ui::Surface;
use cmf_wp2_ui::draw::rrect;
use cmf_wp2_ui::text::{big, big_width, centre, int};

pub const PANEL: Rgb = Rgb(22, 22, 30);
const GREY: u16 = 0x8C71;
const WHITE: u16 = 0xFFFF;
/// Text on the scenery.
const INK: Rgb = Rgb(84, 56, 71);

/// The score's digits sit this far apart, on top of the screen.
pub const SCORE_Y: i32 = 40;
const DIGIT_H: i32 = 36;
const DIGIT_GAP: i32 = 2;
/// The town strip scrolls one px per this many px the pipes travel; the ground goes with the pipes.
const BAND_SLOW: i32 = 40;
/// The bird sprite's size, centred on the critter.
const BIRD_W: i32 = 34;
const BIRD_H: i32 = 24;

const DIGITS: [&Sprite; 10] = [
    &sprites::DIGIT0,
    &sprites::DIGIT1,
    &sprites::DIGIT2,
    &sprites::DIGIT3,
    &sprites::DIGIT4,
    &sprites::DIGIT5,
    &sprites::DIGIT6,
    &sprites::DIGIT7,
    &sprites::DIGIT8,
    &sprites::DIGIT9,
];

/// What was on screen after the last frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Seen {
    xs: [i32; crate::game::GATES],
    band: i32,
    ground: i32,
    y: i32,
    wing: usize,
    score: u32,
    state: State,
}

pub fn seen(g: &Game) -> Seen {
    Seen { xs: g.gates.map(|gt| gt.x), band: band_x(g), ground: ground_x(g), y: g.hop_y(), wing: g.wing(), score: g.score, state: g.state }
}

/// A surface that drops what falls outside a box, so a part of the picture can be redrawn alone.
struct Clip<'a, S: Surface> {
    s: &'a mut S,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl<S: Surface> Surface for Clip<'_, S> {
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        let (ax, ay) = (x.max(self.x0), y.max(self.y0));
        let (bx, by) = ((x + w).min(self.x1), (y + h).min(self.y1));
        if bx > ax && by > ay {
            self.s.rect(ax, ay, bx - ax, by - ay, c);
        }
    }

    fn text(&mut self, x: i32, y: i32, t: &str, c: u16) {
        self.s.text(x, y, t, c);
    }
}

fn blit(s: &mut impl Surface, sp: &Sprite, x: i32, y: i32) {
    for &(rx, ry, w, h, c) in sp.rects {
        s.rect(x + rx as i32, y + ry as i32, w as i32, h as i32, c);
    }
}

/// Upside down.
fn blit_flipped(s: &mut impl Surface, sp: &Sprite, x: i32, y: i32) {
    for &(rx, ry, w, h, c) in sp.rects {
        s.rect(x + rx as i32, y + sp.h - (ry as i32 + h as i32), w as i32, h as i32, c);
    }
}

/// The shaft of a pipe: one row of the sprite, stretched to `h`.
fn shaft(s: &mut impl Surface, x: i32, y: i32, h: i32) {
    if h > 0 {
        for &(rx, _, w, _, c) in PIPE_BODY.rects {
            s.rect(x + rx as i32, y, w as i32, h, c);
        }
    }
}

/// Both pipes of a gate, from the top and the bottom of the screen to the gap, covering all `GATE_W` columns.
fn pipes(s: &mut impl Surface, gt: &Gate) {
    blit_flipped(s, &PIPE_CAP, gt.x, gt.top - PIPE_CAP_H);
    shaft(s, gt.x, 0, gt.top - PIPE_CAP_H);
    blit(s, &PIPE_CAP, gt.x, gt.bottom);
    shaft(s, gt.x, gt.bottom + PIPE_CAP_H, GROUND_Y - gt.bottom - PIPE_CAP_H);
}

/// Where the town strip and the ground are scrolled to: how far their tiles are shifted to the left.
fn band_x(g: &Game) -> i32 {
    g.scroll / BAND_SLOW % sprites::BAND.w
}

fn ground_x(g: &Game) -> i32 {
    g.scroll % sprites::GROUND.w
}

/// The scenery in a box: sky, the town and grass, the ground. Every pixel of the box once.
fn scenery(s: &mut impl Surface, g: &Game, x0: i32, y0: i32, w: i32, h: i32) {
    let band_y = GROUND_Y - sprites::BAND.h;
    let mut clip = Clip { s, x0, y0, x1: x0 + w, y1: y0 + h };
    clip.rect(0, 0, SIZE, band_y, sprites::SKY);
    let tiles = |sp: &Sprite, shift: i32, y: i32, clip: &mut Clip<_>| {
        if y0 + h > y && y0 < y + sp.h {
            for k in 0..3 {
                let x = k * sp.w - shift;
                if x < x0 + w && x + sp.w > x0 {
                    blit(clip, sp, x, y);
                }
            }
        }
    };
    tiles(&sprites::BAND, band_x(g), band_y, &mut clip);
    tiles(&sprites::GROUND, ground_x(g), GROUND_Y, &mut clip);
}

/// The background of a box: the scenery, then the parts of gates in it.
fn background(s: &mut impl Surface, g: &Game, x0: i32, y0: i32, w: i32, h: i32) {
    scenery(s, g, x0, y0, w, h);
    let mut clip = Clip { s, x0, y0, x1: x0 + w, y1: y0 + h };
    for gt in &g.gates {
        pipes(&mut clip, gt);
    }
}

/// A gate at its new place; with `from`, the strip it left behind on the right gets the scenery back first.
fn gate(s: &mut impl Surface, g: &Game, gt: &Gate, from: Option<i32>) {
    if gt.x >= SIZE && from.is_none_or(|x| x >= SIZE) {
        return;
    }
    if let Some(old) = from {
        // What the pipe left behind, and its two margin columns that the sprite does not paint. Not the ground:
        // it is repainted whenever it scrolls.
        let x0 = gt.x + GATE_W - 2;
        scenery(s, g, x0, 0, old + GATE_W - x0, GROUND_Y);
    }
    pipes(s, gt);
}

fn bird(wing: usize) -> &'static Sprite {
    [&sprites::BIRD_UP, &sprites::BIRD_MID, &sprites::BIRD_DOWN][wing.min(2)]
}

/// The bird at its height; `from`: its last height, whose box gets the background back in the same pass.
fn critter(s: &mut impl Surface, g: &Game, from: Option<i32>) {
    let y = g.hop_y();
    let (lo, hi) = from.map_or((y, y), |f| (f.min(y), f.max(y)));
    background(s, g, HOP_X - BIRD_W / 2, lo - BIRD_H / 2, BIRD_W, hi - lo + BIRD_H);
    blit(s, bird(g.wing()), HOP_X - BIRD_W / 2, y - BIRD_H / 2);
}

fn number_width(text: &str) -> i32 {
    text.len() as i32 * (24 + DIGIT_GAP) - DIGIT_GAP - text.bytes().filter(|&b| b == b'1').count() as i32 * 8
}

/// `text` (digits) in the sprite digits, centred on `cx`.
fn digits(s: &mut impl Surface, text: &str, cx: i32, y: i32) {
    let mut x = cx - number_width(text) / 2;
    for d in text.bytes() {
        let sp = DIGITS[(d - b'0').min(9) as usize];
        blit(s, sp, x, y);
        x += sp.w + DIGIT_GAP;
    }
}

fn score(s: &mut impl Surface, g: &Game, clear: bool) {
    let mut buf = [0u8; 11];
    let text = int(g.score as i32, &mut buf);
    if clear {
        background(s, g, C - 80, SCORE_Y, 160, DIGIT_H);
    }
    digits(s, text, C, SCORE_Y);
}

fn best_line(s: &mut impl Surface, y: i32, best: u32, c: u16) {
    let mut buf = [0u8; 11];
    let mut line = [0u8; 16];
    let n = int(best as i32, &mut buf).as_bytes();
    line[..5].copy_from_slice(b"BEST ");
    line[5..5 + n.len()].copy_from_slice(n);
    centre(s, C, y, core::str::from_utf8(&line[..5 + n.len()]).unwrap_or("?"), c);
}

fn ready(s: &mut impl Surface, best: u32) {
    const TITLE: &str = "SKY HOP";
    big(s, C - big_width(TITLE, 4) / 2, 72, TITLE, 4, WHITE);
    blit(s, &sprites::GET_READY, C - sprites::GET_READY.w / 2, 124);
    blit(s, &sprites::TAP_HINT, C - sprites::TAP_HINT.w / 2, 296);
    if best > 0 {
        best_line(s, 424, best, INK.rgb565());
    }
}

fn over(s: &mut impl Surface, g: &Game, best: u32) {
    rrect(s, C - 140, 150, 280, 170, 28, PANEL, Rgb(78, 192, 202));
    blit(s, &sprites::GAMEOVER, C - sprites::GAMEOVER.w / 2, 162);
    let mut buf = [0u8; 11];
    digits(s, int(g.score as i32, &mut buf), C, 212);
    best_line(s, 262, best, GREY);
    centre(s, C, 290, "TAP TO RETRY", GREY);
}

/// The town strip moved: repaint its rows, the pipes in them and the bird.
fn scroll_band(s: &mut impl Surface, g: &Game) {
    let y = GROUND_Y - sprites::BAND.h;
    background(s, g, 0, y, SIZE, sprites::BAND.h);
    blit(s, bird(g.wing()), HOP_X - BIRD_W / 2, g.hop_y() - BIRD_H / 2);
}

/// Draws the frame for `g`. `prev`: what is on screen now; `None`, or another state, repaints everything.
pub fn frame(s: &mut impl Surface, g: &Game, best: u32, prev: Option<Seen>) -> Seen {
    let now = seen(g);
    let prev = prev.filter(|p| p.state == g.state);
    match prev {
        None => {
            scenery(s, g, 0, 0, SIZE, SIZE);
            for gt in &g.gates {
                gate(s, g, gt, None);
            }
            critter(s, g, None);
            match g.state {
                State::Ready => ready(s, best),
                State::Playing => score(s, g, false),
                State::Over => {
                    score(s, g, false);
                    over(s, g, best);
                }
            }
        }
        Some(p) if p == now => {}
        Some(p) => {
            if g.state == State::Playing {
                for (gt, x) in g.gates.iter().zip(p.xs) {
                    gate(s, g, gt, Some(x));
                }
            }
            if g.state == State::Playing && p.band != now.band {
                scroll_band(s, g);
            }
            critter(s, g, Some(p.y));
            if g.state == State::Playing {
                if p.ground != now.ground {
                    let (y, h) = (GROUND_Y, SIZE - GROUND_Y);
                    scenery(s, g, 0, y, SIZE, h);
                }
                score(s, g, p.score != g.score);
            }
        }
    }
    now
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{GATES, Step};
    use cmf_wp2_ui::Fb;
    use std::vec;
    use std::vec::Vec;

    fn screen() -> Vec<u16> {
        vec![0u16; (SIZE * SIZE) as usize]
    }

    fn full(g: &Game, best: u32) -> Vec<u16> {
        let mut px = screen();
        frame(&mut Fb::new(&mut px), g, best, None);
        px
    }

    /// Frame by frame, what the watch ends up with must be what a full repaint draws.
    #[test]
    fn partial_frames_match_a_full_repaint() {
        for seed in [1u32, 8, 77] {
            let mut g = Game::new(seed);
            let mut px = screen();
            let mut prev = None;
            let mut checked = 0;
            for tick in 0..900u32 {
                if tick == 5 || (g.state == State::Playing && tick % 13 == 0) {
                    g.flap();
                }
                if g.state == State::Over {
                    break;
                }
                let _: Step = g.step();
                prev = Some(frame(&mut Fb::new(&mut px), &g, 3, prev));
                if tick % 7 == 0 || g.state == State::Over {
                    let want = full(&g, 3);
                    let bad = px.iter().zip(&want).position(|(a, b)| a != b);
                    assert_eq!(bad.map(|i| (i as i32 % SIZE, i as i32 / SIZE)), None, "seed {seed} tick {tick}");
                    checked += 1;
                }
            }
            assert!(checked > 5);
        }
    }

    #[test]
    fn a_still_frame_draws_nothing() {
        let g = Game::new(4);
        let mut px = screen();
        let mut fb = Fb::new(&mut px);
        let p = frame(&mut fb, &g, 0, None);
        let rects = fb.rects;
        frame(&mut fb, &g, 0, Some(p));
        assert_eq!(fb.rects, rects);
    }

    #[test]
    fn critter_is_drawn_where_it_is() {
        let g = Game::new(4);
        let px = full(&g, 0);
        let fb_at = |x: i32, y: i32| px[(y * SIZE + x) as usize];
        let (rx, ry, _, _, c) = bird(g.wing()).rects[0];
        assert_eq!(fb_at(HOP_X - BIRD_W / 2 + rx as i32, g.hop_y() - BIRD_H / 2 + ry as i32), c);
        let mut bare = screen();
        scenery(&mut Fb::new(&mut bare), &g, 0, 0, SIZE, SIZE);
        let (x, y) = (HOP_X - BIRD_W, g.hop_y());
        assert_eq!(fb_at(x, y), bare[(y * SIZE + x) as usize]);
    }

    /// Collects the rects drawn.
    struct Rects(Vec<(i32, i32, i32, i32)>);

    impl Surface for Rects {
        fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, _: u16) {
            self.0.push((x, y, w, h));
        }
        fn text(&mut self, _: i32, _: i32, _: &str, _: u16) {}
    }

    #[test]
    fn pipe_parts_cover_the_gate_without_overlap() {
        let gt = Gate { x: 100, top: 150, bottom: 290, ..Game::new(1).gates[0] };
        let mut rects = Rects(Vec::new());
        pipes(&mut rects, &gt);
        let rects = rects.0;
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let apart = a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1;
                assert!(apart, "{a:?} {b:?}");
            }
        }
        assert!(rects.iter().all(|r| r.0 >= gt.x && r.0 + r.2 <= gt.x + GATE_W && r.1 >= 0 && r.1 + r.3 <= GROUND_Y));
        assert_eq!(GATES, 3);
        assert_eq!(sprites::PIPE_W, GATE_W);
    }

    #[test]
    fn digits_are_as_wide_as_the_sprites() {
        for text in ["0", "1", "10", "2024", "111"] {
            let sum: i32 = text.bytes().map(|b| DIGITS[(b - b'0') as usize].w).sum::<i32>() + DIGIT_GAP * (text.len() as i32 - 1);
            assert_eq!(number_width(text), sum, "{text}");
        }
    }
}
