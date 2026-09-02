//! Universal `@` record reference graph — shared kernel for 31 contexts.
//!
//! Storage primitive is an ordered, deduped, append-only list with soft cap 50
//! per host (Q9=C). Edge kinds are role labels where module needs them.
//! Strictly additive to existing RLS/CQRS/RBAC (ADR-001/013, ADR-002, ADR-004).

use serde::{Deserialize, Serialize};

pub const MAX_REFERENCE_EDGES_PER_HOST: usize = 50;
pub const MAX_FIELD_EVIDENCE_REFS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceEdgeKind {
    RelatesTo,
    DependsOn,
    Blocks,
    BilledBy,
    Fulfills,
    AuthoredWith,
    Implements,
    Supersedes,
    DerivedFrom,
    EvidenceFor,
}

impl ReferenceEdgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RelatesTo => "relates_to",
            Self::DependsOn => "depends_on",
            Self::Blocks => "blocks",
            Self::BilledBy => "billed_by",
            Self::Fulfills => "fulfills",
            Self::AuthoredWith => "authored_with",
            Self::Implements => "implements",
            Self::Supersedes => "supersedes",
            Self::DerivedFrom => "derived_from",
            Self::EvidenceFor => "evidence_for",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "relates_to" => Some(Self::RelatesTo),
            "depends_on" => Some(Self::DependsOn),
            "blocks" => Some(Self::Blocks),
            "billed_by" => Some(Self::BilledBy),
            "fulfills" => Some(Self::Fulfills),
            "authored_with" => Some(Self::AuthoredWith),
            "implements" => Some(Self::Implements),
            "supersedes" => Some(Self::Supersedes),
            "derived_from" => Some(Self::DerivedFrom),
            "evidence_for" => Some(Self::EvidenceFor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordResolutionState {
    Resolved,
    Stale,
    Degraded,
    Offline,
    Tombstone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordReferenceEdge {
    pub host: crate::reference::RecordRef,
    pub target: crate::reference::RecordRef,
    pub kind: ReferenceEdgeKind,
    pub ordinal: u8,
    pub created_by: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolution: RecordResolutionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordReferenceEdgeView {
    pub edge: RecordReferenceEdge,
    pub display_label: Option<String>,
    pub canonical_route: String,
}
