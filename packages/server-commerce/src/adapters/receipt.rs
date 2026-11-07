//! PostgreSQL adapter for the native Commerce purchase goods receipt.
//!
//! SQL is Commerce-owned (`commerce_orders`, `commerce_order_lines`,
//! `commerce_fulfillments`, `commerce_fulfillment_lines`, `commerce_stock`,
//! `commerce_stock_moves`, `commerce_settings`) plus the shared
//! `command_receipts`, `audit_events` and `outbox_events`. The Finance chart is
//! read only for the Company base currency; no Finance row is written. The
//! caller's serializable transaction is required and this adapter never
//! commits, retries or opens a second connection.
use crate::application::order::{AuditEntry, OutboxEntry, ReceiptEntry};
use crate::application::receipt::CommerceReceiptRepository;
use crate::domain::order::{CommerceOrderActor, OrderCommandReceipt};
use crate::domain::receipt::{
    ReceiptFulfillmentLineReadback, ReceiptFulfillmentPlan, ReceiptFulfillmentReadback,
    ReceiptLineValue, ReceiptOrder, ReceiptOrderAdvance, ReceiptOrderLine, ReceiptReadback,
    ReceiptReadbackLine, ReceiptStockMoveReadback, ReceiptStockReadback, setup_required,
    source_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments a \
 WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND iam_role_allows(a.role,a.company_id,'commerce.read') \
 AND iam_role_allows(a.role,a.company_id,'inventory.prepare') \
 AND ($5::text IS NULL OR a.role=$5::text) \
 AND (a.scope_kind='holding' \
   OR (a.scope_kind='branch' AND a.scope_node_id=$6::text)))";

const ORDER: &str = "SELECT id,state,version,warehouse_id,branch_id,department_id,currency,order_date \
 FROM commerce_orders WHERE id=$1 AND company_id=$2 AND kind='purchase' \
 AND ($3::text IS NULL OR branch_id=$3) \
 AND ($4::text IS NULL OR department_id=$4) FOR UPDATE";

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
pub struct PgCommerceReceipt;

impl PgCommerceReceipt {
    async fn ready(c: &mut PgConnection) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCE_RECEIPT_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("COMMERCE_RECEIPT_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
}

impl CommerceReceiptRepository for PgCommerceReceipt {
    fn ready<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            PgCommerceReceipt::ready(c).await?;
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
                .bind(format!("commerce-receipt-command:{key}"))
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
        Box::pin(async move {
            Self::ready(c).await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *c)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, Option<OrderCommandReceipt>, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *c)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(OrderCommandReceipt {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    authority_json: row.try_get("authority_json")?,
                })
            })
            .transpose()
        })
    }

    fn current_authority<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        now: TimestampMillis,
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

    fn insert_fulfillment<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a ReceiptFulfillmentPlan,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            sqlx::query(
                "INSERT INTO commerce_fulfillments\
                 (id,order_id,number,fulfillment_date,created_by,created_at,evidence_note,\
                 completion_kind,completion_note,version) \
                 VALUES($1,$2,$3,$4,$5,$6,NULL,'goods','',1)",
            )
            .bind(&plan.id)
            .bind(&plan.order_id)
            .bind(&plan.number)
            .bind(&plan.date)
            .bind(&plan.actor_id)
            .bind(plan.now)
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

    fn increment_fulfilled<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        line: &'a ReceiptLineValue,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let changed = sqlx::query(
                "UPDATE commerce_order_lines SET fulfilled_milli=fulfilled_milli+$3 \
                 WHERE id=$1 AND order_id=$2 AND item_type='stock' \
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
        Box::pin(async move {
            Self::ready(c).await?;
            let (branch, department) = scope(actor);
            let order: Option<(String, i64)> = sqlx::query_as(
                "SELECT state,version FROM commerce_orders WHERE id=$1 AND company_id=$2 \
                 AND ($3::text IS NULL OR branch_id=$3) \
                 AND ($4::text IS NULL OR department_id=$4)",
            )
            .bind(order_id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *c)
            .await?;
            let Some((state, version)) = order else {
                return Ok(None);
            };
            let lines = sqlx::query("SELECT id,fulfilled_milli FROM commerce_order_lines WHERE order_id=$1 ORDER BY ordinal,id")
                .bind(order_id)
                .fetch_all(&mut *c)
                .await?
                .iter()
                .map(|row| {
                    Ok::<_, Error>(ReceiptReadbackLine {
                        id: row.try_get("id")?,
                        fulfilled_milli: row.try_get("fulfilled_milli")?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let fulfillment = sqlx::query(
                "SELECT id,order_id,fulfillment_date FROM commerce_fulfillments \
                 WHERE id=$1 AND order_id=$2",
            )
            .bind(fulfillment_id)
            .bind(order_id)
            .fetch_optional(&mut *c)
            .await?;
            let fulfillment = match fulfillment {
                None => None,
                Some(row) => {
                    let id: String = row.try_get("id")?;
                    let order_id: String = row.try_get("order_id")?;
                    let fulfillment_date: String = row.try_get("fulfillment_date")?;
                    let lines = sqlx::query(
                        "SELECT id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor,\
                         stock_currency FROM commerce_fulfillment_lines \
                         WHERE fulfillment_id=$1 ORDER BY id",
                    )
                    .bind(&id)
                    .fetch_all(&mut *c)
                    .await?
                    .iter()
                    .map(|row| {
                        Ok::<_, Error>(ReceiptFulfillmentLineReadback {
                            id: row.try_get("id")?,
                            order_line_id: row.try_get("order_line_id")?,
                            quantity_milli: row.try_get("quantity_milli")?,
                            value_minor: row.try_get("value_minor")?,
                            subtotal_minor: row.try_get("subtotal_minor")?,
                            tax_minor: row.try_get("tax_minor")?,
                            stock_currency: row.try_get("stock_currency")?,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                    Some(ReceiptFulfillmentReadback {
                        id,
                        order_id,
                        fulfillment_date,
                        lines,
                    })
                }
            };
            let stock_moves = sqlx::query(
                "SELECT id,warehouse_id,product_id,quantity_milli,value_minor \
                 FROM commerce_stock_moves WHERE fulfillment_id=$1 ORDER BY id",
            )
            .bind(fulfillment_id)
            .fetch_all(&mut *c)
            .await?
            .iter()
            .map(|row| {
                Ok::<_, Error>(ReceiptStockMoveReadback {
                    id: row.try_get("id")?,
                    warehouse_id: row.try_get("warehouse_id")?,
                    product_id: row.try_get("product_id")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    value_minor: row.try_get("value_minor")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
            let stock = sqlx::query(
                "SELECT s.product_id,s.quantity_milli,s.value_minor,s.version FROM commerce_stock s \
                 JOIN commerce_orders o ON o.warehouse_id=s.warehouse_id AND o.id=$2 \
                 WHERE s.product_id IN (SELECT ol.product_id FROM commerce_fulfillment_lines fl \
                   JOIN commerce_order_lines ol ON ol.id=fl.order_line_id \
                   WHERE fl.fulfillment_id=$1) ORDER BY s.product_id",
            )
            .bind(fulfillment_id)
            .bind(order_id)
            .fetch_all(&mut *c)
            .await?
            .iter()
            .map(|row| {
                Ok::<_, Error>(ReceiptStockReadback {
                    product_id: row.try_get("product_id")?,
                    quantity_milli: row.try_get("quantity_milli")?,
                    value_minor: row.try_get("value_minor")?,
                    version: row.try_get("version")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(ReceiptReadback {
                state,
                version,
                lines,
                fulfillment,
                stock_moves,
                stock,
            }))
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let authority = entry.actor.authority_json();
            let changed = sqlx::query(
                "INSERT INTO command_receipts\
                 (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
                 authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(command_id) DO NOTHING",
            )
            .bind(entry.key)
            .bind(entry.operation)
            .bind(entry.actor.user_id.as_str())
            .bind(entry.request_hash)
            .bind(entry.record_id)
            .bind(entry.result_json)
            .bind(now.get())
            .bind(authority)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn audit<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            sqlx::query(
                "INSERT INTO audit_events\
                 (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
                 record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
            )
            .bind(entry.key)
            .bind(actor.user_id.as_str())
            .bind(actor.assignment_id.as_str())
            .bind(entry.operation)
            .bind(entry.record_type)
            .bind(entry.record_id)
            .bind(entry.version)
            .bind(now.get())
            .bind(entry.authority_json)
            .execute(&mut *c)
            .await?;
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            Self::ready(c).await?;
            let authority: Value =
                serde_json::from_str(entry.authority_json).unwrap_or(Value::Null);
            let company = authority.get("companyId").cloned().unwrap_or(Value::Null);
            let branch = authority.get("branchId").cloned().unwrap_or(Value::Null);
            let department = authority
                .get("departmentId")
                .cloned()
                .unwrap_or(Value::Null);
            let payload = json!({
                "authority": authority,
                "id": entry.record_id,
                "state": entry.state,
                "companyId": company,
                "branchId": branch,
                "departmentId": department,
                "version": entry.version,
                "salesId": entry.sales_id,
            })
            .to_string();
            sqlx::query(
                "INSERT INTO outbox_events\
                 (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
                 payload_json,created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(entry.key)
            .bind(entry.operation)
            .bind(entry.record_type)
            .bind(entry.record_id)
            .bind(entry.version)
            .bind(payload)
            .bind(now.get())
            .execute(&mut *c)
            .await?;
            Ok(())
        })
    }
}
