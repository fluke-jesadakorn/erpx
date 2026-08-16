use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, ProjectionEvent,
    ProjectionFailure, Projector, ProjectorContext, ReadPool, RequestHash, WritePool,
};
use erpx_customer_360::{
    Customer360OwnedRecord, Customer360OwnedRecordQueryPort, Customer360OwnedRecordView,
    Customer360RecordCommand, Customer360RecordKind, Customer360RecordReceipt,
    Customer360RecordStatus, Customer360RecordUnitOfWork, PortError,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    record_policy_id,
};
use erpx_kernel::{AggregateId, DataClassification, EventId, RecordId, TenantScope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
const CONTEXT: &str = "customer-360";
const AGGREGATE: &str = "customer_360_owned_record";
const STATE_EVENT: &str = "customer_360.owned_record_changed";
pub const PROJECTION: &str = "customer360.owned-records";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Payload {
    State(Customer360ProjectionFact),
    Directory(ItemDescriptorDraft),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Customer360ProjectionFact {
    record_id: Uuid,
    record_kind: Customer360RecordKind,
    record_version: u64,
    tenant_scope: TenantScope,
    customer_account_id: Uuid,
    title: String,
    record_status: Customer360RecordStatus,
}
impl From<&Customer360OwnedRecord> for Customer360ProjectionFact {
    fn from(record: &Customer360OwnedRecord) -> Self {
        Self {
            record_id: record.id().as_uuid(),
            record_kind: record.kind(),
            record_version: record.version(),
            tenant_scope: record.scope().clone(),
            customer_account_id: record.customer_account().id().as_uuid(),
            title: record.title().into(),
            record_status: record.status(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct Customer360RecordSqlxAdapter {
    store: CommandStore,
}
impl Customer360RecordSqlxAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Customer360OwnedRecordReadAdapter {
    pool: ReadPool,
}
impl Customer360OwnedRecordReadAdapter {
    pub const fn new(pool: ReadPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl Customer360OwnedRecordQueryPort for Customer360OwnedRecordReadAdapter {
    async fn get_owned_record(
        &self,
        authority_scope: &TenantScope,
        record_id: RecordId,
    ) -> Result<Option<Customer360OwnedRecordView>, PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        require_active_generation(&mut tx).await?;
        let row = sqlx::query(
            "SELECT record.record_id,record.record_kind,record.source_aggregate_version,
                    record.customer_account_id,record.title,record.record_status
             FROM erpx_customer_360_read.owned_record_projection record
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.tenant_id=record.tenant_id
              AND generation.projection_name=record.projection_name
              AND generation.generation=record.generation
              AND generation.status='active'
             WHERE record.tenant_id=erpx_cqrs_read.current_tenant_id()
               AND record.projection_name=$1 AND record.scope_key=$2 AND record.record_id=$3",
        )
        .bind(PROJECTION)
        .bind(scope_key(authority_scope))
        .bind(record_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        row.map(|row| {
            Ok(Customer360OwnedRecordView {
                record_id: RecordId::try_from_uuid(
                    row.try_get("record_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                kind: parse_kind(
                    row.try_get("record_kind")
                        .map_err(|_| PortError::Internal)?,
                )?,
                version: u64::try_from(
                    row.try_get::<i64, _>("source_aggregate_version")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                customer_account_id: RecordId::try_from_uuid(
                    row.try_get("customer_account_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                title: row.try_get("title").map_err(|_| PortError::Internal)?,
                status: parse_status(
                    row.try_get("record_status")
                        .map_err(|_| PortError::Internal)?,
                )?,
            })
        })
        .transpose()
    }
}
async fn require_active_generation(tx: &mut Transaction<'_, Postgres>) -> Result<(), PortError> {
    let active: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_generations WHERE tenant_id=erpx_cqrs_read.current_tenant_id() AND projection_name=$1 AND status='active')").bind(PROJECTION).fetch_one(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    if active {
        Ok(())
    } else {
        Err(PortError::Unavailable)
    }
}
fn parse_kind(v: &str) -> Result<Customer360RecordKind, PortError> {
    match v {
        "profile" => Ok(Customer360RecordKind::Profile),
        "note" => Ok(Customer360RecordKind::Note),
        "merge" => Ok(Customer360RecordKind::Merge),
        _ => Err(PortError::Internal),
    }
}
fn parse_status(v: &str) -> Result<Customer360RecordStatus, PortError> {
    match v {
        "draft" => Ok(Customer360RecordStatus::Draft),
        "active" => Ok(Customer360RecordStatus::Active),
        "closed" => Ok(Customer360RecordStatus::Closed),
        "cancelled" => Ok(Customer360RecordStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}
#[async_trait]
impl Customer360RecordUnitOfWork for Customer360RecordSqlxAdapter {
    async fn save_record(
        &self,
        c: Customer360RecordCommand,
    ) -> Result<Customer360RecordReceipt, PortError> {
        c.record.validate().map_err(|_| PortError::Internal)?;
        let id = c.record.id();
        let partition = format!("customer360.record:{}", id.as_uuid().simple());
        let aid = AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)?;
        let hash = request_hash(&(&c.record, c.expected_version, &c.actor, &c.authority_scope))?;
        let events = vec![
            OutboxEventWrite {
                event_id: derived_event(c.command_id.as_uuid(), b"c360-state")?,
                event_type: STATE_EVENT.into(),
                event_version: 1,
                partition: partition.clone(),
                causation_id: None,
                occurred_at: c.record.updated_at(),
                payload: Payload::State(Customer360ProjectionFact::from(&c.record)),
            },
            OutboxEventWrite {
                event_id: derived_event(c.command_id.as_uuid(), b"c360-directory")?,
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: None,
                occurred_at: c.record.updated_at(),
                payload: Payload::Directory(descriptor(&c.record)?),
            },
        ];
        let guard = Guard(c.record.clone());
        let atomic = AtomicCommand {
            context: CONTEXT.into(),
            partition,
            command_id: c.command_id,
            correlation_id: c.correlation_id,
            idempotency_key: c.idempotency_key,
            request_hash: hash,
            tenant_scope: c.record.scope().clone(),
            actor: c.actor,
            audit_context: c.audit,
            aggregate_type: AGGREGATE.into(),
            aggregate_id: aid,
            expected_version: c.expected_version,
            response: c.record.clone(),
            audit: AuditWrite {
                audit_id: derived_uuid(c.command_id.as_uuid(), b"c360-audit"),
                action: "save_customer_360_owned_record".into(),
                occurred_at: c.record.updated_at(),
                details: serde_json::json!({"record_type":c.record.kind().record_type(),"version":c.record.version()}),
            },
            events,
        };
        let o = self
            .store
            .commit_normalized(atomic, &guard)
            .await
            .map_err(map_error)?;
        Ok(Customer360RecordReceipt {
            record: c.record,
            replayed: matches!(o, AtomicCommandOutcome::Replayed(_)),
        })
    }
}
struct Guard(Customer360OwnedRecord);
#[async_trait]
impl AtomicCommandGuard for Guard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        _context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT record_version
             FROM erpx_customer_360_write.owned_records
             WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND record_id=$1
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
        c: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        let current:Option<i64>=sqlx::query_scalar("SELECT record_version FROM erpx_customer_360_write.owned_records WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND record_id=$1 FOR UPDATE").bind(self.0.id().as_uuid()).fetch_optional(&mut **tx).await?;
        match (c.expected_version, current) {
            (None, None) => Ok(()),
            (Some(e), Some(a)) if i64::try_from(e).ok() == Some(a) => Ok(()),
            _ => Err(CqrsSqlxError::GuardRejected(
                "customer 360 record version conflict",
            )),
        }
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if c.aggregate_version != self.0.version() {
            return Err(CqrsSqlxError::GuardRejected(
                "customer 360 record metadata mismatch",
            ));
        }
        let changed=sqlx::query("INSERT INTO erpx_customer_360_write.owned_records (tenant_id,record_id,record_kind,record_version,tenant_scope,scope_key,customer_account_id,related_customer_account_id,source_version,title,content_object_ref,content_sha256,record_status,created_by,closed_by,created_at,updated_at) VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT (tenant_id,record_id) DO UPDATE SET record_version=EXCLUDED.record_version,title=EXCLUDED.title,content_object_ref=EXCLUDED.content_object_ref,content_sha256=EXCLUDED.content_sha256,record_status=EXCLUDED.record_status,closed_by=EXCLUDED.closed_by,updated_at=EXCLUDED.updated_at WHERE erpx_customer_360_write.owned_records.record_version+1=EXCLUDED.record_version").bind(self.0.id().as_uuid()).bind(kind(self.0.kind())).bind(to_i64(self.0.version())?).bind(serde_json::to_value(self.0.scope())?).bind(scope_key(self.0.scope())).bind(self.0.customer_account().id().as_uuid()).bind(self.0.related_customer_account().map(|v|v.id().as_uuid())).bind(to_i64(self.0.source_version())?).bind(self.0.title()).bind(self.0.content_object_ref()).bind(self.0.content_sha256()).bind(status(self.0.status())).bind(self.0.created_by()).bind(self.0.closed_by()).bind(self.0.created_at()).bind(self.0.updated_at()).execute(&mut **tx).await?.rows_affected();
        if changed != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                "customer 360 record write conflict",
            ));
        }
        Ok(())
    }
}
fn descriptor(r: &Customer360OwnedRecord) -> Result<ItemDescriptorDraft, PortError> {
    let s = r.record_ref().map_err(|_| PortError::Internal)?;
    let v = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&s).map_err(|_| PortError::Internal)?,
        record: s,
        scope: r.scope().clone(),
        lifecycle_state: status(r.status()).into(),
        classification: DataClassification::Confidential,
        descriptor_version: r.version(),
        authorized_search_text: r.title().into(),
    };
    v.validate().map_err(|_| PortError::Internal)?;
    Ok(v)
}
fn kind(v: Customer360RecordKind) -> &'static str {
    match v {
        Customer360RecordKind::Profile => "profile",
        Customer360RecordKind::Note => "note",
        Customer360RecordKind::Merge => "merge",
    }
}
fn status(v: Customer360RecordStatus) -> &'static str {
    match v {
        Customer360RecordStatus::Draft => "draft",
        Customer360RecordStatus::Active => "active",
        Customer360RecordStatus::Closed => "closed",
        Customer360RecordStatus::Cancelled => "cancelled",
    }
}
fn scope_key(v: &erpx_kernel::TenantScope) -> String {
    serde_json::to_string(v).expect("scope")
}
fn to_i64(v: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(v).map_err(|_| CqrsSqlxError::NumericRange)
}
fn request_hash(v: &impl Serialize) -> Result<RequestHash, PortError> {
    let d = Sha256::digest(serde_json::to_vec(v).map_err(|_| PortError::Internal)?);
    let mut b = [0; 32];
    b.copy_from_slice(&d);
    Ok(RequestHash::new(b))
}
fn derived_uuid(n: Uuid, v: &[u8]) -> Uuid {
    Uuid::new_v5(&n, v)
}
fn derived_event(n: Uuid, v: &[u8]) -> Result<EventId, PortError> {
    EventId::try_from_uuid(derived_uuid(n, v)).map_err(|_| PortError::Internal)
}
fn map_error(e: CqrsSqlxError) -> PortError {
    match e {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::Internal
        }
        CqrsSqlxError::GuardRejected(_) | CqrsSqlxError::OptimisticConcurrency { .. } => {
            PortError::Internal
        }
        _ => PortError::Unavailable,
    }
}

#[derive(Debug, Clone)]
pub struct Customer360OwnedRecordProjector {
    events: Vec<String>,
}
impl Customer360OwnedRecordProjector {
    pub fn new() -> Self {
        Self {
            events: vec![STATE_EVENT.into()],
        }
    }
}
impl Default for Customer360OwnedRecordProjector {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl Projector for Customer360OwnedRecordProjector {
    fn name(&self) -> &str {
        PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.events
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &ProjectorContext<'_>,
        e: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if c.projection != PROJECTION
            || e.context != CONTEXT
            || e.event_type != STATE_EVENT
            || e.aggregate_type != AGGREGATE
        {
            return Err(ProjectionFailure::new(
                "customer 360 owned record event mismatch",
            ));
        }
        let r: Customer360ProjectionFact =
            serde_json::from_value(e.payload.clone()).map_err(pfailure)?;
        if r.record_id != e.aggregate_id.as_uuid()
            || r.record_version != e.aggregate_version
            || r.tenant_scope != e.tenant_scope
            || r.tenant_scope.validate().is_err()
            || r.title.trim().is_empty()
        {
            return Err(ProjectionFailure::new(
                "customer 360 owned record metadata mismatch",
            ));
        }
        sqlx::query("INSERT INTO erpx_customer_360_read.owned_record_projection (tenant_id,projection_name,generation,partition_key,record_id,record_kind,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,customer_account_id,title,record_status,projected_at) VALUES (erpx_cqrs_read.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,record_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,title=EXCLUDED.title,record_status=EXCLUDED.record_status,projected_at=now() WHERE erpx_customer_360_read.owned_record_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(PROJECTION).bind(c.generation).bind(&e.partition).bind(r.record_id).bind(kind(r.record_kind)).bind(pi64(r.record_version)?).bind(pi64(e.global_sequence)?).bind(serde_json::to_value(&r.tenant_scope).map_err(pfailure)?).bind(scope_key(&r.tenant_scope)).bind(r.customer_account_id).bind(&r.title).bind(status(r.record_status)).execute(&mut **tx).await.map_err(pfailure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_customer_360_read.owned_record_projection WHERE tenant_id=erpx_cqrs_read.current_tenant_id() AND projection_name=$1 AND generation=$2").bind(PROJECTION).bind(c.generation).execute(&mut **tx).await.map_err(pfailure)?;
        Ok(())
    }
}
fn pi64(v: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(v).map_err(pfailure)
}
fn pfailure(e: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(e.to_string())
}
#[cfg(test)]
mod tests {
    #[test]
    fn normalized_atomic_uow() {
        let s = include_str!("owned_records.rs");
        assert!(s.contains("commit_normalized"));
        assert!(s.contains("owned_records.record_version+1"));
        assert!(s.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(s.contains("generation.status='active'"));
        assert!(s.contains("record.scope_key=$2"));
        assert!(!s.contains(concat!("serde_json::to_value(&c", ".record)")));
    }
}
