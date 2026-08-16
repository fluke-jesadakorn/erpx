//! Explicit record-policy evaluation for Item Directory effective grants.
//!
//! This module accepts only current IAM and Organization evidence supplied by
//! trusted adapters. It never turns organizational scope, a role name, or a
//! descriptor label into authority by itself.

use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_integration_contracts::{
    EffectiveGrantChanged, EffectiveGrantRevoked, ItemDescriptor, ItemDirectoryFact,
    RecordPolicyPrincipalKind, RecordPolicyRealm, RecordPolicyScopeRule, record_policy,
};
use erpx_kernel::{DataClassification, PrincipalId, TenantScope};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_RECORD_POLICY_FANOUT: usize = 1_000;
pub const MAX_EFFECTIVE_GRANT_TTL: Duration = Duration::minutes(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IamAuthorityEvidence {
    pub principal_id: PrincipalId,
    pub principal_kind: RecordPolicyPrincipalKind,
    pub realm: RecordPolicyRealm,
    pub principal_active: bool,
    pub principal_version: u64,
    pub role_version: u64,
    pub assignment_version: u64,
    pub permission: String,
    pub assignment_scope: TenantScope,
    pub classification_ceiling: DataClassification,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    /// Required only when the evaluated subject was expanded from a governed
    /// human group or agent pool. Individual principals leave this `None`.
    pub membership: Option<MembershipEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MembershipEvidence {
    pub subject_kind: RecordPolicyPrincipalKind,
    pub version: u64,
    pub valid_until: DateTime<Utc>,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationScopeEvidenceKind {
    PublishedAncestor,
    TwoSidedSharingAgreement,
}

/// Final Organization decision. The policy producer does not attempt to
/// reconstruct hierarchy or sharing agreements from labels or IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationScopeEvidence {
    pub kind: OrganizationScopeEvidenceKind,
    pub actor_scope: TenantScope,
    pub record_scope: TenantScope,
    pub permits_open_record: bool,
    pub version: u64,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub sharing_agreement_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordGrantEvaluation {
    pub descriptor: ItemDescriptor,
    pub iam: IamAuthorityEvidence,
    pub organization: Option<OrganizationScopeEvidence>,
    pub evaluated_at: DateTime<Utc>,
    pub decision_source_position: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RecordGrantDecisionProducer;

impl RecordGrantDecisionProducer {
    pub fn evaluate(
        &self,
        evaluation: &RecordGrantEvaluation,
    ) -> Result<ItemDirectoryFact, RecordPolicyDecisionError> {
        evaluation
            .descriptor
            .validate()
            .map_err(|_| RecordPolicyDecisionError::MalformedEvidence)?;
        if evaluation.decision_source_position == 0
            || evaluation.decision_source_position < evaluation.descriptor.source_event_position()
        {
            return Err(RecordPolicyDecisionError::MalformedEvidence);
        }
        let definition = record_policy(evaluation.descriptor.record().record_type().as_str())
            .ok_or(RecordPolicyDecisionError::UnknownPolicy)?;
        let principal_id = evaluation.iam.principal_id;
        let allowed = self.is_allowed(evaluation, definition);
        if !allowed {
            return Ok(ItemDirectoryFact::EffectiveGrantRevoked(
                EffectiveGrantRevoked {
                    record_id: evaluation.descriptor.record().id(),
                    principal_id,
                    authorization_policy: definition.policy_id(),
                    policy_version: definition.version,
                    source_event_position: evaluation.decision_source_position,
                },
            ));
        }

        let mut expires_at = evaluation.iam.valid_until;
        if let Some(scope) = &evaluation.organization {
            expires_at = expires_at.min(scope.valid_until);
        }
        if let Some(membership) = evaluation.iam.membership {
            expires_at = expires_at.min(membership.valid_until);
        }
        expires_at = expires_at.min(evaluation.evaluated_at + MAX_EFFECTIVE_GRANT_TTL);
        if expires_at <= evaluation.evaluated_at {
            return Ok(ItemDirectoryFact::EffectiveGrantRevoked(
                EffectiveGrantRevoked {
                    record_id: evaluation.descriptor.record().id(),
                    principal_id,
                    authorization_policy: definition.policy_id(),
                    policy_version: definition.version,
                    source_event_position: evaluation.decision_source_position,
                },
            ));
        }
        let fact = ItemDirectoryFact::EffectiveGrantChanged(EffectiveGrantChanged {
            record: evaluation.descriptor.record().clone(),
            principal_id,
            authorization_policy: definition.policy_id(),
            policy_version: definition.version,
            effective_at: evaluation.evaluated_at,
            expires_at,
            source_event_position: evaluation.decision_source_position,
        });
        fact.validate()
            .map_err(|_| RecordPolicyDecisionError::MalformedEvidence)?;
        Ok(fact)
    }

    pub fn evaluate_fanout(
        &self,
        evaluations: &[RecordGrantEvaluation],
    ) -> Result<Vec<ItemDirectoryFact>, RecordPolicyDecisionError> {
        if evaluations.is_empty() || evaluations.len() > MAX_RECORD_POLICY_FANOUT {
            return Err(RecordPolicyDecisionError::FanoutLimit);
        }
        let mut identities = BTreeSet::new();
        let mut facts = Vec::with_capacity(evaluations.len());
        for evaluation in evaluations {
            if !identities.insert((
                evaluation.descriptor.record().id(),
                evaluation.iam.principal_id,
            )) {
                return Err(RecordPolicyDecisionError::DuplicateDecision);
            }
            facts.push(self.evaluate(evaluation)?);
        }
        Ok(facts)
    }

    fn is_allowed(
        &self,
        evaluation: &RecordGrantEvaluation,
        definition: &erpx_integration_contracts::RecordPolicyDefinition,
    ) -> bool {
        let iam = &evaluation.iam;
        let at = evaluation.evaluated_at;
        if !definition.is_active_at(at.timestamp())
            || evaluation.descriptor.authorization_policy() != definition.policy_id()
            || evaluation.descriptor.authorization_policy_version() != definition.version
            || !evaluation
                .descriptor
                .classification()
                .is_permitted_by(definition.classification_ceiling)
            || !evaluation
                .descriptor
                .classification()
                .is_permitted_by(iam.classification_ceiling)
            || !iam.principal_active
            || iam.principal_version == 0
            || iam.role_version == 0
            || iam.assignment_version == 0
            || !definition.permits_access(&iam.permission, iam.principal_kind, iam.realm)
            || iam.valid_from > at
            || iam.valid_until <= at
            || iam.revoked_at.is_some_and(|revoked_at| revoked_at <= at)
        {
            return false;
        }
        if matches!(
            iam.principal_kind,
            RecordPolicyPrincipalKind::Agent
                | RecordPolicyPrincipalKind::HumanGroup
                | RecordPolicyPrincipalKind::AgentPool
        ) {
            // AI agents execute with an invoking human's effective authority;
            // groups and pools must be expanded to current members. None gets
            // a direct record grant merely because it is addressable.
            return false;
        }
        if let Some(membership) = iam.membership
            && (!membership.active
                || membership.version == 0
                || membership.valid_until <= at
                || !matches!(
                    membership.subject_kind,
                    RecordPolicyPrincipalKind::HumanGroup | RecordPolicyPrincipalKind::AgentPool
                ))
        {
            return false;
        }
        match definition.scope_rule {
            RecordPolicyScopeRule::HierarchyOrExplicitSharedBranch => {
                scope_is_permitted(evaluation)
            }
        }
    }
}

fn scope_is_permitted(evaluation: &RecordGrantEvaluation) -> bool {
    if evaluation
        .iam
        .assignment_scope
        .contains(evaluation.descriptor.scope())
    {
        return true;
    }
    let Some(evidence) = &evaluation.organization else {
        return false;
    };
    if evidence.actor_scope != evaluation.iam.assignment_scope
        || &evidence.record_scope != evaluation.descriptor.scope()
        || !evidence.permits_open_record
        || evidence.version == 0
        || evidence.valid_from > evaluation.evaluated_at
        || evidence.valid_until <= evaluation.evaluated_at
        || evidence
            .revoked_at
            .is_some_and(|revoked_at| revoked_at <= evaluation.evaluated_at)
        || holding_id(&evidence.actor_scope) != holding_id(&evidence.record_scope)
    {
        return false;
    }
    match evidence.kind {
        OrganizationScopeEvidenceKind::PublishedAncestor => {
            evidence.record_scope.contains(&evidence.actor_scope)
                && evidence.sharing_agreement_id.is_none()
        }
        OrganizationScopeEvidenceKind::TwoSidedSharingAgreement => {
            sibling_branches(&evidence.actor_scope, &evidence.record_scope)
                && evidence
                    .sharing_agreement_id
                    .is_some_and(|agreement_id| !agreement_id.is_nil())
        }
    }
}

fn holding_id(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Platform => None,
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. } => Some(*holding_id),
    }
}

fn sibling_branches(left: &TenantScope, right: &TenantScope) -> bool {
    match (left, right) {
        (
            TenantScope::Branch {
                holding_id: left_holding,
                branch_id: left_branch,
                ..
            },
            TenantScope::Branch {
                holding_id: right_holding,
                branch_id: right_branch,
                ..
            },
        ) => left_holding == right_holding && left_branch != right_branch,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RecordPolicyDecisionError {
    #[error("record policy is not registered")]
    UnknownPolicy,
    #[error("record policy evidence is malformed")]
    MalformedEvidence,
    #[error("record policy fanout is outside its bound")]
    FanoutLimit,
    #[error("record policy fanout contains a duplicate decision")]
    DuplicateDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordPolicyTrigger {
    PrincipalAccessChanged,
    PrincipalDisabled,
    RoleDefinitionChanged,
    GroupMembershipChanged,
    PolicySuperseded,
    DescriptorChanged,
    OrganizationSharingChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPolicyDecisionClaim {
    pub claim_id: Uuid,
    pub claim_version: u64,
    pub trigger: RecordPolicyTrigger,
    pub lease_until: DateTime<Utc>,
    pub evaluations: Vec<RecordGrantEvaluation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordPolicyReleaseKind {
    Retryable,
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RecordPolicyRuntimeError {
    #[error("record-policy decision source is unavailable")]
    SourceUnavailable,
    #[error("record-policy fact sink is unavailable")]
    SinkUnavailable,
}

#[async_trait]
pub trait RecordPolicyDecisionSource: Send + Sync {
    async fn claim(
        &self,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<Vec<RecordPolicyDecisionClaim>, RecordPolicyRuntimeError>;

    async fn complete(
        &self,
        claim_id: Uuid,
        claim_version: u64,
    ) -> Result<(), RecordPolicyRuntimeError>;

    async fn release(
        &self,
        claim_id: Uuid,
        claim_version: u64,
        kind: RecordPolicyReleaseKind,
    ) -> Result<(), RecordPolicyRuntimeError>;
}

#[async_trait]
pub trait RecordPolicyFactSink: Send + Sync {
    /// `claim_id` plus `claim_version` is the durable idempotency identity. A
    /// replay must return success only for the exact same fact batch.
    async fn publish(
        &self,
        claim_id: Uuid,
        claim_version: u64,
        facts: &[ItemDirectoryFact],
    ) -> Result<(), RecordPolicyRuntimeError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecordPolicyDecisionRunReport {
    pub claimed: usize,
    pub published: usize,
    pub completed: usize,
    pub released: usize,
}

#[derive(Debug)]
pub struct RecordPolicyDecisionHandler<S, P> {
    source: S,
    sink: P,
    batch_size: usize,
    producer: RecordGrantDecisionProducer,
}

impl<S, P> RecordPolicyDecisionHandler<S, P>
where
    S: RecordPolicyDecisionSource,
    P: RecordPolicyFactSink,
{
    pub fn new(source: S, sink: P, batch_size: usize) -> Result<Self, RecordPolicyDecisionError> {
        if batch_size == 0 || batch_size > MAX_RECORD_POLICY_FANOUT {
            return Err(RecordPolicyDecisionError::FanoutLimit);
        }
        Ok(Self {
            source,
            sink,
            batch_size,
            producer: RecordGrantDecisionProducer,
        })
    }

    pub async fn run_once(
        &self,
        now: DateTime<Utc>,
    ) -> Result<RecordPolicyDecisionRunReport, RecordPolicyRuntimeError> {
        let claims = self.source.claim(self.batch_size, now).await?;
        if claims.len() > self.batch_size {
            return Err(RecordPolicyRuntimeError::SourceUnavailable);
        }
        let mut report = RecordPolicyDecisionRunReport {
            claimed: claims.len(),
            ..RecordPolicyDecisionRunReport::default()
        };
        for claim in claims {
            let valid_claim = !claim.claim_id.is_nil()
                && claim.claim_version > 0
                && claim.lease_until > now
                && !claim.evaluations.is_empty()
                && claim.evaluations.len() <= MAX_RECORD_POLICY_FANOUT;
            let facts = valid_claim
                .then(|| self.producer.evaluate_fanout(&claim.evaluations))
                .transpose();
            let Ok(Some(facts)) = facts else {
                self.source
                    .release(
                        claim.claim_id,
                        claim.claim_version,
                        RecordPolicyReleaseKind::Permanent,
                    )
                    .await?;
                report.released += 1;
                continue;
            };
            if self
                .sink
                .publish(claim.claim_id, claim.claim_version, &facts)
                .await
                .is_err()
            {
                self.source
                    .release(
                        claim.claim_id,
                        claim.claim_version,
                        RecordPolicyReleaseKind::Retryable,
                    )
                    .await?;
                report.released += 1;
                continue;
            }
            report.published += facts.len();
            self.source
                .complete(claim.claim_id, claim.claim_version)
                .await?;
            report.completed += 1;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use erpx_integration_contracts::{
        CURRENT_RECORD_POLICIES, FUTURE_RECORD_POLICIES, ItemDescriptor, RecordTypeId,
        StableRecordRef, record_policies,
    };
    use erpx_kernel::{PolicyId, RecordId};

    use super::*;

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    fn branch(holding_id: Uuid, company_id: Uuid, branch_id: Uuid) -> TenantScope {
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        }
    }

    fn evaluation(
        definition: &erpx_integration_contracts::RecordPolicyDefinition,
    ) -> RecordGrantEvaluation {
        let now = Utc::now();
        let scope = TenantScope::Holding { holding_id: id() };
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(id()).unwrap(),
            definition.owner,
            RecordTypeId::try_new(definition.record_type).unwrap(),
        )
        .unwrap();
        let descriptor = ItemDescriptor::try_new(
            record,
            scope.clone(),
            "active",
            definition.policy_id(),
            definition.classification_ceiling,
            2,
            10,
        )
        .unwrap();
        RecordGrantEvaluation {
            descriptor,
            iam: IamAuthorityEvidence {
                principal_id: PrincipalId::try_from_uuid(id()).unwrap(),
                principal_kind: RecordPolicyPrincipalKind::Human,
                realm: definition.realms[0],
                principal_active: true,
                principal_version: 2,
                role_version: 3,
                assignment_version: 4,
                permission: definition.permission.into(),
                assignment_scope: scope,
                classification_ceiling: DataClassification::Restricted,
                valid_from: now - Duration::minutes(1),
                valid_until: now + Duration::hours(1),
                revoked_at: None,
                membership: None,
            },
            organization: None,
            evaluated_at: now,
            decision_source_position: 20,
        }
    }

    #[test]
    fn every_live_policy_can_produce_a_finite_explicit_grant() {
        let producer = RecordGrantDecisionProducer;
        for definition in record_policies() {
            let fact = producer.evaluate(&evaluation(definition)).unwrap();
            let ItemDirectoryFact::EffectiveGrantChanged(grant) = fact else {
                panic!("{} should be granted", definition.record_type);
            };
            assert_eq!(grant.authorization_policy, definition.policy_id());
            assert_eq!(grant.policy_version, definition.version);
            assert!(grant.expires_at <= grant.effective_at + MAX_EFFECTIVE_GRANT_TTL);
        }
    }

    #[test]
    fn permission_expiry_revocation_classification_and_agent_paths_fail_closed() {
        let definition = record_policy("finance.invoice").unwrap();
        let producer = RecordGrantDecisionProducer;
        let mut value = evaluation(definition);
        value.iam.permission = "sales.read".into();
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
        value = evaluation(definition);
        value.iam.valid_until = value.evaluated_at;
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
        value = evaluation(definition);
        value.iam.revoked_at = Some(value.evaluated_at);
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
        value = evaluation(definition);
        value.iam.classification_ceiling = DataClassification::Confidential;
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
        value = evaluation(definition);
        value.iam.principal_kind = RecordPolicyPrincipalKind::Agent;
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
    }

    #[test]
    fn future_context_realm_permission_pairs_are_exact_and_agents_never_gain_authority() {
        let producer = RecordGrantDecisionProducer;

        for record_type in [
            "manufacturing.order",
            "planning.scenario",
            "project.item",
            "learning.course",
            "benefits.election",
            "agent.assignment",
        ] {
            let definition = record_policy(record_type).unwrap();
            let mut wrong_realm = evaluation(definition);
            wrong_realm.iam.realm = RecordPolicyRealm::CustomerPortal;
            assert!(matches!(
                producer.evaluate(&wrong_realm).unwrap(),
                ItemDirectoryFact::EffectiveGrantRevoked(_)
            ));
        }

        for (record_type, permission) in [
            ("commerce.order", "commerce.customer.order.read"),
            ("talent.application", "talent.candidate.self.read"),
            ("field-service.request", "field_service.customer.read"),
            ("collaboration.thread", "collaboration.portal.read"),
        ] {
            let definition = record_policy(record_type).unwrap();
            let mut external = evaluation(definition);
            external.iam.realm = RecordPolicyRealm::CustomerPortal;
            external.iam.permission = permission.into();
            assert!(matches!(
                producer.evaluate(&external).unwrap(),
                ItemDirectoryFact::EffectiveGrantChanged(_)
            ));
            external.iam.realm = RecordPolicyRealm::InternalWorkforce;
            assert!(matches!(
                producer.evaluate(&external).unwrap(),
                ItemDirectoryFact::EffectiveGrantRevoked(_)
            ));
        }

        let catalog = record_policy("commerce.catalog-publication").unwrap();
        for permission in [
            "commerce.customer.catalog.read",
            "commerce.guest.catalog.read",
        ] {
            let mut external = evaluation(catalog);
            external.iam.realm = RecordPolicyRealm::CustomerPortal;
            external.iam.permission = permission.into();
            assert!(matches!(
                producer.evaluate(&external).unwrap(),
                ItemDirectoryFact::EffectiveGrantChanged(_)
            ));
        }

        let mut agent = evaluation(record_policy("agent.assignment").unwrap());
        agent.iam.principal_kind = RecordPolicyPrincipalKind::Agent;
        assert!(matches!(
            producer.evaluate(&agent).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
    }

    #[test]
    fn project_cross_branch_grant_requires_live_two_sided_sharing() {
        let definition = record_policy("project.item").unwrap();
        let producer = RecordGrantDecisionProducer;
        let mut value = evaluation(definition);
        let holding = id();
        let company = id();
        value.iam.assignment_scope = branch(holding, company, id());
        let target = branch(holding, company, id());
        value.descriptor = ItemDescriptor::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id()).unwrap(),
                definition.owner,
                RecordTypeId::try_new(definition.record_type).unwrap(),
            )
            .unwrap(),
            target.clone(),
            "open",
            definition.policy_id(),
            definition.classification_ceiling,
            3,
            11,
        )
        .unwrap();
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
        value.organization = Some(OrganizationScopeEvidence {
            kind: OrganizationScopeEvidenceKind::TwoSidedSharingAgreement,
            actor_scope: value.iam.assignment_scope.clone(),
            record_scope: target,
            permits_open_record: true,
            version: 2,
            valid_from: value.evaluated_at - Duration::minutes(1),
            valid_until: value.evaluated_at + Duration::minutes(5),
            revoked_at: None,
            sharing_agreement_id: Some(id()),
        });
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantChanged(_)
        ));
    }

    #[test]
    fn sibling_branch_requires_current_two_sided_open_record_evidence() {
        let definition = record_policy("support.customer-case").unwrap();
        let producer = RecordGrantDecisionProducer;
        let mut value = evaluation(definition);
        let holding = id();
        let company = id();
        value.iam.assignment_scope = branch(holding, company, id());
        let target = branch(holding, company, id());
        value.descriptor = ItemDescriptor::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id()).unwrap(),
                definition.owner,
                RecordTypeId::try_new(definition.record_type).unwrap(),
            )
            .unwrap(),
            target.clone(),
            "open",
            definition.policy_id(),
            definition.classification_ceiling,
            3,
            11,
        )
        .unwrap();
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
        value.organization = Some(OrganizationScopeEvidence {
            kind: OrganizationScopeEvidenceKind::TwoSidedSharingAgreement,
            actor_scope: value.iam.assignment_scope.clone(),
            record_scope: target,
            permits_open_record: true,
            version: 3,
            valid_from: value.evaluated_at - Duration::minutes(1),
            valid_until: value.evaluated_at + Duration::minutes(10),
            revoked_at: None,
            sharing_agreement_id: Some(id()),
        });
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantChanged(_)
        ));
        value.organization.as_mut().unwrap().revoked_at = Some(value.evaluated_at);
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
    }

    #[test]
    fn cross_holding_and_stale_record_decisions_are_denied() {
        let definition = record_policy("legal.contract").unwrap();
        let producer = RecordGrantDecisionProducer;
        let mut value = evaluation(definition);
        let actor_holding = id();
        let target_holding = id();
        value.iam.assignment_scope = branch(actor_holding, id(), id());
        let target = branch(target_holding, id(), id());
        value.descriptor = ItemDescriptor::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id()).unwrap(),
                definition.owner,
                RecordTypeId::try_new(definition.record_type).unwrap(),
            )
            .unwrap(),
            target.clone(),
            "active",
            definition.policy_id(),
            definition.classification_ceiling,
            4,
            30,
        )
        .unwrap();
        value.decision_source_position = 29;
        assert_eq!(
            producer.evaluate(&value),
            Err(RecordPolicyDecisionError::MalformedEvidence)
        );
        value.decision_source_position = 31;
        value.organization = Some(OrganizationScopeEvidence {
            kind: OrganizationScopeEvidenceKind::TwoSidedSharingAgreement,
            actor_scope: value.iam.assignment_scope.clone(),
            record_scope: target,
            permits_open_record: true,
            version: 2,
            valid_from: value.evaluated_at - Duration::minutes(1),
            valid_until: value.evaluated_at + Duration::minutes(5),
            revoked_at: None,
            sharing_agreement_id: Some(id()),
        });
        assert!(matches!(
            producer.evaluate(&value).unwrap(),
            ItemDirectoryFact::EffectiveGrantRevoked(_)
        ));
    }

    #[test]
    fn fanout_is_bounded_unique_and_replay_deterministic() {
        let definition = record_policy("finance.invoice").unwrap();
        let producer = RecordGrantDecisionProducer;
        let value = evaluation(definition);
        assert_eq!(producer.evaluate(&value), producer.evaluate(&value));
        assert_eq!(
            producer.evaluate_fanout(&[value.clone(), value]),
            Err(RecordPolicyDecisionError::DuplicateDecision)
        );
        assert_eq!(
            producer.evaluate_fanout(&[]),
            Err(RecordPolicyDecisionError::FanoutLimit)
        );
    }

    #[test]
    fn no_policy_uses_wildcards_named_users_or_generic_admin_bypass() {
        assert_eq!(CURRENT_RECORD_POLICIES.len(), 92);
        assert_eq!(FUTURE_RECORD_POLICIES.len(), 49);
        assert_eq!(record_policies().count(), 141);
        for definition in record_policies() {
            for permission in std::iter::once(definition.permission).chain(
                definition
                    .additional_access_rules
                    .iter()
                    .map(|rule| rule.permission),
            ) {
                assert!(!permission.contains('*'));
                assert!(!permission.contains("admin"));
            }
            assert_ne!(
                definition.policy_id(),
                PolicyId::try_from_uuid(id()).unwrap()
            );
        }
    }

    #[derive(Debug, Clone, Default)]
    struct FakeDecisionSource {
        claims: Arc<Mutex<Vec<RecordPolicyDecisionClaim>>>,
        completed: Arc<Mutex<Vec<(Uuid, u64)>>>,
        released: Arc<Mutex<Vec<(Uuid, u64, RecordPolicyReleaseKind)>>>,
    }

    #[async_trait]
    impl RecordPolicyDecisionSource for FakeDecisionSource {
        async fn claim(
            &self,
            _limit: usize,
            _now: DateTime<Utc>,
        ) -> Result<Vec<RecordPolicyDecisionClaim>, RecordPolicyRuntimeError> {
            Ok(std::mem::take(&mut *self.claims.lock().unwrap()))
        }

        async fn complete(
            &self,
            claim_id: Uuid,
            claim_version: u64,
        ) -> Result<(), RecordPolicyRuntimeError> {
            self.completed
                .lock()
                .unwrap()
                .push((claim_id, claim_version));
            Ok(())
        }

        async fn release(
            &self,
            claim_id: Uuid,
            claim_version: u64,
            kind: RecordPolicyReleaseKind,
        ) -> Result<(), RecordPolicyRuntimeError> {
            self.released
                .lock()
                .unwrap()
                .push((claim_id, claim_version, kind));
            Ok(())
        }
    }

    type PublishedBatch = (Uuid, u64, Vec<ItemDirectoryFact>);

    #[derive(Debug, Clone, Default)]
    struct FakeFactSink {
        fail: bool,
        published: Arc<Mutex<Vec<PublishedBatch>>>,
    }

    #[async_trait]
    impl RecordPolicyFactSink for FakeFactSink {
        async fn publish(
            &self,
            claim_id: Uuid,
            claim_version: u64,
            facts: &[ItemDirectoryFact],
        ) -> Result<(), RecordPolicyRuntimeError> {
            if self.fail {
                return Err(RecordPolicyRuntimeError::SinkUnavailable);
            }
            self.published
                .lock()
                .unwrap()
                .push((claim_id, claim_version, facts.to_vec()));
            Ok(())
        }
    }

    fn claim_at(now: DateTime<Utc>) -> RecordPolicyDecisionClaim {
        RecordPolicyDecisionClaim {
            claim_id: id(),
            claim_version: 1,
            trigger: RecordPolicyTrigger::DescriptorChanged,
            lease_until: now + Duration::minutes(1),
            evaluations: vec![evaluation(record_policy("finance.invoice").unwrap())],
        }
    }

    #[tokio::test]
    async fn durable_handler_publishes_before_checkpointing_exact_claim_identity() {
        let now = Utc::now();
        let claim = claim_at(now);
        let source = FakeDecisionSource {
            claims: Arc::new(Mutex::new(vec![claim.clone()])),
            ..FakeDecisionSource::default()
        };
        let sink = FakeFactSink::default();
        let handler = RecordPolicyDecisionHandler::new(source.clone(), sink.clone(), 10).unwrap();

        let report = handler.run_once(now).await.unwrap();

        assert_eq!(report.claimed, 1);
        assert_eq!(report.published, 1);
        assert_eq!(report.completed, 1);
        assert_eq!(report.released, 0);
        assert_eq!(
            *source.completed.lock().unwrap(),
            vec![(claim.claim_id, claim.claim_version)]
        );
        let published = sink.published.lock().unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, claim.claim_id);
        assert_eq!(published[0].1, claim.claim_version);
    }

    #[tokio::test]
    async fn durable_handler_releases_sink_failure_for_retry_without_checkpointing() {
        let now = Utc::now();
        let claim = claim_at(now);
        let source = FakeDecisionSource {
            claims: Arc::new(Mutex::new(vec![claim.clone()])),
            ..FakeDecisionSource::default()
        };
        let sink = FakeFactSink {
            fail: true,
            ..FakeFactSink::default()
        };
        let handler = RecordPolicyDecisionHandler::new(source.clone(), sink, 10).unwrap();

        let report = handler.run_once(now).await.unwrap();

        assert_eq!(report.completed, 0);
        assert_eq!(report.released, 1);
        assert!(source.completed.lock().unwrap().is_empty());
        assert_eq!(
            *source.released.lock().unwrap(),
            vec![(
                claim.claim_id,
                claim.claim_version,
                RecordPolicyReleaseKind::Retryable
            )]
        );
    }

    #[tokio::test]
    async fn expired_claim_is_dead_lettered_without_publishing() {
        let now = Utc::now();
        let mut claim = claim_at(now);
        claim.lease_until = now;
        let source = FakeDecisionSource {
            claims: Arc::new(Mutex::new(vec![claim.clone()])),
            ..FakeDecisionSource::default()
        };
        let sink = FakeFactSink::default();
        let handler = RecordPolicyDecisionHandler::new(source.clone(), sink.clone(), 10).unwrap();

        let report = handler.run_once(now).await.unwrap();

        assert_eq!(report.released, 1);
        assert!(sink.published.lock().unwrap().is_empty());
        assert_eq!(
            *source.released.lock().unwrap(),
            vec![(
                claim.claim_id,
                claim.claim_version,
                RecordPolicyReleaseKind::Permanent
            )]
        );
    }
}
