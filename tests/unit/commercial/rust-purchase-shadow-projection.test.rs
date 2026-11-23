//! Native parity oracle for the retired TypeScript
//! `purchaseObservationProjection` SQL builder. The TS projection assembled a
//! private grant/history row for shadow owners; native now owns the whole
//! checked-phase projection and emits only this non-personal field set.
use erpx_server_commercial::public::purchase_command_boundary::{
    PURCHASE_AFTER_STEP, PURCHASE_BEFORE_STEP, PURCHASE_SHADOW_OBSERVATION_FIELDS,
};

#[test]
fn native_shadow_projection_is_the_authoritative_telemetry_contract() {
    assert_eq!(PURCHASE_SHADOW_OBSERVATION_FIELDS.len(), 7);
    for field in [
        "event",
        "phase",
        "companyId",
        "capability",
        "purpose",
        "code",
        "purchaseRevision",
    ] {
        assert!(
            PURCHASE_SHADOW_OBSERVATION_FIELDS.contains(&field),
            "missing native telemetry field {field}"
        );
    }
}

#[test]
fn retired_private_observation_fields_never_reappear() {
    // The TS raw row carried grants/history/allowed/observed_at. The native
    // projection reduces it to the fixed contract above before it leaves the
    // owner; a fallback must not reintroduce those private facts.
    for private in [
        "grants",
        "history",
        "allowed",
        "observedAt",
        "newWork",
        "settleExisting",
        "revision",
    ] {
        assert!(
            !PURCHASE_SHADOW_OBSERVATION_FIELDS.contains(&private),
            "private fact leaked into native telemetry: {private}"
        );
    }
}

#[test]
fn retained_assertion_steps_are_unchanged_by_the_retirement() {
    assert_eq!(PURCHASE_BEFORE_STEP, "commercial-purchase-before");
    assert_eq!(PURCHASE_AFTER_STEP, "commercial-purchase-after");
}
