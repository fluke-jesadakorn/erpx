use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_runtime::public::{
    EmbeddedMigrationInventory, MigrationInventory, MigrationInventoryReader, MigrationPhase,
    foundation_router, foundation_router_with_inventory, inspect_migration,
    validate_foundation_address,
};
use tower::ServiceExt;

#[tokio::test]
async fn liveness_never_promotes_the_foundation_to_business_readiness() {
    let response = foundation_router()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(body["data"]["phase"], "foundation");
    assert_eq!(body["data"]["acceptance"], false);

    let response = foundation_router()
        .oneshot(
            Request::builder()
                .uri("/api/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(body["error"]["code"], "RUST_MIGRATION_INCOMPLETE");
    assert_eq!(body["pendingOwners"].as_array().unwrap().len(), 41);
}

#[tokio::test]
async fn missing_business_routes_return_generic_404_without_claiming_an_alias() {
    let response = foundation_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/holdings/acme/finance?token=not-reflected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().get("location").is_none());
    assert!(response.headers().get("set-cookie").is_none());
    assert_eq!(
        &to_bytes(response.into_body(), 65536).await.unwrap()[..],
        b"Not Found"
    );
}

#[test]
fn inventory_covers_the_owner_registry_and_does_not_claim_migration() {
    let (inventory, readiness) = inspect_migration(&EmbeddedMigrationInventory).unwrap();
    let owners: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/contracts/src/ownership.json"
    ))
    .unwrap();
    let mut expected: Vec<_> = owners["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|owner| owner["id"].as_str().unwrap())
        .collect();
    let mut actual: Vec<_> = inventory
        .owners
        .iter()
        .map(|owner| owner.id.as_str())
        .collect();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected);
    assert!(!readiness.ready);
    assert!(!readiness.source_acceptance);
    assert!(
        inventory
            .owners
            .iter()
            .all(|owner| owner.phase != MigrationPhase::Active)
    );
}

#[derive(Clone)]
struct Metadata(MigrationInventory);
impl MigrationInventoryReader for Metadata {
    fn inventory(&self) -> Result<MigrationInventory, String> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn active_metadata_cannot_substitute_for_source_registered_business_owners() {
    for claim_business_complete in [false, true] {
        let mut metadata: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/status/rust-backend-migration.json"
        ))
        .unwrap();
        metadata["backendMigrated"] = serde_json::json!(claim_business_complete);
        for owner in metadata["owners"].as_array_mut().unwrap() {
            owner["phase"] = serde_json::json!("active");
            owner["businessMigrationComplete"] = serde_json::json!(claim_business_complete);
            for phase in owner["surfaces"].as_object_mut().unwrap().values_mut() {
                *phase = serde_json::json!("active");
            }
        }
        let reader = Metadata(serde_json::from_value(metadata).unwrap());
        let (_, readiness) = inspect_migration(&reader).unwrap();
        assert!(!readiness.ready);
        assert_eq!(readiness.pending_owners.len(), 41);
        assert!(readiness.registered_business_owners.is_empty());
        let response = foundation_router_with_inventory(reader)
            .oneshot(
                Request::builder()
                    .uri("/api/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}

#[test]
fn unknown_duplicate_or_missing_inventory_owners_fail_closed() {
    let inventory = EmbeddedMigrationInventory.inventory().unwrap();
    let mut unknown = inventory.clone();
    unknown.owners[0].id = "not-a-real-owner".to_owned();
    let (_, readiness) = inspect_migration(&Metadata(unknown)).unwrap();
    assert!(!readiness.ready);
    assert_eq!(readiness.unknown_owners, vec!["not-a-real-owner"]);
    let mut duplicate = inventory.clone();
    duplicate.owners.push(duplicate.owners[0].clone());
    let (_, readiness) = inspect_migration(&Metadata(duplicate)).unwrap();
    assert!(!readiness.ready);
    assert_eq!(readiness.duplicate_owners, vec!["brand"]);
    let mut missing = inventory;
    missing.owners.pop();
    assert!(!inspect_migration(&Metadata(missing)).unwrap().1.ready);
}

#[test]
fn foundation_refuses_reserved_installed_or_public_listeners() {
    for port in [
        0, 3000, 3001, 3100, 3101, 3103, 3200, 3299, 5432, 5433, 5435, 8080,
    ] {
        assert!(
            validate_foundation_address(&format!("127.0.0.1:{port}")).is_err(),
            "accepted {port}"
        );
    }
    for address in [
        "0.0.0.0:3110",
        "[::]:3110",
        "localhost:3110",
        "not-an-address",
    ] {
        assert!(validate_foundation_address(address).is_err());
    }
    assert_eq!(
        validate_foundation_address("127.0.0.1:3110")
            .unwrap()
            .port(),
        3110
    );
}
