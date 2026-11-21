//! PostgreSQL read adapter for the native Billing workspace.
//!
//! Only Billing-owned Control tables are read: `billing_accounts`,
//! `billing_enrollments`, `billing_plans`, `billing_periods`, `billing_costs`,
//! `billing_invoices`, `billing_audit`, `billing_jobs` and the retained Billing
//! legacy `beta_billing`/`stripe_inbox` sources. The caller owns the serializable
//! Control transaction; this adapter never commits, writes or reads another
//! owner's Holding database.
use crate::application::BillingFuture;
use crate::application::workspace::{BillingWorkspaceRepository, WorkspaceRows};
use crate::domain::workspace::{
    AUDIT_LIMIT, BillingAccountView, BillingAuditView, BillingCostView, BillingEnrollmentView,
    BillingInvoiceView, BillingJobView, BillingLegacyView, BillingPeriodView, BillingPlanView,
    WORKSPACE_LIMIT,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, HoldingId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("BILLING_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("BILLING_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn account(row: sqlx::postgres::PgRow) -> Result<BillingAccountView, Error> {
    Ok(BillingAccountView {
        holding_id: row.try_get("holding_id")?,
        version: i64::from(row.try_get::<i32, _>("version")?),
        customer_id: row.try_get("customer_id")?,
        payment_mode: row.try_get("payment_mode")?,
        consent_at: row.try_get("consent_at")?,
        setup_state: row.try_get("setup_state")?,
        card_json: row.try_get("card_json")?,
    })
}

fn enrollment(row: sqlx::postgres::PgRow) -> Result<BillingEnrollmentView, Error> {
    Ok(BillingEnrollmentView {
        id: row.try_get("id")?,
        plan_id: row.try_get("plan_id")?,
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
        pending_plan: row.try_get("pending_plan")?,
        pending_end: row.try_get("pending_end")?,
        effective_at: row.try_get("effective_at")?,
    })
}

fn plan(row: sqlx::postgres::PgRow) -> Result<BillingPlanView, Error> {
    Ok(BillingPlanView {
        id: row.try_get("id")?,
        revision: i64::from(row.try_get::<i32, _>("revision")?),
        name: row.try_get("name")?,
        support_satang: row.try_get("support_satang")?,
        effective_month: row.try_get("effective_month")?,
    })
}

fn period(row: sqlx::postgres::PgRow) -> Result<BillingPeriodView, Error> {
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

fn cost(row: sqlx::postgres::PgRow) -> Result<BillingCostView, Error> {
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

fn invoice(row: sqlx::postgres::PgRow) -> Result<BillingInvoiceView, Error> {
    Ok(BillingInvoiceView {
        id: row.try_get("id")?,
        period_id: row.try_get("period_id")?,
        kind: row.try_get("kind")?,
        status: row.try_get("status")?,
        total_satang: row.try_get("total_satang")?,
        lines_json: row.try_get("lines_json")?,
        stripe_id: row.try_get("stripe_id")?,
        hosted_url: row.try_get("hosted_url")?,
        pdf_url: row.try_get("pdf_url")?,
        due_at: row.try_get("due_at")?,
        payment_state: row.try_get("payment_state")?,
        created_at: row.try_get("created_at")?,
        original_invoice_id: row.try_get("original_invoice_id")?,
    })
}

fn audit(row: sqlx::postgres::PgRow) -> Result<BillingAuditView, Error> {
    Ok(BillingAuditView {
        id: row.try_get("id")?,
        actor: row.try_get("actor")?,
        action: row.try_get("action")?,
        reason: row.try_get("reason")?,
        created_at: row.try_get("created_at")?,
    })
}

fn job(row: sqlx::postgres::PgRow) -> Result<BillingJobView, Error> {
    Ok(BillingJobView {
        id: row.try_get("id")?,
        state: row.try_get("state")?,
        last_error: row.try_get("last_error")?,
    })
}

pub struct PgBillingWorkspace<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgBillingWorkspace<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl BillingWorkspaceRepository for PgBillingWorkspace<'_> {
    type Error = Error;

    fn load<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, WorkspaceRows, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let account_row = sqlx::query(
                "SELECT holding_id,version,customer_id,payment_mode,consent_at,setup_state,card_json \
                 FROM billing_accounts WHERE holding_id=$1",
            )
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            let account = account_row.map(account).transpose()?;

            let enrollment = sqlx::query(
                "SELECT id,plan_id,started_at,ended_at,pending_plan,pending_end,effective_at \
                 FROM billing_enrollments WHERE holding_id=$1 \
                 ORDER BY started_at DESC,id DESC LIMIT 1",
            )
            .bind(holding.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .map(enrollment)
            .transpose()?;

            let plans = sqlx::query(
                "SELECT id,revision,name,support_satang,effective_month FROM billing_plans \
                 WHERE id IN ('infrastructure','standard','priority') \
                 ORDER BY CASE id WHEN 'infrastructure' THEN 0 WHEN 'standard' THEN 1 ELSE 2 END,\
                 revision DESC",
            )
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(plan)
            .collect::<Result<Vec<_>, _>>()?;

            let periods = sqlx::query(
                "SELECT id,month,start_at,end_at,plan_id,plan_revision,support_satang,\
                 costs_complete,closed_at FROM billing_periods WHERE holding_id=$1 \
                 ORDER BY month DESC,id DESC LIMIT $2",
            )
            .bind(holding.as_str())
            .bind((WORKSPACE_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(period)
            .collect::<Result<Vec<_>, _>>()?;

            let costs = sqlx::query(
                "SELECT id,period_id,category,amount_satang,service_start,service_end,note,\
                 simulated,evidence_id,invoice_id,original_invoice_id FROM billing_costs \
                 WHERE holding_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2",
            )
            .bind(holding.as_str())
            .bind((WORKSPACE_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(cost)
            .collect::<Result<Vec<_>, _>>()?;

            let invoices = sqlx::query(
                "SELECT id,period_id,kind,status,total_satang,lines_json,stripe_id,hosted_url,\
                 pdf_url,due_at,payment_state,created_at,original_invoice_id FROM billing_invoices \
                 WHERE holding_id=$1 ORDER BY created_at DESC,id DESC LIMIT $2",
            )
            .bind(holding.as_str())
            .bind((WORKSPACE_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(invoice)
            .collect::<Result<Vec<_>, _>>()?;

            let audit = sqlx::query(
                "SELECT id,actor,action,reason,created_at FROM billing_audit WHERE holding_id=$1 \
                 ORDER BY created_at DESC,id DESC LIMIT $2",
            )
            .bind(holding.as_str())
            .bind(AUDIT_LIMIT as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(audit)
            .collect::<Result<Vec<_>, _>>()?;

            let mut jobs = sqlx::query(
                "SELECT id,state,last_error FROM billing_jobs WHERE holding_id=$1 AND state<>'done' \
                 ORDER BY created_at,id LIMIT $2",
            )
            .bind(holding.as_str())
            .bind((WORKSPACE_LIMIT - 50) as i64)
            .fetch_all(&mut *self.connection)
            .await?
            .into_iter()
            .map(job)
            .collect::<Result<Vec<_>, _>>()?;
            jobs.extend(
                sqlx::query(
                    "SELECT i.id,'attention' AS state,i.last_error FROM stripe_inbox i \
                     JOIN billing_accounts a ON a.customer_id=i.payload_json::jsonb #>> '{data,object,customer}' \
                     WHERE a.holding_id=$1 AND i.state='pending' AND i.last_error IS NOT NULL \
                     ORDER BY i.created_at,i.id LIMIT 50",
                )
                .bind(holding.as_str())
                .fetch_all(&mut *self.connection)
                .await?
                .into_iter()
                .map(job)
                .collect::<Result<Vec<_>, _>>()?,
            );

            let legacy =
                sqlx::query("SELECT status,subscription_id FROM beta_billing WHERE holding_id=$1")
                    .bind(holding.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?
                    .map(|row| {
                        Ok::<_, Error>(BillingLegacyView {
                            status: row.try_get("status")?,
                            subscription_id: row.try_get("subscription_id")?,
                        })
                    })
                    .transpose()?;

            Ok(WorkspaceRows {
                account,
                enrollment,
                plans,
                periods,
                costs,
                invoices,
                audit,
                jobs,
                legacy,
            })
        })
    }
}
