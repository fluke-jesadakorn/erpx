use async_trait::async_trait;
use erpx_assets::{AssetStatus, AssetView};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, assets};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    asset_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSnapshot {
    pub asset: AssetView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssetOutboxPayload {
    Integration(assets::Event),
    Snapshot(AssetSnapshot),
    Directory(ItemDescriptorDraft),
}

#[derive(Debug, Clone)]
pub struct AssetProjector {
    event_types: Vec<String>,
}

impl AssetProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for AssetProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for AssetProjector {
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
        let view = snapshot.asset;
        let state = serde_json::to_value(&view).map_err(projection_failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_assets_read.asset_projection
                (projection_name,generation,partition_key,asset_id,
                 source_aggregate_version,source_global_sequence,active_scope,
                 scope_key,asset_code,asset_name,location_id,asset_status,
                 asset_condition,state,aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,asset_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                source_global_sequence=EXCLUDED.source_global_sequence,
                active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                asset_code=EXCLUDED.asset_code,asset_name=EXCLUDED.asset_name,
                location_id=EXCLUDED.location_id,asset_status=EXCLUDED.asset_status,
                asset_condition=EXCLUDED.asset_condition,state=EXCLUDED.state,
                aggregate_created_at=EXCLUDED.aggregate_created_at,
                aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_assets_read.asset_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.asset_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(view.scope.active_scope()).map_err(projection_failure)?)
        .bind(scope_key(view.scope.active_scope())?)
        .bind(&view.code)
        .bind(&view.name)
        .bind(view.location_id)
        .bind(status_name(view.status))
        .bind(condition_name(view.condition))
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "asset snapshot aggregate version did not advance",
            ));
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_assets_read.asset_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<AssetSnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Assets projector received an incompatible event contract",
        ));
    }
    let payload: AssetOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let AssetOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Assets snapshot event carried an integration payload",
        ));
    };
    let view = &snapshot.asset;
    if view.version != event.aggregate_version
        || view.asset_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope.active_scope() != &event.active_scope
        || event.partition != asset_partition(view.asset_id)
        || scope_key(view.scope.active_scope()).is_err()
    {
        return Err(ProjectionFailure::new(
            "Assets snapshot metadata does not match its envelope",
        ));
    }
    Ok(snapshot)
}

pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, ProjectionFailure> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("holding:{holding_id}/company:{company_id}")),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!(
            "holding:{holding_id}/company:{company_id}/branch:{branch_id}"
        )),
        _ => Err(ProjectionFailure::new(
            "Assets snapshot uses a non-exact scope",
        )),
    }
}

pub(crate) const fn status_name(status: AssetStatus) -> &'static str {
    match status {
        AssetStatus::Draft => "draft",
        AssetStatus::InService => "in_service",
        AssetStatus::UnderMaintenance => "under_maintenance",
        AssetStatus::Retired => "retired",
    }
}

pub(crate) const fn condition_name(condition: assets::AssetCondition) -> &'static str {
    match condition {
        assets::AssetCondition::Operational => "operational",
        assets::AssetCondition::Degraded => "degraded",
        assets::AssetCondition::OutOfService => "out_of_service",
        assets::AssetCondition::Retired => "retired",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("Assets numeric value is too large"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use erpx_cqrs_sqlx::Projector;

    use super::*;

    #[test]
    fn projector_consumes_private_snapshots_only() {
        let projector = AssetProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
    }

    #[test]
    fn scope_key_is_exact_and_stable() {
        let scope = AuthorityScope::Company {
            holding_id: uuid::Uuid::from_u128(1),
            company_id: uuid::Uuid::from_u128(2),
        };
        assert_eq!(
            scope_key(&scope).unwrap(),
            format!(
                "holding:{}/company:{}",
                uuid::Uuid::from_u128(1),
                uuid::Uuid::from_u128(2)
            )
        );
    }
}
