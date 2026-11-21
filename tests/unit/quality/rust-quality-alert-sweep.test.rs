//! Native Quality alert sweep planning facts. The PostgreSQL sweep itself is
//! proved by rust-quality-alert-sweep-postgres; these cases pin the retained
//! identity, phase and UTC business-date planning without a database.
use erpx_server_quality::public::quality_alerts::{
    QUALITY_ALERT_WARNING_WINDOW_MS, QualityAlertPhase, quality_alert_sweep_plan,
};

/// 2026-09-21T12:00:00Z.
const NOW: i64 = 1_789_992_000_000;
/// 2026-09-21T23:59:59.999Z.
const LAST_MILLISECOND: i64 = 1_790_035_199_999;
/// 2026-09-22T00:00:00Z.
const NEXT_UTC_DAY: i64 = 1_790_035_200_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[test]
fn escalation_identities_and_phases_match_the_retained_alert_keys() {
    assert_eq!(QualityAlertPhase::DueSoon.as_str(), "due_soon");
    assert_eq!(QualityAlertPhase::Overdue.as_str(), "overdue");
    assert_eq!(
        QualityAlertPhase::DueSoon.alert_id("hold-a"),
        "quality-alert-due-soon-hold-a"
    );
    assert_eq!(
        QualityAlertPhase::Overdue.alert_id("hold-a"),
        "quality-alert-overdue-hold-a"
    );
}

#[test]
fn the_plan_keeps_the_utc_business_date_and_the_inclusive_day_window() {
    let plan = quality_alert_sweep_plan(NOW, QUALITY_ALERT_WARNING_WINDOW_MS).unwrap();
    assert_eq!(plan.today(), "2026-09-21");
    assert_eq!(plan.due_soon_horizon(), "2026-09-22");
    assert_eq!(
        plan.phases(),
        [QualityAlertPhase::DueSoon, QualityAlertPhase::Overdue]
    );
}

#[test]
fn the_plan_rolls_over_at_utc_midnight_and_accepts_a_zero_window() {
    let last = quality_alert_sweep_plan(LAST_MILLISECOND, QUALITY_ALERT_WARNING_WINDOW_MS).unwrap();
    assert_eq!(last.today(), "2026-09-21");
    assert_eq!(last.due_soon_horizon(), "2026-09-22");

    let next = quality_alert_sweep_plan(NEXT_UTC_DAY, QUALITY_ALERT_WARNING_WINDOW_MS).unwrap();
    assert_eq!(next.today(), "2026-09-22");
    assert_eq!(next.due_soon_horizon(), "2026-09-23");

    let same_day = quality_alert_sweep_plan(NOW, 0).unwrap();
    assert_eq!(same_day.today(), "2026-09-21");
    assert_eq!(same_day.due_soon_horizon(), "2026-09-21");
}

#[test]
fn an_unusable_clock_is_refused_before_any_write() {
    for (now, window) in [
        (-1, QUALITY_ALERT_WARNING_WINDOW_MS),
        (MAX_SAFE_INTEGER + 1, QUALITY_ALERT_WARNING_WINDOW_MS),
        (NOW, -1),
        (MAX_SAFE_INTEGER, QUALITY_ALERT_WARNING_WINDOW_MS),
        (i64::MAX, 0),
    ] {
        let error = quality_alert_sweep_plan(now, window).unwrap_err();
        assert_eq!(error.code, "QUALITY_ALERT_CLOCK_INVALID", "{now}/{window}");
    }
}
