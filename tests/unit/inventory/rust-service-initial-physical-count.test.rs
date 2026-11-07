use erpx_server_inventory::public::service_initial_physical_count::*;

fn actor() -> CountAuthority {
    CountAuthority {
        same_as_creator: true,
        same_as_preparer: true,
        company_scope: false,
        department_scope: false,
        can_prepare: true,
        can_approve: false,
    }
}
fn reviewer() -> CountAuthority {
    CountAuthority {
        same_as_creator: false,
        same_as_preparer: false,
        company_scope: true,
        department_scope: false,
        can_prepare: false,
        can_approve: true,
    }
}
fn lot(code: &str, quantity_milli: i64, unit_cost_minor: i64) -> InitialCountLot {
    InitialCountLot {
        lot_code: Some(code.into()),
        serial_number: None,
        manufactured_date: None,
        expiry_date: None,
        quantity_milli,
        unit_cost_minor,
    }
}

#[test]
fn actual_first_count_uses_integer_inventory_valuation() {
    let value = value_initial_count(TrackingMode::None, 1500, Some(601), vec![]).unwrap();
    assert_eq!(value.value_minor, 902);
    assert_eq!(
        value_initial_count(TrackingMode::None, 1, Some(1), vec![])
            .unwrap_err()
            .code,
        "ZERO_VALUATION"
    );
    assert_eq!(
        value_initial_count(TrackingMode::None, 1000, None, vec![])
            .unwrap_err()
            .code,
        "VALUATION_REQUIRED"
    );
    assert_eq!(
        value_initial_count(TrackingMode::None, 0, None, vec![])
            .unwrap()
            .value_minor,
        0
    );
}
#[test]
fn tracked_lots_require_unique_found_identity_positive_cost_and_matching_total() {
    let value = value_initial_count(
        TrackingMode::Lot,
        2000,
        None,
        vec![lot(" A ", 1500, 100), lot("b", 500, 200)],
    )
    .unwrap();
    assert_eq!(value.value_minor, 250);
    assert_eq!(value.lots[0].0.lot_code.as_deref(), Some("A"));
    assert_eq!(
        value_initial_count(
            TrackingMode::Lot,
            2000,
            None,
            vec![lot("a", 1000, 100), lot(" A ", 1000, 100)]
        )
        .unwrap_err()
        .code,
        "DUPLICATE_TRACKING_IDENTITY"
    );
    assert_eq!(
        value_initial_count(TrackingMode::Lot, 1000, None, vec![lot("a", 500, 100)])
            .unwrap_err()
            .code,
        "COUNT_LOT_TOTAL_MISMATCH"
    );
    assert_eq!(
        value_initial_count(TrackingMode::Lot, 1000, None, vec![lot("a", 1000, 0)])
            .unwrap_err()
            .code,
        "VALUATION_REQUIRED"
    );
    let empty = value_initial_count(TrackingMode::Lot, 0, None, vec![]).unwrap();
    assert_eq!(
        require_submittable_count(TrackingMode::Lot, &empty)
            .unwrap_err()
            .code,
        "COUNT_LOT_TOTAL_MISMATCH"
    );
    let mut serial = lot("ignored", 500, 100);
    serial.serial_number = Some("serial".into());
    assert_eq!(
        value_initial_count(TrackingMode::Serial, 500, None, vec![serial])
            .unwrap_err()
            .code,
        "SERIAL_QUANTITY_INVALID"
    );
}
#[test]
fn versions_scope_and_independent_reviewer_gate_transitions() {
    assert_eq!(
        transition_count(
            CountStatus::Draft,
            1,
            1,
            CountAction::Submit,
            &actor(),
            None,
            0
        )
        .unwrap(),
        CountStatus::Submitted
    );
    assert_eq!(
        transition_count(
            CountStatus::Submitted,
            2,
            2,
            CountAction::Approve,
            &reviewer(),
            Some("verified"),
            0
        )
        .unwrap(),
        CountStatus::Approved
    );
    assert_eq!(
        transition_count(
            CountStatus::Approved,
            3,
            3,
            CountAction::Post,
            &reviewer(),
            Some("verified"),
            0
        )
        .unwrap(),
        CountStatus::Posted
    );
    assert_eq!(
        transition_count(
            CountStatus::Submitted,
            2,
            1,
            CountAction::Approve,
            &reviewer(),
            Some("verified"),
            0
        )
        .unwrap_err()
        .code,
        "COUNT_VERSION_CHANGED"
    );
    let mut same = reviewer();
    same.same_as_creator = true;
    assert_eq!(
        transition_count(
            CountStatus::Submitted,
            2,
            2,
            CountAction::Approve,
            &same,
            Some("verified"),
            0
        )
        .unwrap_err()
        .code,
        "COUNT_ACTION_FORBIDDEN"
    );
    let mut branch = reviewer();
    branch.company_scope = false;
    assert_eq!(
        transition_count(
            CountStatus::Submitted,
            2,
            2,
            CountAction::Approve,
            &branch,
            Some("verified"),
            0
        )
        .unwrap_err()
        .code,
        "COUNT_ACTION_FORBIDDEN"
    );
    assert_eq!(
        transition_count(
            CountStatus::Draft,
            1,
            1,
            CountAction::Cancel,
            &actor(),
            Some("cancelled"),
            1
        )
        .unwrap_err()
        .code,
        "ATTACHMENT_ACTION_INVALID"
    );
    assert_eq!(
        transition_count(
            CountStatus::Approved,
            3,
            3,
            CountAction::Reject,
            &reviewer(),
            Some("returned"),
            0
        )
        .unwrap(),
        CountStatus::Rejected
    );
}

#[test]
fn javascript_trim_normalizes_feff_and_preserves_nel_for_tracking_and_reason() {
    assert_eq!(
        value_initial_count(
            TrackingMode::Lot,
            2000,
            None,
            vec![lot("A", 1000, 100), lot("\u{feff}A", 1000, 100)]
        )
        .unwrap_err()
        .code,
        "DUPLICATE_TRACKING_IDENTITY"
    );
    let value = value_initial_count(
        TrackingMode::Lot,
        1000,
        None,
        vec![lot("\u{0085}A\u{0085}", 1000, 100)],
    )
    .unwrap();
    assert_eq!(
        value.lots[0].0.lot_code.as_deref(),
        Some("\u{0085}A\u{0085}")
    );
    assert_eq!(
        transition_count(
            CountStatus::Draft,
            1,
            1,
            CountAction::Cancel,
            &actor(),
            Some("\u{feff}ab\u{feff}"),
            0
        )
        .unwrap_err()
        .code,
        "REASON_REQUIRED"
    );
    assert!(
        transition_count(
            CountStatus::Draft,
            1,
            1,
            CountAction::Cancel,
            &actor(),
            Some("\u{0085}ab"),
            0
        )
        .is_ok()
    );
}
