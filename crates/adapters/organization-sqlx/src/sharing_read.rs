use async_trait::async_trait;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionEvent, ProjectionFailure, Projector, ProjectorContext, ReadPool,
    ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::AuthorityScope;
use erpx_organization::{
    OrganizationSharingAgreement, PortError, ReadProjection, SharingAgreementId,
    SharingAgreementList, SharingListFilter, SharingReadRepository, SharingStatus,
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, SHARING_PROJECTION_NAME, SHARING_SNAPSHOT_EVENT_TYPE,
    SHARING_SNAPSHOT_EVENT_VERSION, sharing_partition,
};

const LIST_SCAN_BATCH: i64 = 128;
const MAX_LIST_SCAN_BATCHES: usize = 16;

#[derive(Debug, Clone)]
pub struct OrganizationSharingReadAdapter {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl OrganizationSharingReadAdapter {
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
                    .checkpoint(SHARING_PROJECTION_NAME, partition)
                    .await
                    .map_err(map_read_store),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid_partition = requirement
                    .partition
                    .strip_prefix("organization.sharing:")
                    .is_some_and(valid_compact_uuid);
                if requirement.projection != SHARING_PROJECTION_NAME
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
                        WaitError::NotReady(value) => PortError::NotReady(value),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }

    async fn metadata(
        &self,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE holding_id=erpx_cqrs_read.current_holding_id()
               AND projection_name=$1 AND status IN ('active','building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(SHARING_PROJECTION_NAME)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database)?;
        transaction.commit().await.map_err(map_database)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint.as_ref().and_then(|value| {
            chrono::Utc::now()
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

#[derive(Debug, Clone)]
pub struct OrganizationSharingProjector {
    event_types: Vec<String>,
}

impl Default for OrganizationSharingProjector {
    fn default() -> Self {
        Self::new()
    }
}

impl OrganizationSharingProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SHARING_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

#[async_trait]
impl Projector for OrganizationSharingProjector {
    fn name(&self) -> &str {
        SHARING_PROJECTION_NAME
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let agreement = validate_projection_event(context, event)?;
        sqlx::query(
            "INSERT INTO erpx_organization_read.sharing_agreement_projection
                (holding_id,projection_name,generation,partition_key,sharing_agreement_id,
                 source_aggregate_version,source_global_sequence,source_scope,target_scope,
                 sharing_status,state,aggregate_updated_at,projected_at)
             VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,sharing_agreement_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                           source_global_sequence=EXCLUDED.source_global_sequence,
                           source_scope=EXCLUDED.source_scope,target_scope=EXCLUDED.target_scope,
                           sharing_status=EXCLUDED.sharing_status,state=EXCLUDED.state,
                           aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_organization_read.sharing_agreement_projection.holding_id
                       = erpx_cqrs_read.current_holding_id()
               AND erpx_organization_read.sharing_agreement_projection.source_aggregate_version
                       < EXCLUDED.source_aggregate_version",
        )
        .bind(SHARING_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(agreement.id().as_uuid())
        .bind(to_i64_projection(agreement.version())?)
        .bind(to_i64_projection(event.global_sequence)?)
        .bind(serde_json::to_value(agreement.source_scope()).map_err(projection_failure)?)
        .bind(serde_json::to_value(agreement.target_scope()).map_err(projection_failure)?)
        .bind(status_name(agreement.status()))
        .bind(serde_json::to_value(&agreement).map_err(projection_failure)?)
        .bind(event.occurred_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_organization_read.sharing_agreement_projection
             WHERE holding_id=erpx_cqrs_read.current_holding_id()
               AND projection_name=$1 AND generation=$2",
        )
        .bind(SHARING_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }
}

#[async_trait]
impl SharingReadRepository for OrganizationSharingReadAdapter {
    async fn get_sharing_agreement(
        &self,
        authority_scope: &AuthorityScope,
        sharing_agreement_id: SharingAgreementId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<OrganizationSharingAgreement>>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let partition = sharing_partition(sharing_agreement_id);
        let checkpoint = self
            .consistency_checkpoint(consistency, Some(&partition))
            .await?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let state: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT projection.state
             FROM erpx_organization_read.sharing_agreement_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.holding_id=projection.holding_id
              AND generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation
              AND generation.status='active'
             WHERE projection.holding_id=erpx_cqrs_read.current_holding_id()
               AND projection.projection_name=$1 AND projection.partition_key=$2
               AND projection.sharing_agreement_id=$3",
        )
        .bind(SHARING_PROJECTION_NAME)
        .bind(&partition)
        .bind(sharing_agreement_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database)?;
        transaction.commit().await.map_err(map_database)?;
        let agreement = state.map(decode_agreement).transpose()?.filter(|value| {
            authority_scope.contains(value.source_scope())
                || authority_scope.contains(value.target_scope())
        });
        Ok(ReadProjection {
            value: agreement,
            metadata: self.metadata(checkpoint).await?,
        })
    }

    async fn list_sharing_agreements(
        &self,
        authority_scope: &AuthorityScope,
        filter: &SharingListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SharingAgreementList>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        if !(1..=100).contains(&filter.page_size) {
            return Err(PortError::Denied);
        }
        let checkpoint = self.consistency_checkpoint(consistency, None).await?;
        let mut cursor = filter.after_id.map(SharingAgreementId::as_uuid);
        let mut agreements = Vec::with_capacity(usize::from(filter.page_size) + 1);
        let mut scan_complete = false;
        for _ in 0..MAX_LIST_SCAN_BATCHES {
            let mut transaction = self.pool.begin().await.map_err(map_database)?;
            let rows = sqlx::query(
                "SELECT projection.sharing_agreement_id,projection.state
                 FROM erpx_organization_read.sharing_agreement_projection projection
                 JOIN erpx_cqrs_read.projection_generations generation
                   ON generation.holding_id=projection.holding_id
                  AND generation.projection_name=projection.projection_name
                  AND generation.generation=projection.generation
                  AND generation.status='active'
                 WHERE projection.holding_id=erpx_cqrs_read.current_holding_id()
                   AND projection.projection_name=$1
                   AND ($2::text IS NULL OR projection.sharing_status=$2)
                   AND ($3::uuid IS NULL OR projection.sharing_agreement_id < $3)
                 ORDER BY projection.sharing_agreement_id DESC LIMIT $4",
            )
            .bind(SHARING_PROJECTION_NAME)
            .bind(filter.status.map(status_name))
            .bind(cursor)
            .bind(LIST_SCAN_BATCH)
            .fetch_all(&mut *transaction)
            .await
            .map_err(map_database)?;
            transaction.commit().await.map_err(map_database)?;
            if rows.is_empty() {
                scan_complete = true;
                break;
            }
            let row_count = rows.len();
            for row in rows {
                cursor = Some(row.try_get("sharing_agreement_id").map_err(map_database)?);
                let agreement = decode_agreement(row.try_get("state").map_err(map_database)?)?;
                if authority_scope.contains(agreement.source_scope())
                    || authority_scope.contains(agreement.target_scope())
                {
                    agreements.push(agreement);
                    if agreements.len() > usize::from(filter.page_size) {
                        break;
                    }
                }
            }
            if agreements.len() > usize::from(filter.page_size)
                || row_count < usize::try_from(LIST_SCAN_BATCH).unwrap_or(usize::MAX)
            {
                scan_complete = true;
                break;
            }
        }
        if !scan_complete {
            return Err(PortError::Unavailable);
        }
        let next_cursor = if agreements.len() > usize::from(filter.page_size) {
            agreements.truncate(usize::from(filter.page_size));
            agreements.last().map(|value| value.id())
        } else {
            None
        };
        Ok(ReadProjection {
            value: SharingAgreementList {
                agreements,
                next_cursor,
            },
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

fn validate_projection_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<OrganizationSharingAgreement, ProjectionFailure> {
    if context.projection != SHARING_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != "organization_sharing_agreement"
        || event.event_type != SHARING_SNAPSHOT_EVENT_TYPE
        || event.event_version != SHARING_SNAPSHOT_EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "organization sharing projector received an incompatible event",
        ));
    }
    let agreement: OrganizationSharingAgreement =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    agreement
        .validate_rehydrated()
        .map_err(projection_failure)?;
    if agreement.id().as_uuid() != event.aggregate_id.as_uuid()
        || agreement.version() != event.aggregate_version
        || agreement.source_scope() != &event.active_scope
        || event.partition != sharing_partition(agreement.id())
    {
        return Err(ProjectionFailure::new(
            "organization sharing snapshot metadata does not match its envelope",
        ));
    }
    Ok(agreement)
}

fn decode_agreement(value: serde_json::Value) -> Result<OrganizationSharingAgreement, PortError> {
    let agreement: OrganizationSharingAgreement =
        serde_json::from_value(value).map_err(|_| PortError::Internal)?;
    agreement
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    Ok(agreement)
}

const fn status_name(status: SharingStatus) -> &'static str {
    match status {
        SharingStatus::Proposed => "proposed",
        SharingStatus::Active => "active",
        SharingStatus::Rejected => "rejected",
        SharingStatus::Revoked => "revoked",
        SharingStatus::Expired => "expired",
    }
}

fn valid_compact_uuid(value: &str) -> bool {
    value.len() == 32 && Uuid::parse_str(value).is_ok()
}

fn to_i64_projection(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("projection position overflow"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

fn map_read_store(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

fn map_database(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_is_exactly_bound_to_the_sharing_snapshot_contract() {
        let projector = OrganizationSharingProjector::new();
        assert_eq!(projector.name(), SHARING_PROJECTION_NAME);
        assert_eq!(projector.event_types(), [SHARING_SNAPSHOT_EVENT_TYPE]);
    }

    #[test]
    fn read_side_has_no_write_authority_or_newest_generation_fallback() {
        let source = include_str!("sharing_read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("ORDER BY projected_at ", "DESC")));
        assert!(source.contains("generation.status='active'"));
    }
}
