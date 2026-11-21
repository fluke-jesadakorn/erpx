use erpx_server_shared::public::{CommandKey, CompanyId, HoldingId, RecordId, UserId};

use super::pdf_verification::{TrustMaterialKind, TrustMaterialState};

/// Scope facts supplied by composition. They grant no permission on their own.
#[derive(Clone, Debug)]
pub struct LegalTrustActor {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub user_id: UserId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
}

#[derive(Clone, Debug)]
pub struct LegalTrustRecord {
    pub id: RecordId,
    pub company_id: CompanyId,
    pub version: u64,
    pub kind: TrustMaterialKind,
    pub state: TrustMaterialState,
    pub created_by: UserId,
    pub checksum_sha256: String,
}

#[derive(Clone, Debug)]
pub enum LegalTrustCommand {
    Add {
        kind: TrustMaterialKind,
        name: String,
        source: String,
        content: String,
    },
    Approve {
        id: RecordId,
        version: u64,
    },
    Retire {
        id: RecordId,
        version: u64,
    },
}

impl LegalTrustCommand {
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::Add { .. } => "trust.add",
            Self::Approve { .. } => "trust.approve",
            Self::Retire { .. } => "trust.retire",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LegalTrustReceipt {
    pub id: CompanyId,
    pub version: u32,
}

pub struct LegalTrustCommandRequest<'a> {
    pub actor: &'a LegalTrustActor,
    pub key: &'a CommandKey,
    pub command: &'a LegalTrustCommand,
}

/// Canonical public material, with its checksum computed by a concrete inspector.
#[derive(Clone, Debug)]
pub struct InspectedTrustMaterial {
    pub content: Vec<u8>,
    pub checksum_sha256: String,
}
