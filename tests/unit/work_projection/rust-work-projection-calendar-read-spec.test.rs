//! Frozen native Work Projection calendar read spec (external oracle).
//!
//! Pins the exact allow-list and positional arities of the twelve SELECT facts
//! that the TypeScript feature bridge in
//! `packages/feature-work_projection/src/lib/native-calendar-read.ts` sends to
//! the private `/work/owner-read` route. Any drift in the Rust read surface
//! breaks this test before it can break a live calendar command. No database
//! and no network are required.

use erpx_server_shared::public::BusinessError;
use erpx_server_work_projection::public::calendar_read::{
    READ_12, READ_26, READ_37, READ_60, READ_61, READ_62, READ_63, READ_64, READ_65, READ_66,
    READ_67, arity, supported, validate,
};
use erpx_server_work_projection::public::calendar_read_http::{
    CalendarReadFuture, CalendarReadHttpBackend, CalendarReadHttpRequest, CalendarReadHttpTarget,
    calendar_read_router,
};
use erpx_server_work_projection::public::calendar_read_tail::{
    CalendarReadFamily, OccurrenceException, OccurrenceExceptionChange, OccurrenceResponse,
    OccurrenceResponseValue, ReminderAuthorization, family_of,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// The twelve native read facts and their exact positional arities.
const SPEC: &[(&str, usize)] = &[
    ("wp_cal_read_01", 10),
    ("wp_cal_read_03", 1),
    ("wp_cal_read_04", 2),
    ("wp_cal_read_12", 2),
    ("wp_cal_read_13", 2),
    ("wp_cal_read_23", 2),
    ("wp_cal_read_26", 2),
    ("wp_cal_read_30", 2),
    ("wp_cal_read_37", 2),
    ("wp_cal_read_38", 3),
    ("wp_cal_read_42", 2),
    ("wp_cal_read_53", 2),
];

#[test]
fn native_read_table_is_exact_and_arity_fenced() {
    assert_eq!(SPEC.len(), 12);
    for (kind, size) in SPEC {
        assert!(supported(kind), "{kind} must be allow-listed");
        assert_eq!(arity(kind), Some(*size));
        assert!(validate(kind, *size).is_ok(), "{kind}/{size} must validate");
        assert!(validate(kind, size + 1).is_err(), "{kind} arity fence");
        assert!(validate(kind, size.saturating_sub(1)).is_err(), "{kind} arity fence");
    }
    assert!(!supported("wp_cal_read_02"));
    assert!(!supported("wp_cal_unknown"));
    assert!(validate("wp_cal_unknown", 1).is_err());
    assert_eq!(arity("wp_cal_unknown"), None);
}

struct Noop;
impl CalendarReadHttpBackend for Noop {
    fn read<'a>(&'a self, _: CalendarReadHttpRequest<'a>) -> CalendarReadFuture<'a> {
        Box::pin(async {
            Err::<Value, _>(BusinessError::invalid("NOOP", "noop"))
        })
    }
}

#[test]
fn http_ingress_is_mount_ready_for_the_registered_holding() {
    let target = CalendarReadHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: Arc::new(Noop) as Arc<dyn CalendarReadHttpBackend>,
    };
    assert!(calendar_read_router(vec![target]).is_ok());
    assert!(calendar_read_router(vec![]).is_err());
}

// The future alias must stay object-safe for composition's Arc<dyn ...>.
#[test]
fn backend_future_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<CalendarReadFuture<'static>>();
}

/// The eight owner-local reads that extend the original twelve: occurrence
/// exception/response (60-64) and reminder authority (65-67).
const TAIL_SPEC: &[(&str, usize)] = &[
    (READ_60, 3),
    (READ_61, 4),
    (READ_62, 2),
    (READ_63, 2),
    (READ_64, 2),
    (READ_65, 2),
    (READ_66, 3),
    (READ_67, 3),
];

#[test]
fn read_tail_table_is_exact_and_arity_fenced() {
    assert_eq!(TAIL_SPEC.len(), 8);
    for (kind, size) in TAIL_SPEC {
        assert!(supported(kind), "{kind} must be allow-listed for HTTP ingress");
        assert_eq!(arity(kind), Some(*size));
        assert!(validate(kind, *size).is_ok(), "{kind}/{size} must validate");
        assert!(validate(kind, size + 1).is_err(), "{kind} arity fence");
        assert!(validate(kind, size.saturating_sub(1)).is_err(), "{kind} arity fence");
    }
    assert!(supported("wp_cal_read_68"));
    assert!(!supported("wp_cal_read_59"));
}

#[test]
fn read_tail_covers_appointment_occurrence_and_reminder_authority() {
    for kind in [READ_12, READ_26, READ_37] {
        assert_eq!(family_of(kind), Some(CalendarReadFamily::AppointmentRow));
    }
    for kind in [READ_60, READ_61, READ_62, READ_63, READ_64] {
        assert_eq!(family_of(kind), Some(CalendarReadFamily::Occurrence));
    }
    for kind in [READ_65, READ_66, READ_67] {
        assert_eq!(family_of(kind), Some(CalendarReadFamily::ReminderAuthority));
    }
    assert_eq!(family_of("wp_cal_read_01"), None);
    assert_eq!(family_of("wp_cal_unknown"), None);
}

#[test]
fn read_tail_domain_records_decode_authority_rows() {
    let change: OccurrenceExceptionChange = serde_json::from_value(json!({
        "start_local": "2026-01-08T09:00", "end_local": null, "cancelled": false,
    }))
    .unwrap();
    assert_eq!(change.cancelled, false);
    assert_eq!(change.start_local.as_deref(), Some("2026-01-08T09:00"));

    let response: OccurrenceResponseValue =
        serde_json::from_value(json!({"response": "accepted"})).unwrap();
    assert_eq!(response.response, "accepted");

    let exception: OccurrenceException = serde_json::from_value(json!({
        "appointment_id": "appt-1", "occurrence_key": "k1",
        "start_local": null, "end_local": null, "cancelled": true,
    }))
    .unwrap();
    assert_eq!(exception.occurrence_key, "k1");

    let response_row: OccurrenceResponse = serde_json::from_value(json!({
        "appointment_id": "appt-1", "occurrence_key": "k1",
        "user_id": "user-1", "response": "declined",
    }))
    .unwrap();
    assert_eq!(response_row.user_id, "user-1");

    let authorization: ReminderAuthorization = serde_json::from_value(json!({
        "command_id": "cmd-1", "appointment_id": "appt-1", "recipient_id": "user-1",
        "company_id": "co-1", "actor_id": "user-1", "assignment_id": "asg-1",
        "action": "appointment.save", "authority_json": "{}",
        "source_hash": "abc", "source_version": 7, "created_at": 1000,
    }))
    .unwrap();
    assert_eq!(authorization.source_version, 7);
    assert_eq!(authorization.action, "appointment.save");
}
