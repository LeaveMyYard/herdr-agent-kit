//! Record timestamps: [`ts`] reads a `timestamp` value as Python does.

use serde_json::Value;

use super::py_strip;
use crate::agent_session::Ts;

/// A record's `timestamp` as epoch seconds: panorama's `wire.ts` (and the
/// inbox's `transcript._ts`).
///
/// - A number is seconds, or milliseconds when it is above `1e11`.
/// - A string is an ISO-8601 date-time (`Z` is UTC), else an integer
///   (`"1760432400000"`, read as a number as above).
/// - `null`, booleans, arrays and objects are no time.
///
/// Python's `datetime.fromisoformat` accepts more forms than Claude Code
/// writes. This reads `YYYY-MM-DD`, optionally followed by `T` or a space
/// and `HH[:MM[:SS[.fraction]]]` (`,` also starts a fraction; digits after the
/// sixth are dropped, as in Python), optionally followed by an offset `Z`,
/// `±HH`, `±HHMM` or `±HH:MM[:SS]`. Week dates, the basic format
/// (`20261014T0900`), other date-time separators and fractional offsets are
/// not read, nor are an empty fraction (`12.Z`) and integer strings in
/// non-ASCII digits. A date-time **without an offset is read as UTC**; Python
/// reads it in the machine's local zone, which a pure function cannot know
/// (Claude Code always writes `Z`). A 13-digit string is milliseconds here;
/// Python reads it as a basic-format date first (year 1376 for
/// `"1376050891932"`).
///
/// A time of `0` is a time here (as in Python's `is not None` checks); the
/// continuation rule treats it as no time where both Pythons do (see
/// [`super::Continuation`](crate::agent_session::claude::Continuation)).
pub fn ts(value: &Value) -> Option<Ts> {
    match value {
        Value::Number(n) => n.as_f64().and_then(number_ts),
        Value::String(s) => iso_ts(s).or_else(|| py_int(s).and_then(number_ts)),
        _ => None,
    }
}

fn number_ts(v: f64) -> Option<Ts> {
    let t = if v > 1e11 { v / 1000.0 } else { v };
    t.is_finite().then_some(Ts(t))
}

/// Python's `int(text)` for ASCII digits, as an `f64` (correctly rounded, like
/// Python's int-to-float conversion): surrounding whitespace, a sign, and
/// single underscores between digits.
fn py_int(text: &str) -> Option<f64> {
    let s = py_strip(text);
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return None;
    }
    let mut digits = String::with_capacity(body.len() + 1);
    if neg {
        digits.push('-');
    }
    for c in body.chars() {
        match c {
            '0'..='9' => digits.push(c),
            '_' => {}
            _ => return None,
        }
    }
    digits.parse::<f64>().ok().filter(|v| v.is_finite())
}

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn digits(&mut self, n: usize) -> Option<i64> {
        let end = self.i.checked_add(n)?;
        let part = self.s.get(self.i..end)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.i = end;
        Some(part.iter().fold(0, |acc, d| acc * 10 + i64::from(d - b'0')))
    }

    fn at_digit(&self) -> bool {
        self.peek().is_some_and(|c| c.is_ascii_digit())
    }

    /// `.digits` or `,digits`: microseconds (the first six digits).
    fn fraction(&mut self) -> Option<i64> {
        if !(self.eat(b'.') || self.eat(b',')) {
            return Some(0);
        }
        let start = self.i;
        while self.at_digit() {
            self.i += 1;
        }
        let digits = &self.s[start..self.i];
        if digits.is_empty() {
            return None;
        }
        let mut us = 0i64;
        for k in 0..6 {
            us = us * 10 + digits.get(k).map_or(0, |d| i64::from(d - b'0'));
        }
        Some(us)
    }

    fn done(&self) -> bool {
        self.i == self.s.len()
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn iso_ts(text: &str) -> Option<Ts> {
    let mut c = Cursor {
        s: text.as_bytes(),
        i: 0,
    };
    let year = c.digits(4)?;
    if !c.eat(b'-') {
        return None;
    }
    let month = c.digits(2)?;
    if !c.eat(b'-') {
        return None;
    }
    let day = c.digits(2)?;
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
    {
        return None;
    }
    let (mut secs, mut micros) = (days_from_civil(year, month, day) * 86_400, 0i64);
    if !c.done() {
        if !(c.eat(b'T') || c.eat(b' ')) {
            return None;
        }
        let hour = c.digits(2)?;
        let (mut minute, mut second) = (0, 0);
        if c.eat(b':') {
            minute = c.digits(2)?;
            if c.eat(b':') {
                second = c.digits(2)?;
                micros = c.fraction()?;
            }
        }
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        secs += hour * 3600 + minute * 60 + second;
        if !c.done() {
            let sign = match c.peek() {
                Some(b'Z') => 0,
                Some(b'+') => -1,
                Some(b'-') => 1,
                _ => return None,
            };
            c.i += 1;
            if sign != 0 {
                let oh = c.digits(2)?;
                let (mut om, mut os) = (0, 0);
                if c.eat(b':') {
                    om = c.digits(2)?;
                    if c.eat(b':') {
                        os = c.digits(2)?;
                    }
                } else if c.at_digit() {
                    om = c.digits(2)?;
                }
                if oh > 23 || om > 59 || os > 59 {
                    return None;
                }
                secs += sign * (oh * 3600 + om * 60 + os);
            }
            if !c.done() {
                return None;
            }
        }
    }
    // datetime.timestamp(): the exact microsecond count, divided once (correctly rounded)
    let total = secs.checked_mul(1_000_000)?.checked_add(micros)?;
    Some(Ts(total as f64 / 1e6))
}

#[cfg(test)]
mod tests;
