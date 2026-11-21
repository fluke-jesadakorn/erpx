//! Unit coverage for the generic Finance Money snapshot producer: retained
//! identity/amount validation, ISO and Company scales, exact USD conversion and
//! the effective-date/fallback rate ordering.
use erpx_server_finance::public::money_snapshot::{
    MoneyRateCandidate, MoneyRateSelector, MoneySnapshotInput, money_snapshot_scale,
    money_snapshot_usd, require_money_snapshot_scale, validate_money_snapshot_input,
};

fn input() -> MoneySnapshotInput {
    MoneySnapshotInput {
        source_type: "ecommerce_listing".into(),
        source_id: "listing-1".into(),
        amount_kind: "unit_price".into(),
        transaction_date: "2026-07-15".into(),
        currency: "THB".into(),
        currency_scale: Some(2),
        amount_units: 12_345,
    }
}

fn candidate(id: &str, date: &str, quote: &str, confirmed_at: i64) -> MoneyRateCandidate {
    MoneyRateCandidate {
        id: id.into(),
        effective_date: date.into(),
        currency_per_usd: quote.into(),
        confirmed_at,
    }
}

#[test]
fn scale_uses_iso_then_company_rules_and_rejects_unknown_currencies() {
    assert_eq!(money_snapshot_scale("THB", None).unwrap(), 2);
    assert_eq!(money_snapshot_scale("JPY", None).unwrap(), 0);
    assert_eq!(money_snapshot_scale("XAG", Some(4)).unwrap(), 4);
    // XAG has no ISO minor unit, so a Company scale is mandatory.
    assert_eq!(
        money_snapshot_scale("XAG", None).unwrap_err().code,
        "CURRENCY_SCALE_REQUIRED"
    );
    assert_eq!(
        money_snapshot_scale("XAG", Some(13)).unwrap_err().code,
        "CURRENCY_SCALE_REQUIRED"
    );
    assert_eq!(
        money_snapshot_scale("ZZZ", Some(2)).unwrap_err().code,
        "CURRENCY_SCALE_REQUIRED"
    );
}

#[test]
fn requested_scale_must_match_the_configured_scale() {
    assert!(require_money_snapshot_scale(None, 2).is_ok());
    assert!(require_money_snapshot_scale(Some(2), 2).is_ok());
    assert_eq!(
        require_money_snapshot_scale(Some(3), 2).unwrap_err().code,
        "CURRENCY_SCALE_MISMATCH"
    );
    assert_eq!(
        require_money_snapshot_scale(Some(0), 2).unwrap_err().code,
        "CURRENCY_SCALE_MISMATCH"
    );
}

#[test]
fn identity_dates_currency_and_amounts_are_bounded_like_the_retained_schema() {
    assert!(validate_money_snapshot_input(&input()).is_ok());
    let mut value = input();
    value.source_type = " vendor ".into();
    assert!(validate_money_snapshot_input(&value).is_ok(), "Zod trims before length");
    value.source_type = "   ".into();
    assert_eq!(
        validate_money_snapshot_input(&value).unwrap_err().code,
        "MONEY_SNAPSHOT_INVALID"
    );
    let mut value = input();
    value.source_type = "x".repeat(100);
    value.source_id = "y".repeat(200);
    value.amount_kind = "z".repeat(100);
    assert!(validate_money_snapshot_input(&value).is_ok());
    value.source_type = "x".repeat(101);
    assert!(validate_money_snapshot_input(&value).is_err());
    value = input();
    value.source_id = "y".repeat(201);
    assert!(validate_money_snapshot_input(&value).is_err());
    value = input();
    value.amount_kind = "z".repeat(101);
    assert!(validate_money_snapshot_input(&value).is_err());
    value = input();
    value.transaction_date = "2026-02-30".into();
    assert!(validate_money_snapshot_input(&value).is_err());
    value = input();
    value.currency = "thb".into();
    assert!(validate_money_snapshot_input(&value).is_err());
    value = input();
    value.currency = "TH".into();
    assert!(validate_money_snapshot_input(&value).is_err());
    value = input();
    value.amount_units = 9_007_199_254_740_991;
    assert!(validate_money_snapshot_input(&value).is_ok());
    value.amount_units = -9_007_199_254_740_991;
    assert!(validate_money_snapshot_input(&value).is_ok());
    value.amount_units = 9_007_199_254_740_992;
    assert!(validate_money_snapshot_input(&value).is_err());
    value.amount_units = -9_007_199_254_740_992;
    assert!(validate_money_snapshot_input(&value).is_err());
}

#[test]
fn usd_conversion_rounds_half_away_from_zero_and_rejects_invalid_rates() {
    assert_eq!(money_snapshot_usd(101, 2, "2").unwrap(), "51");
    assert_eq!(money_snapshot_usd(-101, 2, "2").unwrap(), "-51");
    assert_eq!(money_snapshot_usd(0, 2, "2").unwrap(), "0");
    assert_eq!(money_snapshot_usd(1, 2, "3").unwrap(), "0");
    assert_eq!(money_snapshot_usd(2, 2, "3").unwrap(), "1");
    assert_eq!(
        money_snapshot_usd(1, 2, "0.000000000000000001").unwrap(),
        "1000000000000000000"
    );
    assert_eq!(
        money_snapshot_usd(1, 2, "0").unwrap_err().code,
        "MONEY_OVERFLOW"
    );
    assert_eq!(
        money_snapshot_usd(1, 2, "0.0000000000000000001")
            .unwrap_err()
            .code,
        "MONEY_OVERFLOW"
    );
}

#[test]
fn rate_selection_prefers_effective_date_then_confirmation_then_collated_id() {
    let mut selector = MoneyRateSelector::new().unwrap();
    selector.observe(candidate("b", "2026-05-01", "7", 10), "2026-05-15");
    selector.observe(candidate("a", "2026-05-01", "11", 20), "2026-05-15");
    selector.observe(candidate("z", "2026-04-01", "99", 99), "2026-05-15");
    selector.observe(candidate("future", "2026-06-01", "1", 1), "2026-05-15");
    let selected = selector.select("THB", "2026-05-15").unwrap();
    assert_eq!(selected.id, "a");
    assert_eq!(selected.currency_per_usd, "11");
    assert!(!selected.used_latest_fallback);
}

#[test]
fn rate_selection_is_stable_for_canonically_equal_ids() {
    // ICU English compares canonically equivalent IDs equally; the SQL stream
    // order still decides, exactly like the legacy stable JavaScript sort.
    let mut selector = MoneyRateSelector::new().unwrap();
    selector.observe(candidate("é", "2026-05-01", "7", 10), "2026-05-15");
    selector.observe(candidate("e\u{301}", "2026-05-01", "11", 10), "2026-05-15");
    assert_eq!(
        selector.select("THB", "2026-05-15").unwrap().currency_per_usd,
        "7"
    );
}

#[test]
fn future_dates_fall_back_to_the_latest_confirmed_quote() {
    let mut selector = MoneyRateSelector::new().unwrap();
    selector.observe(candidate("a", "2026-05-01", "7", 10), "2025-01-01");
    selector.observe(candidate("z", "2026-04-01", "9", 20), "2025-01-01");
    let selected = selector.select("THB", "2025-01-01").unwrap();
    assert_eq!(selected.id, "z");
    assert_eq!(selected.currency_per_usd, "9");
    assert!(selected.used_latest_fallback);
}

#[test]
fn usd_selection_needs_no_rate_rows() {
    let selected = MoneyRateSelector::new()
        .unwrap()
        .select("USD", "2026-07-15")
        .unwrap();
    assert_eq!(selected.id, "USD");
    assert_eq!(selected.currency_per_usd, "1");
    assert_eq!(selected.effective_date, "2026-07-15");
    assert!(!selected.used_latest_fallback);
}
