use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, HoldingId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DomainEvent, PlatformAdministrationDomainError, PublishedHoldingState, bounded};

const MAX_CODE_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 256;
const MAX_DATA_PROFILE_REFERENCE_BYTES: usize = 255;

macro_rules! identifier {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, PlatformAdministrationDomainError> {
                if value.is_nil() {
                    return Err(PlatformAdministrationDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = PlatformAdministrationDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }
    };
}

pub type PlatformHoldingId = HoldingId;
identifier!(EnvironmentId, "environment_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingLifecycleState {
    PendingSubscription,
    Provisioning,
    ProvisioningFailed,
    Active,
    Suspended,
    Recovery,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingLifecycleAction {
    BeginProvisioning,
    MarkProvisioningFailed,
    Activate,
    Suspend,
    EnterRecovery,
    Retire,
}

impl HoldingLifecycleAction {
    pub const fn target(self) -> HoldingLifecycleState {
        match self {
            Self::BeginProvisioning => HoldingLifecycleState::Provisioning,
            Self::MarkProvisioningFailed => HoldingLifecycleState::ProvisioningFailed,
            Self::Activate => HoldingLifecycleState::Active,
            Self::Suspend => HoldingLifecycleState::Suspended,
            Self::EnterRecovery => HoldingLifecycleState::Recovery,
            Self::Retire => HoldingLifecycleState::Retired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformHolding {
    id: PlatformHoldingId,
    version: u64,
    code: String,
    name: String,
    environment_id: EnvironmentId,
    customer_id: Uuid,
    canonical_holding_id: Uuid,
    data_profile_key: String,
    status: HoldingLifecycleState,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    events: Vec<DomainEvent>,
}

impl PlatformHolding {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: PlatformHoldingId,
        version: u64,
        code: String,
        name: String,
        environment_id: EnvironmentId,
        customer_id: Uuid,
        canonical_holding_id: Uuid,
        data_profile_key: String,
        status: HoldingLifecycleState,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        if version == 0
            || customer_id.is_nil()
            || canonical_holding_id.is_nil()
            || updated_at < created_at
        {
            return Err(PlatformAdministrationDomainError::InvalidHoldingTransition);
        }
        Ok(Self {
            id,
            version,
            code: bounded(&code, "code", MAX_CODE_BYTES)?,
            name: bounded(&name, "name", MAX_NAME_BYTES)?,
            environment_id,
            customer_id,
            canonical_holding_id,
            data_profile_key: bounded(
                &data_profile_key,
                "data_profile_key",
                MAX_DATA_PROFILE_REFERENCE_BYTES,
            )?,
            status,
            created_at,
            updated_at,
            events: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register(
        id: PlatformHoldingId,
        code: String,
        name: String,
        environment_id: EnvironmentId,
        customer_id: Uuid,
        canonical_holding_id: Uuid,
        data_profile_key: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        if customer_id.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier {
                field: "customer_id",
            });
        }
        if canonical_holding_id.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier {
                field: "canonical_holding_id",
            });
        }
        let mut holding = Self {
            id,
            version: 0,
            code: bounded(&code, "code", MAX_CODE_BYTES)?,
            name: bounded(&name, "name", MAX_NAME_BYTES)?,
            environment_id,
            customer_id,
            canonical_holding_id,
            data_profile_key: bounded(
                &data_profile_key,
                "data_profile_key",
                MAX_DATA_PROFILE_REFERENCE_BYTES,
            )?,
            status: HoldingLifecycleState::PendingSubscription,
            created_at: at,
            updated_at: at,
            events: Vec::new(),
        };
        holding.record_changed(at)?;
        Ok(holding)
    }

    pub fn transition(
        &mut self,
        action: HoldingLifecycleAction,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        let target = action.target();
        let allowed = matches!(
            (self.status, target),
            (
                HoldingLifecycleState::PendingSubscription,
                HoldingLifecycleState::Provisioning
            ) | (
                HoldingLifecycleState::Provisioning,
                HoldingLifecycleState::Active
            ) | (
                HoldingLifecycleState::Provisioning,
                HoldingLifecycleState::ProvisioningFailed
            ) | (
                HoldingLifecycleState::ProvisioningFailed,
                HoldingLifecycleState::Provisioning
            ) | (
                HoldingLifecycleState::Active,
                HoldingLifecycleState::Suspended
            ) | (
                HoldingLifecycleState::Active,
                HoldingLifecycleState::Recovery
            ) | (
                HoldingLifecycleState::Suspended,
                HoldingLifecycleState::Active
            ) | (
                HoldingLifecycleState::Suspended,
                HoldingLifecycleState::Recovery
            ) | (
                HoldingLifecycleState::Recovery,
                HoldingLifecycleState::Active
            ) | (
                HoldingLifecycleState::Suspended,
                HoldingLifecycleState::Retired
            ) | (
                HoldingLifecycleState::Recovery,
                HoldingLifecycleState::Retired
            )
        );
        if !allowed {
            return Err(PlatformAdministrationDomainError::InvalidHoldingTransition);
        }
        self.status = target;
        self.record_changed(at)
    }

    fn record_changed(
        &mut self,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlatformAdministrationDomainError::VersionOverflow)?;
        self.updated_at = at;
        self.events.push(DomainEvent::HoldingChanged {
            state: PublishedHoldingState {
                holding_id: self.id,
                code: self.code.clone(),
                name: self.name.clone(),
                environment_id: self.environment_id,
                status: self.status,
            },
            at,
        });
        Ok(())
    }

    pub const fn id(&self) -> PlatformHoldingId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn environment_id(&self) -> EnvironmentId {
        self.environment_id
    }
    pub const fn customer_id(&self) -> Uuid {
        self.customer_id
    }
    pub const fn canonical_holding_id(&self) -> Uuid {
        self.canonical_holding_id
    }
    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }
    pub const fn status(&self) -> HoldingLifecycleState {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn scope(&self) -> AuthorityScope {
        AuthorityScope::Holding {
            holding_id: self.canonical_holding_id,
        }
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.events)
    }
}
