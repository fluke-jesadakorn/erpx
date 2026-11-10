//! Finance-owned supplier-bill match source and lifecycle port.
//! Procurement supplies the exact reviewed match facts; Finance consumes them
//! in its own serializable unit and never writes Procurement tables directly.
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcurementSupplierBillMatch {
    pub match_id: String,
    pub match_version: i64,
    pub status: String,
    pub invoice_id: Option<String>,
    pub currency: String,
    pub chart_id: String,
    pub supplier_id: String,
    pub invoice_date: String,
    pub due_date: String,
    pub expected_subtotal_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub clearing_account_id: String,
    pub payable_account_id: String,
    pub tax_account_id: String,
    pub variance_account_id: Option<String>,
    pub quantity_matches: bool,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROCUREMENT_SUPPLIER_BILL_MATCH_UNAVAILABLE",
        "The Procurement supplier-bill match source is unavailable.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PROCUREMENT_SUPPLIER_BILL_MATCH_CHANGED",
        "The Procurement supplier-bill match changed. Refresh and review again.",
    )
}

/// Fence the Procurement match lifecycle before the first Finance read.
/// The caller's serializable unit retains these locks through its own commit.
pub async fn lock_procurement_bill_match_sources(
    c: &mut PgConnection,
) -> Result<(), BusinessError> {
    sqlx::query(
        "LOCK TABLE procurement_bill_matches,procurement_bill_match_receipts,\
         procurement_bill_match_events IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(())
}

pub async fn load_procurement_supplier_bill_match(
    c: &mut PgConnection,
    company_id: &str,
    invoice_id: &str,
) -> Result<Option<ProcurementSupplierBillMatch>, BusinessError> {
    if company_id.is_empty() || invoice_id.is_empty() || invoice_id.len() > 200 {
        return Err(unavailable());
    }
    let row = sqlx::query(
        "SELECT id,version,status,invoice_id,currency,chart_id,supplier_id,invoice_date,\
         due_date,expected_subtotal_minor,subtotal_minor,tax_minor,clearing_account_id,\
         payable_account_id,tax_account_id,variance_account_id,quantity_matches \
         FROM procurement_bill_matches \
         WHERE company_id=$1 AND invoice_id=$2 FOR UPDATE",
    )
    .bind(company_id)
    .bind(invoice_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    row.map(|row| {
        Ok(ProcurementSupplierBillMatch {
            match_id: row.try_get("id").map_err(|_| unavailable())?,
            match_version: row.try_get("version").map_err(|_| unavailable())?,
            status: row.try_get("status").map_err(|_| unavailable())?,
            invoice_id: row.try_get("invoice_id").map_err(|_| unavailable())?,
            currency: row.try_get("currency").map_err(|_| unavailable())?,
            chart_id: row.try_get("chart_id").map_err(|_| unavailable())?,
            supplier_id: row.try_get("supplier_id").map_err(|_| unavailable())?,
            invoice_date: row.try_get("invoice_date").map_err(|_| unavailable())?,
            due_date: row.try_get("due_date").map_err(|_| unavailable())?,
            expected_subtotal_minor: row
                .try_get("expected_subtotal_minor")
                .map_err(|_| unavailable())?,
            subtotal_minor: row.try_get("subtotal_minor").map_err(|_| unavailable())?,
            tax_minor: row.try_get("tax_minor").map_err(|_| unavailable())?,
            clearing_account_id: row
                .try_get("clearing_account_id")
                .map_err(|_| unavailable())?,
            payable_account_id: row
                .try_get("payable_account_id")
                .map_err(|_| unavailable())?,
            tax_account_id: row.try_get("tax_account_id").map_err(|_| unavailable())?,
            variance_account_id: row
                .try_get("variance_account_id")
                .map_err(|_| unavailable())?,
            quantity_matches: row.try_get("quantity_matches").map_err(|_| unavailable())?,
        })
    })
    .transpose()
}

/// Advance an invoiced match to posted after Finance posts its payable.
pub async fn stage_procurement_supplier_bill_match_post(
    c: &mut PgConnection,
    company_id: &str,
    match_id: &str,
    expected_version: i64,
    invoice_id: &str,
    actor_id: &str,
    command_id: &str,
    now: i64,
) -> Result<(), BusinessError> {
    let next = expected_version + 1;
    let updated = sqlx::query(
        "UPDATE procurement_bill_matches SET status='posted',version=version+1,updated_at=$1 \
         WHERE id=$2 AND company_id=$3 AND version=$4 AND status='invoiced' AND invoice_id=$5",
    )
    .bind(now)
    .bind(match_id)
    .bind(company_id)
    .bind(expected_version)
    .bind(invoice_id)
    .execute(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    sqlx::query(
        "INSERT INTO procurement_bill_match_events(id,match_id,version,action,actor_id,reason,created_at) \
         VALUES($1,$2,$3,'posted',$4,$5,$6)",
    )
    .bind(format!("bill-event-{command_id}"))
    .bind(match_id)
    .bind(next)
    .bind(actor_id)
    .bind("Matched invoice and price variance posted to GL")
    .bind(now)
    .execute(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    Ok(())
}

/// Return a matched bill to Procurement after Finance voids its payable.
/// Receipt links are deactivated so the receipt can be matched again.
pub async fn stage_procurement_supplier_bill_match_return(
    c: &mut PgConnection,
    company_id: &str,
    match_id: &str,
    expected_version: i64,
    invoice_id: &str,
    actor_id: &str,
    command_id: &str,
    now: i64,
    reason: &str,
) -> Result<(), BusinessError> {
    let next = expected_version + 1;
    let updated = sqlx::query(
        "UPDATE procurement_bill_matches SET status='returned',invoice_id=NULL,\
         version=version+1,updated_at=$1 \
         WHERE id=$2 AND company_id=$3 AND version=$4 AND invoice_id=$5 \
         AND status IN ('invoiced','posted')",
    )
    .bind(now)
    .bind(match_id)
    .bind(company_id)
    .bind(expected_version)
    .bind(invoice_id)
    .execute(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    if updated.rows_affected() != 1 {
        return Err(changed());
    }
    sqlx::query(
        "UPDATE procurement_bill_match_receipts SET active=FALSE \
         WHERE match_id=$1 AND active=TRUE",
    )
    .bind(match_id)
    .execute(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    sqlx::query(
        "INSERT INTO procurement_bill_match_events(id,match_id,version,action,actor_id,reason,created_at) \
         VALUES($1,$2,$3,'returned',$4,$5,$6)",
    )
    .bind(format!("bill-event-{command_id}"))
    .bind(match_id)
    .bind(next)
    .bind(actor_id)
    .bind(reason)
    .bind(now)
    .execute(&mut *c)
    .await
    .map_err(|_| unavailable())?;
    Ok(())
}
