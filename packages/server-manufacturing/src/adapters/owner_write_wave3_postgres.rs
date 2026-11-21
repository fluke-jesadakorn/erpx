//! PostgreSQL adapter for the native Manufacturing owner-write registry.
//!
//! The caller owns one serializable Holding transaction. This adapter writes
//! only Manufacturing-owned rows plus the shared command ledger. All SQL for
//! the retired TypeScript literals lives here; no caller supplies SQL text.
use crate::application::owner_write_wave3::{
    LedgerEntry, OwnerWriteFuture, OwnerWriteRepository, StoredReceipt,
};
use crate::domain::owner_write_wave3::{OwnerWrite, invalid};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command was already used.",
    )
}

/// The authoritative owner SQL for every retired Manufacturing write literal.
fn kind_sql(kind: &str) -> &'static str {
    match kind {
        "posting" => "INSERT INTO manufacturing_order_postings\
             (id,company_id,order_id,source_order_version,phase,profile_id,profile_version,\
             journal_id,currency,amount_minor,account_snapshot_json,created_by,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,'THB',$9,$10,$11,$12)",
        "cost_movement" => "INSERT INTO manufacturing_cost_movements\
             (id,company_id,order_id,movement_kind,quantity_milli,amount_minor,source_type,\
             source_id,journal_id,occurred_on,version,created_by,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,'production_event',$7,$8,$9,1,$10,$11)",
        "order_create" => "INSERT INTO manufacturing_orders\
             (id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,\
             specification_version,planned_quantity_milli,status,created_by,created_at,updated_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,'planned',$9,$10,$11)",
        "order_release_event" => "INSERT INTO manufacturing_order_events \
             VALUES($1,$2,1,'release',$3,$4,'planned','{}',$5)",
        "order_reservation" => "INSERT INTO manufacturing_material_reservations \
             VALUES($1,$2,$3,$4,$5,0,0,$6,$7,$8)",
        "order_output_report" => "UPDATE manufacturing_orders SET reported_output_milli=$1,\
             reported_scrap_milli=$2,labor_cost_minor=$3,overhead_cost_minor=$4 WHERE id=$5",
        "order_operation_event" => "INSERT INTO manufacturing_operation_events\
             (id,order_id,operation_id,action,source_order_version,labor_minutes,machine_minutes,\
             evidence,created_by,created_at) VALUES($1,$2,$3,'complete',$4,$5,$6,$7,$8,$9)",
        "reservation_release" => "UPDATE manufacturing_material_reservations \
             SET released_milli=quantity_milli-consumed_milli WHERE order_id=$1",
        "order_transition" => "UPDATE manufacturing_orders SET status=$1,\
             version=version+1,updated_at=$2 WHERE id=$3",
        "order_event" => "INSERT INTO manufacturing_order_events \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        _ => "",
    }
}

fn bind_all<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    values: &'q [Value],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    for value in values {
        query = match value {
            Value::String(text) => query.bind(text.as_str()),
            Value::Number(number) => query.bind(number.as_i64().unwrap_or(0)),
            Value::Bool(flag) => query.bind(*flag),
            Value::Null => query.bind(Option::<i64>::None),
            _ => query.bind(value.to_string()),
        };
    }
    query
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
            return Err(PersistenceError::new("MANUFACTURING_OWNER_WRITE_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("MANUFACTURING_OWNER_WRITE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &LedgerEntry<'_>) -> Result<(), Error> {
        let authority = serde_json::json!({"companyId": &self.company, "actorId": entry.actor_id});
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(format!("mfg-owner-write:{}", entry.key))
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
            return Err(conflict().into());
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
            "kind": entry.record_type,
            "result": entry.result,
        });
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,1,$5,$6)",
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
        let sql = kind_sql(write.kind);
        if sql.is_empty() {
            return Err(invalid().into());
        }
        bind_all(sqlx::query(sql), &write.values)
            .execute(&mut *self.connection)
            .await?;
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
                .bind(format!("manufacturing-owner-write:{key}"))
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
            .bind(format!("mfg-owner-write:{key}"))
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let result = row.try_get::<Option<String>, _>("result_json")?;
            Ok(Some(StoredReceipt {
                request_hash: row.try_get("request_hash")?,
                result: result.map(|text| serde_json::from_str(&text).unwrap_or(Value::Null)),
            }))
        })
    }

    fn ledger<'a>(&'a mut self, entry: &'a LedgerEntry<'a>) -> OwnerWriteFuture<'a, (), Self::Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_exact_sql_and_placeholder_count() {
        let cases = [
            ("posting", 12),
            ("cost_movement", 11),
            ("order_create", 11),
            ("order_release_event", 5),
            ("order_reservation", 8),
            ("order_output_report", 5),
            ("order_operation_event", 9),
            ("reservation_release", 1),
            ("order_transition", 3),
            ("order_event", 9),
        ];
        for (kind, count) in cases {
            let sql = kind_sql(kind);
            assert!(!sql.is_empty(), "missing SQL for {kind}");
            let mut n = 1;
            while sql.contains(format!("${n}").as_str()) {
                n += 1;
            }
            assert_eq!(n - 1, count, "placeholder mismatch for {kind}");
        }
        assert!(kind_sql("bogus").is_empty());
    }
}
