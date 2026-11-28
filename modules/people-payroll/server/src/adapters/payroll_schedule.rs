//! Native payroll schedule save command.
//!
//! This mirrors the retained TypeScript schedule path in
//! `modules/people-payroll/server/src/adapters/payroll-readiness.ts`
//! (`stageSavePayrollScheduleEffect` via
//! `createPayrollReadinessApplication(...).savePayrollSchedule`): a Holding
//! payroll assignment stores one pay group's schedule (enabled flag, pay
//! delay, anchor date) behind the current schedule-version gate, keyed
//! idempotently by the caller idempotency key.
//!
//! The adapter runs on the caller-owned serializable connection and writes
//! the pay-group row plus the command receipt, audit event and outbox event
//! in that same unit. It never opens a pool, commits, retries or mints
//! authority: the composition supplies the authorized Holding actor and
//! enforces the purchase capability (retained `payroll.prepare`).
use crate::domain::payroll_schedule::{
    PayrollScheduleInput, PayrollScheduleView, ScheduleFrequency, ScheduleRefusal, SCHEDULE_OPERATION,
    check_anchor, group_missing, group_missing_read, holding_required, validate_schedule,
    version_conflict,
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
pub struct PayrollScheduleCommandActor {
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    pub company_id: String,
    pub holding_id: String,
    /// Must be `"holding"`. Branch/department assignments cannot save
    /// Company-wide payroll schedules, exactly like the TypeScript guard.
    pub scope_kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayrollScheduleResult {
    pub id: String,
    pub enabled: bool,
    #[serde(rename = "payDelayDays")]
    pub pay_delay_days: i64,
    #[serde(rename = "anchorDate")]
    pub anchor_date: Option<String>,
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
        "PAYROLL_SCHEDULE_UNAVAILABLE",
        "The payroll schedule could not be saved.",
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
        return Err(PersistenceError::new("PAYROLL_COMMAND_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("PAYROLL_COMMAND_SERIALIZABLE_REQUIRED").into());
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

fn fingerprint(actor: &PayrollScheduleCommandActor, operation: &str, record: &str, input: &str) -> String {
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

fn authority_json(actor: &PayrollScheduleCommandActor) -> String {
    json!({
        "role": actor.role,
        "assignmentId": actor.assignment_id,
        "companyId": actor.company_id,
        "holdingId": actor.holding_id,
        "scopeKind": actor.scope_kind,
    })
    .to_string()
}

/// Retained pay-group schedule fact for one decision.
struct ScheduleGroup {
    frequency: ScheduleFrequency,
    schedule_version: i64,
}

async fn schedule_group(
    c: &mut PgConnection,
    company_id: &str,
    group_id: &str,
) -> Result<Option<ScheduleGroup>, Error> {
    sqlx::query(
        "SELECT frequency,schedule_version FROM hr_pay_groups WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE",
    )
    .bind(group_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await?
    .map(|row| {
        let frequency: String = row.try_get("frequency").map_err(|_| unavailable())?;
        let schedule_version: i64 = row.try_get("schedule_version").map_err(|_| unavailable())?;
        let frequency =
            ScheduleFrequency::parse(&frequency).ok_or_else(|| unavailable())?;
        Ok(ScheduleGroup { frequency, schedule_version })
    })
    .transpose()
}

/// Lock named shared writers before the first row read, mirroring the
/// command-side transaction discipline of the sibling payroll writers.
pub async fn lock_payroll_schedule_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query("LOCK TABLE hr_pay_groups,command_receipts IN ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn retain(
    c: &mut PgConnection,
    actor: &PayrollScheduleCommandActor,
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
    // The ledger names the aggregate after the operation (`people_payroll`),
    // exactly like the retained `executeCommand`; the pay-group table is the
    // fact written, not the ledger identity.
    let record_type = operation.split('.').take(2).collect::<Vec<_>>().join("_");
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
    )
    .bind(command)
    .bind(&actor.user_id)
    .bind(&actor.assignment_id)
    .bind(operation)
    .bind(&record_type)
    .bind(record)
    .bind(version)
    .bind(now)
    .bind(authority)
    .execute(&mut *c)
    .await?;
    let payload = json!({
        "authority": serde_json::from_str::<Value>(authority).unwrap_or(Value::Null),
        "id": record,
        "state": result.get("enabled").cloned().unwrap_or(Value::Null),
        "companyId": actor.company_id,
        "branchId": Value::Null,
        "departmentId": Value::Null,
        "version": version,
    });
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(command)
    .bind(operation)
    .bind(&record_type)
    .bind(record)
    .bind(version)
    .bind(payload.to_string())
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Save one pay-group schedule. `key` is the caller idempotency key; the
/// receipt key is the idempotency key itself, exactly like the TypeScript
/// commit. The version assertion is folded into the `UPDATE` predicate,
/// mirroring guard45.
#[allow(clippy::too_many_arguments)]
pub async fn save_payroll_schedule(
    c: &mut PgConnection,
    actor: &PayrollScheduleCommandActor,
    key: &CommandKey,
    group_id: &str,
    enabled: bool,
    pay_delay_days: i64,
    anchor_date: Option<&str>,
    expected_version: i64,
    now: i64,
) -> Result<PayrollScheduleResult, Error> {
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
    let input: PayrollScheduleInput =
        validate_schedule(enabled, pay_delay_days, anchor_date, expected_version)
            .map_err(refusal)?;
    let operation = SCHEDULE_OPERATION;
    let canonical = json!({
        "enabled": input.enabled,
        "payDelayDays": input.pay_delay_days,
        "anchorDate": input.anchor_date,
        "expectedVersion": input.expected_version,
    })
    .to_string();
    let request_hash = fingerprint(actor, operation, group_id, &canonical);
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
            .and_then(|value| serde_json::from_str::<PayrollScheduleResult>(&value).ok())
            .ok_or_else(unavailable);
    }
    let Some(group) = schedule_group(c, &actor.company_id, group_id).await? else {
        return Err(refusal(group_missing()));
    };
    check_anchor(group.frequency, &input).map_err(refusal)?;
    if group.schedule_version != input.expected_version {
        return Err(refusal(version_conflict()));
    }
    let next_version = input.expected_version + 1;
    let changed = sqlx::query(
        "UPDATE hr_pay_groups SET schedule_enabled=$1,pay_delay_days=$2,schedule_anchor_date=$3,\
         schedule_version=schedule_version+1,updated_at=$4 \
         WHERE id=$5 AND company_id=$6 AND status='active' AND schedule_version=$7",
    )
    .bind(input.enabled)
    .bind(input.pay_delay_days)
    .bind(input.anchor_date.as_deref())
    .bind(now)
    .bind(group_id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(refusal(version_conflict()));
    }
    let result = PayrollScheduleResult {
        id: group_id.to_owned(),
        enabled: input.enabled,
        pay_delay_days: input.pay_delay_days,
        anchor_date: input.anchor_date,
        version: next_version,
    };
    let result_json = serde_json::to_value(&result).map_err(|_| unavailable())?;
    retain(
        c,
        actor,
        &authority_json(actor),
        key.as_str(),
        operation,
        &request_hash,
        group_id,
        &result_json,
        next_version,
        now,
    )
    .await?;
    Ok(result)
}

/// Read one pay-group schedule. The query applies no status filter, exactly
/// like the retained `readGetPayrollScheduleGroup` read; the active-group
/// gate lives only on the save path. Read-only: no receipt, audit or outbox
/// row is written.
pub async fn get_payroll_schedule(
    c: &mut PgConnection,
    actor: &PayrollScheduleCommandActor,
    group_id: &str,
) -> Result<PayrollScheduleView, Error> {
    serializable(c).await?;
    if actor.scope_kind != "holding" || actor.company_id.is_empty() {
        return Err(refusal(holding_required()));
    }
    let row = sqlx::query(
        "SELECT id,frequency,timezone,currency,schedule_enabled,pay_delay_days,\
         schedule_anchor_date,schedule_version FROM hr_pay_groups WHERE id=$1 AND company_id=$2",
    )
    .bind(group_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Err(refusal(group_missing_read()));
    };
    let pay_delay_days: i32 = row.try_get("pay_delay_days").map_err(|_| unavailable())?;
    Ok(PayrollScheduleView {
        id: row.try_get("id").map_err(|_| unavailable())?,
        frequency: row.try_get("frequency").map_err(|_| unavailable())?,
        timezone: row.try_get("timezone").map_err(|_| unavailable())?,
        currency: row.try_get("currency").map_err(|_| unavailable())?,
        enabled: row.try_get("schedule_enabled").map_err(|_| unavailable())?,
        pay_delay_days: i64::from(pay_delay_days),
        anchor_date: row.try_get("schedule_anchor_date").map_err(|_| unavailable())?,
        version: row.try_get("schedule_version").map_err(|_| unavailable())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> PayrollScheduleCommandActor {
        PayrollScheduleCommandActor {
            user_id: "user-1".to_owned(),
            assignment_id: "grant-1".to_owned(),
            role: "payroll_writer".to_owned(),
            company_id: "co".to_owned(),
            holding_id: "holding-1".to_owned(),
            scope_kind: "holding".to_owned(),
        }
    }

    #[test]
    fn fingerprint_is_stable_and_input_sensitive() {
        let actor = actor();
        let first = fingerprint(&actor, "people.payroll.schedule", "grp-1", "{}");
        assert_eq!(first, fingerprint(&actor, "people.payroll.schedule", "grp-1", "{}"));
        assert_eq!(first.len(), 64);
        assert_ne!(
            first,
            fingerprint(&actor, "people.payroll.schedule", "grp-1", r#"{"enabled":true}"#)
        );
    }
}
