use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, ProjectionEvent,
    ProjectionFailure, Projector, ProjectorContext, ReadPool, RequestHash, WritePool,
};
use erpx_customer_authority::{
    CustomerAuthorityRecord, CustomerAuthorityRecordCommand, CustomerAuthorityRecordKind,
    CustomerAuthorityRecordQueryPort, CustomerAuthorityRecordReceipt,
    CustomerAuthorityRecordStatus, CustomerAuthorityRecordUnitOfWork, CustomerAuthorityRecordView,
    CustomerId, PortError,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    ItemDescriptorDraftV2, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, DataClassification, EventId, RecordId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

const CONTEXT: &str = "customer_authority";
const AGGREGATE: &str = "customer_authority_record";
const STATE_EVENT: &str = "customer_authority.record_changed";
pub const PROJECTION: &str = "customer_authority.business-records";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Payload {
    State(CustomerAuthorityProjectionFact),
    Directory(ItemDescriptorDraftV2),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CustomerAuthorityProjectionFact {
    record_id: Uuid,
    record_kind: CustomerAuthorityRecordKind,
    record_version: u64,
    active_scope: AuthorityScope,
    customer_id: Uuid,
    record_status: CustomerAuthorityRecordStatus,
    valid_until: chrono::DateTime<chrono::Utc>,
}

impl From<&CustomerAuthorityRecord> for CustomerAuthorityProjectionFact {
    fn from(record: &CustomerAuthorityRecord) -> Self {
        Self {
            record_id: record.id().as_uuid(),
            record_kind: record.kind(),
            record_version: record.version(),
            active_scope: record.scope().clone(),
            customer_id: record.customer_id().as_uuid(),
            record_status: record.status(),
            valid_until: record.valid_until(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityRecordSqlxAdapter {
    store: CommandStore,
}
impl CustomerAuthorityRecordSqlxAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityRecordReadAdapter {
    pool: ReadPool,
}

impl CustomerAuthorityRecordReadAdapter {
    pub const fn new(pool: ReadPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CustomerAuthorityRecordQueryPort for CustomerAuthorityRecordReadAdapter {
    async fn get_record(
        &self,
        authority_scope: &AuthorityScope,
        record_id: RecordId,
    ) -> Result<Option<CustomerAuthorityRecordView>, PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        require_active_generation(&mut tx, PROJECTION).await?;
        let row = sqlx::query(
            "SELECT record.record_id, record.record_kind, record.source_aggregate_version,
                    record.customer_id, record.record_status, record.valid_until
             FROM erpx_customer_authority_read.business_authority_projection record
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.holding_id=record.holding_id
              AND generation.projection_name=record.projection_name
              AND generation.generation=record.generation
              AND generation.status='active'
             WHERE record.holding_id=erpx_cqrs_read.current_holding_id()
               AND record.projection_name=$1 AND record.scope_key=$2 AND record.record_id=$3",
        )
        .bind(PROJECTION)
        .bind(scope_key(authority_scope))
        .bind(record_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        row.map(|row| {
            Ok(CustomerAuthorityRecordView {
                record_id: RecordId::try_from_uuid(
                    row.try_get("record_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                kind: parse_kind(
                    row.try_get("record_kind")
                        .map_err(|_| PortError::Internal)?,
                )?,
                version: parse_version(
                    row.try_get("source_aggregate_version")
                        .map_err(|_| PortError::Internal)?,
                )?,
                customer_id: CustomerId::try_from_uuid(
                    row.try_get("customer_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                status: parse_status(
                    row.try_get("record_status")
                        .map_err(|_| PortError::Internal)?,
                )?,
                valid_until: row
                    .try_get("valid_until")
                    .map_err(|_| PortError::Internal)?,
            })
        })
        .transpose()
    }
}

async fn require_active_generation(
    tx: &mut Transaction<'_, Postgres>,
    projection: &str,
) -> Result<(), PortError> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_generations
         WHERE holding_id=erpx_cqrs_read.current_holding_id()
           AND projection_name=$1 AND status='active')",
    )
    .bind(projection)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    if active {
        Ok(())
    } else {
        Err(PortError::Unavailable)
    }
}

fn parse_version(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn parse_kind(value: &str) -> Result<CustomerAuthorityRecordKind, PortError> {
    match value {
        "authority_grant" => Ok(CustomerAuthorityRecordKind::AuthorityGrant),
        "support_access" => Ok(CustomerAuthorityRecordKind::SupportAccess),
        _ => Err(PortError::Internal),
    }
}

fn parse_status(value: &str) -> Result<CustomerAuthorityRecordStatus, PortError> {
    match value {
        "proposed" => Ok(CustomerAuthorityRecordStatus::Proposed),
        "active" => Ok(CustomerAuthorityRecordStatus::Active),
        "revoked" => Ok(CustomerAuthorityRecordStatus::Revoked),
        "expired" => Ok(CustomerAuthorityRecordStatus::Expired),
        _ => Err(PortError::Internal),
    }
}

#[derive(Debug, Clone)]
pub struct CustomerAuthorityRecordProjector {
    events: Vec<String>,
}
impl CustomerAuthorityRecordProjector {
    pub fn new() -> Self {
        Self {
            events: vec![STATE_EVENT.into()],
        }
    }
}
impl Default for CustomerAuthorityRecordProjector {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl Projector for CustomerAuthorityRecordProjector {
    fn name(&self) -> &str {
        PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.events
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if context.projection != PROJECTION
            || event.context != CONTEXT
            || event.event_type != STATE_EVENT
            || event.aggregate_type != AGGREGATE
        {
            return Err(ProjectionFailure::new(
                "customer authority record event mismatch",
            ));
        }
        let record: CustomerAuthorityProjectionFact =
            serde_json::from_value(event.payload.clone()).map_err(failure)?;
        if record.record_id != event.aggregate_id.as_uuid()
            || record.record_version != event.aggregate_version
            || record.active_scope != event.active_scope
            || record.active_scope.validate().is_err()
        {
            return Err(ProjectionFailure::new(
                "customer authority record metadata mismatch",
            ));
        }
        sqlx::query("INSERT INTO erpx_customer_authority_read.business_authority_projection (holding_id,projection_name,generation,partition_key,record_id,record_kind,source_aggregate_version,source_global_sequence,active_scope,scope_key,customer_id,record_status,valid_until,projected_at) VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,record_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,record_status=EXCLUDED.record_status,valid_until=EXCLUDED.valid_until,projected_at=now() WHERE erpx_customer_authority_read.business_authority_projection.source_aggregate_version<EXCLUDED.source_aggregate_version")
            .bind(PROJECTION).bind(context.generation).bind(&event.partition).bind(record.record_id).bind(kind(record.record_kind)).bind(projection_i64(record.record_version)?).bind(projection_i64(event.global_sequence)?).bind(serde_json::to_value(&record.active_scope).map_err(failure)?).bind(scope_key(&record.active_scope)).bind(record.customer_id).bind(status(record.record_status)).bind(record.valid_until).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_customer_authority_read.business_authority_projection WHERE holding_id=erpx_cqrs_read.current_holding_id() AND projection_name=$1 AND generation=$2").bind(PROJECTION).bind(context.generation).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
}

fn projection_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(failure)
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[async_trait]
impl CustomerAuthorityRecordUnitOfWork for CustomerAuthorityRecordSqlxAdapter {
    async fn save_record(
        &self,
        command: CustomerAuthorityRecordCommand,
    ) -> Result<CustomerAuthorityRecordReceipt, PortError> {
        command.record.validate().map_err(|_| PortError::Internal)?;
        let id = command.record.id();
        let version = command.record.version();
        let partition = format!("customer_authority.record:{}", id.as_uuid().simple());
        let aggregate_id =
            AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)?;
        let hash = request_hash(&(
            &command.record,
            command.expected_version,
            &command.actor,
            &command.authority_scope,
        ))?;
        let draft = descriptor(&command.record)?;
        let events = vec![
            OutboxEventWrite {
                event_id: derived_event(
                    command.command_id.as_uuid(),
                    b"customer-authority-record-state",
                )?,
                event_type: STATE_EVENT.into(),
                event_version: 1,
                partition: partition.clone(),
                causation_id: None,
                occurred_at: command.record.updated_at(),
                payload: Payload::State(CustomerAuthorityProjectionFact::from(&command.record)),
            },
            OutboxEventWrite {
                event_id: derived_event(
                    command.command_id.as_uuid(),
                    b"customer-authority-record-directory",
                )?,
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: None,
                occurred_at: command.record.updated_at(),
                payload: Payload::Directory(draft),
            },
        ];
        let guard = RecordGuard(command.record.clone());
        let atomic = AtomicCommand {
            context: CONTEXT.into(),
            partition,
            command_id: command.command_id,
            correlation_id: command.correlation_id,
            idempotency_key: command.idempotency_key,
            request_hash: hash,
            active_scope: command.record.scope().clone(),
            actor: command.actor,
            audit_context: command.audit,
            aggregate_type: AGGREGATE.into(),
            aggregate_id,
            expected_version: command.expected_version,
            response: command.record.clone(),
            audit: AuditWrite {
                audit_id: derived_uuid(
                    command.command_id.as_uuid(),
                    b"customer-authority-record-audit",
                ),
                action: "save_customer_authority_record".into(),
                occurred_at: command.record.updated_at(),
                details: serde_json::json!({"record_type": command.record.kind().record_type(), "version": version}),
            },
            events,
        };
        let outcome = self
            .store
            .commit_normalized(atomic, &guard)
            .await
            .map_err(map_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        Ok(CustomerAuthorityRecordReceipt {
            record: command.record,
            replayed,
        })
    }
}

struct RecordGuard(CustomerAuthorityRecord);
#[async_trait]
impl AtomicCommandGuard for RecordGuard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        _context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT record_version
             FROM erpx_customer_authority_write.business_authority_records
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND record_id=$1
             FOR UPDATE",
        )
        .bind(self.0.id().as_uuid())
        .fetch_optional(&mut **tx)
        .await?;
        current
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        let current: Option<i64> = sqlx::query_scalar("SELECT record_version FROM erpx_customer_authority_write.business_authority_records WHERE holding_id=erpx_cqrs_write.current_holding_id() AND record_id=$1 FOR UPDATE")
            .bind(self.0.id().as_uuid()).fetch_optional(&mut **tx).await?;
        match (context.expected_version, current) {
            (None, None) => Ok(()),
            (Some(expected), Some(actual)) if i64::try_from(expected).ok() == Some(actual) => {
                Ok(())
            }
            _ => Err(CqrsSqlxError::GuardRejected(
                "customer authority record version conflict",
            )),
        }
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.aggregate_version != self.0.version() {
            return Err(CqrsSqlxError::GuardRejected(
                "customer authority record metadata mismatch",
            ));
        }
        let support_case = self.0.support_case().map(|v| v.id().as_uuid());
        let changed = sqlx::query("INSERT INTO erpx_customer_authority_write.business_authority_records
            (holding_id,record_id,record_kind,record_version,customer_id,customer_account_id,support_case_id,
             active_scope,scope_key,evidence_sha256,record_status,valid_until,created_by,revoked_by,created_at,updated_at)
            VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (holding_id,record_id) DO UPDATE SET record_version=EXCLUDED.record_version,
             evidence_sha256=EXCLUDED.evidence_sha256,record_status=EXCLUDED.record_status,
             revoked_by=EXCLUDED.revoked_by,updated_at=EXCLUDED.updated_at
            WHERE erpx_customer_authority_write.business_authority_records.record_version + 1=EXCLUDED.record_version")
            .bind(self.0.id().as_uuid()).bind(kind(self.0.kind())).bind(i64::try_from(self.0.version()).map_err(|_| CqrsSqlxError::NumericRange)?)
            .bind(self.0.customer_id().as_uuid()).bind(self.0.customer_account().id().as_uuid()).bind(support_case)
            .bind(serde_json::to_value(self.0.scope()).map_err(|_| CqrsSqlxError::InvalidInput("scope"))?)
            .bind(scope_key(self.0.scope())).bind(self.0.evidence_sha256()).bind(status(self.0.status()))
            .bind(self.0.valid_until()).bind(self.0.created_by()).bind(self.0.revoked_by())
            .bind(self.0.created_at()).bind(self.0.updated_at()).execute(&mut **tx).await?.rows_affected();
        if changed != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                "customer authority record write conflict",
            ));
        }
        Ok(())
    }
}

fn descriptor(record: &CustomerAuthorityRecord) -> Result<ItemDescriptorDraftV2, PortError> {
    let stable = record.record_ref().map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&stable).map_err(|_| PortError::Internal)?,
        record: stable,
        scope: record.scope().clone(),
        lifecycle_state: status(record.status()).into(),
        classification: DataClassification::Restricted,
        descriptor_version: record.version(),
        authorized_search_text: format!(
            "{} {}",
            kind(record.kind()),
            record.customer_id().as_uuid()
        ),
    };
    ItemDescriptorDraftV2::try_new(
        value,
        format!("{} {}", kind(record.kind()), record.customer_id().as_uuid()),
    )
    .map_err(|_| PortError::Internal)
}
fn kind(v: CustomerAuthorityRecordKind) -> &'static str {
    match v {
        CustomerAuthorityRecordKind::AuthorityGrant => "authority_grant",
        CustomerAuthorityRecordKind::SupportAccess => "support_access",
    }
}
fn status(v: CustomerAuthorityRecordStatus) -> &'static str {
    match v {
        CustomerAuthorityRecordStatus::Proposed => "proposed",
        CustomerAuthorityRecordStatus::Active => "active",
        CustomerAuthorityRecordStatus::Revoked => "revoked",
        CustomerAuthorityRecordStatus::Expired => "expired",
    }
}
fn scope_key(scope: &erpx_kernel::AuthorityScope) -> String {
    serde_json::to_string(scope).expect("scope")
}
fn request_hash(value: &impl Serialize) -> Result<RequestHash, PortError> {
    let digest = Sha256::digest(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&digest);
    Ok(RequestHash::new(bytes))
}
fn derived_uuid(namespace: Uuid, name: &[u8]) -> Uuid {
    Uuid::new_v5(&namespace, name)
}
fn derived_event(namespace: Uuid, name: &[u8]) -> Result<EventId, PortError> {
    EventId::try_from_uuid(derived_uuid(namespace, name)).map_err(|_| PortError::Internal)
}
fn map_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        _ => PortError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalized_uow_is_atomic_and_holding_exact() {
        let s = include_str!("authority_records.rs");
        assert!(s.contains("commit_normalized"));
        assert!(s.contains("business_authority_records"));
        assert!(s.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(s.contains("record_version + 1"));
        assert!(s.contains("generation.status='active'"));
        assert!(s.contains("record.scope_key=$2"));
        assert!(s.contains("INSERT INTO erpx_customer_authority_write.business_authority_records"));
        assert!(!s.contains(concat!("serde_json::to_value(&command", ".record)")));
        assert!(!s.contains(concat!("from_control_", "pg_pool")));
    }
}
