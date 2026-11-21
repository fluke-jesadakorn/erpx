//! Native Customer Portal invitation delivery use case.
//!
//! One bounded cycle recovers interrupted or expired invitations, claims at
//! most one due invitation under a fresh 30 s lease, re-proves the exact
//! approved binding/order source and purchase entitlement, reproduces and
//! verifies the signed token, sends the private enrollment link with its
//! idempotency key, and commits the terminal `sent` state before returning.
use crate::adapters::portal_invitation_email::{PortalEmail, PortalInvitationEmailPort};
use crate::domain::portal_invitation_delivery::{
    InvitationJob, beta_email_body, origin_not_configured, safe_delivery_code,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type PortalDeliveryFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Owner persistence port. Every mutating method owns one serializable unit.
pub trait PortalInvitationUnit: Send + Sync {
    type Error: From<BusinessError> + Send + 'static;

    /// Recover interrupted leases and expire elapsed invitations.
    fn recover<'a>(&'a self, now: i64) -> PortalDeliveryFuture<'a, (), Self::Error>;

    /// Claim one due invitation under the supplied lease token.
    fn claim<'a>(
        &'a self,
        lease_token: &'a str,
        now: i64,
    ) -> PortalDeliveryFuture<'a, Option<InvitationJob>, Self::Error>;

    /// Re-read the exact source, token digest and purchase entitlement.
    fn current<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
    ) -> PortalDeliveryFuture<'a, bool, Self::Error>;

    fn cancel<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
        state: &'static str,
        reason: &'static str,
    ) -> PortalDeliveryFuture<'a, (), Self::Error>;

    fn sent<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
    ) -> PortalDeliveryFuture<'a, (), Self::Error>;

    fn failed<'a>(
        &'a self,
        job: &'a InvitationJob,
        now: i64,
        state: &'static str,
        code: &'static str,
        next_attempt_at: i64,
    ) -> PortalDeliveryFuture<'a, (), Self::Error>;
}

/// Signed-token reproduction bound to the stored digest.
pub type TokenSigner<'a> = &'a (
        dyn Fn(
    &crate::domain::portal_invitation_delivery::InvitationClaims,
    &str,
) -> Result<String, BusinessError>
            + Sync
    );

/// One cycle. Returns `true` when an invitation was claimed (delivered or
/// terminally settled), `false` when the queue was empty.
#[allow(clippy::too_many_arguments)]
pub async fn deliver_one<U: PortalInvitationUnit>(
    unit: &U,
    email: &dyn PortalInvitationEmailPort,
    signer: TokenSigner<'_>,
    account_origin: Option<&str>,
    lease_token: &str,
    now: i64,
) -> Result<bool, U::Error> {
    unit.recover(now).await?;
    let Some(job) = unit.claim(lease_token, now).await? else {
        return Ok(false);
    };
    if job.token_expires_at <= now {
        unit.cancel(&job, now, "expired", "INVITATION_EXPIRED")
            .await?;
        return Ok(true);
    }
    if !unit.current(&job, now).await? {
        unit.cancel(&job, now, "revoked", "SOURCE_UNAVAILABLE")
            .await?;
        return Ok(true);
    }
    match deliver(&job, email, signer, account_origin).await {
        Ok(()) => {
            unit.sent(&job, now).await?;
        }
        Err(error) => {
            unit.failed(
                &job,
                now,
                job.terminal_state(),
                safe_delivery_code(error.code),
                job.next_attempt_at(now),
            )
            .await?;
        }
    }
    Ok(true)
}

/// Reproduce the signed token, build the private link and send exactly once.
async fn deliver(
    job: &InvitationJob,
    email: &dyn PortalInvitationEmailPort,
    signer: TokenSigner<'_>,
    account_origin: Option<&str>,
) -> Result<(), BusinessError> {
    let account_origin = account_origin.ok_or_else(origin_not_configured)?;
    let token = signer(&job.claims(), &job.token_hash)?;
    let link = job.enrollment_url(account_origin, &token);
    let text = job.email_text(&link);
    email
        .send(PortalEmail {
            delivery_key: job.delivery_key(),
            recipient: job.email.clone(),
            subject: job.email_subject().to_owned(),
            text: beta_email_body(job.email_subject(), &text),
        })
        .await
}
