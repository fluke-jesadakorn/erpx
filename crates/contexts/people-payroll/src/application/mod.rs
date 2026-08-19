use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_cqrs::{
    CommandEnvelope, CommandHandler, CommandReceipt, CqrsError, QueryEnvelope, QueryHandler,
    QueryMetadata, QueryResult,
};
use erpx_kernel::{AuthorityScope, Clock, CorrelationId, ErrorCategory, ErrorContext, KernelError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        Aggregate, Currency, Employee, EmployeeId, EmployeeTransition, ExpenseClaim,
        ExpenseClaimId, ExpenseTransition, LeaveRequest, LeaveRequestId, LeaveTransition,
        PayrollCalculationEvidence, PayrollLine, PayrollRun, PayrollRunId, PayrollTransition,
        PeoplePayrollDomainError,
    },
    integration,
    ports::{
        AuthorizationPort, AuthorizationRequest, CommandFingerprint, CommandIdentity,
        CommandUnitOfWork, EmployeeList, EmployeeListFilter, EmployeeView, EntitlementPort,
        ExpenseClaimList, ExpenseClaimListFilter, ExpenseClaimView, LeaveRequestList,
        LeaveRequestListFilter, LeaveRequestView, PayrollRunList, PayrollRunListFilter,
        PayrollRunView, PeoplePayrollAction, PeoplePayrollCommit, PortError, ReadRepository,
        StoredCommit, WriteRepository,
    },
};

mod time_record;
pub use time_record::TimeRecordCommandHandler;

pub const MAX_PEOPLE_PAYROLL_PAGE_SIZE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HireEmployee {
    pub employee_id: EmployeeId,
    pub employee_code: String,
    pub display_name: String,
    pub hired_on: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionEmployee {
    pub employee_id: EmployeeId,
    pub transition: EmployeeTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateLeaveRequest {
    pub leave_request_id: LeaveRequestId,
    pub employee_id: EmployeeId,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionLeaveRequest {
    pub leave_request_id: LeaveRequestId,
    pub transition: LeaveTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateExpenseClaim {
    pub expense_claim_id: ExpenseClaimId,
    pub employee_id: EmployeeId,
    pub currency: Currency,
    pub amount_minor: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionExpenseClaim {
    pub expense_claim_id: ExpenseClaimId,
    pub transition: ExpenseTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePayrollRun {
    pub payroll_run_id: PayrollRunId,
    pub period_code: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub currency: Currency,
    pub lines: Vec<PayrollLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionPayrollRun {
    pub payroll_run_id: PayrollRunId,
    pub transition: PayrollTransition,
}

/// Separate command contract so existing payroll creation transports remain
/// source compatible while callers migrate to the governed evidence step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinPayrollCalculationEvidence {
    pub payroll_run_id: PayrollRunId,
    pub evidence: PayrollCalculationEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload", rename_all = "snake_case")]
pub enum PeoplePayrollCommand {
    HireEmployee(HireEmployee),
    TransitionEmployee(TransitionEmployee),
    CreateLeaveRequest(CreateLeaveRequest),
    TransitionLeaveRequest(TransitionLeaveRequest),
    CreateExpenseClaim(CreateExpenseClaim),
    TransitionExpenseClaim(TransitionExpenseClaim),
    CreatePayrollRun(CreatePayrollRun),
    TransitionPayrollRun(TransitionPayrollRun),
}

impl PeoplePayrollCommand {
    const fn action(&self) -> PeoplePayrollAction {
        match self {
            Self::HireEmployee(_) => PeoplePayrollAction::HireEmployee,
            Self::TransitionEmployee(_) => PeoplePayrollAction::TransitionEmployee,
            Self::CreateLeaveRequest(_) => PeoplePayrollAction::CreateLeaveRequest,
            Self::TransitionLeaveRequest(_) => PeoplePayrollAction::TransitionLeaveRequest,
            Self::CreateExpenseClaim(_) => PeoplePayrollAction::CreateExpenseClaim,
            Self::TransitionExpenseClaim(_) => PeoplePayrollAction::TransitionExpenseClaim,
            Self::CreatePayrollRun(_) => PeoplePayrollAction::CreatePayrollRun,
            Self::TransitionPayrollRun(_) => PeoplePayrollAction::TransitionPayrollRun,
        }
    }

    fn fingerprint(&self) -> CommandFingerprint {
        match self {
            Self::HireEmployee(value) => CommandFingerprint::HireEmployee {
                employee_id: value.employee_id,
                employee_code: value.employee_code.clone(),
                display_name: value.display_name.clone(),
                hired_on: value.hired_on,
            },
            Self::TransitionEmployee(value) => CommandFingerprint::TransitionEmployee {
                employee_id: value.employee_id,
                transition: value.transition.clone(),
            },
            Self::CreateLeaveRequest(value) => CommandFingerprint::CreateLeaveRequest {
                leave_request_id: value.leave_request_id,
                employee_id: value.employee_id,
                starts_on: value.starts_on,
                ends_on: value.ends_on,
                reason: value.reason.clone(),
            },
            Self::TransitionLeaveRequest(value) => CommandFingerprint::TransitionLeaveRequest {
                leave_request_id: value.leave_request_id,
                transition: value.transition,
            },
            Self::CreateExpenseClaim(value) => CommandFingerprint::CreateExpenseClaim {
                expense_claim_id: value.expense_claim_id,
                employee_id: value.employee_id,
                currency: value.currency,
                amount_minor: value.amount_minor,
                reason: value.reason.clone(),
            },
            Self::TransitionExpenseClaim(value) => CommandFingerprint::TransitionExpenseClaim {
                expense_claim_id: value.expense_claim_id,
                transition: value.transition,
            },
            Self::CreatePayrollRun(value) => CommandFingerprint::CreatePayrollRun {
                payroll_run_id: value.payroll_run_id,
                period_code: value.period_code.clone(),
                starts_on: value.starts_on,
                ends_on: value.ends_on,
                currency: value.currency,
                lines: value.lines.clone(),
            },
            Self::TransitionPayrollRun(value) => CommandFingerprint::TransitionPayrollRun {
                payroll_run_id: value.payroll_run_id,
                transition: value.transition,
            },
        }
    }

    const fn is_create(&self) -> bool {
        matches!(
            self,
            Self::HireEmployee(_)
                | Self::CreateLeaveRequest(_)
                | Self::CreateExpenseClaim(_)
                | Self::CreatePayrollRun(_)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEmployee {
    pub employee_id: EmployeeId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListEmployees {
    pub filter: EmployeeListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetLeaveRequest {
    pub leave_request_id: LeaveRequestId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListLeaveRequests {
    pub filter: LeaveRequestListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetExpenseClaim {
    pub expense_claim_id: ExpenseClaimId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListExpenseClaims {
    pub filter: ExpenseClaimListFilter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPayrollRun {
    pub payroll_run_id: PayrollRunId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPayrollRuns {
    pub filter: PayrollRunListFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", content = "payload", rename_all = "snake_case")]
pub enum PeoplePayrollQuery {
    GetEmployee(GetEmployee),
    ListEmployees(ListEmployees),
    GetLeaveRequest(GetLeaveRequest),
    ListLeaveRequests(ListLeaveRequests),
    GetExpenseClaim(GetExpenseClaim),
    ListExpenseClaims(ListExpenseClaims),
    GetPayrollRun(GetPayrollRun),
    ListPayrollRuns(ListPayrollRuns),
}

impl PeoplePayrollQuery {
    const fn action(&self) -> PeoplePayrollAction {
        match self {
            Self::GetEmployee(_) => PeoplePayrollAction::ViewEmployee,
            Self::ListEmployees(_) => PeoplePayrollAction::ListEmployees,
            Self::GetLeaveRequest(_) => PeoplePayrollAction::ViewLeaveRequest,
            Self::ListLeaveRequests(_) => PeoplePayrollAction::ListLeaveRequests,
            Self::GetExpenseClaim(_) => PeoplePayrollAction::ViewExpenseClaim,
            Self::ListExpenseClaims(_) => PeoplePayrollAction::ListExpenseClaims,
            Self::GetPayrollRun(_) => PeoplePayrollAction::ViewPayrollRun,
            Self::ListPayrollRuns(_) => PeoplePayrollAction::ListPayrollRuns,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum PeoplePayrollMutationResult {
    Employee(EmployeeView),
    LeaveRequest(LeaveRequestView),
    ExpenseClaim(ExpenseClaimView),
    PayrollRun(PayrollRunView),
}

impl From<&Aggregate> for PeoplePayrollMutationResult {
    fn from(value: &Aggregate) -> Self {
        match value {
            Aggregate::Employee(value) => Self::Employee(value.into()),
            Aggregate::LeaveRequest(value) => Self::LeaveRequest(value.into()),
            Aggregate::ExpenseClaim(value) => Self::ExpenseClaim(value.into()),
            Aggregate::PayrollRun(value) => Self::PayrollRun(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum PeoplePayrollQueryResponse {
    Employee(Option<EmployeeView>),
    Employees(EmployeeList),
    LeaveRequest(Option<LeaveRequestView>),
    LeaveRequests(LeaveRequestList),
    ExpenseClaim(Option<ExpenseClaimView>),
    ExpenseClaims(ExpenseClaimList),
    PayrollRun(Option<PayrollRunView>),
    PayrollRuns(PayrollRunList),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeoplePayrollApplicationError {
    #[error(transparent)]
    Domain(#[from] PeoplePayrollDomainError),
    #[error("command scope does not contain the People & Payroll record scope")]
    ScopeOutsideAuthority,
    #[error("create commands cannot declare an aggregate version")]
    UnexpectedCreateVersion,
    #[error("mutating commands require an expected aggregate version")]
    MissingExpectedVersion,
    #[error("People & Payroll aggregate version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("People & Payroll list page size must be between 1 and 100")]
    InvalidPageSize,
    #[error("People & Payroll integration event mapping failed")]
    IntegrationMapping,
    #[error(transparent)]
    Port(#[from] PortError),
}

pub struct PeoplePayrollCommandHandler<W, U, A, E, C> {
    write: W,
    unit_of_work: U,
    authorization: A,
    entitlement: E,
    clock: C,
}

impl<W, U, A, E, C> PeoplePayrollCommandHandler<W, U, A, E, C> {
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

impl<W, U, A, E, C> PeoplePayrollCommandHandler<W, U, A, E, C>
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
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: PeoplePayrollAction,
    ) -> Result<(), PeoplePayrollApplicationError> {
        if !authority_scope.contains(target_scope) {
            return Err(PeoplePayrollApplicationError::ScopeOutsideAuthority);
        }
        self.entitlement
            .ensure_people_payroll_enabled(target_scope, action.requires_write_entitlement())
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
        authority_scope: &AuthorityScope,
        action: PeoplePayrollAction,
    ) -> Result<Option<StoredCommit>, PeoplePayrollApplicationError> {
        let Some(mut stored) = self.unit_of_work.find_receipt(identity).await? else {
            return Ok(None);
        };
        self.preflight(actor, authority_scope, stored.aggregate.scope(), action)
            .await?;
        stored.replayed = true;
        Ok(Some(stored))
    }

    async fn validate_existing(
        &self,
        aggregate: &Aggregate,
        actor: &erpx_kernel::ActorAttribution,
        authority_scope: &AuthorityScope,
        action: PeoplePayrollAction,
        expected_version: Option<u64>,
    ) -> Result<u64, PeoplePayrollApplicationError> {
        self.preflight(actor, authority_scope, aggregate.scope(), action)
            .await?;
        let expected =
            expected_version.ok_or(PeoplePayrollApplicationError::MissingExpectedVersion)?;
        if expected != aggregate.version() {
            return Err(PeoplePayrollApplicationError::VersionConflict {
                expected,
                actual: aggregate.version(),
            });
        }
        Ok(expected)
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<PeoplePayrollCommand>
    for PeoplePayrollCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PeoplePayrollMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<PeoplePayrollCommand>,
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
            .replay(&identity, &envelope.actor, &envelope.active_scope, action)
            .await
            .map_err(|error| to_cqrs(error, correlation))?
        {
            let response = PeoplePayrollMutationResult::from(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }
        if envelope.command.is_create() && envelope.expected_aggregate_version.is_some() {
            return Err(to_cqrs(
                PeoplePayrollApplicationError::UnexpectedCreateVersion,
                correlation,
            ));
        }

        let at = self.clock.now();
        let actor_ref = envelope.actor.principal_ref.as_str();
        let (mut aggregate, expected_version) = match envelope.command {
            PeoplePayrollCommand::HireEmployee(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let aggregate = Employee::hire(
                    value.employee_id,
                    envelope.active_scope.clone(),
                    value.employee_code,
                    value.display_name,
                    value.hired_on,
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (Aggregate::Employee(aggregate), None)
            }
            PeoplePayrollCommand::TransitionEmployee(value) => {
                let employee = self
                    .write
                    .load_employee(&envelope.active_scope, value.employee_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = Aggregate::Employee(employee);
                let expected = self
                    .validate_existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let Aggregate::Employee(employee) = &mut aggregate else {
                    unreachable!()
                };
                employee
                    .transition(value.transition, at)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            PeoplePayrollCommand::CreateLeaveRequest(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let aggregate = LeaveRequest::create(
                    value.leave_request_id,
                    envelope.active_scope.clone(),
                    value.employee_id,
                    value.starts_on,
                    value.ends_on,
                    value.reason,
                    actor_ref.into(),
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (Aggregate::LeaveRequest(aggregate), None)
            }
            PeoplePayrollCommand::TransitionLeaveRequest(value) => {
                let leave = self
                    .write
                    .load_leave_request(&envelope.active_scope, value.leave_request_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = Aggregate::LeaveRequest(leave);
                let expected = self
                    .validate_existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let Aggregate::LeaveRequest(leave) = &mut aggregate else {
                    unreachable!()
                };
                leave
                    .transition(value.transition, actor_ref, at)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            PeoplePayrollCommand::CreateExpenseClaim(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let aggregate = ExpenseClaim::create(
                    value.expense_claim_id,
                    envelope.active_scope.clone(),
                    value.employee_id,
                    value.currency,
                    value.amount_minor,
                    value.reason,
                    actor_ref.into(),
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (Aggregate::ExpenseClaim(aggregate), None)
            }
            PeoplePayrollCommand::TransitionExpenseClaim(value) => {
                let expense = self
                    .write
                    .load_expense_claim(&envelope.active_scope, value.expense_claim_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = Aggregate::ExpenseClaim(expense);
                let expected = self
                    .validate_existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let Aggregate::ExpenseClaim(expense) = &mut aggregate else {
                    unreachable!()
                };
                expense
                    .transition(value.transition, actor_ref, at)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
            PeoplePayrollCommand::CreatePayrollRun(value) => {
                self.preflight(
                    &envelope.actor,
                    &envelope.active_scope,
                    &envelope.active_scope,
                    action,
                )
                .await
                .map_err(|error| to_cqrs(error, correlation))?;
                let aggregate = PayrollRun::create(
                    value.payroll_run_id,
                    envelope.active_scope.clone(),
                    value.period_code,
                    value.starts_on,
                    value.ends_on,
                    value.currency,
                    value.lines,
                    actor_ref.into(),
                    at,
                )
                .map_err(|error| to_cqrs(error.into(), correlation))?;
                (Aggregate::PayrollRun(aggregate), None)
            }
            PeoplePayrollCommand::TransitionPayrollRun(value) => {
                let payroll = self
                    .write
                    .load_payroll_run(&envelope.active_scope, value.payroll_run_id)
                    .await
                    .map_err(|error| to_cqrs(error.into(), correlation))?
                    .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
                let mut aggregate = Aggregate::PayrollRun(payroll);
                let expected = self
                    .validate_existing(
                        &aggregate,
                        &envelope.actor,
                        &envelope.active_scope,
                        action,
                        envelope.expected_aggregate_version,
                    )
                    .await
                    .map_err(|error| to_cqrs(error, correlation))?;
                let Aggregate::PayrollRun(payroll) = &mut aggregate else {
                    unreachable!()
                };
                payroll
                    .transition(value.transition, actor_ref, at)
                    .map_err(|error| to_cqrs(error.into(), correlation))?;
                (aggregate, Some(expected))
            }
        };

        let events =
            integration::published_events(&aggregate.take_uncommitted_events()).map_err(|_| {
                to_cqrs(
                    PeoplePayrollApplicationError::IntegrationMapping,
                    correlation,
                )
            })?;
        let stored = self
            .unit_of_work
            .commit(PeoplePayrollCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                expected_version,
                aggregate,
                integration_events: events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;
        let response = PeoplePayrollMutationResult::from(&stored.aggregate);
        Ok(stored.into_receipt(response))
    }
}

#[async_trait]
impl<W, U, A, E, C> CommandHandler<PinPayrollCalculationEvidence>
    for PeoplePayrollCommandHandler<W, U, A, E, C>
where
    W: WriteRepository,
    U: CommandUnitOfWork,
    A: AuthorizationPort,
    E: EntitlementPort,
    C: Clock,
{
    type Response = PeoplePayrollMutationResult;

    async fn handle(
        &self,
        envelope: CommandEnvelope<PinPayrollCalculationEvidence>,
    ) -> Result<CommandReceipt<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let identity = CommandIdentity {
            command_id: envelope.command_id,
            idempotency_key: envelope.idempotency_key.clone(),
            fingerprint: CommandFingerprint::PinPayrollCalculationEvidence {
                payroll_run_id: envelope.command.payroll_run_id,
                evidence: envelope.command.evidence.clone(),
            },
        };
        if let Some(stored) = self
            .replay(
                &identity,
                &envelope.actor,
                &envelope.active_scope,
                PeoplePayrollAction::PinPayrollCalculationEvidence,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation))?
        {
            let response = PeoplePayrollMutationResult::from(&stored.aggregate);
            return Ok(stored.into_receipt(response));
        }

        let payroll = self
            .write
            .load_payroll_run(&envelope.active_scope, envelope.command.payroll_run_id)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?
            .ok_or_else(|| to_cqrs(PortError::NotFound.into(), correlation))?;
        let mut aggregate = Aggregate::PayrollRun(payroll);
        let expected = self
            .validate_existing(
                &aggregate,
                &envelope.actor,
                &envelope.active_scope,
                PeoplePayrollAction::PinPayrollCalculationEvidence,
                envelope.expected_aggregate_version,
            )
            .await
            .map_err(|error| to_cqrs(error, correlation))?;
        let Aggregate::PayrollRun(payroll) = &mut aggregate else {
            unreachable!()
        };
        payroll
            .pin_calculation_evidence(envelope.command.evidence, self.clock.now())
            .map_err(|error| to_cqrs(error.into(), correlation))?;

        let events =
            integration::published_events(&aggregate.take_uncommitted_events()).map_err(|_| {
                to_cqrs(
                    PeoplePayrollApplicationError::IntegrationMapping,
                    correlation,
                )
            })?;
        let stored = self
            .unit_of_work
            .commit(PeoplePayrollCommit {
                identity,
                correlation_id: correlation,
                causation_id: envelope.causation_id,
                actor: envelope.actor,
                authority_scope: envelope.active_scope,
                audit: envelope.audit,
                expected_version: Some(expected),
                aggregate,
                integration_events: events,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;
        let response = PeoplePayrollMutationResult::from(&stored.aggregate);
        Ok(stored.into_receipt(response))
    }
}

pub struct PeoplePayrollQueryHandler<R, A, E> {
    read: R,
    authorization: A,
    entitlement: E,
}

impl<R, A, E> PeoplePayrollQueryHandler<R, A, E> {
    pub const fn new(read: R, authorization: A, entitlement: E) -> Self {
        Self {
            read,
            authorization,
            entitlement,
        }
    }
}

#[async_trait]
impl<R, A, E> QueryHandler<PeoplePayrollQuery> for PeoplePayrollQueryHandler<R, A, E>
where
    R: ReadRepository,
    A: AuthorizationPort,
    E: EntitlementPort,
{
    type Response = PeoplePayrollQueryResponse;

    async fn handle(
        &self,
        envelope: QueryEnvelope<PeoplePayrollQuery>,
    ) -> Result<QueryResult<Self::Response>, CqrsError> {
        envelope.validate()?;
        let correlation = envelope.correlation_id;
        let action = envelope.query.action();
        self.entitlement
            .ensure_people_payroll_enabled(&envelope.active_scope, false)
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &envelope.actor,
                authority_scope: &envelope.active_scope,
                target_scope: &envelope.active_scope,
                action,
            })
            .await
            .map_err(|error| to_cqrs(error.into(), correlation))?;

        let (value, projection) = match envelope.query {
            PeoplePayrollQuery::GetEmployee(value) => {
                let projected = self
                    .read
                    .get_employee(
                        &envelope.active_scope,
                        value.employee_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::Employee(projected.value),
                    projected.metadata,
                )
            }
            PeoplePayrollQuery::ListEmployees(value) => {
                validate_page_size(value.filter.page_size)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projected = self
                    .read
                    .list_employees(&envelope.active_scope, &value.filter, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::Employees(projected.value),
                    projected.metadata,
                )
            }
            PeoplePayrollQuery::GetLeaveRequest(value) => {
                let projected = self
                    .read
                    .get_leave_request(
                        &envelope.active_scope,
                        value.leave_request_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::LeaveRequest(projected.value),
                    projected.metadata,
                )
            }
            PeoplePayrollQuery::ListLeaveRequests(value) => {
                validate_page_size(value.filter.page_size)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projected = self
                    .read
                    .list_leave_requests(
                        &envelope.active_scope,
                        &value.filter,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::LeaveRequests(projected.value),
                    projected.metadata,
                )
            }
            PeoplePayrollQuery::GetExpenseClaim(value) => {
                let projected = self
                    .read
                    .get_expense_claim(
                        &envelope.active_scope,
                        value.expense_claim_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::ExpenseClaim(projected.value),
                    projected.metadata,
                )
            }
            PeoplePayrollQuery::ListExpenseClaims(value) => {
                validate_page_size(value.filter.page_size)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projected = self
                    .read
                    .list_expense_claims(
                        &envelope.active_scope,
                        &value.filter,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::ExpenseClaims(projected.value),
                    projected.metadata,
                )
            }
            PeoplePayrollQuery::GetPayrollRun(value) => {
                let projected = self
                    .read
                    .get_payroll_run(
                        &envelope.active_scope,
                        value.payroll_run_id,
                        &envelope.consistency,
                    )
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::PayrollRun(projected.value),
                    projected.metadata,
                )
            }
            PeoplePayrollQuery::ListPayrollRuns(value) => {
                validate_page_size(value.filter.page_size)
                    .map_err(|error| to_cqrs(error, correlation))?;
                let projected = self
                    .read
                    .list_payroll_runs(&envelope.active_scope, &value.filter, &envelope.consistency)
                    .await
                    .map_err(|error| port_to_cqrs(error, correlation))?;
                (
                    PeoplePayrollQueryResponse::PayrollRuns(projected.value),
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
fn validate_page_size(page_size: u16) -> Result<(), PeoplePayrollApplicationError> {
    if page_size == 0 || page_size > MAX_PEOPLE_PAYROLL_PAGE_SIZE {
        Err(PeoplePayrollApplicationError::InvalidPageSize)
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

fn to_cqrs(error: PeoplePayrollApplicationError, correlation_id: CorrelationId) -> CqrsError {
    let (code, category, message, retryable) = match error {
        PeoplePayrollApplicationError::ScopeOutsideAuthority
        | PeoplePayrollApplicationError::Port(PortError::Denied | PortError::EntitlementDenied) => {
            (
                "people_payroll.denied",
                ErrorCategory::Denied,
                "People & Payroll operation is not allowed.",
                false,
            )
        }
        PeoplePayrollApplicationError::Port(PortError::NotFound) => (
            "people_payroll.not_found",
            ErrorCategory::NotFound,
            "People & Payroll record was not found.",
            false,
        ),
        PeoplePayrollApplicationError::MissingExpectedVersion
        | PeoplePayrollApplicationError::VersionConflict { .. }
        | PeoplePayrollApplicationError::Port(
            PortError::Conflict { .. } | PortError::IdempotencyConflict,
        ) => (
            "people_payroll.conflict",
            ErrorCategory::Conflict,
            "People & Payroll record changed or command identity conflicts.",
            false,
        ),
        PeoplePayrollApplicationError::Port(PortError::Unavailable) => (
            "people_payroll.unavailable",
            ErrorCategory::Unavailable,
            "People & Payroll is temporarily unavailable.",
            true,
        ),
        PeoplePayrollApplicationError::Domain(_)
        | PeoplePayrollApplicationError::UnexpectedCreateVersion
        | PeoplePayrollApplicationError::InvalidPageSize => (
            "people_payroll.invalid",
            ErrorCategory::Invalid,
            "People & Payroll input is invalid.",
            false,
        ),
        PeoplePayrollApplicationError::IntegrationMapping
        | PeoplePayrollApplicationError::Port(PortError::Internal | PortError::NotReady(_)) => (
            "people_payroll.internal",
            ErrorCategory::Internal,
            "People & Payroll could not complete the operation.",
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
