//! PostgreSQL owner adapter for every native Manage ERPX write. All SQL lives
//! here in the owner crate. The caller owns one transaction; this adapter
//! executes the typed kind and writes the command receipt, audit event and
//! outbox event in that same unit (CQRS command side).
use crate::application::owner_write::{OwnerWriteCommand, OwnerWritePort};
use crate::domain::owner_write_registry::{
    ME_ATTACHMENT_VERSION_STEP, ME_BETA_FEEDBACK_INSERT, ME_BETA_INVITATION_ACCEPT,
    ME_BETA_INVITATION_INSERT, ME_BETA_MEMBERSHIP_DELETE, ME_BETA_MEMBERSHIP_INSERT,
};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use sqlx::Connection;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Database, PgConnection, Postgres, query::Query};

pub type OwnerWriteError = PgTransactionError<BusinessError>;

type BoundQuery<'q> = Query<'q, Postgres, <Postgres as Database>::Arguments>;

pub struct PgOwnerWrite<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOwnerWrite<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn sql(kind: &str) -> Result<&'static str, BusinessError> {
        Ok(match kind {
            ME_ATTACHMENT_VERSION_STEP =>
                "UPDATE company_provisioning SET version=version+1,updated_at=$1 WHERE company_id=$2 AND version=$3",
            ME_BETA_FEEDBACK_INSERT =>
                "INSERT INTO beta_feedback(id,company_id,actor_id,body,created_at) VALUES($1,$2,$3,$4,$5)",
            ME_BETA_MEMBERSHIP_DELETE => "DELETE FROM beta_members WHERE user_id=$1",
            ME_BETA_MEMBERSHIP_INSERT =>
                "INSERT INTO beta_members(user_id,invitation_id,joined_at) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
            ME_BETA_INVITATION_INSERT =>
                "INSERT INTO beta_team_invitations(id,email,branch_id,invited_by,token_hash,expires_at,created_at,role) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            ME_BETA_INVITATION_ACCEPT =>
                "UPDATE beta_team_invitations SET accepted_user_id=$1,accepted_at=$2 WHERE id=$3",
            _ => return Err(BusinessError::invalid(
                "MANAGE_ERPX_OWNER_WRITE_INVALID", "Unsupported owner write kind.")),
        })
    }
}

fn bind_one<'q>(query: BoundQuery<'q>, value: &'q Value) -> BoundQuery<'q> {
    match value {
        Value::Null => query.bind(Option::<String>::None),
        Value::Bool(value) => query.bind(*value),
        Value::Number(number) if number.is_i64() => query.bind(number.as_i64().unwrap_or(0)),
        Value::Number(number) if number.is_u64() => {
            query.bind(number.as_u64().unwrap_or(0) as i64)
        }
        Value::Number(number) => query.bind(number.as_f64().unwrap_or(0.0)),
        Value::String(value) => query.bind(value.clone()),
        other => query.bind(other.to_string()),
    }
}

fn bind_all<'q>(mut query: BoundQuery<'q>, values: &'q [Value]) -> BoundQuery<'q> {
    for value in values {
        query = bind_one(query, value);
    }
    query
}

impl OwnerWritePort for PgOwnerWrite<'_> {
    type Error = OwnerWriteError;

    async fn execute(
        &mut self,
        command: &OwnerWriteCommand<'_>,
        key: &str,
    ) -> Result<Value, Self::Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("MANAGE_ERPX_OWNER_WRITE_TRANSACTION_REQUIRED").into());
        }
        let sql = Self::sql(command.kind).map_err(OwnerWriteError::from)?;
        bind_all(sqlx::query(sql), command.values)
            .execute(&mut *self.connection)
            .await?;

        let receipt_id = format!("manage-owner-write:{}:{}", command.kind, key);
        let audit_id = format!("manage-owner-write-audit:{}:{}", command.kind, key);
        let outbox_id = format!("manage-owner-write-outbox:{}:{}", command.kind, key);
        let result = json!({"kind": command.kind, "companyId": command.company_id});
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&receipt_id)
        .bind(command.kind)
        .bind(command.actor_id)
        .bind(key)
        .bind(command.company_id)
        .bind(&result)
        .bind(command.now)
        .bind(json!({}))
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&audit_id)
        .bind(&receipt_id)
        .bind(command.actor_id)
        .bind("")
        .bind(command.kind)
        .bind("owner_write")
        .bind(command.company_id)
        .bind(1_i64)
        .bind("Manage ERPX native owner write")
        .bind(command.now)
        .bind(json!({}))
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&outbox_id)
        .bind(&receipt_id)
        .bind(format!("{}.written", command.kind))
        .bind("owner_write")
        .bind(command.company_id)
        .bind(1_i64)
        .bind(&result)
        .bind(command.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(result)
    }
}
