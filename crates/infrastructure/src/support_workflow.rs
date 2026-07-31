//! Atomic G4 support grant persistence and rebuildable read adapters.
//!
//! This module is intentionally additive.  It composes the existing support
//! session/capability and incident tables with the control-plane outbox and a
//! small read-only projection.  No bearer material or internal diagnostic
//! payload is ever copied into the read model.

use chrono::{DateTime, Utc};
use erpx_application::{
    PersistApprovedSupportGrantCommand, RevokeSupportGrantCommand, SupportWorkflowPort,
    SupportWorkflowReceipt,
};
use erpx_domain::support::{Incident, SupportIncidentReadSummary, SupportMode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

use crate::{OutboxEnvelope, PersistenceError};

pub const SUPPORT_WORKFLOW_PROJECTION_NAME: &str = "support_workflow";
pub const SUPPORT_SESSION_APPROVED_EVENT: &str = "support_session.approved";
pub const SUPPORT_SESSION_REVOKED_EVENT: &str = "support_session.revoked";
pub const SUPPORT_CAPABILITY_ISSUED_EVENT: &str = "support_capability.issued";
pub const SUPPORT_CAPABILITY_REVOKED_EVENT: &str = "support_capability.revoked";
pub const SUPPORT_DIAGNOSTICS_EVENT: &str = "support.diagnostics.snapshot_recorded.v1";
pub const MAX_SUPPORT_WORKFLOW_REBUILD_EVENTS: u32 = 10_000;
pub const MAX_SUPPORT_WORKFLOW_RECONCILIATION_EVENTS: u32 = 10_000;

const SUPPORT_READ_SCHEMA: &str = r#"
CREATE SCHEMA IF NOT EXISTS erpx_control;
CREATE TABLE IF NOT EXISTS erpx_control.support_access_projection (
    session_id UUID PRIMARY KEY,
    capability_id UUID,
    customer_account_id UUID NOT NULL,
    ticket_id UUID NOT NULL,
    agent_ref TEXT NOT NULL,
    device_ref TEXT NOT NULL,
    mode TEXT NOT NULL,
    scope TEXT NOT NULL,
    module TEXT NOT NULL,
    status TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS support_access_projection_capability_idx
    ON erpx_control.support_access_projection (capability_id)
    WHERE capability_id IS NOT NULL;
CREATE TABLE IF NOT EXISTS erpx_control.support_diagnostics_projection (
    scope TEXT PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    schema_version TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    content_hash TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL
);
CREATE TABLE IF NOT EXISTS erpx_control.support_projection_event_history (
    source_event_id UUID PRIMARY KEY,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL
);
"#;

#[derive(Debug, Error)]
pub enum SupportWorkflowPersistenceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("support workflow persistence error: {0}")]
    Persistence(#[from] PersistenceError),
    #[error("support workflow conflict: {0}")]
    Conflict(String),
    #[error("support workflow resource {entity}/{id} was not found")]
    NotFound { entity: &'static str, id: Uuid },
    #[error("support workflow input is invalid: {0}")]
    Invalid(String),
}

impl SupportWorkflowPersistenceError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportAccessProjectionRow {
    pub session_id: Uuid,
    pub capability_id: Option<Uuid>,
    pub customer_account_id: Uuid,
    pub ticket_id: Uuid,
    pub agent_ref: String,
    pub device_ref: String,
    pub mode: String,
    pub scope: String,
    pub module: String,
    pub status: String,
    pub source_event_id: Uuid,
    pub source_version: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportDiagnosticsProjectionRow {
    pub scope: String,
    pub aggregate_id: Uuid,
    pub schema_version: String,
    pub captured_at: DateTime<Utc>,
    pub content_hash: String,
    pub source_event_id: Uuid,
    pub source_version: u64,
    pub updated_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportWorkflowReconciliation {
    pub events_observed: u64,
    pub access_rows: u64,
    pub diagnostics_rows: u64,
    pub incident_rows: u64,
    pub missing_projection_events: u64,
    pub scope_mismatches: u64,
    pub authoritative_access_mismatches: u64,
    pub healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthoritativeAccessBinding {
    capability_id: Option<Uuid>,
    customer_account_id: Uuid,
    ticket_id: Uuid,
    agent_ref: String,
    device_ref: String,
    mode: String,
    scope: String,
    module: String,
    status: String,
    source_version: i64,
}

#[derive(Debug, Clone)]
pub struct SupportWorkflowStore {
    write: PgPool,
    read: PgPool,
}

impl SupportWorkflowStore {
    pub fn new(write: PgPool, read: PgPool) -> Self {
        Self { write, read }
    }

    pub fn write_pool(&self) -> &PgPool {
        &self.write
    }

    pub fn read_pool(&self) -> &PgPool {
        &self.read
    }

    /// The read adapter owns only rebuildable tables.  Production composition
    /// should call this after the normal control-read migrator and before a
    /// support projection worker is allowed to claim events.
    pub async fn ensure_read_model(&self) -> Result<(), SupportWorkflowPersistenceError> {
        sqlx::raw_sql(SUPPORT_READ_SCHEMA)
            .execute(&self.read)
            .await?;
        Ok(())
    }

    pub async fn persist_approved_grant(
        &self,
        command: PersistApprovedSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, SupportWorkflowPersistenceError> {
        validate_grant_again(&command)?;
        let mut tx = self.write.begin().await?;
        let ticket = sqlx::query(
            "SELECT customer_account_id, status
             FROM erpx_control.support_tickets
             WHERE id = $1 FOR SHARE",
        )
        .bind(command.session.ticket_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(SupportWorkflowPersistenceError::NotFound {
            entity: "support_ticket",
            id: command.session.ticket_id,
        })?;
        let ticket_customer: Uuid = ticket.try_get("customer_account_id")?;
        let ticket_status: String = ticket.try_get("status")?;
        validate_ticket_status_for_support_grant(&ticket_status)?;
        if ticket_customer != command.session.customer_account_id {
            return Err(SupportWorkflowPersistenceError::invalid(
                "support ticket/customer scope mismatch",
            ));
        }

        let session_payload = approved_session_payload(&command)?;
        let session_version = upsert_approved_session(&mut tx, &command, &session_payload).await?;
        let request_version = upsert_approved_request(&mut tx, &command, &session_payload).await?;
        let capability_hash = capability_hash(&command)?;
        let capability_payload = capability_payload(&command)?;
        let capability_version = upsert_capability(
            &mut tx,
            &command,
            capability_hash,
            &capability_payload,
            request_version,
        )
        .await?;

        let mut event_ids = Vec::with_capacity(2);
        let (session_event_id, session_replayed) = append_workflow_event(
            &mut tx,
            SUPPORT_SESSION_APPROVED_EVENT,
            "support_session",
            command.session.id,
            session_version,
            &command.session.scope,
            &command.session.agent_ref,
            format!(
                "support-workflow:session-approved:{}:{}",
                command.session.id, session_version
            ),
            json!({
                "session_id": command.session.id,
                "ticket_id": command.session.ticket_id,
                "customer_account_id": command.session.customer_account_id,
                "agent_ref": command.session.agent_ref,
                "device_ref": command.device.device_ref,
                "mode": support_mode(command.session.mode),
                "scope": command.session.scope,
                "module": command.session.module,
                "status": "approved",
                "mfa_present": command.device.mfa_verified_at.is_some(),
                "sod_passed": command.sod_passed,
                "version": session_version,
            }),
            command.at,
        )
        .await?;
        event_ids.push(session_event_id);
        let (capability_event_id, capability_replayed) = append_workflow_event(
            &mut tx,
            SUPPORT_CAPABILITY_ISSUED_EVENT,
            "support_capability",
            command.capability.id,
            capability_version,
            &command.session.scope,
            &command.session.agent_ref,
            format!(
                "support-workflow:capability-issued:{}:{}",
                command.capability.id, capability_version
            ),
            json!({
                "session_id": command.session.id,
                "capability_id": command.capability.id,
                "ticket_id": command.session.ticket_id,
                "customer_account_id": command.session.customer_account_id,
                "agent_ref": command.session.agent_ref,
                "device_ref": command.device.device_ref,
                "mode": support_mode(command.session.mode),
                "scope": command.session.scope,
                "module": command.session.module,
                "status": "active",
                "version": capability_version,
            }),
            command.at,
        )
        .await?;
        event_ids.push(capability_event_id);
        tx.commit().await?;
        Ok(SupportWorkflowReceipt {
            session_id: command.session.id,
            capability_id: command.capability.id,
            session_version,
            capability_version,
            event_ids,
            replayed: session_replayed && capability_replayed,
        })
    }

    pub async fn revoke_grant(
        &self,
        command: RevokeSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, SupportWorkflowPersistenceError> {
        let mut tx = self.write.begin().await?;
        let row = sqlx::query(
            "SELECT ticket_id, customer_account_id, version, status, payload
             FROM erpx_control.support_sessions WHERE id = $1 FOR UPDATE",
        )
        .bind(command.session_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(SupportWorkflowPersistenceError::NotFound {
            entity: "support_session",
            id: command.session_id,
        })?;
        let ticket_id: Uuid = row.try_get("ticket_id")?;
        let customer_account_id: Uuid = row.try_get("customer_account_id")?;
        let old_version = positive_version(row.try_get("version")?, "support session version")?;
        let old_payload: Value = row.try_get("payload")?;
        let scope = old_payload
            .get("scope")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid("support session scope is missing")
            })?
            .to_owned();
        let agent_ref = old_payload
            .get("agent_ref")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid("support session agent is missing")
            })?
            .to_owned();
        let device_ref = old_payload
            .get("device_ref")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid("support session device is missing")
            })?
            .to_owned();
        let mode = old_payload
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid("support session mode is missing")
            })?
            .to_owned();
        let module = old_payload
            .get("module")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid("support session module is missing")
            })?
            .to_owned();
        let current_status: String = row.try_get("status")?;
        let mut event_ids = Vec::new();
        let session_version = if current_status == "revoked" {
            old_version
        } else {
            let next = old_version.checked_add(1).ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid("support session version overflow")
            })?;
            let mut payload = old_payload;
            if let Some(object) = payload.as_object_mut() {
                object.insert("status".into(), Value::String("revoked".into()));
                object.insert("revoked_at".into(), json!(command.at));
                object.insert("revocation_reason".into(), json!(command.reason));
            }
            sqlx::query(
                "UPDATE erpx_control.support_sessions
                 SET status = 'revoked', version = $2, payload = $3, updated_at = $4
                 WHERE id = $1 AND version = $5 AND status <> 'revoked'",
            )
            .bind(command.session_id)
            .bind(as_i64(next, "support session version")?)
            .bind(payload)
            .bind(command.at)
            .bind(as_i64(old_version, "support session version")?)
            .execute(&mut *tx)
            .await?;
            let (event_id, _) = append_workflow_event(
                &mut tx,
                SUPPORT_SESSION_REVOKED_EVENT,
                "support_session",
                command.session_id,
                next,
                &scope,
                &command.actor_ref,
                format!(
                    "support-workflow:session-revoked:{}:{}",
                    command.session_id, next
                ),
                json!({
                    "session_id": command.session_id,
                    "ticket_id": ticket_id,
                    "customer_account_id": customer_account_id,
                    "agent_ref": agent_ref,
                    "device_ref": device_ref,
                    "mode": mode,
                    "scope": scope,
                    "module": module,
                    "status": "revoked",
                    "reason": command.reason,
                    "version": next,
                }),
                command.at,
            )
            .await?;
            event_ids.push(event_id);
            next
        };

        // The capability trigger intentionally requires its access request to
        // remain approved while the capability's immutable binding is
        // transitioned. Revoke the capability rows first, then revoke the
        // request in the same transaction; reversing this order makes the
        // append-only revocation path fail closed before it can persist its
        // event.
        let capabilities = sqlx::query(
            "UPDATE erpx_control.support_capabilities
             SET status = 'revoked', revoked_at = $2, revocation_reason = $3,
                 version = version + 1, updated_at = $2
             WHERE access_request_id = $1 AND status = 'active'
             RETURNING id, ticket_id, customer_account_id, agent_ref, device_ref,
                       mode, scope, module, version",
        )
        .bind(command.session_id)
        .bind(command.at)
        .bind(&command.reason)
        .fetch_all(&mut *tx)
        .await?;
        let had_capabilities = !capabilities.is_empty();
        let mut capability_id = Uuid::nil();
        let mut capability_version = 0_u64;
        for capability in capabilities {
            capability_id = capability.try_get("id")?;
            let version = positive_version(capability.try_get("version")?, "capability version")?;
            capability_version = capability_version.max(version);
            let (event_id, _) = append_workflow_event(
                &mut tx,
                SUPPORT_CAPABILITY_REVOKED_EVENT,
                "support_capability",
                capability_id,
                version,
                capability.try_get::<String, _>("scope")?.as_str(),
                &command.actor_ref,
                format!("support-workflow:capability-revoked:{capability_id}:{version}"),
                json!({
                    "session_id": command.session_id,
                    "capability_id": capability_id,
                    "ticket_id": capability.try_get::<Uuid, _>("ticket_id")?,
                    "customer_account_id": capability.try_get::<Uuid, _>("customer_account_id")?,
                    "agent_ref": capability.try_get::<String, _>("agent_ref")?,
                    "device_ref": capability.try_get::<String, _>("device_ref")?,
                    "mode": capability.try_get::<String, _>("mode")?,
                    "scope": capability.try_get::<String, _>("scope")?,
                    "module": capability.try_get::<String, _>("module")?,
                    "status": "revoked",
                    "reason": command.reason,
                    "version": version,
                }),
                command.at,
            )
            .await?;
            event_ids.push(event_id);
        }

        sqlx::query(
            "UPDATE erpx_control.support_access_requests
             SET status = 'revoked', revoked_at = $2, version = version + 1,
                 payload = jsonb_set(payload, '{status}', to_jsonb('revoked'::text)),
                 updated_at = $2
             WHERE id = $1 AND status = 'approved'",
        )
        .bind(command.session_id)
        .bind(command.at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(SupportWorkflowReceipt {
            session_id: command.session_id,
            capability_id,
            session_version,
            capability_version,
            event_ids,
            replayed: current_status == "revoked" && !had_capabilities,
        })
    }

    pub async fn project_event(
        &self,
        event: &OutboxEnvelope,
    ) -> Result<bool, SupportWorkflowPersistenceError> {
        if !is_support_event(event) {
            return Ok(false);
        }
        self.verify_authoritative_source_event(event).await?;
        if event.aggregate_type == "incident" {
            let incident: Incident = serde_json::from_value(event.payload.clone())?;
            let summary = SupportIncidentReadSummary::from_incident(
                &incident,
                event.aggregate_version,
                event.id,
                event.occurred_at,
            )
            .map_err(|error| SupportWorkflowPersistenceError::invalid(error.to_string()))?;
            let payload = serde_json::to_value(&summary)?;
            sqlx::query(
                "INSERT INTO erpx_control.incidents
                    (id, status, commander_ref, leads, affected_services,
                     affected_customer_accounts, version, payload, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (id) DO UPDATE SET
                    status = EXCLUDED.status, commander_ref = EXCLUDED.commander_ref,
                    leads = EXCLUDED.leads, affected_services = EXCLUDED.affected_services,
                    affected_customer_accounts = EXCLUDED.affected_customer_accounts,
                    version = EXCLUDED.version, payload = EXCLUDED.payload,
                    updated_at = EXCLUDED.updated_at
                 WHERE erpx_control.incidents.version < EXCLUDED.version",
            )
            .bind(summary.id)
            .bind(incident_status(summary.status))
            .bind(&summary.commander_ref)
            .bind(serde_json::to_value(&summary.leads)?)
            .bind(serde_json::to_value(&summary.affected_services)?)
            .bind(serde_json::to_value(&summary.affected_customer_accounts)?)
            .bind(as_i64(summary.source_version, "incident version")?)
            .bind(payload)
            .bind(summary.projected_at)
            .execute(&self.read)
            .await?;
            self.record_projected_event(event).await?;
            return Ok(true);
        }
        if event.event_type == SUPPORT_DIAGNOSTICS_EVENT {
            let projected = self.project_diagnostics(event).await?;
            if projected {
                self.record_projected_event(event).await?;
            }
            return Ok(projected);
        }
        let projected = self.project_access(event).await?;
        if projected {
            self.record_projected_event(event).await?;
        }
        Ok(projected)
    }

    async fn verify_authoritative_source_event(
        &self,
        event: &OutboxEnvelope,
    ) -> Result<(), SupportWorkflowPersistenceError> {
        let source = sqlx::query(
            "SELECT event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, tenant_scope, actor, correlation_id,
                    causation_id, idempotency_key, payload, occurred_at
             FROM erpx_control.outbox_events
             WHERE id = $1",
        )
        .bind(event.id)
        .fetch_optional(&self.write)
        .await?
        .ok_or_else(|| {
            SupportWorkflowPersistenceError::invalid(
                "support projection event is absent from the authoritative outbox",
            )
        })?;
        let expected_event_version = i32::try_from(event.event_version).map_err(|_| {
            SupportWorkflowPersistenceError::invalid("support projection event version is invalid")
        })?;
        let matches_source = source.try_get::<String, _>("event_type")? == event.event_type
            && source.try_get::<i32, _>("event_version")? == expected_event_version
            && source.try_get::<String, _>("aggregate_type")? == event.aggregate_type
            && source.try_get::<Uuid, _>("aggregate_id")? == event.aggregate_id
            && positive_version(source.try_get("aggregate_version")?, "event version")?
                == event.aggregate_version
            && source.try_get::<Option<Value>, _>("tenant_scope")? == event.tenant_scope
            && source.try_get::<Option<Value>, _>("actor")? == event.actor
            && source.try_get::<Uuid, _>("correlation_id")? == event.correlation_id
            && source.try_get::<Option<Uuid>, _>("causation_id")? == event.causation_id
            && source.try_get::<Option<String>, _>("idempotency_key")? == event.idempotency_key
            && source.try_get::<Value, _>("payload")? == event.payload
            && source.try_get::<DateTime<Utc>, _>("occurred_at")? == event.occurred_at;
        if !matches_source {
            return Err(SupportWorkflowPersistenceError::Conflict(
                "support projection event does not match the authoritative outbox".into(),
            ));
        }
        Ok(())
    }

    pub async fn clear_read_models(&self) -> Result<(), SupportWorkflowPersistenceError> {
        sqlx::query("DELETE FROM erpx_control.support_access_projection")
            .execute(&self.read)
            .await?;
        sqlx::query("DELETE FROM erpx_control.support_diagnostics_projection")
            .execute(&self.read)
            .await?;
        sqlx::query("DELETE FROM erpx_control.support_projection_event_history")
            .execute(&self.read)
            .await?;
        sqlx::query("DELETE FROM erpx_control.incidents")
            .execute(&self.read)
            .await?;
        Ok(())
    }

    pub async fn rebuild_read_models(&self) -> Result<u64, SupportWorkflowPersistenceError> {
        self.ensure_read_model().await?;
        let events = sqlx::query(
            "SELECT id, event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, tenant_scope, actor, correlation_id,
                    causation_id, idempotency_key, payload, occurred_at, available_at,
                    attempts, max_attempts, claimed_by, claimed_at, published_at,
                    dead_lettered_at, last_error, last_error_context
             FROM erpx_control.outbox_events
             WHERE aggregate_type IN ('support_session','support_capability','incident',
                                      'support_diagnostics_audit')
             ORDER BY occurred_at, id
             LIMIT $1",
        )
        .bind(i64::from(MAX_SUPPORT_WORKFLOW_REBUILD_EVENTS) + 1)
        .fetch_all(&self.write)
        .await?;
        if events.len() > MAX_SUPPORT_WORKFLOW_REBUILD_EVENTS as usize {
            return Err(SupportWorkflowPersistenceError::invalid(
                "support workflow rebuild exceeds its bounded event window; drain in batches",
            ));
        }
        self.clear_read_models().await?;
        let mut projected = 0_u64;
        for row in events {
            let event = outbox_from_row(&row)?;
            if self.project_event(&event).await? {
                projected = projected.saturating_add(1);
            }
        }
        Ok(projected)
    }

    pub async fn reconcile(
        &self,
    ) -> Result<SupportWorkflowReconciliation, SupportWorkflowPersistenceError> {
        self.ensure_read_model().await?;
        let event_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.outbox_events
             WHERE aggregate_type IN ('support_session','support_capability','incident',
                                      'support_diagnostics_audit')",
        )
        .fetch_one(&self.write)
        .await?;
        let access_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM erpx_control.support_access_projection")
                .fetch_one(&self.read)
                .await?;
        let diagnostics_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM erpx_control.support_diagnostics_projection")
                .fetch_one(&self.read)
                .await?;
        let incident_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_control.incidents")
            .fetch_one(&self.read)
            .await?;
        let projected_event_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.support_projection_event_history",
        )
        .fetch_one(&self.read)
        .await?;
        let event_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM erpx_control.outbox_events
             WHERE aggregate_type IN ('support_session','support_capability','incident',
                                      'support_diagnostics_audit')
             ORDER BY occurred_at, id LIMIT $1",
        )
        .bind(i64::from(MAX_SUPPORT_WORKFLOW_RECONCILIATION_EVENTS))
        .fetch_all(&self.write)
        .await?;
        let mut missing_projection_events = 0_i64;
        for event_id in &event_ids {
            let present = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (
                    SELECT 1 FROM erpx_control.support_projection_event_history
                    WHERE source_event_id = $1
                )",
            )
            .bind(event_id)
            .fetch_one(&self.read)
            .await?;
            if !present {
                missing_projection_events += 1;
            }
        }
        let scope_mismatches: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.outbox_events e
             WHERE e.aggregate_type IN ('support_session','support_capability','support_diagnostics_audit')
               AND (e.tenant_scope IS NULL
                    OR e.tenant_scope->>'support_scope' IS NULL)",
        )
        .fetch_one(&self.write)
        .await?;
        // A complete outbox history can still produce a false-green result if
        // the rebuild target was tampered with after projection. The source
        // and read pools are separate databases, so compare a bounded set of
        // typed rows in Rust rather than relying on a cross-database join.
        let source_access_count: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT access_request_id)
             FROM erpx_control.support_capabilities c
             WHERE EXISTS (
                 SELECT 1 FROM erpx_control.outbox_events e
                 WHERE e.aggregate_type = 'support_capability'
                   AND e.aggregate_id = c.id
             )",
        )
        .fetch_one(&self.write)
        .await?;
        let source_access_rows = sqlx::query(
            "SELECT DISTINCT ON (access_request_id)
                    access_request_id, id, customer_account_id, ticket_id,
                    agent_ref, device_ref, mode, scope, module, status, version
             FROM erpx_control.support_capabilities c
             WHERE EXISTS (
                 SELECT 1 FROM erpx_control.outbox_events e
                 WHERE e.aggregate_type = 'support_capability'
                   AND e.aggregate_id = c.id
             )
             ORDER BY c.access_request_id, c.version DESC, c.id DESC
             LIMIT $1",
        )
        .bind(i64::from(MAX_SUPPORT_WORKFLOW_RECONCILIATION_EVENTS))
        .fetch_all(&self.write)
        .await?;
        let read_access_rows = sqlx::query(
            "SELECT session_id, capability_id, customer_account_id, ticket_id,
                    agent_ref, device_ref, mode, scope, module, status,
                    source_version
             FROM erpx_control.support_access_projection
             ORDER BY session_id
             LIMIT $1",
        )
        .bind(i64::from(MAX_SUPPORT_WORKFLOW_RECONCILIATION_EVENTS))
        .fetch_all(&self.read)
        .await?;
        let mut authoritative_access = BTreeMap::new();
        for row in source_access_rows {
            let session_id: Uuid = row.try_get("access_request_id")?;
            let status: String = row.try_get("status")?;
            authoritative_access.insert(
                session_id,
                AuthoritativeAccessBinding {
                    capability_id: Some(row.try_get("id")?),
                    customer_account_id: row.try_get("customer_account_id")?,
                    ticket_id: row.try_get("ticket_id")?,
                    agent_ref: row.try_get("agent_ref")?,
                    device_ref: row.try_get("device_ref")?,
                    mode: row.try_get("mode")?,
                    scope: row.try_get("scope")?,
                    module: row.try_get("module")?,
                    status: if status == "active" {
                        "active".into()
                    } else {
                        "revoked".into()
                    },
                    source_version: row.try_get("version")?,
                },
            );
        }
        let mut projected_access = BTreeMap::new();
        for row in read_access_rows {
            let session_id: Uuid = row.try_get("session_id")?;
            projected_access.insert(
                session_id,
                AuthoritativeAccessBinding {
                    capability_id: row.try_get("capability_id")?,
                    customer_account_id: row.try_get("customer_account_id")?,
                    ticket_id: row.try_get("ticket_id")?,
                    agent_ref: row.try_get("agent_ref")?,
                    device_ref: row.try_get("device_ref")?,
                    mode: row.try_get("mode")?,
                    scope: row.try_get("scope")?,
                    module: row.try_get("module")?,
                    status: row.try_get("status")?,
                    source_version: row.try_get("source_version")?,
                },
            );
        }
        let mut authoritative_access_mismatches = if source_access_count != access_rows {
            1_i64
        } else {
            0_i64
        };
        if source_access_count > i64::from(MAX_SUPPORT_WORKFLOW_RECONCILIATION_EVENTS)
            || access_rows > i64::from(MAX_SUPPORT_WORKFLOW_RECONCILIATION_EVENTS)
        {
            // The reconciliation is intentionally bounded; an oversized
            // source/read set is not evidence of parity until it is drained.
            authoritative_access_mismatches = authoritative_access_mismatches.max(1);
        } else {
            authoritative_access_mismatches += authoritative_access
                .iter()
                .filter(|(session_id, binding)| projected_access.get(session_id) != Some(binding))
                .count() as i64;
            authoritative_access_mismatches += projected_access
                .keys()
                .filter(|session_id| !authoritative_access.contains_key(session_id))
                .count() as i64;
        }
        let events_observed = positive_count(event_count, "support workflow event count")?;
        let missing_projection_events = positive_count(
            missing_projection_events,
            "support workflow missing projection count",
        )?;
        let scope_mismatches = positive_count(scope_mismatches, "support workflow scope count")?;
        let authoritative_access_mismatches = positive_count(
            authoritative_access_mismatches,
            "support workflow authoritative access mismatch count",
        )?;
        Ok(SupportWorkflowReconciliation {
            events_observed,
            access_rows: positive_count(access_rows, "support access read count")?,
            diagnostics_rows: positive_count(diagnostics_rows, "support diagnostic read count")?,
            incident_rows: positive_count(incident_rows, "support incident read count")?,
            missing_projection_events,
            scope_mismatches,
            authoritative_access_mismatches,
            healthy: missing_projection_events == 0
                && scope_mismatches == 0
                && authoritative_access_mismatches == 0
                && projected_event_count == event_count,
        })
    }

    pub async fn access_projection(
        &self,
        session_id: Uuid,
    ) -> Result<Option<SupportAccessProjectionRow>, SupportWorkflowPersistenceError> {
        let row = sqlx::query(
            "SELECT session_id, capability_id, customer_account_id, ticket_id,
                    agent_ref, device_ref, mode, scope, module, status,
                    source_event_id, source_version, updated_at
             FROM erpx_control.support_access_projection WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_optional(&self.read)
        .await?;
        row.map(|row| {
            Ok(SupportAccessProjectionRow {
                session_id: row.try_get("session_id")?,
                capability_id: row.try_get("capability_id")?,
                customer_account_id: row.try_get("customer_account_id")?,
                ticket_id: row.try_get("ticket_id")?,
                agent_ref: row.try_get("agent_ref")?,
                device_ref: row.try_get("device_ref")?,
                mode: row.try_get("mode")?,
                scope: row.try_get("scope")?,
                module: row.try_get("module")?,
                status: row.try_get("status")?,
                source_event_id: row.try_get("source_event_id")?,
                source_version: positive_version(row.try_get("source_version")?, "read version")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
    }

    pub async fn diagnostics_projection(
        &self,
        scope: &str,
    ) -> Result<Option<SupportDiagnosticsProjectionRow>, SupportWorkflowPersistenceError> {
        if scope.trim().is_empty() || scope.len() > 255 || scope.chars().any(char::is_control) {
            return Err(SupportWorkflowPersistenceError::invalid(
                "support diagnostics scope is invalid",
            ));
        }
        let row = sqlx::query(
            "SELECT scope, aggregate_id, schema_version, captured_at, content_hash,
                    source_event_id, source_version, updated_at, payload
             FROM erpx_control.support_diagnostics_projection WHERE scope = $1",
        )
        .bind(scope)
        .fetch_optional(&self.read)
        .await?;
        row.map(|row| {
            Ok(SupportDiagnosticsProjectionRow {
                scope: row.try_get("scope")?,
                aggregate_id: row.try_get("aggregate_id")?,
                schema_version: row.try_get("schema_version")?,
                captured_at: row.try_get("captured_at")?,
                content_hash: row.try_get("content_hash")?,
                source_event_id: row.try_get("source_event_id")?,
                source_version: positive_version(row.try_get("source_version")?, "read version")?,
                updated_at: row.try_get("updated_at")?,
                payload: row.try_get("payload")?,
            })
        })
        .transpose()
    }

    async fn project_access(
        &self,
        event: &OutboxEnvelope,
    ) -> Result<bool, SupportWorkflowPersistenceError> {
        let payload = event.payload.as_object().ok_or_else(|| {
            SupportWorkflowPersistenceError::invalid(
                "support access event payload is not an object",
            )
        })?;
        let scope = required_string(payload, "scope")?;
        if event
            .tenant_scope
            .as_ref()
            .and_then(|scope| scope.get("support_scope"))
            .and_then(Value::as_str)
            != Some(scope.as_str())
        {
            return Err(SupportWorkflowPersistenceError::invalid(
                "support access event scope mismatch",
            ));
        }
        let session_id = payload
            .get("session_id")
            .or_else(|| payload.get("access_request_id"))
            .cloned()
            .ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid(
                    "support access event session reference is missing",
                )
            })
            .and_then(|value| {
                let mut fields = serde_json::Map::new();
                fields.insert("session_id".into(), value);
                required_uuid(&fields, "session_id")
            })?;
        let capability_id: Option<Uuid> = payload
            .get("capability_id")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?;
        let source_version = event.aggregate_version;
        if event.aggregate_type == "support_capability" && capability_id.is_none() {
            return Err(SupportWorkflowPersistenceError::invalid(
                "support capability event lacks capability ID",
            ));
        }
        let status = required_string(payload, "status")?;
        let customer_account_id = required_uuid(payload, "customer_account_id")?;
        let ticket_id = required_uuid(payload, "ticket_id")?;
        let agent_ref = required_string(payload, "agent_ref")?;
        let device_ref = required_string(payload, "device_ref")?;
        let mode = required_string(payload, "mode")?;
        let module = required_string(payload, "module")?;
        let read_payload = json!({
            "session_id": session_id,
            "capability_id": capability_id,
            "customer_account_id": customer_account_id,
            "ticket_id": ticket_id,
            "agent_ref": agent_ref,
            "device_ref": device_ref,
            "mode": mode,
            "scope": scope,
            "module": module,
            "status": status,
            "source_event_id": event.id,
            "source_version": source_version,
        });
        sqlx::query(
            "INSERT INTO erpx_control.support_access_projection
                (session_id, capability_id, customer_account_id, ticket_id,
                 agent_ref, device_ref, mode, scope, module, status,
                 source_event_id, source_version, updated_at, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
             ON CONFLICT (session_id) DO UPDATE SET
                capability_id = COALESCE(EXCLUDED.capability_id,
                    erpx_control.support_access_projection.capability_id),
                customer_account_id = EXCLUDED.customer_account_id,
                ticket_id = EXCLUDED.ticket_id, agent_ref = EXCLUDED.agent_ref,
                device_ref = EXCLUDED.device_ref, mode = EXCLUDED.mode,
                scope = EXCLUDED.scope, module = EXCLUDED.module,
                status = EXCLUDED.status, source_event_id = EXCLUDED.source_event_id,
                source_version = EXCLUDED.source_version, updated_at = EXCLUDED.updated_at,
                payload = EXCLUDED.payload
             WHERE erpx_control.support_access_projection.source_version < EXCLUDED.source_version
                OR (erpx_control.support_access_projection.source_version = EXCLUDED.source_version
                    AND EXCLUDED.capability_id IS NOT NULL)",
        )
        .bind(session_id)
        .bind(capability_id)
        .bind(customer_account_id)
        .bind(ticket_id)
        .bind(&agent_ref)
        .bind(&device_ref)
        .bind(&mode)
        .bind(scope)
        .bind(&module)
        .bind(status)
        .bind(event.id)
        .bind(as_i64(source_version, "support access event version")?)
        .bind(event.occurred_at)
        .bind(read_payload)
        .execute(&self.read)
        .await?;
        Ok(true)
    }

    async fn project_diagnostics(
        &self,
        event: &OutboxEnvelope,
    ) -> Result<bool, SupportWorkflowPersistenceError> {
        let payload = event.payload.as_object().ok_or_else(|| {
            SupportWorkflowPersistenceError::invalid("diagnostics event payload is not an object")
        })?;
        let scope = required_string(payload, "scope")?;
        if event
            .tenant_scope
            .as_ref()
            .and_then(|scope| scope.get("support_scope"))
            .and_then(Value::as_str)
            != Some(scope.as_str())
        {
            return Err(SupportWorkflowPersistenceError::invalid(
                "diagnostics event scope mismatch",
            ));
        }
        let snapshot = payload.get("snapshot").ok_or_else(|| {
            SupportWorkflowPersistenceError::invalid("diagnostics event snapshot is missing")
        })?;
        let encoded = serde_json::to_string(snapshot)?;
        let forbidden = [
            "password",
            "secret",
            "token",
            "credential",
            "private_key",
            "database_url",
            "connection_string",
            "authorization",
            "cookie",
        ];
        if forbidden
            .iter()
            .any(|marker| encoded.to_ascii_lowercase().contains(marker))
        {
            return Err(SupportWorkflowPersistenceError::invalid(
                "diagnostics projection contains forbidden material",
            ));
        }
        let schema_version = required_string(payload, "schema_version")?;
        let captured_at: DateTime<Utc> =
            serde_json::from_value(payload.get("captured_at").cloned().ok_or_else(|| {
                SupportWorkflowPersistenceError::invalid("diagnostics captured_at is missing")
            })?)?;
        let content_hash = required_string(payload, "content_hash")?;
        if content_hash != sha256_hex(&serde_json::to_vec(snapshot)?) {
            return Err(SupportWorkflowPersistenceError::invalid(
                "diagnostics snapshot content hash mismatch",
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.support_diagnostics_projection
                (scope, aggregate_id, schema_version, captured_at, content_hash,
                 source_event_id, source_version, updated_at, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (scope) DO UPDATE SET
                aggregate_id = EXCLUDED.aggregate_id,
                schema_version = EXCLUDED.schema_version, captured_at = EXCLUDED.captured_at,
                content_hash = EXCLUDED.content_hash, source_event_id = EXCLUDED.source_event_id,
                source_version = EXCLUDED.source_version, updated_at = EXCLUDED.updated_at,
                payload = EXCLUDED.payload
             WHERE erpx_control.support_diagnostics_projection.source_version < EXCLUDED.source_version",
        )
        .bind(scope)
        .bind(event.aggregate_id)
        .bind(schema_version)
        .bind(captured_at)
        .bind(content_hash)
        .bind(event.id)
        .bind(as_i64(event.aggregate_version, "diagnostics event version")?)
        .bind(event.occurred_at)
        .bind(snapshot)
        .execute(&self.read)
        .await?;
        Ok(true)
    }

    async fn record_projected_event(
        &self,
        event: &OutboxEnvelope,
    ) -> Result<(), SupportWorkflowPersistenceError> {
        sqlx::query(
            "INSERT INTO erpx_control.support_projection_event_history
                (source_event_id, aggregate_type, aggregate_id, source_version, projected_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (source_event_id) DO NOTHING",
        )
        .bind(event.id)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .bind(as_i64(
            event.aggregate_version,
            "support projection event version",
        )?)
        .bind(event.occurred_at)
        .execute(&self.read)
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl SupportWorkflowPort for SupportWorkflowStore {
    async fn persist_approved_grant(
        &self,
        command: PersistApprovedSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, erpx_application::ApplicationError> {
        Self::persist_approved_grant(self, command)
            .await
            .map_err(|error| erpx_application::ApplicationError::Repository(error.to_string()))
    }

    async fn revoke_grant(
        &self,
        command: RevokeSupportGrantCommand,
    ) -> Result<SupportWorkflowReceipt, erpx_application::ApplicationError> {
        Self::revoke_grant(self, command)
            .await
            .map_err(|error| erpx_application::ApplicationError::Repository(error.to_string()))
    }
}

fn validate_grant_again(
    command: &PersistApprovedSupportGrantCommand,
) -> Result<(), SupportWorkflowPersistenceError> {
    command
        .session
        .authorize_device(
            &command.device,
            &command.actor_scope,
            &command.requested_scope,
            command.sod_passed,
            command.at,
        )
        .map_err(|error| SupportWorkflowPersistenceError::invalid(error.to_string()))?;
    if command.session.approved_at.is_none()
        || command.capability.issued_at < command.session.approved_at.expect("checked above")
        || command.capability.binding.customer_account_id != command.session.customer_account_id
        || command.capability.binding.ticket_id != command.session.ticket_id
        || command.capability.binding.agent_ref != command.session.agent_ref
        || command.capability.binding.device_ref != command.device.device_ref
        || command.capability.binding.session_id != command.session.id
        || command.capability.binding.mode != command.session.mode
        || command.capability.binding.scope != command.requested_scope.canonical_key()
        || command.capability.binding.module != command.session.module
        || command.capability.expires_at > command.session.expires_at
        || !command.capability.is_active(command.at)
    {
        return Err(SupportWorkflowPersistenceError::invalid(
            "support grant immutable binding or active window is invalid",
        ));
    }
    if !command
        .session
        .permissions
        .iter()
        .all(|permission| command.capability.binding.permissions.contains(permission))
    {
        return Err(SupportWorkflowPersistenceError::invalid(
            "support grant permissions do not contain the approved session permissions",
        ));
    }
    Ok(())
}

fn validate_ticket_status_for_support_grant(
    status: &str,
) -> Result<(), SupportWorkflowPersistenceError> {
    match status {
        "new"
        | "triaged"
        | "waiting_for_customer"
        | "access_requested"
        | "approved"
        | "in_progress"
        | "resolved"
        | "customer_confirmed" => Ok(()),
        "closed" => Err(SupportWorkflowPersistenceError::invalid(
            "closed support tickets cannot receive support grants",
        )),
        _ => Err(SupportWorkflowPersistenceError::invalid(
            "support ticket status is invalid for a support grant",
        )),
    }
}

fn approved_session_payload(
    command: &PersistApprovedSupportGrantCommand,
) -> Result<Value, SupportWorkflowPersistenceError> {
    let mut payload = serde_json::to_value(&command.session)?;
    let object = payload.as_object_mut().ok_or_else(|| {
        SupportWorkflowPersistenceError::invalid(
            "approved support session payload is not an object",
        )
    })?;
    object.insert("device_ref".into(), json!(command.device.device_ref));
    object.insert(
        "mfa_verified_at".into(),
        json!(command.device.mfa_verified_at),
    );
    object.insert("sod_passed".into(), json!(command.sod_passed));
    object.insert("status".into(), json!("approved"));
    Ok(payload)
}

async fn upsert_approved_session(
    tx: &mut Transaction<'_, Postgres>,
    command: &PersistApprovedSupportGrantCommand,
    payload: &Value,
) -> Result<u64, SupportWorkflowPersistenceError> {
    let row = sqlx::query(
        "INSERT INTO erpx_control.support_sessions
            (id, ticket_id, customer_account_id, status, expires_at, version, payload)
         VALUES ($1,$2,$3,'approved',$4,1,$5)
        ON CONFLICT (id) DO UPDATE SET
            status = 'approved', expires_at = EXCLUDED.expires_at,
            version = erpx_control.support_sessions.version + 1,
            payload = EXCLUDED.payload, updated_at = now()
         WHERE erpx_control.support_sessions.status <> 'revoked'
           AND erpx_control.support_sessions.ticket_id = EXCLUDED.ticket_id
           AND erpx_control.support_sessions.customer_account_id = EXCLUDED.customer_account_id
           AND erpx_control.support_sessions.payload->>'device_ref' = EXCLUDED.payload->>'device_ref'
           AND erpx_control.support_sessions.payload->>'agent_ref' = EXCLUDED.payload->>'agent_ref'
           AND (erpx_control.support_sessions.status <> 'approved'
                OR erpx_control.support_sessions.payload IS DISTINCT FROM EXCLUDED.payload)
         RETURNING version",
    )
    .bind(command.session.id)
    .bind(command.session.ticket_id)
    .bind(command.session.customer_account_id)
    .bind(command.session.expires_at)
    .bind(payload)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = row {
        return positive_version(row.try_get("version")?, "support session version");
    }
    let row = sqlx::query("SELECT status, version, ticket_id, customer_account_id, payload FROM erpx_control.support_sessions WHERE id = $1 FOR SHARE")
        .bind(command.session.id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| SupportWorkflowPersistenceError::invalid("support session insert did not return a row"))?;
    let status: String = row.try_get("status")?;
    let stored_payload: Value = row.try_get("payload")?;
    if status != "approved"
        || row.try_get::<Uuid, _>("ticket_id")? != command.session.ticket_id
        || row.try_get::<Uuid, _>("customer_account_id")? != command.session.customer_account_id
        || stored_payload.get("device_ref") != payload.get("device_ref")
        || stored_payload.get("agent_ref") != payload.get("agent_ref")
    {
        return Err(SupportWorkflowPersistenceError::Conflict(
            "support session immutable binding conflicts with the existing row".into(),
        ));
    }
    positive_version(row.try_get("version")?, "support session version")
}

async fn upsert_approved_request(
    tx: &mut Transaction<'_, Postgres>,
    command: &PersistApprovedSupportGrantCommand,
    payload: &Value,
) -> Result<u64, SupportWorkflowPersistenceError> {
    let permissions = serde_json::to_value(&command.session.permissions)?;
    let row = sqlx::query(
        "INSERT INTO erpx_control.support_access_requests
            (id, ticket_id, customer_account_id, agent_ref, represented_tenant_user,
             mode, permissions, scope, reason, requested_at, expires_at, status,
             approved_by, approved_at, approval_ref, version, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'approved',$12,$13,$14,1,$15)
         ON CONFLICT (id) DO UPDATE SET
            status='approved', approved_by=EXCLUDED.approved_by,
            approved_at=EXCLUDED.approved_at, approval_ref=EXCLUDED.approval_ref,
            version=erpx_control.support_access_requests.version + 1,
            payload=EXCLUDED.payload, updated_at=now()
         WHERE erpx_control.support_access_requests.status='requested'
           AND erpx_control.support_access_requests.approved_at IS NULL
           AND erpx_control.support_access_requests.ticket_id=EXCLUDED.ticket_id
           AND erpx_control.support_access_requests.customer_account_id=EXCLUDED.customer_account_id
         RETURNING version",
    )
    .bind(command.session.id)
    .bind(command.session.ticket_id)
    .bind(command.session.customer_account_id)
    .bind(&command.session.agent_ref)
    .bind(&command.session.represented_tenant_user)
    .bind(support_mode(command.session.mode))
    .bind(permissions)
    .bind(&command.session.scope)
    .bind(&command.session.reason)
    .bind(command.session.requested_at)
    .bind(command.session.expires_at)
    .bind(command.session.approved_by.as_deref())
    .bind(command.session.approved_at)
    .bind(&command.approval_ref)
    .bind(payload)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = row {
        return positive_version(row.try_get("version")?, "support request version");
    }
    let row = sqlx::query(
        "SELECT status, version, approval_ref, ticket_id, customer_account_id
         FROM erpx_control.support_access_requests WHERE id=$1 FOR SHARE",
    )
    .bind(command.session.id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        SupportWorkflowPersistenceError::invalid("support request insert did not return a row")
    })?;
    if row.try_get::<String, _>("status")? != "approved"
        || row.try_get::<Option<String>, _>("approval_ref")?.as_deref()
            != Some(command.approval_ref.as_str())
        || row.try_get::<Uuid, _>("ticket_id")? != command.session.ticket_id
        || row.try_get::<Uuid, _>("customer_account_id")? != command.session.customer_account_id
    {
        return Err(SupportWorkflowPersistenceError::Conflict(
            "support approval conflicts with the existing request".into(),
        ));
    }
    positive_version(row.try_get("version")?, "support request version")
}

async fn upsert_capability(
    tx: &mut Transaction<'_, Postgres>,
    command: &PersistApprovedSupportGrantCommand,
    secret_hash: Vec<u8>,
    payload: &Value,
    _request_version: u64,
) -> Result<u64, SupportWorkflowPersistenceError> {
    let permissions = serde_json::to_value(&command.capability.binding.permissions)?;
    let inserted = sqlx::query(
        "INSERT INTO erpx_control.support_capabilities
            (id, access_request_id, customer_account_id, agent_ref, device_ref,
             ticket_id, mode, permissions, scope, module, issued_at, expires_at,
             secret_hash, reissued_from, status, version, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,'active',1,$15)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(command.capability.id)
    .bind(command.session.id)
    .bind(command.capability.binding.customer_account_id)
    .bind(&command.capability.binding.agent_ref)
    .bind(&command.capability.binding.device_ref)
    .bind(command.capability.binding.ticket_id)
    .bind(support_mode(command.capability.binding.mode))
    .bind(permissions)
    .bind(&command.capability.binding.scope)
    .bind(&command.capability.binding.module)
    .bind(command.capability.issued_at)
    .bind(command.capability.expires_at)
    .bind(secret_hash)
    .bind(command.capability.reissued_from)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    let row = sqlx::query(
        "SELECT version, access_request_id, customer_account_id, agent_ref,
                device_ref, ticket_id, mode, permissions, scope, module,
                issued_at, expires_at, status, secret_hash
         FROM erpx_control.support_capabilities WHERE id=$1 FOR SHARE",
    )
    .bind(command.capability.id)
    .fetch_one(&mut **tx)
    .await?;
    if row.try_get::<Uuid, _>("access_request_id")? != command.session.id
        || row.try_get::<Uuid, _>("customer_account_id")?
            != command.capability.binding.customer_account_id
        || row.try_get::<String, _>("agent_ref")? != command.capability.binding.agent_ref
        || row.try_get::<String, _>("device_ref")? != command.capability.binding.device_ref
        || row.try_get::<String, _>("status")? != "active"
        || row.try_get::<Vec<u8>, _>("secret_hash")? != capability_hash(command)?
    {
        return Err(SupportWorkflowPersistenceError::Conflict(
            "support capability immutable binding conflicts with the existing row".into(),
        ));
    }
    let _ = inserted;
    positive_version(row.try_get("version")?, "capability version")
}

fn capability_hash(
    command: &PersistApprovedSupportGrantCommand,
) -> Result<Vec<u8>, SupportWorkflowPersistenceError> {
    let value = serde_json::to_value(command.capability.secret_hash())?;
    let bytes: Vec<u8> = serde_json::from_value(value)
        .map_err(|_| SupportWorkflowPersistenceError::invalid("capability hash is invalid"))?;
    if bytes.len() != 32 {
        return Err(SupportWorkflowPersistenceError::invalid(
            "capability hash must be exactly 32 bytes",
        ));
    }
    Ok(bytes)
}

fn capability_payload(
    command: &PersistApprovedSupportGrantCommand,
) -> Result<Value, SupportWorkflowPersistenceError> {
    Ok(json!({
        "id": command.capability.id,
        "session_id": command.session.id,
        "ticket_id": command.session.ticket_id,
        "customer_account_id": command.session.customer_account_id,
        "agent_ref": command.session.agent_ref,
        "device_ref": command.device.device_ref,
        "mode": support_mode(command.session.mode),
        "permissions": command.capability.binding.permissions,
        "scope": command.session.scope,
        "module": command.session.module,
        "issued_at": command.capability.issued_at,
        "expires_at": command.capability.expires_at,
        "status": "active",
    }))
}

#[allow(clippy::too_many_arguments)]
async fn append_workflow_event(
    tx: &mut Transaction<'_, Postgres>,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    scope: &str,
    actor_ref: &str,
    idempotency_key: String,
    payload: Value,
    occurred_at: DateTime<Utc>,
) -> Result<(Uuid, bool), SupportWorkflowPersistenceError> {
    if aggregate_id.is_nil() || aggregate_version == 0 || scope.trim().is_empty() {
        return Err(SupportWorkflowPersistenceError::invalid(
            "support workflow event identity is invalid",
        ));
    }
    let tenant_scope = json!({ "support_scope": scope });
    let actor = json!({ "principal_ref": actor_ref });
    let event_id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id,event_type,event_version,aggregate_type,aggregate_id,aggregate_version,
             tenant_scope,actor,correlation_id,idempotency_key,payload,occurred_at,available_at)
         VALUES ($1,$2,1,$3,$4,$5,$6,$7,$1,$8,$9,$10,$10)
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(as_i64(aggregate_version, "support workflow event version")?)
    .bind(&tenant_scope)
    .bind(&actor)
    .bind(&idempotency_key)
    .bind(&payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok((event_id, false));
    }
    let stored = sqlx::query(
        "SELECT id, event_type, event_version, aggregate_type, aggregate_id,
                aggregate_version, tenant_scope, actor, payload, occurred_at
         FROM erpx_control.outbox_events WHERE idempotency_key=$1 FOR SHARE",
    )
    .bind(&idempotency_key)
    .fetch_one(&mut **tx)
    .await?;
    if stored.try_get::<String, _>("event_type")? != event_type
        || stored.try_get::<i32, _>("event_version")? != 1
        || stored.try_get::<String, _>("aggregate_type")? != aggregate_type
        || stored.try_get::<Uuid, _>("aggregate_id")? != aggregate_id
        || positive_version(stored.try_get("aggregate_version")?, "event version")?
            != aggregate_version
        || stored.try_get::<Value, _>("tenant_scope")? != tenant_scope
        || stored.try_get::<Value, _>("actor")? != actor
        || stored.try_get::<Value, _>("payload")? != payload
        || stored.try_get::<DateTime<Utc>, _>("occurred_at")? != occurred_at
    {
        return Err(SupportWorkflowPersistenceError::Conflict(
            "support workflow event idempotency key has a different identity or payload".into(),
        ));
    }
    Ok((stored.try_get("id")?, true))
}

fn outbox_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<OutboxEnvelope, SupportWorkflowPersistenceError> {
    Ok(OutboxEnvelope {
        id: row.try_get("id")?,
        event_type: row.try_get("event_type")?,
        event_version: u32::try_from(row.try_get::<i32, _>("event_version")?)
            .map_err(|_| SupportWorkflowPersistenceError::invalid("event version is invalid"))?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        aggregate_version: positive_version(row.try_get("aggregate_version")?, "event version")?,
        tenant_scope: row.try_get("tenant_scope")?,
        actor: row.try_get("actor")?,
        correlation_id: row.try_get("correlation_id")?,
        causation_id: row.try_get("causation_id")?,
        idempotency_key: row.try_get("idempotency_key")?,
        payload: row.try_get("payload")?,
        occurred_at: row.try_get("occurred_at")?,
        available_at: row.try_get("available_at")?,
        attempts: u32::try_from(row.try_get::<i32, _>("attempts")?)
            .map_err(|_| SupportWorkflowPersistenceError::invalid("event attempts invalid"))?,
        max_attempts: u32::try_from(row.try_get::<i32, _>("max_attempts")?)
            .map_err(|_| SupportWorkflowPersistenceError::invalid("event max attempts invalid"))?,
        claimed_by: row.try_get("claimed_by")?,
        claimed_at: row.try_get("claimed_at")?,
        published_at: row.try_get("published_at")?,
        dead_lettered_at: row.try_get("dead_lettered_at")?,
        last_error: row.try_get("last_error")?,
        last_error_context: row.try_get("last_error_context")?,
    })
}

fn is_support_event(event: &OutboxEnvelope) -> bool {
    support_event_type_allowed(&event.aggregate_type, &event.event_type)
}

fn support_event_type_allowed(aggregate_type: &str, event_type: &str) -> bool {
    match aggregate_type {
        "support_session" => matches!(
            event_type,
            SUPPORT_SESSION_APPROVED_EVENT | SUPPORT_SESSION_REVOKED_EVENT
        ),
        "support_capability" => matches!(
            event_type,
            SUPPORT_CAPABILITY_ISSUED_EVENT | SUPPORT_CAPABILITY_REVOKED_EVENT
        ),
        "incident" => matches!(event_type, "incident.created" | "incident.updated"),
        "support_diagnostics_audit" => event_type == SUPPORT_DIAGNOSTICS_EVENT,
        _ => false,
    }
}

fn required_string(
    payload: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<String, SupportWorkflowPersistenceError> {
    let value = payload.get(name).and_then(Value::as_str).ok_or_else(|| {
        SupportWorkflowPersistenceError::invalid(format!("support payload field {name} is missing"))
    })?;
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(SupportWorkflowPersistenceError::invalid(format!(
            "support payload field {name} is invalid"
        )));
    }
    Ok(value.to_owned())
}

fn required_uuid(
    payload: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<Uuid, SupportWorkflowPersistenceError> {
    let id: Uuid = serde_json::from_value(payload.get(name).cloned().ok_or_else(|| {
        SupportWorkflowPersistenceError::invalid(format!("support payload field {name} is missing"))
    })?)?;
    if id.is_nil() {
        return Err(SupportWorkflowPersistenceError::invalid(format!(
            "support payload field {name} is nil"
        )));
    }
    Ok(id)
}

fn positive_version(value: i64, field: &str) -> Result<u64, SupportWorkflowPersistenceError> {
    u64::try_from(value)
        .map_err(|_| SupportWorkflowPersistenceError::invalid(format!("{field} is invalid")))
}

fn positive_count(value: i64, field: &str) -> Result<u64, SupportWorkflowPersistenceError> {
    u64::try_from(value)
        .map_err(|_| SupportWorkflowPersistenceError::invalid(format!("{field} is invalid")))
}

fn as_i64(value: u64, field: &str) -> Result<i64, SupportWorkflowPersistenceError> {
    i64::try_from(value).map_err(|_| {
        SupportWorkflowPersistenceError::invalid(format!("{field} exceeds database range"))
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

fn support_mode(mode: SupportMode) -> &'static str {
    match mode {
        SupportMode::DiagnosticRead => "diagnostic_read",
        SupportMode::TenantMutation => "tenant_mutation",
        SupportMode::Impersonation => "impersonation",
    }
}

fn incident_status(status: erpx_domain::support::IncidentStatus) -> &'static str {
    match status {
        erpx_domain::support::IncidentStatus::Investigating => "investigating",
        erpx_domain::support::IncidentStatus::Identified => "identified",
        erpx_domain::support::IncidentStatus::Monitoring => "monitoring",
        erpx_domain::support::IncidentStatus::Resolved => "resolved",
        erpx_domain::support::IncidentStatus::PostmortemComplete => "postmortem_complete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_grants_reject_terminal_and_unknown_ticket_states() {
        assert!(validate_ticket_status_for_support_grant("access_requested").is_ok());
        assert_eq!(
            validate_ticket_status_for_support_grant("closed")
                .expect_err("closed tickets are terminal")
                .to_string(),
            "support workflow input is invalid: closed support tickets cannot receive support grants"
        );
        assert!(
            validate_ticket_status_for_support_grant("not_a_ticket_state").is_err(),
            "unknown source states must fail closed"
        );
    }

    #[test]
    fn support_event_allowlist_is_closed_per_aggregate_family() {
        assert!(support_event_type_allowed(
            "support_session",
            SUPPORT_SESSION_APPROVED_EVENT
        ));
        assert!(support_event_type_allowed(
            "support_capability",
            SUPPORT_CAPABILITY_REVOKED_EVENT
        ));
        assert!(support_event_type_allowed("incident", "incident.updated"));
        assert!(support_event_type_allowed(
            "support_diagnostics_audit",
            SUPPORT_DIAGNOSTICS_EVENT
        ));

        assert!(!support_event_type_allowed(
            "support_session",
            SUPPORT_CAPABILITY_ISSUED_EVENT
        ));
        assert!(!support_event_type_allowed(
            "incident",
            SUPPORT_DIAGNOSTICS_EVENT
        ));
        assert!(!support_event_type_allowed(
            "support_session",
            "support_session.unknown"
        ));
    }
}
