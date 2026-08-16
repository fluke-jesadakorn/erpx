//! Generation-v2 product transport acceptance audit.

const SALES_PROTO: &str = include_str!("../../../crates/proto/proto/erpx/sales/v1/service.proto");
const PROCUREMENT_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/procurement/v1/service.proto");
const INVENTORY_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/inventory/v1/service.proto");
const ASSETS_PROTO: &str = include_str!("../../../crates/proto/proto/erpx/assets/v1/service.proto");
const MAINTENANCE_PROTO: &str =
    include_str!("../../../crates/proto/proto/erpx/maintenance/v1/service.proto");
const SERVER: &str = include_str!("../../server/src/lib.rs");

#[test]
fn product_transports_use_owner_command_query_pairs_and_real_methods() {
    for (proto, command, query, method) in [
        (
            SALES_PROTO,
            "SalesCommandService",
            "SalesQueryService",
            "rpc CreateQuote",
        ),
        (
            PROCUREMENT_PROTO,
            "ProcurementCommandService",
            "ProcurementQueryService",
            "rpc Create",
        ),
        (
            INVENTORY_PROTO,
            "InventoryCommandService",
            "InventoryQueryService",
            "rpc Register",
        ),
        (
            ASSETS_PROTO,
            "AssetsCommandService",
            "AssetsQueryService",
            "rpc Create",
        ),
        (
            MAINTENANCE_PROTO,
            "MaintenanceCommandService",
            "MaintenanceQueryService",
            "rpc Open",
        ),
    ] {
        assert!(proto.contains(&format!("service {command}")));
        assert!(proto.contains(&format!("service {query}")));
        assert!(proto.contains(method));
        assert!(!proto.contains("bytes aggregate_json"));
        assert!(!proto.contains(concat!("package erpx.", "platform.v1")));
    }
}

#[test]
fn product_transports_are_registered_only_under_target_packages() {
    for marker in [
        "SalesCommandServiceServer::new",
        "SalesQueryServiceServer::new",
        "ProcurementCommandServiceServer::new",
        "ProcurementQueryServiceServer::new",
        "InventoryCommandServiceServer::new",
        "InventoryQueryServiceServer::new",
        "AssetsCommandServiceServer::new",
        "AssetsQueryServiceServer::new",
        "MaintenanceCommandServiceServer::new",
        "MaintenanceQueryServiceServer::new",
    ] {
        assert!(
            SERVER.contains(marker),
            "missing target registration {marker}"
        );
    }
    assert!(!SERVER.contains(concat!("erpx_proto::", "platform")));
    assert!(!SERVER.contains(concat!("erpx.", "platform.v1")));
}
