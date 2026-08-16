#![doc = "PostgreSQL CQRS adapters for Learning."]
#![forbid(unsafe_code)]

mod directory;
mod projection;
pub use projection::{LearningEnrollmentSnapshot, LearningProjector};

pub const PROJECTION_NAME: &str = "learning.enrollments";
pub const SNAPSHOT_EVENT_TYPE: &str = "learning.enrollment.snapshot.v1";
pub const RECORD_SNAPSHOT_EVENT_TYPE: &str = "learning.record.snapshot.v1";

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::ReadPool;
use erpx_learning::{
    AuthorizedLearningAccess, CommitReceipt, Course, CourseId, Enrollment, EnrollmentId,
    EnrollmentProjection, LearningAggregate, LearningCommit, LearningReadRepository,
    LearningRecord, LearningWriteRepository, PortError,
};
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Debug, Clone)]
pub struct SqlxLearningWriteRepository {
    pool: PgPool,
}
impl SqlxLearningWriteRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl LearningWriteRepository for SqlxLearningWriteRepository {
    async fn load_course(
        &self,
        authority: &AuthorizedLearningAccess,
        id: CourseId,
    ) -> Result<Option<Course>, PortError> {
        load_aggregate(&self.pool, authority, "course", id.as_uuid()).await
    }
    async fn load_enrollment(
        &self,
        authority: &AuthorizedLearningAccess,
        id: EnrollmentId,
    ) -> Result<Option<Enrollment>, PortError> {
        load_aggregate(&self.pool, authority, "enrollment", id.as_uuid()).await
    }
    async fn load_record(
        &self,
        authority: &AuthorizedLearningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<LearningRecord>, PortError> {
        load_aggregate_any(&self.pool, authority, id).await
    }
    async fn commit(&self, request: LearningCommit) -> Result<CommitReceipt, PortError> {
        request.identity.validate()?;
        validate_authority(&request.authority, request.aggregate.scope())?;
        let tenant = request.authority.actor.tenant_id;
        let key = request.aggregate.key();
        let version = request.aggregate.version();
        let (kind, id, document) = match &request.aggregate {
            LearningAggregate::Course(value) => {
                ("course", value.id().as_uuid(), serde_json::to_value(value))
            }
            LearningAggregate::Enrollment(value) => (
                "enrollment",
                value.id().as_uuid(),
                serde_json::to_value(value),
            ),
            LearningAggregate::Record(value) => (
                value.kind().token(),
                value.id(),
                serde_json::to_value(value),
            ),
        };
        let document = document.map_err(|_| PortError::InvalidState)?;
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        set_tenant(&mut tx, tenant.as_uuid()).await?;
        if let Some(row) = sqlx::query("SELECT command_id,request_digest,aggregate_key,aggregate_version,committed_at FROM erpx_learning_write.command_receipts WHERE tenant_id=$1 AND idempotency_key=$2").bind(tenant.as_uuid()).bind(&request.identity.idempotency_key).fetch_optional(&mut *tx).await.map_err(map_error)? {
            let command_id: uuid::Uuid = row.try_get("command_id").map_err(|_| PortError::Internal)?; let digest: String = row.try_get("request_digest").map_err(|_| PortError::Internal)?; let stored_key: String = row.try_get("aggregate_key").map_err(|_| PortError::Internal)?; let stored_version: i64 = row.try_get("aggregate_version").map_err(|_| PortError::Internal)?;
            if command_id != request.identity.command_id.as_uuid() || digest != request.identity.request_digest || stored_key != key || u64::try_from(stored_version).ok() != Some(version) { return Err(PortError::Conflict); }
            let committed_at = row.try_get("committed_at").map_err(|_| PortError::Internal)?;
            let (event_id, global_sequence): (uuid::Uuid, i64) = sqlx::query_as(
                "SELECT event_id,global_sequence FROM erpx_cqrs_write.outbox_events WHERE tenant_id=$1 AND command_id=$2 AND event_type=$3 ORDER BY global_sequence DESC LIMIT 1",
            )
            .bind(tenant.as_uuid())
            .bind(command_id)
            .bind(erpx_integration_contracts::ITEM_DIRECTORY_OWNER_EVENT_TYPE)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_error)?;
            tx.commit().await.map_err(map_error)?;
            return Ok(CommitReceipt {
                aggregate_key: stored_key,
                version,
                committed_at,
                replayed: true,
                event_id,
                global_sequence: u64::try_from(global_sequence)
                    .map_err(|_| PortError::Internal)?,
            });
        }
        let current = sqlx::query_scalar::<_,i64>("SELECT aggregate_version FROM erpx_learning_write.aggregate_versions WHERE tenant_id=$1 AND aggregate_key=$2 FOR UPDATE").bind(tenant.as_uuid()).bind(&key).fetch_optional(&mut *tx).await.map_err(map_error)?;
        match (request.expected_version, current) {
            (None, None) if version == 1 => {}
            (Some(expected), Some(actual))
                if i64::try_from(expected).ok() == Some(actual)
                    && version == expected.checked_add(1).ok_or(PortError::Conflict)? => {}
            _ => return Err(PortError::Conflict),
        }
        let scope =
            serde_json::to_value(request.aggregate.scope()).map_err(|_| PortError::InvalidState)?;
        let projection_document = document.clone();
        sqlx::query("SELECT erpx_learning_write.persist_normalized($1,$2,$3,$4,$5,$6,$7)")
            .bind(tenant.as_uuid())
            .bind(&key)
            .bind(kind)
            .bind(id)
            .bind(&scope)
            .bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?)
            .bind(document)
            .execute(&mut *tx)
            .await
            .map_err(map_error)?;
        sqlx::query("INSERT INTO erpx_learning_write.audit_log (tenant_id,command_id,actor_principal_id,action,authority_scope,aggregate_key,aggregate_version,correlation_id,evidence_digest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(tenant.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(request.authority.actor.principal_id.as_uuid()).bind(&request.event_type).bind(scope).bind(&key).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(request.identity.correlation_id.as_uuid()).bind(&request.identity.request_digest).execute(&mut *tx).await.map_err(map_error)?;
        sqlx::query("INSERT INTO erpx_learning_write.outbox (tenant_id,aggregate_key,aggregate_version,event_type,correlation_id,payload) VALUES ($1,$2,$3,$4,$5,$6)").bind(tenant.as_uuid()).bind(&key).bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?).bind(request.event_type).bind(request.identity.correlation_id.as_uuid()).bind(request.event_payload).execute(&mut *tx).await.map_err(map_error)?;
        if kind == "enrollment" {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"tenant_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'learning',$3,$2,$4,1,'enrollment',$5,$6,$7,$8,$9,NULL,$10,clock_timestamp(),clock_timestamp())")
                .bind(tenant.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(format!("learning.enrollment:{}",id.simple())).bind(SNAPSHOT_EVENT_TYPE).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.identity.correlation_id.as_uuid()).bind(serde_json::json!({"enrollment":projection_document})).execute(&mut *tx).await.map_err(map_error)?;
        } else if matches!(kind, "assignment" | "completion") {
            let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"tenant_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
            sqlx::query("INSERT INTO erpx_cqrs_write.outbox_events (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,causation_id,payload,occurred_at,available_at) VALUES ($1,$2,'learning',$3,$2,$4,1,$5,$6,$7,$8,$9,$10,NULL,$11,clock_timestamp(),clock_timestamp())")
                .bind(tenant.as_uuid()).bind(request.identity.command_id.as_uuid()).bind(format!("learning.{kind}:{}",id.simple())).bind(RECORD_SNAPSHOT_EVENT_TYPE).bind(kind).bind(id).bind(i64::try_from(version).map_err(|_|PortError::InvalidState)?).bind(serde_json::to_value(&request.authority.target_scope).map_err(|_|PortError::InvalidState)?).bind(actor).bind(request.identity.correlation_id.as_uuid()).bind(serde_json::json!({"record":projection_document})).execute(&mut *tx).await.map_err(map_error)?;
        }
        let actor = serde_json::json!({"principal_ref":format!("principal:{}",request.authority.actor.principal_id.as_uuid()),"kind":"tenant_user","session_id":null,"represented_principal_ref":null,"support_capability_id":null});
        let draft = directory::learning_directory_draft(&request.aggregate)?;
        let (event_id, global_sequence) = directory::append_directory_draft(
            &mut tx,
            tenant,
            request.identity.command_id,
            request.identity.correlation_id,
            actor,
            &draft,
        )
        .await?;
        let committed_at: DateTime<Utc> = sqlx::query_scalar("INSERT INTO erpx_learning_write.command_receipts (tenant_id,idempotency_key,command_id,request_digest,aggregate_key,aggregate_version) VALUES ($1,$2,$3,$4,$5,$6) RETURNING committed_at").bind(tenant.as_uuid()).bind(request.identity.idempotency_key).bind(request.identity.command_id.as_uuid()).bind(request.identity.request_digest).bind(&key).bind(i64::try_from(version).map_err(|_| PortError::InvalidState)?).fetch_one(&mut *tx).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(CommitReceipt {
            aggregate_key: key,
            version,
            committed_at,
            replayed: false,
            event_id,
            global_sequence,
        })
    }
}
async fn load_aggregate_any<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    authority: &AuthorizedLearningAccess,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    validate_authority(authority, &authority.target_scope)?;
    let mut tx = pool.begin().await.map_err(map_error)?;
    set_tenant(&mut tx, authority.actor.tenant_id.as_uuid()).await?;
    let row=sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_learning_write.aggregate_rehydration WHERE tenant_id=$1 AND aggregate_kind IN ('assignment','completion') AND aggregate_id=$2").bind(authority.actor.tenant_id.as_uuid()).bind(id).fetch_optional(&mut *tx).await.map_err(map_error)?;
    tx.commit().await.map_err(map_error)?;
    let Some(row) = row else { return Ok(None) };
    let scope: erpx_kernel::TenantScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if scope != authority.target_scope {
        return Err(PortError::Denied);
    }
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}
async fn load_aggregate<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    authority: &AuthorizedLearningAccess,
    kind: &str,
    id: uuid::Uuid,
) -> Result<Option<T>, PortError> {
    validate_authority(authority, &authority.target_scope)?;
    let mut tx = pool.begin().await.map_err(map_error)?;
    set_tenant(&mut tx, authority.actor.tenant_id.as_uuid()).await?;
    let row=sqlx::query("SELECT authority_scope,aggregate_document FROM erpx_learning_write.aggregate_rehydration WHERE tenant_id=$1 AND aggregate_kind=$2 AND aggregate_id=$3").bind(authority.actor.tenant_id.as_uuid()).bind(kind).bind(id).fetch_optional(&mut *tx).await.map_err(map_error)?;
    tx.commit().await.map_err(map_error)?;
    let Some(row) = row else { return Ok(None) };
    let scope: erpx_kernel::TenantScope = serde_json::from_value(
        row.try_get("authority_scope")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    if scope != authority.target_scope {
        return Err(PortError::Denied);
    };
    serde_json::from_value(
        row.try_get("aggregate_document")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

#[derive(Debug, Clone)]
pub struct SqlxLearningReadRepository {
    pool: ReadPool,
    tenant_id: erpx_kernel::TenantId,
}
impl SqlxLearningReadRepository {
    pub fn new(pool: PgPool, tenant_id: erpx_kernel::TenantId) -> Result<Self, PortError> {
        let pool =
            ReadPool::for_tenant(pool, tenant_id.as_uuid()).map_err(|_| PortError::Unavailable)?;
        Ok(Self { pool, tenant_id })
    }
}
#[async_trait]
impl LearningReadRepository for SqlxLearningReadRepository {
    async fn get_enrollment(
        &self,
        authority: &AuthorizedLearningAccess,
        id: EnrollmentId,
    ) -> Result<Option<EnrollmentProjection>, PortError> {
        validate_authority(authority, &authority.target_scope)?;
        if authority.actor.tenant_id != self.tenant_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation:Option<uuid::Uuid>=sqlx::query_scalar("SELECT generation FROM erpx_cqrs_read.projection_generations WHERE tenant_id=$1 AND projection_name='learning.enrollments' AND status='active'").bind(self.tenant_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row=sqlx::query("SELECT p.authority_scope,p.projection,p.generation,p.partition_key,p.source_position,p.projected_at,c.global_sequence AS checkpoint_position,EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.tenant_id=p.tenant_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure FROM erpx_learning_read.enrollment_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.tenant_id=p.tenant_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key WHERE p.tenant_id=$1 AND p.projection_name='learning.enrollments' AND p.generation=$2 AND p.enrollment_id=$3").bind(self.tenant_id.as_uuid()).bind(generation).bind(id.as_uuid()).fetch_optional(&mut *tx).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else { return Ok(None) };
        let source_position = u64::try_from(
            row.try_get::<i64, _>("source_position")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let checkpoint_position = row
            .try_get::<Option<i64>, _>("checkpoint_position")
            .map_err(|_| PortError::Internal)?
            .and_then(|v| u64::try_from(v).ok())
            .ok_or(PortError::Stale)?;
        if row
            .try_get::<bool, _>("has_failure")
            .map_err(|_| PortError::Internal)?
            || checkpoint_position < source_position
        {
            return Err(PortError::Stale);
        }
        let scope: erpx_kernel::TenantScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != authority.target_scope {
            return Err(PortError::Denied);
        };
        Ok(Some(EnrollmentProjection {
            enrollment: serde_json::from_value(
                row.try_get("projection").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Internal)?,
        }))
    }

    async fn get_record(
        &self,
        authority: &AuthorizedLearningAccess,
        id: uuid::Uuid,
    ) -> Result<Option<erpx_learning::LearningRecordProjection>, PortError> {
        validate_authority(authority, &authority.target_scope)?;
        if authority.actor.tenant_id != self.tenant_id {
            return Err(PortError::Denied);
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let generation: Option<uuid::Uuid> = sqlx::query_scalar("SELECT generation FROM erpx_cqrs_read.projection_generations WHERE tenant_id=$1 AND projection_name=$2 AND status='active'")
            .bind(self.tenant_id.as_uuid()).bind(PROJECTION_NAME).fetch_optional(&mut *tx).await.map_err(map_error)?;
        let generation = generation.ok_or(PortError::Unavailable)?;
        let row = sqlx::query("SELECT p.authority_scope,p.projection,p.source_position,p.projected_at,c.global_sequence AS checkpoint_position,EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_failures f WHERE f.tenant_id=p.tenant_id AND f.projection_name=p.projection_name AND f.generation=p.generation AND f.partition_key=p.partition_key) AS has_failure FROM erpx_learning_read.governed_record_projection p LEFT JOIN erpx_cqrs_read.projection_checkpoints c ON c.tenant_id=p.tenant_id AND c.projection_name=p.projection_name AND c.generation=p.generation AND c.partition_key=p.partition_key WHERE p.tenant_id=$1 AND p.projection_name=$2 AND p.generation=$3 AND p.record_id=$4")
            .bind(self.tenant_id.as_uuid()).bind(PROJECTION_NAME).bind(generation).bind(id).fetch_optional(&mut *tx).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        let Some(row) = row else { return Ok(None) };
        let (source_position, checkpoint_position) = record_projection_positions(&row)?;
        let scope: erpx_kernel::TenantScope = serde_json::from_value(
            row.try_get("authority_scope")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        if scope != authority.target_scope {
            return Err(PortError::Denied);
        }
        Ok(Some(erpx_learning::LearningRecordProjection {
            record: serde_json::from_value(
                row.try_get("projection").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            generation,
            source_position,
            checkpoint_position,
            projected_at: row
                .try_get("projected_at")
                .map_err(|_| PortError::Internal)?,
        }))
    }
}

fn record_projection_positions(row: &sqlx::postgres::PgRow) -> Result<(u64, u64), PortError> {
    let source = u64::try_from(
        row.try_get::<i64, _>("source_position")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let checkpoint = row
        .try_get::<Option<i64>, _>("checkpoint_position")
        .map_err(|_| PortError::Internal)?
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(PortError::Stale)?;
    if row
        .try_get::<bool, _>("has_failure")
        .map_err(|_| PortError::Internal)?
        || checkpoint < source
    {
        return Err(PortError::Stale);
    }
    Ok((source, checkpoint))
}

fn validate_authority(
    authority: &AuthorizedLearningAccess,
    scope: &erpx_kernel::TenantScope,
) -> Result<(), PortError> {
    if authority.actor.authority_scope.contains(scope) && &authority.target_scope == scope {
        Ok(())
    } else {
        Err(PortError::Denied)
    }
}
async fn set_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant: uuid::Uuid,
) -> Result<(), PortError> {
    sqlx::query("SELECT set_config('app.tenant_id',$1,true)")
        .bind(tenant.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map_error)?;
    Ok(())
}
fn map_error(error: sqlx::Error) -> PortError {
    match &error {
        sqlx::Error::Database(database) if database.code().as_deref() == Some("23505") => {
            PortError::Conflict
        }
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    const WRITE: &str = include_str!(
        "../../../migration/migrations_tenant_write/0077_learning_benefits_field_service.sql"
    );
    const READ: &str = include_str!(
        "../../../migration/migrations_tenant_read/0068_learning_benefits_field_service_projection.sql"
    );
    #[test]
    fn schema_enforces_evidence_self_edit_and_minimal_completion() {
        assert!(WRITE.contains("learner_principal_id <> recorded_by_principal_id"));
        assert!(WRITE.contains("provider_result_guards"));
        assert!(WRITE.contains("completion_facts"));
        assert!(READ.contains("completion_projection"));
        assert!(WRITE.contains("FORCE ROW LEVEL SECURITY"));
    }
    #[test]
    fn read_side_never_uses_write_authority() {
        let source = include_str!("lib.rs");
        let read = source
            .split("pub struct SqlxLearningReadRepository")
            .nth(1)
            .unwrap();
        assert!(!read.contains("erpx_learning_write.aggregate_versions"));
        assert!(read.contains("governed_record_projection"));
        assert!(read.contains("status='active'"));
    }
    #[test]
    fn normalized_write_path_is_atomic_and_rehydratable() {
        assert!(!WRITE.contains("aggregate_snapshots"));
        assert!(!WRITE.contains("snapshot jsonb"));
        for marker in [
            "courses",
            "enrollments",
            "persist_normalized",
            "aggregate_rehydration",
            "audit_log",
            "outbox",
        ] {
            assert!(WRITE.contains(marker));
        }
        let source = include_str!("lib.rs");
        for marker in [
            "command_id != request.identity.command_id",
            "digest != request.identity.request_digest",
            "FOR UPDATE",
            "ReadPool::for_tenant",
            "status='active'",
        ] {
            assert!(source.contains(marker));
        }
        assert!(READ.contains("projection_name text NOT NULL DEFAULT 'learning.enrollments'"));
        assert!(READ.contains("source_aggregate_version"));
        let read = source
            .split("pub struct SqlxLearningReadRepository")
            .nth(1)
            .unwrap()
            .split("fn validate_authority")
            .next()
            .unwrap();
        assert!(!read.contains(&["ORDER BY projected_at DESC", " LIMIT 1"].concat()));
    }
    #[test]
    fn every_learning_benefits_field_service_table_is_forced_under_rls() {
        for relation in [
            "erpx_learning_write.aggregate_versions",
            "erpx_learning_write.courses",
            "erpx_learning_write.enrollments",
            "erpx_learning_write.attendance_guards",
            "erpx_learning_write.provider_result_guards",
            "erpx_learning_write.completion_facts",
            "erpx_learning_write.command_receipts",
            "erpx_learning_write.audit_log",
            "erpx_learning_write.outbox",
            "erpx_benefits_write.aggregate_versions",
            "erpx_benefits_write.plans",
            "erpx_benefits_write.enrollments",
            "erpx_benefits_write.eligibility_guards",
            "erpx_benefits_write.provider_acknowledgement_guards",
            "erpx_benefits_write.payroll_handoff_guards",
            "erpx_benefits_write.command_receipts",
            "erpx_benefits_write.audit_log",
            "erpx_benefits_write.outbox",
            "erpx_field_service_write.aggregate_versions",
            "erpx_field_service_write.agreements",
            "erpx_field_service_write.service_orders",
            "erpx_field_service_write.part_issue_guards",
            "erpx_field_service_write.visit_evidence_guards",
            "erpx_field_service_write.exception_guards",
            "erpx_field_service_write.billing_handoff_guards",
            "erpx_field_service_write.command_receipts",
            "erpx_field_service_write.audit_log",
            "erpx_field_service_write.outbox",
        ] {
            assert!(
                WRITE.contains(&format!("'{relation}'")),
                "missing RLS inventory for {relation}"
            );
        }
        assert_eq!(READ.matches("CREATE TABLE").count(), 10);
        for ddl in [WRITE, READ] {
            assert!(ddl.contains("ENABLE ROW LEVEL SECURITY"));
            assert!(ddl.contains("FORCE ROW LEVEL SECURITY"));
            assert!(ddl.contains("current_setting(''app.tenant_id'', true)"));
        }
    }
}
