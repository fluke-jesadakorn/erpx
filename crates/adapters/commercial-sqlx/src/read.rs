use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_commercial::{
    CatalogPlanId, CatalogPlanView, EntitlementId, EntitlementView, Page, PageFilter, PortError,
    ReadProjection, ReadRepository, SubscriptionId, SubscriptionView,
};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::AuthorityScope;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    CATALOG_PLAN_PROJECTION, ENTITLEMENT_PROJECTION, SUBSCRIPTION_PROJECTION,
    catalog_plan_partition, entitlement_partition, subscription_partition,
};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct CommercialReadRepository {
    control: ReadPlane,
    holding: ReadPlane,
}

#[derive(Debug, Clone)]
struct ReadPlane {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl CommercialReadRepository {
    pub fn new(control_read: ReadPool, holding_read: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(control_read, holding_read, WaiterConfig::default())
    }

    pub fn with_waiter_config(
        control_read: ReadPool,
        holding_read: ReadPool,
        waiter_config: WaiterConfig,
    ) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            control: ReadPlane::new(control_read, waiter_config)?,
            holding: ReadPlane::new(holding_read, waiter_config)?,
        })
    }

    pub fn from_single_pool(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::new(pool.clone(), pool)
    }

    async fn checkpoint(
        &self,
        plane: &ReadPlane,
        projection: &str,
        prefix: &str,
        consistency: &QueryConsistency,
        exact_partition: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact_partition {
                Some(partition) => plane
                    .read_side
                    .checkpoint(projection, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid_partition = requirement
                    .partition
                    .strip_prefix(prefix)
                    .is_some_and(|id| Uuid::parse_str(id).is_ok());
                if requirement.projection != projection
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
                plane
                    .waiter
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
        plane: &ReadPlane,
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM erpx_cqrs_read.projection_generations WHERE projection_name=$1 AND status IN ('active','building') ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1")
            .bind(projection).fetch_optional(&mut *plane.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_| PortError::Unavailable)?;
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
            .and_then(|duration| u64::try_from(duration.as_millis()).ok());
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

impl ReadPlane {
    fn new(pool: ReadPool, waiter_config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), waiter_config)?,
            pool,
        })
    }
}

#[async_trait]
impl ReadRepository for CommercialReadRepository {
    async fn get_catalog_plan(
        &self,
        scope: &AuthorityScope,
        id: CatalogPlanId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CatalogPlanView>>, PortError> {
        let partition = catalog_plan_partition(id);
        let checkpoint = self
            .checkpoint(
                &self.control,
                CATALOG_PLAN_PROJECTION,
                "commercial.catalog_plan:",
                consistency,
                Some(&partition),
            )
            .await?;
        let value = if scope == &AuthorityScope::Platform {
            sqlx::query("SELECT projection.state FROM erpx_commercial_read.catalog_plan_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.catalog_plan_id=$2 AND projection.partition_key=$3")
                .bind(CATALOG_PLAN_PROJECTION).bind(id.as_uuid()).bind(&partition).fetch_optional(&mut *self.control.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_| PortError::Unavailable)?.map(decode).transpose()?
        } else {
            None
        };
        Ok(ReadProjection {
            value,
            metadata: self
                .metadata(&self.control, CATALOG_PLAN_PROJECTION, checkpoint)
                .await?,
        })
    }

    async fn list_catalog_plans(
        &self,
        scope: &AuthorityScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<CatalogPlanView>>, PortError> {
        let checkpoint = self
            .checkpoint(
                &self.control,
                CATALOG_PLAN_PROJECTION,
                "commercial.catalog_plan:",
                consistency,
                None,
            )
            .await?;
        if scope != &AuthorityScope::Platform {
            return Ok(ReadProjection {
                value: Page {
                    records: vec![],
                    next_cursor: None,
                },
                metadata: self
                    .metadata(&self.control, CATALOG_PLAN_PROJECTION, checkpoint)
                    .await?,
            });
        }
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows=sqlx::query("SELECT projection.state FROM erpx_commercial_read.catalog_plan_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2::timestamptz IS NULL OR (projection.aggregate_updated_at,projection.catalog_plan_id)<($2,$3)) ORDER BY projection.aggregate_updated_at DESC,projection.catalog_plan_id DESC LIMIT $4")
            .bind(CATALOG_PLAN_PROJECTION).bind(cursor.map(|v|v.0)).bind(cursor.map(|v|v.1)).bind(i64::from(filter.page_size)+1)
            .fetch_all(&mut *self.control.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_| PortError::Unavailable)?;
        let mut records = rows
            .into_iter()
            .map(decode)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = paginate(&mut records, filter.page_size, |value: &CatalogPlanView| {
            (value.updated_at, value.id.as_uuid())
        });
        Ok(ReadProjection {
            value: Page {
                records,
                next_cursor,
            },
            metadata: self
                .metadata(&self.control, CATALOG_PLAN_PROJECTION, checkpoint)
                .await?,
        })
    }

    async fn get_subscription(
        &self,
        scope: &AuthorityScope,
        id: SubscriptionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SubscriptionView>>, PortError> {
        let partition = subscription_partition(id);
        let checkpoint = self
            .checkpoint(
                &self.holding,
                SUBSCRIPTION_PROJECTION,
                "commercial.subscription:",
                consistency,
                Some(&partition),
            )
            .await?;
        let authority = Authority::from_scope(scope);
        let row=sqlx::query("SELECT projection.state FROM erpx_commercial_read.subscription_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.subscription_id=$2 AND projection.partition_key=$3 AND ($4 OR ($5::text IS NOT NULL AND projection.scope_key=$5))")
            .bind(SUBSCRIPTION_PROJECTION).bind(id.as_uuid()).bind(&partition).bind(authority.platform).bind(authority.scope_key).fetch_optional(&mut *self.holding.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode).transpose()?;
        if value
            .as_ref()
            .is_some_and(|view: &SubscriptionView| !scope.contains(&view.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self
                .metadata(&self.holding, SUBSCRIPTION_PROJECTION, checkpoint)
                .await?,
        })
    }

    async fn list_subscriptions(
        &self,
        scope: &AuthorityScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<SubscriptionView>>, PortError> {
        let checkpoint = self
            .checkpoint(
                &self.holding,
                SUBSCRIPTION_PROJECTION,
                "commercial.subscription:",
                consistency,
                None,
            )
            .await?;
        let authority = Authority::from_scope(scope);
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows=sqlx::query("SELECT projection.state FROM erpx_commercial_read.subscription_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2 OR ($3::text IS NOT NULL AND projection.scope_key=$3)) AND ($4::timestamptz IS NULL OR (projection.aggregate_updated_at,projection.subscription_id)<($4,$5)) ORDER BY projection.aggregate_updated_at DESC,projection.subscription_id DESC LIMIT $6")
            .bind(SUBSCRIPTION_PROJECTION).bind(authority.platform).bind(authority.scope_key).bind(cursor.map(|v|v.0)).bind(cursor.map(|v|v.1)).bind(i64::from(filter.page_size)+1).fetch_all(&mut *self.holding.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let mut records = rows
            .into_iter()
            .map(decode)
            .collect::<Result<Vec<_>, _>>()?;
        if records
            .iter()
            .any(|view: &SubscriptionView| !scope.contains(&view.scope))
        {
            return Err(PortError::Internal);
        }
        let next_cursor = paginate(
            &mut records,
            filter.page_size,
            |value: &SubscriptionView| (value.updated_at, value.id.as_uuid()),
        );
        Ok(ReadProjection {
            value: Page {
                records,
                next_cursor,
            },
            metadata: self
                .metadata(&self.holding, SUBSCRIPTION_PROJECTION, checkpoint)
                .await?,
        })
    }

    async fn get_entitlement(
        &self,
        scope: &AuthorityScope,
        id: EntitlementId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<EntitlementView>>, PortError> {
        let partition = entitlement_partition(id);
        let checkpoint = self
            .checkpoint(
                &self.holding,
                ENTITLEMENT_PROJECTION,
                "commercial.entitlement:",
                consistency,
                Some(&partition),
            )
            .await?;
        let authority = Authority::from_scope(scope);
        let row=sqlx::query("SELECT projection.state FROM erpx_commercial_read.entitlement_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.entitlement_id=$2 AND projection.partition_key=$3 AND ($4 OR ($5::text IS NOT NULL AND projection.scope_key=$5))")
            .bind(ENTITLEMENT_PROJECTION).bind(id.as_uuid()).bind(&partition).bind(authority.platform).bind(authority.scope_key).fetch_optional(&mut *self.holding.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode).transpose()?;
        if value
            .as_ref()
            .is_some_and(|view: &EntitlementView| !scope.contains(&view.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value,
            metadata: self
                .metadata(&self.holding, ENTITLEMENT_PROJECTION, checkpoint)
                .await?,
        })
    }

    async fn list_entitlements(
        &self,
        scope: &AuthorityScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<EntitlementView>>, PortError> {
        let checkpoint = self
            .checkpoint(
                &self.holding,
                ENTITLEMENT_PROJECTION,
                "commercial.entitlement:",
                consistency,
                None,
            )
            .await?;
        let authority = Authority::from_scope(scope);
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows=sqlx::query("SELECT projection.state FROM erpx_commercial_read.entitlement_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2 OR ($3::text IS NOT NULL AND projection.scope_key=$3)) AND ($4::timestamptz IS NULL OR (projection.aggregate_updated_at,projection.entitlement_id)<($4,$5)) ORDER BY projection.aggregate_updated_at DESC,projection.entitlement_id DESC LIMIT $6")
            .bind(ENTITLEMENT_PROJECTION).bind(authority.platform).bind(authority.scope_key).bind(cursor.map(|v|v.0)).bind(cursor.map(|v|v.1)).bind(i64::from(filter.page_size)+1).fetch_all(&mut *self.holding.pool.begin().await.map_err(|_| PortError::Unavailable)?).await.map_err(|_|PortError::Unavailable)?;
        let mut records = rows
            .into_iter()
            .map(decode)
            .collect::<Result<Vec<_>, _>>()?;
        if records
            .iter()
            .any(|view: &EntitlementView| !scope.contains(&view.scope))
        {
            return Err(PortError::Internal);
        }
        let next_cursor = paginate(&mut records, filter.page_size, |value: &EntitlementView| {
            (value.updated_at, value.id.as_uuid())
        });
        Ok(ReadProjection {
            value: Page {
                records,
                next_cursor,
            },
            metadata: self
                .metadata(&self.holding, ENTITLEMENT_PROJECTION, checkpoint)
                .await?,
        })
    }
}

#[derive(Debug)]
struct Authority {
    platform: bool,
    scope_key: Option<String>,
}
impl Authority {
    fn from_scope(scope: &AuthorityScope) -> Self {
        match scope {
            AuthorityScope::Platform => Self {
                platform: true,
                scope_key: None,
            },
            AuthorityScope::Holding { holding_id } => Self {
                platform: false,
                scope_key: Some(format!("holding:{holding_id}")),
            },
            _ => Self {
                platform: false,
                scope_key: None,
            },
        }
    }
}

fn decode<T: serde::de::DeserializeOwned>(row: sqlx::postgres::PgRow) -> Result<T, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}

fn paginate<T>(
    records: &mut Vec<T>,
    page_size: u16,
    key: impl Fn(&T) -> (DateTime<Utc>, Uuid),
) -> Option<String> {
    if records.len() > usize::from(page_size) {
        records.truncate(usize::from(page_size));
        records.last().map(|value| {
            let (at, id) = key(value);
            encode_cursor(at, id)
        })
    } else {
        None
    }
}

pub fn encode_cursor(updated_at: DateTime<Utc>, id: Uuid) -> String {
    format!(
        "{}{}{}",
        updated_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        CURSOR_SEPARATOR,
        id
    )
}
pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, Uuid), PortError> {
    let (timestamp, id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let updated_at = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id).map_err(|_| PortError::Internal)?;
    if id.is_nil() {
        return Err(PortError::Internal);
    }
    Ok((updated_at, id))
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
    use chrono::TimeZone;

    #[test]
    fn cursor_round_trips_without_precision_loss() {
        let at = Utc
            .with_ymd_and_hms(2026, 8, 5, 14, 30, 0)
            .single()
            .unwrap();
        let id = Uuid::from_u128(7);
        assert_eq!(decode_cursor(&encode_cursor(at, id)), Ok((at, id)));
    }
    #[test]
    fn malformed_and_nil_cursors_fail_closed() {
        assert_eq!(decode_cursor("bad"), Err(PortError::Internal));
        assert_eq!(
            decode_cursor("2026-08-05T14:30:00Z|00000000-0000-0000-0000-000000000000"),
            Err(PortError::Internal)
        );
    }
    #[test]
    fn only_platform_or_exact_holding_can_select_holding_rows() {
        assert!(Authority::from_scope(&AuthorityScope::Platform).platform);
        assert_eq!(
            Authority::from_scope(&AuthorityScope::Holding {
                holding_id: Uuid::from_u128(1)
            })
            .scope_key,
            Some(format!("holding:{}", Uuid::from_u128(1)))
        );
        assert_eq!(
            Authority::from_scope(&AuthorityScope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2)
            })
            .scope_key,
            None
        );
    }

    #[test]
    fn repository_source_keeps_control_and_holding_capabilities_separate() {
        let source = include_str!("read.rs");
        assert!(source.contains("control: ReadPlane"));
        assert!(source.contains("holding: ReadPlane"));
        assert!(source.contains(
            "&mut *self.control.pool.begin().await.map_err(|_| PortError::Unavailable)?"
        ));
        assert!(source.contains(
            "&mut *self.holding.pool.begin().await.map_err(|_| PortError::Unavailable)?"
        ));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }
}
