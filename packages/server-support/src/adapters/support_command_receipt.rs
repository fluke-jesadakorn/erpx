//! Support owner command receipt/audit/outbox retention.
//!
//! The Support crate cannot depend on `erpx-server-workflow` under the pinned
//! `--locked` manifest, so this module preserves the ledger's observable
//! contract on the caller connection: exact request replay, one receipt, one
//! audit and one outbox event in the same unit. Composition still owns current
//! authority and the serializable transaction.
use crate::domain::ticket_portal::digest;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct SupportCommandReceipt<'a> {
    pub key: &'a str,
    pub actor_id: &'a str,
    pub assignment_id: &'a str,
    pub role: &'a str,
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
    pub operation: &'a str,
    pub record_id: &'a str,
    pub record_type: &'a str,
    pub aggregate_type: &'a str,
    pub version: i64,
    pub state: &'a str,
    pub input: &'a Value,
    pub result: &'a Value,
    pub authority_json: &'a str,
    pub created_at: i64,
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn invalid_json() -> Error {
    BusinessError::invalid(
        "SUPPORT_AUTHORITY_INVALID",
        "Provide the current verified command authority.",
    )
    .into()
}

/// Stable fingerprint of the actor/scope/operation/record/input. It is local to
/// the Support owner receipt contract and intentionally does not expose authority.
pub fn support_request_hash(
    actor_id: &str,
    assignment_id: &str,
    role: &str,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    operation: &str,
    record_id: &str,
    input: &Value,
) -> String {
    digest(&json!({
        "actor": actor_id,
        "assignment": assignment_id,
        "role": role,
        "company": company_id,
        "branch": branch_id,
        "department": department_id,
        "operation": operation,
        "recordId": record_id,
        "input": input,
    }))
}

pub fn support_command_fingerprint(receipt: &SupportCommandReceipt<'_>) -> Result<String, Error> {
    Ok(support_request_hash(
        receipt.actor_id,
        receipt.assignment_id,
        receipt.role,
        receipt.company_id,
        receipt.branch_id,
        receipt.department_id,
        receipt.operation,
        receipt.record_id,
        receipt.input,
    ))
}

/// Return the exact retained result only when the current actor, operation,
/// record and request hash all match. A key owned by another request conflicts.
pub async fn replay_support_receipt(
    c: &mut PgConnection,
    key: &str,
    operation: &str,
    actor_id: &str,
    record_id: &str,
    request_hash: &str,
) -> Result<Option<Value>, Error> {
    let row = sqlx::query(
        "SELECT operation,actor_id,record_id,request_hash,result_json \
         FROM command_receipts WHERE command_id=$1 FOR SHARE",
    )
    .bind(key)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.try_get::<String, _>("operation")? != operation
        || row.try_get::<String, _>("actor_id")? != actor_id
        || row.try_get::<String, _>("record_id")? != record_id
        || row.try_get::<String, _>("request_hash")? != request_hash
    {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to another request.",
        ));
    }
    let raw: Option<String> = row.try_get("result_json")?;
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|_| {
            conflict(
                "SUPPORT_COMMAND_STATE_CHANGED",
                "The retained command result is unavailable.",
            )
        })
    })
    .transpose()
}

/// Write one receipt, one audit event and one outbox event before the caller
/// commits. `authority_json` must be the current verified authority JSON.
pub async fn retain_support_receipt(
    c: &mut PgConnection,
    receipt: &SupportCommandReceipt<'_>,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SUPPORT_COMMAND_TRANSACTION_REQUIRED").into());
    }
    let request_hash = support_command_fingerprint(receipt)?;
    let authority = if receipt.authority_json.trim().is_empty() {
        "{}"
    } else {
        receipt.authority_json
    };
    if serde_json::from_str::<Value>(authority).is_err() {
        return Err(invalid_json());
    }
    let result = serde_json::to_string(receipt.result).map_err(|_| invalid_json())?;
    let inserted: String = sqlx::query_scalar(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,\
         record_id,result_json,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id \
         RETURNING request_hash",
    )
    .bind(receipt.key)
    .bind(receipt.operation)
    .bind(receipt.actor_id)
    .bind(&request_hash)
    .bind(receipt.record_id)
    .bind(&result)
    .bind(receipt.created_at)
    .bind(authority)
    .fetch_one(&mut *c)
    .await?;
    if inserted != request_hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to another request.",
        ));
    }
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
         record_id,record_version,purpose,created_at,authority_json) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,\
         (SELECT authority_json FROM command_receipts WHERE command_id=$2))",
    )
    .bind(format!("support-audit-{}", receipt.key))
    .bind(receipt.key)
    .bind(receipt.actor_id)
    .bind(receipt.assignment_id)
    .bind(receipt.operation)
    .bind(receipt.record_type)
    .bind(receipt.record_id)
    .bind(receipt.version)
    .bind(receipt.created_at)
    .execute(&mut *c)
    .await?;
    let payload = json!({
        "id": receipt.record_id,
        "version": receipt.version,
        "state": receipt.state,
        "companyId": receipt.company_id,
        "branchId": receipt.branch_id,
        "departmentId": receipt.department_id,
        "authority": serde_json::from_str::<Value>(authority).unwrap_or(Value::Null),
    })
    .to_string();
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(format!("support-outbox-{}", receipt.key))
    .bind(receipt.key)
    .bind(receipt.operation)
    .bind(receipt.aggregate_type)
    .bind(receipt.record_id)
    .bind(receipt.version)
    .bind(payload)
    .bind(receipt.created_at)
    .execute(&mut *c)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_hash_is_stable_and_binds_actor_scope_and_input() {
        let input = json!({"action":"approve","expectedVersion":1});
        let base = support_request_hash(
            "user-1",
            "assignment-1",
            "support_manager",
            "company",
            None,
            None,
            "support.ticket-portal.approve",
            "binding-1",
            &input,
        );
        let replay = support_request_hash(
            "user-1",
            "assignment-1",
            "support_manager",
            "company",
            None,
            None,
            "support.ticket-portal.approve",
            "binding-1",
            &input,
        );
        assert_eq!(base, replay);
        for changed in [
            support_request_hash(
                "user-2",
                "assignment-1",
                "support_manager",
                "company",
                None,
                None,
                "support.ticket-portal.approve",
                "binding-1",
                &input,
            ),
            support_request_hash(
                "user-1",
                "assignment-2",
                "support_manager",
                "company",
                None,
                None,
                "support.ticket-portal.approve",
                "binding-1",
                &input,
            ),
            support_request_hash(
                "user-1",
                "assignment-1",
                "support_manager",
                "company",
                Some("branch"),
                None,
                "support.ticket-portal.approve",
                "binding-1",
                &input,
            ),
            support_request_hash(
                "user-1",
                "assignment-1",
                "support_manager",
                "company",
                None,
                None,
                "support.ticket-portal.reject",
                "binding-1",
                &input,
            ),
            support_request_hash(
                "user-1",
                "assignment-1",
                "support_manager",
                "company",
                None,
                None,
                "support.ticket-portal.approve",
                "binding-2",
                &input,
            ),
            support_request_hash(
                "user-1",
                "assignment-1",
                "support_manager",
                "company",
                None,
                None,
                "support.ticket-portal.approve",
                "binding-1",
                &json!({"action":"approve","expectedVersion":2}),
            ),
        ] {
            assert_ne!(base, changed);
        }
        assert_eq!(base.len(), 64);
    }
}
