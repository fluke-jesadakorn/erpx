//! PostgreSQL owner adapter for the native PMS payment-status read.
//!
//! This is the only layer that executes SQL. The statement is the retained
//! `pms_rd_stripe_payment_by_id_company` projection with explicit bounded
//! columns (no `SELECT *`, no caller-supplied SQL or table name). The Company
//! predicate keeps the read inside the caller's scope; a foreign Company
//! yields `None`. No reconcile, provider call or write happens here.
use crate::application::payment_status::{
    PmsPaymentStatusFuture, PmsPaymentStatusRepository,
};
use crate::domain::payment_status::{PaymentStatusRow, read_invalid};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

/// Explicit subset of `pms_stripe_payments` needed by the retained `visible()`
/// projection. Byte-stable column order for row mapping.
pub const PAYMENT_STATUS_SQL: &str = "SELECT id,company_id,property_id,state,kind,amount_minor,payer_email,provider_id,qr_data,expires_at,entry_id,refund_state FROM pms_stripe_payments WHERE id=$1 AND company_id=$2";

fn row_from(row: &PgRow, invalid: &BusinessError) -> Result<PaymentStatusRow, Error> {
    Ok(PaymentStatusRow {
        id: row.try_get("id").map_err(|_| invalid.clone())?,
        company_id: row.try_get("company_id").map_err(|_| invalid.clone())?,
        property_id: row.try_get("property_id").map_err(|_| invalid.clone())?,
        state: row.try_get("state").map_err(|_| invalid.clone())?,
        kind: row.try_get("kind").map_err(|_| invalid.clone())?,
        amount_minor: row.try_get("amount_minor").map_err(|_| invalid.clone())?,
        payer_email: row.try_get("payer_email").map_err(|_| invalid.clone())?,
        provider_id: row.try_get("provider_id").map_err(|_| invalid.clone())?,
        qr_data: row.try_get("qr_data").map_err(|_| invalid.clone())?,
        expires_at: row.try_get("expires_at").map_err(|_| invalid.clone())?,
        entry_id: row.try_get("entry_id").map_err(|_| invalid.clone())?,
        refund_state: row.try_get("refund_state").map_err(|_| invalid.clone())?,
    })
}

async fn read(
    connection: &mut PgConnection,
    id: &str,
    company_id: &str,
) -> Result<Option<PaymentStatusRow>, Error> {
    let invalid = read_invalid();
    let row = sqlx::query(PAYMENT_STATUS_SQL)
        .bind(id)
        .bind(company_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| invalid.clone())?;
    row.as_ref().map(|r| row_from(r, &invalid)).transpose()
}

/// Read-only PMS payment-status repository bound to the caller's connection.
/// Composition wraps this in its serializable unit when a reconcile write is
/// also required; the read itself needs no lock.
pub struct PgPmsPaymentStatusRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPmsPaymentStatusRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl PmsPaymentStatusRepository for PgPmsPaymentStatusRead<'_> {
    type Error = Error;

    fn status<'a>(
        &'a mut self,
        facts: &'a crate::domain::payment_status::PmsPaymentStatusFacts,
    ) -> PmsPaymentStatusFuture<'a, Option<PaymentStatusRow>, Self::Error> {
        let id = facts.id.clone();
        let company = facts.company_id.clone();
        Box::pin(async move { read(self.connection, &id, &company).await })
    }
}
