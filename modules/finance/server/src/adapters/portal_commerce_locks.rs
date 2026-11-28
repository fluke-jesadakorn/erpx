//! finance owner Portal source fence only; no authority or data read.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Connection, PgConnection};
/// Acquire in the enclosing global order before the first source snapshot.
pub async fn lock_portal_finance_sources(
    c: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PORTAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_parties,finance_charts,finance_accounts,finance_sales_defaults,finance_periods,finance_book_rules,finance_book_account_mappings,finance_dimension_definitions,finance_dimension_values,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE finance_invoices,finance_journals,finance_journal_lines,finance_book_postings,finance_payments,finance_allocations,finance_refund_obligations,money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE finance_supplier_instructions IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    Ok(())
}
