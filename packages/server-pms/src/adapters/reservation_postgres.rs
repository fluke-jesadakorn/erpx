//! PostgreSQL adapter for the native PMS staff reservation commands.
//!
//! Only PMS-owned rows are read or written here: `pms_properties`, `pms_records`,
//! `pms_sources` and the shared `command_receipts`, `audit_events` and
//! `outbox_events` records. Finance balances, payment providers, email delivery,
//! People/Commercial projections and browser state never enter this unit. The
//! caller owns the serializable transaction; this adapter never commits or
//! retries on its own.
use crate::adapters::room_board::record_row;
use crate::application::reservation::PmsReservationRepository;
use crate::application::room_board::PmsBoardFuture;
use crate::domain::reservation::{
    MAX_ACCOUNTS, MAX_BLOCKS, MAX_RATES, MAX_ROOM_TYPES, MAX_ROOMS, MAX_STAYS, PmsReservationActor,
    PmsReservationCommand, PmsReservationCommandInput, PmsReservationFacts,
    PmsReservationOperation, PmsReservationPlan, PmsReservationResult, read_limit, record_invalid,
};
use crate::domain::room_board::{PmsPropertyRow, PmsRecord, not_found, record_not_found};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Command facts retained from [`PmsReservationRepository::load`] for the
/// matching [`PmsReservationRepository::commit`]. The application trait
/// deliberately passes only the plan to commit, so the adapter keeps the
/// operation and loaded property/reservation identities plus the expected
/// property version from the snapshot it actually read.
#[derive(Clone, Debug)]
struct LoadedCommand {
    operation: PmsReservationOperation,
    property_id: String,
    expected_version: i64,
    target_id: Option<String>,
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
        "PMS_RESERVATION_RECORD_INVALID",
        message,
    )
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PMS_RESERVATION_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PMS_RESERVATION_SERIALIZABLE_REQUIRED").into());
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
pub async fn lock_reservation_sources(c: &mut PgConnection) -> Result<(), Error> {
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
    actor: &PmsReservationActor,
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

async fn rates(c: &mut PgConnection, property_id: &str) -> Result<Vec<PmsRecord>, Error> {
    bounded_records(
        c,
        property_id,
        MAX_RATES,
        "The hotel has more rates than this command can read.",
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='rate' ORDER BY id LIMIT $2",
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
         AND data_json::jsonb->>'kind' IN ('group','maintenance') \
         ORDER BY id LIMIT $2",
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

async fn accounts(
    c: &mut PgConnection,
    property_id: &str,
    reservation_id: &str,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records a \
         WHERE a.property_id=$1 AND a.kind='guest_account' AND \
         (a.data_json::jsonb->>'reservationId'=$2 OR a.id IN (\
           SELECT s.data_json::jsonb->>'guestAccountId' FROM pms_records s \
           WHERE s.property_id=$1 AND s.kind='stay' \
           AND s.data_json::jsonb->>'reservationId'=$2)) \
         ORDER BY a.id LIMIT $3",
    )
    .bind(property_id)
    .bind(reservation_id)
    .bind(MAX_ACCOUNTS as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_ACCOUNTS {
        return Err(Error::from(read_limit(
            "The hotel reservation has more guest accounts than this command can read.",
        )));
    }
    rows.iter().map(record_row).collect()
}

async fn has_financial_activity(
    c: &mut PgConnection,
    property_id: &str,
    reservation_id: &str,
) -> Result<bool, Error> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pms_records e \
         WHERE e.property_id=$1 AND e.kind IN ('entry','refund') \
         AND e.data_json::jsonb->>'guestAccountId' IN (\
           SELECT a.id FROM pms_records a \
           WHERE a.property_id=$1 AND a.kind='guest_account' AND \
           (a.data_json::jsonb->>'reservationId'=$2 OR a.id IN (\
             SELECT s.data_json::jsonb->>'guestAccountId' FROM pms_records s \
             WHERE s.property_id=$1 AND s.kind='stay' \
             AND s.data_json::jsonb->>'reservationId'=$2))))",
    )
    .bind(property_id)
    .bind(reservation_id)
    .fetch_one(&mut *c)
    .await?;
    Ok(exists)
}

async fn external_ref_exists(
    c: &mut PgConnection,
    property_id: &str,
    source: &str,
    external_ref: &str,
) -> Result<bool, Error> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pms_records WHERE property_id=$1 \
         AND kind='reservation' AND data_json::jsonb->>'source'=$2 \
         AND data_json::jsonb->>'externalRef'=$3)",
    )
    .bind(property_id)
    .bind(source)
    .bind(external_ref)
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

async fn insert_record(
    c: &mut PgConnection,
    property_id: &str,
    record: &PmsRecord,
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
    if changed != 1 {
        return Err(Error::from(version_conflict()));
    }
    Ok(())
}

async fn insert_source(
    c: &mut PgConnection,
    property_id: &str,
    key: &str,
    record_id: &str,
) -> Result<(), Error> {
    let changed = sqlx::query(
        "INSERT INTO pms_sources(property_id,source_key,record_id) \
         VALUES($1,$2,$3) ON CONFLICT(property_id,source_key) DO NOTHING",
    )
    .bind(property_id)
    .bind(key)
    .bind(record_id)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(Error::from(conflict(
            "PMS_EXTERNAL_REF_EXISTS",
            "A booking reference for this channel already exists",
        )));
    }
    Ok(())
}

async fn update_property(
    c: &mut PgConnection,
    actor: &PmsReservationActor,
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

fn authority_json(actor: &PmsReservationActor) -> String {
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
    actor: &PmsReservationActor,
    key: &CommandKey,
    request_hash: &str,
    operation: PmsReservationOperation,
    result: &PmsReservationResult,
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
    actor: &PmsReservationActor,
    key: &CommandKey,
    operation: PmsReservationOperation,
    plan: &PmsReservationPlan,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events\
         (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
         record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,'reservation',$6,$7,$5,$8,$9)",
    )
    .bind(format!("pms-reservation-audit-{}", key.as_str()))
    .bind(key.as_str())
    .bind(&actor.board.user_id)
    .bind(&actor.assignment_id)
    .bind(operation.receipt_operation())
    .bind(&plan.reservation.id)
    .bind(plan.property_version)
    .bind(now)
    .bind(authority_json(actor))
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn insert_outbox(
    c: &mut PgConnection,
    actor: &PmsReservationActor,
    key: &CommandKey,
    operation: PmsReservationOperation,
    plan: &PmsReservationPlan,
    now: i64,
) -> Result<(), Error> {
    let authority: Value = serde_json::from_str(&authority_json(actor)).unwrap_or(Value::Null);
    let stay_ids: Vec<&str> = plan.stays.iter().map(|stay| stay.id.as_str()).collect();
    let payload = json!({
        "authority": authority,
        "operation": operation.receipt_operation(),
        "id": plan.reservation.id,
        "propertyId": plan.result.property_id,
        "state": plan.reservation.data.get("state").cloned().unwrap_or(Value::Null),
        "stayIds": stay_ids,
        "version": plan.property_version,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events\
         (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
         payload_json,created_at) VALUES($1,$2,$3,'reservation',$4,$5,$6,$7)",
    )
    .bind(format!("pms-reservation-outbox-{}", key.as_str()))
    .bind(key.as_str())
    .bind(operation.receipt_operation())
    .bind(&plan.reservation.id)
    .bind(plan.property_version)
    .bind(payload)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// SQL repository bound to the caller's retained serializable unit.
pub struct PgPmsReservationRepository<'a> {
    connection: &'a mut PgConnection,
    loaded: Option<LoadedCommand>,
}

impl<'a> PgPmsReservationRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection,
            loaded: None,
        }
    }
}

impl PmsReservationRepository for PgPmsReservationRepository<'_> {
    type Error = Error;

    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsReservationCommand,
    ) -> PmsBoardFuture<'a, Option<PmsReservationResult>, Self::Error> {
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
                .and_then(|value| serde_json::from_str::<PmsReservationResult>(&value).ok())
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
        actor: &'a PmsReservationActor,
        command: &'a PmsReservationCommand,
    ) -> PmsBoardFuture<'a, PmsReservationFacts, Self::Error> {
        Box::pin(async move {
            lock_reservation_sources(self.connection).await?;
            self.loaded = None;
            let property = property(self.connection, actor, &command.property_id).await?;
            let operation = command.operation();
            let target_id = command.input.reservation_id().map(str::to_owned);
            let mut facts = PmsReservationFacts::empty(property.clone());
            match &command.input {
                PmsReservationCommandInput::Create(input) => {
                    if let Some(guest_id) = &input.guest_id {
                        facts.guest = Some(
                            required_record(self.connection, &property.id, guest_id, "guest")
                                .await?,
                        );
                    }
                    if !input.block_id.is_empty() {
                        facts.block = Some(
                            required_record(
                                self.connection,
                                &property.id,
                                &input.block_id,
                                "block",
                            )
                            .await?,
                        );
                    }
                    facts.room_types = room_types(self.connection, &property.id).await?;
                    facts.rooms = rooms(self.connection, &property.id).await?;
                    facts.rates = rates(self.connection, &property.id).await?;
                    facts.blocks = active_blocks(self.connection, &property.id).await?;
                    facts.stays = nonterminal_stays(self.connection, &property.id).await?;
                    if !input.external_ref.is_empty() {
                        facts.external_ref_exists = external_ref_exists(
                            self.connection,
                            &property.id,
                            &input.source,
                            &input.external_ref,
                        )
                        .await?;
                    }
                }
                PmsReservationCommandInput::Change(input) => {
                    let reservation = required_record(
                        self.connection,
                        &property.id,
                        &input.reservation_id,
                        "reservation",
                    )
                    .await?;
                    facts.reservation = Some(reservation);
                    facts.room_types = room_types(self.connection, &property.id).await?;
                    facts.rooms = rooms(self.connection, &property.id).await?;
                    facts.rates = rates(self.connection, &property.id).await?;
                    facts.blocks = active_blocks(self.connection, &property.id).await?;
                    facts.stays = nonterminal_stays(self.connection, &property.id).await?;
                    facts.accounts =
                        accounts(self.connection, &property.id, &input.reservation_id).await?;
                    facts.financial_activity = has_financial_activity(
                        self.connection,
                        &property.id,
                        &input.reservation_id,
                    )
                    .await?;
                }
                PmsReservationCommandInput::Cancel(input) => {
                    let reservation = required_record(
                        self.connection,
                        &property.id,
                        &input.reservation_id,
                        "reservation",
                    )
                    .await?;
                    facts.reservation = Some(reservation);
                    facts.stays = nonterminal_stays(self.connection, &property.id).await?;
                }
            }
            self.loaded = Some(LoadedCommand {
                operation,
                property_id: property.id.clone(),
                expected_version: property.version,
                target_id,
            });
            Ok(facts)
        })
    }

    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsReservationActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsReservationPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsReservationResult, Self::Error> {
        Box::pin(async move {
            serializable(self.connection).await?;
            let loaded = self.loaded.take().ok_or_else(|| {
                Error::from(PersistenceError::new("PMS_RESERVATION_LOAD_REQUIRED"))
            })?;
            let creates = loaded.operation == PmsReservationOperation::Create;
            if plan.result.id != plan.reservation.id
                || plan.result.property_id != loaded.property_id
                || plan.result.version != loaded.expected_version + 1
                || plan.property_version != loaded.expected_version + 1
                || (loaded
                    .target_id
                    .as_ref()
                    .is_some_and(|target| target != &plan.result.id))
                || (creates
                    && (plan.reservation.version != 1
                        || plan.accounts.len() != plan.stays.len()
                        || plan.stays.iter().any(|stay| stay.version != 1)))
                || (!creates
                    && (plan.reservation.version < 2
                        || plan.stays.is_empty()
                        || plan.stays.iter().any(|stay| stay.version < 2)
                        || !plan.accounts.is_empty()))
            {
                return Err(Error::from(conflict(
                    "PMS_RESERVATION_RESULT_INVALID",
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
            if let Some(guest) = &plan.guest {
                insert_record(self.connection, &property_id, guest).await?;
            }
            if creates {
                insert_record(self.connection, &property_id, &plan.reservation).await?;
            } else {
                update_record(self.connection, &property_id, &plan.reservation).await?;
            }
            if let Some(block) = &plan.block {
                update_record(self.connection, &property_id, block).await?;
            }
            for account in &plan.accounts {
                insert_record(self.connection, &property_id, account).await?;
            }
            for stay in &plan.stays {
                if creates {
                    insert_record(self.connection, &property_id, stay).await?;
                } else {
                    update_record(self.connection, &property_id, stay).await?;
                }
            }
            if let Some(source_key) = &plan.source_key {
                insert_source(
                    self.connection,
                    &property_id,
                    source_key,
                    &plan.reservation.id,
                )
                .await?;
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
