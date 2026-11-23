//! Frozen native Work Projection calendar read spec (external oracle).
//!
//! Pins the exact allow-list and positional arities of the twelve SELECT facts
//! that the TypeScript feature bridge in
//! `packages/feature-work_projection/src/lib/native-calendar-read.ts` sends to
//! the private `/work/owner-read` route. Any drift in the Rust read surface
//! breaks this test before it can break a live calendar command. No database
//! and no network are required.

use erpx_server_shared::public::BusinessError;
use erpx_server_work_projection::public::calendar_read::{arity, supported, validate};
use erpx_server_work_projection::public::calendar_read_http::{
    CalendarReadFuture, CalendarReadHttpBackend, CalendarReadHttpRequest, CalendarReadHttpTarget,
    calendar_read_router,
};
use serde_json::Value;
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
