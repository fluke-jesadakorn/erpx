//! PostgreSQL read adapter for native Procurement supplier promise detail.
//!
//! The caller owns the retained serializable Holding transaction. The detail
//! read reuses the retained TypeScript reader's SQL: the scoped Commerce
//! purchase-order header and its lines (`commerce_orders`,
//! `commerce_order_lines`, `commerce_products`, `finance_parties`) plus the
//! Procurement-owned promise tables and open-promise view. Those cross-owner
//! statements are read-only and are the same source the retained adapter
//! queries; no Commerce, Inventory, Finance, Records or IAM row is mutated and
//! no purchase admission is inferred. Composition supplies the current actor,
//! the commercial package overlay and the Holding transaction.
use crate::application::supplier_promise_history::{
    SupplierPromiseHistoryFuture, SupplierPromiseHistoryRepository, supplier_promise_detail,
};
use crate::domain::supplier_promise_history::{
    OpenPromiseRow, SupplierPromiseDetail, SupplierPromiseLineRow, SupplierPromiseOrderRow,
    SupplierPromisePurchaseAccess, SupplierPromiseReadActor, SupplierPromiseRow,
    SupplierPromiseSourceLineRow,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
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

    /// Scoped purchase-order header. Branch/department are applied exactly as
    /// the retained reader's `scope` predicate: a workspace-restricted order is
    /// invisible, not silently widened.
    pub async fn order(
        &mut self,
        actor: &SupplierPromiseReadActor,
        order_id: &str,
    ) -> Result<Option<SupplierPromiseOrderRow>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.number,o.party_id,\
             o.warehouse_id,o.currency,o.order_date,o.due_date,o.note,o.state,\
             o.subtotal_minor,o.tax_minor,o.source_id,o.version,o.created_by,o.approved_by,\
             o.created_at,o.updated_at,o.price_list_id,o.price_list_version,o.discount_reason,\
             o.discount_approval_required,o.discount_approved_by,o.discount_approved_at,\
             p.name AS supplier_name \
             FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id \
             WHERE o.id=$1 AND o.company_id=$2 AND o.kind='purchase' \
             AND ($3::text IS NULL OR o.branch_id=$3) \
             AND ($4::text IS NULL OR o.department_id=$4)",
        )
        .bind(order_id)
        .bind(actor.company_id.as_str())
        .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
        .bind(actor.department_id.as_ref().map(|value| value.as_str()))
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(SupplierPromiseOrderRow {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                kind: row.try_get("kind")?,
                number: row.try_get("number")?,
                party_id: row.try_get("party_id")?,
                warehouse_id: row.try_get("warehouse_id")?,
                currency: row.try_get("currency")?,
                order_date: row.try_get("order_date")?,
                due_date: row.try_get("due_date")?,
                note: row.try_get("note")?,
                state: row.try_get("state")?,
                subtotal_minor: row.try_get("subtotal_minor")?,
                tax_minor: row.try_get("tax_minor")?,
                source_id: row.try_get("source_id")?,
                version: row.try_get("version")?,
                created_by: row.try_get("created_by")?,
                approved_by: row.try_get("approved_by")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                price_list_id: row.try_get("price_list_id")?,
                price_list_version: row.try_get("price_list_version")?,
                discount_reason: row.try_get("discount_reason")?,
                discount_approval_required: row.try_get("discount_approval_required")?,
                discount_approved_by: row.try_get("discount_approved_by")?,
                discount_approved_at: row.try_get("discount_approved_at")?,
                supplier_name: row.try_get("supplier_name")?,
            })
        })
        .transpose()
    }

    /// Commerce order lines joined to the product SKU. Outstanding/unconfirmed
    /// quantities are a Procurement presentation projection applied by the
    /// domain, not persisted or recomputed here.
    pub async fn source_lines(
        &mut self,
        company: &CompanyId,
        order_id: &str,
    ) -> Result<Vec<SupplierPromiseSourceLineRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT l.id,l.order_id,l.product_id,l.description,l.quantity_milli,\
             l.fulfilled_milli,l.price_minor,l.tax_bps,l.subtotal_minor,l.tax_minor,\
             l.ordinal,l.list_price_minor,l.discount_bps,l.item_type,p.sku \
             FROM commerce_order_lines l \
             JOIN commerce_orders o ON o.id=l.order_id AND o.company_id=$2 \
             JOIN commerce_products p ON p.id=l.product_id AND p.company_id=$2 \
             WHERE l.order_id=$1 \
             ORDER BY l.ordinal,l.id LIMIT 10001",
        )
        .bind(order_id)
        .bind(company.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(SupplierPromiseSourceLineRow {
                    id: row.try_get("id")?,
                    order_id: row.try_get("order_id")?,
                    product_id: row.try_get("product_id")?,
                    description: row.try_get("description")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    fulfilled_milli: row.try_get("fulfilled_milli")?,
                    price_minor: row.try_get("price_minor")?,
                    tax_bps: row.try_get("tax_bps")?,
                    subtotal_minor: row.try_get("subtotal_minor")?,
                    tax_minor: row.try_get("tax_minor")?,
                    ordinal: row.try_get("ordinal")?,
                    list_price_minor: row.try_get("list_price_minor")?,
                    discount_bps: row.try_get("discount_bps")?,
                    item_type: row.try_get("item_type")?,
                    sku: row.try_get("sku")?,
                })
            })
            .collect()
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

/// Runs the retained detail read on the caller-owned serializable Holding
/// transaction. Composition supplies the Account-derived actor, the commercial
/// package overlay it read in the same transaction and `now`; this helper never
/// opens a transaction and never derives purchase authority.
pub async fn supplier_promise_detail_in_transaction(
    connection: &mut PgConnection,
    actor: &SupplierPromiseReadActor,
    order_id: &str,
    purchase: Option<SupplierPromisePurchaseAccess>,
    now: TimestampMillis,
) -> Result<SupplierPromiseDetail, Error> {
    let mut repository = PgSupplierPromiseHistoryRead::new(connection);
    supplier_promise_detail(actor, &mut repository, order_id, purchase, now).await
}

impl SupplierPromiseHistoryRepository for PgSupplierPromiseHistoryRead<'_> {
    type Error = Error;

    fn order<'a>(
        &'a mut self,
        actor: &'a SupplierPromiseReadActor,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Option<SupplierPromiseOrderRow>, Self::Error> {
        Box::pin(async move {
            PgSupplierPromiseHistoryRead::order(self, actor, order_id).await
        })
    }

    fn source_lines<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseSourceLineRow>, Self::Error> {
        Box::pin(async move {
            PgSupplierPromiseHistoryRead::source_lines(self, company, order_id).await
        })
    }

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
