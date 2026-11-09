//! PostgreSQL adapter for the native Billing target evidence commands.
//!
//! SQL is Billing-owned Control SQL. The caller's serializable transaction is
//! required; no commit, Stripe call, rollout approval, rating or invoice occurs.
use crate::application::BillingFuture;
use crate::application::cost::{AccountSnapshot, AuditEntry, ReceiptSnapshot};
use crate::application::target_evidence_command::TargetEvidenceRepository;
use crate::domain::actor::BillingActor;
use crate::domain::target_evidence_command::{
    ValidatedCapacity, ValidatedOperatingCost, ValidatedTargetExtra,
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

pub struct PgTargetEvidenceRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgTargetEvidenceRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl TargetEvidenceRepository for PgTargetEvidenceRepository<'_> {
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

    fn insert_operating_cost<'a>(
        &'a mut self,
        draft: &'a ValidatedOperatingCost,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let _ = reason;
            let row = sqlx::query(
                "INSERT INTO billing_target_operating_costs\
                 (holding_id,month,category,amount_satang,evidence_id,verified_by,verified_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING RETURNING holding_id",
            )
            .bind(holding.as_str())
            .bind(&draft.target_month)
            .bind(&draft.category)
            .bind(draft.amount_satang)
            .bind(&draft.evidence_id)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        })
    }

    fn insert_capacity<'a>(
        &'a mut self,
        draft: &'a ValidatedCapacity,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "INSERT INTO billing_target_capacity_reviews\
                 (holding_id,effective_month,projected_original_bytes,primary_available_bytes,\
                  backup_available_bytes,reserved_bytes,company_limit_bytes,evidence_id,\
                  approved_by,reason,approved_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
                 ON CONFLICT DO NOTHING RETURNING holding_id",
            )
            .bind(holding.as_str())
            .bind(&draft.target_month)
            .bind(draft.projected_original_bytes)
            .bind(draft.primary_available_bytes)
            .bind(draft.backup_available_bytes)
            .bind(draft.reserved_bytes)
            .bind(draft.company_limit_bytes)
            .bind(&draft.evidence_id)
            .bind(actor.user_id.as_str())
            .bind(reason)
            .bind(now.get())
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.is_some())
        })
    }

    fn extra_target_assignment<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_target_assignments \
                 WHERE holding_id=$1 AND effective_month<=$2)",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn approved_extra_period<'a>(
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

    fn insert_target_extra<'a>(
        &'a mut self,
        draft: &'a ValidatedTargetExtra,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "INSERT INTO billing_target_extras\
                 (id,holding_id,month,kind,description,amount_satang,service_start,service_end,\
                  evidence_id,approved_by,approval_reason,approved_at) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
                 ON CONFLICT DO NOTHING RETURNING id",
            )
            .bind(holding.as_str())
            .bind(&draft.target_month)
            .bind(&draft.extra_kind)
            .bind(&draft.note)
            .bind(draft.amount_satang)
            .bind(draft.service_start)
            .bind(draft.service_end)
            .bind(&draft.evidence_id)
            .bind(actor.user_id.as_str())
            .bind(reason)
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

pub async fn execute_target_evidence(
    connection: &mut PgConnection,
    actor: &BillingActor,
    target: &HoldingId,
    command: &crate::domain::target_evidence_command::BillingTargetEvidenceCommand,
    now: TimestampMillis,
) -> Result<crate::domain::command::CommandReceipt, Error> {
    crate::application::target_evidence_command::execute_target_evidence(
        actor,
        target,
        command,
        &mut PgTargetEvidenceRepository::new(connection),
        now,
    )
    .await
}
