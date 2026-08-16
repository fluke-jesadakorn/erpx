use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_people_payroll::{
    EmployeeId, ExpenseClaimId, LeaveRequestId, PayrollRunId, PortError, ReadRepository,
    ports::{
        EmployeeList, EmployeeListFilter, EmployeeView, ExpenseClaimList, ExpenseClaimListFilter,
        ExpenseClaimView, LeaveRequestList, LeaveRequestListFilter, LeaveRequestView,
        PayrollRunList, PayrollRunListFilter, PayrollRunView, ReadProjection,
    },
};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    EMPLOYEE_PROJECTION, EXPENSE_PROJECTION, LEAVE_PROJECTION, PAYROLL_PROJECTION,
    employee_partition, expense_partition, leave_partition, payroll_partition,
    projection::{
        employee_status_name, expense_status_name, leave_status_name, payroll_status_name,
    },
};

#[derive(Debug, Clone)]
pub struct PeoplePayrollReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl PeoplePayrollReadRepository {
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
            .bind(projection).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint
            .as_ref()
            .and_then(|value| {
                Utc::now()
                    .signed_duration_since(value.updated_at)
                    .to_std()
                    .ok()
            })
            .and_then(|value| u64::try_from(value.as_millis()).ok());
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

#[async_trait]
impl ReadRepository for PeoplePayrollReadRepository {
    async fn get_employee(
        &self,
        authority: &TenantScope,
        id: EmployeeId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<EmployeeView>>, PortError> {
        let partition = employee_partition(id);
        let checkpoint = self
            .checkpoint(EMPLOYEE_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.employee_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.employee_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(EMPLOYEE_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform)
            .bind(&scope.exact).bind(&scope.prefixes).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value.as_ref().is_some_and(|value: &EmployeeView| {
            value.employee_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(EMPLOYEE_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_employees(
        &self,
        authority: &TenantScope,
        filter: &EmployeeListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<EmployeeList>, PortError> {
        let checkpoint = self
            .checkpoint(EMPLOYEE_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(employee_status_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.employee_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::text IS NULL OR item.employee_status=$5) AND ($6::uuid IS NULL OR item.employee_id>$6) ORDER BY item.employee_id LIMIT $7")
            .bind(EMPLOYEE_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
            .bind(status).bind(after).bind(limit).fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await
            .map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<EmployeeView>(rows)?;
        validate_scopes(authority, values.iter().map(|value| &value.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |value| {
            value.employee_id.as_uuid()
        });
        Ok(ReadProjection {
            value: EmployeeList {
                employees: values,
                next_cursor,
            },
            metadata: self.metadata(EMPLOYEE_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_leave_request(
        &self,
        authority: &TenantScope,
        id: LeaveRequestId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<LeaveRequestView>>, PortError> {
        let partition = leave_partition(id);
        let checkpoint = self
            .checkpoint(LEAVE_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.leave_request_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.leave_request_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(LEAVE_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform)
            .bind(&scope.exact).bind(&scope.prefixes).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value.as_ref().is_some_and(|value: &LeaveRequestView| {
            value.leave_request_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(LEAVE_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_leave_requests(
        &self,
        authority: &TenantScope,
        filter: &LeaveRequestListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<LeaveRequestList>, PortError> {
        let checkpoint = self.checkpoint(LEAVE_PROJECTION, consistency, None).await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(leave_status_name);
        let employee = filter.employee_id.map(|value| value.as_uuid());
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.leave_request_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::uuid IS NULL OR item.employee_id=$5) AND ($6::text IS NULL OR item.leave_status=$6) AND ($7::uuid IS NULL OR item.leave_request_id>$7) ORDER BY item.leave_request_id LIMIT $8")
            .bind(LEAVE_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
            .bind(employee).bind(status).bind(after).bind(limit).fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await
            .map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<LeaveRequestView>(rows)?;
        validate_scopes(authority, values.iter().map(|value| &value.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |value| {
            value.leave_request_id.as_uuid()
        });
        Ok(ReadProjection {
            value: LeaveRequestList {
                leave_requests: values,
                next_cursor,
            },
            metadata: self.metadata(LEAVE_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_expense_claim(
        &self,
        authority: &TenantScope,
        id: ExpenseClaimId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ExpenseClaimView>>, PortError> {
        let partition = expense_partition(id);
        let checkpoint = self
            .checkpoint(EXPENSE_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.expense_claim_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.expense_claim_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(EXPENSE_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform)
            .bind(&scope.exact).bind(&scope.prefixes).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value.as_ref().is_some_and(|value: &ExpenseClaimView| {
            value.expense_claim_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(EXPENSE_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_expense_claims(
        &self,
        authority: &TenantScope,
        filter: &ExpenseClaimListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<ExpenseClaimList>, PortError> {
        let checkpoint = self
            .checkpoint(EXPENSE_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(expense_status_name);
        let employee = filter.employee_id.map(|value| value.as_uuid());
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.expense_claim_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::uuid IS NULL OR item.employee_id=$5) AND ($6::text IS NULL OR item.expense_status=$6) AND ($7::uuid IS NULL OR item.expense_claim_id>$7) ORDER BY item.expense_claim_id LIMIT $8")
            .bind(EXPENSE_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
            .bind(employee).bind(status).bind(after).bind(limit).fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await
            .map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<ExpenseClaimView>(rows)?;
        validate_scopes(authority, values.iter().map(|value| &value.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |value| {
            value.expense_claim_id.as_uuid()
        });
        Ok(ReadProjection {
            value: ExpenseClaimList {
                expense_claims: values,
                next_cursor,
            },
            metadata: self.metadata(EXPENSE_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_payroll_run(
        &self,
        authority: &TenantScope,
        id: PayrollRunId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PayrollRunView>>, PortError> {
        let partition = payroll_partition(id);
        let checkpoint = self
            .checkpoint(PAYROLL_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.payroll_run_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.payroll_run_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))")
            .bind(PAYROLL_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform)
            .bind(&scope.exact).bind(&scope.prefixes).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value.as_ref().is_some_and(|value: &PayrollRunView| {
            value.payroll_run_id != id || !authority.contains(&value.scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(PAYROLL_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_payroll_runs(
        &self,
        authority: &TenantScope,
        filter: &PayrollRunListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PayrollRunList>, PortError> {
        let checkpoint = self
            .checkpoint(PAYROLL_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(payroll_status_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query("SELECT item.state FROM erpx_people_payroll_read.payroll_run_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::text IS NULL OR item.payroll_status=$5) AND ($6::date IS NULL OR item.starts_on >= $6) AND ($7::date IS NULL OR item.ends_on <= $7) AND ($8::uuid IS NULL OR item.payroll_run_id>$8) ORDER BY item.payroll_run_id LIMIT $9")
            .bind(PAYROLL_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes)
            .bind(status).bind(filter.starts_on_or_after).bind(filter.ends_on_or_before).bind(after).bind(limit)
            .fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_| PortError::Unavailable)?;
        let mut values = decode_rows::<PayrollRunView>(rows)?;
        validate_scopes(authority, values.iter().map(|value| &value.scope))?;
        let next_cursor = truncate_cursor(&mut values, filter.page_size, |value| {
            value.payroll_run_id.as_uuid()
        });
        Ok(ReadProjection {
            value: PayrollRunList {
                payroll_runs: values,
                next_cursor,
            },
            metadata: self.metadata(PAYROLL_PROJECTION, checkpoint).await?,
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
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
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
        values.last().map(|value| encode_cursor(id(value)))
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
        EMPLOYEE_PROJECTION => "people_payroll.employee:",
        LEAVE_PROJECTION => "people_payroll.leave_request:",
        EXPENSE_PROJECTION => "people_payroll.expense_claim:",
        PAYROLL_PROJECTION => "people_payroll.payroll_run:",
        _ => return false,
    };
    partition
        .strip_prefix(prefix)
        .is_some_and(|value| value.len() == 32 && u128::from_str_radix(value, 16).is_ok())
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
            TenantScope::Holding { holding_id } => Self {
                platform: false,
                exact: vec![],
                prefixes: vec![format!("h:{holding_id}/%")],
            },
            TenantScope::Company {
                holding_id,
                company_id,
            } => {
                let key = format!("h:{holding_id}/c:{company_id}");
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
                exact: vec![format!("h:{holding_id}/c:{company_id}/b:{branch_id}")],
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
                            vec![format!("h:{holding_id}/c:{company}")]
                        } else {
                            branch_ids
                                .iter()
                                .map(|branch| format!("h:{holding_id}/c:{company}/b:{branch}"))
                                .collect()
                        }
                    })
                    .collect(),
                prefixes: vec![],
            },
        })
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
    fn cursor_round_trips_and_partition_types_do_not_cross() {
        let id = Uuid::from_u128(8);
        assert_eq!(decode_cursor(&encode_cursor(id)).unwrap(), id);
        let employee = employee_partition(EmployeeId::try_from_uuid(id).unwrap());
        assert!(valid_partition(EMPLOYEE_PROJECTION, &employee));
        assert!(!valid_partition(PAYROLL_PROJECTION, &employee));
    }
    #[test]
    fn branch_scope_is_exact() {
        let filter = ScopeFilter::from(&TenantScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        })
        .unwrap();
        assert_eq!(filter.exact.len(), 1);
        assert!(filter.prefixes.is_empty());
    }
}
