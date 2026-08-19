use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandReceipt, EventPosition, IdempotencyKey, ProjectionMetadata, ProjectionNotReady,
    QueryConsistency,
};
use erpx_integration_contracts::RecordRef;
use erpx_integration_contracts::legal;
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    AccessEntry, AccessEntryId, ContractParty, ContractStatus, ContractTransition, DocumentVersion,
    LegalAggregate, LegalContract, LegalContractId, LegalDocument, LegalDocumentId, LegalHold,
    LegalMatter, LegalMatterId, MatterStatus, RecordLink, RecordRelation, SignatureEnvelopeId,
    SignatureRecipient, VerifiedSignatureCallback,
};

mod legal_case;
pub use legal_case::{
    LegalCaseAuthorizationPort, LegalCaseCommand, LegalCaseQueryPort, LegalCaseReceipt,
    LegalCaseUnitOfWork, LegalCaseView,
};

/// Provider-neutral submission payload. Delivery coordinates remain behind
/// the provider adapter and are resolved from the opaque recipient subjects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureProviderSubmission {
    pub envelope_id: SignatureEnvelopeId,
    pub scope: AuthorityScope,
    pub document_id: LegalDocumentId,
    pub document_version: u64,
    pub document_sha256: String,
    pub provider_policy_version: String,
    pub recipients: Vec<SignatureRecipient>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureProviderEnvelope {
    pub provider_envelope_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureEvidencePackage {
    pub provider_envelope_ref: String,
    pub evidence_object_ref: String,
    pub evidence_sha256: String,
}

#[async_trait]
pub trait ESignatureProviderPort: Send + Sync {
    async fn create_envelope(
        &self,
        request: SignatureProviderSubmission,
    ) -> Result<SignatureProviderEnvelope, PortError>;

    async fn cancel_envelope(&self, provider_envelope_ref: &str) -> Result<(), PortError>;

    async fn fetch_evidence(
        &self,
        provider_envelope_ref: &str,
    ) -> Result<SignatureEvidencePackage, PortError>;
}

/// Verification occurs in an outbound adapter before the application may
/// pass a typed callback into the Legal domain lifecycle.
#[async_trait]
pub trait SignatureCallbackVerifierPort: Send + Sync {
    async fn verify_callback(
        &self,
        provider_key: &str,
        headers: &[(String, String)],
        body: &[u8],
        received_at: DateTime<Utc>,
    ) -> Result<VerifiedSignatureCallback, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureEnvelopeView {
    pub envelope_id: SignatureEnvelopeId,
    pub version: u64,
    pub document_id: LegalDocumentId,
    pub document_version: u64,
    pub status: crate::domain::SignatureStatus,
    pub retention_until: DateTime<Utc>,
}

#[async_trait]
pub trait SignatureEnvelopeQueryPort: Send + Sync {
    /// Queries only the active projection generation. Missing generation is a
    /// degraded read and must not be treated as a neutral not-found result.
    async fn get_signature_envelope(
        &self,
        authority_scope: &AuthorityScope,
        envelope_id: SignatureEnvelopeId,
    ) -> Result<Option<SignatureEnvelopeView>, PortError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalAction {
    CreateContract,
    AddContractParty,
    TransitionContract,
    CreateDocument,
    AddDocumentVersion,
    ArchiveDocument,
    RecordDocumentAccess,
    ViewContract,
    ListContracts,
    ViewDocument,
    ListDocuments,
    ListAccessHistory,
    CreateMatter,
    LinkMatterRecord,
    PlaceLegalHold,
    ReleaseLegalHold,
    CloseMatter,
    ViewMatter,
}

impl LegalAction {
    pub const fn requires_write_entitlement(self) -> bool {
        matches!(
            self,
            Self::CreateContract
                | Self::AddContractParty
                | Self::TransitionContract
                | Self::CreateDocument
                | Self::AddDocumentVersion
                | Self::ArchiveDocument
                | Self::RecordDocumentAccess
                | Self::CreateMatter
                | Self::LinkMatterRecord
                | Self::PlaceLegalHold
                | Self::ReleaseLegalHold
                | Self::CloseMatter
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest<'a> {
    pub actor: &'a ActorAttribution,
    pub authority_scope: &'a AuthorityScope,
    pub target_scope: &'a AuthorityScope,
    pub action: LegalAction,
}

#[async_trait]
pub trait AuthorizationPort: Send + Sync {
    async fn authorize(&self, request: AuthorizationRequest<'_>) -> Result<(), PortError>;
}

#[async_trait]
pub trait EntitlementPort: Send + Sync {
    async fn ensure_legal_enabled(
        &self,
        scope: &AuthorityScope,
        write: bool,
    ) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CommandFingerprint {
    CreateContract {
        contract_id: LegalContractId,
        scope: AuthorityScope,
        reference: String,
        title: String,
        object_ref: String,
        content_sha256: String,
        retention_until: DateTime<Utc>,
    },
    AddContractParty {
        contract_id: LegalContractId,
        party: ContractParty,
    },
    TransitionContract {
        contract_id: LegalContractId,
        transition: ContractTransition,
    },
    CreateDocument {
        document_id: LegalDocumentId,
        scope: AuthorityScope,
        title: String,
    },
    AddDocumentVersion {
        document_id: LegalDocumentId,
        object_ref: String,
        content_sha256: String,
    },
    ArchiveDocument {
        document_id: LegalDocumentId,
    },
    RecordDocumentAccess {
        access_entry_id: AccessEntryId,
        document_id: LegalDocumentId,
        document_version: u64,
        action: String,
        purpose: String,
        observed_at: DateTime<Utc>,
    },
    CreateMatter {
        matter_id: LegalMatterId,
        scope: AuthorityScope,
        reference: String,
        title: String,
    },
    LinkMatterRecord {
        matter_id: LegalMatterId,
        relation: RecordRelation,
        target: RecordRef,
        target_scope: AuthorityScope,
        target_digest: String,
    },
    PlaceLegalHold {
        matter_id: LegalMatterId,
        reason: String,
    },
    ReleaseLegalHold {
        matter_id: LegalMatterId,
    },
    CloseMatter {
        matter_id: LegalMatterId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentity {
    pub command_id: CommandId,
    pub idempotency_key: IdempotencyKey,
    pub fingerprint: CommandFingerprint,
}

#[async_trait]
pub trait WriteRepository: Send + Sync {
    async fn load_contract(
        &self,
        authority_scope: &AuthorityScope,
        contract_id: LegalContractId,
    ) -> Result<Option<LegalContract>, PortError>;

    async fn load_document(
        &self,
        authority_scope: &AuthorityScope,
        document_id: LegalDocumentId,
    ) -> Result<Option<LegalDocument>, PortError>;

    async fn load_matter(
        &self,
        authority_scope: &AuthorityScope,
        matter_id: LegalMatterId,
    ) -> Result<Option<LegalMatter>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalCommit {
    pub identity: CommandIdentity,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub audit: AuditContext,
    pub expected_version: Option<u64>,
    pub aggregate: LegalAggregate,
    pub integration_events: Vec<legal::Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommit {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub aggregate: LegalAggregate,
    pub committed_events: Vec<EventPosition>,
    pub committed_at: DateTime<Utc>,
    pub replayed: bool,
}

impl StoredCommit {
    pub fn into_receipt<R>(self, response: R) -> CommandReceipt<R> {
        CommandReceipt {
            command_id: self.command_id,
            correlation_id: self.correlation_id,
            aggregate: erpx_cqrs::AggregateVersion {
                context: "legal".into(),
                aggregate_type: self.aggregate.aggregate_type().into(),
                aggregate_id: AggregateId::try_from_uuid(self.aggregate.id_uuid())
                    .expect("validated Legal aggregate identifier"),
                version: self.aggregate.version(),
            },
            committed_events: self.committed_events,
            committed_at: self.committed_at,
            replayed: self.replayed,
            response,
        }
    }
}

#[async_trait]
pub trait CommandUnitOfWork: Send + Sync {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError>;

    /// Atomically persists aggregate state, stable receipt, audit, and outbox.
    async fn commit(&self, request: LegalCommit) -> Result<StoredCommit, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractView {
    pub contract_id: LegalContractId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub reference: String,
    pub title: String,
    pub object_ref: String,
    pub content_sha256: String,
    pub retention_until: DateTime<Utc>,
    pub status: ContractStatus,
    pub parties: Vec<ContractParty>,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&LegalContract> for ContractView {
    fn from(value: &LegalContract) -> Self {
        Self {
            contract_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            reference: value.reference().into(),
            title: value.title().into(),
            object_ref: value.object_ref().into(),
            content_sha256: value.content_sha256().into(),
            retention_until: value.retention_until(),
            status: value.status(),
            parties: value.parties().to_vec(),
            created_by: value.created_by().into(),
            approved_by: value.approved_by().map(str::to_owned),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentView {
    pub document_id: LegalDocumentId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub title: String,
    pub versions: Vec<DocumentVersion>,
    pub current_version: u64,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&LegalDocument> for DocumentView {
    fn from(value: &LegalDocument) -> Self {
        Self {
            document_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            title: value.title().into(),
            versions: value.versions().to_vec(),
            current_version: value.current_version(),
            archived: value.archived(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessEntryView {
    pub access_entry_id: AccessEntryId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub document_id: LegalDocumentId,
    pub document_version: u64,
    pub actor_ref: String,
    pub action: String,
    pub purpose: String,
    pub observed_at: DateTime<Utc>,
}

impl From<&AccessEntry> for AccessEntryView {
    fn from(value: &AccessEntry) -> Self {
        Self {
            access_entry_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            document_id: value.document_id(),
            document_version: value.document_version(),
            actor_ref: value.actor_ref().into(),
            action: value.action().into(),
            purpose: value.purpose().into(),
            observed_at: value.observed_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatterView {
    pub matter_id: LegalMatterId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub reference: String,
    pub title: String,
    pub status: MatterStatus,
    pub links: Vec<RecordLink>,
    pub legal_hold: Option<LegalHold>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&LegalMatter> for MatterView {
    fn from(value: &LegalMatter) -> Self {
        Self {
            matter_id: value.id(),
            version: value.version(),
            scope: value.scope().clone(),
            reference: value.reference().into(),
            title: value.title().into(),
            status: value.status(),
            links: value.links().to_vec(),
            legal_hold: value.legal_hold().cloned(),
            created_by: value.created_by().into(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContractListFilter {
    pub status: Option<ContractStatus>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractList {
    pub contracts: Vec<ContractView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocumentListFilter {
    pub archived: Option<bool>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentList {
    pub documents: Vec<DocumentView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AccessHistoryFilter {
    pub document_id: Option<LegalDocumentId>,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessHistoryList {
    pub entries: Vec<AccessEntryView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadProjection<T> {
    pub value: T,
    pub metadata: ProjectionMetadata,
}

/// Disposable read-model access. A write-store fallback is deliberately absent.
#[async_trait]
pub trait ReadRepository: Send + Sync {
    async fn get_contract(
        &self,
        authority_scope: &AuthorityScope,
        contract_id: LegalContractId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ContractView>>, PortError>;

    async fn list_contracts(
        &self,
        authority_scope: &AuthorityScope,
        filter: &ContractListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ContractList>, PortError>;

    async fn get_document(
        &self,
        authority_scope: &AuthorityScope,
        document_id: LegalDocumentId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<DocumentView>>, PortError>;

    async fn list_documents(
        &self,
        authority_scope: &AuthorityScope,
        filter: &DocumentListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<DocumentList>, PortError>;

    async fn list_access_history(
        &self,
        authority_scope: &AuthorityScope,
        filter: &AccessHistoryFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<AccessHistoryList>, PortError>;

    async fn get_matter(
        &self,
        authority_scope: &AuthorityScope,
        matter_id: LegalMatterId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<MatterView>>, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Legal operation is denied")]
    Denied,
    #[error("Legal entitlement is unavailable")]
    EntitlementDenied,
    #[error("Legal record was not found")]
    NotFound,
    #[error("Legal aggregate conflict (expected {expected}, actual {actual})")]
    Conflict { expected: u64, actual: u64 },
    #[error("Legal idempotency identity conflicts")]
    IdempotencyConflict,
    #[error("Legal projection is not ready")]
    NotReady(ProjectionNotReady),
    #[error("Legal storage is unavailable")]
    Unavailable,
    #[error("Legal adapter failed")]
    Internal,
}
