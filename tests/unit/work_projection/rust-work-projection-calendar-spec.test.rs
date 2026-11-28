//! Frozen native Work Projection calendar write spec (external oracle).
//!
//! Pins the exact allow-list, positional arities, RETURNING set and reply
//! envelope that the TypeScript dispatcher in
//! `modules/work-projection/server/src/adapters/calendar-commands-facts.ts`
//! relies on. Any drift in the Rust owner surface breaks this test before it
//! can break a live calendar command. This is a pure oracle: no database and no
//! network are required.

use erpx_platform_shared_server::public::BusinessError;
use erpx_work_projection_server::public::owner_write::{
    OwnerWriteCommand, OwnerWritePort, execute_owner_write, returning, supported, validate,
};
use erpx_work_projection_server::public::owner_write_http::{
    OwnerWriteFuture, OwnerWriteHttpBackend, OwnerWriteHttpRequest, OwnerWriteHttpTarget,
    owner_write_router,
};
use serde_json::{Value, json};
use std::{future::Future, sync::Arc};

/// The 36 typed owner commands and their exact positional arities.
const SPEC: &[(&str, usize)] = &[
    ("wp_cal_occurrence_exception_upsert", 5),
    ("wp_cal_appointment_version_bump", 3),
    ("wp_cal_series_until", 2),
    ("wp_cal_appointment_clone", 11),
    ("wp_cal_series_insert", 8),
    ("wp_cal_attendee_clone", 3),
    ("wp_cal_participant_clone", 3),
    ("wp_cal_appointment_update", 8),
    ("wp_cal_announcement_dates", 5),
    ("wp_cal_appointment_upsert", 21),
    ("wp_cal_attendee_delete", 2),
    ("wp_cal_attendee_insert", 8),
    ("wp_cal_participant_delete", 2),
    ("wp_cal_participant_insert", 3),
    ("wp_cal_series_delete", 2),
    ("wp_cal_series_upsert", 8),
    ("wp_cal_occurrence_response_upsert", 6),
    ("wp_cal_attendee_response_update", 5),
    ("wp_cal_attendee_reminder_update", 4),
    ("wp_cal_reminder_delivery_delete", 3),
    ("wp_cal_occurrence_exception_cancel", 3),
    ("wp_cal_appointment_status", 4),
    ("wp_cal_announcement_upsert", 13),
    ("wp_cal_announcement_status", 4),
    ("wp_cal_history_insert", 8),
    ("wp_reminder_delivery_upsert", 5),
    ("wp_reminder_delivery_claim", 5),
    ("wp_reminder_authorization_insert", 11),
    ("wp_daily_plan_write", 8),
    ("wp_page_layout_save", 7),
    ("wp_page_layout_receipt_save", 9),
    ("wp_quick_access_save", 6),
    ("wp_quick_access_receipt_save", 7),
    ("wp_projection_receipt", 4),
    ("wp_work_item_upsert", 11),
    ("wp_projection_checkpoint", 3),
];

#[test]
fn native_owner_command_table_is_exact_and_arity_fenced() {
    assert_eq!(SPEC.len(), 36);
    for (kind, arity) in SPEC {
        assert!(supported(kind), "{kind} must be allow-listed");
        assert!(validate(kind, *arity).is_ok(), "{kind}/{arity} must validate");
        assert!(validate(kind, arity + 1).is_err(), "{kind} arity fence");
        assert!(validate(kind, arity.saturating_sub(1)).is_err(), "{kind} arity fence");
    }
    assert!(!supported("wp_cal_unknown"));
    assert!(validate("wp_cal_unknown", 1).is_err());
}

#[test]
fn returning_set_matches_the_ts_row_dispatch() {
    // calendar-commands-facts.ts read44 is the only calendar RETURNING write.
    assert!(returning("wp_cal_attendee_reminder_update"));
    assert!(returning("wp_reminder_delivery_claim"));
    assert!(!returning("wp_cal_history_insert"));
    assert!(!returning("wp_cal_appointment_upsert"));
}

struct Echo;

impl OwnerWritePort for Echo {
    type Error = BusinessError;

    fn execute<'a>(
        &'a mut self,
        command: &'a OwnerWriteCommand<'a>,
        _key: &'a str,
    ) -> impl Future<Output = Result<Value, Self::Error>> + Send + 'a {
        async move { Ok(json!({"kind": command.kind})) }
    }
}

fn cmd<'a>(kind: &'a str, values: &'a [Value]) -> OwnerWriteCommand<'a> {
    OwnerWriteCommand {
        kind,
        values,
        company_id: "co-1",
        actor_id: "user-1",
        now: 1,
    }
}

#[tokio::test]
async fn reply_envelope_is_native_and_fail_closed_before_the_port() {
    let values = vec![json!(1); 8];
    let reply = execute_owner_write(&mut Echo, &cmd("wp_cal_series_insert", &values), "key-1")
        .await
        .unwrap();
    assert_eq!(
        reply,
        json!({"ok": true, "kind": "wp_cal_series_insert", "result": {"kind": "wp_cal_series_insert"}, "returning": false})
    );

    assert!(execute_owner_write(&mut Echo, &cmd("unknown", &values), "key-1")
        .await
        .is_err());
    assert!(execute_owner_write(&mut Echo, &cmd("wp_cal_series_insert", &[]), "key-1")
        .await
        .is_err());
    assert!(execute_owner_write(&mut Echo, &cmd("wp_cal_series_insert", &values), "")
        .await
        .is_err());
}

struct Noop;

impl OwnerWriteHttpBackend for Noop {
    fn execute<'a>(&'a self, _: OwnerWriteHttpRequest<'a>) -> OwnerWriteFuture<'a> {
        Box::pin(async { Err(BusinessError::invalid("NOOP", "noop")) })
    }
}

#[test]
fn http_ingress_is_mount_ready_for_the_registered_holding() {
    let target = OwnerWriteHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        control: false,
        backend: Arc::new(Noop) as Arc<dyn OwnerWriteHttpBackend>,
    };
    assert!(owner_write_router(vec![target]).is_ok());
}
