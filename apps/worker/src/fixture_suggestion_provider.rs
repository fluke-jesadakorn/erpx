//! Deterministic local provider for the four human-controlled suggestion flows.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_agent_orchestration::{
    BudgetRequest, PortError, ProviderInvocationRequest, ProviderInvocationResponse,
};
use erpx_finance::{AccountingPlanSuggestionPayload, PostingReviewSuggestionPayload};
use erpx_sales::{QuoteDraftPayload, RevenueArrangementSuggestionPayload};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    AgentProviderApproval, AgentProviderLimits, BudgetedAgentProviderPort,
    HoldingBoundAgentProviderPort,
    agent_provider::{provider_response, validate_invocation},
};

const QUOTE_FIXTURE: &str = include_str!("../../../fixtures/ai/sales.quote.compose.v1.json");
const ARRANGEMENT_FIXTURE: &str =
    include_str!("../../../fixtures/ai/sales.revenue-arrangement.v1.json");
const PLAN_FIXTURE: &str = include_str!("../../../fixtures/ai/finance.accounting-plan.v1.json");
const REVIEW_FIXTURE: &str = include_str!("../../../fixtures/ai/finance.posting-review.v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureEnvironment {
    Local,
    Development,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FixtureSuggestionProviderError {
    #[error("Fixture AI is disabled outside local/development")]
    EnvironmentDenied,
    #[error("Fixture AI does not implement this capability")]
    CapabilityUnavailable,
    #[error("Fixture AI payload failed typed domain validation")]
    InvalidFixture,
}

/// A provider with no network client, ERP tools, or mutation capability.
#[derive(Debug, Clone)]
pub struct FixtureAgentProvider {
    approval: AgentProviderApproval,
    limits: AgentProviderLimits,
}

impl FixtureAgentProvider {
    pub fn try_new(
        environment: FixtureEnvironment,
        approval: AgentProviderApproval,
        limits: AgentProviderLimits,
    ) -> Result<Self, FixtureSuggestionProviderError> {
        if environment == FixtureEnvironment::Production {
            return Err(FixtureSuggestionProviderError::EnvironmentDenied);
        }
        Ok(Self { approval, limits })
    }

    pub fn display_label(&self) -> &'static str {
        "Fixture AI"
    }
}

impl HoldingBoundAgentProviderPort for FixtureAgentProvider {
    fn holding_id(&self) -> Uuid {
        self.approval.holding_id()
    }
}

#[async_trait]
impl BudgetedAgentProviderPort for FixtureAgentProvider {
    async fn invoke_bounded(
        &self,
        request: ProviderInvocationRequest,
        budget: &BudgetRequest,
    ) -> Result<ProviderInvocationResponse, PortError> {
        validate_invocation(&self.approval, &self.limits, &request, budget)?;
        let output =
            typed_fixture(&request.pin.manifest.use_case).map_err(|error| match error {
                FixtureSuggestionProviderError::CapabilityUnavailable => PortError::ProviderDenied,
                FixtureSuggestionProviderError::EnvironmentDenied
                | FixtureSuggestionProviderError::InvalidFixture => PortError::ProviderPermanent,
            })?;
        Ok(provider_response(output, 1, 0))
    }
}

fn typed_fixture(capability: &str) -> Result<String, FixtureSuggestionProviderError> {
    match capability {
        "sales.quote.compose.v1" => {
            let payload: QuoteDraftPayload = parse(QUOTE_FIXTURE)?;
            let profiles = payload
                .lines
                .iter()
                .map(|line| line.tax_profile.clone())
                .collect::<Vec<_>>();
            let validation_time = DateTime::<Utc>::from_timestamp(1_800_000_000, 0)
                .ok_or(FixtureSuggestionProviderError::InvalidFixture)?;
            payload
                .validate_against(&profiles, validation_time)
                .map_err(|_| FixtureSuggestionProviderError::InvalidFixture)?;
            serialize(&payload)
        }
        "sales.revenue-arrangement.v1" => {
            let payload: RevenueArrangementSuggestionPayload = parse(ARRANGEMENT_FIXTURE)?;
            payload
                .validate()
                .map_err(|_| FixtureSuggestionProviderError::InvalidFixture)?;
            serialize(&payload)
        }
        "finance.accounting-plan.v1" => {
            let payload: AccountingPlanSuggestionPayload = parse(PLAN_FIXTURE)?;
            payload
                .validate()
                .map_err(|_| FixtureSuggestionProviderError::InvalidFixture)?;
            serialize(&payload)
        }
        "finance.posting-review.v1" => {
            let payload: PostingReviewSuggestionPayload = parse(REVIEW_FIXTURE)?;
            payload
                .validate()
                .map_err(|_| FixtureSuggestionProviderError::InvalidFixture)?;
            serialize(&payload)
        }
        _ => Err(FixtureSuggestionProviderError::CapabilityUnavailable),
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, FixtureSuggestionProviderError> {
    serde_json::from_str(value).map_err(|_| FixtureSuggestionProviderError::InvalidFixture)
}

fn serialize<T: serde::Serialize>(value: &T) -> Result<String, FixtureSuggestionProviderError> {
    serde_json::to_string(value).map_err(|_| FixtureSuggestionProviderError::InvalidFixture)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use erpx_agent_orchestration::{
        AgentDefinitionId, AgentManifest, AgentPin, AgentVersionId, AssignmentId, PayloadManifest,
        ProviderPolicyDecision,
    };
    use erpx_kernel::{AuthorityScope, DataClassification};
    use sha2::{Digest, Sha256};

    fn provider(
        environment: FixtureEnvironment,
    ) -> Result<FixtureAgentProvider, FixtureSuggestionProviderError> {
        FixtureAgentProvider::try_new(
            environment,
            AgentProviderApproval::try_new(
                Uuid::from_u128(1),
                "fixture-ai".into(),
                BTreeSet::from(["fixture-v1".into()]),
                BTreeSet::from([
                    "sales.quote.compose.v1".into(),
                    "sales.revenue-arrangement.v1".into(),
                    "finance.accounting-plan.v1".into(),
                    "finance.posting-review.v1".into(),
                ]),
            )
            .expect("approval"),
            AgentProviderLimits::default(),
        )
    }

    fn request(capability: &str) -> ProviderInvocationRequest {
        let prompt = "fixture request";
        let payload_digest = format!("{:x}", Sha256::digest(prompt.as_bytes()));
        let policy_digest = "a".repeat(64);
        let scope = AuthorityScope::Holding {
            holding_id: Uuid::from_u128(2),
        };
        ProviderInvocationRequest {
            assignment_id: AssignmentId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
            assignment_version: 1,
            attempt: 1,
            authority_scope: scope.clone(),
            lease_token: Uuid::from_u128(4),
            pin: AgentPin {
                definition_id: AgentDefinitionId::try_from_uuid(Uuid::from_u128(5)).unwrap(),
                agent_principal_ref: "agent:fixture".into(),
                manifest: AgentManifest {
                    version_id: AgentVersionId::try_from_uuid(Uuid::from_u128(6)).unwrap(),
                    version_number: 1,
                    purpose: "Prepare a suggestion for human review".into(),
                    provider_ref: "fixture-ai".into(),
                    model_ref: "fixture-v1".into(),
                    use_case: capability.into(),
                    prompt_digest: "b".repeat(64),
                    policy_digest: policy_digest.clone(),
                    capabilities: BTreeSet::from(["suggest".into()]),
                    tool_refs: BTreeSet::new(),
                    knowledge_scopes: BTreeSet::new(),
                    organization_scopes: vec![scope],
                    maximum_classification: DataClassification::Internal,
                    security_review_ref: "fixture-security-review".into(),
                    privacy_review_ref: "fixture-privacy-review".into(),
                    evaluation_refs: BTreeSet::from(["fixture-evaluation".into()]),
                    max_attempts: 1,
                    timeout_seconds: 5,
                },
            },
            prompt_material: prompt.into(),
            payload_manifest: PayloadManifest {
                payload_digest: payload_digest.clone(),
                classification: DataClassification::Internal,
                field_refs: BTreeSet::new(),
                document_refs: BTreeSet::new(),
                authorization_checked_at: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
                contains_secret: false,
                residency: "TH".into(),
            },
            policy_decision: ProviderPolicyDecision {
                provider_ref: "fixture-ai".into(),
                model_ref: "fixture-v1".into(),
                policy_version: 1,
                payload_digest,
                policy_digest,
                authorized_at: DateTime::from_timestamp(1_800_000_001, 0).unwrap(),
            },
        }
    }

    #[test]
    fn production_fails_closed() {
        assert_eq!(
            provider(FixtureEnvironment::Production).unwrap_err(),
            FixtureSuggestionProviderError::EnvironmentDenied
        );
    }

    #[tokio::test]
    async fn every_fixture_is_typed_deterministic_and_never_proposes_actions() {
        let provider = provider(FixtureEnvironment::Development).unwrap();
        let budget = BudgetRequest {
            estimated_units: 1,
            estimated_cost_micros: 0,
            queue_priority: 1,
            stop_after_units: 8,
        };
        for capability in [
            "sales.quote.compose.v1",
            "sales.revenue-arrangement.v1",
            "finance.accounting-plan.v1",
            "finance.posting-review.v1",
        ] {
            let first = provider
                .invoke_bounded(request(capability), &budget)
                .await
                .unwrap_or_else(|error| panic!("fixture {capability}: {error:?}"));
            let second = provider
                .invoke_bounded(request(capability), &budget)
                .await
                .unwrap_or_else(|error| panic!("fixture replay {capability}: {error:?}"));
            assert_eq!(first, second);
            assert!(first.proposed_actions.is_empty());
            assert_eq!(first.provider_cost_micros, 0);
        }
    }
}
