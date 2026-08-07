use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{Clock, CorrelationId, ErrorCategory, ErrorContext, KernelError, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        CustomerId, Payment, PosDomainError, PosRefund, RefundId, RefundLine, RegisterId, Sale,
        SaleId, SaleLine,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        EntitlementPort, PortError, PosAction, PosReadRepository, RefundCommit, RefundList,
        RefundListFilter, RefundUnitOfWork, RefundView, SaleCommit, SaleList, SaleListFilter,
        SaleUnitOfWork, SaleView, WriteRepository,
    },
};

pub const MAX_LIST_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSale {
    pub sale_id: SaleId,
    pub scope: TenantScope,
    pub receipt_code: String,
    pub register_id: RegisterId,
    pub customer_id: Option<CustomerId>,
    pub currency: String,
    pub lines: Vec<SaleLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenSale {
    pub sale_id: SaleId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldSale {
    pub sale_id: SaleId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeSale {
    pub sale_id: SaleId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordPayment {
    pub sale_id: SaleId,
    pub payment: Payment,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteSale {
    pub sale_id: SaleId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoidSale {
    pub sale_id: SaleId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum PosSaleCommand {
    Create(CreateSale),
    Open(OpenSale),
    Hold(HoldSale),
    Resume(ResumeSale),
    RecordPayment(RecordPayment),
    Complete(CompleteSale),
    Void(VoidSale),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestRefund {
    pub refund_id: RefundId,
    pub sale_id: SaleId,
    pub lines: Vec<RefundLine>,
    pub reason: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApproveRefund {
    pub refund_id: RefundId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectRefund {
    pub refund_id: RefundId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostRefund {
    pub refund_id: RefundId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRefund {
    pub refund_id: RefundId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum PosRefundCommand {
    Request(RequestRefund),
    Approve(ApproveRefund),
    Reject(RejectRefund),
    Post(PostRefund),
    Cancel(CancelRefund),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSale {
    pub sale_id: SaleId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSales {
    pub filter: SaleListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRefund {
    pub refund_id: RefundId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRefunds {
    pub filter: RefundListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum PosQuery {
    GetSale(GetSale),
    ListSales(ListSales),
    GetRefund(GetRefund),
    ListRefunds(ListRefunds),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosSaleMutationResult {
    pub sale: SaleView,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosRefundMutationResult {
    pub refund: RefundView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum PosQueryResponse {
    Sale(Option<SaleView>),
    Sales(SaleList),
    Refund(Option<RefundView>),
    Refunds(RefundList),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PosApplicationError {
    #[error(transparent)]
    Domain(#[from] PosDomainError),
    #[error("command scope does not contain the POS record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutations require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("POS aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("POS page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("POS integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct PosSaleCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> PosSaleCommandHandler<W, U, A, E, C> {
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

impl<W, U, A, E, C> PosSaleCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: SaleUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &TenantScope,
        target_scope: &TenantScope,
        action: PosAction,
    ) -> Result<(), PosApplicationError> {
        preflight(
            &self.authorization,
            &self.entitlement,
            actor,
            authority_scope,
            target_scope,
            action,
        )
        .await
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<PosSaleCommand> for PosSaleCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: SaleUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PosSaleMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<PosSaleCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let (identity, action, sale_id) = sale_identity(
            envelope.command_id,
            envelope.idempotency_key.clone(),
            &envelope.command,
        );
        if let Some(mut stored) = self
            .unit_of_work
            .find_sale_receipt(&identity)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
        {
            self.preflight(
                &envelope.actor,
                &envelope.tenant_scope,
                stored.aggregate.scope(),
                action,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = PosSaleMutationResult {
                sale: (&stored.aggregate).into(),
            };
            return Ok(stored.into_receipt(response));
        }

        let expected_version = envelope.expected_aggregate_version;
        let at = self.clock.now();
        let mut aggregate = match envelope.command {
            PosSaleCommand::Create(command) => {
                if expected_version.is_some() {
                    return Err(to_cqrs(
                        PosApplicationError::UnexpectedCreateVersion,
                        correlation_id,
                    ));
                }
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    &command.scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                Sale::create(
                    command.sale_id,
                    command.scope,
                    command.receipt_code,
                    command.register_id,
                    command.customer_id,
                    command.currency,
                    command.lines,
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation_id))?
            }
            command => {
                let mut aggregate = self
                    .write_repository
                    .load_sale(&envelope.tenant_scope, sale_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation_id))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    aggregate.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                ensure_version(expected_version, aggregate.version())
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                match command {
                    PosSaleCommand::Open(_) => aggregate.open(at),
                    PosSaleCommand::Hold(_) => aggregate.hold(at),
                    PosSaleCommand::Resume(_) => aggregate.resume(at),
                    PosSaleCommand::RecordPayment(command) => {
                        aggregate.record_payment(command.payment, at)
                    }
                    PosSaleCommand::Complete(_) => aggregate.complete(at),
                    PosSaleCommand::Void(_) => aggregate.void(at),
                    PosSaleCommand::Create(_) => unreachable!("create handled above"),
                }
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                aggregate
            }
        };
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events())
                .map_err(|_| to_cqrs(PosApplicationError::IntegrationMapping, correlation_id))?;
        let stored = self
            .unit_of_work
            .commit_sale(SaleCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = PosSaleMutationResult {
            sale: (&stored.aggregate).into(),
        };
        Ok(stored.into_receipt(response))
    }
}

pub struct PosRefundCommandHandler<W, U, A, E, C> {
    write_repository: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> PosRefundCommandHandler<W, U, A, E, C> {
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

#[async_trait]
impl<W, U, A, E, C> CommandHandler<PosRefundCommand> for PosRefundCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: RefundUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PosRefundMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<PosRefundCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let (identity, action, refund_id) = refund_identity(
            envelope.command_id,
            envelope.idempotency_key.clone(),
            &envelope.command,
        );
        if let Some(mut stored) = self
            .unit_of_work
            .find_refund_receipt(&identity)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?
        {
            preflight(
                &self.authorization,
                &self.entitlement,
                &envelope.actor,
                &envelope.tenant_scope,
                stored.aggregate.scope(),
                action,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?;
            stored.replayed = true;
            let response = PosRefundMutationResult {
                refund: (&stored.aggregate).into(),
            };
            return Ok(stored.into_receipt(response));
        }

        let expected_version = envelope.expected_aggregate_version;
        let at = self.clock.now();
        let mut aggregate = match envelope.command {
            PosRefundCommand::Request(command) => {
                if expected_version.is_some() {
                    return Err(to_cqrs(
                        PosApplicationError::UnexpectedCreateVersion,
                        correlation_id,
                    ));
                }
                let sale = self
                    .write_repository
                    .load_sale(&envelope.tenant_scope, command.sale_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation_id))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
                preflight(
                    &self.authorization,
                    &self.entitlement,
                    &envelope.actor,
                    &envelope.tenant_scope,
                    sale.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                PosRefund::request(
                    command.refund_id,
                    &sale,
                    command.lines,
                    command.reason,
                    envelope.actor.principal_ref.clone(),
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation_id))?
            }
            command => {
                let mut aggregate = self
                    .write_repository
                    .load_refund(&envelope.tenant_scope, refund_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation_id))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation_id))?;
                preflight(
                    &self.authorization,
                    &self.entitlement,
                    &envelope.actor,
                    &envelope.tenant_scope,
                    aggregate.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                ensure_version(expected_version, aggregate.version())
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                match command {
                    PosRefundCommand::Approve(_) => {
                        aggregate.approve(envelope.actor.principal_ref.clone(), at)
                    }
                    PosRefundCommand::Reject(_) => {
                        aggregate.reject(envelope.actor.principal_ref.clone(), at)
                    }
                    PosRefundCommand::Post(_) => aggregate.post(at),
                    PosRefundCommand::Cancel(_) => aggregate.cancel(at),
                    PosRefundCommand::Request(_) => unreachable!("request handled above"),
                }
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                aggregate
            }
        };
        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events())
                .map_err(|_| to_cqrs(PosApplicationError::IntegrationMapping, correlation_id))?;
        let stored = self
            .unit_of_work
            .commit_refund(RefundCommit {
                identity,
                correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.tenant_scope,
                audit: envelope.audit,
                expected_version,
                aggregate,
                integration_events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation_id))?;
        let response = PosRefundMutationResult {
            refund: (&stored.aggregate).into(),
        };
        Ok(stored.into_receipt(response))
    }
}

pub struct PosQueryHandler<R, A, E> {
    read_repository: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> PosQueryHandler<R, A, E> {
    pub const fn new(read_repository: R, authorization: A, entitlement: E) -> Self {
        Self {
            read_repository,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<PosQuery> for PosQueryHandler<R, A, E>
where
    R: PosReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = PosQueryResponse;

    async fn handle(
        &self,
        envelope: QueryEnvelope<PosQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let (response, metadata) = match &envelope.query {
            PosQuery::GetSale(query) => {
                self.authorize_query(&envelope, PosAction::ViewSale).await?;
                let projection = self
                    .read_repository
                    .get_sale(&envelope.tenant_scope, query.sale_id, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                ensure_sale_scope(
                    &envelope.tenant_scope,
                    projection.value.as_ref(),
                    correlation_id,
                )?;
                (
                    PosQueryResponse::Sale(projection.value),
                    projection.metadata,
                )
            }
            PosQuery::ListSales(query) => {
                validate_page_size(query.filter.page_size, correlation_id)?;
                self.authorize_query(&envelope, PosAction::ListSales)
                    .await?;
                let projection = self
                    .read_repository
                    .list_sales(&envelope.tenant_scope, &query.filter, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                if projection
                    .value
                    .sales
                    .iter()
                    .any(|sale| !envelope.tenant_scope.contains(&sale.scope))
                {
                    return Err(to_cqrs(
                        PosApplicationError::ScopeOutsideAuthority,
                        correlation_id,
                    ));
                }
                (
                    PosQueryResponse::Sales(projection.value),
                    projection.metadata,
                )
            }
            PosQuery::GetRefund(query) => {
                self.authorize_query(&envelope, PosAction::ViewRefund)
                    .await?;
                let projection = self
                    .read_repository
                    .get_refund(
                        &envelope.tenant_scope,
                        query.refund_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                ensure_refund_scope(
                    &envelope.tenant_scope,
                    projection.value.as_ref(),
                    correlation_id,
                )?;
                (
                    PosQueryResponse::Refund(projection.value),
                    projection.metadata,
                )
            }
            PosQuery::ListRefunds(query) => {
                validate_page_size(query.filter.page_size, correlation_id)?;
                self.authorize_query(&envelope, PosAction::ListRefunds)
                    .await?;
                let projection = self
                    .read_repository
                    .list_refunds(&envelope.tenant_scope, &query.filter, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                if projection
                    .value
                    .refunds
                    .iter()
                    .any(|refund| !envelope.tenant_scope.contains(&refund.scope))
                {
                    return Err(to_cqrs(
                        PosApplicationError::ScopeOutsideAuthority,
                        correlation_id,
                    ));
                }
                (
                    PosQueryResponse::Refunds(projection.value),
                    projection.metadata,
                )
            }
        };
        Ok(QueryResult {
            value: response,
            metadata: QueryMetadata {
                correlation_id,
                projection: metadata,
                consistency: envelope.consistency,
            },
        })
    }
}

impl<R, A, E> PosQueryHandler<R, A, E>
where
    R: PosReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn authorize_query(
        &self,
        envelope: &QueryEnvelope<PosQuery>,
        action: PosAction,
    ) -> Result<(), CqrsError> {
        preflight(
            &self.authorization,
            &self.entitlement,
            &envelope.actor,
            &envelope.tenant_scope,
            &envelope.tenant_scope,
            action,
        )
        .await
        .map_err(|error| to_cqrs(error, envelope.correlation_id))
    }
}

async fn preflight<A, E>(
    authorization: &A,
    entitlement: &E,
    actor: &erpx_kernel::ActorAttribution,
    authority_scope: &TenantScope,
    target_scope: &TenantScope,
    action: PosAction,
) -> Result<(), PosApplicationError>
where
    A: AuthorizationPort,
    E: EntitlementPort,
{
    if !authority_scope.contains(target_scope) {
        return Err(PosApplicationError::ScopeOutsideAuthority);
    }
    entitlement
        .ensure_pos_enabled(target_scope, action.requires_write_entitlement())
        .await?;
    authorization
        .authorize(AuthorizationRequest {
            actor,
            authority_scope,
            target_scope,
            action,
        })
        .await?;
    Ok(())
}

#[allow(clippy::result_large_err)]
fn ensure_version(expected: Option<u64>, actual: u64) -> Result<(), PosApplicationError> {
    let expected = expected.ok_or(PosApplicationError::MissingExpectedVersion)?;
    if expected == actual {
        Ok(())
    } else {
        Err(PosApplicationError::VersionConflict { expected, actual })
    }
}

fn sale_identity(
    command_id: erpx_kernel::CommandId,
    idempotency_key: erpx_cqrs::IdempotencyKey,
    command: &PosSaleCommand,
) -> (CommandIdentity, PosAction, SaleId) {
    let (fingerprint, action, sale_id) = match command {
        PosSaleCommand::Create(value) => (
            CommandFingerprint::CreateSale {
                sale_id: value.sale_id,
                scope: value.scope.clone(),
                receipt_code: value.receipt_code.clone(),
                register_id: value.register_id,
                customer_id: value.customer_id,
                currency: value.currency.clone(),
                lines: value.lines.clone(),
            },
            PosAction::CreateSale,
            value.sale_id,
        ),
        PosSaleCommand::Open(value) => (
            CommandFingerprint::OpenSale {
                sale_id: value.sale_id,
            },
            PosAction::OpenSale,
            value.sale_id,
        ),
        PosSaleCommand::Hold(value) => (
            CommandFingerprint::HoldSale {
                sale_id: value.sale_id,
            },
            PosAction::HoldSale,
            value.sale_id,
        ),
        PosSaleCommand::Resume(value) => (
            CommandFingerprint::ResumeSale {
                sale_id: value.sale_id,
            },
            PosAction::ResumeSale,
            value.sale_id,
        ),
        PosSaleCommand::RecordPayment(value) => (
            CommandFingerprint::RecordPayment {
                sale_id: value.sale_id,
                payment: value.payment.clone(),
            },
            PosAction::RecordPayment,
            value.sale_id,
        ),
        PosSaleCommand::Complete(value) => (
            CommandFingerprint::CompleteSale {
                sale_id: value.sale_id,
            },
            PosAction::CompleteSale,
            value.sale_id,
        ),
        PosSaleCommand::Void(value) => (
            CommandFingerprint::VoidSale {
                sale_id: value.sale_id,
            },
            PosAction::VoidSale,
            value.sale_id,
        ),
    };
    (
        CommandIdentity {
            command_id,
            idempotency_key,
            fingerprint,
        },
        action,
        sale_id,
    )
}

fn refund_identity(
    command_id: erpx_kernel::CommandId,
    idempotency_key: erpx_cqrs::IdempotencyKey,
    command: &PosRefundCommand,
) -> (CommandIdentity, PosAction, RefundId) {
    let (fingerprint, action, refund_id) = match command {
        PosRefundCommand::Request(value) => (
            CommandFingerprint::RequestRefund {
                refund_id: value.refund_id,
                sale_id: value.sale_id,
                lines: value.lines.clone(),
                reason: value.reason.clone(),
            },
            PosAction::RequestRefund,
            value.refund_id,
        ),
        PosRefundCommand::Approve(value) => (
            CommandFingerprint::ApproveRefund {
                refund_id: value.refund_id,
            },
            PosAction::ApproveRefund,
            value.refund_id,
        ),
        PosRefundCommand::Reject(value) => (
            CommandFingerprint::RejectRefund {
                refund_id: value.refund_id,
            },
            PosAction::RejectRefund,
            value.refund_id,
        ),
        PosRefundCommand::Post(value) => (
            CommandFingerprint::PostRefund {
                refund_id: value.refund_id,
            },
            PosAction::PostRefund,
            value.refund_id,
        ),
        PosRefundCommand::Cancel(value) => (
            CommandFingerprint::CancelRefund {
                refund_id: value.refund_id,
            },
            PosAction::CancelRefund,
            value.refund_id,
        ),
    };
    (
        CommandIdentity {
            command_id,
            idempotency_key,
            fingerprint,
        },
        action,
        refund_id,
    )
}

#[allow(clippy::result_large_err)]
fn validate_page_size(page_size: u16, correlation_id: CorrelationId) -> Result<(), CqrsError> {
    if (1..=MAX_LIST_PAGE_SIZE).contains(&page_size) {
        Ok(())
    } else {
        Err(to_cqrs(
            PosApplicationError::InvalidPageSize,
            correlation_id,
        ))
    }
}

#[allow(clippy::result_large_err)]
fn ensure_sale_scope(
    authority_scope: &TenantScope,
    sale: Option<&SaleView>,
    correlation_id: CorrelationId,
) -> Result<(), CqrsError> {
    if sale.is_some_and(|value| !authority_scope.contains(&value.scope)) {
        Err(to_cqrs(
            PosApplicationError::ScopeOutsideAuthority,
            correlation_id,
        ))
    } else {
        Ok(())
    }
}

#[allow(clippy::result_large_err)]
fn ensure_refund_scope(
    authority_scope: &TenantScope,
    refund: Option<&RefundView>,
    correlation_id: CorrelationId,
) -> Result<(), CqrsError> {
    if refund.is_some_and(|value| !authority_scope.contains(&value.scope)) {
        Err(to_cqrs(
            PosApplicationError::ScopeOutsideAuthority,
            correlation_id,
        ))
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

fn to_cqrs(error: PosApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        PosApplicationError::Domain(_)
        | PosApplicationError::UnexpectedCreateVersion
        | PosApplicationError::MissingExpectedVersion
        | PosApplicationError::InvalidPageSize
        | PosApplicationError::IntegrationMapping => (
            "pos.invalid",
            ErrorCategory::Invalid,
            "The POS request is invalid.",
            false,
        ),
        PosApplicationError::ScopeOutsideAuthority
        | PosApplicationError::Port(PortError::Denied)
        | PosApplicationError::Port(PortError::EntitlementDenied) => (
            "pos.denied",
            ErrorCategory::Denied,
            "The POS operation is not permitted.",
            false,
        ),
        PosApplicationError::VersionConflict { .. }
        | PosApplicationError::Port(PortError::Conflict { .. })
        | PosApplicationError::Port(PortError::IdempotencyConflict)
        | PosApplicationError::Port(PortError::RefundBalanceConflict) => (
            "pos.conflict",
            ErrorCategory::Conflict,
            "The POS record changed or the command conflicts.",
            false,
        ),
        PosApplicationError::Port(PortError::NotFound) => (
            "pos.not_found",
            ErrorCategory::NotFound,
            "The POS record was not found.",
            false,
        ),
        PosApplicationError::Port(PortError::Unavailable) => (
            "pos.unavailable",
            ErrorCategory::Unavailable,
            "POS is temporarily unavailable.",
            true,
        ),
        PosApplicationError::Port(PortError::Internal)
        | PosApplicationError::Port(PortError::NotReady(_)) => (
            "pos.internal",
            ErrorCategory::Internal,
            "POS could not complete the operation.",
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
