use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{BoundedContext, operations};
use erpx_operations::{
    ExceptionStatus, ExceptionView, OperationsException, SourcePosition, WorkQueuePolicy,
    WorkQueuePolicyStatus, WorkQueuePolicyView,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CONTEXT_NAME, EVENT_VERSION, EXCEPTION_AGGREGATE_TYPE, EXCEPTION_PROJECTION,
    EXCEPTION_SNAPSHOT_EVENT, POLICY_AGGREGATE_TYPE, POLICY_PROJECTION, POLICY_SNAPSHOT_EVENT,
    SOURCE_SIGNAL_PROJECTION, command::scope_key, exception_partition, policy_partition,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "aggregate", rename_all = "snake_case")]
// This exported snapshot is also the serialized outbox contract. Boxing its exception view
// would change the adapter API even though the persisted JSON representation is unchanged.
#[allow(clippy::large_enum_variant)]
pub enum OperationsSnapshot {
    Exception {
        view: ExceptionView,
        source_fact: operations::SourceFact,
        source_event_version: u16,
        source_position: Option<SourcePosition>,
    },
    WorkQueuePolicy {
        view: WorkQueuePolicyView,
    },
}

impl OperationsSnapshot {
    pub fn exception(
        value: &OperationsException,
        source_event_version: u16,
        source_position: Option<SourcePosition>,
    ) -> Self {
        Self::Exception {
            view: value.into(),
            source_fact: value.source_fact().clone(),
            source_event_version,
            source_position,
        }
    }

    pub fn policy(value: &WorkQueuePolicy) -> Self {
        Self::WorkQueuePolicy { view: value.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
// Producers consume this exported payload type directly; preserve its public variant shape
// instead of forcing allocation through every command and source-event producer.
#[allow(clippy::large_enum_variant)]
pub enum OperationsOutboxPayload {
    Integration(operations::Event),
    Snapshot(OperationsSnapshot),
}

macro_rules! projector_type {
    ($name:ident,$projection:expr,$events:expr) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            events: Vec<String>,
        }
        impl $name {
            pub fn new() -> Self {
                Self {
                    events: $events.into_iter().map(str::to_owned).collect(),
                }
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl $name {
            fn projection_name(&self) -> &'static str {
                $projection
            }
            fn subscribed_events(&self) -> &[String] {
                &self.events
            }
        }
    };
}

projector_type!(
    OperationsExceptionProjector,
    EXCEPTION_PROJECTION,
    [EXCEPTION_SNAPSHOT_EVENT]
);
projector_type!(
    OperationsSourceSignalProjector,
    SOURCE_SIGNAL_PROJECTION,
    [EXCEPTION_SNAPSHOT_EVENT]
);
projector_type!(
    OperationsWorkQueuePolicyProjector,
    POLICY_PROJECTION,
    [POLICY_SNAPSHOT_EVENT]
);

#[async_trait]
impl Projector for OperationsExceptionProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let OperationsSnapshot::Exception {
            view,
            source_event_version,
            source_position,
            ..
        } = validate_snapshot(
            context,
            event,
            EXCEPTION_PROJECTION,
            EXCEPTION_AGGREGATE_TYPE,
            EXCEPTION_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "Operations exception projector received policy snapshot",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        let source_sequence = source_position
            .as_ref()
            .map_or(event.global_sequence, |v| v.sequence);
        sqlx::query("INSERT INTO erpx_operations_read.exception_projection (projection_name,generation,partition_key,exception_id,aggregate_version,reference,tenant_scope,scope_key,source_context,source_event_id,source_event_type,source_aggregate_type,source_aggregate_id,source_aggregate_version,source_global_sequence,source_correlation_id,policy_ref,category,severity,assignee_id,manager_id,due_at,escalation_after_minutes,exception_status,state,created_at,updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,now()) ON CONFLICT (projection_name,generation,partition_key,exception_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,reference=EXCLUDED.reference,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,policy_ref=EXCLUDED.policy_ref,category=EXCLUDED.category,severity=EXCLUDED.severity,assignee_id=EXCLUDED.assignee_id,manager_id=EXCLUDED.manager_id,due_at=EXCLUDED.due_at,escalation_after_minutes=EXCLUDED.escalation_after_minutes,exception_status=EXCLUDED.exception_status,state=EXCLUDED.state,updated_at=EXCLUDED.updated_at,projected_at=now() WHERE erpx_operations_read.exception_projection.aggregate_version < EXCLUDED.aggregate_version")
            .bind(EXCEPTION_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.exception_id.as_uuid()).bind(to_i64(view.version)?).bind(&view.reference)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(source_context_name(view.source.context))
            .bind(view.source.event_id.as_uuid()).bind(&view.source.event_type).bind(&view.source.aggregate_kind).bind(view.source.aggregate_id.as_uuid()).bind(to_i64(view.source.aggregate_version)?)
            .bind(to_i64(source_sequence)?).bind(view.source.correlation_id.as_uuid()).bind(&view.policy_ref).bind(&view.category).bind(&view.severity)
            .bind(view.assignee.as_uuid().to_string()).bind(view.manager.as_uuid().to_string()).bind(view.due_at).bind(i32::try_from(view.escalation_after_minutes).map_err(|_| ProjectionFailure::new("numeric value is too large"))?)
            .bind(exception_status_name(view.status)).bind(state).bind(view.created_at).bind(view.updated_at).execute(&mut **tx).await.map_err(failure)?;
        let _ = source_event_version;
        Ok(())
    }

    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            tx,
            context,
            "erpx_operations_read.exception_projection",
            EXCEPTION_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for OperationsSourceSignalProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let OperationsSnapshot::Exception {
            view,
            source_fact,
            source_event_version,
            source_position,
        } = validate_snapshot(
            context,
            event,
            SOURCE_SIGNAL_PROJECTION,
            EXCEPTION_AGGREGATE_TYPE,
            EXCEPTION_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "Operations source-signal projector received policy snapshot",
            ));
        };
        let source_sequence = source_position
            .as_ref()
            .map_or(event.global_sequence, |v| v.sequence);
        sqlx::query("INSERT INTO erpx_operations_read.source_signal_projection (projection_name,generation,partition_key,process_id,source_context,source_event_id,source_event_type,source_event_version,source_aggregate_type,source_aggregate_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,process_code,operational_state,source_fact,observed_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,now()) ON CONFLICT (projection_name,generation,partition_key,process_id) DO UPDATE SET process_code=EXCLUDED.process_code,operational_state=EXCLUDED.operational_state,source_fact=EXCLUDED.source_fact,observed_at=EXCLUDED.observed_at,projected_at=now()")
            .bind(SOURCE_SIGNAL_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.exception_id.as_uuid()).bind(source_context_name(view.source.context))
            .bind(view.source.event_id.as_uuid()).bind(&view.source.event_type).bind(i32::from(source_event_version)).bind(&view.source.aggregate_kind).bind(view.source.aggregate_id.as_uuid())
            .bind(to_i64(view.source.aggregate_version)?).bind(to_i64(source_sequence)?).bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?)
            .bind(&view.reference).bind(exception_status_name(view.status)).bind(serde_json::to_value(source_fact).map_err(failure)?).bind(view.updated_at).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            tx,
            context,
            "erpx_operations_read.source_signal_projection",
            SOURCE_SIGNAL_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for OperationsWorkQueuePolicyProjector {
    fn name(&self) -> &str {
        self.projection_name()
    }
    fn event_types(&self) -> &[String] {
        self.subscribed_events()
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let OperationsSnapshot::WorkQueuePolicy { view } = validate_snapshot(
            context,
            event,
            POLICY_PROJECTION,
            POLICY_AGGREGATE_TYPE,
            POLICY_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "Operations policy projector received exception snapshot",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_operations_read.work_queue_policy_projection (projection_name,generation,partition_key,policy_id,aggregate_version,tenant_scope,scope_key,policy_name,allowed_source_contexts,escalation_after_minutes,policy_status,source_global_sequence,state,created_at,updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now()) ON CONFLICT (projection_name,generation,partition_key,policy_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,policy_name=EXCLUDED.policy_name,allowed_source_contexts=EXCLUDED.allowed_source_contexts,escalation_after_minutes=EXCLUDED.escalation_after_minutes,policy_status=EXCLUDED.policy_status,source_global_sequence=EXCLUDED.source_global_sequence,state=EXCLUDED.state,updated_at=EXCLUDED.updated_at,projected_at=now() WHERE erpx_operations_read.work_queue_policy_projection.aggregate_version < EXCLUDED.aggregate_version")
            .bind(POLICY_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.policy_id.as_uuid()).bind(to_i64(view.version)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(&view.name).bind(serde_json::to_value(&view.allowed_sources).map_err(failure)?)
            .bind(i32::try_from(view.escalation_after_minutes).map_err(|_| ProjectionFailure::new("numeric value is too large"))?).bind(policy_status_name(view.status)).bind(to_i64(event.global_sequence)?)
            .bind(state).bind(view.created_at).bind(view.updated_at).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        reset(
            tx,
            context,
            "erpx_operations_read.work_queue_policy_projection",
            POLICY_PROJECTION,
        )
        .await
    }
}

fn validate_snapshot(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<OperationsSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Operations projector received incompatible event",
        ));
    }
    let payload: OperationsOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let OperationsOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Operations snapshot event carried integration payload",
        ));
    };
    let matches = match &snapshot {
        OperationsSnapshot::Exception { view, .. } => {
            aggregate_type == EXCEPTION_AGGREGATE_TYPE
                && view.version == event.aggregate_version
                && view.exception_id.as_uuid() == event.aggregate_id.as_uuid()
                && view.scope == event.tenant_scope
                && event.partition == exception_partition(view.exception_id)
        }
        OperationsSnapshot::WorkQueuePolicy { view } => {
            aggregate_type == POLICY_AGGREGATE_TYPE
                && view.version == event.aggregate_version
                && view.policy_id.as_uuid() == event.aggregate_id.as_uuid()
                && view.scope == event.tenant_scope
                && event.partition == policy_partition(view.policy_id)
        }
    };
    if !matches {
        return Err(ProjectionFailure::new(
            "Operations snapshot metadata mismatch",
        ));
    }
    Ok(snapshot)
}

async fn reset(
    tx: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    table: &str,
    projection: &str,
) -> Result<(), ProjectionFailure> {
    let statement = format!("DELETE FROM {table} WHERE projection_name=$1 AND generation=$2");
    sqlx::query(&statement)
        .bind(projection)
        .bind(context.generation)
        .execute(&mut **tx)
        .await
        .map_err(failure)?;
    Ok(())
}

pub const fn exception_status_name(value: ExceptionStatus) -> &'static str {
    match value {
        ExceptionStatus::Assigned => "assigned",
        ExceptionStatus::Acknowledged => "acknowledged",
        ExceptionStatus::Escalated => "escalated",
        ExceptionStatus::Resolved => "resolved",
        ExceptionStatus::Closed => "closed",
    }
}

pub const fn policy_status_name(value: WorkQueuePolicyStatus) -> &'static str {
    match value {
        WorkQueuePolicyStatus::Active => "active",
        WorkQueuePolicyStatus::Paused => "paused",
        WorkQueuePolicyStatus::Retired => "retired",
    }
}

pub const fn source_context_name(value: BoundedContext) -> &'static str {
    match value {
        BoundedContext::IdentityAccess => "identity_access",
        BoundedContext::Organization => "organization",
        BoundedContext::Commercial => "commercial",
        BoundedContext::PlatformAdministration => "platform_administration",
        BoundedContext::Brand => "brand",
        BoundedContext::Customer => "customer",
        BoundedContext::Marketing => "marketing",
        BoundedContext::Crm => "crm",
        BoundedContext::Procurement => "procurement",
        BoundedContext::Inventory => "inventory",
        BoundedContext::Assets => "assets",
        BoundedContext::Maintenance => "maintenance",
        BoundedContext::Quality => "quality",
        BoundedContext::Sales => "sales",
        BoundedContext::PeoplePayroll => "people_payroll",
        BoundedContext::Pos => "pos",
        BoundedContext::Finance => "finance",
        BoundedContext::Legal => "legal",
        BoundedContext::Support => "support",
        BoundedContext::Operations => "operations",
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
    fn three_projectors_are_snapshot_specific_and_rebuildable() {
        assert_eq!(
            OperationsExceptionProjector::new().event_types(),
            &[EXCEPTION_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            OperationsSourceSignalProjector::new().event_types(),
            &[EXCEPTION_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            OperationsWorkQueuePolicyProjector::new().event_types(),
            &[POLICY_SNAPSHOT_EVENT.to_owned()]
        );
        let source = include_str!("projection.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert_eq!(production.matches("async fn reset(").count(), 4);
        assert!(source.contains("aggregate_version < EXCLUDED.aggregate_version"));
    }

    #[test]
    fn human_exception_updates_preserve_immutable_source_provenance() {
        let source = include_str!("projection.rs");
        let update = source
            .split("ON CONFLICT (projection_name,generation,partition_key,exception_id)")
            .nth(1)
            .unwrap();
        assert!(
            !update
                .split("WHERE")
                .next()
                .unwrap()
                .contains("source_event_id=EXCLUDED")
        );
    }
}
