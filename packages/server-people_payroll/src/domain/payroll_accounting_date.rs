//! Qualified ISO date profile. Other V8 grammars remain an explicit P2 gap.
use super::payroll_accounting_operand::{Fault, R, Text};
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}
pub(crate) fn parse(t: &Text) -> R<Option<i64>> {
    let s = t.utf8()?;
    if s.is_empty() || s.chars().all(|c| c.is_ascii_alphabetic()) {
        return Ok(None);
    }
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(Fault::gap("P2", "Date.parse", s));
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let a = b.get(range)?;
        if !a.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(a).ok()?.parse().ok()
    };
    let (Some(y), Some(m), Some(d)) = (number(0..4), number(5..7), number(8..10)) else {
        return Err(Fault::gap("P2", "Date.parse", s));
    };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if d < 1 || d > max {
        return Err(Fault::gap("P2", "Date.parse", s));
    }
    let mut value = days(y, m, d) * 86400000;
    if b.len() == 10 {
        return Ok(Some(value));
    }
    if b.len() < 20 || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return Err(Fault::gap("P2", "Date.parse", s));
    }
    let (Some(hour), Some(min), Some(sec)) = (number(11..13), number(14..16), number(17..19))
    else {
        return Err(Fault::gap("P2", "Date.parse", s));
    };
    if hour > 23 || min > 59 || sec > 59 {
        return Err(Fault::gap("P2", "Date.parse", s));
    }
    value += (hour * 3600 + min * 60 + sec) * 1000;
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1
        }
        if i == start {
            return Err(Fault::gap("P2", "Date.parse", s));
        }
        let mut frac = s[start..i].chars().take(3).collect::<String>();
        while frac.len() < 3 {
            frac.push('0')
        }
        value += frac.parse::<i64>().unwrap_or(0)
    }
    match b.get(i) {
        Some(b'Z') if i + 1 == b.len() => {}
        Some(sign @ (b'+' | b'-')) if i + 6 == b.len() && b[i + 3] == b':' => {
            let (Some(h), Some(m)) = (number(i + 1..i + 3), number(i + 4..i + 6)) else {
                return Err(Fault::gap("P2", "Date.parse", s));
            };
            if h > 23 || m > 59 {
                return Err(Fault::gap("P2", "Date.parse", s));
            }
            let offset = (h * 60 + m) * 60000;
            value += if *sign == b'+' { -offset } else { offset }
        }
        _ => return Err(Fault::gap("P2", "Date.parse", s)),
    }
    if value.abs() > 8640000000000000 {
        return Ok(None);
    }
    Ok(Some(value))
}
pub(crate) fn iso_day(millis: i64) -> Text {
    let (y, m, d) = civil(millis.div_euclid(86400000));
    let s = if (0..=9999).contains(&y) {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!(
            "{}{abs:06}-{m:02}-{d:02}",
            if y < 0 { '-' } else { '+' },
            abs = y.abs()
        )
    };
    Text::ascii(&s.chars().take(10).collect::<String>())
}
