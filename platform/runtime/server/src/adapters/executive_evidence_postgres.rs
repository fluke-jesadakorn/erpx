//! PostgreSQL adapter for native Executive evidence-access commands.
//!
//! The caller owns one serializable Holding transaction. This adapter writes
//! only the append-only `executive_evidence_access_events` audit rows plus the
//! standard command receipt/audit/outbox ledger. It never opens a transaction
//! and never writes another owner's rows. All SQL lives here, inside the owner
//! crate.
use crate::application::executive_evidence::{
    EvidenceAccessRepository, EvidenceFuture, EvidenceInsert, EvidenceLedgerEntry,
    StoredEvidenceReceipt,
};
use crate::domain::executive_evidence::unavailable;
use crate::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub struct PgEvidenceAccessCommand<'c> {
    connection: &'c mut PgConnection,
    holding: String,
}

impl<'c> PgEvidenceAccessCommand<'c> {
    /// `holding` is the Holding id carried on the ledger command rows.
    pub fn new(connection: &'c mut PgConnection, holding: &str) -> Self {
        Self {
            connection,
            holding: holding.to_owned(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("EXECUTIVE_EVIDENCE_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("EXECUTIVE_EVIDENCE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &EvidenceLedgerEntry<'_>) -> Result<(), Error> {
        let result = serde_json::to_string(entry.result).map_err(|_| unavailable())?;
        let changed = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(&entry.actor.user_id)
        .bind(entry.request_hash)
        .bind(&entry.actor.user_id)
        .bind(result)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different evidence access.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("executive-evidence-audit-{}", entry.key))
        .bind(entry.key)
        .bind(&entry.actor.user_id)
        .bind(&entry.actor.assignment_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.operation)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        let payload = serde_json::json!({
            "holdingId": self.holding,
            "companyId": entry.actor.company_id,
            "actorId": entry.actor.user_id,
            "assignmentId": entry.actor.assignment_id,
            "provider": entry.result.get("provider").cloned().unwrap_or(serde_json::Value::Null),
            "metric": entry.result.get("metric").cloned().unwrap_or(serde_json::Value::Null),
            "count": entry.result.get("count").cloned().unwrap_or(serde_json::Value::Null),
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("executive-evidence-outbox-{}", entry.key))
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(&entry.actor.user_id)
        .bind(entry.version)
        .bind(payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl EvidenceAccessRepository for PgEvidenceAccessCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> EvidenceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("executive-evidence-access-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> EvidenceFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(identity)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EvidenceFuture<'a, Option<StoredEvidenceReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let request_hash = row.try_get::<String, _>("request_hash")?;
            let raw: Option<String> = row.try_get("result_json")?;
            let result = match raw {
                Some(text) => serde_json::from_str(&text).ok(),
                None => None,
            };
            Ok(Some(StoredEvidenceReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a EvidenceLedgerEntry<'a>,
    ) -> EvidenceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn insert_events<'a>(
        &'a mut self,
        command: &'a EvidenceInsert<'a>,
    ) -> EvidenceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let records = serde_json::to_string(command.records).map_err(|_| unavailable())?;
            sqlx::query(
                "INSERT INTO executive_evidence_access_events \
                 (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,\
                  record_version,field_ids,query_hash,created_at) \
                 SELECT 'executive-evidence-'||gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,\
                  NULLIF(item.record_id,''),NULLIF(item.record_version,0),$7::jsonb,$8,$9 \
                 FROM jsonb_to_recordset($10::jsonb) \
                 AS item(record_id text,record_version bigint)",
            )
            .bind(&command.actor.company_id)
            .bind(&command.actor.user_id)
            .bind(&command.actor.assignment_id)
            .bind(command.provider)
            .bind(command.metric)
            .bind(command.action)
            .bind(command.field_ids)
            .bind(command.query_hash)
            .bind(command.now)
            .bind(records)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
}
