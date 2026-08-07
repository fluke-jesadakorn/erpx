use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_support::{
    CapabilityId, CommandUnitOfWork, Incident, IncidentId, SessionId, SupportAggregate,
    SupportCapability, SupportCommit, SupportSession, SupportTicket, SupportTicketId,
    WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, DependencyVersion, PortError, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CAPABILITY_AGGREGATE_TYPE, CAPABILITY_SNAPSHOT_EVENT, CONTEXT_NAME, EVENT_VERSION,
    INCIDENT_AGGREGATE_TYPE, INCIDENT_SNAPSHOT_EVENT, SESSION_AGGREGATE_TYPE,
    SESSION_SNAPSHOT_EVENT, TICKET_AGGREGATE_TYPE, TICKET_SNAPSHOT_EVENT, capability_partition,
    incident_partition,
    projection::{
        SupportOutboxPayload, SupportSnapshot, capability_revocation_name, incident_status_name,
        message_audience_name, session_status_name, support_mode_name, ticket_status_name,
    },
    session_partition, ticket_partition,
};

#[derive(Debug, Clone)]
pub struct SupportWriteRepository {
    store: CommandStore,
}
impl SupportWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

impl SupportWriteRepository {
    async fn load_aggregate(
        &self,
        authority: &TenantScope,
        partition: &str,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<SupportAggregate>, PortError> {
        let aggregate_id = AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
        let snapshot = self
            .store
            .load_snapshot::<SupportAggregate>(
                CONTEXT_NAME,
                partition,
                aggregate_type,
                aggregate_id,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id_uuid() != id
            || snapshot.state.aggregate_type() != aggregate_type
            || !authority.contains(snapshot.state.scope())
        {
            return Err(PortError::Internal);
        }
        Ok(Some(snapshot.state))
    }
}

#[async_trait]
impl WriteRepository for SupportWriteRepository {
    async fn load_ticket(
        &self,
        scope: &TenantScope,
        id: SupportTicketId,
    ) -> Result<Option<SupportTicket>, PortError> {
        match self
            .load_aggregate(
                scope,
                &ticket_partition(id),
                TICKET_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(SupportAggregate::Ticket(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_incident(
        &self,
        scope: &TenantScope,
        id: IncidentId,
    ) -> Result<Option<Incident>, PortError> {
        match self
            .load_aggregate(
                scope,
                &incident_partition(id),
                INCIDENT_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(SupportAggregate::Incident(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_session(
        &self,
        scope: &TenantScope,
        id: SessionId,
    ) -> Result<Option<SupportSession>, PortError> {
        match self
            .load_aggregate(
                scope,
                &session_partition(id),
                SESSION_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(SupportAggregate::Session(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
    async fn load_capability(
        &self,
        scope: &TenantScope,
        id: CapabilityId,
    ) -> Result<Option<SupportCapability>, PortError> {
        match self
            .load_aggregate(
                scope,
                &capability_partition(id),
                CAPABILITY_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(SupportAggregate::Capability(v)) => Ok(Some(v)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SupportCommandAdapter {
    store: CommandStore,
}
impl SupportCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for SupportCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, aggregate_type) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<SupportAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(|receipt| receipt_to_stored(receipt, aggregate_type))
            .transpose()
    }

    async fn commit(&self, request: SupportCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let aggregate_type = request.aggregate.aggregate_type();
        let snapshot_event = snapshot_event(&request.aggregate);
        let occurred_at = request.aggregate.occurred_at();
        let command_id = request.identity.command_id;
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: SupportOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: SupportOutboxPayload::Snapshot(SupportSnapshot::from(&request.aggregate)),
        });
        let guard = SupportAtomicGuard {
            aggregate: request.aggregate.clone(),
            dependency: request.dependency.clone(),
        };
        let aggregate = request.aggregate;
        let aggregate_id =
            AggregateId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"support-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"aggregate_id": aggregate_id.as_uuid(), "aggregate_type": aggregate_type, "expected_version": request.expected_version, "dependency": request.dependency.as_ref().map(|v| (v.aggregate_type, v.aggregate_id, v.expected_version))}),
            },
            events,
        };
        let outcome = self
            .store
            .commit_guarded(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(v) | AtomicCommandOutcome::Replayed(v) => v,
        };
        let mut stored = receipt_to_stored(receipt, aggregate_type)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

struct SupportAtomicGuard {
    aggregate: SupportAggregate,
    dependency: Option<DependencyVersion>,
}

#[async_trait]
impl AtomicCommandGuard for SupportAtomicGuard {
    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current = guard_version(
            tx,
            self.aggregate.aggregate_type(),
            self.aggregate.id_uuid(),
        )
        .await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "Support aggregate already exists",
                ));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "Support aggregate guard is missing",
                ));
            }
            _ => {}
        }
        if let Some(dependency) = &self.dependency {
            let actual =
                guard_version(tx, dependency.aggregate_type, dependency.aggregate_id).await?;
            if actual != Some(dependency.expected_version) {
                return Err(CqrsSqlxError::GuardRejected(
                    "Support dependency version changed",
                ));
            }
        }
        match &self.aggregate {
            SupportAggregate::Ticket(value) if context.expected_version.is_none() => {
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT ticket_id FROM erpx_support_write.ticket_guards WHERE scope_key=$1 AND case_code=$2 FOR UPDATE").bind(scope_key(value.scope())?).bind(value.case_code()).fetch_optional(&mut **tx).await?;
                if existing.is_some_and(|id| id != value.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support case code already exists",
                    ));
                }
            }
            SupportAggregate::Session(value) => {
                let row = sqlx::query("SELECT customer_id,customer_account_id,scope_key,ticket_status FROM erpx_support_write.ticket_guards WHERE ticket_id=$1 FOR SHARE").bind(value.ticket_id().as_uuid()).fetch_optional(&mut **tx).await?;
                let Some(row) = row else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support ticket guard is missing",
                    ));
                };
                if row.try_get::<Uuid, _>("customer_id")? != value.customer().as_uuid()
                    || row.try_get::<Uuid, _>("customer_account_id")?
                        != value.customer_account().as_uuid()
                    || row.try_get::<String, _>("scope_key")? != scope_key(value.scope())?
                    || row.try_get::<String, _>("ticket_status")? != "access_requested"
                {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support session ticket binding is invalid",
                    ));
                }
            }
            SupportAggregate::Capability(value) => {
                let row = sqlx::query("SELECT session_status,expires_at FROM erpx_support_write.session_guards WHERE session_id=$1 FOR SHARE").bind(value.session_id().as_uuid()).fetch_optional(&mut **tx).await?;
                let Some(row) = row else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support session guard is missing",
                    ));
                };
                if row.try_get::<String, _>("session_status")? != "approved"
                    || row.try_get::<chrono::DateTime<chrono::Utc>, _>("expires_at")?
                        != value.expires_at()
                {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support capability session is inactive",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.aggregate.aggregate_type()
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Support applied guard metadata mismatch",
            ));
        }
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            SupportAggregate::Ticket(v) => {
                sqlx::query("INSERT INTO erpx_support_write.ticket_guards (ticket_id,scope_key,customer_id,customer_account_id,case_code,category,severity,description,ticket_status,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT (ticket_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,case_code=EXCLUDED.case_code,category=EXCLUDED.category,severity=EXCLUDED.severity,description=EXCLUDED.description,ticket_status=EXCLUDED.ticket_status,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.customer().as_uuid()).bind(v.customer_account().as_uuid()).bind(v.case_code()).bind(v.category()).bind(v.severity()).bind(v.description()).bind(ticket_status_name(v.status())).bind(version).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?;
                sqlx::query(
                    "DELETE FROM erpx_support_write.ticket_message_guards WHERE ticket_id=$1",
                )
                .bind(v.id().as_uuid())
                .execute(&mut **tx)
                .await?;
                for message in v.messages() {
                    sqlx::query("INSERT INTO erpx_support_write.ticket_message_guards (ticket_id,message_id,author_ref,audience,body,created_at) VALUES ($1,$2,$3,$4,$5,$6)").bind(v.id().as_uuid()).bind(message.id().as_uuid()).bind(message.author_ref()).bind(message_audience_name(message.audience())).bind(message.body()).bind(message.created_at()).execute(&mut **tx).await?;
                }
            }
            SupportAggregate::Incident(v) => {
                sqlx::query("INSERT INTO erpx_support_write.incident_guards (incident_id,scope_key,incident_code,severity,commander_ref,affected_services,affected_customer_accounts,timeline,broadcasts,incident_status,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (incident_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,incident_code=EXCLUDED.incident_code,severity=EXCLUDED.severity,commander_ref=EXCLUDED.commander_ref,affected_services=EXCLUDED.affected_services,affected_customer_accounts=EXCLUDED.affected_customer_accounts,timeline=EXCLUDED.timeline,broadcasts=EXCLUDED.broadcasts,incident_status=EXCLUDED.incident_status,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.incident_code()).bind(v.severity()).bind(v.commander_ref()).bind(serde_json::to_value(v.affected_services())?).bind(serde_json::to_value(v.affected_customer_accounts())?).bind(serde_json::to_value(v.timeline())?).bind(serde_json::to_value(v.broadcasts())?).bind(incident_status_name(v.status())).bind(version).bind(v.created_at()).bind(v.updated_at()).execute(&mut **tx).await?;
            }
            SupportAggregate::Session(v) => {
                sqlx::query("INSERT INTO erpx_support_write.session_guards (session_id,scope_key,ticket_id,customer_id,customer_account_id,agent_principal_id,represented_principal_id,support_mode,permissions,module,reason,session_status,aggregate_version,requested_at,expires_at,approved_by,approved_at,revoked_at,revoked_reason) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19) ON CONFLICT (session_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,ticket_id=EXCLUDED.ticket_id,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,agent_principal_id=EXCLUDED.agent_principal_id,represented_principal_id=EXCLUDED.represented_principal_id,support_mode=EXCLUDED.support_mode,permissions=EXCLUDED.permissions,module=EXCLUDED.module,reason=EXCLUDED.reason,session_status=EXCLUDED.session_status,aggregate_version=EXCLUDED.aggregate_version,requested_at=EXCLUDED.requested_at,expires_at=EXCLUDED.expires_at,approved_by=EXCLUDED.approved_by,approved_at=EXCLUDED.approved_at,revoked_at=EXCLUDED.revoked_at,revoked_reason=EXCLUDED.revoked_reason")
                    .bind(v.id().as_uuid()).bind(scope_key(v.scope())?).bind(v.ticket_id().as_uuid()).bind(v.customer().as_uuid()).bind(v.customer_account().as_uuid()).bind(v.agent().as_uuid()).bind(v.represented_principal().map(|p| p.as_uuid())).bind(support_mode_name(v.mode())).bind(serde_json::to_value(v.permissions())?).bind(v.module()).bind(v.reason()).bind(session_status_name(v.status())).bind(version).bind(v.requested_at()).bind(v.expires_at()).bind(v.approved_by()).bind(v.approved_at()).bind(v.revoked_at()).bind(v.revoked_reason()).execute(&mut **tx).await?;
            }
            SupportAggregate::Capability(v) => {
                let (reason, revoked_at) = v
                    .revocation()
                    .map(|(r, a)| (Some(capability_revocation_name(r)), Some(a)))
                    .unwrap_or((None, None));
                sqlx::query("INSERT INTO erpx_support_write.capability_guards (capability_id,session_id,ticket_id,customer_id,customer_account_id,agent_principal_id,device_ref,support_mode,permissions,approved_scope,scope_key,module,secret_hash_sha256,aggregate_version,issued_at,expires_at,revocation_reason,revoked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) ON CONFLICT (capability_id) DO UPDATE SET session_id=EXCLUDED.session_id,ticket_id=EXCLUDED.ticket_id,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,agent_principal_id=EXCLUDED.agent_principal_id,device_ref=EXCLUDED.device_ref,support_mode=EXCLUDED.support_mode,permissions=EXCLUDED.permissions,approved_scope=EXCLUDED.approved_scope,scope_key=EXCLUDED.scope_key,module=EXCLUDED.module,secret_hash_sha256=EXCLUDED.secret_hash_sha256,aggregate_version=EXCLUDED.aggregate_version,issued_at=EXCLUDED.issued_at,expires_at=EXCLUDED.expires_at,revocation_reason=EXCLUDED.revocation_reason,revoked_at=EXCLUDED.revoked_at")
                    .bind(v.id().as_uuid()).bind(v.session_id().as_uuid()).bind(v.ticket_id().as_uuid()).bind(v.customer().as_uuid()).bind(v.customer_account().as_uuid()).bind(v.agent().as_uuid()).bind(v.device_ref()).bind(support_mode_name(v.mode())).bind(serde_json::to_value(v.permissions())?).bind(serde_json::to_value(v.scope())?).bind(scope_key(v.scope())?).bind(v.module()).bind(v.secret_hash_sha256().as_slice()).bind(version).bind(v.issued_at()).bind(v.expires_at()).bind(reason).bind(revoked_at).execute(&mut **tx).await?;
            }
        }
        Ok(())
    }
}

async fn guard_version(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    id: Uuid,
) -> Result<Option<u64>, CqrsSqlxError> {
    let statement = match aggregate_type {
        TICKET_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.ticket_guards WHERE ticket_id=$1 FOR UPDATE"
        }
        INCIDENT_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.incident_guards WHERE incident_id=$1 FOR UPDATE"
        }
        SESSION_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.session_guards WHERE session_id=$1 FOR UPDATE"
        }
        CAPABILITY_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.capability_guards WHERE capability_id=$1 FOR UPDATE"
        }
        _ => {
            return Err(CqrsSqlxError::InvalidInput(
                "unknown Support aggregate type",
            ));
        }
    };
    sqlx::query_scalar::<_, i64>(statement)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .map(|v| u64::try_from(v).map_err(|_| CqrsSqlxError::NumericRange))
        .transpose()
}
fn validate_guard_context(
    value: &SupportAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context == CONTEXT_NAME
        && context.aggregate_type == value.aggregate_type()
        && context.aggregate_id.as_uuid() == value.id_uuid()
        && context.partition == aggregate_partition(value)
    {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(
            "Support guard metadata mismatch",
        ))
    }
}
fn validate_commit(request: &SupportCommit) -> Result<(), PortError> {
    let (partition, aggregate_type) = fingerprint_target(&request.identity.fingerprint);
    if partition != aggregate_partition(&request.aggregate)
        || aggregate_type != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}

fn fingerprint_target(v: &CommandFingerprint) -> (String, &'static str) {
    match v {
        CommandFingerprint::OpenTicket { ticket_id, .. }
        | CommandFingerprint::TransitionTicket { ticket_id, .. }
        | CommandFingerprint::AddTicketMessage { ticket_id, .. } => {
            (ticket_partition(*ticket_id), TICKET_AGGREGATE_TYPE)
        }
        CommandFingerprint::OpenIncident { incident_id, .. }
        | CommandFingerprint::TransitionIncident { incident_id, .. }
        | CommandFingerprint::AddIncidentTimeline { incident_id, .. }
        | CommandFingerprint::BroadcastIncident { incident_id, .. } => {
            (incident_partition(*incident_id), INCIDENT_AGGREGATE_TYPE)
        }
        CommandFingerprint::RequestSession { session_id, .. }
        | CommandFingerprint::ApproveSession { session_id }
        | CommandFingerprint::RevokeSession { session_id, .. } => {
            (session_partition(*session_id), SESSION_AGGREGATE_TYPE)
        }
        CommandFingerprint::IssueCapability { capability_id, .. }
        | CommandFingerprint::RevokeCapability { capability_id, .. } => (
            capability_partition(*capability_id),
            CAPABILITY_AGGREGATE_TYPE,
        ),
    }
}
fn aggregate_partition(v: &SupportAggregate) -> String {
    match v {
        SupportAggregate::Ticket(v) => ticket_partition(v.id()),
        SupportAggregate::Incident(v) => incident_partition(v.id()),
        SupportAggregate::Session(v) => session_partition(v.id()),
        SupportAggregate::Capability(v) => capability_partition(v.id()),
    }
}
fn snapshot_event(v: &SupportAggregate) -> &'static str {
    match v {
        SupportAggregate::Ticket(_) => TICKET_SNAPSHOT_EVENT,
        SupportAggregate::Incident(_) => INCIDENT_SNAPSHOT_EVENT,
        SupportAggregate::Session(_) => SESSION_SNAPSHOT_EVENT,
        SupportAggregate::Capability(_) => CAPABILITY_SNAPSHOT_EVENT,
    }
}
fn audit_action(v: &CommandFingerprint) -> &'static str {
    match v {
        CommandFingerprint::OpenTicket { .. } => "support.ticket.open",
        CommandFingerprint::TransitionTicket { .. } => "support.ticket.transition",
        CommandFingerprint::AddTicketMessage { .. } => "support.ticket.message",
        CommandFingerprint::RequestSession { .. } => "support.session.request",
        CommandFingerprint::ApproveSession { .. } => "support.session.approve",
        CommandFingerprint::RevokeSession { .. } => "support.session.revoke",
        CommandFingerprint::IssueCapability { .. } => "support.capability.issue",
        CommandFingerprint::RevokeCapability { .. } => "support.capability.revoke",
        CommandFingerprint::OpenIncident { .. } => "support.incident.open",
        CommandFingerprint::TransitionIncident { .. } => "support.incident.transition",
        CommandFingerprint::AddIncidentTimeline { .. } => "support.incident.timeline",
        CommandFingerprint::BroadcastIncident { .. } => "support.incident.broadcast",
    }
}
fn request_hash(v: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut d = Sha256::new();
    d.update(b"erpx.support.command.v1\0");
    d.update(serde_json::to_vec(v).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(d.finalize().into()))
}
fn deterministic_event_id(command: CommandId, kind: &str, index: usize) -> EventId {
    let mut input = kind.as_bytes().to_vec();
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &input)).expect("derived event id")
}
fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut d = Sha256::new();
    d.update(seed.as_bytes());
    d.update(label);
    let d = d.finalize();
    let mut b = [0; 16];
    b.copy_from_slice(&d[..16]);
    b[6] = (b[6] & 0x0f) | 0x50;
    b[8] = (b[8] & 0x3f) | 0x80;
    Uuid::from_bytes(b)
}
pub(crate) fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(CqrsSqlxError::InvalidInput("Support scope must be exact")),
    }
}
fn to_i64(v: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(v).map_err(|_| CqrsSqlxError::NumericRange)
}
fn receipt_to_stored(
    r: erpx_cqrs::CommandReceipt<SupportAggregate>,
    kind: &str,
) -> Result<StoredCommit, PortError> {
    if r.aggregate.context != CONTEXT_NAME
        || r.aggregate.aggregate_type != kind
        || r.aggregate.aggregate_id.as_uuid() != r.response.id_uuid()
        || r.aggregate.version != r.response.version()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: r.command_id,
        correlation_id: r.correlation_id,
        aggregate: r.response,
        committed_events: r.committed_events,
        committed_at: r.committed_at,
        replayed: r.replayed,
    })
}
fn map_store_error(e: CqrsSqlxError) -> PortError {
    match e {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_hash_and_all_targets_are_distinct() {
        let id = SupportTicketId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let value = CommandFingerprint::TransitionTicket {
            ticket_id: id,
            next: erpx_support::TicketStatus::Triaged,
        };
        assert_eq!(request_hash(&value).unwrap(), request_hash(&value).unwrap());
        assert_eq!(
            fingerprint_target(&value),
            (ticket_partition(id), TICKET_AGGREGATE_TYPE)
        );
        assert_ne!(
            ticket_partition(id),
            format!("support.session:{}", id.as_uuid().simple())
        );
    }
}
