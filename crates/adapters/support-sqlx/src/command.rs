use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, customer, identity_access, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId};
use erpx_support::{
    BroadcastAudience, CapabilityId, CapabilityRevocationReason, CommandUnitOfWork, Incident,
    IncidentBroadcast, IncidentId, IncidentStatus, IncidentTimelineEntry, MessageAudience,
    MessageId, SessionId, SessionStatus, SupportAggregate, SupportAuthorityEffect,
    SupportCapability, SupportCommit, SupportMode, SupportSession, SupportTicket, SupportTicketId,
    TicketMessage, TicketStatus, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, DependencyVersion, PortError, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AUTHORITY_EFFECT_EVENT, CAPABILITY_AGGREGATE_TYPE, CAPABILITY_SNAPSHOT_EVENT, CONTEXT_NAME,
    EVENT_VERSION, INCIDENT_AGGREGATE_TYPE, INCIDENT_SNAPSHOT_EVENT, SESSION_AGGREGATE_TYPE,
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
    pool: WritePool,
}
impl SupportWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

impl SupportWriteRepository {
    async fn load_aggregate(
        &self,
        authority: &AuthorityScope,
        partition: &str,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<SupportAggregate>, PortError> {
        let _ = partition;
        load_support_relational(&self.pool, authority, aggregate_type, id).await
    }
}

async fn load_support_relational(
    pool: &WritePool,
    authority: &AuthorityScope,
    aggregate_type: &str,
    id: Uuid,
) -> Result<Option<SupportAggregate>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    match aggregate_type {
        TICKET_AGGREGATE_TYPE => load_ticket_relational(&mut tx, authority, id)
            .await
            .map(|value| value.map(SupportAggregate::Ticket)),
        INCIDENT_AGGREGATE_TYPE => load_incident_relational(&mut tx, authority, id)
            .await
            .map(|value| value.map(SupportAggregate::Incident)),
        SESSION_AGGREGATE_TYPE => load_session_relational(&mut tx, authority, id)
            .await
            .map(|value| value.map(SupportAggregate::Session)),
        CAPABILITY_AGGREGATE_TYPE => load_capability_relational(&mut tx, authority, id)
            .await
            .map(|value| value.map(SupportAggregate::Capability)),
        _ => Err(PortError::Internal),
    }
}

async fn load_ticket_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<SupportTicket>, PortError> {
    let row = support_authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,customer_id,customer_account_id,case_code,category,severity,description,ticket_status,created_at,updated_at FROM erpx_support_write.ticket_authority WHERE ticket_id=$1",
        "erpx_support_write.ticket_authority",
        "ticket_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let messages = sqlx::query("SELECT message_id,author_ref,audience,body,created_at FROM erpx_support_write.ticket_message_authority WHERE ticket_id=$1 ORDER BY message_ordinal")
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?
        .into_iter()
        .map(|message| {
            TicketMessage::new(
                MessageId::try_from_uuid(message.try_get("message_id").map_err(|_| PortError::Internal)?)
                    .map_err(|_| PortError::Internal)?,
                message.try_get("author_ref").map_err(|_| PortError::Internal)?,
                parse_message_audience(message.try_get::<String, _>("audience").map_err(|_| PortError::Internal)?.as_str())?,
                message.try_get("body").map_err(|_| PortError::Internal)?,
                message.try_get("created_at").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    SupportTicket::try_rehydrate(
        SupportTicketId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        support_row_u64(&row, "aggregate_version")?,
        support_exact_scope(&row)?,
        customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer::CustomerAccountRef::try_from_uuid(
            row.try_get("customer_account_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row.try_get("case_code").map_err(|_| PortError::Internal)?,
        row.try_get("category").map_err(|_| PortError::Internal)?,
        row.try_get("severity").map_err(|_| PortError::Internal)?,
        row.try_get("description")
            .map_err(|_| PortError::Internal)?,
        parse_ticket_status(
            row.try_get::<String, _>("ticket_status")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )?,
        messages,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_incident_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<Incident>, PortError> {
    let row = support_authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,incident_code,severity,commander_ref,incident_status,created_at,updated_at FROM erpx_support_write.incident_authority WHERE incident_id=$1",
        "erpx_support_write.incident_authority",
        "incident_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let affected_services = sqlx::query_scalar::<_, String>("SELECT service_ref FROM erpx_support_write.incident_service_authority WHERE incident_id=$1 ORDER BY service_ordinal")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_| PortError::Unavailable)?;
    let affected_customer_accounts = sqlx::query_scalar::<_, Uuid>("SELECT customer_account_id FROM erpx_support_write.incident_customer_account_authority WHERE incident_id=$1 ORDER BY account_ordinal")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_| PortError::Unavailable)?
        .into_iter().map(customer::CustomerAccountRef::try_from_uuid)
        .collect::<Result<Vec<_>, _>>().map_err(|_| PortError::Internal)?;
    let timeline = sqlx::query("SELECT occurred_at,actor_ref,message FROM erpx_support_write.incident_timeline_authority WHERE incident_id=$1 ORDER BY timeline_ordinal")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_| PortError::Unavailable)?
        .into_iter().map(|entry| Ok(IncidentTimelineEntry {
            at: entry.try_get("occurred_at").map_err(|_| PortError::Internal)?,
            actor_ref: entry.try_get("actor_ref").map_err(|_| PortError::Internal)?,
            message: entry.try_get("message").map_err(|_| PortError::Internal)?,
        })).collect::<Result<Vec<_>, PortError>>()?;
    let broadcasts = sqlx::query("SELECT occurred_at,actor_ref,audience,message FROM erpx_support_write.incident_broadcast_authority WHERE incident_id=$1 ORDER BY broadcast_ordinal")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_| PortError::Unavailable)?
        .into_iter().map(|entry| Ok(IncidentBroadcast {
            at: entry.try_get("occurred_at").map_err(|_| PortError::Internal)?,
            actor_ref: entry.try_get("actor_ref").map_err(|_| PortError::Internal)?,
            audience: parse_broadcast_audience(entry.try_get::<String, _>("audience").map_err(|_| PortError::Internal)?.as_str())?,
            message: entry.try_get("message").map_err(|_| PortError::Internal)?,
        })).collect::<Result<Vec<_>, PortError>>()?;
    Incident::try_rehydrate(
        IncidentId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        support_row_u64(&row, "aggregate_version")?,
        support_exact_scope(&row)?,
        row.try_get("incident_code")
            .map_err(|_| PortError::Internal)?,
        row.try_get("severity").map_err(|_| PortError::Internal)?,
        row.try_get("commander_ref")
            .map_err(|_| PortError::Internal)?,
        affected_services,
        affected_customer_accounts,
        parse_incident_status(
            row.try_get::<String, _>("incident_status")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )?,
        timeline,
        broadcasts,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_session_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<SupportSession>, PortError> {
    let row = support_authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,ticket_id,customer_id,customer_account_id,agent_principal_id,represented_principal_id,support_mode,module,reason,session_status,requested_at,expires_at,approved_by,approved_at,revoked_at,revoked_reason FROM erpx_support_write.session_authority WHERE session_id=$1",
        "erpx_support_write.session_authority",
        "session_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let permissions = sqlx::query_scalar::<_, String>("SELECT permission FROM erpx_support_write.session_permission_authority WHERE session_id=$1 ORDER BY permission_ordinal")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_| PortError::Unavailable)?;
    SupportSession::try_rehydrate(
        SessionId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        support_row_u64(&row, "aggregate_version")?,
        support_exact_scope(&row)?,
        SupportTicketId::try_from_uuid(row.try_get("ticket_id").map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
        customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer::CustomerAccountRef::try_from_uuid(
            row.try_get("customer_account_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        identity_access::PrincipalRef::try_from_uuid(
            row.try_get("agent_principal_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row.try_get::<Option<Uuid>, _>("represented_principal_id")
            .map_err(|_| PortError::Internal)?
            .map(identity_access::PrincipalRef::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        parse_support_mode(
            row.try_get::<String, _>("support_mode")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )?,
        permissions,
        row.try_get("module").map_err(|_| PortError::Internal)?,
        row.try_get("reason").map_err(|_| PortError::Internal)?,
        parse_session_status(
            row.try_get::<String, _>("session_status")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )?,
        row.try_get("requested_at")
            .map_err(|_| PortError::Internal)?,
        row.try_get("expires_at").map_err(|_| PortError::Internal)?,
        row.try_get("approved_by")
            .map_err(|_| PortError::Internal)?,
        row.try_get("approved_at")
            .map_err(|_| PortError::Internal)?,
        row.try_get("revoked_at").map_err(|_| PortError::Internal)?,
        row.try_get("revoked_reason")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_capability_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<SupportCapability>, PortError> {
    let row = support_authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,session_id,ticket_id,customer_id,customer_account_id,agent_principal_id,device_ref,support_mode,module,secret_hash_sha256,issued_at,expires_at,revocation_reason,revoked_at FROM erpx_support_write.capability_authority WHERE capability_id=$1",
        "erpx_support_write.capability_authority",
        "capability_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let permissions = sqlx::query_scalar::<_, String>("SELECT permission FROM erpx_support_write.capability_permission_authority WHERE capability_id=$1 ORDER BY permission_ordinal")
        .bind(id).fetch_all(&mut **tx).await.map_err(|_| PortError::Unavailable)?;
    let secret: Vec<u8> = row
        .try_get("secret_hash_sha256")
        .map_err(|_| PortError::Internal)?;
    let secret_hash_sha256: [u8; 32] = secret.try_into().map_err(|_| PortError::Internal)?;
    let reason = row
        .try_get::<Option<String>, _>("revocation_reason")
        .map_err(|_| PortError::Internal)?
        .map(|value| parse_capability_revocation(value.as_str()))
        .transpose()?;
    let revoked_at = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")
        .map_err(|_| PortError::Internal)?;
    let revocation = match (reason, revoked_at) {
        (None, None) => None,
        (Some(reason), Some(at)) => Some((reason, at)),
        _ => return Err(PortError::Internal),
    };
    SupportCapability::try_rehydrate(
        CapabilityId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        support_row_u64(&row, "aggregate_version")?,
        SessionId::try_from_uuid(row.try_get("session_id").map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
        SupportTicketId::try_from_uuid(row.try_get("ticket_id").map_err(|_| PortError::Internal)?)
            .map_err(|_| PortError::Internal)?,
        customer::CustomerRef::try_from_uuid(
            row.try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        customer::CustomerAccountRef::try_from_uuid(
            row.try_get("customer_account_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        identity_access::PrincipalRef::try_from_uuid(
            row.try_get("agent_principal_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row.try_get("device_ref").map_err(|_| PortError::Internal)?,
        parse_support_mode(
            row.try_get::<String, _>("support_mode")
                .map_err(|_| PortError::Internal)?
                .as_str(),
        )?,
        permissions,
        support_exact_scope(&row)?,
        row.try_get("module").map_err(|_| PortError::Internal)?,
        secret_hash_sha256,
        row.try_get("issued_at").map_err(|_| PortError::Internal)?,
        row.try_get("expires_at").map_err(|_| PortError::Internal)?,
        revocation,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn support_authorized_root(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    base: &str,
    table: &str,
    id_column: &str,
    id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, PortError> {
    let (kind, holding, company, branch, companies, branches) =
        support_authority_parameters(authority)?;
    let query = format!(
        "{base} AND ($2='platform' OR ($2='holding' AND holding_id=$3) OR ($2='company' AND holding_id=$3 AND company_id=$4) OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5) OR ($2='subtree' AND holding_id=$3 AND ((branch_id IS NULL AND cardinality($7::uuid[])=0 AND company_id=ANY($6::uuid[])) OR (branch_id IS NOT NULL AND company_id=ANY($6::uuid[]) AND branch_id=ANY($7::uuid[])))))"
    );
    let row = sqlx::query(&query)
        .bind(id)
        .bind(kind)
        .bind(holding)
        .bind(company)
        .bind(branch)
        .bind(companies)
        .bind(branches)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if row.is_some() {
        return Ok(row);
    }
    let exists_query = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {id_column}=$1)");
    let exists: bool = sqlx::query_scalar(&exists_query)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if exists {
        Err(PortError::Denied)
    } else {
        Ok(None)
    }
}

type SupportAuthorityParameters = (
    &'static str,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Vec<Uuid>,
    Vec<Uuid>,
);

fn support_authority_parameters(
    authority: &AuthorityScope,
) -> Result<SupportAuthorityParameters, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    Ok(match authority {
        AuthorityScope::Platform => ("platform", None, None, None, vec![], vec![]),
        AuthorityScope::Holding { holding_id } => {
            ("holding", Some(*holding_id), None, None, vec![], vec![])
        }
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => (
            "company",
            Some(*holding_id),
            Some(*company_id),
            None,
            vec![],
            vec![],
        ),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => (
            "branch",
            Some(*holding_id),
            Some(*company_id),
            Some(*branch_id),
            vec![],
            vec![],
        ),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => (
            "subtree",
            Some(*holding_id),
            None,
            None,
            vec![*organization_node_id],
            Vec::new(),
        ),
    })
}

fn support_exact_scope(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    Ok(
        match row.try_get("branch_id").map_err(|_| PortError::Internal)? {
            Some(branch_id) => AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            None => AuthorityScope::Company {
                holding_id,
                company_id,
            },
        },
    )
}

fn support_row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}

fn parse_ticket_status(value: &str) -> Result<TicketStatus, PortError> {
    match value {
        "new" => Ok(TicketStatus::New),
        "triaged" => Ok(TicketStatus::Triaged),
        "waiting_for_customer" => Ok(TicketStatus::WaitingForCustomer),
        "access_requested" => Ok(TicketStatus::AccessRequested),
        "approved" => Ok(TicketStatus::Approved),
        "in_progress" => Ok(TicketStatus::InProgress),
        "resolved" => Ok(TicketStatus::Resolved),
        "customer_confirmed" => Ok(TicketStatus::CustomerConfirmed),
        "closed" => Ok(TicketStatus::Closed),
        _ => Err(PortError::Internal),
    }
}

fn parse_incident_status(value: &str) -> Result<IncidentStatus, PortError> {
    match value {
        "investigating" => Ok(IncidentStatus::Investigating),
        "identified" => Ok(IncidentStatus::Identified),
        "monitoring" => Ok(IncidentStatus::Monitoring),
        "resolved" => Ok(IncidentStatus::Resolved),
        "postmortem_complete" => Ok(IncidentStatus::PostmortemComplete),
        _ => Err(PortError::Internal),
    }
}

fn parse_message_audience(value: &str) -> Result<MessageAudience, PortError> {
    match value {
        "customer_visible" => Ok(MessageAudience::CustomerVisible),
        "internal" => Ok(MessageAudience::Internal),
        _ => Err(PortError::Internal),
    }
}

fn parse_broadcast_audience(value: &str) -> Result<BroadcastAudience, PortError> {
    match value {
        "customer_visible" => Ok(BroadcastAudience::CustomerVisible),
        "internal" => Ok(BroadcastAudience::Internal),
        _ => Err(PortError::Internal),
    }
}

fn parse_support_mode(value: &str) -> Result<SupportMode, PortError> {
    match value {
        "diagnostic_read" => Ok(SupportMode::DiagnosticRead),
        "holding_mutation" => Ok(SupportMode::HoldingMutation),
        "impersonation" => Ok(SupportMode::Impersonation),
        _ => Err(PortError::Internal),
    }
}

fn parse_session_status(value: &str) -> Result<SessionStatus, PortError> {
    match value {
        "requested" => Ok(SessionStatus::Requested),
        "approved" => Ok(SessionStatus::Approved),
        "revoked" => Ok(SessionStatus::Revoked),
        _ => Err(PortError::Internal),
    }
}

fn parse_capability_revocation(value: &str) -> Result<CapabilityRevocationReason, PortError> {
    match value {
        "customer_requested" => Ok(CapabilityRevocationReason::CustomerRequested),
        "agent_disabled" => Ok(CapabilityRevocationReason::AgentDisabled),
        "device_revoked" => Ok(CapabilityRevocationReason::DeviceRevoked),
        "session_revoked" => Ok(CapabilityRevocationReason::SessionRevoked),
        "ticket_closed" => Ok(CapabilityRevocationReason::TicketClosed),
        "administrative" => Ok(CapabilityRevocationReason::Administrative),
        _ => Err(PortError::Internal),
    }
}

#[async_trait]
impl WriteRepository for SupportWriteRepository {
    async fn load_ticket(
        &self,
        scope: &AuthorityScope,
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
        scope: &AuthorityScope,
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
        scope: &AuthorityScope,
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
        scope: &AuthorityScope,
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
        let authority_effect_offset = events.len();
        events.extend(
            request
                .authority_effects
                .iter()
                .enumerate()
                .map(|(index, effect)| OutboxEventWrite {
                    event_id: deterministic_event_id(
                        command_id,
                        AUTHORITY_EFFECT_EVENT,
                        authority_effect_offset + index,
                    ),
                    event_type: AUTHORITY_EFFECT_EVENT.into(),
                    event_version: EVENT_VERSION,
                    partition: partition.clone(),
                    causation_id: request.causation_id,
                    occurred_at,
                    payload: SupportOutboxPayload::AuthorityEffect(effect.clone()),
                }),
        );
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: SupportOutboxPayload::Snapshot(SupportSnapshot::from(&request.aggregate)),
        });
        if let Some(directory_draft) = support_directory_draft(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: SupportOutboxPayload::Directory(directory_draft),
            });
        }
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
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
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
            .commit_normalized(command, &guard)
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

fn support_directory_draft(
    aggregate: &SupportAggregate,
) -> Result<Option<ItemDescriptorDraft>, PortError> {
    let (record, lifecycle_state, classification, search_text) = match aggregate {
        SupportAggregate::Ticket(value) => (
            value.record_ref().map_err(|_| PortError::Internal)?,
            match value.status() {
                erpx_support::TicketStatus::New => "new",
                erpx_support::TicketStatus::Triaged => "triaged",
                erpx_support::TicketStatus::WaitingForCustomer => "waiting-for-customer",
                erpx_support::TicketStatus::AccessRequested => "access-requested",
                erpx_support::TicketStatus::Approved => "approved",
                erpx_support::TicketStatus::InProgress => "in-progress",
                erpx_support::TicketStatus::Resolved => "resolved",
                erpx_support::TicketStatus::CustomerConfirmed => "customer-confirmed",
                erpx_support::TicketStatus::Closed => "closed",
            },
            DataClassification::Confidential,
            value.case_code().to_owned(),
        ),
        SupportAggregate::Incident(value) => (
            value.record_ref().map_err(|_| PortError::Internal)?,
            match value.status() {
                erpx_support::IncidentStatus::Investigating => "investigating",
                erpx_support::IncidentStatus::Identified => "identified",
                erpx_support::IncidentStatus::Monitoring => "monitoring",
                erpx_support::IncidentStatus::Resolved => "resolved",
                erpx_support::IncidentStatus::PostmortemComplete => "postmortem-complete",
            },
            DataClassification::Confidential,
            value.incident_code().to_owned(),
        ),
        SupportAggregate::Session(value) => (
            value.record_ref().map_err(|_| PortError::Internal)?,
            match value.status() {
                erpx_support::SessionStatus::Requested => "requested",
                erpx_support::SessionStatus::Approved => "approved",
                erpx_support::SessionStatus::Revoked => "revoked",
            },
            DataClassification::Restricted,
            value.module().to_owned(),
        ),
        // Capabilities are authorization internals of an access session, not a
        // first-class cataloged record. Revocation remains on the session item.
        SupportAggregate::Capability(_) => return Ok(None),
    };
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: lifecycle_state.into(),
        classification,
        descriptor_version: aggregate.version(),
        authorized_search_text: search_text,
    };
    value.validate().map_err(|_| PortError::Internal)?;
    Ok(Some(value))
}

struct SupportAtomicGuard {
    aggregate: SupportAggregate,
    dependency: Option<DependencyVersion>,
}

#[async_trait]
impl AtomicCommandGuard for SupportAtomicGuard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        guard_version(
            tx,
            self.aggregate.aggregate_type(),
            self.aggregate.id_uuid(),
        )
        .await
    }

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
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT ticket_id FROM erpx_support_write.ticket_authority WHERE scope_key=$1 AND case_code=$2 FOR UPDATE").bind(scope_key(value.scope())?).bind(value.case_code()).fetch_optional(&mut **tx).await?;
                if existing.is_some_and(|id| id != value.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support case code already exists",
                    ));
                }
            }
            SupportAggregate::Session(value) => {
                let row = sqlx::query("SELECT customer_id,customer_account_id,holding_id,company_id,branch_id,ticket_status FROM erpx_support_write.ticket_authority WHERE ticket_id=$1 FOR SHARE").bind(value.ticket_id().as_uuid()).fetch_optional(&mut **tx).await?;
                let Some(row) = row else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support ticket guard is missing",
                    ));
                };
                if row.try_get::<Uuid, _>("customer_id")? != value.customer().as_uuid()
                    || row.try_get::<Uuid, _>("customer_account_id")?
                        != value.customer_account().as_uuid()
                    || !support_scope_matches_row(&row, value.scope())?
                    || row.try_get::<String, _>("ticket_status")? != "access_requested"
                {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support session ticket binding is invalid",
                    ));
                }
            }
            SupportAggregate::Capability(value) => {
                let row = sqlx::query("SELECT ticket_id,customer_id,customer_account_id,agent_principal_id,support_mode,module,holding_id,company_id,branch_id,session_status,expires_at FROM erpx_support_write.session_authority WHERE session_id=$1 FOR SHARE").bind(value.session_id().as_uuid()).fetch_optional(&mut **tx).await?;
                let Some(row) = row else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support session guard is missing",
                    ));
                };
                if row.try_get::<String, _>("session_status")? != "approved"
                    || row.try_get::<chrono::DateTime<chrono::Utc>, _>("expires_at")?
                        != value.expires_at()
                    || row.try_get::<Uuid, _>("ticket_id")? != value.ticket_id().as_uuid()
                    || row.try_get::<Uuid, _>("customer_id")? != value.customer().as_uuid()
                    || row.try_get::<Uuid, _>("customer_account_id")?
                        != value.customer_account().as_uuid()
                    || row.try_get::<Uuid, _>("agent_principal_id")? != value.agent().as_uuid()
                    || row.try_get::<String, _>("support_mode")? != support_mode_name(value.mode())
                    || row.try_get::<String, _>("module")? != value.module()
                    || !support_scope_matches_row(&row, value.scope())?
                {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support capability session is inactive",
                    ));
                }
                let permissions = sqlx::query_scalar::<_, String>("SELECT permission FROM erpx_support_write.session_permission_authority WHERE session_id=$1 ORDER BY permission_ordinal FOR SHARE")
                    .bind(value.session_id().as_uuid()).fetch_all(&mut **tx).await?;
                if permissions != value.permissions() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Support capability permissions differ from the session",
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
        match &self.aggregate {
            SupportAggregate::Ticket(value) => apply_ticket(tx, value).await?,
            SupportAggregate::Incident(value) => apply_incident(tx, value).await?,
            SupportAggregate::Session(value) => apply_session(tx, value).await?,
            SupportAggregate::Capability(value) => apply_capability(tx, value).await?,
        }
        Ok(())
    }
}

async fn apply_ticket(
    tx: &mut Transaction<'_, Postgres>,
    value: &SupportTicket,
) -> Result<(), CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = support_scope_columns(value.scope())?;
    let changed = sqlx::query("INSERT INTO erpx_support_write.ticket_authority (ticket_id,aggregate_version,scope_key,holding_id,company_id,branch_id,customer_id,customer_account_id,case_code,category,severity,description,ticket_status,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) ON CONFLICT (holding_id,ticket_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,case_code=EXCLUDED.case_code,category=EXCLUDED.category,severity=EXCLUDED.severity,description=EXCLUDED.description,ticket_status=EXCLUDED.ticket_status,updated_at=EXCLUDED.updated_at WHERE erpx_support_write.ticket_authority.aggregate_version + 1=EXCLUDED.aggregate_version")
        .bind(value.id().as_uuid()).bind(to_i64(value.version())?).bind(scope_key(value.scope())?).bind(holding_id).bind(company_id).bind(branch_id)
        .bind(value.customer().as_uuid()).bind(value.customer_account().as_uuid()).bind(value.case_code()).bind(value.category()).bind(value.severity()).bind(value.description())
        .bind(ticket_status_name(value.status())).bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?.rows_affected();
    ensure_support_root_changed(changed)?;
    sqlx::query("DELETE FROM erpx_support_write.ticket_message_authority WHERE ticket_id=$1")
        .bind(value.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    for (index, message) in value.messages().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_support_write.ticket_message_authority (ticket_id,message_ordinal,message_id,author_ref,audience,body,created_at,ticket_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(value.id().as_uuid()).bind(to_i32(index)?).bind(message.id().as_uuid()).bind(message.author_ref()).bind(message_audience_name(message.audience())).bind(message.body()).bind(message.created_at()).bind(to_i64(value.version())?).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn apply_incident(
    tx: &mut Transaction<'_, Postgres>,
    value: &Incident,
) -> Result<(), CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = support_scope_columns(value.scope())?;
    let changed = sqlx::query("INSERT INTO erpx_support_write.incident_authority (incident_id,aggregate_version,scope_key,holding_id,company_id,branch_id,incident_code,severity,commander_ref,incident_status,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT (holding_id,incident_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,incident_code=EXCLUDED.incident_code,severity=EXCLUDED.severity,commander_ref=EXCLUDED.commander_ref,incident_status=EXCLUDED.incident_status,updated_at=EXCLUDED.updated_at WHERE erpx_support_write.incident_authority.aggregate_version + 1=EXCLUDED.aggregate_version")
        .bind(value.id().as_uuid()).bind(to_i64(value.version())?).bind(scope_key(value.scope())?).bind(holding_id).bind(company_id).bind(branch_id)
        .bind(value.incident_code()).bind(value.severity()).bind(value.commander_ref()).bind(incident_status_name(value.status())).bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?.rows_affected();
    ensure_support_root_changed(changed)?;
    sqlx::query("DELETE FROM erpx_support_write.incident_service_authority WHERE incident_id=$1")
        .bind(value.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "DELETE FROM erpx_support_write.incident_customer_account_authority WHERE incident_id=$1",
    )
    .bind(value.id().as_uuid())
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM erpx_support_write.incident_timeline_authority WHERE incident_id=$1")
        .bind(value.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM erpx_support_write.incident_broadcast_authority WHERE incident_id=$1")
        .bind(value.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    for (index, service) in value.affected_services().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_support_write.incident_service_authority (incident_id,service_ordinal,service_ref,incident_version) VALUES ($1,$2,$3,$4)")
            .bind(value.id().as_uuid()).bind(to_i32(index)?).bind(service).bind(to_i64(value.version())?).execute(&mut **tx).await?;
    }
    for (index, account) in value.affected_customer_accounts().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_support_write.incident_customer_account_authority (incident_id,account_ordinal,customer_account_id,incident_version) VALUES ($1,$2,$3,$4)")
            .bind(value.id().as_uuid()).bind(to_i32(index)?).bind(account.as_uuid()).bind(to_i64(value.version())?).execute(&mut **tx).await?;
    }
    for (index, entry) in value.timeline().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_support_write.incident_timeline_authority (incident_id,timeline_ordinal,occurred_at,actor_ref,message,incident_version) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(value.id().as_uuid()).bind(to_i32(index)?).bind(entry.at).bind(&entry.actor_ref).bind(&entry.message).bind(to_i64(value.version())?).execute(&mut **tx).await?;
    }
    for (index, broadcast) in value.broadcasts().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_support_write.incident_broadcast_authority (incident_id,broadcast_ordinal,occurred_at,actor_ref,audience,message,incident_version) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(value.id().as_uuid()).bind(to_i32(index)?).bind(broadcast.at).bind(&broadcast.actor_ref).bind(broadcast_audience_name(broadcast.audience)).bind(&broadcast.message).bind(to_i64(value.version())?).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn apply_session(
    tx: &mut Transaction<'_, Postgres>,
    value: &SupportSession,
) -> Result<(), CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = support_scope_columns(value.scope())?;
    let changed = sqlx::query("INSERT INTO erpx_support_write.session_authority (session_id,aggregate_version,scope_key,holding_id,company_id,branch_id,ticket_id,customer_id,customer_account_id,agent_principal_id,represented_principal_id,support_mode,module,reason,session_status,requested_at,expires_at,approved_by,approved_at,revoked_at,revoked_reason) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21) ON CONFLICT (holding_id,session_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,ticket_id=EXCLUDED.ticket_id,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,agent_principal_id=EXCLUDED.agent_principal_id,represented_principal_id=EXCLUDED.represented_principal_id,support_mode=EXCLUDED.support_mode,module=EXCLUDED.module,reason=EXCLUDED.reason,session_status=EXCLUDED.session_status,expires_at=EXCLUDED.expires_at,approved_by=EXCLUDED.approved_by,approved_at=EXCLUDED.approved_at,revoked_at=EXCLUDED.revoked_at,revoked_reason=EXCLUDED.revoked_reason WHERE erpx_support_write.session_authority.aggregate_version + 1=EXCLUDED.aggregate_version")
        .bind(value.id().as_uuid()).bind(to_i64(value.version())?).bind(scope_key(value.scope())?).bind(holding_id).bind(company_id).bind(branch_id)
        .bind(value.ticket_id().as_uuid()).bind(value.customer().as_uuid()).bind(value.customer_account().as_uuid()).bind(value.agent().as_uuid()).bind(value.represented_principal().map(|id| id.as_uuid()))
        .bind(support_mode_name(value.mode())).bind(value.module()).bind(value.reason()).bind(session_status_name(value.status())).bind(value.requested_at()).bind(value.expires_at())
        .bind(value.approved_by()).bind(value.approved_at()).bind(value.revoked_at()).bind(value.revoked_reason()).execute(&mut **tx).await?.rows_affected();
    ensure_support_root_changed(changed)?;
    sqlx::query("DELETE FROM erpx_support_write.session_permission_authority WHERE session_id=$1")
        .bind(value.id().as_uuid())
        .execute(&mut **tx)
        .await?;
    for (index, permission) in value.permissions().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_support_write.session_permission_authority (session_id,permission_ordinal,permission,session_version) VALUES ($1,$2,$3,$4)")
            .bind(value.id().as_uuid()).bind(to_i32(index)?).bind(permission).bind(to_i64(value.version())?).execute(&mut **tx).await?;
    }
    Ok(())
}

async fn apply_capability(
    tx: &mut Transaction<'_, Postgres>,
    value: &SupportCapability,
) -> Result<(), CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = support_scope_columns(value.scope())?;
    let (reason, revoked_at) = value
        .revocation()
        .map(|(reason, at)| (Some(capability_revocation_name(reason)), Some(at)))
        .unwrap_or((None, None));
    let changed = sqlx::query("INSERT INTO erpx_support_write.capability_authority (capability_id,aggregate_version,scope_key,holding_id,company_id,branch_id,session_id,ticket_id,customer_id,customer_account_id,agent_principal_id,device_ref,support_mode,module,secret_hash_sha256,issued_at,expires_at,revocation_reason,revoked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19) ON CONFLICT (holding_id,capability_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,session_id=EXCLUDED.session_id,ticket_id=EXCLUDED.ticket_id,customer_id=EXCLUDED.customer_id,customer_account_id=EXCLUDED.customer_account_id,agent_principal_id=EXCLUDED.agent_principal_id,device_ref=EXCLUDED.device_ref,support_mode=EXCLUDED.support_mode,module=EXCLUDED.module,secret_hash_sha256=EXCLUDED.secret_hash_sha256,expires_at=EXCLUDED.expires_at,revocation_reason=EXCLUDED.revocation_reason,revoked_at=EXCLUDED.revoked_at WHERE erpx_support_write.capability_authority.aggregate_version + 1=EXCLUDED.aggregate_version")
        .bind(value.id().as_uuid()).bind(to_i64(value.version())?).bind(scope_key(value.scope())?).bind(holding_id).bind(company_id).bind(branch_id)
        .bind(value.session_id().as_uuid()).bind(value.ticket_id().as_uuid()).bind(value.customer().as_uuid()).bind(value.customer_account().as_uuid()).bind(value.agent().as_uuid())
        .bind(value.device_ref()).bind(support_mode_name(value.mode())).bind(value.module()).bind(value.secret_hash_sha256().as_slice()).bind(value.issued_at()).bind(value.expires_at()).bind(reason).bind(revoked_at)
        .execute(&mut **tx).await?.rows_affected();
    ensure_support_root_changed(changed)?;
    sqlx::query(
        "DELETE FROM erpx_support_write.capability_permission_authority WHERE capability_id=$1",
    )
    .bind(value.id().as_uuid())
    .execute(&mut **tx)
    .await?;
    for (index, permission) in value.permissions().iter().enumerate() {
        sqlx::query("INSERT INTO erpx_support_write.capability_permission_authority (capability_id,permission_ordinal,permission,capability_version) VALUES ($1,$2,$3,$4)")
            .bind(value.id().as_uuid()).bind(to_i32(index)?).bind(permission).bind(to_i64(value.version())?).execute(&mut **tx).await?;
    }
    Ok(())
}

fn support_scope_columns(
    scope: &AuthorityScope,
) -> Result<(Uuid, Uuid, Option<Uuid>), CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(CqrsSqlxError::GuardRejected(
            "Support aggregate scope must be exact",
        )),
    }
}

fn support_scope_matches_row(
    row: &sqlx::postgres::PgRow,
    scope: &AuthorityScope,
) -> Result<bool, CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = support_scope_columns(scope)?;
    Ok(row.try_get::<Uuid, _>("holding_id")? == holding_id
        && row.try_get::<Uuid, _>("company_id")? == company_id
        && row.try_get::<Option<Uuid>, _>("branch_id")? == branch_id)
}

fn ensure_support_root_changed(changed: u64) -> Result<(), CqrsSqlxError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(
            "Support authority version must advance by exactly one",
        ))
    }
}

fn to_i32(value: usize) -> Result<i32, CqrsSqlxError> {
    i32::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

const fn broadcast_audience_name(value: BroadcastAudience) -> &'static str {
    match value {
        BroadcastAudience::CustomerVisible => "customer_visible",
        BroadcastAudience::Internal => "internal",
    }
}

async fn guard_version(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    id: Uuid,
) -> Result<Option<u64>, CqrsSqlxError> {
    let statement = match aggregate_type {
        TICKET_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.ticket_authority WHERE ticket_id=$1 FOR UPDATE"
        }
        INCIDENT_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.incident_authority WHERE incident_id=$1 FOR UPDATE"
        }
        SESSION_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.session_authority WHERE session_id=$1 FOR UPDATE"
        }
        CAPABILITY_AGGREGATE_TYPE => {
            "SELECT aggregate_version FROM erpx_support_write.capability_authority WHERE capability_id=$1 FOR UPDATE"
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
        || !valid_authority_effects(request)
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}

fn valid_authority_effects(request: &SupportCommit) -> bool {
    match (&request.identity.fingerprint, &request.aggregate) {
        (
            CommandFingerprint::TransitionTicket { ticket_id, next },
            SupportAggregate::Ticket(ticket),
        ) if *next == TicketStatus::Closed && ticket.status() == TicketStatus::Closed => matches!(
            request.authority_effects.as_slice(),
            [SupportAuthorityEffect::TerminateTicketAccess {
                ticket_id: effect_ticket,
                effective_at,
            }] if effect_ticket == ticket_id && *effective_at == ticket.updated_at()
        ),
        (
            CommandFingerprint::RevokeSession { session_id, .. },
            SupportAggregate::Session(session),
        ) if session.status() == SessionStatus::Revoked => matches!(
            request.authority_effects.as_slice(),
            [SupportAuthorityEffect::TerminateSessionCapabilities {
                session_id: effect_session,
                effective_at,
            }] if effect_session == session_id && session.revoked_at() == Some(*effective_at)
        ),
        _ => request.authority_effects.is_empty(),
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
    d.update(b"erpx.support.command\0");
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
pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        AuthorityScope::Branch {
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
