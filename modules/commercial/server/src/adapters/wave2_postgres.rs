//! PostgreSQL adapter for the native Commercial wave2 grant-save command.
//!
//! The caller owns one serializable Holding transaction. This adapter writes
//! only the Commercial-owned grant row and grant-event row plus the shared
//! `command_receipts` / `audit_events` / `outbox_events` ledger records. It
//! never opens a transaction and never writes another owner's rows. All SQL
//! lives here, inside the owner crate.
use crate::application::wave2::{
    GrantLedgerEntry, GrantSaveFuture, GrantSaveInput, GrantSaveRepository, StoredGrantReceipt,
};
use crate::domain::wave2::invalid;
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgGrantSave<'c> {
    connection: &'c mut PgConnection,
    company: String,
}

impl<'c> PgGrantSave<'c> {
    pub fn new(connection: &'c mut PgConnection, scope: &str) -> Self {
        Self {
            connection,
            company: scope.to_owned(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCIAL_WAVE2_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("COMMERCIAL_WAVE2_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &GrantLedgerEntry<'_>) -> Result<(), Error> {
        let authority = serde_json::json!({"companyId": &self.company});
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6::jsonb,$7,$8::jsonb) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(format!("wave2:{}", entry.key))
        .bind(entry.operation)
        .bind(entry.actor_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(sqlx::types::Json(entry.result.clone()))
        .bind(entry.now)
        .bind(sqlx::types::Json(authority.clone()))
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(BusinessError::new(
                erpx_platform_shared_server::public::FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different grant write.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
             record_version,purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$2,$3,$4,$5,$6,$3,$7,$8::jsonb)",
        )
        .bind(entry.key)
        .bind(entry.actor_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.now)
        .bind(sqlx::types::Json(authority.clone()))
        .execute(&mut *self.connection)
        .await?;
        let payload = serde_json::json!({
            "companyId": &self.company,
            "recordId": entry.record_id,
            "version": entry.version,
            "grant": entry.result,
        });
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6::jsonb,$7)",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(sqlx::types::Json(payload))
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl GrantSaveRepository for PgGrantSave<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> GrantSaveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("commercial-wave2-grant:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> GrantSaveFuture<'a, String, Self::Error> {
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
    ) -> GrantSaveFuture<'a, Option<StoredGrantReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(format!("wave2:{key}"))
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            Ok(Some(StoredGrantReceipt {
                request_hash: row.try_get("request_hash")?,
                result: Some(row.try_get("result_json")?),
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a GrantLedgerEntry<'a>,
    ) -> GrantSaveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn save_grant<'a>(
        &'a mut self,
        input: &'a GrantSaveInput,
    ) -> GrantSaveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            // The authoritative grant upsert. All columns are bound; no TS copy.
            sqlx::query(
                "INSERT INTO company_package_grants\
                 (company_id,package_id,package_version,state,starts_at,ends_at,version,\
                 reason,evidence,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
                 ON CONFLICT(company_id,package_id) DO UPDATE SET \
                 package_version=excluded.package_version,state=excluded.state,\
                 starts_at=excluded.starts_at,ends_at=excluded.ends_at,version=excluded.version,\
                 reason=excluded.reason,evidence=excluded.evidence,updated_at=excluded.updated_at",
            )
            .bind(&input.company_id)
            .bind(&input.package_id)
            .bind(input.package_version)
            .bind(&input.state)
            .bind(input.starts_at)
            .bind(input.ends_at)
            .bind(input.version)
            .bind(&input.reason)
            .bind(&input.evidence)
            .bind(input.updated_at)
            .execute(&mut *self.connection)
            .await?;
            // Append-only grant event carrying the exact before/after evidence.
            sqlx::query(
                "INSERT INTO company_package_grant_events\
                 (id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6::jsonb,$7::jsonb,$8) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&input.idempotency_key)
            .bind(&input.company_id)
            .bind(&input.package_id)
            .bind(&input.actor_id)
            .bind(&input.request_hash)
            .bind(sqlx::types::Json(
                input.before_json.as_deref().map(|text| {
                    serde_json::from_str::<serde_json::Value>(text)
                        .unwrap_or(serde_json::Value::Null)
                }),
            ))
            .bind(sqlx::types::Json(
                serde_json::from_str::<serde_json::Value>(&input.after_json)
                    .unwrap_or(serde_json::Value::Null),
            ))
            .bind(input.updated_at)
            .execute(&mut *self.connection)
            .await?;
            let _ = invalid; // domain guard symbol retained for parity.
            Ok(())
        })
    }
}
