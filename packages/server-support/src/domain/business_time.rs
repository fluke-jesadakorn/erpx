//! Portable Support business-time calendar. The predicate and minute-window
//! arithmetic mirror the owner contract: a minute is working when its local
//! weekday is selected, its local date is not a holiday and its local time is
//! inside the same-day opening window. Timezone rules come from the bundled
//! IANA database, never from the process locale.
use erpx_server_shared::public::BusinessError;
use jiff::{Timestamp, civil::Weekday, tz::TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use time::macros::format_description;

pub const MAX_HORIZON_MS: i64 = 366 * 24 * 60 * 60 * 1_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessCalendar {
    pub timezone: String,
    pub weekdays: Vec<u8>,
    pub opens_at_minute: u32,
    pub closes_at_minute: u32,
    #[serde(default)]
    pub holidays: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct WorkingCalendar {
    timezone: TimeZone,
    weekdays: [bool; 7],
    opens_at_minute: u32,
    closes_at_minute: u32,
    holidays: BTreeSet<String>,
}

fn invalid(message: &'static str) -> BusinessError {
    BusinessError::invalid("SUPPORT_CALENDAR_INVALID", message)
}

pub fn capacity() -> BusinessError {
    BusinessError::invalid(
        "SUPPORT_CALENDAR_CAPACITY",
        "This calendar cannot fit the target within 366 days. Review its working days, hours and holidays.",
    )
}

fn timestamp(value: i64) -> Result<i64, BusinessError> {
    if value < 0 || Timestamp::from_millisecond(value).is_err() {
        return Err(invalid("Use a valid business clock timestamp."));
    }
    Ok(value)
}

impl BusinessCalendar {
    pub fn parse(self) -> Result<WorkingCalendar, BusinessError> {
        let timezone = TimeZone::get(&self.timezone)
            .map_err(|_| invalid("Choose a valid IANA timezone."))?;
        if self.weekdays.is_empty()
            || self.weekdays.len() > 7
            || self.weekdays.iter().any(|day| *day > 6)
            || self.weekdays.iter().copied().collect::<BTreeSet<_>>().len() != self.weekdays.len()
        {
            return Err(invalid("Choose each weekday once."));
        }
        if self.opens_at_minute > 1_439
            || self.closes_at_minute < 1
            || self.closes_at_minute > 1_440
            || self.closes_at_minute <= self.opens_at_minute
        {
            return Err(invalid(
                "Closing time must follow opening time within the same day.",
            ));
        }
        if self.holidays.len() > 366 {
            return Err(invalid("Record at most 366 holidays."));
        }
        let format = format_description!("[year]-[month]-[day]");
        let mut holidays = BTreeSet::new();
        for value in &self.holidays {
            time::Date::parse(value, &format)
                .map_err(|_| invalid("Use ISO dates for holidays."))?;
            holidays.insert(value.clone());
        }
        let mut weekdays = [false; 7];
        for day in self.weekdays {
            weekdays[usize::from(day)] = true;
        }
        Ok(WorkingCalendar {
            timezone,
            weekdays,
            opens_at_minute: self.opens_at_minute,
            closes_at_minute: self.closes_at_minute,
            holidays,
        })
    }
}

impl WorkingCalendar {
    fn working(&self, at: i64) -> bool {
        let Ok(timestamp) = Timestamp::from_millisecond(at) else {
            return false;
        };
        let zoned = timestamp.to_zoned(self.timezone.clone());
        let day = match zoned.weekday() {
            Weekday::Sunday => 0,
            Weekday::Monday => 1,
            Weekday::Tuesday => 2,
            Weekday::Wednesday => 3,
            Weekday::Thursday => 4,
            Weekday::Friday => 5,
            Weekday::Saturday => 6,
        };
        if !self.weekdays[day as usize] {
            return false;
        }
        let date = format!(
            "{:04}-{:02}-{:02}",
            zoned.year(),
            zoned.month(),
            zoned.day()
        );
        if self.holidays.contains(&date) {
            return false;
        }
        let minute = u32::from(zoned.hour() as u8) * 60 + u32::from(zoned.minute() as u8);
        minute >= self.opens_at_minute && minute < self.closes_at_minute
    }
}

/// Exact minute-window addition. A partial first minute contributes only the
/// remainder of that minute, matching the owner predicate.
pub fn add_business_time(
    calendar: &WorkingCalendar,
    start: i64,
    duration_ms: i64,
) -> Result<i64, BusinessError> {
    let start = timestamp(start)?;
    if !(0..=MAX_HORIZON_MS).contains(&duration_ms) {
        return Err(invalid("Use a valid business clock duration."));
    }
    if duration_ms == 0 {
        return Ok(start);
    }
    let end = start.saturating_add(MAX_HORIZON_MS);
    let mut cursor = start;
    let mut remaining = duration_ms;
    while cursor < end {
        let next = (cursor / 60_000) * 60_000 + 60_000;
        let next = next.min(end);
        if calendar.working(cursor) {
            let available = next - cursor;
            if remaining <= available {
                return Ok(cursor + remaining);
            }
            remaining -= available;
        }
        cursor = next;
    }
    Err(capacity())
}

/// Exact elapsed working milliseconds between two instants.
pub fn elapsed_business_time(
    calendar: &WorkingCalendar,
    start: i64,
    end: i64,
) -> Result<i64, BusinessError> {
    let start = timestamp(start)?;
    let end = timestamp(end)?;
    if end < start || end.saturating_sub(start) > MAX_HORIZON_MS {
        return Err(invalid(
            "Business clock interval must be between zero and 366 days.",
        ));
    }
    let mut cursor = start;
    let mut total = 0_i64;
    while cursor < end {
        let next = ((cursor / 60_000) * 60_000 + 60_000).min(end);
        if calendar.working(cursor) {
            total += next - cursor;
        }
        cursor = next;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calendar() -> WorkingCalendar {
        BusinessCalendar {
            timezone: "Asia/Bangkok".into(),
            weekdays: vec![1, 2, 3, 4, 5],
            opens_at_minute: 9 * 60,
            closes_at_minute: 17 * 60,
            holidays: vec!["2026-11-05".into()],
        }
        .parse()
        .unwrap()
    }

    #[test]
    fn adds_within_a_working_day_and_skips_the_night() {
        // 2026-11-04 is a Wednesday; 09:00 Bangkok is 02:00 UTC.
        let start = 1_793_757_600_000; // 2026-11-04T02:00:00Z
        let due = add_business_time(&calendar(), start, 8 * 60 * 60 * 1_000).unwrap();
        // Eight working hours end at 17:00 Bangkok (10:00 UTC).
        assert_eq!(due, start + 8 * 60 * 60 * 1_000);
        let next = add_business_time(&calendar(), start, 8 * 60 * 60 * 1_000 + 60 * 60 * 1_000)
            .unwrap();
        // The next working hour starts at 09:00 Bangkok on 2026-11-06 because
        // 2026-11-05 is a holiday.
        assert_eq!(next, 1_793_934_000_000);
    }

    #[test]
    fn elapsed_counts_only_working_minutes() {
        let start = 1_793_757_600_000;
        assert_eq!(
            elapsed_business_time(&calendar(), start, start + 2 * 60 * 60 * 1_000).unwrap(),
            2 * 60 * 60 * 1_000
        );
        assert_eq!(
            elapsed_business_time(&calendar(), start, start + 9 * 60 * 60 * 1_000).unwrap(),
            8 * 60 * 60 * 1_000
        );
    }

    #[test]
    fn rejects_invalid_calendars_and_horizons() {
        let bad = BusinessCalendar {
            timezone: "Not/AZone".into(),
            weekdays: vec![1],
            opens_at_minute: 0,
            closes_at_minute: 60,
            holidays: vec![],
        };
        assert_eq!(bad.parse().unwrap_err().code, "SUPPORT_CALENDAR_INVALID");
        let start = 1_793_757_600_000;
        assert_eq!(
            add_business_time(&calendar(), start, MAX_HORIZON_MS + 1)
                .unwrap_err()
                .code,
            "SUPPORT_CALENDAR_INVALID"
        );
    }
}
