//! PostgreSQL adapter for the native Billing `assign-target` command.
//!
//! SQL is Billing-owned Control SQL. The caller's serializable transaction is
//! required; no commit, Stripe call, rating, invoice or Holding write occurs.
use crate::application::BillingFuture;
use crate::application::cost::{AccountSnapshot, AuditEntry, ReceiptSnapshot};
use crate::application::target_command::AssignTargetRepository;
use crate::domain::actor::BillingActor;
use crate::domain::target_command::{TargetCommandPrice, ValidatedAssignment};
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

pub struct PgAssignTargetRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgAssignTargetRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl AssignTargetRepository for PgAssignTargetRepository<'_> {
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

    fn current_price<'a>(
        &'a mut self,
        month: &'a str,
    ) -> BillingFuture<'a, Option<TargetCommandPrice>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT id,effective_month FROM billing_target_prices \
                 WHERE effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1",
            )
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| TargetCommandPrice {
                id: row.get("id"),
                effective_month: row.get("effective_month"),
            }))
        })
    }

    fn selected_price<'a>(
        &'a mut self,
        id: &'a str,
        month: &'a str,
    ) -> BillingFuture<'a, Option<TargetCommandPrice>, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "SELECT id,effective_month FROM billing_target_prices \
                 WHERE id=$1 AND effective_month<=$2",
            )
            .bind(id)
            .bind(month)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(|row| TargetCommandPrice {
                id: row.get("id"),
                effective_month: row.get("effective_month"),
            }))
        })
    }

    fn sandbox_designated<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1)",
            )
            .bind(holding.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn approved_rollout<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM billing_target_rollout_approvals \
                 WHERE holding_id=$1 AND effective_month<=$2)",
            )
            .bind(holding.as_str())
            .bind(month)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn insert_assignment<'a>(
        &'a mut self,
        assignment: &'a ValidatedAssignment,
        holding: &'a HoldingId,
        price_id: &'a str,
        service_started_at: i64,
        actor: &'a BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let row = sqlx::query(
                "INSERT INTO billing_target_assignments\
                 (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING holding_id",
            )
            .bind(holding.as_str())
            .bind(&assignment.target_month)
            .bind(price_id)
            .bind(&assignment.target_mode)
            .bind(service_started_at)
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

pub async fn assign_target(
    connection: &mut PgConnection,
    actor: &BillingActor,
    target: &HoldingId,
    command: &crate::domain::target_command::AssignTarget,
    now: TimestampMillis,
) -> Result<crate::domain::command::CommandReceipt, Error> {
    crate::application::target_command::execute_assign_target(
        actor,
        target,
        command,
        &mut PgAssignTargetRepository::new(connection),
        now,
    )
    .await
}
