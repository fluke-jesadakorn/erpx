//! POS-owned half of day-report sources; no cross-owner tables are queried.
//!
//! Rows are fetched from the caller's serializable transaction in legacy sort
//! order. Raw row JSON is preserved because the previous report hashes the
//! insertion order of every property. Composition must bind PMS room-charge,
//! Commerce warehouse and Finance/Organization facts in this same transaction,
//! then demonstrate byte-for-byte JavaScript stringify parity before use.
use crate::domain::day_report::{PosDayOwnedSources, PosDaySaleSource};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn ensure_safe_row(row: &str) -> Result<(), Error> {
    fn safe(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(n) => n
                .as_i64()
                .is_some_and(|n| n.unsigned_abs() <= 9_007_199_254_740_991),
            serde_json::Value::Array(items) => items.iter().all(safe),
            serde_json::Value::Object(items) => items.values().all(safe),
            _ => true,
        }
    }
    let value: serde_json::Value =
        serde_json::from_str(row).map_err(|_| PersistenceError::new("POS_DAY_SOURCE_INVALID"))?;
    if !value.is_object() || !safe(&value) {
        return Err(PersistenceError::new("POS_DAY_SOURCE_INVALID").into());
    }
    Ok(())
}

pub struct PgPosDayReportSources<'c> {
    connection: &'c mut PgConnection,
}

/// Retains exactly what the POS-owned reader observed in one transaction.
/// This is not an IAM grant or a cross-owner/concurrent-writer fence.
pub struct RetainedPosDaySources {
    transaction: String,
    company: String,
    branch: String,
    date: String,
    sources: PosDayOwnedSources,
}
impl RetainedPosDaySources {
    pub fn sources(&self) -> &PosDayOwnedSources {
        &self.sources
    }
    pub async fn require_current(&self, connection: &mut PgConnection) -> Result<(), Error> {
        let mut reader = PgPosDayReportSources::new(connection);
        if reader.transaction_id().await? != self.transaction {
            return Err(PersistenceError::new("POS_DAY_SOURCE_UNIT_CHANGED").into());
        }
        let current = reader.load(&self.company, &self.branch, &self.date).await?;
        if current != self.sources {
            return Err(PersistenceError::new("POS_DAY_SOURCE_CHANGED").into());
        }
        Ok(())
    }
}
impl<'c> PgPosDayReportSources<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
    async fn transaction_id(&mut self) -> Result<String, Error> {
        if !self.connection.is_in_transaction()
            || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
                .fetch_one(&mut *self.connection)
                .await?
                != "serializable"
        {
            return Err(PersistenceError::new("POS_DAY_SERIALIZABLE_REQUIRED").into());
        }
        Ok(sqlx::query_scalar("SELECT txid_current()::text")
            .fetch_one(&mut *self.connection)
            .await?)
    }
    /// Call only after the composition's pre-snapshot authority and fences.
    pub async fn retain(
        &mut self,
        company: &str,
        branch: &str,
        date: &str,
    ) -> Result<RetainedPosDaySources, Error> {
        let transaction = self.transaction_id().await?;
        let sources = self.load(company, branch, date).await?;
        Ok(RetainedPosDaySources {
            transaction,
            company: company.to_owned(),
            branch: branch.to_owned(),
            date: date.to_owned(),
            sources,
        })
    }
    pub async fn load(
        &mut self,
        company: &str,
        branch: &str,
        date: &str,
    ) -> Result<PosDayOwnedSources, Error> {
        if !self.connection.is_in_transaction()
            || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
                .fetch_one(&mut *self.connection)
                .await?
                != "serializable"
        {
            return Err(PersistenceError::new("POS_DAY_SERIALIZABLE_REQUIRED").into());
        }
        let sales = sqlx::query("SELECT row_to_json(s)::text AS row_json,rr.reversal_entry_id FROM pos_sales s LEFT JOIN pos_room_charge_reversals rr ON rr.sale_id=s.id WHERE s.company_id=$1 AND s.branch_id=$2 AND s.business_date=$3 ORDER BY s.id")
            .bind(company).bind(branch).bind(date).fetch_all(&mut *self.connection).await?
            .into_iter().map(|row| Ok::<_,Error>(PosDaySaleSource { row_json: row.try_get("row_json")?, reversal_entry_id: row.try_get("reversal_entry_id")? })).collect::<Result<Vec<_>,_>>()?;
        // The runtime configures pg's JSONB parser to return text. A cast here
        // preserves that retained string value rather than embedding an object.
        let shifts = sqlx::query_scalar::<_,String>("SELECT row_to_json(s)::text FROM (SELECT id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state,counted_minor,expected_minor,variance_minor,closed_at,reviewed_by,review_reason,version,business_date,closed_by,denominations_json::text AS denominations_json FROM pos_shifts WHERE company_id=$1 AND branch_id=$2 AND business_date=$3 ORDER BY id) s").bind(company).bind(branch).bind(date).fetch_all(&mut *self.connection).await?;
        let payments = sqlx::query_scalar::<_,String>("SELECT row_to_json(q)::text FROM (SELECT p.*,s.number sale_number FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=$1 AND s.branch_id=$2 AND s.business_date=$3 ORDER BY p.id) q").bind(company).bind(branch).bind(date).fetch_all(&mut *self.connection).await?;
        let refunds = sqlx::query_scalar::<_,String>("SELECT row_to_json(q)::text FROM (SELECT r.*,s.number sale_number FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id WHERE r.company_id=$1 AND r.branch_id=$2 AND r.accounting_date=$3 ORDER BY r.id) q").bind(company).bind(branch).bind(date).fetch_all(&mut *self.connection).await?;
        let refund_payments = sqlx::query_scalar::<_,String>("SELECT row_to_json(q)::text FROM (SELECT rp.*,p.method FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.company_id=$1 AND r.branch_id=$2 AND r.accounting_date=$3 ORDER BY rp.id) q").bind(company).bind(branch).bind(date).fetch_all(&mut *self.connection).await?;
        let stock_move_candidates = sqlx::query_scalar::<_,String>("SELECT row_to_json(m)::text FROM pos_stock_moves m WHERE company_id=$1 AND movement_date=$2 ORDER BY id").bind(company).bind(date).fetch_all(&mut *self.connection).await?;
        let kitchen = sqlx::query_scalar::<_,String>("SELECT row_to_json(q)::text FROM (SELECT id,state FROM pos_fulfillments WHERE company_id=$1 AND branch_id=$2 AND business_date=$3 AND state IN ('queued','preparing','ready') ORDER BY id) q").bind(company).bind(branch).bind(date).fetch_all(&mut *self.connection).await?;
        let pending_cash = sqlx::query_scalar::<_,String>("SELECT row_to_json(q)::text FROM (SELECT m.id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=$1 AND s.branch_id=$2 AND s.business_date=$3 AND m.approved_by IS NULL ORDER BY m.id) q").bind(company).bind(branch).bind(date).fetch_all(&mut *self.connection).await?;
        let pending_payouts = sqlx::query_scalar::<_,String>("SELECT row_to_json(q)::text FROM (SELECT id,kind,state,amount_minor,fee_minor,amount_minor-fee_minor net_minor FROM pos_settlements WHERE company_id=$1 AND ((kind='payout' AND state<>'matched') OR (kind='balance' AND state='unmapped')) ORDER BY id) q").bind(company).fetch_all(&mut *self.connection).await?;
        let currency = sqlx::query_scalar::<_, String>(
            "SELECT currency FROM pos_settings WHERE company_id=$1",
        )
        .bind(company)
        .fetch_optional(&mut *self.connection)
        .await?;
        // The TS PostgreSQL adapter rejects unsafe integral values before the
        // report can be computed. Refuse rows we cannot represent exactly.
        for sale in &sales {
            ensure_safe_row(&sale.row_json)?;
        }
        for group in [
            &shifts,
            &payments,
            &refunds,
            &refund_payments,
            &stock_move_candidates,
            &kitchen,
            &pending_cash,
            &pending_payouts,
        ] {
            for row in group {
                ensure_safe_row(row)?;
            }
        }
        Ok(PosDayOwnedSources {
            sales,
            shifts,
            payments,
            refunds,
            refund_payments,
            stock_move_candidates,
            kitchen,
            pending_cash,
            pending_payouts,
            currency,
        })
    }
}
