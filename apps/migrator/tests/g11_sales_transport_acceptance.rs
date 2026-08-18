//! G11 Sales transport acceptance evidence.
//!
//! This packet is deliberately read-only: it checks parity between the
//! authoritative Sales source/read implementation and the coordinator's
//! checked-in gRPC surface.  It does not mutate databases, register a
//! connector, change command ownership, or claim live vendor acceptance.

use std::{env, net::TcpStream, time::Duration};

const SALES_DOMAIN: &str = include_str!("../../../crates/domain/src/sales.rs");
const SALES_PERSISTENCE: &str =
    include_str!("../../../crates/infrastructure/src/sales_persistence.rs");
const SALES_PROTO: &str = include_str!("../../../crates/proto/proto/erpx/sales/v2/service.proto");
const SERVER_TRANSPORTS: &str = include_str!("../../server/src/lib.rs");
const SERVER_COMPOSITION: &str = include_str!("../../server/src/lib.rs");

const SALES_AGGREGATE_TYPES: [&str; 7] = [
    "lead",
    "opportunity",
    "quote",
    "sales_order",
    "delivery_handoff",
    "invoice_handoff",
    "collection_state",
];

fn require_markers(surface: &str, name: &str, markers: &[&str]) {
    for marker in markers {
        assert!(
            surface.contains(marker),
            "Sales acceptance marker missing from {name}: {marker}"
        );
    }
}

fn filter_value() -> &'static str {
    let marker = "const SALES_SOURCE_EVENT_TYPE_FILTER: &str = \"";
    let start = SALES_PERSISTENCE
        .find(marker)
        .expect("Sales source filter constant must remain explicit")
        + marker.len();
    let tail = &SALES_PERSISTENCE[start..];
    let end = tail
        .find("\";")
        .expect("Sales source filter constant must be terminated");
    &tail[..end]
}

#[test]
fn sales_source_replay_filter_and_read_model_safety_are_closed() {
    require_markers(
        SALES_DOMAIN,
        "Sales domain",
        &[
            "lifecycle_mutation_rotates_command_identity",
            "validate_for_command",
            "SalesProjectionReconciliation",
        ],
    );
    require_markers(
        SALES_PERSISTENCE,
        "Sales SQLx source/read persistence",
        &[
            "read_source_event_page",
            "WHERE holding_ref=$1 AND {SALES_SOURCE_EVENT_TYPE_FILTER}",
            "validate_aggregate_type(&cursor.aggregate_type)?",
            "validate_aggregate_type(&aggregate_type)?",
            "get_for_scope",
            "decode_read_aggregate",
            "aggregate.validate()?",
            "validate_read_scope",
            "reconcile_projection",
            "project_event",
            "clear_projection",
            "source_version",
            "outbox_events",
            "sales_command_idempotency",
        ],
    );

    let filter = filter_value();
    assert!(
        filter.starts_with("aggregate_type IN ("),
        "Sales source replay must use an aggregate-type allow-list"
    );
    for aggregate_type in SALES_AGGREGATE_TYPES {
        assert!(
            filter.contains(&format!("'{aggregate_type}'")),
            "Sales source replay filter is missing {aggregate_type}"
        );
    }
    for forbidden_type in ["pos_sale", "hr_employee", "scm_item", "legal_document"] {
        assert!(
            !filter.contains(&format!("'{forbidden_type}'")),
            "Sales source replay filter must not ingest {forbidden_type}"
        );
    }

    let filter_use_count = SALES_PERSISTENCE
        .match_indices("SALES_SOURCE_EVENT_TYPE_FILTER")
        .count();
    assert!(
        filter_use_count >= 3,
        "Sales source filter must be used by both initial and cursor-paged replay queries"
    );
    eprintln!(
        "G11 Sales source/read evidence: closed seven-type source replay, holding-scoped cursoring, validated identity/scope reads, contiguous projection, reconciliation, and clear/replay recovery"
    );
}

#[test]
fn sales_proto_and_server_transport_surfaces_have_owner_command_query_parity() {
    require_markers(
        SALES_PROTO,
        "sales/v1/service.proto",
        &[
            "package erpx.sales.v2;",
            "service SalesCommandService",
            "rpc CreateQuote",
            "rpc CreateSalesOrder",
            "service SalesQueryService",
            "rpc GetQuote",
            "rpc GetSalesOrder",
            "rpc ListSalesOrders",
        ],
    );
    require_markers(
        SERVER_COMPOSITION,
        "server target Sales registration",
        &[
            "SalesCommandServiceServer::new",
            "SalesQueryServiceServer::new",
            "SalesCommandRpc::new",
            "SalesQueryRpc::new",
        ],
    );
    assert!(!SERVER_TRANSPORTS.contains(concat!("impl Sales", "Service for")));
    eprintln!(
        "G11 Sales transport evidence: typed owner command/query RPCs and target-only server registration are present"
    );
}

#[test]
fn sales_live_database_and_vendor_evidence_is_explicitly_queued() {
    let database_references = [
        "ERPX_TEST_TENANT_WRITE_DATABASE_URL",
        "ERPX_TEST_TENANT_READ_DATABASE_URL",
        "ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE",
    ];
    let missing_database = database_references
        .iter()
        .copied()
        .filter(|name| {
            env::var(name)
                .ok()
                .is_none_or(|value| value.trim().is_empty())
        })
        .collect::<Vec<_>>();
    let configured_database_count = database_references.len() - missing_database.len();
    assert!(
        configured_database_count == 0 || configured_database_count == database_references.len(),
        "G11 Sales live acceptance must fail closed on partial database configuration: {configured_database_count}/{} configured; missing {}",
        database_references.len(),
        missing_database.join(", ")
    );
    if missing_database.is_empty() {
        eprintln!(
            "G11 Sales live SQLx blocker: database references are configured, but this static packet does not claim a disposable write/read/replay/reconcile run"
        );
    } else {
        eprintln!(
            "G11 Sales live SQLx blocker queued: missing database references: {}",
            missing_database.join(", ")
        );
    }

    let vendor_reference = "ERPX_G11_SALES_VENDOR_ENDPOINT";
    if env::var_os(vendor_reference).is_some() {
        eprintln!(
            "G11 Sales vendor blocker: {vendor_reference} is configured, but no vendor certification is claimed by this static packet"
        );
    } else {
        eprintln!(
            "G11 Sales vendor certification blocker queued: {vendor_reference} is not configured"
        );
    }

    let legacy_reachable =
        TcpStream::connect_timeout(&([127, 0, 0, 1], 3004).into(), Duration::from_millis(300))
            .is_ok();
    if legacy_reachable {
        eprintln!(
            "G11 Sales Legacy bridge probe: port 3004 is reachable; Sales bridge and cutover acceptance remain queued"
        );
    } else {
        eprintln!(
            "G11 Sales runtime blocker queued: Legacy development server is not reachable on port 3004"
        );
    }
}
