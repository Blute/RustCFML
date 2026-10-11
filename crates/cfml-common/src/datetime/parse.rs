//! The CFML date parser: turns the many textual date forms CFML accepts into
//! a date value. Moved here from `cfml-stdlib` so the VM (date-aware `==`/`<`),
//! the stdlib date functions and the database layer share one parser.

use super::{CfmlDate, DateKind};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

/// What a date string names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Parsed {
    /// A wall-clock time, read in the request zone.
    Local(NaiveDateTime),
    /// A time-only ODBC literal (`{t '…'}`) on the 1899-12-30 base.
    Time(NaiveDateTime),
    /// An absolute instant (the string carried an offset), as UTC.
    Instant(NaiveDateTime),
}

impl Parsed {
    /// The date value, in the request zone, at the request's precision.
    pub fn to_date(self) -> CfmlDate {
        match self {
            Parsed::Local(l) => CfmlDate::from_local(&l),
            Parsed::Time(l) => CfmlDate::from_local(&l).with_kind(DateKind::Time),
            Parsed::Instant(u) => CfmlDate::from_utc(&u),
        }
    }

    /// The wall clock this names in the request zone.
    pub fn to_local(self) -> NaiveDateTime {
        match self {
            Parsed::Local(l) | Parsed::Time(l) => l,
            Parsed::Instant(_) => self.to_date().local(),
        }
    }
}

/// Parse a date string to a date value.
pub fn parse_date(s: &str) -> Option<CfmlDate> {
    parse(s).map(Parsed::to_date)
}

/// Parse a date string to a wall clock in the request zone.
pub fn parse_local(s: &str) -> Option<NaiveDateTime> {
    parse(s).map(Parsed::to_local)
}

/// Lucee's base date for times: 1899-12-30, CFML day zero.
fn time_base() -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(1899, 12, 30)
}

/// Parse an ODBC literal: `{d '...'}`, `{t '...'}`, `{ts '...'}`. A `{t}`
/// literal is a time on Lucee's 1899-12-30 base and keeps its kind.
fn parse_odbc_literal(s: &str) -> Option<Parsed> {
    let start = s.find('\'')?;
    let end = s.rfind('\'')?;
    if start >= end { return None; }
    // `{ts '…'}` and nothing else: a literal with trailing text is not a date.
    if s[end + 1..].trim() != "}" {
        return None;
    }
    let inner = &s[start+1..end];
    let lower = s.to_lowercase();
    if lower.starts_with("{ts ") {
        NaiveDateTime::parse_from_str(inner, "%Y-%m-%d %H:%M:%S%.f").ok().map(Parsed::Local)
    } else if lower.starts_with("{d ") {
        NaiveDate::parse_from_str(inner, "%Y-%m-%d").ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(Parsed::Local)
    } else if lower.starts_with("{t ") {
        NaiveTime::parse_from_str(inner, "%H:%M:%S%.f").ok()
            .and_then(|t| time_base().map(|d| d.and_time(t)))
            .map(Parsed::Time)
    } else {
        None
    }
}


/// Expand a bare year the way Lucee's date parser does: a value below 100 is a
/// two-digit year in the fixed window **1930–2029**.
///
/// Probed on Lucee 7.1.0 — `1.1.29` → 2029 but `1.1.30` → 1930, `1.1.00` → 2000,
/// `1.1.99` → 1999. Note this is a FIXED window, not the JDK's default
/// "80 years before now" sliding one (which in 2026 would put the break at 1946
/// and read `30` as 2030). Three-or-more-digit values are literal years, so
/// `1.1.026` is 2026 and `1.1.2020` is 2020.
pub fn expand_two_digit_year(y: i32) -> i32 {
    match y {
        0..=29 => 2000 + y,
        30..=99 => 1900 + y,
        other => other,
    }
}

/// A time of day, read the way Lucee reads one.
///
/// Grammar: `H:MM[:SS[.fff]]` with an optional trailing AM/PM marker — `am`,
/// `pm`, or the bare initials `a`/`p`, in any case, with any amount of space
/// (including none) before it. Returns the time and how many days it carries
/// into the next date.
///
/// The meridiem is **not** a 12-hour hour constraint. Lucee parses whatever
/// hour is written and then folds it, so hours outside 1–12 are accepted and
/// transformed rather than rejected — probed on Lucee 7.1.0.204:
///
/// | input | Lucee | input | Lucee |
/// |---|---|---|---|
/// | `12:30 AM` | 00:30 | `0:30 PM` | 12:30 |
/// | `12:30 PM` | 12:30 | `13:30 AM` | 01:30 |
/// | `0:30 AM` | 00:30 | `13:30 PM` | 13:30 |
/// | `11:59 pm` | 23:59 | `23:30 AM` | 11:30 |
/// | `12:30p` | 12:30 | `23:30 PM` | 23:30 |
///
/// So the hour is `h % 12`, plus 12 for PM. Writing `%I` formats instead (which
/// is what we did) rejects every right-hand column outright, and that is the
/// shape of GH #489: `isValid("time", …)` had its own private format list that
/// accepted `12:30 AM`, while the parser behind `timeFormat()`/`parseDateTime()`
/// had no `%I:%M %p` entry at all — so the same string validated and then
/// failed to format.
///
/// Hour 24 is accepted and rolls into the next day (`24:00` is
/// `{ts '1899-12-31 00:00:00'}` on Lucee, `1/2/2020 24:00` is the 3rd). With a
/// meridiem it folds to 12 rather than to 0, which is why it is called out
/// separately below — `24:00 AM` is noon and `24:00 PM` is the next midnight.
/// Hour 25 is rejected, as are minutes or seconds above 59.
fn parse_time_of_day(s: &str) -> Option<(NaiveTime, i64)> {
    let s = s.trim();

    // The meridiem marker, if any. It must follow a DIGIT so that a stray
    // trailing letter cannot be eaten off a word — otherwise "2 Sep" would
    // offer "2 Se" to the rest of this function.
    let (body, meridiem) = match strip_meridiem(s) {
        Some((rest, pm)) => (rest, Some(pm)),
        None => (s, None),
    };

    let mut parts = body.split(':');
    let hour: i64 = digits(parts.next()?, 2)?;
    let minute: i64 = digits(parts.next()?, 2)?;
    let (second, nanos) = match parts.next() {
        Some(sec) => {
            let (whole, frac) = match sec.split_once('.') {
                Some((w, f)) => (w, Some(f)),
                None => (sec, None),
            };
            let nanos = match frac {
                // Lucee reads ".5" as half a second, so the fraction is a
                // PADDED field, not an integer. Kept to the nanosecond: the
                // request's `datePrecision` decides what is read back out of
                // it (GH #479), and truncating here to milliseconds would make
                // a nanosecond-precision request silently lossy.
                Some(f) if !f.is_empty() && f.bytes().all(|c| c.is_ascii_digit()) => {
                    let mut p = String::from(f);
                    p.truncate(9);
                    while p.len() < 9 { p.push('0'); }
                    p.parse::<i64>().ok()?
                }
                Some(_) => return None,
                None => 0,
            };
            (digits(whole, 2)?, nanos)
        }
        None => (0, 0),
    };
    if parts.next().is_some() { return None; }
    if hour > 24 || minute > 59 || second > 59 { return None; }

    let (hour, days) = match meridiem {
        // `24` folds to 12, not to 0 — so AM is noon and PM is the next
        // midnight. Every other hour folds by `% 12`.
        Some(pm) => {
            let h = if hour == 24 { 12 } else { hour % 12 } + if pm { 12 } else { 0 };
            if h >= 24 { (h - 24, 1) } else { (h, 0) }
        }
        None if hour == 24 => (0, 1),
        None => (hour, 0),
    };

    let t = NaiveTime::from_hms_nano_opt(
        u32::try_from(hour).ok()?,
        u32::try_from(minute).ok()?,
        u32::try_from(second).ok()?,
        u32::try_from(nanos).ok()?,
    )?;
    Some((t, days))
}

/// Split a trailing `am`/`pm`/`a`/`p` marker off a time. The character before
/// it must be a digit, which is what keeps this from biting into a word.
fn strip_meridiem(s: &str) -> Option<(&str, bool)> {
    for (marker, pm) in [("am", false), ("pm", true), ("a", false), ("p", true)] {
        let Some(rest) = s
            .len()
            .checked_sub(marker.len())
            .and_then(|i| Some((s.get(..i)?, s.get(i..)?)))
            .filter(|(_, tail)| tail.eq_ignore_ascii_case(marker))
            .map(|(head, _)| head.trim_end())
        else { continue };
        if rest.ends_with(|c: char| c.is_ascii_digit()) {
            return Some((rest, pm));
        }
    }
    None
}

/// A 1-or-`max`-digit field, digits only.
fn digits(s: &str, max: usize) -> Option<i64> {
    if s.is_empty() || s.len() > max || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Split a string into its date and time halves at the separator before the
/// first `:`. Lucee composes any date form with any time form, so splitting
/// once here is what lets `2020-01-02T12:30 AM`, `Jan 2 2020 12:30 AM` and
/// `1.2.2020 12:30AM` all work without a format-table entry each.
fn split_date_time(s: &str) -> Option<(&str, &str)> {
    let colon = s.find(':')?;
    let (cut, sep) = s[..colon]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() || *c == 'T')?;
    Some((s[..cut].trim_end(), &s[cut + sep.len_utf8()..]))
}

/// Parse Lucee's dot-separated date forms: `A.B.C`. A trailing time is split
/// off before this is reached.
///
/// Which of A/B/C is the day, month and year is decided positionally, exactly as
/// Lucee decides it — verified against Lucee 7.1.0 across 35 probes:
///
/// | input | Lucee | rule |
/// |---|---|---|
/// | `2020.11.5` | 2020-11-05 | A is a full year → Y.M.D |
/// | `31.12.2020` | 2020-12-31 | A can't be a month, C is a full year → D.M.Y |
/// | `13.1.2` | 2013-01-02 | A can't be a month, C isn't a year → Y.M.D |
/// | `5.3.2` | 2002-05-03 | A could be a month → **M.D.Y** |
/// | `10.20.30` | 1930-10-20 | M.D.Y, and `30` is 1930 |
/// | `0.1.2` | rejected | month 0 — the M.D.Y reading is the ONLY one tried |
///
/// The last row is the subtle one and the reason this is not simply "try every
/// ordering": `0.1.2` would parse fine as Y.M.D (2000-01-02), and Lucee still
/// rejects it. A component that *could* be a month commits the string to M.D.Y,
/// pass or fail. Getting that wrong would make us accept dates Lucee refuses,
/// which for GH #411 means accepting a `numeric` argument Lucee rejects.
///
/// This is how `numeric` accepts `"5.3.2"`: Lucee's `numeric` cast falls back to
/// a date cast, and `5.3.2` is 3 May 2002. It is not a version-string rule — the
/// acceptance set tracks date validity exactly, leap years included (`2.29.2004`
/// passes, `2.29.2005` does not).
fn parse_dotted_date(s: &str) -> Option<NaiveDate> {
    let mut parts = s.split('.');
    let (a, b, c) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None; // four or more components is not a date on either engine
    }
    // Digits only: no signs, no exponents, no empty components. This is what
    // keeps `1.2.3.4`, `-1.2.3`, `5..3` and `a.b.c` out.
    if [a, b, c].iter().any(|p| p.is_empty() || !p.bytes().all(|ch| ch.is_ascii_digit())) {
        return None;
    }
    let (a, b, c): (i32, i32, i32) = (a.parse().ok()?, b.parse().ok()?, c.parse().ok()?);

    let (year, month, day) = if a >= 100 {
        (a, b, c)
    } else if a > 12 {
        if c >= 100 { (c, b, a) } else { (a, b, c) }
    } else {
        (c, a, b)
    };

    NaiveDate::from_ymd_opt(
        expand_two_digit_year(year),
        u32::try_from(month).ok()?,
        u32::try_from(day).ok()?,
    )
}

/// A date with no time of day, in any of the forms Lucee accepts.
fn parse_date_only(s: &str) -> Option<NaiveDate> {
    for fmt in &[
        "%Y-%m-%d",
        // Slash-separated ISO order — `2020/1/2`, which Lucee parses (and
        // `isDate()`/`isValid("date",…)` accept) but we used to reject while
        // accepting the dashed form. Unambiguous against `%m/%d/%Y` below: a
        // leading 4-digit year can't be a month, and a leading month can't be
        // a year with a valid day left over.
        "%Y/%m/%d",
        "%m/%d/%Y",
        "%m-%d-%Y",
        "%d %b %Y",
        "%b %d, %Y",
        "%B %d, %Y",
        "%d-%b-%Y",
        // Month-name forms without a comma (Lucee/ACF accept these) — e.g.
        // "January 1 1970", used by cbsecurity's JwtService epoch base.
        "%B %d %Y",
        "%b %d %Y",
        // Month-comma order (GH #365): Lucee's own serializeJSON writes
        // "August, 25 2026", with the comma after the MONTH.
        "%B, %d %Y",
        "%b, %d %Y",
    ] {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    // Dot-separated dates — `5.3.2`, `31.12.2020`, `2020.11.5`. Lucee parses
    // these; we rejected them outright (GH #411).
    parse_dotted_date(s)
}

/// Central date parser: ODBC literals, ISO 8601, common US/EU forms, dotted
/// dates, time-only and a date serial number.
pub fn parse(s: &str) -> Option<Parsed> {
    let s = s.trim();
    if s.is_empty() { return None; }

    // ODBC literals
    if s.starts_with('{') {
        return parse_odbc_literal(s);
    }

    // Offset-bearing forms are read whole, ahead of the date/time split below:
    // their time half carries a trailing "+0000" or "Z" that is not a time.
    //
    // Lucee's serializeJSON date form WITH the trailing UTC offset:
    // `serializeJSON({d: createDateTime(2026,8,25,9,0,14)})` emits
    // {"D":"August, 25 2026 09:00:14 +0000"} on Lucee 7.0.5, and Lucee's date
    // parser reads it straight back. We rejected it, so every date a Lucee
    // deployment had written into a JSON/jsonb column became unreadable after
    // switching engines — isDate() false, dateDiff "Invalid date2" (GH #365).
    // Our own serializeJSON writes "yyyy-mm-dd HH:mm:ss", which both engines
    // parse, so the gap is one-directional and this is the recovering half.
    for fmt in &["%B, %d %Y %H:%M:%S %z", "%b, %d %Y %H:%M:%S %z"] {
        if let Ok(dt) = chrono::DateTime::parse_from_str(s, fmt) {
            // An absolute instant: the request zone applies only when it is read.
            return Some(Parsed::Instant(dt.naive_utc()));
        }
    }

    // RFC 3339 / ISO 8601 with a timezone offset or 'Z' suffix
    // ("2026-06-10T07:20:42.177+00:00", "...Z").
    //
    // The offset is HONOURED and the result expressed in the server's timezone,
    // which is what Lucee does for every offset-bearing form (probed on Lucee
    // 7.1.0.204 under Europe/London: "2026-08-25T09:00:14Z" -> 10:00:14,
    // "...-05:00" -> 15:00:14). We previously returned the wall-clock fields as
    // written, discarding the offset — so a stored UTC timestamp read back on a
    // non-UTC server was wrong by exactly that server's offset, and by six hours
    // for the -05:00 case. Invisible wherever the server runs in UTC (CI, most
    // containers), silently wrong everywhere else. Found alongside GH #365.
    //
    // `parse_cfml_datetime_utc` remains the accessor for the absolute instant.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(Parsed::Instant(dt.naive_utc()));
    }

    // A date and a time. Lucee composes ANY date form with ANY time form, so
    // this splits the string once and parses the halves, rather than carrying a
    // format entry per combination — which is how an AM/PM time came to be
    // readable after `%m/%d/%Y` and nowhere else (GH #489).
    if let Some((date_part, time_part)) = split_date_time(s) {
        if let (Some(date), Some((time, days))) =
            (parse_date_only(date_part), parse_time_of_day(time_part))
        {
            return chrono::Duration::try_days(days)
                .and_then(|d| date.checked_add_signed(d))
                .map(|d| Parsed::Local(d.and_time(time)));
        }
    }

    // Time-only → Lucee's base date 1899-12-30 (`parseDateTime("10:20:30")`
    // is `{ts '1899-12-30 10:20:30'}` on Lucee 7.1).
    if let Some((time, days)) = parse_time_of_day(s) {
        return time_base()
            .and_then(|b| chrono::Duration::try_days(days).and_then(|d| b.checked_add_signed(d)))
            .map(|b| Parsed::Local(b.and_time(time)));
    }

    // A date on its own → midnight.
    if let Some(d) = parse_date_only(s) {
        return d.and_hms_opt(0, 0, 0).map(Parsed::Local);
    }

    // Date serial number (days since 1899-12-30, OLE Automation date)
    if let Ok(n) = s.parse::<f64>() {
        if n.is_finite() {
            let base = NaiveDate::from_ymd_opt(1899, 12, 30)?;
            let days = n.floor() as i64;
            let frac = n - n.floor();
            // Round to the nearest millisecond (NOT truncate) — the fraction is
            // a float, so 51 seconds can land at 50.9999s and truncate to 50.
            // Milliseconds also preserve sub-second precision through a
            // date -> serial -> date round-trip.
            let ms = (frac * 86_400_000.0).round() as i64;
            // try_days/try_milliseconds return None on overflow instead of
            // panicking ("TimeDelta::days out of bounds") — a bare epoch-millis
            // value like 1.7e12 parsed as a date serial would otherwise crash.
            return base.and_hms_opt(0, 0, 0)
                .and_then(|dt| chrono::Duration::try_days(days).and_then(|d| dt.checked_add_signed(d)))
                .and_then(|dt| chrono::Duration::try_milliseconds(ms).and_then(|d| dt.checked_add_signed(d)))
                .map(Parsed::Local);
        }
    }

    None
}

