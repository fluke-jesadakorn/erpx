//! PostgreSQL adapter for the native Sales confirmed sales-order cancellation.
//!
//! SQL is Commerce-owned (`commerce_orders`, `commerce_order_lines`,
//! `commerce_fulfillments`) plus the Inventory `inventory_order_reservations`
//! release and the shared `command_receipts`, `audit_events` and
//! `outbox_events` records. Current IAM authority is re-read through the same
//! assignments/IAM functions used by the retained owner. No Finance, Quality,
//! Sales document or Files row is mutated. The caller's serializable Holding
//! transaction is required and no commit is performed here.
use crate::application::sales_order_cancellation::{
    CancellationAuditEntry, CancellationOutboxEntry, CancellationReceiptEntry,
    SalesCancellationFuture, SalesCancellationRepository,
};
use crate::domain::sales_order_cancellation::{
    CancellationReceiptRecord, SalesCancellationActor, SalesCancellationPlan, SalesCancellationRow,
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

const RELEASE: &str = "UPDATE inventory_order_reservations r \
 SET released_milli=r.quantity_milli-r.consumed_milli,\
 release_reason='Source order cancelled' \
 WHERE r.order_line_id IN (SELECT id FROM commerce_order_lines WHERE order_id=$1) \
 AND EXISTS (SELECT 1 FROM commerce_orders o WHERE o.id=$1 AND o.company_id=$2) \
 AND r.quantity_milli>r.consumed_milli+r.released_milli";

fn scope(actor: &SalesCancellationActor) -> (Option<&str>, Option<&str>) {
    (
        actor.branch_id.as_ref().map(|value| value.as_str()),
        actor.department_id.as_ref().map(|value| value.as_str()),
    )
}

fn map_order(row: &sqlx::postgres::PgRow) -> Result<SalesCancellationRow, sqlx::Error> {
    Ok(SalesCancellationRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        kind: row.try_get("kind")?,
        state: row.try_get("state")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        approved_by: row.try_get("approved_by")?,
        subtotal_minor: row.try_get("subtotal_minor")?,
        tax_minor: row.try_get("tax_minor")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub struct PgSalesCancellation<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgSalesCancellation<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("SALES_CANCELLATION_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("SALES_CANCELLATION_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn select_order(
        &mut self,
        actor: &SalesCancellationActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<SalesCancellationRow>, Error> {
        self.ready().await?;
        let (branch, department) = scope(actor);
        let row = if lock {
            sqlx::query(
                "SELECT id,company_id,branch_id,department_id,kind,state,version,\
                 created_by,approved_by,subtotal_minor,tax_minor,updated_at \
                 FROM commerce_orders \
                 WHERE id=$1 AND company_id=$2 AND kind IN ('quote','sales','purchase') \
                 AND ($3::text IS NULL OR branch_id=$3) \
                 AND ($4::text IS NULL OR department_id=$4) FOR UPDATE",
            )
        } else {
            sqlx::query(
                "SELECT id,company_id,branch_id,department_id,kind,state,version,\
                 created_by,approved_by,subtotal_minor,tax_minor,updated_at \
                 FROM commerce_orders \
                 WHERE id=$1 AND company_id=$2 AND kind IN ('quote','sales','purchase') \
                 AND ($3::text IS NULL OR branch_id=$3) \
                 AND ($4::text IS NULL OR department_id=$4)",
            )
        }
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
        actor: &SalesCancellationActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> Result<bool, Error> {
        self.ready().await?;
        let (branch, department) = scope(actor);
        Ok(sqlx::query_scalar(AUTHORITY)
            .bind(&actor.assignment_id)
            .bind(&actor.user_id)
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

impl SalesCancellationRepository for PgSalesCancellation<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a SalesCancellationActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, bool, Self::Error> {
        Box::pin(
            async move { PgSalesCancellation::authority(self, actor, write, approve, now).await },
        )
    }

    fn lock_command<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SalesCancellationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("commerce-order-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(
        &'a mut self,
        value: &'a str,
    ) -> SalesCancellationFuture<'a, String, Self::Error> {
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
    ) -> SalesCancellationFuture<'a, Option<CancellationReceiptRecord>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,record_id FROM command_receipts \
                 WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CancellationReceiptRecord {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    record_id: row.try_get("record_id")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn lock_order<'a>(
        &'a mut self,
        actor: &'a SalesCancellationActor,
        id: &'a str,
    ) -> SalesCancellationFuture<'a, Option<SalesCancellationRow>, Self::Error> {
        Box::pin(async move { self.select_order(actor, id, true).await })
    }

    fn count_fulfillments<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> SalesCancellationFuture<'a, i64, Self::Error> {
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

    fn release_reservations<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SalesCancellationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(RELEASE)
                .bind(order_id)
                .bind(company.as_str())
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn transition<'a>(
        &'a mut self,
        plan: &'a SalesCancellationPlan,
    ) -> SalesCancellationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE commerce_orders SET state='cancelled',version=version+1,updated_at=$6 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND state='confirmed' \
                 AND kind='sales' \
                 AND ($4::text IS NULL OR branch_id=$4) \
                 AND ($5::text IS NULL OR department_id=$5)",
            )
            .bind(&plan.order_id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(plan.branch_id.as_deref())
            .bind(plan.department_id.as_deref())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a CancellationReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, bool, Self::Error> {
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
            .bind(&entry.actor.user_id)
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
        actor: &'a SalesCancellationActor,
        entry: &'a CancellationAuditEntry<'a>,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO audit_events\
                 (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
                 record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
            )
            .bind(entry.key)
            .bind(&actor.user_id)
            .bind(&actor.assignment_id)
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
        entry: &'a CancellationOutboxEntry<'a>,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, (), Self::Error> {
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
                "salesId": Value::Null,
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

    fn readback_order<'a>(
        &'a mut self,
        actor: &'a SalesCancellationActor,
        id: &'a str,
    ) -> SalesCancellationFuture<'a, Option<SalesCancellationRow>, Self::Error> {
        Box::pin(async move { self.select_order(actor, id, false).await })
    }
}
