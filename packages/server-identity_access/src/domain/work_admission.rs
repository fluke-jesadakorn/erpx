use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessRoleModules {
    pub id: String,
    pub modules: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct WorkModuleCatalog {
    definitions: Vec<BusinessRoleModules>,
}
impl WorkModuleCatalog {
    pub fn new(definitions: Vec<BusinessRoleModules>) -> Result<Self, BusinessError> {
        let mut ids = BTreeSet::new();
        if definitions.is_empty()
            || definitions.iter().any(|d| {
                d.id.is_empty()
                    || !ids.insert(&d.id)
                    || d.modules.is_empty()
                    || d.modules.iter().any(String::is_empty)
            })
        {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "WORK_MODULE_CATALOG_INVALID",
                "The work module catalog is invalid.",
            ));
        }
        Ok(Self { definitions })
    }
    pub fn definitions(&self) -> &[BusinessRoleModules] {
        &self.definitions
    }
    pub fn effective_modules(
        &self,
        role: &str,
        assigned: &[String],
        permissions: &[String],
        beta: bool,
    ) -> Vec<String> {
        let mut modules = if beta {
            Vec::new()
        } else if role.starts_with("custom_") {
            unique(permissions.iter().filter_map(|p| {
                p.strip_prefix("module.")
                    .map(|p| p.split('.').next().unwrap_or_default().to_owned())
            }))
        } else if let Some(definition) = self.definitions.iter().find(|d| d.id == role) {
            assigned
                .iter()
                .filter(|id| definition.modules.contains(id))
                .cloned()
                .collect()
        } else {
            assigned.to_vec()
        };
        if permissions
            .iter()
            .any(|p| p == "module.people_payroll.read")
        {
            modules.push("people_payroll".into());
            unique(modules)
        } else {
            modules.retain(|id| id != "people_payroll");
            modules
        }
    }
}
fn unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}
/// Trusted composition supplies the actual matched route, without a leading slash.
/// This beta restriction never supplies a permission or bypasses record policy.
pub fn beta_destination_allowed(destination: &str, api: bool) -> bool {
    if destination.chars().any(char::is_control) {
        return false;
    }
    let s: Vec<&str> = destination.split('/').collect();
    let prefix = |base: &str| destination == base || destination.starts_with(&format!("{base}/"));
    let one_id = |root: &str| s.len() == 2 && s[0] == root && !s[1].is_empty();
    if !api {
        return destination == "people/organization"
            || matches!(
                destination,
                "home" | "feed" | "work" | "people" | "settings" | "inbox"
            )
            || ["requests", "handbook", "expenses", "staff-tasks"]
                .iter()
                .any(|root| destination == *root || one_id(root))
            || one_id("posts");
    }
    if [
        "people/organization",
        "messaging",
        "beta",
        "billing",
        "requests",
        "service-teams",
        "handbook",
        "ai-providers",
        "expenses",
        "staff-tasks",
        "branding",
    ]
    .iter()
    .any(|base| prefix(base))
    {
        return true;
    }
    if matches!(
        destination,
        "work/summary" | "work/card-preferences" | "assignments/active" | "companies/active"
    ) {
        return true;
    }
    match s.as_slice() {
        ["flows", "expense" | "request" | "staff_task", id] => !id.is_empty(),
        ["people", "directory"] => true,
        ["people", "directory", id] => !id.is_empty(),
        ["people", "directory", id, "status" | "grant" | "revoke"] => !id.is_empty(),
        ["posts", id] => !id.is_empty(),
        [
            "posts",
            id,
            "task" | "task-source" | "ai-drafts" | "revisions",
        ] => !id.is_empty(),
        ["approvals", id, "reroute"] => !id.is_empty(),
        ["staff", "people" | "settings" | "levels" | "notifications"] => true,
        ["staff", "levels", id] => !id.is_empty(),
        ["staff", "engagements", id, "level"]
        | ["staff", "departments", id, "head"]
        | ["staff", "notifications", id, "read"] => !id.is_empty(),
        ["uploads"] => true,
        ["uploads", id, "content"] | ["attachments", id, "content" | "derivatives"] => {
            !id.is_empty()
        }
        _ => false,
    }
}
