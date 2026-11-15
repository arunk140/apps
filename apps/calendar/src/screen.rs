//! The two screens, in the stock look (`cmf_wp2_app::stock`):
//! - month: the accent month title between `<` and `>`, weekday initials, a 6×7 grid of days with a dot per calendar
//!   that has an event, today in an accent pill, a summary line under it;
//! - widget (the page next to the watch face): today's weekday, date and month, this week with its dots, today's
//!   event count.
//!
//! Each screen is one root object under `ROOT`; leaving it deletes the root and everything in it. A screen is built
//! hidden, over as many batches as it needs, and shown in the last one.

use crate::date::{self, Day, Month, CELLS};
use crate::marks::{self, Marks, CALENDARS};
use cmf_wp2_app::lv::*;
use cmf_wp2_app::stock::{self, ACCENT, BLACK, CIRCLE, GREY, SCREEN, WHITE};
use core::fmt::Write;

/// Font slots besides the system font (slot 0).
pub const F24: u8 = 1;
pub const F48: u8 = 2;

/// Dot colours per calendar: the stock accent, then light blue and light green for the second and third.
pub const DOT_COLOURS: [i32; CALENDARS] = [ACCENT, 0x867D, 0x9772];
const DOT: i32 = 6;
const DOT_STEP: i16 = 10;

// month grid geometry, inside the 466 px circle (the bottom row's corners at radius ~200)
const COL_W: i32 = 48;
const ROW_H: i32 = 44;
const GRID_X: i32 = (SCREEN - 7 * COL_W) / 2;
const GRID_Y: i32 = 128;
const CELL_W: i32 = COL_W - 4;
const CELL_H: i32 = ROW_H - 4;
const TITLE_Y: i16 = 44;
const TITLE_W: i32 = 190;
const ARROW: i32 = 64;
const ARROW_DX: i16 = 125;
const WEEKDAYS_Y: i16 = 94;
const FOOTER_Y: i16 = GRID_Y as i16 + 6 * ROW_H as i16 + 8;

// bytes one cell, dots included, can take: flush first when fewer are left
const GROUP: usize = 320;

pub type Flush<'a> = dyn FnMut(&mut Batch) -> Result<(), LvError> + 'a;

fn room(b: &mut Batch, flush: &mut Flush) -> Result<(), LvError> {
    if b.bytes().len() + GROUP > BUF { flush(b) } else { Ok(()) }
}

/// The fonts, in a batch of their own.
pub fn fonts(b: &mut Batch) -> Result<(), LvError> {
    stock::font(b)?;
    b.font(F24, StockFont::F24M)?;
    b.font(F48, StockFont::F48M)
}

/// A short line of text in a fixed buffer.
pub struct Line {
    buf: [u8; 40],
    len: usize,
}

impl Line {
    pub fn new() -> Line {
        Line { buf: [0; 40], len: 0 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.len + s.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}

fn events(n: u32, out: &mut Line) {
    let _ = match n {
        0 => write!(out, "no events"),
        1 => write!(out, "1 event"),
        n => write!(out, "{n} events"),
    };
}

/// The line under the month grid.
pub fn month_summary(shown: Month, today: Option<Day>, k: &Marks) -> Line {
    let mut out = Line::new();
    if !k.synced(shown) {
        let _ = out.write_str("not synced");
    } else if let Some(t) = today.filter(|t| t.month == shown) {
        let _ = out.write_str("today: ");
        events(marks::count(k.get(t)) as u32, &mut out);
    } else {
        let n = (1..=shown.days()).map(|d| marks::count(k.get(Day { month: shown, day: d })) as u32).sum();
        events(n, &mut out);
    }
    out
}

/// Today's events, for the widget screen and the card.
pub fn today_summary(today: Day, k: &Marks) -> Line {
    let mut out = Line::new();
    if !k.synced(today.month) {
        let _ = out.write_str("not synced");
    } else {
        events(marks::count(k.get(today)) as u32, &mut out);
        let _ = out.write_str(" today");
    }
    out
}

/// The card left of the watch face: "Sun 15 Nov" and today's events.
pub fn card_value(today: Day, k: &Marks) -> Line {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut out = Line::new();
    let _ = write!(
        out,
        "{} {} {}\n{}",
        DAYS[date::weekday(today) as usize],
        today.day,
        date::SHORT[today.month.month as usize - 1],
        today_summary(today, k).as_str()
    );
    out
}

fn bare(b: &mut Batch, o: Obj) -> Result<(), LvError> {
    for p in [Prop::PadTop, Prop::PadBottom, Prop::PadLeft, Prop::PadRight, Prop::BorderWidth, Prop::BgOpa, Prop::Radius] {
        b.style(o, p, 0, 0)?;
    }
    b.flags(o, &[], &[Flag::Scrollable])
}

/// A full-screen, transparent, hidden root that sends CLICKED (the widget screen's tap).
fn root(b: &mut Batch) -> Result<Obj, LvError> {
    let r = b.create(Class::Obj, ROOT)?;
    b.size(r, SCREEN, SCREEN)?;
    bare(b, r)?;
    b.style(r, Prop::TextAlign, 0, 2)?; // LV_TEXT_ALIGN_CENTER, inherited by every label
    b.flags(r, &[Flag::Hidden], &[])?;
    Ok(r)
}

fn label(b: &mut Batch, parent: Obj, text: &str, w: i32, x: i16, y: i16) -> Result<Obj, LvError> {
    let l = b.create(Class::Label, parent)?;
    b.size(l, w, CONTENT)?;
    b.text(l, text)?;
    b.align(l, Align::TopMid, x, y)?;
    Ok(l)
}

fn num(n: u8) -> Line {
    let mut l = Line::new();
    let _ = write!(l, "{n}");
    l
}

/// One day: its number, `today` in the accent pill, a dot per calendar with events. `x`, `y`: top left in `parent`.
fn cell(b: &mut Batch, parent: Obj, d: Day, dim: bool, today: bool, mark: u8, x: i32, y: i32) -> Result<Obj, LvError> {
    let c = b.create(Class::Label, parent)?;
    b.size(c, CELL_W, CELL_H)?;
    b.align(c, Align::TopLeft, x as i16, y as i16)?;
    b.style(c, Prop::PadTop, 0, 2)?;
    b.text(c, num(d.day).as_str())?;
    if today {
        b.style(c, Prop::BgColor, 0, ACCENT)?;
        b.style(c, Prop::BgOpa, 0, 255)?;
        b.style(c, Prop::Radius, 0, 12)?;
    } else if dim {
        b.style(c, Prop::TextColor, 0, GREY)?;
    }
    let cals = marks::calendars(mark);
    let n = cals.count_ones() as i16;
    let mut i = 0;
    for k in 0..CALENDARS {
        if cals & (1 << k) == 0 {
            continue;
        }
        let dot = b.create(Class::Obj, c)?;
        b.size(dot, DOT, DOT)?;
        b.style(dot, Prop::Radius, 0, CIRCLE)?;
        b.style(dot, Prop::BorderWidth, 0, 0)?;
        // on the accent pill the accent dot would vanish: white there
        let colour = if today && DOT_COLOURS[k] == ACCENT { WHITE } else { DOT_COLOURS[k] };
        b.style(dot, Prop::BgColor, 0, colour)?;
        b.style(dot, Prop::BgOpa, 0, if dim { 128 } else { 255 })?;
        b.align(dot, Align::BottomMid, -(n - 1) * DOT_STEP / 2 + i * DOT_STEP, -3)?;
        i += 1;
    }
    Ok(c)
}

/// A tap target with a grey glyph.
fn arrow(b: &mut Batch, parent: Obj, glyph: &str, dx: i16) -> Result<Obj, LvError> {
    let a = b.create(Class::Obj, parent)?;
    b.size(a, ARROW, ARROW)?;
    bare(b, a)?;
    b.style(a, Prop::BgColor, stock::PRESSED, stock::ROW_PRESSED)?;
    b.style(a, Prop::BgOpa, stock::PRESSED, 255)?;
    b.style(a, Prop::Radius, 0, CIRCLE)?;
    b.align(a, Align::TopMid, dx, TITLE_Y - (ARROW as i16 - 32) / 2)?;
    b.events(a, &[Ev::Clicked])?;
    let l = b.create(Class::Label, a)?;
    b.style(l, Prop::TextColor, 0, GREY)?;
    b.text(l, glyph)?;
    b.align(l, Align::Center, 0, 0)?;
    Ok(a)
}

#[derive(Clone, Copy, Debug)]
pub struct MonthView {
    pub root: Obj,
    pub title: Obj,
    pub prev: Obj,
    pub next: Obj,
    pub shown: Month,
}

pub fn month_view(b: &mut Batch, flush: &mut Flush, shown: Month, today: Option<Day>, k: &Marks)
    -> Result<MonthView, LvError> {
    room(b, flush)?;
    let r = root(b)?;
    let mut t = Line::new();
    let _ = write!(t, "{} {}", shown.name(), shown.year);
    let title = label(b, r, t.as_str(), TITLE_W, 0, TITLE_Y)?;
    b.style(title, Prop::TextColor, 0, ACCENT)?;
    b.style(title, Prop::TextFont, 0, F24 as i32)?;
    b.flags(title, &[Flag::Clickable], &[])?;
    b.events(title, &[Ev::Clicked])?;
    let prev = arrow(b, r, "<", -ARROW_DX)?;
    let next = arrow(b, r, ">", ARROW_DX)?;
    room(b, flush)?;
    for (i, w) in date::WEEKDAYS.iter().enumerate() {
        let l = b.create(Class::Label, r)?;
        b.size(l, COL_W, CONTENT)?;
        b.style(l, Prop::TextColor, 0, GREY)?;
        b.style(l, Prop::TextFont, 0, F24 as i32)?;
        b.text(l, w)?;
        b.align(l, Align::TopLeft, (GRID_X + i as i32 * COL_W) as i16, WEEKDAYS_Y)?;
    }
    for (i, d) in date::grid(shown).iter().enumerate().take(CELLS) {
        room(b, flush)?;
        let (x, y) = (GRID_X + (i % 7) as i32 * COL_W + 2, GRID_Y + (i / 7) as i32 * ROW_H + 2);
        cell(b, r, *d, d.month != shown, today == Some(*d), k.get(*d), x, y)?;
    }
    room(b, flush)?;
    let foot = label(b, r, month_summary(shown, today, k).as_str(), 300, 0, FOOTER_Y)?;
    b.style(foot, Prop::TextColor, 0, GREY)?;
    b.style(foot, Prop::TextFont, 0, F24 as i32)?;
    b.flags(r, &[], &[Flag::Hidden])?;
    Ok(MonthView { root: r, title, prev, next, shown })
}

#[derive(Clone, Copy, Debug)]
pub struct WidgetView {
    pub root: Obj,
    pub today: Day,
}

// widget geometry
const W_WEEKDAY_Y: i16 = 70;
const W_DAY_Y: i16 = 104;
const W_MONTH_Y: i16 = 168;
const W_INITIALS_Y: i16 = 222;
const W_WEEK_Y: i32 = 252;
const W_FOOTER_Y: i16 = 322;

pub fn widget_view(b: &mut Batch, flush: &mut Flush, today: Day, k: &Marks) -> Result<WidgetView, LvError> {
    room(b, flush)?;
    let r = root(b)?;
    b.flags(r, &[Flag::Clickable], &[])?;
    b.events(r, &[Ev::Clicked])?;
    let wd = label(b, r, date::WEEKDAY_NAMES[date::weekday(today) as usize], 300, 0, W_WEEKDAY_Y)?;
    b.style(wd, Prop::TextColor, 0, GREY)?;
    let day = label(b, r, num(today.day).as_str(), 200, 0, W_DAY_Y)?;
    b.style(day, Prop::TextFont, 0, F48 as i32)?;
    let mut t = Line::new();
    let _ = write!(t, "{} {}", today.month.name(), today.month.year);
    let mo = label(b, r, t.as_str(), 300, 0, W_MONTH_Y)?;
    b.style(mo, Prop::TextColor, 0, ACCENT)?;
    b.style(mo, Prop::TextFont, 0, F24 as i32)?;
    room(b, flush)?;
    for (i, w) in date::WEEKDAYS.iter().enumerate() {
        let l = b.create(Class::Label, r)?;
        b.size(l, COL_W, CONTENT)?;
        b.style(l, Prop::TextColor, 0, GREY)?;
        b.style(l, Prop::TextFont, 0, F24 as i32)?;
        b.text(l, w)?;
        b.align(l, Align::TopLeft, (GRID_X + i as i32 * COL_W) as i16, W_INITIALS_Y)?;
    }
    for (i, d) in date::week(today).iter().enumerate() {
        room(b, flush)?;
        cell(b, r, *d, d.month != today.month, *d == today, k.get(*d), GRID_X + i as i32 * COL_W + 2, W_WEEK_Y)?;
    }
    room(b, flush)?;
    let foot = label(b, r, today_summary(today, k).as_str(), 300, 0, W_FOOTER_Y)?;
    b.style(foot, Prop::TextFont, 0, F24 as i32)?;
    b.flags(r, &[], &[Flag::Hidden])?;
    Ok(WidgetView { root: r, today })
}

/// Deletes a screen.
pub fn leave(b: &mut Batch, root: Obj) -> Result<(), LvError> {
    if root != ROOT { b.del(root) } else { Ok(()) }
}

/// Black behind everything, the system font and white text: first, once.
pub fn theme(b: &mut Batch) -> Result<(), LvError> {
    stock::theme(b)?;
    b.style(ROOT, Prop::BgColor, 0, BLACK)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::boxed::Box;
    use std::vec::Vec;
    use wp2_core::lvcmd::{self, Table};

    fn batch() -> Batch {
        Batch::with_buf(Box::leak(Box::new([0; BUF])))
    }

    fn m(y: u16, mo: u8) -> Month {
        Month::new(y, mo).unwrap()
    }

    const FONTS: u8 = 0b111;

    /// Runs `build` with a flush that validates each batch against the loader's table and applies it; returns the
    /// table, the batches' sizes and the result.
    fn run<T>(t: &mut Table, b: &mut Batch, build: impl FnOnce(&mut Batch, &mut Flush) -> Result<T, LvError>)
        -> (T, Vec<usize>) {
        let sizes = std::cell::RefCell::new(Vec::new());
        let mut flush = |b: &mut Batch| -> Result<(), LvError> {
            assert_eq!(lvcmd::validate(b.bytes(), t, FONTS), Ok(()));
            for r in lvcmd::records(b.bytes()) {
                lvcmd::apply(t, &r.unwrap());
            }
            sizes.borrow_mut().push(b.bytes().len());
            b.clear();
            Ok(())
        };
        let out = build(b, &mut flush).unwrap();
        flush(b).unwrap();
        (out, sizes.into_inner())
    }

    fn busy() -> Marks {
        // every calendar, every day: the most objects a month can take
        let mut k = Marks::new();
        for mo in [m(2026, 10), m(2026, 11), m(2026, 12)] {
            k.set(mo, &[marks::mark(0b111, 31); 31]);
        }
        k
    }

    #[test]
    fn fonts_validate_on_their_own() {
        let mut b = batch();
        fonts(&mut b).unwrap();
        assert_eq!(lvcmd::validate(b.bytes(), &Table::new(), 0), Ok(()));
    }

    #[test]
    fn month_view_validates_and_fits_the_budget() {
        let today = Day { month: m(2026, 11), day: 15 };
        for k in [Marks::new(), busy()] {
            let mut t = Table::new();
            let mut b = batch();
            let (v, sizes) = run(&mut t, &mut b, |b, f| {
                theme(b)?;
                month_view(b, f, m(2026, 11), Some(today), &k)
            });
            assert!(sizes.iter().all(|&n| n <= BUF));
            std::println!("month view: {} batches, {sizes:?} B", sizes.len());
            // leaving frees every handle: the next month takes the same ones
            let (_, _) = run(&mut t, &mut b, |b, _| leave(b, v.root));
            let (w, _) = run(&mut t, &mut b, |b, f| month_view(b, f, m(2026, 12), Some(today), &k));
            assert_eq!(w.root, v.root);
            assert_eq!((w.title, w.prev, w.next), (v.title, v.prev, v.next));
        }
    }

    #[test]
    fn widget_view_validates() {
        let today = Day { month: m(2026, 12), day: 1 }; // its week starts in November
        for k in [Marks::new(), busy()] {
            let mut t = Table::new();
            let mut b = batch();
            let (v, sizes) = run(&mut t, &mut b, |b, f| {
                theme(b)?;
                widget_view(b, f, today, &k)
            });
            std::println!("widget view: {sizes:?} B");
            run(&mut t, &mut b, |b, _| leave(b, v.root));
            // and the month view after it, as a tap does
            run(&mut t, &mut b, |b, f| month_view(b, f, today.month, Some(today), &k));
        }
    }

    #[test]
    fn summaries() {
        let nov = m(2026, 11);
        let today = Day { month: nov, day: 15 };
        let mut k = Marks::new();
        assert_eq!(month_summary(nov, Some(today), &k).as_str(), "not synced");
        assert_eq!(today_summary(today, &k).as_str(), "not synced");
        let mut days = [0u8; 30];
        days[14] = marks::mark(0b1, 3);
        days[1] = marks::mark(0b10, 1);
        k.set(nov, &days);
        assert_eq!(month_summary(nov, Some(today), &k).as_str(), "today: 3 events");
        assert_eq!(month_summary(nov, None, &k).as_str(), "4 events");
        assert_eq!(today_summary(today, &k).as_str(), "3 events today");
        assert_eq!(today_summary(Day { month: nov, day: 2 }, &k).as_str(), "1 event today");
        assert_eq!(today_summary(Day { month: nov, day: 3 }, &k).as_str(), "no events today");
        assert_eq!(card_value(today, &k).as_str(), "Sun 15 Nov\n3 events today");
        // the card's value limit
        assert!(card_value(Day { month: m(2026, 12), day: 30 }, &busy()).as_str().len() <= 39);
    }
}
