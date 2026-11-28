use crate::application::owner_write::{OwnerWriteCommand, OwnerWritePort};
use crate::domain::owner_write_registry::{
    PL_ATTACHMENT_VERSION_STEP, PL_LEGAL_RELEASE_REVISION_INIT, PL_RELEASE_ACCEPTANCE_INSERT,
    PL_RELEASE_GATE_REVIEW_UPSERT,
    PL_RELEASE_REVISION_INIT, PL_RELEASE_REVOKE, PL_RELEASE_VERSION_STEP,
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
            PL_RELEASE_REVISION_INIT =>
                "INSERT INTO company_release_revisions(company_id,revision) VALUES($1,0) ON CONFLICT DO NOTHING",
            PL_LEGAL_RELEASE_REVISION_INIT =>
                "INSERT INTO company_legal_release_revisions(company_id,revision) VALUES($1,0) ON CONFLICT DO NOTHING",
            PL_RELEASE_ACCEPTANCE_INSERT =>
                "INSERT INTO company_release_acceptances(id,company_id,scope_version,software_version,configuration_revision,setup_version,evidence_sha256,evidence_json,requested_by,reason,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            PL_RELEASE_GATE_REVIEW_UPSERT =>
                "INSERT INTO company_release_gate_reviews(company_id,release_id,criterion_id,state,reviewer_id,assignment_id,permission,reason,reviewed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(release_id,criterion_id) DO UPDATE SET state=excluded.state,reviewer_id=excluded.reviewer_id,assignment_id=excluded.assignment_id,permission=excluded.permission,reason=excluded.reason,reviewed_at=excluded.reviewed_at",
            PL_RELEASE_VERSION_STEP =>
                "UPDATE company_release_acceptances SET version=$1,updated_at=$2 WHERE company_id=$3 AND id=$4",
            PL_RELEASE_REVOKE =>
                "UPDATE company_release_acceptances SET revoked_at=$1,version=$2,updated_at=$3 WHERE company_id=$4 AND id=$5",
            PL_ATTACHMENT_VERSION_STEP =>
                "UPDATE company_release_acceptances SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3",
            _ => return Err(BusinessError::invalid(
                "PLATFORM_OWNER_WRITE_INVALID", "Unsupported owner write kind.")),
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
            return Err(PersistenceError::new("PLATFORM_OWNER_WRITE_TRANSACTION_REQUIRED").into());
        }
        let sql = Self::sql(command.kind).map_err(OwnerWriteError::from)?;
        bind_all(sqlx::query(sql), command.values)
            .execute(&mut *self.connection)
            .await?;

        let receipt_id = format!("platform-owner-write:{}:{}", command.kind, key);
        let audit_id = format!("platform-owner-write-audit:{}:{}", command.kind, key);
        let outbox_id = format!("platform-owner-write-outbox:{}:{}", command.kind, key);
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
        .bind("Platform Administration native owner write")
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
