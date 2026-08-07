use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DomainEvent, PlatformAdministrationDomainError, bounded};

const MAX_KEY_BYTES: usize = 120;
const MAX_CAPABILITY_BYTES: usize = 120;
const MAX_CAPABILITIES: usize = 32;

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
    Tenant(TenantScope),
}

impl ConnectorScope {
    pub fn tenant_scope(&self) -> TenantScope {
        match self {
            Self::Platform => TenantScope::Platform,
            Self::Tenant(scope) => scope.clone(),
        }
    }

    fn validate(&self) -> Result<(), PlatformAdministrationDomainError> {
        match self {
            Self::Platform => Ok(()),
            Self::Tenant(TenantScope::Platform) => {
                Err(PlatformAdministrationDomainError::InvalidAdminScope)
            }
            Self::Tenant(scope) => scope
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
    N8n,
    Ollama,
    Other(String),
}

impl ConnectorKind {
    fn validate(&mut self) -> Result<(), PlatformAdministrationDomainError> {
        if let Self::Other(value) = self {
            *value = bounded(value, "connector_kind", MAX_KEY_BYTES)?;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorBinding {
    id: ConnectorBindingId,
    version: u64,
    scope: ConnectorScope,
    kind: ConnectorKind,
    key: String,
    configuration_digest: String,
    capabilities: BTreeSet<String>,
    state: ConnectorState,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    events: Vec<DomainEvent>,
}

impl ConnectorBinding {
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
        if configuration_digest.len() != 64
            || !configuration_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(PlatformAdministrationDomainError::InvalidConfigurationDigest);
        }
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
        self.state = target;
        self.record(at)
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
