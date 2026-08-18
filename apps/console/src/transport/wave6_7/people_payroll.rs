use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_people_payroll::{
    Currency, EmployeeId, EmployeeStatus, EmployeeTransition, ExpenseClaimId, ExpenseStatus,
    ExpenseTransition, LeaveRequestId, LeaveStatus, LeaveTransition, PayrollLine, PayrollRunId,
    PayrollStatus, PayrollTransition, PeoplePayrollCommand, PeoplePayrollMutationResult,
    PeoplePayrollQuery, PeoplePayrollQueryResponse,
    ports::{
        EmployeeList, EmployeeView, ExpenseClaimList, ExpenseClaimView, LeaveRequestList,
        LeaveRequestView, PayrollRunList, PayrollRunView,
    },
};
use erpx_proto::erpx::people_payroll::v2 as wire;
use erpx_ui_foundation::UiError;

use super::common::{
    AuthenticatedRoute, Wave67Route, command_result, date, decode_scope, invalid_response,
    query_result, status_ui, timestamp, uuid,
};
use crate::{
    features::people_payroll::PeoplePayrollProtoTransport, transport::typed_cqrs::GrpcChannel,
};

type Commands =
    wire::people_payroll_command_service_client::PeoplePayrollCommandServiceClient<GrpcChannel>;
type Queries =
    wire::people_payroll_query_service_client::PeoplePayrollQueryServiceClient<GrpcChannel>;
#[derive(Clone)]
pub(crate) struct PeoplePayrollGrpcTransport {
    auth: AuthenticatedRoute,
    commands: Commands,
    queries: Queries,
}
impl PeoplePayrollGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        token: impl Into<String>,
        route: Wave67Route,
    ) -> Result<Self, UiError> {
        Ok(Self {
            auth: AuthenticatedRoute::try_new(token, route)?,
            commands: Commands::new(channel.clone()),
            queries: Queries::new(channel),
        })
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl PeoplePayrollProtoTransport for PeoplePayrollGrpcTransport {
    async fn command(
        &self,
        envelope: CommandEnvelope<PeoplePayrollCommand>,
    ) -> Result<CommandReceipt<PeoplePayrollMutationResult>, UiError> {
        let metadata = self.auth.command(&envelope)?;
        let actor = envelope.actor.clone();
        let command = envelope.command_id;
        let correlation = envelope.correlation_id;
        let mut client = self.commands.clone();
        match envelope.command {
            PeoplePayrollCommand::HireEmployee(value) => {
                let response = client
                    .hire_employee(self.auth.request(
                        wire::HireEmployeeRequest {
                            metadata: Some(metadata),
                            employee_id: value.employee_id.as_uuid().to_string(),
                            employee_code: value.employee_code,
                            display_name: value.display_name,
                            hired_on: value.hired_on.to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll hire employee", s))?
                    .into_inner();
                employee_result(response, command, correlation)
            }
            PeoplePayrollCommand::TransitionEmployee(value) => {
                let response = client
                    .transition_employee(self.auth.request(
                        wire::TransitionEmployeeRequest {
                            metadata: Some(metadata),
                            employee_id: value.employee_id.as_uuid().to_string(),
                            transition: Some(encode_employee_transition(value.transition)),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll transition employee", s))?
                    .into_inner();
                employee_result(response, command, correlation)
            }
            PeoplePayrollCommand::CreateLeaveRequest(value) => {
                let response = client
                    .create_leave_request(self.auth.request(
                        wire::CreateLeaveRequestRequest {
                            metadata: Some(metadata),
                            leave_request_id: value.leave_request_id.as_uuid().to_string(),
                            employee_id: value.employee_id.as_uuid().to_string(),
                            starts_on: value.starts_on.to_string(),
                            ends_on: value.ends_on.to_string(),
                            reason: value.reason,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll create leave request", s))?
                    .into_inner();
                leave_result(response, command, correlation)
            }
            PeoplePayrollCommand::TransitionLeaveRequest(value) => {
                let response = client
                    .transition_leave_request(self.auth.request(
                        wire::TransitionLeaveRequestRequest {
                            metadata: Some(metadata),
                            leave_request_id: value.leave_request_id.as_uuid().to_string(),
                            transition: encode_leave_transition(value.transition),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll transition leave request", s))?
                    .into_inner();
                leave_result(response, command, correlation)
            }
            PeoplePayrollCommand::CreateExpenseClaim(value) => {
                let response = client
                    .create_expense_claim(self.auth.request(
                        wire::CreateExpenseClaimRequest {
                            metadata: Some(metadata),
                            expense_claim_id: value.expense_claim_id.as_uuid().to_string(),
                            employee_id: value.employee_id.as_uuid().to_string(),
                            currency: value.currency.as_str().into(),
                            amount_minor: value.amount_minor,
                            reason: value.reason,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll create expense claim", s))?
                    .into_inner();
                expense_result(response, command, correlation)
            }
            PeoplePayrollCommand::TransitionExpenseClaim(value) => {
                let response = client
                    .transition_expense_claim(self.auth.request(
                        wire::TransitionExpenseClaimRequest {
                            metadata: Some(metadata),
                            expense_claim_id: value.expense_claim_id.as_uuid().to_string(),
                            transition: encode_expense_transition(value.transition),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll transition expense claim", s))?
                    .into_inner();
                expense_result(response, command, correlation)
            }
            PeoplePayrollCommand::CreatePayrollRun(value) => {
                let response = client
                    .create_payroll_run(self.auth.request(
                        wire::CreatePayrollRunRequest {
                            metadata: Some(metadata),
                            payroll_run_id: value.payroll_run_id.as_uuid().to_string(),
                            period_code: value.period_code,
                            starts_on: value.starts_on.to_string(),
                            ends_on: value.ends_on.to_string(),
                            currency: value.currency.as_str().into(),
                            lines: value.lines.into_iter().map(encode_line).collect(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll create payroll run", s))?
                    .into_inner();
                payroll_result(response, command, correlation)
            }
            PeoplePayrollCommand::TransitionPayrollRun(value) => {
                let response = client
                    .transition_payroll_run(self.auth.request(
                        wire::TransitionPayrollRunRequest {
                            metadata: Some(metadata),
                            payroll_run_id: value.payroll_run_id.as_uuid().to_string(),
                            transition: encode_payroll_transition(value.transition),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll transition payroll run", s))?
                    .into_inner();
                payroll_result(response, command, correlation)
            }
        }
    }
    async fn query(
        &self,
        envelope: QueryEnvelope<PeoplePayrollQuery>,
    ) -> Result<QueryResult<PeoplePayrollQueryResponse>, UiError> {
        let metadata = self.auth.query(&envelope)?;
        let actor = envelope.actor.clone();
        let correlation = envelope.correlation_id;
        let consistency = envelope.consistency.clone();
        let mut client = self.queries.clone();
        match envelope.query {
            PeoplePayrollQuery::GetEmployee(value) => {
                let r = client
                    .get_employee(self.auth.request(
                        wire::GetPeoplePayrollEmployeeRequest {
                            metadata: Some(metadata),
                            employee_id: value.employee_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll get employee", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::Employee(
                        r.employee.map(decode_employee).transpose()?,
                    ),
                    correlation,
                    consistency,
                )
            }
            PeoplePayrollQuery::ListEmployees(value) => {
                let f = value.filter;
                let r = client
                    .list_employees(self.auth.request(
                        wire::ListPeoplePayrollEmployeesRequest {
                            metadata: Some(metadata),
                            status: f.status.map(encode_employee_status),
                            page_size: u32::from(f.page_size),
                            cursor: f.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll list employees", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::Employees(EmployeeList {
                        employees: r
                            .employees
                            .into_iter()
                            .map(decode_employee)
                            .collect::<Result<_, _>>()?,
                        next_cursor: r.next_cursor,
                    }),
                    correlation,
                    consistency,
                )
            }
            PeoplePayrollQuery::GetLeaveRequest(value) => {
                let r = client
                    .get_leave_request(self.auth.request(
                        wire::GetPeoplePayrollLeaveRequest {
                            metadata: Some(metadata),
                            leave_request_id: value.leave_request_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll get leave request", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::LeaveRequest(
                        r.leave_request.map(decode_leave).transpose()?,
                    ),
                    correlation,
                    consistency,
                )
            }
            PeoplePayrollQuery::ListLeaveRequests(value) => {
                let f = value.filter;
                let r = client
                    .list_leave_requests(self.auth.request(
                        wire::ListPeoplePayrollLeaveRequestsRequest {
                            metadata: Some(metadata),
                            employee_id: f.employee_id.map(|id| id.as_uuid().to_string()),
                            status: f.status.map(encode_leave_status),
                            page_size: u32::from(f.page_size),
                            cursor: f.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll list leave requests", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::LeaveRequests(LeaveRequestList {
                        leave_requests: r
                            .leave_requests
                            .into_iter()
                            .map(decode_leave)
                            .collect::<Result<_, _>>()?,
                        next_cursor: r.next_cursor,
                    }),
                    correlation,
                    consistency,
                )
            }
            PeoplePayrollQuery::GetExpenseClaim(value) => {
                let r = client
                    .get_expense_claim(self.auth.request(
                        wire::GetPeoplePayrollExpenseClaimRequest {
                            metadata: Some(metadata),
                            expense_claim_id: value.expense_claim_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll get expense claim", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::ExpenseClaim(
                        r.expense_claim.map(decode_expense).transpose()?,
                    ),
                    correlation,
                    consistency,
                )
            }
            PeoplePayrollQuery::ListExpenseClaims(value) => {
                let f = value.filter;
                let r = client
                    .list_expense_claims(self.auth.request(
                        wire::ListPeoplePayrollExpenseClaimsRequest {
                            metadata: Some(metadata),
                            employee_id: f.employee_id.map(|id| id.as_uuid().to_string()),
                            status: f.status.map(encode_expense_status),
                            page_size: u32::from(f.page_size),
                            cursor: f.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll list expense claims", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::ExpenseClaims(ExpenseClaimList {
                        expense_claims: r
                            .expense_claims
                            .into_iter()
                            .map(decode_expense)
                            .collect::<Result<_, _>>()?,
                        next_cursor: r.next_cursor,
                    }),
                    correlation,
                    consistency,
                )
            }
            PeoplePayrollQuery::GetPayrollRun(value) => {
                let r = client
                    .get_payroll_run(self.auth.request(
                        wire::GetPeoplePayrollRunRequest {
                            metadata: Some(metadata),
                            payroll_run_id: value.payroll_run_id.as_uuid().to_string(),
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll get payroll run", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::PayrollRun(
                        r.payroll_run.map(decode_payroll).transpose()?,
                    ),
                    correlation,
                    consistency,
                )
            }
            PeoplePayrollQuery::ListPayrollRuns(value) => {
                let f = value.filter;
                let r = client
                    .list_payroll_runs(self.auth.request(
                        wire::ListPeoplePayrollRunsRequest {
                            metadata: Some(metadata),
                            status: f.status.map(encode_payroll_status),
                            starts_on_or_after: f.starts_on_or_after.map(|v| v.to_string()),
                            ends_on_or_before: f.ends_on_or_before.map(|v| v.to_string()),
                            page_size: u32::from(f.page_size),
                            cursor: f.cursor,
                        },
                        &actor,
                    )?)
                    .await
                    .map_err(|s| status_ui("people payroll list payroll runs", s))?
                    .into_inner();
                query_result(
                    r.metadata,
                    PeoplePayrollQueryResponse::PayrollRuns(PayrollRunList {
                        payroll_runs: r
                            .payroll_runs
                            .into_iter()
                            .map(decode_payroll)
                            .collect::<Result<_, _>>()?,
                        next_cursor: r.next_cursor,
                    }),
                    correlation,
                    consistency,
                )
            }
        }
    }
}

fn employee_result(
    r: wire::PeoplePayrollEmployeeMutationResponse,
    c: erpx_kernel::CommandId,
    x: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<PeoplePayrollMutationResult>, UiError> {
    let v = decode_employee(
        r.employee
            .ok_or_else(|| invalid_response("decode employee"))?,
    )?;
    command_result(r.receipt, PeoplePayrollMutationResult::Employee(v), c, x)
}
fn leave_result(
    r: wire::PeoplePayrollLeaveMutationResponse,
    c: erpx_kernel::CommandId,
    x: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<PeoplePayrollMutationResult>, UiError> {
    let v = decode_leave(
        r.leave_request
            .ok_or_else(|| invalid_response("decode leave request"))?,
    )?;
    command_result(
        r.receipt,
        PeoplePayrollMutationResult::LeaveRequest(v),
        c,
        x,
    )
}
fn expense_result(
    r: wire::PeoplePayrollExpenseMutationResponse,
    c: erpx_kernel::CommandId,
    x: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<PeoplePayrollMutationResult>, UiError> {
    let v = decode_expense(
        r.expense_claim
            .ok_or_else(|| invalid_response("decode expense claim"))?,
    )?;
    command_result(
        r.receipt,
        PeoplePayrollMutationResult::ExpenseClaim(v),
        c,
        x,
    )
}
fn payroll_result(
    r: wire::PeoplePayrollRunMutationResponse,
    c: erpx_kernel::CommandId,
    x: erpx_kernel::CorrelationId,
) -> Result<CommandReceipt<PeoplePayrollMutationResult>, UiError> {
    let v = decode_payroll(
        r.payroll_run
            .ok_or_else(|| invalid_response("decode payroll run"))?,
    )?;
    command_result(r.receipt, PeoplePayrollMutationResult::PayrollRun(v), c, x)
}

fn decode_employee(v: wire::PeoplePayrollEmployeeRecord) -> Result<EmployeeView, UiError> {
    Ok(EmployeeView {
        employee_id: EmployeeId::try_from_uuid(uuid(&v.employee_id, "decode employee")?)
            .map_err(|_| invalid_response("decode employee"))?,
        version: v.source_version,
        scope: decode_scope(v.scope)?,
        employee_code: v.employee_code,
        display_name: v.display_name,
        hired_on: date(&v.hired_on)?,
        terminated_on: v.terminated_on.map(|v| date(&v)).transpose()?,
        status: decode_employee_status(v.status)?,
        created_at: timestamp(&v.created_at)?,
        updated_at: timestamp(&v.updated_at)?,
    })
}
fn decode_leave(v: wire::PeoplePayrollLeaveRequestRecord) -> Result<LeaveRequestView, UiError> {
    Ok(LeaveRequestView {
        leave_request_id: LeaveRequestId::try_from_uuid(uuid(
            &v.leave_request_id,
            "decode leave request",
        )?)
        .map_err(|_| invalid_response("decode leave request"))?,
        version: v.source_version,
        scope: decode_scope(v.scope)?,
        employee_id: EmployeeId::try_from_uuid(uuid(&v.employee_id, "decode leave request")?)
            .map_err(|_| invalid_response("decode leave request"))?,
        starts_on: date(&v.starts_on)?,
        ends_on: date(&v.ends_on)?,
        reason: v.reason,
        status: decode_leave_status(v.status)?,
        requested_by: v.requested_by,
        approved_by: v.approved_by,
        created_at: timestamp(&v.created_at)?,
        updated_at: timestamp(&v.updated_at)?,
    })
}
fn decode_expense(v: wire::PeoplePayrollExpenseClaimRecord) -> Result<ExpenseClaimView, UiError> {
    Ok(ExpenseClaimView {
        expense_claim_id: ExpenseClaimId::try_from_uuid(uuid(
            &v.expense_claim_id,
            "decode expense claim",
        )?)
        .map_err(|_| invalid_response("decode expense claim"))?,
        version: v.source_version,
        scope: decode_scope(v.scope)?,
        employee_id: EmployeeId::try_from_uuid(uuid(&v.employee_id, "decode expense claim")?)
            .map_err(|_| invalid_response("decode expense claim"))?,
        currency: Currency::try_new(&v.currency)
            .map_err(|_| invalid_response("decode expense claim"))?,
        amount_minor: v.amount_minor,
        reason: v.reason,
        status: decode_expense_status(v.status)?,
        claimed_by: v.claimed_by,
        approved_by: v.approved_by,
        created_at: timestamp(&v.created_at)?,
        updated_at: timestamp(&v.updated_at)?,
    })
}
fn decode_payroll(v: wire::PeoplePayrollRunRecord) -> Result<PayrollRunView, UiError> {
    Ok(PayrollRunView {
        payroll_run_id: PayrollRunId::try_from_uuid(uuid(&v.payroll_run_id, "decode payroll run")?)
            .map_err(|_| invalid_response("decode payroll run"))?,
        version: v.source_version,
        scope: decode_scope(v.scope)?,
        period_code: v.period_code,
        starts_on: date(&v.starts_on)?,
        ends_on: date(&v.ends_on)?,
        currency: Currency::try_new(&v.currency)
            .map_err(|_| invalid_response("decode payroll run"))?,
        lines: v
            .lines
            .into_iter()
            .map(decode_line)
            .collect::<Result<_, _>>()?,
        status: decode_payroll_status(v.status)?,
        prepared_by: v.prepared_by,
        reviewed_by: v.reviewed_by,
        approved_by: v.approved_by,
        total_gross_minor: v.total_gross_minor,
        total_deductions_minor: v.total_deductions_minor,
        total_net_minor: v.total_net_minor,
        created_at: timestamp(&v.created_at)?,
        updated_at: timestamp(&v.updated_at)?,
    })
}
fn encode_line(v: PayrollLine) -> wire::PeoplePayrollLineRecord {
    wire::PeoplePayrollLineRecord {
        employee_id: v.employee_id.as_uuid().to_string(),
        gross_minor: v.gross_minor,
        deductions_minor: v.deductions_minor,
        net_minor: v.net_minor,
    }
}
fn decode_line(v: wire::PeoplePayrollLineRecord) -> Result<PayrollLine, UiError> {
    let line = PayrollLine::try_new(
        EmployeeId::try_from_uuid(uuid(&v.employee_id, "decode payroll line")?)
            .map_err(|_| invalid_response("decode payroll line"))?,
        v.gross_minor,
        v.deductions_minor,
    )
    .map_err(|_| invalid_response("decode payroll line"))?;
    if line.net_minor != v.net_minor {
        return Err(invalid_response("decode payroll line"));
    }
    Ok(line)
}

fn encode_employee_transition(v: EmployeeTransition) -> wire::PeoplePayrollEmployeeTransition {
    use wire::people_payroll_employee_transition::Transition;
    let transition = match v {
        EmployeeTransition::Activate => {
            Transition::Activate(wire::PeoplePayrollTransitionMarker {})
        }
        EmployeeTransition::PutOnLeave => {
            Transition::PutOnLeave(wire::PeoplePayrollTransitionMarker {})
        }
        EmployeeTransition::ReturnFromLeave => {
            Transition::ReturnFromLeave(wire::PeoplePayrollTransitionMarker {})
        }
        EmployeeTransition::Suspend => Transition::Suspend(wire::PeoplePayrollTransitionMarker {}),
        EmployeeTransition::Terminate { terminated_on } => {
            Transition::Terminate(wire::PeoplePayrollTerminateEmployee {
                terminated_on: terminated_on.to_string(),
            })
        }
    };
    wire::PeoplePayrollEmployeeTransition {
        transition: Some(transition),
    }
}
fn encode_leave_transition(v: LeaveTransition) -> i32 {
    match v {
        LeaveTransition::Submit => wire::PeoplePayrollLeaveTransition::Submit as i32,
        LeaveTransition::Approve => wire::PeoplePayrollLeaveTransition::Approve as i32,
        LeaveTransition::Reject => wire::PeoplePayrollLeaveTransition::Reject as i32,
        LeaveTransition::Cancel => wire::PeoplePayrollLeaveTransition::Cancel as i32,
        LeaveTransition::MarkTaken => wire::PeoplePayrollLeaveTransition::MarkTaken as i32,
    }
}
fn encode_expense_transition(v: ExpenseTransition) -> i32 {
    match v {
        ExpenseTransition::Submit => wire::PeoplePayrollExpenseTransition::Submit as i32,
        ExpenseTransition::Approve => wire::PeoplePayrollExpenseTransition::Approve as i32,
        ExpenseTransition::Reject => wire::PeoplePayrollExpenseTransition::Reject as i32,
        ExpenseTransition::Reimburse => wire::PeoplePayrollExpenseTransition::Reimburse as i32,
    }
}
fn encode_payroll_transition(v: PayrollTransition) -> i32 {
    match v {
        PayrollTransition::SubmitForReview => {
            wire::PeoplePayrollRunTransition::SubmitForReview as i32
        }
        PayrollTransition::Review => wire::PeoplePayrollRunTransition::Review as i32,
        PayrollTransition::Post => wire::PeoplePayrollRunTransition::Post as i32,
        PayrollTransition::Cancel => wire::PeoplePayrollRunTransition::Cancel as i32,
    }
}
fn encode_employee_status(v: EmployeeStatus) -> i32 {
    match v {
        EmployeeStatus::Draft => wire::PeoplePayrollEmployeeStatus::Draft as i32,
        EmployeeStatus::Active => wire::PeoplePayrollEmployeeStatus::Active as i32,
        EmployeeStatus::OnLeave => wire::PeoplePayrollEmployeeStatus::OnLeave as i32,
        EmployeeStatus::Suspended => wire::PeoplePayrollEmployeeStatus::Suspended as i32,
        EmployeeStatus::Terminated => wire::PeoplePayrollEmployeeStatus::Terminated as i32,
    }
}
fn encode_leave_status(v: LeaveStatus) -> i32 {
    match v {
        LeaveStatus::Draft => wire::PeoplePayrollLeaveStatus::Draft as i32,
        LeaveStatus::Submitted => wire::PeoplePayrollLeaveStatus::Submitted as i32,
        LeaveStatus::Approved => wire::PeoplePayrollLeaveStatus::Approved as i32,
        LeaveStatus::Rejected => wire::PeoplePayrollLeaveStatus::Rejected as i32,
        LeaveStatus::Cancelled => wire::PeoplePayrollLeaveStatus::Cancelled as i32,
        LeaveStatus::Taken => wire::PeoplePayrollLeaveStatus::Taken as i32,
    }
}
fn encode_expense_status(v: ExpenseStatus) -> i32 {
    match v {
        ExpenseStatus::Draft => wire::PeoplePayrollExpenseStatus::Draft as i32,
        ExpenseStatus::Submitted => wire::PeoplePayrollExpenseStatus::Submitted as i32,
        ExpenseStatus::Approved => wire::PeoplePayrollExpenseStatus::Approved as i32,
        ExpenseStatus::Rejected => wire::PeoplePayrollExpenseStatus::Rejected as i32,
        ExpenseStatus::Reimbursed => wire::PeoplePayrollExpenseStatus::Reimbursed as i32,
    }
}
fn encode_payroll_status(v: PayrollStatus) -> i32 {
    match v {
        PayrollStatus::Draft => wire::PeoplePayrollRunStatus::Draft as i32,
        PayrollStatus::Review => wire::PeoplePayrollRunStatus::Review as i32,
        PayrollStatus::Approved => wire::PeoplePayrollRunStatus::Approved as i32,
        PayrollStatus::Posted => wire::PeoplePayrollRunStatus::Posted as i32,
        PayrollStatus::Cancelled => wire::PeoplePayrollRunStatus::Cancelled as i32,
    }
}
fn decode_employee_status(v: i32) -> Result<EmployeeStatus, UiError> {
    match wire::PeoplePayrollEmployeeStatus::try_from(v).ok() {
        Some(wire::PeoplePayrollEmployeeStatus::Draft) => Ok(EmployeeStatus::Draft),
        Some(wire::PeoplePayrollEmployeeStatus::Active) => Ok(EmployeeStatus::Active),
        Some(wire::PeoplePayrollEmployeeStatus::OnLeave) => Ok(EmployeeStatus::OnLeave),
        Some(wire::PeoplePayrollEmployeeStatus::Suspended) => Ok(EmployeeStatus::Suspended),
        Some(wire::PeoplePayrollEmployeeStatus::Terminated) => Ok(EmployeeStatus::Terminated),
        _ => Err(invalid_response("decode employee status")),
    }
}
fn decode_leave_status(v: i32) -> Result<LeaveStatus, UiError> {
    match wire::PeoplePayrollLeaveStatus::try_from(v).ok() {
        Some(wire::PeoplePayrollLeaveStatus::Draft) => Ok(LeaveStatus::Draft),
        Some(wire::PeoplePayrollLeaveStatus::Submitted) => Ok(LeaveStatus::Submitted),
        Some(wire::PeoplePayrollLeaveStatus::Approved) => Ok(LeaveStatus::Approved),
        Some(wire::PeoplePayrollLeaveStatus::Rejected) => Ok(LeaveStatus::Rejected),
        Some(wire::PeoplePayrollLeaveStatus::Cancelled) => Ok(LeaveStatus::Cancelled),
        Some(wire::PeoplePayrollLeaveStatus::Taken) => Ok(LeaveStatus::Taken),
        _ => Err(invalid_response("decode leave status")),
    }
}
fn decode_expense_status(v: i32) -> Result<ExpenseStatus, UiError> {
    match wire::PeoplePayrollExpenseStatus::try_from(v).ok() {
        Some(wire::PeoplePayrollExpenseStatus::Draft) => Ok(ExpenseStatus::Draft),
        Some(wire::PeoplePayrollExpenseStatus::Submitted) => Ok(ExpenseStatus::Submitted),
        Some(wire::PeoplePayrollExpenseStatus::Approved) => Ok(ExpenseStatus::Approved),
        Some(wire::PeoplePayrollExpenseStatus::Rejected) => Ok(ExpenseStatus::Rejected),
        Some(wire::PeoplePayrollExpenseStatus::Reimbursed) => Ok(ExpenseStatus::Reimbursed),
        _ => Err(invalid_response("decode expense status")),
    }
}
fn decode_payroll_status(v: i32) -> Result<PayrollStatus, UiError> {
    match wire::PeoplePayrollRunStatus::try_from(v).ok() {
        Some(wire::PeoplePayrollRunStatus::Draft) => Ok(PayrollStatus::Draft),
        Some(wire::PeoplePayrollRunStatus::Review) => Ok(PayrollStatus::Review),
        Some(wire::PeoplePayrollRunStatus::Approved) => Ok(PayrollStatus::Approved),
        Some(wire::PeoplePayrollRunStatus::Posted) => Ok(PayrollStatus::Posted),
        Some(wire::PeoplePayrollRunStatus::Cancelled) => Ok(PayrollStatus::Cancelled),
        _ => Err(invalid_response("decode payroll status")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unspecified_statuses_fail_closed() {
        assert!(decode_employee_status(0).is_err());
        assert!(decode_leave_status(0).is_err());
        assert!(decode_expense_status(0).is_err());
        assert!(decode_payroll_status(0).is_err());
    }
}
