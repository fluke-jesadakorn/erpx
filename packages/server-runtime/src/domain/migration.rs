use serde::Serialize;
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReadiness {
    pub ready: bool,
    pub pending_owners: Vec<String>,
    pub unknown_owners: Vec<String>,
    pub duplicate_owners: Vec<String>,
    pub backend_migrated: bool,
    pub registered_business_owners: Vec<String>,
    pub source_acceptance: bool,
}
