use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId,
    record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use erpx_people_payroll::{
    Aggregate, CommandUnitOfWork, Currency, Employee, EmployeeId, EmployeeState, EmployeeStatus,
    ExpenseClaim, ExpenseClaimId, ExpenseClaimState, ExpenseStatus, LeaveRequest, LeaveRequestId,
    LeaveRequestState, LeaveStatus, PayrollCalculationEvidence, PayrollInputEvidence, PayrollLine,
    PayrollRun, PayrollRunId, PayrollRunState, PayrollStatus, PeoplePayrollCommit, PortError,
    WriteRepository,
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
    pool: WritePool,
}

impl PeoplePayrollWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WriteRepository for PeoplePayrollWriteRepository {
    async fn load_employee(
        &self,
        scope: &AuthorityScope,
        id: EmployeeId,
    ) -> Result<Option<Employee>, PortError> {
        load_employee_authority(&self.pool, scope, id).await
    }
    async fn load_leave_request(
        &self,
        scope: &AuthorityScope,
        id: LeaveRequestId,
    ) -> Result<Option<LeaveRequest>, PortError> {
        load_leave_authority(&self.pool, scope, id).await
    }
    async fn load_expense_claim(
        &self,
        scope: &AuthorityScope,
        id: ExpenseClaimId,
    ) -> Result<Option<ExpenseClaim>, PortError> {
        load_expense_authority(&self.pool, scope, id).await
    }
    async fn load_payroll_run(
        &self,
        scope: &AuthorityScope,
        id: PayrollRunId,
    ) -> Result<Option<PayrollRun>, PortError> {
        load_payroll_authority(&self.pool, scope, id).await
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
        for directory_draft in people_payroll_directory_drafts(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: PeoplePayrollOutboxPayload::Directory(directory_draft),
            });
        }
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
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
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
            .commit_normalized(command, &guard)
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

fn people_payroll_directory_drafts(
    aggregate: &Aggregate,
) -> Result<Vec<ItemDescriptorDraftV2>, PortError> {
    let (record_type, lifecycle_state, display_label) = match aggregate {
        Aggregate::Employee(value) => (
            "people.employee",
            employee_status_name(value.status()).to_owned(),
            format!("{} {}", value.employee_code(), value.display_name()),
        ),
        Aggregate::LeaveRequest(value) => (
            "people.leave",
            leave_status_name(value.status()).to_owned(),
            format!("Leave request {}", value.employee_id().as_uuid()),
        ),
        Aggregate::ExpenseClaim(_) => return Ok(Vec::new()),
        Aggregate::PayrollRun(value) => (
            "payroll.run",
            payroll_status_name(value.status()).to_owned(),
            value.period_code().to_owned(),
        ),
    };
    let record_id =
        RecordId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::PeoplePayroll,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state,
        classification: DataClassification::Restricted,
        descriptor_version: aggregate.version(),
        authorized_search_text: display_label.clone(),
    };
    let value =
        ItemDescriptorDraftV2::try_new(value, display_label).map_err(|_| PortError::Internal)?;
    let mut values = vec![value];
    match aggregate {
        Aggregate::Employee(employee) => {
            let record = employee
                .employment_record_ref()
                .map_err(|_| PortError::Internal)?;
            let display_label = format!(
                "{} {} employment",
                employee.employee_code(),
                employee.display_name()
            );
            let value = ItemDescriptorDraft {
                authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
                record,
                scope: employee.scope().clone(),
                lifecycle_state: employee_status_name(employee.status()).to_owned(),
                classification: DataClassification::Restricted,
                descriptor_version: employee.version(),
                authorized_search_text: display_label.clone(),
            };
            let value = ItemDescriptorDraftV2::try_new(value, display_label)
                .map_err(|_| PortError::Internal)?;
            values.push(value);
        }
        Aggregate::PayrollRun(payroll) => {
            for line in payroll.lines() {
                let record = payroll
                    .payslip_record_ref(line.employee_id)
                    .map_err(|_| PortError::Internal)?
                    .ok_or(PortError::Internal)?;
                let display_label =
                    format!("{} {}", payroll.period_code(), line.employee_id.as_uuid());
                let value = ItemDescriptorDraft {
                    authorization_policy: record_policy_id(&record)
                        .map_err(|_| PortError::Internal)?,
                    record,
                    scope: payroll.scope().clone(),
                    lifecycle_state: payroll_status_name(payroll.status()).to_owned(),
                    classification: DataClassification::Restricted,
                    descriptor_version: payroll.version(),
                    authorized_search_text: display_label.clone(),
                };
                let value = ItemDescriptorDraftV2::try_new(value, display_label)
                    .map_err(|_| PortError::Internal)?;
                values.push(value);
            }
        }
        Aggregate::LeaveRequest(_) | Aggregate::ExpenseClaim(_) => {}
    }
    Ok(values)
}

struct PeoplePayrollAtomicGuard {
    aggregate: Aggregate,
}

#[async_trait]
impl AtomicCommandGuard for PeoplePayrollAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        _context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        guard_version(transaction, &self.aggregate)
            .await?
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

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
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT employee_id FROM erpx_people_payroll_write.employee_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND scope_key=$1 AND employee_code=$2 FOR UPDATE")
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
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_people_payroll_write.employee_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND scope_key=$1 AND employee_id=ANY($2::uuid[])")
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
        apply_people_payroll_authority(transaction, &self.aggregate, context.aggregate_version)
            .await?;
        apply_derived_people_records(transaction, &self.aggregate, context.aggregate_version)
            .await?;
        Ok(())
    }
}

#[derive(Debug)]
struct AuthorityScopeParams {
    mode: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl AuthorityScopeParams {
    fn from_authority(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Internal)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                mode: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![*organization_node_id],
                branch_ids: Vec::new(),
            },
        })
    }
}

const AUTHORITY_FILTER: &str = "
 AND ($2='platform' OR ($2='holding' AND holding_id=$3)
   OR ($2='company' AND holding_id=$3 AND company_id=$4)
   OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
   OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
       AND ((branch_id IS NULL AND cardinality($7::uuid[])=0) OR branch_id=ANY($7))))";

async fn authority_row(
    pool: &WritePool,
    scope: &AuthorityScope,
    table: &str,
    id_column: &str,
    id: Uuid,
    columns: &str,
) -> Result<Option<sqlx::postgres::PgRow>, PortError> {
    let authority = AuthorityScopeParams::from_authority(scope)?;
    let sql = format!(
        "SELECT {columns} FROM erpx_people_payroll_write.{table}
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND {id_column}=$1{AUTHORITY_FILTER}"
    );
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let row = sqlx::query(&sql)
        .bind(id)
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    tx.commit().await.map_err(|_| PortError::Unavailable)?;
    Ok(row)
}

async fn load_employee_authority(
    pool: &WritePool,
    scope: &AuthorityScope,
    id: EmployeeId,
) -> Result<Option<Employee>, PortError> {
    let row = authority_row(
        pool,
        scope,
        "employee_authority",
        "employee_id",
        id.as_uuid(),
        "aggregate_version,holding_id,company_id,branch_id,employee_code,display_name,
         hired_on,terminated_on,employee_status,created_at,updated_at",
    )
    .await?;
    row.map(|row| {
        Employee::try_rehydrate(EmployeeState {
            id,
            version: row_u64(&row, "aggregate_version")?,
            scope: exact_scope_from_row(&row)?,
            employee_code: row
                .try_get("employee_code")
                .map_err(|_| PortError::Internal)?,
            display_name: row
                .try_get("display_name")
                .map_err(|_| PortError::Internal)?,
            hired_on: row.try_get("hired_on").map_err(|_| PortError::Internal)?,
            terminated_on: row
                .try_get("terminated_on")
                .map_err(|_| PortError::Internal)?,
            status: parse_employee_status(
                row.try_get("employee_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)
    })
    .transpose()
}

async fn load_leave_authority(
    pool: &WritePool,
    scope: &AuthorityScope,
    id: LeaveRequestId,
) -> Result<Option<LeaveRequest>, PortError> {
    let row = authority_row(
        pool,
        scope,
        "leave_request_authority",
        "leave_request_id",
        id.as_uuid(),
        "aggregate_version,holding_id,company_id,branch_id,employee_id,starts_on,ends_on,
         reason,leave_status,requested_by,approved_by,created_at,updated_at",
    )
    .await?;
    row.map(|row| {
        LeaveRequest::try_rehydrate(LeaveRequestState {
            id,
            version: row_u64(&row, "aggregate_version")?,
            scope: exact_scope_from_row(&row)?,
            employee_id: EmployeeId::try_from_uuid(
                row.try_get("employee_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            starts_on: row.try_get("starts_on").map_err(|_| PortError::Internal)?,
            ends_on: row.try_get("ends_on").map_err(|_| PortError::Internal)?,
            reason: row.try_get("reason").map_err(|_| PortError::Internal)?,
            status: parse_leave_status(
                row.try_get("leave_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            requested_by: row
                .try_get("requested_by")
                .map_err(|_| PortError::Internal)?,
            approved_by: row
                .try_get("approved_by")
                .map_err(|_| PortError::Internal)?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)
    })
    .transpose()
}

async fn load_expense_authority(
    pool: &WritePool,
    scope: &AuthorityScope,
    id: ExpenseClaimId,
) -> Result<Option<ExpenseClaim>, PortError> {
    let row = authority_row(
        pool,
        scope,
        "expense_claim_authority",
        "expense_claim_id",
        id.as_uuid(),
        "aggregate_version,holding_id,company_id,branch_id,employee_id,currency,amount_minor,
         reason,expense_status,claimed_by,approved_by,created_at,updated_at",
    )
    .await?;
    row.map(|row| {
        ExpenseClaim::try_rehydrate(ExpenseClaimState {
            id,
            version: row_u64(&row, "aggregate_version")?,
            scope: exact_scope_from_row(&row)?,
            employee_id: EmployeeId::try_from_uuid(
                row.try_get("employee_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            currency: Currency::try_new(
                row.try_get::<String, _>("currency")
                    .map_err(|_| PortError::Internal)?
                    .as_str(),
            )
            .map_err(|_| PortError::Internal)?,
            amount_minor: row
                .try_get("amount_minor")
                .map_err(|_| PortError::Internal)?,
            reason: row.try_get("reason").map_err(|_| PortError::Internal)?,
            status: parse_expense_status(
                row.try_get("expense_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            claimed_by: row.try_get("claimed_by").map_err(|_| PortError::Internal)?,
            approved_by: row
                .try_get("approved_by")
                .map_err(|_| PortError::Internal)?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)
    })
    .transpose()
}

async fn load_payroll_authority(
    pool: &WritePool,
    scope: &AuthorityScope,
    id: PayrollRunId,
) -> Result<Option<PayrollRun>, PortError> {
    let authority = AuthorityScopeParams::from_authority(scope)?;
    let root_sql = format!(
        "SELECT aggregate_version,holding_id,company_id,branch_id,period_code,starts_on,ends_on,
                currency,payroll_status,prepared_by,reviewed_by,approved_by,total_gross_minor,
                total_deductions_minor,total_net_minor,created_at,updated_at
         FROM erpx_people_payroll_write.payroll_run_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND payroll_run_id=$1{AUTHORITY_FILTER}"
    );
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let row = sqlx::query(&root_sql)
        .bind(id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let line_rows = sqlx::query(
        "SELECT employee_id,gross_minor,deductions_minor,net_minor
         FROM erpx_people_payroll_write.payroll_line_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND payroll_run_id=$1
         ORDER BY employee_id",
    )
    .bind(id.as_uuid())
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let lines = line_rows
        .into_iter()
        .map(|line| {
            let value = PayrollLine {
                employee_id: EmployeeId::try_from_uuid(
                    line.try_get("employee_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                gross_minor: line
                    .try_get("gross_minor")
                    .map_err(|_| PortError::Internal)?,
                deductions_minor: line
                    .try_get("deductions_minor")
                    .map_err(|_| PortError::Internal)?,
                net_minor: line.try_get("net_minor").map_err(|_| PortError::Internal)?,
            };
            value.validate().map_err(|_| PortError::Internal)?;
            Ok(value)
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let evidence_row = sqlx::query(
        "SELECT policy_version,rate_table_version,calculation_sha256,captured_at
         FROM erpx_people_payroll_write.payroll_calculation_evidence_authority
         WHERE holding_id=erpx_cqrs_write.current_holding_id() AND payroll_run_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let calculation_evidence = if let Some(evidence) = evidence_row {
        let input_rows = sqlx::query(
            "SELECT source_record_id,source_owner_context,source_record_type,source_version,
                    content_sha256
             FROM erpx_people_payroll_write.payroll_input_evidence_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND payroll_run_id=$1
             ORDER BY input_ordinal",
        )
        .bind(id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let source_inputs = input_rows
            .into_iter()
            .map(|input| {
                let source = StableRecordRef::try_new(
                    RecordId::try_from_uuid(
                        input
                            .try_get("source_record_id")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    parse_context_id(
                        input
                            .try_get("source_owner_context")
                            .map_err(|_| PortError::Internal)?,
                    )?,
                    RecordTypeId::try_new(
                        input
                            .try_get::<String, _>("source_record_type")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?;
                Ok(PayrollInputEvidence {
                    source,
                    source_version: row_u64(&input, "source_version")?,
                    content_sha256: input
                        .try_get("content_sha256")
                        .map_err(|_| PortError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        Some(PayrollCalculationEvidence {
            policy_version: evidence
                .try_get("policy_version")
                .map_err(|_| PortError::Internal)?,
            rate_table_version: evidence
                .try_get("rate_table_version")
                .map_err(|_| PortError::Internal)?,
            calculation_sha256: evidence
                .try_get("calculation_sha256")
                .map_err(|_| PortError::Internal)?,
            source_inputs,
            captured_at: evidence
                .try_get("captured_at")
                .map_err(|_| PortError::Internal)?,
        })
    } else {
        None
    };
    let value = PayrollRun::try_rehydrate(PayrollRunState {
        id,
        version: row_u64(&row, "aggregate_version")?,
        scope: exact_scope_from_row(&row)?,
        period_code: row
            .try_get("period_code")
            .map_err(|_| PortError::Internal)?,
        starts_on: row.try_get("starts_on").map_err(|_| PortError::Internal)?,
        ends_on: row.try_get("ends_on").map_err(|_| PortError::Internal)?,
        currency: Currency::try_new(
            row.try_get::<String, _>("currency")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )
        .map_err(|_| PortError::Internal)?,
        lines,
        calculation_evidence,
        status: parse_payroll_status(
            row.try_get("payroll_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        prepared_by: row
            .try_get("prepared_by")
            .map_err(|_| PortError::Internal)?,
        reviewed_by: row
            .try_get("reviewed_by")
            .map_err(|_| PortError::Internal)?,
        approved_by: row
            .try_get("approved_by")
            .map_err(|_| PortError::Internal)?,
        total_gross_minor: row
            .try_get("total_gross_minor")
            .map_err(|_| PortError::Internal)?,
        total_deductions_minor: row
            .try_get("total_deductions_minor")
            .map_err(|_| PortError::Internal)?,
        total_net_minor: row
            .try_get("total_net_minor")
            .map_err(|_| PortError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    })
    .map_err(|_| PortError::Internal)?;
    tx.commit().await.map_err(|_| PortError::Unavailable)?;
    Ok(Some(value))
}

async fn apply_people_payroll_authority(
    tx: &mut Transaction<'_, Postgres>,
    aggregate: &Aggregate,
    version: u64,
) -> Result<(), CqrsSqlxError> {
    let version = to_i64(version)?;
    let (holding_id, company_id, branch_id) = exact_scope_columns(aggregate.scope())?;
    let result = match aggregate {
        Aggregate::Employee(value) => sqlx::query(
            "INSERT INTO erpx_people_payroll_write.employee_authority
             (holding_id,employee_id,aggregate_version,scope_key,holding_id,company_id,branch_id,
              employee_code,display_name,hired_on,terminated_on,employee_status,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (holding_id,employee_id) DO UPDATE SET
              aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
              holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
              employee_code=EXCLUDED.employee_code,display_name=EXCLUDED.display_name,
              hired_on=EXCLUDED.hired_on,terminated_on=EXCLUDED.terminated_on,
              employee_status=EXCLUDED.employee_status,updated_at=EXCLUDED.updated_at
             WHERE erpx_people_payroll_write.employee_authority.aggregate_version+1=EXCLUDED.aggregate_version",
        )
        .bind(value.id().as_uuid()).bind(version).bind(scope_key(value.scope())?)
        .bind(holding_id).bind(company_id).bind(branch_id).bind(value.employee_code())
        .bind(value.display_name()).bind(value.hired_on()).bind(value.terminated_on())
        .bind(employee_status_name(value.status())).bind(value.created_at()).bind(value.updated_at())
        .execute(&mut **tx).await?,
        Aggregate::LeaveRequest(value) => sqlx::query(
            "INSERT INTO erpx_people_payroll_write.leave_request_authority
             (holding_id,leave_request_id,aggregate_version,scope_key,holding_id,company_id,branch_id,
              employee_id,starts_on,ends_on,reason,leave_status,requested_by,approved_by,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
             ON CONFLICT (holding_id,leave_request_id) DO UPDATE SET
              aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
              holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
              employee_id=EXCLUDED.employee_id,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,
              reason=EXCLUDED.reason,leave_status=EXCLUDED.leave_status,
              requested_by=EXCLUDED.requested_by,approved_by=EXCLUDED.approved_by,
              updated_at=EXCLUDED.updated_at
             WHERE erpx_people_payroll_write.leave_request_authority.aggregate_version+1=EXCLUDED.aggregate_version",
        )
        .bind(value.id().as_uuid()).bind(version).bind(scope_key(value.scope())?)
        .bind(holding_id).bind(company_id).bind(branch_id).bind(value.employee_id().as_uuid())
        .bind(value.starts_on()).bind(value.ends_on()).bind(value.reason())
        .bind(leave_status_name(value.status())).bind(value.requested_by()).bind(value.approved_by())
        .bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?,
        Aggregate::ExpenseClaim(value) => sqlx::query(
            "INSERT INTO erpx_people_payroll_write.expense_claim_authority
             (holding_id,expense_claim_id,aggregate_version,scope_key,holding_id,company_id,branch_id,
              employee_id,currency,amount_minor,reason,expense_status,claimed_by,approved_by,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
             ON CONFLICT (holding_id,expense_claim_id) DO UPDATE SET
              aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
              holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
              employee_id=EXCLUDED.employee_id,currency=EXCLUDED.currency,amount_minor=EXCLUDED.amount_minor,
              reason=EXCLUDED.reason,expense_status=EXCLUDED.expense_status,
              claimed_by=EXCLUDED.claimed_by,approved_by=EXCLUDED.approved_by,
              updated_at=EXCLUDED.updated_at
             WHERE erpx_people_payroll_write.expense_claim_authority.aggregate_version+1=EXCLUDED.aggregate_version",
        )
        .bind(value.id().as_uuid()).bind(version).bind(scope_key(value.scope())?)
        .bind(holding_id).bind(company_id).bind(branch_id).bind(value.employee_id().as_uuid())
        .bind(value.currency().as_str()).bind(value.amount_minor()).bind(value.reason())
        .bind(expense_status_name(value.status())).bind(value.claimed_by()).bind(value.approved_by())
        .bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?,
        Aggregate::PayrollRun(value) => sqlx::query(
            "INSERT INTO erpx_people_payroll_write.payroll_run_authority
             (holding_id,payroll_run_id,aggregate_version,scope_key,holding_id,company_id,branch_id,
              period_code,starts_on,ends_on,currency,payroll_status,prepared_by,reviewed_by,approved_by,
              total_gross_minor,total_deductions_minor,total_net_minor,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
             ON CONFLICT (holding_id,payroll_run_id) DO UPDATE SET
              aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
              holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
              period_code=EXCLUDED.period_code,starts_on=EXCLUDED.starts_on,ends_on=EXCLUDED.ends_on,
              currency=EXCLUDED.currency,payroll_status=EXCLUDED.payroll_status,
              prepared_by=EXCLUDED.prepared_by,reviewed_by=EXCLUDED.reviewed_by,
              approved_by=EXCLUDED.approved_by,total_gross_minor=EXCLUDED.total_gross_minor,
              total_deductions_minor=EXCLUDED.total_deductions_minor,total_net_minor=EXCLUDED.total_net_minor,
              updated_at=EXCLUDED.updated_at
             WHERE erpx_people_payroll_write.payroll_run_authority.aggregate_version+1=EXCLUDED.aggregate_version",
        )
        .bind(value.id().as_uuid()).bind(version).bind(scope_key(value.scope())?)
        .bind(holding_id).bind(company_id).bind(branch_id).bind(value.period_code())
        .bind(value.starts_on()).bind(value.ends_on()).bind(value.currency().as_str())
        .bind(payroll_status_name(value.status())).bind(value.prepared_by()).bind(value.reviewed_by())
        .bind(value.approved_by()).bind(value.total_gross_minor()).bind(value.total_deductions_minor())
        .bind(value.total_net_minor()).bind(value.created_at()).bind(value.updated_at())
        .execute(&mut **tx).await?,
    };
    if result.rows_affected() != 1 {
        return Err(CqrsSqlxError::GuardRejected(
            "People & Payroll typed authority version changed",
        ));
    }
    if let Aggregate::PayrollRun(value) = aggregate {
        replace_payroll_children(tx, value, version).await?;
    }
    Ok(())
}

async fn replace_payroll_children(
    tx: &mut Transaction<'_, Postgres>,
    value: &PayrollRun,
    version: i64,
) -> Result<(), CqrsSqlxError> {
    for table in [
        "payroll_input_evidence_authority",
        "payroll_calculation_evidence_authority",
        "payroll_line_authority",
    ] {
        let sql = format!(
            "DELETE FROM erpx_people_payroll_write.{table}
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND payroll_run_id=$1"
        );
        sqlx::query(&sql)
            .bind(value.id().as_uuid())
            .execute(&mut **tx)
            .await?;
    }
    for line in value.lines() {
        sqlx::query(
            "INSERT INTO erpx_people_payroll_write.payroll_line_authority
             (holding_id,payroll_run_id,employee_id,gross_minor,deductions_minor,net_minor,payroll_version)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6)",
        )
        .bind(value.id().as_uuid())
        .bind(line.employee_id.as_uuid())
        .bind(line.gross_minor)
        .bind(line.deductions_minor)
        .bind(line.net_minor)
        .bind(version)
        .execute(&mut **tx)
        .await?;
    }
    if let Some(evidence) = value.calculation_evidence() {
        sqlx::query(
            "INSERT INTO erpx_people_payroll_write.payroll_calculation_evidence_authority
             (holding_id,payroll_run_id,policy_version,rate_table_version,calculation_sha256,
              captured_at,payroll_version)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6)",
        )
        .bind(value.id().as_uuid())
        .bind(&evidence.policy_version)
        .bind(&evidence.rate_table_version)
        .bind(&evidence.calculation_sha256)
        .bind(evidence.captured_at)
        .bind(version)
        .execute(&mut **tx)
        .await?;
        for (ordinal, input) in evidence.source_inputs.iter().enumerate() {
            sqlx::query(
                "INSERT INTO erpx_people_payroll_write.payroll_input_evidence_authority
                 (holding_id,payroll_run_id,input_ordinal,source_record_id,source_owner_context,
                  source_record_type,source_version,content_sha256,payroll_version)
                 VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(value.id().as_uuid())
            .bind(i32::try_from(ordinal).map_err(|_| CqrsSqlxError::NumericRange)?)
            .bind(input.source.id().as_uuid())
            .bind(context_id_store(input.source.owner_context())?)
            .bind(input.source.record_type().as_str())
            .bind(to_i64(input.source_version)?)
            .bind(&input.content_sha256)
            .bind(version)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn apply_derived_people_records(
    tx: &mut Transaction<'_, Postgres>,
    aggregate: &Aggregate,
    version: u64,
) -> Result<(), CqrsSqlxError> {
    let version = to_i64(version)?;
    match aggregate {
        Aggregate::Employee(value) => {
            let employment = value
                .employment_record_ref()
                .map_err(|_| CqrsSqlxError::InvalidInput("employment record is invalid"))?;
            sqlx::query(
                "INSERT INTO erpx_people_payroll_write.employment_records
                 (holding_id,employment_id,employee_id,source_employee_version,active_scope,
                  scope_key,hired_on,terminated_on,employment_status,changed_at)
                 VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9)
                 ON CONFLICT (holding_id,employment_id) DO UPDATE SET
                  source_employee_version=EXCLUDED.source_employee_version,
                  active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                  terminated_on=EXCLUDED.terminated_on,employment_status=EXCLUDED.employment_status,
                  changed_at=EXCLUDED.changed_at
                 WHERE erpx_people_payroll_write.employment_records.source_employee_version+1
                       =EXCLUDED.source_employee_version",
            )
            .bind(employment.id().as_uuid())
            .bind(value.id().as_uuid())
            .bind(version)
            .bind(serde_json::to_value(value.scope())?)
            .bind(scope_key(value.scope())?)
            .bind(value.hired_on())
            .bind(value.terminated_on())
            .bind(employee_status_name(value.status()))
            .bind(value.updated_at())
            .execute(&mut **tx)
            .await?;
        }
        Aggregate::PayrollRun(value) => {
            for line in value.lines() {
                let payslip = value
                    .payslip_record_ref(line.employee_id)
                    .map_err(|_| CqrsSqlxError::InvalidInput("payslip record is invalid"))?
                    .ok_or(CqrsSqlxError::InvalidInput("payslip employee is missing"))?;
                sqlx::query(
                    "INSERT INTO erpx_people_payroll_write.payslip_records
                     (holding_id,payslip_id,payroll_run_id,employee_id,source_payroll_version,
                      active_scope,scope_key,period_code,starts_on,ends_on,currency,
                      gross_minor,deductions_minor,net_minor,payslip_status,changed_at)
                     VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
                     ON CONFLICT (holding_id,payslip_id) DO UPDATE SET
                      source_payroll_version=EXCLUDED.source_payroll_version,
                      gross_minor=EXCLUDED.gross_minor,deductions_minor=EXCLUDED.deductions_minor,
                      net_minor=EXCLUDED.net_minor,payslip_status=EXCLUDED.payslip_status,
                      changed_at=EXCLUDED.changed_at
                     WHERE erpx_people_payroll_write.payslip_records.source_payroll_version+1
                           =EXCLUDED.source_payroll_version",
                )
                .bind(payslip.id().as_uuid()).bind(value.id().as_uuid())
                .bind(line.employee_id.as_uuid()).bind(version)
                .bind(serde_json::to_value(value.scope())?).bind(scope_key(value.scope())?)
                .bind(value.period_code()).bind(value.starts_on()).bind(value.ends_on())
                .bind(value.currency().as_str()).bind(line.gross_minor).bind(line.deductions_minor)
                .bind(line.net_minor).bind(payroll_status_name(value.status())).bind(value.updated_at())
                .execute(&mut **tx).await?;
            }
        }
        Aggregate::LeaveRequest(_) | Aggregate::ExpenseClaim(_) => {}
    }
    Ok(())
}

fn exact_scope_columns(
    scope: &AuthorityScope,
) -> Result<(Uuid, Uuid, Option<Uuid>), CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(CqrsSqlxError::InvalidInput(
            "People & Payroll scope must be exact",
        )),
    }
}

fn exact_scope_from_row(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
    let scope = branch_id.map_or(
        AuthorityScope::Company {
            holding_id,
            company_id,
        },
        |branch_id| AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
    );
    scope.validate().map_err(|_| PortError::Internal)?;
    Ok(scope)
}

fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}

fn parse_employee_status(value: &str) -> Result<EmployeeStatus, PortError> {
    match value {
        "draft" => Ok(EmployeeStatus::Draft),
        "active" => Ok(EmployeeStatus::Active),
        "on_leave" => Ok(EmployeeStatus::OnLeave),
        "suspended" => Ok(EmployeeStatus::Suspended),
        "terminated" => Ok(EmployeeStatus::Terminated),
        _ => Err(PortError::Internal),
    }
}

fn parse_leave_status(value: &str) -> Result<LeaveStatus, PortError> {
    match value {
        "draft" => Ok(LeaveStatus::Draft),
        "submitted" => Ok(LeaveStatus::Submitted),
        "approved" => Ok(LeaveStatus::Approved),
        "rejected" => Ok(LeaveStatus::Rejected),
        "cancelled" => Ok(LeaveStatus::Cancelled),
        "taken" => Ok(LeaveStatus::Taken),
        _ => Err(PortError::Internal),
    }
}

fn parse_expense_status(value: &str) -> Result<ExpenseStatus, PortError> {
    match value {
        "draft" => Ok(ExpenseStatus::Draft),
        "submitted" => Ok(ExpenseStatus::Submitted),
        "approved" => Ok(ExpenseStatus::Approved),
        "rejected" => Ok(ExpenseStatus::Rejected),
        "reimbursed" => Ok(ExpenseStatus::Reimbursed),
        _ => Err(PortError::Internal),
    }
}

fn parse_payroll_status(value: &str) -> Result<PayrollStatus, PortError> {
    match value {
        "draft" => Ok(PayrollStatus::Draft),
        "review" => Ok(PayrollStatus::Review),
        "approved" => Ok(PayrollStatus::Approved),
        "posted" => Ok(PayrollStatus::Posted),
        "cancelled" => Ok(PayrollStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}

fn context_id_store(value: ContextId) -> Result<String, CqrsSqlxError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or(CqrsSqlxError::InvalidInput("context ID cannot be encoded"))
}

fn parse_context_id(value: String) -> Result<ContextId, PortError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| PortError::Internal)
}

async fn guard_version(
    transaction: &mut Transaction<'_, Postgres>,
    aggregate: &Aggregate,
) -> Result<Option<i64>, CqrsSqlxError> {
    let (sql, id) = match aggregate {
        Aggregate::Employee(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.employee_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND employee_id=$1 FOR UPDATE",
            value.id().as_uuid(),
        ),
        Aggregate::LeaveRequest(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.leave_request_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND leave_request_id=$1 FOR UPDATE",
            value.id().as_uuid(),
        ),
        Aggregate::ExpenseClaim(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.expense_claim_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND expense_claim_id=$1 FOR UPDATE",
            value.id().as_uuid(),
        ),
        Aggregate::PayrollRun(value) => (
            "SELECT aggregate_version FROM erpx_people_payroll_write.payroll_run_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND payroll_run_id=$1 FOR UPDATE",
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
    scope: &AuthorityScope,
) -> Result<(), CqrsSqlxError> {
    let row = sqlx::query("SELECT scope_key FROM erpx_people_payroll_write.employee_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND employee_id=$1 FOR SHARE")
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
        | CommandFingerprint::TransitionPayrollRun { payroll_run_id, .. }
        | CommandFingerprint::PinPayrollCalculationEvidence { payroll_run_id, .. } => {
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
        CommandFingerprint::PinPayrollCalculationEvidence { .. } => {
            "people_payroll.payroll.pin_calculation_evidence"
        }
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
pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        AuthorityScope::Branch {
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

    #[test]
    fn employment_and_payslip_authority_share_the_owner_uow_and_outbox() {
        let source = include_str!("command.rs");
        assert!(source.contains("employment_records"));
        assert!(source.contains("payslip_records"));
        assert!(source.contains("people_payroll_directory_drafts"));
        assert!(source.contains("PeoplePayrollOutboxPayload::Directory"));
        assert!(source.contains("source_employee_version + 1"));
        assert!(source.contains("source_payroll_version + 1"));
    }
}
