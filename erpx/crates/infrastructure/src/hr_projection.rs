//! Typed, rebuildable HR/payroll read projection.
//!
//! HR source rows and outbox events remain authoritative on the tenant write
//! database. This adapter copies only bounded typed fields into the separate
//! tenant read database and never persists source JSON or command contexts.

use chrono::{DateTime, NaiveDate, Utc};
use erpx_domain::hr::{
    EmployeeRecord, EmploymentScope, EmploymentStatus, ExpenseClaim, ExpenseStatus,
    ExpiredRoleReassignment, HrProjectionReconciliation, LeaveRequest, LeaveStatus, PayrollRun,
    PayrollStatus,
};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const HR_PROJECTION_NAME: &str = "hr";
pub const MAX_HR_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_HR_READ_PAGE_SIZE: u32 = 500;
pub const MAX_HR_READ_PAYROLL_LINES_PER_RUN: u32 = 1_000;
const HR_AGGREGATES: [&str; 5] = [
    "hr_employee",
    "hr_leave_request",
    "hr_expense_claim",
    "hr_payroll_run",
    "hr_expired_role_reassignment",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceProjectionIdentity {
    version: u64,
    event_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectedReadMetadata {
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    source_version: u64,
    source_event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectedEmployeeFact {
    employee_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    employee_ref: String,
    display_name: String,
    status: String,
    hired_on: NaiveDate,
    terminated_on: Option<NaiveDate>,
    source_version: u64,
    source_event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectedLeaveFact {
    leave_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    employee_id: Uuid,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    reason: String,
    status: String,
    requested_by: String,
    approved_by: Option<String>,
    source_version: u64,
    source_event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectedExpenseFact {
    expense_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    employee_id: Uuid,
    currency: String,
    amount_minor: i64,
    status: String,
    submitted_by: String,
    approved_by: Option<String>,
    source_version: u64,
    source_event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectedPayrollLineFact {
    line_no: i32,
    employee_id: Uuid,
    gross_minor: i64,
    deductions_minor: i64,
    net_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectedPayrollFact {
    payroll_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
    status: String,
    prepared_by: String,
    reviewed_by: Option<String>,
    approved_by: Option<String>,
    total_gross_minor: i64,
    total_deductions_minor: i64,
    total_net_minor: i64,
    source_version: u64,
    source_event_id: Uuid,
    lines: Vec<ProjectedPayrollLineFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectedReassignmentFact {
    workflow_ref: String,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    expired_principal_ref: String,
    replacement_principal_ref: String,
    reason: String,
    reassigned_at: DateTime<Utc>,
    source_version: u64,
    source_event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
    pub receipts_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HrSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HrSourceEventPage {
    pub events: Vec<HrSourceEvent>,
    pub next_cursor: Option<HrSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrEmployeeReadRow {
    pub employee_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub employee_ref: String,
    pub display_name: String,
    pub status: String,
    pub hired_on: chrono::NaiveDate,
    pub terminated_on: Option<chrono::NaiveDate>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrEmployeeReadPage {
    pub rows: Vec<HrEmployeeReadRow>,
    pub has_more: bool,
    pub next_before_employee_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrLeaveReadCursor {
    pub starts_on: NaiveDate,
    pub leave_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrLeaveReadRow {
    pub leave_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub employee_id: Uuid,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub reason: String,
    pub status: String,
    pub requested_by: String,
    pub approved_by: Option<String>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrLeaveReadPage {
    pub rows: Vec<HrLeaveReadRow>,
    pub has_more: bool,
    pub next_cursor: Option<HrLeaveReadCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrExpenseReadCursor {
    pub expense_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrExpenseReadRow {
    pub expense_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub employee_id: Uuid,
    pub currency: String,
    pub amount_minor: i64,
    pub status: String,
    pub submitted_by: String,
    pub approved_by: Option<String>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrExpenseReadPage {
    pub rows: Vec<HrExpenseReadRow>,
    pub has_more: bool,
    pub next_cursor: Option<HrExpenseReadCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrPayrollRunReadCursor {
    pub starts_on: NaiveDate,
    pub payroll_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrPayrollLineReadRow {
    pub line_no: i32,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub employee_id: Uuid,
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub net_minor: i64,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrPayrollRunReadRow {
    pub payroll_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub status: String,
    pub prepared_by: String,
    pub reviewed_by: Option<String>,
    pub approved_by: Option<String>,
    pub total_gross_minor: i64,
    pub total_deductions_minor: i64,
    pub total_net_minor: i64,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
    pub lines: Vec<HrPayrollLineReadRow>,
    pub lines_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrPayrollRunReadPage {
    pub rows: Vec<HrPayrollRunReadRow>,
    pub has_more: bool,
    pub next_cursor: Option<HrPayrollRunReadCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrReassignmentReadCursor {
    pub reassigned_at: DateTime<Utc>,
    pub workflow_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrReassignmentReadRow {
    pub workflow_ref: String,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub expired_principal_ref: String,
    pub replacement_principal_ref: String,
    pub reason: String,
    pub reassigned_at: DateTime<Utc>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrReassignmentReadPage {
    pub rows: Vec<HrReassignmentReadRow>,
    pub has_more: bool,
    pub next_cursor: Option<HrReassignmentReadCursor>,
}

#[derive(Debug, Clone)]
pub struct SqlxHrReadStore {
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxHrReadStore {
    pub fn new(
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            read_pool,
            holding_ref,
        })
    }

    pub async fn list_employees(
        &self,
        scope: HrReadScope,
        page_size: u32,
        before_employee_id: Option<Uuid>,
    ) -> Result<HrEmployeeReadPage, PersistenceError> {
        if scope.holding_id.is_nil()
            || scope.company_id.is_nil()
            || scope.branch_id.is_some_and(|branch_id| branch_id.is_nil())
            || page_size == 0
            || page_size > MAX_HR_READ_PAGE_SIZE
        {
            return Err(invalid("HR employee read request is invalid"));
        }
        if before_employee_id.is_some_and(|employee_id| employee_id.is_nil()) {
            return Err(invalid("HR employee read cursor is invalid"));
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT employee_id, holding_id, company_id, branch_id, employee_ref,
                    display_name, status, hired_on, terminated_on, source_version,
                    source_event_id, projected_at
             FROM erpx_tenant.hr_employee_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::UUID IS NULL OR employee_id < $5)
             ORDER BY employee_id DESC
             LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before_employee_id)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let page_len = usize::try_from(page_size)
            .map_err(|_| invalid("HR employee page size is not representable"))?;
        let has_more = rows.len() > page_len;
        let rows = rows
            .into_iter()
            .take(page_len)
            .map(|row| {
                Ok(HrEmployeeReadRow {
                    employee_id: row.try_get("employee_id")?,
                    holding_id: row.try_get("holding_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    employee_ref: row.try_get("employee_ref")?,
                    display_name: row.try_get("display_name")?,
                    status: row.try_get("status")?,
                    hired_on: row.try_get("hired_on")?,
                    terminated_on: row.try_get("terminated_on")?,
                    source_version: as_u64(row.try_get("source_version")?, "HR source version")?,
                    source_event_id: row.try_get("source_event_id")?,
                    projected_at: row.try_get("projected_at")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_before_employee_id = if has_more {
            rows.last().map(|row| row.employee_id)
        } else {
            None
        };
        Ok(HrEmployeeReadPage {
            rows,
            has_more,
            next_before_employee_id,
        })
    }

    pub async fn list_leave_requests(
        &self,
        scope: HrReadScope,
        page_size: u32,
        before: Option<&HrLeaveReadCursor>,
    ) -> Result<HrLeaveReadPage, PersistenceError> {
        validate_hr_read_request(scope, page_size)?;
        if let Some(cursor) = before {
            validate_leave_cursor(cursor)?;
        }
        let limit = i64::from(page_size) + 1;
        let before_starts_on = before.map(|cursor| cursor.starts_on);
        let before_leave_id = before.map(|cursor| cursor.leave_id);
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT leave_id, holding_id, company_id, branch_id, employee_id,
                    starts_on, ends_on, reason, status, requested_by, approved_by,
                    source_version, source_event_id, projected_at
             FROM erpx_tenant.hr_leave_request_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::DATE IS NULL OR (starts_on, leave_id) < ($5, $6))
             ORDER BY starts_on DESC, leave_id DESC
             LIMIT $7",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before_starts_on)
        .bind(before_leave_id)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let page_len = page_len(page_size)?;
        let has_more = rows.len() > page_len;
        let rows = rows
            .into_iter()
            .take(page_len)
            .map(decode_leave_read_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if has_more {
            rows.last().map(|row| HrLeaveReadCursor {
                starts_on: row.starts_on,
                leave_id: row.leave_id,
            })
        } else {
            None
        };
        Ok(HrLeaveReadPage {
            rows,
            has_more,
            next_cursor,
        })
    }

    pub async fn list_expense_claims(
        &self,
        scope: HrReadScope,
        page_size: u32,
        before: Option<&HrExpenseReadCursor>,
    ) -> Result<HrExpenseReadPage, PersistenceError> {
        validate_hr_read_request(scope, page_size)?;
        if let Some(cursor) = before {
            validate_expense_cursor(cursor)?;
        }
        let limit = i64::from(page_size) + 1;
        let before_expense_id = before.map(|cursor| cursor.expense_id);
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT expense_id, holding_id, company_id, branch_id, employee_id,
                    currency, amount_minor, status, submitted_by, approved_by,
                    source_version, source_event_id, projected_at
             FROM erpx_tenant.hr_expense_claim_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::UUID IS NULL OR expense_id < $5)
             ORDER BY expense_id DESC
             LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before_expense_id)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let page_len = page_len(page_size)?;
        let has_more = rows.len() > page_len;
        let rows = rows
            .into_iter()
            .take(page_len)
            .map(decode_expense_read_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if has_more {
            rows.last().map(|row| HrExpenseReadCursor {
                expense_id: row.expense_id,
            })
        } else {
            None
        };
        Ok(HrExpenseReadPage {
            rows,
            has_more,
            next_cursor,
        })
    }

    pub async fn list_payroll_runs(
        &self,
        scope: HrReadScope,
        page_size: u32,
        line_page_size: u32,
        before: Option<&HrPayrollRunReadCursor>,
    ) -> Result<HrPayrollRunReadPage, PersistenceError> {
        validate_hr_read_request(scope, page_size)?;
        validate_payroll_line_page_size(line_page_size)?;
        if let Some(cursor) = before {
            validate_payroll_cursor(cursor)?;
        }
        let limit = i64::from(page_size) + 1;
        let line_limit = i64::from(line_page_size) + 1;
        let before_starts_on = before.map(|cursor| cursor.starts_on);
        let before_payroll_id = before.map(|cursor| cursor.payroll_id);
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let parent_rows = sqlx::query(
            "SELECT payroll_id, holding_id, company_id, branch_id, starts_on,
                    ends_on, status, prepared_by, reviewed_by, approved_by,
                    total_gross_minor, total_deductions_minor, total_net_minor,
                    source_version, source_event_id, projected_at
             FROM erpx_tenant.hr_payroll_run_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::DATE IS NULL OR (starts_on, payroll_id) < ($5, $6))
             ORDER BY starts_on DESC, payroll_id DESC
             LIMIT $7",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before_starts_on)
        .bind(before_payroll_id)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        let page_len = page_len(page_size)?;
        let has_more = parent_rows.len() > page_len;
        let mut rows = Vec::with_capacity(parent_rows.len().min(page_len));
        for parent_row in parent_rows.into_iter().take(page_len) {
            let payroll_id: Uuid = parent_row.try_get("payroll_id")?;
            let mut line_rows = sqlx::query(
                "SELECT line_no, holding_id, company_id, branch_id, employee_id,
                        gross_minor, deductions_minor, net_minor,
                        source_version, source_event_id, projected_at
                 FROM erpx_tenant.hr_payroll_line_read_rows
                 WHERE holding_ref=$1 AND payroll_id=$2 AND holding_id=$3
                   AND company_id=$4 AND ($5::UUID IS NULL OR branch_id=$5)
                 ORDER BY line_no ASC
                 LIMIT $6",
            )
            .bind(&self.holding_ref)
            .bind(payroll_id)
            .bind(scope.holding_id)
            .bind(scope.company_id)
            .bind(scope.branch_id)
            .bind(line_limit)
            .fetch_all(&mut *tx)
            .await?;
            let lines_truncated = line_rows.len() > line_page_size as usize;
            line_rows.truncate(line_page_size as usize);
            let lines = line_rows
                .into_iter()
                .map(decode_payroll_line_read_row)
                .collect::<Result<Vec<_>, _>>()?;
            let mut payroll = decode_payroll_run_read_row(&parent_row)?;
            payroll.lines = lines;
            payroll.lines_truncated = lines_truncated;
            rows.push(payroll);
        }
        tx.commit().await?;
        let next_cursor = if has_more {
            rows.last().map(|row| HrPayrollRunReadCursor {
                starts_on: row.starts_on,
                payroll_id: row.payroll_id,
            })
        } else {
            None
        };
        Ok(HrPayrollRunReadPage {
            rows,
            has_more,
            next_cursor,
        })
    }

    pub async fn list_reassignments(
        &self,
        scope: HrReadScope,
        page_size: u32,
        before: Option<&HrReassignmentReadCursor>,
    ) -> Result<HrReassignmentReadPage, PersistenceError> {
        validate_hr_read_request(scope, page_size)?;
        if let Some(cursor) = before {
            validate_reassignment_cursor(cursor)?;
        }
        let limit = i64::from(page_size) + 1;
        let before_reassigned_at = before.map(|cursor| cursor.reassigned_at);
        let before_workflow_ref = before.map(|cursor| cursor.workflow_ref.as_str());
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT workflow_ref, holding_id, company_id, branch_id,
                    expired_principal_ref, replacement_principal_ref, reason,
                    reassigned_at, source_version, source_event_id, projected_at
             FROM erpx_tenant.hr_expired_role_reassignment_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::TIMESTAMPTZ IS NULL
                    OR (reassigned_at, workflow_ref) < ($5, $6))
             ORDER BY reassigned_at DESC, workflow_ref DESC
             LIMIT $7",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before_reassigned_at)
        .bind(before_workflow_ref)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let page_len = page_len(page_size)?;
        let has_more = rows.len() > page_len;
        let rows = rows
            .into_iter()
            .take(page_len)
            .map(decode_reassignment_read_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if has_more {
            rows.last().map(|row| HrReassignmentReadCursor {
                reassigned_at: row.reassigned_at,
                workflow_ref: row.workflow_ref.clone(),
            })
        } else {
            None
        };
        Ok(HrReassignmentReadPage {
            rows,
            has_more,
            next_cursor,
        })
    }
}

fn validate_hr_read_request(scope: HrReadScope, page_size: u32) -> Result<(), PersistenceError> {
    if scope.holding_id.is_nil()
        || scope.company_id.is_nil()
        || scope.branch_id.is_some_and(|branch_id| branch_id.is_nil())
        || page_size == 0
        || page_size > MAX_HR_READ_PAGE_SIZE
    {
        return Err(invalid("HR read scope or page size is invalid"));
    }
    Ok(())
}

fn page_len(page_size: u32) -> Result<usize, PersistenceError> {
    usize::try_from(page_size).map_err(|_| invalid("HR read page size is not representable"))
}

fn validate_payroll_line_page_size(page_size: u32) -> Result<(), PersistenceError> {
    if page_size == 0 || page_size > MAX_HR_READ_PAYROLL_LINES_PER_RUN {
        return Err(invalid("HR payroll line page size is invalid"));
    }
    Ok(())
}

fn validate_leave_cursor(cursor: &HrLeaveReadCursor) -> Result<(), PersistenceError> {
    if cursor.leave_id.is_nil() {
        return Err(invalid("HR leave read cursor is invalid"));
    }
    Ok(())
}

fn validate_expense_cursor(cursor: &HrExpenseReadCursor) -> Result<(), PersistenceError> {
    if cursor.expense_id.is_nil() {
        return Err(invalid("HR expense read cursor is invalid"));
    }
    Ok(())
}

fn validate_payroll_cursor(cursor: &HrPayrollRunReadCursor) -> Result<(), PersistenceError> {
    if cursor.payroll_id.is_nil() {
        return Err(invalid("HR payroll read cursor is invalid"));
    }
    Ok(())
}

fn validate_reassignment_cursor(cursor: &HrReassignmentReadCursor) -> Result<(), PersistenceError> {
    if !valid_opaque_reference(&cursor.workflow_ref, 128) {
        return Err(invalid("HR reassignment read cursor is invalid"));
    }
    Ok(())
}

fn valid_opaque_reference(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

fn decode_leave_read_row(row: sqlx::postgres::PgRow) -> Result<HrLeaveReadRow, PersistenceError> {
    Ok(HrLeaveReadRow {
        leave_id: row.try_get("leave_id")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        employee_id: row.try_get("employee_id")?,
        starts_on: row.try_get("starts_on")?,
        ends_on: row.try_get("ends_on")?,
        reason: row.try_get("reason")?,
        status: row.try_get("status")?,
        requested_by: row.try_get("requested_by")?,
        approved_by: row.try_get("approved_by")?,
        source_version: as_u64(row.try_get("source_version")?, "HR leave source version")?,
        source_event_id: row.try_get("source_event_id")?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_expense_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<HrExpenseReadRow, PersistenceError> {
    Ok(HrExpenseReadRow {
        expense_id: row.try_get("expense_id")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        employee_id: row.try_get("employee_id")?,
        currency: row.try_get("currency")?,
        amount_minor: row.try_get("amount_minor")?,
        status: row.try_get("status")?,
        submitted_by: row.try_get("submitted_by")?,
        approved_by: row.try_get("approved_by")?,
        source_version: as_u64(row.try_get("source_version")?, "HR expense source version")?,
        source_event_id: row.try_get("source_event_id")?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_payroll_run_read_row(
    row: &sqlx::postgres::PgRow,
) -> Result<HrPayrollRunReadRow, PersistenceError> {
    Ok(HrPayrollRunReadRow {
        payroll_id: row.try_get("payroll_id")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        starts_on: row.try_get("starts_on")?,
        ends_on: row.try_get("ends_on")?,
        status: row.try_get("status")?,
        prepared_by: row.try_get("prepared_by")?,
        reviewed_by: row.try_get("reviewed_by")?,
        approved_by: row.try_get("approved_by")?,
        total_gross_minor: row.try_get("total_gross_minor")?,
        total_deductions_minor: row.try_get("total_deductions_minor")?,
        total_net_minor: row.try_get("total_net_minor")?,
        source_version: as_u64(row.try_get("source_version")?, "HR payroll source version")?,
        source_event_id: row.try_get("source_event_id")?,
        projected_at: row.try_get("projected_at")?,
        lines: Vec::new(),
        lines_truncated: false,
    })
}

fn decode_payroll_line_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<HrPayrollLineReadRow, PersistenceError> {
    Ok(HrPayrollLineReadRow {
        line_no: row.try_get("line_no")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        employee_id: row.try_get("employee_id")?,
        gross_minor: row.try_get("gross_minor")?,
        deductions_minor: row.try_get("deductions_minor")?,
        net_minor: row.try_get("net_minor")?,
        source_version: as_u64(
            row.try_get("source_version")?,
            "HR payroll line source version",
        )?,
        source_event_id: row.try_get("source_event_id")?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_reassignment_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<HrReassignmentReadRow, PersistenceError> {
    Ok(HrReassignmentReadRow {
        workflow_ref: row.try_get("workflow_ref")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        expired_principal_ref: row.try_get("expired_principal_ref")?,
        replacement_principal_ref: row.try_get("replacement_principal_ref")?,
        reason: row.try_get("reason")?,
        reassigned_at: row.try_get("reassigned_at")?,
        source_version: as_u64(
            row.try_get("source_version")?,
            "HR reassignment source version",
        )?,
        source_event_id: row.try_get("source_event_id")?,
        projected_at: row.try_get("projected_at")?,
    })
}

#[derive(Debug, Clone)]
pub struct SqlxHrProjectionStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxHrProjectionStore {
    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool: write_pool.clone(),
            read_pool: read_pool.clone(),
            holding_ref,
        })
    }

    /// Rebuild the disposable read model atomically against a stable source
    /// snapshot.  The source lock prevents authoritative outbox writes from
    /// moving while the read transaction is cleared and replayed.  Any decode,
    /// ordering, authorization, or SQL failure rolls back the read transaction
    /// and preserves the previously committed projection.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        page_size: u32,
    ) -> Result<(HrProjectionRebuildPreparation, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_HR_SOURCE_EVENT_PAGE_SIZE {
            return Err(invalid(format!(
                "HR source event page size must be 1..={MAX_HR_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        validate_source_event_versions(&mut source_tx, &self.holding_ref).await?;
        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let preparation = clear_projection_in_transaction(&mut read_tx, &self.holding_ref).await?;
        let mut cursor = None;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_source_event_page_in_transaction(
                &mut source_tx,
                &self.holding_ref,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                verify_authoritative_source_event_in_transaction(
                    &mut source_tx,
                    &self.holding_ref,
                    event,
                )
                .await?;
                if self
                    .project_event_in_transaction(&mut read_tx, event)
                    .await?
                    .applied
                {
                    events_replayed = events_replayed.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(invalid("HR source page omitted its continuation cursor"));
            }
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&HrSourceEventCursor>,
        page_size: u32,
    ) -> Result<HrSourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_HR_SOURCE_EVENT_PAGE_SIZE {
            return Err(invalid(format!(
                "HR source event page size must be 1..={MAX_HR_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let page =
            read_source_event_page_in_transaction(&mut tx, &self.holding_ref, cursor, page_size)
                .await?;
        tx.commit().await?;
        Ok(page)
    }

    /// Compare the authoritative HR/payroll outbox with the disposable
    /// projection checkpoints, including the immutable latest-event identity.
    /// The report is read-only and holding-scoped; callers must rebuild or
    /// investigate mismatches rather than mutate the source from this path.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<HrProjectionReconciliation, PersistenceError> {
        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        validate_source_event_versions(&mut source_tx, &self.holding_ref).await?;
        let source_rows = sqlx::query(
            "WITH ranked_source AS (
                 SELECT event_type, aggregate_type, aggregate_id, aggregate_version,
                        event_id, payload, occurred_at,
                        COUNT(*) OVER (
                            PARTITION BY aggregate_type, aggregate_id
                        ) AS event_count,
                        ROW_NUMBER() OVER (
                            PARTITION BY aggregate_type, aggregate_id
                            ORDER BY aggregate_version DESC, event_id DESC
                        ) AS latest_rank
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type = ANY($2)
             )
             SELECT event_type, aggregate_type, aggregate_id,
                    aggregate_version AS latest_version,
                    event_id AS latest_event_id, payload, occurred_at, event_count
             FROM ranked_source
             WHERE latest_rank=1",
        )
        .bind(&self.holding_ref)
        .bind(HR_AGGREGATES)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;

        let mut source = HashMap::with_capacity(source_rows.len());
        let mut source_events = Vec::with_capacity(source_rows.len());
        let mut source_event_count = 0_u64;
        for row in source_rows {
            let event_type: String = row.try_get("event_type")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let latest_version = as_u64(row.try_get("latest_version")?, "HR source version")?;
            let latest_event_id: Uuid = row.try_get("latest_event_id")?;
            if latest_event_id.is_nil() {
                return Err(invalid("HR source event ID is invalid"));
            }
            let event_count = as_u64(row.try_get("event_count")?, "HR source event count")?;
            source_event_count = source_event_count
                .checked_add(event_count)
                .ok_or_else(|| invalid("HR event count overflow"))?;
            let event = HrSourceEvent {
                event_id: latest_event_id,
                event_type,
                aggregate_type: aggregate_type.clone(),
                aggregate_id,
                aggregate_version: latest_version,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            };
            validate_source_event(&event)?;
            if payload_version(&event.payload)? != latest_version {
                return Err(invalid("HR source version does not match payload"));
            }
            source_events.push(event);
            source.insert(
                (aggregate_type, aggregate_id),
                SourceProjectionIdentity {
                    version: latest_version,
                    event_id: latest_event_id,
                },
            );
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let read_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, source_version, last_event_id
             FROM erpx_tenant.hr_projection_checkpoints
             WHERE holding_ref=$1 AND projection_name=$2",
        )
        .bind(&self.holding_ref)
        .bind(HR_PROJECTION_NAME)
        .fetch_all(&mut *read_tx)
        .await?;
        let projected = read_rows
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>((
                    (row.try_get("aggregate_type")?, row.try_get("aggregate_id")?),
                    SourceProjectionIdentity {
                        version: as_u64(row.try_get("source_version")?, "HR projected version")?,
                        event_id: row.try_get("last_event_id")?,
                    },
                ))
            })
            .collect::<Result<HashMap<(String, Uuid), SourceProjectionIdentity>, _>>()?;
        let projected_metadata = read_projection_metadata(&mut read_tx, &self.holding_ref).await?;
        let projected_employee_facts =
            read_employee_projection_facts(&mut read_tx, &self.holding_ref).await?;
        let projected_leave_facts =
            read_leave_projection_facts(&mut read_tx, &self.holding_ref).await?;
        let projected_expense_facts =
            read_expense_projection_facts(&mut read_tx, &self.holding_ref).await?;
        let projected_payroll_facts =
            read_payroll_projection_facts(&mut read_tx, &self.holding_ref).await?;
        let projected_reassignment_facts =
            read_reassignment_projection_facts(&mut read_tx, &self.holding_ref).await?;
        read_tx.commit().await?;

        let mut projected_read_identities = HashSet::new();
        projected_read_identities.extend(
            projected_employee_facts
                .keys()
                .copied()
                .map(|id| ("hr_employee".to_owned(), id)),
        );
        projected_read_identities.extend(
            projected_leave_facts
                .keys()
                .copied()
                .map(|id| ("hr_leave_request".to_owned(), id)),
        );
        projected_read_identities.extend(
            projected_expense_facts
                .keys()
                .copied()
                .map(|id| ("hr_expense_claim".to_owned(), id)),
        );
        projected_read_identities.extend(
            projected_payroll_facts
                .keys()
                .copied()
                .map(|id| ("hr_payroll_run".to_owned(), id)),
        );
        projected_read_identities.extend(projected_reassignment_facts.keys().map(|workflow_ref| {
            (
                "hr_expired_role_reassignment".to_owned(),
                stable_reassignment_id(&self.holding_ref, workflow_ref),
            )
        }));

        let missing_aggregate_count = source
            .keys()
            .filter(|key| !projected.contains_key(*key))
            .count() as u64;
        let stale_aggregate_count = source
            .iter()
            .filter(|(key, source_identity)| {
                projected.get(*key).is_some_and(|projected_identity| {
                    projected_identity.version != source_identity.version
                        || projected_identity.event_id != source_identity.event_id
                })
            })
            .count() as u64;
        let mut orphan_aggregate_count = projected
            .keys()
            .filter(|key| !source.contains_key(*key))
            .count() as u64;
        orphan_aggregate_count = orphan_aggregate_count.saturating_add(
            projected_read_identities
                .iter()
                .filter(|key| !projected.contains_key(*key))
                .count() as u64,
        );
        let source_event_keys = source_events
            .iter()
            .map(|event| (event.aggregate_type.clone(), event.event_id))
            .collect::<HashSet<_>>();
        orphan_aggregate_count = orphan_aggregate_count.saturating_add(
            projected_metadata
                .keys()
                .filter(|key| !source_event_keys.contains(*key))
                .count() as u64,
        );
        let mut stale_aggregate_count = stale_aggregate_count;
        for event in source_events {
            let key = (event.aggregate_type.clone(), event.aggregate_id);
            let Some(projected_identity) = projected.get(&key) else {
                continue;
            };
            if projected_identity.version != event.aggregate_version
                || projected_identity.event_id != event.event_id
            {
                continue;
            }
            let expected_rows = expected_projection_row_count(&event)?;
            let rows = projected_metadata
                .get(&(event.aggregate_type.clone(), event.event_id))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let (holding_id, company_id, branch_id) = event_scope(&event)?;
            let metadata_matches = rows.len() as u64 == expected_rows
                && rows.iter().all(|row| {
                    row.holding_id == holding_id
                        && row.company_id == company_id
                        && row.branch_id == branch_id
                        && row.source_version == event.aggregate_version
                        && row.source_event_id == event.event_id
                });
            let payload_matches = projected_payload_matches_source(
                &event,
                &self.holding_ref,
                &projected_employee_facts,
                &projected_leave_facts,
                &projected_expense_facts,
                &projected_payroll_facts,
                &projected_reassignment_facts,
            )?;
            if !metadata_matches || !payload_matches {
                stale_aggregate_count = stale_aggregate_count.saturating_add(1);
            }
        }

        Ok(HrProjectionReconciliation {
            source_event_count,
            source_aggregate_count: source.len() as u64,
            projected_aggregate_count: projected.len() as u64,
            missing_aggregate_count,
            stale_aggregate_count,
            orphan_aggregate_count,
        })
    }

    pub async fn project_event(
        &self,
        event: &HrSourceEvent,
    ) -> Result<HrProjectionApplication, PersistenceError> {
        validate_source_event(event)?;
        self.verify_authoritative_source_event(event).await?;
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let result = self.project_event_in_transaction(&mut tx, event).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn project_event_in_transaction(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: &HrSourceEvent,
    ) -> Result<HrProjectionApplication, PersistenceError> {
        validate_source_event(event)?;
        if let Some(receipt) = sqlx::query(
            "SELECT aggregate_type, aggregate_id, aggregate_version
             FROM erpx_tenant.hr_projection_event_receipts
             WHERE holding_ref=$1 AND projection_name=$2 AND event_id=$3
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(HR_PROJECTION_NAME)
        .bind(event.event_id)
        .fetch_optional(&mut **tx)
        .await?
        {
            let receipt_aggregate_type: String = receipt.try_get("aggregate_type")?;
            let receipt_aggregate_id: Uuid = receipt.try_get("aggregate_id")?;
            let receipt_version =
                as_u64(receipt.try_get("aggregate_version")?, "HR receipt version")?;
            if receipt_aggregate_type != event.aggregate_type
                || receipt_aggregate_id != event.aggregate_id
                || receipt_version != event.aggregate_version
            {
                return Err(invalid("HR projection event receipt identity conflicts"));
            }
            return Ok(application(event, false));
        }
        let current = sqlx::query(
            "SELECT source_version, last_event_id
             FROM erpx_tenant.hr_projection_checkpoints
             WHERE holding_ref=$1 AND projection_name=$2
               AND aggregate_type=$3 AND aggregate_id=$4
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(HR_PROJECTION_NAME)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(current) = current {
            let current_version =
                as_u64(current.try_get("source_version")?, "HR projection version")?;
            let current_event_id: Uuid = current.try_get("last_event_id")?;
            if event.aggregate_version <= current_version {
                if event.aggregate_version == current_version {
                    if event.event_id != current_event_id {
                        return Err(invalid("HR projection version is bound to another event"));
                    }
                    return Ok(application(event, false));
                }
                return Err(invalid("HR projection is missing the source event receipt"));
            }
            if event.aggregate_version != current_version.saturating_add(1) {
                return Err(invalid("HR projection version is not contiguous"));
            }
        } else if event.aggregate_version != 1 {
            return Err(invalid("HR projection must start at version one"));
        }

        match event.aggregate_type.as_str() {
            "hr_employee" => project_employee(tx, &self.holding_ref, event).await?,
            "hr_leave_request" => project_leave(tx, &self.holding_ref, event).await?,
            "hr_expense_claim" => project_expense(tx, &self.holding_ref, event).await?,
            "hr_payroll_run" => project_payroll(tx, &self.holding_ref, event).await?,
            "hr_expired_role_reassignment" => {
                project_reassignment(tx, &self.holding_ref, event).await?
            }
            _ => return Err(invalid("HR aggregate type is unsupported")),
        }
        let (holding_id, company_id, branch_id) = event_scope(event)?;
        sqlx::query(
            "INSERT INTO erpx_tenant.hr_projection_checkpoints
                (holding_ref, projection_name, aggregate_type, aggregate_id,
                 holding_id, company_id, branch_id, last_event_id, source_version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (holding_ref, projection_name, aggregate_type, aggregate_id)
             DO UPDATE SET holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
                 branch_id=EXCLUDED.branch_id, last_event_id=EXCLUDED.last_event_id,
                 source_version=EXCLUDED.source_version, projected_at=now()",
        )
        .bind(&self.holding_ref)
        .bind(HR_PROJECTION_NAME)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(event.event_id)
        .bind(as_i64(event.aggregate_version, "HR source version")?)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.hr_projection_event_receipts
                (holding_ref, projection_name, event_id, aggregate_type,
                 aggregate_id, aggregate_version)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(&self.holding_ref)
        .bind(HR_PROJECTION_NAME)
        .bind(event.event_id)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .bind(as_i64(event.aggregate_version, "HR source version")?)
        .execute(&mut **tx)
        .await?;
        Ok(application(event, true))
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<HrProjectionRebuildPreparation, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let preparation = clear_projection_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(preparation)
    }

    /// Read-side projection is permitted only for the exact immutable source
    /// event committed for this holding. This check is intentionally made
    /// before opening the read transaction so a forged but valid HR payload
    /// cannot create a read row or receipt.
    async fn verify_authoritative_source_event(
        &self,
        event: &HrSourceEvent,
    ) -> Result<(), PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let result =
            verify_authoritative_source_event_in_transaction(&mut tx, &self.holding_ref, event)
                .await;
        tx.commit().await?;
        result
    }
}

async fn clear_projection_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<HrProjectionRebuildPreparation, PersistenceError> {
    let mut rows_cleared = 0_u64;
    for table in [
        "hr_employee_read_rows",
        "hr_leave_request_read_rows",
        "hr_expense_claim_read_rows",
        "hr_payroll_run_read_rows",
        "hr_payroll_line_read_rows",
        "hr_expired_role_reassignment_read_rows",
    ] {
        rows_cleared = rows_cleared.saturating_add(
            sqlx::query(&format!(
                "DELETE FROM erpx_tenant.{table} WHERE holding_ref=$1"
            ))
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected(),
        );
    }
    let checkpoints_cleared =
        sqlx::query("DELETE FROM erpx_tenant.hr_projection_checkpoints WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    let receipts_cleared =
        sqlx::query("DELETE FROM erpx_tenant.hr_projection_event_receipts WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    Ok(HrProjectionRebuildPreparation {
        holding_ref: holding_ref.into(),
        rows_cleared,
        checkpoints_cleared,
        receipts_cleared,
    })
}

async fn read_source_event_page_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
    cursor: Option<&HrSourceEventCursor>,
    page_size: u32,
) -> Result<HrSourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_HR_SOURCE_EVENT_PAGE_SIZE {
        return Err(invalid(format!(
            "HR source event page size must be 1..={MAX_HR_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    if let Some(cursor) = cursor {
        validate_cursor(cursor)?;
    }
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
               AND (aggregate_type > $3
                    OR (aggregate_type=$3 AND aggregate_id > $4)
                    OR (aggregate_type=$3 AND aggregate_id=$4
                        AND aggregate_version > $5))
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $6",
        )
        .bind(holding_ref)
        .bind(HR_AGGREGATES)
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "HR source cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $3",
        )
        .bind(holding_ref)
        .bind(HR_AGGREGATES)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let page_len = usize::try_from(page_size)
        .map_err(|_| invalid("HR source page size is not representable"))?;
    let has_more = rows.len() > page_len;
    let events = rows
        .into_iter()
        .take(page_len)
        .map(decode_source_event)
        .collect::<Result<Vec<_>, _>>()?;
    for event in &events {
        validate_source_event(event)?;
    }
    let next_cursor = if has_more {
        events.last().map(|event| HrSourceEventCursor {
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(HrSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

fn decode_source_event(row: sqlx::postgres::PgRow) -> Result<HrSourceEvent, PersistenceError> {
    let event = HrSourceEvent {
        event_id: row.try_get("event_id")?,
        event_type: row.try_get("event_type")?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        aggregate_version: as_u64(row.try_get("aggregate_version")?, "HR source version")?,
        payload: row.try_get("payload")?,
        occurred_at: row.try_get("occurred_at")?,
    };
    validate_source_event(&event)?;
    Ok(event)
}

fn validate_source_event(event: &HrSourceEvent) -> Result<(), PersistenceError> {
    if event.event_id.is_nil()
        || event.aggregate_id.is_nil()
        || event.aggregate_version == 0
        || event.event_type.trim().is_empty()
        || event.event_type.len() > 120
        || event.event_type.chars().any(char::is_control)
        || !HR_AGGREGATES.contains(&event.aggregate_type.as_str())
        || !event.payload.is_object()
        || event.payload.get("aggregate").is_none()
    {
        return Err(invalid("HR source event metadata is invalid"));
    }
    let expected_event_type = match event.aggregate_type.as_str() {
        "hr_employee" => "hr.employee_saved.v1",
        "hr_leave_request" => "hr.leave_saved.v1",
        "hr_expense_claim" => "hr.expense_saved.v1",
        "hr_payroll_run" => "hr.payroll_saved.v1",
        "hr_expired_role_reassignment" => "hr.expired_role_reassigned.v1",
        _ => return Err(invalid("HR aggregate type is unsupported")),
    };
    if event.event_type != expected_event_type {
        return Err(invalid("HR source event type does not match aggregate"));
    }
    if event.payload.get("aggregate_type").and_then(Value::as_str)
        != Some(event.aggregate_type.as_str())
        || event.payload.get("event_type").and_then(Value::as_str)
            != Some(event.event_type.as_str())
    {
        return Err(invalid("HR source event envelope is invalid"));
    }
    if payload_version(&event.payload)? != event.aggregate_version {
        return Err(invalid("HR source version does not match payload"));
    }
    Ok(())
}

async fn validate_source_event_versions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let invalid_rows: i64 = sqlx::query_scalar(
        "WITH ordered AS (
             SELECT aggregate_type, aggregate_id, aggregate_version,
                    ROW_NUMBER() OVER (
                        PARTITION BY aggregate_type, aggregate_id
                        ORDER BY aggregate_version, event_id
                    ) AS expected_version,
                    COUNT(*) OVER (
                        PARTITION BY aggregate_type, aggregate_id, aggregate_version
                    ) AS version_count
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
         )
         SELECT COUNT(*)
         FROM ordered
         WHERE aggregate_version <> expected_version
            OR version_count <> 1",
    )
    .bind(holding_ref)
    .bind(HR_AGGREGATES)
    .fetch_one(&mut **tx)
    .await?;
    if invalid_rows != 0 {
        return Err(invalid("HR source aggregate versions are not contiguous"));
    }
    Ok(())
}

async fn verify_authoritative_source_event_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
    event: &HrSourceEvent,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT event_type, aggregate_type, aggregate_id, aggregate_version,
                payload, occurred_at
         FROM erpx_tenant.outbox_events
         WHERE event_id=$1 AND holding_ref=$2
         FOR SHARE",
    )
    .bind(event.event_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Err(invalid("HR projection event is not authoritative"));
    };
    let source_event_type: String = row.try_get("event_type")?;
    let source_aggregate_type: String = row.try_get("aggregate_type")?;
    let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let source_version = as_u64(row.try_get("aggregate_version")?, "HR source version")?;
    let source_payload: Value = row.try_get("payload")?;
    let source_occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
    if source_event_type != event.event_type
        || source_aggregate_type != event.aggregate_type
        || source_aggregate_id != event.aggregate_id
        || source_version != event.aggregate_version
        || source_payload != event.payload
        || source_occurred_at != event.occurred_at
    {
        return Err(invalid(
            "HR projection event does not match the authoritative outbox",
        ));
    }
    Ok(())
}

async fn read_projection_metadata(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<HashMap<(String, Uuid), Vec<ProjectedReadMetadata>>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT 'hr_employee'::TEXT AS aggregate_type, source_event_id,
                holding_id, company_id, branch_id, source_version
         FROM erpx_tenant.hr_employee_read_rows
         WHERE holding_ref=$1
         UNION ALL
         SELECT 'hr_leave_request'::TEXT AS aggregate_type, source_event_id,
                holding_id, company_id, branch_id, source_version
         FROM erpx_tenant.hr_leave_request_read_rows
         WHERE holding_ref=$1
         UNION ALL
         SELECT 'hr_expense_claim'::TEXT AS aggregate_type, source_event_id,
                holding_id, company_id, branch_id, source_version
         FROM erpx_tenant.hr_expense_claim_read_rows
         WHERE holding_ref=$1
         UNION ALL
         SELECT 'hr_payroll_run'::TEXT AS aggregate_type, source_event_id,
                holding_id, company_id, branch_id, source_version
         FROM erpx_tenant.hr_payroll_run_read_rows
         WHERE holding_ref=$1
         UNION ALL
         SELECT 'hr_payroll_run'::TEXT AS aggregate_type, source_event_id,
                holding_id, company_id, branch_id, source_version
         FROM erpx_tenant.hr_payroll_line_read_rows
         WHERE holding_ref=$1
         UNION ALL
         SELECT 'hr_expired_role_reassignment'::TEXT AS aggregate_type,
                source_event_id, holding_id, company_id, branch_id, source_version
         FROM erpx_tenant.hr_expired_role_reassignment_read_rows
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    let mut metadata = HashMap::new();
    for row in rows {
        let aggregate_type: String = row.try_get("aggregate_type")?;
        let source_event_id: Uuid = row.try_get("source_event_id")?;
        if source_event_id.is_nil() {
            return Err(invalid("HR projected source event ID is invalid"));
        }
        metadata
            .entry((aggregate_type, source_event_id))
            .or_insert_with(Vec::new)
            .push(ProjectedReadMetadata {
                holding_id: row.try_get("holding_id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                source_version: as_u64(
                    row.try_get("source_version")?,
                    "HR projected source version",
                )?,
                source_event_id,
            });
    }
    Ok(metadata)
}

async fn read_employee_projection_facts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<HashMap<Uuid, ProjectedEmployeeFact>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT employee_id, holding_id, company_id, branch_id,
                employee_ref, display_name, status, hired_on, terminated_on,
                source_version, source_event_id
         FROM erpx_tenant.hr_employee_read_rows
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    let mut facts = HashMap::with_capacity(rows.len());
    for row in rows {
        let employee_id: Uuid = row.try_get("employee_id")?;
        if employee_id.is_nil() {
            return Err(invalid("HR projected employee ID is invalid"));
        }
        let source_event_id: Uuid = row.try_get("source_event_id")?;
        if source_event_id.is_nil() {
            return Err(invalid("HR projected employee source event ID is invalid"));
        }
        let fact = ProjectedEmployeeFact {
            employee_id,
            holding_id: row.try_get("holding_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            employee_ref: row.try_get("employee_ref")?,
            display_name: row.try_get("display_name")?,
            status: row.try_get("status")?,
            hired_on: row.try_get("hired_on")?,
            terminated_on: row.try_get("terminated_on")?,
            source_version: as_u64(
                row.try_get("source_version")?,
                "HR projected source version",
            )?,
            source_event_id,
        };
        if facts.insert(employee_id, fact).is_some() {
            return Err(invalid("HR projected employee identity is duplicated"));
        }
    }
    Ok(facts)
}

async fn read_leave_projection_facts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<HashMap<Uuid, ProjectedLeaveFact>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT leave_id, holding_id, company_id, branch_id, employee_id,
                starts_on, ends_on, reason, status, requested_by, approved_by,
                source_version, source_event_id
         FROM erpx_tenant.hr_leave_request_read_rows
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    let mut facts = HashMap::with_capacity(rows.len());
    for row in rows {
        let leave_id: Uuid = row.try_get("leave_id")?;
        let source_event_id: Uuid = row.try_get("source_event_id")?;
        if leave_id.is_nil() || source_event_id.is_nil() {
            return Err(invalid("HR projected leave identity is invalid"));
        }
        let fact = ProjectedLeaveFact {
            leave_id,
            holding_id: row.try_get("holding_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            employee_id: row.try_get("employee_id")?,
            starts_on: row.try_get("starts_on")?,
            ends_on: row.try_get("ends_on")?,
            reason: row.try_get("reason")?,
            status: row.try_get("status")?,
            requested_by: row.try_get("requested_by")?,
            approved_by: row.try_get("approved_by")?,
            source_version: as_u64(
                row.try_get("source_version")?,
                "HR projected leave source version",
            )?,
            source_event_id,
        };
        if facts.insert(leave_id, fact).is_some() {
            return Err(invalid("HR projected leave identity is duplicated"));
        }
    }
    Ok(facts)
}

async fn read_expense_projection_facts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<HashMap<Uuid, ProjectedExpenseFact>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT expense_id, holding_id, company_id, branch_id, employee_id,
                currency, amount_minor, status, submitted_by, approved_by,
                source_version, source_event_id
         FROM erpx_tenant.hr_expense_claim_read_rows
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    let mut facts = HashMap::with_capacity(rows.len());
    for row in rows {
        let expense_id: Uuid = row.try_get("expense_id")?;
        let source_event_id: Uuid = row.try_get("source_event_id")?;
        if expense_id.is_nil() || source_event_id.is_nil() {
            return Err(invalid("HR projected expense identity is invalid"));
        }
        let fact = ProjectedExpenseFact {
            expense_id,
            holding_id: row.try_get("holding_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            employee_id: row.try_get("employee_id")?,
            currency: row.try_get("currency")?,
            amount_minor: row.try_get("amount_minor")?,
            status: row.try_get("status")?,
            submitted_by: row.try_get("submitted_by")?,
            approved_by: row.try_get("approved_by")?,
            source_version: as_u64(
                row.try_get("source_version")?,
                "HR projected expense source version",
            )?,
            source_event_id,
        };
        if facts.insert(expense_id, fact).is_some() {
            return Err(invalid("HR projected expense identity is duplicated"));
        }
    }
    Ok(facts)
}

async fn read_payroll_projection_facts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<HashMap<Uuid, ProjectedPayrollFact>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT payroll_id, holding_id, company_id, branch_id, starts_on,
                ends_on, status, prepared_by, reviewed_by, approved_by,
                total_gross_minor, total_deductions_minor, total_net_minor,
                source_version, source_event_id
         FROM erpx_tenant.hr_payroll_run_read_rows
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    let mut facts = HashMap::with_capacity(rows.len());
    for row in rows {
        let payroll_id: Uuid = row.try_get("payroll_id")?;
        let source_event_id: Uuid = row.try_get("source_event_id")?;
        if payroll_id.is_nil() || source_event_id.is_nil() {
            return Err(invalid("HR projected payroll identity is invalid"));
        }
        let fact = ProjectedPayrollFact {
            payroll_id,
            holding_id: row.try_get("holding_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            starts_on: row.try_get("starts_on")?,
            ends_on: row.try_get("ends_on")?,
            status: row.try_get("status")?,
            prepared_by: row.try_get("prepared_by")?,
            reviewed_by: row.try_get("reviewed_by")?,
            approved_by: row.try_get("approved_by")?,
            total_gross_minor: row.try_get("total_gross_minor")?,
            total_deductions_minor: row.try_get("total_deductions_minor")?,
            total_net_minor: row.try_get("total_net_minor")?,
            source_version: as_u64(
                row.try_get("source_version")?,
                "HR projected payroll source version",
            )?,
            source_event_id,
            lines: Vec::new(),
        };
        if facts.insert(payroll_id, fact).is_some() {
            return Err(invalid("HR projected payroll identity is duplicated"));
        }
    }
    let line_rows = sqlx::query(
        "SELECT payroll_id, line_no, employee_id, gross_minor, deductions_minor, net_minor
         FROM erpx_tenant.hr_payroll_line_read_rows
         WHERE holding_ref=$1
         ORDER BY payroll_id, line_no",
    )
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    for row in line_rows {
        let payroll_id: Uuid = row.try_get("payroll_id")?;
        let fact = facts
            .get_mut(&payroll_id)
            .ok_or_else(|| invalid("HR payroll line has no projected parent"))?;
        let line = ProjectedPayrollLineFact {
            line_no: row.try_get("line_no")?,
            employee_id: row.try_get("employee_id")?,
            gross_minor: row.try_get("gross_minor")?,
            deductions_minor: row.try_get("deductions_minor")?,
            net_minor: row.try_get("net_minor")?,
        };
        if fact
            .lines
            .iter()
            .any(|existing| existing.line_no == line.line_no)
        {
            return Err(invalid("HR payroll line number is duplicated"));
        }
        fact.lines.push(line);
    }
    Ok(facts)
}

async fn read_reassignment_projection_facts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
) -> Result<HashMap<String, ProjectedReassignmentFact>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT workflow_ref, holding_id, company_id, branch_id,
                expired_principal_ref, replacement_principal_ref, reason,
                reassigned_at, source_version, source_event_id
         FROM erpx_tenant.hr_expired_role_reassignment_read_rows
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .fetch_all(&mut **tx)
    .await?;
    let mut facts = HashMap::with_capacity(rows.len());
    for row in rows {
        let workflow_ref: String = row.try_get("workflow_ref")?;
        let source_event_id: Uuid = row.try_get("source_event_id")?;
        if !valid_opaque_reference(&workflow_ref, 128) || source_event_id.is_nil() {
            return Err(invalid("HR projected reassignment identity is invalid"));
        }
        let fact = ProjectedReassignmentFact {
            workflow_ref: workflow_ref.clone(),
            holding_id: row.try_get("holding_id")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            expired_principal_ref: row.try_get("expired_principal_ref")?,
            replacement_principal_ref: row.try_get("replacement_principal_ref")?,
            reason: row.try_get("reason")?,
            reassigned_at: row.try_get("reassigned_at")?,
            source_version: as_u64(
                row.try_get("source_version")?,
                "HR projected reassignment source version",
            )?,
            source_event_id,
        };
        if facts.insert(workflow_ref, fact).is_some() {
            return Err(invalid("HR projected reassignment is duplicated"));
        }
    }
    Ok(facts)
}

fn employee_projection_matches_source(
    employee: &EmployeeRecord,
    event: &HrSourceEvent,
    projected: Option<&ProjectedEmployeeFact>,
) -> bool {
    projected.is_some_and(|projected| {
        projected.employee_id == employee.id
            && event.aggregate_type == "hr_employee"
            && event.aggregate_id == employee.id
            && projected.holding_id == employee.scope.holding_id
            && projected.company_id == employee.scope.company_id
            && projected.branch_id == employee.scope.branch_id
            && projected.employee_ref == employee.employee_ref
            && projected.display_name == employee.display_name
            && projected.status == employee_status(employee.status)
            && projected.hired_on == employee.hired_on
            && projected.terminated_on == employee.terminated_on
            && projected.source_version == event.aggregate_version
            && projected.source_event_id == event.event_id
    })
}

fn projected_payload_matches_source(
    event: &HrSourceEvent,
    holding_ref: &str,
    employees: &HashMap<Uuid, ProjectedEmployeeFact>,
    leaves: &HashMap<Uuid, ProjectedLeaveFact>,
    expenses: &HashMap<Uuid, ProjectedExpenseFact>,
    payrolls: &HashMap<Uuid, ProjectedPayrollFact>,
    reassignments: &HashMap<String, ProjectedReassignmentFact>,
) -> Result<bool, PersistenceError> {
    match event.aggregate_type.as_str() {
        "hr_employee" => {
            let employee: EmployeeRecord = aggregate(event)?;
            employee
                .validate()
                .map_err(|error| invalid(format!("invalid HR employee aggregate: {error}")))?;
            Ok(employee_projection_matches_source(
                &employee,
                event,
                employees.get(&event.aggregate_id),
            ))
        }
        "hr_leave_request" => {
            let leave: LeaveRequest = aggregate(event)?;
            leave
                .validate()
                .map_err(|error| invalid(format!("invalid HR leave aggregate: {error}")))?;
            if leave.id != event.aggregate_id {
                return Ok(false);
            }
            Ok(leaves.get(&leave.id).is_some_and(|projected| {
                projected.leave_id == leave.id
                    && projected.holding_id == leave.scope.holding_id
                    && projected.company_id == leave.scope.company_id
                    && projected.branch_id == leave.scope.branch_id
                    && projected.employee_id == leave.employee_id
                    && projected.starts_on == leave.starts_on
                    && projected.ends_on == leave.ends_on
                    && projected.reason == leave.reason
                    && projected.status == leave_status(leave.status)
                    && projected.requested_by == leave.requested_by
                    && projected.approved_by == leave.approved_by
                    && projected.source_version == event.aggregate_version
                    && projected.source_event_id == event.event_id
            }))
        }
        "hr_expense_claim" => {
            let expense: ExpenseClaim = aggregate(event)?;
            expense
                .validate()
                .map_err(|error| invalid(format!("invalid HR expense aggregate: {error}")))?;
            if expense.id != event.aggregate_id {
                return Ok(false);
            }
            Ok(expenses.get(&expense.id).is_some_and(|projected| {
                projected.expense_id == expense.id
                    && projected.holding_id == expense.scope.holding_id
                    && projected.company_id == expense.scope.company_id
                    && projected.branch_id == expense.scope.branch_id
                    && projected.employee_id == expense.employee_id
                    && projected.currency == expense.currency
                    && projected.amount_minor == expense.amount_minor
                    && projected.status == expense_status(expense.status)
                    && projected.submitted_by == expense.submitted_by
                    && projected.approved_by == expense.approved_by
                    && projected.source_version == event.aggregate_version
                    && projected.source_event_id == event.event_id
            }))
        }
        "hr_payroll_run" => {
            let payroll: PayrollRun = aggregate(event)?;
            payroll
                .validate()
                .map_err(|error| invalid(format!("invalid HR payroll aggregate: {error}")))?;
            if payroll.id != event.aggregate_id {
                return Ok(false);
            }
            Ok(payrolls.get(&payroll.id).is_some_and(|projected| {
                projected.payroll_id == payroll.id
                    && projected.holding_id == payroll.scope.holding_id
                    && projected.company_id == payroll.scope.company_id
                    && projected.branch_id == payroll.scope.branch_id
                    && projected.starts_on == payroll.starts_on
                    && projected.ends_on == payroll.ends_on
                    && projected.status == payroll_status(payroll.status)
                    && projected.prepared_by == payroll.prepared_by
                    && projected.reviewed_by == payroll.reviewed_by
                    && projected.approved_by == payroll.approved_by
                    && projected.total_gross_minor == payroll.total_gross_minor
                    && projected.total_deductions_minor == payroll.total_deductions_minor
                    && projected.total_net_minor == payroll.total_net_minor
                    && projected.source_version == event.aggregate_version
                    && projected.source_event_id == event.event_id
                    && projected.lines.len() == payroll.lines.len()
                    && projected.lines.iter().zip(&payroll.lines).enumerate().all(
                        |(line_no, (projected, source))| {
                            projected.line_no == line_no as i32
                                && projected.employee_id == source.employee_id
                                && projected.gross_minor == source.gross_minor
                                && projected.deductions_minor == source.deductions_minor
                                && projected.net_minor == source.net_minor
                        },
                    )
            }))
        }
        "hr_expired_role_reassignment" => {
            let reassignment: ExpiredRoleReassignment = aggregate(event)?;
            reassignment
                .validate()
                .map_err(|error| invalid(format!("invalid HR reassignment aggregate: {error}")))?;
            Ok(
                stable_reassignment_id(holding_ref, &reassignment.workflow_ref)
                    == event.aggregate_id
                    && reassignments
                        .get(&reassignment.workflow_ref)
                        .is_some_and(|projected| {
                            projected.workflow_ref == reassignment.workflow_ref
                                && projected.holding_id == reassignment.scope.holding_id
                                && projected.company_id == reassignment.scope.company_id
                                && projected.branch_id == reassignment.scope.branch_id
                                && projected.expired_principal_ref
                                    == reassignment.expired_principal_ref
                                && projected.replacement_principal_ref
                                    == reassignment.replacement_principal_ref
                                && projected.reason == reassignment.reason
                                && projected.reassigned_at == reassignment.reassigned_at
                                && projected.source_version == event.aggregate_version
                                && projected.source_event_id == event.event_id
                        }),
            )
        }
        _ => Err(invalid("HR aggregate type is unsupported")),
    }
}

fn expected_projection_row_count(event: &HrSourceEvent) -> Result<u64, PersistenceError> {
    let expected = match event.aggregate_type.as_str() {
        "hr_employee" => {
            let employee: EmployeeRecord = aggregate(event)?;
            employee
                .validate()
                .map_err(|error| invalid(format!("invalid HR employee aggregate: {error}")))?;
            1
        }
        "hr_leave_request" => {
            let leave: LeaveRequest = aggregate(event)?;
            leave
                .validate()
                .map_err(|error| invalid(format!("invalid HR leave aggregate: {error}")))?;
            1
        }
        "hr_expense_claim" => {
            let expense: ExpenseClaim = aggregate(event)?;
            expense
                .validate()
                .map_err(|error| invalid(format!("invalid HR expense aggregate: {error}")))?;
            1
        }
        "hr_payroll_run" => {
            let payroll: PayrollRun = aggregate(event)?;
            payroll
                .validate()
                .map_err(|error| invalid(format!("invalid HR payroll aggregate: {error}")))?;
            1_u64
                .checked_add(
                    u64::try_from(payroll.lines.len())
                        .map_err(|_| invalid("HR payroll line count is not representable"))?,
                )
                .ok_or_else(|| invalid("HR payroll projection row count overflow"))?
        }
        "hr_expired_role_reassignment" => {
            let reassignment: ExpiredRoleReassignment = aggregate(event)?;
            reassignment
                .validate()
                .map_err(|error| invalid(format!("invalid HR reassignment aggregate: {error}")))?;
            1
        }
        _ => return Err(invalid("HR aggregate type is unsupported")),
    };
    Ok(expected)
}

async fn project_employee(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
    event: &HrSourceEvent,
) -> Result<(), PersistenceError> {
    let employee: EmployeeRecord = aggregate(event)?;
    employee
        .validate()
        .map_err(|error| invalid(format!("invalid HR employee aggregate: {error}")))?;
    if employee.id != event.aggregate_id {
        return Err(invalid("HR employee event identity is invalid"));
    }
    validate_existing_scope(
        tx,
        "hr_employee_read_rows",
        "employee_id",
        employee.id,
        holding_ref,
        &employee.scope,
        "HR employee",
    )
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.hr_employee_read_rows
            (holding_ref, employee_id, holding_id, company_id, branch_id,
             employee_ref, display_name, status, hired_on, terminated_on,
             source_version, source_event_id, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now())
         ON CONFLICT (holding_ref, employee_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, employee_ref=EXCLUDED.employee_ref,
             display_name=EXCLUDED.display_name, status=EXCLUDED.status,
             hired_on=EXCLUDED.hired_on, terminated_on=EXCLUDED.terminated_on,
             source_version=EXCLUDED.source_version,
             source_event_id=EXCLUDED.source_event_id, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(employee.id)
    .bind(employee.scope.holding_id)
    .bind(employee.scope.company_id)
    .bind(employee.scope.branch_id)
    .bind(employee.employee_ref)
    .bind(employee.display_name)
    .bind(employee_status(employee.status))
    .bind(employee.hired_on)
    .bind(employee.terminated_on)
    .bind(as_i64(event.aggregate_version, "HR source version")?)
    .bind(event.event_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn project_leave(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
    event: &HrSourceEvent,
) -> Result<(), PersistenceError> {
    let leave: LeaveRequest = aggregate(event)?;
    leave
        .validate()
        .map_err(|error| invalid(format!("invalid HR leave aggregate: {error}")))?;
    if leave.id != event.aggregate_id {
        return Err(invalid("HR leave event identity is invalid"));
    }
    validate_existing_scope(
        tx,
        "hr_leave_request_read_rows",
        "leave_id",
        leave.id,
        holding_ref,
        &leave.scope,
        "HR leave request",
    )
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.hr_leave_request_read_rows
            (holding_ref, leave_id, holding_id, company_id, branch_id, employee_id,
             starts_on, ends_on, reason, status, requested_by, approved_by,
             source_version, source_event_id, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now())
         ON CONFLICT (holding_ref, leave_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, employee_id=EXCLUDED.employee_id,
             starts_on=EXCLUDED.starts_on, ends_on=EXCLUDED.ends_on,
             reason=EXCLUDED.reason, status=EXCLUDED.status,
             requested_by=EXCLUDED.requested_by, approved_by=EXCLUDED.approved_by,
             source_version=EXCLUDED.source_version, source_event_id=EXCLUDED.source_event_id,
             projected_at=now()",
    )
    .bind(holding_ref)
    .bind(leave.id)
    .bind(leave.scope.holding_id)
    .bind(leave.scope.company_id)
    .bind(leave.scope.branch_id)
    .bind(leave.employee_id)
    .bind(leave.starts_on)
    .bind(leave.ends_on)
    .bind(leave.reason)
    .bind(leave_status(leave.status))
    .bind(leave.requested_by)
    .bind(leave.approved_by)
    .bind(as_i64(event.aggregate_version, "HR source version")?)
    .bind(event.event_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn project_expense(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
    event: &HrSourceEvent,
) -> Result<(), PersistenceError> {
    let expense: ExpenseClaim = aggregate(event)?;
    expense
        .validate()
        .map_err(|error| invalid(format!("invalid HR expense aggregate: {error}")))?;
    if expense.id != event.aggregate_id {
        return Err(invalid("HR expense event identity is invalid"));
    }
    validate_existing_scope(
        tx,
        "hr_expense_claim_read_rows",
        "expense_id",
        expense.id,
        holding_ref,
        &expense.scope,
        "HR expense claim",
    )
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.hr_expense_claim_read_rows
            (holding_ref, expense_id, holding_id, company_id, branch_id, employee_id,
             currency, amount_minor, status, submitted_by, approved_by,
             source_version, source_event_id, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now())
         ON CONFLICT (holding_ref, expense_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, employee_id=EXCLUDED.employee_id,
             currency=EXCLUDED.currency, amount_minor=EXCLUDED.amount_minor,
             status=EXCLUDED.status, submitted_by=EXCLUDED.submitted_by,
             approved_by=EXCLUDED.approved_by, source_version=EXCLUDED.source_version,
             source_event_id=EXCLUDED.source_event_id, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(expense.id)
    .bind(expense.scope.holding_id)
    .bind(expense.scope.company_id)
    .bind(expense.scope.branch_id)
    .bind(expense.employee_id)
    .bind(expense.currency)
    .bind(expense.amount_minor)
    .bind(expense_status(expense.status))
    .bind(expense.submitted_by)
    .bind(expense.approved_by)
    .bind(as_i64(event.aggregate_version, "HR source version")?)
    .bind(event.event_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn project_payroll(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
    event: &HrSourceEvent,
) -> Result<(), PersistenceError> {
    let payroll: PayrollRun = aggregate(event)?;
    payroll
        .validate()
        .map_err(|error| invalid(format!("invalid HR payroll aggregate: {error}")))?;
    if payroll.id != event.aggregate_id {
        return Err(invalid("HR payroll event identity is invalid"));
    }
    validate_existing_scope(
        tx,
        "hr_payroll_run_read_rows",
        "payroll_id",
        payroll.id,
        holding_ref,
        &payroll.scope,
        "HR payroll run",
    )
    .await?;
    sqlx::query(
        "DELETE FROM erpx_tenant.hr_payroll_line_read_rows
         WHERE holding_ref=$1 AND payroll_id=$2",
    )
    .bind(holding_ref)
    .bind(payroll.id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.hr_payroll_run_read_rows
            (holding_ref, payroll_id, holding_id, company_id, branch_id, starts_on,
             ends_on, status, prepared_by, reviewed_by, approved_by,
             total_gross_minor, total_deductions_minor, total_net_minor,
             source_version, source_event_id, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now())
         ON CONFLICT (holding_ref, payroll_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, starts_on=EXCLUDED.starts_on,
             ends_on=EXCLUDED.ends_on, status=EXCLUDED.status,
             prepared_by=EXCLUDED.prepared_by, reviewed_by=EXCLUDED.reviewed_by,
             approved_by=EXCLUDED.approved_by, total_gross_minor=EXCLUDED.total_gross_minor,
             total_deductions_minor=EXCLUDED.total_deductions_minor,
             total_net_minor=EXCLUDED.total_net_minor, source_version=EXCLUDED.source_version,
             source_event_id=EXCLUDED.source_event_id, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(payroll.id)
    .bind(payroll.scope.holding_id)
    .bind(payroll.scope.company_id)
    .bind(payroll.scope.branch_id)
    .bind(payroll.starts_on)
    .bind(payroll.ends_on)
    .bind(payroll_status(payroll.status))
    .bind(payroll.prepared_by)
    .bind(payroll.reviewed_by)
    .bind(payroll.approved_by)
    .bind(payroll.total_gross_minor)
    .bind(payroll.total_deductions_minor)
    .bind(payroll.total_net_minor)
    .bind(as_i64(event.aggregate_version, "HR source version")?)
    .bind(event.event_id)
    .execute(&mut **tx)
    .await?;
    for (line_no, line) in payroll.lines.into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO erpx_tenant.hr_payroll_line_read_rows
                (holding_ref, payroll_id, line_no, holding_id, company_id, branch_id,
                 employee_id, gross_minor, deductions_minor, net_minor,
                 source_version, source_event_id, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now())",
        )
        .bind(holding_ref)
        .bind(payroll.id)
        .bind(i32::try_from(line_no).map_err(|_| invalid("HR payroll line number is invalid"))?)
        .bind(payroll.scope.holding_id)
        .bind(payroll.scope.company_id)
        .bind(payroll.scope.branch_id)
        .bind(line.employee_id)
        .bind(line.gross_minor)
        .bind(line.deductions_minor)
        .bind(line.net_minor)
        .bind(as_i64(event.aggregate_version, "HR source version")?)
        .bind(event.event_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn project_reassignment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    holding_ref: &str,
    event: &HrSourceEvent,
) -> Result<(), PersistenceError> {
    let reassignment: ExpiredRoleReassignment = aggregate(event)?;
    reassignment
        .validate()
        .map_err(|error| invalid(format!("invalid HR reassignment aggregate: {error}")))?;
    if stable_reassignment_id(holding_ref, &reassignment.workflow_ref) != event.aggregate_id {
        return Err(invalid("HR reassignment event identity is invalid"));
    }
    validate_existing_scope_by_workflow(
        tx,
        "hr_expired_role_reassignment_read_rows",
        "workflow_ref",
        &reassignment.workflow_ref,
        holding_ref,
        &reassignment.scope,
        "HR role reassignment",
    )
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.hr_expired_role_reassignment_read_rows
            (holding_ref, workflow_ref, holding_id, company_id, branch_id,
             expired_principal_ref, replacement_principal_ref, reason, reassigned_at,
             source_version, source_event_id, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now())
         ON CONFLICT (holding_ref, workflow_ref) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, expired_principal_ref=EXCLUDED.expired_principal_ref,
             replacement_principal_ref=EXCLUDED.replacement_principal_ref,
             reason=EXCLUDED.reason, reassigned_at=EXCLUDED.reassigned_at,
             source_version=EXCLUDED.source_version, source_event_id=EXCLUDED.source_event_id,
             projected_at=now()",
    )
    .bind(holding_ref)
    .bind(reassignment.workflow_ref)
    .bind(reassignment.scope.holding_id)
    .bind(reassignment.scope.company_id)
    .bind(reassignment.scope.branch_id)
    .bind(reassignment.expired_principal_ref)
    .bind(reassignment.replacement_principal_ref)
    .bind(reassignment.reason)
    .bind(reassignment.reassigned_at)
    .bind(as_i64(event.aggregate_version, "HR source version")?)
    .bind(event.event_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn aggregate<T: serde::de::DeserializeOwned>(event: &HrSourceEvent) -> Result<T, PersistenceError> {
    let value = event
        .payload
        .get("aggregate")
        .cloned()
        .ok_or_else(|| invalid("HR source aggregate is missing"))?;
    serde_json::from_value(value).map_err(PersistenceError::from)
}

async fn validate_existing_scope(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
    id_column: &str,
    aggregate_id: Uuid,
    holding_ref: &str,
    requested_scope: &EmploymentScope,
    entity: &'static str,
) -> Result<(), PersistenceError> {
    let query = format!(
        "SELECT holding_id, company_id, branch_id
         FROM erpx_tenant.{table}
         WHERE holding_ref=$1 AND {id_column}=$2 FOR SHARE"
    );
    let row = sqlx::query(&query)
        .bind(holding_ref)
        .bind(aggregate_id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(row) = row
        && (row.try_get::<Uuid, _>("holding_id")? != requested_scope.holding_id
            || row.try_get::<Uuid, _>("company_id")? != requested_scope.company_id
            || row.try_get::<Option<Uuid>, _>("branch_id")? != requested_scope.branch_id)
    {
        return Err(invalid(format!("{entity} scope cannot be rebound")));
    }
    Ok(())
}

async fn validate_existing_scope_by_workflow(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &str,
    id_column: &str,
    workflow_ref: &str,
    holding_ref: &str,
    requested_scope: &EmploymentScope,
    entity: &'static str,
) -> Result<(), PersistenceError> {
    let query = format!(
        "SELECT holding_id, company_id, branch_id
         FROM erpx_tenant.{table}
         WHERE holding_ref=$1 AND {id_column}=$2 FOR SHARE"
    );
    let row = sqlx::query(&query)
        .bind(holding_ref)
        .bind(workflow_ref)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(row) = row
        && (row.try_get::<Uuid, _>("holding_id")? != requested_scope.holding_id
            || row.try_get::<Uuid, _>("company_id")? != requested_scope.company_id
            || row.try_get::<Option<Uuid>, _>("branch_id")? != requested_scope.branch_id)
    {
        return Err(invalid(format!("{entity} scope cannot be rebound")));
    }
    Ok(())
}

fn event_scope(event: &HrSourceEvent) -> Result<(Uuid, Uuid, Option<Uuid>), PersistenceError> {
    match event.aggregate_type.as_str() {
        "hr_employee" => {
            let value: EmployeeRecord = aggregate(event)?;
            Ok((
                value.scope.holding_id,
                value.scope.company_id,
                value.scope.branch_id,
            ))
        }
        "hr_leave_request" => {
            let value: LeaveRequest = aggregate(event)?;
            Ok((
                value.scope.holding_id,
                value.scope.company_id,
                value.scope.branch_id,
            ))
        }
        "hr_expense_claim" => {
            let value: ExpenseClaim = aggregate(event)?;
            Ok((
                value.scope.holding_id,
                value.scope.company_id,
                value.scope.branch_id,
            ))
        }
        "hr_payroll_run" => {
            let value: PayrollRun = aggregate(event)?;
            Ok((
                value.scope.holding_id,
                value.scope.company_id,
                value.scope.branch_id,
            ))
        }
        "hr_expired_role_reassignment" => {
            let value: ExpiredRoleReassignment = aggregate(event)?;
            Ok((
                value.scope.holding_id,
                value.scope.company_id,
                value.scope.branch_id,
            ))
        }
        _ => Err(invalid("HR aggregate scope is unsupported")),
    }
}

fn payload_version(payload: &Value) -> Result<u64, PersistenceError> {
    payload
        .get("version")
        .and_then(Value::as_u64)
        .filter(|version| *version > 0)
        .ok_or_else(|| invalid("HR source payload version is invalid"))
}

fn stable_reassignment_id(holding_ref: &str, workflow_ref: &str) -> Uuid {
    let digest = Sha256::digest(
        format!("hr_expired_role_reassignment:{holding_ref}:{workflow_ref}").as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    let mut id = Uuid::from_bytes(bytes);
    if id.is_nil() {
        id = Uuid::from_u128(1);
    }
    id
}

fn application(event: &HrSourceEvent, applied: bool) -> HrProjectionApplication {
    HrProjectionApplication {
        event_id: event.event_id,
        aggregate_type: event.aggregate_type.clone(),
        aggregate_id: event.aggregate_id,
        aggregate_version: event.aggregate_version,
        applied,
    }
}

fn validate_cursor(cursor: &HrSourceEventCursor) -> Result<(), PersistenceError> {
    if !HR_AGGREGATES.contains(&cursor.aggregate_type.as_str())
        || cursor.aggregate_id.is_nil()
        || cursor.aggregate_version == 0
    {
        return Err(invalid("HR source cursor is invalid"));
    }
    Ok(())
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn as_i64(value: u64, field: &str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| invalid(format!("{field} is negative")))
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

fn employee_status(status: EmploymentStatus) -> &'static str {
    match status {
        EmploymentStatus::Draft => "draft",
        EmploymentStatus::Active => "active",
        EmploymentStatus::OnLeave => "on_leave",
        EmploymentStatus::Suspended => "suspended",
        EmploymentStatus::Terminated => "terminated",
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

fn expense_status(status: ExpenseStatus) -> &'static str {
    match status {
        ExpenseStatus::Draft => "draft",
        ExpenseStatus::Submitted => "submitted",
        ExpenseStatus::Approved => "approved",
        ExpenseStatus::Rejected => "rejected",
        ExpenseStatus::Reimbursed => "reimbursed",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_allow_list_and_cursor_are_closed() {
        assert!(
            validate_cursor(&HrSourceEventCursor {
                aggregate_type: "finance_journal".into(),
                aggregate_id: Uuid::new_v4(),
                aggregate_version: 1,
            })
            .is_err()
        );
        assert!(
            validate_cursor(&HrSourceEventCursor {
                aggregate_type: "hr_employee".into(),
                aggregate_id: Uuid::new_v4(),
                aggregate_version: 1,
            })
            .is_ok()
        );
    }

    #[test]
    fn source_payload_version_is_required_and_redacted() {
        assert!(payload_version(&json!({"version": 0})).is_err());
        let error = payload_version(&json!({"aggregate": {}})).unwrap_err();
        assert!(!error.to_string().contains("aggregate"));
    }

    #[test]
    fn source_event_validation_is_typed_and_fail_closed() {
        let valid = HrSourceEvent {
            event_id: Uuid::from_u128(6),
            event_type: "hr.employee_saved.v1".into(),
            aggregate_type: "hr_employee".into(),
            aggregate_id: Uuid::from_u128(4),
            aggregate_version: 1,
            payload: json!({
                "aggregate_type": "hr_employee",
                "aggregate": {},
                "version": 1,
                "event_type": "hr.employee_saved.v1"
            }),
            occurred_at: Utc::now(),
        };
        assert!(validate_source_event(&valid).is_ok());

        let mut wrong_type = valid.clone();
        wrong_type.event_type = "hr.expense_saved.v1".into();
        assert!(validate_source_event(&wrong_type).is_err());

        let mut malformed_payload = valid.clone();
        malformed_payload.payload = json!({"version": 1});
        assert!(validate_source_event(&malformed_payload).is_err());

        let mut mismatched_envelope = HrSourceEvent {
            payload: json!({
                "aggregate_type": "hr_leave_request",
                "aggregate": {},
                "version": 1,
                "event_type": "hr.employee_saved.v1"
            }),
            ..valid
        };
        assert!(validate_source_event(&mismatched_envelope).is_err());
        mismatched_envelope.payload = json!({
            "aggregate_type": "hr_employee",
            "aggregate": {},
            "version": 1,
            "event_type": "hr.leave_saved.v1"
        });
        assert!(validate_source_event(&mismatched_envelope).is_err());
    }

    #[test]
    fn reassignment_identity_is_stable_and_non_nil() {
        let first = stable_reassignment_id("holding-a", "workflow-1");
        assert_eq!(first, stable_reassignment_id("holding-a", "workflow-1"));
        assert_ne!(first, stable_reassignment_id("holding-b", "workflow-1"));
        assert!(!first.is_nil());
    }

    #[test]
    fn employee_reconciliation_compares_authoritative_payload_fields() {
        let scope = erpx_domain::hr::EmploymentScope::new(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Some(Uuid::from_u128(3)),
        )
        .unwrap();
        let employee = EmployeeRecord {
            id: Uuid::from_u128(4),
            employee_ref: "EMP-001".into(),
            display_name: "Authoritative Employee".into(),
            scope,
            status: EmploymentStatus::Active,
            hired_on: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            terminated_on: None,
            version: 1,
            last_command_id: Uuid::from_u128(5),
            last_idempotency_key: "hr-test-1".into(),
        };
        let event = HrSourceEvent {
            event_id: Uuid::from_u128(6),
            event_type: "hr.employee_saved.v1".into(),
            aggregate_type: "hr_employee".into(),
            aggregate_id: employee.id,
            aggregate_version: employee.version,
            payload: json!({}),
            occurred_at: Utc::now(),
        };
        let projected = ProjectedEmployeeFact {
            employee_id: employee.id,
            holding_id: employee.scope.holding_id,
            company_id: employee.scope.company_id,
            branch_id: employee.scope.branch_id,
            employee_ref: employee.employee_ref.clone(),
            display_name: employee.display_name.clone(),
            status: employee_status(employee.status).into(),
            hired_on: employee.hired_on,
            terminated_on: employee.terminated_on,
            source_version: employee.version,
            source_event_id: event.event_id,
        };
        assert!(employee_projection_matches_source(
            &employee,
            &event,
            Some(&projected)
        ));

        let mut tampered = projected.clone();
        tampered.display_name = "Tampered Employee".into();
        assert!(!employee_projection_matches_source(
            &employee,
            &event,
            Some(&tampered)
        ));
    }

    #[test]
    fn read_scope_and_page_bounds_fail_closed_before_database_access() {
        let valid_scope = HrReadScope {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Some(Uuid::from_u128(3)),
        };
        assert!(validate_hr_read_request(valid_scope, 1).is_ok());
        assert!(validate_hr_read_request(valid_scope, 0).is_err());
        assert!(validate_hr_read_request(valid_scope, MAX_HR_READ_PAGE_SIZE + 1).is_err());
        assert!(
            validate_hr_read_request(
                HrReadScope {
                    holding_id: Uuid::nil(),
                    ..valid_scope
                },
                1
            )
            .is_err()
        );
        assert!(
            validate_hr_read_request(
                HrReadScope {
                    branch_id: Some(Uuid::nil()),
                    ..valid_scope
                },
                1
            )
            .is_err()
        );
        assert_eq!(
            page_len(MAX_HR_READ_PAGE_SIZE).unwrap(),
            MAX_HR_READ_PAGE_SIZE as usize
        );
    }

    #[test]
    fn read_cursors_reject_nil_and_invalid_opaque_values() {
        assert!(
            validate_leave_cursor(&HrLeaveReadCursor {
                starts_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                leave_id: Uuid::nil(),
            })
            .is_err()
        );
        assert!(
            validate_expense_cursor(&HrExpenseReadCursor {
                expense_id: Uuid::nil(),
            })
            .is_err()
        );
        assert!(
            validate_payroll_cursor(&HrPayrollRunReadCursor {
                starts_on: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                payroll_id: Uuid::nil(),
            })
            .is_err()
        );
        assert!(
            validate_reassignment_cursor(&HrReassignmentReadCursor {
                reassigned_at: Utc::now(),
                workflow_ref: "  ".into(),
            })
            .is_err()
        );
        assert!(
            validate_reassignment_cursor(&HrReassignmentReadCursor {
                reassigned_at: Utc::now(),
                workflow_ref: "bad\nref".into(),
            })
            .is_err()
        );
        assert!(
            validate_reassignment_cursor(&HrReassignmentReadCursor {
                reassigned_at: Utc::now(),
                workflow_ref: "x".repeat(129),
            })
            .is_err()
        );
    }

    #[test]
    fn payroll_line_bounds_are_explicit() {
        assert!(validate_payroll_line_page_size(1).is_ok());
        assert!(validate_payroll_line_page_size(MAX_HR_READ_PAYROLL_LINES_PER_RUN).is_ok());
        assert!(validate_payroll_line_page_size(0).is_err());
        assert!(validate_payroll_line_page_size(MAX_HR_READ_PAYROLL_LINES_PER_RUN + 1).is_err());
    }
}
