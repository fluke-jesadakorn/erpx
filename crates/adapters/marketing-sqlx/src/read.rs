use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_marketing::{
    CampaignBudgetId, CampaignBudgetView, LeadCandidateId, LeadCandidateList,
    LeadCandidateListFilter, LeadCandidateView, PortError, ReadRepository, ports::ReadProjection,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    BUDGET_PROJECTION_NAME, LEAD_PROJECTION_NAME, budget_partition, lead_partition,
    projection::lead_status_name,
};

#[derive(Debug, Clone)]
pub struct MarketingReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl MarketingReadRepository {
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
        partition_prefix: &str,
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
                let valid = requirement.projection == projection
                    && requirement
                        .partition
                        .strip_prefix(partition_prefix)
                        .is_some_and(|value| {
                            value.len() == 32 && u128::from_str_radix(value, 16).is_ok()
                        })
                    && exact.is_none_or(|partition| partition == requirement.partition);
                if !valid {
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
            .bind(projection)
            .fetch_optional(self.pool.as_pg_pool())
            .await
            .map_err(|_| PortError::Unavailable)?;
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
impl ReadRepository for MarketingReadRepository {
    async fn get_lead(
        &self,
        authority: &TenantScope,
        id: LeadCandidateId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<LeadCandidateView>>, PortError> {
        let partition = lead_partition(id);
        let checkpoint = self
            .checkpoint(
                LEAD_PROJECTION_NAME,
                "marketing.lead:",
                consistency,
                Some(&partition),
            )
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT lead.state FROM erpx_marketing_read.lead_candidate_projection lead JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=lead.projection_name AND generation.generation=lead.generation AND generation.status='active' WHERE lead.projection_name=$1 AND lead.partition_key=$2 AND lead.lead_id=$3 AND ($4 OR lead.scope_key=ANY($5::text[]) OR lead.scope_key LIKE ANY($6::text[]))")
            .bind(LEAD_PROJECTION_NAME)
            .bind(&partition)
            .bind(id.as_uuid())
            .bind(scope.platform)
            .bind(&scope.exact)
            .bind(&scope.prefixes)
            .fetch_optional(self.pool.as_pg_pool())
            .await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_lead).transpose()?;
        if value
            .as_ref()
            .is_some_and(|value| value.id != id || !authority.contains(&value.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(LEAD_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn list_leads(
        &self,
        authority: &TenantScope,
        filter: &LeadCandidateListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<LeadCandidateList>, PortError> {
        let checkpoint = self
            .checkpoint(LEAD_PROJECTION_NAME, "marketing.lead:", consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(lead_status_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query("SELECT lead.state FROM erpx_marketing_read.lead_candidate_projection lead JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=lead.projection_name AND generation.generation=lead.generation AND generation.status='active' WHERE lead.projection_name=$1 AND ($2 OR lead.scope_key=ANY($3::text[]) OR lead.scope_key LIKE ANY($4::text[])) AND ($5::text IS NULL OR lead.lead_status=$5) AND ($6::uuid IS NULL OR lead.lead_id>$6) ORDER BY lead.lead_id LIMIT $7")
            .bind(LEAD_PROJECTION_NAME)
            .bind(scope.platform)
            .bind(&scope.exact)
            .bind(&scope.prefixes)
            .bind(status)
            .bind(after)
            .bind(limit)
            .fetch_all(self.pool.as_pg_pool())
            .await
            .map_err(|_| PortError::Unavailable)?;
        let mut values = rows
            .into_iter()
            .map(decode_lead)
            .collect::<Result<Vec<_>, _>>()?;
        if values.iter().any(|value| !authority.contains(&value.scope)) {
            return Err(PortError::Internal);
        }
        let next_cursor = if values.len() > usize::from(filter.page_size) {
            values.truncate(usize::from(filter.page_size));
            values.last().map(|value| encode_cursor(value.id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: LeadCandidateList {
                candidates: values,
                next_cursor,
            },
            metadata: self.metadata(LEAD_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn get_budget(
        &self,
        authority: &TenantScope,
        id: CampaignBudgetId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CampaignBudgetView>>, PortError> {
        let partition = budget_partition(id);
        let checkpoint = self
            .checkpoint(
                BUDGET_PROJECTION_NAME,
                "marketing.budget:",
                consistency,
                Some(&partition),
            )
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row = sqlx::query("SELECT budget.state FROM erpx_marketing_read.campaign_budget_projection budget JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=budget.projection_name AND generation.generation=budget.generation AND generation.status='active' WHERE budget.projection_name=$1 AND budget.partition_key=$2 AND budget.budget_id=$3 AND ($4 OR budget.scope_key=ANY($5::text[]) OR budget.scope_key LIKE ANY($6::text[]))")
            .bind(BUDGET_PROJECTION_NAME)
            .bind(&partition)
            .bind(id.as_uuid())
            .bind(scope.platform)
            .bind(&scope.exact)
            .bind(&scope.prefixes)
            .fetch_optional(self.pool.as_pg_pool())
            .await
            .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_budget).transpose()?;
        if value
            .as_ref()
            .is_some_and(|value| value.id != id || !authority.contains(&value.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(BUDGET_PROJECTION_NAME, checkpoint).await?,
        })
    }
}

pub fn encode_cursor(id: LeadCandidateId) -> String {
    id.as_uuid().to_string()
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

fn decode_lead(row: sqlx::postgres::PgRow) -> Result<LeadCandidateView, PortError> {
    let state: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(state).map_err(|_| PortError::Internal)
}

fn decode_budget(row: sqlx::postgres::PgRow) -> Result<CampaignBudgetView, PortError> {
    let state: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(state).map_err(|_| PortError::Internal)
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
    fn cursor_round_trips() {
        let id = LeadCandidateId::try_from_uuid(Uuid::from_u128(7)).unwrap();
        assert_eq!(decode_cursor(&encode_cursor(id)).unwrap(), id.as_uuid());
    }

    #[test]
    fn holding_scope_uses_descendant_prefix() {
        let filter = ScopeFilter::from(&TenantScope::Holding {
            holding_id: Uuid::from_u128(1),
        })
        .unwrap();
        assert_eq!(filter.prefixes.len(), 1);
    }
}
