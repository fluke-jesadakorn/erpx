//! PostgreSQL read adapter for native Procurement supplier promise history.
//!
//! The caller owns the retained serializable Holding transaction. Only
//! Procurement-owned promise tables and the Procurement-owned open-promise
//! view are read. No command, Commerce, Inventory, Finance, Records or IAM
//! mutation SQL is invented here; composition supplies the current actor and
//! any cross-owner purchase admission.
use crate::application::supplier_promise_history::{
    SupplierPromiseHistoryFuture, SupplierPromiseHistoryRepository,
};
use crate::domain::supplier_promise_history::{
    OpenPromiseRow, SupplierPromiseLineRow, SupplierPromiseRow,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_HISTORY_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("SUPPLIER_PROMISE_HISTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgSupplierPromiseHistoryRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgSupplierPromiseHistoryRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn history(
        &mut self,
        company: &CompanyId,
        order_id: &str,
    ) -> Result<Vec<SupplierPromiseRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT h.id,h.order_id,h.company_id,h.order_version,h.supplier_id,h.currency,\
             h.required_date,h.kind,h.confirmation_date,h.supplier_reference,h.evidence,\
             h.recorded_by,h.created_at \
             FROM procurement_delivery_promises h \
             WHERE h.company_id=$1 AND h.order_id=$2 \
             ORDER BY h.order_version DESC,h.id LIMIT 1001",
        )
        .bind(company.as_str())
        .bind(order_id)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(SupplierPromiseRow {
                    id: row.try_get("id")?,
                    order_id: row.try_get("order_id")?,
                    company_id: row.try_get("company_id")?,
                    order_version: row.try_get("order_version")?,
                    supplier_id: row.try_get("supplier_id")?,
                    currency: row.try_get("currency")?,
                    required_date: row.try_get("required_date")?,
                    kind: row.try_get("kind")?,
                    confirmation_date: row.try_get("confirmation_date")?,
                    supplier_reference: row.try_get("supplier_reference")?,
                    evidence: row.try_get("evidence")?,
                    recorded_by: row.try_get("recorded_by")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    pub async fn lines(
        &mut self,
        company: &CompanyId,
        order_id: &str,
    ) -> Result<Vec<SupplierPromiseLineRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT l.id,l.promise_id,l.order_line_id,l.ordinal::bigint AS ordinal,\
             l.quantity_milli,l.fulfilled_snapshot_milli,l.promised_date \
             FROM procurement_delivery_promise_lines l \
             JOIN procurement_delivery_promises h ON h.id=l.promise_id \
             WHERE h.company_id=$1 AND h.order_id=$2 \
             ORDER BY h.order_version DESC,l.promised_date,l.ordinal,l.id LIMIT 10001",
        )
        .bind(company.as_str())
        .bind(order_id)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(SupplierPromiseLineRow {
                    id: row.try_get("id")?,
                    promise_id: row.try_get("promise_id")?,
                    order_line_id: row.try_get("order_line_id")?,
                    ordinal: row.try_get("ordinal")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    fulfilled_snapshot_milli: row.try_get("fulfilled_snapshot_milli")?,
                    promised_date: row.try_get("promised_date")?,
                })
            })
            .collect()
    }

    pub async fn open(
        &mut self,
        company: &CompanyId,
        order_id: &str,
    ) -> Result<Vec<OpenPromiseRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT p.id,p.promise_id,h.order_id,p.company_id,p.branch_id,p.department_id,\
             p.number AS order_number,p.state AS order_state,p.order_line_id,\
             p.ordinal::bigint AS ordinal,p.quantity_milli,p.fulfilled_snapshot_milli,\
             p.promised_date,p.product_id,p.description,p.fulfilled_milli,\
             p.prior_quantity::bigint AS prior_quantity,p.remaining_milli \
             FROM procurement_open_promise_lines p \
             JOIN procurement_delivery_promises h ON h.id=p.promise_id \
             WHERE p.company_id=$1 AND h.order_id=$2 \
             ORDER BY p.promised_date,p.ordinal,p.id LIMIT 10001",
        )
        .bind(company.as_str())
        .bind(order_id)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(OpenPromiseRow {
                    id: row.try_get("id")?,
                    promise_id: row.try_get("promise_id")?,
                    order_id: row.try_get("order_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    order_number: row.try_get("order_number")?,
                    order_state: row.try_get("order_state")?,
                    order_line_id: row.try_get("order_line_id")?,
                    ordinal: row.try_get("ordinal")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    fulfilled_snapshot_milli: row.try_get("fulfilled_snapshot_milli")?,
                    promised_date: row.try_get("promised_date")?,
                    product_id: row.try_get("product_id")?,
                    description: row.try_get("description")?,
                    fulfilled_milli: row.try_get("fulfilled_milli")?,
                    prior_quantity: row.try_get("prior_quantity")?,
                    remaining_milli: row.try_get("remaining_milli")?,
                })
            })
            .collect()
    }
}

impl SupplierPromiseHistoryRepository for PgSupplierPromiseHistoryRead<'_> {
    type Error = Error;

    fn history<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseRow>, Self::Error> {
        Box::pin(
            async move { PgSupplierPromiseHistoryRead::history(self, company, order_id).await },
        )
    }

    fn lines<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseLineRow>, Self::Error> {
        Box::pin(async move { PgSupplierPromiseHistoryRead::lines(self, company, order_id).await })
    }

    fn open<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<OpenPromiseRow>, Self::Error> {
        Box::pin(async move { PgSupplierPromiseHistoryRead::open(self, company, order_id).await })
    }
}
