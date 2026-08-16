use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_customer_authority::{
    CustomerAuthorityView, CustomerCode, CustomerId, CustomerList, CustomerListFilter, PortError,
    ReadProjection, ReadRepository,
};
use erpx_kernel::TenantScope;
use sqlx::Row;
use uuid::Uuid;

use crate::{PROJECTION_NAME, customer_partition, projection::status_name};

#[derive(Debug, Clone)]
pub struct CustomerAuthorityReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl CustomerAuthorityReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }

    pub fn with_waiter_config(
        pool: ReadPool,
        waiter_config: WaiterConfig,
    ) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), waiter_config)?,
            pool,
        })
    }

    async fn consistency_checkpoint(
        &self,
        consistency: &QueryConsistency,
        exact_partition: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact_partition {
                Some(partition) => self
                    .read_side
                    .checkpoint(PROJECTION_NAME, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid_partition = requirement
                    .partition
                    .strip_prefix("customer_authority.customer:")
                    .is_some_and(|value| Uuid::parse_str(value).is_ok());
                if requirement.projection != PROJECTION_NAME
                    || !valid_partition
                    || exact_partition.is_some_and(|partition| partition != requirement.partition)
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
                        WaitError::NotReady(not_ready) => PortError::NotReady(not_ready),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }

    async fn metadata(
        &self,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status
             FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status IN ('active', 'building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(PROJECTION_NAME)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint.as_ref().and_then(|value| {
            Utc::now()
                .signed_duration_since(value.updated_at)
                .to_std()
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        });
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

#[async_trait]
impl ReadRepository for CustomerAuthorityReadRepository {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        customer_id: CustomerId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CustomerAuthorityView>>, PortError> {
        let partition = customer_partition(customer_id);
        let checkpoint = self
            .consistency_checkpoint(consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from_scope(authority_scope)?;
        let row = sqlx::query(
            "SELECT customer.state
             FROM erpx_customer_authority_read.customer_projection customer
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = customer.projection_name
              AND generation.generation = customer.generation
              AND generation.status = 'active'
             WHERE customer.projection_name = $1
               AND customer.customer_id = $2 AND customer.partition_key = $3
               AND ($4 OR EXISTS (
                    SELECT 1
                    FROM erpx_customer_authority_read.authority_link_projection authority
                    WHERE authority.projection_name = customer.projection_name
                      AND authority.generation = customer.generation
                      AND authority.partition_key = customer.partition_key
                      AND authority.customer_id = customer.customer_id
                      AND (authority.scope_key = ANY($5::text[])
                           OR authority.scope_key LIKE ANY($6::text[]))
               ))",
        )
        .bind(PROJECTION_NAME)
        .bind(customer_id.as_uuid())
        .bind(&partition)
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let view = row.map(decode_view).transpose()?;
        if view.as_ref().is_some_and(|view| {
            view.customer_id != customer_id || !view.visible_under(authority_scope)
        }) {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value: view,
            metadata: self.metadata(checkpoint).await?,
        })
    }

    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &CustomerListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<CustomerList>, PortError> {
        let checkpoint = self.consistency_checkpoint(consistency, None).await?;
        let scope = ScopeFilter::from_scope(authority_scope)?;
        let search = filter
            .search
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(escaped_search_pattern);
        let status = filter.status.map(status_name);
        let after_code = filter.after_code.as_ref().map(CustomerCode::as_str);
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query(
            "SELECT customer.state
             FROM erpx_customer_authority_read.customer_projection customer
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = customer.projection_name
              AND generation.generation = customer.generation
              AND generation.status = 'active'
             WHERE customer.projection_name = $1
               AND ($2 OR EXISTS (
                    SELECT 1
                    FROM erpx_customer_authority_read.authority_link_projection authority
                    WHERE authority.projection_name = customer.projection_name
                      AND authority.generation = customer.generation
                      AND authority.partition_key = customer.partition_key
                      AND authority.customer_id = customer.customer_id
                      AND (authority.scope_key = ANY($3::text[])
                           OR authority.scope_key LIKE ANY($4::text[]))
               ))
               AND ($5::text IS NULL OR customer.customer_status = $5)
               AND ($6 OR customer.merged_into_customer_id IS NULL)
               AND ($7::text IS NULL OR customer.customer_code ILIKE $7 ESCAPE '\\'
                    OR customer.trading_name ILIKE $7 ESCAPE '\\')
               AND ($8::text IS NULL OR customer.customer_code > $8)
             ORDER BY customer.customer_code ASC, customer.customer_id ASC
             LIMIT $9",
        )
        .bind(PROJECTION_NAME)
        .bind(scope.platform)
        .bind(&scope.exact)
        .bind(&scope.prefixes)
        .bind(status)
        .bind(filter.include_merged)
        .bind(search)
        .bind(after_code)
        .bind(limit)
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut customers = rows
            .into_iter()
            .map(decode_view)
            .collect::<Result<Vec<_>, _>>()?;
        if customers
            .iter()
            .any(|view| !view.visible_under(authority_scope))
        {
            return Err(PortError::Internal);
        }
        let next_cursor = if customers.len() > usize::from(filter.page_size) {
            customers.truncate(usize::from(filter.page_size));
            customers.last().map(|view| view.code.clone())
        } else {
            None
        };
        Ok(ReadProjection {
            value: CustomerList {
                customers,
                next_cursor,
            },
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ScopeFilter {
    platform: bool,
    exact: Vec<String>,
    prefixes: Vec<String>,
}

impl ScopeFilter {
    fn from_scope(scope: &TenantScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            TenantScope::Platform => Self {
                platform: true,
                exact: Vec::new(),
                prefixes: Vec::new(),
            },
            TenantScope::Holding { holding_id } => {
                let holding = format!("h:{holding_id}");
                Self {
                    platform: false,
                    exact: vec![holding.clone()],
                    prefixes: vec![format!("{holding}/%")],
                }
            }
            TenantScope::Company {
                holding_id,
                company_id,
            } => {
                let holding = format!("h:{holding_id}");
                let company = format!("{holding}/c:{company_id}");
                Self {
                    platform: false,
                    exact: vec![holding, company.clone()],
                    prefixes: vec![format!("{company}/%")],
                }
            }
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => {
                let holding = format!("h:{holding_id}");
                let company = format!("{holding}/c:{company_id}");
                Self {
                    platform: false,
                    exact: vec![holding, company.clone(), format!("{company}/b:{branch_id}")],
                    prefixes: Vec::new(),
                }
            }
            TenantScope::Subtree {
                holding_id,
                company_ids,
                branch_ids,
            } => {
                let holding = format!("h:{holding_id}");
                let mut exact = vec![holding];
                if branch_ids.is_empty() {
                    exact.extend(
                        company_ids
                            .iter()
                            .map(|company| format!("h:{holding_id}/c:{company}")),
                    );
                } else {
                    exact.extend(company_ids.iter().flat_map(|company| {
                        branch_ids
                            .iter()
                            .map(move |branch| format!("h:{holding_id}/c:{company}/b:{branch}"))
                    }));
                }
                Self {
                    platform: false,
                    exact,
                    prefixes: Vec::new(),
                }
            }
        })
    }
}

fn escaped_search_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn decode_view(row: sqlx::postgres::PgRow) -> Result<CustomerAuthorityView, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    let view: CustomerAuthorityView =
        serde_json::from_value(value).map_err(|_| PortError::Internal)?;
    if CustomerCode::try_new(view.code.as_str()).ok().as_ref() != Some(&view.code)
        || !view.authority_links.contains(&view.primary_authority)
    {
        return Err(PortError::Internal);
    }
    Ok(view)
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
    fn search_wildcards_are_escaped() {
        assert_eq!(escaped_search_pattern("A%_\\B"), "%A\\%\\_\\\\B%");
    }

    #[test]
    fn branch_scope_includes_only_its_ancestors_and_exact_branch() {
        let filter = ScopeFilter::from_scope(&TenantScope::Branch {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: Uuid::from_u128(3),
        })
        .unwrap();
        assert_eq!(filter.exact.len(), 3);
        assert!(filter.exact[0].starts_with("h:"));
        assert!(filter.exact[2].contains("/b:"));
        assert!(filter.prefixes.is_empty());
    }

    #[test]
    fn read_your_writes_partition_parser_accepts_stable_partition() {
        let customer = CustomerId::try_from_uuid(Uuid::from_u128(9)).unwrap();
        let partition = customer_partition(customer);
        assert!(
            partition
                .strip_prefix("customer_authority.customer:")
                .is_some_and(|value| Uuid::parse_str(value).is_ok())
        );
    }
}
