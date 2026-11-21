//! Native Agent evidence audit-read use cases.
//!
//! The repository owns only Agent governance tables. The application requires
//! the current actor before any source read, applies session privacy and
//! revalidates every returned row before presentation. This is a read
//! projection; command idempotency, CAS, audit and outbox writes stay on the
//! retained TypeScript command path.
use crate::domain::agent_evidence::{
    AgentEvidenceActor, AgentEvidenceKind, AgentEvidenceRecord, AgentEvidenceWorkspace,
    AgentInvocationEvidence, AgentPrivateSession, not_found, validate_record, validate_workspace,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type AgentEvidenceFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait AgentEvidenceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn invocations<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> AgentEvidenceFuture<'a, Vec<AgentInvocationEvidence>, Self::Error>;

    fn sessions<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
        manage: bool,
    ) -> AgentEvidenceFuture<'a, Vec<AgentPrivateSession>, Self::Error>;

    fn invocation<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> AgentEvidenceFuture<'a, Option<AgentInvocationEvidence>, Self::Error>;

    fn session<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
        user_id: &'a str,
        manage: bool,
    ) -> AgentEvidenceFuture<'a, Option<AgentPrivateSession>, Self::Error>;
}

pub async fn agent_evidence_workspace<R: AgentEvidenceRepository>(
    actor: &AgentEvidenceActor,
    repository: &mut R,
    now: i64,
) -> Result<AgentEvidenceWorkspace, R::Error> {
    actor.require_read(now)?;
    let invocations = repository.invocations(&actor.company_id).await?;
    let sessions = repository
        .sessions(&actor.company_id, &actor.user_id, actor.can_manage())
        .await?;
    validate_workspace(actor, now, invocations, sessions).map_err(Into::into)
}

pub async fn agent_evidence_record<R: AgentEvidenceRepository>(
    actor: &AgentEvidenceActor,
    kind: AgentEvidenceKind,
    id: &str,
    repository: &mut R,
    now: i64,
) -> Result<AgentEvidenceRecord, R::Error> {
    actor.require_read(now)?;
    let row = match kind {
        AgentEvidenceKind::Invocation => repository
            .invocation(&actor.company_id, id)
            .await?
            .map(|record| AgentEvidenceRecord::Invocation { record })
            .ok_or_else(not_found)?,
        AgentEvidenceKind::Session => repository
            .session(&actor.company_id, id, &actor.user_id, actor.can_manage())
            .await?
            .map(|record| AgentEvidenceRecord::Session { record })
            .ok_or_else(not_found)?,
    };
    validate_record(actor, kind, row, now).map_err(Into::into)
}
