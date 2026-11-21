//! PostgreSQL adapter for the native Billing target provider evidence commands.
//! SQL is Billing-owned Control SQL; caller owns the serializable transaction.
use crate::application::BillingFuture;
use crate::application::cost::{AccountSnapshot, AuditEntry, ReceiptSnapshot};
use crate::application::target_provider_command::TargetProviderRepository;
use crate::domain::actor::BillingActor;
use crate::domain::target_provider_command::{
    ProviderMeter, ValidatedProviderCost, ValidatedStatement,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("BILLING_COMMAND_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("BILLING_COMMAND_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgTargetProviderRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgTargetProviderRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl TargetProviderRepository for PgTargetProviderRepository<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> BillingFuture<'a, String, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn lock_holding<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("billing:{}", holding.as_str()))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> BillingFuture<'a, (), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("billing-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query("SELECT digest,result_json FROM billing_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(ReceiptSnapshot {
                    digest: row.try_get("digest")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }

    fn holding_exists<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM holding_bindings WHERE holding_id=$1)",
            )
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn account_for_update<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<AccountSnapshot>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row =
                sqlx::query("SELECT version FROM billing_accounts WHERE holding_id=$1 FOR UPDATE")
                    .bind(holding.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?;
            row.map(|row| {
                Ok::<_, Error>(AccountSnapshot {
                    version: i64::from(row.try_get::<i32, _>("version")?),
                })
            })
            .transpose()
        })
    }

    fn successful_platform_meter<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
        source_key: &'a str,
    ) -> BillingFuture<'a, Option<ProviderMeter>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT provider,cost_status,cost_reconciled_at::text cost_reconciled_at,\
                 verified_cost_satang::text verified_cost_satang FROM billing_usage_meters \
                 WHERE holding_id=$1 AND month=$2 AND source_key=$3 \
                 AND meter_kind='ai' AND funding_source='erpx' AND request_status='succeeded'",
            )
            .bind(holding.as_str())
            .bind(month)
            .bind(source_key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(ProviderMeter {
                    provider: row.try_get("provider")?,
                    cost_status: row.try_get("cost_status")?,
                    cost_reconciled_at: row.try_get("cost_reconciled_at")?,
                    verified_cost_satang: row.try_get("verified_cost_satang")?,
                })
            })
            .transpose()
        })
    }

    fn available_evidence<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_evidence \
                 WHERE id=$1 AND holding_id=$2 AND source_state='available')",
            )
            .bind(id)
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn approved_period<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_target_rated_periods \
                 WHERE holding_id=$1 AND month=$2 AND state='approved')",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn existing_provider_cost<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
        source_key: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_target_provider_costs \
                 WHERE holding_id=$1 AND month=$2 AND source_key=$3)",
            )
            .bind(holding.as_str())
            .bind(month)
            .bind(source_key)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn insert_provider_cost<'a>(
        &'a mut self,
        draft: &'a ValidatedProviderCost,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "INSERT INTO billing_target_provider_costs\
                 (holding_id,month,source_key,provider,statement_id,original_currency,\
                  original_minor,fx_numerator,fx_denominator,provider_cost_satang,evidence_id,\
                  verified_by,verified_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) \
                 ON CONFLICT DO NOTHING RETURNING holding_id",
            )
            .bind(holding.as_str())
            .bind(&draft.target_month)
            .bind(&draft.source_key)
            .bind(&draft.provider)
            .bind(&draft.statement_id)
            .bind(&draft.original_currency)
            .bind(draft.original_minor)
            .bind(draft.fx_numerator)
            .bind(draft.fx_denominator)
            .bind(draft.provider_cost_satang)
            .bind(&draft.evidence_id)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        })
    }

    fn provider_statement_total<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
        provider: &'a str,
        statement_id: &'a str,
        currency: &'a str,
    ) -> BillingFuture<'a, (i64, i128), Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row: (i64, String) = sqlx::query_as(
                "SELECT COUNT(*)::bigint,COALESCE(SUM(original_minor),0)::text \
                 FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 \
                 AND provider=$3 AND statement_id=$4 AND original_currency=$5",
            )
            .bind(holding.as_str())
            .bind(month)
            .bind(provider)
            .bind(statement_id)
            .bind(currency)
            .fetch_one(&mut *self.connection)
            .await?;
            let total = row.1.parse::<i128>().map_err(|_| {
                BusinessError::invalid("BILLING_SOURCE_INVALID", "Invalid statement total.")
            })?;
            Ok((row.0, total))
        })
    }

    fn insert_statement<'a>(
        &'a mut self,
        draft: &'a ValidatedStatement,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "INSERT INTO billing_target_statements\
                 (holding_id,month,provider,statement_id,evidence_id,provider_total_minor,\
                  original_currency,reconciled_total_minor,verified_by,verified_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$6,$8,$9) \
                 ON CONFLICT DO NOTHING RETURNING holding_id",
            )
            .bind(holding.as_str())
            .bind(&draft.target_month)
            .bind(&draft.provider)
            .bind(&draft.statement_id)
            .bind(&draft.evidence_id)
            .bind(draft.provider_total_minor)
            .bind(&draft.original_currency)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        })
    }

    fn increment_account_version<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        expected_version: i64,
    ) -> BillingFuture<'a, Option<i64>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "UPDATE billing_accounts SET version=version+1 \
                 WHERE holding_id=$1 AND version=$2 RETURNING version",
            )
            .bind(holding.as_str())
            .bind(expected_version)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| i64::from(row.get::<i32, _>("version"))))
        })
    }

    fn insert_audit<'a>(
        &'a mut self,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_audit(id,holding_id,actor,action,reason,details_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(format!("billing-audit-{}", entry.key))
            .bind(entry.target.as_str())
            .bind(entry.actor.user_id.as_str())
            .bind(entry.action)
            .bind(entry.reason)
            .bind(entry.details_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        digest: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let changed = sqlx::query(
                "INSERT INTO billing_receipts(id,digest,result_json,created_at) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(id) DO NOTHING",
            )
            .bind(key)
            .bind(digest)
            .bind(result_json)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn serialized_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query("SELECT digest,result_json FROM billing_receipts WHERE id=$1")
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, Error>(ReceiptSnapshot {
                    digest: row.try_get("digest")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        })
    }
}

pub async fn execute_target_provider(
    connection: &mut PgConnection,
    actor: &BillingActor,
    target: &HoldingId,
    command: &crate::domain::target_provider_command::BillingTargetProviderCommand,
    now: TimestampMillis,
) -> Result<crate::domain::command::CommandReceipt, Error> {
    crate::application::target_provider_command::execute_target_provider(
        actor,
        target,
        command,
        &mut PgTargetProviderRepository::new(connection),
        now,
    )
    .await
}
