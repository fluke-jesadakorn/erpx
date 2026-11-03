//! Field/Operations/Collaboration/Files projection: the retained TS projector creates
//! no work_items/provider effects for these finite families. My actions uses
//! authoritative owner reads. Only the read receipt/checkpoint and write lease
//! acknowledgement are projected, preserving commit-before-ack recovery.
use crate::service_count_worker::{ProjectionScope, run_once_with_scope};
use sqlx::PgPool;
pub(crate) fn predicates() -> (String, String) {
    let q = format!(
        "({}) OR ({}) OR ({}) OR ({}) OR ({})",
        erpx_server_field_service::public::field_outbox::QUARANTINE,
        erpx_server_operations::public::field_outbox::QUARANTINE,
        erpx_server_collaboration::public::field_outbox::QUARANTINE,
        erpx_server_files::public::field_outbox::QUARANTINE,
        erpx_server_finance::public::field_outbox::QUARANTINE
    );
    let source = format!(
        "({}) OR ({}) OR ({})",
        erpx_server_field_service::public::field_outbox::ELIGIBLE,
        erpx_server_operations::public::field_outbox::ELIGIBLE,
        erpx_server_collaboration::public::field_outbox::ELIGIBLE
    );
    let eligible = format!(
        "(({source}) AND ({})) OR (({}) AND ({})) OR ({})",
        erpx_server_workflow::public::field_outbox::COMMAND_EVIDENCE,
        erpx_server_files::public::field_outbox::ELIGIBLE,
        erpx_server_workflow::public::field_outbox::FILES_COMMAND_EVIDENCE,
        erpx_server_finance::public::field_outbox::ELIGIBLE
    );
    (q, eligible)
}
pub async fn run_once(
    write: &PgPool,
    read: &PgPool,
    token: &str,
    now: i64,
) -> Result<bool, sqlx::Error> {
    let (q, e) = predicates();
    run_once_with_scope(
        write,
        read,
        token,
        now,
        ProjectionScope {
            quarantine: &q,
            eligible: &e,
            ambiguous: "FALSE",
        },
    )
    .await
}
#[cfg(feature = "private-field-api")]
pub(crate) async fn run(
    pairs: Vec<(PgPool, PgPool)>,
    shutdown: impl std::future::Future<Output = ()> + Send,
) {
    let (q, e) = predicates();
    let writes = pairs.iter().map(|(write, _)| write.clone()).collect();
    tokio::select! {
        _=shutdown=>{},
        _=crate::service_count_worker::run_with_scope(pairs,std::future::pending(),ProjectionScope{quarantine:&q,eligible:&e,ambiguous:"FALSE"})=>{},
        _=crate::field_reservation_alerts::run(writes)=>{},
    }
}
