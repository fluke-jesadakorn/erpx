mod records;

pub use records::{SalesLifecycleRecordApplicationError, SalesLifecycleRecordCommandHandler};

use async_trait::async_trait;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::{DimensionSet, crm, legal, sales};
use erpx_kernel::{
    ActorAttribution, Clock, CorrelationId, ErrorCategory, ErrorContext, KernelError, TenantScope,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        BillingEventId, FulfillmentId, PerformanceObligationDefinition, PerformanceObligationId,
        Quote, QuoteId, RevenueArrangement, RevenueArrangementId, SalesAggregate, SalesDomainError,
        SalesOrder, SalesOrderId, SalesOrderLine,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EntitlementPort, PortError, QuoteView, ReadRepository,
        RevenueArrangementView, SalesAction, SalesCommit, SalesOrderList, SalesOrderListFilter,
        SalesOrderView, StoredCommit, WriteRepository,
    },
};

pub const MAX_SALES_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateQuote {
    pub quote_id: QuoteId,
    pub scope: TenantScope,
    pub opportunity: crm::OpportunityFact,
    pub quote_code: String,
    pub currency: String,
    pub lines: Vec<SalesOrderLine>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
}

macro_rules! quote_command {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            pub quote_id: QuoteId,
        }
    };
}

quote_command!(SendQuote);
quote_command!(AcceptQuote);
quote_command!(RejectQuote);
quote_command!(ExpireQuote);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSalesOrder {
    pub sales_order_id: SalesOrderId,
    pub quote_id: QuoteId,
    pub expected_quote_version: u64,
    pub order_code: String,
}

macro_rules! order_command {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            pub sales_order_id: SalesOrderId,
        }
    };
}

order_command!(ConfirmSalesOrder);
order_command!(MarkSalesOrderPartiallyFulfilled);
order_command!(MarkSalesOrderFulfilled);
order_command!(CancelSalesOrder);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateRevenueArrangement {
    pub arrangement_id: RevenueArrangementId,
    pub sales_order_id: SalesOrderId,
    pub expected_sales_order_version: u64,
    pub legal_contract: Option<legal::LegalContractRef>,
    pub dimensions: DimensionSet,
    pub obligations: Vec<PerformanceObligationDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptRevenueFulfillment {
    pub arrangement_id: RevenueArrangementId,
    pub fulfillment_id: FulfillmentId,
    pub billing_event_id: BillingEventId,
    pub obligation_id: PerformanceObligationId,
    pub kind: sales::FulfillmentKind,
    pub accepted_minor: i64,
    pub tax_minor: i64,
    pub eligible_on: chrono::NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRevenueArrangement {
    pub arrangement_id: RevenueArrangementId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum SalesCommand {
    CreateQuote(CreateQuote),
    SendQuote(SendQuote),
    AcceptQuote(AcceptQuote),
    RejectQuote(RejectQuote),
    ExpireQuote(ExpireQuote),
    CreateSalesOrder(CreateSalesOrder),
    ConfirmSalesOrder(ConfirmSalesOrder),
    MarkSalesOrderPartiallyFulfilled(MarkSalesOrderPartiallyFulfilled),
    MarkSalesOrderFulfilled(MarkSalesOrderFulfilled),
    CancelSalesOrder(CancelSalesOrder),
    ActivateRevenueArrangement(ActivateRevenueArrangement),
    AcceptRevenueFulfillment(AcceptRevenueFulfillment),
    CancelRevenueArrangement(CancelRevenueArrangement),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetQuote {
    pub quote_id: QuoteId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSalesOrder {
    pub sales_order_id: SalesOrderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRevenueArrangement {
    pub arrangement_id: RevenueArrangementId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSalesOrders {
    pub filter: SalesOrderListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum SalesQuery {
    GetQuote(GetQuote),
    GetSalesOrder(GetSalesOrder),
    GetRevenueArrangement(GetRevenueArrangement),
    ListSalesOrders(ListSalesOrders),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum SalesMutationResult {
    Quote(QuoteView),
    SalesOrder(SalesOrderView),
    RevenueArrangement(Box<RevenueArrangementView>),
}

impl SalesMutationResult {
    fn from_aggregate(aggregate: &SalesAggregate) -> Self {
        match aggregate {
            SalesAggregate::Quote(value) => Self::Quote(value.into()),
            SalesAggregate::SalesOrder(value) => Self::SalesOrder(value.into()),
            SalesAggregate::RevenueArrangement(value) => {
                Self::RevenueArrangement(Box::new(value.as_ref().into()))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum SalesQueryResponse {
    Quote(Option<QuoteView>),
    SalesOrder(Option<SalesOrderView>),
    RevenueArrangement(Option<Box<RevenueArrangementView>>),
    SalesOrders(SalesOrderList),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SalesApplicationError {
    #[error(transparent)]
    Domain(#[from] SalesDomainError),
    #[error("command scope does not contain the Sales record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("Sales aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("the accepted quote changed before the Sales order was created")]
    QuoteVersionConflict { expected: u64, actual: u64 },
    #[error("the confirmed Sales order changed before the revenue arrangement was activated")]
    SalesOrderVersionConflict { expected: u64, actual: u64 },
    #[error("Sales page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("Sales integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct SalesCommandHandler<W, U, A, E, C> {
    write: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> SalesCommandHandler<W, U, A, E, C> {
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

impl<W, U, A, E, C> SalesCommandHandler<W, U, A, E, C>
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
        action: SalesAction,
    ) -> Result<(), SalesApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(SalesApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_sales_enabled(target_scope, action.requires_write_entitlement())
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
        actor: &ActorAttribution,
        authority_scope: &TenantScope,
        action: SalesAction,
    ) -> Result<Option<StoredCommit>, SalesApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(actor, authority_scope, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    // Preserve the bounded application error taxonomy at the private version fence.
    #[allow(clippy::result_large_err)]
    fn expected(expected: Option<u64>, actual: u64) -> Result<u64, SalesApplicationError> {
        let expected = expected.ok_or(SalesApplicationError::MissingExpectedVersion)?;
        if expected != actual {
            return Err(SalesApplicationError::VersionConflict { expected, actual });
        }
        Ok(expected)
    }

    async fn load_quote(
        &self,
        authority_scope: &TenantScope,
        quote_id: QuoteId,
    ) -> Result<Quote, SalesApplicationError> {
        self.write
            .load_quote(authority_scope, quote_id)
            .await?
            .ok_or_else(|| PortError::NotFound.into())
    }

    async fn load_order(
        &self,
        authority_scope: &TenantScope,
        sales_order_id: SalesOrderId,
    ) -> Result<SalesOrder, SalesApplicationError> {
        self.write
            .load_order(authority_scope, sales_order_id)
            .await?
            .ok_or_else(|| PortError::NotFound.into())
    }

    async fn load_revenue_arrangement(
        &self,
        authority_scope: &TenantScope,
        arrangement_id: RevenueArrangementId,
    ) -> Result<RevenueArrangement, SalesApplicationError> {
        self.write
            .load_revenue_arrangement(authority_scope, arrangement_id)
            .await?
            .ok_or_else(|| PortError::NotFound.into())
    }
}

#[derive(Debug, Clone, Copy)]
enum QuoteMutation {
    Send,
    Accept,
    Reject,
    Expire,
}

#[derive(Debug, Clone, Copy)]
enum OrderMutation {
    Confirm,
    PartiallyFulfill,
    Fulfill,
    Cancel,
}

#[derive(Debug, Clone)]
enum ArrangementMutation {
    Accept(AcceptRevenueFulfillment),
    Cancel,
}

#[derive(Debug)]
enum Execution {
    CreateQuote(CreateQuote),
    MutateQuote {
        quote_id: QuoteId,
        mutation: QuoteMutation,
    },
    CreateOrder(CreateSalesOrder),
    MutateOrder {
        sales_order_id: SalesOrderId,
        mutation: OrderMutation,
    },
    ActivateArrangement(ActivateRevenueArrangement),
    MutateArrangement {
        arrangement_id: RevenueArrangementId,
        mutation: ArrangementMutation,
    },
}

impl SalesCommand {
    fn execution(&self) -> (SalesAction, CommandFingerprint, Execution) {
        match self {
            Self::CreateQuote(command) => (
                SalesAction::CreateQuote,
                CommandFingerprint::CreateQuote {
                    quote_id: command.quote_id,
                    scope: command.scope.clone(),
                    opportunity: command.opportunity.clone(),
                    quote_code: command.quote_code.clone(),
                    currency: command.currency.clone(),
                    lines: command.lines.clone(),
                    valid_until: command.valid_until,
                },
                Execution::CreateQuote(command.clone()),
            ),
            Self::SendQuote(command) => (
                SalesAction::SendQuote,
                CommandFingerprint::SendQuote {
                    quote_id: command.quote_id,
                },
                Execution::MutateQuote {
                    quote_id: command.quote_id,
                    mutation: QuoteMutation::Send,
                },
            ),
            Self::AcceptQuote(command) => (
                SalesAction::AcceptQuote,
                CommandFingerprint::AcceptQuote {
                    quote_id: command.quote_id,
                },
                Execution::MutateQuote {
                    quote_id: command.quote_id,
                    mutation: QuoteMutation::Accept,
                },
            ),
            Self::RejectQuote(command) => (
                SalesAction::RejectQuote,
                CommandFingerprint::RejectQuote {
                    quote_id: command.quote_id,
                },
                Execution::MutateQuote {
                    quote_id: command.quote_id,
                    mutation: QuoteMutation::Reject,
                },
            ),
            Self::ExpireQuote(command) => (
                SalesAction::ExpireQuote,
                CommandFingerprint::ExpireQuote {
                    quote_id: command.quote_id,
                },
                Execution::MutateQuote {
                    quote_id: command.quote_id,
                    mutation: QuoteMutation::Expire,
                },
            ),
            Self::CreateSalesOrder(command) => (
                SalesAction::CreateOrder,
                CommandFingerprint::CreateOrder {
                    sales_order_id: command.sales_order_id,
                    quote_id: command.quote_id,
                    expected_quote_version: command.expected_quote_version,
                    order_code: command.order_code.clone(),
                },
                Execution::CreateOrder(command.clone()),
            ),
            Self::ConfirmSalesOrder(command) => (
                SalesAction::ConfirmOrder,
                CommandFingerprint::ConfirmOrder {
                    sales_order_id: command.sales_order_id,
                },
                Execution::MutateOrder {
                    sales_order_id: command.sales_order_id,
                    mutation: OrderMutation::Confirm,
                },
            ),
            Self::MarkSalesOrderPartiallyFulfilled(command) => (
                SalesAction::PartiallyFulfillOrder,
                CommandFingerprint::PartiallyFulfillOrder {
                    sales_order_id: command.sales_order_id,
                },
                Execution::MutateOrder {
                    sales_order_id: command.sales_order_id,
                    mutation: OrderMutation::PartiallyFulfill,
                },
            ),
            Self::MarkSalesOrderFulfilled(command) => (
                SalesAction::FulfillOrder,
                CommandFingerprint::FulfillOrder {
                    sales_order_id: command.sales_order_id,
                },
                Execution::MutateOrder {
                    sales_order_id: command.sales_order_id,
                    mutation: OrderMutation::Fulfill,
                },
            ),
            Self::CancelSalesOrder(command) => (
                SalesAction::CancelOrder,
                CommandFingerprint::CancelOrder {
                    sales_order_id: command.sales_order_id,
                },
                Execution::MutateOrder {
                    sales_order_id: command.sales_order_id,
                    mutation: OrderMutation::Cancel,
                },
            ),
            Self::ActivateRevenueArrangement(command) => (
                SalesAction::ActivateRevenueArrangement,
                CommandFingerprint::ActivateRevenueArrangement {
                    arrangement_id: command.arrangement_id,
                    sales_order_id: command.sales_order_id,
                    expected_sales_order_version: command.expected_sales_order_version,
                    legal_contract: command.legal_contract,
                    dimensions: command.dimensions.clone(),
                    obligations: command.obligations.clone(),
                },
                Execution::ActivateArrangement(command.clone()),
            ),
            Self::AcceptRevenueFulfillment(command) => (
                SalesAction::AcceptRevenueFulfillment,
                CommandFingerprint::AcceptRevenueFulfillment {
                    arrangement_id: command.arrangement_id,
                    fulfillment_id: command.fulfillment_id,
                    billing_event_id: command.billing_event_id,
                    obligation_id: command.obligation_id,
                    kind: command.kind,
                    accepted_minor: command.accepted_minor,
                    tax_minor: command.tax_minor,
                    eligible_on: command.eligible_on,
                },
                Execution::MutateArrangement {
                    arrangement_id: command.arrangement_id,
                    mutation: ArrangementMutation::Accept(command.clone()),
                },
            ),
            Self::CancelRevenueArrangement(command) => (
                SalesAction::CancelRevenueArrangement,
                CommandFingerprint::CancelRevenueArrangement {
                    arrangement_id: command.arrangement_id,
                },
                Execution::MutateArrangement {
                    arrangement_id: command.arrangement_id,
                    mutation: ArrangementMutation::Cancel,
                },
            ),
        }
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<SalesCommand> for SalesCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = SalesMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<SalesCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let (action, fingerprint, execution) = envelope.command.execution();
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key,
            fingerprint,
        };
        if let Some(stored) = self
            .replay(&identity, &envelope.actor, &envelope.tenant_scope, action)
            .await
            .map_err(|error| to_cqrs(error, correlation_id))?
        {
            let response = SalesMutationResult::from_aggregate(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }

        let now = self.clock.now();
        let (mut aggregate, expected_version) = match execution {
            Execution::CreateQuote(command) => {
                if envelope.expected_aggregate_version.is_some() {
                    return Err(to_cqrs(
                        SalesApplicationError::UnexpectedCreateVersion,
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
                let quote = Quote::create_from_opportunity(
                    command.quote_id,
                    command.scope,
                    command.opportunity,
                    command.quote_code,
                    command.currency,
                    command.lines,
                    command.valid_until,
                    now,
                )
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                (SalesAggregate::Quote(quote), None)
            }
            Execution::MutateQuote { quote_id, mutation } => {
                let mut quote = self
                    .load_quote(&envelope.tenant_scope, quote_id)
                    .await
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    quote.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                let expected = Self::expected(envelope.expected_aggregate_version, quote.version())
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                match mutation {
                    QuoteMutation::Send => quote.send(now),
                    QuoteMutation::Accept => quote.accept(now),
                    QuoteMutation::Reject => quote.reject(now),
                    QuoteMutation::Expire => quote.expire(now),
                }
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                (SalesAggregate::Quote(quote), Some(expected))
            }
            Execution::CreateOrder(command) => {
                if envelope.expected_aggregate_version.is_some() {
                    return Err(to_cqrs(
                        SalesApplicationError::UnexpectedCreateVersion,
                        correlation_id,
                    ));
                }
                let quote = self
                    .load_quote(&envelope.tenant_scope, command.quote_id)
                    .await
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    quote.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                if command.expected_quote_version != quote.version() {
                    return Err(to_cqrs(
                        SalesApplicationError::QuoteVersionConflict {
                            expected: command.expected_quote_version,
                            actual: quote.version(),
                        },
                        correlation_id,
                    ));
                }
                let order = SalesOrder::create_from_quote(
                    command.sales_order_id,
                    &quote,
                    command.order_code,
                    now,
                )
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                (SalesAggregate::SalesOrder(order), None)
            }
            Execution::MutateOrder {
                sales_order_id,
                mutation,
            } => {
                let mut order = self
                    .load_order(&envelope.tenant_scope, sales_order_id)
                    .await
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    order.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                let expected = Self::expected(envelope.expected_aggregate_version, order.version())
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                match mutation {
                    OrderMutation::Confirm => order.confirm(now),
                    OrderMutation::PartiallyFulfill => order.mark_partially_fulfilled(now),
                    OrderMutation::Fulfill => order.mark_fulfilled(now),
                    OrderMutation::Cancel => order.cancel(now),
                }
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                (SalesAggregate::SalesOrder(order), Some(expected))
            }
            Execution::ActivateArrangement(command) => {
                if envelope.expected_aggregate_version.is_some() {
                    return Err(to_cqrs(
                        SalesApplicationError::UnexpectedCreateVersion,
                        correlation_id,
                    ));
                }
                let order = self
                    .load_order(&envelope.tenant_scope, command.sales_order_id)
                    .await
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    order.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                if command.expected_sales_order_version != order.version() {
                    return Err(to_cqrs(
                        SalesApplicationError::SalesOrderVersionConflict {
                            expected: command.expected_sales_order_version,
                            actual: order.version(),
                        },
                        correlation_id,
                    ));
                }
                let arrangement = RevenueArrangement::activate_from_order(
                    command.arrangement_id,
                    &order,
                    command.legal_contract,
                    command.dimensions,
                    command.obligations,
                    now,
                )
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                (
                    SalesAggregate::RevenueArrangement(Box::new(arrangement)),
                    None,
                )
            }
            Execution::MutateArrangement {
                arrangement_id,
                mutation,
            } => {
                let mut arrangement = self
                    .load_revenue_arrangement(&envelope.tenant_scope, arrangement_id)
                    .await
                    .map_err(|error| to_cqrs(error, correlation_id))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    arrangement.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation_id))?;
                let expected =
                    Self::expected(envelope.expected_aggregate_version, arrangement.version())
                        .map_err(|error| to_cqrs(error, correlation_id))?;
                match mutation {
                    ArrangementMutation::Accept(command) => arrangement.accept_fulfillment(
                        command.fulfillment_id,
                        command.billing_event_id,
                        command.obligation_id,
                        command.kind,
                        command.accepted_minor,
                        command.tax_minor,
                        command.eligible_on,
                        now,
                    ),
                    ArrangementMutation::Cancel => arrangement.cancel(now),
                }
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                (
                    SalesAggregate::RevenueArrangement(Box::new(arrangement)),
                    Some(expected),
                )
            }
        };

        let integration_events =
            integration::published_events(&aggregate.take_uncommitted_events())
                .map_err(|_| to_cqrs(SalesApplicationError::IntegrationMapping, correlation_id))?;
        let stored = self
            .unit_of_work
            .commit(SalesCommit {
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
        let response = SalesMutationResult::from_aggregate(&stored.aggregate);
        Ok(stored.into_receipt(response))
    }
}

pub struct SalesQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> SalesQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

impl<R, A, E> SalesQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    async fn preflight(
        &self,
        actor: &ActorAttribution,
        scope: &TenantScope,
        action: SalesAction,
    ) -> Result<(), PortError> {
        self.entitlement.ensure_sales_enabled(scope, false).await?;
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
impl<R, A, E> QueryHandler<SalesQuery> for SalesQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = SalesQueryResponse;

    async fn handle(
        &self,
        envelope: QueryEnvelope<SalesQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation_id = envelope.correlation_id;
        let (response, projection) = match &envelope.query {
            SalesQuery::GetQuote(command) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    SalesAction::ViewQuote,
                )
                .await
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                let result = self
                    .read
                    .get_quote(
                        &envelope.tenant_scope,
                        command.quote_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                if result
                    .value
                    .as_ref()
                    .is_some_and(|value| !envelope.tenant_scope.contains(&value.scope))
                {
                    return Err(to_cqrs(
                        SalesApplicationError::ScopeOutsideAuthority,
                        correlation_id,
                    ));
                }
                (SalesQueryResponse::Quote(result.value), result.metadata)
            }
            SalesQuery::GetSalesOrder(command) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    SalesAction::ViewOrder,
                )
                .await
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                let result = self
                    .read
                    .get_order(
                        &envelope.tenant_scope,
                        command.sales_order_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                if result
                    .value
                    .as_ref()
                    .is_some_and(|value| !envelope.tenant_scope.contains(&value.scope))
                {
                    return Err(to_cqrs(
                        SalesApplicationError::ScopeOutsideAuthority,
                        correlation_id,
                    ));
                }
                (
                    SalesQueryResponse::SalesOrder(result.value),
                    result.metadata,
                )
            }
            SalesQuery::GetRevenueArrangement(command) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    SalesAction::ViewRevenueArrangement,
                )
                .await
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                let result = self
                    .read
                    .get_revenue_arrangement(
                        &envelope.tenant_scope,
                        command.arrangement_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                if result
                    .value
                    .as_ref()
                    .is_some_and(|value| !envelope.tenant_scope.contains(&value.scope))
                {
                    return Err(to_cqrs(
                        SalesApplicationError::ScopeOutsideAuthority,
                        correlation_id,
                    ));
                }
                (
                    SalesQueryResponse::RevenueArrangement(result.value.map(Box::new)),
                    result.metadata,
                )
            }
            SalesQuery::ListSalesOrders(command) => {
                if !(1..=MAX_SALES_PAGE_SIZE).contains(&command.filter.page_size) {
                    return Err(to_cqrs(
                        SalesApplicationError::InvalidPageSize,
                        correlation_id,
                    ));
                }
                self.preflight(
                    &envelope.actor,
                    &envelope.tenant_scope,
                    SalesAction::ListOrders,
                )
                .await
                .map_err(|error| to_cqrs(error.into(), correlation_id))?;
                let result = self
                    .read
                    .list_orders(
                        &envelope.tenant_scope,
                        &command.filter,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation_id))?;
                if result
                    .value
                    .orders
                    .iter()
                    .any(|value| !envelope.tenant_scope.contains(&value.scope))
                {
                    return Err(to_cqrs(
                        SalesApplicationError::ScopeOutsideAuthority,
                        correlation_id,
                    ));
                }
                (
                    SalesQueryResponse::SalesOrders(result.value),
                    result.metadata,
                )
            }
        };

        Ok(QueryResult {
            value: response,
            metadata: QueryMetadata {
                correlation_id,
                projection,
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

fn to_cqrs(error: SalesApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        SalesApplicationError::Domain(_)
        | SalesApplicationError::UnexpectedCreateVersion
        | SalesApplicationError::MissingExpectedVersion
        | SalesApplicationError::InvalidPageSize
        | SalesApplicationError::IntegrationMapping => (
            "sales.invalid",
            ErrorCategory::Invalid,
            "The Sales request is invalid.",
            false,
        ),
        SalesApplicationError::ScopeOutsideAuthority
        | SalesApplicationError::Port(PortError::Denied)
        | SalesApplicationError::Port(PortError::EntitlementDenied) => (
            "sales.denied",
            ErrorCategory::Denied,
            "The Sales operation is not permitted.",
            false,
        ),
        SalesApplicationError::VersionConflict { .. }
        | SalesApplicationError::QuoteVersionConflict { .. }
        | SalesApplicationError::SalesOrderVersionConflict { .. }
        | SalesApplicationError::Port(PortError::Conflict { .. })
        | SalesApplicationError::Port(PortError::IdempotencyConflict) => (
            "sales.conflict",
            ErrorCategory::Conflict,
            "The Sales record changed or the command identity conflicts.",
            false,
        ),
        SalesApplicationError::Port(PortError::NotFound) => (
            "sales.not_found",
            ErrorCategory::NotFound,
            "The Sales record was not found.",
            false,
        ),
        SalesApplicationError::Port(PortError::Unavailable) => (
            "sales.unavailable",
            ErrorCategory::Unavailable,
            "Sales is temporarily unavailable.",
            true,
        ),
        SalesApplicationError::Port(PortError::Internal)
        | SalesApplicationError::Port(PortError::NotReady(_)) => (
            "sales.internal",
            ErrorCategory::Internal,
            "Sales could not complete the operation.",
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
