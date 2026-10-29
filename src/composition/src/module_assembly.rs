//! Build-time outer assembly: descriptors validate before a router is exposed.
use axum::{Json, Router, routing::get};
use erpx_server_runtime::public::module_contracts::{ModuleCatalog, validate};
const MANIFEST: &str = include_str!("../../../contracts/modules/backend-modules.json");
const CORE: &[&str] = &["erpx-server-shared", "erpx-server-runtime"];
#[cfg(feature = "existing-business")]
const BUSINESS: &[&str] = &[
    "erpx-server-assets",
    "erpx-server-collaboration",
    "erpx-server-commerce",
    "erpx-server-commercial",
    "erpx-server-field-service",
    "erpx-server-files",
    "erpx-server-finance",
    "erpx-server-identity-access",
    "erpx-server-inventory",
    "erpx-server-legal",
    "erpx-server-maintenance",
    "erpx-server-manage-erpx",
    "erpx-server-organization",
    "erpx-server-people-payroll",
    "erpx-server-platform-administration",
    "erpx-server-pms",
    "erpx-server-quality",
    "erpx-server-records",
    "erpx-server-support",
    "erpx-server-workflow",
    "erpx-existing-business-composition",
];
#[cfg(all(feature = "field-service", not(feature = "existing-business")))]
const FIELD: &[&str] = &[
    "erpx-server-assets",
    "erpx-server-collaboration",
    "erpx-server-commerce",
    "erpx-server-commercial",
    "erpx-server-field-service",
    "erpx-server-files",
    "erpx-server-finance",
    "erpx-server-identity-access",
    "erpx-server-inventory",
    "erpx-server-manage-erpx",
    "erpx-server-organization",
    "erpx-server-people-payroll",
    "erpx-server-platform-administration",
    "erpx-server-pms",
    "erpx-server-quality",
    "erpx-server-records",
    "erpx-server-support",
    "erpx-server-workflow",
    "erpx-field-service-composition",
];
fn compiled_modules() -> Result<Vec<serde_json::Value>, &'static str> {
    let catalog: ModuleCatalog =
        serde_json::from_str(MANIFEST).map_err(|_| "MODULE_CATALOG_INVALID")?;
    let enabled = CORE.to_vec();
    #[cfg(feature = "existing-business")]
    let enabled = {
        let mut enabled = enabled;
        enabled.extend_from_slice(BUSINESS);
        enabled
    };
    #[cfg(all(feature = "field-service", not(feature = "existing-business")))]
    let enabled = {
        let mut enabled = enabled;
        enabled.extend_from_slice(FIELD);
        enabled
    };
    #[cfg(feature = "module-example")]
    let enabled = {
        let mut enabled = enabled;
        enabled.push("erpx-example");
        enabled
    };
    for id in &enabled {
        let m = catalog
            .modules
            .iter()
            .find(|m| m.id == *id)
            .ok_or("MODULE_SELECTION_INVALID")?;
        let expected = if CORE.contains(id) {
            "core"
        } else if *id == "erpx-example" {
            "example"
        } else {
            "existing-business"
        };
        if m.group != expected {
            return Err("MODULE_SELECTION_INVALID");
        }
    }
    validate(&catalog, &enabled)?;
    let listed = catalog
        .modules
        .iter()
        .filter(|m| enabled.contains(&m.id.as_str()))
        .map(|m| serde_json::json!({"id":m.id,"version":m.version,"group":m.group}))
        .collect::<Vec<_>>();
    Ok(listed)
}
/// Validate compiled registrations without mounting routes or granting readiness.
pub fn validate_build() -> Result<(), &'static str> {
    compiled_modules().map(|_| ())
}
/// Returned descriptors describe compiled registrations, not business readiness.
pub fn assemble() -> Result<Router, &'static str> {
    let listed = compiled_modules()?;
    let routes = if cfg!(feature = "module-example") {
        vec!["/api/modules/example"]
    } else {
        vec![]
    };
    let info = serde_json::json!({"data":{"compiledModules":listed,"registeredExampleRoutes":routes,"scope":"build-registration","businessReady":false},"acceptance":false});
    let router = erpx_server_runtime::public::foundation_router()
        .route("/api/modules", get(move || async move { Json(info) }));
    #[cfg(feature = "module-example")]
    let router = router.merge(crate::module_example::router());
    Ok(router)
}
