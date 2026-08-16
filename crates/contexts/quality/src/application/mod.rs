mod records;

pub use records::{QualityLifecycleRecordApplicationError, QualityLifecycleRecordCommandHandler};

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{
    ActorAttribution, AuditContext, Clock, CorrelationId, ErrorCategory, ErrorContext, EventId,
    KernelError, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{InspectionId, InspectionKind, QualityDomainError, QualityInspection},
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, PortError, QualityAction, QualityCommit,
        ReadRepository, StoredCommit, WriteRepository,
    },
};

pub use crate::ports::{InspectionList, InspectionListFilter, InspectionView};

pub const MAX_QUALITY_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateInspection {
    pub inspection_id: InspectionId,
    pub scope: TenantScope,
    pub kind: InspectionKind,
    pub product_id: Uuid,
    pub lot_reference: Option<String>,
    pub quantity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartInspection {
    pub inspection_id: InspectionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordInspectionResult {
    pub inspection_id: InspectionId,
    pub passed_quantity: u64,
    pub failed_quantity: u64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInspection {
    pub inspection_id: InspectionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelInspection {
    pub inspection_id: InspectionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInspection {
    pub inspection_id: InspectionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListInspections {
    pub filter: InspectionListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateInspectionResult {
    pub inspection: InspectionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionMutationResult {
    pub inspection: InspectionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInspectionResult {
    pub inspection: Option<InspectionView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QualityApplicationError {
    #[error(transparent)]
    Domain(#[from] QualityDomainError),
    #[error("command scope does not contain the Quality record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an existing aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("quality aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("quality list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("quality integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct QualityCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> QualityCommandHandler<W, U, A, E, C> {
    pub const fn new(
        write_repository: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        clock: C,
    ) -> Self {
        Self {
            write_repository,
            unit_of_work,
            authorization,
            entitlement,
            clock,
        }
    }
}

struct ExistingCommand {
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    actor: ActorAttribution,
    authority_scope: TenantScope,
    audit: AuditContext,
    expected_version: Option<u64>,
    identity: CommandIdentity,
    inspection_id: InspectionId,
    action: QualityAction,
}

impl<W, U, A, E, C> QualityCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: QualityAction,
    ) -> Result<(), QualityApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(QualityApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_quality_enabled(target_scope, action.requires_write_entitlement())
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

    async fn execute_existing<F>(
        &self,
        command: ExistingCommand,
        mutate: F,
    ) -> Result<StoredCommit, CqrsError>
    where
        F: FnOnce(
                &mut QualityInspection,
                chrono::DateTime<chrono::Utc>,
            ) -> Result<(), QualityDomainError>
            + Send,
    {
        if let Some(mut receipt) = self
            .unit_of_work
            .find_receipt(&command.identity)
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?
        {
            self.preflight(
                &command.actor,
                &command.authority_scope,
                receipt.aggregate.scope(),
                command.action,
            )
            .await
            .map_err(|error| to_cqrs(error, command.correlation_id))?;
            receipt.replayed = true;
            return Ok(receipt);
        }

        let mut aggregate = self
            .write_repository
            .load(&command.authority_scope, command.inspection_id)
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
                QualityApplicationError::MissingExpectedVersion,
                command.correlation_id,
            )
        })?;
        if expected != aggregate.version() {
            return Err(to_cqrs(
                QualityApplicationError::VersionConflict {
                    expected,
                    actual: aggregate.version(),
                },
                command.correlation_id,
            ));
        }
        mutate(&mut aggregate, self.clock.now())
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))?;
        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&domain_events).map_err(|_| {
            to_cqrs(
                QualityApplicationError::IntegrationMapping,
                command.correlation_id,
            )
        })?;
        self.unit_of_work
            .commit(QualityCommit {
                identity: command.identity,
                correlation_id: command.correlation_id,
                causation_id: command.causation_id,
                actor: command.actor,
                authority_scope: command.authority_scope,
                audit: command.audit,
                expected_version: Some(expected),
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), command.correlation_id))
    }
}

macro_rules! existing_handler {
    ($command:ty, $action:expr, $fingerprint:expr, $mutation:expr) => {
        #[async_trait]
        impl<W, U, A, E, C> CommandHandler<$command> for QualityCommandHandler<W, U, A, E, C>
        where
            W: WriteRepository,
            U: CommandUnitOfWork,
            A: AuthorizationPort,
            E: EntitlementPort,
            C: Clock,
        {
            type Response = InspectionMutationResult;

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
                    authority_scope: envelope.tenant_scope,
                    audit: envelope.audit,
                    expected_version: envelope.expected_aggregate_version,
                    identity: CommandIdentity {
                        command_id: envelope.command_id,
                        idempotency_key: envelope.idempotency_key,
                        fingerprint: ($fingerprint)(&command),
                    },
                    inspection_id: command.inspection_id,
                    action: $action,
                };
                let stored = self
                    .execute_existing(existing, |aggregate, at| {
                        ($mutation)(aggregate, command, at)
                    })
                    .await?;
                let response = InspectionMutationResult {
                    inspection: InspectionView::from(&stored.aggregate),
                };
                Ok(stored.into_receipt(response))
            }
        }
    };
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<CreateInspection> for QualityCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = CreateInspectionResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<CreateInspection>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let command = envelope.command;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint: CommandFingerprint::Create {
                inspection_id: command.inspection_id,
                scope: command.scope.clone(),
                kind: command.kind,
                product_id: command.product_id,
                lot_reference: command.lot_reference.clone(),
                quantity: command.quantity,
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
                &envelope.tenant_scope,
                stored.aggregate.scope(),
                QualityAction::Create,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = CreateInspectionResult {
                inspection: InspectionView::from(&stored.aggregate),
            };
            return Ok(stored.into_receipt(response));
        }
        self.preflight(
            &envelope.actor,
            &envelope.tenant_scope,
            &command.scope,
            QualityAction::Create,
        )
        .await
        .map_err(|error| to_cqrs(error, correlation_id))?;
        if envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                QualityApplicationError::UnexpectedCreateVersion,
                correlation_id,
            ));
        }
        let mut aggregate = QualityInspection::create(
            command.inspection_id,
            command.scope,
            command.kind,
            command.product_id,
            command.lot_reference,
            command.quantity,
            self.clock.now(),
        )
        .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let domain_events = aggregate.take_uncommitted_events();
        let integration_events = integration::published_events(&domain_events)
            .map_err(|_| to_cqrs(QualityApplicationError::IntegrationMapping, correlation_id))?;
        let stored = self
            .unit_of_work
            .commit(QualityCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version: None,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = CreateInspectionResult {
            inspection: InspectionView::from(&stored.aggregate),
        };
        Ok(stored.into_receipt(response))
    }
}

existing_handler!(
    StartInspection,
    QualityAction::Start,
    |command: &StartInspection| CommandFingerprint::Start {
        inspection_id: command.inspection_id,
    },
    |aggregate: &mut QualityInspection, _command: StartInspection, at| aggregate.start(at)
);
existing_handler!(
    RecordInspectionResult,
    QualityAction::RecordResult,
    |command: &RecordInspectionResult| CommandFingerprint::RecordResult {
        inspection_id: command.inspection_id,
        passed_quantity: command.passed_quantity,
        failed_quantity: command.failed_quantity,
        note: command.note.clone(),
    },
    |aggregate: &mut QualityInspection, command: RecordInspectionResult, at| aggregate
        .record_result(
            command.passed_quantity,
            command.failed_quantity,
            command.note,
            at,
        )
);
existing_handler!(
    ReleaseInspection,
    QualityAction::Release,
    |command: &ReleaseInspection| CommandFingerprint::Release {
        inspection_id: command.inspection_id,
    },
    |aggregate: &mut QualityInspection, _command: ReleaseInspection, at| aggregate.release(at)
);
existing_handler!(
    CancelInspection,
    QualityAction::Cancel,
    |command: &CancelInspection| CommandFingerprint::Cancel {
        inspection_id: command.inspection_id,
    },
    |aggregate: &mut QualityInspection, _command: CancelInspection, at| aggregate.cancel(at)
);

pub struct QualityQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> QualityQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> QualityQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        actor: &ActorAttribution,
        scope: &TenantScope,
        action: QualityAction,
    ) -> Result<(), PortError> {
        self.entitlement
            .ensure_quality_enabled(scope, false)
            .await?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope: scope,
                target_scope: scope,
                action,
            })
            .await
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<GetInspection> for QualityQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = GetInspectionResult;

    async fn handle(
        &self,
        envelope: QueryEnvelope<GetInspection>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        self.authorize_query(&envelope.actor, &envelope.tenant_scope, QualityAction::View)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .get(
                &envelope.tenant_scope,
                envelope.query.inspection_id,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .as_ref()
            .is_some_and(|view| !envelope.tenant_scope.contains(&view.scope))
        {
            return Err(to_cqrs(
                QualityApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
        Ok(QueryResult {
            value: GetInspectionResult {
                inspection: projection.value,
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
impl<R, A, E> QueryHandler<ListInspections> for QualityQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = InspectionList;

    async fn handle(
        &self,
        envelope: QueryEnvelope<ListInspections>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        if !(1..=MAX_QUALITY_LIST_PAGE_SIZE).contains(&envelope.query.filter.page_size) {
            return Err(to_cqrs(
                QualityApplicationError::InvalidPageSize,
                envelope.correlation_id,
            ));
        }
        self.authorize_query(&envelope.actor, &envelope.tenant_scope, QualityAction::List)
            .await
            .map_err(|error| to_cqrs(error.into(), envelope.correlation_id))?;
        let projection = self
            .read_repository
            .list(
                &envelope.tenant_scope,
                &envelope.query.filter,
                &envelope.consistency,
            )
            .await
            .map_err(|error| port_to_cqrs(error, envelope.correlation_id))?;
        if projection
            .value
            .inspections
            .iter()
            .any(|view| !envelope.tenant_scope.contains(&view.scope))
        {
            return Err(to_cqrs(
                QualityApplicationError::ScopeOutsideAuthority,
                envelope.correlation_id,
            ));
        }
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

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(not_ready) => CqrsError::NotReady(not_ready),
        other => to_cqrs(other.into(), correlation_id),
    }
}

fn to_cqrs(error: QualityApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        QualityApplicationError::Domain(_)
        | QualityApplicationError::UnexpectedCreateVersion
        | QualityApplicationError::MissingExpectedVersion
        | QualityApplicationError::InvalidPageSize
        | QualityApplicationError::IntegrationMapping => (
            "quality.invalid",
            ErrorCategory::Invalid,
            "The Quality request is invalid.",
            false,
        ),
        QualityApplicationError::ScopeOutsideAuthority
        | QualityApplicationError::Port(PortError::Denied)
        | QualityApplicationError::Port(PortError::EntitlementDenied) => (
            "quality.denied",
            ErrorCategory::Denied,
            "The Quality operation is not permitted.",
            false,
        ),
        QualityApplicationError::VersionConflict { .. }
        | QualityApplicationError::Port(PortError::Conflict { .. })
        | QualityApplicationError::Port(PortError::IdempotencyConflict) => (
            "quality.conflict",
            ErrorCategory::Conflict,
            "The Quality record changed or the command identity conflicts.",
            false,
        ),
        QualityApplicationError::Port(PortError::NotFound) => (
            "quality.not_found",
            ErrorCategory::NotFound,
            "The Quality inspection was not found.",
            false,
        ),
        QualityApplicationError::Port(PortError::Unavailable) => (
            "quality.unavailable",
            ErrorCategory::Unavailable,
            "Quality is temporarily unavailable.",
            true,
        ),
        QualityApplicationError::Port(PortError::Internal)
        | QualityApplicationError::Port(PortError::NotReady(_)) => (
            "quality.internal",
            ErrorCategory::Internal,
            "Quality could not complete the operation.",
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
