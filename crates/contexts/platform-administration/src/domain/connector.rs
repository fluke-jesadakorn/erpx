use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DomainEvent, PlatformAdministrationDomainError, bounded};

const MAX_KEY_BYTES: usize = 120;
const MAX_CAPABILITY_BYTES: usize = 120;
const MAX_CAPABILITIES: usize = 32;
const MAX_EVIDENCE_ID_BYTES: usize = 160;
const MAX_FAILURE_CATEGORY_BYTES: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectorBindingId(Uuid);

impl ConnectorBindingId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, PlatformAdministrationDomainError> {
        if value.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier {
                field: "connector_id",
            });
        }
        Ok(Self(value))
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for ConnectorBindingId {
    type Error = PlatformAdministrationDomainError;
    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "scope", rename_all = "snake_case")]
pub enum ConnectorScope {
    Platform,
    Holding(AuthorityScope),
}

impl ConnectorScope {
    pub fn active_scope(&self) -> AuthorityScope {
        match self {
            Self::Platform => AuthorityScope::Platform,
            Self::Holding(scope) => scope.clone(),
        }
    }

    fn validate(&self) -> Result<(), PlatformAdministrationDomainError> {
        match self {
            Self::Platform => Ok(()),
            Self::Holding(AuthorityScope::Platform) => {
                Err(PlatformAdministrationDomainError::InvalidAdminScope)
            }
            Self::Holding(scope) => scope
                .validate()
                .map_err(|_| PlatformAdministrationDomainError::InvalidAdminScope),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    Stripe,
    Minio,
    Ollama,
    Other(String),
}

impl ConnectorKind {
    fn validate(&mut self) -> Result<(), PlatformAdministrationDomainError> {
        if let Self::Other(value) = self {
            *value = bounded(value, "connector_kind", MAX_KEY_BYTES)?;
            if value.eq_ignore_ascii_case("n8n") {
                return Err(PlatformAdministrationDomainError::InvalidConnectorKind);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorState {
    Pending,
    Active,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorTransition {
    Activate,
    Suspend,
    Retire,
}

impl ConnectorTransition {
    const fn target(self) -> ConnectorState {
        match self {
            Self::Activate => ConnectorState::Active,
            Self::Suspend => ConnectorState::Suspended,
            Self::Retire => ConnectorState::Retired,
        }
    }
}

/// Redacted result of an actual adapter/provider preflight. Endpoint details,
/// credentials, and provider response bodies are intentionally excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorReadinessEvidence {
    evidence_id: String,
    configuration_digest: String,
    observed_capabilities: BTreeSet<String>,
    ready: bool,
    failure_category: Option<String>,
    observed_at: DateTime<Utc>,
}

impl ConnectorReadinessEvidence {
    pub fn try_new(
        evidence_id: String,
        configuration_digest: String,
        observed_capabilities: BTreeSet<String>,
        ready: bool,
        failure_category: Option<String>,
        observed_at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        let evidence_id = bounded(&evidence_id, "readiness_evidence_id", MAX_EVIDENCE_ID_BYTES)?;
        validate_digest(&configuration_digest)?;
        if observed_capabilities.len() > MAX_CAPABILITIES {
            return Err(PlatformAdministrationDomainError::InvalidCapabilities);
        }
        let observed_capabilities = observed_capabilities
            .into_iter()
            .map(|value| bounded(&value, "observed_capability", MAX_CAPABILITY_BYTES))
            .collect::<Result<_, _>>()?;
        let failure_category = failure_category
            .map(|value| bounded(&value, "failure_category", MAX_FAILURE_CATEGORY_BYTES))
            .transpose()?;
        if failure_category.as_ref().is_some_and(|category| {
            let category = category.to_ascii_lowercase();
            ["password", "secret", "token", "credential"]
                .iter()
                .any(|marker| category.contains(marker))
        }) {
            return Err(PlatformAdministrationDomainError::InvalidReadinessEvidence);
        }
        if ready != failure_category.is_none() {
            return Err(PlatformAdministrationDomainError::InvalidReadinessEvidence);
        }
        Ok(Self {
            evidence_id,
            configuration_digest: configuration_digest.to_ascii_lowercase(),
            observed_capabilities,
            ready,
            failure_category,
            observed_at,
        })
    }

    pub fn evidence_id(&self) -> &str {
        &self.evidence_id
    }
    pub fn configuration_digest(&self) -> &str {
        &self.configuration_digest
    }
    pub fn observed_capabilities(&self) -> &BTreeSet<String> {
        &self.observed_capabilities
    }
    pub const fn ready(&self) -> bool {
        self.ready
    }
    pub fn failure_category(&self) -> Option<&str> {
        self.failure_category.as_deref()
    }
    pub const fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorBinding {
    id: ConnectorBindingId,
    version: u64,
    scope: ConnectorScope,
    kind: ConnectorKind,
    key: String,
    configuration_digest: String,
    capabilities: BTreeSet<String>,
    readiness: Option<ConnectorReadinessEvidence>,
    state: ConnectorState,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    events: Vec<DomainEvent>,
}

impl ConnectorBinding {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: ConnectorBindingId,
        version: u64,
        scope: ConnectorScope,
        mut kind: ConnectorKind,
        key: String,
        configuration_digest: String,
        capabilities: BTreeSet<String>,
        readiness: Option<ConnectorReadinessEvidence>,
        state: ConnectorState,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        scope.validate()?;
        kind.validate()?;
        validate_digest(&configuration_digest)?;
        if version == 0
            || capabilities.is_empty()
            || capabilities.len() > MAX_CAPABILITIES
            || updated_at < created_at
        {
            return Err(PlatformAdministrationDomainError::InvalidCapabilities);
        }
        let capabilities = capabilities
            .into_iter()
            .map(|value| bounded(&value, "connector_capability", MAX_CAPABILITY_BYTES))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if let Some(evidence) = &readiness
            && (evidence.observed_at > updated_at
                || evidence.configuration_digest != configuration_digest
                || !capabilities.is_subset(&evidence.observed_capabilities))
        {
            return Err(PlatformAdministrationDomainError::InvalidReadinessEvidence);
        }
        Ok(Self {
            id,
            version,
            scope,
            kind,
            key: bounded(&key, "connector_key", MAX_KEY_BYTES)?,
            configuration_digest: configuration_digest.to_ascii_lowercase(),
            capabilities,
            readiness,
            state,
            created_at,
            updated_at,
            events: Vec::new(),
        })
    }

    pub fn register(
        id: ConnectorBindingId,
        scope: ConnectorScope,
        mut kind: ConnectorKind,
        key: String,
        configuration_digest: String,
        capabilities: BTreeSet<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        scope.validate()?;
        kind.validate()?;
        validate_digest(&configuration_digest)?;
        if capabilities.is_empty() || capabilities.len() > MAX_CAPABILITIES {
            return Err(PlatformAdministrationDomainError::InvalidCapabilities);
        }
        let capabilities = capabilities
            .into_iter()
            .map(|value| bounded(&value, "connector_capability", MAX_CAPABILITY_BYTES))
            .collect::<Result<_, _>>()?;
        let mut connector = Self {
            id,
            version: 0,
            scope,
            kind,
            key: bounded(&key, "connector_key", MAX_KEY_BYTES)?,
            configuration_digest: configuration_digest.to_ascii_lowercase(),
            capabilities,
            readiness: None,
            state: ConnectorState::Pending,
            created_at: at,
            updated_at: at,
            events: Vec::new(),
        };
        connector.record(at)?;
        Ok(connector)
    }

    pub fn transition(
        &mut self,
        action: ConnectorTransition,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        let target = action.target();
        let allowed = matches!(
            (self.state, target),
            (ConnectorState::Pending, ConnectorState::Active)
                | (ConnectorState::Pending, ConnectorState::Retired)
                | (ConnectorState::Active, ConnectorState::Suspended)
                | (ConnectorState::Active, ConnectorState::Retired)
                | (ConnectorState::Suspended, ConnectorState::Active)
                | (ConnectorState::Suspended, ConnectorState::Retired)
        );
        if !allowed {
            return Err(PlatformAdministrationDomainError::InvalidConnectorTransition);
        }
        if action == ConnectorTransition::Activate && !self.has_current_ready_evidence() {
            return Err(PlatformAdministrationDomainError::ConnectorNotReady);
        }
        if at < self.updated_at {
            return Err(PlatformAdministrationDomainError::InvalidReadinessEvidence);
        }
        self.state = target;
        self.record(at)
    }

    pub fn observe_readiness(
        &mut self,
        evidence: ConnectorReadinessEvidence,
    ) -> Result<(), PlatformAdministrationDomainError> {
        if self.state == ConnectorState::Retired || evidence.observed_at < self.updated_at {
            return Err(PlatformAdministrationDomainError::InvalidReadinessEvidence);
        }
        if evidence.configuration_digest != self.configuration_digest
            || !self.capabilities.is_subset(&evidence.observed_capabilities)
        {
            return Err(PlatformAdministrationDomainError::ReadinessConfigurationMismatch);
        }
        if self.readiness.as_ref().is_some_and(|current| {
            current.observed_at >= evidence.observed_at
                || current.evidence_id == evidence.evidence_id
        }) {
            return Err(PlatformAdministrationDomainError::StaleReadinessEvidence);
        }
        let at = evidence.observed_at;
        let ready = evidence.ready;
        self.readiness = Some(evidence);
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlatformAdministrationDomainError::VersionOverflow)?;
        self.updated_at = at;
        self.events.push(DomainEvent::ConnectorReadinessObserved {
            connector_id: self.id,
            ready,
            at,
        });
        Ok(())
    }

    fn has_current_ready_evidence(&self) -> bool {
        self.readiness.as_ref().is_some_and(|evidence| {
            evidence.ready
                && evidence.configuration_digest == self.configuration_digest
                && self.capabilities.is_subset(&evidence.observed_capabilities)
        })
    }

    fn record(&mut self, at: DateTime<Utc>) -> Result<(), PlatformAdministrationDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlatformAdministrationDomainError::VersionOverflow)?;
        self.updated_at = at;
        self.events.push(DomainEvent::ConnectorChanged {
            connector_id: self.id,
            state: self.state,
            at,
        });
        Ok(())
    }

    pub const fn id(&self) -> ConnectorBindingId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &ConnectorScope {
        &self.scope
    }
    pub fn kind(&self) -> &ConnectorKind {
        &self.kind
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn configuration_digest(&self) -> &str {
        &self.configuration_digest
    }
    pub fn capabilities(&self) -> &BTreeSet<String> {
        &self.capabilities
    }
    pub const fn readiness(&self) -> Option<&ConnectorReadinessEvidence> {
        self.readiness.as_ref()
    }
    pub const fn state(&self) -> ConnectorState {
        self.state
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.events)
    }
}

fn validate_digest(value: &str) -> Result<(), PlatformAdministrationDomainError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Err(PlatformAdministrationDomainError::InvalidConfigurationDigest)
    } else {
        Ok(())
    }
}
