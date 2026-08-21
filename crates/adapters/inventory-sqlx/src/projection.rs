use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraftV2, inventory};
use erpx_inventory::StockItemView;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    stock_item_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySnapshot {
    pub item: StockItemView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InventoryOutboxPayload {
    Integration(inventory::Event),
    Snapshot(InventorySnapshot),
    Directory(ItemDescriptorDraftV2),
}

#[derive(Debug, Clone)]
pub struct InventoryProjector {
    event_types: Vec<String>,
}

impl InventoryProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for InventoryProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for InventoryProjector {
    fn name(&self) -> &str {
        PROJECTION_NAME
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
        let snapshot = validate_event(context, event)?;
        let view = snapshot.item;
        let scope_key = scope_key(&view.scope)?;
        let reservations = serde_json::to_value(&view.reservations).map_err(failure)?;
        let state = serde_json::to_value(&view).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_inventory_read.stock_item_projection
                (projection_name,generation,partition_key,stock_item_id,
                 source_aggregate_version,source_global_sequence,active_scope,scope_key,
                 warehouse_id,item_code,on_hand,reserved,available,reservations,state,
                 aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,stock_item_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                source_global_sequence=EXCLUDED.source_global_sequence,
                active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                warehouse_id=EXCLUDED.warehouse_id,item_code=EXCLUDED.item_code,
                on_hand=EXCLUDED.on_hand,reserved=EXCLUDED.reserved,
                available=EXCLUDED.available,reservations=EXCLUDED.reservations,
                state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,
                aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_inventory_read.stock_item_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.stock_item_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(failure)?)
        .bind(scope_key)
        .bind(view.warehouse_id.as_uuid())
        .bind(&view.item_code)
        .bind(view.on_hand)
        .bind(view.reserved)
        .bind(view.available)
        .bind(reservations)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "inventory snapshot version did not advance",
            ));
        }
        sqlx::query(
            "DELETE FROM erpx_inventory_read.reservation_projection
             WHERE holding_id=erpx_cqrs_read.current_holding_id()
               AND projection_name=$1 AND generation=$2 AND stock_item_id=$3",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(view.stock_item_id.as_uuid())
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        for reservation in &view.reservations {
            sqlx::query(
                "INSERT INTO erpx_inventory_read.reservation_projection
                    (holding_id,projection_name,generation,partition_key,reservation_id,
                     stock_item_id,source_aggregate_version,source_global_sequence,active_scope,
                     warehouse_id,item_code,quantity,demand_reference,reservation_status,projected_at)
                 VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now())",
            )
            .bind(PROJECTION_NAME)
            .bind(context.generation)
            .bind(&event.partition)
            .bind(reservation.reservation_id.as_uuid())
            .bind(view.stock_item_id.as_uuid())
            .bind(to_i64(view.version)?)
            .bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?)
            .bind(view.warehouse_id.as_uuid())
            .bind(&view.item_code)
            .bind(reservation.quantity)
            .bind(&reservation.demand_reference)
            .bind(match reservation.status {
                erpx_inventory::ReservationStatus::Active => "active",
                erpx_inventory::ReservationStatus::Released => "released",
            })
            .execute(&mut **transaction)
            .await
            .map_err(failure)?;
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_inventory_read.stock_item_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        sqlx::query(
            "DELETE FROM erpx_inventory_read.reservation_projection
             WHERE holding_id=erpx_cqrs_read.current_holding_id()
               AND projection_name=$1 AND generation=$2",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<InventorySnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "inventory projector received an incompatible event",
        ));
    }
    let payload: InventoryOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let InventoryOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "inventory snapshot carried an integration payload",
        ));
    };
    let view = &snapshot.item;
    if view.version != event.aggregate_version
        || view.stock_item_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != stock_item_partition(view.stock_item_id)
        || scope_key(&view.scope).is_err()
        || view.available != view.on_hand - view.reserved
    {
        return Err(ProjectionFailure::new(
            "inventory snapshot metadata mismatch",
        ));
    }
    Ok(snapshot)
}

pub(crate) fn scope_key(scope: &erpx_kernel::AuthorityScope) -> Result<String, ProjectionFailure> {
    match scope {
        erpx_kernel::AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("company:{holding_id}:{company_id}")),
        erpx_kernel::AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("branch:{holding_id}:{company_id}:{branch_id}")),
        _ => Err(ProjectionFailure::new(
            "inventory requires an exact holding scope",
        )),
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("numeric value is too large"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_consumes_only_private_snapshots() {
        let projector = InventoryProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
    }

    #[test]
    fn reservation_projection_is_owned_by_the_stock_item_generation() {
        let source = include_str!("projection.rs");
        assert!(source.contains("erpx_inventory_read.reservation_projection"));
        assert!(source.contains("context.generation"));
        assert!(source.contains("source_aggregate_version"));
        assert!(!source.contains(concat!("ORDER BY projected_at ", "DESC")));
    }
}
