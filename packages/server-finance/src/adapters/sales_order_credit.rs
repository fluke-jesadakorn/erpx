//! Finance-owned sales-order credit-limit guard.
//!
//! This is the retained Commerce approval guard evaluated by the Finance owner
//! on the caller's serializable transaction. Commerce owns the order; the
//! exposure query reads Finance receivables and Commerce sales documents
//! through their public columns. No row is mutated here.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

/// Re-check the exact retained customer credit-limit assertion for one
/// prospective sales-order total. The party row is locked first, matching the
/// retained `prepareFinanceParties1` step, so a concurrent credit-limit or
/// receivable change participates in the serializable unit.
pub async fn sales_order_credit_allows(
    c: &mut PgConnection,
    company: &CompanyId,
    party_id: &str,
    order_total_minor: i64,
) -> Result<bool, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    if order_total_minor < 0 {
        return Ok(false);
    }
    let party = sqlx::query_scalar::<_, String>(
        "SELECT id FROM finance_parties WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(party_id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if party.is_none() {
        return Ok(false);
    }
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(\
         SELECT 1 FROM finance_parties p WHERE p.id=$1 AND p.company_id=$2 \
         AND p.kind='customer' AND (\
           p.credit_limit_minor=0 OR p.credit_limit_minor >= $3 + \
             COALESCE((SELECT SUM(CASE WHEN i.status IN ('draft','in_review') \
               THEN i.total_minor ELSE i.outstanding_minor END) \
               FROM finance_invoices i WHERE i.company_id=p.company_id \
               AND i.party_id=p.id AND i.kind='receivable' AND i.status!='voided'),0) + \
             COALESCE((SELECT SUM(o.subtotal_minor+o.tax_minor-COALESCE((\
               SELECT SUM(fl.subtotal_minor+fl.tax_minor) \
               FROM commerce_fulfillments f \
               JOIN commerce_fulfillment_lines fl ON fl.fulfillment_id=f.id \
               WHERE f.order_id=o.id AND f.invoice_id IS NOT NULL),0)) \
               FROM commerce_orders o WHERE o.company_id=p.company_id \
               AND o.party_id=p.id AND o.kind='sales' \
               AND o.state IN ('confirmed','partially_fulfilled','fulfilled')),0)\
         ))",
    )
    .bind(party_id)
    .bind(company.as_str())
    .bind(order_total_minor)
    .fetch_one(&mut *c)
    .await?)
}
