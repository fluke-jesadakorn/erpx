//! Native Finance due-draft schedule command.
//!
//! This mirrors the retained TypeScript schedule path in
//! `modules/finance/server/src/adapters/finance-due-drafts.ts`
//! (`commitSchedule` via `createFinanceDueDrafts(...).scheduleFinanceDraft`):
//! a Holding Finance assignment schedules one Company draft from the current
//! active source edition, keyed idempotently by the caller idempotency key.
//!
//! The adapter runs on the caller-owned serializable connection and writes
//! the draft row plus the command receipt, audit event and outbox event in
//! that same unit. It never opens a pool, commits, retries or mints
//! authority: the composition supplies the authorized Holding actor and
//! enforces the purchase capability named by
//! [`crate::domain::due_draft_schedule::DueDraftKind::capability`].
//!
//! Faithful-mirror notes (documented, not silent): the TypeScript
//! `requirePermission` JWT-static half of the read is subsumed by the
//! composition-supplied actor; the database-current half (the delegated
//! assignment row) is rechecked here exactly like
//! `financeDelegatedAssignment`. The input hash is sha256 over the
//! canonical JSON payload; key order follows serde (sorted), while the
//! TypeScript hash follows zod schema order, so hashes are stable within
//! each path but not byte-identical across them. Replay identity is the
//! caller idempotency key in both paths.
use crate::domain::due_draft_schedule::{
    DueDraftKind, DueDraftScheduleInput, ScheduleRefusal, check_decision_state, holding_required,
    resolve_expected_version, schedule_changed, source_mismatch, validate_decision,
    validate_schedule,
};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Current command identity. The composition supplies this after the retained
/// Holding proof; the adapter never constructs or widens authority.
#[derive(Clone, Debug)]
pub struct DueDraftCommandActor {
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    pub company_id: String,
    pub holding_id: String,
    /// Must be `"holding"`. Branch/department assignments cannot schedule
    /// Company-wide drafts, exactly like the TypeScript `holding()` guard.
    pub scope_kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DueDraftScheduleResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

fn refusal(value: ScheduleRefusal) -> Error {
    let status = value.status;
    let failure = match status {
        403 => FailureKind::Forbidden,
        404 => FailureKind::NotFound,
        409 => FailureKind::Conflict,
        422 => FailureKind::InvalidInput,
        _ => FailureKind::Unavailable,
    };
    BusinessError::new(failure, value.code, value.message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_DUE_DRAFT_UNAVAILABLE",
        "The Finance draft could not be scheduled.",
    )
    .into()
}

fn conflict_idempotency() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to a different request.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

/// Work fingerprint over the exact retained authority fields, mirroring the
/// TypeScript `replayCommand` identity inputs (actor, assignment, role,
/// operation, record, canonical input).
fn fingerprint(actor: &DueDraftCommandActor, operation: &str, record: &str, input: &str) -> String {
    hash(&format!(
        "{{\"actor\":{},\"assignment\":{},\"role\":{},\"company\":{},\"holding\":{},\"operation\":{},\"recordId\":{},\"input\":{}}}",
        string(&actor.user_id),
        string(&actor.assignment_id),
        string(&actor.role),
        string(&actor.company_id),
        string(&actor.holding_id),
        string(operation),
        string(record),
        input
    ))
}

fn authority_json(actor: &DueDraftCommandActor, assignment_version: i64) -> String {
    json!({
        "role": actor.role,
        "assignmentId": actor.assignment_id,
        "assignmentVersion": assignment_version,
        "companyId": actor.company_id,
        "holdingId": actor.holding_id,
        "scopeKind": actor.scope_kind,
    })
    .to_string()
}

/// Retained source-edition fact for one draft kind.
struct SourceEdition {
    version: i64,
    active: bool,
}

/// Retained source-edition read. Each kind uses its own static statement over
/// its retained table (never caller text); recognition billing runs carry no
/// status column, so every run counts as eligible and only existence plus the
/// version gate decide, exactly like the TypeScript oracle.
async fn source_edition(
    c: &mut PgConnection,
    kind: DueDraftKind,
    company_id: &str,
    source_id: &str,
) -> Result<Option<SourceEdition>, Error> {
    match kind {
        DueDraftKind::Billing => {
            sqlx::query(
                "SELECT version,status FROM finance_business_contracts WHERE id=$1 AND company_id=$2 FOR SHARE",
            )
            .bind(source_id)
            .bind(company_id)
            .fetch_optional(&mut *c)
            .await?
            .map(|row| {
                let version: i64 = row.try_get("version").map_err(|_| unavailable())?;
                let status: String = row.try_get("status").map_err(|_| unavailable())?;
                Ok(SourceEdition { version, active: status == "active" })
            })
            .transpose()
        }
        DueDraftKind::Recognition => {
            sqlx::query(
                "SELECT version FROM finance_business_billing_runs WHERE id=$1 AND company_id=$2 FOR SHARE",
            )
            .bind(source_id)
            .bind(company_id)
            .fetch_optional(&mut *c)
            .await?
            .map(|row| {
                let version: i64 = row.try_get("version").map_err(|_| unavailable())?;
                Ok(SourceEdition { version, active: true })
            })
            .transpose()
        }
        DueDraftKind::AssetDepreciation => {
            sqlx::query(
                "SELECT version,status FROM finance_asset_policy_editions WHERE id=$1 AND company_id=$2 FOR SHARE",
            )
            .bind(source_id)
            .bind(company_id)
            .fetch_optional(&mut *c)
            .await?
            .map(|row| {
                let version: i64 = row.try_get("version").map_err(|_| unavailable())?;
                let status: String = row.try_get("status").map_err(|_| unavailable())?;
                Ok(SourceEdition { version, active: status == "active" })
            })
            .transpose()
        }
        DueDraftKind::InstrumentPayment => {
            sqlx::query(
                "SELECT version,status FROM finance_instrument_editions WHERE id=$1 AND company_id=$2 FOR SHARE",
            )
            .bind(source_id)
            .bind(company_id)
            .fetch_optional(&mut *c)
            .await?
            .map(|row| {
                let version: i64 = row.try_get("version").map_err(|_| unavailable())?;
                let status: String = row.try_get("status").map_err(|_| unavailable())?;
                Ok(SourceEdition { version, active: status == "active" })
            })
            .transpose()
        }
    }
}

/// Native mirror of `financeDelegatedAssignment`: the reader's current Holding
/// assignment over the same Company/role must still be valid. The static JWT
/// `requirePermission` half of the oracle is subsumed by the
/// composition-supplied actor.
async fn delegated_assignment_version(
    c: &mut PgConnection,
    actor: &DueDraftCommandActor,
    now: i64,
) -> Result<Option<i64>, Error> {
    sqlx::query_scalar(
        "SELECT version FROM assignments WHERE id=$1 AND user_id=$2 AND company_id=$3 AND role=$4 \
         AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=$5 AND valid_until>$5",
    )
    .bind(&actor.assignment_id)
    .bind(&actor.user_id)
    .bind(&actor.company_id)
    .bind(&actor.role)
    .bind(now)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| unavailable())
}

/// Lock named shared writers before the first row read, mirroring the
/// command-side transaction discipline of the sibling Finance writers.
pub async fn lock_due_draft_schedule_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_due_drafts,command_receipts IN ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn retain(
    c: &mut PgConnection,
    actor: &DueDraftCommandActor,
    authority: &str,
    command: &str,
    operation: &str,
    request_hash: &str,
    record: &str,
    result: &Value,
    version: i64,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(command)
    .bind(operation)
    .bind(&actor.user_id)
    .bind(request_hash)
    .bind(record)
    .bind(result.to_string())
    .bind(now)
    .bind(authority)
    .execute(&mut *c)
    .await?;
    let record_type = operation.split('.').take(2).collect::<Vec<_>>().join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$9,$4,$7,$8)",
    )
    .bind(command)
    .bind(&actor.user_id)
    .bind(&actor.assignment_id)
    .bind(operation)
    .bind(&record_type)
    .bind(record)
    .bind(now)
    .bind(authority)
    .bind(version)
    .execute(&mut *c)
    .await?;
    let payload = json!({
        "authority": serde_json::from_str::<Value>(authority).unwrap_or(Value::Null),
        "id": record,
        "state": result.get("status").cloned().unwrap_or(Value::Null),
        "companyId": actor.company_id,
        "branchId": Value::Null,
        "departmentId": Value::Null,
        "version": version,
    });
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$7,$5,$6)",
    )
    .bind(command)
    .bind(operation)
    .bind(&record_type)
    .bind(record)
    .bind(payload.to_string())
    .bind(now)
    .bind(version)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Schedule one Finance due draft. `key` is the caller idempotency key; the
/// draft id is the retained `finance-due-{key}` derivation and the receipt
/// key is the idempotency key itself, exactly like the TypeScript commit.
pub async fn schedule_due_draft(
    c: &mut PgConnection,
    actor: &DueDraftCommandActor,
    key: &CommandKey,
    kind: &str,
    source_id: &str,
    run_on: &str,
    payload_json: &str,
    reason: &str,
    now: i64,
) -> Result<DueDraftScheduleResult, Error> {
    serializable(c).await?;
    if actor.scope_kind != "holding"
        || actor.company_id.is_empty()
        || actor.holding_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.role.is_empty()
    {
        return Err(refusal(holding_required()));
    }
    let input: DueDraftScheduleInput =
        validate_schedule(kind, source_id, run_on, payload_json, reason).map_err(refusal)?;
    let operation = "finance.due-draft.schedule";
    let record = format!("finance-due-{}", key.as_str());
    let request_hash = fingerprint(actor, operation, &record, &input.payload_json);

    // Idempotent replay on the caller transaction: the same key returns the
    // retained result, a reused key with different inputs conflicts.
    let existing = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(key.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = existing {
        let saved: String = row.try_get("request_hash").map_err(|_| unavailable())?;
        if saved != request_hash {
            return Err(conflict_idempotency());
        }
        let raw: Option<String> = row.try_get("result_json").map_err(|_| unavailable())?;
        return raw
            .and_then(|value| serde_json::from_str::<DueDraftScheduleResult>(&value).ok())
            .ok_or_else(unavailable);
    }

    let Some(edition) = source_edition(c, input.kind, &actor.company_id, &input.source_id).await?
    else {
        return Err(refusal(source_mismatch()));
    };
    if !edition.active {
        return Err(refusal(source_mismatch()));
    }
    let expected = resolve_expected_version(&input.payload_json, edition.version).map_err(refusal)?;
    if expected != edition.version {
        return Err(refusal(source_mismatch()));
    }
    let Some(assignment_version) = delegated_assignment_version(c, actor, now).await? else {
        return Err(refusal(source_mismatch()));
    };
    let input_hash = hash(&input.payload_json);
    sqlx::query(
        "INSERT INTO finance_due_drafts(id,company_id,holding_id,kind,source_id,run_on,source_version,input_json,input_hash, \
         delegated_by,assignment_id,assignment_version,role,reason,created_at,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15)",
    )
    .bind(&record)
    .bind(&actor.company_id)
    .bind(&actor.holding_id)
    .bind(input.kind.as_str())
    .bind(&input.source_id)
    .bind(&input.run_on)
    .bind(edition.version)
    .bind(&input.payload_json)
    .bind(&input_hash)
    .bind(&actor.user_id)
    .bind(&actor.assignment_id)
    .bind(assignment_version)
    .bind(&actor.role)
    .bind(&input.reason)
    .bind(now)
    .execute(&mut *c)
    .await?;
    let result = DueDraftScheduleResult {
        id: record.clone(),
        status: "scheduled".to_owned(),
        version: 1,
    };
    let result_json = serde_json::to_value(&result).map_err(|_| unavailable())?;
    retain(
        c,
        actor,
        &authority_json(actor, assignment_version),
        key.as_str(),
        operation,
        &request_hash,
        &record,
        &result_json,
        1,
        now,
    )
    .await?;
    Ok(result)
}

/// Decide a pending Finance due draft. `key` is the caller idempotency key;
/// the receipt key is the idempotency key itself, exactly like the TypeScript
/// commit. Retry reopens the draft as `scheduled`, cancel closes it as
/// `cancelled`, both at `expected_version + 1` behind the retained
/// version assertion.
pub async fn decide_due_draft(
    c: &mut PgConnection,
    actor: &DueDraftCommandActor,
    key: &CommandKey,
    id: &str,
    action: &str,
    expected_version: i64,
    reason: &str,
    now: i64,
) -> Result<DueDraftScheduleResult, Error> {
    serializable(c).await?;
    if actor.scope_kind != "holding"
        || actor.company_id.is_empty()
        || actor.holding_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.role.is_empty()
    {
        return Err(refusal(holding_required()));
    }
    let input = validate_decision(action, expected_version, reason).map_err(refusal)?;
    let operation = input.action.operation();
    let input_json = json!({"expectedVersion": input.expected_version, "reason": input.reason}).to_string();
    let request_hash = fingerprint(actor, operation, id, &input_json);
    let existing = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(key.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = existing {
        let saved: String = row.try_get("request_hash").map_err(|_| unavailable())?;
        if saved != request_hash {
            return Err(conflict_idempotency());
        }
        let raw: Option<String> = row.try_get("result_json").map_err(|_| unavailable())?;
        return raw
            .and_then(|value| serde_json::from_str::<DueDraftScheduleResult>(&value).ok())
            .ok_or_else(unavailable);
    }
    let row = sqlx::query(
        "SELECT version,status FROM finance_due_drafts WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Err(refusal(schedule_changed()));
    };
    let row_version: i64 = row.try_get("version").map_err(|_| unavailable())?;
    let row_status: String = row.try_get("status").map_err(|_| unavailable())?;
    check_decision_state(row_version, &row_status, input.expected_version).map_err(refusal)?;
    let next_version = input.expected_version + 1;
    let next_status = input.action.status();
    sqlx::query(
        "UPDATE finance_due_drafts SET status=$1,version=$2,updated_at=$3 WHERE id=$4 AND company_id=$5",
    )
    .bind(next_status)
    .bind(next_version)
    .bind(now)
    .bind(id)
    .bind(&actor.company_id)
    .execute(&mut *c)
    .await?;
    // Authority document pins the current delegation when the grant is
    // still live (0 when it lapsed). Unlike schedule, the retained decide
    // path performs no delegation gate — the TypeScript commit replays under
    // the JWT-time authority — so a lapsed grant does not fail the decision
    // here; execution still rechecks authority at drain time.
    let assignment_version = delegated_assignment_version(c, actor, now).await?.unwrap_or(0);
    let result = DueDraftScheduleResult {
        id: id.to_owned(),
        status: next_status.to_owned(),
        version: next_version,
    };
    let result_json = serde_json::to_value(&result).map_err(|_| unavailable())?;
    retain(
        c,
        actor,
        &authority_json(actor, assignment_version),
        key.as_str(),
        operation,
        &request_hash,
        id,
        &result_json,
        next_version,
        now,
    )
    .await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> DueDraftCommandActor {
        DueDraftCommandActor {
            user_id: "user-1".to_owned(),
            assignment_id: "grant-1".to_owned(),
            role: "finance_writer".to_owned(),
            company_id: "co".to_owned(),
            holding_id: "holding-1".to_owned(),
            scope_kind: "holding".to_owned(),
        }
    }

    #[test]
    fn record_id_derives_from_the_idempotency_key() {
        let key = CommandKey::new("key-abc-123").unwrap();
        assert_eq!(format!("finance-due-{}", key.as_str()), "finance-due-key-abc-123");
    }

    #[test]
    fn fingerprint_is_stable_and_input_sensitive() {
        let actor = actor();
        let first = fingerprint(&actor, "finance.due-draft.schedule", "finance-due-k", "{}");
        let again = fingerprint(&actor, "finance.due-draft.schedule", "finance-due-k", "{}");
        assert_eq!(first, again);
        assert_eq!(first.len(), 64);
        assert_ne!(
            first,
            fingerprint(&actor, "finance.due-draft.schedule", "finance-due-k", r#"{"a":1}"#)
        );
    }

    #[test]
    fn capability_names_follow_the_retained_purchase_boundary() {
        assert_eq!(DueDraftKind::InstrumentPayment.capability(), "finance.advanced");
        assert_eq!(DueDraftKind::Billing.capability(), "finance.accounting");
    }
}
