// The game itself: no Api, so `cargo test` runs it on the host. Fixed point only (the watch has no FPU): the
// critter's height and speed are in 1/256 px, everything else in whole pixels. One `step` is one 33 ms tick.

/// Screen size and centre; the panel is a disc of radius `C`.
pub const SIZE: i32 = 466;
pub const C: i32 = SIZE / 2;

/// Fixed-point one for the critter's height and speed.
pub const Q: i32 = 256;
/// The critter: its x never changes, its body is a disc of this radius.
pub const HOP_X: i32 = C;
pub const R: i32 = 13;
/// Per tick², per tick, and the fastest fall per tick (Q).
pub const GRAVITY: i32 = 140;
pub const FLAP: i32 = -2150;
pub const MAX_FALL: i32 = 2600;

/// Gates: width, distance from one gate's left edge to the next one's, gap height, how far the gap's centre may
/// sit from the screen's centre line and how far it may move from one gate to the next.
pub const GATE_W: i32 = 52; // the pipe sprite's width
pub const SPACING: i32 = 230;
pub const GATES: usize = 3;
/// The ground's top edge: the bird's wall at the bottom, and where the pipes end.
pub const GROUND_Y: i32 = 388;
pub const GAP_START: i32 = 160;
pub const GAP_MIN: i32 = 120;
pub const GAP_SWING: i32 = 70;
pub const GAP_STEP: i32 = 70;
/// Where the first gate enters, just off the right edge.
pub const FIRST_X: i32 = SIZE + 40;

/// Ready: hovering, waiting for the first tap. Over: crashed, the screen holds still.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ready,
    Playing,
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Nothing,
    Scored,
    Crashed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Gate {
    /// Left edge.
    pub x: i32,
    /// The gap is `top..bottom`.
    pub top: i32,
    pub bottom: i32,
    pub(crate) passed: bool,
}

impl Gate {
    pub fn centre(&self) -> i32 {
        (self.top + self.bottom) / 2
    }
}

#[derive(Clone)]
pub struct Game {
    pub state: State,
    /// Centre of the critter, Q.
    pub y: i32,
    pub vy: i32,
    pub gates: [Gate; GATES],
    pub score: u32,
    /// How far the pipes have travelled this run, px: the scenery scrolls by fractions of it.
    pub scroll: i32,
    tick: u32,
    rng: u32,
}

/// xorshift32: no random source on the watch, the app seeds it from the clock.
fn next(rng: &mut u32) -> u32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *rng = x;
    x
}

/// Gates move this many px per tick: 4, one more every 10 points, at most 6.
pub fn speed(score: u32) -> i32 {
    4 + (score / 10).min(2) as i32
}

/// The gap narrows by 3 px a point down to `GAP_MIN`.
pub fn gap(score: u32) -> i32 {
    (GAP_START - 3 * score.min(100) as i32).max(GAP_MIN)
}

/// Ready-screen hover: a triangle wave of ±6 px over 48 ticks.
fn hover(tick: u32) -> i32 {
    let t = (tick % 48) as i32;
    (if t < 24 { t } else { 48 - t }) / 2 - 6
}

impl Game {
    pub fn new(seed: u32) -> Game {
        let mut g = Game {
            state: State::Ready,
            y: C * Q,
            vy: 0,
            gates: [Gate { x: 0, top: 0, bottom: 0, passed: false }; GATES],
            score: 0,
            scroll: 0,
            tick: 0,
            rng: seed | 1, // xorshift sticks at 0
        };
        g.restart();
        g
    }

    /// Back to Ready, keeping the random sequence going.
    pub fn restart(&mut self) {
        self.state = State::Ready;
        self.y = C * Q;
        self.vy = 0;
        self.score = 0;
        self.scroll = 0;
        self.tick = 0;
        let mut prev = C;
        for i in 0..GATES {
            self.gates[i] = self.gate(FIRST_X + i as i32 * SPACING, prev);
            prev = self.gates[i].centre();
        }
    }

    /// A new gate at `x`, its gap at most `GAP_STEP` from `prev` (the gap before it) so it can be reached.
    fn gate(&mut self, x: i32, prev: i32) -> Gate {
        let h = gap(self.score);
        let shift = (next(&mut self.rng) % (2 * GAP_STEP as u32 + 1)) as i32 - GAP_STEP;
        let centre = (prev + shift).clamp(C - GAP_SWING, C + GAP_SWING);
        Gate { x, top: centre - h / 2, bottom: centre - h / 2 + h, passed: false }
    }

    /// Centre of the critter in px.
    pub fn hop_y(&self) -> i32 {
        (self.y + Q / 2).div_euclid(Q)
    }

    /// Which of the bird's three wing frames to show: up 0, middle 1, down 2. A flapping cycle, folded when it crashed.
    pub fn wing(&self) -> usize {
        if self.state == State::Over { 2 } else { [1, 0, 1, 2][(self.tick / 3 % 4) as usize] }
    }

    /// A tap or a crown detent. Ready: starts the run. Over: does nothing (the app decides when to restart).
    pub fn flap(&mut self) {
        match self.state {
            State::Ready => {
                self.state = State::Playing;
                self.vy = FLAP;
            }
            State::Playing => self.vy = FLAP,
            State::Over => {}
        }
    }

    pub fn step(&mut self) -> Step {
        self.tick = self.tick.wrapping_add(1);
        match self.state {
            State::Ready => {
                self.y = (C + hover(self.tick)) * Q;
                Step::Nothing
            }
            State::Over => Step::Nothing,
            State::Playing => self.fly(),
        }
    }

    fn fly(&mut self) -> Step {
        self.vy = (self.vy + GRAVITY).min(MAX_FALL);
        self.y += self.vy;
        let dx = speed(self.score);
        self.scroll += dx;
        let mut out = Step::Nothing;
        for i in 0..GATES {
            self.gates[i].x -= dx;
            let g = self.gates[i];
            if !g.passed && g.x + GATE_W < HOP_X - R {
                self.gates[i].passed = true;
                self.score += 1;
                out = Step::Scored;
            }
        }
        for i in 0..GATES {
            if self.gates[i].x + GATE_W < 0 {
                let far = *self.gates.iter().max_by_key(|g| g.x).unwrap_or(&self.gates[i]);
                self.gates[i] = self.gate(far.x + SPACING, far.centre());
            }
        }
        if self.crashed() {
            self.state = State::Over;
            return Step::Crashed;
        }
        out
    }

    /// Out of the round screen, on the ground, or touching a gate.
    pub fn crashed(&self) -> bool {
        let (x, y) = (HOP_X, self.hop_y());
        let (dx, dy) = (x - C, y - C);
        let reach = C - R - 2;
        if dx * dx + dy * dy > reach * reach || y + R >= GROUND_Y {
            return true;
        }
        self.gates.iter().any(|g| hits(x, y, R, g.x, i32::MIN / 2, GATE_W, g.top) || hits(x, y, R, g.x, g.bottom, GATE_W, i32::MAX / 2))
    }
}

/// A disc against the rectangle `x0..x0+w`, `y0..y1`.
fn hits(cx: i32, cy: i32, r: i32, x0: i32, y0: i32, w: i32, y1: i32) -> bool {
    let nx = cx.clamp(x0, x0 + w - 1);
    let ny = cy.clamp(y0, y1 - 1);
    let (dx, dy) = ((cx - nx) as i64, (cy - ny) as i64);
    dx * dx + dy * dy < (r * r) as i64
}

/// The saved blob: "SH1" and the best score, little endian.
pub const MAGIC: &[u8; 3] = b"SH1";

pub fn encode(best: u32) -> [u8; 7] {
    let b = best.to_le_bytes();
    [MAGIC[0], MAGIC[1], MAGIC[2], b[0], b[1], b[2], b[3]]
}

/// Anything that isn't a blob of ours is no best score yet.
pub fn decode(blob: &[u8]) -> u32 {
    match blob {
        [b'S', b'H', b'1', a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(seed: u32) -> Game {
        let mut g = Game::new(seed);
        g.flap();
        g
    }

    /// Whether some sequence of hops (at most one every 3 ticks) survives the next `depth` ticks.
    fn survives(g: &Game, depth: u32, since: u32) -> bool {
        if depth == 0 {
            return true;
        }
        let mut wait = g.clone();
        if wait.step() != Step::Crashed && survives(&wait, depth - 1, since + 1) {
            return true;
        }
        if since < 3 {
            return false;
        }
        let mut hop = g.clone();
        hop.flap();
        hop.step() != Step::Crashed && survives(&hop, depth - 1, 0)
    }

    /// Hops only when waiting would leave no way through the next second: a player who plans ahead.
    fn autopilot(g: &mut Game) {
        let mut wait = g.clone();
        if wait.step() == Step::Crashed || !survives(&wait, 30, 3) {
            g.flap();
        }
    }

    #[test]
    fn ready_hovers_until_the_first_flap() {
        let mut g = Game::new(7);
        for _ in 0..200 {
            assert_eq!(g.step(), Step::Nothing);
            assert!((g.hop_y() - C).abs() <= 6);
            assert_eq!(g.state, State::Ready);
        }
        let xs = g.gates.map(|gt| gt.x);
        g.flap();
        assert_eq!(g.state, State::Playing);
        assert_eq!(g.vy, FLAP);
        g.step();
        assert!(g.gates.iter().zip(xs).all(|(gt, x)| gt.x == x - speed(0)));
    }

    #[test]
    fn falling_without_flaps_hits_the_bottom_edge() {
        let mut g = playing(3);
        let mut ticks = 0;
        while g.step() != Step::Crashed {
            ticks += 1;
            assert!(ticks < 200);
        }
        assert_eq!(g.state, State::Over);
        assert!(g.hop_y() > C);
        // over: nothing moves, flaps are ignored
        let y = g.y;
        g.flap();
        g.step();
        assert_eq!((g.y, g.state), (y, State::Over));
    }

    #[test]
    fn flapping_all_the_time_hits_the_top_edge() {
        let mut g = playing(3);
        let mut ticks = 0;
        loop {
            g.flap();
            if g.step() == Step::Crashed {
                break;
            }
            ticks += 1;
            assert!(ticks < 200);
        }
        assert!(g.hop_y() < C);
    }

    #[test]
    fn fall_speed_is_capped() {
        let mut g = playing(1);
        g.gates.iter_mut().for_each(|gt| gt.x = 10_000);
        g.y = 0;
        for _ in 0..100 {
            g.fly();
            g.state = State::Playing;
        }
        assert_eq!(g.vy, MAX_FALL);
    }

    #[test]
    fn autopilot_scores_and_the_game_gets_harder() {
        for seed in [1, 2, 3, 99, 12345] {
            let mut g = playing(seed);
            let mut scored = 0;
            while g.score < 100 {
                autopilot(&mut g);
                match g.step() {
                    Step::Crashed => panic!("seed {seed}: crashed at {} (y {}) {:?}", g.score, g.hop_y(), g.gates),
                    Step::Scored => scored += 1,
                    Step::Nothing => {}
                }
            }
            assert_eq!(g.score, scored);
            assert_eq!(speed(g.score), 6);
        }
    }

    #[test]
    fn gates_keep_their_spacing_and_gaps_stay_on_screen() {
        let mut g = playing(42);
        for _ in 0..5000 {
            autopilot(&mut g);
            g.step();
            let mut xs = g.gates.map(|gt| gt.x);
            xs.sort();
            assert_eq!(xs[1] - xs[0], SPACING);
            assert_eq!(xs[2] - xs[1], SPACING);
            for gt in g.gates {
                assert!(gt.bottom - gt.top >= GAP_MIN);
                assert!(gt.top >= C - GAP_SWING - GAP_START / 2 && gt.bottom <= C + GAP_SWING + GAP_START / 2);
            }
            for (a, b) in [(xs[0], xs[1]), (xs[1], xs[2])] {
                let at = |x| g.gates.iter().find(|gt| gt.x == x).unwrap().centre();
                assert!((at(a) - at(b)).abs() <= GAP_STEP + 1);
            }
        }
    }

    #[test]
    fn touching_a_gate_crashes() {
        let mut g = playing(5);
        g.gates[0] = Gate { x: HOP_X - 10, top: 0, bottom: SIZE, passed: false };
        g.gates[1].x = 1000;
        g.gates[2].x = 1230;
        assert!(!g.crashed(), "a gap as tall as the screen");
        g.gates[0].top = C - 20;
        assert!(!g.crashed(), "the critter's top is 8 px below the top part");
        g.y = (C - 8) * Q;
        assert!(!g.crashed(), "just touching");
        g.y = (C - 9) * Q;
        assert!(g.crashed(), "the top part reaches into the critter");
        g.gates[0] = Gate { x: HOP_X + R, top: 0, bottom: C - 8, passed: false };
        assert!(!g.crashed(), "the gate starts right after the critter's edge");
    }

    #[test]
    fn the_scenery_scrolls_with_the_pipes_while_playing() {
        let mut g = Game::new(2);
        for _ in 0..30 {
            g.step();
        }
        assert_eq!(g.scroll, 0, "not while ready");
        g.flap();
        let mut dist = 0;
        for _ in 0..20 {
            autopilot(&mut g);
            dist += speed(g.score);
            g.step();
        }
        assert_eq!(g.scroll, dist);
        g.restart();
        assert_eq!(g.scroll, 0);
    }

    #[test]
    fn restart_resets_the_run_but_not_the_randomness() {
        let mut g = playing(9);
        let first = g.gates;
        while g.step() != Step::Crashed {}
        g.restart();
        assert_eq!((g.state, g.score, g.hop_y()), (State::Ready, 0, C));
        assert_eq!(g.gates[0].x, FIRST_X);
        assert_ne!(g.gates.map(|gt| gt.top), first.map(|gt| gt.top));
    }

    #[test]
    fn best_score_round_trips_and_garbage_is_zero() {
        assert_eq!(decode(&encode(0)), 0);
        assert_eq!(decode(&encode(1234)), 1234);
        assert_eq!(decode(b""), 0);
        assert_eq!(decode(b"SH2\x01\0\0\0"), 0);
        assert_eq!(decode(b"SH1\x01\0\0"), 0);
    }
}
