//! Called only after the finite native Finance effect and its retained before /
//! after validation. The marker grants no IAM, purchase or source authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
pub(crate) async fn mark(
    c: &mut PgConnection,
    company: &str,
    payment: &str,
    bank_rows: &[String],
) -> Result<(), PgTransactionError<BusinessError>> {
    let result = sqlx::query(include_str!("native_service_receipt_marker.sql"))
        .bind(payment)
        .bind(company)
        .bind(bank_rows)
        .execute(c)
        .await?;
    if result.rows_affected() != 1 {
        return Err(PersistenceError::new("FINANCE_SERVICE_RECEIPT_RUNTIME_MARKER_MISSING").into());
    }
    Ok(())
}

/// The existing posting marker intentionally covers only its immediate UPDATE.
/// Retain the owner-verified final invoice image separately for deferred fences.
pub(crate) async fn mark_invoice(
    c: &mut PgConnection,
    company: &str,
    invoice: &str,
) -> Result<(), PgTransactionError<BusinessError>> {
    let result=sqlx::query("WITH previous AS (SELECT erpx_service_receipt_runtime_marker() marker), invoice AS (SELECT to_jsonb(i) image FROM finance_invoices i WHERE i.id=$1 AND i.company_id=$2) SELECT set_config('erpx.service_receipt_runtime',(previous.marker||jsonb_build_object('database',current_database(),'pid',pg_backend_pid()::text,'xid',pg_current_xact_id()::text,'started',pg_postmaster_start_time()::text,'invoices',COALESCE(previous.marker->'invoices','{}'::jsonb)||jsonb_build_object($1::text,invoice.image)))::text,true) FROM previous,invoice")
        .bind(invoice).bind(company).execute(c).await?;
    if result.rows_affected() != 1 {
        return Err(PersistenceError::new("FINANCE_SERVICE_INVOICE_RUNTIME_MARKER_MISSING").into());
    }
    Ok(())
}
