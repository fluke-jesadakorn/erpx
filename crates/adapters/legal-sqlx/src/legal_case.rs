use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, ProjectionEvent,
    ProjectionFailure, Projector, ProjectorContext, ReadPool, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, DataClassification, EventId, RecordId};
use erpx_legal::{
    LegalCase, LegalCaseCommand, LegalCaseQueryPort, LegalCaseReceipt, LegalCaseStatus,
    LegalCaseUnitOfWork, LegalCaseView, PortError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
const CONTEXT: &str = "legal";
const AGGREGATE: &str = "legal_case";
const STATE_EVENT: &str = "legal.case_changed";
pub const PROJECTION: &str = "legal.cases";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Payload {
    State(LegalCaseProjectionFact),
    Directory(ItemDescriptorDraft),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegalCaseProjectionFact {
    case_id: Uuid,
    case_version: u64,
    active_scope: AuthorityScope,
    case_reference: String,
    title: String,
    case_status: LegalCaseStatus,
    retention_until: chrono::DateTime<chrono::Utc>,
}
impl From<&LegalCase> for LegalCaseProjectionFact {
    fn from(case: &LegalCase) -> Self {
        Self {
            case_id: case.id().as_uuid(),
            case_version: case.version(),
            active_scope: case.scope().clone(),
            case_reference: case.case_reference().into(),
            title: case.title().into(),
            case_status: case.status(),
            retention_until: case.retention_until(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct LegalCaseSqlxAdapter {
    store: CommandStore,
}
impl LegalCaseSqlxAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegalCaseReadAdapter {
    pool: ReadPool,
}
impl LegalCaseReadAdapter {
    pub const fn new(pool: ReadPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl LegalCaseQueryPort for LegalCaseReadAdapter {
    async fn get_legal_case(
        &self,
        authority_scope: &AuthorityScope,
        case_id: RecordId,
    ) -> Result<Option<LegalCaseView>, PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        require_active_generation(&mut tx).await?;
        let row=sqlx::query("SELECT record.case_id,record.source_aggregate_version,record.case_reference,record.title,record.case_status,record.retention_until FROM erpx_legal_read.legal_case_projection record JOIN erpx_cqrs_read.projection_generations generation ON generation.holding_id=record.holding_id AND generation.projection_name=record.projection_name AND generation.generation=record.generation AND generation.status='active' WHERE record.holding_id=erpx_cqrs_read.current_holding_id() AND record.projection_name=$1 AND record.scope_key=$2 AND record.case_id=$3")
            .bind(PROJECTION).bind(scope_key(authority_scope)).bind(case_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(|_|PortError::Unavailable)?;
        row.map(|row| {
            Ok(LegalCaseView {
                case_id: RecordId::try_from_uuid(
                    row.try_get("case_id").map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                version: u64::try_from(
                    row.try_get::<i64, _>("source_aggregate_version")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                case_reference: row
                    .try_get("case_reference")
                    .map_err(|_| PortError::Internal)?,
                title: row.try_get("title").map_err(|_| PortError::Internal)?,
                status: parse_status(
                    row.try_get("case_status")
                        .map_err(|_| PortError::Internal)?,
                )?,
                retention_until: row
                    .try_get("retention_until")
                    .map_err(|_| PortError::Internal)?,
            })
        })
        .transpose()
    }
}
async fn require_active_generation(tx: &mut Transaction<'_, Postgres>) -> Result<(), PortError> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_generations WHERE holding_id=erpx_cqrs_read.current_holding_id() AND projection_name=$1 AND status='active')").bind(PROJECTION).fetch_one(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    if active {
        Ok(())
    } else {
        Err(PortError::Unavailable)
    }
}
fn parse_status(v: &str) -> Result<LegalCaseStatus, PortError> {
    match v {
        "proposed" => Ok(LegalCaseStatus::Proposed),
        "open" => Ok(LegalCaseStatus::Open),
        "on-hold" => Ok(LegalCaseStatus::OnHold),
        "resolved" => Ok(LegalCaseStatus::Resolved),
        "closed" => Ok(LegalCaseStatus::Closed),
        "cancelled" => Ok(LegalCaseStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}
#[async_trait]
impl LegalCaseUnitOfWork for LegalCaseSqlxAdapter {
    async fn save_legal_case(&self, c: LegalCaseCommand) -> Result<LegalCaseReceipt, PortError> {
        c.record.validate().map_err(|_| PortError::Internal)?;
        let id = c.record.id();
        let partition = format!("legal.case:{}", id.as_uuid().simple());
        let aid = AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)?;
        let hash = request_hash(&(&c.record, c.expected_version, &c.actor, &c.authority_scope))?;
        let events = vec![
            OutboxEventWrite {
                event_id: event(c.command_id.as_uuid(), b"legal-case-state")?,
                event_type: STATE_EVENT.into(),
                event_version: 1,
                partition: partition.clone(),
                causation_id: None,
                occurred_at: c.record.updated_at(),
                payload: Payload::State(LegalCaseProjectionFact::from(&c.record)),
            },
            OutboxEventWrite {
                event_id: event(c.command_id.as_uuid(), b"legal-case-directory")?,
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
            active_scope: c.record.scope().clone(),
            actor: c.actor,
            audit_context: c.audit,
            aggregate_type: AGGREGATE.into(),
            aggregate_id: aid,
            expected_version: c.expected_version,
            response: c.record.clone(),
            audit: AuditWrite {
                audit_id: Uuid::new_v5(&c.command_id.as_uuid(), b"legal-case-audit"),
                action: "save_legal_case".into(),
                occurred_at: c.record.updated_at(),
                details: serde_json::json!({"case_reference":c.record.case_reference(),"version":c.record.version()}),
            },
            events,
        };
        let o = self
            .store
            .commit_normalized(atomic, &guard)
            .await
            .map_err(map_error)?;
        Ok(LegalCaseReceipt {
            record: c.record,
            replayed: matches!(o, AtomicCommandOutcome::Replayed(_)),
        })
    }
}
struct Guard(LegalCase);
#[async_trait]
impl AtomicCommandGuard for Guard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        if c.context != CONTEXT
            || c.aggregate_type != AGGREGATE
            || c.aggregate_id.as_uuid() != self.0.id().as_uuid()
        {
            return Err(CqrsSqlxError::GuardRejected("legal case metadata mismatch"));
        }
        sqlx::query_scalar::<_, i64>(
            "SELECT case_version FROM erpx_legal_write.legal_cases
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND case_id=$1 FOR UPDATE",
        )
        .bind(self.0.id().as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
        .transpose()
    }

    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        let a:Option<i64>=sqlx::query_scalar("SELECT case_version FROM erpx_legal_write.legal_cases WHERE holding_id=erpx_cqrs_write.current_holding_id() AND case_id=$1 FOR UPDATE").bind(self.0.id().as_uuid()).fetch_optional(&mut **tx).await?;
        match (c.expected_version, a) {
            (None, None) => Ok(()),
            (Some(e), Some(a)) if i64::try_from(e).ok() == Some(a) => Ok(()),
            _ => Err(CqrsSqlxError::GuardRejected("legal case version conflict")),
        }
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if c.aggregate_version != self.0.version() {
            return Err(CqrsSqlxError::GuardRejected("legal case metadata mismatch"));
        }
        let changed=sqlx::query("INSERT INTO erpx_legal_write.legal_cases (holding_id,case_id,case_version,active_scope,scope_key,case_reference,title,related_matter_id,source_version,evidence_sha256,case_status,owner_ref,closed_by,retention_until,created_at,updated_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) ON CONFLICT (holding_id,case_id) DO UPDATE SET case_version=EXCLUDED.case_version,evidence_sha256=EXCLUDED.evidence_sha256,case_status=EXCLUDED.case_status,closed_by=EXCLUDED.closed_by,updated_at=EXCLUDED.updated_at WHERE erpx_legal_write.legal_cases.case_version+1=EXCLUDED.case_version").bind(self.0.id().as_uuid()).bind(to_i64(self.0.version())?).bind(serde_json::to_value(self.0.scope())?).bind(scope_key(self.0.scope())).bind(self.0.case_reference()).bind(self.0.title()).bind(self.0.related_matter().map(|v|v.id().as_uuid())).bind(self.0.source_version().map(to_i64).transpose()?).bind(self.0.evidence_sha256()).bind(status(self.0.status())).bind(self.0.owner_ref()).bind(self.0.closed_by()).bind(self.0.retention_until()).bind(self.0.created_at()).bind(self.0.updated_at()).execute(&mut **tx).await?.rows_affected();
        if changed != 1 {
            return Err(CqrsSqlxError::GuardRejected("legal case write conflict"));
        }
        Ok(())
    }
}
fn descriptor(r: &LegalCase) -> Result<ItemDescriptorDraft, PortError> {
    let s = r.record_ref().map_err(|_| PortError::Internal)?;
    let v = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&s).map_err(|_| PortError::Internal)?,
        record: s,
        scope: r.scope().clone(),
        lifecycle_state: status(r.status()).into(),
        classification: DataClassification::Restricted,
        descriptor_version: r.version(),
        authorized_search_text: format!("{} {}", r.case_reference(), r.title()),
    };
    v.validate().map_err(|_| PortError::Internal)?;
    Ok(v)
}
fn status(v: LegalCaseStatus) -> &'static str {
    match v {
        LegalCaseStatus::Proposed => "proposed",
        LegalCaseStatus::Open => "open",
        LegalCaseStatus::OnHold => "on-hold",
        LegalCaseStatus::Resolved => "resolved",
        LegalCaseStatus::Closed => "closed",
        LegalCaseStatus::Cancelled => "cancelled",
    }
}
fn scope_key(v: &erpx_kernel::AuthorityScope) -> String {
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
fn event(n: Uuid, v: &[u8]) -> Result<EventId, PortError> {
    EventId::try_from_uuid(Uuid::new_v5(&n, v)).map_err(|_| PortError::Internal)
}
fn map_error(e: CqrsSqlxError) -> PortError {
    match e {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::GuardRejected(_) | CqrsSqlxError::OptimisticConcurrency { .. } => {
            PortError::Conflict {
                expected: 0,
                actual: 0,
            }
        }
        _ => PortError::Unavailable,
    }
}
#[derive(Debug, Clone)]
pub struct LegalCaseProjector {
    events: Vec<String>,
}
impl LegalCaseProjector {
    pub fn new() -> Self {
        Self {
            events: vec![STATE_EVENT.into()],
        }
    }
}
impl Default for LegalCaseProjector {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl Projector for LegalCaseProjector {
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
            return Err(ProjectionFailure::new("legal case event mismatch"));
        }
        let r: LegalCaseProjectionFact =
            serde_json::from_value(e.payload.clone()).map_err(pfailure)?;
        if r.case_id != e.aggregate_id.as_uuid()
            || r.case_version != e.aggregate_version
            || r.active_scope != e.active_scope
            || r.active_scope.validate().is_err()
            || r.case_reference.trim().is_empty()
            || r.title.trim().is_empty()
        {
            return Err(ProjectionFailure::new("legal case metadata mismatch"));
        }
        sqlx::query("INSERT INTO erpx_legal_read.legal_case_projection (holding_id,projection_name,generation,partition_key,case_id,source_aggregate_version,source_global_sequence,active_scope,scope_key,case_reference,title,case_status,retention_until,projected_at) VALUES (erpx_cqrs_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now()) ON CONFLICT (holding_id,projection_name,generation,partition_key,case_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,title=EXCLUDED.title,case_status=EXCLUDED.case_status,retention_until=EXCLUDED.retention_until,projected_at=now() WHERE erpx_legal_read.legal_case_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(PROJECTION).bind(c.generation).bind(&e.partition).bind(r.case_id).bind(pi64(r.case_version)?).bind(pi64(e.global_sequence)?).bind(serde_json::to_value(&r.active_scope).map_err(pfailure)?).bind(scope_key(&r.active_scope)).bind(&r.case_reference).bind(&r.title).bind(status(r.case_status)).bind(r.retention_until).execute(&mut **tx).await.map_err(pfailure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_legal_read.legal_case_projection WHERE holding_id=erpx_cqrs_read.current_holding_id() AND projection_name=$1 AND generation=$2").bind(PROJECTION).bind(c.generation).execute(&mut **tx).await.map_err(pfailure)?;
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
    fn legal_case_is_normalized_atomic() {
        let s = include_str!("legal_case.rs");
        assert!(s.contains("legal_cases.case_version+1"));
        assert!(s.contains("commit_normalized"));
        assert!(s.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(s.contains("generation.status='active'"));
        assert!(s.contains("record.scope_key=$2"));
        assert!(!s.contains(concat!("serde_json::to_value(&c", ".record)")));
    }
}
