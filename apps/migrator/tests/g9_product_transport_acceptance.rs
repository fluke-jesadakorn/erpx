//! G9 product transport acceptance audit.
//!
//! This packet is intentionally read-only: it binds the current protobuf,
//! server composition, and worker composition roots without changing any of
//! those coordinator-owned surfaces.  Source evidence is not promoted to
//! live acceptance; the final test reports the disposable database references
//! required for that certification.

use std::env;

const PLATFORM_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/platform/v1/platform.proto");
const SERVER_MAIN: &str = include_str!("../../server/src/main.rs");
const WORKER_MAIN: &str = include_str!("../../worker/src/main.rs");

fn require_markers(surface: &str, surface_name: &str, markers: &[&str]) {
    let missing = markers
        .iter()
        .copied()
        .filter(|marker| !surface.contains(marker))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "G9 {surface_name} acceptance markers missing: {}",
        missing.join(", ")
    );
}

fn rpc_region<'a>(surface: &'a str, rpc_name: &str, surface_name: &str) -> &'a str {
    let signature = format!("async fn {rpc_name}(");
    let start = surface
        .find(&signature)
        .unwrap_or_else(|| panic!("G9 {surface_name} RPC {rpc_name} acceptance marker is missing"));
    let region = &surface[start..];
    let end = region.find("\n    async fn ").unwrap_or(region.len());
    &region[..end]
}

#[derive(Clone, Copy)]
struct ProductTransport {
    name: &'static str,
    proto_service: &'static str,
    request_json_field: &'static str,
    aggregate_type: &'static str,
    write_rpc: &'static str,
    read_rpc: &'static str,
    scope_label: &'static str,
    module_key: &'static str,
    write_permission: &'static str,
    read_permission: &'static str,
    worker_kind: &'static str,
    worker_executor: &'static str,
    worker_runner: &'static str,
}

const PRODUCT_TRANSPORTS: [ProductTransport; 3] = [
    ProductTransport {
        name: "Sales",
        proto_service: "SalesService",
        request_json_field: "SaveSalesAggregateRequest",
        aggregate_type: "SalesAggregate",
        write_rpc: "save_sales_aggregate",
        read_rpc: "get_sales_aggregate",
        scope_label: "Sales",
        module_key: "sales",
        write_permission: "sales.write",
        read_permission: "sales.read",
        worker_kind: "\"sales\" =>",
        worker_executor: "ProductProjectionExecutor::Sales",
        worker_runner: "SalesProjectionRunner",
    },
    ProductTransport {
        name: "SCM",
        proto_service: "ScmService",
        request_json_field: "SaveScmAggregateRequest",
        aggregate_type: "ScmAggregate",
        write_rpc: "save_scm_aggregate",
        read_rpc: "get_scm_aggregate",
        scope_label: "SCM",
        module_key: "scm",
        write_permission: "scm.write",
        read_permission: "scm.read",
        worker_kind: "\"scm\" =>",
        worker_executor: "ProductProjectionExecutor::Scm",
        worker_runner: "ScmProjectionRunner",
    },
    ProductTransport {
        name: "Asset/Maintenance",
        proto_service: "AssetMaintenanceService",
        request_json_field: "SaveAssetMaintenanceAggregateRequest",
        aggregate_type: "AssetMaintenanceAggregate",
        write_rpc: "save_asset_maintenance_aggregate",
        read_rpc: "get_asset_maintenance_aggregate",
        scope_label: "Asset/Maintenance",
        module_key: "asset_maintenance",
        write_permission: "asset_maintenance.write",
        read_permission: "asset_maintenance.read",
        worker_kind: "\"asset_maintenance\" =>",
        worker_executor: "ProductProjectionExecutor::AssetMaintenance",
        worker_runner: "AssetMaintenanceProjectionRunner",
    },
];

#[test]
fn g9_services_are_registered_in_proto_server_and_worker_composition() {
    for product in PRODUCT_TRANSPORTS {
        require_markers(
            PLATFORM_PROTO,
            product.name,
            &[
                &format!("service {}", product.proto_service),
                product.request_json_field,
                "bytes aggregate_json = 3",
                "Get",
            ],
        );
        require_markers(
            SERVER_MAIN,
            product.name,
            &[
                &format!("impl {} for", product.proto_service),
                &format!("{}Server::new", product.proto_service),
                product.aggregate_type,
            ],
        );
        require_markers(
            WORKER_MAIN,
            product.name,
            &[
                product.worker_kind,
                product.worker_executor,
                product.worker_runner,
                "handler_registry_with_control_plane_multi_tenant_projection_scheduler_executor",
            ],
        );
        eprintln!(
            "G9 {} registration: protobuf service, server RPC, server registration, and worker scheduler branch present",
            product.name
        );
    }
}

#[test]
fn g9_transports_enforce_typed_bounded_json_and_tenant_auth_routing() {
    for product in PRODUCT_TRANSPORTS {
        require_markers(
            SERVER_MAIN,
            product.name,
            &[
                "aggregate_json.is_empty() || input.aggregate_json.len() > 1024 * 1024",
                &format!("let aggregate: {}", product.aggregate_type),
                "serde_json::from_slice(&input.aggregate_json)",
                &format!(
                    "explicit_product_scope(access_scope(&context.tenant_scope), \"{}\")?;",
                    product.scope_label
                ),
                "authorize_entitled_command(",
                product.module_key,
                product.write_permission,
                "authorize_role(",
                product.read_permission,
                "validate_transport_reference(&input.tenant_id, \"tenant_id\")?",
                "validate_transport_reference(&input.holding_ref, \"holding_ref\")?",
                "RoutedDatabaseSide::Write",
                "RoutedDatabaseSide::Read",
                "customer_scope_contains(customer_id, &caller_scope)",
            ],
        );
        eprintln!(
            "G9 {} transport: typed aggregate decode, 1 MiB JSON bound, tenant/holding routing, and labeled write/read permissions present",
            product.name
        );
    }
}

#[test]
fn g9_product_rpc_boundaries_are_exactly_scoped_and_read_only() {
    for product in PRODUCT_TRANSPORTS {
        let write_rpc = rpc_region(SERVER_MAIN, product.write_rpc, product.name);
        require_markers(
            write_rpc,
            product.name,
            &[
                "let claims = self.state.auth.authorize(&request).await?;",
                "explicit_product_scope(access_scope(&context.tenant_scope),",
                "validate_transport_reference(&input.tenant_id, \"tenant_id\")?",
                "validate_transport_reference(&input.holding_ref, \"holding_ref\")?",
                "authorize_entitled_command(",
                product.write_permission,
                "RoutedDatabaseSide::Write",
                "RoutedDatabaseSide::Read",
                "write.database_pool()",
                "read.database_pool()",
            ],
        );

        let read_rpc = rpc_region(SERVER_MAIN, product.read_rpc, product.name);
        require_markers(
            read_rpc,
            product.name,
            &[
                "let claims = self.state.auth.authorize(&request).await?;",
                "explicit_product_scope(query_access_scope(proto_context)?,",
                "validate_transport_reference(&input.tenant_id, \"tenant_id\")?",
                "validate_transport_reference(&input.holding_ref, \"holding_ref\")?",
                "authorize_role(",
                product.read_permission,
                "customer_scope_contains(",
                "RoutedDatabaseSide::Read",
                "read.database_pool()",
            ],
        );
        assert!(
            !read_rpc.contains("RoutedDatabaseSide::Write")
                && !read_rpc.contains("write.database_pool()")
                && !read_rpc.contains("authorize_entitled_command("),
            "G9 {} read RPC must remain read-model-only and role-authorized",
            product.name
        );
        eprintln!(
            "G9 {} exact RPC boundary: authenticated, product-scoped, holding-routed write and read-only read path",
            product.name
        );
    }
}

#[test]
fn g9_live_database_acceptance_blocker_is_explicitly_reported() {
    let required_database_references = [
        "ERPX_TEST_TENANT_WRITE_DATABASE_URL",
        "ERPX_TEST_TENANT_READ_DATABASE_URL",
        "ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE",
        "ERPX_TEST_ADMIN_DATABASE_URL",
    ];
    let missing = required_database_references
        .iter()
        .copied()
        .filter(|name| {
            env::var(name)
                .ok()
                .is_none_or(|value| value.trim().is_empty())
        })
        .collect::<Vec<_>>();
    let configured = required_database_references.len() - missing.len();
    assert!(
        configured == 0 || configured == required_database_references.len(),
        "G9 live acceptance must fail closed on partial database configuration: {configured}/{} configured; missing {}",
        required_database_references.len(),
        missing.join(", ")
    );

    if missing.is_empty() {
        eprintln!(
            "G9 live SQLx blocker remains queued: database references are configured, but this source-bound audit does not claim disposable write/read routing, outbox, projector, and rollback execution"
        );
    } else {
        eprintln!(
            "G9 live SQLx blocker queued: missing disposable database references: {}",
            missing.join(", ")
        );
    }
    eprintln!(
        "G9 certification status: protobuf/server/worker registration and transport contract are source-verified; live database acceptance remains required"
    );
}
