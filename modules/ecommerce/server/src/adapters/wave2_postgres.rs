//! PostgreSQL adapter for the native Ecommerce wave2 owner-write registry.
//!
//! The caller owns one serializable Holding transaction. Only Ecommerce-owned
//! rows plus the shared command ledger are written here; no product, customer,
//! warehouse, Sales, Inventory or Finance row is touched. All SQL lives here,
//! inside the owner crate. Each closed kind maps to fixed owner SQL with bound
//! positional values (data only), never to caller-supplied SQL text.
use crate::application::wave2::{
    LedgerEntry, OwnerWriteFuture, OwnerWriteRepository, StoredReceipt,
};
use crate::domain::wave2::OwnerWrite;
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_WAVE2_VALUE_INVALID",
        "Review the E-commerce write values.",
    )
}

fn s(values: &[Value], index: usize) -> Result<String, Error> {
    values
        .get(index)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid().into())
}
fn n(values: &[Value], index: usize) -> Result<i64, Error> {
    values
        .get(index)
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid().into())
}
fn ns(values: &[Value], index: usize) -> Result<Option<String>, Error> {
    match values.get(index) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        _ => Err(invalid().into()),
    }
}

pub struct PgOwnerWrite<'c> {
    connection: &'c mut PgConnection,
    company: String,
}

impl<'c> PgOwnerWrite<'c> {
    pub fn new(connection: &'c mut PgConnection, scope: &str) -> Self {
        Self {
            connection,
            company: scope.to_owned(),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ECOMMERCE_WAVE2_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("ECOMMERCE_WAVE2_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &LedgerEntry<'_>) -> Result<(), Error> {
        let authority = serde_json::json!({"companyId": &self.company});
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(format!("wave2:{}", entry.key))
        .bind(entry.operation)
        .bind(entry.actor_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(entry.result)
        .bind(entry.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(conflict("IDEMPOTENCY_CONFLICT", "This command was already used.").into());
        }
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
             record_version,purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$2,$3,$4,$5,1,$3,$6,$7)",
        )
        .bind(entry.key)
        .bind(entry.actor_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        let payload = serde_json::json!({
            "companyId": &self.company,
            "recordId": entry.record_id,
            "result": entry.result,
        });
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,1,$5,$6)",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(&payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn apply_owner(&mut self, write: &OwnerWrite) -> Result<(), Error> {
        let v = &write.values;
        match write.kind {
            "channel_create" => {
                sqlx::query(
                    "INSERT INTO ecommerce_channels\
                     (id,company_id,name,channel_type,configuration_json,status,created_by,\
                     reviewed_by,version,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,'draft',$6,NULL,1,$7,$8) ON CONFLICT(id) DO NOTHING",
                )
                .bind(s(v, 0)?)
                .bind(s(v, 1)?)
                .bind(s(v, 2)?)
                .bind(s(v, 3)?)
                .bind(s(v, 4)?)
                .bind(s(v, 5)?)
                .bind(n(v, 6)?)
                .bind(n(v, 7)?)
                .execute(&mut *self.connection)
                .await?;
            }
            "listing_create" => {
                sqlx::query(
                    "INSERT INTO ecommerce_listings\
                     (id,channel_id,product_id,title,currency,price_minor,status,created_by,\
                     reviewed_by,version,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,'draft',$7,NULL,1,$8,$9) ON CONFLICT(id) DO NOTHING",
                )
                .bind(s(v, 0)?)
                .bind(s(v, 1)?)
                .bind(s(v, 2)?)
                .bind(s(v, 3)?)
                .bind(s(v, 4)?)
                .bind(n(v, 5)?)
                .bind(s(v, 6)?)
                .bind(n(v, 7)?)
                .bind(n(v, 8)?)
                .execute(&mut *self.connection)
                .await?;
            }
            "cart_create" => {
                sqlx::query(
                    "INSERT INTO ecommerce_carts\
                     (id,company_id,branch_id,department_id,channel_id,party_id,warehouse_id,\
                     currency,status,created_by,version,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,'open',$9,1,$10,$11) ON CONFLICT(id) DO NOTHING",
                )
                .bind(s(v, 0)?)
                .bind(s(v, 1)?)
                .bind(ns(v, 2)?)
                .bind(ns(v, 3)?)
                .bind(s(v, 4)?)
                .bind(s(v, 5)?)
                .bind(s(v, 6)?)
                .bind(s(v, 7)?)
                .bind(s(v, 8)?)
                .bind(n(v, 9)?)
                .bind(n(v, 10)?)
                .execute(&mut *self.connection)
                .await?;
            }
            "cart_add_line" => {
                sqlx::query(
                    "INSERT INTO ecommerce_cart_lines\
                     (id,cart_id,listing_id,listing_version,product_id,quantity_milli,price_minor,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                     ON CONFLICT(cart_id,listing_id) DO UPDATE SET \
                     quantity_milli=ecommerce_cart_lines.quantity_milli+excluded.quantity_milli",
                )
                .bind(s(v, 0)?)
                .bind(s(v, 1)?)
                .bind(s(v, 2)?)
                .bind(n(v, 3)?)
                .bind(s(v, 4)?)
                .bind(n(v, 5)?)
                .bind(n(v, 6)?)
                .bind(n(v, 7)?)
                .execute(&mut *self.connection)
                .await?;
            }
            "cart_version_bump" => {
                sqlx::query(
                    "UPDATE ecommerce_carts SET version=version+1,updated_at=$1 WHERE id=$2",
                )
                .bind(n(v, 0)?)
                .bind(s(v, 1)?)
                .execute(&mut *self.connection)
                .await?;
            }
            "cart_abandon" => {
                let changed = sqlx::query(
                    "UPDATE ecommerce_carts SET status='abandoned',version=version+1,updated_at=$1 \
                     WHERE id=$2 AND status='open' AND version=$3 \
                     AND NOT EXISTS(SELECT 1 FROM ecommerce_checkouts WHERE cart_id=$2)",
                )
                .bind(n(v, 0)?)
                .bind(s(v, 1)?)
                .bind(n(v, 2)?)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(conflict(
                        "ECOMMERCE_CART_SOURCE_CHANGED",
                        "Cart cannot be abandoned.",
                    )
                    .into());
                }
            }
            "checkout_create" => {
                // values: [now, cartId, checkoutId, cartId, expectedVersion, orderId, 2, userId, now, now]
                sqlx::query("UPDATE ecommerce_carts SET status='checked_out',version=version+1,updated_at=$1 WHERE id=$2")
                    .bind(n(v, 0)?)
                    .bind(s(v, 1)?)
                    .execute(&mut *self.connection)
                    .await?;
                sqlx::query(
                    "INSERT INTO ecommerce_checkouts\
                     (id,cart_id,cart_version,sales_order_id,status,created_by,version,created_at,updated_at) \
                     VALUES($1,$2,$3,$4,'submitted',$5,1,$6,$7) ON CONFLICT(id) DO NOTHING",
                )
                .bind(s(v, 2)?)
                .bind(s(v, 3)?)
                .bind(n(v, 4)?)
                .bind(s(v, 5)?)
                .bind(s(v, 7)?)
                .bind(n(v, 8)?)
                .bind(n(v, 9)?)
                .execute(&mut *self.connection)
                .await?;
                let _ = n(v, 6)?; // fixed settlement version witness
            }
            "checkout_confirm" => {
                let changed = sqlx::query(
                    "UPDATE ecommerce_checkouts \
                     SET status='confirmed',sales_order_version=$1,version=version+1,updated_at=$2 \
                     WHERE id=$3 AND version=$4 AND status='submitted'",
                )
                .bind(n(v, 0)?)
                .bind(n(v, 1)?)
                .bind(s(v, 2)?)
                .bind(n(v, 3)?)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(conflict(
                        "ECOMMERCE_CHECKOUT_SOURCE_CHANGED",
                        "Checkout cannot be confirmed.",
                    )
                    .into());
                }
            }
            "checkout_fulfillment" => {
                // values: [status, salesOrderVersion, now, checkoutId, linksJson]
                let links: Vec<Vec<Value>> =
                    serde_json::from_value(v[4].clone()).map_err(|_| invalid())?;
                for link in links {
                    if link.len() != 6 {
                        return Err(invalid().into());
                    }
                    sqlx::query(
                        "INSERT INTO ecommerce_fulfillment_links\
                         (id,checkout_id,fulfillment_id,order_version,state,created_at) \
                         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(fulfillment_id) DO NOTHING",
                    )
                    .bind(link[0].as_str().ok_or_else(invalid)?)
                    .bind(link[1].as_str().ok_or_else(invalid)?)
                    .bind(link[2].as_str().ok_or_else(invalid)?)
                    .bind(link[3].as_i64().ok_or_else(invalid)?)
                    .bind(link[4].as_str().ok_or_else(invalid)?)
                    .bind(link[5].as_i64().ok_or_else(invalid)?)
                    .execute(&mut *self.connection)
                    .await?;
                }
                sqlx::query(
                    "UPDATE ecommerce_checkouts SET status=$1,sales_order_version=$2,\
                     version=version+1,updated_at=$3 WHERE id=$4",
                )
                .bind(s(v, 0)?)
                .bind(n(v, 1)?)
                .bind(n(v, 2)?)
                .bind(s(v, 3)?)
                .execute(&mut *self.connection)
                .await?;
            }
            "checkout_return" => {
                // values: [linkId, checkoutId, fulfillmentId, commerceReturnId,
                //          fulfillmentVersion, returnStatus, now, now, checkoutId]
                sqlx::query(
                    "INSERT INTO ecommerce_return_links\
                     (id,checkout_id,fulfillment_id,commerce_return_id,fulfillment_version,\
                     return_status,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) \
                     ON CONFLICT(id) DO NOTHING",
                )
                .bind(s(v, 0)?)
                .bind(s(v, 1)?)
                .bind(s(v, 2)?)
                .bind(s(v, 3)?)
                .bind(n(v, 4)?)
                .bind(s(v, 5)?)
                .bind(n(v, 6)?)
                .execute(&mut *self.connection)
                .await?;
                sqlx::query(
                    "UPDATE ecommerce_checkouts SET version=version+1,updated_at=$1 WHERE id=$2",
                )
                .bind(n(v, 7)?)
                .bind(s(v, 8)?)
                .execute(&mut *self.connection)
                .await?;
            }
            "channel_review" | "listing_review" => {
                let table = if write.kind == "channel_review" {
                    "ecommerce_channels"
                } else {
                    "ecommerce_listings"
                };
                let sql = format!(
                    "UPDATE {table} SET status=$1,reviewed_by=$2,version=version+1,updated_at=$3 WHERE id=$4"
                );
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(s(v, 0)?)
                    .bind(s(v, 1)?)
                    .bind(n(v, 2)?)
                    .bind(s(v, 3)?)
                    .execute(&mut *self.connection)
                    .await?;
            }
            _ => return Err(invalid().into()),
        }
        Ok(())
    }
}

impl OwnerWriteRepository for PgOwnerWrite<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> OwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("ecommerce-wave2:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> OwnerWriteFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(identity)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> OwnerWriteFuture<'a, Option<StoredReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(format!("wave2:{key}"))
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else { return Ok(None) };
            Ok(Some(StoredReceipt {
                request_hash: row.try_get("request_hash")?,
                result: Some(row.try_get("result_json")?),
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a LedgerEntry<'a>,
    ) -> OwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn apply<'a>(&'a mut self, write: &'a OwnerWrite) -> OwnerWriteFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.apply_owner(write).await
        })
    }
}
