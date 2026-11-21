//! PostgreSQL adapter for the native PMS day-close commands.
//!
//! Only PMS-owned rows are read or written here: `pms_properties`, `pms_records`
//! and `pms_jobs` plus the shared `command_receipts`, `audit_events` and
//! `outbox_events` records. Finance balances, folio postings, People/Commercial
//! projections and HTTP clients never enter this unit. The caller owns the
//! serializable transaction; this adapter never commits or retries on its own.
use crate::adapters::room_board::record_row;
use crate::application::night_audit::PmsNightAuditRepository;
use crate::application::room_board::PmsBoardFuture;
use crate::domain::night_audit::{
    MAX_BLOCKS, MAX_NIGHT_ENTRIES, MAX_NIGHTS, MAX_RATES, MAX_ROOM_TYPES, MAX_ROOMS, MAX_SHIFTS,
    MAX_STAYS, MAX_TASKS, PmsNightAuditActor, PmsNightAuditCommand, PmsNightAuditFacts,
    PmsNightAuditOperation, PmsNightAuditPlan, PmsNightAuditResult, PmsNightJobPlan, read_limit,
    record_invalid,
};
use crate::domain::room_board::{PmsPropertyRow, PmsRecord, not_found, record_not_found};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Command facts retained from [`PmsNightAuditRepository::load`] for the
/// matching [`PmsNightAuditRepository::commit`]. The application trait
/// deliberately passes only the plan to commit, so the adapter keeps the
/// operation, the loaded property identity and version, and the night record
/// the load actually read.
#[derive(Clone, Debug)]
struct LoadedCommand {
    operation: PmsNightAuditOperation,
    property_id: String,
    expected_version: i64,
    business_date: String,
    state: String,
    target_id: Option<String>,
    result_is_target: bool,
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn version_conflict() -> BusinessError {
    conflict(
        "PMS_VERSION_CONFLICT",
        "Hotel data changed. Load the latest version and review again.",
    )
}

fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
}

fn unavailable(message: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_NIGHT_AUDIT_RECORD_INVALID",
        message,
    )
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PMS_NIGHT_AUDIT_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PMS_NIGHT_AUDIT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Take the reviewed PMS pre-snapshot fence before the first owner read.
///
/// The caller must invoke this (or let `load` invoke it) before `replay` reads a
/// retained receipt so the receipt and the facts come from one serializable
/// unit. `SHARE ROW EXCLUSIVE` excludes other PMS command writers before their
/// snapshots but still admits owner read transactions, so this writer never
/// upgrades a shared lock into an update and cannot deadlock with a peer
/// command. `NOWAIT` maps contention to `PRE_SNAPSHOT_LOCK_CONTENDED`; the
/// caller retries the complete transaction instead of waiting on another owner.
pub async fn lock_night_audit_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE pms_properties,pms_records IN SHARE ROW EXCLUSIVE MODE NOWAIT")
        .execute(&mut *c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

fn property_row(row: &PgRow) -> Result<PmsPropertyRow, Error> {
    Ok(PmsPropertyRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        name: row.try_get("name")?,
        business_date: row.try_get("business_date")?,
        state: row.try_get("state")?,
        config_json: row.try_get("config_json")?,
        version: row.try_get("version")?,
    })
}

async fn property(
    c: &mut PgConnection,
    actor: &PmsNightAuditActor,
    property_id: &str,
) -> Result<PmsPropertyRow, Error> {
    let row = sqlx::query(
        "SELECT id,company_id,branch_id,name,business_date,state,config_json,version \
         FROM pms_properties WHERE id=$1 AND company_id=$2 \
         AND ($3::text IS NULL OR branch_id=$3) FOR UPDATE",
    )
    .bind(property_id)
    .bind(&actor.board.company_id)
    .bind(actor.board.branch_id.as_deref())
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| Error::from(not_found()))?;
    property_row(&row)
}

async fn read_record(
    c: &mut PgConnection,
    property_id: &str,
    id: &str,
    kind: &'static str,
) -> Result<Option<PmsRecord>, Error> {
    let row = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND id=$2 AND kind=$3 FOR UPDATE",
    )
    .bind(property_id)
    .bind(id)
    .bind(kind)
    .fetch_optional(&mut *c)
    .await?;
    row.as_ref().map(record_row).transpose()
}

async fn required_record(
    c: &mut PgConnection,
    property_id: &str,
    id: &str,
    kind: &'static str,
) -> Result<PmsRecord, Error> {
    read_record(c, property_id, id, kind)
        .await?
        .ok_or_else(|| Error::from(record_not_found()))
}

async fn bounded_records(
    c: &mut PgConnection,
    property_id: &str,
    limit: usize,
    message: &'static str,
    sql: &'static str,
) -> Result<Vec<PmsRecord>, Error> {
    bounded_records_with(c, property_id, None, limit, message, sql).await
}

async fn bounded_records_with(
    c: &mut PgConnection,
    property_id: &str,
    filter: Option<&str>,
    limit: usize,
    message: &'static str,
    sql: &'static str,
) -> Result<Vec<PmsRecord>, Error> {
    let mut query = sqlx::query(sql).bind(property_id);
    if let Some(filter) = filter {
        query = query.bind(filter);
    }
    let rows = query.bind(limit as i64 + 1).fetch_all(&mut *c).await?;
    if rows.len() > limit {
        return Err(Error::from(read_limit(message)));
    }
    rows.iter().map(record_row).collect()
}

async fn nights(
    c: &mut PgConnection,
    property_id: &str,
    business_date: &str,
    target_id: Option<&str>,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='night' \
         AND (($2::text IS NOT NULL AND id=$2) OR data_json::jsonb->>'date'=$3) \
         ORDER BY id LIMIT $4",
    )
    .bind(property_id)
    .bind(target_id)
    .bind(business_date)
    .bind(MAX_NIGHTS as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_NIGHTS {
        return Err(Error::from(read_limit(
            "The hotel has more day-close jobs than this command can read.",
        )));
    }
    rows.iter().map(record_row).collect()
}

async fn stays(
    c: &mut PgConnection,
    property_id: &str,
    business_date: &str,
) -> Result<Vec<PmsRecord>, Error> {
    bounded_records_with(
        c,
        property_id,
        Some(business_date),
        MAX_STAYS,
        "The hotel has more stay facts than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='stay' \
         AND (data_json::jsonb->>'state' IN ('reserved','in_house') \
           OR (data_json::jsonb->>'departure' > $2 AND data_json::jsonb->>'arrival' <= $2)) \
         ORDER BY id LIMIT $3",
    )
    .await
}

async fn shifts(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_SHIFTS,
        "The hotel has more shift facts than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='shift' \
         AND (data_json::jsonb->>'state'='open' OR (data_json::jsonb->>'state'='closed' \
           AND (CASE WHEN data_json::jsonb->>'variance' ~ '^-?[0-9]+$' \
             THEN (data_json::jsonb->>'variance')::bigint ELSE 0 END) <> 0)) \
         ORDER BY id LIMIT $2",
    )
    .await
}

async fn room_types(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_ROOM_TYPES,
        "The hotel has more room types than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='room_type' ORDER BY id LIMIT $2",
    )
    .await
}

async fn rooms(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_ROOMS,
        "The hotel has more rooms than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='room' ORDER BY id LIMIT $2",
    )
    .await
}

async fn rates(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_RATES,
        "The hotel has more rate plans than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='rate' ORDER BY id LIMIT $2",
    )
    .await
}

async fn blocks(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_BLOCKS,
        "The hotel has more blocks than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='block' ORDER BY id LIMIT $2",
    )
    .await
}

async fn stayover_tasks(
    c: &mut PgConnection,
    property_id: &str,
    next_date: &str,
) -> Result<Vec<PmsRecord>, Error> {
    bounded_records_with(
        c,
        property_id,
        Some(next_date),
        MAX_TASKS,
        "The hotel has more room tasks than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='task' \
         AND data_json::jsonb->>'type'='stayover' AND data_json::jsonb->>'date'=$2 \
         ORDER BY id LIMIT $3",
    )
    .await
}

/// The night charges the retained `postNights` would create, limited to the
/// in-house stays of this business date. Reading only these keys keeps the
/// check bounded and index-backed (`pms_entry_source_key_reader`).
async fn night_entries(
    c: &mut PgConnection,
    property_id: &str,
    business_date: &str,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT r.id,r.kind,r.version,r.updated_at,r.data_json FROM pms_records r \
         WHERE r.property_id=$1 AND r.kind='entry' \
         AND r.data_json::jsonb->>'sourceKey' IN ( \
           SELECT 'night:'||s.id||':'||$2 FROM pms_records s \
           WHERE s.property_id=$1 AND s.kind='stay' \
           AND s.data_json::jsonb->>'state'='in_house' \
           AND s.data_json::jsonb->>'arrival' <= $2 \
           AND s.data_json::jsonb->>'departure' > $2) \
         ORDER BY r.id LIMIT $3",
    )
    .bind(property_id)
    .bind(business_date)
    .bind(MAX_NIGHT_ENTRIES as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_NIGHT_ENTRIES {
        return Err(Error::from(read_limit(
            "The hotel has more night charges than this command can read.",
        )));
    }
    rows.iter().map(record_row).collect()
}

fn record_json(record: &PmsRecord) -> Result<String, Error> {
    serde_json::to_string(&record.data)
        .map_err(|_| Error::from(unavailable("The hotel record could not be written safely.")))
}

async fn update_record(
    c: &mut PgConnection,
    property_id: &str,
    record: &PmsRecord,
) -> Result<(), Error> {
    if record.version < 2 {
        return Err(Error::from(record_invalid()));
    }
    let data = record_json(record)?;
    let changed = sqlx::query(
        "UPDATE pms_records SET data_json=$1,version=$2,updated_at=$3 \
         WHERE property_id=$4 AND id=$5 AND kind=$6 AND version=$7",
    )
    .bind(data)
    .bind(record.version)
    .bind(record.updated_at)
    .bind(property_id)
    .bind(&record.id)
    .bind(&record.kind)
    .bind(record.version - 1)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(Error::from(version_conflict()));
    }
    Ok(())
}

async fn insert_record(
    c: &mut PgConnection,
    property_id: &str,
    record: &PmsRecord,
) -> Result<(), Error> {
    if record.version != 1 {
        return Err(Error::from(record_invalid()));
    }
    let data = record_json(record)?;
    sqlx::query(
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(&record.id)
    .bind(property_id)
    .bind(&record.kind)
    .bind(data)
    .bind(record.version)
    .bind(record.updated_at)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn update_property(
    c: &mut PgConnection,
    actor: &PmsNightAuditActor,
    property_id: &str,
    expected_version: i64,
    state: &str,
    business_date: &str,
    version: i64,
    now: i64,
) -> Result<(), Error> {
    if version != expected_version + 1 {
        return Err(Error::from(version_conflict()));
    }
    let changed = sqlx::query(
        "UPDATE pms_properties SET business_date=$1,state=$2,version=$3,updated_at=$4 \
         WHERE id=$5 AND company_id=$6 AND version=$7",
    )
    .bind(business_date)
    .bind(state)
    .bind(version)
    .bind(now)
    .bind(property_id)
    .bind(&actor.board.company_id)
    .bind(expected_version)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(Error::from(version_conflict()));
    }
    Ok(())
}

/// Retained day-close job actor evidence. It records the selected authority
/// identity, never a permission or a session proof: composition re-resolves the
/// current Account authority before the worker rebuilds the actor.
pub(crate) fn authority_json(actor: &PmsNightAuditActor) -> String {
    json!({
        "companyId": actor.board.company_id,
        "branchId": actor.board.branch_id,
        "departmentId": actor.board.department_id,
        "userId": actor.board.user_id,
        "assignmentId": actor.assignment_id,
        "role": actor.role,
        "revision": actor.revision,
        "assignmentVersion": actor.assignment_version,
    })
    .to_string()
}

async fn enqueue_job(
    c: &mut PgConnection,
    actor: &PmsNightAuditActor,
    property_id: &str,
    business_date: &str,
    id: &str,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO pms_jobs(id,property_id,business_date,actor_json,updated_at) \
         VALUES($1,$2,$3,$4,$5) ON CONFLICT(property_id,business_date) DO UPDATE \
         SET state='pending',actor_json=excluded.actor_json,error_code=NULL,\
         updated_at=excluded.updated_at",
    )
    .bind(id)
    .bind(property_id)
    .bind(business_date)
    .bind(authority_json(actor))
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn complete_job(c: &mut PgConnection, id: &str, now: i64) -> Result<(), Error> {
    sqlx::query("UPDATE pms_jobs SET state='completed',error_code=NULL,updated_at=$1 WHERE id=$2")
        .bind(now)
        .bind(id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn insert_receipt(
    c: &mut PgConnection,
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    request_hash: &str,
    operation: PmsNightAuditOperation,
    result: &PmsNightAuditResult,
    now: i64,
) -> Result<(), Error> {
    let raw = serde_json::to_string(result)
        .map_err(|_| Error::from(unavailable("The hotel result could not be retained.")))?;
    let changed = sqlx::query(
        "INSERT INTO command_receipts\
         (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
         authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT(command_id) DO NOTHING",
    )
    .bind(key.as_str())
    .bind(operation.receipt_operation())
    .bind(&actor.board.user_id)
    .bind(request_hash)
    .bind(&result.property_id)
    .bind(raw)
    .bind(now)
    .bind(authority_json(actor))
    .execute(&mut *c)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(Error::from(idempotency_conflict()));
    }
    Ok(())
}

async fn insert_audit(
    c: &mut PgConnection,
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    operation: PmsNightAuditOperation,
    plan: &PmsNightAuditPlan,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events\
         (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
         record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10)",
    )
    .bind(format!("pms-night-audit-{}", key.as_str()))
    .bind(key.as_str())
    .bind(&actor.board.user_id)
    .bind(&actor.assignment_id)
    .bind(operation.receipt_operation())
    .bind("night")
    .bind(&plan.result.id)
    .bind(plan.property_version)
    .bind(now)
    .bind(authority_json(actor))
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn insert_outbox(
    c: &mut PgConnection,
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    operation: PmsNightAuditOperation,
    plan: &PmsNightAuditPlan,
    now: i64,
) -> Result<(), Error> {
    let authority: Value = serde_json::from_str(&authority_json(actor)).unwrap_or(Value::Null);
    let night = plan.night.as_ref().or(plan.new_night.as_ref());
    let payload = json!({
        "authority": authority,
        "operation": operation.receipt_operation(),
        "id": plan.result.id,
        "propertyId": plan.result.property_id,
        "businessDate": plan.property_business_date,
        "state": night.and_then(|row| row.data.get("state").cloned()).unwrap_or(Value::Null),
        "roomMetrics": night.and_then(|row| row.data.get("roomMetrics").cloned()).unwrap_or(Value::Null),
        "version": plan.property_version,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events\
         (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
         payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("pms-night-audit-outbox-{}", key.as_str()))
    .bind(key.as_str())
    .bind(operation.receipt_operation())
    .bind("night")
    .bind(&plan.result.id)
    .bind(plan.property_version)
    .bind(payload)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Reject a plan whose written rows do not match the operation the load read.
/// This is the adapter's own guard against a mismatched plan, not an
/// authorization decision.
fn shape_ok(loaded: &LoadedCommand, plan: &PmsNightAuditPlan) -> bool {
    let no_tasks = plan.tasks.is_empty();
    let no_blocks = plan.blocks.is_empty();
    match loaded.operation {
        PmsNightAuditOperation::Start => {
            plan.new_night.as_ref().is_some_and(|night| {
                night.version == 1
                    && night.id == plan.result.id
                    && night.data.get("date").and_then(Value::as_str)
                        == Some(loaded.business_date.as_str())
                    && night.data.get("state").and_then(Value::as_str) == Some("pending")
            }) && plan.night.is_none()
                && no_tasks
                && no_blocks
                && plan.property_state == "closing"
                && plan.property_business_date == loaded.business_date
                && plan.job == PmsNightJobPlan::Enqueue
        }
        PmsNightAuditOperation::Resume => {
            plan.night.as_ref().is_some_and(|night| {
                night.version >= 2
                    && night.id == plan.result.id
                    && night.data.get("state").and_then(Value::as_str) == Some("pending")
            }) && plan.new_night.is_none()
                && no_tasks
                && no_blocks
                && plan.property_state == loaded.property_state()
                && plan.property_business_date == loaded.business_date
                && plan.job == PmsNightJobPlan::Enqueue
        }
        PmsNightAuditOperation::Step => {
            plan.night.as_ref().is_some_and(|night| {
                night.version >= 2
                    && night.id == plan.result.id
                    && night.data.get("state").and_then(Value::as_str) == Some("completed")
            }) && plan.new_night.is_none()
                && plan.property_state == "open"
                && plan.property_business_date > loaded.business_date
                && plan.job == PmsNightJobPlan::Complete
        }
    }
}

impl LoadedCommand {
    fn property_state(&self) -> &str {
        self.state.as_str()
    }
}

/// SQL repository bound to the caller's retained serializable unit.
pub struct PgPmsNightAuditRepository<'a> {
    connection: &'a mut PgConnection,
    loaded: Option<LoadedCommand>,
}

impl<'a> PgPmsNightAuditRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection,
            loaded: None,
        }
    }
}

impl PmsNightAuditRepository for PgPmsNightAuditRepository<'_> {
    type Error = Error;

    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsNightAuditCommand,
    ) -> PmsBoardFuture<'a, Option<PmsNightAuditResult>, Self::Error> {
        Box::pin(async move {
            serializable(self.connection).await?;
            let row = sqlx::query(
                "SELECT operation,request_hash,record_id,result_json \
                 FROM command_receipts WHERE command_id=$1 FOR UPDATE",
            )
            .bind(key.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let operation: String = row.try_get("operation")?;
            let stored_hash: String = row.try_get("request_hash")?;
            let record_id: String = row.try_get("record_id")?;
            if operation != command.operation().receipt_operation()
                || stored_hash != request_hash
                || record_id != command.property_id
            {
                return Err(Error::from(idempotency_conflict()));
            }
            let raw: Option<String> = row.try_get("result_json")?;
            let result = raw
                .and_then(|value| serde_json::from_str::<PmsNightAuditResult>(&value).ok())
                .ok_or_else(|| {
                    Error::from(conflict(
                        "IDEMPOTENCY_CONFLICT",
                        "The retained result is unavailable.",
                    ))
                })?;
            Ok(Some(result))
        })
    }

    fn load<'a>(
        &'a mut self,
        actor: &'a PmsNightAuditActor,
        command: &'a PmsNightAuditCommand,
    ) -> PmsBoardFuture<'a, PmsNightAuditFacts, Self::Error> {
        Box::pin(async move {
            lock_night_audit_sources(self.connection).await?;
            self.loaded = None;
            let property = property(self.connection, actor, &command.property_id).await?;
            let operation = command.operation();
            let target_id = command.target_id().map(str::to_owned);
            let business_date = property.business_date.clone();
            let mut facts = PmsNightAuditFacts::empty(property.clone());
            if let Some(target) = &target_id {
                facts.night =
                    Some(required_record(self.connection, &property.id, target, "night").await?);
            }
            // The worker-only step still needs the same bounded PMS facts; the
            // user commands add the guard reads through the public port.
            facts.nights_on_date = nights(
                self.connection,
                &property.id,
                &business_date,
                target_id.as_deref(),
            )
            .await?;
            facts.stays = stays(self.connection, &property.id, &business_date).await?;
            facts.shifts = shifts(self.connection, &property.id).await?;
            facts.room_types = room_types(self.connection, &property.id).await?;
            facts.rooms = rooms(self.connection, &property.id).await?;
            facts.rates = rates(self.connection, &property.id).await?;
            facts.blocks = blocks(self.connection, &property.id).await?;
            if operation == PmsNightAuditOperation::Step {
                let next = crate::domain::night_audit::next_business_date(&business_date)?;
                facts.stayover_tasks = stayover_tasks(self.connection, &property.id, &next).await?;
                facts.night_entries =
                    night_entries(self.connection, &property.id, &business_date).await?;
            }
            self.loaded = Some(LoadedCommand {
                operation,
                property_id: property.id.clone(),
                expected_version: property.version,
                business_date,
                target_id,
                result_is_target: command.result_id_is_target(),
                state: property.state.clone(),
            });
            Ok(facts)
        })
    }

    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsNightAuditActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsNightAuditPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsNightAuditResult, Self::Error> {
        Box::pin(async move {
            serializable(self.connection).await?;
            let loaded = self.loaded.take().ok_or_else(|| {
                Error::from(PersistenceError::new("PMS_NIGHT_AUDIT_LOAD_REQUIRED"))
            })?;
            if plan.result.property_id != loaded.property_id
                || plan.result.version != loaded.expected_version + 1
                || plan.property_version != loaded.expected_version + 1
                || (loaded.result_is_target
                    && loaded
                        .target_id
                        .as_ref()
                        .is_some_and(|target| target != &plan.result.id))
                || !shape_ok(&loaded, &plan)
            {
                return Err(Error::from(conflict(
                    "PMS_NIGHT_AUDIT_RESULT_INVALID",
                    "The hotel day-close result is unavailable.",
                )));
            }
            insert_receipt(
                self.connection,
                actor,
                key,
                request_hash,
                loaded.operation,
                &plan.result,
                now,
            )
            .await?;
            let property_id = plan.result.property_id.clone();
            if let Some(night) = &plan.new_night {
                insert_record(self.connection, &property_id, night).await?;
            }
            if let Some(night) = &plan.night {
                update_record(self.connection, &property_id, night).await?;
            }
            for task in &plan.tasks {
                insert_record(self.connection, &property_id, task).await?;
            }
            for block in &plan.blocks {
                update_record(self.connection, &property_id, block).await?;
            }
            update_property(
                self.connection,
                actor,
                &property_id,
                loaded.expected_version,
                &plan.property_state,
                &plan.property_business_date,
                plan.property_version,
                now,
            )
            .await?;
            match plan.job {
                PmsNightJobPlan::Enqueue => {
                    enqueue_job(
                        self.connection,
                        actor,
                        &property_id,
                        &plan.property_business_date,
                        &plan.result.id,
                        now,
                    )
                    .await?
                }
                PmsNightJobPlan::Complete => {
                    complete_job(self.connection, &plan.result.id, now).await?
                }
            }
            insert_audit(self.connection, actor, key, loaded.operation, &plan, now).await?;
            insert_outbox(self.connection, actor, key, loaded.operation, &plan, now).await?;
            Ok(plan.result)
        })
    }
}
