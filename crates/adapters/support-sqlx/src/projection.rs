use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraftV2, support};
use erpx_support::{
    CapabilityRevocationReason, IncidentStatus, MessageAudience, SessionStatus, SupportAggregate,
    SupportCapabilityView, SupportMode, SupportSessionView, SupportTicketView, TicketReadAudience,
    TicketStatus, ports::IncidentView,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CAPABILITY_AGGREGATE_TYPE, CAPABILITY_PROJECTION, CAPABILITY_SNAPSHOT_EVENT, CONTEXT_NAME,
    EVENT_VERSION, INCIDENT_AGGREGATE_TYPE, INCIDENT_PROJECTION, INCIDENT_SNAPSHOT_EVENT,
    SESSION_AGGREGATE_TYPE, SESSION_PROJECTION, SESSION_SNAPSHOT_EVENT, TICKET_AGGREGATE_TYPE,
    TICKET_PROJECTION, TICKET_SNAPSHOT_EVENT, capability_partition, command::scope_key,
    incident_partition, session_partition, ticket_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum SupportSnapshot {
    Ticket(SupportTicketView),
    Incident(IncidentView),
    Session(SupportSessionView),
    Capability(SupportCapabilityView),
}

impl From<&SupportAggregate> for SupportSnapshot {
    fn from(value: &SupportAggregate) -> Self {
        match value {
            SupportAggregate::Ticket(value) => Self::Ticket(SupportTicketView::from_aggregate(
                value,
                TicketReadAudience::Internal,
            )),
            SupportAggregate::Incident(value) => Self::Incident(value.into()),
            SupportAggregate::Session(value) => Self::Session(value.into()),
            SupportAggregate::Capability(value) => Self::Capability(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
// This public adapter payload is serialized directly into the outbox. Boxing
// its snapshot variant would change the construction API without reducing the
// serialized record, so keep the stable boundary explicit.
#[allow(clippy::large_enum_variant)]
pub enum SupportOutboxPayload {
    Integration(support::Event),
    AuthorityEffect(erpx_support::SupportAuthorityEffect),
    Snapshot(SupportSnapshot),
    Directory(ItemDescriptorDraftV2),
}

macro_rules! projector_type {
    ($name:ident, $projection:expr, $event:expr) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            events: Vec<String>,
        }
        impl $name {
            pub fn new() -> Self {
                Self {
                    events: vec![$event.into()],
                }
            }
            fn projection_name(&self) -> &'static str {
                $projection
            }
            fn subscribed_events(&self) -> &[String] {
                &self.events
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

projector_type!(TicketProjector, TICKET_PROJECTION, TICKET_SNAPSHOT_EVENT);
projector_type!(
    IncidentProjector,
    INCIDENT_PROJECTION,
    INCIDENT_SNAPSHOT_EVENT
);
projector_type!(SessionProjector, SESSION_PROJECTION, SESSION_SNAPSHOT_EVENT);
projector_type!(
    CapabilityProjector,
    CAPABILITY_PROJECTION,
    CAPABILITY_SNAPSHOT_EVENT
);

#[async_trait]
impl Projector for TicketProjector {
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
        let SupportSnapshot::Ticket(view) = validate_event(
            context,
            event,
            TICKET_PROJECTION,
            TICKET_AGGREGATE_TYPE,
            TICKET_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "ticket projector received another aggregate",
            ));
        };
        let result = sqlx::query("INSERT INTO erpx_support_read.ticket_projection (projection_name,generation,partition_key,ticket_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,customer_id,customer_account_id,case_code,category,severity,description,ticket_status,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,ticket_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,case_code=EXCLUDED.case_code,category=EXCLUDED.category,severity=EXCLUDED.severity,description=EXCLUDED.description,ticket_status=EXCLUDED.ticket_status,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_support_read.ticket_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(TICKET_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.ticket_id.as_uuid())
            .bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(serde_json::to_value(&view.scope).map_err(failure)?)
            .bind(scope_key(&view.scope).map_err(failure)?).bind(view.customer.as_uuid()).bind(view.customer_account.as_uuid())
            .bind(&view.case_code).bind(&view.category).bind(&view.severity).bind(&view.description).bind(ticket_status_name(view.status))
            .bind(view.created_at).bind(view.updated_at).execute(&mut **tx).await.map_err(failure)?;
        if result.rows_affected() == 0 {
            return Ok(());
        }
        sqlx::query("DELETE FROM erpx_support_read.ticket_customer_message_projection WHERE projection_name=$1 AND generation=$2 AND partition_key=$3 AND ticket_id=$4")
            .bind(TICKET_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.ticket_id.as_uuid()).execute(&mut **tx).await.map_err(failure)?;
        sqlx::query("DELETE FROM erpx_support_read.ticket_internal_message_projection WHERE projection_name=$1 AND generation=$2 AND partition_key=$3 AND ticket_id=$4")
            .bind(TICKET_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.ticket_id.as_uuid()).execute(&mut **tx).await.map_err(failure)?;
        for message in &view.customer_messages {
            sqlx::query("INSERT INTO erpx_support_read.ticket_customer_message_projection (projection_name,generation,partition_key,ticket_id,message_id,author_ref,body,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(TICKET_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.ticket_id.as_uuid()).bind(message.message_id.as_uuid()).bind(&message.author_ref).bind(&message.body).bind(message.created_at).execute(&mut **tx).await.map_err(failure)?;
        }
        for message in view.internal_messages.as_deref().unwrap_or_default() {
            sqlx::query("INSERT INTO erpx_support_read.ticket_internal_message_projection (projection_name,generation,partition_key,ticket_id,message_id,author_ref,audience,body,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(TICKET_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.ticket_id.as_uuid()).bind(message.message_id.as_uuid()).bind(&message.author_ref).bind(message_audience_name(message.audience)).bind(&message.body).bind(message.created_at).execute(&mut **tx).await.map_err(failure)?;
        }
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
            "erpx_support_read.ticket_projection",
            TICKET_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for IncidentProjector {
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
        let SupportSnapshot::Incident(view) = validate_event(
            context,
            event,
            INCIDENT_PROJECTION,
            INCIDENT_AGGREGATE_TYPE,
            INCIDENT_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "incident projector received another aggregate",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_support_read.incident_projection (projection_name,generation,partition_key,incident_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,incident_code,severity,commander_ref,affected_services,affected_customer_accounts,incident_status,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,incident_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,incident_code=EXCLUDED.incident_code,severity=EXCLUDED.severity,commander_ref=EXCLUDED.commander_ref,affected_services=EXCLUDED.affected_services,affected_customer_accounts=EXCLUDED.affected_customer_accounts,incident_status=EXCLUDED.incident_status,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_support_read.incident_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(INCIDENT_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.incident_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(&view.incident_code).bind(&view.severity).bind(&view.commander_ref)
            .bind(serde_json::to_value(&view.affected_services).map_err(failure)?).bind(serde_json::to_value(&view.affected_customer_accounts).map_err(failure)?).bind(incident_status_name(view.status)).bind(state).bind(view.created_at).bind(view.updated_at)
            .execute(&mut **tx).await.map_err(failure)?;
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
            "erpx_support_read.incident_projection",
            INCIDENT_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for SessionProjector {
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
        let SupportSnapshot::Session(view) = validate_event(
            context,
            event,
            SESSION_PROJECTION,
            SESSION_AGGREGATE_TYPE,
            SESSION_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "session projector received another aggregate",
            ));
        };
        sqlx::query("INSERT INTO erpx_support_read.session_projection (projection_name,generation,partition_key,session_id,source_aggregate_version,source_global_sequence,approved_scope,scope_key,ticket_id,customer_id,customer_account_id,agent_principal_id,represented_principal_id,support_mode,permissions,module,reason,session_status,requested_at,expires_at,approved_by,approved_at,revoked_at,revoked_reason,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,session_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,approved_scope=EXCLUDED.approved_scope,scope_key=EXCLUDED.scope_key,ticket_id=EXCLUDED.ticket_id,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,agent_principal_id=EXCLUDED.agent_principal_id,represented_principal_id=EXCLUDED.represented_principal_id,support_mode=EXCLUDED.support_mode,permissions=EXCLUDED.permissions,module=EXCLUDED.module,reason=EXCLUDED.reason,session_status=EXCLUDED.session_status,requested_at=EXCLUDED.requested_at,expires_at=EXCLUDED.expires_at,approved_by=EXCLUDED.approved_by,approved_at=EXCLUDED.approved_at,revoked_at=EXCLUDED.revoked_at,revoked_reason=EXCLUDED.revoked_reason,projected_at=now() WHERE erpx_support_read.session_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(SESSION_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.session_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(view.ticket_id.as_uuid()).bind(view.customer.as_uuid()).bind(view.customer_account.as_uuid()).bind(view.agent.as_uuid()).bind(view.represented_principal.map(|v| v.as_uuid()))
            .bind(support_mode_name(view.mode)).bind(serde_json::to_value(&view.permissions).map_err(failure)?).bind(&view.module).bind(&view.reason).bind(session_status_name(view.status)).bind(view.requested_at).bind(view.expires_at).bind(&view.approved_by).bind(view.approved_at).bind(view.revoked_at).bind(&view.revoked_reason)
            .execute(&mut **tx).await.map_err(failure)?;
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
            "erpx_support_read.session_projection",
            SESSION_PROJECTION,
        )
        .await
    }
}

#[async_trait]
impl Projector for CapabilityProjector {
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
        let SupportSnapshot::Capability(view) = validate_event(
            context,
            event,
            CAPABILITY_PROJECTION,
            CAPABILITY_AGGREGATE_TYPE,
            CAPABILITY_SNAPSHOT_EVENT,
        )?
        else {
            return Err(ProjectionFailure::new(
                "capability projector received another aggregate",
            ));
        };
        let (reason, revoked_at) = view
            .revocation
            .map(|(reason, at)| (Some(capability_revocation_name(reason)), Some(at)))
            .unwrap_or((None, None));
        sqlx::query("INSERT INTO erpx_support_read.capability_projection (projection_name,generation,partition_key,capability_id,source_aggregate_version,source_global_sequence,session_id,ticket_id,customer_id,customer_account_id,agent_principal_id,device_ref,support_mode,permissions,approved_scope,scope_key,module,issued_at,expires_at,revocation_reason,revoked_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,capability_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,session_id=EXCLUDED.session_id,ticket_id=EXCLUDED.ticket_id,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,agent_principal_id=EXCLUDED.agent_principal_id,device_ref=EXCLUDED.device_ref,support_mode=EXCLUDED.support_mode,permissions=EXCLUDED.permissions,approved_scope=EXCLUDED.approved_scope,scope_key=EXCLUDED.scope_key,module=EXCLUDED.module,issued_at=EXCLUDED.issued_at,expires_at=EXCLUDED.expires_at,revocation_reason=EXCLUDED.revocation_reason,revoked_at=EXCLUDED.revoked_at,projected_at=now() WHERE erpx_support_read.capability_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(CAPABILITY_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.capability_id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(view.session_id.as_uuid()).bind(view.ticket_id.as_uuid()).bind(view.customer.as_uuid()).bind(view.customer_account.as_uuid()).bind(view.agent.as_uuid()).bind(&view.device_ref).bind(support_mode_name(view.mode)).bind(serde_json::to_value(&view.permissions).map_err(failure)?).bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(&view.module).bind(view.issued_at).bind(view.expires_at).bind(reason).bind(revoked_at)
            .execute(&mut **tx).await.map_err(failure)?;
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
            "erpx_support_read.capability_projection",
            CAPABILITY_PROJECTION,
        )
        .await
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<SupportSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Support projector received incompatible event",
        ));
    }
    let payload: SupportOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let SupportOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "snapshot event carried integration payload",
        ));
    };
    let matches = match &snapshot {
        SupportSnapshot::Ticket(v) => {
            aggregate_type == TICKET_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.ticket_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == ticket_partition(v.ticket_id)
        }
        SupportSnapshot::Incident(v) => {
            aggregate_type == INCIDENT_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.incident_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == incident_partition(v.incident_id)
        }
        SupportSnapshot::Session(v) => {
            aggregate_type == SESSION_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.session_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == session_partition(v.session_id)
        }
        SupportSnapshot::Capability(v) => {
            aggregate_type == CAPABILITY_AGGREGATE_TYPE
                && v.version == event.aggregate_version
                && v.capability_id.as_uuid() == event.aggregate_id.as_uuid()
                && v.scope == event.active_scope
                && event.partition == capability_partition(v.capability_id)
        }
    };
    if matches {
        Ok(snapshot)
    } else {
        Err(ProjectionFailure::new("Support snapshot metadata mismatch"))
    }
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

pub(crate) const fn ticket_status_name(v: TicketStatus) -> &'static str {
    match v {
        TicketStatus::New => "new",
        TicketStatus::Triaged => "triaged",
        TicketStatus::WaitingForCustomer => "waiting_for_customer",
        TicketStatus::AccessRequested => "access_requested",
        TicketStatus::Approved => "approved",
        TicketStatus::InProgress => "in_progress",
        TicketStatus::Resolved => "resolved",
        TicketStatus::CustomerConfirmed => "customer_confirmed",
        TicketStatus::Closed => "closed",
    }
}
pub(crate) const fn incident_status_name(v: IncidentStatus) -> &'static str {
    match v {
        IncidentStatus::Investigating => "investigating",
        IncidentStatus::Identified => "identified",
        IncidentStatus::Monitoring => "monitoring",
        IncidentStatus::Resolved => "resolved",
        IncidentStatus::PostmortemComplete => "postmortem_complete",
    }
}
pub(crate) const fn support_mode_name(v: SupportMode) -> &'static str {
    match v {
        SupportMode::DiagnosticRead => "diagnostic_read",
        SupportMode::HoldingMutation => "holding_mutation",
        SupportMode::Impersonation => "impersonation",
    }
}
pub(crate) const fn session_status_name(v: SessionStatus) -> &'static str {
    match v {
        SessionStatus::Requested => "requested",
        SessionStatus::Approved => "approved",
        SessionStatus::Revoked => "revoked",
    }
}
pub(crate) const fn message_audience_name(v: MessageAudience) -> &'static str {
    match v {
        MessageAudience::CustomerVisible => "customer_visible",
        MessageAudience::Internal => "internal",
    }
}
pub(crate) const fn capability_revocation_name(v: CapabilityRevocationReason) -> &'static str {
    match v {
        CapabilityRevocationReason::CustomerRequested => "customer_requested",
        CapabilityRevocationReason::AgentDisabled => "agent_disabled",
        CapabilityRevocationReason::DeviceRevoked => "device_revoked",
        CapabilityRevocationReason::SessionRevoked => "session_revoked",
        CapabilityRevocationReason::TicketClosed => "ticket_closed",
        CapabilityRevocationReason::Administrative => "administrative",
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
    fn four_projectors_are_snapshot_specific_and_capability_is_secret_free() {
        assert_eq!(
            TicketProjector::new().event_types(),
            &[TICKET_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            IncidentProjector::new().event_types(),
            &[INCIDENT_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            SessionProjector::new().event_types(),
            &[SESSION_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(
            CapabilityProjector::new().event_types(),
            &[CAPABILITY_SNAPSHOT_EVENT.to_owned()]
        );
        let source = include_str!("projection.rs");
        assert!(!source.contains(concat!("capability_", "secret")));
        assert!(!source.contains(concat!("secret_hash", "_sha256 BYTEA")));
    }

    #[test]
    fn every_projection_rejects_duplicate_or_out_of_order_snapshots() {
        let source = include_str!("projection.rs");
        assert_eq!(
            source
                .matches(concat!(
                    "source_aggregate_version < ",
                    "EXCLUDED.source_aggregate_version"
                ))
                .count(),
            4
        );
        assert!(source.contains("if result.rows_affected() == 0"));
        assert!(source.contains("WHERE projection_name=$1 AND generation=$2"));
    }
}
