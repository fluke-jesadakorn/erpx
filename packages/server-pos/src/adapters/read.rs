//! PostgreSQL adapter for the native POS workspace/day read.
//!
//! All owner SQL is POS-owned (`pos_settings`, `pos_registers`, `pos_shifts`,
//! `pos_cash_movements`, `pos_day_closes`, `pos_sales`, `pos_lines`,
//! `pos_fulfillments`, `pos_payments`). The only cross-owner read is the
//! bounded `commerce_warehouses.name` label the retained read already joined
//! for the register row; no Commerce, Finance, Inventory, Organization or PMS
//! business row is written. The caller owns the serializable Holding
//! transaction and this adapter never commits.
use crate::application::read::{PosReadFuture, PosWorkspaceRepository};
use crate::domain::read::{
    MAX_CASH_MOVEMENTS, MAX_LINES, MAX_PAYMENTS, MAX_REGISTERS, MAX_SHIFTS, PosCashMovementRow,
    PosCashSummary, PosDayCloseRow, PosLineRow, PosPaymentRow, PosReadActor, PosRegisterRow,
    PosSaleRow, PosSettingsRow, PosShiftRow, PosWorkspaceQuery, PosWorkspaceSource,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("POS_WORKSPACE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("POS_WORKSPACE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgPosWorkspaceRead<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPosWorkspaceRead<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn registers(&mut self, actor: &PosReadActor) -> Result<Vec<PosRegisterRow>, Error> {
        let rows = sqlx::query(
            "SELECT r.id,r.company_id,r.branch_id,r.warehouse_id,r.name,r.mode,r.active,\
             r.version,w.name warehouse_name \
             FROM pos_registers r JOIN commerce_warehouses w ON w.id=r.warehouse_id \
             WHERE r.company_id=$1 AND ($2::text IS NULL OR r.branch_id=$2) \
             ORDER BY r.name,r.id LIMIT $3",
        )
        .bind(actor.company_id().as_str())
        .bind(actor.branch_id().map(|value| value.as_str()))
        .bind(MAX_REGISTERS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok::<_, Error>(PosRegisterRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    warehouse_id: row.try_get("warehouse_id")?,
                    name: row.try_get("name")?,
                    mode: row.try_get("mode")?,
                    active: row.try_get("active")?,
                    version: row.try_get("version")?,
                    warehouse_name: row.try_get("warehouse_name")?,
                })
            })
            .collect()
    }

    async fn settings(&mut self, actor: &PosReadActor) -> Result<Option<PosSettingsRow>, Error> {
        let row = sqlx::query(
            "SELECT enabled,currency,timezone,cashier_discount_bps,version \
             FROM pos_settings WHERE company_id=$1",
        )
        .bind(actor.company_id().as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok::<_, Error>(PosSettingsRow {
                enabled: row.try_get("enabled")?,
                currency: row.try_get("currency")?,
                timezone: row.try_get("timezone")?,
                cashier_discount_bps: row.try_get("cashier_discount_bps")?,
                version: row.try_get("version")?,
            })
        })
        .transpose()
    }

    async fn shifts(&mut self, actor: &PosReadActor) -> Result<Vec<PosShiftRow>, Error> {
        let rows = sqlx::query(
            "SELECT s.id,s.register_id,r.name register_name,s.company_id,s.branch_id,\
             s.business_date,s.opened_by,s.opened_at,s.opening_minor,s.state,s.counted_minor,\
             s.expected_minor,s.variance_minor,s.closed_by,s.closed_at,s.version,\
             (s.opening_minor \
             +COALESCE((SELECT SUM(p.amount_minor) FROM pos_payments p \
               JOIN pos_sales b ON b.id=p.sale_id \
               WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded'),0) \
             -COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp \
               JOIN pos_refunds rf ON rf.id=rp.refund_id \
               JOIN pos_payments p ON p.id=rp.payment_id \
               WHERE rf.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded'),0) \
             +COALESCE((SELECT SUM(m.amount_minor) FROM pos_cash_movements m \
               WHERE m.shift_id=s.id AND m.approved_by IS NOT NULL),0))::bigint cash_expected_minor \
             FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id \
             WHERE s.company_id=$1 AND ($2::text IS NULL OR s.branch_id=$2) \
             ORDER BY s.opened_at DESC,s.id LIMIT $3",
        )
        .bind(actor.company_id().as_str())
        .bind(actor.branch_id().map(|value| value.as_str()))
        .bind(MAX_SHIFTS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok::<_, Error>(PosShiftRow {
                    id: row.try_get("id")?,
                    register_id: row.try_get("register_id")?,
                    register_name: row.try_get("register_name")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    business_date: row.try_get("business_date")?,
                    opened_by: row.try_get("opened_by")?,
                    opened_at: row.try_get("opened_at")?,
                    opening_minor: row.try_get("opening_minor")?,
                    state: row.try_get("state")?,
                    counted_minor: row.try_get("counted_minor")?,
                    expected_minor: row.try_get("expected_minor")?,
                    variance_minor: row.try_get("variance_minor")?,
                    closed_by: row.try_get("closed_by")?,
                    closed_at: row.try_get("closed_at")?,
                    version: row.try_get("version")?,
                    cash_expected_minor: row.try_get("cash_expected_minor")?,
                })
            })
            .collect()
    }

    async fn cash_movements(
        &mut self,
        actor: &PosReadActor,
    ) -> Result<Vec<PosCashMovementRow>, Error> {
        let rows = sqlx::query(
            "SELECT m.id,m.shift_id,s.register_id,m.amount_minor,m.reason,m.created_by,\
             m.approved_by,m.created_at \
             FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id \
             WHERE s.company_id=$1 AND ($2::text IS NULL OR s.branch_id=$2) \
             ORDER BY m.created_at DESC,m.id LIMIT $3",
        )
        .bind(actor.company_id().as_str())
        .bind(actor.branch_id().map(|value| value.as_str()))
        .bind(MAX_CASH_MOVEMENTS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok::<_, Error>(PosCashMovementRow {
                    id: row.try_get("id")?,
                    shift_id: row.try_get("shift_id")?,
                    register_id: row.try_get("register_id")?,
                    amount_minor: row.try_get("amount_minor")?,
                    reason: row.try_get("reason")?,
                    created_by: row.try_get("created_by")?,
                    approved_by: row.try_get("approved_by")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn cash_summary(&mut self, actor: &PosReadActor) -> Result<PosCashSummary, Error> {
        let row = sqlx::query(
            "SELECT COUNT(*) FILTER (WHERE m.approved_by IS NULL)::bigint pending_count,\
             COALESCE(SUM(m.amount_minor) FILTER (WHERE m.approved_by IS NULL),0)::bigint \
             pending_minor,\
             COUNT(*) FILTER (WHERE m.approved_by IS NOT NULL)::bigint approved_count,\
             COALESCE(SUM(m.amount_minor) FILTER (WHERE m.approved_by IS NOT NULL),0)::bigint \
             approved_minor \
             FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id \
             WHERE s.company_id=$1 AND ($2::text IS NULL OR s.branch_id=$2)",
        )
        .bind(actor.company_id().as_str())
        .bind(actor.branch_id().map(|value| value.as_str()))
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(PosCashSummary {
            pending_count: row.try_get("pending_count")?,
            pending_minor: row.try_get("pending_minor")?,
            approved_count: row.try_get("approved_count")?,
            approved_minor: row.try_get("approved_minor")?,
        })
    }

    async fn day_close(
        &mut self,
        actor: &PosReadActor,
        business_date: &str,
    ) -> Result<Option<PosDayCloseRow>, Error> {
        let Some(branch) = actor.branch_id() else {
            // A Company-wide work context has no single branch day close; the
            // retained report read also required a branch for day close state.
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT id,state,prepared_by,prepared_at,confirmed_by,confirmed_at,version \
             FROM pos_day_closes \
             WHERE company_id=$1 AND branch_id=$2 AND business_date=$3 LIMIT 2",
        )
        .bind(actor.company_id().as_str())
        .bind(branch.as_str())
        .bind(business_date)
        .fetch_all(&mut *self.connection)
        .await?;
        if rows.len() > 1 {
            return Err(PersistenceError::new("POS_WORKSPACE_DAY_DUPLICATE").into());
        }
        rows.into_iter()
            .next()
            .map(|row| {
                Ok::<_, Error>(PosDayCloseRow {
                    id: row.try_get("id")?,
                    state: row.try_get("state")?,
                    prepared_by: row.try_get("prepared_by")?,
                    prepared_at: row.try_get("prepared_at")?,
                    confirmed_by: row.try_get("confirmed_by")?,
                    confirmed_at: row.try_get("confirmed_at")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
    }

    /// `LIMIT $5` is the requested bounded page; the retained read used a
    /// fixed `LIMIT 150`, so an explicit shorter page truncates instead of
    /// failing the workspace.
    async fn sales(
        &mut self,
        actor: &PosReadActor,
        query: &PosWorkspaceQuery,
    ) -> Result<Vec<PosSaleRow>, Error> {
        let rows = sqlx::query(
            "SELECT s.id,s.company_id,s.branch_id,s.register_id,s.shift_id,s.number,\
             s.business_date,s.state,s.currency,s.party_id,s.subtotal_minor,s.tax_minor,\
             s.discount_minor,s.total_minor,s.created_by,s.created_at,s.version \
             FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id \
             WHERE s.company_id=$1 AND ($2::text IS NULL OR s.branch_id=$2) \
               AND ($3='' OR s.number ILIKE $4) \
             ORDER BY s.created_at DESC,s.id DESC LIMIT $5",
        )
        .bind(actor.company_id().as_str())
        .bind(actor.branch_id().map(|value| value.as_str()))
        .bind(&query.search)
        .bind(format!("%{}%", query.search))
        .bind(query.limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok::<_, Error>(PosSaleRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    register_id: row.try_get("register_id")?,
                    shift_id: row.try_get("shift_id")?,
                    number: row.try_get("number")?,
                    business_date: row.try_get("business_date")?,
                    state: row.try_get("state")?,
                    currency: row.try_get("currency")?,
                    party_id: row.try_get("party_id")?,
                    subtotal_minor: row.try_get("subtotal_minor")?,
                    tax_minor: row.try_get("tax_minor")?,
                    discount_minor: row.try_get("discount_minor")?,
                    total_minor: row.try_get("total_minor")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                    version: row.try_get("version")?,
                })
            })
            .collect()
    }

    async fn sale(
        &mut self,
        actor: &PosReadActor,
        sale_id: &str,
    ) -> Result<Option<PosSaleRow>, Error> {
        let row = sqlx::query(
            "SELECT id,company_id,branch_id,register_id,shift_id,number,business_date,state,\
             currency,party_id,subtotal_minor,tax_minor,discount_minor,total_minor,created_by,\
             created_at,version \
             FROM pos_sales \
             WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) LIMIT 2",
        )
        .bind(sale_id)
        .bind(actor.company_id().as_str())
        .bind(actor.branch_id().map(|value| value.as_str()))
        .fetch_all(&mut *self.connection)
        .await?;
        if row.len() > 1 {
            return Err(PersistenceError::new("POS_WORKSPACE_SALE_DUPLICATE").into());
        }
        row.into_iter()
            .next()
            .map(|row| {
                Ok::<_, Error>(PosSaleRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    register_id: row.try_get("register_id")?,
                    shift_id: row.try_get("shift_id")?,
                    number: row.try_get("number")?,
                    business_date: row.try_get("business_date")?,
                    state: row.try_get("state")?,
                    currency: row.try_get("currency")?,
                    party_id: row.try_get("party_id")?,
                    subtotal_minor: row.try_get("subtotal_minor")?,
                    tax_minor: row.try_get("tax_minor")?,
                    discount_minor: row.try_get("discount_minor")?,
                    total_minor: row.try_get("total_minor")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
    }

    async fn lines(&mut self, sale_id: &str) -> Result<Vec<PosLineRow>, Error> {
        let rows = sqlx::query(
            "SELECT l.id,l.sale_id,l.fulfillment_id,f.state fulfillment_state,l.product_id,\
             l.name,l.quantity_milli,l.price_minor,l.discount_minor,l.subtotal_minor,\
             l.tax_minor,l.returned_milli,l.refunded_minor \
             FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id \
             WHERE l.sale_id=$1 ORDER BY l.id LIMIT $2",
        )
        .bind(sale_id)
        .bind(MAX_LINES as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok::<_, Error>(PosLineRow {
                    id: row.try_get("id")?,
                    sale_id: row.try_get("sale_id")?,
                    fulfillment_id: row.try_get("fulfillment_id")?,
                    fulfillment_state: row.try_get("fulfillment_state")?,
                    product_id: row.try_get("product_id")?,
                    name: row.try_get("name")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    price_minor: row.try_get("price_minor")?,
                    discount_minor: row.try_get("discount_minor")?,
                    subtotal_minor: row.try_get("subtotal_minor")?,
                    tax_minor: row.try_get("tax_minor")?,
                    returned_milli: row.try_get("returned_milli")?,
                    refunded_minor: row.try_get("refunded_minor")?,
                })
            })
            .collect()
    }

    async fn payments(&mut self, sale_id: &str) -> Result<Vec<PosPaymentRow>, Error> {
        let rows = sqlx::query(
            "SELECT id,sale_id,method,state,amount_minor,tendered_minor,reference,created_by,\
             created_at,version,(provider_received_at IS NOT NULL) provider_confirmed \
             FROM pos_payments WHERE sale_id=$1 ORDER BY created_at,id LIMIT $2",
        )
        .bind(sale_id)
        .bind(MAX_PAYMENTS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok::<_, Error>(PosPaymentRow {
                    id: row.try_get("id")?,
                    sale_id: row.try_get("sale_id")?,
                    method: row.try_get("method")?,
                    state: row.try_get("state")?,
                    amount_minor: row.try_get("amount_minor")?,
                    tendered_minor: row.try_get("tendered_minor")?,
                    reference: row.try_get("reference")?,
                    created_by: row.try_get("created_by")?,
                    created_at: row.try_get("created_at")?,
                    version: row.try_get("version")?,
                    provider_confirmed: row.try_get("provider_confirmed")?,
                })
            })
            .collect()
    }
}

impl PosWorkspaceRepository for PgPosWorkspaceRead<'_> {
    type Error = Error;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a PosReadActor,
        query: &'a PosWorkspaceQuery,
        business_date: &'a str,
    ) -> PosReadFuture<'a, PosWorkspaceSource, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let registers = self.registers(actor).await?;
            let settings = self.settings(actor).await?;
            let shifts = if query.reads_shifts() {
                self.shifts(actor).await?
            } else {
                Vec::new()
            };
            let (cash_movements, cash_summary) = if query.reads_cash() {
                (
                    self.cash_movements(actor).await?,
                    self.cash_summary(actor).await?,
                )
            } else {
                // The retained `wants('shifts')` gate skipped both the list and
                // its aggregate; the payload stays internally consistent.
                (Vec::new(), PosCashSummary::default())
            };
            let day_close = self.day_close(actor, business_date).await?;
            let sales = if query.reads_sales() {
                self.sales(actor, query).await?
            } else {
                Vec::new()
            };
            let (sale, lines, payments) = match query.sale_id.as_deref() {
                Some(sale_id) => {
                    let sale = self.sale(actor, sale_id).await?;
                    if sale.is_some() {
                        (
                            sale,
                            self.lines(sale_id).await?,
                            self.payments(sale_id).await?,
                        )
                    } else {
                        (None, Vec::new(), Vec::new())
                    }
                }
                None => (None, Vec::new(), Vec::new()),
            };
            Ok(PosWorkspaceSource {
                registers,
                settings,
                shifts,
                cash_movements,
                cash_summary,
                day_close,
                sales,
                sale,
                lines,
                payments,
            })
        })
    }
}
