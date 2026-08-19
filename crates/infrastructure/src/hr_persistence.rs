//! Holding-write SQLx persistence for HR and payroll aggregates.
//!
//! The adapter stores bounded, typed source fields in the holding write
//! database.  Payroll lines are normalized child rows rather than a serialized
//! aggregate payload.  A small internal idempotency ledger and the existing
//! holding outbox are committed in the same transaction as each source change.
//! No read-model or transport type is exposed from this module.

use chrono::NaiveDate;
use erpx_domain::hr::{
    EmployeeRecord, EmploymentScope, EmploymentStatus, ExpenseClaim, ExpenseStatus,
    ExpiredRoleReassignment, HR_CONTRACT_VERSION, LeaveRequest, LeaveStatus, MAX_HR_NAME_BYTES,
    MAX_HR_REASON_BYTES, MAX_HR_REFERENCE_BYTES, MAX_PAYROLL_LINES, PayrollLine, PayrollRun,
    PayrollStatus,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

const MAX_IDEMPOTENCY_KEY_BYTES: usize = 255;
const MAX_DATA_PROFILE_KEY_BYTES: usize = 255;

const EMPLOYEE_AGGREGATE: &str = "hr_employee";
const LEAVE_AGGREGATE: &str = "hr_leave_request";
const EXPENSE_AGGREGATE: &str = "hr_expense_claim";
const PAYROLL_AGGREGATE: &str = "hr_payroll_run";
const REASSIGNMENT_AGGREGATE: &str = "hr_expired_role_reassignment";
const HR_EVENT_VERSION: u16 = 1;

/// The typed result of an accepted or replayed HR source mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub idempotency_key: String,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxHrPersistence {
    write_pool: PgPool,
    data_profile_key: String,
}

impl SqlxHrPersistence {
    pub fn new(
        write_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool,
            data_profile_key,
        })
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    /// Persist an EmployeeRecord with source state, idempotency, and outbox
    /// delivery atomically.
    pub async fn save_employee(
        &self,
        employee: &EmployeeRecord,
        expected_version: Option<u64>,
    ) -> Result<HrWriteReceipt, PersistenceError> {
        validate_employee(employee)?;
        let key = employee.last_idempotency_key.as_str();
        validate_idempotency_key(key)?;
        let aggregate_id = employee.id;
        let request_hash =
            request_hash(EMPLOYEE_AGGREGATE, aggregate_id, employee, expected_version)?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) =
            find_replay(&mut tx, &self.data_profile_key, key, &request_hash).await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let current = current_version(
            &mut tx,
            "hr_employee_records",
            "employee_id",
            aggregate_id,
            &self.data_profile_key,
        )
        .await?;
        ensure_source_event_continuity(
            &mut tx,
            &self.data_profile_key,
            EMPLOYEE_AGGREGATE,
            aggregate_id,
            current,
        )
        .await?;
        ensure_expected_version(EMPLOYEE_AGGREGATE, aggregate_id, expected_version, current)?;
        let next_version = next_version(current)?;
        ensure_domain_version(
            EMPLOYEE_AGGREGATE,
            aggregate_id,
            employee.version,
            next_version,
        )?;

        if let Some(current_version) = current {
            let affected = sqlx::query(
                "UPDATE erpx_holding.hr_employee_records
                 SET holding_id=$3, company_id=$4, branch_id=$5, employee_ref=$6,
                     display_name=$7, status=$8, hired_on=$9, terminated_on=$10,
                     version=$11, command_id=$12, idempotency_key=$13, updated_at=now()
                 WHERE data_profile_key=$1 AND employee_id=$2 AND version=$14",
            )
            .bind(&self.data_profile_key)
            .bind(employee.id)
            .bind(employee.scope.holding_id)
            .bind(employee.scope.company_id)
            .bind(employee.scope.branch_id)
            .bind(&employee.employee_ref)
            .bind(&employee.display_name)
            .bind(employee_status(employee.status))
            .bind(employee.hired_on)
            .bind(employee.terminated_on)
            .bind(as_i64(next_version, "employee version")?)
            .bind(employee.last_command_id)
            .bind(key)
            .bind(as_i64(current_version, "employee version")?)
            .execute(&mut *tx)
            .await?;
            ensure_update_affected(
                EMPLOYEE_AGGREGATE,
                aggregate_id,
                current_version,
                affected.rows_affected(),
            )?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.hr_employee_records
                    (data_profile_key, employee_id, holding_id, company_id, branch_id,
                     employee_ref, display_name, status, hired_on, terminated_on,
                     version, command_id, idempotency_key)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(&self.data_profile_key)
            .bind(employee.id)
            .bind(employee.scope.holding_id)
            .bind(employee.scope.company_id)
            .bind(employee.scope.branch_id)
            .bind(&employee.employee_ref)
            .bind(&employee.display_name)
            .bind(employee_status(employee.status))
            .bind(employee.hired_on)
            .bind(employee.terminated_on)
            .bind(as_i64(next_version, "employee version")?)
            .bind(employee.last_command_id)
            .bind(key)
            .execute(&mut *tx)
            .await?;
        }
        let receipt = HrWriteReceipt {
            aggregate_type: EMPLOYEE_AGGREGATE.into(),
            aggregate_id,
            version: next_version,
            idempotency_key: key.into(),
            replayed: false,
        };
        append_hr_event(
            &mut tx,
            &self.data_profile_key,
            EMPLOYEE_AGGREGATE,
            aggregate_id,
            next_version,
            "hr.employee_saved.v1",
            &employee.scope,
            Some(employee.last_command_id),
            key,
            employee,
        )
        .await?;
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Apply one domain-approved employee lifecycle transition against the
    /// authoritative holding-write row, then reuse the atomic employee writer
    /// for source state, outbox, and idempotency persistence.
    pub async fn transition_employee(
        &self,
        employee_id: Uuid,
        transition: &str,
        terminated_on: Option<NaiveDate>,
        expected_version: u64,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<HrWriteReceipt, PersistenceError> {
        validate_uuid(employee_id, "employee_id")?;
        validate_positive_version(expected_version, "employee transition expected version")?;
        let transition = transition.trim();
        if transition.is_empty()
            || transition.len() > 64
            || transition.chars().any(char::is_control)
        {
            return Err(invalid("employee transition is outside its bound"));
        }
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT holding_id, company_id, branch_id, employee_ref, display_name,
                    status, hired_on, terminated_on, version, command_id, idempotency_key
               FROM erpx_holding.hr_employee_records
              WHERE data_profile_key=$1 AND employee_id=$2
              FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(employee_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else {
            return Err(PersistenceError::NotFound {
                entity: "HR employee",
                id: employee_id,
            });
        };
        let actual_version = as_u64(row.try_get("version")?, "employee version")?;
        if actual_version != expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: EMPLOYEE_AGGREGATE.into(),
                aggregate_id: employee_id,
                expected: Some(expected_version),
                actual: Some(actual_version),
            });
        }
        let employee = EmployeeRecord {
            id: employee_id,
            employee_ref: row.try_get("employee_ref")?,
            display_name: row.try_get("display_name")?,
            scope: EmploymentScope::new(
                row.try_get("holding_id")?,
                row.try_get("company_id")?,
                row.try_get("branch_id")?,
            )
            .map_err(|error| invalid(error.to_string()))?,
            status: parse_employee_status(row.try_get::<String, _>("status")?)?,
            hired_on: row.try_get("hired_on")?,
            terminated_on: row.try_get("terminated_on")?,
            version: actual_version,
            last_command_id: row.try_get("command_id")?,
            last_idempotency_key: row.try_get("idempotency_key")?,
        };
        employee
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        let next = parse_employee_status(transition)?;
        let mut employee = employee;
        employee
            .transition(next, terminated_on, context)
            .map_err(|error| invalid(error.to_string()))?;
        self.save_employee(&employee, Some(expected_version)).await
    }

    /// Persist a LeaveRequest.  LeaveRequest intentionally has no idempotency
    /// field; its key is supplied by the command boundary and bound to the
    /// request hash in the write database.
    pub async fn save_leave(
        &self,
        leave: &LeaveRequest,
        idempotency_key: &str,
        expected_version: Option<u64>,
    ) -> Result<HrWriteReceipt, PersistenceError> {
        validate_leave(leave)?;
        validate_idempotency_key(idempotency_key)?;
        let aggregate_id = leave.id;
        let request_hash = request_hash(LEAVE_AGGREGATE, aggregate_id, leave, expected_version)?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        ensure_employee_reference(
            &mut tx,
            &self.data_profile_key,
            leave.employee_id,
            &leave.scope,
        )
        .await?;
        let current = current_version(
            &mut tx,
            "hr_leave_requests",
            "leave_id",
            aggregate_id,
            &self.data_profile_key,
        )
        .await?;
        ensure_source_event_continuity(
            &mut tx,
            &self.data_profile_key,
            LEAVE_AGGREGATE,
            aggregate_id,
            current,
        )
        .await?;
        ensure_expected_version(LEAVE_AGGREGATE, aggregate_id, expected_version, current)?;
        let next_version = next_version(current)?;
        ensure_domain_version(LEAVE_AGGREGATE, aggregate_id, leave.version, next_version)?;
        if let Some(current_version) = current {
            sqlx::query(
                "UPDATE erpx_holding.hr_leave_requests
                 SET holding_id=$3, company_id=$4, branch_id=$5, employee_id=$6,
                     starts_on=$7, ends_on=$8, reason=$9, status=$10, requested_by=$11,
                     approved_by=$12, version=$13, command_id=$14, idempotency_key=$15,
                     updated_at=now()
                 WHERE data_profile_key=$1 AND leave_id=$2 AND version=$16",
            )
            .bind(&self.data_profile_key)
            .bind(leave.id)
            .bind(leave.scope.holding_id)
            .bind(leave.scope.company_id)
            .bind(leave.scope.branch_id)
            .bind(leave.employee_id)
            .bind(leave.starts_on)
            .bind(leave.ends_on)
            .bind(&leave.reason)
            .bind(leave_status(leave.status))
            .bind(&leave.requested_by)
            .bind(&leave.approved_by)
            .bind(as_i64(next_version, "leave version")?)
            .bind(leave.last_command_id)
            .bind(idempotency_key)
            .bind(as_i64(current_version, "leave version")?)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.hr_leave_requests
                    (data_profile_key, leave_id, holding_id, company_id, branch_id, employee_id,
                     starts_on, ends_on, reason, status, requested_by, approved_by, version,
                     command_id, idempotency_key)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
            )
            .bind(&self.data_profile_key)
            .bind(leave.id)
            .bind(leave.scope.holding_id)
            .bind(leave.scope.company_id)
            .bind(leave.scope.branch_id)
            .bind(leave.employee_id)
            .bind(leave.starts_on)
            .bind(leave.ends_on)
            .bind(&leave.reason)
            .bind(leave_status(leave.status))
            .bind(&leave.requested_by)
            .bind(&leave.approved_by)
            .bind(as_i64(next_version, "leave version")?)
            .bind(leave.last_command_id)
            .bind(idempotency_key)
            .execute(&mut *tx)
            .await?;
        }
        let receipt = receipt(LEAVE_AGGREGATE, aggregate_id, next_version, idempotency_key);
        append_hr_event(
            &mut tx,
            &self.data_profile_key,
            LEAVE_AGGREGATE,
            aggregate_id,
            next_version,
            "hr.leave_saved.v1",
            &leave.scope,
            Some(leave.last_command_id),
            idempotency_key,
            leave,
        )
        .await?;
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Persist an ExpenseClaim with integer minor-unit amounts only.
    pub async fn save_expense(
        &self,
        expense: &ExpenseClaim,
        idempotency_key: &str,
        expected_version: Option<u64>,
    ) -> Result<HrWriteReceipt, PersistenceError> {
        validate_expense(expense)?;
        validate_idempotency_key(idempotency_key)?;
        let aggregate_id = expense.id;
        let request_hash =
            request_hash(EXPENSE_AGGREGATE, aggregate_id, expense, expected_version)?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        ensure_employee_reference(
            &mut tx,
            &self.data_profile_key,
            expense.employee_id,
            &expense.scope,
        )
        .await?;
        let current = current_version(
            &mut tx,
            "hr_expense_claims",
            "expense_id",
            aggregate_id,
            &self.data_profile_key,
        )
        .await?;
        ensure_source_event_continuity(
            &mut tx,
            &self.data_profile_key,
            EXPENSE_AGGREGATE,
            aggregate_id,
            current,
        )
        .await?;
        ensure_expected_version(EXPENSE_AGGREGATE, aggregate_id, expected_version, current)?;
        let next_version = next_version(current)?;
        ensure_domain_version(
            EXPENSE_AGGREGATE,
            aggregate_id,
            expense.version,
            next_version,
        )?;
        if let Some(current_version) = current {
            sqlx::query(
                "UPDATE erpx_holding.hr_expense_claims
                 SET holding_id=$3, company_id=$4, branch_id=$5, employee_id=$6,
                     currency=$7, amount_minor=$8, status=$9, submitted_by=$10,
                     approved_by=$11, version=$12, idempotency_key=$13, updated_at=now()
                 WHERE data_profile_key=$1 AND expense_id=$2 AND version=$14",
            )
            .bind(&self.data_profile_key)
            .bind(expense.id)
            .bind(expense.scope.holding_id)
            .bind(expense.scope.company_id)
            .bind(expense.scope.branch_id)
            .bind(expense.employee_id)
            .bind(&expense.currency)
            .bind(expense.amount_minor)
            .bind(expense_status(expense.status))
            .bind(&expense.submitted_by)
            .bind(&expense.approved_by)
            .bind(as_i64(next_version, "expense version")?)
            .bind(idempotency_key)
            .bind(as_i64(current_version, "expense version")?)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.hr_expense_claims
                    (data_profile_key, expense_id, holding_id, company_id, branch_id, employee_id,
                     currency, amount_minor, status, submitted_by, approved_by, version,
                     idempotency_key)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(&self.data_profile_key)
            .bind(expense.id)
            .bind(expense.scope.holding_id)
            .bind(expense.scope.company_id)
            .bind(expense.scope.branch_id)
            .bind(expense.employee_id)
            .bind(&expense.currency)
            .bind(expense.amount_minor)
            .bind(expense_status(expense.status))
            .bind(&expense.submitted_by)
            .bind(&expense.approved_by)
            .bind(as_i64(next_version, "expense version")?)
            .bind(idempotency_key)
            .execute(&mut *tx)
            .await?;
        }
        let receipt = receipt(
            EXPENSE_AGGREGATE,
            aggregate_id,
            next_version,
            idempotency_key,
        );
        append_hr_event(
            &mut tx,
            &self.data_profile_key,
            EXPENSE_AGGREGATE,
            aggregate_id,
            next_version,
            "hr.expense_saved.v1",
            &expense.scope,
            None,
            idempotency_key,
            expense,
        )
        .await?;
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Persist a payroll run and its normalized lines atomically.
    pub async fn save_payroll(
        &self,
        payroll: &PayrollRun,
        idempotency_key: &str,
        expected_version: Option<u64>,
    ) -> Result<HrWriteReceipt, PersistenceError> {
        validate_payroll(payroll)?;
        validate_idempotency_key(idempotency_key)?;
        let aggregate_id = payroll.id;
        let request_hash =
            request_hash(PAYROLL_AGGREGATE, aggregate_id, payroll, expected_version)?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        ensure_payroll_employee_references(&mut tx, &self.data_profile_key, payroll).await?;
        let current = current_version(
            &mut tx,
            "hr_payroll_runs",
            "payroll_id",
            aggregate_id,
            &self.data_profile_key,
        )
        .await?;
        ensure_source_event_continuity(
            &mut tx,
            &self.data_profile_key,
            PAYROLL_AGGREGATE,
            aggregate_id,
            current,
        )
        .await?;
        ensure_expected_version(PAYROLL_AGGREGATE, aggregate_id, expected_version, current)?;
        let next_version = next_version(current)?;
        ensure_domain_version(
            PAYROLL_AGGREGATE,
            aggregate_id,
            payroll.version,
            next_version,
        )?;
        if let Some(current_version) = current {
            sqlx::query(
                "UPDATE erpx_holding.hr_payroll_runs
                 SET holding_id=$3, company_id=$4, branch_id=$5, starts_on=$6, ends_on=$7,
                     status=$8, prepared_by=$9, reviewed_by=$10, approved_by=$11,
                     total_gross_minor=$12, total_deductions_minor=$13, total_net_minor=$14,
                     version=$15, created_at=$16, idempotency_key=$17, updated_at=now()
                 WHERE data_profile_key=$1 AND payroll_id=$2 AND version=$18",
            )
            .bind(&self.data_profile_key)
            .bind(payroll.id)
            .bind(payroll.scope.holding_id)
            .bind(payroll.scope.company_id)
            .bind(payroll.scope.branch_id)
            .bind(payroll.starts_on)
            .bind(payroll.ends_on)
            .bind(payroll_status(payroll.status))
            .bind(&payroll.prepared_by)
            .bind(&payroll.reviewed_by)
            .bind(&payroll.approved_by)
            .bind(payroll.total_gross_minor)
            .bind(payroll.total_deductions_minor)
            .bind(payroll.total_net_minor)
            .bind(as_i64(next_version, "payroll version")?)
            .bind(payroll.created_at)
            .bind(idempotency_key)
            .bind(as_i64(current_version, "payroll version")?)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.hr_payroll_runs
                    (data_profile_key, payroll_id, holding_id, company_id, branch_id, starts_on,
                     ends_on, status, prepared_by, reviewed_by, approved_by, total_gross_minor,
                     total_deductions_minor, total_net_minor, version, created_at, idempotency_key)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
            )
            .bind(&self.data_profile_key)
            .bind(payroll.id)
            .bind(payroll.scope.holding_id)
            .bind(payroll.scope.company_id)
            .bind(payroll.scope.branch_id)
            .bind(payroll.starts_on)
            .bind(payroll.ends_on)
            .bind(payroll_status(payroll.status))
            .bind(&payroll.prepared_by)
            .bind(&payroll.reviewed_by)
            .bind(&payroll.approved_by)
            .bind(payroll.total_gross_minor)
            .bind(payroll.total_deductions_minor)
            .bind(payroll.total_net_minor)
            .bind(as_i64(next_version, "payroll version")?)
            .bind(payroll.created_at)
            .bind(idempotency_key)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "DELETE FROM erpx_holding.hr_payroll_lines
             WHERE data_profile_key=$1 AND payroll_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(payroll.id)
        .execute(&mut *tx)
        .await?;
        for (line_no, line) in payroll.lines.iter().enumerate() {
            sqlx::query(
                "INSERT INTO erpx_holding.hr_payroll_lines
                    (data_profile_key, payroll_id, line_no, employee_id, gross_minor,
                     deductions_minor, net_minor)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(&self.data_profile_key)
            .bind(payroll.id)
            .bind(i32::try_from(line_no).map_err(|_| {
                PersistenceError::InvalidData("payroll line number exceeds INTEGER".into())
            })?)
            .bind(line.employee_id)
            .bind(line.gross_minor)
            .bind(line.deductions_minor)
            .bind(line.net_minor)
            .execute(&mut *tx)
            .await?;
        }
        let receipt = receipt(
            PAYROLL_AGGREGATE,
            aggregate_id,
            next_version,
            idempotency_key,
        );
        append_hr_event(
            &mut tx,
            &self.data_profile_key,
            PAYROLL_AGGREGATE,
            aggregate_id,
            next_version,
            "hr.payroll_saved.v1",
            &payroll.scope,
            None,
            idempotency_key,
            payroll,
        )
        .await?;
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Record the immutable reassignment metadata created after an expired
    /// role.  A stable derived identifier gives the outbox a UUID without
    /// adding an identifier to the domain contract.
    pub async fn save_expired_role_reassignment(
        &self,
        reassignment: &ExpiredRoleReassignment,
        idempotency_key: &str,
    ) -> Result<HrWriteReceipt, PersistenceError> {
        validate_reassignment(reassignment)?;
        validate_idempotency_key(idempotency_key)?;
        let aggregate_id =
            stable_reassignment_id(&self.data_profile_key, &reassignment.workflow_ref);
        let request_hash = request_hash(REASSIGNMENT_AGGREGATE, aggregate_id, reassignment, None)?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM erpx_holding.hr_expired_role_reassignments
             WHERE data_profile_key=$1 AND workflow_ref=$2 FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(&reassignment.workflow_ref)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing_version) = existing {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: REASSIGNMENT_AGGREGATE.into(),
                aggregate_id,
                expected: None,
                actual: Some(as_u64(existing_version, "reassignment version")?),
            });
        }
        sqlx::query(
            "INSERT INTO erpx_holding.hr_expired_role_reassignments
                (data_profile_key, workflow_ref, holding_id, company_id, branch_id,
                 expired_principal_ref, replacement_principal_ref, reason, reassigned_at,
                 command_id, idempotency_key, version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1)",
        )
        .bind(&self.data_profile_key)
        .bind(&reassignment.workflow_ref)
        .bind(reassignment.scope.holding_id)
        .bind(reassignment.scope.company_id)
        .bind(reassignment.scope.branch_id)
        .bind(&reassignment.expired_principal_ref)
        .bind(&reassignment.replacement_principal_ref)
        .bind(&reassignment.reason)
        .bind(reassignment.reassigned_at)
        .bind(reassignment.command_id)
        .bind(idempotency_key)
        .execute(&mut *tx)
        .await?;
        let receipt = receipt(REASSIGNMENT_AGGREGATE, aggregate_id, 1, idempotency_key);
        append_hr_event(
            &mut tx,
            &self.data_profile_key,
            REASSIGNMENT_AGGREGATE,
            aggregate_id,
            1,
            "hr.expired_role_reassigned.v1",
            &reassignment.scope,
            Some(reassignment.command_id),
            idempotency_key,
            reassignment,
        )
        .await?;
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.data_profile_key).await
    }
}

#[derive(Debug)]
struct IdempotencyRecord {
    request_hash: String,
    receipt: HrWriteReceipt,
}

async fn find_replay(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    key: &str,
    request_hash: &str,
) -> Result<Option<HrWriteReceipt>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_hash, aggregate_type, aggregate_id, version
         FROM erpx_holding.hr_command_idempotency
         WHERE data_profile_key=$1 AND idempotency_key=$2 FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let stored = IdempotencyRecord {
        request_hash: row.try_get("request_hash")?,
        receipt: HrWriteReceipt {
            aggregate_type: row.try_get("aggregate_type")?,
            aggregate_id: row.try_get("aggregate_id")?,
            version: as_u64(row.try_get("version")?, "idempotency version")?,
            idempotency_key: key.into(),
            replayed: true,
        },
    };
    if stored.request_hash != request_hash {
        return Err(PersistenceError::IdempotencyConflict { key: key.into() });
    }
    Ok(Some(stored.receipt))
}

async fn record_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    key: &str,
    request_hash: &str,
    receipt: &HrWriteReceipt,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.hr_command_idempotency
            (data_profile_key, idempotency_key, request_hash, aggregate_type,
             aggregate_id, version)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(data_profile_key)
    .bind(key)
    .bind(request_hash)
    .bind(&receipt.aggregate_type)
    .bind(receipt.aggregate_id)
    .bind(as_i64(receipt.version, "idempotency version")?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Ensure a leave or expense cannot bind an employee from another company or
/// branch while keeping the employee row locked until the source mutation and
/// outbox event commit. A company-scoped employee is valid for a branch record;
/// a branch-scoped employee is valid for a company record or that same branch.
async fn ensure_employee_reference(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    employee_id: Uuid,
    record_scope: &EmploymentScope,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT holding_id, company_id, branch_id
         FROM erpx_holding.hr_employee_records
         WHERE data_profile_key=$1 AND employee_id=$2
         FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(employee_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "HR employee",
        id: employee_id,
    })?;
    let employee_holding_id: Uuid = row.try_get("holding_id")?;
    let employee_company_id: Uuid = row.try_get("company_id")?;
    let employee_branch_id: Option<Uuid> = row.try_get("branch_id")?;
    if !employee_reference_is_compatible(
        employee_holding_id,
        employee_company_id,
        employee_branch_id,
        record_scope,
    ) {
        return Err(invalid("HR employee reference is outside record scope"));
    }
    Ok(())
}

/// Lock every payroll employee in UUID order before writing the run. This
/// makes concurrent payroll commands deterministic and prevents one run from
/// including a source employee outside the run's holding/company/branch scope.
async fn ensure_payroll_employee_references(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    payroll: &PayrollRun,
) -> Result<(), PersistenceError> {
    let mut employee_ids: Vec<Uuid> = payroll.lines.iter().map(|line| line.employee_id).collect();
    employee_ids.sort_unstable();
    employee_ids.dedup();
    if employee_ids.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT employee_id, holding_id, company_id, branch_id
         FROM erpx_holding.hr_employee_records
         WHERE data_profile_key=$1 AND employee_id = ANY($2)
         ORDER BY employee_id
         FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(&employee_ids)
    .fetch_all(&mut **tx)
    .await?;
    let mut found = HashSet::with_capacity(rows.len());
    for row in rows {
        let employee_id: Uuid = row.try_get("employee_id")?;
        found.insert(employee_id);
        let employee_holding_id: Uuid = row.try_get("holding_id")?;
        let employee_company_id: Uuid = row.try_get("company_id")?;
        let employee_branch_id: Option<Uuid> = row.try_get("branch_id")?;
        if !employee_reference_is_compatible(
            employee_holding_id,
            employee_company_id,
            employee_branch_id,
            &payroll.scope,
        ) {
            return Err(invalid(
                "HR payroll employee reference is outside run scope",
            ));
        }
    }
    if let Some(missing_employee_id) = employee_ids
        .iter()
        .find(|employee_id| !found.contains(employee_id))
    {
        return Err(PersistenceError::NotFound {
            entity: "HR employee",
            id: *missing_employee_id,
        });
    }
    Ok(())
}

fn employee_reference_is_compatible(
    employee_holding_id: Uuid,
    employee_company_id: Uuid,
    employee_branch_id: Option<Uuid>,
    record_scope: &EmploymentScope,
) -> bool {
    employee_holding_id == record_scope.holding_id
        && employee_company_id == record_scope.company_id
        && match (employee_branch_id, record_scope.branch_id) {
            (Some(employee_branch_id), Some(record_branch_id)) => {
                employee_branch_id == record_branch_id
            }
            _ => true,
        }
}

async fn current_version(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    aggregate_id: Uuid,
    data_profile_key: &str,
) -> Result<Option<u64>, PersistenceError> {
    // Table and column names are closed internal constants at every call site;
    // values remain bound parameters.
    let sql = format!(
        "SELECT version FROM erpx_holding.{table} WHERE data_profile_key=$1 AND {id_column}=$2 FOR UPDATE"
    );
    let value: Option<i64> = sqlx::query_scalar(&sql)
        .bind(data_profile_key)
        .bind(aggregate_id)
        .fetch_optional(&mut **tx)
        .await?;
    value
        .map(|value| as_u64(value, "aggregate version"))
        .transpose()
}

/// Do not advance a normalized HR source row when its immutable delivery
/// history is already incomplete.  The projector can only rebuild a read
/// model from this outbox stream; accepting version N+1 after a missing or
/// duplicate version N would make the authoritative source unreplayable.
async fn ensure_source_event_continuity(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    current_version: Option<u64>,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS event_count,
                COUNT(DISTINCT aggregate_version) AS distinct_version_count,
                COALESCE(MIN(aggregate_version), 0) AS first_version,
                COALESCE(MAX(aggregate_version), 0) AS last_version
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3",
    )
    .bind(data_profile_key)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .fetch_one(&mut **tx)
    .await?;

    let event_count = as_u64(row.try_get("event_count")?, "HR source event count")?;
    let distinct_version_count = as_u64(
        row.try_get("distinct_version_count")?,
        "HR source distinct version count",
    )?;
    let first_version = as_u64(row.try_get("first_version")?, "HR source first version")?;
    let last_version = as_u64(row.try_get("last_version")?, "HR source last version")?;
    let expected_count = current_version.unwrap_or(0);
    let contiguous = event_count == expected_count
        && distinct_version_count == expected_count
        && (expected_count == 0 || (first_version == 1 && last_version == expected_count));
    if !contiguous {
        return Err(invalid(format!(
            "HR source outbox history is not contiguous for {aggregate_type}"
        )));
    }
    Ok(())
}

fn ensure_expected_version(
    aggregate_type: &str,
    aggregate_id: Uuid,
    expected: Option<u64>,
    actual: Option<u64>,
) -> Result<(), PersistenceError> {
    if expected != actual {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected,
            actual,
        });
    }
    Ok(())
}

fn ensure_domain_version(
    aggregate_type: &str,
    aggregate_id: Uuid,
    supplied: u64,
    expected: u64,
) -> Result<(), PersistenceError> {
    if supplied != expected {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected: Some(expected),
            actual: Some(supplied),
        });
    }
    Ok(())
}

fn ensure_update_affected(
    aggregate_type: &str,
    aggregate_id: Uuid,
    expected_version: u64,
    affected: u64,
) -> Result<(), PersistenceError> {
    if affected != 1 {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected: Some(expected_version),
            actual: None,
        });
    }
    Ok(())
}

fn next_version(current: Option<u64>) -> Result<u64, PersistenceError> {
    current
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| PersistenceError::InvalidData("HR aggregate version overflow".into()))
}

fn request_hash<T: Serialize>(
    aggregate_type: &str,
    aggregate_id: Uuid,
    value: &T,
    expected_version: Option<u64>,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        aggregate_type,
        aggregate_id,
        value,
        expected_version,
    ))?)))
}

// The arguments intentionally mirror the immutable outbox envelope fields;
// keeping them explicit prevents accidentally omitting holding or replay
// metadata at one of the five aggregate write call sites.
#[allow(clippy::too_many_arguments)]
async fn append_hr_event<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    event_type: &str,
    scope: &EmploymentScope,
    command_id: Option<Uuid>,
    idempotency_key: &str,
    aggregate: &T,
) -> Result<(), PersistenceError> {
    let event_id = Uuid::new_v4();
    let correlation_id = command_id
        .filter(|command_id| !command_id.is_nil())
        .unwrap_or(event_id);
    let payload = json!({
        "aggregate_type": aggregate_type,
        "aggregate": aggregate,
        "version": version,
        "event_type": event_type,
    });
    let active_scope = serde_json::to_value(scope.active_scope())?;
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, active_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now(),$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(event_id)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(data_profile_key)
    .bind(as_i64(version, "outbox aggregate version")?)
    .bind(event_type)
    .bind(payload)
    .bind(i32::from(HR_CONTRACT_VERSION))
    .bind(i32::from(HR_EVENT_VERSION))
    .bind(active_scope)
    // The persistence adapter intentionally has no command actor input. A
    // null actor is honest and keeps authorization fail-closed for callers
    // that require authenticated command attribution.
    .bind(Option::<Value>::None)
    .bind(correlation_id)
    .bind(Option::<Uuid>::None)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn receipt(
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    idempotency_key: &str,
) -> HrWriteReceipt {
    HrWriteReceipt {
        aggregate_type: aggregate_type.into(),
        aggregate_id,
        version,
        idempotency_key: idempotency_key.into(),
        replayed: false,
    }
}

fn validate_employee(employee: &EmployeeRecord) -> Result<(), PersistenceError> {
    validate_uuid(employee.id, "employee_id")?;
    validate_scope(&employee.scope)?;
    validate_bounded(
        &employee.employee_ref,
        "employee_ref",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_bounded(&employee.display_name, "display_name", MAX_HR_NAME_BYTES)?;
    validate_idempotency_key(&employee.last_idempotency_key)?;
    validate_uuid(employee.last_command_id, "employee command_id")?;
    if employee.status == EmploymentStatus::Terminated {
        if employee.terminated_on.is_none() {
            return Err(invalid("terminated employee requires terminated_on"));
        }
    } else if employee.terminated_on.is_some() {
        return Err(invalid("non-terminated employee cannot have terminated_on"));
    }
    if employee
        .terminated_on
        .is_some_and(|date| date < employee.hired_on)
    {
        return Err(invalid("employee termination precedes hire date"));
    }
    validate_positive_version(employee.version, "employee version")
}

fn validate_leave(leave: &LeaveRequest) -> Result<(), PersistenceError> {
    validate_uuid(leave.id, "leave_id")?;
    validate_uuid(leave.employee_id, "leave employee_id")?;
    validate_scope(&leave.scope)?;
    validate_bounded(&leave.reason, "leave reason", MAX_HR_REASON_BYTES)?;
    validate_bounded(
        &leave.requested_by,
        "leave requested_by",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_optional_bounded(
        leave.approved_by.as_deref(),
        "leave approved_by",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_uuid(leave.last_command_id, "leave command_id")?;
    if leave.ends_on < leave.starts_on {
        return Err(invalid("leave date window is invalid"));
    }
    if leave.approved_by.as_deref() == Some(leave.requested_by.as_str()) {
        return Err(invalid("leave approval separation is invalid"));
    }
    if matches!(leave.status, LeaveStatus::Approved | LeaveStatus::Taken)
        && leave.approved_by.is_none()
    {
        return Err(invalid("approved leave requires approved_by"));
    }
    validate_positive_version(leave.version, "leave version")
}

fn validate_expense(expense: &ExpenseClaim) -> Result<(), PersistenceError> {
    validate_uuid(expense.id, "expense_id")?;
    validate_uuid(expense.employee_id, "expense employee_id")?;
    validate_scope(&expense.scope)?;
    validate_bounded(&expense.currency, "expense currency", 3)?;
    if expense.amount_minor < 0 {
        return Err(invalid("expense amount cannot be negative"));
    }
    validate_currency(&expense.currency)?;
    validate_bounded(
        &expense.submitted_by,
        "expense submitted_by",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_optional_bounded(
        expense.approved_by.as_deref(),
        "expense approved_by",
        MAX_HR_REFERENCE_BYTES,
    )?;
    if expense.approved_by.as_deref() == Some(expense.submitted_by.as_str()) {
        return Err(invalid("expense approval separation is invalid"));
    }
    let approval_required = matches!(
        expense.status,
        ExpenseStatus::Approved | ExpenseStatus::Reimbursed
    );
    if approval_required != expense.approved_by.is_some() {
        return Err(invalid("approved expense requires approved_by"));
    }
    validate_positive_version(expense.version, "expense version")
}

fn validate_payroll(payroll: &PayrollRun) -> Result<(), PersistenceError> {
    validate_uuid(payroll.id, "payroll_id")?;
    validate_scope(&payroll.scope)?;
    if payroll.ends_on < payroll.starts_on {
        return Err(invalid("payroll date window is invalid"));
    }
    if payroll.lines.is_empty() {
        return Err(invalid("payroll must contain at least one employee line"));
    }
    if payroll.lines.len() > MAX_PAYROLL_LINES {
        return Err(invalid("payroll line count exceeds maximum"));
    }
    validate_bounded(
        &payroll.prepared_by,
        "payroll prepared_by",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_optional_bounded(
        payroll.reviewed_by.as_deref(),
        "payroll reviewed_by",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_optional_bounded(
        payroll.approved_by.as_deref(),
        "payroll approved_by",
        MAX_HR_REFERENCE_BYTES,
    )?;
    if payroll.reviewed_by.as_deref() == Some(payroll.prepared_by.as_str())
        || payroll.approved_by.as_deref() == Some(payroll.prepared_by.as_str())
        || payroll.reviewed_by == payroll.approved_by && payroll.reviewed_by.is_some()
    {
        return Err(invalid("payroll approval separation is invalid"));
    }
    let mut gross = 0_i64;
    let mut deductions = 0_i64;
    let mut net = 0_i64;
    let mut employee_ids = HashSet::with_capacity(payroll.lines.len());
    for line in &payroll.lines {
        validate_payroll_line(line)?;
        if !employee_ids.insert(line.employee_id) {
            return Err(invalid("payroll contains duplicate employee lines"));
        }
        gross = gross
            .checked_add(line.gross_minor)
            .ok_or_else(|| invalid("payroll gross total overflow"))?;
        deductions = deductions
            .checked_add(line.deductions_minor)
            .ok_or_else(|| invalid("payroll deductions total overflow"))?;
        net = net
            .checked_add(line.net_minor)
            .ok_or_else(|| invalid("payroll net total overflow"))?;
    }
    if (gross, deductions, net)
        != (
            payroll.total_gross_minor,
            payroll.total_deductions_minor,
            payroll.total_net_minor,
        )
        || gross.checked_sub(deductions) != Some(net)
    {
        return Err(invalid("payroll totals do not reconcile"));
    }
    if payroll.status == PayrollStatus::Approved && payroll.reviewed_by.is_none() {
        return Err(invalid("approved payroll requires reviewed_by"));
    }
    if payroll.status == PayrollStatus::Posted
        && (payroll.reviewed_by.is_none() || payroll.approved_by.is_none())
    {
        return Err(invalid("posted payroll requires review and approval"));
    }
    validate_positive_version(payroll.version, "payroll version")
}

fn validate_payroll_line(line: &PayrollLine) -> Result<(), PersistenceError> {
    validate_uuid(line.employee_id, "payroll line employee_id")?;
    if line.gross_minor < 0
        || line.deductions_minor < 0
        || line.deductions_minor > line.gross_minor
        || line.net_minor != line.gross_minor - line.deductions_minor
    {
        return Err(invalid("payroll line totals are invalid"));
    }
    Ok(())
}

fn validate_reassignment(reassignment: &ExpiredRoleReassignment) -> Result<(), PersistenceError> {
    validate_scope(&reassignment.scope)?;
    validate_bounded(
        &reassignment.workflow_ref,
        "workflow_ref",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_bounded(
        &reassignment.expired_principal_ref,
        "expired_principal_ref",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_bounded(
        &reassignment.replacement_principal_ref,
        "replacement_principal_ref",
        MAX_HR_REFERENCE_BYTES,
    )?;
    validate_bounded(
        &reassignment.reason,
        "reassignment reason",
        MAX_HR_REASON_BYTES,
    )?;
    validate_uuid(reassignment.command_id, "reassignment command_id")?;
    if reassignment.expired_principal_ref == reassignment.replacement_principal_ref {
        return Err(invalid("reassignment principals must differ"));
    }
    Ok(())
}

fn validate_scope(scope: &EmploymentScope) -> Result<(), PersistenceError> {
    scope
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))
}

fn validate_bounded(value: &str, field: &'static str, max: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{field} cannot be empty")));
    }
    if value.len() > max {
        return Err(invalid(format!("{field} exceeds {max} bytes")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} contains a control character")));
    }
    Ok(())
}

fn validate_optional_bounded(
    value: Option<&str>,
    field: &'static str,
    max: usize,
) -> Result<(), PersistenceError> {
    if let Some(value) = value {
        validate_bounded(value, field, max)?;
    }
    Ok(())
}

fn validate_currency(value: &str) -> Result<(), PersistenceError> {
    if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(invalid(
            "expense currency must be a three-letter uppercase code",
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<(), PersistenceError> {
    validate_bounded(value, "idempotency_key", MAX_IDEMPOTENCY_KEY_BYTES)
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_DATA_PROFILE_KEY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
}

fn validate_positive_version(value: u64, field: &'static str) -> Result<(), PersistenceError> {
    if value == 0 {
        return Err(invalid(format!("{field} must be positive")));
    }
    Ok(())
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
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

fn parse_employee_status(value: impl AsRef<str>) -> Result<EmploymentStatus, PersistenceError> {
    match value.as_ref() {
        "draft" => Ok(EmploymentStatus::Draft),
        "active" => Ok(EmploymentStatus::Active),
        "on_leave" => Ok(EmploymentStatus::OnLeave),
        "suspended" => Ok(EmploymentStatus::Suspended),
        "terminated" => Ok(EmploymentStatus::Terminated),
        _ => Err(invalid("employee status is invalid")),
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

fn stable_reassignment_id(data_profile_key: &str, workflow_ref: &str) -> Uuid {
    let digest = Sha256::digest(
        format!("{REASSIGNMENT_AGGREGATE}:{data_profile_key}:{workflow_ref}").as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    let mut id = Uuid::from_bytes(bytes);
    if id.is_nil() {
        id = Uuid::from_u128(1);
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn constructor_rejects_unscoped_data_profile_references() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxHrPersistence::new(pool.clone(), " ").is_err());
        assert!(SqlxHrPersistence::new(pool, "holding\nref").is_err());
    }

    #[test]
    fn bounded_validation_rejects_control_and_overlong_values() {
        assert!(validate_bounded("ok", "field", 2).is_ok());
        assert!(validate_bounded("", "field", 2).is_err());
        assert!(validate_bounded("a\n", "field", 2).is_err());
        assert!(validate_bounded("abc", "field", 2).is_err());
    }

    #[test]
    fn optimistic_versions_and_hashes_are_deterministic() {
        let id = Uuid::from_u128(1);
        assert_eq!(next_version(None).expect("first version"), 1);
        assert_eq!(next_version(Some(1)).expect("next version"), 2);
        let first = request_hash("hr_employee", id, &("name", 1_u8), Some(1)).expect("hash");
        let second = request_hash("hr_employee", id, &("name", 1_u8), Some(1)).expect("hash");
        let changed = request_hash("hr_employee", id, &("name", 2_u8), Some(1)).expect("hash");
        assert_eq!(first, second);
        assert_ne!(first, changed);
    }

    #[test]
    fn reassignment_id_is_stable_and_non_nil() {
        let first = stable_reassignment_id("holding-a", "workflow-a");
        let second = stable_reassignment_id("holding-a", "workflow-a");
        assert_eq!(first, second);
        assert!(!first.is_nil());
    }

    #[test]
    fn expense_source_validation_matches_the_typed_read_contract() {
        let scope = EmploymentScope::new(Uuid::new_v4(), Uuid::new_v4(), None).expect("scope");
        let expense = ExpenseClaim {
            id: Uuid::new_v4(),
            employee_id: Uuid::new_v4(),
            scope: scope.clone(),
            currency: "thb".into(),
            amount_minor: 100,
            status: ExpenseStatus::Draft,
            submitted_by: "requester".into(),
            approved_by: None,
            version: 1,
        };
        assert!(validate_expense(&expense).is_err());

        let reimbursed_without_approval = ExpenseClaim {
            currency: "THB".into(),
            status: ExpenseStatus::Reimbursed,
            ..expense
        };
        assert!(validate_expense(&reimbursed_without_approval).is_err());
    }
}
