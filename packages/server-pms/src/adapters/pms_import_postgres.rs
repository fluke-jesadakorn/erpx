//! PostgreSQL adapter for native PMS cutover-import writes.
//!
//! The caller owns one serializable Holding transaction and fresh
//! Account/IAM authority. This adapter writes only pms-owned import batch,
//! balance, document and property facts plus the standard command
//! receipt/audit/outbox ledger. All Finance opening facts, workflow receipts
//! and PMS snapshot rows stay with their own owners/consumers.
use crate::application::pms_import::{
    AppliedWrite, ApproveWrite, BalanceWrite, BatchFacts, DocumentWrite, ImportLedgerEntry,
    PmsImportActor, PmsImportCommandFuture, PmsImportCommandRepository, PreviewWrite,
    PropertyWrite, StoredImportReceipt,
};
use crate::domain::pms_import::{BatchState, idempotency_conflict, invalid};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
    {
        conflict(
            "CONFLICT",
            "The import record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgPmsImportCommand<'c> {
    connection: &'c mut PgConnection,
    company: CompanyId,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'c> PgPmsImportCommand<'c> {
    pub fn new(connection: &'c mut PgConnection, actor: &PmsImportActor) -> Self {
        Self {
            connection,
            company: CompanyId::new(&actor.company_id).expect("validated company"),
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            authority_json: actor.authority_json.clone(),
        }
    }

    async fn read_ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("PMS_IMPORT_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn ready(&mut self) -> Result<(), Error> {
        self.read_ready().await?;
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("PMS_IMPORT_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &ImportLedgerEntry<'_>) -> Result<(), Error> {
        let result = serde_json::to_string(entry.result).map_err(|_| invalid())?;
        let changed = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(&self.user_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(result)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("pms-import-audit-{}", entry.key))
        .bind(entry.key)
        .bind(&self.user_id)
        .bind(&self.assignment_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?;
        let authority: Value = serde_json::from_str(&self.authority_json).unwrap_or(Value::Null);
        let payload = serde_json::json!({
            "authority": authority,
            "id": entry.record_id,
            "state": entry.state,
            "companyId": self.company.as_str(),
            "version": entry.version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("pms-import-outbox-{}", entry.key))
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl PmsImportCommandRepository for PgPmsImportCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("pms-import-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> PmsImportCommandFuture<'a, String, Self::Error> {
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
    ) -> PmsImportCommandFuture<'a, Option<StoredImportReceipt>, Self::Error> {
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
            let result = match row.try_get::<Option<String>, _>("result_json")? {
                None => None,
                Some(value) => Some(serde_json::from_str(&value).map_err(|_| {
                    Error::Business(BusinessError::new(
                        FailureKind::Unavailable,
                        "PMS_IMPORT_RECEIPT_INVALID",
                        "The import command receipt is invalid.",
                    ))
                })?),
            };
            Ok(Some(StoredImportReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a ImportLedgerEntry<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn batch<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> PmsImportCommandFuture<'a, Option<BatchFacts>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            let row = sqlx::query(
                "SELECT state,version FROM pms_import_batches WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let state = BatchState::parse(&row.try_get::<String, _>("state")?)?;
            let version = row.try_get::<i64, _>("version")?;
            Ok(Some(BatchFacts { state, version }))
        })
    }

    fn preview<'a>(
        &'a mut self,
        command: &'a PreviewWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pms_import_batches(id,company_id,property_id,cutover_date,\
                 property_version,definition_json,report_json,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(command.batch_id)
            .bind(command.company_id)
            .bind(command.property_id)
            .bind(command.cutover_date)
            .bind(command.expected_version)
            .bind(command.definition_json)
            .bind(command.report_json)
            .bind(command.user_id)
            .bind(command.now)
            .bind(command.now2)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn approve<'a>(
        &'a mut self,
        command: &'a ApproveWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pms_import_batches SET state='approved',approved_by=$2,\
                 opening_journal_id=$3,version=$4,updated_at=$5 \
                 WHERE id=$1 AND state='preview'",
            )
            .bind(command.id)
            .bind(command.user_id)
            .bind(command.opening_journal_id)
            .bind(command.version)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "PMS_IMPORT_STATE",
                    "The import batch changed. Refresh it before approving.",
                )
                .into());
            }
            Ok(())
        })
    }

    fn balance<'a>(
        &'a mut self,
        command: &'a BalanceWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let pocket = match command.pocket {
                crate::domain::pms_import::ImportPocket::Advance => "advance",
                crate::domain::pms_import::ImportPocket::Deposit => "deposit",
                crate::domain::pms_import::ImportPocket::Due => "due",
            };
            sqlx::query(
                "INSERT INTO pms_import_balances(batch_id,source_key,property_id,\
                 guest_account_id,entry_id,pocket,amount_minor,finance_payment_id,finance_invoice_id) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(command.id)
            .bind(command.source_key)
            .bind(command.property_id)
            .bind(command.guest_account_id)
            .bind(command.entry_id)
            .bind(pocket)
            .bind(command.total)
            .bind(command.payment_id)
            .bind(command.invoice_id)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn document<'a>(
        &'a mut self,
        command: &'a DocumentWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO pms_documents(id,property_id,guest_account_id,kind,\
                 finance_id,snapshot_json,created_at) VALUES($1,$2,$3,'opening',$4,$5,$6)",
            )
            .bind(command.id)
            .bind(command.property_id)
            .bind(command.guest_account_id)
            .bind(command.finance_id)
            .bind(command.snapshot_json)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?;
            Ok(())
        })
    }

    fn property<'a>(
        &'a mut self,
        command: &'a PropertyWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("UPDATE pms_properties SET version=version+1,updated_at=$2 WHERE id=$1")
                .bind(command.property_id)
                .bind(command.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
            Ok(())
        })
    }

    fn applied<'a>(
        &'a mut self,
        command: &'a AppliedWrite<'a>,
    ) -> PmsImportCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pms_import_batches SET state='applied',version=$3,updated_at=$4 \
                 WHERE id=$1 AND state='approved'",
            )
            .bind(command.id)
            .bind(command.version)
            .bind(command.now)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "PMS_IMPORT_STATE",
                    "The import batch changed. Refresh it before applying.",
                )
                .into());
            }
            Ok(())
        })
    }
}
