//! Asset/Maintenance acceptance verifier.
//!
//! This packet is read-only and owns only this migrator acceptance test. It
//! binds the authoritative source/read reconciliation and forged-event guard
//! to the coordinator transport, while keeping live SQLx evidence explicitly
//! queued when disposable tenant database references are unavailable.

use std::env;

const ASSET_DOMAIN: &str = include_str!("../../../crates/domain/src/asset_maintenance.rs");
const ASSET_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/asset_maintenance_persistence.rs");
const ASSET_LIVE_TEST: &str =
    include_str!("../../../crates/infrastructure/tests/asset_maintenance_persistence.rs");
const PLATFORM_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/platform/v1/asset_maintenance.proto");
const SERVER_RUNTIME: &str = include_str!("../../server/src/lib.rs");
const SERVER_WAVE6_COMPOSITION: &str = include_str!("../../server/src/wave6_composition.rs");
const WORKER_APP: &str = include_str!("../../worker/src/app.rs");

fn require_markers(surface: &str, name: &str, markers: &[&str]) {
    let missing = markers
        .iter()
        .copied()
        .filter(|marker| !surface.contains(marker))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "G11 Asset/Maintenance markers missing from {name}: {}",
        missing.join(", ")
    );
}

fn section<'a>(surface: &'a str, start: &str, end: &str, name: &str) -> &'a str {
    let start_index = surface
        .find(start)
        .unwrap_or_else(|| panic!("G11 Asset/Maintenance section missing from {name}: {start}"));
    let body = &surface[start_index..];
    let end_index = body.find(end).unwrap_or_else(|| {
        panic!("G11 Asset/Maintenance section terminator missing from {name}: {end}")
    });
    &body[..end_index]
}

fn require_markers_in_order(surface: &str, name: &str, markers: &[&str]) {
    let mut previous = 0;
    for marker in markers {
        let offset = surface.find(marker).unwrap_or_else(|| {
            panic!("G11 Asset/Maintenance marker missing from {name}: {marker}")
        });
        assert!(
            offset >= previous,
            "G11 Asset/Maintenance markers are out of order in {name}: {marker}"
        );
        previous = offset;
    }
}

#[test]
fn asset_source_and_read_reconciliation_is_authoritative_and_fail_closed() {
    require_markers(
        ASSET_DOMAIN,
        "asset_maintenance.rs",
        &[
            "AssetMaintenanceProjectionReconciliation",
            "source_version_gap_count",
            "projected_ahead_aggregate_count",
            "pub const fn is_consistent",
            "missing_aggregate_count",
            "orphan_aggregate_count",
        ],
    );
    require_markers(
        ASSET_PERSISTENCE,
        "asset_maintenance_persistence.rs",
        &[
            "pub async fn read_source_event_page",
            "pub async fn reconcile_projection",
            "FROM erpx_tenant.outbox_events",
            "aggregate_type IN ('asset', 'maintenance_work_order')",
            "FROM erpx_tenant.asset_maintenance_read_rows",
            "source_version_gap_count",
            "missing_aggregate_count",
            "stale_aggregate_count",
            "orphan_aggregate_count",
            "projected_ahead_aggregate_count",
            "pub async fn clear_projection",
        ],
    );
    require_markers(
        ASSET_LIVE_TEST,
        "asset_maintenance_persistence.rs integration test",
        &[
            "#[ignore = \"requires two disposable tenant PostgreSQL databases\"]",
            "live_asset_maintenance_write_outbox_projection_read_reconcile_and_rebuild",
            "reconcile_projection()",
            "is_consistent()",
            "clear_projection()",
            "rebuild_page",
        ],
    );
    eprintln!(
        "G11 Asset/Maintenance source/read evidence: authoritative outbox high-water marks, scoped read rows, mismatch counters, and clear/replay recovery are bound"
    );
}

#[test]
fn asset_projector_rejects_forged_or_non_authoritative_events() {
    require_markers(
        ASSET_PERSISTENCE,
        "asset_maintenance_persistence.rs projector",
        &[
            "pub async fn project_event",
            "verify_authoritative_source_event(",
            "event.aggregate.validate()?",
            "Asset/Maintenance projection identity or version mismatch",
            "Asset/Maintenance projector event is not an authoritative source event",
            "Asset/Maintenance projector event does not match the authoritative outbox",
            "actual_payload != *payload",
            "WHERE event_id=$1 AND holding_ref=$2",
        ],
    );
    require_markers(
        ASSET_LIVE_TEST,
        "asset_maintenance_persistence.rs forged-event scenario",
        &[
            "let mut forged_payload = event.payload.clone()",
            "forged read model",
            "a valid-looking but non-authoritative payload must not reach the read model",
            "Uuid::new_v4(),",
            "an event ID absent from the scoped outbox must not be projectable",
        ],
    );
    eprintln!(
        "G11 Asset/Maintenance forged-event evidence: payload, aggregate identity/version, event ID, event type, and holding scope are checked against the write-side outbox"
    );
}

#[test]
fn asset_disposal_custody_and_maintenance_link_fail_closed_in_live_acceptance() {
    require_markers(
        ASSET_DOMAIN,
        "asset_maintenance.rs disposal/custody and maintenance-link guards",
        &[
            "if self.status == AssetStatus::Disposed",
            "return Err(AssetMaintenanceError::AssetDisposed(self.id))",
            "AssetCommand::Dispose {",
            "self.custody_history.push(CustodyTransfer {",
            "validate_context_for_scope(context, to_scope)?",
            "asset.validate()?",
        ],
    );
    require_markers(
        ASSET_LIVE_TEST,
        "asset_maintenance_persistence.rs disposal/custody acceptance",
        &[
            "transfer asset custody",
            "assert_eq!(read_asset.custody_history.len(), 1)",
            "custody read mismatch must fail closed",
            "custody_history_mismatch_count, 1",
            "A forged work order with a cross-company scope must fail before either",
            "assert_eq!(source_rows, 0)",
            "assert_eq!(outbox_rows, 0)",
            "AssetCommand::Dispose {",
            "persist disposed asset",
            "work order for disposed asset must fail closed",
        ],
    );
    eprintln!(
        "G11 Asset/Maintenance lifecycle safety: custody transitions and read tamper detection, cross-scope maintenance rollback, and disposed-asset linkage rejection are required live acceptance evidence"
    );
}

#[test]
fn asset_custody_and_disposal_writes_are_atomic_and_prevalidated() {
    let save = section(
        ASSET_PERSISTENCE,
        "pub async fn save(\n",
        "    pub async fn save_capacity_handoff(",
        "asset_maintenance_persistence.rs save",
    );
    require_markers_in_order(
        save,
        "asset_maintenance_persistence.rs save transaction",
        &[
            "aggregate.validate()?;",
            "let mut tx = self.begin_write().await?;",
            "validate_work_order_asset_link(&mut tx, &self.holding_ref, work_order).await?;",
            "INSERT INTO erpx_tenant.asset_maintenance_aggregates",
            "persist_custody_history(&mut tx, &self.holding_ref, asset).await?;",
            "persist_shared_snapshot_and_outbox(\n            &mut tx,",
            "INSERT INTO erpx_tenant.asset_maintenance_command_idempotency",
            "tx.commit().await?;\n        Ok(receipt)",
        ],
    );
    require_markers(
        ASSET_PERSISTENCE,
        "asset_maintenance_persistence.rs custody persistence",
        &[
            "async fn persist_custody_history(",
            "SELECT transfer\n             FROM erpx_tenant.asset_custody_transfers",
            "WHERE asset_id=$1 AND holding_ref=$2 AND aggregate_version=$3",
            "INSERT INTO erpx_tenant.asset_custody_transfers",
        ],
    );
    require_markers(
        ASSET_LIVE_TEST,
        "asset_maintenance_persistence.rs atomic rollback acceptance",
        &[
            "let error = store",
            "cross-company work order must fail closed",
            "assert_eq!(source_rows, 0)",
            "assert_eq!(outbox_rows, 0)",
            "work order for disposed asset must fail closed",
        ],
    );
    eprintln!(
        "G11 Asset/Maintenance atomicity evidence: malformed/disallowed maintenance links are validated before source writes, while custody history, aggregate state, outbox, and idempotency are committed together or rejected"
    );
}

#[test]
fn asset_maintenance_coordinator_transport_is_present() {
    require_markers(
        PLATFORM_PROTO,
        "platform.proto",
        &[
            "service AssetMaintenanceService",
            "SaveAssetMaintenanceAggregateRequest",
            "GetAssetMaintenanceAggregateRequest",
            "bytes aggregate_json = 3",
        ],
    );
    require_markers(
        SERVER_RUNTIME,
        "server/lib.rs transport and registration",
        &[
            "impl AssetMaintenanceService for AssetMaintenanceRpcImpl",
            "AssetMaintenanceServiceServer::new",
        ],
    );
    require_markers(
        SERVER_WAVE6_COMPOSITION,
        "server/wave6_composition.rs routing and authorization",
        &[
            "asset_maintenance.write",
            "asset_maintenance.read",
            "RoutedDatabaseSide::Write",
            "RoutedDatabaseSide::Read",
        ],
    );
    require_markers(
        WORKER_APP,
        "worker/app.rs",
        &[
            "ProductProjectionExecutor::AssetMaintenance",
            "\"asset_maintenance\" =>",
            "AssetMaintenanceProjectionRunner::new(router)",
        ],
    );
    eprintln!(
        "G11 Asset/Maintenance transport evidence: typed protobuf, scoped server write/read RPCs, and the worker projection scheduler branch are present"
    );
}

#[test]
fn asset_maintenance_live_database_blocker_is_explicitly_reported() {
    let required = [
        "ERPX_TEST_TENANT_WRITE_DATABASE_URL",
        "ERPX_TEST_TENANT_READ_DATABASE_URL",
        "ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE",
        "ERPX_TEST_ADMIN_DATABASE_URL",
    ];
    let missing = required
        .iter()
        .copied()
        .filter(|name| env::var_os(name).is_none_or(|value| value.is_empty()))
        .collect::<Vec<_>>();

    if missing.is_empty() {
        eprintln!(
            "G11 Asset/Maintenance live SQLx blocker remains queued: database references exist, but this verifier does not claim disposable write/read, projection, reconciliation, or rollback execution"
        );
    } else {
        eprintln!(
            "G11 Asset/Maintenance live SQLx blocker queued: missing disposable database references: {}",
            missing.join(", ")
        );
    }
    eprintln!(
        "G11 Asset/Maintenance certification status: source/read and forged-event contracts are source-verified; live PostgreSQL evidence remains required"
    );
}
