//! PostgreSQL adapter for the native Procurement supplier promise command.
//!
//! All Procurement SQL owns only the immutable `procurement_delivery_promises`
//! and `procurement_delivery_promise_lines` history plus the shared Workflow
//! command receipt/audit/outbox evidence. The one Commerce write is the exact
//! `commerce_orders.version` CAS effect exported by Commerce's retained
//! `stageSupplierPromiseOrderVersion` port. No Commerce, Inventory, Finance,
//! Records or IAM source value is trusted from the request; composition supplies
//! the current actor and the Commerce source through the owner port.
use crate::application::supplier_promise_command::{
    StoredSupplierPromiseReceipt, SupplierPromiseCommandFuture, SupplierPromiseCommandRepository,
    SupplierPromiseCommitPlan,
};
use crate::domain::supplier_promise_command::{
    PreparedPromiseLine, PromiseOrderLineSource, PromiseOrderSource, PromiseSource,
    SupplierPromiseCommandActor, SupplierPromiseReadback, SupplierPromiseRecordReceipt,
    line_id, source_changed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn source_limit() -> Error {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "SUPPLIER_PROMISE_SOURCE_LIMIT",
        "The purchase order has too many lines for this bounded confirmation.",
    )
    .into()
}

pub struct PgSupplierPromiseCommand<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgSupplierPromiseCommand<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        require_serializable(self.connection).await
    }
}

async fn require_serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("PROCUREMENT_PROMISE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Procurement history only. Acquire before the first snapshot when the same
/// unit may insert a promise and its lines; this is not source authorization.
pub async fn lock_supplier_promise_history_writes(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PROCUREMENT_PROMISE_SERIALIZABLE_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE procurement_delivery_promises,procurement_delivery_promise_lines IN SHARE ROW EXCLUSIVE MODE")
        .execute(c).await?;
    Ok(())
}

/// Exact image of one selected promise and its lines, not the order's complete
/// promise history. Text remains unbounded and this is not a TS fingerprint.
/// The caller must separately prove Commerce, Workflow, Files and actor facts.
pub struct CurrentSupplierPromiseHistory {
    company: CompanyId,
    order_id: String,
    promise_id: String,
    unit: (String, i32, String),
    image: Option<serde_json::Value>,
}

impl CurrentSupplierPromiseHistory {
    pub fn image(&self) -> Option<&serde_json::Value> { self.image.as_ref() }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if promise_history_unit(c).await? != self.unit
            || promise_history_image(c, &self.company, &self.order_id, &self.promise_id).await? != self.image {
            return Err(source_changed().into());
        }
        Ok(())
    }
}

async fn promise_history_unit(c: &mut PgConnection) -> Result<(String, i32, String), Error> {
    require_serializable(c).await?;
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('procurement_delivery_promises'::regclass,'procurement_delivery_promise_lines'::regclass)")
        .fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("PROCUREMENT_PROMISE_HISTORY_LOCKS_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text").fetch_one(c).await?)
}

async fn promise_history_image(c: &mut PgConnection, company: &CompanyId, order_id: &str, promise_id: &str) -> Result<Option<serde_json::Value>, Error> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('promise',to_jsonb(p),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal,l.id) FROM procurement_delivery_promise_lines l WHERE l.promise_id=p.id),'[]'::jsonb)) FROM procurement_delivery_promises p WHERE p.company_id=$1 AND p.order_id=$2 AND p.id=$3")
        .bind(company.as_str()).bind(order_id).bind(promise_id).fetch_optional(c).await?)
}

/// Bind only after all owner locks have been acquired before the snapshot.
pub async fn inspect_supplier_promise_history(c: &mut PgConnection, company: &CompanyId, order_id: &str, promise_id: &str) -> Result<CurrentSupplierPromiseHistory, Error> {
    let unit = promise_history_unit(c).await?;
    let image = promise_history_image(c, company, order_id, promise_id).await?;
    Ok(CurrentSupplierPromiseHistory { company: company.clone(), order_id: order_id.into(), promise_id: promise_id.into(), unit, image })
}

impl SupplierPromiseCommandRepository for PgSupplierPromiseCommand<'_> {
    type Error = Error;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, Option<StoredSupplierPromiseReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT operation,actor_id,record_id,request_hash,authority_json,result_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(StoredSupplierPromiseReceipt {
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    request_fingerprint: row.try_get("request_hash")?,
                    authority_json: row.try_get("authority_json")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }

    fn source<'a>(
        &'a mut self,
        actor: &'a SupplierPromiseCommandActor,
        order_id: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, Option<PromiseSource>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.state,o.version,\
                 o.order_date,o.due_date,o.currency,o.party_id \
                 FROM commerce_orders o \
                 WHERE o.id=$1 AND o.company_id=$2 \
                   AND ($3::text IS NULL OR o.branch_id=$3) \
                   AND ($4::text IS NULL OR o.department_id=$4) \
                 FOR UPDATE",
            )
            .bind(order_id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let order = PromiseOrderSource {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                kind: row.try_get("kind")?,
                state: row.try_get("state")?,
                version: row.try_get("version")?,
                order_date: row.try_get("order_date")?,
                due_date: row.try_get("due_date")?,
                currency: row.try_get("currency")?,
                supplier_id: row.try_get("party_id")?,
            };
            let rows = sqlx::query(
                "SELECT l.id,l.quantity_milli,l.fulfilled_milli \
                 FROM commerce_order_lines l JOIN commerce_orders o ON o.id=l.order_id \
                 WHERE l.order_id=$1 AND o.company_id=$2 \
                 ORDER BY l.ordinal,l.id LIMIT 10001 FOR SHARE OF l",
            )
            .bind(order_id)
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 10_000 {
                return Err(source_limit());
            }
            let lines = rows
                .iter()
                .map(|row| {
                    Ok(PromiseOrderLineSource {
                        id: row.try_get("id")?,
                        quantity_milli: row.try_get("quantity_milli")?,
                        fulfilled_milli: row.try_get("fulfilled_milli")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            Ok(Some(PromiseSource { order, lines }))
        })
    }

    fn insert_promise<'a>(
        &'a mut self,
        plan: &'a SupplierPromiseCommitPlan,
    ) -> SupplierPromiseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let inserted = sqlx::query(
                "INSERT INTO procurement_delivery_promises\
                 (id,order_id,company_id,order_version,supplier_id,currency,required_date,kind,\
                  confirmation_date,supplier_reference,evidence,recorded_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(&plan.promise_id)
            .bind(&plan.order_id)
            .bind(plan.company_id.as_str())
            .bind(plan.order_version)
            .bind(&plan.supplier_id)
            .bind(&plan.currency)
            .bind(&plan.required_date)
            .bind(&plan.kind)
            .bind(&plan.confirmation_date)
            .bind(&plan.supplier_reference)
            .bind(&plan.evidence)
            .bind(&plan.actor_id)
            .bind(plan.now.get())
            .execute(&mut *self.connection)
            .await;
            if let Err(error) = inserted {
                return if is_conflict(&error) {
                    Err(source_changed().into())
                } else {
                    Err(error.into())
                };
            }
            for line in &plan.lines {
                let inserted = sqlx::query(
                    "INSERT INTO procurement_delivery_promise_lines\
                     (id,promise_id,order_line_id,ordinal,quantity_milli,\
                      fulfilled_snapshot_milli,promised_date) \
                     VALUES($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(line_id(&plan.promise_id, line.ordinal))
                .bind(&plan.promise_id)
                .bind(&line.order_line_id)
                .bind(line.ordinal)
                .bind(line.quantity_milli)
                .bind(line.fulfilled_snapshot_milli)
                .bind(&line.promised_date)
                .execute(&mut *self.connection)
                .await;
                if let Err(error) = inserted {
                    return if is_conflict(&error) {
                        Err(source_changed().into())
                    } else {
                        Err(error.into())
                    };
                }
            }
            Ok(())
        })
    }

    fn advance_order_version<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
        expected_version: i64,
        now: TimestampMillis,
    ) -> SupplierPromiseCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE commerce_orders SET version=version+1,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND version=$4 \
                   AND kind='purchase' AND state IN ('confirmed','partially_fulfilled')",
            )
            .bind(now.get())
            .bind(order_id)
            .bind(company.as_str())
            .bind(expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        operation: &'a str,
        actor: &'a SupplierPromiseCommandActor,
        request_fingerprint: &'a str,
        record_id: &'a str,
        result_json: &'a str,
        authority_json: &'a str,
        now: TimestampMillis,
    ) -> SupplierPromiseCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO command_receipts\
                 (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
                  authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(command_id) DO NOTHING",
            )
            .bind(key)
            .bind(operation)
            .bind(&actor.user_id)
            .bind(request_fingerprint)
            .bind(record_id)
            .bind(result_json)
            .bind(now.get())
            .bind(authority_json)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn audit<'a>(
        &'a mut self,
        plan: &'a SupplierPromiseCommitPlan,
        receipt: &'a SupplierPromiseRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO audit_events\
                 (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
                  record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            )
            .bind(plan.key.as_str())
            .bind(&plan.actor_id)
            .bind(&actor_assignment(plan))
            .bind(&plan.operation)
            .bind("procurement_promise")
            .bind(&plan.order_id)
            .bind(receipt.version)
            .bind(&plan.operation)
            .bind(plan.now.get())
            .bind(authority_json)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        plan: &'a SupplierPromiseCommitPlan,
        receipt: &'a SupplierPromiseRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let authority: serde_json::Value =
                serde_json::from_str(authority_json).map_err(|_| invalid_authority())?;
            let payload = serde_json::json!({
                "authority": authority,
                "id": plan.order_id,
                "promiseId": receipt.promise_id,
                "kind": plan.kind,
                "version": receipt.version,
                "companyId": plan.company_id.as_str(),
                "branchId": authority_branch(plan),
                "departmentId": authority_department(plan),
            });
            sqlx::query(
                "INSERT INTO outbox_events\
                 (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
                  payload_json,created_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(plan.key.as_str())
            .bind(&plan.operation)
            .bind("procurement_promise")
            .bind(&plan.order_id)
            .bind(receipt.version)
            .bind(payload.to_string())
            .bind(plan.now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn readback<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
        promise_id: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, Option<SupplierPromiseReadback>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT p.id,p.order_id,p.order_version,p.kind,p.recorded_by,p.created_at,\
                 (SELECT count(*)::bigint FROM procurement_delivery_promise_lines l \
                  WHERE l.promise_id=p.id) line_count \
                 FROM procurement_delivery_promises p \
                 WHERE p.id=$1 AND p.order_id=$2 AND p.company_id=$3",
            )
            .bind(promise_id)
            .bind(order_id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(SupplierPromiseReadback {
                    id: row.try_get("id")?,
                    order_id: row.try_get("order_id")?,
                    order_version: row.try_get("order_version")?,
                    kind: row.try_get("kind")?,
                    recorded_by: row.try_get("recorded_by")?,
                    created_at: row.try_get("created_at")?,
                    line_count: row.try_get("line_count")?,
                })
            })
            .transpose()
        })
    }
}

fn is_conflict(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
}

fn invalid_authority() -> Error {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "COMMAND_RECEIPT_INVALID",
        "The current Procurement authority could not be retained.",
    )
    .into()
}

fn actor_assignment(plan: &SupplierPromiseCommitPlan) -> String {
    plan.assignment_id.clone()
}

fn authority_branch(plan: &SupplierPromiseCommitPlan) -> serde_json::Value {
    plan.branch_id
        .as_ref()
        .map(|value| serde_json::Value::String(value.clone()))
        .unwrap_or(serde_json::Value::Null)
}

fn authority_department(plan: &SupplierPromiseCommitPlan) -> serde_json::Value {
    plan.department_id
        .as_ref()
        .map(|value| serde_json::Value::String(value.clone()))
        .unwrap_or(serde_json::Value::Null)
}

// Keep the imported domain line type visible to readers of this adapter.
#[allow(dead_code)]
fn line_type(_: &PreparedPromiseLine) {}
