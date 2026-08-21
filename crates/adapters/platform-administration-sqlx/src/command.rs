use std::collections::BTreeSet;

use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId,
    record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use erpx_platform_administration::{
    AdminContextId, AdminContextView, CommandFingerprint, CommandIdentity, CommandUnitOfWork,
    ConnectorBinding, ConnectorBindingId, ConnectorKind, ConnectorReadinessEvidence,
    ConnectorScope, ConnectorState, ConnectorView, EnvironmentId, HoldingAdminContext,
    HoldingLifecycleState, HoldingView, PlatformAdministrationCommit, PlatformAggregate,
    PlatformHolding, PlatformHoldingId, PortError, StoredCommit, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    ADMIN_CONTEXT_AGGREGATE, ADMIN_CONTEXT_SNAPSHOT_EVENT, CONNECTOR_AGGREGATE,
    CONNECTOR_SNAPSHOT_EVENT, CONTEXT_NAME, EVENT_VERSION, HOLDING_AGGREGATE,
    HOLDING_SNAPSHOT_EVENT, admin_context_partition, connector_partition, holding_partition,
    projection::{
        PlatformAdministrationOutboxPayload, PlatformAdministrationSnapshot, connector_kind,
        connector_state, holding_state, scope_key,
    },
};

#[derive(Debug, Clone)]
pub struct PlatformAdministrationWriteRepository {
    write: WritePool,
}

impl PlatformAdministrationWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { write: pool }
    }
}

#[async_trait]
impl WriteRepository for PlatformAdministrationWriteRepository {
    async fn load_holding(
        &self,
        authority: &AuthorityScope,
        id: PlatformHoldingId,
    ) -> Result<Option<PlatformHolding>, PortError> {
        load_holding_relational(&self.write, authority, id).await
    }

    async fn load_connector(
        &self,
        authority: &AuthorityScope,
        id: ConnectorBindingId,
    ) -> Result<Option<ConnectorBinding>, PortError> {
        load_connector_relational(&self.write, authority, id).await
    }

    async fn load_admin_context(
        &self,
        authority: &AuthorityScope,
        id: AdminContextId,
    ) -> Result<Option<erpx_platform_administration::HoldingAdminContext>, PortError> {
        load_admin_context_relational(&self.write, authority, id).await
    }
}

#[derive(Debug, Clone)]
pub struct PlatformAdministrationCommandAdapter {
    store: CommandStore,
}

impl PlatformAdministrationCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for PlatformAdministrationCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let key = fingerprint_key(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<PlatformAggregate>(
                CONTEXT_NAME,
                &key.partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(
        &self,
        request: PlatformAdministrationCommit,
    ) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let key = aggregate_key(&request.aggregate);
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let command_id = request.identity.command_id;
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: key.partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: PlatformAdministrationOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, key.snapshot_event, events.len()),
            event_type: key.snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: key.partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: PlatformAdministrationOutboxPayload::Snapshot(snapshot(
                &request.aggregate,
                occurred_at,
            )),
        });
        if let Some(directory_draft) = platform_directory_draft(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: key.partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: PlatformAdministrationOutboxPayload::Directory(directory_draft),
            });
        }
        let aggregate = request.aggregate;
        let guard = PlatformAdministrationGuard {
            aggregate: aggregate.clone(),
        };
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition: key.partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            active_scope: aggregate.scope(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: key.aggregate_type.into(),
            aggregate_id: aggregate_id(aggregate.id_uuid())?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(
                    command_id.as_uuid(),
                    b"platform-administration-audit-v1",
                ),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"aggregate_id": guard.aggregate.id_uuid(), "expected_version": request.expected_version}),
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
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn platform_directory_draft(
    aggregate: &PlatformAggregate,
) -> Result<Option<ItemDescriptorDraftV2>, PortError> {
    let (record_type, lifecycle_state, classification, display_label) = match aggregate {
        PlatformAggregate::Holding(value) => (
            "platform.holding",
            holding_state(value.status()),
            DataClassification::Confidential,
            format!("{} {}", value.code(), value.name()),
        ),
        PlatformAggregate::Connector(value) => (
            "platform.provider",
            connector_state(value.state()),
            DataClassification::Restricted,
            value.key().to_owned(),
        ),
        PlatformAggregate::AdminContext(_) => return Ok(None),
    };
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?,
        ContextId::PlatformAdministration,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let draft = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope(),
        lifecycle_state: lifecycle_state.into(),
        classification,
        descriptor_version: aggregate.version(),
        authorized_search_text: display_label.clone(),
    };
    ItemDescriptorDraftV2::try_new(draft, display_label)
        .map(Some)
        .map_err(|_| PortError::Internal)
}

#[derive(Debug)]
struct PlatformAdministrationGuard {
    aggregate: PlatformAggregate,
}

#[async_trait]
impl AtomicCommandGuard for PlatformAdministrationGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        let key = aggregate_key(&self.aggregate);
        if context.context != CONTEXT_NAME
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.aggregate_type != key.aggregate_type
            || context.partition != key.partition
        {
            return Err(CqrsSqlxError::GuardRejected(
                "platform administration guard metadata mismatch",
            ));
        }
        let query = match &self.aggregate {
            PlatformAggregate::Holding(_) => {
                "SELECT aggregate_version FROM erpx_platform_administration_write.holding_guards WHERE holding_id=$1 FOR UPDATE"
            }
            PlatformAggregate::Connector(_) => {
                "SELECT aggregate_version FROM erpx_platform_administration_write.connector_guards WHERE connector_id=$1 FOR UPDATE"
            }
            PlatformAggregate::AdminContext(_) => {
                "SELECT aggregate_version FROM erpx_platform_administration_write.admin_context_guards WHERE context_id=$1 FOR UPDATE"
            }
        };
        let value: Option<i64> = sqlx::query_scalar(query)
            .bind(self.aggregate.id_uuid())
            .fetch_optional(&mut **transaction)
            .await?;
        value.map(from_i64).transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        let key = aggregate_key(&self.aggregate);
        if context.context != CONTEXT_NAME
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.aggregate_type != key.aggregate_type
            || context.partition != key.partition
        {
            return Err(CqrsSqlxError::GuardRejected(
                "platform administration guard metadata mismatch",
            ));
        }
        match &self.aggregate {
            PlatformAggregate::Holding(value) => {
                let conflict: Option<Uuid> = sqlx::query_scalar("SELECT holding_id FROM erpx_platform_administration_write.holding_guards WHERE holding_id<>$1 AND (holding_code=$2 OR customer_id=$3 OR canonical_holding_id=$4 OR data_profile_key=$5) ORDER BY holding_id LIMIT 1 FOR UPDATE")
                    .bind(value.id().as_uuid()).bind(value.code()).bind(value.customer_id()).bind(value.canonical_holding_id()).bind(value.data_profile_key()).fetch_optional(&mut **transaction).await?;
                if conflict.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "platform holding binding is not unique",
                    ));
                }
            }
            PlatformAggregate::Connector(value) => {
                let conflict: Option<Uuid> = sqlx::query_scalar("SELECT connector_id FROM erpx_platform_administration_write.connector_guards WHERE connector_id<>$1 AND scope_key=$2 AND connector_key=$3 ORDER BY connector_id LIMIT 1 FOR UPDATE")
                    .bind(value.id().as_uuid()).bind(scope_key(&value.scope().active_scope())?).bind(value.key()).fetch_optional(&mut **transaction).await?;
                if conflict.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "platform connector key is not unique in scope",
                    ));
                }
            }
            PlatformAggregate::AdminContext(value) => {
                let holding_id = match value.scope() {
                    AuthorityScope::Holding { holding_id } => *holding_id,
                    _ => {
                        return Err(CqrsSqlxError::GuardRejected(
                            "admin context requires holding scope",
                        ));
                    }
                };
                let matches_holding: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM erpx_platform_administration_write.holding_guards WHERE holding_id=$1 AND canonical_holding_id=$2 AND lifecycle_state='active')")
                    .bind(value.holding_id().as_uuid()).bind(holding_id).fetch_one(&mut **transaction).await?;
                if !matches_holding {
                    return Err(CqrsSqlxError::GuardRejected(
                        "admin context holding binding is not active",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.aggregate_version != self.aggregate.version() {
            return Err(CqrsSqlxError::GuardRejected(
                "platform administration applied version mismatch",
            ));
        }
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            PlatformAggregate::Holding(value) => {
                sqlx::query("INSERT INTO erpx_platform_administration_write.holding_guards (holding_id,holding_code,holding_name,environment_id,customer_id,canonical_holding_id,data_profile_key,lifecycle_state,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT (holding_id) DO UPDATE SET holding_code=EXCLUDED.holding_code,holding_name=EXCLUDED.holding_name,environment_id=EXCLUDED.environment_id,customer_id=EXCLUDED.customer_id,canonical_holding_id=EXCLUDED.canonical_holding_id,data_profile_key=EXCLUDED.data_profile_key,lifecycle_state=EXCLUDED.lifecycle_state,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(value.code()).bind(value.name()).bind(value.environment_id().as_uuid()).bind(value.customer_id()).bind(value.canonical_holding_id()).bind(value.data_profile_key()).bind(holding_state(value.status())).bind(version).bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
            }
            PlatformAggregate::Connector(value) => {
                apply_connector_authority(transaction, value, version).await?;
            }
            PlatformAggregate::AdminContext(value) => {
                let holding_id = match value.scope() {
                    AuthorityScope::Holding { holding_id } => *holding_id,
                    _ => {
                        return Err(CqrsSqlxError::GuardRejected(
                            "admin context requires holding scope",
                        ));
                    }
                };
                sqlx::query("INSERT INTO erpx_platform_administration_write.admin_context_guards (context_id,holding_id,scope_key,holding_id,principal_ref,device_id,session_id,reason,issued_at,expires_at,revoked_at,aggregate_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT (context_id) DO UPDATE SET holding_id=EXCLUDED.holding_id,scope_key=EXCLUDED.scope_key,holding_id=EXCLUDED.holding_id,principal_ref=EXCLUDED.principal_ref,device_id=EXCLUDED.device_id,session_id=EXCLUDED.session_id,reason=EXCLUDED.reason,issued_at=EXCLUDED.issued_at,expires_at=EXCLUDED.expires_at,revoked_at=EXCLUDED.revoked_at,aggregate_version=EXCLUDED.aggregate_version")
                    .bind(value.id().as_uuid()).bind(value.holding_id().as_uuid()).bind(scope_key(value.scope())?).bind(holding_id).bind(value.principal_ref()).bind(value.device_id()).bind(value.session_id()).bind(value.reason()).bind(value.issued_at()).bind(value.expires_at()).bind(value.revoked_at()).bind(version).execute(&mut **transaction).await?;
            }
        }
        Ok(())
    }
}

struct AggregateKey {
    partition: String,
    aggregate_type: &'static str,
    snapshot_event: &'static str,
}

fn fingerprint_key(value: &CommandFingerprint) -> AggregateKey {
    match value {
        CommandFingerprint::RegisterHolding { holding_id, .. }
        | CommandFingerprint::TransitionHolding { holding_id, .. } => AggregateKey {
            partition: holding_partition(*holding_id),
            aggregate_type: HOLDING_AGGREGATE,
            snapshot_event: HOLDING_SNAPSHOT_EVENT,
        },
        CommandFingerprint::RegisterConnector { connector_id, .. }
        | CommandFingerprint::TransitionConnector { connector_id, .. }
        | CommandFingerprint::ObserveConnectorReadiness { connector_id, .. } => AggregateKey {
            partition: connector_partition(*connector_id),
            aggregate_type: CONNECTOR_AGGREGATE,
            snapshot_event: CONNECTOR_SNAPSHOT_EVENT,
        },
        CommandFingerprint::IssueAdminContext { context_id, .. }
        | CommandFingerprint::RevokeAdminContext { context_id } => AggregateKey {
            partition: admin_context_partition(*context_id),
            aggregate_type: ADMIN_CONTEXT_AGGREGATE,
            snapshot_event: ADMIN_CONTEXT_SNAPSHOT_EVENT,
        },
    }
}

fn aggregate_key(value: &PlatformAggregate) -> AggregateKey {
    match value {
        PlatformAggregate::Holding(value) => AggregateKey {
            partition: holding_partition(value.id()),
            aggregate_type: HOLDING_AGGREGATE,
            snapshot_event: HOLDING_SNAPSHOT_EVENT,
        },
        PlatformAggregate::Connector(value) => AggregateKey {
            partition: connector_partition(value.id()),
            aggregate_type: CONNECTOR_AGGREGATE,
            snapshot_event: CONNECTOR_SNAPSHOT_EVENT,
        },
        PlatformAggregate::AdminContext(value) => AggregateKey {
            partition: admin_context_partition(value.id()),
            aggregate_type: ADMIN_CONTEXT_AGGREGATE,
            snapshot_event: ADMIN_CONTEXT_SNAPSHOT_EVENT,
        },
    }
}

fn validate_commit(request: &PlatformAdministrationCommit) -> Result<(), PortError> {
    let fingerprint = fingerprint_key(&request.identity.fingerprint);
    let aggregate = aggregate_key(&request.aggregate);
    if fingerprint.partition != aggregate.partition
        || fingerprint.aggregate_type != aggregate.aggregate_type
        || !request.authority_scope.contains(&request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn snapshot(
    value: &PlatformAggregate,
    at: chrono::DateTime<chrono::Utc>,
) -> PlatformAdministrationSnapshot {
    match value {
        PlatformAggregate::Holding(value) => {
            PlatformAdministrationSnapshot::Holding(HoldingView::from(value))
        }
        PlatformAggregate::Connector(value) => {
            PlatformAdministrationSnapshot::Connector(ConnectorView::from(value))
        }
        PlatformAggregate::AdminContext(value) => {
            PlatformAdministrationSnapshot::AdminContext(AdminContextView::from_context(value, at))
        }
    }
}
fn aggregate_updated_at(value: &PlatformAggregate) -> chrono::DateTime<chrono::Utc> {
    match value {
        PlatformAggregate::Holding(v) => v.updated_at(),
        PlatformAggregate::Connector(v) => v.updated_at(),
        PlatformAggregate::AdminContext(v) => v.revoked_at().unwrap_or(v.issued_at()),
    }
}
fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<PlatformAggregate>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != receipt.response.aggregate_type()
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
        || receipt.aggregate.version != receipt.response.version()
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
fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.platform_administration.command-fingerprint.v1\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn aggregate_id(value: Uuid) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(value).map_err(|_| PortError::Internal)
}
fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::RegisterHolding { .. } => "platform_administration.holding.register",
        CommandFingerprint::TransitionHolding { .. } => {
            "platform_administration.holding.transition"
        }
        CommandFingerprint::RegisterConnector { .. } => {
            "platform_administration.connector.register"
        }
        CommandFingerprint::TransitionConnector { .. } => {
            "platform_administration.connector.transition"
        }
        CommandFingerprint::ObserveConnectorReadiness { .. } => {
            "platform_administration.connector.readiness.observe"
        }
        CommandFingerprint::IssueAdminContext { .. } => {
            "platform_administration.admin_context.issue"
        }
        CommandFingerprint::RevokeAdminContext { .. } => {
            "platform_administration.admin_context.revoke"
        }
    }
}
fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = Vec::new();
    input.extend_from_slice(event_type.as_bytes());
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &input))
        .expect("derived UUID is non-nil")
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
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn decode_sha256(value: &str) -> Result<Vec<u8>, CqrsSqlxError> {
    if value.len() != 64 {
        return Err(CqrsSqlxError::GuardRejected("invalid connector digest"));
    }
    (0..64)
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| CqrsSqlxError::GuardRejected("invalid connector digest"))
        })
        .collect()
}
fn from_i64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

async fn load_holding_relational(
    write: &WritePool,
    authority: &AuthorityScope,
    id: PlatformHoldingId,
) -> Result<Option<PlatformHolding>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut transaction = write.begin().await.map_err(map_store_error)?;
    let holding_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT canonical_holding_id FROM erpx_platform_administration_write.holding_guards
         WHERE holding_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(holding_id) = holding_id else {
        return Ok(None);
    };
    if !authority.contains(&AuthorityScope::Holding { holding_id }) {
        return Err(PortError::Denied);
    }
    let row = sqlx::query(
        "SELECT aggregate_version,holding_code,holding_name,environment_id,customer_id,
                canonical_holding_id,data_profile_key,lifecycle_state,created_at,updated_at
         FROM erpx_platform_administration_write.holding_guards WHERE holding_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    PlatformHolding::try_rehydrate(
        id,
        port_u64(
            row.try_get("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("holding_code")
            .map_err(|_| PortError::Internal)?,
        row.try_get("holding_name")
            .map_err(|_| PortError::Internal)?,
        EnvironmentId::try_from_uuid(
            row.try_get("environment_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row.try_get("customer_id")
            .map_err(|_| PortError::Internal)?,
        row.try_get("canonical_holding_id")
            .map_err(|_| PortError::Internal)?,
        row.try_get("data_profile_key")
            .map_err(|_| PortError::Internal)?,
        parse_holding_state(
            row.try_get("lifecycle_state")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_connector_relational(
    write: &WritePool,
    authority: &AuthorityScope,
    id: ConnectorBindingId,
) -> Result<Option<ConnectorBinding>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut transaction = write.begin().await.map_err(map_store_error)?;
    let scope_row = sqlx::query(
        "SELECT scope_kind,holding_id,company_id,branch_id
         FROM erpx_platform_administration_write.connector_guards WHERE connector_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(scope_row) = scope_row else {
        return Ok(None);
    };
    let scope = connector_scope_from_row(&scope_row)?;
    if !authority.contains(&scope.active_scope()) {
        return Err(PortError::Denied);
    }
    let row = sqlx::query(
        "SELECT aggregate_version,connector_kind_type,connector_kind_other,connector_key,
                configuration_digest,connector_state,readiness_evidence_id,
                readiness_configuration_digest,readiness_ready,readiness_failure_category,
                readiness_observed_at,created_at,updated_at
         FROM erpx_platform_administration_write.connector_guards WHERE connector_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let capabilities = load_connector_capabilities(&mut transaction, id, false).await?;
    let readiness = match row
        .try_get::<Option<String>, _>("readiness_evidence_id")
        .map_err(|_| PortError::Internal)?
    {
        None => None,
        Some(evidence_id) => Some(
            ConnectorReadinessEvidence::try_new(
                evidence_id,
                encode_sha256_port(
                    row.try_get("readiness_configuration_digest")
                        .map_err(|_| PortError::Internal)?,
                )?,
                load_connector_capabilities(&mut transaction, id, true).await?,
                row.try_get::<Option<bool>, _>("readiness_ready")
                    .map_err(|_| PortError::Internal)?
                    .ok_or(PortError::Internal)?,
                row.try_get("readiness_failure_category")
                    .map_err(|_| PortError::Internal)?,
                row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("readiness_observed_at")
                    .map_err(|_| PortError::Internal)?
                    .ok_or(PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
        ),
    };
    ConnectorBinding::try_rehydrate(
        id,
        port_u64(
            row.try_get("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )?,
        scope,
        parse_connector_kind(
            row.try_get("connector_kind_type")
                .map_err(|_| PortError::Internal)?,
            row.try_get("connector_kind_other")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("connector_key")
            .map_err(|_| PortError::Internal)?,
        encode_sha256_port(
            row.try_get("configuration_digest")
                .map_err(|_| PortError::Internal)?,
        )?,
        capabilities,
        readiness,
        parse_connector_state(
            row.try_get("connector_state")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_admin_context_relational(
    write: &WritePool,
    authority: &AuthorityScope,
    id: AdminContextId,
) -> Result<Option<HoldingAdminContext>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut transaction = write.begin().await.map_err(map_store_error)?;
    let holding_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT holding_id FROM erpx_platform_administration_write.admin_context_guards
         WHERE context_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(holding_id) = holding_id else {
        return Ok(None);
    };
    let scope = AuthorityScope::Holding { holding_id };
    if !authority.contains(&scope) {
        return Err(PortError::Denied);
    }
    let row = sqlx::query(
        "SELECT aggregate_version,holding_id,principal_ref,device_id,session_id,reason,
                issued_at,expires_at,revoked_at
         FROM erpx_platform_administration_write.admin_context_guards WHERE context_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    HoldingAdminContext::try_rehydrate(
        id,
        port_u64(
            row.try_get("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )?,
        PlatformHoldingId::try_from_uuid(
            row.try_get("holding_id").map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        scope,
        row.try_get("principal_ref")
            .map_err(|_| PortError::Internal)?,
        row.try_get("device_id").map_err(|_| PortError::Internal)?,
        row.try_get("session_id").map_err(|_| PortError::Internal)?,
        row.try_get("reason").map_err(|_| PortError::Internal)?,
        row.try_get("issued_at").map_err(|_| PortError::Internal)?,
        row.try_get("expires_at").map_err(|_| PortError::Internal)?,
        row.try_get("revoked_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_connector_capabilities(
    transaction: &mut Transaction<'_, Postgres>,
    id: ConnectorBindingId,
    readiness: bool,
) -> Result<BTreeSet<String>, PortError> {
    let query = if readiness {
        "SELECT capability FROM erpx_platform_administration_write.connector_readiness_capabilities WHERE connector_id=$1 ORDER BY capability"
    } else {
        "SELECT capability FROM erpx_platform_administration_write.connector_capabilities WHERE connector_id=$1 ORDER BY capability"
    };
    sqlx::query_scalar(query)
        .bind(id.as_uuid())
        .fetch_all(&mut **transaction)
        .await
        .map(|values: Vec<String>| values.into_iter().collect())
        .map_err(|_| PortError::Unavailable)
}

async fn apply_connector_authority(
    transaction: &mut Transaction<'_, Postgres>,
    value: &ConnectorBinding,
    version: i64,
) -> Result<(), CqrsSqlxError> {
    let (scope_kind, holding_id, company_id, branch_id) = scope_relational_parts(value.scope())?;
    let (kind_type, kind_other) = connector_kind_parts(value.kind());
    let readiness = value.readiness();
    sqlx::query("INSERT INTO erpx_platform_administration_write.connector_guards (connector_id,scope_key,connector_kind,connector_key,configuration_digest,connector_state,aggregate_version,updated_at,scope_kind,holding_id,company_id,branch_id,connector_kind_type,connector_kind_other,readiness_evidence_id,readiness_configuration_digest,readiness_ready,readiness_failure_category,readiness_observed_at,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20) ON CONFLICT (connector_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,connector_kind=EXCLUDED.connector_kind,connector_key=EXCLUDED.connector_key,configuration_digest=EXCLUDED.configuration_digest,connector_state=EXCLUDED.connector_state,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at,scope_kind=EXCLUDED.scope_kind,holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,connector_kind_type=EXCLUDED.connector_kind_type,connector_kind_other=EXCLUDED.connector_kind_other,readiness_evidence_id=EXCLUDED.readiness_evidence_id,readiness_configuration_digest=EXCLUDED.readiness_configuration_digest,readiness_ready=EXCLUDED.readiness_ready,readiness_failure_category=EXCLUDED.readiness_failure_category,readiness_observed_at=EXCLUDED.readiness_observed_at,created_at=EXCLUDED.created_at")
        .bind(value.id().as_uuid()).bind(scope_key(&value.scope().active_scope())?).bind(connector_kind(value.kind())).bind(value.key()).bind(decode_sha256(value.configuration_digest())?).bind(connector_state(value.state())).bind(version).bind(value.updated_at()).bind(scope_kind).bind(holding_id).bind(company_id).bind(branch_id).bind(kind_type).bind(kind_other).bind(readiness.map(ConnectorReadinessEvidence::evidence_id)).bind(readiness.map(|e| decode_sha256(e.configuration_digest())).transpose()?).bind(readiness.map(ConnectorReadinessEvidence::ready)).bind(readiness.and_then(ConnectorReadinessEvidence::failure_category)).bind(readiness.map(ConnectorReadinessEvidence::observed_at)).bind(value.created_at()).execute(&mut **transaction).await?;
    sqlx::query("DELETE FROM erpx_platform_administration_write.connector_capabilities WHERE connector_id=$1")
        .bind(value.id().as_uuid()).execute(&mut **transaction).await?;
    sqlx::query("DELETE FROM erpx_platform_administration_write.connector_readiness_capabilities WHERE connector_id=$1")
        .bind(value.id().as_uuid()).execute(&mut **transaction).await?;
    for capability in value.capabilities() {
        sqlx::query("INSERT INTO erpx_platform_administration_write.connector_capabilities (connector_id,capability) VALUES ($1,$2)")
            .bind(value.id().as_uuid()).bind(capability).execute(&mut **transaction).await?;
    }
    if let Some(readiness) = readiness {
        for capability in readiness.observed_capabilities() {
            sqlx::query("INSERT INTO erpx_platform_administration_write.connector_readiness_capabilities (connector_id,capability) VALUES ($1,$2)")
                .bind(value.id().as_uuid()).bind(capability).execute(&mut **transaction).await?;
        }
    }
    Ok(())
}

type RelationalScopeParts = (&'static str, Option<Uuid>, Option<Uuid>, Option<Uuid>);

fn scope_relational_parts(scope: &ConnectorScope) -> Result<RelationalScopeParts, CqrsSqlxError> {
    Ok(match scope {
        ConnectorScope::Platform => ("platform", None, None, None),
        ConnectorScope::Holding(AuthorityScope::Holding { holding_id }) => {
            ("holding", Some(*holding_id), None, None)
        }
        ConnectorScope::Holding(AuthorityScope::Company {
            holding_id,
            company_id,
        }) => ("company", Some(*holding_id), Some(*company_id), None),
        ConnectorScope::Holding(AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        }) => (
            "branch",
            Some(*holding_id),
            Some(*company_id),
            Some(*branch_id),
        ),
        ConnectorScope::Holding(AuthorityScope::Platform | AuthorityScope::Subtree { .. }) => {
            return Err(CqrsSqlxError::GuardRejected(
                "connector scope is not relationally representable",
            ));
        }
    })
}

fn connector_kind_parts(kind: &ConnectorKind) -> (&'static str, Option<&str>) {
    match kind {
        ConnectorKind::Stripe => ("stripe", None),
        ConnectorKind::Minio => ("minio", None),
        ConnectorKind::Ollama => ("ollama", None),
        ConnectorKind::Other(value) => ("other", Some(value)),
    }
}

fn connector_scope_from_row(row: &sqlx::postgres::PgRow) -> Result<ConnectorScope, PortError> {
    let kind: String = row.try_get("scope_kind").map_err(|_| PortError::Internal)?;
    let holding: Option<Uuid> = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company: Option<Uuid> = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
    match (kind.as_str(), holding, company, branch) {
        ("platform", None, None, None) => Ok(ConnectorScope::Platform),
        ("holding", Some(holding_id), None, None) => {
            Ok(ConnectorScope::Holding(AuthorityScope::Holding {
                holding_id,
            }))
        }
        ("company", Some(holding_id), Some(company_id), None) => {
            Ok(ConnectorScope::Holding(AuthorityScope::Company {
                holding_id,
                company_id,
            }))
        }
        ("branch", Some(holding_id), Some(company_id), Some(branch_id)) => {
            Ok(ConnectorScope::Holding(AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            }))
        }
        _ => Err(PortError::Internal),
    }
}

fn parse_connector_kind(kind: String, other: Option<String>) -> Result<ConnectorKind, PortError> {
    match (kind.as_str(), other) {
        ("stripe", None) => Ok(ConnectorKind::Stripe),
        ("minio", None) => Ok(ConnectorKind::Minio),
        ("ollama", None) => Ok(ConnectorKind::Ollama),
        ("other", Some(value)) => Ok(ConnectorKind::Other(value)),
        _ => Err(PortError::Internal),
    }
}

fn parse_connector_state(value: String) -> Result<ConnectorState, PortError> {
    match value.as_str() {
        "pending" => Ok(ConnectorState::Pending),
        "active" => Ok(ConnectorState::Active),
        "suspended" => Ok(ConnectorState::Suspended),
        "retired" => Ok(ConnectorState::Retired),
        _ => Err(PortError::Internal),
    }
}
fn parse_holding_state(value: String) -> Result<HoldingLifecycleState, PortError> {
    match value.as_str() {
        "pending_subscription" => Ok(HoldingLifecycleState::PendingSubscription),
        "provisioning" => Ok(HoldingLifecycleState::Provisioning),
        "provisioning_failed" => Ok(HoldingLifecycleState::ProvisioningFailed),
        "active" => Ok(HoldingLifecycleState::Active),
        "suspended" => Ok(HoldingLifecycleState::Suspended),
        "recovery" => Ok(HoldingLifecycleState::Recovery),
        "retired" => Ok(HoldingLifecycleState::Retired),
        _ => Err(PortError::Internal),
    }
}
fn port_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}
fn encode_sha256_port(value: Vec<u8>) -> Result<String, PortError> {
    if value.len() != 32 {
        return Err(PortError::Internal);
    }
    Ok(value
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
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
    #[test]
    fn stable_hash_is_payload_sensitive() {
        let id = PlatformHoldingId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let a = CommandFingerprint::TransitionHolding {
            holding_id: id,
            action: erpx_platform_administration::HoldingLifecycleAction::Activate,
        };
        let b = CommandFingerprint::TransitionHolding {
            holding_id: id,
            action: erpx_platform_administration::HoldingLifecycleAction::Suspend,
        };
        assert_eq!(request_hash(&a).unwrap(), request_hash(&a).unwrap());
        assert_ne!(request_hash(&a).unwrap(), request_hash(&b).unwrap());
    }
    #[test]
    fn guard_source_covers_all_frozen_control_tables() {
        let source = include_str!("command.rs");
        for table in ["holding_guards", "connector_guards", "admin_context_guards"] {
            assert!(source.contains(table));
        }
        assert!(source.contains("commit_normalized"));
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("load_snapshot"));
        assert!(!production.contains("serde_json::to_value(value.capabilities())"));
        assert!(production.contains("connector_capabilities"));
    }
}
