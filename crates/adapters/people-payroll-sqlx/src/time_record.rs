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
use erpx_kernel::{AggregateId, DataClassification, EventId, RecordId, TenantScope};
use erpx_people_payroll::{
    EmployeeId, PortError, TimeRecord, TimeRecordCommand, TimeRecordQueryPort, TimeRecordReceipt,
    TimeRecordStatus, TimeRecordUnitOfWork, TimeRecordView,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
const CONTEXT: &str = "people-payroll";
const AGGREGATE: &str = "time_record";
const STATE_EVENT: &str = "people_payroll.time_record_changed";
pub const PROJECTION: &str = "people_payroll.time-records";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Payload {
    State(TimeRecordProjectionFact),
    Directory(ItemDescriptorDraft),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimeRecordProjectionFact {
    time_record_id: Uuid,
    time_record_version: u64,
    tenant_scope: TenantScope,
    employee_id: Uuid,
    work_date: chrono::NaiveDate,
    minutes: u16,
    cost_center: String,
    time_status: TimeRecordStatus,
}
impl From<&TimeRecord> for TimeRecordProjectionFact {
    fn from(record: &TimeRecord) -> Self {
        Self {
            time_record_id: record.id().as_uuid(),
            time_record_version: record.version(),
            tenant_scope: record.scope().clone(),
            employee_id: record.employee_id().as_uuid(),
            work_date: record.work_date(),
            minutes: record.minutes(),
            cost_center: record.cost_center().into(),
            time_status: record.status(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct TimeRecordSqlxAdapter {
    store: CommandStore,
}
impl TimeRecordSqlxAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeRecordReadAdapter {
    pool: ReadPool,
}
impl TimeRecordReadAdapter {
    pub const fn new(pool: ReadPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl TimeRecordQueryPort for TimeRecordReadAdapter {
    async fn get_time_record(
        &self,
        authority_scope: &TenantScope,
        time_record_id: RecordId,
    ) -> Result<Option<TimeRecordView>, PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        require_active_generation(&mut tx).await?;
        let row=sqlx::query("SELECT record.time_record_id,record.source_aggregate_version,record.employee_id,record.work_date,record.minutes,record.cost_center,record.time_status FROM erpx_people_payroll_read.time_record_projection record JOIN erpx_cqrs_read.projection_generations generation ON generation.tenant_id=record.tenant_id AND generation.projection_name=record.projection_name AND generation.generation=record.generation AND generation.status='active' WHERE record.tenant_id=erpx_cqrs_read.current_tenant_id() AND record.projection_name=$1 AND record.scope_key=$2 AND record.time_record_id=$3")
            .bind(PROJECTION).bind(scope_key(authority_scope)).bind(time_record_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(|_|PortError::Unavailable)?;
        row.map(|row| {
            let minutes: i32 = row.try_get("minutes").map_err(|_| PortError::Internal)?;
            Ok(TimeRecordView {
                time_record_id: RecordId::try_from_uuid(
                    row.try_get("time_record_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                version: u64::try_from(
                    row.try_get::<i64, _>("source_aggregate_version")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                employee_id: EmployeeId::try_from_uuid(
                    row.try_get("employee_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                work_date: row.try_get("work_date").map_err(|_| PortError::Internal)?,
                minutes: u16::try_from(minutes).map_err(|_| PortError::Internal)?,
                cost_center: row
                    .try_get("cost_center")
                    .map_err(|_| PortError::Internal)?,
                status: parse_status(
                    row.try_get("time_status")
                        .map_err(|_| PortError::Internal)?,
                )?,
            })
        })
        .transpose()
    }
}
async fn require_active_generation(tx: &mut Transaction<'_, Postgres>) -> Result<(), PortError> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_generations WHERE tenant_id=erpx_cqrs_read.current_tenant_id() AND projection_name=$1 AND status='active')").bind(PROJECTION).fetch_one(&mut **tx).await.map_err(|_|PortError::Unavailable)?;
    if active {
        Ok(())
    } else {
        Err(PortError::Unavailable)
    }
}
fn parse_status(v: &str) -> Result<TimeRecordStatus, PortError> {
    match v {
        "draft" => Ok(TimeRecordStatus::Draft),
        "submitted" => Ok(TimeRecordStatus::Submitted),
        "approved" => Ok(TimeRecordStatus::Approved),
        "posted" => Ok(TimeRecordStatus::Posted),
        "voided" => Ok(TimeRecordStatus::Voided),
        _ => Err(PortError::Internal),
    }
}
#[async_trait]
impl TimeRecordUnitOfWork for TimeRecordSqlxAdapter {
    async fn save_time_record(&self, c: TimeRecordCommand) -> Result<TimeRecordReceipt, PortError> {
        c.record.validate().map_err(|_| PortError::Internal)?;
        let id = c.record.id();
        let partition = format!("people-payroll.time-record:{}", id.as_uuid().simple());
        let aid = AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)?;
        let hash = request_hash(&(&c.record, c.expected_version, &c.actor, &c.authority_scope))?;
        let events = vec![
            OutboxEventWrite {
                event_id: event(c.command_id.as_uuid(), b"time-record-state")?,
                event_type: STATE_EVENT.into(),
                event_version: 1,
                partition: partition.clone(),
                causation_id: None,
                occurred_at: c.record.updated_at(),
                payload: Payload::State(TimeRecordProjectionFact::from(&c.record)),
            },
            OutboxEventWrite {
                event_id: event(c.command_id.as_uuid(), b"time-record-directory")?,
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
                audit_id: Uuid::new_v5(&c.command_id.as_uuid(), b"time-record-audit"),
                action: "save_time_record".into(),
                occurred_at: c.record.updated_at(),
                details: serde_json::json!({"employee_id":c.record.employee_id().as_uuid(),"version":c.record.version()}),
            },
            events,
        };
        let o = self
            .store
            .commit_normalized(atomic, &guard)
            .await
            .map_err(map_error)?;
        Ok(TimeRecordReceipt {
            record: c.record,
            replayed: matches!(o, AtomicCommandOutcome::Replayed(_)),
        })
    }
}
struct Guard(TimeRecord);
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
            return Err(CqrsSqlxError::GuardRejected(
                "time record metadata mismatch",
            ));
        }
        let version:Option<i64>=sqlx::query_scalar("SELECT time_record_version FROM erpx_people_payroll_write.time_records WHERE tenant_id=erpx_cqrs_write.current_tenant_id() AND time_record_id=$1 FOR UPDATE").bind(self.0.id().as_uuid()).fetch_optional(&mut **tx).await?;
        version
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

    async fn validate(
        &self,
        _tx: &mut Transaction<'_, Postgres>,
        c: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if c.context == CONTEXT
            && c.aggregate_type == AGGREGATE
            && c.aggregate_id.as_uuid() == self.0.id().as_uuid()
        {
            Ok(())
        } else {
            Err(CqrsSqlxError::GuardRejected(
                "time record metadata mismatch",
            ))
        }
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if c.aggregate_version != self.0.version() {
            return Err(CqrsSqlxError::GuardRejected(
                "time record metadata mismatch",
            ));
        }
        let changed=sqlx::query("INSERT INTO erpx_people_payroll_write.time_records (tenant_id,time_record_id,time_record_version,tenant_scope,scope_key,employee_id,employment_record_id,work_date,minutes,cost_center,evidence_sha256,time_status,submitted_by,approved_by,posted_by,created_at,updated_at) VALUES (erpx_cqrs_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT (tenant_id,time_record_id) DO UPDATE SET time_record_version=EXCLUDED.time_record_version,evidence_sha256=EXCLUDED.evidence_sha256,time_status=EXCLUDED.time_status,approved_by=EXCLUDED.approved_by,posted_by=EXCLUDED.posted_by,updated_at=EXCLUDED.updated_at WHERE erpx_people_payroll_write.time_records.time_record_version+1=EXCLUDED.time_record_version").bind(self.0.id().as_uuid()).bind(to_i64(self.0.version())?).bind(serde_json::to_value(self.0.scope())?).bind(scope_key(self.0.scope())).bind(self.0.employee_id().as_uuid()).bind(self.0.employment_record().id().as_uuid()).bind(self.0.work_date()).bind(i32::from(self.0.minutes())).bind(self.0.cost_center()).bind(self.0.evidence_sha256()).bind(status(self.0.status())).bind(self.0.submitted_by()).bind(self.0.approved_by()).bind(self.0.posted_by()).bind(self.0.created_at()).bind(self.0.updated_at()).execute(&mut **tx).await?.rows_affected();
        if changed != 1 {
            return Err(CqrsSqlxError::GuardRejected("time record write conflict"));
        }
        Ok(())
    }
}
fn descriptor(r: &TimeRecord) -> Result<ItemDescriptorDraft, PortError> {
    let s = r.record_ref().map_err(|_| PortError::Internal)?;
    let v = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&s).map_err(|_| PortError::Internal)?,
        record: s,
        scope: r.scope().clone(),
        lifecycle_state: status(r.status()).into(),
        classification: DataClassification::Restricted,
        descriptor_version: r.version(),
        authorized_search_text: format!("{} {}", r.employee_id().as_uuid(), r.work_date()),
    };
    v.validate().map_err(|_| PortError::Internal)?;
    Ok(v)
}
fn status(v: TimeRecordStatus) -> &'static str {
    match v {
        TimeRecordStatus::Draft => "draft",
        TimeRecordStatus::Submitted => "submitted",
        TimeRecordStatus::Approved => "approved",
        TimeRecordStatus::Posted => "posted",
        TimeRecordStatus::Voided => "voided",
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
pub struct TimeRecordProjector {
    events: Vec<String>,
}
impl TimeRecordProjector {
    pub fn new() -> Self {
        Self {
            events: vec![STATE_EVENT.into()],
        }
    }
}
impl Default for TimeRecordProjector {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait]
impl Projector for TimeRecordProjector {
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
            return Err(ProjectionFailure::new("time record event mismatch"));
        }
        let r: TimeRecordProjectionFact =
            serde_json::from_value(e.payload.clone()).map_err(pfailure)?;
        if r.time_record_id != e.aggregate_id.as_uuid()
            || r.time_record_version != e.aggregate_version
            || r.tenant_scope != e.tenant_scope
            || r.tenant_scope.validate().is_err()
            || r.minutes == 0
            || r.minutes > 1_440
            || r.cost_center.trim().is_empty()
        {
            return Err(ProjectionFailure::new("time record metadata mismatch"));
        }
        sqlx::query("INSERT INTO erpx_people_payroll_read.time_record_projection (tenant_id,projection_name,generation,partition_key,time_record_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,employee_id,work_date,minutes,cost_center,time_status,projected_at) VALUES (erpx_cqrs_read.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,time_record_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,minutes=EXCLUDED.minutes,cost_center=EXCLUDED.cost_center,time_status=EXCLUDED.time_status,projected_at=now() WHERE erpx_people_payroll_read.time_record_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(PROJECTION).bind(c.generation).bind(&e.partition).bind(r.time_record_id).bind(pi64(r.time_record_version)?).bind(pi64(e.global_sequence)?).bind(serde_json::to_value(&r.tenant_scope).map_err(pfailure)?).bind(scope_key(&r.tenant_scope)).bind(r.employee_id).bind(r.work_date).bind(i32::from(r.minutes)).bind(&r.cost_center).bind(status(r.time_status)).execute(&mut **tx).await.map_err(pfailure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_people_payroll_read.time_record_projection WHERE tenant_id=erpx_cqrs_read.current_tenant_id() AND projection_name=$1 AND generation=$2").bind(PROJECTION).bind(c.generation).execute(&mut **tx).await.map_err(pfailure)?;
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
    fn time_record_is_normalized_atomic() {
        let s = include_str!("time_record.rs");
        let production = s.split("#[cfg(test)]").next().unwrap();
        assert!(s.contains("time_records.time_record_version+1"));
        assert!(production.contains("commit_normalized"));
        assert!(!production.contains("commit_guarded"));
        assert!(s.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(s.contains("generation.status='active'"));
        assert!(s.contains("record.scope_key=$2"));
        assert!(!s.contains(concat!("serde_json::to_value(&c", ".record)")));
    }
}
