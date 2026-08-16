use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DomainEvent, PlatformAdministrationDomainError, PublishedTenantState, bounded};

const MAX_CODE_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 256;
const MAX_HOLDING_REFERENCE_BYTES: usize = 255;

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

identifier!(PlatformTenantId, "tenant_id");
identifier!(EnvironmentId, "environment_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantLifecycleState {
    Provisioning,
    Active,
    Suspended,
    Recovery,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantLifecycleAction {
    Activate,
    Suspend,
    EnterRecovery,
    Retire,
}

impl TenantLifecycleAction {
    pub const fn target(self) -> TenantLifecycleState {
        match self {
            Self::Activate => TenantLifecycleState::Active,
            Self::Suspend => TenantLifecycleState::Suspended,
            Self::EnterRecovery => TenantLifecycleState::Recovery,
            Self::Retire => TenantLifecycleState::Retired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformTenant {
    id: PlatformTenantId,
    version: u64,
    code: String,
    name: String,
    environment_id: EnvironmentId,
    customer_id: Uuid,
    root_holding_id: Uuid,
    holding_ref: String,
    status: TenantLifecycleState,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    events: Vec<DomainEvent>,
}

impl PlatformTenant {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: PlatformTenantId,
        version: u64,
        code: String,
        name: String,
        environment_id: EnvironmentId,
        customer_id: Uuid,
        root_holding_id: Uuid,
        holding_ref: String,
        status: TenantLifecycleState,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        if version == 0
            || customer_id.is_nil()
            || root_holding_id.is_nil()
            || updated_at < created_at
        {
            return Err(PlatformAdministrationDomainError::InvalidTenantTransition);
        }
        Ok(Self {
            id,
            version,
            code: bounded(&code, "code", MAX_CODE_BYTES)?,
            name: bounded(&name, "name", MAX_NAME_BYTES)?,
            environment_id,
            customer_id,
            root_holding_id,
            holding_ref: bounded(&holding_ref, "holding_ref", MAX_HOLDING_REFERENCE_BYTES)?,
            status,
            created_at,
            updated_at,
            events: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register(
        id: PlatformTenantId,
        code: String,
        name: String,
        environment_id: EnvironmentId,
        customer_id: Uuid,
        root_holding_id: Uuid,
        holding_ref: String,
        at: DateTime<Utc>,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        if customer_id.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier {
                field: "customer_id",
            });
        }
        if root_holding_id.is_nil() {
            return Err(PlatformAdministrationDomainError::NilIdentifier {
                field: "root_holding_id",
            });
        }
        let mut tenant = Self {
            id,
            version: 0,
            code: bounded(&code, "code", MAX_CODE_BYTES)?,
            name: bounded(&name, "name", MAX_NAME_BYTES)?,
            environment_id,
            customer_id,
            root_holding_id,
            holding_ref: bounded(&holding_ref, "holding_ref", MAX_HOLDING_REFERENCE_BYTES)?,
            status: TenantLifecycleState::Provisioning,
            created_at: at,
            updated_at: at,
            events: Vec::new(),
        };
        tenant.record_changed(at)?;
        Ok(tenant)
    }

    pub fn transition(
        &mut self,
        action: TenantLifecycleAction,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        let target = action.target();
        let allowed = matches!(
            (self.status, target),
            (
                TenantLifecycleState::Provisioning,
                TenantLifecycleState::Active
            ) | (
                TenantLifecycleState::Active,
                TenantLifecycleState::Suspended
            ) | (TenantLifecycleState::Active, TenantLifecycleState::Recovery)
                | (
                    TenantLifecycleState::Suspended,
                    TenantLifecycleState::Active
                )
                | (
                    TenantLifecycleState::Suspended,
                    TenantLifecycleState::Recovery
                )
                | (TenantLifecycleState::Recovery, TenantLifecycleState::Active)
                | (
                    TenantLifecycleState::Suspended,
                    TenantLifecycleState::Retired
                )
                | (
                    TenantLifecycleState::Recovery,
                    TenantLifecycleState::Retired
                )
        );
        if !allowed {
            return Err(PlatformAdministrationDomainError::InvalidTenantTransition);
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
        self.events.push(DomainEvent::TenantChanged {
            state: PublishedTenantState {
                tenant_id: self.id,
                code: self.code.clone(),
                name: self.name.clone(),
                environment_id: self.environment_id,
                status: self.status,
            },
            at,
        });
        Ok(())
    }

    pub const fn id(&self) -> PlatformTenantId {
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
    pub const fn root_holding_id(&self) -> Uuid {
        self.root_holding_id
    }
    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }
    pub const fn status(&self) -> TenantLifecycleState {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn scope(&self) -> TenantScope {
        TenantScope::Holding {
            holding_id: self.root_holding_id,
        }
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.events)
    }
}
