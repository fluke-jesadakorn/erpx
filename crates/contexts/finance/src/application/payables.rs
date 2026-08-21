use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_integration_contracts::{MoneyAmount, RecordRef, procurement};
use erpx_kernel::{
    AuthorityScope, Clock, CorrelationId, ErrorCategory, ErrorContext, HumanDecisionProof,
    KernelError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        PayablesAggregate, PayablesDomainError, PayablesDomainEvent, Payment, PaymentAllocation,
        PaymentId, ThreeWayMatchTolerance, VendorInvoice, VendorInvoiceId, VendorInvoiceLine,
        payment_release_decision_ref, vendor_invoice_post_decision_ref,
    },
    ports::{
        PayableView, PayablesAction, PayablesAuthorizationPort, PayablesAuthorizationRequest,
        PayablesCommandFingerprint, PayablesCommandIdentity, PayablesCommandUnitOfWork,
        PayablesCommit, PayablesEntitlementPort, PayablesReadRepository, PayablesReferenceVersion,
        PayablesWriteRepository, PaymentView, PortError, StoredPayablesCommit,
        ThreeWayMatchEvidencePort, VendorInvoiceView,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateVendorInvoice {
    pub vendor_invoice_id: VendorInvoiceId,
    pub scope: AuthorityScope,
    pub invoice_number: String,
    pub supplier: procurement::SupplierRef,
    pub purchase_order: RecordRef,
    pub receipt: RecordRef,
    pub tolerance: ThreeWayMatchTolerance,
    pub lines: Vec<VendorInvoiceLine>,
    pub tax_amount: MoneyAmount,
    pub due_on: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostVendorInvoice {
    pub vendor_invoice_id: VendorInvoiceId,
    pub purchase_order: RecordRef,
    pub receipt: RecordRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePayment {
    pub payment_id: PaymentId,
    pub scope: AuthorityScope,
    pub payment_code: String,
    pub supplier: procurement::SupplierRef,
    pub amount: MoneyAmount,
    pub allocations: Vec<PaymentAllocation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePayment {
    pub payment_id: PaymentId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum PayablesCommand {
    CreateVendorInvoice(CreateVendorInvoice),
    PostVendorInvoice(PostVendorInvoice),
    CreatePayment(CreatePayment),
    ReleasePayment(ReleasePayment),
}

impl PayablesCommand {
    pub const fn action(&self) -> PayablesAction {
        match self {
            Self::CreateVendorInvoice(_) => PayablesAction::CreateVendorInvoice,
            Self::PostVendorInvoice(_) => PayablesAction::PostVendorInvoice,
            Self::CreatePayment(_) => PayablesAction::CreatePayment,
            Self::ReleasePayment(_) => PayablesAction::ReleasePayment,
        }
    }

    pub const fn is_create(&self) -> bool {
        matches!(self, Self::CreateVendorInvoice(_) | Self::CreatePayment(_))
    }

    pub fn decision_ref(&self, command_id: erpx_kernel::CommandId) -> Option<String> {
        match self {
            Self::PostVendorInvoice(value) => Some(vendor_invoice_post_decision_ref(
                command_id,
                value.vendor_invoice_id,
            )),
            Self::ReleasePayment(value) => {
                Some(payment_release_decision_ref(command_id, value.payment_id))
            }
            _ => None,
        }
    }

    pub fn fingerprint(&self) -> PayablesCommandFingerprint {
        match self {
            Self::CreateVendorInvoice(value) => PayablesCommandFingerprint::CreateVendorInvoice {
                vendor_invoice_id: value.vendor_invoice_id,
                scope: value.scope.clone(),
                invoice_number: value.invoice_number.clone(),
                supplier: value.supplier,
                purchase_order: value.purchase_order.clone(),
                receipt: value.receipt.clone(),
                tolerance: value.tolerance,
                lines: value.lines.clone(),
                tax_amount: value.tax_amount,
                due_on: value.due_on,
            },
            Self::PostVendorInvoice(value) => PayablesCommandFingerprint::PostVendorInvoice {
                vendor_invoice_id: value.vendor_invoice_id,
                purchase_order: value.purchase_order.clone(),
                receipt: value.receipt.clone(),
            },
            Self::CreatePayment(value) => PayablesCommandFingerprint::CreatePayment {
                payment_id: value.payment_id,
                scope: value.scope.clone(),
                payment_code: value.payment_code.clone(),
                supplier: value.supplier,
                amount: value.amount,
                allocations: value.allocations.clone(),
            },
            Self::ReleasePayment(value) => PayablesCommandFingerprint::ReleasePayment {
                payment_id: value.payment_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum PayablesMutationResult {
    VendorInvoice(VendorInvoiceView),
    Payment(PaymentView),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetVendorInvoice {
    pub vendor_invoice_id: VendorInvoiceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPayment {
    pub payment_id: PaymentId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum PayablesQuery {
    GetVendorInvoice(GetVendorInvoice),
    GetPayment(GetPayment),
}

impl PayablesQuery {
    const fn action(self) -> PayablesAction {
        match self {
            Self::GetVendorInvoice(_) => PayablesAction::ViewVendorInvoice,
            Self::GetPayment(_) => PayablesAction::ViewPayment,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // Typed query payloads stay direct for RPC mapping.
pub enum PayablesQueryResponse {
    VendorInvoice {
        vendor_invoice: Option<VendorInvoiceView>,
        payable: Option<PayableView>,
    },
    Payment(Option<PaymentView>),
}

impl From<&PayablesAggregate> for PayablesMutationResult {
    fn from(value: &PayablesAggregate) -> Self {
        match value {
            PayablesAggregate::VendorInvoice(value) => Self::VendorInvoice(value.into()),
            PayablesAggregate::Payment(value) => Self::Payment(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PayablesApplicationError {
    #[error(transparent)]
    Domain(#[from] PayablesDomainError),
    #[error("command scope does not contain the payables record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutations require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("payables aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("exact three-way Procurement evidence is unavailable")]
    EvidenceUnavailable,
    #[error("payables decision requires a directly authenticated human")]
    HumanDecisionRequired,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct PayablesCommandHandler<W, U, A, E, V, C> {
    write: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    evidence: V,
    clock: C,
}

impl<W, U, A, E, V, C> PayablesCommandHandler<W, U, A, E, V, C> {
    pub const fn new(
        write: W,
        unit_of_work: U,
        authorization: A,
        entitlement: E,
        evidence: V,
        clock: C,
    ) -> Self {
        Self {
            write,
            unit_of_work,
            authorization,
            entitlement,
            evidence,
            clock,
        }
    }
}

pub struct PayablesQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> PayablesQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<PayablesQuery> for PayablesQueryHandler<R, A, E>
where
    R: PayablesReadRepository,
    A: PayablesAuthorizationPort,
    E: PayablesEntitlementPort,
{
    type Response = PayablesQueryResponse;

    async fn handle(
        &self,
        envelope: QueryEnvelope<PayablesQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.query.action();
        self.entitlement
            .ensure_payables_enabled(&envelope.active_scope, false)
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?;
        self.authorization
            .authorize(PayablesAuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
            })
            .await
            .map_err(|error| port_to_cqrs(error, correlation))?;
        let (value, projection) = match envelope.query {
            PayablesQuery::GetVendorInvoice(query) => {
                let projection = self
                    .read
                    .get_vendor_invoice(
                        &envelope.active_scope,
                        query.vendor_invoice_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                if projection
                    .value
                    .as_ref()
                    .is_some_and(|record| !envelope.active_scope.contains(&record.scope))
                {
                    return Err(to_cqrs(
                        PayablesApplicationError::ScopeOutsideAuthority,
                        correlation,
                    ));
                }
                let payable = projection
                    .value
                    .as_ref()
                    .and_then(VendorInvoiceView::posted_payable);
                (
                    PayablesQueryResponse::VendorInvoice {
                        vendor_invoice: projection.value,
                        payable,
                    },
                    projection.metadata,
                )
            }
            PayablesQuery::GetPayment(query) => {
                let projection = self
                    .read
                    .get_payment(
                        &envelope.active_scope,
                        query.payment_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                if projection
                    .value
                    .as_ref()
                    .is_some_and(|record| !envelope.active_scope.contains(&record.scope))
                {
                    return Err(to_cqrs(
                        PayablesApplicationError::ScopeOutsideAuthority,
                        correlation,
                    ));
                }
                (
                    PayablesQueryResponse::Payment(projection.value),
                    projection.metadata,
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

impl<W, U, A, E, V, C> PayablesCommandHandler<W, U, A, E, V, C>
where
    W: PayablesWriteRepository,
    U: PayablesCommandUnitOfWork,
    A: PayablesAuthorizationPort,
    E: PayablesEntitlementPort,
    V: ThreeWayMatchEvidencePort,
    C: Clock,
{
    async fn preflight(
        &self,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: PayablesAction,
    ) -> Result<(), PayablesApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(PayablesApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_payables_enabled(target_scope, action.requires_write())
            .await?;
        self.authorization
            .authorize(PayablesAuthorizationRequest {
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
        identity: &PayablesCommandIdentity,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &AuthorityScope,
        action: PayablesAction,
    ) -> Result<Option<StoredPayablesCommit>, PayablesApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(actor, authority_scope, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    async fn load_payables(
        &self,
        scope: &AuthorityScope,
        allocations: &[PaymentAllocation],
    ) -> Result<Vec<crate::domain::Payable>, PayablesApplicationError> {
        let references = allocations
            .iter()
            .map(|allocation| (allocation.payable_id, allocation.payable_version))
            .collect::<Vec<_>>();
        let values = self.write.load_payables_exact(scope, &references).await?;
        if values.len() != references.len() {
            return Err(PayablesApplicationError::Port(PortError::NotFound));
        }
        Ok(values)
    }
}

#[async_trait]
impl<W, U, A, E, V, C> CommandHandler<PayablesCommand> for PayablesCommandHandler<W, U, A, E, V, C>
where
    W: PayablesWriteRepository,
    U: PayablesCommandUnitOfWork,
    A: PayablesAuthorizationPort,
    E: PayablesEntitlementPort,
    V: ThreeWayMatchEvidencePort,
    C: Clock,
{
    type Response = PayablesMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<PayablesCommand>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.command.action();
        let at = self.clock.now();
        // Construct this before receipt lookup or aggregate access. Agents,
        // services, represented users, and sessionless humans cannot replay a
        // human-only posting/release command.
        let mut decision = envelope
            .command
            .decision_ref(envelope.command_id)
            .map(|decision_ref| {
                HumanDecisionProof::try_new(
                    envelope.actor.clone(),
                    envelope.active_scope.clone(),
                    decision_ref,
                    at,
                )
                .map_err(|_| PayablesApplicationError::HumanDecisionRequired)
            })
            .transpose()
            .map_err(|error| to_cqrs(error, correlation))?;
        let identity = PayablesCommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: envelope.command.fingerprint(),
        };
        if let Some(stored) = self
            .replay(&identity, &envelope.actor, &envelope.active_scope, action)
            .await
            .map_err(|error| to_cqrs(error, correlation))?
        {
            let response = PayablesMutationResult::from(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }
        if envelope.command.is_create() && envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                PayablesApplicationError::UnexpectedCreateVersion,
                correlation,
            ));
        }

        let mut evidence = None;
        let mut payable = None;
        let mut references = Vec::new();
        let mut events = Vec::new();
        let (aggregate, expected_version) = match envelope.command {
            PayablesCommand::CreateVendorInvoice(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &value.scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let loaded = self
                    .evidence
                    .load_exact(&value.scope, &value.purchase_order, &value.receipt)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| {
                        to_cqrs(PayablesApplicationError::EvidenceUnavailable, correlation)
                    })?;
                if loaded.purchase_order != value.purchase_order || loaded.receipt != value.receipt
                {
                    return Err(to_cqrs(
                        PayablesApplicationError::EvidenceUnavailable,
                        correlation,
                    ));
                }
                references.push(PayablesReferenceVersion::ProcurementEvidence {
                    purchase_order: value.purchase_order,
                    receipt: value.receipt,
                });
                let invoice = VendorInvoice::create(
                    value.vendor_invoice_id,
                    value.scope,
                    value.invoice_number,
                    value.supplier,
                    loaded.clone(),
                    value.tolerance,
                    value.lines,
                    value.tax_amount,
                    value.due_on,
                    envelope.actor.principal_ref.clone(),
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                evidence = Some(loaded);
                (PayablesAggregate::VendorInvoice(invoice), None)
            }
            PayablesCommand::PostVendorInvoice(value) => {
                let mut invoice = self
                    .write
                    .load_vendor_invoice(&envelope.active_scope, value.vendor_invoice_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    invoice.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let expected =
                    require_expected(envelope.expected_aggregate_version, invoice.version())
                        .map_err(|error| to_cqrs(error, correlation))?;
                invoice
                    .post(
                        decision.take().ok_or_else(|| {
                            to_cqrs(PayablesApplicationError::HumanDecisionRequired, correlation)
                        })?,
                        envelope.command_id,
                        &value.purchase_order,
                        &value.receipt,
                        at,
                    )
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                let posted = invoice
                    .posted_state()
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| {
                        to_cqrs(PayablesApplicationError::EvidenceUnavailable, correlation)
                    })?;
                payable = Some(posted.payable.clone());
                events.push(PayablesDomainEvent::VendorInvoicePosted(posted.clone()));
                events.push(PayablesDomainEvent::PayablePosted(posted.payable));
                references.push(PayablesReferenceVersion::ProcurementEvidence {
                    purchase_order: value.purchase_order,
                    receipt: value.receipt,
                });
                (PayablesAggregate::VendorInvoice(invoice), Some(expected))
            }
            PayablesCommand::CreatePayment(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &value.scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let payables = self
                    .load_payables(&value.scope, &value.allocations)
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                references.extend(value.allocations.iter().map(|allocation| {
                    PayablesReferenceVersion::Payable {
                        payable_id: allocation.payable_id,
                        version: allocation.payable_version,
                    }
                }));
                let payment = Payment::create(
                    value.payment_id,
                    value.scope,
                    value.payment_code,
                    value.supplier,
                    value.amount,
                    value.allocations,
                    &payables,
                    envelope.actor.principal_ref.clone(),
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (PayablesAggregate::Payment(payment), None)
            }
            PayablesCommand::ReleasePayment(value) => {
                let mut payment = self
                    .write
                    .load_payment(&envelope.active_scope, value.payment_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    payment.scope(),
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let expected =
                    require_expected(envelope.expected_aggregate_version, payment.version())
                        .map_err(|error| to_cqrs(error, correlation))?;
                let payables = self
                    .load_payables(payment.scope(), payment.allocations())
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                references.extend(payment.allocations().iter().map(|allocation| {
                    PayablesReferenceVersion::Payable {
                        payable_id: allocation.payable_id,
                        version: allocation.payable_version,
                    }
                }));
                payment
                    .release(
                        decision.take().ok_or_else(|| {
                            to_cqrs(PayablesApplicationError::HumanDecisionRequired, correlation)
                        })?,
                        envelope.command_id,
                        &payables,
                        at,
                    )
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                let released = payment
                    .released_state()
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| {
                        to_cqrs(PayablesApplicationError::HumanDecisionRequired, correlation)
                    })?;
                events.push(PayablesDomainEvent::PaymentReleased(released));
                (PayablesAggregate::Payment(payment), Some(expected))
            }
        };
        let response = PayablesMutationResult::from(&aggregate);
        let stored = self
            .unit_of_work
            .commit(PayablesCommit {
                identity,
                correlation_id: envelope.correlation_id,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                expected_version,
                references,
                aggregate,
                payable,
                evidence,
                domain_events: events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;
        Ok(stored.into_receipt(response))
    }
}

fn require_expected(expected: Option<u64>, actual: u64) -> Result<u64, PayablesApplicationError> {
    let expected = expected.ok_or(PayablesApplicationError::MissingExpectedVersion)?;
    if expected != actual {
        return Err(PayablesApplicationError::VersionConflict { expected, actual });
    }
    Ok(expected)
}

fn to_cqrs(error: PayablesApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, safe_message, retryable) = match &error {
        PayablesApplicationError::Domain(_)
        | PayablesApplicationError::UnexpectedCreateVersion
        | PayablesApplicationError::MissingExpectedVersion => (
            "finance.payables.invalid",
            ErrorCategory::Invalid,
            "The Finance payables request is invalid.",
            false,
        ),
        PayablesApplicationError::ScopeOutsideAuthority
        | PayablesApplicationError::HumanDecisionRequired
        | PayablesApplicationError::Port(PortError::Denied)
        | PayablesApplicationError::Port(PortError::EntitlementDenied) => (
            "finance.payables.denied",
            ErrorCategory::Denied,
            "The Finance payables operation is not permitted.",
            false,
        ),
        PayablesApplicationError::VersionConflict { .. }
        | PayablesApplicationError::Port(PortError::Conflict { .. })
        | PayablesApplicationError::Port(PortError::IdempotencyConflict) => (
            "finance.payables.conflict",
            ErrorCategory::Conflict,
            "The Finance payables record changed or the command identity conflicts.",
            false,
        ),
        PayablesApplicationError::EvidenceUnavailable
        | PayablesApplicationError::Port(PortError::NotFound) => (
            "finance.payables.not_found",
            ErrorCategory::NotFound,
            "The Finance payables record or exact evidence is unavailable.",
            false,
        ),
        PayablesApplicationError::Port(PortError::Unavailable) => (
            "finance.payables.unavailable",
            ErrorCategory::Unavailable,
            "Finance payables is temporarily unavailable.",
            true,
        ),
        PayablesApplicationError::Port(PortError::Internal)
        | PayablesApplicationError::Port(PortError::NotReady(_)) => (
            "finance.payables.internal",
            ErrorCategory::Internal,
            "Finance payables could not complete the operation.",
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

fn port_to_cqrs(error: PortError, correlation_id: CorrelationId) -> CqrsError {
    match error {
        PortError::NotReady(value) => CqrsError::NotReady(value),
        other => to_cqrs(other.into(), correlation_id),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::{TimeZone, Utc};
    use erpx_cqrs::{CONTRACT_VERSION, EventPosition, IdempotencyKey};
    use erpx_integration_contracts::{BoundedContext, CurrencyCode, Quantity, ReferenceCode};
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuditContext, CommandId, EventId, FixedClock, SessionId,
    };
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        Payable, PayableId, PaymentStatus, ThreeWayMatchEvidence, ThreeWayMatchLineEvidence,
        VendorInvoiceLineId,
    };

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).single().unwrap()
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn actor(principal: &str) -> ActorAttribution {
        ActorAttribution {
            principal_ref: principal.into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(id(3)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    fn money(minor_units: i64) -> MoneyAmount {
        MoneyAmount::new(minor_units, CurrencyCode::try_new("THB").unwrap())
    }

    fn evidence() -> ThreeWayMatchEvidence {
        ThreeWayMatchEvidence {
            purchase_order: RecordRef::try_new(
                BoundedContext::Procurement,
                "purchase_order",
                id(10),
                4,
            )
            .unwrap(),
            receipt: RecordRef::try_new(BoundedContext::Procurement, "receipt", id(11), 3).unwrap(),
            supplier: procurement::SupplierRef::try_from_uuid(id(12)).unwrap(),
            lines: vec![ThreeWayMatchLineEvidence {
                purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(id(13))
                    .unwrap(),
                receipt_line_id: id(14),
                item_code: ReferenceCode::try_new("ITEM-1").unwrap(),
                ordered_quantity: Quantity::try_new(100, 0).unwrap(),
                received_quantity: Quantity::try_new(100, 0).unwrap(),
                ordered_amount: money(10_000),
                received_amount: money(10_000),
            }],
            captured_at: at(),
        }
    }

    fn invoice_line() -> VendorInvoiceLine {
        VendorInvoiceLine {
            line_id: VendorInvoiceLineId::try_from_uuid(id(15)).unwrap(),
            purchase_order_line: procurement::PurchaseOrderLineRef::try_from_uuid(id(13)).unwrap(),
            receipt_line_id: id(14),
            invoiced_quantity: Quantity::try_new(100, 0).unwrap(),
            invoiced_amount: money(10_000),
        }
    }

    fn create_invoice_command() -> PayablesCommand {
        let evidence = evidence();
        PayablesCommand::CreateVendorInvoice(CreateVendorInvoice {
            vendor_invoice_id: VendorInvoiceId::try_from_uuid(id(20)).unwrap(),
            scope: scope(),
            invoice_number: "VIN-2026-001".into(),
            supplier: evidence.supplier,
            purchase_order: evidence.purchase_order,
            receipt: evidence.receipt,
            tolerance: ThreeWayMatchTolerance {
                quantity_basis_points: 0,
                amount_basis_points: 0,
                absolute_amount_minor: 0,
            },
            lines: vec![invoice_line()],
            tax_amount: money(700),
            due_on: at().date_naive(),
        })
    }

    fn draft_invoice() -> VendorInvoice {
        let value = evidence();
        VendorInvoice::create(
            VendorInvoiceId::try_from_uuid(id(20)).unwrap(),
            scope(),
            "VIN-2026-001".into(),
            value.supplier,
            value,
            ThreeWayMatchTolerance {
                quantity_basis_points: 0,
                amount_basis_points: 0,
                absolute_amount_minor: 0,
            },
            vec![invoice_line()],
            money(700),
            at().date_naive(),
            "invoice-preparer".into(),
            at(),
        )
        .unwrap()
    }

    fn payable() -> Payable {
        Payable::try_new(
            PayableId::try_from_uuid(id(30)).unwrap(),
            scope(),
            VendorInvoiceId::try_from_uuid(id(20)).unwrap(),
            2,
            evidence().supplier,
            money(10_700),
            at(),
        )
        .unwrap()
    }

    fn draft_payment(value: &Payable) -> Payment {
        Payment::create(
            PaymentId::try_from_uuid(id(40)).unwrap(),
            scope(),
            "PAY-2026-001".into(),
            value.supplier(),
            value.amount(),
            vec![PaymentAllocation {
                payable_id: value.id(),
                payable_version: value.version(),
                amount: value.amount(),
            }],
            std::slice::from_ref(value),
            "payment-preparer".into(),
            at(),
        )
        .unwrap()
    }

    fn envelope(
        sequence: u128,
        expected: Option<u64>,
        command: PayablesCommand,
    ) -> CommandEnvelope<PayablesCommand> {
        CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
            idempotency_key: IdempotencyKey::try_new(format!("finance-payables-{sequence}"))
                .unwrap(),
            correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
            causation_id: None,
            actor: actor("finance-controller"),
            active_scope: scope(),
            audit: AuditContext {
                purpose: "finance.payables.test".into(),
                customer_visible: false,
                module: Some("finance".into()),
            },
            expected_aggregate_version: expected,
            command,
        }
    }

    #[derive(Clone)]
    struct FakeAuthorization(bool);

    #[async_trait]
    impl PayablesAuthorizationPort for FakeAuthorization {
        async fn authorize(
            &self,
            _request: PayablesAuthorizationRequest<'_>,
        ) -> Result<(), PortError> {
            if self.0 {
                Ok(())
            } else {
                Err(PortError::Denied)
            }
        }
    }

    #[derive(Clone)]
    struct FakeEntitlement(bool);

    #[async_trait]
    impl PayablesEntitlementPort for FakeEntitlement {
        async fn ensure_payables_enabled(
            &self,
            _scope: &AuthorityScope,
            _write: bool,
        ) -> Result<(), PortError> {
            if self.0 {
                Ok(())
            } else {
                Err(PortError::EntitlementDenied)
            }
        }
    }

    #[derive(Clone)]
    struct FakeEvidence(ThreeWayMatchEvidence);

    #[async_trait]
    impl ThreeWayMatchEvidencePort for FakeEvidence {
        async fn load_exact(
            &self,
            _scope: &AuthorityScope,
            purchase_order: &RecordRef,
            receipt: &RecordRef,
        ) -> Result<Option<ThreeWayMatchEvidence>, PortError> {
            Ok(
                (self.0.purchase_order == *purchase_order && self.0.receipt == *receipt)
                    .then(|| self.0.clone()),
            )
        }
    }

    #[derive(Clone, Default)]
    struct FakeWrite {
        invoice: Option<VendorInvoice>,
        payment: Option<Payment>,
        payables: Vec<Payable>,
        loads: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl PayablesWriteRepository for FakeWrite {
        async fn load_vendor_invoice(
            &self,
            _scope: &AuthorityScope,
            _id: VendorInvoiceId,
        ) -> Result<Option<VendorInvoice>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.invoice.clone())
        }

        async fn load_payment(
            &self,
            _scope: &AuthorityScope,
            _id: PaymentId,
        ) -> Result<Option<Payment>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.payment.clone())
        }

        async fn load_payables_exact(
            &self,
            _scope: &AuthorityScope,
            references: &[(PayableId, u64)],
        ) -> Result<Vec<Payable>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .payables
                .iter()
                .filter(|payable| references.contains(&(payable.id(), payable.version())))
                .cloned()
                .collect())
        }
    }

    #[derive(Clone, Default)]
    struct FakeUow {
        receipts: Arc<Mutex<Vec<(PayablesCommandIdentity, StoredPayablesCommit)>>>,
        commits: Arc<AtomicUsize>,
        receipt_lookups: Arc<AtomicUsize>,
        posted_events: Arc<Mutex<Vec<PayablesDomainEvent>>>,
        posted_payable: Arc<Mutex<Option<Payable>>>,
    }

    #[async_trait]
    impl PayablesCommandUnitOfWork for FakeUow {
        async fn find_receipt(
            &self,
            identity: &PayablesCommandIdentity,
        ) -> Result<Option<StoredPayablesCommit>, PortError> {
            self.receipt_lookups.fetch_add(1, Ordering::SeqCst);
            for (stored_identity, receipt) in self.receipts.lock().unwrap().iter() {
                if stored_identity.command_id == identity.command_id
                    || stored_identity.idempotency_key == identity.idempotency_key
                {
                    return if stored_identity == identity {
                        Ok(Some(receipt.clone()))
                    } else {
                        Err(PortError::IdempotencyConflict)
                    };
                }
            }
            Ok(None)
        }

        async fn commit(&self, request: PayablesCommit) -> Result<StoredPayablesCommit, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            *self.posted_events.lock().unwrap() = request.domain_events.clone();
            *self.posted_payable.lock().unwrap() = request.payable.clone();
            let stored = StoredPayablesCommit {
                command_id: request.identity.command_id,
                correlation_id: request.correlation_id,
                aggregate: request.aggregate,
                payable: request.payable,
                committed_events: vec![EventPosition {
                    event_id: EventId::try_from_uuid(id(900)).unwrap(),
                    global_sequence: 1,
                }],
                committed_at: at(),
                replayed: false,
            };
            self.receipts
                .lock()
                .unwrap()
                .push((request.identity, stored.clone()));
            Ok(stored)
        }
    }

    type TestHandler = PayablesCommandHandler<
        FakeWrite,
        FakeUow,
        FakeAuthorization,
        FakeEntitlement,
        FakeEvidence,
        FixedClock,
    >;

    fn handler(write: FakeWrite, uow: FakeUow) -> TestHandler {
        PayablesCommandHandler::new(
            write,
            uow,
            FakeAuthorization(true),
            FakeEntitlement(true),
            FakeEvidence(evidence()),
            FixedClock::new(at()),
        )
    }

    #[tokio::test]
    async fn stable_identity_replays_and_changed_payload_conflicts() {
        let uow = FakeUow::default();
        let handler = handler(FakeWrite::default(), uow.clone());
        let request = envelope(1, None, create_invoice_command());
        assert!(!handler.handle(request.clone()).await.unwrap().replayed);
        assert!(handler.handle(request.clone()).await.unwrap().replayed);
        let mut changed = request;
        let PayablesCommand::CreateVendorInvoice(value) = &mut changed.command else {
            unreachable!();
        };
        value.invoice_number = "VIN-CHANGED".into();
        assert_eq!(
            handler.handle(changed).await.unwrap_err().category(),
            ErrorCategory::Conflict
        );
        assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn expected_version_is_required_and_checked_before_invoice_post() {
        let invoice = draft_invoice();
        let command = PayablesCommand::PostVendorInvoice(PostVendorInvoice {
            vendor_invoice_id: invoice.id(),
            purchase_order: invoice.evidence().purchase_order.clone(),
            receipt: invoice.evidence().receipt.clone(),
        });
        for (sequence, expected, category) in [
            (2, None, ErrorCategory::Invalid),
            (3, Some(99), ErrorCategory::Conflict),
        ] {
            let uow = FakeUow::default();
            let error = handler(
                FakeWrite {
                    invoice: Some(invoice.clone()),
                    ..FakeWrite::default()
                },
                uow.clone(),
            )
            .handle(envelope(sequence, expected, command.clone()))
            .await
            .unwrap_err();
            assert_eq!(error.category(), category);
            assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn posting_commits_vendor_invoice_and_deterministic_payable_together() {
        let invoice = draft_invoice();
        let uow = FakeUow::default();
        let receipt = handler(
            FakeWrite {
                invoice: Some(invoice.clone()),
                ..FakeWrite::default()
            },
            uow.clone(),
        )
        .handle(envelope(
            4,
            Some(invoice.version()),
            PayablesCommand::PostVendorInvoice(PostVendorInvoice {
                vendor_invoice_id: invoice.id(),
                purchase_order: invoice.evidence().purchase_order.clone(),
                receipt: invoice.evidence().receipt.clone(),
            }),
        ))
        .await
        .unwrap();
        let PayablesMutationResult::VendorInvoice(posted) = receipt.response else {
            panic!("expected vendor invoice response");
        };
        let payable = uow.posted_payable.lock().unwrap().clone().unwrap();
        assert_eq!(posted.payable_id, Some(payable.id()));
        assert_eq!(payable.vendor_invoice_version(), posted.version);
        assert!(matches!(
            uow.posted_events.lock().unwrap().as_slice(),
            [
                PayablesDomainEvent::VendorInvoicePosted(_),
                PayablesDomainEvent::PayablePosted(_)
            ]
        ));
    }

    #[tokio::test]
    async fn post_and_release_reject_non_human_attribution_before_replay_or_load() {
        let payable = payable();
        let payment = draft_payment(&payable);
        for kind in [
            ActorKind::Agent,
            ActorKind::Service,
            ActorKind::SystemWorker,
        ] {
            let cases = [
                PayablesCommand::PostVendorInvoice(PostVendorInvoice {
                    vendor_invoice_id: VendorInvoiceId::try_from_uuid(id(20)).unwrap(),
                    purchase_order: evidence().purchase_order,
                    receipt: evidence().receipt,
                }),
                PayablesCommand::ReleasePayment(ReleasePayment {
                    payment_id: payment.id(),
                }),
            ];
            for (offset, command) in cases.into_iter().enumerate() {
                let loads = Arc::new(AtomicUsize::new(0));
                let uow = FakeUow::default();
                let mut request = envelope(10 + offset as u128, Some(1), command);
                request.actor.kind = kind;
                let error = handler(
                    FakeWrite {
                        invoice: Some(draft_invoice()),
                        payment: Some(payment.clone()),
                        payables: vec![payable.clone()],
                        loads: loads.clone(),
                    },
                    uow.clone(),
                )
                .handle(request)
                .await
                .unwrap_err();
                assert_eq!(error.category(), ErrorCategory::Denied);
                assert_eq!(uow.receipt_lookups.load(Ordering::SeqCst), 0);
                assert_eq!(loads.load(Ordering::SeqCst), 0);
                assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
            }
        }
    }

    #[tokio::test]
    async fn human_payment_release_checks_exact_payable_version() {
        let payable = payable();
        let payment = draft_payment(&payable);
        let uow = FakeUow::default();
        let receipt = handler(
            FakeWrite {
                payment: Some(payment.clone()),
                payables: vec![payable],
                ..FakeWrite::default()
            },
            uow,
        )
        .handle(envelope(
            20,
            Some(payment.version()),
            PayablesCommand::ReleasePayment(ReleasePayment {
                payment_id: payment.id(),
            }),
        ))
        .await
        .unwrap();
        let PayablesMutationResult::Payment(value) = receipt.response else {
            panic!("expected payment response");
        };
        assert_eq!(value.status, PaymentStatus::Released);
        assert_eq!(value.version, payment.version() + 1);
    }
}
