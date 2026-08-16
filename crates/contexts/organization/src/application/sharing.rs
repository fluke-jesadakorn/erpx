use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    OrganizationDomainError, OrganizationSharingAgreement,
    ports::{
        PortError, SharingAction, SharingAuthorizationPort, SharingAuthorizationRequest,
        SharingCommandPayload, SharingCommandRequest, SharingCommandUnitOfWork, SharingCommit,
        SharingWriteRepository, StoredSharingCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SharingApplicationError {
    #[error(transparent)]
    Domain(#[from] OrganizationDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("sharing agreement was not found")]
    NotFound,
    #[error("sharing create cannot declare an existing version")]
    UnexpectedCreateVersion,
    #[error("sharing mutation requires the exact expected version")]
    MissingExpectedVersion,
    #[error("sharing authority does not cover the required agreement side")]
    ScopeOutsideAuthority,
    #[error("sharing payload actor does not match the authenticated actor")]
    ActorMismatch,
}

pub struct OrganizationSharingCommandHandler<W, U, A, C> {
    repository: W,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<W, U, A, C> OrganizationSharingCommandHandler<W, U, A, C> {
    pub const fn new(repository: W, unit_of_work: U, authorization: A, clock: C) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<W, U, A, C> OrganizationSharingCommandHandler<W, U, A, C>
where
    W: SharingWriteRepository,
    U: SharingCommandUnitOfWork,
    A: SharingAuthorizationPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: SharingCommandRequest,
    ) -> Result<StoredSharingCommit, SharingApplicationError> {
        if let Some(receipt) = self
            .unit_of_work
            .find_sharing_receipt(&request.identity)
            .await?
        {
            self.authorize(&request, &receipt.aggregate).await?;
            return Ok(receipt);
        }

        let now = self.clock.now();
        let aggregate = match &request.identity.payload {
            SharingCommandPayload::Propose(command) => {
                if request.expected_version.is_some() {
                    return Err(SharingApplicationError::UnexpectedCreateVersion);
                }
                if command.id != request.identity.sharing_agreement_id {
                    return Err(SharingApplicationError::ActorMismatch);
                }
                ensure_actor(&request, &command.proposed_by)?;
                let aggregate = OrganizationSharingAgreement::propose(command.as_ref().clone())?;
                self.authorize(&request, &aggregate).await?;
                aggregate
            }
            _ => {
                let expected = request
                    .expected_version
                    .ok_or(SharingApplicationError::MissingExpectedVersion)?;
                let mut aggregate = self
                    .repository
                    .load_sharing_agreement(
                        &request.authority_scope,
                        request.identity.sharing_agreement_id,
                    )
                    .await?
                    .ok_or(SharingApplicationError::NotFound)?;
                self.authorize(&request, &aggregate).await?;
                match &request.identity.payload {
                    SharingCommandPayload::ApproveSource { approver_ref } => {
                        ensure_actor(&request, approver_ref)?;
                        aggregate.approve_source(expected, approver_ref)?;
                    }
                    SharingCommandPayload::ApproveTarget { approver_ref } => {
                        ensure_actor(&request, approver_ref)?;
                        aggregate.approve_target(expected, approver_ref)?;
                    }
                    SharingCommandPayload::Reject { actor_ref } => {
                        ensure_actor(&request, actor_ref)?;
                        aggregate.reject(expected, actor_ref, now)?;
                    }
                    SharingCommandPayload::Revoke { actor_ref } => {
                        ensure_actor(&request, actor_ref)?;
                        aggregate.revoke(expected, actor_ref, now)?;
                    }
                    SharingCommandPayload::Expire => {
                        aggregate.expire(expected, now)?;
                    }
                    SharingCommandPayload::Propose(_) => unreachable!("handled above"),
                }
                aggregate
            }
        };
        // Revalidates same-holding and lifecycle invariants before persistence.
        if aggregate.id() != request.identity.sharing_agreement_id {
            return Err(SharingApplicationError::ActorMismatch);
        }
        self.unit_of_work
            .commit_sharing(SharingCommit {
                request,
                aggregate,
                changed_at: now,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &SharingCommandRequest,
        aggregate: &OrganizationSharingAgreement,
    ) -> Result<(), SharingApplicationError> {
        let required = match request.identity.payload.action() {
            SharingAction::Propose | SharingAction::ApproveSource | SharingAction::Revoke => {
                aggregate.source_scope()
            }
            SharingAction::ApproveTarget => aggregate.target_scope(),
            SharingAction::Reject | SharingAction::Expire => {
                if request.authority_scope.contains(aggregate.source_scope()) {
                    aggregate.source_scope()
                } else {
                    aggregate.target_scope()
                }
            }
        };
        if !request.authority_scope.contains(required) {
            return Err(SharingApplicationError::ScopeOutsideAuthority);
        }
        self.authorization
            .authorize(SharingAuthorizationRequest {
                actor: &request.actor,
                authority_scope: &request.authority_scope,
                source_scope: aggregate.source_scope(),
                target_scope: aggregate.target_scope(),
                action: request.identity.payload.action(),
            })
            .await?;
        Ok(())
    }
}

fn ensure_actor(
    request: &SharingCommandRequest,
    actor_ref: &str,
) -> Result<(), SharingApplicationError> {
    if actor_ref != request.actor.principal_ref {
        return Err(SharingApplicationError::ActorMismatch);
    }
    Ok(())
}
