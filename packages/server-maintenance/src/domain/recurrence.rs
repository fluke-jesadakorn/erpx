//! Portable wall-clock arithmetic for preventive maintenance plans.
//!
//! The maintained plan stores a local `YYYY-MM-DDTHH:MM` minute. These helpers
//! only implement deterministic calendar arithmetic; resolving a local minute to
//! an instant (including IANA DST rules) is the calendar port's responsibility,
//! so no process-locale or invented timezone table is used here.
use erpx_server_shared::public::BusinessError;

/// Longest forward adjustment the retained recurrence searched for a DST gap.
pub const MAX_GAP_MINUTES: i64 = 180;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NextOccurrence {
    /// The plan keeps its original wall-clock minute, even inside a DST gap.
    pub local: String,
    /// Earlier instant represented by the local minute (fold) or the first
    /// valid minute after a gap.
    pub at: i64,
    /// The local minute actually used for `at`.
    pub resolved_local: String,
}

pub fn unavailable() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_TIME_UNAVAILABLE",
        "Choose a plan time with an available occurrence in this time zone.",
    )
}

/// Parse the maintained `YYYY-MM-DDTHH:MM` wall-clock value.
pub fn parse_local(value: &str) -> Option<(i64, i64, i64, i64, i64)> {
    let bytes = value.as_bytes();
    if bytes.len() != 16
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7 | 10 | 13) || byte.is_ascii_digit())
    {
        return None;
    }
    let year = value[0..4].parse::<i64>().ok()?;
    let month = value[5..7].parse::<i64>().ok()?;
    let day = value[8..10].parse::<i64>().ok()?;
    let hour = value[11..13].parse::<i64>().ok()?;
    let minute = value[14..16].parse::<i64>().ok()?;
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
    {
        return None;
    }
    Some((year, month, day, hour, minute))
}

fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a Gregorian date (Howard Hinnant's `days_from_civil`).
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = shifted + if shifted < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

/// Unix milliseconds interpreting the local value as if it were UTC.
pub fn naive_millis(local: &str) -> Option<i64> {
    let (year, month, day, hour, minute) = parse_local(local)?;
    Some(
        days_from_civil(year, month, day) * 86_400_000
            + (hour * 60 + minute) * 60_000,
    )
}

pub fn format_local(year: i64, month: i64, day: i64, hour: i64, minute: i64) -> String {
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}")
}

/// Add whole calendar days to a wall-clock value without a timezone transition
/// changing the result (the retained helper anchored on UTC noon).
pub fn add_days(local: &str, days: i64) -> Result<String, BusinessError> {
    let (year, month, day, hour, minute) = parse_local(local).ok_or_else(unavailable)?;
    let (year, month, day) = civil_from_days(days_from_civil(year, month, day) + days);
    Ok(format_local(year, month, day, hour, minute))
}

/// Add whole minutes to a wall-clock value. The retained recurrence uses this to
/// walk out of a spring-forward gap minute by minute.
pub fn add_minutes(local: &str, minutes: i64) -> Result<String, BusinessError> {
    let (year, month, day, hour, minute) = parse_local(local).ok_or_else(unavailable)?;
    let total = days_from_civil(year, month, day) * 86_400_000
        + (hour * 60 + minute) * 60_000
        + minutes * 60_000;
    let days = total.div_euclid(86_400_000);
    let remainder = total.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    Ok(format_local(
        year,
        month,
        day,
        remainder / 3_600_000,
        (remainder % 3_600_000) / 60_000,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_days_are_stable_across_transitions() {
        assert_eq!(add_days("2026-11-01T01:30", 1).unwrap(), "2026-11-02T01:30");
        assert_eq!(add_days("2026-02-28T09:00", 1).unwrap(), "2026-03-01T09:00");
        assert_eq!(add_days("2024-02-28T09:00", 1).unwrap(), "2024-02-29T09:00");
        assert_eq!(add_days("2026-12-31T23:59", 1).unwrap(), "2027-01-01T23:59");
        assert_eq!(add_minutes("2026-03-08T02:30", 30).unwrap(), "2026-03-08T03:00");
        assert_eq!(add_minutes("2026-03-08T23:59", 1).unwrap(), "2026-03-09T00:00");
    }

    #[test]
    fn local_values_round_trip_through_the_epoch_day_arithmetic() {
        for value in ["1970-01-01T00:00", "2026-11-04T09:00", "2126-02-28T23:59"] {
            let millis = naive_millis(value).unwrap();
            let days = millis.div_euclid(86_400_000);
            let remainder = millis.rem_euclid(86_400_000);
            let (year, month, day) = civil_from_days(days);
            assert_eq!(
                format_local(
                    year,
                    month,
                    day,
                    remainder / 3_600_000,
                    (remainder % 3_600_000) / 60_000
                ),
                value
            );
        }
        assert!(parse_local("2026-02-30T09:00").is_none());
        assert!(parse_local("2026-1-01T09:00").is_none());
    }
}
