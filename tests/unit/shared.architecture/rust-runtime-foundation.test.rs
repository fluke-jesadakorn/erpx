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
    assert!(readiness.registered_business_owners.is_empty());
    let mut pending: Vec<_> = readiness.pending_owners.iter().map(String::as_str).collect();
    pending.sort_unstable();
    assert_eq!(pending, expected);
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

#[test]
fn native_engineering_roster_matches_actual_typescript_on_json_values() {
    use erpx_server_runtime::public::native_engineering_roster::validate_native_engineering_roster;
    use std::io::Write;
    let bindings=["CONTROL_DB","CONTROL_READ_DB","H001_DB","H001_READ_DB","H002_DB","H002_READ_DB","H003_DB","H003_READ_DB"];
    let mut valid=serde_json::json!({"version":1,"run":"0123456789ab","sourceFingerprint":"source","fileManifestSha256":"manifest","postmasterStartedMs":1700000000000_i64,"entries":[]});
    valid["entries"]=serde_json::Value::Array(bindings.iter().enumerate().map(|(i,b)|serde_json::json!({"binding":b,"token":format!("{:032x}",i+1),"oid":i+1})).collect());
    let mut cases=vec![valid.clone()];
    for (key,value) in [("version",serde_json::json!("1")),("run",serde_json::json!("other")),("postmasterStartedMs",serde_json::json!(0)),("postmasterStartedMs",serde_json::json!(1.5)),("postmasterStartedMs",serde_json::json!(9007199254740992_u64)),("entries",serde_json::json!([]))] {let mut v=valid.clone();v[key]=value;cases.push(v);}
    for (key,value) in [("binding",serde_json::json!("H001_DB")),("token",serde_json::json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")),("token",serde_json::json!(null)),("oid",serde_json::json!(0)),("oid",serde_json::json!(4294967296_u64)),("oid",serde_json::json!(1.5))] {let mut v=valid.clone();v["entries"][0][key]=value;cases.push(v);}
    let mut dup_token=valid.clone();dup_token["entries"][1]["token"]=dup_token["entries"][0]["token"].clone();cases.push(dup_token);
    let mut dup_oid=valid.clone();dup_oid["entries"][1]["oid"]=dup_oid["entries"][0]["oid"].clone();cases.push(dup_oid);
    let mut array_token=valid.clone();array_token["entries"][0]["token"]=serde_json::json!(["00000000000000000000000000000001"]);cases.push(array_token.clone());
    array_token["entries"][1]["token"]=serde_json::json!(["00000000000000000000000000000001"]);cases.push(array_token);
    let mut extras=valid.clone();extras["extra"]=serde_json::json!(true);extras["entries"][0]["extra"]=serde_json::json!("allowed");cases.push(extras);
    cases.extend([serde_json::Value::Null,serde_json::json!([]),serde_json::json!({})]);
    let node=std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into());let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain/native-engineering-roster.ts");
    let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const m=await import(process.argv[1]);let data='';for await(const chunk of process.stdin)data+=chunk;const cases=JSON.parse(data);process.stdout.write(JSON.stringify(cases.map(v=>m.validateNativeEngineeringRoster(v,'0123456789ab','source','manifest'))));"#;
    let mut child=std::process::Command::new(node).arg("--input-type=module").arg("-e").arg(script).arg(path).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();child.stdin.take().unwrap().write_all(&serde_json::to_vec(&cases).unwrap()).unwrap();let output=child.wait_with_output().unwrap();assert!(output.status.success(),"actual TS roster oracle: {}",String::from_utf8_lossy(&output.stderr));let expected:Vec<bool>=serde_json::from_slice(&output.stdout).unwrap();let actual:Vec<bool>=cases.iter().map(|v|validate_native_engineering_roster(v,"0123456789ab","source","manifest").is_some()).collect();assert_eq!(actual,expected);
}
