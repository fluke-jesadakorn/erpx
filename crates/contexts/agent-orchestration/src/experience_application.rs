use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, Clock, CommandId, CorrelationId,
    HumanDecisionProof,
};
use uuid::Uuid;

use crate::{
    application::AgentOrchestrationError,
    domain::{
        AgentPool, AgentPoolId, AgentSession, AgentSessionId, AgentVersionId, CompareRequest,
        PoolRouteRequest, PoolRoutingDecision, PrincipalKind, SessionContext, SessionMessage,
        SessionMessageId, session_message_publication_decision_ref,
    },
    ports::{
        AgentAuthorizationPort, AgentExperienceRepository, AgentExperienceUnitOfWork,
        AuthorizationRequest, OrchestrationAction, PoolCandidatePort, PoolCommit,
        PoolRoutingCommit, SessionCommit, SessionMutationReceipt,
    },
};

#[derive(Debug, Clone)]
pub struct RegisterAgentPool {
    pub pool_id: AgentPoolId,
    pub scope: AuthorityScope,
    pub name: String,
    pub member_versions: BTreeSet<AgentVersionId>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct ReplaceAgentPoolMembers {
    pub pool_id: AgentPoolId,
    pub expected_version: u64,
    pub member_versions: BTreeSet<AgentVersionId>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct ComparePoolSelection {
    pub initiating_human_ref: String,
    pub confirmed: bool,
    pub selected_versions: BTreeSet<AgentVersionId>,
    pub previewed_cost_micros: u64,
    pub max_fan_out: u16,
}

#[derive(Debug, Clone)]
pub struct RouteAgentPool {
    pub pool_id: AgentPoolId,
    pub expected_pool_version: u64,
    pub route: PoolRouteRequest,
    pub compare: Option<ComparePoolSelection>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct StartPrivateSession {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub session_id: AgentSessionId,
    pub scope: AuthorityScope,
    pub owner_human_ref: String,
    pub context: Option<SessionContext>,
    pub expires_at: DateTime<Utc>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct AppendPrivateSessionMessage {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub session_id: AgentSessionId,
    pub expected_version: u64,
    pub message_id: SessionMessageId,
    pub author_kind: PrincipalKind,
    pub author_ref: String,
    pub encrypted_payload_ref: String,
    pub content_digest: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct PublishPrivateSessionMessage {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub session_id: AgentSessionId,
    pub expected_version: u64,
    pub message_id: SessionMessageId,
    pub approving_human_ref: String,
    pub audience_reauthorized: bool,
    pub publication_anchor_ref: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct SetPrivateSessionLegalHold {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub session_id: AgentSessionId,
    pub expected_version: u64,
    pub enabled: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct ExpirePrivateSession {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub session_id: AgentSessionId,
    pub expected_version: u64,
    pub idempotency_key: String,
}

pub struct AgentExperienceService<R, U, K, A, C> {
    repository: R,
    unit_of_work: U,
    candidates: K,
    authorization: A,
    clock: C,
}

impl<R, U, K, A, C> AgentExperienceService<R, U, K, A, C> {
    pub const fn new(
        repository: R,
        unit_of_work: U,
        candidates: K,
        authorization: A,
        clock: C,
    ) -> Self {
        Self {
            repository,
            unit_of_work,
            candidates,
            authorization,
            clock,
        }
    }
}

impl<R, U, K, A, C> AgentExperienceService<R, U, K, A, C>
where
    R: AgentExperienceRepository,
    U: AgentExperienceUnitOfWork,
    K: PoolCandidatePort,
    A: AgentAuthorizationPort,
    C: Clock,
{
    pub async fn register_pool(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: RegisterAgentPool,
    ) -> Result<AgentPool, AgentOrchestrationError> {
        self.authorize(
            &actor,
            &authority_scope,
            &command.scope,
            OrchestrationAction::ManagePool,
        )
        .await?;
        let pool = AgentPool::register(
            command.pool_id,
            command.scope,
            command.name,
            command.member_versions,
            self.clock.now(),
        )?;
        self.unit_of_work
            .commit_pool(PoolCommit {
                actor,
                authority_scope,
                audit,
                expected_version: None,
                idempotency_key: command.idempotency_key,
                pool,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn replace_pool_members(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: ReplaceAgentPoolMembers,
    ) -> Result<AgentPool, AgentOrchestrationError> {
        let mut pool = self.load_pool(&authority_scope, command.pool_id).await?;
        ensure_version(pool.version(), command.expected_version)?;
        self.authorize(
            &actor,
            &authority_scope,
            pool.scope(),
            OrchestrationAction::ManagePool,
        )
        .await?;
        pool.replace_members(command.member_versions, self.clock.now())?;
        self.unit_of_work
            .commit_pool(PoolCommit {
                actor,
                authority_scope,
                audit,
                expected_version: Some(command.expected_version),
                idempotency_key: command.idempotency_key,
                pool,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn route_pool(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: RouteAgentPool,
    ) -> Result<PoolRoutingDecision, AgentOrchestrationError> {
        let pool = self.load_pool(&authority_scope, command.pool_id).await?;
        ensure_version(pool.version(), command.expected_pool_version)?;
        let action = if command.compare.is_some() {
            OrchestrationAction::ComparePool
        } else {
            OrchestrationAction::RoutePool
        };
        self.authorize(
            &actor,
            &authority_scope,
            &command.route.requested_scope,
            action,
        )
        .await?;
        let candidates = self
            .candidates
            .eligible_candidates(
                &authority_scope,
                &pool,
                &command.route.capability,
                &command.route.requested_scope,
            )
            .await?;
        let decision = match command.compare {
            None => pool.route(command.route, candidates)?,
            Some(compare) => {
                if !is_human(&actor) || actor.principal_ref != compare.initiating_human_ref {
                    return Err(AgentOrchestrationError::Port(
                        crate::ports::PortError::Denied,
                    ));
                }
                let selected = candidates
                    .into_iter()
                    .filter(|candidate| {
                        compare
                            .selected_versions
                            .contains(&candidate.pin.manifest.version_id)
                    })
                    .map(|candidate| candidate.pin)
                    .collect::<Vec<_>>();
                if selected.len() != compare.selected_versions.len() {
                    return Err(AgentOrchestrationError::NotFound);
                }
                pool.route_compare(
                    command.route,
                    CompareRequest {
                        initiating_human_ref: compare.initiating_human_ref,
                        confirmed: compare.confirmed,
                        pins: selected,
                        previewed_cost_micros: compare.previewed_cost_micros,
                    },
                    compare.max_fan_out,
                )?
            }
        };
        self.unit_of_work
            .commit_pool_routing(PoolRoutingCommit {
                actor,
                authority_scope,
                audit,
                idempotency_key: command.idempotency_key,
                decision,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn start_session(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: StartPrivateSession,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        if !is_human(&actor) || actor.principal_ref != command.owner_human_ref {
            return Err(AgentOrchestrationError::Port(
                crate::ports::PortError::Denied,
            ));
        }
        self.authorize(
            &actor,
            &authority_scope,
            &command.scope,
            OrchestrationAction::StartSession,
        )
        .await?;
        let session = AgentSession::start(
            command.session_id,
            command.scope,
            command.owner_human_ref,
            command.context,
            command.expires_at,
            self.clock.now(),
        )?;
        self.commit_session(
            actor,
            authority_scope,
            audit,
            command.command_id,
            command.correlation_id,
            None,
            command.idempotency_key,
            session,
            None,
        )
        .await
    }

    pub async fn append_session_message(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: AppendPrivateSessionMessage,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        let mut session = self
            .load_session(&authority_scope, command.session_id)
            .await?;
        ensure_version(session.version(), command.expected_version)?;
        self.authorize(
            &actor,
            &authority_scope,
            session.scope(),
            OrchestrationAction::AppendSessionMessage,
        )
        .await?;
        if actor.principal_ref != command.author_ref
            || (command.author_kind == PrincipalKind::Human && !is_human(&actor))
            || (command.author_kind == PrincipalKind::Agent && actor.kind != ActorKind::Agent)
        {
            return Err(AgentOrchestrationError::Port(
                crate::ports::PortError::Denied,
            ));
        }
        let at = self.clock.now();
        session.add_message(
            SessionMessage {
                id: command.message_id,
                author_kind: command.author_kind,
                author_ref: command.author_ref,
                encrypted_payload_ref: command.encrypted_payload_ref,
                content_digest: command.content_digest,
                version: 1,
                created_at: at,
                published_at: None,
                publishing_approver_human_ref: None,
                publication_anchor_ref: None,
            },
            at,
        )?;
        self.commit_session(
            actor,
            authority_scope,
            audit,
            command.command_id,
            command.correlation_id,
            Some(command.expected_version),
            command.idempotency_key,
            session,
            None,
        )
        .await
    }

    pub async fn publish_session_message(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: PublishPrivateSessionMessage,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        let at = self.clock.now();
        let human_decision = HumanDecisionProof::try_new(
            actor.clone(),
            authority_scope.clone(),
            session_message_publication_decision_ref(
                command.session_id,
                command.message_id,
                command.expected_version,
                command.command_id,
            ),
            at,
        )
        .map_err(|_| AgentOrchestrationError::Port(crate::ports::PortError::Denied))?;
        if human_decision.principal_ref() != command.approving_human_ref {
            return Err(AgentOrchestrationError::Port(
                crate::ports::PortError::Denied,
            ));
        }
        let mut session = self
            .load_session(&authority_scope, command.session_id)
            .await?;
        ensure_version(session.version(), command.expected_version)?;
        self.authorize(
            &actor,
            &authority_scope,
            session.scope(),
            OrchestrationAction::PublishSession,
        )
        .await?;
        session.publish_agent_message(
            command.message_id,
            command.audience_reauthorized,
            command.publication_anchor_ref,
            command.command_id,
            &human_decision,
        )?;
        self.commit_session(
            actor,
            authority_scope,
            audit,
            command.command_id,
            command.correlation_id,
            Some(command.expected_version),
            command.idempotency_key,
            session,
            Some(human_decision),
        )
        .await
    }

    pub async fn set_session_legal_hold(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: SetPrivateSessionLegalHold,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        let mut session = self
            .load_session(&authority_scope, command.session_id)
            .await?;
        ensure_version(session.version(), command.expected_version)?;
        self.authorize(
            &actor,
            &authority_scope,
            session.scope(),
            OrchestrationAction::HoldSession,
        )
        .await?;
        session.set_legal_hold(command.enabled, self.clock.now())?;
        self.commit_session(
            actor,
            authority_scope,
            audit,
            command.command_id,
            command.correlation_id,
            Some(command.expected_version),
            command.idempotency_key,
            session,
            None,
        )
        .await
    }

    pub async fn expire_session(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: ExpirePrivateSession,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        let mut session = self
            .load_session(&authority_scope, command.session_id)
            .await?;
        ensure_version(session.version(), command.expected_version)?;
        self.authorize(
            &actor,
            &authority_scope,
            session.scope(),
            OrchestrationAction::ExpireSession,
        )
        .await?;
        session.expire(self.clock.now())?;
        self.commit_session(
            actor,
            authority_scope,
            audit,
            command.command_id,
            command.correlation_id,
            Some(command.expected_version),
            command.idempotency_key,
            session,
            None,
        )
        .await
    }

    async fn load_pool(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentPoolId,
    ) -> Result<AgentPool, AgentOrchestrationError> {
        self.repository
            .load_pool(authority_scope, id)
            .await?
            .ok_or(AgentOrchestrationError::NotFound)
    }
    async fn load_session(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentSessionId,
    ) -> Result<AgentSession, AgentOrchestrationError> {
        self.repository
            .load_session(authority_scope, id)
            .await?
            .ok_or(AgentOrchestrationError::NotFound)
    }
    async fn authorize(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: OrchestrationAction,
    ) -> Result<(), AgentOrchestrationError> {
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target_scope,
                action,
                agent_definition_id: None,
                assignment_id: None,
            })
            .await
            .map_err(Into::into)
    }
    #[allow(clippy::too_many_arguments)] // Transaction metadata remains explicit at this boundary.
    async fn commit_session(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command_id: CommandId,
        correlation_id: CorrelationId,
        expected_version: Option<u64>,
        idempotency_key: String,
        session: AgentSession,
        human_decision: Option<HumanDecisionProof>,
    ) -> Result<SessionMutationReceipt, AgentOrchestrationError> {
        self.unit_of_work
            .commit_session(SessionCommit {
                command_id,
                correlation_id,
                actor,
                authority_scope,
                audit,
                human_decision,
                expected_version,
                idempotency_key,
                session,
            })
            .await
            .map_err(Into::into)
    }
}

fn ensure_version(actual: u64, expected: u64) -> Result<(), AgentOrchestrationError> {
    if actual == expected {
        Ok(())
    } else {
        Err(AgentOrchestrationError::VersionConflict)
    }
}

const fn is_human(actor: &ActorAttribution) -> bool {
    matches!(actor.kind, ActorKind::PlatformUser | ActorKind::HoldingUser)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use erpx_kernel::{FixedClock, SessionId};

    use super::*;
    use crate::{
        domain::{PoolCandidate, ProviderInvocationEvidence},
        ports::{InvocationEvidenceCommit, PortError},
    };

    #[derive(Clone, Default)]
    struct NeverPorts {
        loads: Arc<AtomicUsize>,
        authorizations: Arc<AtomicUsize>,
        commits: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AgentExperienceRepository for NeverPorts {
        async fn load_pool(
            &self,
            _: &AuthorityScope,
            _: AgentPoolId,
        ) -> Result<Option<AgentPool>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_session(
            &self,
            _: &AuthorityScope,
            _: AgentSessionId,
        ) -> Result<Option<AgentSession>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
    }

    #[async_trait]
    impl AgentAuthorizationPort for NeverPorts {
        async fn authorize(&self, _: AuthorizationRequest<'_>) -> Result<(), PortError> {
            self.authorizations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[async_trait]
    impl PoolCandidatePort for NeverPorts {
        async fn eligible_candidates(
            &self,
            _: &AuthorityScope,
            _: &AgentPool,
            _: &str,
            _: &AuthorityScope,
        ) -> Result<Vec<PoolCandidate>, PortError> {
            Ok(Vec::new())
        }
    }

    #[async_trait]
    impl AgentExperienceUnitOfWork for NeverPorts {
        async fn commit_pool(&self, _: PoolCommit) -> Result<AgentPool, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(PortError::Persistence)
        }

        async fn commit_pool_routing(
            &self,
            _: PoolRoutingCommit,
        ) -> Result<PoolRoutingDecision, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(PortError::Persistence)
        }

        async fn commit_session(
            &self,
            _: SessionCommit,
        ) -> Result<SessionMutationReceipt, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(PortError::Persistence)
        }

        async fn commit_invocation_evidence(
            &self,
            _: InvocationEvidenceCommit,
        ) -> Result<ProviderInvocationEvidence, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(PortError::Persistence)
        }
    }

    fn actor(kind: ActorKind) -> ActorAttribution {
        ActorAttribution {
            principal_ref: "human:owner".into(),
            kind,
            session_id: Some(SessionId::try_from_uuid(Uuid::from_u128(90)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    #[tokio::test]
    async fn publication_denies_non_direct_humans_before_load_authorization_or_commit() {
        let mut actors = vec![
            actor(ActorKind::Agent),
            actor(ActorKind::Service),
            actor(ActorKind::SystemWorker),
        ];
        let mut sessionless = actor(ActorKind::HoldingUser);
        sessionless.session_id = None;
        actors.push(sessionless);
        let mut represented = actor(ActorKind::HoldingUser);
        represented.represented_principal_ref = Some("human:represented".into());
        actors.push(represented);
        let at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();

        for actor in actors {
            let ports = NeverPorts::default();
            let service = AgentExperienceService::new(
                ports.clone(),
                ports.clone(),
                ports.clone(),
                ports.clone(),
                FixedClock::new(at),
            );
            let error = service
                .publish_session_message(
                    actor,
                    AuthorityScope::Holding {
                        holding_id: Uuid::from_u128(1),
                    },
                    AuditContext {
                        purpose: "publish private session message".into(),
                        customer_visible: false,
                        module: Some("agent-orchestration".into()),
                    },
                    PublishPrivateSessionMessage {
                        command_id: CommandId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
                        correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
                        session_id: AgentSessionId::try_from_uuid(Uuid::from_u128(4)).unwrap(),
                        expected_version: 2,
                        message_id: SessionMessageId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
                        approving_human_ref: "human:owner".into(),
                        audience_reauthorized: true,
                        publication_anchor_ref: Uuid::from_u128(6),
                        idempotency_key: "publish-agent-message".into(),
                    },
                )
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                AgentOrchestrationError::Port(PortError::Denied)
            ));
            assert_eq!(ports.loads.load(Ordering::SeqCst), 0);
            assert_eq!(ports.authorizations.load(Ordering::SeqCst), 0);
            assert_eq!(ports.commits.load(Ordering::SeqCst), 0);
        }
    }
}
