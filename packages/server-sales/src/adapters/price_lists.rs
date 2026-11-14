//! PostgreSQL adapter for the native Sales price-list lifecycle.
//!
//! SQL ownership is split deliberately:
//! - `sales_price_lists` and `sales_price_list_items` are Sales-owned and are
//!   the only business rows mutated here.
//! - `commerce_settings`, `finance_parties` and `commerce_products` are
//!   read-only source facts used for the same-transaction currency, customer
//!   and product checks already performed by the retained owner flow.
//! - `command_receipts`, `audit_events` and `outbox_events` are shared
//!   Workflow records.
//!
//! The caller owns the retained serializable Holding transaction; no transaction
//! is opened, committed or retried here.
use crate::application::price_lists::{
    AuditEntry, OutboxEntry, PriceListCommandRepository, PriceListFuture, PriceListWritePlan,
    ReceiptEntry,
};
use crate::domain::price_lists::{
    PriceListCommandReceipt, PriceListDocument, PriceListItemRow, PriceListProductSource,
    PriceListRow, source_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments a \
 WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND a.scope_kind='holding' \
 AND iam_role_allows(a.role,a.company_id,'commerce.read') \
 AND iam_role_allows(a.role,a.company_id,'commerce.sales') \
 AND (NOT $5::boolean OR iam_role_allows(a.role,a.company_id,'commerce.approve')) \
 AND ($6::text IS NULL OR a.role=$6::text))";

fn map_list(row: &sqlx::postgres::PgRow) -> Result<PriceListRow, sqlx::Error> {
    Ok(PriceListRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        name: row.try_get("name")?,
        currency: row.try_get("currency")?,
        customer_id: row.try_get("customer_id")?,
        valid_from: row.try_get("valid_from")?,
        valid_until: row.try_get("valid_until")?,
        max_auto_discount_bps: row.try_get("max_auto_discount_bps")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        prepared_by: row.try_get("prepared_by")?,
        approved_by: row.try_get("approved_by")?,
        approval_reason: row.try_get("approval_reason")?,
        activated_at: row.try_get("activated_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_item(row: &sqlx::postgres::PgRow) -> Result<PriceListItemRow, sqlx::Error> {
    Ok(PriceListItemRow {
        id: row.try_get("id")?,
        price_list_id: row.try_get("price_list_id")?,
        product_id: row.try_get("product_id")?,
        price_minor: row.try_get("price_minor")?,
        tax_bps: row.try_get("tax_bps")?,
        ordinal: row.try_get("ordinal")?,
    })
}

pub struct PgPriceLists<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPriceLists<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("SALES_PRICING_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("SALES_PRICING_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn authority(
        &mut self,
        actor: &crate::domain::price_lists::PriceListActor,
        approve: bool,
        now: TimestampMillis,
    ) -> Result<bool, Error> {
        self.ready().await?;
        Ok(sqlx::query_scalar(AUTHORITY)
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now.get())
            .bind(approve)
            .bind(actor.role.as_deref())
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn select_list(
        &mut self,
        company: &CompanyId,
        id: &str,
        lock: bool,
    ) -> Result<Option<PriceListRow>, Error> {
        self.ready().await?;
        let sql = if lock {
            "SELECT id,company_id,name,currency,customer_id,valid_from,valid_until,\
             max_auto_discount_bps,status,version,created_by,prepared_by,approved_by,\
             approval_reason,activated_at,created_at,updated_at FROM sales_price_lists \
             WHERE id=$1 AND company_id=$2 FOR UPDATE"
        } else {
            "SELECT id,company_id,name,currency,customer_id,valid_from,valid_until,\
             max_auto_discount_bps,status,version,created_by,prepared_by,approved_by,\
             approval_reason,activated_at,created_at,updated_at FROM sales_price_lists \
             WHERE id=$1 AND company_id=$2"
        };
        let row = sqlx::query(sql)
            .bind(id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| map_list(&row))
            .transpose()
            .map_err(Into::into)
    }

    async fn select_items(
        &mut self,
        company: &CompanyId,
        id: &str,
    ) -> Result<Vec<PriceListItemRow>, Error> {
        self.ready().await?;
        let rows = sqlx::query(
            "SELECT i.id,i.price_list_id,i.product_id,i.price_minor,i.tax_bps,i.ordinal \
             FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id \
             WHERE i.price_list_id=$1 AND l.company_id=$2 ORDER BY i.ordinal,i.id",
        )
        .bind(id)
        .bind(company.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(map_item)
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    async fn readback_document(
        &mut self,
        company: &CompanyId,
        id: &str,
    ) -> Result<Option<PriceListDocument>, Error> {
        let Some(list) = self.select_list(company, id, false).await? else {
            return Ok(None);
        };
        let items = self.select_items(company, id).await?;
        Ok(Some(PriceListDocument { list, items }))
    }
}

impl PriceListCommandRepository for PgPriceLists<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a crate::domain::price_lists::PriceListActor,
        approve: bool,
        now: TimestampMillis,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move { PgPriceLists::authority(self, actor, approve, now).await })
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> PriceListFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("sales-price-list-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> PriceListFuture<'a, String, Self::Error> {
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
    ) -> PriceListFuture<'a, Option<PriceListCommandReceipt>, Self::Error> {
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
                Ok::<_, Error>(PriceListCommandReceipt {
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

    fn currency<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PriceListFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT currency FROM commerce_settings WHERE company_id=$1")
                    .bind(company.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn customer_valid<'a>(
        &'a mut self,
        company: &'a CompanyId,
        customer_id: &'a str,
        currency: &'a str,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM finance_parties \
                 WHERE id=$1 AND company_id=$2 AND kind='customer' AND status='active' \
                 AND currency=$3 AND person_id IS NULL)",
            )
            .bind(customer_id)
            .bind(company.as_str())
            .bind(currency)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn product<'a>(
        &'a mut self,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListProductSource>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,version FROM commerce_products \
                 WHERE id=$1 AND company_id=$2 AND status='active'",
            )
            .bind(product_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PriceListProductSource {
                    id: row.try_get("id")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn product_unchanged<'a>(
        &'a mut self,
        company: &'a CompanyId,
        product_id: &'a str,
        version: i64,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM commerce_products \
                 WHERE id=$1 AND company_id=$2 AND status='active' AND version=$3)",
            )
            .bind(product_id)
            .bind(company.as_str())
            .bind(version)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn list<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        lock: bool,
    ) -> PriceListFuture<'a, Option<PriceListRow>, Self::Error> {
        Box::pin(async move { PgPriceLists::select_list(self, company, id, lock).await })
    }

    fn items<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> PriceListFuture<'a, Vec<PriceListItemRow>, Self::Error> {
        Box::pin(async move { PgPriceLists::select_items(self, company, id).await })
    }

    fn insert_list<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO sales_price_lists\
                 (id,company_id,name,currency,customer_id,valid_from,valid_until,\
                 max_auto_discount_bps,status,version,created_by,prepared_by,approved_by,\
                 approval_reason,activated_at,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.row.id)
            .bind(&plan.row.company_id)
            .bind(&plan.row.name)
            .bind(&plan.row.currency)
            .bind(plan.row.customer_id.as_deref())
            .bind(&plan.row.valid_from)
            .bind(&plan.row.valid_until)
            .bind(plan.row.max_auto_discount_bps)
            .bind(&plan.row.status)
            .bind(plan.row.version)
            .bind(&plan.row.created_by)
            .bind(&plan.row.prepared_by)
            .bind(plan.row.approved_by.as_deref())
            .bind(plan.row.approval_reason.as_deref())
            .bind(plan.row.activated_at)
            .bind(plan.row.created_at)
            .bind(plan.row.updated_at)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn update_list<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let expected_version = plan.row.version - 1;
            let changed = sqlx::query(
                "UPDATE sales_price_lists SET name=$1,currency=$2,customer_id=$3,\
                 valid_from=$4,valid_until=$5,max_auto_discount_bps=$6,prepared_by=$7,\
                 version=$8,updated_at=$9 \
                 WHERE id=$10 AND company_id=$11 AND version=$12 AND status='draft'",
            )
            .bind(&plan.row.name)
            .bind(&plan.row.currency)
            .bind(plan.row.customer_id.as_deref())
            .bind(&plan.row.valid_from)
            .bind(&plan.row.valid_until)
            .bind(plan.row.max_auto_discount_bps)
            .bind(&plan.row.prepared_by)
            .bind(plan.row.version)
            .bind(plan.row.updated_at)
            .bind(&plan.row.id)
            .bind(&plan.row.company_id)
            .bind(expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn replace_items<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            if plan.item_replace {
                sqlx::query("DELETE FROM sales_price_list_items WHERE price_list_id=$1")
                    .bind(&plan.row.id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            for item in plan.items {
                let changed = sqlx::query(
                    "INSERT INTO sales_price_list_items\
                     (id,price_list_id,product_id,price_minor,tax_bps,ordinal) \
                     VALUES($1,$2,$3,$4,$5,$6)",
                )
                .bind(&item.id)
                .bind(&item.price_list_id)
                .bind(&item.product_id)
                .bind(item.price_minor)
                .bind(item.tax_bps)
                .bind(item.ordinal)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(source_invalid().into());
                }
            }
            Ok(true)
        })
    }

    fn decide_list<'a>(
        &'a mut self,
        row: &'a PriceListRow,
        expected_version: i64,
    ) -> PriceListFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE sales_price_lists SET status=$1,approved_by=$2,approval_reason=$3,\
                 activated_at=$4,version=$5,updated_at=$6 \
                 WHERE id=$7 AND company_id=$8 AND version=$9 AND status='draft'",
            )
            .bind(&row.status)
            .bind(row.approved_by.as_deref())
            .bind(row.approval_reason.as_deref())
            .bind(row.activated_at)
            .bind(row.version)
            .bind(row.updated_at)
            .bind(&row.id)
            .bind(&row.company_id)
            .bind(expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn readback<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListDocument>, Self::Error> {
        Box::pin(async move { PgPriceLists::readback_document(self, company, id).await })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> PriceListFuture<'a, bool, Self::Error> {
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
        actor: &'a crate::domain::price_lists::PriceListActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> PriceListFuture<'a, (), Self::Error> {
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
    ) -> PriceListFuture<'a, (), Self::Error> {
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
                "status": entry.status,
                "companyId": company,
                "branchId": branch,
                "departmentId": department,
                "version": entry.version,
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
