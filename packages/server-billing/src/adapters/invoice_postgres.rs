//! PostgreSQL adapter for the native Billing invoice lifecycle commands.
//!
//! SQL is Billing-owned Control SQL. The caller's serializable transaction is
//! required; no commit, Stripe call, customer creation, provider invoice or
//! Holding write occurs. The `billing_jobs` rows written here are the existing
//! durable provider handoff/outbox and are executed by the retained jobs
//! runtime.
use crate::application::BillingFuture;
use crate::application::cost::{AuditEntry, ReceiptSnapshot};
use crate::application::invoice::BillingInvoiceRepository;
use crate::domain::actor::BillingActor;
use crate::domain::invoice::{
    BillingAccountFacts, BillingCostFacts, BillingInvoiceRecord, BillingPeriodFacts,
    ValidatedInvoiceWrite,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("BILLING_INVOICE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("BILLING_INVOICE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

const INVOICE_COLUMNS: &str = "id,period_id,kind,status,total_satang,lines_json,payment_mode,\
     stripe_id,due_at,payment_state,original_invoice_id,created_at,updated_at";

fn invoice_record(row: &sqlx::postgres::PgRow) -> Result<BillingInvoiceRecord, sqlx::Error> {
    Ok(BillingInvoiceRecord {
        id: row.try_get("id")?,
        period_id: row.try_get("period_id")?,
        kind: row.try_get("kind")?,
        status: row.try_get("status")?,
        total_satang: row.try_get("total_satang")?,
        lines_json: row.try_get("lines_json")?,
        payment_mode: row.try_get("payment_mode")?,
        stripe_id: row.try_get("stripe_id")?,
        due_at: row.try_get("due_at")?,
        payment_state: row.try_get("payment_state")?,
        original_invoice_id: row.try_get("original_invoice_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub struct PgBillingInvoiceRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgBillingInvoiceRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl BillingInvoiceRepository for PgBillingInvoiceRepository<'_> {
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
    ) -> BillingFuture<'a, Option<BillingAccountFacts>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT version,payment_mode,consent_at,card_json FROM billing_accounts \
                 WHERE holding_id=$1 FOR UPDATE",
            )
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(BillingAccountFacts {
                    version: i64::from(row.try_get::<i32, _>("version")?),
                    payment_mode: row.try_get("payment_mode")?,
                    consent: row.try_get::<Option<i64>, _>("consent_at")?.is_some(),
                    card: row.try_get::<Option<String>, _>("card_json")?.is_some(),
                })
            })
            .transpose()
        })
    }

    fn period_for_update<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingPeriodFacts>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT p.id,p.start_at,p.end_at,p.plan_revision,p.support_satang,\
                 p.costs_complete,p.closed_at,COALESCE(pl.name,'') AS plan_name \
                 FROM billing_periods p \
                 LEFT JOIN billing_plans pl ON pl.id=p.plan_id AND pl.revision=p.plan_revision \
                 WHERE p.id=$1 AND p.holding_id=$2 FOR UPDATE OF p",
            )
            .bind(id)
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(BillingPeriodFacts {
                    id: row.try_get("id")?,
                    start_at: row.try_get("start_at")?,
                    end_at: row.try_get("end_at")?,
                    plan_name: row.try_get("plan_name")?,
                    plan_revision: i64::from(row.try_get::<i32, _>("plan_revision")?),
                    support_satang: row.try_get("support_satang")?,
                    costs_complete: row.try_get("costs_complete")?,
                    closed_at: row.try_get("closed_at")?,
                })
            })
            .transpose()
        })
    }

    fn invoice_for_update<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let sql = format!(
                "SELECT {INVOICE_COLUMNS} FROM billing_invoices \
                 WHERE id=$1 AND holding_id=$2 FOR UPDATE"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(holding.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| invoice_record(&row).map_err(Error::from))
                .transpose()
        })
    }

    fn current_main_invoice<'a>(
        &'a mut self,
        period_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let sql = format!(
                "SELECT {INVOICE_COLUMNS} FROM billing_invoices \
                 WHERE period_id=$1 AND holding_id=$2 AND kind='main' AND status<>'void' \
                 ORDER BY created_at DESC,id DESC LIMIT 1 FOR UPDATE"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(period_id)
                .bind(holding.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| invoice_record(&row).map_err(Error::from))
                .transpose()
        })
    }

    fn pending_invoice<'a>(
        &'a mut self,
        period_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT id FROM billing_invoices \
                 WHERE period_id=$1 AND holding_id=$2 AND status IN ('draft','queued') \
                 ORDER BY created_at,id LIMIT 1",
            )
            .bind(period_id)
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn unissued_costs<'a>(
        &'a mut self,
        period_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Vec<BillingCostFacts>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let rows = sqlx::query(
                "SELECT id,period_id,category,amount_satang,service_start,service_end,note,\
                 simulated,invoice_id FROM billing_costs \
                 WHERE period_id=$1 AND holding_id=$2 AND invoice_id IS NULL \
                 ORDER BY created_at,id LIMIT 251",
            )
            .bind(period_id)
            .bind(holding.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok::<_, Error>(BillingCostFacts {
                        id: row.try_get("id")?,
                        period_id: row.try_get("period_id")?,
                        category: row.try_get("category")?,
                        amount_satang: row.try_get("amount_satang")?,
                        service_start: row.try_get("service_start")?,
                        service_end: row.try_get("service_end")?,
                        note: row.try_get("note")?,
                        simulated: row.try_get("simulated")?,
                        invoice_id: row.try_get("invoice_id")?,
                    })
                })
                .collect()
        })
    }

    fn insert_invoice<'a>(
        &'a mut self,
        invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_invoices\
                 (id,holding_id,period_id,kind,status,total_satang,lines_json,payment_mode,\
                  created_at,approved_by,original_invoice_id,updated_at,due_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,(SELECT payment_mode FROM billing_accounts WHERE holding_id=$2),\
                  $8,$9,$10,$8,$11) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&invoice.id)
            .bind(holding.as_str())
            .bind(&invoice.period_id)
            .bind(&invoice.kind)
            .bind(&invoice.status)
            .bind(invoice.total_satang)
            .bind(&invoice.lines_json)
            .bind(now.get())
            .bind(actor.user_id.as_str())
            .bind(&invoice.original_invoice_id)
            .bind(invoice.due_at)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn update_invoice<'a>(
        &'a mut self,
        invoice_id: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "UPDATE billing_invoices SET total_satang=$3,lines_json=$4,updated_at=$5 \
                 WHERE id=$1 AND holding_id=$2 AND status='draft' AND stripe_id IS NULL",
            )
            .bind(invoice_id)
            .bind(holding.as_str())
            .bind(invoice.total_satang)
            .bind(&invoice.lines_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn finalize_invoice<'a>(
        &'a mut self,
        invoice_id: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "UPDATE billing_invoices SET status='queued',due_at=$3,updated_at=$4 \
                 WHERE id=$1 AND holding_id=$2 AND status='draft' AND stripe_id IS NULL",
            )
            .bind(invoice_id)
            .bind(holding.as_str())
            .bind(invoice.due_at)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn void_invoice<'a>(
        &'a mut self,
        invoice_id: &'a str,
        _invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "UPDATE billing_invoices SET status='void',updated_at=$3 \
                 WHERE id=$1 AND holding_id=$2 AND status IN ('draft','open','uncollectible') \
                 AND COALESCE(payment_state,'')<>'paid'",
            )
            .bind(invoice_id)
            .bind(holding.as_str())
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn attach_cost<'a>(
        &'a mut self,
        cost_id: &'a str,
        invoice_id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "UPDATE billing_costs SET invoice_id=$2 \
                 WHERE id=$1 AND holding_id=$3 AND period_id=$4 AND invoice_id IS NULL",
            )
            .bind(cost_id)
            .bind(invoice_id)
            .bind(holding.as_str())
            .bind(period_id)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn release_costs<'a>(
        &'a mut self,
        invoice_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, u64, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query(
                "UPDATE billing_costs SET invoice_id=NULL WHERE invoice_id=$1 AND holding_id=$2",
            )
            .bind(invoice_id)
            .bind(holding.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected())
        })
    }

    fn enqueue_job<'a>(
        &'a mut self,
        id: &'a str,
        invoice_id: &'a str,
        kind: &'a str,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) \
                 VALUES($1,$2,$3,$4,$5,$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(id)
            .bind(holding.as_str())
            .bind(invoice_id)
            .bind(kind)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn attached_cost_count<'a>(
        &'a mut self,
        invoice_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar(
                "SELECT count(*) FROM billing_costs WHERE invoice_id=$1 AND holding_id=$2",
            )
            .bind(invoice_id)
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn invoice_readback<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let sql = format!(
                "SELECT {INVOICE_COLUMNS} FROM billing_invoices WHERE id=$1 AND holding_id=$2"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id)
                .bind(holding.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| invoice_record(&row).map_err(Error::from))
                .transpose()
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

pub async fn invoice_command(
    connection: &mut PgConnection,
    actor: &BillingActor,
    target: &HoldingId,
    command: &crate::domain::invoice::BillingInvoiceCommand,
    now: TimestampMillis,
) -> Result<crate::domain::command::CommandReceipt, Error> {
    crate::application::invoice::execute_invoice_command(
        actor,
        target,
        command,
        &mut PgBillingInvoiceRepository::new(connection),
        now,
    )
    .await
}
