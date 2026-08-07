use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_operations::ports::{
    ExceptionQueue, ExceptionView, OperationsSourceCount, OperationsSummary, QueueFilter,
    ReadProjection, WorkQueuePolicyList, WorkQueuePolicyListFilter, WorkQueuePolicyView,
};
use erpx_operations::{ExceptionId, PortError, ReadRepository};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

use crate::{
    EXCEPTION_PROJECTION, POLICY_PROJECTION, SOURCE_SIGNAL_PROJECTION, exception_partition,
};

#[derive(Debug, Clone)]
pub struct OperationsReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl OperationsReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }

    pub fn with_waiter_config(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
        })
    }

    async fn checkpoint(
        &self,
        projection: &str,
        consistency: &QueryConsistency,
        exact: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact {
                Some(partition) => self
                    .read_side
                    .checkpoint(projection, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != projection
                    || !valid_partition(projection, &requirement.partition)
                    || exact.is_some_and(|partition| partition != requirement.partition)
                {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: requirement.projection.clone(),
                        partition: requirement.partition.clone(),
                        target: requirement.target,
                        observed: None,
                    }));
                }
                self.waiter
                    .wait_until_checked(requirement)
                    .await
                    .map(Some)
                    .map_err(|error| match error {
                        WaitError::NotReady(value) => PortError::NotReady(value),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }

    async fn metadata(
        &self,
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM erpx_cqrs_read.projection_generations WHERE projection_name=$1 AND status IN ('active','building') ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1")
            .bind(projection).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint.as_ref().and_then(|value| {
            (Utc::now() - value.updated_at)
                .num_milliseconds()
                .try_into()
                .ok()
        });
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

#[async_trait]
impl ReadRepository for OperationsReadRepository {
    async fn get_exception(
        &self,
        authority: &TenantScope,
        id: ExceptionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ExceptionView>>, PortError> {
        let partition = exception_partition(id);
        let checkpoint = self
            .checkpoint(EXCEPTION_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_operations_read.exception_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.exception_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(EXCEPTION_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value.as_ref().is_some_and(|value: &ExceptionView| {
            value.exception_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(EXCEPTION_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_queue(
        &self,
        authority: &TenantScope,
        filter: &QueueFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ExceptionQueue>, PortError> {
        let checkpoint = self
            .checkpoint(EXCEPTION_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let statuses = filter
            .statuses
            .iter()
            .map(|v| crate::projection::exception_status_name(*v).to_owned())
            .collect::<Vec<_>>();
        let source_context = filter
            .source_context
            .map(crate::projection::source_context_name);
        let assignee = filter.assignee.map(|v| v.as_uuid().to_string());
        let rows = sqlx::query("SELECT item.state FROM erpx_operations_read.exception_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND (cardinality($5::text[])=0 OR item.exception_status=ANY($5::text[])) AND ($6::text IS NULL OR item.source_context=$6) AND ($7::text IS NULL OR item.assignee_id=$7) AND ($8::timestamptz IS NULL OR item.due_at<=$8) AND ($9::uuid IS NULL OR item.exception_id>$9) ORDER BY item.exception_id LIMIT $10")
            .bind(EXCEPTION_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).bind(&statuses).bind(source_context).bind(assignee).bind(filter.due_before).bind(after).bind(i64::from(filter.page_size)+1)
            .fetch_all(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<ExceptionView>(rows)?;
        validate_scopes(authority, values.iter().map(|v| &v.scope))?;
        let next_cursor =
            truncate_cursor(&mut values, filter.page_size, |v| v.exception_id.as_uuid());
        Ok(ReadProjection {
            value: ExceptionQueue {
                exceptions: values,
                next_cursor,
            },
            metadata: self.metadata(EXCEPTION_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_work_queue_policies(
        &self,
        authority: &TenantScope,
        filter: &WorkQueuePolicyListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<WorkQueuePolicyList>, PortError> {
        let checkpoint = self
            .checkpoint(POLICY_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows = sqlx::query("SELECT item.state FROM erpx_operations_read.work_queue_policy_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::uuid IS NULL OR item.policy_id>$5) ORDER BY item.policy_id LIMIT $6")
            .bind(POLICY_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).bind(after).bind(i64::from(filter.page_size)+1)
            .fetch_all(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<WorkQueuePolicyView>(rows)?;
        validate_scopes(authority, values.iter().map(|v| &v.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |v| v.policy_id.as_uuid());
        Ok(ReadProjection {
            value: WorkQueuePolicyList {
                policies: values,
                next_cursor,
            },
            metadata: self.metadata(POLICY_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_summary(
        &self,
        authority: &TenantScope,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<OperationsSummary>, PortError> {
        let projection = match consistency {
            QueryConsistency::ReadYourWrites(v)
                if v.projection == EXCEPTION_PROJECTION
                    || v.projection == POLICY_PROJECTION
                    || v.projection == SOURCE_SIGNAL_PROJECTION =>
            {
                v.projection.as_str()
            }
            QueryConsistency::ReadYourWrites(v) => {
                return Err(PortError::NotReady(ProjectionNotReady {
                    projection: v.projection.clone(),
                    partition: v.partition.clone(),
                    target: v.target,
                    observed: None,
                }));
            }
            QueryConsistency::Eventual => EXCEPTION_PROJECTION,
        };
        let checkpoint = self.checkpoint(projection, consistency, None).await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("WITH active AS (SELECT projection_name,generation FROM erpx_cqrs_read.projection_generations WHERE status='active'), exceptions AS (SELECT e.* FROM erpx_operations_read.exception_projection e JOIN active a USING(projection_name,generation) WHERE ($1 OR e.scope_key=ANY($2::text[]) OR e.scope_key LIKE ANY($3::text[]))), policies AS (SELECT p.* FROM erpx_operations_read.work_queue_policy_projection p JOIN active a USING(projection_name,generation) WHERE ($1 OR p.scope_key=ANY($2::text[]) OR p.scope_key LIKE ANY($3::text[]))), signals AS (SELECT s.* FROM erpx_operations_read.source_signal_projection s JOIN active a USING(projection_name,generation) WHERE ($1 OR s.scope_key=ANY($2::text[]) OR s.scope_key LIKE ANY($3::text[]))) SELECT (SELECT count(*) FROM exceptions)+(SELECT count(*) FROM policies)+(SELECT count(*) FROM signals) AS read_rows,(SELECT count(*) FROM exceptions) AS exceptions,(SELECT count(*) FROM policies) AS policies,(SELECT count(*) FROM (SELECT source_context,source_event_id FROM exceptions UNION SELECT 'operations',policy_id FROM policies) x) AS events,(SELECT count(*) FROM signals) AS source_refs,(SELECT max(observed_at) FROM signals) AS latest")
            .bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_one(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let source_rows = sqlx::query("SELECT s.source_context,count(*) AS reference_count FROM erpx_operations_read.source_signal_projection s JOIN erpx_cqrs_read.projection_generations g ON g.projection_name=s.projection_name AND g.generation=s.generation AND g.status='active' WHERE ($1 OR s.scope_key=ANY($2::text[]) OR s.scope_key LIKE ANY($3::text[])) GROUP BY s.source_context ORDER BY s.source_context")
            .bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_all(self.pool.as_pg_pool()).await.map_err(|_| PortError::Unavailable)?;
        let source_counts = source_rows
            .into_iter()
            .map(|row| {
                Ok(OperationsSourceCount {
                    source_context: parse_source_context(
                        &row.try_get::<String, _>("source_context")
                            .map_err(|_| PortError::Internal)?,
                    )?,
                    reference_count: u64::try_from(
                        row.try_get::<i64, _>("reference_count")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        Ok(ReadProjection {
            value: OperationsSummary {
                read_row_count: read_count(&row, "read_rows")?,
                exception_count: read_count(&row, "exceptions")?,
                work_queue_policy_count: read_count(&row, "policies")?,
                event_count: read_count(&row, "events")?,
                source_reference_count: read_count(&row, "source_refs")?,
                source_counts,
                latest_event_at: row.try_get("latest").map_err(|_| PortError::Internal)?,
            },
            metadata: self.metadata(projection, checkpoint).await?,
        })
    }
}

pub fn encode_cursor(id: Uuid) -> String {
    id.to_string()
}
pub fn decode_cursor(value: &str) -> Result<Uuid, PortError> {
    Uuid::parse_str(value)
        .map_err(|_| PortError::Internal)
        .and_then(|id| {
            if id.is_nil() {
                Err(PortError::Internal)
            } else {
                Ok(id)
            }
        })
}
fn decode_state<T: serde::de::DeserializeOwned>(
    row: sqlx::postgres::PgRow,
) -> Result<T, PortError> {
    serde_json::from_value(row.try_get("state").map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)
}
fn read_count(row: &PgRow, name: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(name)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}
fn decode_rows<T: serde::de::DeserializeOwned>(
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<Vec<T>, PortError> {
    rows.into_iter().map(decode_state).collect()
}
fn truncate_cursor<T>(
    values: &mut Vec<T>,
    page_size: u16,
    id: impl Fn(&T) -> Uuid,
) -> Option<String> {
    if values.len() > usize::from(page_size) {
        values.truncate(usize::from(page_size));
        values.last().map(|v| encode_cursor(id(v)))
    } else {
        None
    }
}
fn validate_scopes<'a>(
    authority: &TenantScope,
    scopes: impl Iterator<Item = &'a TenantScope>,
) -> Result<(), PortError> {
    if scopes.into_iter().all(|scope| authority.contains(scope)) {
        Ok(())
    } else {
        Err(PortError::Internal)
    }
}
fn valid_partition(projection: &str, partition: &str) -> bool {
    let prefix = match projection {
        EXCEPTION_PROJECTION | SOURCE_SIGNAL_PROJECTION => "operations.exception:",
        POLICY_PROJECTION => "operations.policy:",
        _ => return false,
    };
    partition
        .strip_prefix(prefix)
        .is_some_and(|v| v.len() == 32 && u128::from_str_radix(v, 16).is_ok())
}

struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
}
impl ScopeFilter {
    fn from(scope: &TenantScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            TenantScope::Platform => Self {
                platform: true,
                exact: vec![],
                prefixes: vec![],
            },
            TenantScope::Holding { holding_id } => {
                let key = format!("holding:{holding_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                }
            }
            TenantScope::Company {
                holding_id,
                company_id,
            } => {
                let key = format!("holding:{holding_id}/company:{company_id}");
                Self {
                    platform: false,
                    exact: vec![key.clone()],
                    prefixes: vec![format!("{key}/%")],
                }
            }
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                platform: false,
                exact: vec![format!(
                    "holding:{holding_id}/company:{company_id}/branch:{branch_id}"
                )],
                prefixes: vec![],
            },
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => Self {
                platform: false,
                exact: company_ids
                    .iter()
                    .flat_map(|company| {
                        if branch_ids.is_empty() {
                            vec![format!("holding:{holding_id}/company:{company}")]
                        } else {
                            branch_ids
                                .iter()
                                .map(|branch| {
                                    format!(
                                        "holding:{holding_id}/company:{company}/branch:{branch}"
                                    )
                                })
                                .collect()
                        }
                    })
                    .collect(),
                prefixes: vec![],
            },
        })
    }
}
fn parse_source_context(
    value: &str,
) -> Result<erpx_integration_contracts::BoundedContext, PortError> {
    use erpx_integration_contracts::BoundedContext;
    match value {
        "commercial" => Ok(BoundedContext::Commercial),
        "customer" => Ok(BoundedContext::Customer),
        "marketing" => Ok(BoundedContext::Marketing),
        "crm" => Ok(BoundedContext::Crm),
        "procurement" => Ok(BoundedContext::Procurement),
        "inventory" => Ok(BoundedContext::Inventory),
        "assets" => Ok(BoundedContext::Assets),
        "maintenance" => Ok(BoundedContext::Maintenance),
        "quality" => Ok(BoundedContext::Quality),
        "sales" => Ok(BoundedContext::Sales),
        "people_payroll" => Ok(BoundedContext::PeoplePayroll),
        "pos" => Ok(BoundedContext::Pos),
        "finance" => Ok(BoundedContext::Finance),
        "support" => Ok(BoundedContext::Support),
        _ => Err(PortError::Internal),
    }
}
fn map_read_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_and_projection_partitions_are_bounded() {
        let id = Uuid::from_u128(8);
        assert_eq!(decode_cursor(&encode_cursor(id)).unwrap(), id);
        let partition = exception_partition(ExceptionId::try_from_uuid(id).unwrap());
        assert!(valid_partition(EXCEPTION_PROJECTION, &partition));
        assert!(!valid_partition(POLICY_PROJECTION, &partition));
    }
    #[test]
    fn all_decoded_rows_are_post_validated_against_authority() {
        let source = include_str!("read.rs");
        assert!(source.matches("validate_scopes(authority").count() >= 2);
        assert!(source.contains("!authority.contains(&value.scope)"));
    }
}
