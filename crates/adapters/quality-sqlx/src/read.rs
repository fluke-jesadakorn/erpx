use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::AuthorityScope;
use erpx_quality::{
    application::InspectionView,
    domain::InspectionId,
    ports::{InspectionList, InspectionListFilter, PortError, ReadProjection, ReadRepository},
};
use sqlx::Row;
use uuid::Uuid;

use crate::{PROJECTION_NAME, inspection_partition, projection::status_name};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct QualityReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl QualityReadRepository {
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
                    .strip_prefix("quality.inspection:")
                    .is_some_and(|id| Uuid::parse_str(id).is_ok());
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
impl ReadRepository for QualityReadRepository {
    async fn get(
        &self,
        authority_scope: &AuthorityScope,
        inspection_id: InspectionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<InspectionView>>, PortError> {
        let partition = inspection_partition(inspection_id);
        let checkpoint = self
            .consistency_checkpoint(consistency, Some(&partition))
            .await?;
        let scope = ScopeFilter::from_scope(authority_scope)?;
        let row = sqlx::query(
            "SELECT projection.state
             FROM erpx_quality_read.inspection_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND projection.inspection_id = $2
               AND projection.partition_key = $3
               AND (
                    $4 = 'platform'
                    OR ($4 = 'holding' AND projection.holding_id = $5)
                    OR ($4 = 'company' AND projection.holding_id = $5
                        AND projection.company_id = $6)
                    OR ($4 = 'branch' AND projection.holding_id = $5
                        AND projection.company_id = $6 AND projection.branch_id = $7)
                    OR ($4 = 'subtree' AND projection.holding_id = $5 AND (
                        (projection.branch_id IS NULL AND cardinality($9::uuid[]) = 0
                         AND projection.company_id = ANY($8::uuid[]))
                        OR (projection.branch_id IS NOT NULL
                            AND projection.company_id = ANY($8::uuid[])
                            AND projection.branch_id = ANY($9::uuid[]))
                    ))
               )",
        )
        .bind(PROJECTION_NAME)
        .bind(inspection_id.as_uuid())
        .bind(&partition)
        .bind(scope.kind)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(&scope.company_ids)
        .bind(&scope.branch_ids)
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
        if view
            .as_ref()
            .is_some_and(|view| !authority_scope.contains(&view.scope))
        {
            return Err(PortError::Internal);
        }
        Ok(ReadProjection {
            value: view,
            metadata: self.metadata(checkpoint).await?,
        })
    }

    async fn list(
        &self,
        authority_scope: &AuthorityScope,
        filter: &InspectionListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<InspectionList>, PortError> {
        let checkpoint = self.consistency_checkpoint(consistency, None).await?;
        let scope = ScopeFilter::from_scope(authority_scope)?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let cursor_at = cursor.as_ref().map(|(updated_at, _)| *updated_at);
        let cursor_id = cursor
            .as_ref()
            .map(|(_, inspection_id)| inspection_id.as_uuid());
        let status = filter.status.map(status_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query(
            "SELECT projection.state
             FROM erpx_quality_read.inspection_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = projection.projection_name
              AND generation.generation = projection.generation
              AND generation.status = 'active'
             WHERE projection.projection_name = $1
               AND (
                    $2 = 'platform'
                    OR ($2 = 'holding' AND projection.holding_id = $3)
                    OR ($2 = 'company' AND projection.holding_id = $3
                        AND projection.company_id = $4)
                    OR ($2 = 'branch' AND projection.holding_id = $3
                        AND projection.company_id = $4 AND projection.branch_id = $5)
                    OR ($2 = 'subtree' AND projection.holding_id = $3 AND (
                        (projection.branch_id IS NULL AND cardinality($7::uuid[]) = 0
                         AND projection.company_id = ANY($6::uuid[]))
                        OR (projection.branch_id IS NOT NULL
                            AND projection.company_id = ANY($6::uuid[])
                            AND projection.branch_id = ANY($7::uuid[]))
                    ))
               )
               AND ($8::text IS NULL OR projection.inspection_status = $8)
               AND ($9::uuid IS NULL OR projection.product_id = $9)
               AND ($10::timestamptz IS NULL OR
                    (projection.aggregate_updated_at, projection.inspection_id) < ($10, $11))
             ORDER BY projection.aggregate_updated_at DESC, projection.inspection_id DESC
             LIMIT $12",
        )
        .bind(PROJECTION_NAME)
        .bind(scope.kind)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(&scope.company_ids)
        .bind(&scope.branch_ids)
        .bind(status)
        .bind(filter.product_id)
        .bind(cursor_at)
        .bind(cursor_id)
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
        let mut inspections = rows
            .into_iter()
            .map(decode_view)
            .collect::<Result<Vec<_>, _>>()?;
        if inspections
            .iter()
            .any(|view| !authority_scope.contains(&view.scope))
        {
            return Err(PortError::Internal);
        }
        let next_cursor = if inspections.len() > usize::from(filter.page_size) {
            inspections.truncate(usize::from(filter.page_size));
            inspections
                .last()
                .map(|view| encode_cursor(view.updated_at, view.inspection_id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: InspectionList {
                inspections,
                next_cursor,
            },
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

#[derive(Debug)]
struct ScopeFilter {
    kind: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl ScopeFilter {
    fn from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                kind: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Holding { holding_id } => Self {
                kind: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                kind: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                kind: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                kind: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![*organization_node_id],
                branch_ids: Vec::new(),
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cursor {
    updated_at: DateTime<Utc>,
    inspection_id: Uuid,
}

pub fn encode_cursor(updated_at: DateTime<Utc>, inspection_id: InspectionId) -> String {
    format!(
        "{}{}{}",
        updated_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        CURSOR_SEPARATOR,
        inspection_id.as_uuid()
    )
}

pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, InspectionId), PortError> {
    let cursor = parse_cursor(value)?;
    let inspection_id =
        InspectionId::try_from_uuid(cursor.inspection_id).map_err(|_| PortError::Internal)?;
    Ok((cursor.updated_at, inspection_id))
}

fn parse_cursor(value: &str) -> Result<Cursor, PortError> {
    let (timestamp, inspection_id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let updated_at = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let inspection_id = Uuid::parse_str(inspection_id).map_err(|_| PortError::Internal)?;
    if inspection_id.is_nil() {
        return Err(PortError::Internal);
    }
    Ok(Cursor {
        updated_at,
        inspection_id,
    })
}

fn decode_view(row: sqlx::postgres::PgRow) -> Result<InspectionView, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
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
    use erpx_quality::domain::InspectionStatus;

    use super::*;

    #[test]
    fn cursor_round_trips_without_precision_loss() {
        let at = Utc
            .with_ymd_and_hms(2026, 8, 5, 14, 30, 0)
            .single()
            .unwrap();
        let id = InspectionId::try_from_uuid(Uuid::from_u128(7)).unwrap();
        assert_eq!(decode_cursor(&encode_cursor(at, id)), Ok((at, id)));
    }

    #[test]
    fn malformed_or_nil_cursors_fail_closed() {
        assert_eq!(decode_cursor("not-a-cursor"), Err(PortError::Internal));
        assert_eq!(
            decode_cursor("2026-08-05T14:30:00Z|00000000-0000-0000-0000-000000000000"),
            Err(PortError::Internal)
        );
    }

    #[test]
    fn subtree_filter_preserves_explicit_company_and_branch_sets() {
        let scope = AuthorityScope::Subtree {
            holding_id: Uuid::from_u128(1),
            organization_node_id: Uuid::from_u128(2),
        };
        let filter = ScopeFilter::from_scope(&scope).unwrap();
        assert_eq!(filter.kind, "subtree");
        assert_eq!(filter.company_ids, vec![Uuid::from_u128(2)]);
        assert!(filter.branch_ids.is_empty());
    }

    #[test]
    fn status_names_match_the_read_contract() {
        assert_eq!(status_name(InspectionStatus::InProgress), "in_progress");
    }
}
