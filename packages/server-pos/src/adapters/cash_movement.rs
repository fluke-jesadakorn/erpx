//! PostgreSQL adapter for the native POS cash-movement command.
//!
//! All SQL is POS-owned (`pos_shifts`, `pos_cash_movements`, `pos_receipts`,
//! `pos_audit`, `pos_settings`, `pos_day_closes`). No Finance, Inventory or
//! Commerce row is read or written: the retained `cash.create` command has no
//! such effect. The caller's serializable Holding transaction is required and
//! no commit is performed here.
use crate::application::cash_movement::{CashMovementFuture, CashMovementRepository};
use crate::domain::cash_movement::{
    CashMovementActor, CashMovementReadback, CashMovementReceipt, CashReceipt, CashShift,
    OPERATION, ValidatedCashMovement,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis, UserId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgCashMovementCommand<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgCashMovementCommand<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        require_serializable(self.connection).await
    }
}

async fn require_serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("POS_CASH_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

impl CashMovementRepository for PgCashMovementCommand<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> CashMovementFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CashMovementFuture<'a, Option<CashReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query("SELECT request_hash,result_json FROM pos_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(CashReceipt {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }

    fn shift<'a>(
        &'a mut self,
        company: &'a CompanyId,
        shift_id: &'a str,
    ) -> CashMovementFuture<'a, Option<CashShift>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,business_date,state,version \
                 FROM pos_shifts WHERE id=$1 AND company_id=$2 FOR UPDATE",
            )
            .bind(shift_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CashShift {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    business_date: row.try_get("business_date")?,
                    state: row.try_get("state")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn setup_exists<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pos_settings WHERE company_id=$1)",
            )
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn day_writable<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT NOT EXISTS(SELECT 1 FROM pos_day_closes \
                 WHERE company_id=$1 AND branch_id=$2 AND business_date=$3 AND state='prepared')",
            )
            .bind(company.as_str())
            .bind(branch_id)
            .bind(business_date)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn insert_movement<'a>(
        &'a mut self,
        movement: &'a ValidatedCashMovement,
        actor: &'a UserId,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "INSERT INTO pos_cash_movements\
                 (id,shift_id,amount_minor,reason,created_by,approved_by,created_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,NULL,$5) RETURNING id",
            )
            .bind(&movement.shift_id)
            .bind(movement.amount_minor)
            .bind(&movement.reason)
            .bind(actor.as_str())
            .bind(now.get())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn bump_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE pos_shifts SET version=version+1 \
                 WHERE id=$1 AND version=$2 AND state='open'",
            )
            .bind(shift_id)
            .bind(expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        company: &'a CompanyId,
        request_hash: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO pos_receipts(id,company_id,request_hash,result_json,created_at) \
                 VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(key)
            .bind(company.as_str())
            .bind(request_hash)
            .bind(result_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn audit<'a>(
        &'a mut self,
        actor: &'a CashMovementActor,
        movement: &'a ValidatedCashMovement,
        receipt: &'a CashMovementReceipt,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let details = serde_json::json!({
                "input": {
                    "id": movement.shift_id,
                    "expectedVersion": movement.expected_version,
                    "amountMinor": movement.amount_minor,
                    "reason": movement.reason,
                },
                "result": { "id": receipt.id },
                "assignmentId": actor.assignment_id.as_str(),
            })
            .to_string();
            sqlx::query(
                "INSERT INTO pos_audit\
                 (id,company_id,branch_id,actor_id,operation,record_id,details_json,created_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.user_id.as_str())
            .bind(OPERATION)
            .bind(&receipt.id)
            .bind(details)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn readback<'a>(
        &'a mut self,
        movement_id: &'a str,
        shift_id: &'a str,
    ) -> CashMovementFuture<'a, Option<CashMovementReadback>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT m.id,m.shift_id,m.amount_minor,m.reason,m.approved_by,m.created_at,\
                 s.version shift_version,s.state shift_state \
                 FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id \
                 WHERE m.id=$1 AND m.shift_id=$2",
            )
            .bind(movement_id)
            .bind(shift_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CashMovementReadback {
                    id: row.try_get("id")?,
                    shift_id: row.try_get("shift_id")?,
                    amount_minor: row.try_get("amount_minor")?,
                    reason: row.try_get("reason")?,
                    approved_by: row.try_get("approved_by")?,
                    created_at: row.try_get("created_at")?,
                    shift_version: row.try_get("shift_version")?,
                    shift_state: row.try_get("shift_state")?,
                })
            })
            .transpose()
        })
    }
}
