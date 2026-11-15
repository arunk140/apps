//! What the watch knows about events: one byte per day, never a title. Bits 0..=2: which of three calendars have
//! an event that day (a dot each); bits 3..=7: how many events (0..=31). Kept for up to [`MONTHS`] months in the
//! app's storage blob, and filled by host messages (`c1M`, [`parse`]).
//!
//! Protocol (`docs/app-guide.md` section 4 rules): every message starts with `c1` and a kind byte.
//! - app → host `c1R` year(u16 LE) month n: "send me n months from this one" (on start, on RESUME, on a gap).
//! - host → app `c1M` seq year(u16 LE) month days…: one byte per day of that month, exactly as many as it has.

use crate::date::{Day, Month};

/// Calendars with their own dot colour.
pub const CALENDARS: usize = 3;
pub const MONTHS: usize = 13;
const ENTRY: usize = 3 + 31;
const MAGIC: [u8; 2] = *b"C1";
/// Bytes of the storage blob at most.
pub const BLOB: usize = MAGIC.len() + 1 + MONTHS * ENTRY;

pub fn calendars(mark: u8) -> u8 {
    mark & 0b111
}

pub fn count(mark: u8) -> u8 {
    mark >> 3
}

/// A day byte, as the host builds it.
#[allow(dead_code)]
pub fn mark(calendars: u8, count: u8) -> u8 {
    (calendars & 0b111) | (count.min(31) << 3)
}

#[derive(Clone, Copy)]
pub struct Marks {
    months: [Month; MONTHS],
    days: [[u8; 31]; MONTHS],
    n: usize,
}

impl Default for Marks {
    fn default() -> Self {
        Self::new()
    }
}

impl Marks {
    pub const fn new() -> Marks {
        Marks { months: [Month { year: 1970, month: 1 }; MONTHS], days: [[0; 31]; MONTHS], n: 0 }
    }

    fn find(&self, m: Month) -> Option<usize> {
        self.months[..self.n].iter().position(|&x| x == m)
    }

    /// Whether the host ever sent `m`.
    pub fn synced(&self, m: Month) -> bool {
        self.find(m).is_some()
    }

    /// The day's mark; `0` for an unknown month.
    pub fn get(&self, d: Day) -> u8 {
        self.find(d.month).map_or(0, |i| self.days[i][d.day as usize - 1])
    }

    /// Replaces `m` (days past `days.len()` are 0). Full: drops the month farthest from `m`.
    pub fn set(&mut self, m: Month, days: &[u8]) {
        let i = match self.find(m) {
            Some(i) => i,
            None if self.n < MONTHS => {
                self.n += 1;
                self.n - 1
            }
            None => {
                let key = |x: Month| (x.year as i32 * 12 + x.month as i32 - (m.year as i32 * 12 + m.month as i32)).abs();
                (0..MONTHS).max_by_key(|&i| key(self.months[i])).unwrap_or(0)
            }
        };
        self.months[i] = m;
        self.days[i] = [0; 31];
        let n = days.len().min(m.days() as usize);
        self.days[i][..n].copy_from_slice(&days[..n]);
    }

    /// The storage blob into `out`; returns its length.
    pub fn encode(&self, out: &mut [u8; BLOB]) -> usize {
        out[..2].copy_from_slice(&MAGIC);
        out[2] = self.n as u8;
        let mut at = 3;
        for i in 0..self.n {
            let m = self.months[i];
            out[at..at + 2].copy_from_slice(&m.year.to_le_bytes());
            out[at + 2] = m.month;
            out[at + 3..at + ENTRY].copy_from_slice(&self.days[i]);
            at += ENTRY;
        }
        at
    }

    /// A blob from [`Marks::encode`]; anything else (an old version, a cut or foreign blob) is empty.
    pub fn decode(blob: &[u8]) -> Marks {
        let mut out = Marks::new();
        if blob.len() < 3 || blob[..2] != MAGIC {
            return out;
        }
        let n = blob[2] as usize;
        if n > MONTHS || blob.len() != 3 + n * ENTRY {
            return out;
        }
        for e in blob[3..].chunks_exact(ENTRY) {
            let Some(m) = Month::new(u16::from_le_bytes([e[0], e[1]]), e[2]) else { return Marks::new() };
            if out.synced(m) {
                return Marks::new();
            }
            out.set(m, &e[3..3 + m.days() as usize]);
        }
        out
    }
}

/// `c1R`: ask for `n` months from `from`.
pub fn request(from: Month, n: u8) -> [u8; 7] {
    let y = from.year.to_le_bytes();
    [b'c', b'1', b'R', y[0], y[1], from.month, n]
}

/// A host message the app acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum Msg<'a> {
    Month { seq: u8, month: Month, days: &'a [u8] },
}

/// Parses a host message; `None` for anything malformed or unknown (it's ignored).
pub fn parse(m: &[u8]) -> Option<Msg<'_>> {
    match m {
        [b'c', b'1', b'M', seq, ylo, yhi, month, days @ ..] => {
            let month = Month::new(u16::from_le_bytes([*ylo, *yhi]), *month)?;
            (days.len() == month.days() as usize).then_some(Msg::Month { seq: *seq, month, days })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    fn m(y: u16, mo: u8) -> Month {
        Month::new(y, mo).unwrap()
    }

    fn day(month: Month, day: u8) -> Day {
        Day { month, day }
    }

    #[test]
    fn mark_bits() {
        let k = mark(0b101, 3);
        assert_eq!((calendars(k), count(k)), (0b101, 3));
        assert_eq!(count(mark(0b1, 200)), 31);
        assert_eq!(calendars(mark(0xFF, 0)), 0b111);
    }

    #[test]
    fn set_get_and_unknown_months() {
        let mut k = Marks::new();
        let nov = m(2026, 11);
        assert!(!k.synced(nov));
        assert_eq!(k.get(day(nov, 4)), 0);
        let mut days = [0u8; 30];
        days[3] = mark(0b11, 2);
        k.set(nov, &days);
        assert!(k.synced(nov));
        assert_eq!(k.get(day(nov, 4)), mark(0b11, 2));
        assert_eq!(k.get(day(nov, 5)), 0);
        // replacing keeps one entry
        k.set(nov, &[mark(1, 1); 30]);
        assert_eq!(k.get(day(nov, 4)), mark(1, 1));
        assert_eq!(k.n, 1);
    }

    #[test]
    fn full_drops_the_farthest_month() {
        let mut k = Marks::new();
        let start = m(2026, 1);
        for i in 0..MONTHS as i32 {
            k.set(start.add(i), &[1; 31]);
        }
        // a month after the last: January (12+ away) goes, February stays
        let next = start.add(MONTHS as i32);
        k.set(next, &[2; 31]);
        assert!(!k.synced(start));
        assert!(k.synced(start.add(1)) && k.synced(next));
        assert_eq!(k.get(day(next, 1)), 2);
    }

    #[test]
    fn blob_round_trip() {
        let mut k = Marks::new();
        k.set(m(2026, 11), &[mark(1, 1); 30]);
        k.set(m(2026, 12), &[mark(2, 4); 31]);
        k.set(m(2027, 2), &[mark(4, 9); 28]);
        let mut blob = [0u8; BLOB];
        let n = k.encode(&mut blob);
        assert_eq!(n, 3 + 3 * ENTRY);
        let back = Marks::decode(&blob[..n]);
        for (mo, d, want) in [(m(2026, 11), 30, mark(1, 1)), (m(2026, 12), 31, mark(2, 4)), (m(2027, 2), 28, mark(4, 9))] {
            assert!(back.synced(mo));
            assert_eq!(back.get(day(mo, d)), want);
        }
        assert!(BLOB <= cmf_wp2_app::STORE_MAX);
    }

    #[test]
    fn bad_blobs_load_empty() {
        let mut k = Marks::new();
        k.set(m(2026, 11), &[1; 30]);
        let mut blob = [0u8; BLOB];
        let n = k.encode(&mut blob);
        let good = blob[..n].to_vec();
        let empty = |b: &[u8]| !Marks::decode(b).synced(m(2026, 11));
        assert!(!empty(&good));
        assert!(empty(&[]));
        assert!(empty(&good[..n - 1]));
        let mut bad = good.clone();
        bad[0] = b'X';
        assert!(empty(&bad));
        let mut bad = good.clone();
        bad[5] = 13; // month
        assert!(empty(&bad));
        let mut bad = good.clone();
        bad[2] = 2; // count without the entry
        assert!(empty(&bad));
        let mut twice: Vec<u8> = good.clone();
        twice.extend_from_slice(&good[3..]);
        twice[2] = 2;
        assert!(empty(&twice));
    }

    #[test]
    fn parse_month_messages() {
        let mut msg = std::vec![b'c', b'1', b'M', 7, 0xEA, 0x07, 11];
        msg.extend_from_slice(&[mark(1, 2); 30]);
        assert_eq!(
            parse(&msg),
            Some(Msg::Month { seq: 7, month: m(2026, 11), days: &[mark(1, 2); 30] })
        );
        // a day too few or too many, a bad month, another magic, a cut header
        assert_eq!(parse(&msg[..msg.len() - 1]), None);
        let mut long = msg.clone();
        long.push(0);
        assert_eq!(parse(&long), None);
        let mut bad = msg.clone();
        bad[6] = 0;
        assert_eq!(parse(&bad), None);
        let mut bad = msg.clone();
        bad[1] = b'2';
        assert_eq!(parse(&bad), None);
        assert_eq!(parse(b"c1M"), None);
        assert_eq!(parse(b""), None);
        // fits a host message
        assert!(7 + 31 <= cmf_wp2_app::MSG_MAX);
    }

    #[test]
    fn request_bytes() {
        assert_eq!(request(m(2026, 11), 3), [b'c', b'1', b'R', 0xEA, 0x07, 11, 3]);
    }
}
