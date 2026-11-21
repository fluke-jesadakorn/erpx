use crate::domain::permission_catalog::{PermissionCatalog, PermissionMetadata};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::sync::OnceLock;
static CATALOG: OnceLock<Result<PermissionCatalog, BusinessError>> = OnceLock::new();
pub fn permission_catalog() -> Result<&'static PermissionCatalog, BusinessError> {
    CATALOG
        .get_or_init(|| {
            let definitions: Vec<PermissionMetadata> = serde_json::from_str(include_str!(
                "../../resources/permissions.v1.json"
            ))
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "PERMISSION_CATALOG_INVALID",
                    "The permission catalog is invalid.",
                )
            })?;
            PermissionCatalog::new(definitions)
        })
        .as_ref()
        .map_err(Clone::clone)
}
