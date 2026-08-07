use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_platform_administration::{
    AdminContextId, AdminContextStatus, AdminContextView, ConnectorBindingId, ConnectorView, Page,
    PageFilter, PlatformTenantId, PortError, ReadProjection, ReadRepository, TenantView,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    ADMIN_CONTEXT_PROJECTION, CONNECTOR_PROJECTION, TENANT_PROJECTION, admin_context_partition,
    connector_partition, tenant_partition,
};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct PlatformAdministrationReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl PlatformAdministrationReadRepository {
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
        prefix: &str,
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
                let valid = requirement
                    .partition
                    .strip_prefix(prefix)
                    .is_some_and(|id| Uuid::parse_str(id).is_ok());
                if requirement.projection != projection
                    || !valid
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
        let status:Option<String>=sqlx::query_scalar("SELECT status FROM erpx_cqrs_read.projection_generations WHERE projection_name=$1 AND status IN ('active','building') ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1").bind(projection).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
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
impl ReadRepository for PlatformAdministrationReadRepository {
    async fn get_tenant(
        &self,
        scope: &TenantScope,
        id: PlatformTenantId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<TenantView>>, PortError> {
        let partition = tenant_partition(id);
        let checkpoint = self
            .checkpoint(
                TENANT_PROJECTION,
                "platform_administration.tenant:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row=sqlx::query("SELECT projection.state FROM erpx_platform_administration_read.tenant_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.tenant_id=$2 AND projection.partition_key=$3")
            .bind(TENANT_PROJECTION).bind(id.as_uuid()).bind(&partition).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode).transpose()?;
        if value.as_ref().is_some_and(|view: &TenantView| {
            !scope.contains(&TenantScope::Holding {
                holding_id: view.root_holding_id,
            })
        }) {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(TENANT_PROJECTION, checkpoint).await?,
        })
    }
    async fn list_tenants(
        &self,
        scope: &TenantScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<TenantView>>, PortError> {
        let checkpoint = self
            .checkpoint(
                TENANT_PROJECTION,
                "platform_administration.tenant:",
                consistency,
                None,
            )
            .await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let holding = holding_filter(scope)?;
        let rows=sqlx::query("SELECT projection.state FROM erpx_platform_administration_read.tenant_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2 OR projection.root_holding_id=$3) AND ($4::timestamptz IS NULL OR (projection.aggregate_updated_at,projection.tenant_id)<($4,$5)) ORDER BY projection.aggregate_updated_at DESC,projection.tenant_id DESC LIMIT $6")
            .bind(TENANT_PROJECTION).bind(matches!(scope,TenantScope::Platform)).bind(holding).bind(cursor.map(|value|value.0)).bind(cursor.map(|value|value.1)).bind(i64::from(filter.page_size)+1).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut records = rows
            .into_iter()
            .map(decode)
            .collect::<Result<Vec<TenantView>, _>>()?;
        if records.iter().any(|view| {
            !scope.contains(&TenantScope::Holding {
                holding_id: view.root_holding_id,
            })
        }) {
            return Err(PortError::Internal);
        }
        let next_cursor = paginate(&mut records, filter.page_size, |view| {
            (view.updated_at, view.tenant_id.as_uuid())
        });
        Ok(ReadProjection {
            value: Page {
                records,
                next_cursor,
            },
            metadata: self.metadata(TENANT_PROJECTION, checkpoint).await?,
        })
    }
    async fn get_connector(
        &self,
        scope: &TenantScope,
        id: ConnectorBindingId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ConnectorView>>, PortError> {
        let partition = connector_partition(id);
        let checkpoint = self
            .checkpoint(
                CONNECTOR_PROJECTION,
                "platform_administration.connector:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row=sqlx::query("SELECT projection.state FROM erpx_platform_administration_read.connector_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.connector_id=$2 AND projection.partition_key=$3")
            .bind(CONNECTOR_PROJECTION).bind(id.as_uuid()).bind(&partition).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let value = row.map(decode).transpose()?;
        if value
            .as_ref()
            .is_some_and(|view: &ConnectorView| !scope.contains(&view.scope.tenant_scope()))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(CONNECTOR_PROJECTION, checkpoint).await?,
        })
    }
    async fn list_connectors(
        &self,
        scope: &TenantScope,
        filter: &PageFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<ConnectorView>>, PortError> {
        let checkpoint = self
            .checkpoint(
                CONNECTOR_PROJECTION,
                "platform_administration.connector:",
                consistency,
                None,
            )
            .await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let prefix = scope_prefix(scope)?;
        let rows=sqlx::query("SELECT projection.state FROM erpx_platform_administration_read.connector_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND ($2 OR projection.scope_key=$3 OR projection.scope_key LIKE $4) AND ($5::timestamptz IS NULL OR (projection.aggregate_updated_at,projection.connector_id)<($5,$6)) ORDER BY projection.aggregate_updated_at DESC,projection.connector_id DESC LIMIT $7")
            .bind(CONNECTOR_PROJECTION).bind(matches!(scope,TenantScope::Platform)).bind(&prefix).bind(prefix.as_ref().map(|value|format!("{value}/%"))).bind(cursor.map(|value|value.0)).bind(cursor.map(|value|value.1)).bind(i64::from(filter.page_size)+1).fetch_all(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut records = rows
            .into_iter()
            .map(decode)
            .collect::<Result<Vec<ConnectorView>, _>>()?;
        if records
            .iter()
            .any(|view| !scope.contains(&view.scope.tenant_scope()))
        {
            return Err(PortError::Internal);
        }
        let next_cursor = paginate(&mut records, filter.page_size, |view| {
            (view.updated_at, view.connector_id.as_uuid())
        });
        Ok(ReadProjection {
            value: Page {
                records,
                next_cursor,
            },
            metadata: self.metadata(CONNECTOR_PROJECTION, checkpoint).await?,
        })
    }
    async fn get_admin_context(
        &self,
        scope: &TenantScope,
        id: AdminContextId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<AdminContextView>>, PortError> {
        let partition = admin_context_partition(id);
        let checkpoint = self
            .checkpoint(
                ADMIN_CONTEXT_PROJECTION,
                "platform_administration.admin_context:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row=sqlx::query("SELECT projection.state FROM erpx_platform_administration_read.admin_context_projection projection JOIN erpx_cqrs_read.projection_generations generation ON generation.projection_name=projection.projection_name AND generation.generation=projection.generation AND generation.status='active' WHERE projection.projection_name=$1 AND projection.context_id=$2 AND projection.partition_key=$3")
            .bind(ADMIN_CONTEXT_PROJECTION).bind(id.as_uuid()).bind(&partition).fetch_optional(self.pool.as_pg_pool()).await.map_err(|_|PortError::Unavailable)?;
        let mut value: Option<AdminContextView> = row.map(decode).transpose()?;
        if let Some(view) = &mut value {
            if !scope.contains(&view.scope) {
                return Err(PortError::Denied);
            }
            if view.status == AdminContextStatus::Active && Utc::now() >= view.expires_at {
                view.status = AdminContextStatus::Expired;
            }
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(ADMIN_CONTEXT_PROJECTION, checkpoint).await?,
        })
    }
}

fn holding_filter(scope: &TenantScope) -> Result<Option<Uuid>, PortError> {
    match scope {
        TenantScope::Platform => Ok(None),
        TenantScope::Holding { holding_id } => Ok(Some(*holding_id)),
        _ => Err(PortError::Denied),
    }
}
fn scope_prefix(scope: &TenantScope) -> Result<Option<String>, PortError> {
    scope.validate().map_err(|_| PortError::Denied)?;
    Ok(match scope {
        TenantScope::Platform => None,
        TenantScope::Holding { holding_id } => Some(format!("h:{holding_id}")),
        TenantScope::Company {
            holding_id,
            company_id,
        } => Some(format!("h:{holding_id}/c:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Some(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        TenantScope::Subtree { .. } => return Err(PortError::Denied),
    })
}
fn decode<T: serde::de::DeserializeOwned>(row: sqlx::postgres::PgRow) -> Result<T, PortError> {
    serde_json::from_value(row.try_get("state").map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)
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
pub fn encode_cursor(at: DateTime<Utc>, id: Uuid) -> String {
    format!(
        "{}{}{}",
        at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        CURSOR_SEPARATOR,
        id
    )
}
pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, Uuid), PortError> {
    let (timestamp, id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let at = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id).map_err(|_| PortError::Internal)?;
    if id.is_nil() {
        return Err(PortError::Internal);
    }
    Ok((at, id))
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
    fn cursor_round_trips() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let id = Uuid::from_u128(7);
        assert_eq!(decode_cursor(&encode_cursor(at, id)), Ok((at, id)));
    }
    #[test]
    fn malformed_cursor_fails_closed() {
        assert_eq!(decode_cursor("bad"), Err(PortError::Internal));
    }
}
