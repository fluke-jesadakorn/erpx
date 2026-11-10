//! PostgreSQL adapter for the Customer Authority portal enrollment read.
//!
//! Identity Access owns the opaque session/SSO read in Control. This adapter
//! owns only the Customer Authority facts in the exact Holding: a durable
//! `customer_access_identities` row, its approved unexpired binding, and the
//! accepted invitation that names the registered Holding slug. No row is
//! mutated and no transaction is begun or committed here.
use crate::application::portal_session::{PortalEnrollmentSource, PortalSessionFuture};
use crate::domain::portal_session::{
    normalize_email, PortalCustomerProof, PortalCustomerRequest, PortalSessionEvidence,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgPortalEnrollment<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPortalEnrollment<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl PortalEnrollmentSource for PgPortalEnrollment<'_> {
    type Error = Error;

    fn enrolled_customer<'a>(
        &'a mut self,
        session: &'a PortalSessionEvidence,
        request: &'a PortalCustomerRequest<'_>,
        now: i64,
    ) -> PortalSessionFuture<'a, Option<PortalCustomerProof>, Self::Error> {
        Box::pin(async move {
            let Some(email) = normalize_email(&session.email) else {
                return Ok(None);
            };
            let row = sqlx::query(
                r#"SELECT identity.user_id,identity.email
                  FROM customer_access_identities identity
                  JOIN customer_access_bindings binding
                    ON binding.id=identity.binding_id
                   AND binding.email=$2
                   AND binding.status='approved'
                   AND binding.expires_at>$3
                  JOIN customer_portal_invitations invitation
                    ON invitation.binding_id=binding.id
                   AND invitation.email=$2
                   AND invitation.state='accepted'
                   AND invitation.accepted_user_id=identity.user_id
                   AND invitation.holding_slug=$4
                  WHERE identity.user_id=$1 AND identity.email=$2
                  ORDER BY invitation.accepted_at DESC,invitation.generation DESC
                  LIMIT 1"#,
            )
            .bind(&session.user_id)
            .bind(&email)
            .bind(now)
            .bind(request.holding_slug)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<PortalCustomerProof, sqlx::Error>(PortalCustomerProof {
                    user_id: row.try_get("user_id")?,
                    email: row.try_get("email")?,
                    holding_id: Some(request.holding_id.to_owned()),
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }
}
