use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_crm::{
    CrmLeadId, CrmLeadView, OpportunityId, OpportunityList, OpportunityListFilter, OpportunityView,
    PortError, ReadRepository, ports::ReadProjection,
};
use erpx_kernel::AuthorityScope;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    LEAD_PROJECTION, OPPORTUNITY_PROJECTION, lead_partition, opportunity_partition,
    projection::stage_name,
};

#[derive(Debug, Clone)]
pub struct CrmReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}
impl CrmReadRepository {
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
                let prefix = if projection == LEAD_PROJECTION {
                    "crm.lead:"
                } else {
                    "crm.opportunity:"
                };
                let valid = requirement.projection == projection
                    && requirement
                        .partition
                        .strip_prefix(prefix)
                        .is_some_and(|v| v.len() == 32 && u128::from_str_radix(v, 16).is_ok())
                    && exact.is_none_or(|v| v == requirement.partition);
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
                        WaitError::NotReady(v) => PortError::NotReady(v),
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
        let status:Option<String>=sqlx::query_scalar("SELECT status FROM erpx_cqrs_read.projection_generations WHERE projection_name=$1 AND status IN ('active','building') ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1").bind(projection).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint
            .as_ref()
            .and_then(|v| Utc::now().signed_duration_since(v.updated_at).to_std().ok())
            .and_then(|v| u64::try_from(v.as_millis()).ok());
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

#[async_trait]
impl ReadRepository for CrmReadRepository {
    async fn get_lead(
        &self,
        authority: &AuthorityScope,
        id: CrmLeadId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CrmLeadView>>, PortError> {
        let partition = lead_partition(id);
        let checkpoint = self
            .checkpoint(LEAD_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row=sqlx::query("SELECT item.state FROM erpx_crm_read.lead_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.lead_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))").bind(LEAD_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode_lead).transpose()?;
        if value
            .as_ref()
            .is_some_and(|v| v.id != id || !authority.contains(&v.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(LEAD_PROJECTION, checkpoint).await?,
        })
    }
    async fn get_opportunity(
        &self,
        authority: &AuthorityScope,
        id: OpportunityId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<OpportunityView>>, PortError> {
        let partition = opportunity_partition(id);
        let checkpoint = self
            .checkpoint(OPPORTUNITY_PROJECTION, consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let row=sqlx::query("SELECT item.state FROM erpx_crm_read.opportunity_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND item.partition_key=$2 AND item.opportunity_id=$3 AND ($4 OR item.scope_key=ANY($5::text[]) OR item.scope_key LIKE ANY($6::text[]))").bind(OPPORTUNITY_PROJECTION).bind(&partition).bind(id.as_uuid()).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode_opportunity).transpose()?;
        if value
            .as_ref()
            .is_some_and(|v| v.id != id || !authority.contains(&v.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(OPPORTUNITY_PROJECTION, checkpoint).await?,
        })
    }
    async fn list_opportunities(
        &self,
        authority: &AuthorityScope,
        filter: &OpportunityListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<OpportunityList>, PortError> {
        let checkpoint = self
            .checkpoint(OPPORTUNITY_PROJECTION, consistency, None)
            .await?;
        let scope = ScopeFilter::from(authority)?;
        let after = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let stage = filter.stage.map(stage_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows=sqlx::query("SELECT item.state FROM erpx_crm_read.opportunity_projection item JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=item.projection_name AND generation.generation=item.generation AND generation.status='active' WHERE item.projection_name=$1 AND ($2 OR item.scope_key=ANY($3::text[]) OR item.scope_key LIKE ANY($4::text[])) AND ($5::text IS NULL OR item.opportunity_stage=$5) AND ($6::uuid IS NULL OR item.customer_id=$6) AND ($7::uuid IS NULL OR item.opportunity_id>$7) ORDER BY item.opportunity_id LIMIT $8").bind(OPPORTUNITY_PROJECTION).bind(scope.platform).bind(&scope.exact).bind(&scope.prefixes).bind(stage).bind(filter.customer_id).bind(after).bind(limit).fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let mut values = rows
            .into_iter()
            .map(decode_opportunity)
            .collect::<Result<Vec<_>, _>>()?;
        if values.iter().any(|v| !authority.contains(&v.scope)) {
            return Err(PortError::Internal);
        }
        let next_cursor = if values.len() > usize::from(filter.page_size) {
            values.truncate(usize::from(filter.page_size));
            values.last().map(|v| encode_cursor(v.id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: OpportunityList {
                opportunities: values,
                next_cursor,
            },
            metadata: self.metadata(OPPORTUNITY_PROJECTION, checkpoint).await?,
        })
    }
}

pub fn encode_cursor(id: OpportunityId) -> String {
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
fn decode_lead(row: sqlx::postgres::PgRow) -> Result<CrmLeadView, PortError> {
    decode_state(row)
}
fn decode_opportunity(row: sqlx::postgres::PgRow) -> Result<OpportunityView, PortError> {
    decode_state(row)
}
fn decode_state<T: serde::de::DeserializeOwned>(
    row: sqlx::postgres::PgRow,
) -> Result<T, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}
struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
}
impl ScopeFilter {
    fn from(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                platform: true,
                exact: vec![],
                prefixes: vec![],
            },
            AuthorityScope::Holding { holding_id } => Self {
                platform: false,
                exact: vec![],
                prefixes: vec![format!("h:{holding_id}/%")],
            },
            AuthorityScope::Company {
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
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}/c:{company_id}/b:{branch_id}")],
                prefixes: vec![],
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                platform: false,
                exact: vec![format!("h:{holding_id}/c:{organization_node_id}")],
                prefixes: vec![format!("h:{holding_id}/c:{organization_node_id}/%")],
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
        let id = OpportunityId::try_from_uuid(Uuid::from_u128(8)).unwrap();
        assert_eq!(decode_cursor(&encode_cursor(id)).unwrap(), id.as_uuid());
    }
    #[test]
    fn branch_scope_is_exact() {
        let filter = ScopeFilter::from(&AuthorityScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        })
        .unwrap();
        assert_eq!(filter.exact.len(), 1);
        assert!(filter.prefixes.is_empty());
    }
}
