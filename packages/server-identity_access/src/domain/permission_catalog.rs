use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAvailability {
    Live,
    Preview,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionNamespace {
    Customer,
    Platform,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionMetadata {
    pub id: String,
    pub label: String,
    pub module: String,
    pub roles: Vec<String>,
    pub conditions: Vec<String>,
    pub risk: String,
    pub availability: PermissionAvailability,
    pub namespace: PermissionNamespace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permitted_scopes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub identity_based: bool,
}
#[derive(Clone, Debug)]
pub struct PermissionCatalog {
    definitions: Vec<PermissionMetadata>,
}
impl PermissionCatalog {
    pub fn new(definitions: Vec<PermissionMetadata>) -> Result<Self, BusinessError> {
        let mut ids = BTreeSet::new();
        if definitions.is_empty()
            || definitions.iter().any(|d| {
                d.id.is_empty()
                    || d.id.len() > 200
                    || !ids.insert(&d.id)
                    || !matches!(d.risk.as_str(), "normal" | "privileged")
                    || d.permitted_scopes.as_ref().is_some_and(|scopes| {
                        scopes.iter().any(|scope| {
                            !matches!(
                                scope.as_str(),
                                "holding" | "branch" | "department" | "platform"
                            )
                        })
                    })
            })
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PERMISSION_CATALOG_INVALID",
                "The permission catalog is invalid.",
            ));
        }
        Ok(Self { definitions })
    }
    pub fn definitions(&self) -> &[PermissionMetadata] {
        &self.definitions
    }
    pub fn definition(&self, id: &str) -> Option<&PermissionMetadata> {
        self.definitions.iter().find(|p| p.id == id)
    }
    /// The current published IAM list is mandatory; this never falls back to a role name.
    pub fn allows_current_permissions(
        &self,
        id: &str,
        workspace_kind: &str,
        identity_present: bool,
        permissions: &BTreeSet<String>,
    ) -> bool {
        self.definition(id).is_some_and(|p| {
            p.availability == PermissionAvailability::Live
                && p.permitted_scopes
                    .as_ref()
                    .is_none_or(|scopes| scopes.iter().any(|scope| scope == workspace_kind))
                && if p.identity_based {
                    identity_present
                } else {
                    permissions.contains(id)
                }
        })
    }
}
