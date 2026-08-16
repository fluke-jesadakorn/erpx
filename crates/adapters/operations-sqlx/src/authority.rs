//! Typed relational Operations write authority.
//!
//! Source facts are stored in source-specific scalar/child relations. No
//! aggregate or source-fact JSON document participates in rehydration.

use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use erpx_cqrs_sqlx::{CqrsSqlxError, WritePool};
use erpx_integration_contracts::BoundedContext;
use erpx_kernel::{AggregateId, CorrelationId, EventId, TenantScope};
use erpx_operations::{
    ExceptionId, OperationsException, OperationsExceptionState, PortError, PrincipalAssignment,
    SourceReference, WorkQueuePolicy, WorkQueuePolicyId, WorkQueuePolicyState,
};

use crate::{
    authority_source::{load_source_fact, replace_source_fact},
    command::{map_store_error, scope_key},
    projection::{exception_status_name, policy_status_name},
};

#[derive(Debug)]
struct AuthorityScope {
    mode: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl AuthorityScope {
    fn from_scope(scope: &TenantScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Internal)?;
        Ok(match scope {
            TenantScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            TenantScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            TenantScope::Company {
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
            TenantScope::Branch {
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
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: company_ids.clone(),
                branch_ids: branch_ids.clone(),
            },
        })
    }
}

const AUTHORITY_FILTER: &str = "
 AND ($2='platform' OR ($2='holding' AND holding_id=$3)
   OR ($2='company' AND holding_id=$3 AND company_id=$4)
   OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
   OR ($2='subtree' AND holding_id=$3
       AND ((company_id IS NULL AND cardinality($6::uuid[])=0) OR company_id=ANY($6))
       AND ((branch_id IS NULL AND cardinality($7::uuid[])=0) OR branch_id=ANY($7))))";

pub(crate) async fn load_exception_authority(
    pool: &WritePool,
    authority_scope: &TenantScope,
    id: ExceptionId,
) -> Result<Option<OperationsException>, PortError> {
    let authority = AuthorityScope::from_scope(authority_scope)?;
    let sql = format!(
        "SELECT aggregate_version,scope_mode,holding_id,company_id,branch_id,reference,
                source_context,source_event_id,source_event_type,source_aggregate_type,
                source_aggregate_id,source_aggregate_version,source_correlation_id,
                source_occurred_at,policy_ref,category,severity,assignee_id,manager_id,
                assignment_authority_ref,assignment_valid_from,assignment_valid_until,due_at,
                escalation_after_minutes,exception_status,created_at,updated_at
         FROM erpx_operations_write.exception_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND exception_id=$1{AUTHORITY_FILTER}"
    );
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let row = sqlx::query(&sql)
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
    let fact = load_source_fact(&mut tx, id).await?;
    let source_context: BoundedContext = parse_scalar(
        row.try_get::<String, _>("source_context")
            .map_err(|_| PortError::Internal)?,
    )?;
    if source_context != fact.source_context() {
        return Err(PortError::Internal);
    }
    let value = OperationsException::try_rehydrate(OperationsExceptionState {
        id,
        version: row_u64(&row, "aggregate_version")?,
        reference: row.try_get("reference").map_err(|_| PortError::Internal)?,
        scope: scope_from_row(&row)?,
        source: SourceReference {
            context: source_context,
            event_id: EventId::try_from_uuid(
                row.try_get("source_event_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            event_type: row
                .try_get("source_event_type")
                .map_err(|_| PortError::Internal)?,
            aggregate_kind: row
                .try_get("source_aggregate_type")
                .map_err(|_| PortError::Internal)?,
            aggregate_id: AggregateId::try_from_uuid(
                row.try_get("source_aggregate_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            aggregate_version: row_u64(&row, "source_aggregate_version")?,
            correlation_id: CorrelationId::try_from_uuid(
                row.try_get("source_correlation_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            occurred_at: row
                .try_get("source_occurred_at")
                .map_err(|_| PortError::Internal)?,
        },
        source_fact: fact,
        policy_ref: row.try_get("policy_ref").map_err(|_| PortError::Internal)?,
        category: row.try_get("category").map_err(|_| PortError::Internal)?,
        severity: row.try_get("severity").map_err(|_| PortError::Internal)?,
        assignment: PrincipalAssignment {
            assignee: erpx_integration_contracts::identity_access::PrincipalRef::try_from_uuid(
                row.try_get("assignee_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            manager: erpx_integration_contracts::identity_access::PrincipalRef::try_from_uuid(
                row.try_get("manager_id").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            authority_ref: row
                .try_get("assignment_authority_ref")
                .map_err(|_| PortError::Internal)?,
            valid_from: row
                .try_get("assignment_valid_from")
                .map_err(|_| PortError::Internal)?,
            valid_until: row
                .try_get("assignment_valid_until")
                .map_err(|_| PortError::Internal)?,
        },
        due_at: row.try_get("due_at").map_err(|_| PortError::Internal)?,
        escalation_after_minutes: row_u32(&row, "escalation_after_minutes")?,
        status: parse_scalar(
            row.try_get::<String, _>("exception_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    })
    .map_err(|_| PortError::Internal)?;
    tx.commit().await.map_err(|_| PortError::Unavailable)?;
    Ok(Some(value))
}

pub(crate) async fn load_policy_authority(
    pool: &WritePool,
    authority_scope: &TenantScope,
    id: WorkQueuePolicyId,
) -> Result<Option<WorkQueuePolicy>, PortError> {
    let authority = AuthorityScope::from_scope(authority_scope)?;
    let sql = "SELECT aggregate_version,holding_id,policy_name,escalation_after_minutes,
                      policy_status,created_at,updated_at
               FROM erpx_operations_write.work_queue_policy_authority
               WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND policy_id=$1
                 AND ($2='platform' OR ($2 IN ('holding','subtree') AND holding_id=$3))";
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let row = sqlx::query(sql)
        .bind(id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let sources = sqlx::query(
        "SELECT source_context FROM erpx_operations_write.work_queue_policy_source_authority
         WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND policy_id=$1
         ORDER BY source_context",
    )
    .bind(id.as_uuid())
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| PortError::Unavailable)?
    .into_iter()
    .map(|source| {
        parse_scalar(
            source
                .try_get::<String, _>("source_context")
                .map_err(|_| PortError::Internal)?,
        )
    })
    .collect::<Result<Vec<_>, PortError>>()?;
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let value = WorkQueuePolicy::try_rehydrate(WorkQueuePolicyState {
        id,
        version: row_u64(&row, "aggregate_version")?,
        scope: TenantScope::Holding { holding_id },
        name: row
            .try_get("policy_name")
            .map_err(|_| PortError::Internal)?,
        allowed_sources: sources,
        escalation_after_minutes: row_u32(&row, "escalation_after_minutes")?,
        status: parse_scalar(
            row.try_get::<String, _>("policy_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    })
    .map_err(|_| PortError::Internal)?;
    tx.commit().await.map_err(|_| PortError::Unavailable)?;
    Ok(Some(value))
}

pub(crate) async fn apply_exception_authority(
    tx: &mut Transaction<'_, Postgres>,
    value: &OperationsException,
) -> Result<(), CqrsSqlxError> {
    let scope = scope_columns(value.scope())?;
    let source = value.source();
    let result = sqlx::query(
        "INSERT INTO erpx_operations_write.exception_authority
         (tenant_id,exception_id,aggregate_version,scope_key,scope_mode,holding_id,company_id,
          branch_id,reference,source_context,source_event_id,source_event_type,source_aggregate_type,
          source_aggregate_id,source_aggregate_version,source_correlation_id,source_occurred_at,
          policy_ref,category,severity,assignee_id,manager_id,assignment_authority_ref,
          assignment_valid_from,assignment_valid_until,due_at,escalation_after_minutes,
          exception_status,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,
                 $14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29)
         ON CONFLICT (tenant_id,exception_id) DO UPDATE SET
          aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,
          scope_mode=EXCLUDED.scope_mode,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,
          branch_id=EXCLUDED.branch_id,policy_ref=EXCLUDED.policy_ref,category=EXCLUDED.category,
          severity=EXCLUDED.severity,assignee_id=EXCLUDED.assignee_id,manager_id=EXCLUDED.manager_id,
          assignment_authority_ref=EXCLUDED.assignment_authority_ref,
          assignment_valid_from=EXCLUDED.assignment_valid_from,
          assignment_valid_until=EXCLUDED.assignment_valid_until,due_at=EXCLUDED.due_at,
          escalation_after_minutes=EXCLUDED.escalation_after_minutes,
          exception_status=EXCLUDED.exception_status,updated_at=EXCLUDED.updated_at
         WHERE erpx_operations_write.exception_authority.aggregate_version+1=EXCLUDED.aggregate_version",
    )
    .bind(value.id().as_uuid()).bind(to_i64(value.version())?).bind(scope_key(value.scope())?)
    .bind(scope.mode).bind(scope.holding_id).bind(scope.company_id).bind(scope.branch_id).bind(value.reference())
    .bind(scalar_name(source.context)?).bind(source.event_id.as_uuid()).bind(&source.event_type)
    .bind(&source.aggregate_kind).bind(source.aggregate_id.as_uuid())
    .bind(to_i64(source.aggregate_version)?).bind(source.correlation_id.as_uuid())
    .bind(source.occurred_at).bind(value.policy_ref()).bind(value.category()).bind(value.severity())
    .bind(value.assignment().assignee.as_uuid()).bind(value.assignment().manager.as_uuid())
    .bind(&value.assignment().authority_ref).bind(value.assignment().valid_from)
    .bind(value.assignment().valid_until).bind(value.due_at())
    .bind(i32::try_from(value.escalation_after_minutes()).map_err(|_| CqrsSqlxError::NumericRange)?)
    .bind(exception_status_name(value.status())).bind(value.created_at()).bind(value.updated_at())
    .execute(&mut **tx).await?;
    if result.rows_affected() != 1 {
        return Err(CqrsSqlxError::GuardRejected(
            "Operations exception authority version changed",
        ));
    }
    replace_source_fact(tx, value.id(), value.version(), value.source_fact()).await
}

pub(crate) async fn apply_policy_authority(
    tx: &mut Transaction<'_, Postgres>,
    value: &WorkQueuePolicy,
) -> Result<(), CqrsSqlxError> {
    let TenantScope::Holding { holding_id } = value.scope() else {
        return Err(CqrsSqlxError::InvalidInput(
            "Operations policy scope must be holding",
        ));
    };
    let version = to_i64(value.version())?;
    let result=sqlx::query(
        "INSERT INTO erpx_operations_write.work_queue_policy_authority
         (tenant_id,policy_id,aggregate_version,scope_key,holding_id,policy_name,
          escalation_after_minutes,policy_status,created_at,updated_at)
         VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9)
         ON CONFLICT (tenant_id,policy_id) DO UPDATE SET
          aggregate_version=EXCLUDED.aggregate_version,policy_name=EXCLUDED.policy_name,
          escalation_after_minutes=EXCLUDED.escalation_after_minutes,
          policy_status=EXCLUDED.policy_status,updated_at=EXCLUDED.updated_at
         WHERE erpx_operations_write.work_queue_policy_authority.aggregate_version+1=EXCLUDED.aggregate_version",
    ).bind(value.id().as_uuid()).bind(version).bind(scope_key(value.scope())?).bind(*holding_id)
      .bind(value.name()).bind(i32::try_from(value.escalation_after_minutes()).map_err(|_| CqrsSqlxError::NumericRange)?)
      .bind(policy_status_name(value.status())).bind(value.created_at()).bind(value.updated_at())
      .execute(&mut **tx).await?;
    if result.rows_affected() != 1 {
        return Err(CqrsSqlxError::GuardRejected(
            "Operations policy authority version changed",
        ));
    }
    sqlx::query("DELETE FROM erpx_operations_write.work_queue_policy_source_authority WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND policy_id=$1")
        .bind(value.id().as_uuid()).execute(&mut **tx).await?;
    for source in value.allowed_sources() {
        sqlx::query("INSERT INTO erpx_operations_write.work_queue_policy_source_authority (tenant_id,policy_id,source_context,policy_version) VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3)")
            .bind(value.id().as_uuid()).bind(scalar_name(*source)?).bind(version)
            .execute(&mut **tx).await?;
    }
    Ok(())
}

struct ExactScopeColumns {
    mode: &'static str,
    holding_id: Uuid,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
}

fn scope_columns(scope: &TenantScope) -> Result<ExactScopeColumns, CqrsSqlxError> {
    match scope {
        TenantScope::Holding { holding_id } => Ok(ExactScopeColumns {
            mode: "holding",
            holding_id: *holding_id,
            company_id: None,
            branch_id: None,
        }),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(ExactScopeColumns {
            mode: "company",
            holding_id: *holding_id,
            company_id: Some(*company_id),
            branch_id: None,
        }),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(ExactScopeColumns {
            mode: "branch",
            holding_id: *holding_id,
            company_id: Some(*company_id),
            branch_id: Some(*branch_id),
        }),
        TenantScope::Platform | TenantScope::Subtree { .. } => Err(CqrsSqlxError::InvalidInput(
            "Operations scope must be exact",
        )),
    }
}

fn scope_from_row(row: &sqlx::postgres::PgRow) -> Result<TenantScope, PortError> {
    let mode: String = row.try_get("scope_mode").map_err(|_| PortError::Internal)?;
    let holding_id: Uuid = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id: Option<Uuid> = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
    let scope = match (mode.as_str(), company_id, branch_id) {
        ("holding", None, None) => TenantScope::Holding { holding_id },
        ("company", Some(company_id), None) => TenantScope::Company {
            holding_id,
            company_id,
        },
        ("branch", Some(company_id), Some(branch_id)) => TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        _ => return Err(PortError::Internal),
    };
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
fn row_u32(row: &sqlx::postgres::PgRow, column: &str) -> Result<u32, PortError> {
    u32::try_from(
        row.try_get::<i32, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn scalar_name<T: Serialize>(value: T) -> Result<String, CqrsSqlxError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or(CqrsSqlxError::InvalidInput("scalar enum cannot be encoded"))
}
fn parse_scalar<T: DeserializeOwned>(value: String) -> Result<T, PortError> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| PortError::Internal)
}

// Source-specific relational mapping is below.
