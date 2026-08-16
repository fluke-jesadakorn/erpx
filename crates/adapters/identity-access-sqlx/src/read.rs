use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_identity_access::{
    PortError, PrincipalId, PrincipalList, PrincipalListFilter, PrincipalView, ReadProjection,
    ReadRepository, RoleId, RoleList, RoleListFilter, RoleView,
};
use erpx_kernel::TenantScope;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    PRINCIPAL_PROJECTION_NAME, ROLE_PROJECTION_NAME, principal_partition, projection::status_name,
    role_partition,
};

const CURSOR_SEPARATOR: char = '|';
const MAX_SCAN_ROWS: i64 = 1_001;

#[derive(Debug, Clone)]
pub struct IdentityAccessReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl IdentityAccessReadRepository {
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
        projection: &'static str,
        consistency: &QueryConsistency,
        exact_partition: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact_partition {
                Some(partition) => self
                    .read_side
                    .checkpoint(projection, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid_partition = match projection {
                    PRINCIPAL_PROJECTION_NAME => requirement
                        .partition
                        .strip_prefix("identity_access.principal:")
                        .is_some_and(valid_partition_uuid),
                    ROLE_PROJECTION_NAME => requirement
                        .partition
                        .strip_prefix("identity_access.role:")
                        .is_some_and(valid_partition_uuid),
                    _ => false,
                };
                if requirement.projection != projection
                    || !valid_partition
                    || exact_partition.is_some_and(|value| value != requirement.partition)
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
        projection: &'static str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status IN ('active', 'building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(projection)
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
impl ReadRepository for IdentityAccessReadRepository {
    async fn get_principal(
        &self,
        authority_scope: &TenantScope,
        principal_id: PrincipalId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PrincipalView>>, PortError> {
        validate_authority(authority_scope)?;
        let partition = principal_partition(principal_id);
        let checkpoint = self
            .consistency_checkpoint(PRINCIPAL_PROJECTION_NAME, consistency, Some(&partition))
            .await?;
        let row = sqlx::query(
            "SELECT projection.state
             FROM erpx_identity_access_read.principal_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND projection.principal_id = $2 AND projection.partition_key = $3",
        )
        .bind(PRINCIPAL_PROJECTION_NAME)
        .bind(principal_id.as_uuid())
        .bind(&partition)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_principal).transpose()?;
        if value
            .as_ref()
            .is_some_and(|view| !authority_scope.contains(&view.scope))
        {
            return Ok(ReadProjection {
                value: None,
                metadata: self.metadata(PRINCIPAL_PROJECTION_NAME, checkpoint).await?,
            });
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(PRINCIPAL_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn list_principals(
        &self,
        authority_scope: &TenantScope,
        filter: &PrincipalListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PrincipalList>, PortError> {
        validate_authority(authority_scope)?;
        let checkpoint = self
            .consistency_checkpoint(PRINCIPAL_PROJECTION_NAME, consistency, None)
            .await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows = sqlx::query(
            "SELECT projection.state
             FROM erpx_identity_access_read.principal_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND ($2::text IS NULL OR projection.principal_status = $2)
               AND ($3::timestamptz IS NULL OR
                    (projection.aggregate_updated_at, projection.principal_id) < ($3, $4))
             ORDER BY projection.aggregate_updated_at DESC, projection.principal_id DESC
             LIMIT $5",
        )
        .bind(PRINCIPAL_PROJECTION_NAME)
        .bind(filter.status.map(status_name))
        .bind(cursor.map(|value| value.0))
        .bind(cursor.map(|value| value.1))
        .bind(MAX_SCAN_ROWS)
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut principals = rows
            .into_iter()
            .map(decode_principal)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|view| authority_scope.contains(&view.scope))
            .take(usize::from(filter.page_size) + 1)
            .collect::<Vec<_>>();
        let next_cursor = truncate_with_cursor(&mut principals, filter.page_size, |view| {
            (view.updated_at, view.principal_id.as_uuid())
        });
        Ok(ReadProjection {
            value: PrincipalList {
                principals,
                next_cursor,
            },
            metadata: self.metadata(PRINCIPAL_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn get_role(
        &self,
        authority_scope: &TenantScope,
        role_id: RoleId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RoleView>>, PortError> {
        validate_authority(authority_scope)?;
        let partition = role_partition(role_id);
        let checkpoint = self
            .consistency_checkpoint(ROLE_PROJECTION_NAME, consistency, Some(&partition))
            .await?;
        let row = sqlx::query(
            "SELECT projection.state
             FROM erpx_identity_access_read.role_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND projection.role_id = $2 AND projection.partition_key = $3",
        )
        .bind(ROLE_PROJECTION_NAME)
        .bind(role_id.as_uuid())
        .bind(&partition)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_role).transpose()?;
        let value = value.filter(|view| authority_scope.contains(&view.scope));
        Ok(ReadProjection {
            value,
            metadata: self.metadata(ROLE_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn list_roles(
        &self,
        authority_scope: &TenantScope,
        filter: &RoleListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<RoleList>, PortError> {
        validate_authority(authority_scope)?;
        let checkpoint = self
            .consistency_checkpoint(ROLE_PROJECTION_NAME, consistency, None)
            .await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let rows = sqlx::query(
            "SELECT projection.state
             FROM erpx_identity_access_read.role_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND ($2::text IS NULL OR projection.risk_tier = $2)
               AND ($3::timestamptz IS NULL OR
                    (projection.aggregate_updated_at, projection.role_id) < ($3, $4))
             ORDER BY projection.aggregate_updated_at DESC, projection.role_id DESC
             LIMIT $5",
        )
        .bind(ROLE_PROJECTION_NAME)
        .bind(filter.risk_tier.map(crate::command::risk_name))
        .bind(cursor.map(|value| value.0))
        .bind(cursor.map(|value| value.1))
        .bind(MAX_SCAN_ROWS)
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut roles = rows
            .into_iter()
            .map(decode_role)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|view| authority_scope.contains(&view.scope))
            .take(usize::from(filter.page_size) + 1)
            .collect::<Vec<_>>();
        let next_cursor = truncate_with_cursor(&mut roles, filter.page_size, |view| {
            (view.updated_at, view.role_id.as_uuid())
        });
        Ok(ReadProjection {
            value: RoleList { roles, next_cursor },
            metadata: self.metadata(ROLE_PROJECTION_NAME, checkpoint).await?,
        })
    }
}

fn validate_authority(scope: &TenantScope) -> Result<(), PortError> {
    scope.validate().map_err(|_| PortError::Denied)
}

fn valid_partition_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|value| !value.is_nil())
}

fn decode_principal(row: sqlx::postgres::PgRow) -> Result<PrincipalView, PortError> {
    serde_json::from_value(row.try_get("state").map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)
}

fn decode_role(row: sqlx::postgres::PgRow) -> Result<RoleView, PortError> {
    serde_json::from_value(row.try_get("state").map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)
}

pub fn encode_cursor(updated_at: DateTime<Utc>, aggregate_id: Uuid) -> String {
    format!(
        "{}{}{}",
        updated_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        CURSOR_SEPARATOR,
        aggregate_id
    )
}

pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, Uuid), PortError> {
    let (timestamp, aggregate_id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let updated_at = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let aggregate_id = Uuid::parse_str(aggregate_id).map_err(|_| PortError::Internal)?;
    if aggregate_id.is_nil() {
        return Err(PortError::Internal);
    }
    Ok((updated_at, aggregate_id))
}

fn truncate_with_cursor<T>(
    values: &mut Vec<T>,
    page_size: u16,
    position: impl Fn(&T) -> (DateTime<Utc>, Uuid),
) -> Option<String> {
    if values.len() <= usize::from(page_size) {
        return None;
    }
    values.truncate(usize::from(page_size));
    values.last().map(|value| {
        let (at, id) = position(value);
        encode_cursor(at, id)
    })
}

fn map_read_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

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
        assert_eq!(decode_cursor("invalid"), Err(PortError::Internal));
        assert_eq!(
            decode_cursor("2026-08-05T14:30:00Z|00000000-0000-0000-0000-000000000000"),
            Err(PortError::Internal)
        );
    }

    #[test]
    fn read_your_writes_partition_names_are_context_specific() {
        assert!(valid_partition_uuid(
            &Uuid::from_u128(1).simple().to_string()
        ));
        assert!(!valid_partition_uuid("not-an-id"));
    }
}
