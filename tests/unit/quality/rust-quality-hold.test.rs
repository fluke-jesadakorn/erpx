//! Native Quality manual hold-create admission.
use erpx_server_quality::public::quality_commands::{
    QualityHoldCommand, normalize_quality_hold,
};
use erpx_server_shared::public::{CommandKey, RecordId};

fn command() -> QualityHoldCommand {
    QualityHoldCommand {
        key: CommandKey::new("quality-hold-key").unwrap(),
        warehouse_id: RecordId::new("warehouse").unwrap(),
        product_id: RecordId::new("product").unwrap(),
        lot_id: None,
        expected_stock_version: 4,
        expected_lot_version: None,
        due_date: "2026-11-10".into(),
        reason: "Damaged carton requires independent review".into(),
    }
}

#[test]
fn manual_hold_admits_the_retained_source_input_and_identity() {
    let normalized = normalize_quality_hold(command()).unwrap();
    assert_eq!(normalized.operation(), "quality.hold.create");
    assert_eq!(
        normalized.record_id().unwrap().as_str(),
        "quality-quality-hold-key"
    );
    assert_eq!(
        normalized.reason,
        "Damaged carton requires independent review"
    );
}

#[test]
fn manual_hold_normalizes_surrounding_whitespace_in_the_reason() {
    let mut input = command();
    input.reason = "  Damaged carton requires review  ".into();
    assert_eq!(
        normalize_quality_hold(input).unwrap().reason,
        "Damaged carton requires review"
    );
}

#[test]
fn manual_hold_requires_positive_expected_versions() {
    let mut input = command();
    input.expected_stock_version = 0;
    assert_eq!(
        normalize_quality_hold(input).unwrap_err().code,
        "VERSION_REQUIRED"
    );

    let mut input = command();
    input.lot_id = Some(RecordId::new("lot").unwrap());
    assert_eq!(
        normalize_quality_hold(input).unwrap_err().code,
        "VERSION_REQUIRED"
    );

    let mut input = command();
    input.expected_lot_version = Some(2);
    assert_eq!(
        normalize_quality_hold(input).unwrap_err().code,
        "QUALITY_LOT_REQUIRED"
    );
}

#[test]
fn manual_hold_requires_a_calendar_due_date_and_bounded_reason() {
    let mut input = command();
    input.due_date = "2026-02-30".into();
    assert_eq!(
        normalize_quality_hold(input).unwrap_err().code,
        "QUALITY_DUE_DATE_REQUIRED"
    );

    let mut input = command();
    input.reason = "  x  ".into();
    assert_eq!(
        normalize_quality_hold(input).unwrap_err().code,
        "QUALITY_HOLD_REASON"
    );
}

#[test]
fn manual_hold_accepts_lot_and_expected_lot_version() {
    let mut input = command();
    input.lot_id = Some(RecordId::new("lot").unwrap());
    input.expected_lot_version = Some(7);
    let normalized = normalize_quality_hold(input).unwrap();
    assert_eq!(normalized.lot_id.unwrap().as_str(), "lot");
    assert_eq!(normalized.expected_lot_version, Some(7));
}
