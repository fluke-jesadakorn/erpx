use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::Utc;
use erpx_brand::{
    BrandAssetView, BrandCategory, BrandHistory, BrandHistoryEntry, BrandLifecycle, BrandProfileId,
    BrandProfileView, BrandPublicationView, BrandScope, DerivedRecordReadRepository, PortError,
    ReadProjection, ReadRepository, ResolvedBrandView,
};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{CqrsSqlxError, ReadPool, SqlxProjectionWaiter, WaitError, WaiterConfig};
use erpx_kernel::{RecordId, TenantScope};
use sqlx::Row;
use uuid::Uuid;

use crate::{HISTORY_PROJECTION, PROFILE_PROJECTION, RESOLVED_PROJECTION};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct BrandReadRepository {
    pool: ReadPool,
    waiter: SqlxProjectionWaiter,
}

impl BrandReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }

    pub fn with_waiter_config(
        pool: ReadPool,
        waiter_config: WaiterConfig,
    ) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            waiter: SqlxProjectionWaiter::new(pool.clone(), waiter_config)?,
            pool,
        })
    }

    async fn checkpoint(
        &self,
        projection: &str,
        consistency: &QueryConsistency,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => Ok(None),
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid_partition = requirement
                    .partition
                    .strip_prefix("brand.profile:")
                    .is_some_and(|id| Uuid::parse_str(id).is_ok());
                if requirement.projection != projection || !valid_partition {
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
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name=$1 AND status IN ('active','building')
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
impl ReadRepository for BrandReadRepository {
    async fn get_draft(
        &self,
        authority_scope: &TenantScope,
        scope: &BrandScope,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<BrandProfileView>>, PortError> {
        ensure_scope(authority_scope, scope)?;
        let checkpoint = self.checkpoint(PROFILE_PROJECTION, consistency).await?;
        let row = sqlx::query(
            "SELECT projection.partition_key,projection.state
             FROM erpx_brand_read.profile_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation
              AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.scope_key=$2",
        )
        .bind(PROFILE_PROJECTION)
        .bind(scope.canonical_key())
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_profile).transpose()?.and_then(|view| {
            if view.draft.is_some() {
                Some(view)
            } else {
                None
            }
        });
        if value.as_ref().is_some_and(|view| &view.scope != scope) {
            return Err(PortError::Internal);
        }
        if let (QueryConsistency::ReadYourWrites(requirement), Some(view)) = (consistency, &value)
            && requirement.partition != crate::profile_partition(view.profile_id)
        {
            return Err(PortError::NotReady(ProjectionNotReady {
                projection: requirement.projection.clone(),
                partition: requirement.partition.clone(),
                target: requirement.target,
                observed: checkpoint.map(Box::new),
            }));
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(PROFILE_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_resolved(
        &self,
        authority_scope: &TenantScope,
        scope: &BrandScope,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ResolvedBrandView>>, PortError> {
        ensure_scope(authority_scope, scope)?;
        let checkpoint = self.checkpoint(RESOLVED_PROJECTION, consistency).await?;
        let row = sqlx::query(
            "SELECT published_revision,definition,inherited_categories,
                    effective_locked_categories,digest
             FROM erpx_brand_read.resolved_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation
              AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.scope_key=$2",
        )
        .bind(RESOLVED_PROJECTION)
        .bind(scope.canonical_key())
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row
            .map(|row| decode_resolved(row, scope.clone()))
            .transpose()?;
        Ok(ReadProjection {
            value,
            metadata: self.metadata(RESOLVED_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_history(
        &self,
        authority_scope: &TenantScope,
        scope: &BrandScope,
        page_size: u16,
        cursor: Option<&str>,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<BrandHistory>, PortError> {
        ensure_scope(authority_scope, scope)?;
        let checkpoint = self.checkpoint(HISTORY_PROJECTION, consistency).await?;
        let cursor = cursor.map(decode_history_cursor).transpose()?;
        let rows = sqlx::query(
            "SELECT profile_id,revision,lifecycle,definition,parent_published_revision,
                    updated_by,reason,created_at,published_at,archived_at
             FROM erpx_brand_read.history_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation
              AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.scope_key=$2
               AND ($3::bigint IS NULL OR (projection.revision,projection.profile_id)<($3,$4))
             ORDER BY projection.revision DESC,projection.profile_id DESC LIMIT $5",
        )
        .bind(HISTORY_PROJECTION)
        .bind(scope.canonical_key())
        .bind(cursor.map(|value| to_i64_port(value.0)).transpose()?)
        .bind(cursor.map(|value| value.1.as_uuid()))
        .bind(i64::from(page_size) + 1)
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut entries = rows
            .into_iter()
            .map(decode_history)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if entries.len() > usize::from(page_size) {
            entries.truncate(usize::from(page_size));
            entries
                .last()
                .map(|entry| encode_history_cursor(entry.revision, entry.profile_id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: BrandHistory {
                entries,
                next_cursor,
            },
            metadata: self.metadata(HISTORY_PROJECTION, checkpoint).await?,
        })
    }
}

#[async_trait]
impl DerivedRecordReadRepository for BrandReadRepository {
    async fn get_asset(
        &self,
        authority_scope: &TenantScope,
        target_scope: &BrandScope,
        record_id: RecordId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<BrandAssetView>>, PortError> {
        ensure_scope(authority_scope, target_scope)?;
        let checkpoint = self.checkpoint(HISTORY_PROJECTION, consistency).await?;
        let history = self.history_for_scope(target_scope).await?;
        let mut value = None;
        'revisions: for entry in history {
            for asset in entry.definition.assets.references() {
                let record = erpx_brand::integration::asset_record_ref(entry.profile_id, asset)
                    .map_err(|_| PortError::Internal)?;
                if record.id() == record_id {
                    ensure_partition(consistency, entry.profile_id, checkpoint.as_ref())?;
                    value = Some(BrandAssetView {
                        record,
                        profile_id: entry.profile_id,
                        source_revision: entry.revision,
                        scope: target_scope.clone(),
                        lifecycle: entry.lifecycle,
                        asset: asset.clone(),
                        revision_updated_at: entry.created_at,
                        archived_at: entry.archived_at,
                    });
                    break 'revisions;
                }
            }
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(HISTORY_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_publication(
        &self,
        authority_scope: &TenantScope,
        target_scope: &BrandScope,
        record_id: RecordId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<BrandPublicationView>>, PortError> {
        ensure_scope(authority_scope, target_scope)?;
        let checkpoint = self.checkpoint(HISTORY_PROJECTION, consistency).await?;
        let mut value = None;
        for entry in self.history_for_scope(target_scope).await? {
            if entry.lifecycle != BrandLifecycle::Published {
                continue;
            }
            let record =
                erpx_brand::integration::publication_record_ref(entry.profile_id, entry.revision)
                    .map_err(|_| PortError::Internal)?;
            if record.id() == record_id {
                ensure_partition(consistency, entry.profile_id, checkpoint.as_ref())?;
                let published_at = entry.published_at.ok_or(PortError::Internal)?;
                value = Some(BrandPublicationView {
                    record,
                    profile_id: entry.profile_id,
                    revision: entry.revision,
                    scope: target_scope.clone(),
                    definition: entry.definition,
                    updated_by: entry.updated_by,
                    reason: entry.reason,
                    published_at,
                    archived_at: entry.archived_at,
                });
                break;
            }
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(HISTORY_PROJECTION, checkpoint).await?,
        })
    }
}

impl BrandReadRepository {
    async fn history_for_scope(
        &self,
        scope: &BrandScope,
    ) -> Result<Vec<BrandHistoryEntry>, PortError> {
        let rows = sqlx::query(
            "SELECT profile_id,revision,lifecycle,definition,parent_published_revision,
                    updated_by,reason,created_at,published_at,archived_at
             FROM erpx_brand_read.history_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation
              AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.scope_key=$2
             ORDER BY projection.revision DESC,projection.profile_id DESC",
        )
        .bind(HISTORY_PROJECTION)
        .bind(scope.canonical_key())
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        rows.into_iter().map(decode_history).collect()
    }
}

fn ensure_partition(
    consistency: &QueryConsistency,
    profile_id: BrandProfileId,
    observed: Option<&ProjectionCheckpoint>,
) -> Result<(), PortError> {
    if let QueryConsistency::ReadYourWrites(requirement) = consistency
        && requirement.partition != crate::profile_partition(profile_id)
    {
        return Err(PortError::NotReady(ProjectionNotReady {
            projection: requirement.projection.clone(),
            partition: requirement.partition.clone(),
            target: requirement.target,
            observed: observed.cloned().map(Box::new),
        }));
    }
    Ok(())
}

fn ensure_scope(authority: &TenantScope, target: &BrandScope) -> Result<(), PortError> {
    let target = target.authority_scope().map_err(|_| PortError::Denied)?;
    if authority.contains(&target) {
        Ok(())
    } else {
        Err(PortError::Denied)
    }
}

fn decode_profile(row: sqlx::postgres::PgRow) -> Result<BrandProfileView, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}

fn decode_resolved(
    row: sqlx::postgres::PgRow,
    scope: BrandScope,
) -> Result<ResolvedBrandView, PortError> {
    Ok(ResolvedBrandView {
        scope,
        published_revision: from_i64(
            row.try_get("published_revision")
                .map_err(|_| PortError::Internal)?,
        )?,
        definition: decode_json(&row, "definition")?,
        inherited_categories: decode_json::<BTreeSet<BrandCategory>>(&row, "inherited_categories")?,
        effective_locked_categories: decode_json::<BTreeSet<BrandCategory>>(
            &row,
            "effective_locked_categories",
        )?,
        digest: row.try_get("digest").map_err(|_| PortError::Internal)?,
    })
}

fn decode_history(row: sqlx::postgres::PgRow) -> Result<BrandHistoryEntry, PortError> {
    let lifecycle = match row
        .try_get::<String, _>("lifecycle")
        .map_err(|_| PortError::Internal)?
        .as_str()
    {
        "draft" => BrandLifecycle::Draft,
        "published" => BrandLifecycle::Published,
        _ => return Err(PortError::Internal),
    };
    Ok(BrandHistoryEntry {
        profile_id: BrandProfileId::try_from_uuid(
            row.try_get("profile_id").map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        revision: from_i64(row.try_get("revision").map_err(|_| PortError::Internal)?)?,
        lifecycle,
        definition: decode_json(&row, "definition")?,
        parent_published_revision: row
            .try_get::<Option<i64>, _>("parent_published_revision")
            .map_err(|_| PortError::Internal)?
            .map(from_i64)
            .transpose()?,
        updated_by: row.try_get("updated_by").map_err(|_| PortError::Internal)?,
        reason: row.try_get("reason").map_err(|_| PortError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
        published_at: row
            .try_get("published_at")
            .map_err(|_| PortError::Internal)?,
        archived_at: row
            .try_get("archived_at")
            .map_err(|_| PortError::Internal)?,
    })
}

fn decode_json<T: serde::de::DeserializeOwned>(
    row: &sqlx::postgres::PgRow,
    field: &str,
) -> Result<T, PortError> {
    let value: serde_json::Value = row.try_get(field).map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}

pub fn encode_history_cursor(revision: u64, profile_id: BrandProfileId) -> String {
    format!("{revision}{CURSOR_SEPARATOR}{}", profile_id.as_uuid())
}

pub fn decode_history_cursor(value: &str) -> Result<(u64, BrandProfileId), PortError> {
    let (revision, profile_id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let revision = revision.parse().map_err(|_| PortError::Internal)?;
    if revision == 0 {
        return Err(PortError::Internal);
    }
    let profile_id = Uuid::parse_str(profile_id).map_err(|_| PortError::Internal)?;
    Ok((
        revision,
        BrandProfileId::try_from_uuid(profile_id).map_err(|_| PortError::Internal)?,
    ))
}

fn from_i64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn to_i64_port(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_cursor_round_trips_and_rejects_nil() {
        let id = BrandProfileId::try_from_uuid(Uuid::from_u128(7)).unwrap();
        assert_eq!(
            decode_history_cursor(&encode_history_cursor(42, id)),
            Ok((42, id))
        );
        assert_eq!(decode_history_cursor("0|bad"), Err(PortError::Internal));
    }

    #[test]
    fn read_source_cannot_fall_back_to_write_state() {
        let production = include_str!("read.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("read adapter has production source");
        for forbidden in [
            concat!("Write", "Pool"),
            concat!("erpx_brand_", "write"),
            concat!("load_", "snapshot"),
        ] {
            assert!(!production.contains(forbidden));
        }
    }

    #[test]
    fn derived_records_use_only_the_active_history_generation() {
        let production = include_str!("read.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("read adapter has production source");
        assert!(production.contains("generation.status='active'"));
        assert!(production.contains("history_for_scope"));
        assert!(!production.contains(concat!("ORDER BY projected_at ", "DESC")));
    }
}
