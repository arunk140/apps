//! calendar: a month grid with a dot per calendar on the days that have events, and today in a widget.
//!
//! - From the Apps menu: the month view. The crown or `<` `>` change the month, a tap on the title goes back to
//!   today's month, BUTTON exits.
//! - As the page next to the watch face (`app_features!(PAGE)`): the widget view (today, this week, today's count).
//!   A tap opens the month view, a tap on its title comes back.
//! - The card left of the watch face (`app_features!(WIDGET)`): "Sun 15 Nov" and today's count.
//!
//! Events come from a host as one byte per day (calendars, count; never a title): `marks.rs`. Without one the
//! calendar works and says "not synced".
#![cfg_attr(target_os = "none", no_std, no_main)]

#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod date;
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod marks;
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod screen;

#[cfg(not(target_os = "none"))]
fn main() {}

#[cfg(target_os = "none")]
mod device {
    use crate::date::{Day, Month};
    use crate::marks::{self, Marks, Msg, BLOB};
    use crate::screen::{self, MonthView, WidgetView};
    use cmf_wp2_app::lv::{Batch, Lv, LvError};
    use cmf_wp2_app::{Api, BUTTON, CROWN, LV, MESSAGE, MSG_MAX, PAUSE, RESUME};

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();
    cmf_wp2_app::app_features!(cmf_wp2_app::WIDGET | cmf_wp2_app::PAGE);

    const CLICKED: i16 = 7;
    /// How often to look at the clock (a new day) and, while unsynced, to ask the host again.
    const TICK_MS: u32 = 30_000;
    const SYNC_MS: u32 = 10_000;
    /// Months asked for: the one before today's and two after.
    const SYNC_MONTHS: u8 = 4;
    const FALLBACK: Month = Month { year: 2026, month: 1 };

    // read by an emulator e2e: last submit result, view (1 month, 2 widget), shown month (yyyymm), host months taken
    const RC: usize = 0;
    const VIEW: usize = 1;
    const SHOWN: usize = 2;
    const SYNCS: usize = 3;
    // non-zero: in .data, fresh at every launch; RC 1 until the first screen is up
    #[unsafe(no_mangle)]
    static mut STATS: [i32; 4] = [1, 0, 0, 0];
    static mut STORE: [u8; BLOB] = [0; BLOB];

    fn set(i: usize, v: i32) {
        unsafe { (&raw mut STATS[i]).write_volatile(v) }
    }
    fn get(i: usize) -> i32 {
        unsafe { (&raw const STATS[i]).read_volatile() }
    }
    fn code(r: Result<(), LvError>) -> i32 {
        match r {
            Ok(()) => 0,
            Err(LvError::Code(c)) => c,
            Err(_) => -1000,
        }
    }

    fn today(api: &Api) -> Option<Day> {
        let t = api.time()?;
        let month = Month::new(t.year, t.month)?;
        (1..=month.days()).contains(&t.day).then_some(Day { month, day: t.day })
    }

    #[derive(Clone, Copy)]
    enum View {
        Month(MonthView),
        Widget(WidgetView),
    }

    impl View {
        fn root(&self) -> cmf_wp2_app::lv::Obj {
            match self {
                View::Month(v) => v.root,
                View::Widget(v) => v.root,
            }
        }
    }

    enum Want {
        Month(Month),
        Widget(Day),
    }

    struct App {
        api: &'static Api,
        lv: Lv,
        b: Batch,
        marks: Marks,
        view: View,
        today: Option<Day>,
    }

    impl App {
        /// Replaces the screen with `want`; on an error the old screen stays.
        fn show(&mut self, want: Want) {
            let lv = self.lv;
            let mut flush = |b: &mut Batch| b.submit(&lv);
            let old = self.view.root();
            let r = screen::leave(&mut self.b, old).and_then(|_| match want {
                Want::Month(m) => {
                    screen::month_view(&mut self.b, &mut flush, m, self.today, &self.marks).map(View::Month)
                }
                Want::Widget(d) => screen::widget_view(&mut self.b, &mut flush, d, &self.marks).map(View::Widget),
            });
            let r = match r {
                Ok(v) => {
                    self.view = v;
                    self.b.submit(&lv)
                }
                Err(e) => Err(e),
            };
            set(RC, code(r));
            match self.view {
                View::Month(v) => {
                    set(VIEW, 1);
                    set(SHOWN, v.shown.year as i32 * 100 + v.shown.month as i32);
                }
                View::Widget(_) => set(VIEW, 2),
            }
        }

        /// The same screen again, for a new day or new marks.
        fn refresh(&mut self) {
            let want = match self.view {
                View::Month(v) => Want::Month(v.shown),
                View::Widget(_) => match self.today {
                    Some(d) => Want::Widget(d),
                    None => Want::Month(FALLBACK),
                },
            };
            self.show(want);
        }

        fn card(&self) {
            if let Some(d) = self.today {
                self.api.widget("Calendar", screen::card_value(d, &self.marks).as_str());
            }
        }

        fn ask(&self) -> bool {
            let from = self.today.map_or(FALLBACK, |d| d.month).add(-1);
            self.api.send(&marks::request(from, SYNC_MONTHS)) == 0
        }

        /// A new day since the last look: the screens and the card follow.
        fn tick(&mut self) {
            let now = today(self.api);
            if now == self.today {
                return;
            }
            let was = self.today;
            self.today = now;
            if let (View::Month(v), Some(old), Some(new)) = (self.view, was, now) {
                if v.shown == old.month {
                    self.show(Want::Month(new.month));
                    self.card();
                    return;
                }
            }
            self.refresh();
            self.card();
        }

        fn message(&mut self, m: &[u8]) {
            if let Some(Msg::Month { month, days, .. }) = marks::parse(m) {
                self.marks.set(month, days);
                let blob = unsafe { &mut *core::ptr::addr_of_mut!(STORE) };
                let n = self.marks.encode(blob);
                self.api.save(&blob[..n]);
                set(SYNCS, get(SYNCS) + 1);
                let visible = match self.view {
                    View::Month(v) => v.shown == month || v.shown.add(-1) == month || v.shown.add(1) == month,
                    View::Widget(w) => w.today.month == month || crate::date::week(w.today).iter().any(|d| d.month == month),
                };
                if visible {
                    self.refresh();
                }
                if self.today.is_some_and(|d| d.month == month) {
                    self.card();
                }
            }
        }
    }

    fn main(api: &'static Api) -> i32 {
        let Some(lv) = api.lv() else { return -95 }; // loader older than ABI 1.2
        let Some(mut b) = Batch::new() else { return -1 };
        if screen::fonts(&mut b).and_then(|_| b.submit(&lv)).is_err() {
            return 1;
        }
        if screen::theme(&mut b).and_then(|_| b.submit(&lv)).is_err() {
            return 2;
        }
        let blob = unsafe { &mut *core::ptr::addr_of_mut!(STORE) };
        let n = api.load(blob);
        let marks = if n > 0 { Marks::decode(&blob[..n as usize]) } else { Marks::new() };
        let today = today(api);
        let placeholder = View::Month(MonthView {
            root: cmf_wp2_app::lv::ROOT,
            title: cmf_wp2_app::lv::ROOT,
            prev: cmf_wp2_app::lv::ROOT,
            next: cmf_wp2_app::lv::ROOT,
            shown: FALLBACK,
        });
        let mut app = App { api, lv, b, marks, view: placeholder, today };
        let on_page = api.on_page();
        app.show(match (on_page, today) {
            (true, Some(d)) => Want::Widget(d),
            (_, d) => Want::Month(d.map_or(FALLBACK, |d| d.month)),
        });
        app.card();
        app.ask(); // asked, not answered: keep asking until a month arrives
        let mut synced = false;
        let mut next_ask = api.now().wrapping_add(SYNC_MS);
        let mut paused = false;
        let mut msg = [0u8; MSG_MAX];
        loop {
            let e = api.event(if paused { u32::MAX } else if synced { TICK_MS } else { SYNC_MS });
            if !paused {
                app.tick();
                if !synced && (api.now().wrapping_sub(next_ask) as i32) >= 0 {
                    app.ask();
                    next_ask = api.now().wrapping_add(SYNC_MS);
                }
            }
            let Some(e) = e else { continue };
            match e.kind {
                BUTTON => break,
                PAUSE => paused = true,
                RESUME => {
                    paused = false;
                    synced = false;
                    app.tick();
                }
                MESSAGE => {
                    while let Some(m) = api.recv(&mut msg) {
                        let before = get(SYNCS);
                        app.message(m);
                        synced |= get(SYNCS) != before;
                    }
                }
                CROWN if !paused => {
                    if let View::Month(v) = app.view {
                        app.show(Want::Month(v.shown.add(if e.delta > 0 { 1 } else { -1 })));
                    }
                }
                LV if e.y == CLICKED && !paused => {
                    let h = e.x as u16;
                    match app.view {
                        View::Month(v) if h == v.prev.0 => app.show(Want::Month(v.shown.add(-1))),
                        View::Month(v) if h == v.next.0 => app.show(Want::Month(v.shown.add(1))),
                        View::Month(v) if h == v.title.0 => app.show(match (on_page, app.today) {
                            (true, Some(d)) => Want::Widget(d),
                            (_, d) => Want::Month(d.map_or(v.shown, |d| d.month)),
                        }),
                        View::Widget(w) if h == w.root.0 => app.show(Want::Month(w.today.month)),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        0
    }
}
