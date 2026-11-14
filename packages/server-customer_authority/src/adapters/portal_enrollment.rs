//! PostgreSQL adapter for the native Customer Portal invitation enrollment.
//!
//! SQL is Customer Authority-owned (`customer_portal_invitations`,
//! `customer_portal_enrollment_attempts`, `customer_access_identities`) over
//! the approved `customer_access_bindings` and the original Commerce order.
//! The caller owns the serializable Holding transaction; the portal session is
//! re-proved through the supplied Control port.
use crate::application::portal_enrollment::{
    EnrollmentFuture, EnrollmentRepository, PortalActor, PortalSessionPort,
};
use crate::domain::portal_enrollment::{
    EnrollmentReservation, InvitationMatch, invitation_unavailable,
};
use crate::domain::portal_invitation_delivery::{INVITABLE_ORDER_STATES, InvitationClaims};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const FEATURE_READY: &str = "EXISTS(SELECT 1 FROM engineering_verification_contexts e \
 WHERE e.company_id=$1 AND e.enabled \
 AND e.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";

pub struct PgPortalEnrollmentCommand<'a> {
    connection: &'a mut PgConnection,
    session: &'a mut dyn PortalSessionPort,
    mode: PackageEnforcementMode,
    production: bool,
}

impl<'a> PgPortalEnrollmentCommand<'a> {
    pub fn new(
        connection: &'a mut PgConnection,
        session: &'a mut dyn PortalSessionPort,
        mode: PackageEnforcementMode,
        production: bool,
    ) -> Self {
        Self {
            connection,
            session,
            mode,
            production,
        }
    }

    fn ready(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("CUSTOMER_PORTAL_ENROLLMENT_TRANSACTION_REQUIRED").into(),
            );
        }
        Ok(())
    }

    async fn feature_ready(&mut self, company_id: &str) -> Result<bool, Error> {
        if self.mode == PackageEnforcementMode::Disabled || !self.production {
            return Ok(true);
        }
        Ok(
            sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(format!("SELECT {FEATURE_READY}")))
                .bind(company_id)
                .fetch_one(&mut *self.connection)
                .await?,
        )
    }

    /// Retained `customerPortalPurchaseScope('binding','settle-existing')`.
    async fn purchase_ready(&mut self, company_id: &str, now: i64) -> Result<bool, Error> {
        if self.mode == PackageEnforcementMode::Disabled {
            return Ok(true);
        }
        let catalogue = current_package_catalogue()?;
        let company = erpx_server_shared::public::CompanyId::new(company_id)?;
        match PgCapabilityDecisionReader::new(company, self.connection, &catalogue)
            .require(
                "sales.portal",
                CapabilityPurpose::SettleExisting,
                self.mode,
                TimestampMillis::new(now),
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(PgTransactionError::Business(_)) => Ok(false),
            Err(PgTransactionError::Persistence(error)) => {
                Err(PgTransactionError::Persistence(error))
            }
        }
    }
}

impl EnrollmentRepository for PgPortalEnrollmentCommand<'_> {
    type Error = Error;

    fn session<'b>(
        &'b mut self,
        expected: Option<&'b crate::application::portal_enrollment::PortalSession>,
    ) -> EnrollmentFuture<'b, crate::application::portal_enrollment::PortalSession, Self::Error>
    {
        Box::pin(async move {
            self.session
                .current(expected)
                .await
                .map_err(PgTransactionError::Business)
        })
    }

    fn reserve<'b>(
        &'b mut self,
        invitation: &'b str,
        now: i64,
    ) -> EnrollmentFuture<'b, (i64, i64), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let row = sqlx::query(
                "INSERT INTO customer_portal_enrollment_attempts(invitation_id,window_start,attempts) \
                 VALUES($1,$2,1) ON CONFLICT(invitation_id) DO UPDATE SET \
                 attempts=CASE WHEN customer_portal_enrollment_attempts.window_start<$3 THEN 1 \
                   ELSE customer_portal_enrollment_attempts.attempts+1 END, \
                 window_start=CASE WHEN customer_portal_enrollment_attempts.window_start<$3 \
                   THEN excluded.window_start \
                   ELSE customer_portal_enrollment_attempts.window_start END \
                 RETURNING attempts,window_start",
            )
            .bind(invitation)
            .bind(now)
            .bind(now - crate::domain::portal_enrollment::ATTEMPT_WINDOW_MILLIS)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok((
                i64::from(row.try_get::<i32, _>("attempts")?),
                row.try_get::<i64, _>("window_start")?,
            ))
        })
    }

    fn release<'b>(
        &'b mut self,
        reservation: &'b EnrollmentReservation,
    ) -> EnrollmentFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            sqlx::query(
                "DELETE FROM customer_portal_enrollment_attempts \
                 WHERE invitation_id=$1 AND window_start=$2 AND attempts<=1",
            )
            .bind(&reservation.invitation_id)
            .bind(reservation.window_start)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "UPDATE customer_portal_enrollment_attempts SET attempts=attempts-1 \
                 WHERE invitation_id=$1 AND window_start=$2 AND attempts>1",
            )
            .bind(&reservation.invitation_id)
            .bind(reservation.window_start)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn invitation<'b>(
        &'b mut self,
        claims: &'b InvitationClaims,
        now: i64,
        allow_accepted: bool,
    ) -> EnrollmentFuture<'b, Option<InvitationMatch>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let states: Vec<&str> = if allow_accepted {
                vec!["sent", "accepted"]
            } else {
                vec!["sent"]
            };
            // The purchase/release gate reuses the owner reader on the same unit.
            let company = sqlx::query_scalar::<_, String>(
                "SELECT binding.company_id FROM customer_portal_invitations invitation \
                 JOIN customer_access_bindings binding ON binding.id=invitation.binding_id \
                 WHERE invitation.id=$1 AND invitation.binding_id=$2",
            )
            .bind(&claims.invitation_id)
            .bind(&claims.binding_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(company) = company else {
                return Ok(None);
            };
            if !self.feature_ready(&company).await? {
                return Ok(None);
            }
            if !self.purchase_ready(&company, now).await? {
                return Ok(None);
            }
            let row = sqlx::query(
                "SELECT invitation.id,invitation.binding_id,invitation.holding_slug,\
                 invitation.email,invitation.state,invitation.accepted_user_id,\
                 invitation.token_hash \
                 FROM customer_portal_invitations invitation \
                 JOIN customer_access_bindings binding ON binding.id=invitation.binding_id \
                 JOIN commerce_orders original ON original.id=binding.order_id \
                   AND original.company_id=binding.company_id \
                   AND original.party_id=binding.customer_id \
                 WHERE invitation.id=$1 AND invitation.binding_id=$2 \
                   AND invitation.holding_slug=$3 AND invitation.generation=$4 \
                   AND invitation.token_key_version=$5 AND invitation.token_expires_at=$6 \
                   AND invitation.token_expires_at>$7 AND invitation.state = ANY($8) \
                   AND binding.email=invitation.email AND binding.status='approved' \
                   AND binding.expires_at>$7 AND original.kind IN ('quote','sales') \
                   AND original.state = ANY($9) \
                 FOR UPDATE OF invitation,binding,original",
            )
            .bind(&claims.invitation_id)
            .bind(&claims.binding_id)
            .bind(&claims.holding_slug)
            .bind(i64::from(claims.generation))
            .bind(claims.key_version)
            .bind(claims.expires_at)
            .bind(now)
            .bind(&states)
            .bind(INVITABLE_ORDER_STATES.to_vec())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(InvitationMatch {
                    invitation_id: row.try_get("id")?,
                    binding_id: row.try_get("binding_id")?,
                    holding_slug: row.try_get("holding_slug")?,
                    email: row.try_get("email")?,
                    state: row.try_get("state")?,
                    accepted_user_id: row.try_get("accepted_user_id")?,
                    token_hash: row.try_get("token_hash")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn identity<'b>(
        &'b mut self,
        binding_id: &'b str,
        lock: bool,
    ) -> EnrollmentFuture<'b, Option<(String, String)>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let sql = if lock {
                "SELECT user_id,email FROM customer_access_identities WHERE binding_id=$1 FOR UPDATE"
            } else {
                "SELECT user_id,email FROM customer_access_identities WHERE binding_id=$1"
            };
            let row = sqlx::query(sql)
                .bind(binding_id)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>((
                    row.try_get::<String, _>("user_id")?,
                    row.try_get::<String, _>("email")?,
                ))
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn bind_identity<'b>(
        &'b mut self,
        binding_id: &'b str,
        actor: &'b PortalActor,
        now: i64,
    ) -> EnrollmentFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            sqlx::query(
                "INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(binding_id) DO NOTHING",
            )
            .bind(binding_id)
            .bind(&actor.user_id)
            .bind(actor.email.trim().to_lowercase())
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn accept<'b>(
        &'b mut self,
        invitation_id: &'b str,
        actor: &'b PortalActor,
        now: i64,
    ) -> EnrollmentFuture<'b, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE customer_portal_invitations SET state='accepted',accepted_at=$1,\
                 accepted_user_id=$2 WHERE id=$3 AND state='sent' RETURNING id",
            )
            .bind(now)
            .bind(&actor.user_id)
            .bind(invitation_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            if changed.is_none() {
                return Err(invitation_unavailable().into());
            }
            sqlx::query(
                "INSERT INTO customer_portal_invitation_events\
                 (id,invitation_id,action,actor_id,reason,created_at) \
                 VALUES($1,$2,'accepted',$3,$4,$5)",
            )
            .bind(crate::domain::portal_enrollment::accepted_event_id(
                invitation_id,
            ))
            .bind(invitation_id)
            .bind(&actor.user_id)
            .bind(crate::domain::portal_enrollment::ACCEPT_REASON)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
}
