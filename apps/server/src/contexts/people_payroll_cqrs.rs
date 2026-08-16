//! Typed, storage-agnostic gRPC inbound adapter for People & Payroll CQRS.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, CqrsError, FreshnessState, IdempotencyKey, QueryConsistency,
    QueryEnvelope, QueryMetadata, QueryResult, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    QueryId, SessionId, TenantScope,
};
use erpx_people_payroll::{
    CreateExpenseClaim, CreateLeaveRequest, CreatePayrollRun, Currency, EmployeeId,
    EmployeeTransition, ExpenseClaimId, ExpenseTransition, GetEmployee, GetExpenseClaim,
    GetLeaveRequest, GetPayrollRun, HireEmployee, LeaveRequestId, LeaveTransition, ListEmployees,
    ListExpenseClaims, ListLeaveRequests, ListPayrollRuns, PayrollLine, PayrollRunId,
    PayrollTransition, PeoplePayrollCommand, PeoplePayrollMutationResult, PeoplePayrollQuery,
    PeoplePayrollQueryResponse, TransitionEmployee, TransitionExpenseClaim, TransitionLeaveRequest,
    TransitionPayrollRun,
    ports::{
        EmployeeListFilter, EmployeeView, ExpenseClaimListFilter, ExpenseClaimView,
        LeaveRequestListFilter, LeaveRequestView, PayrollRunListFilter, PayrollRunView,
    },
};
use erpx_proto::{common, erpx::people_payroll::v1 as platform};
use platform::people_payroll_command_service_server::PeoplePayrollCommandService;
use platform::people_payroll_query_service_server::PeoplePayrollQueryService;
use tonic::{Request, Response, Status, metadata::MetadataMap};
use uuid::Uuid;

const MAX_ROUTE_LENGTH: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeoplePayrollTenantRoute {
    pub tenant_id: String,
    pub holding_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeoplePayrollAccessMode {
    Read,
    Write,
}

pub struct PeoplePayrollAuthenticationRequest {
    transport_metadata: MetadataMap,
    presented_actor: ActorAttribution,
    presented_scope: TenantScope,
    presented_route: PeoplePayrollTenantRoute,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
    access_mode: PeoplePayrollAccessMode,
}
impl PeoplePayrollAuthenticationRequest {
    pub fn transport_metadata(&self) -> &MetadataMap {
        &self.transport_metadata
    }
    pub const fn presented_actor(&self) -> &ActorAttribution {
        &self.presented_actor
    }
    pub const fn presented_scope(&self) -> &TenantScope {
        &self.presented_scope
    }
    pub const fn presented_route(&self) -> &PeoplePayrollTenantRoute {
        &self.presented_route
    }
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }
    pub fn mfa_code(&self) -> Option<&str> {
        self.mfa_code.as_deref()
    }
    pub const fn access_mode(&self) -> PeoplePayrollAccessMode {
        self.access_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPeoplePayrollRequest {
    actor: ActorAttribution,
    tenant_scope: TenantScope,
    route: PeoplePayrollTenantRoute,
}
impl AuthenticatedPeoplePayrollRequest {
    pub fn try_new(
        actor: ActorAttribution,
        tenant_scope: TenantScope,
        route: PeoplePayrollTenantRoute,
    ) -> Result<Self, PeoplePayrollAuthenticationError> {
        actor
            .validate()
            .map_err(|_| PeoplePayrollAuthenticationError::Internal)?;
        tenant_scope
            .validate()
            .map_err(|_| PeoplePayrollAuthenticationError::Internal)?;
        validate_route(&route).map_err(|_| PeoplePayrollAuthenticationError::Internal)?;
        validate_route_scope(&route, &tenant_scope)
            .map_err(|_| PeoplePayrollAuthenticationError::Internal)?;
        Ok(Self {
            actor,
            tenant_scope,
            route,
        })
    }
    pub const fn actor(&self) -> &ActorAttribution {
        &self.actor
    }
    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }
    pub const fn route(&self) -> &PeoplePayrollTenantRoute {
        &self.route
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeoplePayrollAuthenticationError {
    Unauthenticated,
    Denied,
    Unavailable,
    Internal,
}

#[async_trait]
pub trait PeoplePayrollRequestAuthenticator: Send + Sync + 'static {
    async fn authenticate(
        &self,
        request: PeoplePayrollAuthenticationRequest,
    ) -> Result<AuthenticatedPeoplePayrollRequest, PeoplePayrollAuthenticationError>;
}

#[async_trait]
pub trait PeoplePayrollCommandApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: PeoplePayrollTenantRoute,
        command: CommandEnvelope<PeoplePayrollCommand>,
    ) -> Result<CommandReceipt<PeoplePayrollMutationResult>, CqrsError>;
}

#[async_trait]
pub trait PeoplePayrollQueryApplication: Send + Sync + 'static {
    async fn execute(
        &self,
        route: PeoplePayrollTenantRoute,
        query: QueryEnvelope<PeoplePayrollQuery>,
    ) -> Result<QueryResult<PeoplePayrollQueryResponse>, CqrsError>;
}

#[derive(Debug, Clone)]
pub struct PeoplePayrollCommandRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> PeoplePayrollCommandRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}
#[derive(Debug, Clone)]
pub struct PeoplePayrollQueryRpc<A, G> {
    application: Arc<A>,
    authenticator: Arc<G>,
}
impl<A, G> PeoplePayrollQueryRpc<A, G> {
    pub const fn new(application: Arc<A>, authenticator: Arc<G>) -> Self {
        Self {
            application,
            authenticator,
        }
    }
}

#[async_trait]
impl<A: PeoplePayrollCommandApplication, G: PeoplePayrollRequestAuthenticator>
    PeoplePayrollCommandService for PeoplePayrollCommandRpc<A, G>
{
    async fn hire_employee(
        &self,
        request: Request<platform::HireEmployeeRequest>,
    ) -> Result<Response<platform::PeoplePayrollEmployeeMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::HireEmployee(HireEmployee {
                    employee_id: employee_id(&input.employee_id)?,
                    employee_code: input.employee_code,
                    display_name: input.display_name,
                    hired_on: date(&input.hired_on, "hired_on")?,
                }),
            )
            .await?;
        employee_mutation(receipt)
    }

    async fn transition_employee(
        &self,
        request: Request<platform::TransitionEmployeeRequest>,
    ) -> Result<Response<platform::PeoplePayrollEmployeeMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let transition = decode_employee_transition(
            input
                .transition
                .ok_or_else(|| invalid("employee transition is required"))?,
        )?;
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::TransitionEmployee(TransitionEmployee {
                    employee_id: employee_id(&input.employee_id)?,
                    transition,
                }),
            )
            .await?;
        employee_mutation(receipt)
    }

    async fn create_leave_request(
        &self,
        request: Request<platform::CreateLeaveRequestRequest>,
    ) -> Result<Response<platform::PeoplePayrollLeaveMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::CreateLeaveRequest(CreateLeaveRequest {
                    leave_request_id: leave_id(&input.leave_request_id)?,
                    employee_id: employee_id(&input.employee_id)?,
                    starts_on: date(&input.starts_on, "starts_on")?,
                    ends_on: date(&input.ends_on, "ends_on")?,
                    reason: input.reason,
                }),
            )
            .await?;
        leave_mutation(receipt)
    }

    async fn transition_leave_request(
        &self,
        request: Request<platform::TransitionLeaveRequestRequest>,
    ) -> Result<Response<platform::PeoplePayrollLeaveMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::TransitionLeaveRequest(TransitionLeaveRequest {
                    leave_request_id: leave_id(&input.leave_request_id)?,
                    transition: decode_leave_transition(input.transition)?,
                }),
            )
            .await?;
        leave_mutation(receipt)
    }

    async fn create_expense_claim(
        &self,
        request: Request<platform::CreateExpenseClaimRequest>,
    ) -> Result<Response<platform::PeoplePayrollExpenseMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::CreateExpenseClaim(CreateExpenseClaim {
                    expense_claim_id: expense_id(&input.expense_claim_id)?,
                    employee_id: employee_id(&input.employee_id)?,
                    currency: Currency::try_new(&input.currency)
                        .map_err(|_| invalid("currency is invalid"))?,
                    amount_minor: input.amount_minor,
                    reason: input.reason,
                }),
            )
            .await?;
        expense_mutation(receipt)
    }

    async fn transition_expense_claim(
        &self,
        request: Request<platform::TransitionExpenseClaimRequest>,
    ) -> Result<Response<platform::PeoplePayrollExpenseMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::TransitionExpenseClaim(TransitionExpenseClaim {
                    expense_claim_id: expense_id(&input.expense_claim_id)?,
                    transition: decode_expense_transition(input.transition)?,
                }),
            )
            .await?;
        expense_mutation(receipt)
    }

    async fn create_payroll_run(
        &self,
        request: Request<platform::CreatePayrollRunRequest>,
    ) -> Result<Response<platform::PeoplePayrollRunMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let lines = input
            .lines
            .into_iter()
            .map(decode_payroll_line)
            .collect::<Result<Vec<_>, _>>()?;
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::CreatePayrollRun(CreatePayrollRun {
                    payroll_run_id: payroll_id(&input.payroll_run_id)?,
                    period_code: input.period_code,
                    starts_on: date(&input.starts_on, "starts_on")?,
                    ends_on: date(&input.ends_on, "ends_on")?,
                    currency: Currency::try_new(&input.currency)
                        .map_err(|_| invalid("currency is invalid"))?,
                    lines,
                }),
            )
            .await?;
        payroll_mutation(receipt)
    }

    async fn transition_payroll_run(
        &self,
        request: Request<platform::TransitionPayrollRunRequest>,
    ) -> Result<Response<platform::PeoplePayrollRunMutationResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let receipt = self
            .command(
                transport,
                input.metadata,
                PeoplePayrollCommand::TransitionPayrollRun(TransitionPayrollRun {
                    payroll_run_id: payroll_id(&input.payroll_run_id)?,
                    transition: decode_payroll_transition(input.transition)?,
                }),
            )
            .await?;
        payroll_mutation(receipt)
    }
}

impl<A: PeoplePayrollCommandApplication, G: PeoplePayrollRequestAuthenticator>
    PeoplePayrollCommandRpc<A, G>
{
    async fn command(
        &self,
        transport: MetadataMap,
        metadata: Option<common::CommandRequestMetadata>,
        command: PeoplePayrollCommand,
    ) -> Result<CommandReceipt<PeoplePayrollMutationResult>, Status> {
        let decoded = decode_command(metadata, command)?;
        let (route, envelope) =
            authenticate_command(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

#[async_trait]
impl<A: PeoplePayrollQueryApplication, G: PeoplePayrollRequestAuthenticator>
    PeoplePayrollQueryService for PeoplePayrollQueryRpc<A, G>
{
    async fn get_employee(
        &self,
        request: Request<platform::GetPeoplePayrollEmployeeRequest>,
    ) -> Result<Response<platform::GetPeoplePayrollEmployeeResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::GetEmployee(GetEmployee {
                    employee_id: employee_id(&input.employee_id)?,
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::Employee(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::GetPeoplePayrollEmployeeResponse {
            employee: value.as_ref().map(encode_employee),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_employees(
        &self,
        request: Request<platform::ListPeoplePayrollEmployeesRequest>,
    ) -> Result<Response<platform::ListPeoplePayrollEmployeesResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::ListEmployees(ListEmployees {
                    filter: EmployeeListFilter {
                        status: input.status.map(decode_employee_status).transpose()?,
                        page_size: page_size(input.page_size)?,
                        cursor: input.cursor,
                    },
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::Employees(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::ListPeoplePayrollEmployeesResponse {
                employees: value.employees.iter().map(encode_employee).collect(),
                next_cursor: value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    async fn get_leave_request(
        &self,
        request: Request<platform::GetPeoplePayrollLeaveRequest>,
    ) -> Result<Response<platform::GetPeoplePayrollLeaveResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::GetLeaveRequest(GetLeaveRequest {
                    leave_request_id: leave_id(&input.leave_request_id)?,
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::LeaveRequest(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::GetPeoplePayrollLeaveResponse {
            leave_request: value.as_ref().map(encode_leave),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_leave_requests(
        &self,
        request: Request<platform::ListPeoplePayrollLeaveRequestsRequest>,
    ) -> Result<Response<platform::ListPeoplePayrollLeaveRequestsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::ListLeaveRequests(ListLeaveRequests {
                    filter: LeaveRequestListFilter {
                        employee_id: input.employee_id.as_deref().map(employee_id).transpose()?,
                        status: input.status.map(decode_leave_status).transpose()?,
                        page_size: page_size(input.page_size)?,
                        cursor: input.cursor,
                    },
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::LeaveRequests(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::ListPeoplePayrollLeaveRequestsResponse {
                leave_requests: value.leave_requests.iter().map(encode_leave).collect(),
                next_cursor: value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    async fn get_expense_claim(
        &self,
        request: Request<platform::GetPeoplePayrollExpenseClaimRequest>,
    ) -> Result<Response<platform::GetPeoplePayrollExpenseClaimResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::GetExpenseClaim(GetExpenseClaim {
                    expense_claim_id: expense_id(&input.expense_claim_id)?,
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::ExpenseClaim(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::GetPeoplePayrollExpenseClaimResponse {
                expense_claim: value.as_ref().map(encode_expense),
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    async fn list_expense_claims(
        &self,
        request: Request<platform::ListPeoplePayrollExpenseClaimsRequest>,
    ) -> Result<Response<platform::ListPeoplePayrollExpenseClaimsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::ListExpenseClaims(ListExpenseClaims {
                    filter: ExpenseClaimListFilter {
                        employee_id: input.employee_id.as_deref().map(employee_id).transpose()?,
                        status: input.status.map(decode_expense_status).transpose()?,
                        page_size: page_size(input.page_size)?,
                        cursor: input.cursor,
                    },
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::ExpenseClaims(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(
            platform::ListPeoplePayrollExpenseClaimsResponse {
                expense_claims: value.expense_claims.iter().map(encode_expense).collect(),
                next_cursor: value.next_cursor,
                metadata: Some(encode_query_metadata(&result.metadata)),
            },
        ))
    }

    async fn get_payroll_run(
        &self,
        request: Request<platform::GetPeoplePayrollRunRequest>,
    ) -> Result<Response<platform::GetPeoplePayrollRunResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::GetPayrollRun(GetPayrollRun {
                    payroll_run_id: payroll_id(&input.payroll_run_id)?,
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::PayrollRun(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::GetPeoplePayrollRunResponse {
            payroll_run: value.as_ref().map(encode_payroll),
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }

    async fn list_payroll_runs(
        &self,
        request: Request<platform::ListPeoplePayrollRunsRequest>,
    ) -> Result<Response<platform::ListPeoplePayrollRunsResponse>, Status> {
        let transport = request.metadata().clone();
        let input = request.into_inner();
        let result = self
            .query(
                transport,
                input.metadata,
                PeoplePayrollQuery::ListPayrollRuns(ListPayrollRuns {
                    filter: PayrollRunListFilter {
                        status: input.status.map(decode_payroll_status).transpose()?,
                        starts_on_or_after: input
                            .starts_on_or_after
                            .as_deref()
                            .map(|value| date(value, "starts_on_or_after"))
                            .transpose()?,
                        ends_on_or_before: input
                            .ends_on_or_before
                            .as_deref()
                            .map(|value| date(value, "ends_on_or_before"))
                            .transpose()?,
                        page_size: page_size(input.page_size)?,
                        cursor: input.cursor,
                    },
                }),
            )
            .await?;
        let PeoplePayrollQueryResponse::PayrollRuns(value) = result.value else {
            return Err(response_mismatch());
        };
        Ok(Response::new(platform::ListPeoplePayrollRunsResponse {
            payroll_runs: value.payroll_runs.iter().map(encode_payroll).collect(),
            next_cursor: value.next_cursor,
            metadata: Some(encode_query_metadata(&result.metadata)),
        }))
    }
}

impl<A: PeoplePayrollQueryApplication, G: PeoplePayrollRequestAuthenticator>
    PeoplePayrollQueryRpc<A, G>
{
    async fn query(
        &self,
        transport: MetadataMap,
        metadata: Option<common::QueryRequestMetadata>,
        query: PeoplePayrollQuery,
    ) -> Result<QueryResult<PeoplePayrollQueryResponse>, Status> {
        let decoded = decode_query(metadata, query)?;
        let (route, envelope) =
            authenticate_query(self.authenticator.as_ref(), transport, decoded).await?;
        self.application
            .execute(route, envelope)
            .await
            .map_err(status_from_cqrs)
    }
}

fn employee_mutation(
    receipt: CommandReceipt<PeoplePayrollMutationResult>,
) -> Result<Response<platform::PeoplePayrollEmployeeMutationResponse>, Status> {
    let PeoplePayrollMutationResult::Employee(value) = &receipt.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(
        platform::PeoplePayrollEmployeeMutationResponse {
            receipt: Some(encode_receipt(&receipt)),
            employee: Some(encode_employee(value)),
        },
    ))
}
fn leave_mutation(
    receipt: CommandReceipt<PeoplePayrollMutationResult>,
) -> Result<Response<platform::PeoplePayrollLeaveMutationResponse>, Status> {
    let PeoplePayrollMutationResult::LeaveRequest(value) = &receipt.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(
        platform::PeoplePayrollLeaveMutationResponse {
            receipt: Some(encode_receipt(&receipt)),
            leave_request: Some(encode_leave(value)),
        },
    ))
}
fn expense_mutation(
    receipt: CommandReceipt<PeoplePayrollMutationResult>,
) -> Result<Response<platform::PeoplePayrollExpenseMutationResponse>, Status> {
    let PeoplePayrollMutationResult::ExpenseClaim(value) = &receipt.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(
        platform::PeoplePayrollExpenseMutationResponse {
            receipt: Some(encode_receipt(&receipt)),
            expense_claim: Some(encode_expense(value)),
        },
    ))
}
fn payroll_mutation(
    receipt: CommandReceipt<PeoplePayrollMutationResult>,
) -> Result<Response<platform::PeoplePayrollRunMutationResponse>, Status> {
    let PeoplePayrollMutationResult::PayrollRun(value) = &receipt.response else {
        return Err(response_mismatch());
    };
    Ok(Response::new(platform::PeoplePayrollRunMutationResponse {
        receipt: Some(encode_receipt(&receipt)),
        payroll_run: Some(encode_payroll(value)),
    }))
}

struct DecodedCommand<C> {
    route: PeoplePayrollTenantRoute,
    envelope: CommandEnvelope<C>,
    subscription_id: Option<String>,
    mfa_code: Option<String>,
}
struct DecodedQuery<Q> {
    route: PeoplePayrollTenantRoute,
    envelope: QueryEnvelope<Q>,
}

async fn authenticate_command<C, G: PeoplePayrollRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedCommand<C>,
) -> Result<(PeoplePayrollTenantRoute, CommandEnvelope<C>), Status> {
    let authenticated = auth
        .authenticate(PeoplePayrollAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: decoded.subscription_id,
            mfa_code: decoded.mfa_code,
            access_mode: PeoplePayrollAccessMode::Write,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated People & Payroll authority is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}
async fn authenticate_query<Q, G: PeoplePayrollRequestAuthenticator>(
    auth: &G,
    transport: MetadataMap,
    decoded: DecodedQuery<Q>,
) -> Result<(PeoplePayrollTenantRoute, QueryEnvelope<Q>), Status> {
    let authenticated = auth
        .authenticate(PeoplePayrollAuthenticationRequest {
            transport_metadata: transport,
            presented_actor: decoded.envelope.actor.clone(),
            presented_scope: decoded.envelope.tenant_scope.clone(),
            presented_route: decoded.route,
            subscription_id: None,
            mfa_code: None,
            access_mode: PeoplePayrollAccessMode::Read,
        })
        .await
        .map_err(status_from_auth)?;
    let mut envelope = decoded.envelope;
    envelope.actor = authenticated.actor().clone();
    envelope.tenant_scope = authenticated.tenant_scope().clone();
    envelope
        .validate()
        .map_err(|_| Status::internal("authenticated People & Payroll authority is invalid"))?;
    Ok((authenticated.route().clone(), envelope))
}

fn decode_command<C>(
    metadata: Option<common::CommandRequestMetadata>,
    command: C,
) -> Result<DecodedCommand<C>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("command metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let envelope = CommandEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        command_id: CommandId::try_from_uuid(parse_uuid(&metadata.command_id, "command_id")?)
            .map_err(|_| invalid("command_id is invalid"))?,
        idempotency_key: IdempotencyKey::try_new(metadata.idempotency_key)
            .map_err(|_| invalid("idempotency_key is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        causation_id: metadata.causation_id.as_deref().map(event_id).transpose()?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        expected_aggregate_version: metadata.expected_aggregate_version,
        command,
    };
    envelope
        .validate()
        .map_err(|_| invalid("command metadata is invalid"))?;
    Ok(DecodedCommand {
        route,
        envelope,
        subscription_id: clean(metadata.subscription_id),
        mfa_code: clean(metadata.mfa_code),
    })
}
fn decode_query<Q>(
    metadata: Option<common::QueryRequestMetadata>,
    query: Q,
) -> Result<DecodedQuery<Q>, Status> {
    let metadata = metadata.ok_or_else(|| invalid("query metadata is required"))?;
    let route = decode_route(metadata.tenant_route)?;
    let envelope = QueryEnvelope {
        contract_version: u16::try_from(metadata.contract_version)
            .map_err(|_| invalid("contract_version is invalid"))?,
        query_id: QueryId::try_from_uuid(parse_uuid(&metadata.query_id, "query_id")?)
            .map_err(|_| invalid("query_id is invalid"))?,
        correlation_id: correlation(&metadata.correlation_id)?,
        actor: decode_actor(metadata.actor)?,
        tenant_scope: decode_scope(
            metadata
                .tenant_scope
                .ok_or_else(|| invalid("tenant_scope is required"))?,
        )?,
        audit: decode_audit(metadata.audit)?,
        consistency: decode_consistency(metadata.consistency)?,
        query,
    };
    envelope
        .validate()
        .map_err(|_| invalid("query metadata is invalid"))?;
    Ok(DecodedQuery { route, envelope })
}

fn encode_employee(value: &EmployeeView) -> platform::PeoplePayrollEmployeeRecord {
    platform::PeoplePayrollEmployeeRecord {
        employee_id: value.employee_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        employee_code: value.employee_code.clone(),
        display_name: value.display_name.clone(),
        hired_on: value.hired_on.to_string(),
        terminated_on: value.terminated_on.map(|date| date.to_string()),
        status: encode_employee_status(value.status),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_leave(value: &LeaveRequestView) -> platform::PeoplePayrollLeaveRequestRecord {
    platform::PeoplePayrollLeaveRequestRecord {
        leave_request_id: value.leave_request_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        employee_id: value.employee_id.as_uuid().to_string(),
        starts_on: value.starts_on.to_string(),
        ends_on: value.ends_on.to_string(),
        reason: value.reason.clone(),
        status: encode_leave_status(value.status),
        requested_by: value.requested_by.clone(),
        approved_by: value.approved_by.clone(),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_expense(value: &ExpenseClaimView) -> platform::PeoplePayrollExpenseClaimRecord {
    platform::PeoplePayrollExpenseClaimRecord {
        expense_claim_id: value.expense_claim_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        employee_id: value.employee_id.as_uuid().to_string(),
        currency: value.currency.as_str().into(),
        amount_minor: value.amount_minor,
        reason: value.reason.clone(),
        status: encode_expense_status(value.status),
        claimed_by: value.claimed_by.clone(),
        approved_by: value.approved_by.clone(),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_payroll(value: &PayrollRunView) -> platform::PeoplePayrollRunRecord {
    platform::PeoplePayrollRunRecord {
        payroll_run_id: value.payroll_run_id.as_uuid().to_string(),
        source_version: value.version,
        scope: Some(encode_scope(&value.scope)),
        period_code: value.period_code.clone(),
        starts_on: value.starts_on.to_string(),
        ends_on: value.ends_on.to_string(),
        currency: value.currency.as_str().into(),
        lines: value.lines.iter().map(encode_payroll_line).collect(),
        status: encode_payroll_status(value.status),
        prepared_by: value.prepared_by.clone(),
        reviewed_by: value.reviewed_by.clone(),
        approved_by: value.approved_by.clone(),
        total_gross_minor: value.total_gross_minor,
        total_deductions_minor: value.total_deductions_minor,
        total_net_minor: value.total_net_minor,
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        projected_at: String::new(),
    }
}
fn encode_payroll_line(value: &PayrollLine) -> platform::PeoplePayrollLineRecord {
    platform::PeoplePayrollLineRecord {
        employee_id: value.employee_id.as_uuid().to_string(),
        gross_minor: value.gross_minor,
        deductions_minor: value.deductions_minor,
        net_minor: value.net_minor,
    }
}
fn decode_payroll_line(value: platform::PeoplePayrollLineRecord) -> Result<PayrollLine, Status> {
    let line = PayrollLine::try_new(
        employee_id(&value.employee_id)?,
        value.gross_minor,
        value.deductions_minor,
    )
    .map_err(|_| invalid("payroll line is invalid"))?;
    if line.net_minor != value.net_minor {
        return Err(invalid("payroll line net amount does not reconcile"));
    }
    Ok(line)
}

fn decode_employee_transition(
    value: platform::PeoplePayrollEmployeeTransition,
) -> Result<EmployeeTransition, Status> {
    use platform::people_payroll_employee_transition::Transition;
    match value
        .transition
        .ok_or_else(|| invalid("employee transition is required"))?
    {
        Transition::Activate(_) => Ok(EmployeeTransition::Activate),
        Transition::PutOnLeave(_) => Ok(EmployeeTransition::PutOnLeave),
        Transition::ReturnFromLeave(_) => Ok(EmployeeTransition::ReturnFromLeave),
        Transition::Suspend(_) => Ok(EmployeeTransition::Suspend),
        Transition::Terminate(value) => Ok(EmployeeTransition::Terminate {
            terminated_on: date(&value.terminated_on, "terminated_on")?,
        }),
    }
}
fn decode_leave_transition(value: i32) -> Result<LeaveTransition, Status> {
    match platform::PeoplePayrollLeaveTransition::try_from(value).ok() {
        Some(platform::PeoplePayrollLeaveTransition::Submit) => Ok(LeaveTransition::Submit),
        Some(platform::PeoplePayrollLeaveTransition::Approve) => Ok(LeaveTransition::Approve),
        Some(platform::PeoplePayrollLeaveTransition::Reject) => Ok(LeaveTransition::Reject),
        Some(platform::PeoplePayrollLeaveTransition::Cancel) => Ok(LeaveTransition::Cancel),
        Some(platform::PeoplePayrollLeaveTransition::MarkTaken) => Ok(LeaveTransition::MarkTaken),
        _ => Err(invalid("leave transition is invalid")),
    }
}
fn decode_expense_transition(value: i32) -> Result<ExpenseTransition, Status> {
    match platform::PeoplePayrollExpenseTransition::try_from(value).ok() {
        Some(platform::PeoplePayrollExpenseTransition::Submit) => Ok(ExpenseTransition::Submit),
        Some(platform::PeoplePayrollExpenseTransition::Approve) => Ok(ExpenseTransition::Approve),
        Some(platform::PeoplePayrollExpenseTransition::Reject) => Ok(ExpenseTransition::Reject),
        Some(platform::PeoplePayrollExpenseTransition::Reimburse) => {
            Ok(ExpenseTransition::Reimburse)
        }
        _ => Err(invalid("expense transition is invalid")),
    }
}
fn decode_payroll_transition(value: i32) -> Result<PayrollTransition, Status> {
    match platform::PeoplePayrollRunTransition::try_from(value).ok() {
        Some(platform::PeoplePayrollRunTransition::SubmitForReview) => {
            Ok(PayrollTransition::SubmitForReview)
        }
        Some(platform::PeoplePayrollRunTransition::Review) => Ok(PayrollTransition::Review),
        Some(platform::PeoplePayrollRunTransition::Post) => Ok(PayrollTransition::Post),
        Some(platform::PeoplePayrollRunTransition::Cancel) => Ok(PayrollTransition::Cancel),
        _ => Err(invalid("payroll transition is invalid")),
    }
}
fn decode_employee_status(value: i32) -> Result<erpx_people_payroll::EmployeeStatus, Status> {
    use erpx_people_payroll::EmployeeStatus;
    match platform::PeoplePayrollEmployeeStatus::try_from(value).ok() {
        Some(platform::PeoplePayrollEmployeeStatus::Draft) => Ok(EmployeeStatus::Draft),
        Some(platform::PeoplePayrollEmployeeStatus::Active) => Ok(EmployeeStatus::Active),
        Some(platform::PeoplePayrollEmployeeStatus::OnLeave) => Ok(EmployeeStatus::OnLeave),
        Some(platform::PeoplePayrollEmployeeStatus::Suspended) => Ok(EmployeeStatus::Suspended),
        Some(platform::PeoplePayrollEmployeeStatus::Terminated) => Ok(EmployeeStatus::Terminated),
        _ => Err(invalid("employee status is invalid")),
    }
}
fn encode_employee_status(value: erpx_people_payroll::EmployeeStatus) -> i32 {
    use erpx_people_payroll::EmployeeStatus;
    match value {
        EmployeeStatus::Draft => platform::PeoplePayrollEmployeeStatus::Draft as i32,
        EmployeeStatus::Active => platform::PeoplePayrollEmployeeStatus::Active as i32,
        EmployeeStatus::OnLeave => platform::PeoplePayrollEmployeeStatus::OnLeave as i32,
        EmployeeStatus::Suspended => platform::PeoplePayrollEmployeeStatus::Suspended as i32,
        EmployeeStatus::Terminated => platform::PeoplePayrollEmployeeStatus::Terminated as i32,
    }
}
fn decode_leave_status(value: i32) -> Result<erpx_people_payroll::LeaveStatus, Status> {
    use erpx_people_payroll::LeaveStatus;
    match platform::PeoplePayrollLeaveStatus::try_from(value).ok() {
        Some(platform::PeoplePayrollLeaveStatus::Draft) => Ok(LeaveStatus::Draft),
        Some(platform::PeoplePayrollLeaveStatus::Submitted) => Ok(LeaveStatus::Submitted),
        Some(platform::PeoplePayrollLeaveStatus::Approved) => Ok(LeaveStatus::Approved),
        Some(platform::PeoplePayrollLeaveStatus::Rejected) => Ok(LeaveStatus::Rejected),
        Some(platform::PeoplePayrollLeaveStatus::Cancelled) => Ok(LeaveStatus::Cancelled),
        Some(platform::PeoplePayrollLeaveStatus::Taken) => Ok(LeaveStatus::Taken),
        _ => Err(invalid("leave status is invalid")),
    }
}
fn encode_leave_status(value: erpx_people_payroll::LeaveStatus) -> i32 {
    use erpx_people_payroll::LeaveStatus;
    match value {
        LeaveStatus::Draft => platform::PeoplePayrollLeaveStatus::Draft as i32,
        LeaveStatus::Submitted => platform::PeoplePayrollLeaveStatus::Submitted as i32,
        LeaveStatus::Approved => platform::PeoplePayrollLeaveStatus::Approved as i32,
        LeaveStatus::Rejected => platform::PeoplePayrollLeaveStatus::Rejected as i32,
        LeaveStatus::Cancelled => platform::PeoplePayrollLeaveStatus::Cancelled as i32,
        LeaveStatus::Taken => platform::PeoplePayrollLeaveStatus::Taken as i32,
    }
}
fn decode_expense_status(value: i32) -> Result<erpx_people_payroll::ExpenseStatus, Status> {
    use erpx_people_payroll::ExpenseStatus;
    match platform::PeoplePayrollExpenseStatus::try_from(value).ok() {
        Some(platform::PeoplePayrollExpenseStatus::Draft) => Ok(ExpenseStatus::Draft),
        Some(platform::PeoplePayrollExpenseStatus::Submitted) => Ok(ExpenseStatus::Submitted),
        Some(platform::PeoplePayrollExpenseStatus::Approved) => Ok(ExpenseStatus::Approved),
        Some(platform::PeoplePayrollExpenseStatus::Rejected) => Ok(ExpenseStatus::Rejected),
        Some(platform::PeoplePayrollExpenseStatus::Reimbursed) => Ok(ExpenseStatus::Reimbursed),
        _ => Err(invalid("expense status is invalid")),
    }
}
fn encode_expense_status(value: erpx_people_payroll::ExpenseStatus) -> i32 {
    use erpx_people_payroll::ExpenseStatus;
    match value {
        ExpenseStatus::Draft => platform::PeoplePayrollExpenseStatus::Draft as i32,
        ExpenseStatus::Submitted => platform::PeoplePayrollExpenseStatus::Submitted as i32,
        ExpenseStatus::Approved => platform::PeoplePayrollExpenseStatus::Approved as i32,
        ExpenseStatus::Rejected => platform::PeoplePayrollExpenseStatus::Rejected as i32,
        ExpenseStatus::Reimbursed => platform::PeoplePayrollExpenseStatus::Reimbursed as i32,
    }
}
fn decode_payroll_status(value: i32) -> Result<erpx_people_payroll::PayrollStatus, Status> {
    use erpx_people_payroll::PayrollStatus;
    match platform::PeoplePayrollRunStatus::try_from(value).ok() {
        Some(platform::PeoplePayrollRunStatus::Draft) => Ok(PayrollStatus::Draft),
        Some(platform::PeoplePayrollRunStatus::Review) => Ok(PayrollStatus::Review),
        Some(platform::PeoplePayrollRunStatus::Approved) => Ok(PayrollStatus::Approved),
        Some(platform::PeoplePayrollRunStatus::Posted) => Ok(PayrollStatus::Posted),
        Some(platform::PeoplePayrollRunStatus::Cancelled) => Ok(PayrollStatus::Cancelled),
        _ => Err(invalid("payroll status is invalid")),
    }
}
fn encode_payroll_status(value: erpx_people_payroll::PayrollStatus) -> i32 {
    use erpx_people_payroll::PayrollStatus;
    match value {
        PayrollStatus::Draft => platform::PeoplePayrollRunStatus::Draft as i32,
        PayrollStatus::Review => platform::PeoplePayrollRunStatus::Review as i32,
        PayrollStatus::Approved => platform::PeoplePayrollRunStatus::Approved as i32,
        PayrollStatus::Posted => platform::PeoplePayrollRunStatus::Posted as i32,
        PayrollStatus::Cancelled => platform::PeoplePayrollRunStatus::Cancelled as i32,
    }
}

fn employee_id(value: &str) -> Result<EmployeeId, Status> {
    EmployeeId::try_from_uuid(parse_uuid(value, "employee_id")?)
        .map_err(|_| invalid("employee_id is invalid"))
}
fn leave_id(value: &str) -> Result<LeaveRequestId, Status> {
    LeaveRequestId::try_from_uuid(parse_uuid(value, "leave_request_id")?)
        .map_err(|_| invalid("leave_request_id is invalid"))
}
fn expense_id(value: &str) -> Result<ExpenseClaimId, Status> {
    ExpenseClaimId::try_from_uuid(parse_uuid(value, "expense_claim_id")?)
        .map_err(|_| invalid("expense_claim_id is invalid"))
}
fn payroll_id(value: &str) -> Result<PayrollRunId, Status> {
    PayrollRunId::try_from_uuid(parse_uuid(value, "payroll_run_id")?)
        .map_err(|_| invalid("payroll_run_id is invalid"))
}
fn date(value: &str, field: &'static str) -> Result<NaiveDate, Status> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| invalid(&format!("{field} is invalid")))
}
fn page_size(value: u32) -> Result<u16, Status> {
    u16::try_from(value).map_err(|_| invalid("page_size is invalid"))
}

fn encode_receipt<R>(value: &CommandReceipt<R>) -> common::CommandReceipt {
    common::CommandReceipt {
        command_id: value.command_id.to_string(),
        correlation_id: value.correlation_id.to_string(),
        aggregate: Some(common::AggregateVersion {
            context: value.aggregate.context.clone(),
            aggregate_type: value.aggregate.aggregate_type.clone(),
            aggregate_id: value.aggregate.aggregate_id.to_string(),
            version: value.aggregate.version,
        }),
        committed_events: value
            .committed_events
            .iter()
            .map(|position| common::EventPosition {
                event_id: position.event_id.to_string(),
                global_sequence: position.global_sequence,
            })
            .collect(),
        committed_at: value.committed_at.to_rfc3339(),
        replayed: value.replayed,
    }
}
fn encode_query_metadata(value: &QueryMetadata) -> common::QueryMetadata {
    common::QueryMetadata {
        correlation_id: value.correlation_id.to_string(),
        projection: Some(common::ProjectionMetadata {
            checkpoint: value.projection.checkpoint.as_ref().map(|checkpoint| {
                common::ProjectionCheckpoint {
                    projection: checkpoint.projection.clone(),
                    partition: checkpoint.partition.clone(),
                    event_id: checkpoint.event_id.to_string(),
                    global_sequence: checkpoint.global_sequence,
                    updated_at: checkpoint.updated_at.to_rfc3339(),
                }
            }),
            freshness: match value.projection.freshness {
                FreshnessState::Current => common::ProjectionFreshnessState::Current as i32,
                FreshnessState::Stale => common::ProjectionFreshnessState::Stale as i32,
                FreshnessState::Rebuilding => common::ProjectionFreshnessState::Rebuilding as i32,
                FreshnessState::Unavailable => common::ProjectionFreshnessState::Unavailable as i32,
            },
            lag_millis: value.projection.lag_millis,
        }),
        consistency: Some(encode_consistency(&value.consistency)),
    }
}
fn decode_consistency(value: Option<common::QueryConsistency>) -> Result<QueryConsistency, Status> {
    use common::query_consistency::Requirement;
    match value.and_then(|value| value.requirement) {
        None | Some(Requirement::Eventual(_)) => Ok(QueryConsistency::Eventual),
        Some(Requirement::ReadYourWrites(value)) => {
            Ok(QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: value.projection,
                partition: value.partition,
                target: erpx_cqrs::ProjectionTarget {
                    event_id: event_id(&value.event_id)?,
                    global_sequence: value.global_sequence,
                },
                timeout_millis: value.timeout_millis,
            }))
        }
        Some(Requirement::AggregateVersion(_)) => {
            Err(invalid("aggregate-version reads are unsupported"))
        }
    }
}
fn encode_consistency(value: &QueryConsistency) -> common::QueryConsistency {
    use common::query_consistency::Requirement;
    common::QueryConsistency {
        requirement: Some(match value {
            QueryConsistency::Eventual => Requirement::Eventual(common::EventualConsistency {}),
            QueryConsistency::ReadYourWrites(value) => {
                let wire = common::ReadYourWrites {
                    projection: value.projection.clone(),
                    partition: value.partition.clone(),
                    event_id: value.target.event_id.to_string(),
                    global_sequence: value.target.global_sequence,
                    timeout_millis: value.timeout_millis,
                    ..Default::default()
                };
                Requirement::ReadYourWrites(wire)
            }
        }),
    }
}
fn decode_route(value: Option<common::TenantRoute>) -> Result<PeoplePayrollTenantRoute, Status> {
    let value = value.ok_or_else(|| invalid("tenant route is required"))?;
    let route = PeoplePayrollTenantRoute {
        tenant_id: value.tenant_id.trim().into(),
        holding_ref: value.holding_ref.trim().into(),
    };
    validate_route(&route)?;
    Ok(route)
}
fn validate_route(value: &PeoplePayrollTenantRoute) -> Result<(), Status> {
    if [&value.tenant_id, &value.holding_ref]
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_ROUTE_LENGTH)
    {
        Err(invalid("tenant route is invalid"))
    } else {
        Ok(())
    }
}
fn validate_route_scope(
    route: &PeoplePayrollTenantRoute,
    scope: &TenantScope,
) -> Result<(), Status> {
    let holding = match scope {
        TenantScope::Platform => return Ok(()),
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => holding_id,
    };
    if parse_uuid(&route.holding_ref, "holding_ref")? == *holding {
        Ok(())
    } else {
        Err(invalid(
            "tenant route does not match People & Payroll scope",
        ))
    }
}
fn decode_actor(value: Option<common::ActorAttribution>) -> Result<ActorAttribution, Status> {
    let value = value.ok_or_else(|| invalid("actor is required"))?;
    let kind = match common::ActorKind::try_from(value.kind).ok() {
        Some(common::ActorKind::PlatformUser) => ActorKind::PlatformUser,
        Some(common::ActorKind::TenantUser) => ActorKind::TenantUser,
        Some(common::ActorKind::Service) => ActorKind::Service,
        Some(common::ActorKind::SystemWorker) => ActorKind::SystemWorker,
        _ => return Err(invalid("actor kind is invalid")),
    };
    Ok(ActorAttribution {
        principal_ref: value.principal_ref,
        kind,
        session_id: optional_uuid(&value.session_id)?
            .map(SessionId::try_from_uuid)
            .transpose()
            .map_err(|_| invalid("session_id is invalid"))?,
        represented_principal_ref: nonempty(value.represented_principal_ref),
        support_capability_id: optional_uuid(&value.support_capability_id)?,
    })
}
fn decode_audit(value: Option<common::AuditContext>) -> Result<AuditContext, Status> {
    let value = value.ok_or_else(|| invalid("audit is required"))?;
    Ok(AuditContext {
        purpose: value.purpose,
        customer_visible: value.customer_visible,
        module: nonempty(value.module),
    })
}
fn decode_scope(value: common::TenantScope) -> Result<TenantScope, Status> {
    use common::tenant_scope::Scope;
    match value.scope.ok_or_else(|| invalid("scope is required"))? {
        Scope::Platform(_) => Ok(TenantScope::Platform),
        Scope::Holding(value) => Ok(TenantScope::Holding {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
        }),
        Scope::Company(value) => Ok(TenantScope::Company {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
        }),
        Scope::Branch(value) => Ok(TenantScope::Branch {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_id: parse_uuid(&value.company_id, "company_id")?,
            branch_id: parse_uuid(&value.branch_id, "branch_id")?,
        }),
        Scope::Subtree(value) => Ok(TenantScope::Subtree {
            holding_id: parse_uuid(&value.holding_id, "holding_id")?,
            company_ids: value
                .company_ids
                .iter()
                .map(|id| parse_uuid(id, "company_id"))
                .collect::<Result<_, _>>()?,
            branch_ids: value
                .branch_ids
                .iter()
                .map(|id| parse_uuid(id, "branch_id"))
                .collect::<Result<_, _>>()?,
        }),
    }
}
fn encode_scope(value: &TenantScope) -> common::TenantScope {
    use common::tenant_scope::Scope;
    common::TenantScope {
        scope: Some(match value {
            TenantScope::Platform => Scope::Platform(common::PlatformScope {}),
            TenantScope::Holding { holding_id } => Scope::Holding(common::HoldingScope {
                holding_id: holding_id.to_string(),
            }),
            TenantScope::Company {
                holding_id,
                company_id,
            } => Scope::Company(common::CompanyScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
            }),
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Scope::Branch(common::BranchScope {
                holding_id: holding_id.to_string(),
                company_id: company_id.to_string(),
                branch_id: branch_id.to_string(),
            }),
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Scope::Subtree(common::SubtreeScope {
                holding_id: holding_id.to_string(),
                company_ids: company_ids.iter().map(ToString::to_string).collect(),
                branch_ids: branch_ids.iter().map(ToString::to_string).collect(),
            }),
        }),
    }
}
fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, Status> {
    Uuid::parse_str(value)
        .map_err(|_| invalid(&format!("{field} is invalid")))
        .and_then(|id| {
            if id.is_nil() {
                Err(invalid(&format!("{field} is invalid")))
            } else {
                Ok(id)
            }
        })
}
fn correlation(value: &str) -> Result<CorrelationId, Status> {
    CorrelationId::try_from_uuid(parse_uuid(value, "correlation_id")?)
        .map_err(|_| invalid("correlation_id is invalid"))
}
fn event_id(value: &str) -> Result<EventId, Status> {
    EventId::try_from_uuid(parse_uuid(value, "event_id")?)
        .map_err(|_| invalid("event_id is invalid"))
}
fn optional_uuid(value: &str) -> Result<Option<Uuid>, Status> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse_uuid(value, "optional UUID").map(Some)
    }
}
fn nonempty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
fn clean(value: Option<String>) -> Option<String> {
    value.and_then(nonempty)
}
fn response_mismatch() -> Status {
    Status::internal("People & Payroll response mismatch")
}
fn status_from_cqrs(value: CqrsError) -> Status {
    match value.category() {
        ErrorCategory::Invalid => Status::invalid_argument(value.to_string()),
        ErrorCategory::Denied => Status::permission_denied(value.to_string()),
        ErrorCategory::NotFound => Status::not_found(value.to_string()),
        ErrorCategory::Conflict => Status::aborted(value.to_string()),
        ErrorCategory::NotReady => Status::failed_precondition(value.to_string()),
        ErrorCategory::Unavailable => Status::unavailable(value.to_string()),
        ErrorCategory::Internal => Status::internal(value.to_string()),
    }
}
fn status_from_auth(value: PeoplePayrollAuthenticationError) -> Status {
    match value {
        PeoplePayrollAuthenticationError::Unauthenticated => {
            Status::unauthenticated("authentication required")
        }
        PeoplePayrollAuthenticationError::Denied => {
            Status::permission_denied("People & Payroll access denied")
        }
        PeoplePayrollAuthenticationError::Unavailable => {
            Status::unavailable("authentication unavailable")
        }
        PeoplePayrollAuthenticationError::Internal => Status::internal("authentication failed"),
    }
}
fn invalid(value: &str) -> Status {
    Status::invalid_argument(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: "people-payroll-route-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }
    #[test]
    fn status_and_scope_round_trip() {
        use erpx_people_payroll::PayrollStatus;
        assert_eq!(
            decode_payroll_status(encode_payroll_status(PayrollStatus::Posted)).unwrap(),
            PayrollStatus::Posted
        );
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        };
        assert_eq!(decode_scope(encode_scope(&scope)).unwrap(), scope);
    }
    #[test]
    fn secret_bearing_auth_request_has_no_debug() {
        let source = include_str!("people_payroll_cqrs.rs");
        let declaration = source
            .find(concat!("pub struct PeoplePayroll", "AuthenticationRequest"))
            .unwrap();
        let attributes = source[..declaration]
            .lines()
            .rev()
            .take_while(|line| line.trim_start().starts_with("#["))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!attributes.contains("Debug"));
    }
    #[test]
    fn authenticated_route_must_match_authority_holding() {
        let holding = Uuid::from_u128(11);
        let scope = TenantScope::Company {
            holding_id: holding,
            company_id: Uuid::from_u128(12),
        };
        let matching = PeoplePayrollTenantRoute {
            tenant_id: "tenant-a".into(),
            holding_ref: holding.to_string(),
        };
        assert!(
            AuthenticatedPeoplePayrollRequest::try_new(actor(), scope.clone(), matching).is_ok()
        );
        let mismatch = PeoplePayrollTenantRoute {
            tenant_id: "tenant-a".into(),
            holding_ref: Uuid::from_u128(99).to_string(),
        };
        assert_eq!(
            AuthenticatedPeoplePayrollRequest::try_new(actor(), scope, mismatch),
            Err(PeoplePayrollAuthenticationError::Internal)
        );
    }
}
