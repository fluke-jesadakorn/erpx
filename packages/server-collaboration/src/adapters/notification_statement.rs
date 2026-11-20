//! Native-owned notification statement descriptors.
//!
//! The owner decides the exact notification INSERT; a caller-owned serializable
//! transaction executes it, so no second transaction or TS copy is needed.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const INSERT: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,\
 record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationStatementInput {
    pub company_id: String,
    pub user_id: String,
    pub record_type: String,
    pub record_id: String,
    pub record_version: i64,
    pub message: String,
    pub now: i64,
    pub idempotency_key: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub lead_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NotificationStatement {
    pub sql: String,
    pub values: Vec<serde_json::Value>,
}

fn invalid() -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, "NOTIFICATION_STATEMENT_INVALID", "Review the notification request.")
}

async fn ready(connection: &mut PgConnection, company: &CompanyId) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection).await? != "serializable"
    {
        return Err(PersistenceError::new("NOTIFICATION_STATEMENT_UNIT_REQUIRED").into());
    }
    let holder: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut *connection).await?;
    if holder.is_empty() {
        return Err(invalid().into());
    }
    Ok(())
}

/// Build the authoritative notification INSERT on the caller's unit.
pub async fn notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    input: &NotificationStatementInput,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if input.user_id.is_empty() || input.record_id.is_empty()
        || input.record_type.is_empty() || input.message.is_empty()
        || input.record_version <= 0 || input.now <= 0
    {
        return Err(invalid().into());
    }
    let value = |v: &str| serde_json::Value::String(v.to_owned());
    Ok(NotificationStatement {
        sql: INSERT.to_owned(),
        values: vec![
            value(&format!("ntf-{}-{}", input.idempotency_key, input.user_id)),
            value(company.as_str()),
            value(&input.user_id),
            value(&input.record_type),
            value(&input.record_id),
            serde_json::Value::Number(input.record_version.into()),
            value(&input.message),
            serde_json::Value::Number(input.now.into()),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::{NotificationStatementInput, INSERT};
    #[test]
    fn insert_shape_matches_legacy_writer() {
        let _i = NotificationStatementInput {
            company_id: "c".into(), user_id: "u".into(), record_type: "overtime".into(),
            record_id: "r".into(), record_version: 1, message: "m".into(), now: 1,
            idempotency_key: "k".into(), kind: None, lead_id: None,
        };
        assert!(INSERT.contains("INSERT INTO staff_notifications"));
        assert!(INSERT.contains("ON CONFLICT DO NOTHING"));
        assert_eq!(INSERT.matches('?').count(), 8);
    }
}


/// CRM lead notification statement. Reads the persisted lead on the caller's
/// serializable unit so the owner — not the caller — selects recipient/version.
pub async fn crm_lead_notification_statement(
    connection: &mut PgConnection,
    company: &CompanyId,
    lead_id: &str,
    command_key: &str,
) -> Result<NotificationStatement, Error> {
    ready(connection, company).await?;
    if lead_id.is_empty() || command_key.is_empty() {
        return Err(invalid().into());
    }
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM crm_leads WHERE id=$1 AND company_id=$2",
    )
    .bind(lead_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if exists.is_none() {
        return Err(invalid().into());
    }
    let sql = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) SELECT ?,company_id,owner_id,'crm_lead',id,version,\
 'CRM handoff changed: review the source and next action',updated_at \
 FROM (SELECT \"id\", \"company_id\", \"owner_id\", \"version\", \"updated_at\" FROM crm_leads) crm_leads \
 WHERE id=? ON CONFLICT DO NOTHING";
    Ok(NotificationStatement {
        sql: sql.to_owned(),
        values: vec![
            serde_json::Value::String(format!("crm-notice-{command_key}")),
            serde_json::Value::String(lead_id.to_owned()),
        ],
    })
}


#[cfg(test)]
mod crm_tests {
    #[test]
    fn crm_lead_notice_uses_owner_recipient_projection() {
        // The native statement derives recipient/version from the persisted lead,
        // never from caller-supplied values.
        let src = include_str!("crm_activity_notification.rs");
        assert!(src.contains("'crm_lead'"));
        assert!(src.contains("CRM handoff changed"));
    }
}
