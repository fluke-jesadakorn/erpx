//! Wall-clock recurrence parity for preventive plans. The synthetic zone models
//! the 2026 America/New_York transitions; the PostgreSQL integration test proves
//! the real IANA calendar adapter.
use erpx_server_maintenance::public::preventive_plan::*;
use erpx_server_shared::public::BusinessError;

const SPRING_GAP: i64 = 1_772_953_200_000; // 2026-03-08T07:00Z (03:00 EDT)
const FALL_BACK: i64 = 1_793_512_800_000; // 2026-11-01T06:00Z (01:00 EST)

struct NewYork;

impl NewYork {
    fn offset(instant: i64) -> i64 {
        if (SPRING_GAP..FALL_BACK).contains(&instant) {
            -4 * 3_600_000
        } else {
            -5 * 3_600_000
        }
    }
}

impl PreventiveCalendar for NewYork {
    fn valid_zone<'a>(
        &'a mut self,
        zone: &'a str,
    ) -> PreventiveCalendarFuture<'a, bool> {
        Box::pin(async move { Ok(zone == "America/New_York" || zone == "UTC") })
    }
    fn utc_offset_ms<'a>(
        &'a mut self,
        zone: &'a str,
        instant: i64,
    ) -> PreventiveCalendarFuture<'a, i64> {
        Box::pin(async move {
            Ok(if zone == "UTC" { 0 } else { Self::offset(instant) })
        })
    }
    fn local_minute<'a>(
        &'a mut self,
        zone: &'a str,
        instant: i64,
    ) -> PreventiveCalendarFuture<'a, String> {
        Box::pin(async move {
            let offset = if zone == "UTC" { 0 } else { Self::offset(instant) };
            let total = instant + offset;
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
        })
    }
}

fn assert_err<T: std::fmt::Debug>(result: Result<T, BusinessError>, code: &str) {
    assert_eq!(result.unwrap_err().code, code);
}

#[tokio::test]
async fn wall_clock_truncates_seconds_and_uses_the_selected_zone() {
    let mut zone = NewYork;
    assert_eq!(
        wall_clock(&mut zone, 1_772_868_659_000, "America/New_York")
            .await
            .unwrap(),
        "2026-03-07T02:30"
    );
    assert_eq!(
        wall_clock(&mut zone, 1_793_757_600_000, "UTC")
            .await
            .unwrap(),
        "2026-11-04T02:00"
    );
}

#[tokio::test]
async fn spring_gap_keeps_the_wall_clock_and_resolves_to_the_first_valid_minute() {
    let mut zone = NewYork;
    assert!(
        local_candidates(&mut zone, "2026-03-08T02:30", "America/New_York")
            .await
            .unwrap()
            .is_empty()
    );
    let next = next_occurrence(&mut zone, "2026-03-07T02:30", "America/New_York", 1)
        .await
        .unwrap();
    assert_eq!(next.local, "2026-03-08T02:30");
    assert_eq!(next.resolved_local, "2026-03-08T03:00");
    assert_eq!(next.at, SPRING_GAP);
    assert_eq!(
        wall_clock(&mut zone, next.at, "America/New_York")
            .await
            .unwrap(),
        "2026-03-08T03:00"
    );
}

#[tokio::test]
async fn fall_back_fold_selects_the_earlier_instant_and_keeps_the_minute() {
    let mut zone = NewYork;
    assert_eq!(
        local_candidates(&mut zone, "2026-11-01T01:30", "America/New_York")
            .await
            .unwrap(),
        vec![1_793_511_000_000, 1_793_514_600_000]
    );
    let next = next_occurrence(&mut zone, "2026-10-31T01:30", "America/New_York", 1)
        .await
        .unwrap();
    assert_eq!(next.local, "2026-11-01T01:30");
    assert_eq!(next.resolved_local, "2026-11-01T01:30");
    assert_eq!(next.at, 1_793_511_000_000);
}

#[tokio::test]
async fn save_time_resolution_truncates_the_submitted_instant() {
    let mut zone = NewYork;
    let (local, at) = resolve_save_time(&mut zone, 1_772_868_659_000, "America/New_York")
        .await
        .unwrap();
    assert_eq!(local, "2026-03-07T02:30");
    assert_eq!(at, 1_772_868_600_000);
    assert_err(
        resolve_save_time(&mut zone, 1, "Not/AZone").await,
        "MAINTENANCE_TIME_UNAVAILABLE",
    );
}

#[tokio::test]
async fn calendar_days_are_stable_across_transitions() {
    assert_eq!(add_days("2026-11-01T01:30", 1).unwrap(), "2026-11-02T01:30");
    assert_eq!(add_days("2026-02-28T09:00", 1).unwrap(), "2026-03-01T09:00");
    assert_eq!(add_days("2024-02-28T09:00", 1).unwrap(), "2024-02-29T09:00");
    assert_eq!(add_minutes("2026-03-08T02:30", 30).unwrap(), "2026-03-08T03:00");
    assert!(parse_local("2026-02-30T09:00").is_none());
}
