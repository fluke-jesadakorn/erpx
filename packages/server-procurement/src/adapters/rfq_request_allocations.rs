//! Procurement-owned allocation rows for Records requestQuotationSource.
//! This is a source read, not a visibility or command authorization grant.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RfqRequestAllocation {
    pub ordinal: Option<i32>,
    /// PostgreSQL sum(bigint) is numeric and may exceed i64; retain its exact
    /// decimal text until the composition applies JavaScript Number semantics.
    pub allocated_milli_decimal: String,
}

/// Execute only after Records detail, purchase/handler checks and service
/// scope validation, at the source's original allocation-read position.
pub async fn read_rfq_request_allocations(
    c: &mut PgConnection,
    request_id: &str,
) -> Result<Vec<RfqRequestAllocation>, PgTransactionError<BusinessError>> {
    let rows: Vec<(Option<i32>,String)> = sqlx::query_as("SELECT l.request_item_ordinal,COALESCE(sum(l.request_quantity_milli),0)::text FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=$1 AND r.status<>'cancelled' GROUP BY l.request_item_ordinal")
        .bind(request_id).fetch_all(c).await?;
    Ok(rows
        .into_iter()
        .map(|(ordinal, allocated_milli_decimal)| RfqRequestAllocation {
            ordinal,
            allocated_milli_decimal,
        })
        .collect())
}
