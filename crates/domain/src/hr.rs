//! HR, leave, expense, attendance, and payroll lifecycle contracts.
//!
//! This module is intentionally transport- and persistence-independent. It
//! owns bounded values, scope containment, closed state transitions, approval
//! separation, and version/idempotency metadata; repositories and command
//! handlers remain responsible for authoritative access evaluation.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{CommandContext, TenantScope};

pub const HR_CONTRACT_VERSION: u16 = 1;
pub const MAX_HR_REFERENCE_BYTES: usize = 128;
pub const MAX_HR_NAME_BYTES: usize = 255;
pub const MAX_HR_REASON_BYTES: usize = 1_024;
pub const MAX_PAYROLL_LINES: usize = 100_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HrError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("HR scope is invalid")]
    InvalidScope,
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("command scope does not contain the record scope")]
    ScopeNotContained,
    #[error("actor principal reference cannot be empty")]
    EmptyActor,
    #[error("{entity} cannot transition from {from} to {to}")]
    InvalidTransition {
        entity: &'static str,
        from: &'static str,
        to: &'static str,
    },
    #[error("the record is terminal")]
    Terminal,
    #[error("approval must be performed by a different principal")]
    ApprovalSeparation,
    #[error("date window is invalid")]
    InvalidDateWindow,
    #[error("amount must be non-negative")]
    NegativeAmount,
    #[error("currency must be a three-letter uppercase code")]
    InvalidCurrency,
    #[error("payroll line count exceeds the maximum")]
    TooManyPayrollLines,
    #[error("payroll must contain at least one employee line")]
    EmptyPayrollLines,
    #[error("payroll contains duplicate employee line for {0}")]
    DuplicatePayrollEmployee(Uuid),
    #[error("payroll totals do not reconcile")]
    PayrollTotalsMismatch,
    #[error("aggregate version must be positive")]
    InvalidVersion,
}

/// Tenant-scoped source/read reconciliation facts for the disposable HR and
/// payroll projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HrProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
}

impl HrProjectionReconciliation {
    pub const fn is_consistent(self) -> bool {
        self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.orphan_aggregate_count == 0
    }
}

fn bounded(value: String, field: &'static str, max: usize) -> Result<String, HrError> {
    if value.trim().is_empty() {
        return Err(HrError::EmptyField { field });
    }
    if value.len() > max {
        return Err(HrError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(HrError::ControlCharacter { field });
    }
    Ok(value)
}

fn actor(context: &CommandContext) -> Result<&str, HrError> {
    context
        .validate()
        .map_err(|error| HrError::InvalidCommandContext(error.to_string()))?;
    let principal = context.actor.principal_ref.trim();
    if principal.is_empty() {
        return Err(HrError::EmptyActor);
    }
    Ok(principal)
}

fn command_scope(context: &CommandContext, record_scope: &TenantScope) -> Result<String, HrError> {
    actor(context)?;
    if !context.tenant_scope.contains(record_scope) {
        return Err(HrError::ScopeNotContained);
    }
    Ok(context.actor.principal_ref.clone())
}

/// A holding/company/optional-branch scope for an HR record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmploymentScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

impl EmploymentScope {
    pub fn new(
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
    ) -> Result<Self, HrError> {
        let scope = Self {
            holding_id,
            company_id,
            branch_id,
        };
        scope.validate()?;
        Ok(scope)
    }

    pub fn validate(&self) -> Result<(), HrError> {
        if self.holding_id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "holding_id",
            });
        }
        if self.company_id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "company_id",
            });
        }
        if self.branch_id.is_some_and(|branch_id| branch_id.is_nil()) {
            return Err(HrError::NilIdentifier { field: "branch_id" });
        }
        Ok(())
    }

    pub fn tenant_scope(&self) -> TenantScope {
        match self.branch_id {
            Some(branch_id) => TenantScope::Branch {
                holding_id: self.holding_id,
                company_id: self.company_id,
                branch_id,
            },
            None => TenantScope::Company {
                holding_id: self.holding_id,
                company_id: self.company_id,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmploymentStatus {
    Draft,
    Active,
    OnLeave,
    Suspended,
    Terminated,
}

impl EmploymentStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::OnLeave => "on_leave",
            Self::Suspended => "suspended",
            Self::Terminated => "terminated",
        }
    }
}

/// Versioned employee record. Sensitive payroll and identity details are kept
/// outside this transport-neutral lifecycle aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmployeeRecord {
    pub id: Uuid,
    pub employee_ref: String,
    pub display_name: String,
    pub scope: EmploymentScope,
    pub status: EmploymentStatus,
    pub hired_on: NaiveDate,
    pub terminated_on: Option<NaiveDate>,
    pub version: u64,
    pub last_command_id: Uuid,
    pub last_idempotency_key: String,
}

impl EmployeeRecord {
    pub fn new(
        id: Uuid,
        employee_ref: impl Into<String>,
        display_name: impl Into<String>,
        scope: EmploymentScope,
        hired_on: NaiveDate,
        context: &CommandContext,
    ) -> Result<Self, HrError> {
        if id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "employee_id",
            });
        }
        scope.validate()?;
        command_scope(context, &scope.tenant_scope())?;
        let employee_ref = bounded(employee_ref.into(), "employee_ref", MAX_HR_REFERENCE_BYTES)?;
        let display_name = bounded(display_name.into(), "display_name", MAX_HR_NAME_BYTES)?;
        Ok(Self {
            id,
            employee_ref,
            display_name,
            scope,
            status: EmploymentStatus::Draft,
            hired_on,
            terminated_on: None,
            version: 1,
            last_command_id: context.command_id,
            last_idempotency_key: bounded(
                context.idempotency_key.clone(),
                "idempotency_key",
                MAX_HR_REFERENCE_BYTES,
            )?,
        })
    }

    /// Validate a deserialized employee before it crosses a read-model boundary.
    pub fn validate(&self) -> Result<(), HrError> {
        if self.id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "employee_id",
            });
        }
        self.scope.validate()?;
        bounded(
            self.employee_ref.clone(),
            "employee_ref",
            MAX_HR_REFERENCE_BYTES,
        )?;
        bounded(self.display_name.clone(), "display_name", MAX_HR_NAME_BYTES)?;
        bounded(
            self.last_idempotency_key.clone(),
            "idempotency_key",
            MAX_HR_REFERENCE_BYTES,
        )?;
        if self.last_command_id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "command_id",
            });
        }
        if self.status == EmploymentStatus::Terminated {
            if self.terminated_on.is_none() {
                return Err(HrError::InvalidDateWindow);
            }
        } else if self.terminated_on.is_some() {
            return Err(HrError::InvalidDateWindow);
        }
        if self.terminated_on.is_some_and(|date| date < self.hired_on) {
            return Err(HrError::InvalidDateWindow);
        }
        if self.version == 0 {
            return Err(HrError::InvalidVersion);
        }
        Ok(())
    }

    pub fn transition(
        &mut self,
        next: EmploymentStatus,
        terminated_on: Option<NaiveDate>,
        context: &CommandContext,
    ) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        // Commands operate on deserialized aggregates as well as freshly
        // constructed ones. Revalidate the full state before applying a
        // transition so a damaged read-model fact cannot be "repaired" by a
        // lifecycle command and then persisted as authoritative state.
        self.validate()?;
        let allowed = matches!(
            (self.status, next),
            (EmploymentStatus::Draft, EmploymentStatus::Active)
                | (EmploymentStatus::Active, EmploymentStatus::OnLeave)
                | (EmploymentStatus::OnLeave, EmploymentStatus::Active)
                | (EmploymentStatus::Active, EmploymentStatus::Suspended)
                | (EmploymentStatus::Suspended, EmploymentStatus::Active)
                | (EmploymentStatus::Draft, EmploymentStatus::Terminated)
                | (EmploymentStatus::Active, EmploymentStatus::Terminated)
                | (EmploymentStatus::OnLeave, EmploymentStatus::Terminated)
                | (EmploymentStatus::Suspended, EmploymentStatus::Terminated)
        );
        if !allowed {
            return Err(HrError::InvalidTransition {
                entity: "employee",
                from: self.status.as_str(),
                to: next.as_str(),
            });
        }
        if next == EmploymentStatus::Terminated && terminated_on.is_none() {
            return Err(HrError::InvalidDateWindow);
        }
        if next != EmploymentStatus::Terminated && terminated_on.is_some() {
            return Err(HrError::InvalidDateWindow);
        }
        self.status = next;
        self.terminated_on = terminated_on;
        self.commit(context)
    }

    fn commit(&mut self, context: &CommandContext) -> Result<(), HrError> {
        if context.command_id.is_nil() || context.idempotency_key.trim().is_empty() {
            return Err(HrError::InvalidCommandContext("command identity".into()));
        }
        self.version = self.version.checked_add(1).ok_or(HrError::InvalidVersion)?;
        self.last_command_id = context.command_id;
        self.last_idempotency_key = bounded(
            context.idempotency_key.clone(),
            "idempotency_key",
            MAX_HR_REFERENCE_BYTES,
        )?;
        Ok(())
    }
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

/// A leave request with a closed approval lifecycle and exact date bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaveRequest {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub scope: EmploymentScope,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: String,
    pub status: LeaveStatus,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub version: u64,
    pub last_command_id: Uuid,
}

impl LeaveRequest {
    pub fn new(
        id: Uuid,
        employee_id: Uuid,
        scope: EmploymentScope,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        reason: impl Into<String>,
        context: &CommandContext,
    ) -> Result<Self, HrError> {
        if id.is_nil() || employee_id.is_nil() {
            return Err(HrError::NilIdentifier { field: "leave_id" });
        }
        if ends_on < starts_on {
            return Err(HrError::InvalidDateWindow);
        }
        scope.validate()?;
        let requested_by = command_scope(context, &scope.tenant_scope())?;
        Ok(Self {
            id,
            employee_id,
            scope,
            starts_on,
            ends_on,
            reason: bounded(reason.into(), "leave_reason", MAX_HR_REASON_BYTES)?,
            status: LeaveStatus::Draft,
            requested_by,
            approved_by: None,
            version: 1,
            last_command_id: context.command_id,
        })
    }

    /// Validate a deserialized leave request before projection.
    pub fn validate(&self) -> Result<(), HrError> {
        if self.id.is_nil() || self.employee_id.is_nil() {
            return Err(HrError::NilIdentifier { field: "leave_id" });
        }
        if self.ends_on < self.starts_on {
            return Err(HrError::InvalidDateWindow);
        }
        self.scope.validate()?;
        bounded(self.reason.clone(), "leave_reason", MAX_HR_REASON_BYTES)?;
        bounded(
            self.requested_by.clone(),
            "requested_by",
            MAX_HR_REFERENCE_BYTES,
        )?;
        if let Some(approved_by) = &self.approved_by {
            bounded(approved_by.clone(), "approved_by", MAX_HR_REFERENCE_BYTES)?;
            if approved_by == &self.requested_by {
                return Err(HrError::ApprovalSeparation);
            }
        }
        if matches!(self.status, LeaveStatus::Approved | LeaveStatus::Taken)
            && self.approved_by.is_none()
        {
            return Err(HrError::InvalidTransition {
                entity: "leave",
                from: "submitted",
                to: leave_status(self.status),
            });
        }
        if self.last_command_id.is_nil() || self.version == 0 {
            return Err(HrError::InvalidVersion);
        }
        Ok(())
    }

    pub fn submit(&mut self, context: &CommandContext) -> Result<(), HrError> {
        self.apply(LeaveStatus::Submitted, None, context)
    }

    pub fn approve(&mut self, context: &CommandContext) -> Result<(), HrError> {
        let reviewer = command_scope(context, &self.scope.tenant_scope())?;
        if reviewer == self.requested_by {
            return Err(HrError::ApprovalSeparation);
        }
        self.apply(LeaveStatus::Approved, Some(reviewer), context)
    }

    pub fn reject(&mut self, context: &CommandContext) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        self.apply(LeaveStatus::Rejected, None, context)
    }

    pub fn cancel(&mut self, context: &CommandContext) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        self.apply(LeaveStatus::Cancelled, None, context)
    }

    pub fn take(&mut self, context: &CommandContext) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        self.apply(LeaveStatus::Taken, None, context)
    }

    fn apply(
        &mut self,
        next: LeaveStatus,
        approved_by: Option<String>,
        context: &CommandContext,
    ) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        let allowed = matches!(
            (self.status, next),
            (LeaveStatus::Draft, LeaveStatus::Submitted)
                | (LeaveStatus::Submitted, LeaveStatus::Approved)
                | (LeaveStatus::Submitted, LeaveStatus::Rejected)
                | (LeaveStatus::Submitted, LeaveStatus::Cancelled)
                | (LeaveStatus::Approved, LeaveStatus::Taken)
                | (LeaveStatus::Approved, LeaveStatus::Cancelled)
        );
        if !allowed {
            return Err(HrError::InvalidTransition {
                entity: "leave",
                from: leave_status(self.status),
                to: leave_status(next),
            });
        }
        let version = self.version.checked_add(1).ok_or(HrError::InvalidVersion)?;
        self.status = next;
        if approved_by.is_some() {
            self.approved_by = approved_by;
        }
        self.version = version;
        self.last_command_id = context.command_id;
        Ok(())
    }
}

fn leave_status(status: LeaveStatus) -> &'static str {
    match status {
        LeaveStatus::Draft => "draft",
        LeaveStatus::Submitted => "submitted",
        LeaveStatus::Approved => "approved",
        LeaveStatus::Rejected => "rejected",
        LeaveStatus::Cancelled => "cancelled",
        LeaveStatus::Taken => "taken",
    }
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

/// An expense claim with integer minor-unit accounting and separated approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpenseClaim {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub scope: EmploymentScope,
    pub currency: String,
    pub amount_minor: i64,
    pub status: ExpenseStatus,
    pub submitted_by: String,
    pub approved_by: Option<String>,
    pub version: u64,
}

impl ExpenseClaim {
    pub fn new(
        id: Uuid,
        employee_id: Uuid,
        scope: EmploymentScope,
        currency: impl Into<String>,
        amount_minor: i64,
        context: &CommandContext,
    ) -> Result<Self, HrError> {
        if id.is_nil() || employee_id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "expense_id",
            });
        }
        if amount_minor < 0 {
            return Err(HrError::NegativeAmount);
        }
        scope.validate()?;
        let submitted_by = command_scope(context, &scope.tenant_scope())?;
        let currency = canonical_currency(currency.into())?;
        Ok(Self {
            id,
            employee_id,
            scope,
            currency,
            amount_minor,
            status: ExpenseStatus::Draft,
            submitted_by,
            approved_by: None,
            version: 1,
        })
    }

    /// Validate a deserialized expense claim before projection.
    pub fn validate(&self) -> Result<(), HrError> {
        if self.id.is_nil() || self.employee_id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "expense_id",
            });
        }
        if self.amount_minor < 0 {
            return Err(HrError::NegativeAmount);
        }
        self.scope.validate()?;
        canonical_currency(self.currency.clone())?;
        bounded(
            self.submitted_by.clone(),
            "submitted_by",
            MAX_HR_REFERENCE_BYTES,
        )?;
        if let Some(approved_by) = &self.approved_by {
            bounded(approved_by.clone(), "approved_by", MAX_HR_REFERENCE_BYTES)?;
            if approved_by == &self.submitted_by {
                return Err(HrError::ApprovalSeparation);
            }
        }
        let approval_required = matches!(
            self.status,
            ExpenseStatus::Approved | ExpenseStatus::Reimbursed
        );
        if approval_required != self.approved_by.is_some() {
            return Err(HrError::InvalidTransition {
                entity: "expense",
                from: "submitted",
                to: expense_status(self.status),
            });
        }
        if self.version == 0 {
            return Err(HrError::InvalidVersion);
        }
        Ok(())
    }

    pub fn submit(&mut self, context: &CommandContext) -> Result<(), HrError> {
        self.transition(ExpenseStatus::Submitted, None, context)
    }

    pub fn reject(&mut self, context: &CommandContext) -> Result<(), HrError> {
        self.transition(ExpenseStatus::Rejected, None, context)
    }

    pub fn approve(&mut self, context: &CommandContext) -> Result<(), HrError> {
        let approver = command_scope(context, &self.scope.tenant_scope())?;
        if approver == self.submitted_by {
            return Err(HrError::ApprovalSeparation);
        }
        self.transition(ExpenseStatus::Approved, Some(approver), context)
    }

    pub fn reimburse(&mut self, context: &CommandContext) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        self.transition(ExpenseStatus::Reimbursed, None, context)
    }

    fn transition(
        &mut self,
        next: ExpenseStatus,
        approver: Option<String>,
        context: &CommandContext,
    ) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        self.validate()?;
        let allowed = matches!(
            (self.status, next),
            (ExpenseStatus::Draft, ExpenseStatus::Submitted)
                | (ExpenseStatus::Submitted, ExpenseStatus::Approved)
                | (ExpenseStatus::Submitted, ExpenseStatus::Rejected)
                | (ExpenseStatus::Approved, ExpenseStatus::Reimbursed)
        );
        if !allowed {
            return Err(HrError::InvalidTransition {
                entity: "expense",
                from: expense_status(self.status),
                to: expense_status(next),
            });
        }
        let version = self.version.checked_add(1).ok_or(HrError::InvalidVersion)?;
        self.status = next;
        self.approved_by = approver;
        self.version = version;
        Ok(())
    }
}

fn canonical_currency(value: String) -> Result<String, HrError> {
    if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(HrError::InvalidCurrency);
    }
    Ok(value)
}

fn expense_status(status: ExpenseStatus) -> &'static str {
    match status {
        ExpenseStatus::Draft => "draft",
        ExpenseStatus::Submitted => "submitted",
        ExpenseStatus::Approved => "approved",
        ExpenseStatus::Rejected => "rejected",
        ExpenseStatus::Reimbursed => "reimbursed",
    }
}

/// One bounded payroll line in a payroll run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollLine {
    pub employee_id: Uuid,
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub net_minor: i64,
}

impl PayrollLine {
    pub fn new(
        employee_id: Uuid,
        gross_minor: i64,
        deductions_minor: i64,
    ) -> Result<Self, HrError> {
        if employee_id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "employee_id",
            });
        }
        if gross_minor < 0 || deductions_minor < 0 || deductions_minor > gross_minor {
            return Err(HrError::NegativeAmount);
        }
        Ok(Self {
            employee_id,
            gross_minor,
            deductions_minor,
            net_minor: gross_minor - deductions_minor,
        })
    }

    /// Validate a deserialized line as well as lines created through `new`.
    /// Transport boundaries may deserialize the struct directly, so callers
    /// must not rely on the constructor having run.
    pub fn validate(&self) -> Result<(), HrError> {
        if self.employee_id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "employee_id",
            });
        }
        if self.gross_minor < 0
            || self.deductions_minor < 0
            || self.deductions_minor > self.gross_minor
            || self.net_minor != self.gross_minor - self.deductions_minor
        {
            return Err(HrError::NegativeAmount);
        }
        Ok(())
    }
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

/// A payroll run. Posting is terminal and requires a distinct reviewer and
/// approver, while the run remains scoped to one company/optional branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayrollRun {
    pub id: Uuid,
    pub scope: EmploymentScope,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub lines: Vec<PayrollLine>,
    pub status: PayrollStatus,
    pub prepared_by: String,
    pub reviewed_by: Option<String>,
    pub approved_by: Option<String>,
    pub total_gross_minor: i64,
    pub total_deductions_minor: i64,
    pub total_net_minor: i64,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

impl PayrollRun {
    pub fn new(
        id: Uuid,
        scope: EmploymentScope,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        lines: Vec<PayrollLine>,
        context: &CommandContext,
    ) -> Result<Self, HrError> {
        if id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "payroll_id",
            });
        }
        if ends_on < starts_on {
            return Err(HrError::InvalidDateWindow);
        }
        if lines.len() > MAX_PAYROLL_LINES {
            return Err(HrError::TooManyPayrollLines);
        }
        scope.validate()?;
        let prepared_by = command_scope(context, &scope.tenant_scope())?;
        validate_payroll_lines(&lines)?;
        let (gross, deductions, net) = totals(&lines)?;
        let run = Self {
            id,
            scope,
            starts_on,
            ends_on,
            lines,
            status: PayrollStatus::Draft,
            prepared_by,
            reviewed_by: None,
            approved_by: None,
            total_gross_minor: gross,
            total_deductions_minor: deductions,
            total_net_minor: net,
            version: 1,
            created_at: Utc::now(),
        };
        run.validate()?;
        Ok(run)
    }

    /// Revalidate a payroll aggregate after transport deserialization or
    /// before a lifecycle command. Constructors are not the only way an
    /// aggregate enters the application boundary.
    pub fn validate(&self) -> Result<(), HrError> {
        if self.id.is_nil() {
            return Err(HrError::NilIdentifier {
                field: "payroll_id",
            });
        }
        if self.ends_on < self.starts_on {
            return Err(HrError::InvalidDateWindow);
        }
        if self.lines.len() > MAX_PAYROLL_LINES {
            return Err(HrError::TooManyPayrollLines);
        }
        self.scope.validate()?;
        validate_payroll_lines(&self.lines)?;
        if self.prepared_by.trim().is_empty() {
            return Err(HrError::EmptyField {
                field: "prepared_by",
            });
        }
        if self.reviewed_by.as_deref() == Some(self.prepared_by.as_str())
            || self.approved_by.as_deref() == Some(self.prepared_by.as_str())
            || (self.reviewed_by.is_some() && self.reviewed_by == self.approved_by)
        {
            return Err(HrError::ApprovalSeparation);
        }
        if self.status == PayrollStatus::Approved && self.reviewed_by.is_none() {
            return Err(HrError::InvalidTransition {
                entity: "payroll",
                from: "review",
                to: "approved",
            });
        }
        if self.status == PayrollStatus::Posted
            && (self.reviewed_by.is_none() || self.approved_by.is_none())
        {
            return Err(HrError::InvalidTransition {
                entity: "payroll",
                from: "approved",
                to: "posted",
            });
        }
        let totals = totals(&self.lines)?;
        if totals
            != (
                self.total_gross_minor,
                self.total_deductions_minor,
                self.total_net_minor,
            )
        {
            return Err(HrError::PayrollTotalsMismatch);
        }
        if self.version == 0 {
            return Err(HrError::InvalidVersion);
        }
        Ok(())
    }

    pub fn submit_for_review(&mut self, context: &CommandContext) -> Result<(), HrError> {
        command_scope(context, &self.scope.tenant_scope())?;
        self.transition(PayrollStatus::Review, context)
    }

    pub fn review(&mut self, context: &CommandContext) -> Result<(), HrError> {
        let reviewer = command_scope(context, &self.scope.tenant_scope())?;
        if reviewer == self.prepared_by {
            return Err(HrError::ApprovalSeparation);
        }
        self.transition(PayrollStatus::Approved, context)?;
        self.reviewed_by = Some(reviewer);
        Ok(())
    }

    pub fn post(&mut self, context: &CommandContext) -> Result<(), HrError> {
        let approver = command_scope(context, &self.scope.tenant_scope())?;
        if self.reviewed_by.as_deref() == Some(approver.as_str()) || self.prepared_by == approver {
            return Err(HrError::ApprovalSeparation);
        }
        self.transition(PayrollStatus::Posted, context)?;
        self.approved_by = Some(approver);
        Ok(())
    }

    fn transition(&mut self, next: PayrollStatus, context: &CommandContext) -> Result<(), HrError> {
        self.validate()?;
        let allowed = matches!(
            (self.status, next),
            (PayrollStatus::Draft, PayrollStatus::Review)
                | (PayrollStatus::Review, PayrollStatus::Approved)
                | (PayrollStatus::Approved, PayrollStatus::Posted)
                | (PayrollStatus::Draft, PayrollStatus::Cancelled)
                | (PayrollStatus::Review, PayrollStatus::Cancelled)
        );
        if !allowed {
            return Err(HrError::InvalidTransition {
                entity: "payroll",
                from: payroll_status(self.status),
                to: payroll_status(next),
            });
        }
        let version = self.version.checked_add(1).ok_or(HrError::InvalidVersion)?;
        self.status = next;
        self.version = version;
        self.created_at = self.created_at.max(Utc::now());
        let _ = context;
        Ok(())
    }
}

fn payroll_status(status: PayrollStatus) -> &'static str {
    match status {
        PayrollStatus::Draft => "draft",
        PayrollStatus::Review => "review",
        PayrollStatus::Approved => "approved",
        PayrollStatus::Posted => "posted",
        PayrollStatus::Cancelled => "cancelled",
    }
}

fn totals(lines: &[PayrollLine]) -> Result<(i64, i64, i64), HrError> {
    let mut gross = 0_i64;
    let mut deductions = 0_i64;
    let mut net = 0_i64;
    for line in lines {
        gross = gross
            .checked_add(line.gross_minor)
            .ok_or(HrError::PayrollTotalsMismatch)?;
        deductions = deductions
            .checked_add(line.deductions_minor)
            .ok_or(HrError::PayrollTotalsMismatch)?;
        net = net
            .checked_add(line.net_minor)
            .ok_or(HrError::PayrollTotalsMismatch)?;
    }
    if gross.checked_sub(deductions) != Some(net) {
        return Err(HrError::PayrollTotalsMismatch);
    }
    Ok((gross, deductions, net))
}

fn validate_payroll_lines(lines: &[PayrollLine]) -> Result<(), HrError> {
    if lines.is_empty() {
        return Err(HrError::EmptyPayrollLines);
    }
    let mut employee_ids = HashSet::with_capacity(lines.len());
    for line in lines {
        line.validate()?;
        if !employee_ids.insert(line.employee_id) {
            return Err(HrError::DuplicatePayrollEmployee(line.employee_id));
        }
    }
    Ok(())
}

/// Explicit reassignment record used when an expired role would otherwise
/// leave an HR workflow without an active owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpiredRoleReassignment {
    pub workflow_ref: String,
    pub expired_principal_ref: String,
    pub replacement_principal_ref: String,
    pub scope: EmploymentScope,
    pub reason: String,
    pub reassigned_at: DateTime<Utc>,
    pub command_id: Uuid,
}

impl ExpiredRoleReassignment {
    pub fn new(
        workflow_ref: impl Into<String>,
        expired_principal_ref: impl Into<String>,
        replacement_principal_ref: impl Into<String>,
        scope: EmploymentScope,
        reason: impl Into<String>,
        context: &CommandContext,
    ) -> Result<Self, HrError> {
        scope.validate()?;
        let actor = command_scope(context, &scope.tenant_scope())?;
        let expired_principal_ref = bounded(
            expired_principal_ref.into(),
            "expired_principal_ref",
            MAX_HR_REFERENCE_BYTES,
        )?;
        let replacement_principal_ref = bounded(
            replacement_principal_ref.into(),
            "replacement_principal_ref",
            MAX_HR_REFERENCE_BYTES,
        )?;
        if expired_principal_ref == replacement_principal_ref
            || actor == expired_principal_ref
            || replacement_principal_ref == actor
        {
            return Err(HrError::ApprovalSeparation);
        }
        Ok(Self {
            workflow_ref: bounded(workflow_ref.into(), "workflow_ref", MAX_HR_REFERENCE_BYTES)?,
            expired_principal_ref,
            replacement_principal_ref,
            scope,
            reason: bounded(reason.into(), "reason", MAX_HR_REASON_BYTES)?,
            reassigned_at: Utc::now(),
            command_id: context.command_id,
        })
    }

    /// Validate a deserialized reassignment before projection.
    pub fn validate(&self) -> Result<(), HrError> {
        self.scope.validate()?;
        let expired = bounded(
            self.expired_principal_ref.clone(),
            "expired_principal_ref",
            MAX_HR_REFERENCE_BYTES,
        )?;
        let replacement = bounded(
            self.replacement_principal_ref.clone(),
            "replacement_principal_ref",
            MAX_HR_REFERENCE_BYTES,
        )?;
        bounded(
            self.workflow_ref.clone(),
            "workflow_ref",
            MAX_HR_REFERENCE_BYTES,
        )?;
        bounded(self.reason.clone(), "reason", MAX_HR_REASON_BYTES)?;
        if expired == replacement || self.command_id.is_nil() {
            return Err(HrError::ApprovalSeparation);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};

    fn context(actor: &str, scope: TenantScope, key: &str) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: actor.into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "hr_test",
        )
        .expect("valid context")
    }

    fn scope() -> EmploymentScope {
        EmploymentScope::new(Uuid::new_v4(), Uuid::new_v4(), Some(Uuid::new_v4()))
            .expect("valid scope")
    }

    #[test]
    fn employee_and_leave_lifecycles_are_closed_and_scoped() {
        let employment_scope = scope();
        let tenant_scope = employment_scope.tenant_scope();
        let owner = context("owner", tenant_scope.clone(), "employee-open");
        let reviewer = context("reviewer", tenant_scope, "leave-review");
        let mut employee = EmployeeRecord::new(
            Uuid::new_v4(),
            "emp-001",
            "Ari",
            employment_scope.clone(),
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            &owner,
        )
        .unwrap();
        employee
            .transition(EmploymentStatus::Active, None, &owner)
            .unwrap();
        let mut leave = LeaveRequest::new(
            Uuid::new_v4(),
            employee.id,
            employment_scope,
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 2, 3).unwrap(),
            "annual leave",
            &owner,
        )
        .unwrap();
        leave.submit(&owner).unwrap();
        assert_eq!(leave.approve(&owner), Err(HrError::ApprovalSeparation));
        leave.approve(&reviewer).unwrap();
        leave.take(&reviewer).unwrap();
        assert!(matches!(
            leave.submit(&reviewer),
            Err(HrError::InvalidTransition { .. })
        ));
        assert!(employee.validate().is_ok());
        assert!(leave.validate().is_ok());
    }

    #[test]
    fn deserialized_hr_aggregates_reject_invalid_read_model_facts() {
        let employment_scope = scope();
        let owner = context("owner", employment_scope.tenant_scope(), "hr-aggregate");
        let mut employee = EmployeeRecord::new(
            Uuid::new_v4(),
            "emp-001",
            "Ari",
            employment_scope.clone(),
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            &owner,
        )
        .unwrap();
        employee.last_idempotency_key.clear();
        assert!(employee.validate().is_err());

        let mut malformed_employee = EmployeeRecord::new(
            Uuid::new_v4(),
            "emp-002",
            "Bea",
            employment_scope.clone(),
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            &owner,
        )
        .unwrap();
        malformed_employee.terminated_on = Some(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
        let employee_before_transition = malformed_employee.clone();
        assert_eq!(
            malformed_employee.transition(EmploymentStatus::Active, None, &owner),
            Err(HrError::InvalidDateWindow)
        );
        assert_eq!(malformed_employee, employee_before_transition);

        let expense_scope = employment_scope.clone();
        let mut expense = ExpenseClaim::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            expense_scope,
            "USD",
            10,
            &owner,
        )
        .unwrap();
        expense.amount_minor = -1;
        assert!(expense.validate().is_err());

        let invalid_leave = LeaveRequest::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            employment_scope,
            NaiveDate::from_ymd_opt(2026, 2, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            "annual leave",
            &owner,
        );
        assert!(invalid_leave.is_err());
    }

    #[test]
    fn payroll_requires_distinct_preparer_reviewer_and_poster() {
        let employment_scope = scope();
        let tenant_scope = employment_scope.tenant_scope();
        let preparer = context("preparer", tenant_scope.clone(), "payroll-open");
        let reviewer = context("reviewer", tenant_scope.clone(), "payroll-review");
        let poster = context("poster", tenant_scope, "payroll-post");
        let line = PayrollLine::new(Uuid::new_v4(), 10_000, 1_000).unwrap();
        let mut run = PayrollRun::new(
            Uuid::new_v4(),
            employment_scope,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            vec![line],
            &preparer,
        )
        .unwrap();
        run.submit_for_review(&preparer).unwrap();
        assert_eq!(run.review(&preparer), Err(HrError::ApprovalSeparation));
        run.review(&reviewer).unwrap();
        assert_eq!(run.post(&reviewer), Err(HrError::ApprovalSeparation));
        run.post(&poster).unwrap();
        assert_eq!(run.status, PayrollStatus::Posted);
        assert!(matches!(
            run.post(&poster),
            Err(HrError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn payroll_and_reassignment_reject_unsafe_or_cross_scope_values() {
        let employment_scope = scope();
        let tenant_scope = employment_scope.tenant_scope();
        let admin_context = context("admin", tenant_scope, "hr-safe");
        assert_eq!(
            PayrollLine::new(Uuid::new_v4(), 100, 101),
            Err(HrError::NegativeAmount)
        );
        assert_eq!(
            ExpiredRoleReassignment::new(
                "workflow",
                "expired",
                "admin",
                employment_scope.clone(),
                "expired role",
                &admin_context,
            ),
            Err(HrError::ApprovalSeparation)
        );
        let other_scope = TenantScope::Company {
            holding_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
        };
        let other_context = context("other", other_scope, "hr-cross-scope");
        assert_eq!(
            LeaveRequest::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                employment_scope,
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                "leave",
                &other_context,
            ),
            Err(HrError::ScopeNotContained)
        );
    }

    #[test]
    fn payroll_rejects_empty_invalid_and_duplicate_employee_lines() {
        let employment_scope = scope();
        let tenant_scope = employment_scope.tenant_scope();
        let preparer = context("preparer", tenant_scope, "payroll-lines");
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        assert_eq!(
            PayrollRun::new(
                Uuid::new_v4(),
                employment_scope.clone(),
                date,
                date,
                Vec::new(),
                &preparer,
            ),
            Err(HrError::EmptyPayrollLines)
        );

        let invalid_line = PayrollLine {
            employee_id: Uuid::nil(),
            gross_minor: 100,
            deductions_minor: 0,
            net_minor: 100,
        };
        assert_eq!(
            PayrollRun::new(
                Uuid::new_v4(),
                employment_scope.clone(),
                date,
                date,
                vec![invalid_line],
                &preparer,
            ),
            Err(HrError::NilIdentifier {
                field: "employee_id"
            })
        );

        let employee_id = Uuid::new_v4();
        let line = PayrollLine::new(employee_id, 100, 10).unwrap();
        assert_eq!(
            PayrollRun::new(
                Uuid::new_v4(),
                employment_scope,
                date,
                date,
                vec![line.clone(), line],
                &preparer,
            ),
            Err(HrError::DuplicatePayrollEmployee(employee_id))
        );

        let deserialized_scope = scope();
        let deserialized_preparer = context(
            "preparer",
            deserialized_scope.tenant_scope(),
            "payroll-revalidate",
        );
        let mut deserialized_like = PayrollRun::new(
            Uuid::new_v4(),
            deserialized_scope,
            date,
            date,
            vec![PayrollLine::new(Uuid::new_v4(), 100, 10).unwrap()],
            &deserialized_preparer,
        )
        .unwrap();
        deserialized_like.lines[0].net_minor = 999;
        assert_eq!(deserialized_like.validate(), Err(HrError::NegativeAmount));
    }

    #[test]
    fn rejected_payroll_transitions_do_not_mutate_attribution() {
        let employment_scope = scope();
        let tenant_scope = employment_scope.tenant_scope();
        let preparer = context("preparer", tenant_scope.clone(), "payroll-atomic-open");
        let reviewer = context("reviewer", tenant_scope.clone(), "payroll-atomic-review");
        let poster = context("poster", tenant_scope, "payroll-atomic-post");
        let line = PayrollLine::new(Uuid::new_v4(), 10_000, 1_000).unwrap();
        let mut run = PayrollRun::new(
            Uuid::new_v4(),
            employment_scope,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            vec![line],
            &preparer,
        )
        .unwrap();

        let before_review = run.clone();
        assert!(matches!(
            run.review(&reviewer),
            Err(HrError::InvalidTransition { .. })
        ));
        assert_eq!(run, before_review);

        run.submit_for_review(&preparer).unwrap();
        let before_post = run.clone();
        assert!(matches!(
            run.post(&poster),
            Err(HrError::InvalidTransition { .. })
        ));
        assert_eq!(run, before_post);
    }

    #[test]
    fn payroll_version_overflow_does_not_mutate_the_aggregate() {
        let employment_scope = scope();
        let tenant_scope = employment_scope.tenant_scope();
        let preparer = context("preparer", tenant_scope, "payroll-overflow");
        let line = PayrollLine::new(Uuid::new_v4(), 10_000, 1_000).unwrap();
        let mut run = PayrollRun::new(
            Uuid::new_v4(),
            employment_scope,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            vec![line],
            &preparer,
        )
        .unwrap();
        run.version = u64::MAX;
        let before = run.clone();

        assert_eq!(
            run.submit_for_review(&preparer),
            Err(HrError::InvalidVersion)
        );
        assert_eq!(run, before);
    }

    #[test]
    fn expense_submission_is_scoped_and_overflow_safe() {
        let employment_scope = scope();
        let owner = context("owner", employment_scope.tenant_scope(), "expense-open");
        let outsider = context(
            "outsider",
            TenantScope::Company {
                holding_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            },
            "expense-outsider",
        );
        let mut expense = ExpenseClaim::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            employment_scope,
            "THB",
            100,
            &owner,
        )
        .unwrap();
        let before = expense.clone();

        assert_eq!(expense.submit(&outsider), Err(HrError::ScopeNotContained));
        assert_eq!(expense, before);

        expense.version = u64::MAX;
        let before_overflow = expense.clone();
        assert_eq!(expense.submit(&owner), Err(HrError::InvalidVersion));
        assert_eq!(expense, before_overflow);
    }

    #[test]
    fn expense_lifecycle_rejects_malformed_currency_and_supports_rejection() {
        let employment_scope = scope();
        let owner = context("owner", employment_scope.tenant_scope(), "expense-owner");
        let reviewer = context(
            "reviewer",
            employment_scope.tenant_scope(),
            "expense-reviewer",
        );
        assert_eq!(
            ExpenseClaim::new(
                Uuid::new_v4(),
                Uuid::new_v4(),
                employment_scope.clone(),
                "thb",
                100,
                &owner,
            ),
            Err(HrError::InvalidCurrency)
        );

        let mut expense = ExpenseClaim::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            employment_scope,
            "THB",
            100,
            &owner,
        )
        .unwrap();
        expense.submit(&owner).unwrap();
        expense.reject(&reviewer).unwrap();
        assert_eq!(expense.status, ExpenseStatus::Rejected);
        assert!(expense.validate().is_ok());
    }

    #[test]
    fn projection_reconciliation_is_green_only_without_mismatches() {
        assert!(HrProjectionReconciliation::default().is_consistent());
        assert!(
            !HrProjectionReconciliation {
                stale_aggregate_count: 1,
                ..HrProjectionReconciliation::default()
            }
            .is_consistent()
        );
    }
}
