use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::platform_administration;
use erpx_kernel::TenantScope;
use erpx_platform_administration::{
    AdminContextStatus, AdminContextView, ConnectorKind, ConnectorState, ConnectorView,
    TenantLifecycleState, TenantView,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    ADMIN_CONTEXT_AGGREGATE, ADMIN_CONTEXT_PROJECTION, ADMIN_CONTEXT_SNAPSHOT_EVENT,
    CONNECTOR_AGGREGATE, CONNECTOR_PROJECTION, CONNECTOR_SNAPSHOT_EVENT, CONTEXT_NAME,
    EVENT_VERSION, TENANT_AGGREGATE, TENANT_PROJECTION, TENANT_SNAPSHOT_EVENT,
    admin_context_partition, connector_partition, tenant_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate_type", content = "state", rename_all = "snake_case")]
pub enum PlatformAdministrationSnapshot {
    Tenant(TenantView),
    Connector(ConnectorView),
    AdminContext(AdminContextView),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlatformAdministrationOutboxPayload {
    Integration(platform_administration::Event),
    Snapshot(PlatformAdministrationSnapshot),
}

macro_rules! projector {
    ($name:ident, $projection:expr, $event:expr, $aggregate:expr, $variant:ident, $apply:ident, $reset:ident) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            event_types: Vec<String>,
        }
        impl $name {
            pub fn new() -> Self {
                Self {
                    event_types: vec![$event.into()],
                }
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        #[async_trait]
        impl Projector for $name {
            fn name(&self) -> &str {
                $projection
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
                validate_envelope(context, event, $projection, $event, $aggregate)?;
                let payload: PlatformAdministrationOutboxPayload =
                    serde_json::from_value(event.payload.clone()).map_err(failure)?;
                let PlatformAdministrationOutboxPayload::Snapshot(
                    PlatformAdministrationSnapshot::$variant(view),
                ) = payload
                else {
                    return Err(ProjectionFailure::new(
                        "platform administration snapshot variant mismatch",
                    ));
                };
                $apply(transaction, context, event, view).await
            }
            async fn reset(
                &self,
                transaction: &mut Transaction<'_, Postgres>,
                context: &ProjectorContext<'_>,
            ) -> Result<(), ProjectionFailure> {
                $reset(transaction, context).await
            }
        }
    };
}

projector!(
    TenantProjector,
    TENANT_PROJECTION,
    TENANT_SNAPSHOT_EVENT,
    TENANT_AGGREGATE,
    Tenant,
    apply_tenant,
    reset_tenant
);
projector!(
    ConnectorProjector,
    CONNECTOR_PROJECTION,
    CONNECTOR_SNAPSHOT_EVENT,
    CONNECTOR_AGGREGATE,
    Connector,
    apply_connector,
    reset_connector
);
projector!(
    AdminContextProjector,
    ADMIN_CONTEXT_PROJECTION,
    ADMIN_CONTEXT_SNAPSHOT_EVENT,
    ADMIN_CONTEXT_AGGREGATE,
    AdminContext,
    apply_admin_context,
    reset_admin_context
);

async fn apply_tenant(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    view: TenantView,
) -> Result<(), ProjectionFailure> {
    if view.version != event.aggregate_version
        || view.tenant_id.as_uuid() != event.aggregate_id.as_uuid()
        || event.partition != tenant_partition(view.tenant_id)
        || event.tenant_scope
            != (TenantScope::Holding {
                holding_id: view.root_holding_id,
            })
    {
        return Err(ProjectionFailure::new(
            "platform tenant snapshot metadata mismatch",
        ));
    }
    let state = serde_json::to_value(&view).map_err(failure)?;
    let result=sqlx::query("INSERT INTO erpx_platform_administration_read.tenant_projection (projection_name,generation,partition_key,tenant_id,source_aggregate_version,source_global_sequence,tenant_code,tenant_name,environment_id,customer_id,root_holding_id,holding_ref,lifecycle_state,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now()) ON CONFLICT (projection_name,generation,partition_key,tenant_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_code=EXCLUDED.tenant_code,tenant_name=EXCLUDED.tenant_name,environment_id=EXCLUDED.environment_id,customer_id=EXCLUDED.customer_id,root_holding_id=EXCLUDED.root_holding_id,holding_ref=EXCLUDED.holding_ref,lifecycle_state=EXCLUDED.lifecycle_state,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_platform_administration_read.tenant_projection.source_aggregate_version<EXCLUDED.source_aggregate_version")
        .bind(TENANT_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.tenant_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(&view.code).bind(&view.name).bind(view.environment_id.as_uuid()).bind(view.customer_id).bind(view.root_holding_id).bind(&view.holding_ref).bind(tenant_state(view.status)).bind(state).bind(view.created_at).bind(view.updated_at).execute(&mut **transaction).await.map_err(failure)?;
    ensure_advanced(result.rows_affected(), "tenant")
}

async fn apply_connector(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    view: ConnectorView,
) -> Result<(), ProjectionFailure> {
    let owned_scope = view.scope.tenant_scope();
    if view.version != event.aggregate_version
        || view.connector_id.as_uuid() != event.aggregate_id.as_uuid()
        || event.partition != connector_partition(view.connector_id)
        || event.tenant_scope != owned_scope
    {
        return Err(ProjectionFailure::new(
            "platform connector snapshot metadata mismatch",
        ));
    }
    let state = serde_json::to_value(&view).map_err(failure)?;
    let result=sqlx::query("INSERT INTO erpx_platform_administration_read.connector_projection (projection_name,generation,partition_key,connector_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,connector_kind,connector_key,configuration_digest,capabilities,connector_state,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now()) ON CONFLICT (projection_name,generation,partition_key,connector_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,connector_kind=EXCLUDED.connector_kind,connector_key=EXCLUDED.connector_key,configuration_digest=EXCLUDED.configuration_digest,capabilities=EXCLUDED.capabilities,connector_state=EXCLUDED.connector_state,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_platform_administration_read.connector_projection.source_aggregate_version<EXCLUDED.source_aggregate_version")
        .bind(CONNECTOR_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.connector_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(serde_json::to_value(&owned_scope).map_err(failure)?).bind(scope_key(&owned_scope)?).bind(connector_kind(&view.kind)).bind(&view.key).bind(&view.configuration_digest).bind(serde_json::to_value(&view.capabilities).map_err(failure)?).bind(connector_state(view.state)).bind(state).bind(view.created_at).bind(view.updated_at).execute(&mut **transaction).await.map_err(failure)?;
    ensure_advanced(result.rows_affected(), "connector")
}

async fn apply_admin_context(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    view: AdminContextView,
) -> Result<(), ProjectionFailure> {
    if view.version != event.aggregate_version
        || view.context_id.as_uuid() != event.aggregate_id.as_uuid()
        || event.partition != admin_context_partition(view.context_id)
        || event.tenant_scope != view.scope
    {
        return Err(ProjectionFailure::new(
            "platform admin context snapshot metadata mismatch",
        ));
    }
    let state = serde_json::to_value(&view).map_err(failure)?;
    let result=sqlx::query("INSERT INTO erpx_platform_administration_read.admin_context_projection (projection_name,generation,partition_key,context_id,source_aggregate_version,source_global_sequence,tenant_id,tenant_scope,scope_key,principal_ref,reason,issued_at,expires_at,revoked_at,context_status,state,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now()) ON CONFLICT (projection_name,generation,partition_key,context_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_id=EXCLUDED.tenant_id,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,principal_ref=EXCLUDED.principal_ref,reason=EXCLUDED.reason,issued_at=EXCLUDED.issued_at,expires_at=EXCLUDED.expires_at,revoked_at=EXCLUDED.revoked_at,context_status=EXCLUDED.context_status,state=EXCLUDED.state,projected_at=now() WHERE erpx_platform_administration_read.admin_context_projection.source_aggregate_version<EXCLUDED.source_aggregate_version")
        .bind(ADMIN_CONTEXT_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.context_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(view.tenant_id.as_uuid()).bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope)?).bind(&view.principal_ref).bind(&view.reason).bind(view.issued_at).bind(view.expires_at).bind(view.revoked_at).bind(admin_status(view.status)).bind(state).execute(&mut **transaction).await.map_err(failure)?;
    ensure_advanced(result.rows_affected(), "admin context")
}

macro_rules! reset_fn {
    ($name:ident,$table:literal,$projection:expr) => {
        async fn $name(
            transaction: &mut Transaction<'_, Postgres>,
            context: &ProjectorContext<'_>,
        ) -> Result<(), ProjectionFailure> {
            sqlx::query(concat!(
                "DELETE FROM ",
                $table,
                " WHERE projection_name=$1 AND generation=$2"
            ))
            .bind($projection)
            .bind(context.generation)
            .execute(&mut **transaction)
            .await
            .map_err(failure)?;
            Ok(())
        }
    };
}
reset_fn!(
    reset_tenant,
    "erpx_platform_administration_read.tenant_projection",
    TENANT_PROJECTION
);
reset_fn!(
    reset_connector,
    "erpx_platform_administration_read.connector_projection",
    CONNECTOR_PROJECTION
);
reset_fn!(
    reset_admin_context,
    "erpx_platform_administration_read.admin_context_projection",
    ADMIN_CONTEXT_PROJECTION
);

fn validate_envelope(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    event_type: &str,
    aggregate: &str,
) -> Result<(), ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        Err(ProjectionFailure::new(
            "platform administration projector received incompatible event",
        ))
    } else {
        Ok(())
    }
}
fn ensure_advanced(rows: u64, kind: &str) -> Result<(), ProjectionFailure> {
    if rows == 1 {
        Ok(())
    } else {
        Err(ProjectionFailure::new(format!(
            "platform {kind} snapshot version did not advance"
        )))
    }
}
pub(crate) fn scope_key(scope: &TenantScope) -> Result<String, ProjectionFailure> {
    scope
        .validate()
        .map_err(|_| ProjectionFailure::new("invalid platform administration scope"))?;
    Ok(match scope {
        TenantScope::Platform => "platform".into(),
        TenantScope::Holding { holding_id } => format!("h:{holding_id}"),
        TenantScope::Company {
            holding_id,
            company_id,
        } => format!("h:{holding_id}/c:{company_id}"),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("h:{holding_id}/c:{company_id}/b:{branch_id}"),
        TenantScope::Subtree { .. } => {
            return Err(ProjectionFailure::new(
                "subtree cannot own a platform record",
            ));
        }
    })
}
pub(crate) const fn tenant_state(value: TenantLifecycleState) -> &'static str {
    match value {
        TenantLifecycleState::Provisioning => "provisioning",
        TenantLifecycleState::Active => "active",
        TenantLifecycleState::Suspended => "suspended",
        TenantLifecycleState::Recovery => "recovery",
        TenantLifecycleState::Retired => "retired",
    }
}
pub(crate) fn connector_kind(value: &ConnectorKind) -> &str {
    match value {
        ConnectorKind::Stripe => "stripe",
        ConnectorKind::Minio => "minio",
        ConnectorKind::N8n => "n8n",
        ConnectorKind::Ollama => "ollama",
        ConnectorKind::Other(value) => value,
    }
}
pub(crate) const fn connector_state(value: ConnectorState) -> &'static str {
    match value {
        ConnectorState::Pending => "pending",
        ConnectorState::Active => "active",
        ConnectorState::Suspended => "suspended",
        ConnectorState::Retired => "retired",
    }
}
pub(crate) const fn admin_status(value: AdminContextStatus) -> &'static str {
    match value {
        AdminContextStatus::Active => "active",
        AdminContextStatus::Revoked => "revoked",
        AdminContextStatus::Expired => "expired",
    }
}
fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("platform administration numeric value is too large"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projectors_consume_only_private_snapshot_events() {
        assert_eq!(
            TenantProjector::new().event_types(),
            &[TENANT_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            ConnectorProjector::new().event_types(),
            &[CONNECTOR_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            AdminContextProjector::new().event_types(),
            &[ADMIN_CONTEXT_SNAPSHOT_EVENT.to_owned()]
        );
    }
    #[test]
    fn frozen_names_include_recovery() {
        assert_eq!(tenant_state(TenantLifecycleState::Recovery), "recovery");
        assert_eq!(connector_state(ConnectorState::Retired), "retired");
    }
}
