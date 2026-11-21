//! PostgreSQL adapter for the native Commerce wave2 warehouse-master command.
//!
//! The caller owns one serializable Holding transaction. Only Commerce-owned
//! warehouse rows plus the shared command ledger are written here. All SQL
//! lives here, inside the owner crate.
use crate::application::wave2::{
    WarehouseLedgerEntry, WarehouseSaveFuture, WarehouseSaveInput, WarehouseSaveRepository,
    WarehouseWriteKind,
};
use crate::public::wave2::StoredWarehouseReceipt;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

pub struct PgWarehouseSave<'c> {
    connection: &'c mut PgConnection,
    company: String,
}

impl<'c> PgWarehouseSave<'c> {
    pub fn new(connection: &'c mut PgConnection, scope: &str) -> Self {
        Self {
            connection,
            company: scope.to_owned(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCE_WAVE2_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("COMMERCE_WAVE2_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &WarehouseLedgerEntry<'_>) -> Result<(), Error> {
        let authority = serde_json::json!({"companyId": &self.company});
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(format!("wave2:{}", entry.key))
        .bind(entry.operation)
        .bind(entry.actor_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(entry.result)
        .bind(entry.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different warehouse write.",
            )
            .into());
        }
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
             record_version,purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$2,$3,$4,$5,1,$3,$6,$7)",
        )
        .bind(entry.key)
        .bind(entry.actor_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        let payload = serde_json::json!({
            "companyId": &self.company,
            "recordId": entry.record_id,
            "result": entry.result,
        });
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,1,$5,$6)",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(&payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl WarehouseSaveRepository for PgWarehouseSave<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> WarehouseSaveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("commerce-wave2-warehouse:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> WarehouseSaveFuture<'a, String, Self::Error> {
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
    ) -> WarehouseSaveFuture<'a, Option<StoredWarehouseReceipt>, Self::Error> {
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
            Ok(Some(StoredWarehouseReceipt {
                request_hash: row.try_get("request_hash")?,
                result: Some(row.try_get("result_json")?),
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a WarehouseLedgerEntry<'a>,
    ) -> WarehouseSaveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn write_warehouse<'a>(
        &'a mut self,
        input: &'a WarehouseSaveInput,
        kind: WarehouseWriteKind,
    ) -> WarehouseSaveFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            match kind {
                WarehouseWriteKind::Create => {
                    sqlx::query(
                        "INSERT INTO commerce_warehouses\
                         (id,company_id,branch_id,code,name,status) VALUES($1,$2,$3,$4,$5,$6) \
                         ON CONFLICT(id) DO NOTHING",
                    )
                    .bind(&input.record)
                    .bind(&input.company_id)
                    .bind(&input.branch_id)
                    .bind(&input.code)
                    .bind(&input.name)
                    .bind(&input.status)
                    .execute(&mut *self.connection)
                    .await?;
                }
                WarehouseWriteKind::Update => {
                    let changed = sqlx::query(
                        "UPDATE commerce_warehouses SET code=$1,name=$2,status=$3,version=version+1 \
                         WHERE id=$4 AND company_id=$5",
                    )
                    .bind(&input.code)
                    .bind(&input.name)
                    .bind(&input.status)
                    .bind(input.id.as_deref().unwrap_or_default())
                    .bind(&input.company_id)
                    .execute(&mut *self.connection)
                    .await?
                    .rows_affected();
                    if changed != 1 {
                        return Err(conflict(
                            "COMMERCE_WAREHOUSE_SOURCE_CHANGED",
                            "This warehouse no longer exists in the Company.",
                        )
                        .into());
                    }
                }
            }
            Ok(())
        })
    }
}
