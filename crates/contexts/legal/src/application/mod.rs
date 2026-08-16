use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::RecordRef;
use erpx_kernel::{Clock, CorrelationId, ErrorCategory, ErrorContext, KernelError, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        AccessEntry, AccessEntryId, ContractParty, ContractTransition, LegalAggregate,
        LegalContract, LegalContractId, LegalDocument, LegalDocumentId, LegalDomainError,
        LegalMatter, LegalMatterId, RecordRelation,
    },
    integration,
    ports::{
        AccessEntryView, AccessHistoryFilter, AccessHistoryList, AuthorizationPort,
        AuthorizationRequest, CommandFingerprint, CommandIdentity, CommandUnitOfWork, ContractList,
        ContractListFilter, ContractView, DocumentList, DocumentListFilter, DocumentView,
        EntitlementPort, LegalAction, LegalCommit, MatterView, PortError, ReadRepository,
        StoredCommit, WriteRepository,
    },
};

mod legal_case;
pub use legal_case::LegalCaseCommandHandler;

pub const MAX_LEGAL_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateContract {
    pub contract_id: LegalContractId,
    pub scope: TenantScope,
    pub reference: String,
    pub title: String,
    pub object_ref: String,
    pub content_sha256: String,
    pub retention_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddContractParty {
    pub contract_id: LegalContractId,
    pub party: ContractParty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionContract {
    pub contract_id: LegalContractId,
    pub transition: ContractTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateDocument {
    pub document_id: LegalDocumentId,
    pub scope: TenantScope,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddDocumentVersion {
    pub document_id: LegalDocumentId,
    pub object_ref: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveDocument {
    pub document_id: LegalDocumentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordDocumentAccess {
    pub access_entry_id: AccessEntryId,
    pub document_id: LegalDocumentId,
    pub document_version: u64,
    pub action: String,
    pub purpose: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateMatter {
    pub matter_id: LegalMatterId,
    pub scope: TenantScope,
    pub reference: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkMatterRecord {
    pub matter_id: LegalMatterId,
    pub relation: RecordRelation,
    pub target: RecordRef,
    pub target_scope: TenantScope,
    pub target_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceLegalHold {
    pub matter_id: LegalMatterId,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseLegalHold {
    pub matter_id: LegalMatterId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseMatter {
    pub matter_id: LegalMatterId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum LegalCommand {
    CreateContract(CreateContract),
    AddContractParty(AddContractParty),
    TransitionContract(TransitionContract),
    CreateDocument(CreateDocument),
    AddDocumentVersion(AddDocumentVersion),
    ArchiveDocument(ArchiveDocument),
    RecordDocumentAccess(RecordDocumentAccess),
    CreateMatter(CreateMatter),
    LinkMatterRecord(LinkMatterRecord),
    PlaceLegalHold(PlaceLegalHold),
    ReleaseLegalHold(ReleaseLegalHold),
    CloseMatter(CloseMatter),
}

impl LegalCommand {
    const fn action(&self) -> LegalAction {
        match self {
            Self::CreateContract(_) => LegalAction::CreateContract,
            Self::AddContractParty(_) => LegalAction::AddContractParty,
            Self::TransitionContract(_) => LegalAction::TransitionContract,
            Self::CreateDocument(_) => LegalAction::CreateDocument,
            Self::AddDocumentVersion(_) => LegalAction::AddDocumentVersion,
            Self::ArchiveDocument(_) => LegalAction::ArchiveDocument,
            Self::RecordDocumentAccess(_) => LegalAction::RecordDocumentAccess,
            Self::CreateMatter(_) => LegalAction::CreateMatter,
            Self::LinkMatterRecord(_) => LegalAction::LinkMatterRecord,
            Self::PlaceLegalHold(_) => LegalAction::PlaceLegalHold,
            Self::ReleaseLegalHold(_) => LegalAction::ReleaseLegalHold,
            Self::CloseMatter(_) => LegalAction::CloseMatter,
        }
    }

    const fn is_create(&self) -> bool {
        matches!(
            self,
            Self::CreateContract(_)
                | Self::CreateDocument(_)
                | Self::RecordDocumentAccess(_)
                | Self::CreateMatter(_)
        )
    }

    fn fingerprint(&self) -> CommandFingerprint {
        match self {
            Self::CreateContract(value) => CommandFingerprint::CreateContract {
                contract_id: value.contract_id,
                scope: value.scope.clone(),
                reference: value.reference.clone(),
                title: value.title.clone(),
                object_ref: value.object_ref.clone(),
                content_sha256: value.content_sha256.clone(),
                retention_until: value.retention_until,
            },
            Self::AddContractParty(value) => CommandFingerprint::AddContractParty {
                contract_id: value.contract_id,
                party: value.party.clone(),
            },
            Self::TransitionContract(value) => CommandFingerprint::TransitionContract {
                contract_id: value.contract_id,
                transition: value.transition,
            },
            Self::CreateDocument(value) => CommandFingerprint::CreateDocument {
                document_id: value.document_id,
                scope: value.scope.clone(),
                title: value.title.clone(),
            },
            Self::AddDocumentVersion(value) => CommandFingerprint::AddDocumentVersion {
                document_id: value.document_id,
                object_ref: value.object_ref.clone(),
                content_sha256: value.content_sha256.clone(),
            },
            Self::ArchiveDocument(value) => CommandFingerprint::ArchiveDocument {
                document_id: value.document_id,
            },
            Self::RecordDocumentAccess(value) => CommandFingerprint::RecordDocumentAccess {
                access_entry_id: value.access_entry_id,
                document_id: value.document_id,
                document_version: value.document_version,
                action: value.action.clone(),
                purpose: value.purpose.clone(),
                observed_at: value.observed_at,
            },
            Self::CreateMatter(value) => CommandFingerprint::CreateMatter {
                matter_id: value.matter_id,
                scope: value.scope.clone(),
                reference: value.reference.clone(),
                title: value.title.clone(),
            },
            Self::LinkMatterRecord(value) => CommandFingerprint::LinkMatterRecord {
                matter_id: value.matter_id,
                relation: value.relation,
                target: value.target.clone(),
                target_scope: value.target_scope.clone(),
                target_digest: value.target_digest.clone(),
            },
            Self::PlaceLegalHold(value) => CommandFingerprint::PlaceLegalHold {
                matter_id: value.matter_id,
                reason: value.reason.clone(),
            },
            Self::ReleaseLegalHold(value) => CommandFingerprint::ReleaseLegalHold {
                matter_id: value.matter_id,
            },
            Self::CloseMatter(value) => CommandFingerprint::CloseMatter {
                matter_id: value.matter_id,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetContract {
    pub contract_id: LegalContractId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListContracts {
    pub filter: ContractListFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetDocument {
    pub document_id: LegalDocumentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListDocuments {
    pub filter: DocumentListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListAccessHistory {
    pub filter: AccessHistoryFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetMatter {
    pub matter_id: LegalMatterId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum LegalQuery {
    GetContract(GetContract),
    ListContracts(ListContracts),
    GetDocument(GetDocument),
    ListDocuments(ListDocuments),
    ListAccessHistory(ListAccessHistory),
    GetMatter(GetMatter),
}

impl LegalQuery {
    const fn action(&self) -> LegalAction {
        match self {
            Self::GetContract(_) => LegalAction::ViewContract,
            Self::ListContracts(_) => LegalAction::ListContracts,
            Self::GetDocument(_) => LegalAction::ViewDocument,
            Self::ListDocuments(_) => LegalAction::ListDocuments,
            Self::ListAccessHistory(_) => LegalAction::ListAccessHistory,
            Self::GetMatter(_) => LegalAction::ViewMatter,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum LegalMutationResult {
    Contract(ContractView),
    Document(DocumentView),
    AccessEntry(AccessEntryView),
    Matter(MatterView),
}

impl From<&LegalAggregate> for LegalMutationResult {
    fn from(value: &LegalAggregate) -> Self {
        match value {
            LegalAggregate::Contract(value) => Self::Contract(value.into()),
            LegalAggregate::Document(value) => Self::Document(value.into()),
            LegalAggregate::AccessEntry(value) => Self::AccessEntry(value.into()),
            LegalAggregate::Matter(value) => Self::Matter(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum LegalQueryResponse {
    Contract(Option<ContractView>),
    Contracts(ContractList),
    Document(Option<DocumentView>),
    Documents(DocumentList),
    AccessHistory(AccessHistoryList),
    Matter(Option<MatterView>),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LegalApplicationError {
    #[error(transparent)]
    Domain(#[from] LegalDomainError),
    #[error("command scope does not contain the Legal record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("Legal aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("the requested document version does not exist")]
    DocumentVersionNotFound,
    #[error("Legal list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct LegalCommandHandler<W, U, A, E, C> {
    write: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> LegalCommandHandler<W, U, A, E, C> {
    pub const fn new(
        write: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        clock: C,
    ) -> Self {
        Self {
            write,
            unit_of_work,
            authorization,
            entitlement,
            clock,
        }
    }
}

impl<W, U, A, E, C> LegalCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: LegalAction,
    ) -> Result<(), LegalApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(LegalApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_legal_enabled(target_scope, action.requires_write_entitlement())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target_scope,
                action,
            })
            .await?;
        Ok(())
    }

    async fn replay(
        &self,
        identity: &CommandIdentity,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &TenantScope,
        action: LegalAction,
    ) -> Result<Option<StoredCommit>, LegalApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(actor, authority_scope, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    async fn existing(
        &self,
        aggregate: &LegalAggregate,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &TenantScope,
        action: LegalAction,
        expected: Option<u64>,
    ) -> Result<u64, LegalApplicationError> {
        self.preflight(actor, authority_scope, aggregate.scope(), action)
            .await?;
        let expected = expected.ok_or(LegalApplicationError::MissingExpectedVersion)?;
        if expected != aggregate.version() {
            return Err(LegalApplicationError::VersionConflict {
                expected,
                actual: aggregate.version(),
            });
        }
        Ok(expected)
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<LegalCommand> for LegalCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = LegalMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<LegalCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.command.action();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: envelope.command.fingerprint(),
        };
        if let Some(stored) = self
            .replay(&identity, &envelope.actor, &envelope.tenant_scope, action)
            .await
            .map_err(|error| to_cqrs(error, correlation))?
        {
            let response = LegalMutationResult::from(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }
        if envelope.command.is_create() && envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                LegalApplicationError::UnexpectedCreateVersion,
                correlation,
            ));
        }

        let now = self.clock.now();
        let actor_ref = envelope.actor.principal_ref.clone();
        let (mut aggregate, expected_version) = match envelope.command {
            LegalCommand::CreateContract(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    &value.scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let aggregate = LegalContract::create(
                    value.contract_id,
                    value.scope,
                    value.reference,
                    value.title,
                    value.object_ref,
                    value.content_sha256,
                    value.retention_until,
                    actor_ref,
                    now,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (LegalAggregate::Contract(aggregate), None)
            }
            LegalCommand::AddContractParty(value) => {
                let contract = self
                    .write
                    .load_contract(&envelope.tenant_scope, value.contract_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Contract(contract);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Contract(contract) = &mut aggregate else {
                    unreachable!()
                };
                contract
                    .add_party(value.party, now)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            LegalCommand::TransitionContract(value) => {
                let contract = self
                    .write
                    .load_contract(&envelope.tenant_scope, value.contract_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Contract(contract);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Contract(contract) = &mut aggregate else {
                    unreachable!()
                };
                contract
                    .transition(value.transition, &actor_ref, now)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            LegalCommand::CreateDocument(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    &value.scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let aggregate =
                    LegalDocument::create(value.document_id, value.scope, value.title, now)
                        .map_err(|error| to_cqrs(error.into(), correlation))?;
                (LegalAggregate::Document(aggregate), None)
            }
            LegalCommand::AddDocumentVersion(value) => {
                let document = self
                    .write
                    .load_document(&envelope.tenant_scope, value.document_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Document(document);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Document(document) = &mut aggregate else {
                    unreachable!()
                };
                document
                    .add_version(value.object_ref, value.content_sha256, actor_ref, now)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            LegalCommand::ArchiveDocument(value) => {
                let document = self
                    .write
                    .load_document(&envelope.tenant_scope, value.document_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Document(document);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Document(document) = &mut aggregate else {
                    unreachable!()
                };
                document
                    .archive(now)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            LegalCommand::RecordDocumentAccess(value) => {
                let document = self
                    .write
                    .load_document(&envelope.tenant_scope, value.document_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    document.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                if value.document_version == 0
                    || value.document_version > document.current_version()
                {
                    return Err(to_cqrs(
                        LegalApplicationError::DocumentVersionNotFound,
                        correlation,
                    ));
                }
                let access = AccessEntry::record(
                    value.access_entry_id,
                    document.scope().clone(),
                    value.document_id,
                    value.document_version,
                    actor_ref,
                    value.action,
                    value.purpose,
                    value.observed_at,
                    now,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (LegalAggregate::AccessEntry(access), None)
            }
            LegalCommand::CreateMatter(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    &value.scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let matter = LegalMatter::create(
                    value.matter_id,
                    value.scope,
                    value.reference,
                    value.title,
                    actor_ref,
                    now,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (LegalAggregate::Matter(matter), None)
            }
            LegalCommand::LinkMatterRecord(value) => {
                let matter = self
                    .write
                    .load_matter(&envelope.tenant_scope, value.matter_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Matter(matter);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                // Cross-company links are authorized against the target scope as well;
                // a company authority therefore cannot create a sibling-company edge.
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    &value.target_scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Matter(matter) = &mut aggregate else {
                    unreachable!()
                };
                matter
                    .link_record(
                        value.relation,
                        value.target,
                        value.target_scope,
                        value.target_digest,
                        actor_ref,
                        now,
                    )
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            LegalCommand::PlaceLegalHold(value) => {
                let matter = self
                    .write
                    .load_matter(&envelope.tenant_scope, value.matter_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Matter(matter);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Matter(matter) = &mut aggregate else {
                    unreachable!()
                };
                matter
                    .place_hold(value.reason, actor_ref, now)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            LegalCommand::ReleaseLegalHold(value) => {
                let matter = self
                    .write
                    .load_matter(&envelope.tenant_scope, value.matter_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Matter(matter);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Matter(matter) = &mut aggregate else {
                    unreachable!()
                };
                matter
                    .release_hold(actor_ref, now)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            LegalCommand::CloseMatter(value) => {
                let matter = self
                    .write
                    .load_matter(&envelope.tenant_scope, value.matter_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = LegalAggregate::Matter(matter);
                let expected = self
                    .existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.tenant_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let LegalAggregate::Matter(matter) = &mut aggregate else {
                    unreachable!()
                };
                matter
                    .close(now)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
        };

        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&aggregate, &domain_events);
        let stored = self
            .unit_of_work
            .commit(LegalCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;
        let response = LegalMutationResult::from(&stored.aggregate);
        Ok(stored.into_receipt(response))
    }
}

pub struct LegalQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> LegalQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<LegalQuery> for LegalQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = LegalQueryResponse;

    async fn handle(
        &self,
        envelope: QueryEnvelope<LegalQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.query.action();
        self.entitlement
            .ensure_legal_enabled(&envelope.tenant_scope, false)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.tenant_scope,
                target_scope: &envelope.tenant_scope,
                action,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;

        let (value, projection) = match envelope.query {
            LegalQuery::GetContract(value) => {
                let projected = self
                    .read
                    .get_contract(
                        &envelope.tenant_scope,
                        value.contract_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    LegalQueryResponse::Contract(projected.value),
                    projected.metadata,
                )
            }
            LegalQuery::ListContracts(value) => {
                validate_page_size(value.filter.page_size)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projected = self
                    .read
                    .list_contracts(&envelope.tenant_scope, &value.filter, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    LegalQueryResponse::Contracts(projected.value),
                    projected.metadata,
                )
            }
            LegalQuery::GetDocument(value) => {
                let projected = self
                    .read
                    .get_document(
                        &envelope.tenant_scope,
                        value.document_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    LegalQueryResponse::Document(projected.value),
                    projected.metadata,
                )
            }
            LegalQuery::ListDocuments(value) => {
                validate_page_size(value.filter.page_size)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projected = self
                    .read
                    .list_documents(&envelope.tenant_scope, &value.filter, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    LegalQueryResponse::Documents(projected.value),
                    projected.metadata,
                )
            }
            LegalQuery::ListAccessHistory(value) => {
                validate_page_size(value.filter.page_size)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projected = self
                    .read
                    .list_access_history(
                        &envelope.tenant_scope,
                        &value.filter,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    LegalQueryResponse::AccessHistory(projected.value),
                    projected.metadata,
                )
            }
            LegalQuery::GetMatter(value) => {
                let projected = self
                    .read
                    .get_matter(
                        &envelope.tenant_scope,
                        value.matter_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    LegalQueryResponse::Matter(projected.value),
                    projected.metadata,
                )
            }
        };
        Ok(QueryResult {
            value,
            metadata: QueryMetadata {
                correlation_id: correlation,
                projection,
                consistency: envelope.consistency,
            },
        })
    }
}

#[allow(clippy::result_large_err)]
fn validate_page_size(page_size: u16) -> Result<(), LegalApplicationError> {
    if page_size == 0 || page_size > MAX_LEGAL_PAGE_SIZE {
        Err(LegalApplicationError::InvalidPageSize)
    } else {
        Ok(())
    }
}

fn port_to_cqrs(error: PortError, correlation: CorrelationId) -> CqrsError {
    if let PortError::NotReady(value) = error {
        CqrsError::NotReady(value)
    } else {
        to_cqrs(error.into(), correlation)
    }
}

fn to_cqrs(error: LegalApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, message, retryable) = match error {
        LegalApplicationError::ScopeOutsideAuthority
        | LegalApplicationError::Port(PortError::Denied | PortError::EntitlementDenied) => (
            "legal.denied",
            ErrorCategory::Denied,
            "Legal operation is not allowed.",
            false,
        ),
        LegalApplicationError::Port(PortError::NotFound) => (
            "legal.not_found",
            ErrorCategory::NotFound,
            "Legal record was not found.",
            false,
        ),
        LegalApplicationError::MissingExpectedVersion
        | LegalApplicationError::VersionConflict { .. }
        | LegalApplicationError::Port(
            PortError::Conflict { .. } | PortError::IdempotencyConflict,
        ) => (
            "legal.conflict",
            ErrorCategory::Conflict,
            "Legal record changed or command identity conflicts.",
            false,
        ),
        LegalApplicationError::Port(PortError::Unavailable) => (
            "legal.unavailable",
            ErrorCategory::Unavailable,
            "Legal is temporarily unavailable.",
            true,
        ),
        LegalApplicationError::Domain(_)
        | LegalApplicationError::UnexpectedCreateVersion
        | LegalApplicationError::DocumentVersionNotFound
        | LegalApplicationError::InvalidPageSize => (
            "legal.invalid",
            ErrorCategory::Invalid,
            "Legal input is invalid.",
            false,
        ),
        LegalApplicationError::Port(PortError::Internal | PortError::NotReady(_)) => (
            "legal.internal",
            ErrorCategory::Internal,
            "Legal could not complete the operation.",
            false,
        ),
    };
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

#[cfg(test)]
mod tests;
