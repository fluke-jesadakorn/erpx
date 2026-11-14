//! Workflow approval oversight use cases.
//!
//! Composition supplies the live Account/Holding/Company authority. These use
//! cases authorize the owner read before any repository call, keep the exact
//! Company, and revalidate every persisted row before presentation. They never
//! open a transaction, resolve a session or read an HTTP request.
use crate::domain::approval_oversight::{
    ApprovalEvidence, ApprovalEvidenceQuery, ApprovalOversightActor, ApprovalQueue, validate_actor,
    validate_evidence, validate_queue,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type ApprovalOversightFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait ApprovalOversightRepository {
    type Error: From<BusinessError>;

    fn open_queue<'a>(
        &'a mut self,
        actor: &'a ApprovalOversightActor,
        now: i64,
    ) -> ApprovalOversightFuture<
        'a,
        Vec<crate::domain::approval_oversight::ApprovalQueueRow>,
        Self::Error,
    >;

    fn evidence<'a>(
        &'a mut self,
        actor: &'a ApprovalOversightActor,
        query: &'a ApprovalEvidenceQuery,
        now: i64,
    ) -> ApprovalOversightFuture<'a, Option<ApprovalEvidence>, Self::Error>;
}

/// Bounded open queue for the approval administration issue list.
pub async fn approval_queue<R: ApprovalOversightRepository>(
    repository: &mut R,
    actor: &ApprovalOversightActor,
    now: i64,
) -> Result<ApprovalQueue, R::Error> {
    validate_actor(actor)?;
    let rows = repository.open_queue(actor, now).await?;
    let mut queue = validate_queue(rows, actor)?;
    queue.evaluated_at = now;
    Ok(queue)
}

/// Immutable approval and command evidence for one Company record.
pub async fn approval_evidence<R: ApprovalOversightRepository>(
    repository: &mut R,
    actor: &ApprovalOversightActor,
    query: &ApprovalEvidenceQuery,
    now: i64,
) -> Result<Option<serde_json::Value>, R::Error> {
    validate_actor(actor)?;
    let evidence = repository.evidence(actor, query, now).await?;
    let Some(evidence) = evidence else {
        return Ok(None);
    };
    let evidence = validate_evidence(evidence, actor, query)?;
    Ok(Some(serde_json::json!({
        "evaluatedAt": now,
        "approval": {
            "instance": evidence.instance,
            "steps": evidence.steps,
            "history": evidence.history,
            "receipts": evidence.receipts,
            "audit": evidence.audit,
            "outbox": evidence.outbox,
        }
    })))
}

/// Typed form used by the owner-local HTTP backend. Keeping the queue typed
/// avoids re-reading JSON in transport tests; the evidence view is already a
/// bounded value because its nested arrays are validated above.
pub async fn approval_queue_value<R: ApprovalOversightRepository>(
    repository: &mut R,
    actor: &ApprovalOversightActor,
    now: i64,
) -> Result<serde_json::Value, R::Error> {
    let queue = approval_queue(repository, actor, now).await?;
    Ok(serde_json::json!({
        "evaluatedAt": queue.evaluated_at,
        "approvals": queue.approvals,
    }))
}
