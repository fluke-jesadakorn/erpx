//! PostgreSQL execution for the native R3 supplier-bill workspace read.
//!
//! This is the only layer that executes SQL for this operation. The statement
//! below is the retained Supply workspace read rendered verbatim: the same
//! Procurement bill-match rows, the same Finance party and Commerce order
//! projections, the same purchase-receipt binding witness, the same scope binds
//! and the same `updated_at DESC` order. No IAM authority, commercial package
//! admission or command write happens here; composition supplies the actor and
//! the caller-owned Holding unit.
use crate::application::native_r3_supplier_bill_workspace::{
    WorkspaceFuture, WorkspaceRepository,
};
use crate::domain::native_r3_supplier_bill_workspace::Row;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{types::Json, PgConnection, Row as SqlRow};

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPLIER_BILL_WORKSPACE_UNAVAILABLE",
        "The Procurement supplier-bill workspace is unavailable.",
    )
    .into()
}

/// Exact retained statement. `$1` company, `$2` branch (nullable), `$3`
/// department (nullable) reproduce the TS `company, branch, branch, department,
/// department` bind order with each scope bind reused in place. The outer
/// `jsonb_agg` only serializes the identical projected rows into one value so
/// the row mapping cannot drift column by column.
const WORKSPACE_SQL: &str = r#"
SELECT COALESCE(jsonb_agg(to_jsonb(q) ORDER BY q.updated_at DESC),'[]'::jsonb) AS rows
FROM (
  SELECT m.*,p.name supplier_name,o.number order_number,
    COALESCE((SELECT json_agg(br.fulfillment_id ORDER BY f.fulfillment_date,f.id)::text
      FROM procurement_bill_match_receipts br
      JOIN (SELECT "id","order_id","number","fulfillment_date","created_by","created_at","journal_id","invoice_id","version","evidence_note" FROM commerce_fulfillments) f ON f.id=br.fulfillment_id
      WHERE br.match_id=m.id),'[]') receipt_ids_json,
    (SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=m.id) receipt_count
  FROM procurement_bill_matches m
  JOIN (SELECT "id","company_id","kind","code","name","currency","tax_registration","payment_terms_days","credit_limit_minor","status","version","created_at","updated_at","person_id","notification_email","notification_consent","merged_into_id","merged_at","merged_by","phone","address_json" FROM finance_parties) p ON p.id=m.supplier_id
  JOIN (SELECT "id","company_id","branch_id","department_id","kind","number","party_id","warehouse_id","currency","order_date","due_date","note","state","subtotal_minor","tax_minor","source_id","version","created_by","approved_by","created_at","updated_at","price_list_id","price_list_version","discount_reason","discount_approval_required","discount_approved_by","discount_approved_at" FROM commerce_orders) o ON o.id=m.order_id
  WHERE m.company_id=$1 AND ($2::text IS NULL OR m.branch_id=$2) AND ($3::text IS NULL OR m.department_id=$3)
    AND EXISTS(SELECT 1 FROM commerce_fulfillments primary_receipt JOIN commerce_orders primary_order ON primary_order.id=primary_receipt.order_id WHERE primary_receipt.id=m.fulfillment_id AND primary_receipt.order_id=m.order_id AND primary_order.company_id=m.company_id AND primary_order.kind='purchase' AND primary_order.party_id=m.supplier_id AND primary_order.currency=m.currency)
    AND EXISTS(SELECT 1 FROM procurement_bill_match_receipts br WHERE br.match_id=m.id AND br.fulfillment_id=m.fulfillment_id)
    AND NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br LEFT JOIN commerce_fulfillments f ON f.id=br.fulfillment_id LEFT JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=m.id AND (o.id IS NULL OR o.kind<>'purchase' OR o.company_id<>m.company_id OR o.party_id<>m.supplier_id OR o.currency<>m.currency))
) q
"#;

pub struct PgSupplierBillWorkspaceRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgSupplierBillWorkspaceRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn rows(
        &mut self,
        company: &CompanyId,
        branch: Option<&str>,
        department: Option<&str>,
    ) -> Result<Vec<Row>, Error> {
        if company.as_str().is_empty() {
            return Err(unavailable());
        }
        let row = sqlx::query(WORKSPACE_SQL)
            .bind(company.as_str())
            .bind(branch)
            .bind(department)
            .fetch_one(&mut *self.connection)
            .await?;
        let rows: Json<Vec<Row>> = row.try_get("rows")?;
        Ok(rows.0)
    }
}

impl WorkspaceRepository for PgSupplierBillWorkspaceRead<'_> {
    type Error = Error;

    fn rows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a str>,
        department: Option<&'a str>,
    ) -> WorkspaceFuture<'a, Vec<Row>, Self::Error> {
        Box::pin(async move { PgSupplierBillWorkspaceRead::rows(self, company, branch, department).await })
    }
}

/// Runs the retained workspace read on the caller-owned Holding connection.
/// Composition supplies the Account-derived actor and enforces the commercial
/// purchased-package admission; this helper never opens a transaction and never
/// grants authority.
pub async fn supplier_bill_workspace_in_unit(
    connection: &mut PgConnection,
    actor: &crate::domain::native_r3_supplier_bill_workspace::WorkspaceActor,
    now: erpx_platform_shared_server::public::TimestampMillis,
) -> Result<Vec<Row>, Error> {
    let mut repository = PgSupplierBillWorkspaceRead::new(connection);
    crate::application::native_r3_supplier_bill_workspace::supplier_bill_workspace(
        actor, &mut repository, now,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_matches_the_retained_read_shape() {
        assert!(WORKSPACE_SQL.contains("FROM procurement_bill_matches m"));
        assert!(WORKSPACE_SQL.contains("receipt_ids_json"));
        assert!(WORKSPACE_SQL.contains("receipt_count"));
        assert!(WORKSPACE_SQL.contains("supplier_name"));
        assert!(WORKSPACE_SQL.contains("order_number"));
        assert!(WORKSPACE_SQL.contains("ORDER BY q.updated_at DESC"));
        assert!(WORKSPACE_SQL.contains("primary_order.kind='purchase'"));
    }
}
