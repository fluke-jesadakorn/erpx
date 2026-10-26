use crate::domain::work_admission::{BusinessRoleModules, WorkModuleCatalog};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::OnceLock;
static CATALOG: OnceLock<Result<WorkModuleCatalog, BusinessError>> = OnceLock::new();
pub fn work_module_catalog() -> Result<&'static WorkModuleCatalog, BusinessError> {
    CATALOG
        .get_or_init(|| {
            let definitions: Vec<BusinessRoleModules> = serde_json::from_str(include_str!(
                "../../resources/business-role-modules.v1.json"
            ))
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "WORK_MODULE_CATALOG_INVALID",
                    "The work module catalog is invalid.",
                )
            })?;
            WorkModuleCatalog::new(definitions)
        })
        .as_ref()
        .map_err(Clone::clone)
}
