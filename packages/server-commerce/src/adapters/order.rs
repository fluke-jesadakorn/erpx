//! PostgreSQL adapter for the native Commerce quotation/sales-order slice.
//!
//! SQL is Commerce-owned (`commerce_orders`, `commerce_order_lines`,
//! `commerce_products`, `commerce_fulfillments`) plus the shared
//! `command_receipts`, `audit_events` and `outbox_events` records written by
//! the retained workflow command boundary. Current IAM authority is read
//! through the existing assignments/IAM functions used by the retained owner.
//! No Inventory, Finance, Quality, Sales or Files row is mutated. The caller's
//! serializable Holding transaction is required and no commit is performed
//! here.
use crate::application::order::{
    AuditEntry, CommerceOrderCommandRepository, CommerceOrderFuture, CommerceOrderReadRepository,
    OutboxEntry, ReceiptEntry,
};
use crate::domain::order::{
    CommerceOrderActor, ConvertedReadback, OrderCommandReceipt, SalesOrderLineRow, SalesOrderRow,
    TransitionPlan,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments a \
 WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND (($5::boolean AND iam_role_allows(a.role,a.company_id,'commerce.sales')) \
   OR (NOT $5::boolean AND iam_role_allows(a.role,a.company_id,'commerce.read'))) \
 AND (NOT $6::boolean OR iam_role_allows(a.role,a.company_id,'commerce.approve')) \
 AND ($7::text IS NULL OR a.role=$7::text) \
 AND (a.scope_kind='holding' \
   OR (a.scope_kind='branch' AND a.scope_node_id=$8::text) \
   OR (a.scope_kind='department' AND a.scope_node_id=$9::text)))";

fn scope(actor: &CommerceOrderActor) -> (Option<&str>, Option<&str>) {
    (
        actor.branch_id.as_ref().map(|value| value.as_str()),
        actor.department_id.as_ref().map(|value| value.as_str()),
    )
}

fn map_order(row: &sqlx::postgres::PgRow) -> Result<SalesOrderRow, sqlx::Error> {
    Ok(SalesOrderRow {
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
    })
}

fn map_line(row: &sqlx::postgres::PgRow) -> Result<SalesOrderLineRow, sqlx::Error> {
    Ok(SalesOrderLineRow {
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
        sku: row.try_get::<Option<String>, _>("sku")?.unwrap_or_default(),
        unit: row
            .try_get::<Option<String>, _>("unit")?
            .unwrap_or_default(),
        tracking_mode: row
            .try_get::<Option<String>, _>("tracking_mode")?
            .unwrap_or_default(),
    })
}

pub struct PgCommerceOrders<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgCommerceOrders<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("COMMERCE_ORDER_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("COMMERCE_ORDER_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn select_order(
        &mut self,
        actor: &CommerceOrderActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<SalesOrderRow>, Error> {
        self.ready().await?;
        let (branch, department) = scope(actor);
        let sql = if lock {
            "SELECT id,company_id,branch_id,department_id,kind,number,party_id,\
             warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,\
             source_id,version,created_by,approved_by,created_at,updated_at,price_list_id,\
             price_list_version,discount_reason,discount_approval_required,\
             discount_approved_by,discount_approved_at FROM commerce_orders \
             WHERE id=$1 AND company_id=$2 AND kind IN ('quote','sales') \
             AND ($3::text IS NULL OR branch_id=$3) \
             AND ($4::text IS NULL OR department_id=$4) FOR UPDATE"
        } else {
            "SELECT id,company_id,branch_id,department_id,kind,number,party_id,\
             warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,\
             source_id,version,created_by,approved_by,created_at,updated_at,price_list_id,\
             price_list_version,discount_reason,discount_approval_required,\
             discount_approved_by,discount_approved_at FROM commerce_orders \
             WHERE id=$1 AND company_id=$2 AND kind IN ('quote','sales') \
             AND ($3::text IS NULL OR branch_id=$3) \
             AND ($4::text IS NULL OR department_id=$4)"
        };
        let row = sqlx::query(sql)
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.as_ref().map(map_order).transpose().map_err(Into::into)
    }

    async fn authority(
        &mut self,
        actor: &CommerceOrderActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> Result<bool, Error> {
        self.ready().await?;
        let (branch, department) = scope(actor);
        Ok(sqlx::query_scalar(AUTHORITY)
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now.get())
            .bind(write)
            .bind(approve)
            .bind(actor.role.as_deref())
            .bind(branch)
            .bind(department)
            .fetch_one(&mut *self.connection)
            .await?)
    }
}

impl CommerceOrderReadRepository for PgCommerceOrders<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        Box::pin(async move { PgCommerceOrders::authority(self, actor, write, approve, now).await })
    }

    fn order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        Box::pin(async move { PgCommerceOrders::select_order(self, actor, id, false).await })
    }

    fn lines<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, Vec<SalesOrderLineRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT l.id,l.order_id,l.product_id,l.description,l.quantity_milli,\
                 l.fulfilled_milli,l.price_minor,l.tax_bps,l.subtotal_minor,l.tax_minor,\
                 l.ordinal,l.list_price_minor,l.discount_bps,l.item_type,p.sku,p.unit,\
                 p.tracking_mode FROM commerce_order_lines l \
                 LEFT JOIN commerce_products p ON p.id=l.product_id AND p.company_id=$2 \
                 WHERE l.order_id=$1 ORDER BY l.ordinal,l.id",
            )
            .bind(order_id)
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(map_line)
                .collect::<Result<_, _>>()
                .map_err(Into::into)
        })
    }
}

impl CommerceOrderCommandRepository for PgCommerceOrders<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        Box::pin(async move { PgCommerceOrders::authority(self, actor, write, approve, now).await })
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> CommerceOrderFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("commerce-order-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> CommerceOrderFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CommerceOrderFuture<'a, Option<OrderCommandReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
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

    fn lock_order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        Box::pin(async move { PgCommerceOrders::select_order(self, actor, id, true).await })
    }

    fn count_fulfillments<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT count(*) FROM commerce_fulfillments WHERE order_id=$1")
                    .bind(order_id)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn transition<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE commerce_orders SET state=$3,\
                 approved_by=COALESCE($4,approved_by),\
                 discount_approved_by=CASE WHEN $6 THEN NULL WHEN $5 THEN $7 \
                   ELSE discount_approved_by END,\
                 discount_approved_at=CASE WHEN $6 THEN NULL WHEN $5 THEN $8 \
                   ELSE discount_approved_at END,\
                 version=version+1,updated_at=$9 \
                 WHERE id=$1 AND company_id=$2 AND version=$10 AND state=$11 \
                 AND kind IN ('quote','sales') \
                 AND ($12::text IS NULL OR branch_id=$12) \
                 AND ($13::text IS NULL OR department_id=$13)",
            )
            .bind(&plan.order_id)
            .bind(&plan.company_id)
            .bind(&plan.next_state)
            .bind(plan.approved_by.as_deref())
            .bind(plan.set_discount_approval)
            .bind(plan.clear_discount_approval)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .bind(plan.now)
            .bind(plan.expected_version)
            .bind(&plan.expected_state)
            .bind(plan.branch_id.as_deref())
            .bind(plan.department_id.as_deref())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_converted_order<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sales_id = plan
                .sales_id
                .as_deref()
                .ok_or_else(|| PersistenceError::new("COMMERCE_ORDER_CONVERSION_ID_REQUIRED"))?;
            let number = plan.sales_number.as_deref().ok_or_else(|| {
                PersistenceError::new("COMMERCE_ORDER_CONVERSION_NUMBER_REQUIRED")
            })?;
            let changed = sqlx::query(
                "INSERT INTO commerce_orders\
                 (id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,\
                 currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,\
                 price_list_id,price_list_version,discount_reason,discount_approval_required,\
                 discount_approved_by,discount_approved_at,created_by,created_at,updated_at) \
                 SELECT $1,company_id,branch_id,department_id,'sales',$2,party_id,warehouse_id,\
                 currency,order_date,due_date,note,'draft',subtotal_minor,tax_minor,id,\
                 price_list_id,price_list_version,discount_reason,discount_approval_required,\
                 discount_approved_by,discount_approved_at,$3,$4,$4 \
                 FROM commerce_orders WHERE id=$5 AND company_id=$6 AND version=$7 \
                 AND state='converted'",
            )
            .bind(sales_id)
            .bind(number)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .bind(&plan.order_id)
            .bind(&plan.company_id)
            .bind(plan.expected_version + 1)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn copy_converted_lines<'a>(
        &'a mut self,
        quote_id: &'a str,
        sales_id: &'a str,
    ) -> CommerceOrderFuture<'a, u64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO commerce_order_lines\
                 (id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,\
                 tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type) \
                 SELECT $1||'-'||ordinal,$1,product_id,description,quantity_milli,0,price_minor,\
                 tax_bps,subtotal_minor,tax_minor,ordinal,list_price_minor,discount_bps,item_type \
                 FROM commerce_order_lines WHERE order_id=$2 ORDER BY ordinal",
            )
            .bind(sales_id)
            .bind(quote_id)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed)
        })
    }

    fn converted_readback<'a>(
        &'a mut self,
        sales_id: &'a str,
        company: &'a CompanyId,
    ) -> CommerceOrderFuture<'a, Option<ConvertedReadback>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,\
                 currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,\
                 version,created_by,approved_by,created_at,updated_at,price_list_id,\
                 price_list_version,discount_reason,discount_approval_required,\
                 discount_approved_by,discount_approved_at,\
                 (SELECT count(*) FROM commerce_order_lines l WHERE l.order_id=o.id) line_count,\
                 (SELECT COALESCE(sum(l.subtotal_minor),0)::bigint \
                   FROM commerce_order_lines l WHERE l.order_id=o.id) line_subtotal_minor,\
                 (SELECT COALESCE(sum(l.tax_minor),0)::bigint \
                   FROM commerce_order_lines l WHERE l.order_id=o.id) line_tax_minor \
                 FROM commerce_orders o WHERE o.id=$1 AND o.company_id=$2",
            )
            .bind(sales_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ConvertedReadback {
                    order: map_order(&row)?,
                    line_count: row.try_get("line_count")?,
                    line_subtotal_minor: row.try_get("line_subtotal_minor")?,
                    line_tax_minor: row.try_get("line_tax_minor")?,
                })
            })
            .transpose()
        })
    }

    fn readback_order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        Box::pin(async move { PgCommerceOrders::select_order(self, actor, id, false).await })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
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
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn audit<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
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
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
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
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
}
