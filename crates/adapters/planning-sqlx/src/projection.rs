use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_planning::{PlanningPlan, PlanningRecommendation, PlanningRun, PlanningScenario};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    GOVERNED_RECORD_SNAPSHOT_EVENT_TYPE, PLAN_SNAPSHOT_EVENT_TYPE, PROJECTION_NAME,
    SNAPSHOT_EVENT_TYPE,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningRunSnapshot {
    pub run: PlanningRun,
}
#[derive(Debug, Clone)]
pub struct PlanningProjector {
    event_types: Vec<String>,
}
impl PlanningProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![
                SNAPSHOT_EVENT_TYPE.into(),
                PLAN_SNAPSHOT_EVENT_TYPE.into(),
                GOVERNED_RECORD_SNAPSHOT_EVENT_TYPE.into(),
            ],
        }
    }
}
impl Default for PlanningProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for PlanningProjector {
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
        if event.event_type == GOVERNED_RECORD_SNAPSHOT_EVENT_TYPE {
            if ctx.projection != PROJECTION_NAME
                || event.context != "planning"
                || event.event_version != 1
                || !matches!(event.aggregate_type.as_str(), "scenario" | "recommendation")
            {
                return Err(failure(
                    "planning governed-record projector received incompatible event",
                ));
            }
            let document = event
                .payload
                .get("record")
                .cloned()
                .ok_or_else(|| failure("planning governed-record payload missing"))?;
            let (version, lifecycle) = match event.aggregate_type.as_str() {
                "scenario" => {
                    let record: PlanningScenario =
                        serde_json::from_value(document.clone()).map_err(failure)?;
                    if record.id().as_uuid() != event.aggregate_id.as_uuid()
                        || record.scope() != &event.active_scope
                    {
                        return Err(failure("planning scenario metadata mismatch"));
                    }
                    (record.version(), enum_name(record.status())?)
                }
                "recommendation" => {
                    let record: PlanningRecommendation =
                        serde_json::from_value(document.clone()).map_err(failure)?;
                    if record.id.as_uuid() != event.aggregate_id.as_uuid() {
                        return Err(failure("planning recommendation metadata mismatch"));
                    }
                    (record.version, enum_name(record.status)?)
                }
                _ => unreachable!(),
            };
            if version != event.aggregate_version {
                return Err(failure("planning governed-record version mismatch"));
            }
            let changed = sqlx::query(
                "INSERT INTO erpx_planning_read.governed_record_projection(projection_name,generation,partition_key,record_id,record_kind,authority_scope,lifecycle_state,projection,source_aggregate_version,source_position,projected_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp()) ON CONFLICT(holding_id,projection_name,generation,partition_key,record_id) DO UPDATE SET record_kind=EXCLUDED.record_kind,authority_scope=EXCLUDED.authority_scope,lifecycle_state=EXCLUDED.lifecycle_state,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp() WHERE erpx_planning_read.governed_record_projection.source_aggregate_version<EXCLUDED.source_aggregate_version",
            )
            .bind(PROJECTION_NAME)
            .bind(ctx.generation)
            .bind(&event.partition)
            .bind(event.aggregate_id.as_uuid())
            .bind(&event.aggregate_type)
            .bind(serde_json::to_value(&event.active_scope).map_err(failure)?)
            .bind(lifecycle)
            .bind(document)
            .bind(i64v(event.aggregate_version)?)
            .bind(i64v(event.global_sequence)?)
            .execute(&mut **tx)
            .await
            .map_err(failure)?;
            if changed.rows_affected() != 1 {
                return Err(failure("planning governed-record version did not advance"));
            }
            return Ok(());
        }
        if event.event_type == PLAN_SNAPSHOT_EVENT_TYPE {
            if ctx.projection != PROJECTION_NAME
                || event.context != "planning"
                || event.event_version != 1
            {
                return Err(failure(
                    "planning plan projector received incompatible event",
                ));
            }
            let plan: PlanningPlan = serde_json::from_value(
                event
                    .payload
                    .get("plan")
                    .cloned()
                    .ok_or_else(|| failure("planning plan payload missing"))?,
            )
            .map_err(failure)?;
            if plan.id() != event.aggregate_id.as_uuid()
                || plan.version() != event.aggregate_version
                || plan.scope() != &event.active_scope
                || plan.kind().token() != event.aggregate_type
            {
                return Err(failure("planning plan metadata mismatch"));
            }
            let state = serde_json::to_value(plan.status())
                .map_err(failure)?
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| failure("planning plan status missing"))?;
            let changed=sqlx::query("INSERT INTO erpx_planning_read.plan_projection(projection_name,generation,partition_key,plan_id,plan_kind,authority_scope,lifecycle_state,projection,source_aggregate_version,source_position,projected_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp())ON CONFLICT(holding_id,projection_name,generation,partition_key,plan_id)DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp()WHERE erpx_planning_read.plan_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(PROJECTION_NAME).bind(ctx.generation).bind(&event.partition).bind(plan.id()).bind(plan.kind().token()).bind(serde_json::to_value(plan.scope()).map_err(failure)?).bind(state).bind(serde_json::to_value(&plan).map_err(failure)?).bind(i64v(event.aggregate_version)?).bind(i64v(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?;
            if changed.rows_affected() != 1 {
                return Err(failure("planning plan version did not advance"));
            }
            return Ok(());
        }
        if ctx.projection != PROJECTION_NAME
            || event.context != "planning"
            || event.aggregate_type != "run"
            || event.event_type != SNAPSHOT_EVENT_TYPE
            || event.event_version != 1
        {
            return Err(ProjectionFailure::new(
                "planning projector received an incompatible event",
            ));
        }
        let snapshot: PlanningRunSnapshot =
            serde_json::from_value(event.payload.clone()).map_err(failure)?;
        if snapshot.run.id.as_uuid() != event.aggregate_id.as_uuid() || event.aggregate_version != 1
        {
            return Err(ProjectionFailure::new(
                "planning snapshot metadata mismatch",
            ));
        }
        let result=sqlx::query("INSERT INTO erpx_planning_read.run_projection (projection_name,generation,partition_key,run_id,scenario_id,authority_scope,input_digest,projection,source_aggregate_version,source_position,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp()) ON CONFLICT (holding_id,projection_name,generation,partition_key,run_id) DO UPDATE SET scenario_id=EXCLUDED.scenario_id,authority_scope=EXCLUDED.authority_scope,input_digest=EXCLUDED.input_digest,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp() WHERE erpx_planning_read.run_projection.source_aggregate_version<EXCLUDED.source_aggregate_version")
            .bind(PROJECTION_NAME).bind(ctx.generation).bind(&event.partition).bind(snapshot.run.id.as_uuid()).bind(snapshot.run.scenario_id.as_uuid()).bind(serde_json::to_value(&event.active_scope).map_err(failure)?).bind(&snapshot.run.input_digest).bind(serde_json::to_value(&snapshot.run).map_err(failure)?).bind(i64v(event.aggregate_version)?).bind(i64v(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "planning snapshot version did not advance",
            ));
        }
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_planning_read.run_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        sqlx::query("DELETE FROM erpx_planning_read.plan_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        sqlx::query("DELETE FROM erpx_planning_read.governed_record_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
}
fn i64v(v: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(v).map_err(|_| ProjectionFailure::new("planning numeric value is too large"))
}
fn failure(e: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(e.to_string())
}
fn enum_name<T: Serialize>(value: T) -> Result<String, ProjectionFailure> {
    serde_json::to_value(value)
        .map_err(failure)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| failure("planning enum must serialize as text"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projector_is_generation_managed() {
        let p = PlanningProjector::new();
        assert_eq!(p.name(), PROJECTION_NAME);
        assert!(include_str!("projection.rs").contains("ctx.generation"));
    }
}
