//! PostgreSQL adapter for the native PMS room and housekeeping commands.
//!
//! Only PMS-owned rows are read or written here: `pms_properties`, `pms_records`
//! and the shared `command_receipts`, `audit_events` and `outbox_events`
//! records. Finance balances, People/Commercial projections, HTTP clients and
//! browser state never enter this unit. The caller owns the serializable
//! transaction; this adapter never commits or retries on its own.
use crate::adapters::room_board::record_row;
use crate::application::room_board::PmsBoardFuture;
use crate::application::room_command::PmsRoomCommandRepository;
use crate::domain::room_board::{PmsPropertyRow, PmsRecord, not_found, record_not_found};
use crate::domain::room_command::{
    MAX_BLOCKS, MAX_ROOM_TYPES, MAX_ROOMS, MAX_STAYS, MAX_TASKS, PmsRoomCommand,
    PmsRoomCommandActor, PmsRoomCommandFacts, PmsRoomCommandInput, PmsRoomCommandOperation,
    PmsRoomCommandPlan, PmsRoomCommandResult, read_limit, record_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Command facts retained from [`PmsRoomCommandRepository::load`] for the
/// matching [`PmsRoomCommandRepository::commit`]. The application trait
/// deliberately passes only the plan to commit, so the adapter keeps the
/// operation and loaded property/target identities plus the expected property
/// version from the snapshot it actually read.
#[derive(Clone, Debug)]
struct LoadedCommand {
    operation: PmsRoomCommandOperation,
    property_id: String,
    expected_version: i64,
    /// Room/type/task/block id the load actually read. Absent only for creation
    /// commands (`room.save`, `room_type.save`, `block.create`).
    target_id: Option<String>,
    /// Whether the retained result id is that target id.
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
        "PMS_ROOM_COMMAND_RECORD_INVALID",
        message,
    )
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PMS_ROOM_COMMAND_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PMS_ROOM_COMMAND_SERIALIZABLE_REQUIRED").into());
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
pub async fn lock_room_command_sources(c: &mut PgConnection) -> Result<(), Error> {
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
    actor: &PmsRoomCommandActor,
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

fn record_text<'a>(record: &'a PmsRecord, key: &'static str) -> &'a str {
    record
        .data
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
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

async fn nonterminal_stays(
    c: &mut PgConnection,
    property_id: &str,
) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_STAYS,
        "The hotel has more stay facts than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='stay' \
         AND data_json::jsonb->>'state' IN ('reserved','in_house') \
         ORDER BY id LIMIT $2",
    )
    .await
}

async fn active_blocks(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_BLOCKS,
        "The hotel has more active blocks than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='block' \
         AND data_json::jsonb->>'state'='active' \
         ORDER BY id LIMIT $2",
    )
    .await
}

async fn room_tasks(
    c: &mut PgConnection,
    property_id: &str,
    room_id: &str,
) -> Result<Vec<PmsRecord>, Error> {
    bounded_records_with(
        c,
        property_id,
        Some(room_id),
        MAX_TASKS,
        "The hotel has more room tasks than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='task' AND data_json::jsonb->>'roomId'=$2 \
         ORDER BY id LIMIT $3",
    )
    .await
}

async fn in_house(c: &mut PgConnection, property_id: &str, room_id: &str) -> Result<bool, Error> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pms_records WHERE property_id=$1 AND kind='stay' \
         AND data_json::jsonb->>'state'='in_house' AND data_json::jsonb->>'roomId'=$2)",
    )
    .bind(property_id)
    .bind(room_id)
    .fetch_one(&mut *c)
    .await?;
    Ok(exists)
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

/// Insert a record this command created. `require_new` is true for a room the
/// command minted, where an existing id is an anomaly, and false for the
/// retained `task()` dedupe creation, where an existing row is the retained
/// outcome.
async fn insert_record(
    c: &mut PgConnection,
    property_id: &str,
    record: &PmsRecord,
    require_new: bool,
) -> Result<(), Error> {
    if record.version != 1 {
        return Err(Error::from(record_invalid()));
    }
    let data = record_json(record)?;
    let changed = sqlx::query(
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&record.id)
    .bind(property_id)
    .bind(&record.kind)
    .bind(data)
    .bind(record.version)
    .bind(record.updated_at)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if require_new && changed != 1 {
        return Err(Error::from(version_conflict()));
    }
    Ok(())
}

async fn update_property(
    c: &mut PgConnection,
    actor: &PmsRoomCommandActor,
    property_id: &str,
    expected_version: i64,
    version: i64,
    now: i64,
) -> Result<(), Error> {
    if version != expected_version + 1 {
        return Err(Error::from(version_conflict()));
    }
    let changed = sqlx::query(
        "UPDATE pms_properties SET version=$1,updated_at=$2 \
         WHERE id=$3 AND company_id=$4 AND version=$5",
    )
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

fn authority_json(actor: &PmsRoomCommandActor) -> String {
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

async fn insert_receipt(
    c: &mut PgConnection,
    actor: &PmsRoomCommandActor,
    key: &CommandKey,
    request_hash: &str,
    operation: PmsRoomCommandOperation,
    result: &PmsRoomCommandResult,
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

fn aggregate(operation: PmsRoomCommandOperation) -> &'static str {
    match operation {
        PmsRoomCommandOperation::RoomTypeSave => "room_type",
        PmsRoomCommandOperation::MaintenanceOpen
        | PmsRoomCommandOperation::MaintenanceClose
        | PmsRoomCommandOperation::BlockCreate
        | PmsRoomCommandOperation::BlockRelease => "block",
        operation if operation.as_str().starts_with("task.") => "task",
        _ => "room",
    }
}

/// Reject a plan whose written rows do not match the operation the load read.
/// This is the adapter's own guard against a mismatched plan, not an
/// authorization decision.
fn shape_ok(loaded: &LoadedCommand, plan: &PmsRoomCommandPlan) -> bool {
    let no_room = plan.room.is_none() && plan.new_room.is_none();
    let no_room_type = plan.room_type.is_none() && plan.new_room_type.is_none();
    let no_block = plan.block.is_none() && plan.new_block.is_none();
    let no_task = plan.task.is_none() && plan.new_task.is_none();
    let no_supersede = plan.superseded_tasks.is_empty();
    let creates = |id: &str, record: Option<&PmsRecord>| {
        record.is_some_and(|record| record.version == 1 && record.id == id)
    };
    let updates = |id: &str, record: Option<&PmsRecord>| {
        record.is_some_and(|record| record.version >= 2 && record.id == id)
    };
    match loaded.operation {
        PmsRoomCommandOperation::RoomTypeSave if loaded.target_id.is_none() => {
            creates(&plan.result.id, plan.new_room_type.as_ref())
                && plan.room_type.is_none()
                && no_room
                && no_block
                && no_task
                && no_supersede
        }
        PmsRoomCommandOperation::RoomTypeSave => {
            updates(&plan.result.id, plan.room_type.as_ref())
                && plan.new_room_type.is_none()
                && no_room
                && no_block
                && no_task
                && no_supersede
        }
        PmsRoomCommandOperation::Save if loaded.target_id.is_none() => {
            creates(&plan.result.id, plan.new_room.as_ref())
                && plan.room.is_none()
                && no_room_type
                && no_block
                && no_task
                && no_supersede
        }
        PmsRoomCommandOperation::Save => {
            updates(&plan.result.id, plan.room.as_ref())
                && plan.new_room.is_none()
                && no_room_type
                && no_block
                && no_task
                && no_supersede
        }
        PmsRoomCommandOperation::Dnd => {
            updates(&plan.result.id, plan.room.as_ref())
                && plan.new_room.is_none()
                && no_room_type
                && no_block
                && no_task
                && no_supersede
        }
        PmsRoomCommandOperation::Observe => {
            updates(&plan.result.id, plan.room.as_ref())
                && plan.new_room.is_none()
                && no_room_type
                && no_block
                && plan.task.is_none()
                && no_supersede
        }
        PmsRoomCommandOperation::TaskAssign
        | PmsRoomCommandOperation::TaskStart
        | PmsRoomCommandOperation::TaskComplete
        | PmsRoomCommandOperation::TaskInspect => {
            updates(&plan.result.id, plan.task.as_ref())
                && plan.new_room.is_none()
                && no_room_type
                && no_block
                && no_supersede
                && plan.new_task.is_none()
        }
        PmsRoomCommandOperation::TaskCreate => {
            no_room
                && no_room_type
                && no_block
                && no_supersede
                && plan.task.is_none()
                && plan
                    .new_task
                    .as_ref()
                    .is_none_or(|task| task.version == 1 && task.id == plan.result.id)
        }
        PmsRoomCommandOperation::MaintenanceOpen => {
            creates(&plan.result.id, plan.new_block.as_ref())
                && plan.block.is_none()
                && no_room
                && no_room_type
                && plan.task.is_none()
                && no_supersede
        }
        PmsRoomCommandOperation::MaintenanceClose => {
            updates(&plan.result.id, plan.block.as_ref())
                && plan.new_block.is_none()
                && no_room_type
                && plan.task.is_none()
                && plan.room.as_ref().is_some_and(|room| room.version >= 2)
                && plan.new_room.is_none()
        }
        PmsRoomCommandOperation::BlockCreate => {
            creates(&plan.result.id, plan.new_block.as_ref())
                && plan.block.is_none()
                && no_room
                && no_room_type
                && no_task
                && no_supersede
        }
        PmsRoomCommandOperation::BlockRelease => {
            updates(&plan.result.id, plan.block.as_ref())
                && plan.new_block.is_none()
                && no_room
                && no_room_type
                && no_task
                && no_supersede
        }
    }
}

async fn insert_audit(
    c: &mut PgConnection,
    actor: &PmsRoomCommandActor,
    key: &CommandKey,
    operation: PmsRoomCommandOperation,
    plan: &PmsRoomCommandPlan,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events\
         (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
         record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10)",
    )
    .bind(format!("pms-room-audit-{}", key.as_str()))
    .bind(key.as_str())
    .bind(&actor.board.user_id)
    .bind(&actor.assignment_id)
    .bind(operation.receipt_operation())
    .bind(aggregate(operation))
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
    actor: &PmsRoomCommandActor,
    key: &CommandKey,
    operation: PmsRoomCommandOperation,
    plan: &PmsRoomCommandPlan,
    now: i64,
) -> Result<(), Error> {
    let authority: Value = serde_json::from_str(&authority_json(actor)).unwrap_or(Value::Null);
    let record = plan
        .room
        .as_ref()
        .or(plan.new_room.as_ref())
        .or(plan.block.as_ref())
        .or(plan.new_block.as_ref())
        .or(plan.task.as_ref())
        .or(plan.new_task.as_ref());
    let room_id = match operation {
        PmsRoomCommandOperation::RoomTypeSave => Value::Null,
        PmsRoomCommandOperation::Save
        | PmsRoomCommandOperation::Dnd
        | PmsRoomCommandOperation::Observe => Value::String(plan.result.id.clone()),
        PmsRoomCommandOperation::BlockCreate => Value::String(String::new()),
        _ => record
            .and_then(|row| row.data.get("roomId").cloned())
            .unwrap_or(Value::Null),
    };
    let payload = json!({
        "authority": authority,
        "operation": operation.receipt_operation(),
        "id": plan.result.id,
        "propertyId": plan.result.property_id,
        "roomId": room_id,
        "state": record
            .and_then(|row| row.data.get("state").cloned())
            .or_else(|| record.and_then(|row| row.data.get("cleanliness").cloned()))
            .unwrap_or(Value::Null),
        "version": plan.property_version,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events\
         (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
         payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("pms-room-outbox-{}", key.as_str()))
    .bind(key.as_str())
    .bind(operation.receipt_operation())
    .bind(aggregate(operation))
    .bind(&plan.result.id)
    .bind(plan.property_version)
    .bind(payload)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// SQL repository bound to the caller's retained serializable unit.
pub struct PgPmsRoomCommandRepository<'a> {
    connection: &'a mut PgConnection,
    loaded: Option<LoadedCommand>,
}

impl<'a> PgPmsRoomCommandRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection,
            loaded: None,
        }
    }
}

impl PmsRoomCommandRepository for PgPmsRoomCommandRepository<'_> {
    type Error = Error;

    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsRoomCommand,
    ) -> PmsBoardFuture<'a, Option<PmsRoomCommandResult>, Self::Error> {
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
                .and_then(|value| serde_json::from_str::<PmsRoomCommandResult>(&value).ok())
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
        actor: &'a PmsRoomCommandActor,
        command: &'a PmsRoomCommand,
    ) -> PmsBoardFuture<'a, PmsRoomCommandFacts, Self::Error> {
        Box::pin(async move {
            lock_room_command_sources(self.connection).await?;
            self.loaded = None;
            let property = property(self.connection, actor, &command.property_id).await?;
            let operation = command.operation();
            let target_id = command.input.target_id().map(str::to_owned);
            let mut facts = PmsRoomCommandFacts::empty(property.clone());
            match operation {
                PmsRoomCommandOperation::RoomTypeSave => {
                    if let Some(type_id) = &target_id {
                        facts.room_type = Some(
                            required_record(self.connection, &property.id, type_id, "room_type")
                                .await?,
                        );
                        facts.stays = nonterminal_stays(self.connection, &property.id).await?;
                    }
                }
                PmsRoomCommandOperation::Save => {
                    if let Some(room_id) = &target_id {
                        facts.room = Some(
                            required_record(self.connection, &property.id, room_id, "room").await?,
                        );
                    }
                    let type_id = match &command.input {
                        PmsRoomCommandInput::Save(input) => input.type_id.clone(),
                        _ => return Err(Error::from(record_invalid())),
                    };
                    facts.room_type = Some(
                        required_record(self.connection, &property.id, &type_id, "room_type")
                            .await?,
                    );
                    facts.rooms = rooms(self.connection, &property.id).await?;
                    facts.room_types = room_types(self.connection, &property.id).await?;
                    facts.stays = nonterminal_stays(self.connection, &property.id).await?;
                    facts.blocks = active_blocks(self.connection, &property.id).await?;
                }
                PmsRoomCommandOperation::Dnd | PmsRoomCommandOperation::Observe => {
                    let room_id = target_id.as_deref().ok_or_else(record_invalid)?;
                    let room =
                        required_record(self.connection, &property.id, room_id, "room").await?;
                    facts.room_tasks = room_tasks(self.connection, &property.id, room_id).await?;
                    facts.in_house = in_house(self.connection, &property.id, room_id).await?;
                    facts.room = Some(room);
                }
                PmsRoomCommandOperation::TaskCreate => {
                    let room_id = target_id.as_deref().ok_or_else(record_invalid)?;
                    facts.room_tasks = room_tasks(self.connection, &property.id, room_id).await?;
                    facts.room = Some(
                        required_record(self.connection, &property.id, room_id, "room").await?,
                    );
                }
                PmsRoomCommandOperation::TaskAssign
                | PmsRoomCommandOperation::TaskStart
                | PmsRoomCommandOperation::TaskComplete
                | PmsRoomCommandOperation::TaskInspect => {
                    let task_id = target_id.as_deref().ok_or_else(record_invalid)?;
                    let task =
                        required_record(self.connection, &property.id, task_id, "task").await?;
                    let room_id = record_text(&task, "roomId").to_owned();
                    if !room_id.is_empty() {
                        facts.room = Some(
                            required_record(self.connection, &property.id, &room_id, "room")
                                .await?,
                        );
                    }
                    facts.task = Some(task);
                }
                PmsRoomCommandOperation::MaintenanceOpen => {
                    let room_id = target_id.as_deref().ok_or_else(record_invalid)?;
                    facts.room_tasks = room_tasks(self.connection, &property.id, room_id).await?;
                    facts.room = Some(
                        required_record(self.connection, &property.id, room_id, "room").await?,
                    );
                    facts.rooms = rooms(self.connection, &property.id).await?;
                    facts.room_types = room_types(self.connection, &property.id).await?;
                    facts.stays = nonterminal_stays(self.connection, &property.id).await?;
                    facts.blocks = active_blocks(self.connection, &property.id).await?;
                }
                PmsRoomCommandOperation::MaintenanceClose => {
                    let block_id = target_id.as_deref().ok_or_else(record_invalid)?;
                    let block =
                        required_record(self.connection, &property.id, block_id, "block").await?;
                    let room_id = record_text(&block, "roomId").to_owned();
                    if !room_id.is_empty() {
                        facts.room_tasks =
                            room_tasks(self.connection, &property.id, &room_id).await?;
                        facts.room = Some(
                            required_record(self.connection, &property.id, &room_id, "room")
                                .await?,
                        );
                    }
                    facts.block = Some(block);
                }
                PmsRoomCommandOperation::BlockCreate => {
                    let type_id = match &command.input {
                        PmsRoomCommandInput::BlockCreate(input) => input.type_id.clone(),
                        _ => return Err(Error::from(record_invalid())),
                    };
                    facts.room_type = Some(
                        required_record(self.connection, &property.id, &type_id, "room_type")
                            .await?,
                    );
                    facts.rooms = rooms(self.connection, &property.id).await?;
                    facts.room_types = room_types(self.connection, &property.id).await?;
                    facts.stays = nonterminal_stays(self.connection, &property.id).await?;
                    facts.blocks = active_blocks(self.connection, &property.id).await?;
                }
                PmsRoomCommandOperation::BlockRelease => {
                    let block_id = target_id.as_deref().ok_or_else(record_invalid)?;
                    facts.block = Some(
                        required_record(self.connection, &property.id, block_id, "block").await?,
                    );
                }
            }
            self.loaded = Some(LoadedCommand {
                operation,
                property_id: property.id.clone(),
                expected_version: property.version,
                target_id,
                result_is_target: command.input.result_id_is_target(),
            });
            Ok(facts)
        })
    }

    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsRoomCommandActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsRoomCommandPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsRoomCommandResult, Self::Error> {
        Box::pin(async move {
            serializable(self.connection).await?;
            let loaded = self.loaded.take().ok_or_else(|| {
                Error::from(PersistenceError::new("PMS_ROOM_COMMAND_LOAD_REQUIRED"))
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
                    "PMS_ROOM_COMMAND_RESULT_INVALID",
                    "The hotel command result is unavailable.",
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
            if let Some(room_type) = &plan.room_type {
                update_record(self.connection, &property_id, room_type).await?;
            }
            if let Some(room_type) = &plan.new_room_type {
                insert_record(self.connection, &property_id, room_type, true).await?;
            }
            if let Some(room) = &plan.room {
                update_record(self.connection, &property_id, room).await?;
            }
            if let Some(room) = &plan.new_room {
                insert_record(self.connection, &property_id, room, true).await?;
            }
            if let Some(block) = &plan.block {
                update_record(self.connection, &property_id, block).await?;
            }
            if let Some(block) = &plan.new_block {
                insert_record(self.connection, &property_id, block, true).await?;
            }
            for task in &plan.superseded_tasks {
                update_record(self.connection, &property_id, task).await?;
            }
            if let Some(task) = &plan.task {
                update_record(self.connection, &property_id, task).await?;
            }
            if let Some(task) = &plan.new_task {
                insert_record(self.connection, &property_id, task, false).await?;
            }
            update_property(
                self.connection,
                actor,
                &property_id,
                loaded.expected_version,
                plan.property_version,
                now,
            )
            .await?;
            insert_audit(self.connection, actor, key, loaded.operation, &plan, now).await?;
            insert_outbox(self.connection, actor, key, loaded.operation, &plan, now).await?;
            Ok(plan.result)
        })
    }
}
