use erpx_kernel::Clock;
use thiserror::Error;

use crate::{
    CustomerAuthorityDomainError, CustomerPortalPrincipalBinding,
    ports::{
        CustomerRecordAuthorityPort, PortError, PortalBindingAuthorizationPort,
        PortalBindingCommandPayload, PortalBindingCommandRequest, PortalBindingCommandUnitOfWork,
        PortalBindingCommit, PortalBindingWriteRepository, StoredPortalBindingCommit,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortalBindingApplicationError {
    #[error(transparent)]
    Domain(#[from] CustomerAuthorityDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("portal binding create command cannot declare an existing version")]
    UnexpectedCreateVersion,
    #[error("portal binding mutation requires an expected version")]
    MissingExpectedVersion,
    #[error("portal binding version conflict (expected {expected}, actual {actual})")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("portal binding actor does not match the governed maker/checker identity")]
    ActorMismatch,
}

pub struct PortalBindingCommandHandler<R, U, A, V, C> {
    repository: R,
    unit_of_work: U,
    authorization: A,
    customer_authority: V,
    clock: C,
}

impl<R, U, A, V, C> PortalBindingCommandHandler<R, U, A, V, C> {
    pub const fn new(
        repository: R,
        unit_of_work: U,
        authorization: A,
        customer_authority: V,
        clock: C,
    ) -> Self {
        Self {
            repository,
            unit_of_work,
            authorization,
            customer_authority,
            clock,
        }
    }
}

impl<R, U, A, V, C> PortalBindingCommandHandler<R, U, A, V, C>
where
    R: PortalBindingWriteRepository,
    U: PortalBindingCommandUnitOfWork,
    A: PortalBindingAuthorizationPort,
    V: CustomerRecordAuthorityPort,
    C: Clock,
{
    pub async fn execute(
        &self,
        request: PortalBindingCommandRequest,
    ) -> Result<StoredPortalBindingCommit, PortalBindingApplicationError> {
        let action = request.identity.payload.action();
        if let Some(mut receipt) = self
            .unit_of_work
            .find_portal_binding_receipt(&request)
            .await?
        {
            self.authorize(&request, &receipt.aggregate).await?;
            receipt.replayed = true;
            return Ok(receipt);
        }

        let now = self.clock.now();
        let aggregate = match &request.identity.payload {
            PortalBindingCommandPayload::Propose(proposal) => {
                if request.expected_version.is_some() {
                    return Err(PortalBindingApplicationError::UnexpectedCreateVersion);
                }
                self.authorization
                    .authorize_portal_binding(
                        &request.actor,
                        request.actor_principal_id,
                        &request.authority_scope,
                        &proposal.scope,
                        action,
                    )
                    .await?;
                self.customer_authority
                    .ensure_active_customer_version(&proposal.customer, &proposal.scope)
                    .await?;
                CustomerPortalPrincipalBinding::propose(
                    proposal.binding_id,
                    proposal.customer.clone(),
                    proposal.portal_principal_id,
                    proposal.principal_kind,
                    proposal.scope.clone(),
                    proposal.verified_identity.clone(),
                    proposal.permitted_use_cases.clone(),
                    proposal.valid_from,
                    proposal.valid_until,
                    request.actor_principal_id,
                    now,
                )?
            }
            payload => {
                let mut aggregate = self
                    .repository
                    .load_portal_binding(&request.authority_scope, payload.binding_id())
                    .await?
                    .ok_or(PortError::NotFound)?;
                self.authorize(&request, &aggregate).await?;
                let expected = request
                    .expected_version
                    .ok_or(PortalBindingApplicationError::MissingExpectedVersion)?;
                if expected != aggregate.version() {
                    return Err(PortalBindingApplicationError::VersionConflict {
                        expected,
                        actual: aggregate.version(),
                    });
                }
                match payload {
                    PortalBindingCommandPayload::Approve {
                        decision_evidence_digest,
                        ..
                    } => {
                        self.customer_authority
                            .ensure_active_customer_version(aggregate.customer(), aggregate.scope())
                            .await?;
                        aggregate.approve(
                            request.actor_principal_id,
                            decision_evidence_digest,
                            now,
                        )?;
                    }
                    PortalBindingCommandPayload::Reject {
                        decision_evidence_digest,
                        ..
                    } => aggregate.reject(
                        request.actor_principal_id,
                        decision_evidence_digest,
                        now,
                    )?,
                    PortalBindingCommandPayload::Revoke {
                        decision_evidence_digest,
                        ..
                    } => aggregate.revoke(
                        request.actor_principal_id,
                        decision_evidence_digest,
                        now,
                    )?,
                    PortalBindingCommandPayload::Expire { .. } => aggregate.expire(now)?,
                    PortalBindingCommandPayload::Propose(_) => unreachable!(),
                }
                aggregate
            }
        };
        self.unit_of_work
            .commit_portal_binding(PortalBindingCommit {
                request,
                aggregate,
                changed_at: now,
            })
            .await
            .map_err(Into::into)
    }

    async fn authorize(
        &self,
        request: &PortalBindingCommandRequest,
        aggregate: &CustomerPortalPrincipalBinding,
    ) -> Result<(), PortalBindingApplicationError> {
        if request.identity.payload.binding_id() != aggregate.id() {
            return Err(PortalBindingApplicationError::ActorMismatch);
        }
        self.authorization
            .authorize_portal_binding(
                &request.actor,
                request.actor_principal_id,
                &request.authority_scope,
                aggregate.scope(),
                request.identity.payload.action(),
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        future::Future,
        sync::{Arc, Mutex},
        task::{Context, Poll, Wake, Waker},
    };

    use async_trait::async_trait;
    use chrono::{Duration, TimeZone, Utc};
    use erpx_cqrs::IdempotencyKey;
    use erpx_integration_contracts::{
        RecordTypeId, RecordVersionRef, StableRecordRef, catalog::ContextId,
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, AuthorityScope, CommandId, CorrelationId, FixedClock,
        PrincipalId, RecordId,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{
        CustomerPortalBindingId, PortalBindingAction, PortalBindingCommandIdentity,
        PortalBindingProposal, PortalPrincipalKind, PortalUseCase, VerifiedIdentityEvidence,
    };

    fn id(value: u128) -> Uuid {
        let mut bytes = value.to_be_bytes();
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, 8, 0, 0).single().unwrap()
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn principal(value: u128) -> PrincipalId {
        PrincipalId::try_from_uuid(id(value)).unwrap()
    }

    fn customer() -> RecordVersionRef {
        RecordVersionRef::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id(10)).unwrap(),
                ContextId::CustomerAuthority,
                RecordTypeId::try_new("customer.account").unwrap(),
            )
            .unwrap(),
            3,
        )
        .unwrap()
    }

    fn proposal() -> PortalBindingProposal {
        PortalBindingProposal {
            binding_id: CustomerPortalBindingId::try_from_uuid(id(20)).unwrap(),
            customer: customer(),
            portal_principal_id: principal(30),
            principal_kind: PortalPrincipalKind::Human,
            scope: scope(),
            verified_identity: VerifiedIdentityEvidence::try_new("a".repeat(64), 2, now()).unwrap(),
            permitted_use_cases: BTreeSet::from([PortalUseCase::FieldServiceRecordAccess]),
            valid_from: now(),
            valid_until: now() + Duration::days(30),
        }
    }

    fn request(
        sequence: u128,
        actor_principal_id: PrincipalId,
        payload: PortalBindingCommandPayload,
        expected_version: Option<u64>,
    ) -> PortalBindingCommandRequest {
        PortalBindingCommandRequest {
            identity: PortalBindingCommandIdentity {
                command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
                idempotency_key: IdempotencyKey::try_new(format!("portal-binding-{sequence}"))
                    .unwrap(),
                payload,
            },
            correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
            actor: ActorAttribution {
                principal_ref: format!("principal:{actor_principal_id}"),
                kind: ActorKind::PlatformUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            actor_principal_id,
            authority_scope: scope(),
            expected_version,
        }
    }

    #[derive(Clone, Default)]
    struct MemoryStore {
        aggregate: Arc<Mutex<Option<CustomerPortalPrincipalBinding>>>,
        receipts: Arc<Mutex<Vec<(PortalBindingCommandRequest, StoredPortalBindingCommit)>>>,
    }

    #[async_trait]
    impl PortalBindingWriteRepository for MemoryStore {
        async fn load_portal_binding(
            &self,
            _authority_scope: &AuthorityScope,
            binding_id: CustomerPortalBindingId,
        ) -> Result<Option<CustomerPortalPrincipalBinding>, PortError> {
            Ok(self
                .aggregate
                .lock()
                .unwrap()
                .as_ref()
                .filter(|value| value.id() == binding_id)
                .cloned())
        }
    }

    #[async_trait]
    impl PortalBindingCommandUnitOfWork for MemoryStore {
        async fn find_portal_binding_receipt(
            &self,
            request: &PortalBindingCommandRequest,
        ) -> Result<Option<StoredPortalBindingCommit>, PortError> {
            let identity = &request.identity;
            for (stored_request, receipt) in self.receipts.lock().unwrap().iter() {
                let stored_identity = &stored_request.identity;
                if stored_identity.command_id == identity.command_id
                    || stored_identity.idempotency_key == identity.idempotency_key
                {
                    return if stored_identity == identity
                        && stored_request.actor == request.actor
                        && stored_request.actor_principal_id == request.actor_principal_id
                        && stored_request.authority_scope == request.authority_scope
                        && stored_request.expected_version == request.expected_version
                    {
                        Ok(Some(receipt.clone()))
                    } else {
                        Err(PortError::IdempotencyConflict)
                    };
                }
            }
            Ok(None)
        }

        async fn commit_portal_binding(
            &self,
            commit: PortalBindingCommit,
        ) -> Result<StoredPortalBindingCommit, PortError> {
            let receipt = StoredPortalBindingCommit {
                command_id: commit.request.identity.command_id,
                correlation_id: commit.request.correlation_id,
                aggregate: commit.aggregate.clone(),
                committed_at: commit.changed_at,
                replayed: false,
            };
            *self.aggregate.lock().unwrap() = Some(commit.aggregate);
            self.receipts
                .lock()
                .unwrap()
                .push((commit.request, receipt.clone()));
            Ok(receipt)
        }
    }

    #[derive(Clone, Copy)]
    struct Authorization(bool);

    #[async_trait]
    impl PortalBindingAuthorizationPort for Authorization {
        async fn authorize_portal_binding(
            &self,
            _actor: &ActorAttribution,
            _actor_principal_id: PrincipalId,
            authority_scope: &AuthorityScope,
            target_scope: &AuthorityScope,
            _action: PortalBindingAction,
        ) -> Result<(), PortError> {
            (self.0 && authority_scope.contains(target_scope))
                .then_some(())
                .ok_or(PortError::Denied)
        }
    }

    #[derive(Clone, Copy)]
    struct CustomerAuthority(bool);

    #[async_trait]
    impl CustomerRecordAuthorityPort for CustomerAuthority {
        async fn ensure_active_customer_version(
            &self,
            value: &RecordVersionRef,
            _scope: &AuthorityScope,
        ) -> Result<(), PortError> {
            (self.0 && value == &customer())
                .then_some(())
                .ok_or(PortError::Denied)
        }
    }

    fn handler(
        store: MemoryStore,
        authorize: bool,
        customer_active: bool,
    ) -> PortalBindingCommandHandler<
        MemoryStore,
        MemoryStore,
        Authorization,
        CustomerAuthority,
        FixedClock,
    > {
        PortalBindingCommandHandler::new(
            store.clone(),
            store,
            Authorization(authorize),
            CustomerAuthority(customer_active),
            FixedClock::new(now()),
        )
    }

    #[test]
    fn propose_replays_exactly_and_rejects_payload_reuse() {
        let store = MemoryStore::default();
        let command = request(
            1,
            principal(40),
            PortalBindingCommandPayload::Propose(Box::new(proposal())),
            None,
        );
        let first = block_on(handler(store.clone(), true, true).execute(command.clone())).unwrap();
        let replay = block_on(handler(store.clone(), true, true).execute(command.clone())).unwrap();
        assert_eq!(first.aggregate.version(), 1);
        assert!(replay.replayed);

        let mut changed_actor = command.clone();
        changed_actor.actor_principal_id = principal(41);
        assert_eq!(
            block_on(handler(store.clone(), true, true).execute(changed_actor)),
            Err(PortalBindingApplicationError::Port(
                PortError::IdempotencyConflict
            ))
        );
        let mut changed = command;
        let PortalBindingCommandPayload::Propose(value) = &mut changed.identity.payload else {
            unreachable!()
        };
        value.valid_until += Duration::days(1);
        assert_eq!(
            block_on(handler(store, true, true).execute(changed)),
            Err(PortalBindingApplicationError::Port(
                PortError::IdempotencyConflict
            ))
        );
    }

    #[test]
    fn maker_checker_version_and_customer_authority_fail_closed() {
        let store = MemoryStore::default();
        let maker = principal(40);
        let proposed = request(
            2,
            maker,
            PortalBindingCommandPayload::Propose(Box::new(proposal())),
            None,
        );
        block_on(handler(store.clone(), true, true).execute(proposed)).unwrap();

        let approve = PortalBindingCommandPayload::Approve {
            binding_id: proposal().binding_id,
            decision_evidence_digest: "b".repeat(64),
        };
        assert!(matches!(
            block_on(handler(store.clone(), true, true).execute(request(
                3,
                maker,
                approve.clone(),
                Some(1),
            ))),
            Err(PortalBindingApplicationError::Domain(
                CustomerAuthorityDomainError::InvalidPortalBindingTransition
            ))
        ));
        assert_eq!(
            block_on(handler(store.clone(), true, true).execute(request(
                4,
                principal(41),
                approve.clone(),
                Some(99),
            ))),
            Err(PortalBindingApplicationError::VersionConflict {
                expected: 99,
                actual: 1,
            })
        );
        assert_eq!(
            block_on(handler(store.clone(), true, false).execute(request(
                5,
                principal(41),
                approve.clone(),
                Some(1),
            ))),
            Err(PortalBindingApplicationError::Port(PortError::Denied))
        );
        let active = block_on(handler(store, true, true).execute(request(
            6,
            principal(41),
            approve,
            Some(1),
        )))
        .unwrap();
        assert_eq!(active.aggregate.version(), 2);
        assert!(active.aggregate.is_effective_at(now()));
    }

    struct ThreadWake(std::thread::Thread);

    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::park(),
            }
        }
    }
}
