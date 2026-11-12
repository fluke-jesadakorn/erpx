//! PostgreSQL adapter for the native Sales customer-notice delivery worker.
//!
//! SQL is Sales-owned (`commerce_customer_notifications`,
//! `sales_notification_execution_authorizations`,
//! `sales_notification_execution_attempts`) over the Commerce fulfillment,
//! Finance customer and Identity queued-command sources. Each phase opens its
//! own serializable unit: the durable attempt commits before provider I/O and
//! the terminal state is committed only after the exact lease/source re-check.
use crate::application::sales_notification_execution::{
    DeliveryStage, DispatchOutcome, DispatchPlan, DispatchTrace, EmailDispatchPort,
    ExecutionFuture, NotificationExecutionUnit, OutboundEmail, sandbox_provider_id,
    uncertain_attempt,
};
use crate::domain::sales_notification_execution::{
    ClaimDecision, ExecutionAttempt, ExecutionAuthorization, HELD_UNTIL, LEASE_MILLIS,
    NotificationSource, QueuedActor, TRANSACTION_REQUIRED, attempt_current, attempt_uncertain,
    authority_changed, authorization_current, claim_decision, invalid_reason, lease_changed,
    parse_queued_actor, source_changed, terminal_code,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

type Error = PgTransactionError<BusinessError>;

/// Current Commerce delivery + Finance customer + provider source image. The
/// `image` array is byte-identical to the retained TypeScript source hash so
/// existing immutable attempts and authorizations stay valid.
pub(crate) const SOURCE_IMAGE: &str = "jsonb_build_array(\
 n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,n.party_version,\
 n.recipient_email,n.subject,n.body_text,n.adapter,n.created_by,n.created_at,\
 delivery.fulfillment_id,delivery.order_id,delivery.company_id,delivery.branch_id,\
 delivery.department_id,delivery.party_id,delivery.order_kind,\
 customer.id,customer.company_id,customer.kind)::text";

/// Development keeps the retained `TRUE` predicate. Sales is not a release
/// module, so production selects only the enabled, unexpired isolated
/// engineering-verification context exactly like the retained scheduled gate.
pub(crate) const RELEASE_READY: &str = "(NOT $2::boolean \
 OR EXISTS(SELECT 1 FROM engineering_verification_contexts e WHERE e.company_id=n.company_id \
   AND e.enabled AND e.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))";

pub(crate) const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments g \
 WHERE g.id=$1 AND g.user_id=$2 AND ($3::text IS NULL OR g.role=$3) AND g.company_id=$4 \
 AND iam_assignment_active(g.id,$5) AND iam_principal_active(g.user_id,g.company_id,$5) \
 AND iam_role_allows(g.role,g.company_id,'commerce.sales') \
 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=$6 \
   OR g.scope_kind='department' AND g.scope_node_id=$7)) \
 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r \
   WHERE r.company_id=$4 AND r.user_id=$2 AND r.session_id=$8) \
 AND ($9::bigint IS NULL OR EXISTS(SELECT 1 FROM assignments a WHERE a.id=$1 AND a.version=$9))";

const NEXT_DUE: &str = "SELECT n.id FROM commerce_customer_notifications n \
 WHERE (((n.status IN ('pending','failed') AND n.attempts<8 AND n.next_attempt_at<=$1) \
   OR (n.status='sending' AND n.lease_until<$1))) \
 AND (NOT $2::boolean \
   OR EXISTS(SELECT 1 FROM engineering_verification_contexts e WHERE e.company_id=n.company_id \
     AND e.enabled AND e.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)) \
 ORDER BY n.next_attempt_at,n.created_at,n.id LIMIT 1";

#[derive(Clone)]
pub struct PgNotificationExecution {
    pool: DatabasePool,
    mode: PackageEnforcementMode,
    production: bool,
}

impl PgNotificationExecution {
    pub fn new(pool: DatabasePool, mode: PackageEnforcementMode, production: bool) -> Self {
        Self {
            pool,
            mode,
            production,
        }
    }

    /// Current immutable source (including its exact `source_hash`) for one
    /// notice. Read-only witness used by tests and diagnostics; it opens no
    /// transaction and grants no authority.
    pub async fn current_source(
        &self,
        connection: &mut PgConnection,
        notification_id: &str,
    ) -> Result<Option<NotificationSource>, Error> {
        self.source(connection, notification_id).await
    }

    fn ready(connection: &PgConnection) -> Result<(), Error> {
        if !connection.is_in_transaction() {
            return Err(
                erpx_server_runtime::public::postgres::PersistenceError::new(TRANSACTION_REQUIRED)
                    .into(),
            );
        }
        Ok(())
    }

    async fn source(
        &self,
        connection: &mut PgConnection,
        notification_id: &str,
    ) -> Result<Option<NotificationSource>, Error> {
        let sql = format!(
            "SELECT n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,\
             n.recipient_email,n.subject,n.body_text,n.adapter,n.status,n.version,n.attempts,\
             n.next_attempt_at,n.lease_until,n.provider_message_id,n.created_by,\
             delivery.department_id,delivery.order_state,delivery.company_id source_company,\
             customer.company_id customer_company,customer.status customer_status,\
             customer.notification_email,customer.notification_consent,\
             settings.adapter current_adapter,{RELEASE_READY} release_ready,\
             encode(sha256(convert_to({SOURCE_IMAGE},'UTF8')),'hex') source_hash \
             FROM commerce_customer_notifications n \
             LEFT JOIN (SELECT f.id fulfillment_id,f.version fulfillment_version,o.id order_id,\
               o.company_id,o.branch_id,o.department_id,o.kind order_kind,o.state order_state,\
               o.currency,o.party_id FROM commerce_fulfillments f \
               JOIN commerce_orders o ON o.id=f.order_id AND o.kind='sales') delivery \
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
            .bind(self.production)
            .fetch_optional(&mut *connection)
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

    async fn authorization(
        &self,
        connection: &mut PgConnection,
        notification_id: &str,
    ) -> Result<Option<ExecutionAuthorization>, Error> {
        let row = sqlx::query(
            "SELECT command_id,notification_id,company_id,kind,actor_id,assignment_id,\
             authority_json,source_hash,notification_version \
             FROM sales_notification_execution_authorizations WHERE notification_id=$1 \
             ORDER BY notification_version DESC,created_at DESC,command_id DESC LIMIT 1",
        )
        .bind(notification_id)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| {
            Ok::<_, sqlx::Error>(ExecutionAuthorization {
                command_id: row.try_get("command_id")?,
                notification_id: row.try_get("notification_id")?,
                company_id: row.try_get("company_id")?,
                kind: row.try_get("kind")?,
                actor_id: row.try_get("actor_id")?,
                assignment_id: row.try_get("assignment_id")?,
                authority_json: row.try_get("authority_json")?,
                source_hash: row.try_get("source_hash")?,
                notification_version: row.try_get("notification_version")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn current_authority(
        &self,
        connection: &mut PgConnection,
        actor: &QueuedActor,
        now: i64,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar(AUTHORITY)
            .bind(&actor.assignment_id)
            .bind(&actor.user_id)
            .bind(actor.role.as_deref())
            .bind(&actor.company_id)
            .bind(now)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.session_id.as_deref())
            .bind(actor.queued_assignment_version)
            .fetch_one(&mut *connection)
            .await?)
    }

    async fn require_capability(
        &self,
        connection: &mut PgConnection,
        company_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        if self.mode == PackageEnforcementMode::Disabled {
            return Ok(());
        }
        let catalogue = current_package_catalogue()?;
        let company = CompanyId::new(company_id)?;
        PgCapabilityDecisionReader::new(company, connection, &catalogue)
            .require(
                "sales.orders",
                CapabilityPurpose::SettleExisting,
                self.mode,
                TimestampMillis::new(now),
            )
            .await?;
        Ok(())
    }

    async fn require_execution(
        &self,
        connection: &mut PgConnection,
        source: &NotificationSource,
        authorization: &ExecutionAuthorization,
        now: i64,
    ) -> Result<QueuedActor, Error> {
        let actor = parse_queued_actor(&authorization.authority_json)?;
        if !authorization_current(source, authorization, &actor) {
            return Err(source_changed().into());
        }
        let operation = format!("commerce.customer-notification.{}", authorization.kind);
        let record_id = if authorization.kind == "queue" {
            source.fulfillment_id.as_str()
        } else {
            source.id.as_str()
        };
        let receipt = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2 \
             AND record_id=$3 AND actor_id=$4 AND authority_json::jsonb->>'companyId'=$5 \
             AND result_json::jsonb->>'id'=$6)",
        )
        .bind(&authorization.command_id)
        .bind(&operation)
        .bind(record_id)
        .bind(&actor.user_id)
        .bind(&source.company_id)
        .bind(&source.id)
        .fetch_one(&mut *connection)
        .await?;
        if !receipt {
            return Err(source_changed().into());
        }
        if !self.current_authority(connection, &actor, now).await? {
            return Err(authority_changed().into());
        }
        self.require_capability(connection, &source.company_id, now)
            .await?;
        Ok(actor)
    }

    async fn mark_authorization_failed(
        &self,
        connection: &mut PgConnection,
        notification_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "UPDATE commerce_customer_notifications SET status='failed',version=version+1,\
             next_attempt_at=$1,lease_until=NULL,\
             last_error='CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED',updated_at=$2 \
             WHERE id=$3 AND status IN ('pending','failed')",
        )
        .bind(HELD_UNTIL)
        .bind(now)
        .bind(notification_id)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    /// A `sending` lease that expired by process death is never a fresh send.
    async fn take_over_unknown(
        &self,
        connection: &mut PgConnection,
        notification_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "UPDATE commerce_customer_notifications SET status='unknown',version=version+1,\
             lease_until=NULL,next_attempt_at=$1,\
             last_error='CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN',updated_at=$2 \
             WHERE id=$3 AND status='sending'",
        )
        .bind(HELD_UNTIL)
        .bind(now)
        .bind(notification_id)
        .execute(&mut *connection)
        .await?;
        sqlx::query(
            "UPDATE sales_notification_execution_attempts SET state='unknown',\
             result_code='CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN',updated_at=$1 \
             WHERE notification_id=$2 AND state='sending'",
        )
        .bind(now)
        .bind(notification_id)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    async fn flush_deferred(&self, connection: &mut PgConnection) -> Result<(), Error> {
        sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn terminal(
        &self,
        connection: &mut PgConnection,
        stage: &DeliveryStage,
        state: &str,
        code: Option<&str>,
        provider_id: Option<&str>,
        now: i64,
    ) -> Result<(), Error> {
        let changed = sqlx::query_scalar::<_, String>(
            "UPDATE commerce_customer_notifications SET status=$1,version=version+1,\
             lease_until=NULL,next_attempt_at=$2,last_error=$3,provider_message_id=$4,\
             sent_at=CASE WHEN $1='sent' THEN $5 ELSE sent_at END,updated_at=$5 \
             WHERE id=$6 AND company_id=$7 AND status='sending' AND attempts=$8 \
             AND lease_until=$9 RETURNING id",
        )
        .bind(state)
        .bind(HELD_UNTIL)
        .bind(code)
        .bind(provider_id)
        .bind(now)
        .bind(&stage.source.id)
        .bind(&stage.source.company_id)
        .bind(stage.attempt.attempt)
        .bind(stage.attempt.lease_until)
        .fetch_optional(&mut *connection)
        .await?;
        if changed.is_none() {
            return Err(lease_changed().into());
        }
        if stage.authorization.is_none() {
            // Retained development boundary: no purchased attempt row exists.
            return Ok(());
        }
        let attempt = sqlx::query_scalar::<_, String>(
            "UPDATE sales_notification_execution_attempts SET state=$1,provider_message_id=$2,\
             result_code=$3,updated_at=$4 WHERE id=$5 AND notification_id=$6 AND company_id=$7 \
             AND state='sending' RETURNING id",
        )
        .bind(state)
        .bind(provider_id)
        .bind(code)
        .bind(now)
        .bind(&stage.attempt.id)
        .bind(&stage.source.id)
        .bind(&stage.source.company_id)
        .fetch_optional(&mut *connection)
        .await?;
        if attempt.is_none() {
            return Err(lease_changed().into());
        }
        Ok(())
    }

    async fn current(
        &self,
        connection: &mut PgConnection,
        stage: &DeliveryStage,
        now: i64,
    ) -> Result<NotificationSource, Error> {
        let source = self
            .source(connection, &stage.source.id)
            .await?
            .ok_or_else(source_changed)?;
        if stage.authorization.is_some() {
            let attempt = sqlx::query_scalar::<_, String>(
                "SELECT state FROM sales_notification_execution_attempts \
                 WHERE id=$1 AND notification_id=$2 AND company_id=$3 AND state='sending'",
            )
            .bind(&stage.attempt.id)
            .bind(&stage.source.id)
            .bind(&stage.source.company_id)
            .fetch_optional(&mut *connection)
            .await?;
            if attempt.is_none() {
                return Err(source_changed().into());
            }
        }
        if !attempt_current(&source, &stage.attempt, now) {
            return Err(source_changed().into());
        }
        if let Some(authorization) = stage.authorization.as_ref() {
            self.require_execution(connection, &source, authorization, now)
                .await?;
        }
        Ok(source)
    }
}

impl PgNotificationExecution {
    async fn begin(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, Error> {
        let mut transaction = self.pool.raw().begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *transaction)
            .await?;
        Ok(transaction)
    }

    async fn claim_in(
        &self,
        connection: &mut PgConnection,
        company: &str,
        id: &str,
        now: i64,
    ) -> Result<Option<DeliveryStage>, Error> {
        Self::ready(connection)?;
        // Same Company lock key as the original queue/retry purchase unit.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("commercial.package:company:{company}"))
            .execute(&mut *connection)
            .await?;
        let Some(source) = self.source(connection, id).await? else {
            return Ok(None);
        };
        if source.company_id != company {
            return Ok(None);
        }
        match claim_decision(&source, now) {
            ClaimDecision::NotQueued | ClaimDecision::LeaseHeld | ClaimDecision::Exhausted => {
                return Ok(None);
            }
            ClaimDecision::Takeover => {
                self.take_over_unknown(connection, id, now).await?;
                return Ok(None);
            }
            ClaimDecision::Ready => {}
        }
        let authorization = if self.mode == PackageEnforcementMode::Disabled {
            None
        } else {
            let Some(authorization) = self.authorization(connection, id).await? else {
                self.mark_authorization_failed(connection, id, now).await?;
                return Ok(None);
            };
            if self
                .require_execution(connection, &source, &authorization, now)
                .await
                .is_err()
            {
                self.mark_authorization_failed(connection, id, now).await?;
                return Ok(None);
            }
            Some(authorization)
        };
        let attempt = ExecutionAttempt {
            id: format!("sales-notification-attempt-{}", Uuid::new_v4()),
            notification_id: id.to_owned(),
            company_id: company.to_owned(),
            attempt: source.attempts + 1,
            lease_until: now + LEASE_MILLIS,
            source_hash: source.source_hash.clone(),
            state: "sending".into(),
        };
        sqlx::query(
            "UPDATE commerce_customer_notifications SET status='sending',\
             attempts=attempts+1,lease_until=$1,updated_at=$2 \
             WHERE id=$3 AND status IN ('pending','failed')",
        )
        .bind(attempt.lease_until)
        .bind(now)
        .bind(id)
        .execute(&mut *connection)
        .await?;
        if let Some(authorization) = authorization.as_ref() {
            sqlx::query(
                "INSERT INTO sales_notification_execution_attempts\
                 (id,notification_id,authorization_command_id,company_id,attempt,\
                 lease_until,source_hash,state,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,'sending',$8,$8)",
            )
            .bind(&attempt.id)
            .bind(id)
            .bind(&authorization.command_id)
            .bind(company)
            .bind(attempt.attempt)
            .bind(attempt.lease_until)
            .bind(&attempt.source_hash)
            .bind(now)
            .execute(&mut *connection)
            .await?;
        }
        let stage = DeliveryStage {
            source,
            authorization,
            attempt,
            plan: DispatchPlan::Sandbox,
        };
        if let Some(reason) = invalid_reason(&stage.source) {
            self.terminal(connection, &stage, "cancelled", Some(reason), None, now)
                .await?;
            return Ok(None);
        }
        self.flush_deferred(connection).await?;
        let final_source = self
            .source(connection, id)
            .await?
            .ok_or_else(source_changed)?;
        if final_source.source_hash != stage.attempt.source_hash
            || invalid_reason(&final_source).is_some()
        {
            return Err(source_changed().into());
        }
        if let Some(authorization) = stage.authorization.as_ref() {
            self.require_execution(connection, &final_source, authorization, now)
                .await?;
        }
        let plan = if final_source.adapter == "sandbox" {
            DispatchPlan::Sandbox
        } else {
            DispatchPlan::Resend
        };
        Ok(Some(DeliveryStage {
            source: final_source,
            authorization: stage.authorization,
            attempt: stage.attempt,
            plan,
        }))
    }

    async fn deliver_in(
        &self,
        stage: &DeliveryStage,
        dispatch: std::sync::Arc<dyn EmailDispatchPort>,
        trace: std::sync::Arc<DispatchTrace>,
        now: i64,
    ) -> Result<(), Error> {
        let mut transaction = self.begin().await?;
        let result: Result<(), Error> = async {
            let connection = &mut *transaction;
            Self::ready(connection)?;
            self.current(connection, stage, now).await?;
            self.flush_deferred(connection).await?;
            let source = self.current(connection, stage, now).await?;
            if trace.invoked() {
                return Err(attempt_uncertain().into());
            }
            let provider_id = match stage.plan {
                DispatchPlan::Sandbox => sandbox_provider_id(&source.id),
                DispatchPlan::Resend => {
                    // Missing provider configuration fails closed before the
                    // attempt becomes ambiguous.
                    dispatch.prepare(DispatchPlan::Resend)?;
                    trace.invoke();
                    let email = OutboundEmail {
                        notification_id: &source.id,
                        recipient_email: &source.recipient_email,
                        subject: &source.subject,
                        body_text: &source.body_text,
                        plan: DispatchPlan::Resend,
                    };
                    match dispatch.dispatch(email).await? {
                        DispatchOutcome::Accepted { provider_id } => provider_id,
                        DispatchOutcome::Uncertain => return Err(uncertain_attempt().into()),
                    }
                }
            };
            trace.provider(Some(provider_id.clone()));
            self.terminal(connection, stage, "sent", None, Some(&provider_id), now)
                .await?;
            self.flush_deferred(connection).await?;
            let final_source = self
                .source(connection, &stage.source.id)
                .await?
                .ok_or_else(source_changed)?;
            if final_source.source_hash != stage.attempt.source_hash
                || invalid_reason(&final_source).is_some()
            {
                return Err(source_changed().into());
            }
            if let Some(authorization) = stage.authorization.as_ref() {
                self.require_execution(connection, &final_source, authorization, now)
                    .await?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                transaction.commit().await?;
                Ok(())
            }
            Err(error) => {
                transaction.rollback().await.ok();
                Err(error)
            }
        }
    }

    async fn fail_in(
        &self,
        stage: &DeliveryStage,
        invoked: bool,
        provider_id: Option<&str>,
        now: i64,
    ) -> Result<(), Error> {
        let mut transaction = self.begin().await?;
        let result: Result<(), Error> = async {
            let connection = &mut *transaction;
            Self::ready(connection)?;
            let sending = if stage.authorization.is_some() {
                sqlx::query_scalar::<_, String>(
                    "SELECT state FROM sales_notification_execution_attempts \
                     WHERE id=$1 AND state='sending'",
                )
                .bind(&stage.attempt.id)
                .fetch_optional(&mut *connection)
                .await?
            } else {
                sqlx::query_scalar::<_, String>(
                    "SELECT status FROM commerce_customer_notifications \
                     WHERE id=$1 AND status='sending' AND attempts=$2 AND lease_until=$3",
                )
                .bind(&stage.source.id)
                .bind(stage.attempt.attempt)
                .bind(stage.attempt.lease_until)
                .fetch_optional(&mut *connection)
                .await?
            };
            if sending.is_none() {
                return Ok(());
            }
            let (state, code) = terminal_code(invoked);
            self.terminal(connection, stage, state, Some(code), provider_id, now)
                .await?;
            self.flush_deferred(connection).await?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                transaction.commit().await?;
                Ok(())
            }
            Err(error) => {
                transaction.rollback().await.ok();
                Err(error)
            }
        }
    }
}

impl NotificationExecutionUnit for PgNotificationExecution {
    type Error = Error;

    fn company_of<'a>(
        &'a self,
        notification_id: &'a str,
    ) -> ExecutionFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar::<_, String>(
                "SELECT company_id FROM commerce_customer_notifications WHERE id=$1",
            )
            .bind(notification_id)
            .fetch_optional(self.pool.raw())
            .await?)
        })
    }

    fn claim<'a>(
        &'a self,
        company_id: &'a str,
        notification_id: &'a str,
        now: i64,
    ) -> ExecutionFuture<'a, Option<DeliveryStage>, Self::Error> {
        Box::pin(async move {
            let mut transaction = self.begin().await?;
            let result = self
                .claim_in(&mut transaction, company_id, notification_id, now)
                .await;
            match result {
                Ok(stage) => {
                    transaction.commit().await?;
                    Ok(stage)
                }
                Err(error) => {
                    transaction.rollback().await.ok();
                    Err(error)
                }
            }
        })
    }

    fn deliver<'a>(
        &'a self,
        stage: &'a DeliveryStage,
        dispatch: std::sync::Arc<dyn EmailDispatchPort>,
        trace: std::sync::Arc<DispatchTrace>,
        now: i64,
    ) -> ExecutionFuture<'a, (), Self::Error> {
        Box::pin(async move {
            'A: for attempt in 1..=4 {
                match self
                    .deliver_in(
                        stage,
                        std::sync::Arc::clone(&dispatch),
                        std::sync::Arc::clone(&trace),
                        now,
                    )
                    .await
                {
                    Err(PgTransactionError::Persistence(error))
                        if attempt < 4 && error.retryable() =>
                    {
                        continue 'A;
                    }
                    result => return result,
                }
            }
            unreachable!("bounded transaction attempts")
        })
    }

    fn fail<'a>(
        &'a self,
        stage: &'a DeliveryStage,
        invoked: bool,
        provider_id: Option<&'a str>,
        now: i64,
    ) -> ExecutionFuture<'a, (), Self::Error> {
        Box::pin(async move { self.fail_in(stage, invoked, provider_id, now).await })
    }

    fn next_due<'a>(&'a self, now: i64) -> ExecutionFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            Ok(sqlx::query_scalar(NEXT_DUE)
                .bind(now)
                .bind(self.production)
                .fetch_optional(self.pool.raw())
                .await?)
        })
    }
}
