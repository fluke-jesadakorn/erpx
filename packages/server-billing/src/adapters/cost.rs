//! PostgreSQL adapter for the native Billing cost-review commands.
//!
//! SQL is Billing-owned Control SQL: `billing_accounts`, `billing_periods`,
//! `billing_costs`, `billing_evidence`, `billing_invoices`, `billing_receipts`
//! and `billing_audit` plus the existing `holding_bindings` existence check used
//! by the retained command boundary. No Stripe request, invoice issue, job
//! dispatch or cross-owner Holding write occurs here. The caller's serializable
//! Control transaction is required and no commit is performed.
use crate::application::BillingFuture;
use crate::application::cost::{
    AccountSnapshot, AuditEntry, BillingCostCommandRepository, ReceiptSnapshot,
};
use crate::domain::command::ValidatedCostDraft;
use crate::domain::workspace::{BillingCostView, BillingPeriodView};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn cost_row(row: sqlx::postgres::PgRow) -> Result<BillingCostView, Error> {
    Ok(BillingCostView {
        id: row.try_get("id")?,
        period_id: row.try_get("period_id")?,
        category: row.try_get("category")?,
        amount_satang: row.try_get("amount_satang")?,
        service_start: row.try_get("service_start")?,
        service_end: row.try_get("service_end")?,
        note: row.try_get("note")?,
        simulated: row.try_get("simulated")?,
        evidence_id: row.try_get("evidence_id")?,
        invoice_id: row.try_get("invoice_id")?,
        original_invoice_id: row.try_get("original_invoice_id")?,
    })
}

fn period_row(row: sqlx::postgres::PgRow) -> Result<BillingPeriodView, Error> {
    Ok(BillingPeriodView {
        id: row.try_get("id")?,
        month: row.try_get("month")?,
        start_at: row.try_get("start_at")?,
        end_at: row.try_get("end_at")?,
        plan_id: row.try_get("plan_id")?,
        plan_revision: i64::from(row.try_get::<i32, _>("plan_revision")?),
        support_satang: row.try_get("support_satang")?,
        costs_complete: row.try_get("costs_complete")?,
        closed_at: row.try_get("closed_at")?,
    })
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("BILLING_COMMAND_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("BILLING_COMMAND_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgBillingCostCommand<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgBillingCostCommand<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl BillingCostCommandRepository for PgBillingCostCommand<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> BillingFuture<'a, String, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn lock_holding<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("billing:{}", holding.as_str()))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("billing-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query("SELECT digest,result_json FROM billing_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(ReceiptSnapshot {
                    digest: row.try_get("digest")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }

    fn holding_exists<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM holding_bindings WHERE holding_id=$1)",
            )
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn account_for_update<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<AccountSnapshot>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row =
                sqlx::query("SELECT version FROM billing_accounts WHERE holding_id=$1 FOR UPDATE")
                    .bind(holding.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?;
            row.map(|row| {
                Ok::<_, Error>(AccountSnapshot {
                    version: i64::from(row.try_get::<i32, _>("version")?),
                })
            })
            .transpose()
        })
    }

    fn period<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingPeriodView>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT id,month,start_at,end_at,plan_id,plan_revision,support_satang,\
                 costs_complete,closed_at FROM billing_periods WHERE id=$1 AND holding_id=$2",
            )
            .bind(id)
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(period_row).transpose()
        })
    }

    fn available_evidence<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_evidence \
                 WHERE id=$1 AND holding_id=$2 AND source_state='available')",
            )
            .bind(id)
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn original_invoice<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_invoices \
                 WHERE id=$1 AND holding_id=$2 AND period_id=$3)",
            )
            .bind(id)
            .bind(holding.as_str())
            .bind(period_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn existing_cost<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, Option<BillingCostView>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT id,period_id,category,amount_satang,service_start,service_end,note,\
                 simulated,evidence_id,invoice_id,original_invoice_id FROM billing_costs \
                 WHERE id=$1 AND holding_id=$2 AND period_id=$3 FOR UPDATE",
            )
            .bind(id)
            .bind(holding.as_str())
            .bind(period_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(cost_row).transpose()
        })
    }

    fn insert_cost<'a>(
        &'a mut self,
        cost: &'a ValidatedCostDraft,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_costs\
                 (id,holding_id,period_id,category,amount_satang,service_start,service_end,note,\
                  simulated,evidence_id,original_invoice_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) \
                 ON CONFLICT(id) DO UPDATE SET category=EXCLUDED.category,\
                 amount_satang=EXCLUDED.amount_satang,service_start=EXCLUDED.service_start,\
                 service_end=EXCLUDED.service_end,note=EXCLUDED.note,\
                 simulated=EXCLUDED.simulated,evidence_id=EXCLUDED.evidence_id,\
                 original_invoice_id=EXCLUDED.original_invoice_id \
                 WHERE billing_costs.holding_id=$2 AND billing_costs.period_id=$3 \
                 AND billing_costs.invoice_id IS NULL",
            )
            .bind(&cost.id)
            .bind(holding.as_str())
            .bind(&cost.period_id)
            .bind(&cost.category)
            .bind(cost.amount_satang)
            .bind(cost.service_start)
            .bind(cost.service_end)
            .bind(&cost.note)
            .bind(cost.simulated)
            .bind(&cost.evidence_id)
            .bind(&cost.original_invoice_id)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn cost_readback<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, Option<BillingCostView>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT id,period_id,category,amount_satang,service_start,service_end,note,\
                 simulated,evidence_id,invoice_id,original_invoice_id FROM billing_costs \
                 WHERE id=$1 AND holding_id=$2 AND period_id=$3",
            )
            .bind(id)
            .bind(holding.as_str())
            .bind(period_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(cost_row).transpose()
        })
    }

    fn delete_unissued_cost<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "DELETE FROM billing_costs WHERE id=$1 AND holding_id=$2 AND invoice_id IS NULL \
                 RETURNING period_id",
            )
            .bind(id)
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| row.get::<String, _>("period_id")))
        })
    }

    fn cost_exists<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_costs WHERE id=$1 AND holding_id=$2)",
            )
            .bind(id)
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn mark_costs_incomplete<'a>(
        &'a mut self,
        period_id: &'a str,
    ) -> BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1")
                .bind(period_id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn set_costs_complete<'a>(
        &'a mut self,
        period_id: &'a str,
        complete: bool,
    ) -> BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("UPDATE billing_periods SET costs_complete=$2 WHERE id=$1")
                .bind(period_id)
                .bind(complete)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn increment_account_version<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        expected_version: i64,
    ) -> BillingFuture<'a, Option<i64>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "UPDATE billing_accounts SET version=version+1 \
                 WHERE holding_id=$1 AND version=$2 RETURNING version",
            )
            .bind(holding.as_str())
            .bind(expected_version)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| i64::from(row.get::<i32, _>("version"))))
        })
    }

    fn insert_audit<'a>(
        &'a mut self,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_audit(id,holding_id,actor,action,reason,details_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(format!("billing-audit-{}", entry.key))
            .bind(entry.target.as_str())
            .bind(entry.actor.user_id.as_str())
            .bind(entry.action)
            .bind(entry.reason)
            .bind(entry.details_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        digest: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_receipts(id,digest,result_json,created_at) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(id) DO NOTHING",
            )
            .bind(key)
            .bind(digest)
            .bind(result_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn serialized_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query("SELECT digest,result_json FROM billing_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(ReceiptSnapshot {
                    digest: row.try_get("digest")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }
}
