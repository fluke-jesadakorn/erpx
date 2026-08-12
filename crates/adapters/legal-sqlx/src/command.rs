use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use erpx_legal::{
    CommandUnitOfWork, LegalAggregate, LegalCommit, LegalContract, LegalContractId, LegalDocument,
    LegalDocumentId, LegalMatter, LegalMatterId, PortError, WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    ACCESS_AGGREGATE_TYPE, ACCESS_SNAPSHOT_EVENT, CONTEXT_NAME, CONTRACT_AGGREGATE_TYPE,
    CONTRACT_SNAPSHOT_EVENT, DOCUMENT_AGGREGATE_TYPE, DOCUMENT_SNAPSHOT_EVENT, EVENT_VERSION,
    MATTER_AGGREGATE_TYPE, MATTER_SNAPSHOT_EVENT, access_partition, contract_partition,
    document_partition, matter_partition,
    projection::{LegalOutboxPayload, LegalSnapshot, contract_status_name, matter_status_name},
};

#[derive(Debug, Clone)]
pub struct LegalWriteRepository {
    store: CommandStore,
}

impl LegalWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }

    async fn load_aggregate(
        &self,
        authority: &TenantScope,
        partition: &str,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<LegalAggregate>, PortError> {
        let aggregate_id = AggregateId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
        let snapshot = self
            .store
            .load_snapshot::<LegalAggregate>(CONTEXT_NAME, partition, aggregate_type, aggregate_id)
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id_uuid() != id
            || snapshot.state.aggregate_type() != aggregate_type
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        if !authority.contains(snapshot.state.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }
}

#[async_trait]
impl WriteRepository for LegalWriteRepository {
    async fn load_contract(
        &self,
        authority: &TenantScope,
        id: LegalContractId,
    ) -> Result<Option<LegalContract>, PortError> {
        match self
            .load_aggregate(
                authority,
                &contract_partition(id),
                CONTRACT_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(LegalAggregate::Contract(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_document(
        &self,
        authority: &TenantScope,
        id: LegalDocumentId,
    ) -> Result<Option<LegalDocument>, PortError> {
        match self
            .load_aggregate(
                authority,
                &document_partition(id),
                DOCUMENT_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(LegalAggregate::Document(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_matter(
        &self,
        authority: &TenantScope,
        id: LegalMatterId,
    ) -> Result<Option<LegalMatter>, PortError> {
        match self
            .load_aggregate(
                authority,
                &matter_partition(id),
                MATTER_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(LegalAggregate::Matter(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegalCommandAdapter {
    store: CommandStore,
}
impl LegalCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for LegalCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, aggregate_type) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<LegalAggregate>(
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

    async fn commit(&self, request: LegalCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let aggregate_type = request.aggregate.aggregate_type();
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
                payload: LegalOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        let snapshot_event = snapshot_event(&request.aggregate);
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: LegalOutboxPayload::Snapshot(LegalSnapshot::from(&request.aggregate)),
        });
        let guard = LegalAtomicGuard {
            aggregate: request.aggregate.clone(),
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
                audit_id: deterministic_uuid(command_id.as_uuid(), b"legal-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"aggregate_id": aggregate_id.as_uuid(), "aggregate_type": aggregate_type, "expected_version": request.expected_version}),
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

struct LegalAtomicGuard {
    aggregate: LegalAggregate,
}

#[async_trait]
impl AtomicCommandGuard for LegalAtomicGuard {
    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current = guard_version(tx, &self.aggregate).await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "Legal aggregate already exists",
                ));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "Legal aggregate guard is missing",
                ));
            }
            _ => {}
        }
        match &self.aggregate {
            LegalAggregate::Contract(value) if context.expected_version.is_none() => {
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT contract_id FROM erpx_legal_write.contract_guards WHERE scope_key=$1 AND reference=$2 FOR UPDATE")
                    .bind(scope_key(value.scope())?).bind(value.reference()).fetch_optional(&mut **tx).await?;
                if existing.is_some_and(|id| id != value.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal contract reference already exists",
                    ));
                }
            }
            LegalAggregate::Matter(value) if context.expected_version.is_none() => {
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT matter_id FROM erpx_legal_write.matter_guards WHERE scope_key=$1 AND reference=$2 FOR UPDATE")
                    .bind(scope_key(value.scope())?).bind(value.reference()).fetch_optional(&mut **tx).await?;
                if existing.is_some_and(|id| id != value.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal matter reference already exists",
                    ));
                }
                validate_existing_links(tx, value).await?;
            }
            LegalAggregate::Document(value) => validate_existing_versions(tx, value).await?,
            LegalAggregate::AccessEntry(value) => {
                let row = sqlx::query("SELECT d.scope_key FROM erpx_legal_write.document_guards d JOIN erpx_legal_write.document_version_guards v ON v.document_id=d.document_id WHERE d.document_id=$1 AND v.document_version=$2 FOR SHARE")
                    .bind(value.document_id().as_uuid()).bind(to_i64(value.document_version())?)
                    .fetch_optional(&mut **tx).await?;
                let Some(row) = row else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal document version is missing",
                    ));
                };
                let document_scope: String = row.try_get("scope_key")?;
                if document_scope != scope_key(value.scope())? {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal access document is cross-scope",
                    ));
                }
            }
            LegalAggregate::Matter(value) => validate_existing_links(tx, value).await?,
            LegalAggregate::Contract(_) => {}
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
                "Legal applied guard metadata mismatch",
            ));
        }
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            LegalAggregate::Contract(value) => {
                sqlx::query("INSERT INTO erpx_legal_write.contract_guards (contract_id,scope_key,reference,title,object_ref,content_sha256,retention_until,contract_status,created_by,approved_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (contract_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,reference=EXCLUDED.reference,title=EXCLUDED.title,object_ref=EXCLUDED.object_ref,content_sha256=EXCLUDED.content_sha256,retention_until=EXCLUDED.retention_until,contract_status=EXCLUDED.contract_status,created_by=EXCLUDED.created_by,approved_by=EXCLUDED.approved_by,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.reference()).bind(value.title())
                    .bind(value.object_ref()).bind(value.content_sha256()).bind(value.retention_until())
                    .bind(contract_status_name(value.status())).bind(value.created_by()).bind(value.approved_by())
                    .bind(version).bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?;
                sqlx::query(
                    "DELETE FROM erpx_legal_write.contract_party_guards WHERE contract_id=$1",
                )
                .bind(value.id().as_uuid())
                .execute(&mut **tx)
                .await?;
                for party in value.parties() {
                    sqlx::query("INSERT INTO erpx_legal_write.contract_party_guards (contract_id,party_id,role) VALUES ($1,$2,$3)")
                        .bind(value.id().as_uuid()).bind(party.party_id).bind(&party.role).execute(&mut **tx).await?;
                }
            }
            LegalAggregate::Document(value) => {
                sqlx::query("INSERT INTO erpx_legal_write.document_guards (document_id,scope_key,title,current_document_version,archived,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (document_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,title=EXCLUDED.title,current_document_version=EXCLUDED.current_document_version,archived=EXCLUDED.archived,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.title())
                    .bind(to_i64(value.current_version())?).bind(value.archived()).bind(version)
                    .bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?;
                let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_legal_write.document_version_guards WHERE document_id=$1")
                    .bind(value.id().as_uuid()).fetch_one(&mut **tx).await?;
                let existing =
                    usize::try_from(existing).map_err(|_| CqrsSqlxError::NumericRange)?;
                for child in value.versions().iter().skip(existing) {
                    sqlx::query("INSERT INTO erpx_legal_write.document_version_guards (document_id,document_version,object_ref,content_sha256,created_at,created_by) VALUES ($1,$2,$3,$4,$5,$6)")
                        .bind(value.id().as_uuid()).bind(to_i64(child.version)?).bind(&child.object_ref)
                        .bind(&child.content_sha256).bind(child.created_at).bind(&child.created_by)
                        .execute(&mut **tx).await?;
                }
            }
            LegalAggregate::AccessEntry(value) => {
                sqlx::query("INSERT INTO erpx_legal_write.access_entry_guards (access_entry_id,scope_key,document_id,document_version,actor_ref,action,purpose,observed_at,aggregate_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.document_id().as_uuid())
                    .bind(to_i64(value.document_version())?).bind(value.actor_ref()).bind(value.action())
                    .bind(value.purpose()).bind(value.observed_at()).bind(version).execute(&mut **tx).await?;
            }
            LegalAggregate::Matter(value) => {
                sqlx::query("INSERT INTO erpx_legal_write.matter_guards (matter_id,scope_key,reference,title,matter_status,legal_hold,created_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (matter_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,reference=EXCLUDED.reference,title=EXCLUDED.title,matter_status=EXCLUDED.matter_status,legal_hold=EXCLUDED.legal_hold,created_by=EXCLUDED.created_by,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.reference()).bind(value.title())
                    .bind(matter_status_name(value.status())).bind(serde_json::to_value(value.legal_hold()).map_err(|_| CqrsSqlxError::InvalidInput("Legal hold cannot be encoded"))?)
                    .bind(value.created_by()).bind(version).bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?;
                let existing: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM erpx_legal_write.matter_link_guards WHERE matter_id=$1",
                )
                .bind(value.id().as_uuid())
                .fetch_one(&mut **tx)
                .await?;
                let existing =
                    usize::try_from(existing).map_err(|_| CqrsSqlxError::NumericRange)?;
                for (ordinal, link) in value.links().iter().enumerate().skip(existing) {
                    sqlx::query("INSERT INTO erpx_legal_write.matter_link_guards (matter_id,link_ordinal,relation,target_context,target_aggregate_kind,target_aggregate_id,target_aggregate_version,target_scope,target_scope_key,target_digest,linked_by,linked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
                        .bind(value.id().as_uuid()).bind(to_i64(u64::try_from(ordinal + 1).map_err(|_| CqrsSqlxError::NumericRange)?)?)
                        .bind(record_relation_name(link.relation)).bind(bounded_context_name(link.target.context)?)
                        .bind(link.target.aggregate_kind.as_str()).bind(link.target.aggregate_id).bind(to_i64(link.target.aggregate_version)?)
                        .bind(serde_json::to_value(&link.target_scope).map_err(|_| CqrsSqlxError::InvalidInput("target scope cannot be encoded"))?)
                        .bind(scope_key(&link.target_scope)?).bind(&link.target_digest).bind(&link.linked_by).bind(link.linked_at)
                        .execute(&mut **tx).await?;
                }
            }
        }
        Ok(())
    }
}

async fn validate_existing_versions(
    tx: &mut Transaction<'_, Postgres>,
    value: &LegalDocument,
) -> Result<(), CqrsSqlxError> {
    let rows = sqlx::query("SELECT document_version,object_ref,content_sha256,created_at,created_by FROM erpx_legal_write.document_version_guards WHERE document_id=$1 ORDER BY document_version FOR SHARE")
        .bind(value.id().as_uuid()).fetch_all(&mut **tx).await?;
    if rows.len() > value.versions().len() {
        return Err(CqrsSqlxError::GuardRejected(
            "Legal document versions cannot be removed",
        ));
    }
    for (row, expected) in rows.iter().zip(value.versions()) {
        let version: i64 = row.try_get("document_version")?;
        let object_ref: String = row.try_get("object_ref")?;
        let digest: String = row.try_get("content_sha256")?;
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at")?;
        let created_by: String = row.try_get("created_by")?;
        if u64::try_from(version).ok() != Some(expected.version)
            || object_ref != expected.object_ref
            || digest != expected.content_sha256
            || created_at.timestamp_micros() != expected.created_at.timestamp_micros()
            || created_by != expected.created_by
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Legal document version evidence changed",
            ));
        }
    }
    Ok(())
}

async fn validate_existing_links(
    tx: &mut Transaction<'_, Postgres>,
    value: &LegalMatter,
) -> Result<(), CqrsSqlxError> {
    let rows = sqlx::query("SELECT relation,target_context,target_aggregate_kind,target_aggregate_id,target_aggregate_version,target_scope,target_digest,linked_by,linked_at FROM erpx_legal_write.matter_link_guards WHERE matter_id=$1 ORDER BY link_ordinal FOR SHARE")
        .bind(value.id().as_uuid()).fetch_all(&mut **tx).await?;
    if rows.len() > value.links().len() {
        return Err(CqrsSqlxError::GuardRejected(
            "Legal matter links cannot be removed",
        ));
    }
    for (row, expected) in rows.iter().zip(value.links()) {
        let relation: String = row.try_get("relation")?;
        let target_context: String = row.try_get("target_context")?;
        let target_kind: String = row.try_get("target_aggregate_kind")?;
        let target_id: Uuid = row.try_get("target_aggregate_id")?;
        let target_version: i64 = row.try_get("target_aggregate_version")?;
        let target_scope: serde_json::Value = row.try_get("target_scope")?;
        let target_digest: String = row.try_get("target_digest")?;
        let linked_by: String = row.try_get("linked_by")?;
        let linked_at: chrono::DateTime<chrono::Utc> = row.try_get("linked_at")?;
        if relation != record_relation_name(expected.relation)
            || target_context != bounded_context_name(expected.target.context)?
            || target_kind != expected.target.aggregate_kind.as_str()
            || target_id != expected.target.aggregate_id
            || u64::try_from(target_version).ok() != Some(expected.target.aggregate_version)
            || target_scope
                != serde_json::to_value(&expected.target_scope)
                    .map_err(|_| CqrsSqlxError::InvalidInput("target scope cannot be encoded"))?
            || target_digest != expected.target_digest
            || linked_by != expected.linked_by
            || linked_at.timestamp_micros() != expected.linked_at.timestamp_micros()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Legal matter link evidence changed",
            ));
        }
    }
    Ok(())
}

async fn guard_version(
    tx: &mut Transaction<'_, Postgres>,
    value: &LegalAggregate,
) -> Result<Option<i64>, CqrsSqlxError> {
    let (sql, id) = match value {
        LegalAggregate::Contract(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.contract_guards WHERE contract_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
        LegalAggregate::Document(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.document_guards WHERE document_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
        LegalAggregate::AccessEntry(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.access_entry_guards WHERE access_entry_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
        LegalAggregate::Matter(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.matter_guards WHERE matter_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
    };
    Ok(sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?)
}

fn validate_guard_context(
    value: &LegalAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != value.aggregate_type()
        || context.aggregate_id.as_uuid() != value.id_uuid()
        || context.partition != aggregate_partition(value)
    {
        Err(CqrsSqlxError::GuardRejected(
            "Legal guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}
fn validate_commit(request: &LegalCommit) -> Result<(), PortError> {
    let (partition, aggregate_type) = fingerprint_target(&request.identity.fingerprint);
    if request.aggregate.validate().is_err()
        || partition != aggregate_partition(&request.aggregate)
        || aggregate_type != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}
fn fingerprint_target(value: &CommandFingerprint) -> (String, &'static str) {
    match value {
        CommandFingerprint::CreateContract { contract_id, .. }
        | CommandFingerprint::AddContractParty { contract_id, .. }
        | CommandFingerprint::TransitionContract { contract_id, .. } => {
            (contract_partition(*contract_id), CONTRACT_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateDocument { document_id, .. }
        | CommandFingerprint::AddDocumentVersion { document_id, .. }
        | CommandFingerprint::ArchiveDocument { document_id } => {
            (document_partition(*document_id), DOCUMENT_AGGREGATE_TYPE)
        }
        CommandFingerprint::RecordDocumentAccess {
            access_entry_id, ..
        } => (access_partition(*access_entry_id), ACCESS_AGGREGATE_TYPE),
        CommandFingerprint::CreateMatter { matter_id, .. }
        | CommandFingerprint::LinkMatterRecord { matter_id, .. }
        | CommandFingerprint::PlaceLegalHold { matter_id, .. }
        | CommandFingerprint::ReleaseLegalHold { matter_id }
        | CommandFingerprint::CloseMatter { matter_id } => {
            (matter_partition(*matter_id), MATTER_AGGREGATE_TYPE)
        }
    }
}
fn aggregate_partition(value: &LegalAggregate) -> String {
    match value {
        LegalAggregate::Contract(v) => contract_partition(v.id()),
        LegalAggregate::Document(v) => document_partition(v.id()),
        LegalAggregate::AccessEntry(v) => access_partition(v.id()),
        LegalAggregate::Matter(v) => matter_partition(v.id()),
    }
}
fn snapshot_event(value: &LegalAggregate) -> &'static str {
    match value {
        LegalAggregate::Contract(_) => CONTRACT_SNAPSHOT_EVENT,
        LegalAggregate::Document(_) => DOCUMENT_SNAPSHOT_EVENT,
        LegalAggregate::AccessEntry(_) => ACCESS_SNAPSHOT_EVENT,
        LegalAggregate::Matter(_) => MATTER_SNAPSHOT_EVENT,
    }
}
fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::CreateContract { .. } => "legal.contract.create",
        CommandFingerprint::AddContractParty { .. } => "legal.contract.add_party",
        CommandFingerprint::TransitionContract { .. } => "legal.contract.transition",
        CommandFingerprint::CreateDocument { .. } => "legal.document.create",
        CommandFingerprint::AddDocumentVersion { .. } => "legal.document.add_version",
        CommandFingerprint::ArchiveDocument { .. } => "legal.document.archive",
        CommandFingerprint::RecordDocumentAccess { .. } => "legal.access.record",
        CommandFingerprint::CreateMatter { .. } => "legal.matter.create",
        CommandFingerprint::LinkMatterRecord { .. } => "legal.matter.link_record",
        CommandFingerprint::PlaceLegalHold { .. } => "legal.matter.place_hold",
        CommandFingerprint::ReleaseLegalHold { .. } => "legal.matter.release_hold",
        CommandFingerprint::CloseMatter { .. } => "legal.matter.close",
    }
}

fn record_relation_name(value: erpx_legal::RecordRelation) -> &'static str {
    match value {
        erpx_legal::RecordRelation::GovernedBy => "governed_by",
        erpx_legal::RecordRelation::Supports => "supports",
        erpx_legal::RecordRelation::BilledUnder => "billed_under",
        erpx_legal::RecordRelation::AttributedTo => "attributed_to",
        erpx_legal::RecordRelation::Disputes => "disputes",
        erpx_legal::RecordRelation::SettlementFor => "settlement_for",
        erpx_legal::RecordRelation::Supersedes => "supersedes",
        erpx_legal::RecordRelation::AccountingFor => "accounting_for",
    }
}

fn bounded_context_name(
    value: erpx_integration_contracts::BoundedContext,
) -> Result<String, CqrsSqlxError> {
    let encoded = serde_json::to_value(value)
        .map_err(|_| CqrsSqlxError::InvalidInput("bounded context cannot be encoded"))?;
    encoded
        .as_str()
        .map(str::to_owned)
        .ok_or(CqrsSqlxError::InvalidInput(
            "bounded context must encode as text",
        ))
}
fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.legal.command.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn deterministic_event_id(command: CommandId, kind: &str, index: usize) -> EventId {
    let mut input = kind.as_bytes().to_vec();
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &input)).expect("derived event id")
}
fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
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
        _ => Err(CqrsSqlxError::InvalidInput("Legal scope must be exact")),
    }
}
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<LegalAggregate>,
    aggregate_type: &str,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != aggregate_type
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}
fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
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
    use erpx_legal::AccessEntryId;
    #[test]
    fn stable_hash_and_targets_cover_access_evidence() {
        let entry = AccessEntryId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let document = LegalDocumentId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        let value = CommandFingerprint::RecordDocumentAccess {
            access_entry_id: entry,
            document_id: document,
            document_version: 1,
            action: "download".into(),
            purpose: "review".into(),
            observed_at: chrono::Utc::now(),
        };
        assert_eq!(request_hash(&value).unwrap(), request_hash(&value).unwrap());
        assert_eq!(
            fingerprint_target(&value),
            (access_partition(entry), ACCESS_AGGREGATE_TYPE)
        );
    }

    #[test]
    fn append_only_sql_uses_insert_and_prefix_comparison_only() {
        let source = include_str!("command.rs");
        assert!(source.contains("Legal document version evidence changed"));
        assert!(source.contains("value.versions().iter().skip(existing)"));
        assert!(!source.contains(concat!(
            "DELETE FROM erpx_legal_write.document_",
            "version_guards"
        )));
    }
}
