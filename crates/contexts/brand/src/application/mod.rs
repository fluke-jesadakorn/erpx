use std::collections::BTreeSet;

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CorrelationId, ErrorCategory,
    ErrorContext, EventId, KernelError, RecordId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        BrandCategory, BrandDefinition, BrandDomainError, BrandProfile, BrandProfileId, BrandScope,
        ParentBrandWitness,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, BrandAction, BrandCommit, BrandStoredCommit,
        CommandFingerprint, CommandIdentity, CommandUnitOfWork, DerivedRecordReadRepository,
        EntitlementPort, ParentAuthorityPort, PortError, ReadRepository, WriteRepository,
    },
};

pub use crate::ports::{
    BrandAssetView, BrandHistory, BrandProfileView, BrandPublicationView, ResolvedBrandView,
};

pub const MAX_BRAND_HISTORY_PAGE_SIZE: u16 = 100;
pub const MAX_BRAND_REASON_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateBrandDraft {
    pub profile_id: BrandProfileId,
    pub scope: BrandScope,
    pub definition: BrandDefinition,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveBrandDraft {
    pub profile_id: BrandProfileId,
    pub definition: BrandDefinition,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishBrandDraft {
    pub profile_id: BrandProfileId,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackBrand {
    pub profile_id: BrandProfileId,
    pub source_revision: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetBrandCategoryLocks {
    pub profile_id: BrandProfileId,
    pub locked_categories: BTreeSet<BrandCategory>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBrandDraft {
    pub scope: BrandScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetResolvedBrand {
    pub scope: BrandScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListBrandHistory {
    pub scope: BrandScope,
    pub page_size: u16,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBrandAsset {
    pub record_id: RecordId,
    pub scope: BrandScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBrandPublication {
    pub record_id: RecordId,
    pub scope: BrandScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateBrandDraftResult {
    pub profile: BrandProfileView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandMutationResult {
    pub profile: BrandProfileView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBrandDraftResult {
    pub profile: Option<BrandProfileView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetResolvedBrandResult {
    pub brand: Option<ResolvedBrandView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBrandAssetResult {
    pub asset: Option<BrandAssetView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBrandPublicationResult {
    pub publication: Option<BrandPublicationView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BrandApplicationError {
    #[error(transparent)]
    Domain(#[from] BrandDomainError),
    #[error("command scope does not contain the Brand target")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an existing aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("brand aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("brand reason is required and must be at most 512 safe bytes")]
    InvalidReason,
    #[error("brand source revision must be nonzero")]
    InvalidSourceRevision,
    #[error("brand history page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("brand integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct BrandCommandHandler<W, U, A, E, P, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    parent_authority: P,
    clock: C,
}

impl<W, U, A, E, P, C> BrandCommandHandler<W, U, A, E, P, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        parent_authority: P,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            parent_authority,
            clock,
        }
    }
}

enum ExistingMutation {
    SaveDraft {
        definition: Box<BrandDefinition>,
    },
    Publish,
    Rollback {
        source_revision: u64,
    },
    SetCategoryLocks {
        locked_categories: BTreeSet<BrandCategory>,
    },
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: AuthorityScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    profile_id: BrandProfileId,
    action: BrandAction,
    reason: String,
    mutation: ExistingMutation,
}

impl<W, U, A, E, P, C> BrandCommandHandler<W, U, A, E, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    P: ParentAuthorityPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target: &BrandScope,
        action: BrandAction,
    ) -> Result<(), BrandApplicationError> {
        let target_scope = target.authority_scope()?;
        if !authority_scope.contains(&target_scope) {
            return Err(BrandApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_brand_enabled(&target_scope, action.requires_write_entitlement())
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target,
                action,
            })
            .await?;
        Ok(())
    }

    async fn authoritative_parent(
        &self,
        scope: &BrandScope,
    ) -> Result<Option<ParentBrandWitness>, PortError> {
        self.parent_authority.current_parent(scope).await
    }

    async fn execute_existing(
        &self,
        command: ExistingCommand,
    ) -> Result<BrandStoredCommit, CqrsError> {
        validate_reason(&command.reason).map_err(|error| to_cqrs(error, command.correlation_id))?;
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                stored.aggregate.scope(),
                command.action,
            )
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
            stored.replayed = true;
            return Ok(stored);
        }

        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.profile_id)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), command.correlation_id))?;
        self.preflight(
            &command.actor,
            &command.authority_scope,
            aggregate.scope(),
            command.action,
        )
        .await
        .map_err(|error| to_cqrs(error, command.correlation_id))?;
        let expected = command.expected_version.ok_or_else(|| {
            to_cqrs(
                BrandApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                BrandApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }

        let parent = self
            .authoritative_parent(aggregate.scope())
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let actor_ref = command.actor.principal_ref.clone();
        let at = self.clock.now();
        match command.mutation {
            ExistingMutation::SaveDraft { definition } => {
                aggregate.save_draft(*definition, parent.as_ref(), actor_ref, at)
            }
            ExistingMutation::Publish => aggregate.publish_draft(parent.as_ref(), actor_ref, at),
            ExistingMutation::Rollback { source_revision } => {
                if source_revision == 0 {
                    return Err(to_cqrs(
                        BrandApplicationError::InvalidSourceRevision,
                        command.correlation_id,
                    ));
                }
                let source = self
                    .write_repository
                    .load_archived_revision(
                        &command.authority_scope,
                        command.profile_id,
                        source_revision,
                    )
                    .await
                    .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), command.correlation_id))?;
                aggregate.rollback(source, parent.as_ref(), actor_ref, at)
            }
            ExistingMutation::SetCategoryLocks { locked_categories } => {
                aggregate.set_category_locks(locked_categories, parent.as_ref(), actor_ref, at)
            }
        }
        .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;

        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&domain_events).map_err(|_| {
            to_cqrs(
                BrandApplicationError::IntegrationMapping,
                command.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(BrandCommit {
                identity: command.identity,
                correlation_id: command.correlation_id,
                causation_id: command.causation_id,
                actor: command.actor,
                authority_scope: command.authority_scope,
                audit: command.audit,
                reason: command.reason,
                expected_version: Some(expected),
                ensure_scope_vacant: false,
                parent_guard: parent,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))
    }
}

#[async_trait]
impl<W, U, A, E, P, C> CommandHandler<CreateBrandDraft> for BrandCommandHandler<W, U, A, E, P, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    P: ParentAuthorityPort,
    C: Clock,
{
    type Response = CreateBrandDraftResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateBrandDraft>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        validate_reason(&command.reason).map_err(|error| to_cqrs(error, correlation_id))?;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::CreateDraft {
                profile_id: command.profile_id,
                scope: command.scope.clone(),
                definition: command.definition.clone(),
                reason: command.reason.clone(),
            },
        };
        if let Some(mut stored) = self
            .unit_of_work
            .find_receipt(&identity)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.active_scope,
                stored.aggregate.scope(),
                BrandAction::CreateDraft,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = CreateBrandDraftResult {
                profile: BrandProfileView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.active_scope,
            &command.scope,
            BrandAction::CreateDraft,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                BrandApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let parent = self
            .authoritative_parent(&command.scope)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let mut aggregate = BrandProfile::create_draft(
            command.profile_id,
            command.scope,
            command.definition,
            parent.as_ref(),
            envelope.actor.principal_ref.clone(),
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events())
                .map_err(|_| to_cqrs(BrandApplicationError::IntegrationMapping, correlation_id))?;
        let stored = self
            .unit_of_work
            .commit(BrandCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                reason: command.reason,
                expected_version: None,
                ensure_scope_vacant: true,
                parent_guard: parent,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = CreateBrandDraftResult {
            profile: BrandProfileView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $reason:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, P, C> CommandHandler<$command> for BrandCommandHandler<W, U, A, E, P, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            P: ParentAuthorityPort,
            C: Clock,
        {
            type Response = BrandMutationResult;

            async fn handle(
                &self,
                envelope: CommandEnvelope<$command>,
            ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
                envelope.validate()?;
                let command = envelope.command.clone();
                let existing = ExistingCommand {
                    correlation_id: envelope.correlation_id,
                    causation_id: envelope.causation_id,
                    actor: envelope.actor,
                    authority_scope: envelope.active_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint: ($fingerprint)(&command),
                    },
                    profile_id: command.profile_id,
                    action: $action,
                    reason: ($reason)(&command),
                    mutation: ($mutation)(command),
                };
                let stored = self.execute_existing(existing).await?;
                let response = BrandMutationResult {
                    profile: BrandProfileView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

existing_handler!(
    SaveBrandDraft,
    BrandAction::SaveDraft,
    |command: &SaveBrandDraft| CommandFingerprint::SaveDraft {
        profile_id: command.profile_id,
        definition: command.definition.clone(),
        reason: command.reason.clone(),
    },
    |command: &SaveBrandDraft| command.reason.clone(),
    |command: SaveBrandDraft| ExistingMutation::SaveDraft {
        definition: Box::new(command.definition),
    }
);
existing_handler!(
    PublishBrandDraft,
    BrandAction::Publish,
    |command: &PublishBrandDraft| CommandFingerprint::Publish {
        profile_id: command.profile_id,
        reason: command.reason.clone(),
    },
    |command: &PublishBrandDraft| command.reason.clone(),
    |_command: PublishBrandDraft| ExistingMutation::Publish
);
existing_handler!(
    RollbackBrand,
    BrandAction::Rollback,
    |command: &RollbackBrand| CommandFingerprint::Rollback {
        profile_id: command.profile_id,
        source_revision: command.source_revision,
        reason: command.reason.clone(),
    },
    |command: &RollbackBrand| command.reason.clone(),
    |command: RollbackBrand| ExistingMutation::Rollback {
        source_revision: command.source_revision,
    }
);
existing_handler!(
    SetBrandCategoryLocks,
    BrandAction::SetCategoryLocks,
    |command: &SetBrandCategoryLocks| CommandFingerprint::SetCategoryLocks {
        profile_id: command.profile_id,
        locked_categories: command.locked_categories.clone(),
        reason: command.reason.clone(),
    },
    |command: &SetBrandCategoryLocks| command.reason.clone(),
    |command: SetBrandCategoryLocks| ExistingMutation::SetCategoryLocks {
        locked_categories: command.locked_categories,
    }
);

pub struct BrandQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> BrandQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> BrandQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target: &BrandScope,
        action: BrandAction,
    ) -> Result<(), BrandApplicationError> {
        let target_scope = target.authority_scope()?;
        if !authority_scope.contains(&target_scope) {
            return Err(BrandApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_brand_enabled(&target_scope, false)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target,
                action,
            })
            .await?;
        Ok(())
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetBrandDraft> for BrandQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetBrandDraftResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetBrandDraft>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.active_scope,
            &envelope.query.scope,
            BrandAction::ViewDraft,
        )
        .await
        .map_err(|error| to_cqrs(error, envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get_draft(
                &envelope.active_scope,
                &envelope.query.scope,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| view.scope != envelope.query.scope)
        {
            return Err(to_cqrs(
                BrandApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetBrandDraftResult {
                profile: projection.value,
            },
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetResolvedBrand> for BrandQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetResolvedBrandResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetResolvedBrand>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.active_scope,
            &envelope.query.scope,
            BrandAction::ViewResolved,
        )
        .await
        .map_err(|error| to_cqrs(error, envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get_resolved(
                &envelope.active_scope,
                &envelope.query.scope,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| view.scope != envelope.query.scope)
        {
            return Err(to_cqrs(
                BrandApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetResolvedBrandResult {
                brand: projection.value,
            },
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<ListBrandHistory> for BrandQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = BrandHistory;

    async fn handle(
        &self,
        envelope: QueryEnvelope<ListBrandHistory>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_BRAND_HISTORY_PAGE_SIZE).contains(&envelope.query.page_size) {
            return Err(to_cqrs(
                BrandApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(
            &envelope.actor,
            &envelope.active_scope,
            &envelope.query.scope,
            BrandAction::ViewHistory,
        )
        .await
        .map_err(|error| to_cqrs(error, envelope.correlation_id))?;
        let projection = self
            .read_repository
            .list_history(
                &envelope.active_scope,
                &envelope.query.scope,
                envelope.query.page_size,
                envelope.query.cursor.as_deref(),
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        Ok(QueryResult {
            value: projection.value,
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetBrandAsset> for BrandQueryHandler<R, A, E>
where
    R: ReadRepository + DerivedRecordReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetBrandAssetResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetBrandAsset>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.active_scope,
            &envelope.query.scope,
            BrandAction::ViewAsset,
        )
        .await
        .map_err(|error| to_cqrs(error, envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get_asset(
                &envelope.active_scope,
                &envelope.query.scope,
                envelope.query.record_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|asset| asset.scope != envelope.query.scope)
        {
            return Err(to_cqrs(
                BrandApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetBrandAssetResult {
                asset: projection.value,
            },
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetBrandPublication> for BrandQueryHandler<R, A, E>
where
    R: ReadRepository + DerivedRecordReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetBrandPublicationResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetBrandPublication>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(
            &envelope.actor,
            &envelope.active_scope,
            &envelope.query.scope,
            BrandAction::ViewPublication,
        )
        .await
        .map_err(|error| to_cqrs(error, envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get_publication(
                &envelope.active_scope,
                &envelope.query.scope,
                envelope.query.record_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|publication| publication.scope != envelope.query.scope)
        {
            return Err(to_cqrs(
                BrandApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetBrandPublicationResult {
                publication: projection.value,
            },
            metadata: QueryMetadata {
                correlation_id: envelope.correlation_id,
                projection: projection.metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

// Keep the public application error shape stable for command callers.
#[allow(clippy::result_large_err)]
fn validate_reason(value: &str) -> Result<(), BrandApplicationError> {
    if value.trim().is_empty()
        || value.len() > MAX_BRAND_REASON_BYTES
        || value.chars().any(char::is_control)
    {
        Err(BrandApplicationError::InvalidReason)
    } else {
        Ok(())
    }
}

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(not_ready) => CqrsError::NotReady(not_ready),
        other => to_cqrs(other.into(), correlation_id),
    }
}

fn to_cqrs(error: BrandApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        BrandApplicationError::Domain(_)
        | BrandApplicationError::UnexpectedCreateVersion
        | BrandApplicationError::MissingExpectedVersion
        | BrandApplicationError::InvalidReason
        | BrandApplicationError::InvalidSourceRevision
        | BrandApplicationError::InvalidPageSize
        | BrandApplicationError::IntegrationMapping => (
            "brand.invalid",
            ErrorCategory::Invalid,
            "The Brand request is invalid.",
            false,
        ),
        BrandApplicationError::ScopeOutsideAuthority
        | BrandApplicationError::Port(PortError::Denied)
        | BrandApplicationError::Port(PortError::EntitlementDenied) => (
            "brand.denied",
            ErrorCategory::Denied,
            "The Brand operation is not permitted.",
            false,
        ),
        BrandApplicationError::VersionConflict { .. }
        | BrandApplicationError::Port(PortError::ScopeConflict)
        | BrandApplicationError::Port(PortError::Conflict { .. })
        | BrandApplicationError::Port(PortError::ParentConflict)
        | BrandApplicationError::Port(PortError::IdempotencyConflict) => (
            "brand.conflict",
            ErrorCategory::Conflict,
            "The Brand profile changed or the command identity conflicts.",
            false,
        ),
        BrandApplicationError::Port(PortError::NotFound) => (
            "brand.not_found",
            ErrorCategory::NotFound,
            "The Brand profile was not found.",
            false,
        ),
        BrandApplicationError::Port(PortError::Unavailable) => (
            "brand.unavailable",
            ErrorCategory::Unavailable,
            "Brand is temporarily unavailable.",
            true,
        ),
        BrandApplicationError::Port(PortError::Internal)
        | BrandApplicationError::Port(PortError::NotReady(_)) => (
            "brand.internal",
            ErrorCategory::Internal,
            "Brand could not complete the operation.",
            false,
        ),
    };
    CqrsError::Operation(KernelError {
        code: code.into(),
        category,
        safe_message: safe_message.into(),
        retryable,
        correlation_id,
        context: ErrorContext::empty(),
    })
}

#[cfg(test)]
mod tests;
