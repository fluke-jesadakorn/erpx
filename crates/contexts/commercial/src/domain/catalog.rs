use chrono::{DateTime, Utc};
use erpx_integration_contracts::CurrencyCode;
use erpx_kernel::{AuthorityScope, CommandId, HumanDecisionProof};
use serde::{Deserialize, Serialize};

use super::{CatalogPlanId, CommercialDomainError, DomainEvent, next_version, validate_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogPlanStatus {
    Draft,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogPlan {
    id: CatalogPlanId,
    version: u64,
    scope: AuthorityScope,
    key: String,
    display_name: String,
    amount_minor: i64,
    currency: CurrencyCode,
    status: CatalogPlanStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl CatalogPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: CatalogPlanId,
        version: u64,
        key: String,
        display_name: String,
        amount_minor: i64,
        currency: CurrencyCode,
        status: CatalogPlanStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, CommercialDomainError> {
        let value = Self {
            id,
            version,
            scope: AuthorityScope::Platform,
            key,
            display_name,
            amount_minor,
            currency,
            status,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        if value.updated_at < value.created_at
            || !matches!(
                (value.version, value.status),
                (1, CatalogPlanStatus::Draft) | (2, CatalogPlanStatus::Published)
            )
        {
            return Err(CommercialDomainError::InvalidTimestamp);
        }
        validate_text(&value.key, "plan_key")?;
        validate_text(&value.display_name, "display_name")?;
        if value.amount_minor <= 0 {
            return Err(CommercialDomainError::NonPositiveAmount);
        }
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: CatalogPlanId,
        scope: AuthorityScope,
        key: String,
        display_name: String,
        amount_minor: i64,
        currency: CurrencyCode,
        at: DateTime<Utc>,
    ) -> Result<Self, CommercialDomainError> {
        if scope != AuthorityScope::Platform {
            return Err(CommercialDomainError::InvalidCatalogScope);
        }
        validate_text(&key, "plan_key")?;
        validate_text(&display_name, "display_name")?;
        if amount_minor <= 0 {
            return Err(CommercialDomainError::NonPositiveAmount);
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            key,
            display_name,
            amount_minor,
            currency,
            status: CatalogPlanStatus::Draft,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::CatalogPlanCreated],
        })
    }

    pub fn publish(
        &mut self,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommercialDomainError> {
        if decision.authority_scope() != &self.scope
            || decision.decision_ref()
                != catalog_plan_publish_decision_ref(command_id, self.id, expected_version)
            || decision.decided_at() != at
        {
            return Err(CommercialDomainError::InvalidCatalogTransition);
        }
        if self.status != CatalogPlanStatus::Draft {
            return Err(CommercialDomainError::InvalidCatalogTransition);
        }
        if at < self.updated_at {
            return Err(CommercialDomainError::InvalidTimestamp);
        }
        self.version = next_version(self.version)?;
        self.status = CatalogPlanStatus::Published;
        self.updated_at = at;
        self.uncommitted_events
            .push(DomainEvent::CatalogPlanPublished);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn publish_for_test(
        &mut self,
        at: DateTime<Utc>,
    ) -> Result<(), CommercialDomainError> {
        use erpx_kernel::{ActorAttribution, ActorKind, SessionId};
        use uuid::Uuid;

        let command_id = CommandId::try_from_uuid(Uuid::new_v4()).expect("test command id");
        let decision = HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "commercial-test-user".into(),
                kind: ActorKind::PlatformUser,
                session_id: Some(
                    SessionId::try_from_uuid(Uuid::new_v4()).expect("test session id"),
                ),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            self.scope.clone(),
            catalog_plan_publish_decision_ref(command_id, self.id, self.version),
            at,
        )
        .expect("test human decision");
        self.publish(&decision, command_id, self.version, at)
    }

    pub const fn id(&self) -> CatalogPlanId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
    pub const fn currency(&self) -> CurrencyCode {
        self.currency
    }
    pub const fn status(&self) -> CatalogPlanStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

pub fn catalog_plan_publish_decision_ref(
    command_id: CommandId,
    plan_id: CatalogPlanId,
    expected_version: u64,
) -> String {
    format!(
        "commercial:publish_catalog_plan:{command_id}:{}:{expected_version}",
        plan_id.as_uuid()
    )
}
