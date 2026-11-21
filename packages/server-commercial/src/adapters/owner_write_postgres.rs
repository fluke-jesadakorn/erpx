//! PostgreSQL adapter for the native Commercial owner-write registry.
//!
//! The caller owns one serializable Holding/Control transaction. Only
//! Commercial-owned rows plus the shared command ledger are written here. Each
//! closed kind maps to fixed owner SQL with bound positional values (data only),
//! never to caller-supplied SQL text.
use crate::application::owner_write::{
    LedgerEntry, OwnerWriteFuture, OwnerWriteRepository, StoredReceipt,
};
use crate::domain::owner_write::OwnerWrite;
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
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_OWNER_WRITE_VALUE_INVALID",
        "Review the Commercial write values.",
    )
}

fn kind_sql(kind: &str) -> &'static str {
    match kind {
        "review_evidence" => {
            "INSERT INTO company_package_review_evidence\
            (change_set_id,company_id,actor_id,assignment_id,role_id,role_revision,\
            authority_revision,catalogue_hash,review_hash,reviewed_at,applied_at,comparison_json) \
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::jsonb) ON CONFLICT DO NOTHING"
        }
        "grant_revision_anchor" => {
            "INSERT INTO company_package_revisions(company_id,version) VALUES($1,0) ON CONFLICT DO NOTHING"
        }
        "change_set_receipt" => {
            "INSERT INTO company_package_change_sets(id,company_id,actor_id,request_hash,result_json,created_at) VALUES($1,$2,$3,$4,$5::jsonb,$6) ON CONFLICT(id) DO NOTHING"
        }
        "usage_snapshot_ensure" => {
            "INSERT INTO billing_usage_snapshots\
            (holding_id,month,snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,\
            seat_coverage,seat_source_count,unreconciled_originals,active_seat_count,active_seat_observed_at) \
            VALUES($1,$2,$3,$4,0,$5,$6,'unavailable',0,$7,$8,$9) ON CONFLICT(holding_id,month) DO NOTHING"
        }
        "usage_snapshot_finish" => {
            "UPDATE billing_usage_snapshots SET snapshot_id=$1,refreshed_at=$2,\
            source_count=$3,ai_coverage=$4,storage_coverage=$5,seat_coverage=$6,seat_source_count=$7,\
            unreconciled_originals=$8,active_seat_count=$9,active_seat_observed_at=$10 \
            WHERE holding_id=$11 AND month=$12"
        }
        // values: 24 row/coverage fields + snapshotId at $25, then 3 WHERE keys ($26-$28).
        "usage_meter" => {
            "INSERT INTO billing_usage_meters\
            (holding_id,month,source_key,company_id,company_name,branch_id,branch_name,meter_kind,\
            funding_source,pricing_status,quantity,input_tokens,output_tokens,average_daily_bytes,\
            active_at_period_end,snapshot_id,request_status,gateway_status,provider,model,\
            units_json,cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,\
            active_from,active_until,byte_milliseconds) \
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,\
            $21::jsonb,$22,$23,$24,$25,$26,$27,$28::text) \
            ON CONFLICT(holding_id,month,source_key) DO UPDATE SET \
            company_id=excluded.company_id,company_name=excluded.company_name,branch_id=excluded.branch_id,\
            branch_name=excluded.branch_name,meter_kind=excluded.meter_kind,funding_source=excluded.funding_source,\
            pricing_status=excluded.pricing_status,quantity=excluded.quantity,input_tokens=excluded.input_tokens,\
            output_tokens=excluded.output_tokens,average_daily_bytes=excluded.average_daily_bytes,\
            active_at_period_end=excluded.active_at_period_end,snapshot_id=excluded.snapshot_id,\
            request_status=excluded.request_status,gateway_status=excluded.gateway_status,provider=excluded.provider,\
            model=excluded.model,units_json=excluded.units_json,cost_status=excluded.cost_status,\
            verified_cost_satang=excluded.verified_cost_satang,cost_evidence_key=excluded.cost_evidence_key,\
            cost_reconciled_at=excluded.cost_reconciled_at,active_from=excluded.active_from,\
            active_until=excluded.active_until,byte_milliseconds=excluded.byte_milliseconds"
        }
        "usage_meter_prune" => {
            "DELETE FROM billing_usage_meters WHERE holding_id=$1 AND month=$2 AND snapshot_id<>$3"
        }
        "usage_seat" => {
            "INSERT INTO billing_seat_usage(holding_id,month,user_key,intervals_json,snapshot_id) VALUES($1,$2,$3,$4::jsonb,$5) ON CONFLICT(holding_id,month,user_key) DO UPDATE SET intervals_json=excluded.intervals_json,snapshot_id=excluded.snapshot_id"
        }
        "usage_seat_prune" => {
            "DELETE FROM billing_seat_usage WHERE holding_id=$1 AND month=$2 AND snapshot_id<>$3"
        }
        "purchase_assertion" => {
            "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,$2,$3) ON CONFLICT DO NOTHING"
        }
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
            return Err(
                PersistenceError::new("COMMERCIAL_OWNER_WRITE_TRANSACTION_REQUIRED").into(),
            );
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(
                PersistenceError::new("COMMERCIAL_OWNER_WRITE_SERIALIZABLE_REQUIRED").into(),
            );
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
        .bind(format!("owner-write:{}", entry.key))
        .bind(entry.operation)
        .bind(entry.actor_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(entry.result)
        .bind(entry.now)
        .bind(&authority)
        .execute(&mut *self.connection).await?.rows_affected();
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
        let payload = serde_json::json!({"companyId": &self.company, "recordId": entry.record_id, "result": entry.result});
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,1,$5,$6)",
        )
        .bind(entry.key).bind(entry.operation).bind(entry.record_type)
        .bind(entry.record_id).bind(&payload).bind(entry.now)
        .execute(&mut *self.connection).await?;
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
                .bind(format!("commercial-owner-write:{key}"))
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
            .bind(format!("owner-write:{key}"))
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
