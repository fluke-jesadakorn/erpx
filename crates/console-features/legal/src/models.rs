//! Target-neutral Legal UI models.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractPartyItem {
    pub party_id: String,
    pub role: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractItem {
    pub contract_id: String,
    pub version: u64,
    pub scope: String,
    pub reference: String,
    pub title: String,
    pub object_ref: String,
    pub content_sha256: String,
    pub retention_until: String,
    pub status: String,
    pub parties: Vec<ContractPartyItem>,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractItems {
    pub contracts: Vec<ContractItem>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentVersionItem {
    pub version: u64,
    pub object_ref: String,
    pub content_sha256: String,
    pub created_at: String,
    pub created_by: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentItem {
    pub document_id: String,
    pub version: u64,
    pub scope: String,
    pub title: String,
    pub versions: Vec<DocumentVersionItem>,
    pub current_version: u64,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentItems {
    pub documents: Vec<DocumentItem>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessEntryItem {
    pub access_entry_id: String,
    pub version: u64,
    pub scope: String,
    pub document_id: String,
    pub document_version: u64,
    pub actor_ref: String,
    pub action: String,
    pub purpose: String,
    pub observed_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessEntryItems {
    pub entries: Vec<AccessEntryItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLinkItem {
    pub relation: String,
    pub target_context: String,
    pub target_aggregate_kind: String,
    pub target_aggregate_id: String,
    pub target_aggregate_version: u64,
    pub target_scope: String,
    pub target_digest: String,
    pub linked_by: String,
    pub linked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalHoldItem {
    pub active: bool,
    pub reason: String,
    pub placed_by: String,
    pub placed_at: String,
    pub released_by: Option<String>,
    pub released_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatterItem {
    pub matter_id: String,
    pub version: u64,
    pub scope: String,
    pub reference: String,
    pub title: String,
    pub status: String,
    pub links: Vec<RecordLinkItem>,
    pub legal_hold: Option<LegalHoldItem>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}
