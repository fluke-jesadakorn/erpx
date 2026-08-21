use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, CommandId, HumanDecisionProof, RecordId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod time_record;
pub use time_record::{TimeRecord, TimeRecordStatus};

pub const MAX_REFERENCE_BYTES: usize = 128;
pub const MAX_NAME_BYTES: usize = 255;
pub const MAX_REASON_BYTES: usize = 1_024;
pub const MAX_PAYROLL_LINES: usize = 100_000;
pub const MAX_PAYROLL_INPUTS: usize = 10_000;

macro_rules! context_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, PeoplePayrollDomainError> {
                if value.is_nil() {
                    return Err(PeoplePayrollDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = PeoplePayrollDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

context_id!(EmployeeId, "employee_id");
context_id!(LeaveRequestId, "leave_request_id");
context_id!(ExpenseClaimId, "expense_claim_id");
context_id!(PayrollRunId, "payroll_run_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeoplePayrollDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("People & Payroll records require an exact company or branch scope")]
    InvalidScope,
    #[error("the lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("the approval actor must be distinct from prior lifecycle actors")]
    ApprovalSeparation,
    #[error("the lifecycle decision requires matching authenticated human evidence")]
    InvalidHumanDecision,
    #[error("the date or timestamp window is invalid")]
    InvalidWindow,
    #[error("currency must be a three-letter uppercase ASCII code")]
    InvalidCurrency,
    #[error("monetary amounts must be non-negative and reconcile")]
    InvalidAmount,
    #[error("a payroll run must contain between 1 and 100,000 unique employee lines")]
    InvalidPayrollLines,
    #[error("the payroll calculation evidence is missing or invalid")]
    InvalidPayrollEvidence,
    #[error("time record lifecycle or evidence is invalid")]
    InvalidTimeRecord,
    #[error("People & Payroll aggregate version overflowed")]
    VersionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Currency([u8; 3]);

impl Currency {
    pub fn try_new(value: &str) -> Result<Self, PeoplePayrollDomainError> {
        let bytes: [u8; 3] = value
            .as_bytes()
            .try_into()
            .map_err(|_| PeoplePayrollDomainError::InvalidCurrency)?;
        if !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(PeoplePayrollDomainError::InvalidCurrency);
        }
        Ok(Self(bytes))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("validated ASCII currency")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmployeeStatus {
    Draft,
    Active,
    OnLeave,
    Suspended,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transition", rename_all = "snake_case")]
pub enum EmployeeTransition {
    Activate,
    PutOnLeave,
    ReturnFromLeave,
    Suspend,
    Terminate { terminated_on: NaiveDate },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Cancelled,
    Taken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveTransition {
    Submit,
    Approve,
    Reject,
    Cancel,
    MarkTaken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Reimbursed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseTransition {
    Submit,
    Approve,
    Reject,
    Reimburse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayrollStatus {
    Draft,
    Review,
    Approved,
    Posted,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayrollTransition {
    SubmitForReview,
    Review,
    Post,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    EmployeeStatusChanged {
        employee_id: EmployeeId,
        employee_code: String,
        display_name: String,
        status: EmployeeStatus,
        at: DateTime<Utc>,
    },
    LeaveStatusChanged {
        leave_request_id: LeaveRequestId,
        status: LeaveStatus,
        at: DateTime<Utc>,
    },
    ExpenseStatusChanged {
        expense_claim_id: ExpenseClaimId,
        status: ExpenseStatus,
        at: DateTime<Utc>,
    },
    PayrollStatusChanged {
        payroll_run_id: PayrollRunId,
        status: PayrollStatus,
        at: DateTime<Utc>,
    },
    PayrollEvidencePinned,
    PayrollFinalized {
        payroll_run_id: PayrollRunId,
        period_code: String,
        currency: Currency,
        total_gross_minor: i64,
        total_deductions_minor: i64,
        total_net_minor: i64,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Employee {
    id: EmployeeId,
    version: u64,
    scope: AuthorityScope,
    employee_code: String,
    display_name: String,
    hired_on: NaiveDate,
    terminated_on: Option<NaiveDate>,
    status: EmployeeStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

/// Complete relational state required to rehydrate an employee aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmployeeState {
    pub id: EmployeeId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub employee_code: String,
    pub display_name: String,
    pub hired_on: NaiveDate,
    pub terminated_on: Option<NaiveDate>,
    pub status: EmployeeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Employee {
    /// Rehydrates only from typed owner columns and revalidates every domain
    /// invariant. Persistence never restores uncommitted domain events.
    pub fn try_rehydrate(state: EmployeeState) -> Result<Self, PeoplePayrollDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            employee_code: state.employee_code,
            display_name: state.display_name,
            hired_on: state.hired_on,
            terminated_on: state.terminated_on,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn hire(
        id: EmployeeId,
        scope: AuthorityScope,
        employee_code: String,
        display_name: String,
        hired_on: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<Self, PeoplePayrollDomainError> {
        validate_scope(&scope)?;
        validate_text(&employee_code, "employee_code", MAX_REFERENCE_BYTES)?;
        validate_text(&display_name, "display_name", MAX_NAME_BYTES)?;
        let value = Self {
            id,
            version: 1,
            scope,
            employee_code,
            display_name,
            hired_on,
            terminated_on: None,
            status: EmployeeStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::EmployeeStatusChanged {
                employee_id: id,
                employee_code: String::new(),
                display_name: String::new(),
                status: EmployeeStatus::Draft,
                at,
            }],
        };
        // Build the publication payload only after all owned values are validated.
        let mut value = value;
        value.uncommitted_events[0] = value.status_event(at);
        value.validate()?;
        Ok(value)
    }

    pub fn transition(
        &mut self,
        transition: EmployeeTransition,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        if decision.authority_scope() != &self.scope
            || decision.decision_ref()
                != employee_transition_decision_ref(
                    command_id,
                    self.id,
                    expected_version,
                    &transition,
                )
            || decision.decided_at() != at
        {
            return Err(PeoplePayrollDomainError::InvalidHumanDecision);
        }
        let (next, terminated_on) = match transition {
            EmployeeTransition::Activate
                if matches!(
                    self.status,
                    EmployeeStatus::Draft | EmployeeStatus::Suspended
                ) =>
            {
                (EmployeeStatus::Active, None)
            }
            EmployeeTransition::PutOnLeave if self.status == EmployeeStatus::Active => {
                (EmployeeStatus::OnLeave, None)
            }
            EmployeeTransition::ReturnFromLeave if self.status == EmployeeStatus::OnLeave => {
                (EmployeeStatus::Active, None)
            }
            EmployeeTransition::Suspend
                if matches!(
                    self.status,
                    EmployeeStatus::Active | EmployeeStatus::OnLeave
                ) =>
            {
                (EmployeeStatus::Suspended, None)
            }
            EmployeeTransition::Terminate { terminated_on }
                if self.status != EmployeeStatus::Terminated && terminated_on >= self.hired_on =>
            {
                (EmployeeStatus::Terminated, Some(terminated_on))
            }
            _ => return Err(PeoplePayrollDomainError::InvalidTransition),
        };
        self.advance(at)?;
        self.status = next;
        self.terminated_on = terminated_on;
        self.uncommitted_events.push(self.status_event(at));
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn transition_for_test(
        &mut self,
        transition: EmployeeTransition,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        use erpx_kernel::{ActorAttribution, ActorKind, SessionId};

        let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("test command id");
        let decision = HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "people-payroll-test-user".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(
                    SessionId::try_from_uuid(Uuid::new_v4()).expect("test session id"),
                ),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            self.scope.clone(),
            employee_transition_decision_ref(command_id, self.id, self.version, &transition),
            at,
        )
        .expect("test human decision");
        self.transition(transition, &decision, command_id, self.version, at)
    }

    pub fn validate(&self) -> Result<(), PeoplePayrollDomainError> {
        validate_scope(&self.scope)?;
        validate_text(&self.employee_code, "employee_code", MAX_REFERENCE_BYTES)?;
        validate_text(&self.display_name, "display_name", MAX_NAME_BYTES)?;
        validate_common(self.version, self.created_at, self.updated_at)?;
        match (self.status, self.terminated_on) {
            (EmployeeStatus::Terminated, Some(date)) if date >= self.hired_on => Ok(()),
            (EmployeeStatus::Terminated, _) | (_, Some(_)) => {
                Err(PeoplePayrollDomainError::InvalidWindow)
            }
            _ => Ok(()),
        }
    }

    fn status_event(&self, at: DateTime<Utc>) -> DomainEvent {
        DomainEvent::EmployeeStatusChanged {
            employee_id: self.id,
            employee_code: self.employee_code.clone(),
            display_name: self.display_name.clone(),
            status: self.status,
            at,
        }
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PeoplePayrollDomainError> {
        advance(&mut self.version, &mut self.updated_at, at)
    }

    pub const fn id(&self) -> EmployeeId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn employee_code(&self) -> &str {
        &self.employee_code
    }
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    pub const fn hired_on(&self) -> NaiveDate {
        self.hired_on
    }
    pub const fn terminated_on(&self) -> Option<NaiveDate> {
        self.terminated_on
    }
    pub const fn status(&self) -> EmployeeStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        stable_record_ref(
            self.id.as_uuid(),
            ContextId::PeoplePayroll,
            "people.employee",
        )
    }
    /// Stable first-class employment identity derived by the owning domain.
    /// It is distinct from the employee identity and follows the employment
    /// lifecycle atomically with this aggregate.
    pub fn employment_record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        employment_record_ref_for(self.id)
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

pub fn employee_transition_decision_ref(
    command_id: CommandId,
    employee_id: EmployeeId,
    expected_version: u64,
    transition: &EmployeeTransition,
) -> String {
    let action = match transition {
        EmployeeTransition::Activate => "activate".to_owned(),
        EmployeeTransition::PutOnLeave => "put_on_leave".to_owned(),
        EmployeeTransition::ReturnFromLeave => "return_from_leave".to_owned(),
        EmployeeTransition::Suspend => "suspend".to_owned(),
        EmployeeTransition::Terminate { terminated_on } => {
            format!("terminate:{terminated_on}")
        }
    };
    format!(
        "people_payroll:transition_employee:{command_id}:{}:{expected_version}:{action}",
        employee_id.as_uuid()
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaveRequest {
    id: LeaveRequestId,
    version: u64,
    scope: AuthorityScope,
    employee_id: EmployeeId,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    reason: String,
    status: LeaveStatus,
    requested_by: String,
    approved_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveRequestState {
    pub id: LeaveRequestId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub employee_id: EmployeeId,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: String,
    pub status: LeaveStatus,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LeaveRequest {
    pub fn try_rehydrate(state: LeaveRequestState) -> Result<Self, PeoplePayrollDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            employee_id: state.employee_id,
            starts_on: state.starts_on,
            ends_on: state.ends_on,
            reason: state.reason,
            status: state.status,
            requested_by: state.requested_by,
            approved_by: state.approved_by,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: LeaveRequestId,
        scope: AuthorityScope,
        employee_id: EmployeeId,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        reason: String,
        requested_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PeoplePayrollDomainError> {
        validate_scope(&scope)?;
        validate_window(starts_on, ends_on)?;
        validate_text(&reason, "leave_reason", MAX_REASON_BYTES)?;
        validate_text(&requested_by, "requested_by", MAX_REFERENCE_BYTES)?;
        let value = Self {
            id,
            version: 1,
            scope,
            employee_id,
            starts_on,
            ends_on,
            reason,
            status: LeaveStatus::Draft,
            requested_by,
            approved_by: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::LeaveStatusChanged {
                leave_request_id: id,
                status: LeaveStatus::Draft,
                at,
            }],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn transition(
        &mut self,
        transition: LeaveTransition,
        actor: &str,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        if matches!(
            transition,
            LeaveTransition::Approve | LeaveTransition::Reject
        ) {
            return Err(PeoplePayrollDomainError::InvalidHumanDecision);
        }
        validate_text(actor, "actor", MAX_REFERENCE_BYTES)?;
        let (next, approver) = match transition {
            LeaveTransition::Submit if self.status == LeaveStatus::Draft => {
                (LeaveStatus::Submitted, None)
            }
            LeaveTransition::Cancel
                if matches!(self.status, LeaveStatus::Submitted | LeaveStatus::Approved) =>
            {
                (LeaveStatus::Cancelled, None)
            }
            LeaveTransition::MarkTaken if self.status == LeaveStatus::Approved => {
                (LeaveStatus::Taken, self.approved_by.clone())
            }
            _ => return Err(PeoplePayrollDomainError::InvalidTransition),
        };
        self.advance(at)?;
        self.status = next;
        if approver.is_some() {
            self.approved_by = approver;
        }
        self.uncommitted_events
            .push(DomainEvent::LeaveStatusChanged {
                leave_request_id: self.id,
                status: self.status,
                at,
            });
        Ok(())
    }

    /// Applies an approve or reject decision from authenticated human
    /// evidence. The lifecycle actor is derived from the proof rather than
    /// accepted independently from a command payload.
    pub fn decide(
        &mut self,
        transition: LeaveTransition,
        decision: HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        let actor = human_decision_actor(&decision, command_id, at, &self.scope)?;
        let (status, approver) = match transition {
            LeaveTransition::Approve
                if self.status == LeaveStatus::Submitted && actor != self.requested_by =>
            {
                (LeaveStatus::Approved, Some(actor.to_owned()))
            }
            LeaveTransition::Reject if self.status == LeaveStatus::Submitted => {
                (LeaveStatus::Rejected, None)
            }
            LeaveTransition::Approve => {
                return Err(PeoplePayrollDomainError::ApprovalSeparation);
            }
            LeaveTransition::Reject => return Err(PeoplePayrollDomainError::InvalidTransition),
            _ => return Err(PeoplePayrollDomainError::InvalidHumanDecision),
        };

        let mut next = self.clone();
        next.advance(at)?;
        next.status = status;
        if approver.is_some() {
            next.approved_by = approver;
        }
        next.uncommitted_events
            .push(DomainEvent::LeaveStatusChanged {
                leave_request_id: next.id,
                status: next.status,
                at,
            });
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), PeoplePayrollDomainError> {
        validate_scope(&self.scope)?;
        validate_window(self.starts_on, self.ends_on)?;
        validate_text(&self.reason, "leave_reason", MAX_REASON_BYTES)?;
        validate_text(&self.requested_by, "requested_by", MAX_REFERENCE_BYTES)?;
        validate_common(self.version, self.created_at, self.updated_at)?;
        if let Some(approver) = &self.approved_by {
            validate_text(approver, "approved_by", MAX_REFERENCE_BYTES)?;
            if approver == &self.requested_by {
                return Err(PeoplePayrollDomainError::ApprovalSeparation);
            }
        }
        if matches!(self.status, LeaveStatus::Approved | LeaveStatus::Taken)
            && self.approved_by.is_none()
        {
            return Err(PeoplePayrollDomainError::InvalidTransition);
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PeoplePayrollDomainError> {
        advance(&mut self.version, &mut self.updated_at, at)
    }
    pub const fn id(&self) -> LeaveRequestId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn employee_id(&self) -> EmployeeId {
        self.employee_id
    }
    pub const fn starts_on(&self) -> NaiveDate {
        self.starts_on
    }
    pub const fn ends_on(&self) -> NaiveDate {
        self.ends_on
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub const fn status(&self) -> LeaveStatus {
        self.status
    }
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }
    pub fn approved_by(&self) -> Option<&str> {
        self.approved_by.as_deref()
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        stable_record_ref(self.id.as_uuid(), ContextId::PeoplePayroll, "people.leave")
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseClaim {
    id: ExpenseClaimId,
    version: u64,
    scope: AuthorityScope,
    employee_id: EmployeeId,
    currency: Currency,
    amount_minor: i64,
    reason: String,
    status: ExpenseStatus,
    claimed_by: String,
    approved_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpenseClaimState {
    pub id: ExpenseClaimId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub employee_id: EmployeeId,
    pub currency: Currency,
    pub amount_minor: i64,
    pub reason: String,
    pub status: ExpenseStatus,
    pub claimed_by: String,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ExpenseClaim {
    pub fn try_rehydrate(state: ExpenseClaimState) -> Result<Self, PeoplePayrollDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            employee_id: state.employee_id,
            currency: state.currency,
            amount_minor: state.amount_minor,
            reason: state.reason,
            status: state.status,
            claimed_by: state.claimed_by,
            approved_by: state.approved_by,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: ExpenseClaimId,
        scope: AuthorityScope,
        employee_id: EmployeeId,
        currency: Currency,
        amount_minor: i64,
        reason: String,
        claimed_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PeoplePayrollDomainError> {
        validate_scope(&scope)?;
        if amount_minor < 0 {
            return Err(PeoplePayrollDomainError::InvalidAmount);
        }
        validate_text(&reason, "expense_reason", MAX_REASON_BYTES)?;
        validate_text(&claimed_by, "claimed_by", MAX_REFERENCE_BYTES)?;
        let value = Self {
            id,
            version: 1,
            scope,
            employee_id,
            currency,
            amount_minor,
            reason,
            status: ExpenseStatus::Draft,
            claimed_by,
            approved_by: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::ExpenseStatusChanged {
                expense_claim_id: id,
                status: ExpenseStatus::Draft,
                at,
            }],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn transition(
        &mut self,
        transition: ExpenseTransition,
        actor: &str,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        if matches!(
            transition,
            ExpenseTransition::Approve | ExpenseTransition::Reject | ExpenseTransition::Reimburse
        ) {
            return Err(PeoplePayrollDomainError::InvalidHumanDecision);
        }
        validate_text(actor, "actor", MAX_REFERENCE_BYTES)?;
        let (next, approver) = match transition {
            ExpenseTransition::Submit if self.status == ExpenseStatus::Draft => {
                (ExpenseStatus::Submitted, None)
            }
            _ => return Err(PeoplePayrollDomainError::InvalidTransition),
        };
        self.advance(at)?;
        self.status = next;
        if approver.is_some() {
            self.approved_by = approver;
        }
        self.uncommitted_events
            .push(DomainEvent::ExpenseStatusChanged {
                expense_claim_id: self.id,
                status: self.status,
                at,
            });
        Ok(())
    }

    /// Applies an approve, reject, or reimburse decision from authenticated
    /// human evidence and derives the lifecycle actor from that evidence.
    pub fn decide(
        &mut self,
        transition: ExpenseTransition,
        decision: HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        let actor = human_decision_actor(&decision, command_id, at, &self.scope)?;
        let (status, approver) = match transition {
            ExpenseTransition::Approve
                if self.status == ExpenseStatus::Submitted && actor != self.claimed_by =>
            {
                (ExpenseStatus::Approved, Some(actor.to_owned()))
            }
            ExpenseTransition::Reject if self.status == ExpenseStatus::Submitted => {
                (ExpenseStatus::Rejected, None)
            }
            ExpenseTransition::Reimburse if self.status == ExpenseStatus::Approved => {
                (ExpenseStatus::Reimbursed, self.approved_by.clone())
            }
            ExpenseTransition::Approve => {
                return Err(PeoplePayrollDomainError::ApprovalSeparation);
            }
            ExpenseTransition::Reject | ExpenseTransition::Reimburse => {
                return Err(PeoplePayrollDomainError::InvalidTransition);
            }
            _ => return Err(PeoplePayrollDomainError::InvalidHumanDecision),
        };

        let mut next = self.clone();
        next.advance(at)?;
        next.status = status;
        if approver.is_some() {
            next.approved_by = approver;
        }
        next.uncommitted_events
            .push(DomainEvent::ExpenseStatusChanged {
                expense_claim_id: next.id,
                status: next.status,
                at,
            });
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), PeoplePayrollDomainError> {
        validate_scope(&self.scope)?;
        if self.amount_minor < 0 {
            return Err(PeoplePayrollDomainError::InvalidAmount);
        }
        validate_text(&self.reason, "expense_reason", MAX_REASON_BYTES)?;
        validate_text(&self.claimed_by, "claimed_by", MAX_REFERENCE_BYTES)?;
        validate_common(self.version, self.created_at, self.updated_at)?;
        if let Some(approver) = &self.approved_by {
            validate_text(approver, "approved_by", MAX_REFERENCE_BYTES)?;
            if approver == &self.claimed_by {
                return Err(PeoplePayrollDomainError::ApprovalSeparation);
            }
        }
        if matches!(
            self.status,
            ExpenseStatus::Approved | ExpenseStatus::Reimbursed
        ) && self.approved_by.is_none()
        {
            return Err(PeoplePayrollDomainError::InvalidTransition);
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PeoplePayrollDomainError> {
        advance(&mut self.version, &mut self.updated_at, at)
    }
    pub const fn id(&self) -> ExpenseClaimId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn employee_id(&self) -> EmployeeId {
        self.employee_id
    }
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub const fn status(&self) -> ExpenseStatus {
        self.status
    }
    pub fn claimed_by(&self) -> &str {
        &self.claimed_by
    }
    pub fn approved_by(&self) -> Option<&str> {
        self.approved_by.as_deref()
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollLine {
    pub employee_id: EmployeeId,
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub net_minor: i64,
}

/// Version-pinned fact supplied by a domain that contributes to payroll.
///
/// The stable record identity deliberately excludes a route. `source_version`
/// and `content_sha256` preserve the exact revision that was calculated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollInputEvidence {
    pub source: StableRecordRef,
    pub source_version: u64,
    pub content_sha256: String,
}

/// Immutable calculation authority retained with a governed payroll run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollCalculationEvidence {
    pub policy_version: String,
    pub rate_table_version: String,
    pub calculation_sha256: String,
    pub source_inputs: Vec<PayrollInputEvidence>,
    pub captured_at: DateTime<Utc>,
}

impl PayrollCalculationEvidence {
    pub fn validate(&self, not_after: DateTime<Utc>) -> Result<(), PeoplePayrollDomainError> {
        validate_text(&self.policy_version, "policy_version", MAX_REFERENCE_BYTES)?;
        validate_text(
            &self.rate_table_version,
            "rate_table_version",
            MAX_REFERENCE_BYTES,
        )?;
        validate_digest(&self.calculation_sha256)?;
        if self.captured_at > not_after || self.source_inputs.len() > MAX_PAYROLL_INPUTS {
            return Err(PeoplePayrollDomainError::InvalidPayrollEvidence);
        }
        let mut sources = HashSet::with_capacity(self.source_inputs.len());
        for input in &self.source_inputs {
            input
                .source
                .validate()
                .map_err(|_| PeoplePayrollDomainError::InvalidPayrollEvidence)?;
            if input.source_version == 0
                || !sources.insert((input.source.clone(), input.source_version))
            {
                return Err(PeoplePayrollDomainError::InvalidPayrollEvidence);
            }
            validate_digest(&input.content_sha256)?;
        }
        Ok(())
    }
}

impl PayrollLine {
    pub fn try_new(
        employee_id: EmployeeId,
        gross_minor: i64,
        deductions_minor: i64,
    ) -> Result<Self, PeoplePayrollDomainError> {
        if gross_minor < 0 || deductions_minor < 0 || deductions_minor > gross_minor {
            return Err(PeoplePayrollDomainError::InvalidAmount);
        }
        Ok(Self {
            employee_id,
            gross_minor,
            deductions_minor,
            net_minor: gross_minor - deductions_minor,
        })
    }

    pub fn validate(&self) -> Result<(), PeoplePayrollDomainError> {
        if self.gross_minor < 0
            || self.deductions_minor < 0
            || self.deductions_minor > self.gross_minor
            || self.net_minor != self.gross_minor - self.deductions_minor
        {
            return Err(PeoplePayrollDomainError::InvalidAmount);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollRun {
    id: PayrollRunId,
    version: u64,
    scope: AuthorityScope,
    period_code: String,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    currency: Currency,
    lines: Vec<PayrollLine>,
    #[serde(default)]
    calculation_evidence: Option<PayrollCalculationEvidence>,
    status: PayrollStatus,
    prepared_by: String,
    reviewed_by: Option<String>,
    approved_by: Option<String>,
    total_gross_minor: i64,
    total_deductions_minor: i64,
    total_net_minor: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayrollRunState {
    pub id: PayrollRunId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub period_code: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub currency: Currency,
    pub lines: Vec<PayrollLine>,
    pub calculation_evidence: Option<PayrollCalculationEvidence>,
    pub status: PayrollStatus,
    pub prepared_by: String,
    pub reviewed_by: Option<String>,
    pub approved_by: Option<String>,
    pub total_gross_minor: i64,
    pub total_deductions_minor: i64,
    pub total_net_minor: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PayrollRun {
    pub fn try_rehydrate(state: PayrollRunState) -> Result<Self, PeoplePayrollDomainError> {
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            period_code: state.period_code,
            starts_on: state.starts_on,
            ends_on: state.ends_on,
            currency: state.currency,
            lines: state.lines,
            calculation_evidence: state.calculation_evidence,
            status: state.status,
            prepared_by: state.prepared_by,
            reviewed_by: state.reviewed_by,
            approved_by: state.approved_by,
            total_gross_minor: state.total_gross_minor,
            total_deductions_minor: state.total_deductions_minor,
            total_net_minor: state.total_net_minor,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: PayrollRunId,
        scope: AuthorityScope,
        period_code: String,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        currency: Currency,
        lines: Vec<PayrollLine>,
        prepared_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PeoplePayrollDomainError> {
        validate_scope(&scope)?;
        validate_text(&period_code, "period_code", MAX_REFERENCE_BYTES)?;
        validate_window(starts_on, ends_on)?;
        validate_text(&prepared_by, "prepared_by", MAX_REFERENCE_BYTES)?;
        validate_payroll_lines(&lines)?;
        let (gross, deductions, net) = payroll_totals(&lines)?;
        let value = Self {
            id,
            version: 1,
            scope,
            period_code,
            starts_on,
            ends_on,
            currency,
            lines,
            calculation_evidence: None,
            status: PayrollStatus::Draft,
            prepared_by,
            reviewed_by: None,
            approved_by: None,
            total_gross_minor: gross,
            total_deductions_minor: deductions,
            total_net_minor: net,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::PayrollStatusChanged {
                payroll_run_id: id,
                status: PayrollStatus::Draft,
                at,
            }],
        };
        value.validate()?;
        Ok(value)
    }

    /// Pins policy, rate-table, calculation, and typed inbound facts before
    /// review. The evidence cannot be replaced, including by an idempotent
    /// retry with different content.
    pub fn pin_calculation_evidence(
        &mut self,
        evidence: PayrollCalculationEvidence,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        if self.status != PayrollStatus::Draft || self.calculation_evidence.is_some() {
            return Err(PeoplePayrollDomainError::InvalidPayrollEvidence);
        }
        evidence.validate(at)?;
        let mut next = self.clone();
        next.advance(at)?;
        next.calculation_evidence = Some(evidence);
        next.uncommitted_events
            .push(DomainEvent::PayrollEvidencePinned);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn transition(
        &mut self,
        transition: PayrollTransition,
        actor: &str,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        if matches!(
            transition,
            PayrollTransition::Review | PayrollTransition::Post
        ) {
            return Err(PeoplePayrollDomainError::InvalidHumanDecision);
        }
        validate_text(actor, "actor", MAX_REFERENCE_BYTES)?;
        let (next, reviewer, approver) = match transition {
            PayrollTransition::SubmitForReview
                if self.status == PayrollStatus::Draft && self.calculation_evidence.is_some() =>
            {
                (PayrollStatus::Review, None, None)
            }
            PayrollTransition::Cancel
                if matches!(self.status, PayrollStatus::Draft | PayrollStatus::Review) =>
            {
                (PayrollStatus::Cancelled, self.reviewed_by.clone(), None)
            }
            _ => return Err(PeoplePayrollDomainError::InvalidTransition),
        };
        self.advance(at)?;
        self.status = next;
        if reviewer.is_some() {
            self.reviewed_by = reviewer;
        }
        if approver.is_some() {
            self.approved_by = approver;
        }
        self.uncommitted_events
            .push(DomainEvent::PayrollStatusChanged {
                payroll_run_id: self.id,
                status: self.status,
                at,
            });
        if self.status == PayrollStatus::Posted {
            self.uncommitted_events.push(DomainEvent::PayrollFinalized {
                payroll_run_id: self.id,
                period_code: self.period_code.clone(),
                currency: self.currency,
                total_gross_minor: self.total_gross_minor,
                total_deductions_minor: self.total_deductions_minor,
                total_net_minor: self.total_net_minor,
                at,
            });
        }
        Ok(())
    }

    /// Applies review or posting from authenticated human evidence. Review
    /// and posting actors are derived from the proof so preparation, review,
    /// and posting separation cannot be forged through an actor field.
    pub fn decide(
        &mut self,
        transition: PayrollTransition,
        decision: HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), PeoplePayrollDomainError> {
        let actor = human_decision_actor(&decision, command_id, at, &self.scope)?;
        let (status, reviewer, approver) = match transition {
            PayrollTransition::Review
                if self.status == PayrollStatus::Review && actor != self.prepared_by =>
            {
                (PayrollStatus::Approved, Some(actor.to_owned()), None)
            }
            PayrollTransition::Post
                if self.status == PayrollStatus::Approved
                    && actor != self.prepared_by
                    && self.reviewed_by.as_deref() != Some(actor) =>
            {
                (
                    PayrollStatus::Posted,
                    self.reviewed_by.clone(),
                    Some(actor.to_owned()),
                )
            }
            PayrollTransition::Review | PayrollTransition::Post => {
                return Err(PeoplePayrollDomainError::ApprovalSeparation);
            }
            _ => return Err(PeoplePayrollDomainError::InvalidHumanDecision),
        };

        let mut next = self.clone();
        next.advance(at)?;
        next.status = status;
        if reviewer.is_some() {
            next.reviewed_by = reviewer;
        }
        if approver.is_some() {
            next.approved_by = approver;
        }
        next.uncommitted_events
            .push(DomainEvent::PayrollStatusChanged {
                payroll_run_id: next.id,
                status: next.status,
                at,
            });
        if next.status == PayrollStatus::Posted {
            next.uncommitted_events.push(DomainEvent::PayrollFinalized {
                payroll_run_id: next.id,
                period_code: next.period_code.clone(),
                currency: next.currency,
                total_gross_minor: next.total_gross_minor,
                total_deductions_minor: next.total_deductions_minor,
                total_net_minor: next.total_net_minor,
                at,
            });
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), PeoplePayrollDomainError> {
        validate_scope(&self.scope)?;
        validate_text(&self.period_code, "period_code", MAX_REFERENCE_BYTES)?;
        validate_window(self.starts_on, self.ends_on)?;
        validate_text(&self.prepared_by, "prepared_by", MAX_REFERENCE_BYTES)?;
        validate_payroll_lines(&self.lines)?;
        if let Some(evidence) = &self.calculation_evidence {
            evidence.validate(self.updated_at)?;
        }
        if matches!(
            self.status,
            PayrollStatus::Review | PayrollStatus::Approved | PayrollStatus::Posted
        ) && self.calculation_evidence.is_none()
        {
            return Err(PeoplePayrollDomainError::InvalidPayrollEvidence);
        }
        validate_common(self.version, self.created_at, self.updated_at)?;
        if self.reviewed_by.as_deref() == Some(self.prepared_by.as_str())
            || self.approved_by.as_deref() == Some(self.prepared_by.as_str())
            || (self.reviewed_by.is_some() && self.reviewed_by == self.approved_by)
        {
            return Err(PeoplePayrollDomainError::ApprovalSeparation);
        }
        if matches!(self.status, PayrollStatus::Approved | PayrollStatus::Posted)
            && self.reviewed_by.is_none()
        {
            return Err(PeoplePayrollDomainError::InvalidTransition);
        }
        if self.status == PayrollStatus::Posted && self.approved_by.is_none() {
            return Err(PeoplePayrollDomainError::InvalidTransition);
        }
        if payroll_totals(&self.lines)?
            != (
                self.total_gross_minor,
                self.total_deductions_minor,
                self.total_net_minor,
            )
        {
            return Err(PeoplePayrollDomainError::InvalidAmount);
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PeoplePayrollDomainError> {
        advance(&mut self.version, &mut self.updated_at, at)
    }
    pub const fn id(&self) -> PayrollRunId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn period_code(&self) -> &str {
        &self.period_code
    }
    pub const fn starts_on(&self) -> NaiveDate {
        self.starts_on
    }
    pub const fn ends_on(&self) -> NaiveDate {
        self.ends_on
    }
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    pub fn lines(&self) -> &[PayrollLine] {
        &self.lines
    }
    pub fn calculation_evidence(&self) -> Option<&PayrollCalculationEvidence> {
        self.calculation_evidence.as_ref()
    }
    pub const fn status(&self) -> PayrollStatus {
        self.status
    }
    pub fn prepared_by(&self) -> &str {
        &self.prepared_by
    }
    pub fn reviewed_by(&self) -> Option<&str> {
        self.reviewed_by.as_deref()
    }
    pub fn approved_by(&self) -> Option<&str> {
        self.approved_by.as_deref()
    }
    pub const fn total_gross_minor(&self) -> i64 {
        self.total_gross_minor
    }
    pub const fn total_deductions_minor(&self) -> i64 {
        self.total_deductions_minor
    }
    pub const fn total_net_minor(&self) -> i64 {
        self.total_net_minor
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        stable_record_ref(self.id.as_uuid(), ContextId::PeoplePayroll, "payroll.run")
    }
    /// Returns the stable payslip identity for an employee actually included
    /// in this run. A caller cannot manufacture a payslip for another worker.
    pub fn payslip_record_ref(
        &self,
        employee_id: EmployeeId,
    ) -> Result<Option<StableRecordRef>, ReferenceValidationError> {
        if !self
            .lines
            .iter()
            .any(|line| line.employee_id == employee_id)
        {
            return Ok(None);
        }
        Ok(Some(payslip_record_ref_for(self.id, employee_id)?))
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum Aggregate {
    Employee(Employee),
    LeaveRequest(LeaveRequest),
    ExpenseClaim(ExpenseClaim),
    PayrollRun(PayrollRun),
}

impl Aggregate {
    pub fn validate(&self) -> Result<(), PeoplePayrollDomainError> {
        match self {
            Self::Employee(value) => value.validate(),
            Self::LeaveRequest(value) => value.validate(),
            Self::ExpenseClaim(value) => value.validate(),
            Self::PayrollRun(value) => value.validate(),
        }
    }

    pub const fn version(&self) -> u64 {
        match self {
            Self::Employee(value) => value.version(),
            Self::LeaveRequest(value) => value.version(),
            Self::ExpenseClaim(value) => value.version(),
            Self::PayrollRun(value) => value.version(),
        }
    }

    pub fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Employee(value) => value.scope(),
            Self::LeaveRequest(value) => value.scope(),
            Self::ExpenseClaim(value) => value.scope(),
            Self::PayrollRun(value) => value.scope(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Employee(_) => "employee",
            Self::LeaveRequest(_) => "leave_request",
            Self::ExpenseClaim(_) => "expense_claim",
            Self::PayrollRun(_) => "payroll_run",
        }
    }

    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Employee(value) => value.id().as_uuid(),
            Self::LeaveRequest(value) => value.id().as_uuid(),
            Self::ExpenseClaim(value) => value.id().as_uuid(),
            Self::PayrollRun(value) => value.id().as_uuid(),
        }
    }

    pub const fn updated_at(&self) -> DateTime<Utc> {
        match self {
            Self::Employee(value) => value.updated_at(),
            Self::LeaveRequest(value) => value.updated_at(),
            Self::ExpenseClaim(value) => value.updated_at(),
            Self::PayrollRun(value) => value.updated_at(),
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Employee(value) => value.take_uncommitted_events(),
            Self::LeaveRequest(value) => value.take_uncommitted_events(),
            Self::ExpenseClaim(value) => value.take_uncommitted_events(),
            Self::PayrollRun(value) => value.take_uncommitted_events(),
        }
    }
}

fn validate_scope(scope: &AuthorityScope) -> Result<(), PeoplePayrollDomainError> {
    match scope {
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
            if scope.validate().is_ok() =>
        {
            Ok(())
        }
        _ => Err(PeoplePayrollDomainError::InvalidScope),
    }
}

fn human_decision_actor<'a>(
    decision: &'a HumanDecisionProof,
    command_id: CommandId,
    at: DateTime<Utc>,
    record_scope: &AuthorityScope,
) -> Result<&'a str, PeoplePayrollDomainError> {
    let actor = decision.principal_ref();
    validate_text(actor, "decision_actor", MAX_REFERENCE_BYTES)?;
    if decision.decision_ref() != command_id.to_string()
        || decision.decided_at() != at
        || !decision.authority_scope().contains(record_scope)
    {
        return Err(PeoplePayrollDomainError::InvalidHumanDecision);
    }
    Ok(actor)
}

fn validate_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), PeoplePayrollDomainError> {
    if value.trim().is_empty() {
        return Err(PeoplePayrollDomainError::Required { field });
    }
    if value.len() > maximum {
        return Err(PeoplePayrollDomainError::TooLong { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(PeoplePayrollDomainError::ControlCharacter { field });
    }
    Ok(())
}

fn validate_window(
    starts_on: NaiveDate,
    ends_on: NaiveDate,
) -> Result<(), PeoplePayrollDomainError> {
    if ends_on < starts_on {
        Err(PeoplePayrollDomainError::InvalidWindow)
    } else {
        Ok(())
    }
}

fn validate_common(
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> Result<(), PeoplePayrollDomainError> {
    if version == 0 {
        return Err(PeoplePayrollDomainError::VersionOverflow);
    }
    if updated_at < created_at {
        return Err(PeoplePayrollDomainError::InvalidWindow);
    }
    Ok(())
}

fn advance(
    version: &mut u64,
    updated_at: &mut DateTime<Utc>,
    at: DateTime<Utc>,
) -> Result<(), PeoplePayrollDomainError> {
    if at < *updated_at {
        return Err(PeoplePayrollDomainError::InvalidWindow);
    }
    *version = version
        .checked_add(1)
        .ok_or(PeoplePayrollDomainError::VersionOverflow)?;
    *updated_at = at;
    Ok(())
}

fn validate_payroll_lines(lines: &[PayrollLine]) -> Result<(), PeoplePayrollDomainError> {
    if lines.is_empty() || lines.len() > MAX_PAYROLL_LINES {
        return Err(PeoplePayrollDomainError::InvalidPayrollLines);
    }
    let mut employees = HashSet::with_capacity(lines.len());
    for line in lines {
        line.validate()?;
        if !employees.insert(line.employee_id) {
            return Err(PeoplePayrollDomainError::InvalidPayrollLines);
        }
    }
    Ok(())
}

fn payroll_totals(lines: &[PayrollLine]) -> Result<(i64, i64, i64), PeoplePayrollDomainError> {
    let mut gross = 0_i64;
    let mut deductions = 0_i64;
    let mut net = 0_i64;
    for line in lines {
        gross = gross
            .checked_add(line.gross_minor)
            .ok_or(PeoplePayrollDomainError::InvalidAmount)?;
        deductions = deductions
            .checked_add(line.deductions_minor)
            .ok_or(PeoplePayrollDomainError::InvalidAmount)?;
        net = net
            .checked_add(line.net_minor)
            .ok_or(PeoplePayrollDomainError::InvalidAmount)?;
    }
    if gross.checked_sub(deductions) != Some(net) {
        return Err(PeoplePayrollDomainError::InvalidAmount);
    }
    Ok((gross, deductions, net))
}

fn validate_digest(value: &str) -> Result<(), PeoplePayrollDomainError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(PeoplePayrollDomainError::InvalidPayrollEvidence);
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PeoplePayrollDomainError::InvalidPayrollEvidence);
    }
    Ok(())
}

fn stable_record_ref(
    id: Uuid,
    owner: ContextId,
    record_type: &str,
) -> Result<StableRecordRef, ReferenceValidationError> {
    let record_id = RecordId::try_from_uuid(id).expect("validated People & Payroll identifier");
    StableRecordRef::try_new(record_id, owner, RecordTypeId::try_new(record_type)?)
}

pub fn employment_record_ref_for(
    employee_id: EmployeeId,
) -> Result<StableRecordRef, ReferenceValidationError> {
    derived_record_ref(
        employee_id.as_uuid(),
        b"people.employment",
        "people.employment",
    )
}

pub fn payslip_record_ref_for(
    payroll_run_id: PayrollRunId,
    employee_id: EmployeeId,
) -> Result<StableRecordRef, ReferenceValidationError> {
    derived_record_ref(
        payroll_run_id.as_uuid(),
        employee_id.as_uuid().as_bytes(),
        "payroll.payslip",
    )
}

fn derived_record_ref(
    namespace: Uuid,
    discriminator: &[u8],
    record_type: &'static str,
) -> Result<StableRecordRef, ReferenceValidationError> {
    let mut bytes = *Uuid::new_v5(&namespace, discriminator).as_bytes();
    // StableRecordRef deliberately accepts only opaque v4-shaped IDs. The
    // owner derives the bytes deterministically, then applies the v4/variant
    // bits so callers cannot infer the parent identity from the route.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    stable_record_ref(
        Uuid::from_bytes(bytes),
        ContextId::PeoplePayroll,
        record_type,
    )
}

#[cfg(test)]
mod tests;
