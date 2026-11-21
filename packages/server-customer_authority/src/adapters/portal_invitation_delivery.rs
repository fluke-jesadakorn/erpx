//! PostgreSQL adapter for the native Customer Portal invitation worker.
//!
//! SQL is Customer Authority-owned (`customer_portal_invitations`,
//! `customer_portal_invitation_events`) over the approved
//! `customer_access_bindings` and the original Commerce order. The Commercial
//! purchase gate reuses the owner's native capability reader instead of
//! duplicating entitlement SQL. Every phase opens one serializable unit.
use crate::application::portal_invitation_delivery::{PortalDeliveryFuture, PortalInvitationUnit};
use crate::domain::portal_invitation_delivery::{
    INVITABLE_ORDER_STATES, InvitationJob, LEASE_MILLIS, SYSTEM_ACTOR, TRANSACTION_REQUIRED,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Production selects only the enabled, isolated engineering-verification
/// context, exactly like the retained scheduled feature gate for this module.
const FEATURE_READY: &str = "EXISTS(SELECT 1 FROM engineering_verification_contexts e \
 WHERE e.company_id=$1 AND e.enabled \
 AND e.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";

const CANDIDATE_SQL: &str = "SELECT candidate.id,candidate.binding_id,binding.company_id \
 FROM customer_portal_invitations candidate \
 JOIN customer_access_bindings binding ON binding.id=candidate.binding_id \
 JOIN commerce_orders original ON original.id=binding.order_id \
   AND original.company_id=binding.company_id AND original.party_id=binding.customer_id \
 WHERE candidate.state='queued' AND candidate.next_attempt_at<=$1 \
   AND candidate.token_expires_at>$1 AND binding.status='approved' \
   AND binding.expires_at>$1 AND original.kind IN ('quote','sales') \
   AND original.state = ANY($2) \
 ORDER BY candidate.created_at,candidate.id FOR UPDATE OF candidate SKIP LOCKED LIMIT 8";

const CLAIMED_SQL: &str = "UPDATE customer_portal_invitations invitation SET state='running',\
 attempts=attempts+1,lease_token=$1,lease_until=$2 \
 WHERE invitation.id=(SELECT candidate.id FROM customer_portal_invitations candidate \
   JOIN customer_access_bindings binding ON binding.id=candidate.binding_id \
   JOIN commerce_orders original ON original.id=binding.order_id \
     AND original.company_id=binding.company_id AND original.party_id=binding.customer_id \
   WHERE candidate.id=$3 AND candidate.state='queued' \
     AND candidate.next_attempt_at<=$4 AND candidate.token_expires_at>$4 \
     AND binding.status='approved' AND binding.expires_at>$4 \
     AND original.kind IN ('quote','sales') AND original.state = ANY($5)) \
 RETURNING invitation.id,invitation.binding_id,invitation.holding_slug,invitation.generation,\
 invitation.email,invitation.token_hash,invitation.token_key_version,\
 invitation.token_expires_at,invitation.attempts,invitation.lease_token";

const CURRENT_SQL: &str = "SELECT 1 FROM customer_portal_invitations invitation \
 JOIN customer_access_bindings binding ON binding.id=invitation.binding_id \
 JOIN commerce_orders original ON original.id=binding.order_id \
   AND original.company_id=binding.company_id AND original.party_id=binding.customer_id \
 WHERE invitation.id=$1 AND invitation.state='running' AND invitation.lease_token=$2 \
   AND invitation.holding_slug=$3 AND invitation.token_hash=$4 \
   AND invitation.email=binding.email AND invitation.token_expires_at>$5 \
   AND binding.status='approved' AND binding.expires_at>$5 \
   AND original.kind IN ('quote','sales') AND original.state = ANY($6)";

/// The worker: one pool, one package-enforcement mode, one production flag.
#[derive(Clone)]
pub struct PgPortalInvitationDelivery {
    pool: DatabasePool,
    mode: PackageEnforcementMode,
    production: bool,
}

impl PgPortalInvitationDelivery {
    pub fn new(pool: DatabasePool, mode: PackageEnforcementMode, production: bool) -> Self {
        Self {
            pool,
            mode,
            production,
        }
    }

    fn ready(connection: &PgConnection) -> Result<(), Error> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new(TRANSACTION_REQUIRED).into());
        }
        Ok(())
    }

    async fn begin(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, Error> {
        let mut transaction = self.pool.raw().begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *transaction)
            .await?;
        Ok(transaction)
    }

    /// Retained `scheduledFeaturePredicate(company,'customer_authority')` for
    /// the enabled modes. The disabled boundary keeps the retained `TRUE`.
    async fn feature_ready(
        &self,
        connection: &mut PgConnection,
        company_id: &str,
    ) -> Result<bool, Error> {
        if self.mode == PackageEnforcementMode::Disabled || !self.production {
            return Ok(true);
        }
        Ok(
            sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(format!("SELECT {FEATURE_READY}")))
                .bind(company_id)
                .fetch_one(&mut *connection)
                .await?,
        )
    }

    /// Retained `customerPortalPurchaseScope('binding','settle-existing')`:
    /// disabled is unconditional; shadow observes through the owner reader;
    /// enforced refuses without the purchased `sales.portal` capability.
    async fn purchase_ready(
        &self,
        connection: &mut PgConnection,
        company_id: &str,
        now: i64,
    ) -> Result<bool, Error> {
        if self.mode == PackageEnforcementMode::Disabled {
            return Ok(true);
        }
        let catalogue = current_package_catalogue()?;
        let company = CompanyId::new(company_id)?;
        Ok(
            PgCapabilityDecisionReader::new(company, connection, &catalogue)
                .require(
                    "sales.portal",
                    CapabilityPurpose::SettleExisting,
                    self.mode,
                    TimestampMillis::new(now),
                )
                .await
                .is_ok(),
        )
    }

    async fn event(
        &self,
        connection: &mut PgConnection,
        id: String,
        invitation_id: &str,
        action: &str,
        reason: &str,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO customer_portal_invitation_events\
             (id,invitation_id,action,actor_id,reason,created_at) VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(id)
        .bind(invitation_id)
        .bind(action)
        .bind(SYSTEM_ACTOR)
        .bind(reason)
        .bind(now)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    async fn claim_in(
        &self,
        connection: &mut PgConnection,
        lease_token: &str,
        now: i64,
    ) -> Result<Option<InvitationJob>, Error> {
        Self::ready(connection)?;
        let states: Vec<&str> = INVITABLE_ORDER_STATES.to_vec();
        let candidates = sqlx::query(CANDIDATE_SQL)
            .bind(now)
            .bind(&states)
            .fetch_all(&mut *connection)
            .await?;
        let mut selected = None;
        for candidate in candidates {
            let company_id: String = candidate.try_get("company_id")?;
            if !self.feature_ready(connection, &company_id).await? {
                continue;
            }
            if !self.purchase_ready(connection, &company_id, now).await? {
                continue;
            }
            selected = Some((
                candidate.try_get::<String, _>("id")?,
                candidate.try_get::<String, _>("binding_id")?,
                company_id,
            ));
            break;
        }
        let Some((id, binding_id, company_id)) = selected else {
            return Ok(None);
        };
        let row = sqlx::query(CLAIMED_SQL)
            .bind(lease_token)
            .bind(now + LEASE_MILLIS)
            .bind(&id)
            .bind(now)
            .bind(&states)
            .fetch_optional(&mut *connection)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let job = InvitationJob {
            id: row.try_get("id")?,
            binding_id: row.try_get("binding_id")?,
            company_id,
            holding_slug: row.try_get("holding_slug")?,
            generation: row.try_get::<i32, _>("generation")?,
            email: row.try_get("email")?,
            token_hash: row.try_get("token_hash")?,
            token_key_version: row.try_get::<i16, _>("token_key_version")?,
            token_expires_at: row.try_get("token_expires_at")?,
            attempts: row.try_get::<i32, _>("attempts")?,
            lease_token: lease_token.to_owned(),
        };
        let _ = binding_id;
        self.event(
            connection,
            format!("customer-portal-delivery-event-{}-{lease_token}", job.id),
            &job.id,
            "delivery-started",
            "",
            now,
        )
        .await?;
        Ok(Some(job))
    }

    /// Terminal transition under the exact lease, plus its immutable event.
    #[allow(clippy::too_many_arguments)]
    async fn transition<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
        state: &'static str,
        code: Option<&'static str>,
        next_attempt_at: i64,
        event_action: &'static str,
        event_id: String,
        statement: TransitionStatement,
    ) -> Result<(), Error> {
        let mut transaction = self.begin().await?;
        let result: Result<(), Error> = async {
            let connection = &mut *transaction;
            Self::ready(connection)?;
            let changed = match statement {
                TransitionStatement::Cancel => {
                    sqlx::query_scalar::<_, String>(CANCEL_SQL)
                        .bind(state)
                        .bind(code)
                        .bind(&job.id)
                        .bind(&job.lease_token)
                        .fetch_optional(&mut *connection)
                        .await?
                }
                TransitionStatement::Sent => {
                    sqlx::query_scalar::<_, String>(SENT_SQL)
                        .bind(now)
                        .bind(&job.id)
                        .bind(&job.lease_token)
                        .fetch_optional(&mut *connection)
                        .await?
                }
                TransitionStatement::Failed => {
                    sqlx::query_scalar::<_, String>(FAILED_SQL)
                        .bind(state)
                        .bind(next_attempt_at)
                        .bind(code)
                        .bind(&job.id)
                        .bind(&job.lease_token)
                        .fetch_optional(&mut *connection)
                        .await?
                }
            };
            if changed.is_some() {
                self.event(
                    connection,
                    event_id,
                    &job.id,
                    event_action,
                    code.unwrap_or(""),
                    now,
                )
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
}

#[derive(Clone, Copy)]
enum TransitionStatement {
    Cancel,
    Sent,
    Failed,
}

const CANCEL_SQL: &str = "UPDATE customer_portal_invitations SET state=$1,lease_token=NULL,\
 lease_until=NULL,last_error=$2 \
 WHERE id=$3 AND state='running' AND lease_token=$4 RETURNING id";
const SENT_SQL: &str = "UPDATE customer_portal_invitations SET state='sent',sent_at=$1,\
 lease_token=NULL,lease_until=NULL,last_error=NULL \
 WHERE id=$2 AND state='running' AND lease_token=$3 RETURNING id";
const FAILED_SQL: &str = "UPDATE customer_portal_invitations SET state=$1,next_attempt_at=$2,\
 lease_token=NULL,lease_until=NULL,last_error=$3 \
 WHERE id=$4 AND state='running' AND lease_token=$5 RETURNING id";

impl PortalInvitationUnit for PgPortalInvitationDelivery {
    type Error = Error;

    fn recover<'a>(&'a self, now: i64) -> PortalDeliveryFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let mut transaction = self.begin().await?;
            let result: Result<(), Error> = async {
                let connection = &mut *transaction;
                Self::ready(connection)?;
                let expired: Vec<(String, String, String)> = sqlx::query_as(
                    "UPDATE customer_portal_invitations SET \
                     state=CASE WHEN token_expires_at<=$1 THEN 'expired' \
                       WHEN attempts<6 THEN 'queued' ELSE 'failed' END,\
                     lease_token=NULL,lease_until=NULL,\
                     last_error=CASE WHEN token_expires_at<=$1 THEN 'INVITATION_EXPIRED' \
                       WHEN attempts<6 THEN 'DELIVERY_INTERRUPTED' ELSE 'DELIVERY_ATTEMPTS_EXHAUSTED' END \
                     WHERE (state='running' AND (lease_until<$1 OR token_expires_at<=$1)) \
                       OR (state IN ('queued','sent') AND token_expires_at<=$1) \
                     RETURNING id,state,last_error",
                )
                .bind(now)
                .fetch_all(&mut *connection)
                .await?;
                for (id, state, last_error) in expired {
                    let action = match state.as_str() {
                        "expired" => "expired",
                        "failed" => "delivery-failed",
                        _ => "delivery-retried",
                    };
                    self.event(
                        connection,
                        format!("customer-portal-delivery-recovery-{id}-{now}"),
                        &id,
                        action,
                        &last_error,
                        now,
                    )
                    .await?;
                }
                let unavailable: Vec<String> = sqlx::query_scalar(
                    "UPDATE customer_portal_invitations invitation SET state='revoked',\
                     lease_token=NULL,lease_until=NULL,last_error='SOURCE_UNAVAILABLE' \
                     WHERE invitation.state IN ('queued','running','sent') \
                       AND invitation.token_expires_at>$1 \
                       AND NOT EXISTS (SELECT 1 FROM customer_access_bindings binding \
                         JOIN commerce_orders original ON original.id=binding.order_id \
                           AND original.company_id=binding.company_id \
                           AND original.party_id=binding.customer_id \
                         WHERE binding.id=invitation.binding_id \
                           AND binding.status='approved' AND binding.expires_at>$1 \
                           AND original.kind IN ('quote','sales') \
                           AND original.state = ANY($2)) \
                     RETURNING id",
                )
                .bind(now)
                .bind(INVITABLE_ORDER_STATES.to_vec())
                .fetch_all(&mut *connection)
                .await?;
                for id in unavailable {
                    self.event(
                        connection,
                        format!("customer-portal-delivery-recovery-source-{id}-{now}"),
                        &id,
                        "revoked",
                        "SOURCE_UNAVAILABLE",
                        now,
                    )
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
        })
    }

    fn claim<'a>(
        &'a self,
        lease_token: &'a str,
        now: i64,
    ) -> PortalDeliveryFuture<'a, Option<InvitationJob>, Self::Error> {
        Box::pin(async move {
            let mut transaction = self.begin().await?;
            let result = self.claim_in(&mut transaction, lease_token, now).await;
            match result {
                Ok(job) => {
                    transaction.commit().await?;
                    Ok(job)
                }
                Err(error) => {
                    transaction.rollback().await.ok();
                    Err(error)
                }
            }
        })
    }

    fn current<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
    ) -> PortalDeliveryFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut transaction = self.begin().await?;
            let result: Result<bool, Error> = async {
                let connection = &mut *transaction;
                Self::ready(connection)?;
                if !self.feature_ready(connection, &job.company_id).await? {
                    return Ok(false);
                }
                if !self
                    .purchase_ready(connection, &job.company_id, now)
                    .await?
                {
                    return Ok(false);
                }
                Ok(sqlx::query_scalar::<_, i32>(CURRENT_SQL)
                    .bind(&job.id)
                    .bind(&job.lease_token)
                    .bind(&job.holding_slug)
                    .bind(&job.token_hash)
                    .bind(now)
                    .bind(INVITABLE_ORDER_STATES.to_vec())
                    .fetch_optional(&mut *connection)
                    .await?
                    .is_some())
            }
            .await;
            let _ = transaction.rollback().await;
            result
        })
    }

    fn cancel<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
        state: &'static str,
        reason: &'static str,
    ) -> PortalDeliveryFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transition(
                job,
                now,
                state,
                Some(reason),
                now,
                state,
                format!("{}-{}-{}", job.id, state, now),
                TransitionStatement::Cancel,
            )
            .await
        })
    }

    fn sent<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
    ) -> PortalDeliveryFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transition(
                job,
                now,
                "sent",
                None,
                now,
                "delivery-sent",
                format!("{}-sent-{}", job.id, now),
                TransitionStatement::Sent,
            )
            .await
        })
    }

    fn failed<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
        state: &'static str,
        code: &'static str,
        next_attempt_at: i64,
    ) -> PortalDeliveryFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.transition(
                job,
                now,
                state,
                Some(code),
                next_attempt_at,
                if state == "failed" {
                    "delivery-failed"
                } else {
                    "delivery-retried"
                },
                format!("{}-failed-{}", job.id, now),
                TransitionStatement::Failed,
            )
            .await
        })
    }
}
