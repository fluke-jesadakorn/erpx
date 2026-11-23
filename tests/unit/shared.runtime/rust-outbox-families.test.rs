//! Canonical native outbox family partition. Locks the reconciled inventory:
//! which families the native supervisor owns, which stay with the TS runOne
//! loop, and that the claim predicate is static and fail-closed. No database.
#![allow(dead_code)]

use erpx_server_runtime::public::postgres::{
    FamilyClass, NativeFamily, classify, owned_predicate,
};

const PMS: [&str; 3] = ["pms.reservation.create", "pms.stay.check_in", "pms.task.complete"];

#[test]
fn every_native_ready_family_is_classified_native() {
    let cases = [
        ("task.created", NativeFamily::Task),
        ("task.assigned", NativeFamily::Task),
        ("routine.run", NativeFamily::Routine),
        ("leave.requested", NativeFamily::Leave),
        ("leave.approved", NativeFamily::Leave),
        ("overtime.logged", NativeFamily::Overtime),
        ("people.payroll.scheduled", NativeFamily::Payroll),
        ("pms.reservation.create", NativeFamily::PmsSocial),
        ("pms.stay.check_in", NativeFamily::PmsSocial),
        ("pms.task.complete", NativeFamily::PmsSocial),
    ];
    for (event_type, family) in cases {
        assert_eq!(
            classify(event_type, &PMS),
            FamilyClass::Native(family),
            "{event_type} must stay in the native partition"
        );
    }
}

#[test]
fn projector_exclusions_and_retained_pulses_stay_with_ts_runone() {
    // Exact exclusions mirrored from the composition owned predicate.
    for event_type in ["leave.policy.changed", "overtime.eligibility"] {
        assert_eq!(
            classify(event_type, &PMS),
            FamilyClass::RetainedTs,
            "{event_type} is explicitly retained by the TS projector"
        );
    }
    // The remaining runOne families from inventory.runtime are not native-ready.
    for event_type in [
        "support.sla.due",
        "legal.reminder.due",
        "pms.booking.payment",
        "pos.stripe.pending",
        "attachment.scan.due",
        "billing.cycle.due",
        "directory.provisioning",
        "ai.expense.prepare",
        "customer.notification.due",
        "finance.due_draft",
        "inventory.count.observed",
        "field.order.created",
    ] {
        assert_eq!(
            classify(event_type, &PMS),
            FamilyClass::RetainedTs,
            "{event_type} must not be claimed by the native supervisor"
        );
    }
}

#[test]
fn classification_is_fail_closed_on_empty_or_unreviewed_input() {
    assert!(FamilyClass::Native(NativeFamily::Task).is_native());
    assert!(FamilyClass::Native(NativeFamily::Leave).is_native());
    assert!(!FamilyClass::RetainedTs.is_native());
    for event_type in ["", " ", "pms.reservation.unknown", &"x".repeat(257)] {
        assert_eq!(classify(event_type, &PMS), FamilyClass::RetainedTs);
    }
}

#[test]
fn predicate_is_the_exact_static_native_family_clause() {
    let predicate = owned_predicate(&PMS).unwrap();
    for fragment in [
        "e.event_type LIKE 'task.%'",
        "e.event_type = 'routine.run'",
        "(e.event_type LIKE 'leave.%' AND e.event_type NOT LIKE 'leave.policy.%')",
        "(e.event_type LIKE 'overtime.%' AND e.event_type <> 'overtime.eligibility')",
        "e.event_type LIKE 'people.payroll.%'",
        "e.event_type IN ('pms.reservation.create','pms.stay.check_in','pms.task.complete')",
    ] {
        assert!(predicate.contains(fragment), "missing predicate fragment: {fragment}");
    }
    assert!(predicate.starts_with('(') && predicate.ends_with(')'));
    assert!(!predicate.contains("support."));
    assert!(!predicate.contains("legal."));
    assert!(!predicate.contains("billing."));

    // No PMS labels still yields a valid prefix-only predicate.
    let without_pms = owned_predicate(&[]).unwrap();
    assert!(!without_pms.contains("e.event_type IN"));
    assert!(without_pms.contains("e.event_type LIKE 'people.payroll.%'"));
}

#[test]
fn predicate_rejects_request_shaped_label_input() {
    for label in ["", "pms.x' OR 1=1 --", &"x".repeat(257), "pms bad"] {
        assert_eq!(
            owned_predicate(&[label]).unwrap_err().code,
            "OUTBOX_FAMILY_PREDICATE_INVALID",
            "unsafe label must fail closed"
        );
    }
}
