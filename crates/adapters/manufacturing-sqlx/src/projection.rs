use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_manufacturing::{ManufacturingRecord, ManufacturingSpecification, ProductionOrder};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    EVENT_VERSION, PROJECTION_NAME, RECORD_SNAPSHOT_EVENT_TYPE, SNAPSHOT_EVENT_TYPE,
    SPECIFICATION_SNAPSHOT_EVENT_TYPE,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingOrderSnapshot {
    pub order: ProductionOrder,
}

#[derive(Debug, Clone)]
pub struct ManufacturingProjector {
    event_types: Vec<String>,
}
impl ManufacturingProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![
                SNAPSHOT_EVENT_TYPE.into(),
                RECORD_SNAPSHOT_EVENT_TYPE.into(),
                SPECIFICATION_SNAPSHOT_EVENT_TYPE.into(),
            ],
        }
    }
}
impl Default for ManufacturingProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for ManufacturingProjector {
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
        if event.event_type == SPECIFICATION_SNAPSHOT_EVENT_TYPE {
            if ctx.projection != PROJECTION_NAME
                || event.context != "manufacturing"
                || event.aggregate_type != "specification"
                || event.event_version != EVENT_VERSION
            {
                return Err(failure(
                    "manufacturing specification projector received incompatible event",
                ));
            }
            let specification: ManufacturingSpecification = serde_json::from_value(
                event
                    .payload
                    .get("specification")
                    .cloned()
                    .ok_or_else(|| failure("manufacturing specification payload missing"))?,
            )
            .map_err(failure)?;
            if specification.reference().specification_id.as_uuid() != event.aggregate_id.as_uuid()
                || specification.reference().version != event.aggregate_version
                || specification.scope() != &event.tenant_scope
            {
                return Err(failure("manufacturing specification metadata mismatch"));
            }
            let state = serde_json::to_value(specification.status())
                .map_err(failure)?
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| failure("manufacturing specification status missing"))?;
            let changed = sqlx::query(
                "INSERT INTO erpx_manufacturing_read.specification_projection(projection_name,generation,partition_key,specification_id,authority_scope,lifecycle_state,projection,source_aggregate_version,source_position,projected_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,clock_timestamp()) ON CONFLICT(tenant_id,projection_name,generation,partition_key,specification_id) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,lifecycle_state=EXCLUDED.lifecycle_state,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp() WHERE erpx_manufacturing_read.specification_projection.source_aggregate_version<EXCLUDED.source_aggregate_version",
            )
            .bind(PROJECTION_NAME)
            .bind(ctx.generation)
            .bind(&event.partition)
            .bind(specification.reference().specification_id.as_uuid())
            .bind(serde_json::to_value(specification.scope()).map_err(failure)?)
            .bind(state)
            .bind(serde_json::to_value(&specification).map_err(failure)?)
            .bind(to_i64(event.aggregate_version)?)
            .bind(to_i64(event.global_sequence)?)
            .execute(&mut **tx)
            .await
            .map_err(failure)?;
            if changed.rows_affected() != 1 {
                return Err(failure(
                    "manufacturing specification version did not advance",
                ));
            }
            return Ok(());
        }
        if event.event_type == RECORD_SNAPSHOT_EVENT_TYPE {
            if ctx.projection != PROJECTION_NAME
                || event.context != "manufacturing"
                || event.event_version != EVENT_VERSION
            {
                return Err(failure(
                    "manufacturing record projector received incompatible event",
                ));
            }
            let record: ManufacturingRecord = serde_json::from_value(
                event
                    .payload
                    .get("record")
                    .cloned()
                    .ok_or_else(|| failure("manufacturing record payload missing"))?,
            )
            .map_err(failure)?;
            if record.id() != event.aggregate_id.as_uuid()
                || record.version() != event.aggregate_version
                || record.scope() != &event.tenant_scope
                || record.kind().token() != event.aggregate_type
            {
                return Err(failure("manufacturing record metadata mismatch"));
            }
            let state = serde_json::to_value(record.status())
                .map_err(failure)?
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| failure("manufacturing record status missing"))?;
            let changed=sqlx::query("INSERT INTO erpx_manufacturing_read.governed_record_projection(projection_name,generation,partition_key,record_id,record_kind,authority_scope,lifecycle_state,projection,source_aggregate_version,source_position,projected_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp())ON CONFLICT(tenant_id,projection_name,generation,partition_key,record_id)DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp()WHERE erpx_manufacturing_read.governed_record_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(PROJECTION_NAME).bind(ctx.generation).bind(&event.partition).bind(record.id()).bind(record.kind().token()).bind(serde_json::to_value(record.scope()).map_err(failure)?).bind(state).bind(serde_json::to_value(&record).map_err(failure)?).bind(to_i64(event.aggregate_version)?).bind(to_i64(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?;
            if changed.rows_affected() != 1 {
                return Err(failure("manufacturing record version did not advance"));
            }
            return Ok(());
        }
        if ctx.projection != PROJECTION_NAME
            || event.context != "manufacturing"
            || event.aggregate_type != "order"
            || event.event_type != SNAPSHOT_EVENT_TYPE
            || event.event_version != EVENT_VERSION
        {
            return Err(ProjectionFailure::new(
                "manufacturing projector received an incompatible event",
            ));
        }
        let snapshot: ManufacturingOrderSnapshot =
            serde_json::from_value(event.payload.clone()).map_err(failure)?;
        let document = serde_json::to_value(&snapshot.order).map_err(failure)?;
        if snapshot.order.id().as_uuid() != event.aggregate_id.as_uuid()
            || snapshot.order.version() != event.aggregate_version
            || snapshot.order.scope() != &event.tenant_scope
        {
            return Err(ProjectionFailure::new(
                "manufacturing snapshot metadata mismatch",
            ));
        }
        let status = document
            .get("status")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| ProjectionFailure::new("manufacturing status is missing"))?;
        let changed=sqlx::query("INSERT INTO erpx_manufacturing_read.order_projection (projection_name,generation,partition_key,order_id,authority_scope,lifecycle_state,projection,source_aggregate_version,source_position,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,clock_timestamp()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,order_id) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,lifecycle_state=EXCLUDED.lifecycle_state,projection=EXCLUDED.projection,source_aggregate_version=EXCLUDED.source_aggregate_version,source_position=EXCLUDED.source_position,projected_at=clock_timestamp() WHERE erpx_manufacturing_read.order_projection.source_aggregate_version<EXCLUDED.source_aggregate_version")
   .bind(PROJECTION_NAME).bind(ctx.generation).bind(&event.partition).bind(snapshot.order.id().as_uuid()).bind(serde_json::to_value(snapshot.order.scope()).map_err(failure)?).bind(status).bind(document).bind(to_i64(event.aggregate_version)?).bind(to_i64(event.global_sequence)?).execute(&mut **tx).await.map_err(failure)?;
        if changed.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "manufacturing snapshot version did not advance",
            ));
        }
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        ctx: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_manufacturing_read.order_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        sqlx::query("DELETE FROM erpx_manufacturing_read.governed_record_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        sqlx::query("DELETE FROM erpx_manufacturing_read.specification_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(ctx.generation).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
}
fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("manufacturing numeric value is too large"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projector_is_generation_managed() {
        let p = ManufacturingProjector::new();
        assert_eq!(p.name(), PROJECTION_NAME);
        assert_eq!(
            p.event_types(),
            &[
                SNAPSHOT_EVENT_TYPE.to_owned(),
                RECORD_SNAPSHOT_EVENT_TYPE.to_owned(),
                SPECIFICATION_SNAPSHOT_EVENT_TYPE.to_owned()
            ]
        );
        let source = include_str!("projection.rs");
        assert!(source.contains("ctx.generation"));
        assert!(source.contains("source_aggregate_version<EXCLUDED.source_aggregate_version"));
    }
}
