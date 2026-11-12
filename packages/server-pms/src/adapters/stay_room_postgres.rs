//! PostgreSQL adapter for the native PMS `stay.assign`/`stay.move` commands.
//!
//! Only PMS-owned rows are read or written here: `pms_properties`, `pms_records`
//! and the shared `command_receipts`, `audit_events` and `outbox_events`
//! records. Finance balances, People/Commercial projections, HTTP clients and
//! browser state never enter this unit. The caller owns the serializable
//! transaction; this adapter never commits or retries on its own.
use crate::adapters::room_board::record_row;
use crate::application::room_board::PmsBoardFuture;
use crate::application::stay_room::PmsStayRoomRepository;
use crate::domain::room_board::{PmsPropertyRow, PmsRecord, not_found, record_not_found};
use crate::domain::stay_room::{
    MAX_BLOCKS, MAX_STAYS, MAX_TASKS, PmsStayRoomActor, PmsStayRoomCommand, PmsStayRoomFacts,
    PmsStayRoomOperation, PmsStayRoomPlan, PmsStayRoomResult, read_limit, record_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Command facts retained from [`PmsStayRoomRepository::load`] for the matching
/// [`PmsStayRoomRepository::commit`]. The application trait deliberately passes
/// only the plan to commit, so the adapter keeps the operation and loaded
/// property/stay identities plus the expected property version from the
/// snapshot it actually read.
#[derive(Clone, Debug)]
struct LoadedCommand {
    operation: PmsStayRoomOperation,
    property_id: String,
    stay_id: String,
    expected_version: i64,
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
        "PMS_STAY_ROOM_RECORD_INVALID",
        message,
    )
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PMS_STAY_ROOM_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PMS_STAY_ROOM_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Take the reviewed PMS pre-snapshot fence before the first owner read.
///
/// The caller must invoke this (or let `load` invoke it) before `replay` reads a
/// retained receipt so the receipt and the facts come from one serializable
/// unit. `NOWAIT` maps lock contention to `PRE_SNAPSHOT_LOCK_CONTENDED`; the
/// caller retries the complete transaction instead of waiting on another owner.
pub async fn lock_stay_room_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE pms_properties,pms_records IN SHARE MODE NOWAIT")
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
    actor: &PmsStayRoomActor,
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
    let rows = sqlx::query(sql)
        .bind(property_id)
        .bind(limit as i64 + 1)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > limit {
        return Err(Error::from(read_limit(message)));
    }
    rows.iter().map(record_row).collect()
}

async fn availability_stays(
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
        "The hotel has more maintenance blocks than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='block' \
         AND data_json::jsonb->>'kind'='maintenance' \
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
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='task' AND data_json::jsonb->>'roomId'=$2 \
         ORDER BY id LIMIT $3",
    )
    .bind(property_id)
    .bind(room_id)
    .bind(MAX_TASKS as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_TASKS {
        return Err(Error::from(read_limit(
            "The hotel has more room tasks than this command can read.",
        )));
    }
    rows.iter().map(record_row).collect()
}

fn record_text<'a>(record: &'a PmsRecord, key: &'static str) -> &'a str {
    record
        .data
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
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

async fn insert_task(
    c: &mut PgConnection,
    property_id: &str,
    task: &PmsRecord,
) -> Result<(), Error> {
    let data = record_json(task)?;
    sqlx::query(
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&task.id)
    .bind(property_id)
    .bind(&task.kind)
    .bind(data)
    .bind(task.version)
    .bind(task.updated_at)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn update_property(
    c: &mut PgConnection,
    actor: &PmsStayRoomActor,
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

fn authority_json(actor: &PmsStayRoomActor) -> String {
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
    actor: &PmsStayRoomActor,
    key: &CommandKey,
    request_hash: &str,
    operation: PmsStayRoomOperation,
    result: &PmsStayRoomResult,
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
    .bind(operation.as_str())
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
    actor: &PmsStayRoomActor,
    key: &CommandKey,
    operation: PmsStayRoomOperation,
    plan: &PmsStayRoomPlan,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events\
         (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
         record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,'stay',$6,$7,$5,$8,$9)",
    )
    .bind(format!("pms-stay-audit-{}", key.as_str()))
    .bind(key.as_str())
    .bind(&actor.board.user_id)
    .bind(&actor.assignment_id)
    .bind(operation.as_str())
    .bind(&plan.stay.id)
    .bind(plan.property_version)
    .bind(now)
    .bind(authority_json(actor))
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn insert_outbox(
    c: &mut PgConnection,
    actor: &PmsStayRoomActor,
    key: &CommandKey,
    operation: PmsStayRoomOperation,
    plan: &PmsStayRoomPlan,
    now: i64,
) -> Result<(), Error> {
    let authority: Value = serde_json::from_str(&authority_json(actor)).unwrap_or(Value::Null);
    let payload = json!({
        "authority": authority,
        "operation": operation.as_str(),
        "id": plan.stay.id,
        "propertyId": plan.result.property_id,
        "roomId": plan.stay.data.get("roomId").cloned().unwrap_or(Value::Null),
        "state": plan.stay.data.get("state").cloned().unwrap_or(Value::Null),
        "version": plan.property_version,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events\
         (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
         payload_json,created_at) VALUES($1,$2,$3,'stay',$4,$5,$6,$7)",
    )
    .bind(format!("pms-stay-outbox-{}", key.as_str()))
    .bind(key.as_str())
    .bind(operation.as_str())
    .bind(&plan.stay.id)
    .bind(plan.property_version)
    .bind(payload)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// SQL repository bound to the caller's retained serializable unit.
pub struct PgPmsStayRoomRepository<'a> {
    connection: &'a mut PgConnection,
    loaded: Option<LoadedCommand>,
}

impl<'a> PgPmsStayRoomRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection,
            loaded: None,
        }
    }
}

impl PmsStayRoomRepository for PgPmsStayRoomRepository<'_> {
    type Error = Error;

    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsStayRoomCommand,
    ) -> PmsBoardFuture<'a, Option<PmsStayRoomResult>, Self::Error> {
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
            if operation != command.operation().as_str()
                || stored_hash != request_hash
                || record_id != command.property_id
            {
                return Err(Error::from(idempotency_conflict()));
            }
            let raw: Option<String> = row.try_get("result_json")?;
            let result = raw
                .and_then(|value| serde_json::from_str::<PmsStayRoomResult>(&value).ok())
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
        actor: &'a PmsStayRoomActor,
        command: &'a PmsStayRoomCommand,
    ) -> PmsBoardFuture<'a, PmsStayRoomFacts, Self::Error> {
        Box::pin(async move {
            lock_stay_room_sources(self.connection).await?;
            self.loaded = None;
            let property = property(self.connection, actor, &command.property_id).await?;
            let stay =
                required_record(self.connection, &property.id, &command.stay_id, "stay").await?;
            let reservation_id = record_text(&stay, "reservationId");
            let reservation = if reservation_id.is_empty() {
                None
            } else {
                read_record(self.connection, &property.id, reservation_id, "reservation").await?
            };
            let room =
                required_record(self.connection, &property.id, &command.room_id, "room").await?;
            let old_room_id = record_text(&stay, "roomId");
            let moving_in_house =
                record_text(&stay, "state") == "in_house" && !old_room_id.is_empty();
            let old_room = if moving_in_house && old_room_id != command.room_id {
                read_record(self.connection, &property.id, old_room_id, "room").await?
            } else {
                None
            };
            let stays = availability_stays(self.connection, &property.id).await?;
            let blocks = active_blocks(self.connection, &property.id).await?;
            let tasks = match &old_room {
                Some(old) => room_tasks(self.connection, &property.id, &old.id).await?,
                None => Vec::new(),
            };
            self.loaded = Some(LoadedCommand {
                operation: command.operation(),
                property_id: property.id.clone(),
                stay_id: stay.id.clone(),
                expected_version: property.version,
            });
            Ok(PmsStayRoomFacts {
                property,
                stay,
                reservation,
                room,
                old_room,
                stays,
                blocks,
                tasks,
            })
        })
    }

    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsStayRoomActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsStayRoomPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsStayRoomResult, Self::Error> {
        Box::pin(async move {
            serializable(self.connection).await?;
            let loaded = self
                .loaded
                .take()
                .ok_or_else(|| Error::from(PersistenceError::new("PMS_STAY_ROOM_LOAD_REQUIRED")))?;
            if plan.result.id != loaded.stay_id
                || plan.stay.id != loaded.stay_id
                || plan.result.property_id != loaded.property_id
                || plan.result.version != loaded.expected_version + 1
                || plan.property_version != loaded.expected_version + 1
            {
                return Err(Error::from(conflict(
                    "PMS_STAY_ROOM_RESULT_INVALID",
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
            update_record(self.connection, &property_id, &plan.stay).await?;
            if let Some(reservation) = &plan.reservation {
                update_record(self.connection, &property_id, reservation).await?;
            }
            if let Some(old_room) = &plan.old_room {
                update_record(self.connection, &property_id, old_room).await?;
            }
            for task in &plan.superseded_tasks {
                update_record(self.connection, &property_id, task).await?;
            }
            if let Some(task) = &plan.departure_task {
                insert_task(self.connection, &property_id, task).await?;
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
