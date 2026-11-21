//! PostgreSQL adapter for native Sales customer-notice reconciliation.
//!
//! The caller owns the retained serializable Holding transaction. This adapter
//! reads the current Commerce/Finance/provider source image (byte-identical to
//! the delivery worker hash), preserves the immutable attempt, and writes one
//! reconciliation, receipt, audit and outbox unit before deferred constraints,
//! current reviewer authority and the settlement capability are re-checked.
use crate::adapters::sales_notification_execution::{
    AUTHORITY, RELEASE_READY, SOURCE_IMAGE,
};
use crate::application::sales_notification_reconciliation::{
    NotificationReconciliationRepository, ReconcileFuture, ReconcilePlan,
    ReconcileReceiptSnapshot,
};
use crate::domain::sales_notification_execution::NotificationSource;
use crate::domain::sales_notification_reconciliation::{
    OPERATION, ReconcileActor, ReconcileAttempt, authority_changed, source_changed,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PersistenceError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgNotificationReconciliation<'c> {
    connection: &'c mut PgConnection,
    mode: PackageEnforcementMode,
}

impl<'c> PgNotificationReconciliation<'c> {
    pub fn new(connection: &'c mut PgConnection, mode: PackageEnforcementMode) -> Self {
        Self { connection, mode }
    }

    fn ready(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("SALES_NOTIFICATION_RECONCILIATION_TRANSACTION_REQUIRED")
                    .into(),
            );
        }
        Ok(())
    }

    async fn load_source(&mut self, notification_id: &str) -> Result<Option<NotificationSource>, Error> {
        let sql = format!(
            "SELECT n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,\
             n.party_id,n.recipient_email,n.subject,n.body_text,n.adapter,n.status,n.version,\
             n.attempts,n.next_attempt_at,n.lease_until,n.provider_message_id,n.created_by,\
             delivery.department_id,delivery.order_state,delivery.company_id source_company,\
             customer.company_id customer_company,customer.status customer_status,\
             customer.notification_email,customer.notification_consent,\
             settings.adapter current_adapter,{RELEASE_READY} release_ready,\
             encode(sha256(convert_to({SOURCE_IMAGE},'UTF8')),'hex') source_hash \
             FROM commerce_customer_notifications n \
             LEFT JOIN (SELECT f.id fulfillment_id,o.id order_id,o.company_id,o.branch_id,\
               o.department_id,o.kind order_kind,o.state order_state,o.currency,o.party_id \
               FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.kind='sales') delivery \
               ON delivery.fulfillment_id=n.fulfillment_id AND delivery.order_id=n.order_id \
               AND delivery.company_id=n.company_id AND delivery.party_id=n.party_id \
             LEFT JOIN (SELECT id,company_id,kind,version,status,notification_email,\
               notification_consent FROM finance_parties WHERE kind='customer') customer \
               ON customer.id=n.party_id AND customer.company_id=n.company_id \
             LEFT JOIN commerce_customer_notification_settings settings \
               ON settings.company_id=n.company_id \
             WHERE n.id=$1"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(notification_id)
            .bind(false)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            Ok::<_, sqlx::Error>(NotificationSource {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                order_id: row.try_get("order_id")?,
                order_state: row.try_get("order_state")?,
                fulfillment_id: row.try_get("fulfillment_id")?,
                party_id: row.try_get("party_id")?,
                recipient_email: row.try_get("recipient_email")?,
                subject: row.try_get("subject")?,
                body_text: row.try_get("body_text")?,
                adapter: row.try_get("adapter")?,
                current_adapter: row.try_get("current_adapter")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                attempts: row.try_get("attempts")?,
                next_attempt_at: row.try_get("next_attempt_at")?,
                lease_until: row.try_get("lease_until")?,
                provider_message_id: row.try_get("provider_message_id")?,
                created_by: row.try_get("created_by")?,
                source_company: row.try_get("source_company")?,
                customer_company: row.try_get("customer_company")?,
                customer_status: row.try_get("customer_status")?,
                notification_email: row.try_get("notification_email")?,
                notification_consent: row.try_get("notification_consent")?,
                release_ready: row.try_get("release_ready")?,
                source_hash: row.try_get("source_hash")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn load_attempt(
        &mut self,
        notification_id: &str,
        attempt_id: &str,
        company_id: &str,
    ) -> Result<Option<ReconcileAttempt>, Error> {
        let row = sqlx::query(
            "SELECT attempt.id,attempt.notification_id,attempt.company_id,\
             attempt.authorization_command_id,attempt.source_hash,attempt.state,\
             attempt.provider_message_id,attempt.attempt,attempt.created_at,attempt.updated_at,\
             authority.actor_id \
             FROM sales_notification_execution_attempts attempt \
             JOIN sales_notification_execution_authorizations authority \
               ON authority.command_id=attempt.authorization_command_id \
              AND authority.company_id=attempt.company_id \
              AND authority.notification_id=attempt.notification_id \
             WHERE attempt.id=$1 AND attempt.notification_id=$2 AND attempt.company_id=$3",
        )
        .bind(attempt_id)
        .bind(notification_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok::<_, sqlx::Error>(ReconcileAttempt {
                id: row.try_get("id")?,
                notification_id: row.try_get("notification_id")?,
                company_id: row.try_get("company_id")?,
                authorization_command_id: row.try_get("authorization_command_id")?,
                actor_id: row.try_get("actor_id")?,
                source_hash: row.try_get("source_hash")?,
                state: row.try_get("state")?,
                provider_message_id: row.try_get("provider_message_id")?,
                attempt: row.try_get("attempt")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn capability(
        &mut self,
        company_id: &str,
        purpose: CapabilityPurpose,
        now: i64,
    ) -> Result<(), Error> {
        if self.mode == PackageEnforcementMode::Disabled {
            return Ok(());
        }
        let catalogue = current_package_catalogue()?;
        let company = CompanyId::new(company_id)?;
        PgCapabilityDecisionReader::new(company, self.connection, &catalogue)
            .require(
                "sales.orders",
                purpose,
                self.mode,
                TimestampMillis::new(now),
            )
            .await?;
        Ok(())
    }

    async fn authority(&mut self, actor: &ReconcileActor, now: i64) -> Result<bool, Error> {
        Ok(sqlx::query_scalar(AUTHORITY)
            .bind(&actor.assignment_id)
            .bind(&actor.user_id)
            .bind(actor.role.as_deref())
            .bind(&actor.company_id)
            .bind(now)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.session_id.as_deref())
            .bind(actor.assignment_version)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn readback(&mut self, plan: &ReconcilePlan<'_>) -> Result<bool, Error> {
        let source = self.load_source(plan.notification_id).await?;
        let notification = source.is_some_and(|source| {
            source.status == plan.outcome
                && source.version == plan.version
                && source.provider_message_id.as_deref() == Some(plan.observation.id.as_str())
                && source.source_hash == plan.attempt.source_hash
        });
        let attempt: Option<String> = sqlx::query_scalar(
            "SELECT provider_message_id FROM sales_notification_execution_attempts \
             WHERE id=$1 AND notification_id=$2 AND company_id=$3 AND state='unknown'",
        )
        .bind(&plan.attempt.id)
        .bind(plan.notification_id)
        .bind(&plan.actor.company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let reconciliation: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sales_notification_reconciliations \
             WHERE id=$1 AND command_id=$2 AND notification_id=$3 AND attempt_id=$4 \
               AND company_id=$5 AND source_hash=$6 AND provider_message_id=$7 \
               AND outcome=$8 AND notification_version=$9)",
        )
        .bind(&plan.event_id)
        .bind(plan.key)
        .bind(plan.notification_id)
        .bind(&plan.attempt.id)
        .bind(&plan.actor.company_id)
        .bind(&plan.attempt.source_hash)
        .bind(&plan.observation.id)
        .bind(plan.outcome)
        .bind(plan.version)
        .fetch_one(&mut *self.connection)
        .await?;
        Ok(notification
            && attempt.as_deref() == Some(plan.observation.id.as_str())
            && reconciliation)
    }
}

impl NotificationReconciliationRepository for PgNotificationReconciliation<'_> {
    type Error = Error;

    fn lock_command<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ReconcileFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("sales-notification-reconciliation:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> ReconcileFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            Ok(sqlx::query_scalar(
                "SELECT encode(sha256(convert_to($1,'UTF8')),'hex')",
            )
            .bind(value)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ReconcileFuture<'a, Option<ReconcileReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(ReconcileReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn reconciliation_witness<'a>(
        &'a mut self,
        key: &'a str,
        notification_id: &'a str,
        attempt_id: &'a str,
        actor_id: &'a str,
        source_hash: &'a str,
    ) -> ReconcileFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sales_notification_reconciliations \
                 WHERE command_id=$1 AND notification_id=$2 AND attempt_id=$3 \
                   AND actor_id=$4 AND source_hash=$5)",
            )
            .bind(key)
            .bind(notification_id)
            .bind(attempt_id)
            .bind(actor_id)
            .bind(source_hash)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn source<'a>(
        &'a mut self,
        notification_id: &'a str,
    ) -> ReconcileFuture<'a, Option<NotificationSource>, Self::Error> {
        Box::pin(async move { self.load_source(notification_id).await })
    }

    fn attempt<'a>(
        &'a mut self,
        notification_id: &'a str,
        attempt_id: &'a str,
        company_id: &'a str,
    ) -> ReconcileFuture<'a, Option<ReconcileAttempt>, Self::Error> {
        Box::pin(async move { self.load_attempt(notification_id, attempt_id, company_id).await })
    }

    fn settle_capability<'a>(
        &'a mut self,
        company_id: &'a str,
        history: bool,
        now: i64,
    ) -> ReconcileFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            self.capability(
                company_id,
                if history {
                    CapabilityPurpose::History
                } else {
                    CapabilityPurpose::SettleExisting
                },
                now,
            )
            .await
        })
    }

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a ReconcileActor,
        now: i64,
    ) -> ReconcileFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.authority(actor, now).await })
    }

    fn commit<'a>(
        &'a mut self,
        plan: &'a ReconcilePlan<'a>,
        now: i64,
    ) -> ReconcileFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE commerce_customer_notifications SET status=$1,version=version+1,\
                 lease_until=NULL,last_error=NULL,\
                 sent_at=CASE WHEN $1='sent' THEN $2 ELSE sent_at END,updated_at=$2 \
                 WHERE id=$3 AND company_id=$4 AND version=$5 AND status='unknown' \
                   AND provider_message_id=$6 RETURNING id",
            )
            .bind(plan.outcome)
            .bind(now)
            .bind(plan.notification_id)
            .bind(&plan.actor.company_id)
            .bind(plan.input.expected_version)
            .bind(&plan.observation.id)
            .fetch_optional(&mut *self.connection)
            .await?;
            if changed.is_none() {
                return Err(crate::domain::sales_notification_reconciliation::version_conflict().into());
            }
            let observation = plan.observation.wire().to_string();
            sqlx::query(
                "INSERT INTO sales_notification_reconciliations\
                 (id,command_id,notification_id,attempt_id,company_id,actor_id,assignment_id,\
                 expected_version,notification_version,source_hash,provider_message_id,\
                 provider_event,outcome,provider_observation_json,provider_observed_at,reason,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
            )
            .bind(&plan.event_id)
            .bind(plan.key)
            .bind(plan.notification_id)
            .bind(&plan.attempt.id)
            .bind(&plan.actor.company_id)
            .bind(&plan.actor.user_id)
            .bind(&plan.actor.assignment_id)
            .bind(plan.input.expected_version)
            .bind(plan.version)
            .bind(&plan.attempt.source_hash)
            .bind(&plan.observation.id)
            .bind(plan.observation.last_event.as_str())
            .bind(plan.outcome)
            .bind(observation)
            .bind(plan.observation.created_at)
            .bind(&plan.input.reason)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            let result = plan.receipt.wire().to_string();
            let authority = plan.actor.authority_json();
            sqlx::query(
                "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,\
                 record_id,result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(plan.key)
            .bind(OPERATION)
            .bind(&plan.actor.user_id)
            .bind(plan.request_hash)
            .bind(plan.notification_id)
            .bind(&result)
            .bind(now)
            .bind(&authority)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,\
                 record_type,record_id,record_version,purpose,created_at,authority_json) \
                 VALUES($1,$2,$3,$4,$5,'commerce_customer_notification',$6,$7,$8,$9,$10)",
            )
            .bind(format!("audit-{}", plan.event_id))
            .bind(plan.key)
            .bind(&plan.actor.user_id)
            .bind(&plan.actor.assignment_id)
            .bind(OPERATION)
            .bind(plan.notification_id)
            .bind(plan.version)
            .bind(&plan.input.reason)
            .bind(now)
            .bind(&authority)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,\
                 aggregate_id,aggregate_version,payload_json,created_at) \
                 VALUES($1,$2,$3,'commerce_customer_notification',$4,$5,$6,$7)",
            )
            .bind(format!("outbox-{}", plan.event_id))
            .bind(plan.key)
            .bind(OPERATION)
            .bind(plan.notification_id)
            .bind(plan.version)
            .bind(&result)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
                .execute(&mut *self.connection)
                .await?;
            if !self.authority(plan.actor, now).await? {
                return Err(authority_changed().into());
            }
            self.capability(
                &plan.actor.company_id,
                CapabilityPurpose::SettleExisting,
                now,
            )
            .await?;
            if !self.readback(plan).await? {
                return Err(source_changed().into());
            }
            Ok(())
        })
    }
}
