use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_people_payroll::{
    Aggregate, CommandUnitOfWork, Employee, EmployeeId, ExpenseClaim, ExpenseClaimId, LeaveRequest,
    LeaveRequestId, PayrollRun, PayrollRunId, PeoplePayrollCommit, PortError, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EMPLOYEE_AGGREGATE_TYPE, EMPLOYEE_SNAPSHOT_EVENT, EVENT_VERSION,
    EXPENSE_AGGREGATE_TYPE, EXPENSE_SNAPSHOT_EVENT, LEAVE_AGGREGATE_TYPE, LEAVE_SNAPSHOT_EVENT,
    PAYROLL_AGGREGATE_TYPE, PAYROLL_SNAPSHOT_EVENT, employee_partition, expense_partition,
    leave_partition, payroll_partition,
    projection::{
        PeoplePayrollOutboxPayload, PeoplePayrollSnapshot, employee_status_name,
        expense_status_name, leave_status_name, payroll_status_name,
    },
};

#[derive(Debug, Clone)]
pub struct PeoplePayrollWriteRepository {
    store: CommandStore,
}

impl PeoplePayrollWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }

    async fn load_aggregate(
        &self,
        authority_scope: &TenantScope,
        partition: &str,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<Aggregate>, PortError> {
        let aggregate_id = AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
        let snapshot = self
            .store
            .load_snapshot::<Aggregate>(CONTEXT_NAME, partition, aggregate_type, aggregate_id)
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id_uuid() != id
            || snapshot.state.aggregate_type() != aggregate_type
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        if !authority_scope.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}

#[async_trait]
impl WriteRepository for PeoplePayrollWriteRepository {
    async fn load_employee(
        &self,
        scope: &TenantScope,
        id: EmployeeId,
    ) -> Result<Option<Employee>, PortError> {
        match self
            .load_aggregate(
                scope,
                &employee_partition(id),
                EMPLOYEE_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(Aggregate::Employee(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_leave_request(
        &self,
        scope: &TenantScope,
        id: LeaveRequestId,
    ) -> Result<Option<LeaveRequest>, PortError> {
        match self
            .load_aggregate(
                scope,
                &leave_partition(id),
                LEAVE_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(Aggregate::LeaveRequest(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_expense_claim(
        &self,
        scope: &TenantScope,
        id: ExpenseClaimId,
    ) -> Result<Option<ExpenseClaim>, PortError> {
        match self
            .load_aggregate(
                scope,
                &expense_partition(id),
                EXPENSE_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(Aggregate::ExpenseClaim(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_payroll_run(
        &self,
        scope: &TenantScope,
        id: PayrollRunId,
    ) -> Result<Option<PayrollRun>, PortError> {
        match self
            .load_aggregate(
                scope,
                &payroll_partition(id),
                PAYROLL_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(Aggregate::PayrollRun(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeoplePayrollCommandAdapter {
    store: CommandStore,
}
impl PeoplePayrollCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for PeoplePayrollCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, aggregate_type) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<Aggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(|receipt| receipt_to_stored(receipt, aggregate_type))
            .transpose()
    }

    async fn commit(&self, request: PeoplePayrollCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let aggregate_type = request.aggregate.aggregate_type();
        let snapshot_event = snapshot_event(&request.aggregate);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: PeoplePayrollOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: PeoplePayrollOutboxPayload::Snapshot(PeoplePayrollSnapshot::from(
                &request.aggregate,
            )),
        });
        let guard = PeoplePayrollAtomicGuard {
            aggregate: request.aggregate.clone(),
        };
        let aggregate = request.aggregate;
        let aggregate_id =
            AggregateId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"people-payroll-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"aggregate_id": aggregate_id.as_uuid(), "aggregate_type": aggregate_type, "expected_version": request.expected_version}),
            },
            events,
        };
        let outcome = self
            .store
            .commit_guarded(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = receipt_to_stored(receipt, aggregate_type)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct PeoplePayrollAtomicGuard {
    aggregate: Aggregate,
}

#[async_trait]
impl AtomicCommandGuard for PeoplePayrollAtomicGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current = guard_version(transaction, &self.aggregate).await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "People & Payroll aggregate already exists",
                ));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "People & Payroll aggregate guard is missing",
                ));
            }
            _ => {}
        }
        match &self.aggregate {
            Aggregate::Employee(value) if context.expected_version.is_none() => {
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT employee_id FROM erpx_people_payroll_write.employee_guards WHERE scope_key=$1 AND employee_code=$2 FOR UPDATE")
                    .bind(scope_key(value.scope())?).bind(value.employee_code())
                    .fetch_optional(&mut **transaction).await?;
                if existing.is_some_and(|id| id != value.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected("employee code already exists"));
                }
            }
            Aggregate::LeaveRequest(value) => {
                ensure_employee(transaction, value.employee_id(), value.scope()).await?;
            }
            Aggregate::ExpenseClaim(value) => {
                ensure_employee(transaction, value.employee_id(), value.scope()).await?;
            }
            Aggregate::PayrollRun(value) => {
                let ids = value
                    .lines()
                    .iter()
                    .map(|line| line.employee_id.as_uuid())
                    .collect::<Vec<_>>();
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_people_payroll_write.employee_guards WHERE scope_key=$1 AND employee_id=ANY($2::uuid[])")
                    .bind(scope_key(value.scope())?).bind(&ids).fetch_one(&mut **transaction).await?;
                if usize::try_from(count).ok() != Some(ids.len()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "payroll employee guard is missing or cross-scope",
                    ));
                }
            }
            Aggregate::Employee(_) => {}
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.aggregate.aggregate_type()
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "People & Payroll applied guard metadata mismatch",
            ));
        }
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            Aggregate::Employee(value) => {
                sqlx::query("INSERT INTO erpx_people_payroll_write.employee_guards (employee_id,scope_key,employee_code,hired_on,terminated_on,employee_status,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (employee_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,employee_code=EXCLUDED.employee_code,hired_on=EXCLUDED.hired_on,terminated_on=EXCLUDED.terminated_on,employee_status=EXCLUDED.employee_status,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.employee_code())
                    .bind(value.hired_on()).bind(value.terminated_on()).bind(employee_status_name(value.status()))
                    .bind(version).bind(value.created_at()).bind(value.updated_at())
                    .execute(&mut **transaction).await?;
            }
            Aggregate::LeaveRequest(value) => {
                sqlx::query("INSERT INTO erpx_people_payroll_write.leave_request_guards (leave_request_id,scope_key,employee_id,starts_on,ends_on,leave_status,requested_by,approved_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (leave_request_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,employee_id=EXCLUDED.employee_id,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,leave_status=EXCLUDED.leave_status,requested_by=EXCLUDED.requested_by,approved_by=EXCLUDED.approved_by,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.employee_id().as_uuid())
                    .bind(value.starts_on()).bind(value.ends_on()).bind(leave_status_name(value.status()))
                    .bind(value.requested_by()).bind(value.approved_by()).bind(version)
                    .bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
            }
            Aggregate::ExpenseClaim(value) => {
                sqlx::query("INSERT INTO erpx_people_payroll_write.expense_claim_guards (expense_claim_id,scope_key,employee_id,currency,amount_minor,expense_status,claimed_by,approved_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (expense_claim_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,employee_id=EXCLUDED.employee_id,currency=EXCLUDED.currency,amount_minor=EXCLUDED.amount_minor,expense_status=EXCLUDED.expense_status,claimed_by=EXCLUDED.claimed_by,approved_by=EXCLUDED.approved_by,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.employee_id().as_uuid())
                    .bind(value.currency().as_str()).bind(value.amount_minor()).bind(expense_status_name(value.status()))
                    .bind(value.claimed_by()).bind(value.approved_by()).bind(version)
                    .bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
            }
            Aggregate::PayrollRun(value) => {
                sqlx::query("INSERT INTO erpx_people_payroll_write.payroll_run_guards (payroll_run_id,scope_key,period_code,starts_on,ends_on,currency,payroll_status,prepared_by,reviewed_by,approved_by,total_gross_minor,total_deductions_minor,total_net_minor,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT (payroll_run_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,period_code=EXCLUDED.period_code,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,currency=EXCLUDED.currency,payroll_status=EXCLUDED.payroll_status,prepared_by=EXCLUDED.prepared_by,reviewed_by=EXCLUDED.reviewed_by,approved_by=EXCLUDED.approved_by,total_gross_minor=EXCLUDED.total_gross_minor,total_deductions_minor=EXCLUDED.total_deductions_minor,total_net_minor=EXCLUDED.total_net_minor,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.period_code())
                    .bind(value.starts_on()).bind(value.ends_on()).bind(value.currency().as_str())
                    .bind(payroll_status_name(value.status())).bind(value.prepared_by()).bind(value.reviewed_by()).bind(value.approved_by())
                    .bind(value.total_gross_minor()).bind(value.total_deductions_minor()).bind(value.total_net_minor())
                    .bind(version).bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
                sqlx::query("DELETE FROM erpx_people_payroll_write.payroll_line_guards WHERE payroll_run_id=$1")
                    .bind(value.id().as_uuid()).execute(&mut **transaction).await?;
                for line in value.lines() {
                    sqlx::query("INSERT INTO erpx_people_payroll_write.payroll_line_guards (payroll_run_id,employee_id,gross_minor,deductions_minor,net_minor) VALUES ($1,$2,$3,$4,$5)")
                        .bind(value.id().as_uuid()).bind(line.employee_id.as_uuid()).bind(line.gross_minor)
                        .bind(line.deductions_minor).bind(line.net_minor).execute(&mut **transaction).await?;
                }
            }
        }
        Ok(())
    }
}

async fn guard_version(
    transaction: &mut Transaction<'_, Postgres>,
    aggregate: &Aggregate,
) -> Result<Option<i64>, CqrsSqlxError> {
    let (sql, id) = match aggregate {
        Aggregate::Employee(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.employee_guards WHERE employee_id=$1 FOR UPDATE",
            value.id().as_uuid(),
        ),
        Aggregate::LeaveRequest(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.leave_request_guards WHERE leave_request_id=$1 FOR UPDATE",
            value.id().as_uuid(),
        ),
        Aggregate::ExpenseClaim(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.expense_claim_guards WHERE expense_claim_id=$1 FOR UPDATE",
            value.id().as_uuid(),
        ),
        Aggregate::PayrollRun(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.payroll_run_guards WHERE payroll_run_id=$1 FOR UPDATE",
            value.id().as_uuid(),
        ),
    };
    Ok(sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await?)
}

async fn ensure_employee(
    transaction: &mut Transaction<'_, Postgres>,
    employee_id: EmployeeId,
    scope: &TenantScope,
) -> Result<(), CqrsSqlxError> {
    let row = sqlx::query("SELECT scope_key FROM erpx_people_payroll_write.employee_guards WHERE employee_id=$1 FOR SHARE")
        .bind(employee_id.as_uuid()).fetch_optional(&mut **transaction).await?;
    let Some(row) = row else {
        return Err(CqrsSqlxError::GuardRejected("employee guard is missing"));
    };
    let employee_scope: String = row.try_get("scope_key")?;
    if employee_scope != scope_key(scope)? {
        return Err(CqrsSqlxError::GuardRejected(
            "employee guard is cross-scope",
        ));
    }
    Ok(())
}

fn validate_guard_context(
    value: &Aggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != value.aggregate_type()
        || context.aggregate_id.as_uuid() != value.id_uuid()
        || context.partition != aggregate_partition(value)
    {
        Err(CqrsSqlxError::GuardRejected(
            "People & Payroll guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}

fn validate_commit(request: &PeoplePayrollCommit) -> Result<(), PortError> {
    let (partition, aggregate_type) = fingerprint_target(&request.identity.fingerprint);
    if request.aggregate.validate().is_err()
        || partition != aggregate_partition(&request.aggregate)
        || aggregate_type != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}

fn fingerprint_target(value: &CommandFingerprint) -> (String, &'static str) {
    match value {
        CommandFingerprint::HireEmployee { employee_id, .. }
        | CommandFingerprint::TransitionEmployee { employee_id, .. } => {
            (employee_partition(*employee_id), EMPLOYEE_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateLeaveRequest {
            leave_request_id, ..
        }
        | CommandFingerprint::TransitionLeaveRequest {
            leave_request_id, ..
        } => (leave_partition(*leave_request_id), LEAVE_AGGREGATE_TYPE),
        CommandFingerprint::CreateExpenseClaim {
            expense_claim_id, ..
        }
        | CommandFingerprint::TransitionExpenseClaim {
            expense_claim_id, ..
        } => (expense_partition(*expense_claim_id), EXPENSE_AGGREGATE_TYPE),
        CommandFingerprint::CreatePayrollRun { payroll_run_id, .. }
        | CommandFingerprint::TransitionPayrollRun { payroll_run_id, .. } => {
            (payroll_partition(*payroll_run_id), PAYROLL_AGGREGATE_TYPE)
        }
    }
}
fn aggregate_partition(value: &Aggregate) -> String {
    match value {
        Aggregate::Employee(value) => employee_partition(value.id()),
        Aggregate::LeaveRequest(value) => leave_partition(value.id()),
        Aggregate::ExpenseClaim(value) => expense_partition(value.id()),
        Aggregate::PayrollRun(value) => payroll_partition(value.id()),
    }
}
fn snapshot_event(value: &Aggregate) -> &'static str {
    match value {
        Aggregate::Employee(_) => EMPLOYEE_SNAPSHOT_EVENT,
        Aggregate::LeaveRequest(_) => LEAVE_SNAPSHOT_EVENT,
        Aggregate::ExpenseClaim(_) => EXPENSE_SNAPSHOT_EVENT,
        Aggregate::PayrollRun(_) => PAYROLL_SNAPSHOT_EVENT,
    }
}
fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::HireEmployee { .. } => "people_payroll.employee.hire",
        CommandFingerprint::TransitionEmployee { .. } => "people_payroll.employee.transition",
        CommandFingerprint::CreateLeaveRequest { .. } => "people_payroll.leave.create",
        CommandFingerprint::TransitionLeaveRequest { .. } => "people_payroll.leave.transition",
        CommandFingerprint::CreateExpenseClaim { .. } => "people_payroll.expense.create",
        CommandFingerprint::TransitionExpenseClaim { .. } => "people_payroll.expense.transition",
        CommandFingerprint::CreatePayrollRun { .. } => "people_payroll.payroll.create",
        CommandFingerprint::TransitionPayrollRun { .. } => "people_payroll.payroll.transition",
    }
}
fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.people-payroll.command.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn deterministic_event_id(command: CommandId, kind: &str, index: usize) -> EventId {
    let mut input = kind.as_bytes().to_vec();
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &input)).expect("derived event id")
}
fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
pub(crate) fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(CqrsSqlxError::InvalidInput(
            "People & Payroll scope must be exact",
        )),
    }
}
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<Aggregate>,
    aggregate_type: &str,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != aggregate_type
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}
fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_people_payroll::{EmployeeTransition, ports::CommandFingerprint};
    #[test]
    fn stable_hash_and_targets_cover_every_aggregate() {
        let id = EmployeeId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let value = CommandFingerprint::TransitionEmployee {
            employee_id: id,
            transition: EmployeeTransition::Activate,
        };
        assert_eq!(request_hash(&value).unwrap(), request_hash(&value).unwrap());
        assert_eq!(
            fingerprint_target(&value),
            (employee_partition(id), EMPLOYEE_AGGREGATE_TYPE)
        );
    }
}
