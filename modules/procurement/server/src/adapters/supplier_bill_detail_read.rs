//! PostgreSQL execution for the native supplier-bill detail read.
//!
//! Every statement below is executed here with sqlx on the caller-owned
//! serializable Holding transaction. The projections are byte-for-byte the
//! retained Supply route's owner reads (match header, match lines, receipts,
//! events and expense accounts); no SQL is planned for TypeScript to run and
//! no TypeScript reader is consulted.

use crate::application::supplier_bill_detail::{
    SupplierBillDetailFuture, SupplierBillDetailRepository,
};
use crate::domain::supplier_bill_detail::{Row, SupplierBillDetailActor};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row as SqlRow, types::Json};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("SUPPLIER_BILL_DETAIL_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("SUPPLIER_BILL_DETAIL_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgSupplierBillDetailRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgSupplierBillDetailRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

fn json(row: &sqlx::postgres::PgRow, column: &str) -> Result<Value, Error> {
    Ok(row.try_get::<Json<Value>, _>(column)?.0)
}

impl SupplierBillDetailRepository for PgSupplierBillDetailRead<'_> {
    type Error = Error;

    fn bill<'a>(
        &'a mut self,
        actor: &'a SupplierBillDetailActor,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Option<Row>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT to_jsonb(m) || jsonb_build_object( \
                 'fulfillment_version',f.version,'invoice_status',i.status, \
                 'invoice_created_by',i.created_by,'invoice_version',i.version, \
                 'receipt_count',(SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=m.id), \
                 'supplier_name',p.name,'order_number',o.number) AS row \
                 FROM procurement_bill_matches m \
                 JOIN finance_parties p ON p.id=m.supplier_id AND p.company_id=m.company_id \
                 JOIN commerce_orders o ON o.id=m.order_id AND o.company_id=m.company_id \
                 JOIN commerce_fulfillments f ON f.id=m.fulfillment_id \
                 LEFT JOIN finance_invoices i ON i.id=m.invoice_id \
                 WHERE m.id=$1 AND m.company_id=$2 \
                 AND ($3::text IS NULL OR m.branch_id=$3) \
                 AND ($4::text IS NULL OR m.department_id=$4)",
            )
            .bind(bill_id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| json(&row, "row")).transpose()
        })
    }

    fn lines<'a>(
        &'a mut self,
        company: &'a str,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT COALESCE(jsonb_agg( \
                 to_jsonb(l) || jsonb_build_object( \
                 'description',ol.description,'fulfillment_id',fl.fulfillment_id, \
                 'receipt_number',f.number,'fulfillment_date',f.fulfillment_date, \
                 'order_number',o.number) ORDER BY f.fulfillment_date,f.id,ol.ordinal), \
                 '[]'::jsonb) AS rows \
                 FROM procurement_bill_match_lines l \
                 JOIN commerce_fulfillment_lines fl ON fl.id=l.fulfillment_line_id \
                 JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id \
                 JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=$1 \
                 JOIN commerce_order_lines ol ON ol.id=fl.order_line_id \
                 WHERE l.match_id=$2",
            )
            .bind(company)
            .bind(bill_id)
            .fetch_one(&mut *self.connection)
            .await?;
            json(&row, "rows").map(|value| value.as_array().cloned().unwrap_or_default())
        })
    }

    fn receipts<'a>(
        &'a mut self,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT COALESCE(jsonb_agg( \
                 to_jsonb(br) || jsonb_build_object( \
                 'receipt_number',f.number,'fulfillment_date',f.fulfillment_date, \
                 'current_version',f.version,'invoice_id',f.invoice_id, \
                 'order_id',o.id,'order_number',o.number) \
                 ORDER BY f.fulfillment_date,f.id),'[]'::jsonb) AS rows \
                 FROM procurement_bill_match_receipts br \
                 JOIN commerce_fulfillments f ON f.id=br.fulfillment_id \
                 JOIN commerce_orders o ON o.id=f.order_id \
                 WHERE br.match_id=$1",
            )
            .bind(bill_id)
            .fetch_one(&mut *self.connection)
            .await?;
            json(&row, "rows").map(|value| value.as_array().cloned().unwrap_or_default())
        })
    }

    fn events<'a>(
        &'a mut self,
        bill_id: &'a str,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY e.version DESC),'[]'::jsonb) AS rows \
                 FROM procurement_bill_match_events e WHERE e.match_id=$1",
            )
            .bind(bill_id)
            .fetch_one(&mut *self.connection)
            .await?;
            json(&row, "rows").map(|value| value.as_array().cloned().unwrap_or_default())
        })
    }

    fn accounts<'a>(
        &'a mut self,
        company: &'a str,
        chart_id: Option<&'a str>,
    ) -> SupplierBillDetailFuture<'a, Vec<Row>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT COALESCE(jsonb_agg(jsonb_build_object( \
                 'id',a.id,'code',a.code,'name',a.name) ORDER BY a.code),'[]'::jsonb) AS rows \
                 FROM finance_accounts a \
                 JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id \
                 WHERE a.company_id=$1 AND a.chart_id=$2 \
                 AND a.currency=c.base_currency AND a.category='expense' AND a.status='active'",
            )
            .bind(company)
            .bind(chart_id)
            .fetch_one(&mut *self.connection)
            .await?;
            json(&row, "rows").map(|value| value.as_array().cloned().unwrap_or_default())
        })
    }
}
