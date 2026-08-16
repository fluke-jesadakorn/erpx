#![doc = "People & Payroll bounded-context core."]
#![doc = ""]
#![doc = "This context owns employment, leave, employee-expense, and payroll-run"]
#![doc = "lifecycles. Identity accounts, organization structure, general-ledger"]
#![doc = "posting, and payment execution remain outside this boundary. Other"]
#![doc = "contexts consume only the typed employment and finalized-payroll facts."]
#![forbid(unsafe_code)]

pub mod application;
pub mod domain;
pub mod integration;
pub mod ports;

pub use application::{
    CreateExpenseClaim, CreateLeaveRequest, CreatePayrollRun, GetEmployee, GetExpenseClaim,
    GetLeaveRequest, GetPayrollRun, HireEmployee, ListEmployees, ListExpenseClaims,
    ListLeaveRequests, ListPayrollRuns, PeoplePayrollApplicationError, PeoplePayrollCommand,
    PeoplePayrollCommandHandler, PeoplePayrollMutationResult, PeoplePayrollQuery,
    PeoplePayrollQueryHandler, PeoplePayrollQueryResponse, PinPayrollCalculationEvidence,
    TimeRecordCommandHandler, TransitionEmployee, TransitionExpenseClaim, TransitionLeaveRequest,
    TransitionPayrollRun,
};
pub use domain::{
    Aggregate, Currency, Employee, EmployeeId, EmployeeState, EmployeeStatus, EmployeeTransition,
    ExpenseClaim, ExpenseClaimId, ExpenseClaimState, ExpenseStatus, ExpenseTransition,
    LeaveRequest, LeaveRequestId, LeaveRequestState, LeaveStatus, LeaveTransition,
    PayrollCalculationEvidence, PayrollInputEvidence, PayrollLine, PayrollRun, PayrollRunId,
    PayrollRunState, PayrollStatus, PayrollTransition, PeoplePayrollDomainError, TimeRecord,
    TimeRecordStatus, employment_record_ref_for, payslip_record_ref_for,
};
pub use ports::{
    AuthorizationPort, CommandUnitOfWork, EntitlementPort, PeoplePayrollAction,
    PeoplePayrollCommit, PortError, ReadRepository, StoredCommit, TimeRecordAuthorizationPort,
    TimeRecordCommand, TimeRecordQueryPort, TimeRecordReceipt, TimeRecordUnitOfWork,
    TimeRecordView, WriteRepository,
};
