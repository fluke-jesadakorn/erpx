//! PostgreSQL adapter for the native Commerce sales goods/mixed delivery.
//!
//! SQL is Commerce-owned (`commerce_orders`, `commerce_order_lines`,
//! `commerce_warehouses`, `commerce_fulfillments`,
//! `commerce_fulfillment_lines`, `commerce_settings`, and the shared
//! `commerce_goods_valuations`, `command_receipts`, `audit_events` and
//! `outbox_events`). The Finance chart is read only for the Company base
//! currency and posting accounts; no Finance row is written here. The caller's
//! serializable transaction is required and this adapter never commits, retries
//! or opens a second connection.
//!
//! Generic command-receipt, digest, readback, audit and outbox evidence reuses
//! [`PgCommerceReceipt`]; only the sales source, delivery fulfillment, journal
//! attachment and fulfilled-line increment are sales-specific.
use crate::adapters::receipt::PgCommerceReceipt;
use crate::adapters::receipt_use_cases::CommerceReceiptRepository;
use crate::application::order::{AuditEntry, OutboxEntry, ReceiptEntry};
use crate::application::sales_delivery::CommerceSalesDeliveryRepository;
use crate::domain::order::{CommerceOrderActor, OrderCommandReceipt};
use crate::domain::receipt::{
    GoodsEffectReadback, GoodsValuationPlan, ReceiptOrder, ReceiptOrderAdvance, ReceiptOrderLine,
    ReceiptReadback, setup_required, source_invalid,
};
use crate::domain::sales_delivery::{
    SalesDeliveryFulfillmentPlan, SalesDeliveryLineValue, SalesDeliveryPostingSettings,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments a \
 WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND iam_role_allows(a.role,a.company_id,'commerce.read') \
 AND iam_role_allows(a.role,a.company_id,'inventory.prepare') \
 AND (NOT $7::boolean OR iam_role_allows(a.role,a.company_id,'commerce.sales')) \
 AND ($5::text IS NULL OR a.role=$5::text) \
 AND (a.scope_kind='holding' \
   OR (a.scope_kind='branch' AND a.scope_node_id=$6::text)))";

const ORDER: &str = "SELECT o.id,o.state,o.version,o.warehouse_id,w.branch_id,o.department_id,\
 o.currency,o.order_date FROM commerce_orders o \
 JOIN commerce_warehouses w ON w.id=o.warehouse_id \
 WHERE o.id=$1 AND o.company_id=$2 AND o.kind='sales' \
 AND ($3::text IS NULL OR w.branch_id=$3) \
 AND ($4::text IS NULL OR o.department_id=$4) FOR UPDATE";

const BASE_CURRENCY: &str = "SELECT c.base_currency FROM commerce_settings s \
 JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id AND c.status='active' \
 WHERE s.company_id=$1";

const LINES: &str = "SELECT l.id,l.product_id,l.item_type,l.description,l.quantity_milli,\
 l.fulfilled_milli,l.price_minor,l.tax_bps,l.subtotal_minor,l.tax_minor,\
 COALESCE((SELECT SUM(fl.subtotal_minor)::bigint FROM commerce_fulfillment_lines fl \
   JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id \
   WHERE fl.order_line_id=l.id AND f.order_id=l.order_id),0) prior_subtotal_minor,\
 COALESCE((SELECT SUM(fl.tax_minor)::bigint FROM commerce_fulfillment_lines fl \
   JOIN commerce_fulfillments f ON f.id=fl.fulfillment_id \
   WHERE fl.order_line_id=l.id AND f.order_id=l.order_id),0) prior_tax_minor,\
 COALESCE(p.tracking_mode,'none') tracking_mode \
 FROM commerce_order_lines l \
 LEFT JOIN commerce_products p ON p.id=l.product_id AND p.company_id=$2 \
 WHERE l.order_id=$1 ORDER BY l.ordinal,l.id LIMIT 201";

type OrderRow = (
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    String,
    String,
);

fn scope(actor: &CommerceOrderActor) -> (Option<&str>, Option<&str>) {
    (
        actor.branch_id.as_ref().map(|value| value.as_str()),
        actor.department_id.as_ref().map(|value| value.as_str()),
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PgCommerceSalesDelivery {
    generic: PgCommerceReceipt,
}

impl PgCommerceSalesDelivery {
    pub const fn new() -> Self {
        Self {
            generic: PgCommerceReceipt,
        }
    }

    async fn ready(c: &mut PgConnection) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCE_DELIVERY_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("COMMERCE_DELIVERY_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
}

impl CommerceSalesDeliveryRepository for PgCommerceSalesDelivery {
    fn ready<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            Ok(())
        })
    }

    fn lock_command<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("commerce-delivery-command:{key}"))
                .execute(&mut *c)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        value: &'a str,
    ) -> PgWorkFuture<'a, String, BusinessError> {
        CommerceReceiptRepository::digest(&mut self.generic, c, value)
    }

    fn receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, Option<OrderCommandReceipt>, BusinessError> {
        CommerceReceiptRepository::receipt(&mut self.generic, c, key)
    }

    fn current_authority<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        now: TimestampMillis,
        mixed: bool,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let (branch, _) = scope(actor);
            Ok(sqlx::query_scalar(AUTHORITY)
                .bind(actor.assignment_id.as_str())
                .bind(actor.user_id.as_str())
                .bind(actor.company_id.as_str())
                .bind(now.get())
                .bind(actor.role.as_deref())
                .bind(branch)
                .bind(mixed)
                .fetch_one(&mut *c)
                .await?)
        })
    }

    fn order_source<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        order_id: &'a str,
    ) -> PgWorkFuture<'a, Option<ReceiptOrder>, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let (branch, department) = scope(actor);
            let row: Option<OrderRow> = sqlx::query_as(ORDER)
                .bind(order_id)
                .bind(actor.company_id.as_str())
                .bind(branch)
                .bind(department)
                .fetch_optional(&mut *c)
                .await?;
            let Some((
                id,
                state,
                version,
                warehouse_id,
                branch_id,
                department_id,
                currency,
                order_date,
            )) = row
            else {
                return Ok(None);
            };
            let base_currency: Option<String> = sqlx::query_scalar(BASE_CURRENCY)
                .bind(actor.company_id.as_str())
                .fetch_optional(&mut *c)
                .await?;
            let base_currency = base_currency.ok_or_else(setup_required)?;
            let rows = sqlx::query(LINES)
                .bind(order_id)
                .bind(actor.company_id.as_str())
                .fetch_all(&mut *c)
                .await?;
            if rows.len() > crate::domain::receipt::MAX_LINES {
                return Err(source_invalid().into());
            }
            let lines = rows
                .iter()
                .map(|row| {
                    Ok::<_, Error>(ReceiptOrderLine {
                        id: row.try_get("id")?,
                        product_id: row.try_get("product_id")?,
                        item_type: row.try_get("item_type")?,
                        description: row.try_get("description")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        fulfilled_milli: row.try_get("fulfilled_milli")?,
                        price_minor: row.try_get("price_minor")?,
                        tax_bps: row.try_get("tax_bps")?,
                        subtotal_minor: row.try_get("subtotal_minor")?,
                        tax_minor: row.try_get("tax_minor")?,
                        prior_subtotal_minor: row.try_get("prior_subtotal_minor")?,
                        prior_tax_minor: row.try_get("prior_tax_minor")?,
                        tracking_mode: row.try_get("tracking_mode")?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(ReceiptOrder {
                id,
                state,
                version,
                warehouse_id,
                branch_id,
                department_id,
                currency,
                base_currency,
                order_date,
                lines,
            }))
        })
    }

    fn delivery_settings<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
    ) -> PgWorkFuture<'a, Option<SalesDeliveryPostingSettings>, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let row: Option<(String, String, String)> = sqlx::query_as(
                "SELECT s.chart_id,s.inventory_account_id,s.cogs_account_id \
                 FROM commerce_settings s JOIN finance_charts c \
                   ON c.id=s.chart_id AND c.company_id=s.company_id AND c.status='active' \
                 WHERE s.company_id=$1",
            )
            .bind(company)
            .fetch_optional(&mut *c)
            .await?;
            Ok(
                row.map(|(chart_id, inventory_account_id, cogs_account_id)| {
                    SalesDeliveryPostingSettings {
                        chart_id,
                        inventory_account_id,
                        cogs_account_id,
                    }
                }),
            )
        })
    }

    fn insert_fulfillment<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a SalesDeliveryFulfillmentPlan,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            sqlx::query(
                "INSERT INTO commerce_fulfillments\
                 (id,order_id,number,fulfillment_date,created_by,created_at,evidence_note,\
                 completion_kind,completion_note,version) \
                 VALUES($1,$2,$3,$4,$5,$6,NULL,$7,$8,1)",
            )
            .bind(&plan.id)
            .bind(&plan.order_id)
            .bind(&plan.number)
            .bind(&plan.date)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .bind(&plan.completion_kind)
            .bind(&plan.completion_note)
            .execute(&mut *c)
            .await?;
            for line in &plan.lines {
                sqlx::query(
                    "INSERT INTO commerce_fulfillment_lines\
                     (id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,\
                     tax_minor,stock_currency) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(format!("{}-{}", plan.id, line.index))
                .bind(&plan.id)
                .bind(&line.order_line_id)
                .bind(line.quantity_milli)
                .bind(line.value_minor)
                .bind(line.subtotal_minor)
                .bind(line.tax_minor)
                .bind(&line.stock_currency)
                .execute(&mut *c)
                .await?;
            }
            Ok(())
        })
    }

    fn attach_journal<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        fulfillment_id: &'a str,
        order_id: &'a str,
        journal_id: &'a str,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let changed = sqlx::query(
                "UPDATE commerce_fulfillments f SET journal_id=$3 \
                 FROM commerce_orders o \
                 WHERE f.id=$1 AND f.order_id=$2 AND o.id=f.order_id \
                 AND o.kind='sales' AND f.journal_id IS NULL",
            )
            .bind(fulfillment_id)
            .bind(order_id)
            .bind(journal_id)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_goods_valuation<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a GoodsValuationPlan,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        CommerceReceiptRepository::insert_goods_valuation(&mut self.generic, c, plan)
    }

    fn goods_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        fulfillment_id: &'a str,
    ) -> PgWorkFuture<'a, Option<GoodsEffectReadback>, BusinessError> {
        CommerceReceiptRepository::goods_readback(&mut self.generic, c, company, fulfillment_id)
    }

    fn increment_fulfilled<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        line: &'a SalesDeliveryLineValue,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let changed = sqlx::query(
                "UPDATE commerce_order_lines SET fulfilled_milli=fulfilled_milli+$3 \
                 WHERE id=$1 AND order_id=$2 \
                 AND fulfilled_milli=$4 AND fulfilled_milli+$3<=quantity_milli",
            )
            .bind(&line.line_id)
            .bind(&line.order_id)
            .bind(line.quantity_milli)
            .bind(line.expected_fulfilled_milli)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn advance_order<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a ReceiptOrderAdvance,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let changed = sqlx::query(
                "UPDATE commerce_orders SET state=CASE WHEN EXISTS(SELECT 1 FROM commerce_order_lines \
                 WHERE order_id=$1 AND fulfilled_milli<quantity_milli) THEN 'partially_fulfilled' \
                 ELSE 'fulfilled' END, version=version+1, updated_at=$3 \
                 WHERE id=$1 AND company_id=$2 AND version=$4 \
                 AND state IN ('confirmed','partially_fulfilled')",
            )
            .bind(&plan.order_id)
            .bind(&plan.company_id)
            .bind(plan.now)
            .bind(plan.expected_version)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        order_id: &'a str,
        fulfillment_id: &'a str,
    ) -> PgWorkFuture<'a, Option<ReceiptReadback>, BusinessError> {
        CommerceReceiptRepository::readback(&mut self.generic, c, actor, order_id, fulfillment_id)
    }

    fn insert_receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        CommerceReceiptRepository::insert_receipt(&mut self.generic, c, entry, now)
    }

    fn audit<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        CommerceReceiptRepository::audit(&mut self.generic, c, actor, entry, now)
    }

    fn outbox<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        CommerceReceiptRepository::outbox(&mut self.generic, c, entry, now)
    }
}
