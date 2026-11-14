//! Native PMS social event projector over the caller's transaction.
//!
//! Exact port of `packages/server-pms/src/application/social-events.ts` and the
//! owner wiring in `packages/server-pms/src/adapters/pms/social-events.ts`.
//! Every owner read/write is supplied by [`PmsSocialPorts`] on the same
//! `&mut sqlx::PgConnection`; this adapter never opens an independent
//! transaction, commits, retries or calls an external provider.
use crate::domain::job_projector::EventMessage;
use crate::domain::pms_social_event::{
    PmsSocialNotice, PmsSocialPost, PmsSocialSource, pms_social_body, pms_social_event,
    pms_social_notice_id, pms_social_post_id,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::PgConnection;
use std::{future::Future, pin::Pin};

pub type PmsSocialFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner operations required by [`project_pms_social`].
///
/// Composition implements this trait by forwarding each call to the owning
/// crate function with the same `connection`, so one transaction contains the
/// Workflow source, PMS reads, Identity recipients and Collaboration writes.
pub trait PmsSocialPorts: Send {
    /// Workflow-owned `pms_social_original`.
    fn original<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        event_id: &'a str,
        event_type: &'a str,
    ) -> PmsSocialFuture<'a, Option<crate::domain::pms_social_event::PmsSocialOriginal>>;

    /// PMS-owned `pms_social_payout_source`.
    fn payout_source<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: &'a str,
        property_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<String>>;

    /// PMS-owned `pms_social_source`.
    fn source<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: Option<&'a str>,
        company_id: &'a str,
        property_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<PmsSocialSource>>;

    /// Collaboration-owned `pms_social_lock`.
    fn lock<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post_id: &'a str,
    ) -> PmsSocialFuture<'a, ()>;

    /// Collaboration-owned `pms_social_received`.
    fn received<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        event_id: &'a str,
    ) -> PmsSocialFuture<'a, bool>;

    /// Collaboration-owned `pms_social_publish`.
    fn publish<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post: PmsSocialPost,
        source_id: &'a str,
        event_id: &'a str,
        received_at: i64,
    ) -> PmsSocialFuture<'a, ()>;

    /// Identity-owned `pms_social_recipients`.
    fn recipients<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        id: &'a str,
        company_id: &'a str,
        actor_id: &'a str,
        review: Option<&'a str>,
        recipient: Option<&'a str>,
    ) -> PmsSocialFuture<'a, Vec<String>>;

    /// Collaboration-owned `pms_social_version`.
    fn version<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        post_id: &'a str,
    ) -> PmsSocialFuture<'a, Option<i64>>;

    /// Collaboration-owned `pms_social_notify`.
    fn notify<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        notice: PmsSocialNotice,
    ) -> PmsSocialFuture<'a, ()>;
}

/// Project one PMS social event on the caller's transaction.
///
/// `Ok(None)` means the event type is not in the retained `pmsSocialEvents`
/// table, so the composition handler must leave it to the existing projection
/// families. Every event in the table is acknowledged (`Ok(Some(()))`), even
/// when the retained source/receipt filters write nothing.
pub async fn project_pms_social(
    connection: &mut PgConnection,
    ports: &mut impl PmsSocialPorts,
    message: &EventMessage,
    now: i64,
) -> Result<Option<()>, BusinessError> {
    let Some(event) = pms_social_event(&message.event_type) else {
        return Ok(None);
    };

    let Some(original) = ports
        .original(connection, &message.id, &message.event_type)
        .await?
    else {
        return Ok(Some(()));
    };

    let result: Value =
        serde_json::from_str(&original.result_json).map_err(|_| result_invalid())?;
    let result_id = result.get("id").and_then(Value::as_str);
    let property_id = result
        .get("propertyId")
        .and_then(Value::as_str)
        .unwrap_or(&message.aggregate_id);

    let source_id = if message.event_type == "pms.refund.pay" {
        ports
            .payout_source(connection, result_id.unwrap_or(""), property_id)
            .await?
    } else {
        Some(result_id.unwrap_or(&message.aggregate_id).to_owned())
    };
    let Some(source) = ports
        .source(
            connection,
            source_id.as_deref(),
            &message.payload.company_id,
            property_id,
        )
        .await?
    else {
        return Ok(Some(()));
    };

    if !matches!(
        source.kind.as_str(),
        "reservation" | "stay" | "task" | "transfer" | "correction" | "refund" | "shift" | "night"
    ) {
        return Ok(Some(()));
    }

    let post_id = pms_social_post_id(
        &source.company_id,
        &source.id,
        original.created_at,
        event.group,
    );
    ports.lock(connection, &post_id).await?;
    if ports.received(connection, &message.id).await? {
        return Ok(Some(()));
    }

    let body = pms_social_body(&source, event.label);
    ports
        .publish(
            connection,
            PmsSocialPost {
                post_id: post_id.clone(),
                company_id: source.company_id.clone(),
                branch_id: source.branch_id.clone(),
                actor_id: original.actor_id.clone(),
                body: body.clone(),
                command_id: original.command_id.clone(),
                at: original.created_at,
            },
            &source.id,
            &message.id,
            now,
        )
        .await?;

    let review = match message.event_type.as_str() {
        "pms.payment.request_transfer" => Some("finance.approve"),
        "pms.refund.request" | "pms.charge.correction.request" => Some("pms.approve"),
        "pms.task.complete" => Some("pms.inspect"),
        _ => None,
    };
    let recipient = ["recordedBy", "requestedBy", "assigneeId", "userId"]
        .iter()
        .find_map(|key| source.data_json.get(*key).and_then(Value::as_str));
    let recipients = ports
        .recipients(
            connection,
            &source.id,
            &source.company_id,
            &original.actor_id,
            review,
            recipient,
        )
        .await?;
    let version = ports.version(connection, &post_id).await?;
    for user_id in recipients {
        ports
            .notify(
                connection,
                PmsSocialNotice {
                    id: pms_social_notice_id(&post_id, &user_id),
                    company_id: source.company_id.clone(),
                    user_id,
                    post_id: post_id.clone(),
                    version,
                    body: body.clone(),
                    at: original.created_at,
                },
            )
            .await?;
    }

    Ok(Some(()))
}

fn result_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_SOCIAL_RESULT_INVALID",
        "The PMS social command result is not valid JSON.",
    )
}
