use crate::{PROJECTION_NAME, RECORD_SNAPSHOT_EVENT_TYPE, SNAPSHOT_EVENT_TYPE};
use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_learning::{Enrollment, LearningRecord};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearningEnrollmentSnapshot {
    pub enrollment: Enrollment,
}
#[derive(Debug, Clone)]
pub struct LearningProjector {
    event_types: Vec<String>,
}
impl LearningProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![
                SNAPSHOT_EVENT_TYPE.into(),
                RECORD_SNAPSHOT_EVENT_TYPE.into(),
            ],
        }
    }
}
impl Default for LearningProjector {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl Projector for LearningProjector {
    fn name(&self) -> &str {
        PROJECTION_NAME
    }
    fn event_types(&self) -> &[String] {
        &self.event_types
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if event.event_type == RECORD_SNAPSHOT_EVENT_TYPE {
            if ctx.projection != PROJECTION_NAME
                || event.context != "learning"
                || event.event_version != 1
            {
                return Err(failure(
                    "learning record projector received an incompatible event",
                ));
            }
            let record: LearningRecord = serde_json::from_value(
                event
                    .payload
                    .get("record")
                    .cloned()
                    .ok_or_else(|| failure("learning record payload is missing"))?,
            )
            .map_err(failure)?;
            if record.id() != event.aggregate_id.as_uuid()
                || record.version() != event.aggregate_version
                || record.scope() != &event.tenant_scope
                || record.kind().token() != event.aggregate_type
            {
                return Err(failure("learning record metadata mismatch"));
            }
            let lifecycle = serde_json::to_value(record.status())
                .map_err(failure)?
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| failure("learning record status is missing"))?;
            let changed = sqlx::query(
                "INSERT INTO erpx_learning_read.governed_record_projection (projection_name,generation,partition_key,record_id,record_kind,authority_scope,lifecycle_state,projection,source_aggregate_version,source_position,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,record_id) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,lifecycle_state=EXCLUDED.lifecycle_state,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp() WHERE erpx_learning_read.governed_record_projection.source_aggregate_version < EXCLUDED.source_aggregate_version",
            )
            .bind(PROJECTION_NAME)
            .bind(ctx.generation)
            .bind(&event.partition)
            .bind(record.id())
            .bind(record.kind().token())
            .bind(serde_json::to_value(record.scope()).map_err(failure)?)
            .bind(lifecycle)
            .bind(serde_json::to_value(&record).map_err(failure)?)
            .bind(i64v(event.aggregate_version)?)
            .bind(i64v(event.global_sequence)?)
            .execute(&mut **tx)
            .await
            .map_err(failure)?;
            if changed.rows_affected() != 1 {
                return Err(failure("learning record version did not advance"));
            }
            return Ok(());
        }
        if ctx.projection != PROJECTION_NAME
            || event.context != "learning"
            || event.aggregate_type != "enrollment"
            || event.event_type != SNAPSHOT_EVENT_TYPE
            || event.event_version != 1
        {
            return Err(ProjectionFailure::new(
                "learning projector received an incompatible event",
            ));
        }
        let snapshot: LearningEnrollmentSnapshot =
            serde_json::from_value(event.payload.clone()).map_err(failure)?;
        if snapshot.enrollment.id().as_uuid() != event.aggregate_id.as_uuid()
            || snapshot.enrollment.version() != event.aggregate_version
            || snapshot.enrollment.scope() != &event.tenant_scope
        {
            return Err(ProjectionFailure::new(
                "learning snapshot metadata mismatch",
            ));
        }
        let document = serde_json::to_value(&snapshot.enrollment).map_err(failure)?;
        let status = document
            .get("status")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| ProjectionFailure::new("learning status is missing"))?;
        let result=sqlx::query("INSERT INTO erpx_learning_read.enrollment_projection (projection_name,generation,partition_key,enrollment_id,authority_scope,lifecycle_state,projection,source_aggregate_version,source_position,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,clock_timestamp()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,enrollment_id) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,lifecycle_state=EXCLUDED.lifecycle_state,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp() WHERE erpx_learning_read.enrollment_projection.source_aggregate_version<EXCLUDED.source_aggregate_version")
   .bind(PROJECTION_NAME).bind(ctx.generation).bind(&event.partition).bind(snapshot.enrollment.id().as_uuid()).bind(serde_json::to_value(snapshot.enrollment.scope()).map_err(failure)?).bind(status).bind(document).bind(i64v(event.aggregate_version)?).bind(i64v(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "learning snapshot version did not advance",
            ));
        }
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_learning_read.enrollment_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        sqlx::query("DELETE FROM erpx_learning_read.governed_record_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
}
fn i64v(v: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(v).map_err(|_| ProjectionFailure::new("learning numeric value is too large"))
}
fn failure(e: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projector_is_generation_managed() {
        let p = LearningProjector::new();
        assert_eq!(p.name(), PROJECTION_NAME);
        assert_eq!(
            p.event_types(),
            &[
                SNAPSHOT_EVENT_TYPE.to_owned(),
                RECORD_SNAPSHOT_EVENT_TYPE.to_owned()
            ]
        );
        assert!(include_str!("projection.rs").contains("ctx.generation"));
    }
}
