//! Real outer assembly and explicit module contract checks.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use erpx_server_runtime::public::module_contracts::{ModuleCatalog, validate};
use tower::ServiceExt;
const MANIFEST: &str = include_str!("../../../contracts/modules/backend-modules.json");
fn catalog() -> ModuleCatalog {
    serde_json::from_str(MANIFEST).unwrap()
}
#[tokio::test]
async fn field_selection_reports_its_actual_closure_without_other_business_owners() {
    let response = erpx_service::module_assembly::assemble()
        .unwrap()
        .oneshot(
            Request::builder()
                .uri("/api/modules")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let ids = body["data"]["compiledModules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        ids.contains("erpx-server-field-service"),
        cfg!(feature = "field-service")
    );
    if cfg!(all(
        feature = "field-service",
        not(feature = "existing-business")
    )) {
        assert!(ids.contains("erpx-field-service-composition"));
        assert!(ids.contains("erpx-server-finance"));
        assert!(!ids.contains("erpx-server-legal"));
        assert!(!ids.contains("erpx-server-maintenance"));
        // Initial Count preserves the scheduled-module release gate through
        // PMS/Platform, and Records owns its evidence planning.
        assert!(ids.contains("erpx-server-pms"));
        assert!(ids.contains("erpx-server-platform-administration"));
        assert!(ids.contains("erpx-server-records"));
    }
}
#[tokio::test]
async fn real_outer_router_keeps_core_health_and_optional_route() {
    let router = erpx_service::module_assembly::assemble().unwrap();
    for (path, status) in [
        ("/api/health", StatusCode::OK),
        ("/api/ready", StatusCode::SERVICE_UNAVAILABLE),
        ("/api/modules", StatusCode::OK),
        (
            "/api/modules/example",
            if cfg!(feature = "module-example") {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            },
        ),
    ] {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{path}");
    }
}
#[test]
fn required_api_and_event_versions_and_external_boundary_are_checked() {
    let core = ["erpx-server-shared", "erpx-server-runtime", "erpx-example"];
    let mut c = catalog();
    assert!(validate(&c, &core).is_ok());
    assert_eq!(
        validate(&c, &["erpx-server-runtime", "erpx-example"]),
        Err("MODULE_CONTRACT_MISSING")
    );
    let example = c
        .modules
        .iter_mut()
        .find(|m| m.id == "erpx-example")
        .unwrap();
    example.requires[0].range = ">=9.0.0".into();
    assert_eq!(validate(&c, &core), Err("MODULE_CONTRACT_INCOMPATIBLE"));
    let mut c = catalog();
    c.modules
        .iter_mut()
        .find(|m| m.id == "erpx-example")
        .unwrap()
        .requires[0]
        .range = "*".into();
    assert_eq!(validate(&c, &core), Err("MODULE_RANGE_INVALID"));
    let mut c = catalog();
    let example = c
        .modules
        .iter_mut()
        .find(|m| m.id == "erpx-example")
        .unwrap();
    example.requires_events.push(
        erpx_server_runtime::public::module_contracts::RequiredContract {
            name: "erpx.example.observed".into(),
            range: ">=2.0.0".into(),
        },
    );
    assert_eq!(validate(&c, &core), Err("MODULE_CONTRACT_INCOMPATIBLE"));
    let mut c = catalog();
    c.modules
        .iter_mut()
        .find(|m| m.id == "erpx-example")
        .unwrap()
        .provides[0]
        .version = "2.0.0".into();
    assert_eq!(validate(&c, &core), Err("MODULE_CONTRACT_INCOMPATIBLE"));
    let mut c = catalog();
    let external:erpx_server_runtime::public::module_contracts::ExternalDescriptor=serde_json::from_value(serde_json::json!({"id":"external","api":[{"name":"external.api","version":"1.0.0"}],"events":[{"name":"external.event","version":"2.0.0"}]})).unwrap();
    c.external.push(external);
    let example = c
        .modules
        .iter_mut()
        .find(|m| m.id == "erpx-example")
        .unwrap();
    example.requires.push(
        erpx_server_runtime::public::module_contracts::RequiredContract {
            name: "external.api".into(),
            range: ">=1.0.0, <2.0.0".into(),
        },
    );
    example.requires_events.push(
        erpx_server_runtime::public::module_contracts::RequiredContract {
            name: "external.event".into(),
            range: ">=2.0.0, <3.0.0".into(),
        },
    );
    assert!(validate(&c, &core).is_ok());
    let mut impersonation = c.clone();
    impersonation.external[0].api[0].name = "erpx-server-shared::public".into();
    assert_eq!(
        validate(&impersonation, &core),
        Err("MODULE_DESCRIPTOR_INVALID")
    );
    for prohibited in ["databaseBinding", "transactionHandle", "localFactory"] {
        let mut json =
            serde_json::to_value(serde_json::json!({"id":"external","api":[],"events":[]}))
                .unwrap();
        json[prohibited] = serde_json::json!("not-allowed");
        assert!(
            serde_json::from_value::<
                erpx_server_runtime::public::module_contracts::ExternalDescriptor,
            >(json)
            .is_err()
        );
    }
}

#[test]
fn field_composition_cannot_validate_without_its_owner_contracts() {
    let c = catalog();
    assert_eq!(
        validate(
            &c,
            &[
                "erpx-server-shared",
                "erpx-server-runtime",
                "erpx-field-service-composition",
            ],
        ),
        Err("MODULE_CONTRACT_MISSING")
    );
}
#[test]
fn declared_owner_contracts_and_dependencies_match_actual_workspace_packages() {
    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--no-deps",
            "--format-version=1",
        ])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(output.status.success());
    let meta: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = meta["packages"].as_array().unwrap();
    let c = catalog();
    let mut pending = vec!["erpx-service".to_owned()];
    let mut actual = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop() {
        let pkg = packages
            .iter()
            .find(|p| p["name"] == name)
            .expect("workspace package");
        for dep in pkg["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["kind"].is_null())
        {
            let name = dep["name"].as_str().unwrap();
            if name.starts_with("erpx-server-") && actual.insert(name.to_owned()) {
                pending.push(name.to_owned());
            }
        }
    }
    let declared = c
        .modules
        .iter()
        .filter(|m| m.id.starts_with("erpx-server-"))
        .map(|m| m.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        declared, actual,
        "catalog must cover the actual owner dependency closure"
    );
    for module in c
        .modules
        .iter()
        .filter(|m| m.id.starts_with("erpx-server-"))
    {
        let pkg = packages
            .iter()
            .find(|p| p["name"] == module.id)
            .expect("registered owner must be actual workspace package");
        assert_eq!(pkg["version"], module.version);
        let public = module
            .provides
            .iter()
            .find(|p| p.name == format!("{}::public", module.id))
            .expect("explicit public contract");
        assert_eq!(public.version, module.version);
        for contract in &module.provides {
            assert!(
                std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
                    .join(&contract.source)
                    .is_file()
            );
        }
        let expected = pkg["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["kind"].is_null())
            .filter_map(|d| d["name"].as_str())
            .filter(|name| name.starts_with("erpx-server-"))
            .map(|name| format!("{name}::public"))
            .collect::<std::collections::BTreeSet<_>>();
        let declared = module
            .requires
            .iter()
            .map(|r| r.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            declared, expected,
            "{} dependency contract drift",
            module.id
        );
    }
    let composition = c
        .modules
        .iter()
        .find(|m| m.id == "erpx-existing-business-composition")
        .expect("technical compatibility composition");
    let service = packages
        .iter()
        .find(|p| p["name"] == "erpx-service")
        .unwrap();
    for dependency in service["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"].is_null())
        .filter_map(|d| d["name"].as_str())
        .filter(|name| {
            name.starts_with("erpx-server-")
                && !["erpx-server-shared", "erpx-server-runtime"].contains(name)
        })
    {
        assert!(
            composition
                .requires
                .iter()
                .any(|r| r.name == format!("{dependency}::public")),
            "composition contract for {dependency}"
        );
    }
    for public in [
        "erpx-server-field-service::public::reservation_read",
        "erpx-server-workflow::public::commands",
        "erpx-server-commercial::public::purchases",
    ] {
        assert!(
            composition.requires.iter().any(|r| r.name == public),
            "specific owner port {public}"
        );
    }
}
