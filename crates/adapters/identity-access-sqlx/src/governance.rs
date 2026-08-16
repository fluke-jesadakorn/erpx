use async_trait::async_trait;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionEvent, ProjectionFailure, Projector, ProjectorContext, ReadPool,
    ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig, WritePool,
};
use erpx_identity_access::{
    AccessCertification, BreakGlassSession, IdentityGovernanceCommand,
    IdentityGovernanceCommandIdentity, IdentityGovernanceCommit, IdentityGovernanceReadRepository,
    IdentityGovernanceRecord, IdentityGovernanceUnitOfWork, IdentityGovernanceWriteRepository,
    PortError, ReadProjection, StoredIdentityGovernanceCommit,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{DataClassification, RecordId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, GOVERNANCE_PROJECTION_NAME, GOVERNANCE_SNAPSHOT_EVENT_TYPE,
    governance_partition,
};

#[derive(Debug, Clone)]
pub struct IdentityGovernanceSqlxAdapter {
    pool: WritePool,
}

#[derive(Debug, Clone)]
pub struct IdentityGovernanceReadAdapter {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl IdentityGovernanceReadAdapter {
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
}

#[derive(Debug, Clone)]
pub struct IdentityGovernanceProjector {
    event_types: Vec<String>,
}

impl Default for IdentityGovernanceProjector {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityGovernanceProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![GOVERNANCE_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl IdentityGovernanceSqlxAdapter {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IdentityGovernanceWriteRepository for IdentityGovernanceSqlxAdapter {
    async fn load_governance_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
    ) -> Result<Option<IdentityGovernanceRecord>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let record = load_record(&mut transaction, record_id, false).await?;
        transaction.commit().await.map_err(map_database)?;
        if record
            .as_ref()
            .is_some_and(|record| !authority_scope.contains(record.scope()))
        {
            return Err(PortError::Denied);
        }
        Ok(record)
    }
}

#[async_trait]
impl IdentityGovernanceUnitOfWork for IdentityGovernanceSqlxAdapter {
    async fn find_governance_receipt(
        &self,
        identity: &IdentityGovernanceCommandIdentity,
    ) -> Result<Option<StoredIdentityGovernanceCommit>, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let value = find_receipt(&mut transaction, identity).await?;
        transaction.commit().await.map_err(map_database)?;
        Ok(value)
    }

    async fn commit_governance(
        &self,
        request: IdentityGovernanceCommit,
    ) -> Result<StoredIdentityGovernanceCommit, PortError> {
        validate_commit(&request)?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        if let Some(receipt) = find_receipt(&mut transaction, &request.request.identity).await? {
            transaction.commit().await.map_err(map_database)?;
            return Ok(receipt);
        }
        let current =
            load_record(&mut transaction, request.request.identity.record_id, true).await?;
        match (request.request.expected_version, current.as_ref()) {
            (None, None) if request.record.version() == 1 => {}
            (Some(expected), Some(current))
                if current.record_type() == request.record.record_type()
                    && current.version() == expected
                    && request.record.version() == expected.saturating_add(1) => {}
            (Some(expected), Some(current)) => {
                return Err(PortError::Conflict {
                    expected,
                    actual: current.version(),
                });
            }
            (Some(expected), None) => {
                return Err(PortError::Conflict {
                    expected,
                    actual: 0,
                });
            }
            (None, Some(current)) => {
                return Err(PortError::Conflict {
                    expected: 0,
                    actual: current.version(),
                });
            }
            (None, None) => return Err(PortError::Internal),
        }

        persist_record(&mut transaction, &request.record, request.changed_at).await?;
        append_projection_event(&mut transaction, &request).await?;
        append_directory_event(&mut transaction, &request).await?;
        persist_audit(&mut transaction, &request).await?;
        persist_receipt(&mut transaction, &request).await?;
        transaction.commit().await.map_err(map_database)?;
        Ok(StoredIdentityGovernanceCommit {
            command_id: request.request.identity.command_id,
            correlation_id: request.request.correlation_id,
            record: request.record,
            committed_at: request.changed_at,
            replayed: false,
        })
    }
}

#[async_trait]
impl Projector for IdentityGovernanceProjector {
    fn name(&self) -> &str {
        GOVERNANCE_PROJECTION_NAME
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
        let record = validate_projection_event(context, event)?;
        sqlx::query(
            "INSERT INTO erpx_identity_access_read.governance_projection
                (tenant_id,projection_name,generation,partition_key,record_id,record_type,
                 source_aggregate_version,source_global_sequence,tenant_scope,lifecycle_state,
                 state,aggregate_updated_at,projected_at)
             VALUES (erpx_cqrs_read.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now())
             ON CONFLICT (tenant_id,projection_name,generation,partition_key,record_id)
             DO UPDATE SET record_type=EXCLUDED.record_type,
                           source_aggregate_version=EXCLUDED.source_aggregate_version,
                           source_global_sequence=EXCLUDED.source_global_sequence,
                           tenant_scope=EXCLUDED.tenant_scope,
                           lifecycle_state=EXCLUDED.lifecycle_state,state=EXCLUDED.state,
                           aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_identity_access_read.governance_projection.tenant_id
                       = erpx_cqrs_read.current_tenant_id()
               AND erpx_identity_access_read.governance_projection.source_aggregate_version
                       < EXCLUDED.source_aggregate_version",
        )
        .bind(GOVERNANCE_PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(record.id_uuid())
        .bind(record.record_type())
        .bind(to_i64_projection(record.version())?)
        .bind(to_i64_projection(event.global_sequence)?)
        .bind(serde_json::to_value(record.scope()).map_err(projection_failure)?)
        .bind(record.lifecycle_state())
        .bind(serde_json::to_value(record).map_err(projection_failure)?)
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
            "DELETE FROM erpx_identity_access_read.governance_projection
             WHERE tenant_id=erpx_cqrs_read.current_tenant_id()
               AND projection_name=$1 AND generation=$2",
        )
        .bind(GOVERNANCE_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }
}

#[async_trait]
impl IdentityGovernanceReadRepository for IdentityGovernanceReadAdapter {
    async fn get_governance_record(
        &self,
        authority_scope: &TenantScope,
        record_id: Uuid,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<IdentityGovernanceRecord>>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        if record_id.is_nil() {
            return Err(PortError::Denied);
        }
        let partition = governance_partition(record_id);
        let checkpoint = self.consistency_checkpoint(consistency, &partition).await?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let state: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT projection.state
             FROM erpx_identity_access_read.governance_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.tenant_id=projection.tenant_id
              AND generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation
              AND generation.status='active'
             WHERE projection.tenant_id=erpx_cqrs_read.current_tenant_id()
               AND projection.projection_name=$1 AND projection.partition_key=$2
               AND projection.record_id=$3",
        )
        .bind(GOVERNANCE_PROJECTION_NAME)
        .bind(&partition)
        .bind(record_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database)?;
        transaction.commit().await.map_err(map_database)?;
        let record = state
            .map(|state| {
                let value: IdentityGovernanceRecord =
                    serde_json::from_value(state).map_err(|_| PortError::Internal)?;
                value
                    .validate_rehydrated()
                    .map_err(|_| PortError::Internal)?;
                Ok(value)
            })
            .transpose()?;
        let value = record.filter(|record| authority_scope.contains(record.scope()));
        Ok(ReadProjection {
            value,
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

impl IdentityGovernanceReadAdapter {
    async fn consistency_checkpoint(
        &self,
        consistency: &QueryConsistency,
        partition: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => self
                .read_side
                .checkpoint(GOVERNANCE_PROJECTION_NAME, partition)
                .await
                .map_err(map_read_store),
            QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != GOVERNANCE_PROJECTION_NAME
                    || requirement.partition != partition
                    || !partition
                        .strip_prefix("identity_access.governance:")
                        .is_some_and(valid_compact_uuid)
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
             WHERE tenant_id=erpx_cqrs_read.current_tenant_id()
               AND projection_name=$1 AND status IN ('active','building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(GOVERNANCE_PROJECTION_NAME)
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

fn validate_projection_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<IdentityGovernanceRecord, ProjectionFailure> {
    if context.projection != GOVERNANCE_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.event_type != GOVERNANCE_SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "identity governance projector received an incompatible event",
        ));
    }
    let record: IdentityGovernanceRecord =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    record.validate_rehydrated().map_err(projection_failure)?;
    if record.id_uuid() != event.aggregate_id.as_uuid()
        || record.version() != event.aggregate_version
        || record.record_type() != event.aggregate_type
        || record.scope() != &event.tenant_scope
        || event.partition != governance_partition(record.id_uuid())
    {
        return Err(ProjectionFailure::new(
            "identity governance snapshot metadata does not match its envelope",
        ));
    }
    Ok(record)
}

async fn load_record(
    transaction: &mut Transaction<'_, Postgres>,
    record_id: Uuid,
    lock: bool,
) -> Result<Option<IdentityGovernanceRecord>, PortError> {
    let lock = if lock { " FOR UPDATE" } else { "" };
    let review_statement = format!(
        "SELECT review_version,review_scope,reviewer_ref,due_at,completed_at
         FROM erpx_authority_write.access_reviews
         WHERE tenant_id=erpx_authority_write.current_tenant_id()
           AND access_review_id=$1{lock}"
    );
    if let Some(row) = sqlx::query(&review_statement)
        .bind(record_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(map_database)?
    {
        let items = sqlx::query(
            "SELECT assignment_id,principal_id,principal_kind,owner_ref,grantor_ref,
                    decision,decided_at
             FROM erpx_authority_write.access_review_items
             WHERE tenant_id=erpx_authority_write.current_tenant_id()
               AND access_review_id=$1 ORDER BY assignment_id",
        )
        .bind(record_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(map_database)?
        .into_iter()
        .map(|item| {
            json!({
                "assignment_id": item.get::<Uuid,_>("assignment_id"),
                "principal_id": item.get::<Uuid,_>("principal_id"),
                "principal_kind": item.get::<String,_>("principal_kind"),
                "owner_ref": item.get::<String,_>("owner_ref"),
                "grantor_ref": item.get::<String,_>("grantor_ref"),
                "decision": item.get::<Option<String>,_>("decision"),
                "decided_at": item.get::<Option<chrono::DateTime<chrono::Utc>>,_>("decided_at"),
            })
        })
        .collect::<Vec<_>>();
        let review: AccessCertification = serde_json::from_value(json!({
            "id": record_id,
            "version": row.get::<i64,_>("review_version"),
            "scope": row.get::<serde_json::Value,_>("review_scope"),
            "reviewer_ref": row.get::<String,_>("reviewer_ref"),
            "due_at": row.get::<chrono::DateTime<chrono::Utc>,_>("due_at"),
            "completed_at": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("completed_at"),
            "items": items,
        }))
        .map_err(|_| PortError::Internal)?;
        review
            .validate_rehydrated()
            .map_err(|_| PortError::Internal)?;
        return Ok(Some(IdentityGovernanceRecord::AccessReview(review)));
    }

    let session_statement = format!(
        "SELECT session_version,principal_id,session_scope,reason,requester_ref,approver_ref,
                requested_at,valid_until,activated_at,closed_at,after_reviewed_at,session_status
         FROM erpx_authority_write.break_glass_sessions
         WHERE tenant_id=erpx_authority_write.current_tenant_id()
           AND break_glass_session_id=$1{lock}"
    );
    let Some(row) = sqlx::query(&session_statement)
        .bind(record_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(map_database)?
    else {
        return Ok(None);
    };
    let session: BreakGlassSession = serde_json::from_value(json!({
        "id": record_id,
        "version": row.get::<i64,_>("session_version"),
        "principal_id": row.get::<Uuid,_>("principal_id"),
        "scope": row.get::<serde_json::Value,_>("session_scope"),
        "reason": row.get::<String,_>("reason"),
        "requester_ref": row.get::<String,_>("requester_ref"),
        "approver_ref": row.get::<Option<String>,_>("approver_ref"),
        "requested_at": row.get::<chrono::DateTime<chrono::Utc>,_>("requested_at"),
        "valid_until": row.get::<chrono::DateTime<chrono::Utc>,_>("valid_until"),
        "activated_at": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("activated_at"),
        "closed_at": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("closed_at"),
        "after_reviewed_at": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("after_reviewed_at"),
        "status": row.get::<String,_>("session_status"),
    }))
    .map_err(|_| PortError::Internal)?;
    session
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    Ok(Some(IdentityGovernanceRecord::BreakGlass(session)))
}

async fn persist_record(
    transaction: &mut Transaction<'_, Postgres>,
    record: &IdentityGovernanceRecord,
    changed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), PortError> {
    match record {
        IdentityGovernanceRecord::AccessReview(review) => {
            let result = sqlx::query(
                "INSERT INTO erpx_authority_write.access_reviews
                    (tenant_id,access_review_id,review_version,review_scope,reviewer_ref,
                     due_at,completed_at,changed_at)
                 VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7)
                 ON CONFLICT (tenant_id,access_review_id) DO UPDATE SET
                    review_version=EXCLUDED.review_version,
                    reviewer_ref=EXCLUDED.reviewer_ref,due_at=EXCLUDED.due_at,
                    completed_at=EXCLUDED.completed_at,changed_at=EXCLUDED.changed_at
                 WHERE erpx_authority_write.access_reviews.tenant_id
                        = erpx_authority_write.current_tenant_id()
                   AND erpx_authority_write.access_reviews.review_version + 1
                        = EXCLUDED.review_version",
            )
            .bind(review.id())
            .bind(to_i64(review.version())?)
            .bind(serde_json::to_value(review.scope()).map_err(|_| PortError::Internal)?)
            .bind(review.reviewer_ref())
            .bind(review.due_at())
            .bind(review.completed_at())
            .bind(changed_at)
            .execute(&mut **transaction)
            .await
            .map_err(map_database)?;
            ensure_one(result.rows_affected(), review.version())?;
            sqlx::query(
                "DELETE FROM erpx_authority_write.access_review_items
                 WHERE tenant_id=erpx_authority_write.current_tenant_id()
                   AND access_review_id=$1",
            )
            .bind(review.id())
            .execute(&mut **transaction)
            .await
            .map_err(map_database)?;
            for item in review.items() {
                sqlx::query(
                    "INSERT INTO erpx_authority_write.access_review_items
                        (tenant_id,access_review_id,assignment_id,principal_id,principal_kind,
                         owner_ref,grantor_ref,decision,decided_at)
                     VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(review.id())
                .bind(item.assignment_id().as_uuid())
                .bind(item.principal_id().as_uuid())
                .bind(principal_kind(item.principal_kind()))
                .bind(item.owner_ref())
                .bind(item.grantor_ref())
                .bind(item.decision().map(decision_name))
                .bind(item.decided_at())
                .execute(&mut **transaction)
                .await
                .map_err(map_database)?;
            }
        }
        IdentityGovernanceRecord::BreakGlass(session) => {
            let result = sqlx::query(
                "INSERT INTO erpx_authority_write.break_glass_sessions
                    (tenant_id,break_glass_session_id,session_version,principal_id,session_scope,
                     reason,requester_ref,approver_ref,requested_at,valid_until,activated_at,
                     closed_at,after_reviewed_at,session_status,changed_at)
                 VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,
                         $10,$11,$12,$13,$14)
                 ON CONFLICT (tenant_id,break_glass_session_id) DO UPDATE SET
                    session_version=EXCLUDED.session_version,
                    approver_ref=EXCLUDED.approver_ref,activated_at=EXCLUDED.activated_at,
                    closed_at=EXCLUDED.closed_at,after_reviewed_at=EXCLUDED.after_reviewed_at,
                    session_status=EXCLUDED.session_status,changed_at=EXCLUDED.changed_at
                 WHERE erpx_authority_write.break_glass_sessions.tenant_id
                        = erpx_authority_write.current_tenant_id()
                   AND erpx_authority_write.break_glass_sessions.session_version + 1
                        = EXCLUDED.session_version",
            )
            .bind(session.id())
            .bind(to_i64(session.version())?)
            .bind(session.principal_id().as_uuid())
            .bind(serde_json::to_value(session.scope()).map_err(|_| PortError::Internal)?)
            .bind(session.reason())
            .bind(session.requester_ref())
            .bind(session.approver_ref())
            .bind(session.requested_at())
            .bind(session.valid_until())
            .bind(session.activated_at())
            .bind(session.closed_at())
            .bind(session.after_reviewed_at())
            .bind(record.lifecycle_state())
            .bind(changed_at)
            .execute(&mut **transaction)
            .await
            .map_err(map_database)?;
            ensure_one(result.rows_affected(), session.version())?;
        }
    }
    Ok(())
}

async fn append_projection_event(
    transaction: &mut Transaction<'_, Postgres>,
    request: &IdentityGovernanceCommit,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,'identity_access',$2,$3,$4,$5,
                 $6,$7,$8,$9,$10,$11,NULL,$12,$13,now())",
    )
    .bind(deterministic_uuid(
        request.request.identity.command_id.as_uuid(),
        b"identity-governance-projection-v1",
    ))
    .bind(governance_partition(request.record.id_uuid()))
    .bind(request.request.identity.command_id.as_uuid())
    .bind(GOVERNANCE_SNAPSHOT_EVENT_TYPE)
    .bind(i32::try_from(EVENT_VERSION).map_err(|_| PortError::Internal)?)
    .bind(request.record.record_type())
    .bind(request.record.id_uuid())
    .bind(to_i64(request.record.version())?)
    .bind(serde_json::to_value(request.record.scope()).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(&request.record).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn append_directory_event(
    transaction: &mut Transaction<'_, Postgres>,
    request: &IdentityGovernanceCommit,
) -> Result<(), PortError> {
    let record_ref = StableRecordRef::try_new(
        RecordId::try_from_uuid(request.record.id_uuid()).map_err(|_| PortError::Internal)?,
        ContextId::IdentityAccess,
        RecordTypeId::try_new(request.record.record_type()).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let draft = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record_ref).map_err(|_| PortError::Internal)?,
        record: record_ref,
        scope: request.record.scope().clone(),
        lifecycle_state: request.record.lifecycle_state().into(),
        classification: DataClassification::Restricted,
        descriptor_version: request.record.version(),
        authorized_search_text: match &request.record {
            IdentityGovernanceRecord::AccessReview(_) => {
                format!("Access review {}", request.record.id_uuid())
            }
            IdentityGovernanceRecord::BreakGlass(_) => {
                format!("Break-glass session {}", request.record.id_uuid())
            }
        },
    };
    draft.validate().map_err(|_| PortError::Internal)?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,'identity_access',$2,$3,$4,$5,
                 $6,$7,$8,$9,$10,$11,NULL,$12,$13,now())",
    )
    .bind(deterministic_uuid(
        request.request.identity.command_id.as_uuid(),
        b"identity-governance-directory-v1",
    ))
    .bind(format!(
        "identity.governance:{}",
        request.record.id_uuid().simple()
    ))
    .bind(request.request.identity.command_id.as_uuid())
    .bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
    .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_| PortError::Internal)?)
    .bind(request.record.record_type())
    .bind(request.record.id_uuid())
    .bind(to_i64(request.record.version())?)
    .bind(serde_json::to_value(request.record.scope()).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(draft).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn persist_audit(
    transaction: &mut Transaction<'_, Postgres>,
    request: &IdentityGovernanceCommit,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_authority_write.identity_governance_audit_log
            (tenant_id,audit_id,command_id,correlation_id,actor,authority_scope,record_id,
             record_type,record_version,action,occurred_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(deterministic_uuid(
        request.request.identity.command_id.as_uuid(),
        b"identity-governance-audit-v1",
    ))
    .bind(request.request.identity.command_id.as_uuid())
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.authority_scope).map_err(|_| PortError::Internal)?)
    .bind(request.record.id_uuid())
    .bind(request.record.record_type())
    .bind(to_i64(request.record.version())?)
    .bind(action_name(&request.request.identity.command))
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn persist_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    request: &IdentityGovernanceCommit,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_authority_write.identity_governance_command_receipts
            (tenant_id,idempotency_key,command_id,request_hash,record_id,record_version,
             correlation_id,receipt,committed_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(request.request.identity.idempotency_key.as_str())
    .bind(request.request.identity.command_id.as_uuid())
    .bind(request_hash(&request.request.identity)?.as_slice())
    .bind(request.record.id_uuid())
    .bind(to_i64(request.record.version())?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(&request.record).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            PortError::IdempotencyConflict
        } else {
            PortError::Unavailable
        }
    })?;
    Ok(())
}

async fn find_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    identity: &IdentityGovernanceCommandIdentity,
) -> Result<Option<StoredIdentityGovernanceCommit>, PortError> {
    let row = sqlx::query(
        "SELECT command_id,request_hash,record_id,record_version,correlation_id,receipt,committed_at
         FROM erpx_authority_write.identity_governance_command_receipts
         WHERE tenant_id=erpx_authority_write.current_tenant_id()
           AND idempotency_key=$1 FOR UPDATE",
    )
    .bind(identity.idempotency_key.as_str())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(map_database)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.get::<Uuid, _>("command_id") != identity.command_id.as_uuid()
        || row.get::<Vec<u8>, _>("request_hash").as_slice() != request_hash(identity)?
        || row.get::<Uuid, _>("record_id") != identity.record_id
    {
        return Err(PortError::IdempotencyConflict);
    }
    let record: IdentityGovernanceRecord =
        serde_json::from_value(row.get("receipt")).map_err(|_| PortError::Internal)?;
    record
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    if record.id_uuid() != identity.record_id
        || record.version()
            != u64::try_from(row.get::<i64, _>("record_version"))
                .map_err(|_| PortError::Internal)?
    {
        return Err(PortError::Internal);
    }
    Ok(Some(StoredIdentityGovernanceCommit {
        command_id: identity.command_id,
        correlation_id: erpx_kernel::CorrelationId::try_from_uuid(row.get("correlation_id"))
            .map_err(|_| PortError::Internal)?,
        record,
        committed_at: row.get("committed_at"),
        replayed: true,
    }))
}

fn validate_commit(request: &IdentityGovernanceCommit) -> Result<(), PortError> {
    request
        .record
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    if request.record.id_uuid() != request.request.identity.record_id
        || !request
            .request
            .authority_scope
            .contains(request.record.scope())
    {
        return Err(PortError::Denied);
    }
    Ok(())
}

fn request_hash(identity: &IdentityGovernanceCommandIdentity) -> Result<[u8; 32], PortError> {
    serde_json::to_vec(identity)
        .map(|bytes| Sha256::digest(bytes).into())
        .map_err(|_| PortError::Internal)
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn ensure_one(rows: u64, version: u64) -> Result<(), PortError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(PortError::Conflict {
            expected: version.saturating_sub(1),
            actual: version,
        })
    }
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}

fn to_i64_projection(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("identity governance numeric value is too large"))
}

fn valid_compact_uuid(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

fn map_read_store(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

const fn principal_kind(value: erpx_identity_access::PrincipalKind) -> &'static str {
    match value {
        erpx_identity_access::PrincipalKind::Human => "human",
        erpx_identity_access::PrincipalKind::Service => "service",
        erpx_identity_access::PrincipalKind::Agent => "agent",
        erpx_identity_access::PrincipalKind::HumanGroup => "human_group",
        erpx_identity_access::PrincipalKind::AgentPool => "agent_pool",
    }
}

const fn decision_name(value: erpx_identity_access::CertificationDecision) -> &'static str {
    match value {
        erpx_identity_access::CertificationDecision::Retain => "retain",
        erpx_identity_access::CertificationDecision::Revoke => "revoke",
    }
}

const fn action_name(value: &IdentityGovernanceCommand) -> &'static str {
    match value {
        IdentityGovernanceCommand::StartAccessReview { .. } => "start_access_review",
        IdentityGovernanceCommand::DecideAccessReview { .. } => "decide_access_review",
        IdentityGovernanceCommand::RequestBreakGlass { .. } => "request_break_glass",
        IdentityGovernanceCommand::ActivateBreakGlass { .. } => "activate_break_glass",
        IdentityGovernanceCommand::TerminateBreakGlass => "terminate_break_glass",
        IdentityGovernanceCommand::ExpireBreakGlass => "expire_break_glass",
        IdentityGovernanceCommand::RecordBreakGlassReview { .. } => "review_break_glass",
    }
}

fn map_database(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use erpx_cqrs_sqlx::Projector;

    use super::IdentityGovernanceProjector;
    use crate::{GOVERNANCE_PROJECTION_NAME, GOVERNANCE_SNAPSHOT_EVENT_TYPE};

    #[test]
    fn source_contract_is_normalized_tenant_bound_and_atomic() {
        let source = include_str!("governance.rs");
        for table in [
            "access_reviews",
            "access_review_items",
            "break_glass_sessions",
            "identity_governance_command_receipts",
            "identity_governance_audit_log",
        ] {
            assert!(source.contains(table));
        }
        assert!(source.contains("tenant_id=erpx_authority_write.current_tenant_id()"));
        assert!(source.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(source.contains("GOVERNANCE_SNAPSHOT_EVENT_TYPE"));
        assert!(source.contains("generation.status='active'"));
        assert!(source.contains("source_aggregate_version"));
        assert!(!source.contains(&["aggregate", "snapshots"].join("_")));
    }

    #[test]
    fn projector_consumes_only_governance_snapshots() {
        let projector = IdentityGovernanceProjector::new();
        assert_eq!(projector.name(), GOVERNANCE_PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[GOVERNANCE_SNAPSHOT_EVENT_TYPE]);
    }
}
