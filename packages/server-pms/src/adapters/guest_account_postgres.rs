//! PostgreSQL adapter for the native PMS guest-account and charge commands.
//!
//! Only PMS-owned rows are read or written here: `pms_properties`, `pms_records`,
//! `pms_sources` and the shared `command_receipts`, `audit_events` and
//! `outbox_events` records. Finance balances/journals, cash/card settlement,
//! refunds, invoice documents and stock never enter this unit. The caller owns
//! the serializable transaction; this adapter never commits or retries on its
//! own.
use crate::adapters::room_board::record_row;
use crate::application::guest_account::PmsGuestAccountRepository;
use crate::application::room_board::PmsBoardFuture;
use crate::domain::guest_account::{
    MAX_REFERENCES, MAX_REFUNDS, PmsGuestAccountActor, PmsGuestAccountCommand,
    PmsGuestAccountFacts, PmsGuestAccountOperation, PmsGuestAccountPlan, PmsGuestAccountResult,
    read_limit, record_invalid,
};
use crate::domain::room_board::{PmsPropertyRow, PmsRecord, not_found, record_not_found};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Command facts retained from [`PmsGuestAccountRepository::load`] for the
/// matching [`PmsGuestAccountRepository::commit`].
#[derive(Clone, Debug)]
struct LoadedCommand {
    operation: PmsGuestAccountOperation,
    property_id: String,
    expected_version: i64,
    /// Account id the load actually read (transfer source for `transfer`).
    account_id: Option<String>,
    /// Transfer destination id.
    destination_id: Option<String>,
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
        "PMS_GUEST_ACCOUNT_RECORD_INVALID",
        message,
    )
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PMS_GUEST_ACCOUNT_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PMS_GUEST_ACCOUNT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Take the reviewed PMS pre-snapshot fence before the first owner read.
///
/// `SHARE ROW EXCLUSIVE` excludes other PMS command writers before their
/// snapshots but still admits owner read transactions, so this writer never
/// upgrades a shared lock into an update and cannot deadlock with a peer
/// command. `NOWAIT` maps contention to `PRE_SNAPSHOT_LOCK_CONTENDED`; the
/// caller retries the complete transaction instead of waiting on another owner.
pub async fn lock_guest_account_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE pms_properties,pms_records,pms_sources IN SHARE ROW EXCLUSIVE MODE NOWAIT",
    )
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
    actor: &PmsGuestAccountActor,
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

/// Existing entry retained for one provider source key.
async fn source_entry(
    c: &mut PgConnection,
    property_id: &str,
    source_key: &str,
) -> Result<Option<PmsRecord>, Error> {
    let row = sqlx::query(
        "SELECT r.id,r.kind,r.version,r.updated_at,r.data_json FROM pms_sources s \
         JOIN pms_records r ON r.property_id=s.property_id AND r.id=s.record_id \
         WHERE s.property_id=$1 AND s.source_key=$2",
    )
    .bind(property_id)
    .bind(source_key)
    .fetch_optional(&mut *c)
    .await?;
    row.as_ref().map(record_row).transpose()
}

/// Bounded `requested`/`approved` refund facts for one account.
async fn account_refunds(
    c: &mut PgConnection,
    property_id: &str,
    account_id: &str,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='refund' \
         AND data_json::jsonb->>'guestAccountId'=$2 \
         AND data_json::jsonb->>'state' IN ('requested','approved') \
         ORDER BY id LIMIT $3",
    )
    .bind(property_id)
    .bind(account_id)
    .bind(MAX_REFUNDS as i64 + 1)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_REFUNDS {
        return Err(Error::from(read_limit(
            "The guest account has more refund facts than this command can read.",
        )));
    }
    rows.iter().map(record_row).collect()
}

/// Entries already referencing one original entry.
async fn original_references(
    c: &mut PgConnection,
    property_id: &str,
    entry_id: &str,
) -> Result<Vec<PmsRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id,kind,version,updated_at,data_json FROM pms_records \
         WHERE property_id=$1 AND kind='entry' \
         AND data_json::jsonb->>'originalId'=$2 ORDER BY id LIMIT $3",
    )
    .bind(property_id)
    .bind(entry_id)
    .bind(MAX_REFERENCES as i64)
    .fetch_all(&mut *c)
    .await?;
    rows.iter().map(record_row).collect()
}

fn record_json(record: &PmsRecord) -> Result<String, Error> {
    serde_json::to_string(&record.data).map_err(|_| {
        Error::from(unavailable(
            "The guest account record could not be written safely.",
        ))
    })
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
    source_key: &str,
    record_id: &str,
) -> Result<(), Error> {
    let changed = sqlx::query(
        "INSERT INTO pms_sources(property_id,source_key,record_id) VALUES($1,$2,$3) \
         ON CONFLICT(property_id,source_key) DO NOTHING",
    )
    .bind(property_id)
    .bind(source_key)
    .bind(record_id)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(Error::from(conflict(
            "PMS_SOURCE_CONFLICT",
            "This guest account source key is already retained.",
        )));
    }
    Ok(())
}

async fn update_property(
    c: &mut PgConnection,
    actor: &PmsGuestAccountActor,
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

fn authority_json(actor: &PmsGuestAccountActor) -> String {
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
    actor: &PmsGuestAccountActor,
    key: &CommandKey,
    request_hash: &str,
    operation: PmsGuestAccountOperation,
    result: &PmsGuestAccountResult,
    now: i64,
) -> Result<(), Error> {
    let raw = serde_json::to_string(result).map_err(|_| {
        Error::from(unavailable(
            "The guest account result could not be retained.",
        ))
    })?;
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

fn transferred_account(record: Option<&PmsRecord>) -> Option<&str> {
    record
        .and_then(|record| record.data.get("guestAccountId"))
        .and_then(Value::as_str)
}

/// Reject a plan whose written rows do not match the operation the load read.
/// This is the adapter's own guard against a mismatched plan, not an
/// authorization decision.
fn shape_ok(loaded: &LoadedCommand, plan: &PmsGuestAccountPlan) -> bool {
    let no_entries = plan.new_entry.is_none() && plan.new_destination_entry.is_none();
    let creates = |id: &str, record: Option<&PmsRecord>| {
        record.is_some_and(|record| record.version == 1 && record.id == id)
    };
    let updates = |id: &str, record: Option<&PmsRecord>| {
        record.is_some_and(|record| record.version >= 2 && record.id == id)
    };
    match loaded.operation {
        PmsGuestAccountOperation::Create => {
            creates(&plan.result.id, plan.new_account.as_ref())
                && plan.account.is_none()
                && plan.destination.is_none()
                && no_entries
                && plan.source_key.is_none()
        }
        PmsGuestAccountOperation::Update => {
            updates(&plan.result.id, plan.account.as_ref())
                && plan.new_account.is_none()
                && plan.destination.is_none()
                && no_entries
                && plan.source_key.is_none()
        }
        PmsGuestAccountOperation::ChargeAdd
        | PmsGuestAccountOperation::ChargeImport
        | PmsGuestAccountOperation::Adjustment => match plan.new_entry.as_ref() {
            Some(entry) => {
                entry.version == 1
                    && entry.id == plan.result.id
                    && updates(&plan.result.account_id, plan.account.as_ref())
                    && plan.new_account.is_none()
                    && plan.destination.is_none()
                    && plan.new_destination_entry.is_none()
            }
            None => {
                plan.account.is_none()
                    && plan.new_account.is_none()
                    && plan.destination.is_none()
                    && plan.new_destination_entry.is_none()
                    && plan.source_key.is_none()
            }
        },
        PmsGuestAccountOperation::Transfer => {
            creates(&plan.result.id, plan.new_entry.as_ref())
                && loaded.account_id.as_deref() == transferred_account(plan.new_entry.as_ref())
                && updates(&plan.result.account_id, plan.account.as_ref())
                && plan.new_account.is_none()
                && plan.destination.as_ref().is_some_and(|record| {
                    record.version >= 2
                        && loaded.destination_id.as_deref() == Some(record.id.as_str())
                })
                && plan.new_destination_entry.as_ref().is_some_and(|entry| {
                    entry.version == 1
                        && loaded.destination_id.as_deref() == transferred_account(Some(entry))
                })
                && plan.source_key.is_none()
        }
    }
}

async fn insert_audit(
    c: &mut PgConnection,
    actor: &PmsGuestAccountActor,
    key: &CommandKey,
    operation: PmsGuestAccountOperation,
    plan: &PmsGuestAccountPlan,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events\
         (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
         record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10)",
    )
    .bind(format!("pms-guest-account-audit-{}", key.as_str()))
    .bind(key.as_str())
    .bind(&actor.board.user_id)
    .bind(&actor.assignment_id)
    .bind(operation.receipt_operation())
    .bind(operation.aggregate())
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
    actor: &PmsGuestAccountActor,
    key: &CommandKey,
    operation: PmsGuestAccountOperation,
    plan: &PmsGuestAccountPlan,
    now: i64,
) -> Result<(), Error> {
    let authority: Value = serde_json::from_str(&authority_json(actor)).unwrap_or(Value::Null);
    let entry_id = plan
        .new_entry
        .as_ref()
        .map(|entry| entry.id.clone())
        .unwrap_or_default();
    let payload = json!({
        "authority": authority,
        "operation": operation.receipt_operation(),
        "id": plan.result.id,
        "accountId": plan.result.account_id,
        "entryId": entry_id,
        "propertyId": plan.result.property_id,
        "state": plan.result.state,
        "due": plan.result.due,
        "advance": plan.result.advance,
        "deposit": plan.result.deposit,
        "version": plan.property_version,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events\
         (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
         payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("pms-guest-account-outbox-{}", key.as_str()))
    .bind(key.as_str())
    .bind(operation.receipt_operation())
    .bind(operation.aggregate())
    .bind(&plan.result.id)
    .bind(plan.property_version)
    .bind(payload)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// SQL repository bound to the caller's retained serializable unit.
pub struct PgPmsGuestAccountRepository<'a> {
    connection: &'a mut PgConnection,
    loaded: Option<LoadedCommand>,
}

impl<'a> PgPmsGuestAccountRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self {
            connection,
            loaded: None,
        }
    }
}

impl PmsGuestAccountRepository for PgPmsGuestAccountRepository<'_> {
    type Error = Error;

    fn replay<'a>(
        &'a mut self,
        key: &'a CommandKey,
        request_hash: &'a str,
        command: &'a PmsGuestAccountCommand,
    ) -> PmsBoardFuture<'a, Option<PmsGuestAccountResult>, Self::Error> {
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
                .and_then(|value| serde_json::from_str::<PmsGuestAccountResult>(&value).ok())
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
        actor: &'a PmsGuestAccountActor,
        command: &'a PmsGuestAccountCommand,
    ) -> PmsBoardFuture<'a, PmsGuestAccountFacts, Self::Error> {
        Box::pin(async move {
            lock_guest_account_sources(self.connection).await?;
            self.loaded = None;
            let property = property(self.connection, actor, &command.property_id).await?;
            let operation = command.operation();
            let mut facts = PmsGuestAccountFacts::empty(property.clone());
            let mut account_id = None;
            let mut destination_id = None;
            match &command.input {
                crate::domain::guest_account::PmsGuestAccountInput::Create(input) => {
                    facts.guest = Some(
                        required_record(self.connection, &property.id, &input.guest_id, "guest")
                            .await?,
                    );
                    if !input.reservation_id.is_empty() {
                        facts.reservation = Some(
                            required_record(
                                self.connection,
                                &property.id,
                                &input.reservation_id,
                                "reservation",
                            )
                            .await?,
                        );
                    }
                }
                crate::domain::guest_account::PmsGuestAccountInput::Update(input) => {
                    account_id = Some(input.account_id.clone());
                    facts.account = Some(
                        required_record(
                            self.connection,
                            &property.id,
                            &input.account_id,
                            "guest_account",
                        )
                        .await?,
                    );
                }
                crate::domain::guest_account::PmsGuestAccountInput::ChargeAdd(input) => {
                    account_id = Some(input.account_id.clone());
                    facts.account = Some(
                        required_record(
                            self.connection,
                            &property.id,
                            &input.account_id,
                            "guest_account",
                        )
                        .await?,
                    );
                    facts.account_refunds =
                        account_refunds(self.connection, &property.id, &input.account_id).await?;
                    if !input.source_key.is_empty() {
                        facts.source_entry =
                            source_entry(self.connection, &property.id, &input.source_key).await?;
                    }
                }
                crate::domain::guest_account::PmsGuestAccountInput::ChargeImport(input) => {
                    account_id = Some(input.account_id.clone());
                    facts.account = Some(
                        required_record(
                            self.connection,
                            &property.id,
                            &input.account_id,
                            "guest_account",
                        )
                        .await?,
                    );
                    facts.account_refunds =
                        account_refunds(self.connection, &property.id, &input.account_id).await?;
                    facts.source_entry =
                        source_entry(self.connection, &property.id, &input.source_key).await?;
                }
                crate::domain::guest_account::PmsGuestAccountInput::Adjustment(input) => {
                    account_id = Some(input.account_id.clone());
                    facts.account = Some(
                        required_record(
                            self.connection,
                            &property.id,
                            &input.account_id,
                            "guest_account",
                        )
                        .await?,
                    );
                    facts.account_refunds =
                        account_refunds(self.connection, &property.id, &input.account_id).await?;
                    if !input.source_key.is_empty() {
                        facts.source_entry =
                            source_entry(self.connection, &property.id, &input.source_key).await?;
                    }
                    if !input.original_entry_id.is_empty() {
                        facts.original_entry = Some(
                            required_record(
                                self.connection,
                                &property.id,
                                &input.original_entry_id,
                                "entry",
                            )
                            .await?,
                        );
                        facts.original_references = original_references(
                            self.connection,
                            &property.id,
                            &input.original_entry_id,
                        )
                        .await?;
                    }
                }
                crate::domain::guest_account::PmsGuestAccountInput::Transfer(input) => {
                    let original =
                        required_record(self.connection, &property.id, &input.entry_id, "entry")
                            .await?;
                    let from_account = record_text(&original, "guestAccountId").to_owned();
                    if from_account.is_empty() {
                        return Err(Error::from(record_invalid()));
                    }
                    account_id = Some(from_account.clone());
                    destination_id = Some(input.to_account_id.clone());
                    facts.original_entry = Some(original);
                    facts.account = Some(
                        required_record(
                            self.connection,
                            &property.id,
                            &from_account,
                            "guest_account",
                        )
                        .await?,
                    );
                    facts.destination = Some(
                        required_record(
                            self.connection,
                            &property.id,
                            &input.to_account_id,
                            "guest_account",
                        )
                        .await?,
                    );
                    facts.account_refunds =
                        account_refunds(self.connection, &property.id, &from_account).await?;
                    facts.destination_refunds =
                        account_refunds(self.connection, &property.id, &input.to_account_id)
                            .await?;
                    facts.original_references =
                        original_references(self.connection, &property.id, &input.entry_id).await?;
                }
            }
            self.loaded = Some(LoadedCommand {
                operation,
                property_id: property.id.clone(),
                expected_version: property.version,
                account_id,
                destination_id,
            });
            Ok(facts)
        })
    }

    fn commit<'a>(
        &'a mut self,
        actor: &'a PmsGuestAccountActor,
        key: &'a CommandKey,
        request_hash: &'a str,
        plan: PmsGuestAccountPlan,
        now: i64,
    ) -> PmsBoardFuture<'a, PmsGuestAccountResult, Self::Error> {
        Box::pin(async move {
            serializable(self.connection).await?;
            let loaded = self.loaded.take().ok_or_else(|| {
                Error::from(PersistenceError::new("PMS_GUEST_ACCOUNT_LOAD_REQUIRED"))
            })?;
            if plan.result.property_id != loaded.property_id
                || plan.result.version != loaded.expected_version + 1
                || plan.property_version != loaded.expected_version + 1
                || loaded
                    .account_id
                    .as_ref()
                    .is_some_and(|account| account != &plan.result.account_id)
                || !shape_ok(&loaded, &plan)
            {
                return Err(Error::from(conflict(
                    "PMS_GUEST_ACCOUNT_RESULT_INVALID",
                    "The guest account command result is unavailable.",
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
            if let Some(account) = &plan.new_account {
                insert_record(self.connection, &property_id, account).await?;
            }
            if let Some(account) = &plan.account {
                update_record(self.connection, &property_id, account).await?;
            }
            if let Some(destination) = &plan.destination {
                update_record(self.connection, &property_id, destination).await?;
            }
            if let Some(entry) = &plan.new_entry {
                insert_record(self.connection, &property_id, entry).await?;
            }
            if let Some(entry) = &plan.new_destination_entry {
                insert_record(self.connection, &property_id, entry).await?;
            }
            if let Some(source_key) = &plan.source_key {
                insert_source(self.connection, &property_id, source_key, &plan.result.id).await?;
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
