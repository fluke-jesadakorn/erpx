//! G13 deployment, recovery, and observability acceptance verifier.
//!
//! This test owns no runtime implementation. It reads the authoritative
//! migration, protobuf, worker, and server sources and checks
//! that the deployment boundary still has the required fail-closed seams.
//! Live database/restart/rollback evidence is intentionally reported as a
//! queued blocker when a disposable runtime is not configured.

use std::{
    env,
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

const MIGRATION_LIB: &str = include_str!("../../../crates/migration/src/lib.rs");
const CONTROL_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations/0001_control_plane.sql");
const COMMAND_RECEIPTS_MIGRATION: &str =
    include_str!("../../../crates/migration/migrations/0002_g1_002_command_receipts.sql");
const PROTO_BUILD: &str = include_str!("../../../crates/proto/build.rs");
const PROTO_LIB: &str = include_str!("../../../crates/proto/src/lib.rs");
const COMMON_PROTO: &str = include_str!("../../../crates/proto/proto/erpx/common/v1/context.proto");
const IDENTITY_ACCESS_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/identity_access/v1/service.proto");
const PLATFORM_ADMINISTRATION_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/platform_administration/v1/service.proto");
const ACTIVE_PROTO_CATALOG: &str =
    include_str!("../../../crates/proto/proto/erpx/catalog/v2/active.proto");
const MIGRATOR_MAIN: &str = include_str!("../src/main.rs");
const WORKER_LIB: &str = include_str!("../../worker/src/lib.rs");
const WORKER_APP: &str = include_str!("../../worker/src/app.rs");
const PROJECTION_SCHEDULER: &str =
    include_str!("../../../crates/infrastructure/src/tenant_projection_scheduler.rs");
const G13_TARGET_RUNTIME: &str = include_str!("../../worker/src/g13_target_runtime.rs");
const SERVER_LIB: &str = include_str!("../../server/src/lib.rs");

fn require_markers(surface: &str, name: &str, markers: &[&str]) {
    for marker in markers {
        assert!(
            surface.contains(marker),
            "G13 contract marker missing from {name}: {marker}"
        );
    }
}

fn require_ordered(surface: &str, name: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = surface[cursor..].find(marker).unwrap_or_else(|| {
            panic!("G13 ordered recovery marker missing from {name}: {marker}");
        });
        cursor += offset + marker.len();
    }
}

fn env_reference_is_safe(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
}

fn parse_http_status(response: &[u8]) -> Option<u16> {
    let line = response.split(|byte| *byte == b'\n').next()?;
    let line = std::str::from_utf8(line).ok()?.trim_end_matches('\r');
    let mut fields = line.split_ascii_whitespace();
    let version = fields.next()?;
    let status = fields.next()?.parse::<u16>().ok()?;
    version.starts_with("HTTP/").then_some(status)
}

fn erpx_http_status() -> Option<u16> {
    let mut stream =
        TcpStream::connect_timeout(&([127, 0, 0, 1], 3004).into(), Duration::from_millis(300))
            .ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = [0_u8; 4096];
    let bytes = stream.read(&mut response).ok()?;
    parse_http_status(&response[..bytes])
}

#[test]
fn g13_authoritative_migration_and_protobuf_compatibility_surfaces_are_present() {
    require_markers(
        MIGRATION_LIB,
        "crates/migration/src/lib.rs",
        &[
            "CONTROL_PLANE_MIGRATOR",
            "CONTROL_READ_MIGRATOR",
            "TENANT_WRITE_MIGRATOR",
            "TENANT_READ_MIGRATOR",
            "pub enum MigrationPhase",
            "RollbackRehearsed",
            "FinalDeltaImported",
            "ErpxCommandOwner",
            "LegacyReadOnly",
            "RetentionComplete",
            "LegacyRetired",
            "pub enum CommandOwner",
            "RetentionNotSchedulable",
            "pub struct LegacySourceSnapshot",
            "legacy_snapshot_contract_hash",
            "Source-specific extractors live outside ERPX",
        ],
    );
    require_markers(
        CONTROL_MIGRATION,
        "migrations/0001_control_plane.sql",
        &[
            "erpx_control.idempotency_keys",
            "erpx_control.outbox_events",
            "erpx_control.projection_checkpoints",
            "erpx_control.projection_event_receipts",
            "CONSTRAINT outbox_idempotency_key_unique UNIQUE (idempotency_key)",
        ],
    );
    require_markers(
        COMMAND_RECEIPTS_MIGRATION,
        "migrations/0002_g1_002_command_receipts.sql",
        &[
            "ALTER TABLE erpx_control.idempotency_keys",
            "CREATE TABLE IF NOT EXISTS erpx_control.projection_event_receipts",
            "PRIMARY KEY (projection_name, event_id)",
        ],
    );

    // Generation-v2 sources are the only inputs to the approved descriptor
    // and generated Rust business surface.
    require_markers(
        PROTO_BUILD,
        "crates/proto/build.rs",
        &[
            "const ACTIVE_PROTO_SOURCE_FILES",
            "const ACTIVE_PROTO_ENTRYPOINTS",
            "proto/erpx/catalog/v2/active.proto",
            "cargo:rerun-if-changed={proto_file}",
            "build_client(true)",
            "build_server(true)",
            "file_descriptor_set_path",
            "compile_protos(ACTIVE_PROTO_ENTRYPOINTS",
            "INFRASTRUCTURE_PROTO_ENTRYPOINTS",
            "generated_hash",
            "approved_hash",
        ],
    );
    require_markers(
        PROTO_LIB,
        "crates/proto/src/lib.rs",
        &[
            "tonic::include_proto!(\"erpx.common.v1\")",
            "pub const ACTIVE_SERVICE_COUNT: usize = 65",
            "pub const ACTIVE_SERVICE_ID_FIRST: u16 = 101",
            "pub const ACTIVE_SERVICE_ID_LAST: u16 = 165",
        ],
    );
    require_markers(
        ACTIVE_PROTO_CATALOG,
        "catalog/v2/active.proto",
        &[
            "package erpx.catalog.v2;",
            "import \"erpx/identity_access/v1/service.proto\";",
            "import \"erpx/collaboration/v1/service.proto\";",
            "import \"erpx/agent/v1/service.proto\";",
            "import \"erpx/reference/v1/service.proto\";",
        ],
    );
    require_markers(
        COMMON_PROTO,
        "common/v1/context.proto",
        &["package erpx.common.v1;"],
    );
    require_markers(
        PLATFORM_ADMINISTRATION_PROTO,
        "platform_administration/v1/service.proto",
        &[
            "package erpx.platform_administration.v1;",
            "service PlatformAdministrationCommandService",
            "service PlatformAdministrationQueryService",
        ],
    );
    require_markers(
        IDENTITY_ACCESS_PROTO,
        "identity_access/v1/service.proto",
        &[
            "package erpx.identity_access.v1;",
            "service IdentityAccessCommandService",
            "service IdentityAccessQueryService",
        ],
    );
    assert!(!PROTO_BUILD.contains(concat!("proto/erpx/platform/", "v1/")));
    assert!(!PROTO_LIB.contains(concat!("include_proto!(\"erpx.platform", ".v1\")")));
}

#[test]
fn g13_worker_restart_idempotency_and_observability_seams_are_present() {
    require_markers(
        WORKER_LIB,
        "apps/worker/src/lib.rs",
        &[
            "pub struct WorkerHandlerReport",
            "pub trait NamedWorkerHandler",
            "pub struct WorkerHandlerRegistry",
            "pub async fn run_registered_until_stopped",
            "pub async fn run_registered_until_shutdown",
            "tokio::signal::ctrl_c()",
            "run_registered_handler_until_stopped",
            "heartbeat.record_at",
            "record_worker_heartbeat",
            "retry_policy",
        ],
    );
    require_markers(
        WORKER_APP,
        "apps/worker/src/app.rs",
        &[
            "tracing_subscriber::fmt()",
            "with_env_filter(tracing_subscriber::EnvFilter::from_default_env())",
            "worker_id = %worker_id",
            "handler = %handler_name",
            "\"worker_started\"",
            "run_registered_until_shutdown",
            "retried = report.retried",
            "dead_lettered = report.dead_lettered",
            "DIRECTORY_SYNC_HANDLER_NAME",
        ],
    );
    require_markers(
        PROJECTION_SCHEDULER,
        "crates/infrastructure/src/tenant_projection_scheduler.rs",
        &[
            "pub struct ProjectionLease",
            "pub fn claim(",
            "lease_until",
            "retry budget is exhausted",
            "dead_letter",
            "ON CONFLICT (claim_key) DO NOTHING",
            "claim_key_retries_are_idempotent_and_scope_bound",
        ],
    );
    require_markers(
        SERVER_LIB,
        "apps/server/src/lib.rs",
        &[
            "\"/healthz\"",
            "\"/readyz\"",
            "\"/metrics\"",
            "HttpRequestTrace::start",
            "worker_heartbeat",
            "projection_freshness",
        ],
    );
}

#[test]
fn g13_recovery_readiness_is_fail_closed_before_expiry_or_success_evidence() {
    // The readiness gate must remain a dependency chain: retention expiry is
    // possible only after Legacy is read-only, durable work is settled,
    // reconciliation is matched, and persisted rollback evidence is complete.
    // The rollback worker must likewise validate the immutable source fence and
    // recovery cardinality/read-model state before it can append success
    // evidence. This source-level assertion keeps a future refactor from
    // accidentally moving the durable success boundary earlier.
    require_ordered(
        G13_TARGET_RUNTIME,
        "apps/worker/src/g13_target_runtime.rs",
        &[
            "async fn due_retention(",
            "if phase != \"legacy_read_only\" || retention_until > now",
            "if work_pending || work_failed",
            "if reconciliation_state != \"matched\"",
            "rollback_evidence_is_ready(",
            "Ok(Some((source_fence, retention_until)))",
            "async fn process_claim(",
            "\"rollback\" => {",
            "if !self.rollback_writes_blocked",
            "rehearse_rollback(self.run_id, &claim.source_fence)",
            "if !rollback_recovery_is_complete(&result, snapshot)",
            "self.store\n                    .record_evidence(",
            "MigrationPhase::RollbackRehearsed",
            "writes_blocked: true",
            "outbox_replayed: true",
            "fn rollback_recovery_is_complete(",
            "try_fold(0_u64",
            "result.rows_captured == expected_rows",
            "&& result.rows_restored == expected_rows",
            "&& result.outbox_events_replayed == expected_rows",
            "&& result.read_model_rebuilt",
            "result.restored_state_hash.len() == 64",
            ".restored_state_hash\n            .bytes()\n            .all",
        ],
    );
}

#[test]
fn g13_rollback_boundary_is_fail_closed() {
    require_markers(
        MIGRATOR_MAIN,
        "apps/migrator/src/main.rs",
        &[
            "read-only and dry-run by default",
            "rollback_rehearsal",
            "command_owner_switch",
            "legacy_read_only_retention",
            "performed: false",
            "state_change: false",
            "read_only: true",
        ],
    );
    assert!(
        !MIGRATOR_MAIN.contains("Command::Cutover")
            && !MIGRATOR_MAIN.contains("perform_cutover")
            && !MIGRATOR_MAIN.contains("switch_command_owner"),
        "the migrator verifier must not acquire a production cutover command"
    );
}

#[test]
fn g13_external_and_runtime_acceptance_blockers_are_explicitly_queued() {
    let runtime_references = [
        "ERPX_CONTROL_WRITE_DATABASE_URL",
        "ERPX_CONTROL_READ_DATABASE_URL",
        "ERPX_TENANT_ADMIN_DATABASE_URL",
        "ERPX_TENANT_DATABASE_URL_TEMPLATE",
    ];
    let missing_runtime_references = runtime_references
        .iter()
        .copied()
        .filter(|name| !env_reference_is_safe(name))
        .collect::<Vec<_>>();
    if !missing_runtime_references.is_empty() {
        eprintln!(
            "G13 runtime blocker queued: missing configured database references: {}",
            missing_runtime_references.join(", ")
        );
    }

    match erpx_http_status() {
        Some(200) => eprintln!(
            "G13 ERPX runtime probe: HTTP 200 on port 3004; deployment/rollback acceptance remains queued"
        ),
        Some(status) => eprintln!(
            "G13 runtime blocker queued: ERPX port 3004 responded with HTTP {status}, not the required application-ready 200"
        ),
        None => eprintln!(
            "G13 runtime blocker queued: ERPX development server did not return an HTTP response on port 3004"
        ),
    }

    let external_acceptance_references = [
        "ERPX_G13_TARGET_DATABASE_URL",
        "ERPX_G13_ROLLBACK_DATABASE_URL",
    ];
    let missing_external_references = external_acceptance_references
        .iter()
        .copied()
        .filter(|name| !env_reference_is_safe(name))
        .collect::<Vec<_>>();
    if !missing_external_references.is_empty() {
        eprintln!(
            "G13 disposable deployment/rollback evidence queued: missing target or rollback runtime references: {}",
            missing_external_references.join(", ")
        );
    }

    // This test is a queueing probe, not a certification claim. It never
    // opens a database, writes a row, changes a process, or activates a
    // customer connector.
}

#[cfg(test)]
mod tests {
    use super::parse_http_status;

    #[test]
    fn erpx_probe_parses_only_real_http_status_lines() {
        assert_eq!(
            parse_http_status(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"),
            Some(200)
        );
        assert_eq!(parse_http_status(b"not-http\r\n"), None);
        assert_eq!(parse_http_status(b"HTTP/1.1 nope\r\n"), None);
    }
}
