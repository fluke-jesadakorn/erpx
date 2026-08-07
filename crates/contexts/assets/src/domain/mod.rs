use chrono::{DateTime, Utc};
use erpx_integration_contracts::{DisplayName, ReferenceCode, assets};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_ASSET_NAME_BYTES: usize = 255;
pub const MAX_ASSET_REASON_BYTES: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(Uuid);

impl AssetId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, AssetDomainError> {
        require_id(value, "asset_id")?;
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for AssetId {
    type Error = AssetDomainError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

impl From<AssetId> for Uuid {
    fn from(value: AssetId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetScope {
    tenant_scope: TenantScope,
}

impl AssetScope {
    pub fn try_new(tenant_scope: TenantScope) -> Result<Self, AssetDomainError> {
        if !matches!(
            tenant_scope,
            TenantScope::Company { .. } | TenantScope::Branch { .. }
        ) || tenant_scope.validate().is_err()
        {
            return Err(AssetDomainError::InvalidScope);
        }
        Ok(Self { tenant_scope })
    }

    pub const fn tenant_scope(&self) -> &TenantScope {
        &self.tenant_scope
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Draft,
    InService,
    UnderMaintenance,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetLifecycle {
    pub placed_in_service_at: Option<DateTime<Utc>>,
    pub retired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEvent {
    Created {
        asset_id: AssetId,
        scope: TenantScope,
        asset_code: String,
        name: String,
        location_id: Uuid,
        at: DateTime<Utc>,
    },
    ConditionChanged {
        asset_id: AssetId,
        asset_code: String,
        name: String,
        location_id: Uuid,
        condition: assets::AssetCondition,
        at: DateTime<Utc>,
    },
    MaintenanceRequested {
        asset_id: AssetId,
        request_code: String,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AssetDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("assets require an exact company or branch scope")]
    InvalidScope,
    #[error("asset lifecycle transition is not allowed: {from:?} -> {to:?}")]
    InvalidTransition { from: AssetStatus, to: AssetStatus },
    #[error("asset version overflowed")]
    VersionOverflow,
    #[error("asset timestamps are out of order")]
    InvalidWindow,
    #[error("asset reference or integration-safe display value is invalid")]
    InvalidReference,
}

/// Authoritative Assets aggregate. Maintenance sees only the typed published
/// facts mapped in `integration`; it never receives this aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    id: AssetId,
    version: u64,
    code: String,
    name: String,
    scope: AssetScope,
    location_id: Uuid,
    status: AssetStatus,
    condition: assets::AssetCondition,
    lifecycle: AssetLifecycle,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Asset {
    pub fn create(
        id: AssetId,
        code: impl Into<String>,
        name: impl Into<String>,
        scope: AssetScope,
        location_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, AssetDomainError> {
        let code = code.into();
        let name = name.into();
        require_text(&code, "asset_code", 128)?;
        require_text(&name, "asset_name", MAX_ASSET_NAME_BYTES)?;
        require_id(location_id, "location_id")?;
        ReferenceCode::try_new(code.clone()).map_err(|_| AssetDomainError::InvalidReference)?;
        DisplayName::try_new(name.clone()).map_err(|_| AssetDomainError::InvalidReference)?;
        let mut value = Self {
            id,
            version: 1,
            code: code.clone(),
            name: name.clone(),
            scope,
            location_id,
            status: AssetStatus::Draft,
            condition: assets::AssetCondition::OutOfService,
            lifecycle: AssetLifecycle {
                placed_in_service_at: None,
                retired_at: None,
            },
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.uncommitted_events.push(DomainEvent::Created {
            asset_id: id,
            scope: value.scope.tenant_scope().clone(),
            asset_code: code,
            name,
            location_id,
            at,
        });
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), AssetDomainError> {
        require_id(self.id.as_uuid(), "asset_id")?;
        require_id(self.location_id, "location_id")?;
        require_text(&self.code, "asset_code", 128)?;
        require_text(&self.name, "asset_name", MAX_ASSET_NAME_BYTES)?;
        AssetScope::try_new(self.scope.tenant_scope().clone())?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(AssetDomainError::InvalidWindow);
        }
        match self.status {
            AssetStatus::Draft
                if self.condition != assets::AssetCondition::OutOfService
                    || self.lifecycle.placed_in_service_at.is_some()
                    || self.lifecycle.retired_at.is_some() =>
            {
                Err(AssetDomainError::InvalidWindow)
            }
            AssetStatus::InService
                if !matches!(
                    self.condition,
                    assets::AssetCondition::Operational | assets::AssetCondition::Degraded
                ) || self.lifecycle.placed_in_service_at.is_none()
                    || self.lifecycle.retired_at.is_some() =>
            {
                Err(AssetDomainError::InvalidWindow)
            }
            AssetStatus::UnderMaintenance
                if self.condition != assets::AssetCondition::OutOfService
                    || self.lifecycle.placed_in_service_at.is_none()
                    || self.lifecycle.retired_at.is_some() =>
            {
                Err(AssetDomainError::InvalidWindow)
            }
            AssetStatus::Retired
                if self.condition != assets::AssetCondition::Retired
                    || self.lifecycle.retired_at.is_none() =>
            {
                Err(AssetDomainError::InvalidWindow)
            }
            _ => Ok(()),
        }
    }

    pub fn place_in_service(&mut self, at: DateTime<Utc>) -> Result<(), AssetDomainError> {
        self.transition(
            AssetStatus::InService,
            assets::AssetCondition::Operational,
            at,
        )?;
        self.lifecycle.placed_in_service_at = Some(at);
        self.validate()
    }

    pub fn mark_degraded(&mut self, at: DateTime<Utc>) -> Result<(), AssetDomainError> {
        if self.status != AssetStatus::InService {
            return Err(AssetDomainError::InvalidTransition {
                from: self.status,
                to: AssetStatus::InService,
            });
        }
        self.change_condition(assets::AssetCondition::Degraded, at)
    }

    pub fn begin_maintenance(
        &mut self,
        request_code: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), AssetDomainError> {
        let request_code = request_code.into();
        ReferenceCode::try_new(request_code.clone())
            .map_err(|_| AssetDomainError::InvalidReference)?;
        self.transition(
            AssetStatus::UnderMaintenance,
            assets::AssetCondition::OutOfService,
            at,
        )?;
        self.uncommitted_events
            .push(DomainEvent::MaintenanceRequested {
                asset_id: self.id,
                request_code,
                at,
            });
        self.validate()
    }

    pub fn return_to_service(&mut self, at: DateTime<Utc>) -> Result<(), AssetDomainError> {
        if self.status != AssetStatus::UnderMaintenance {
            return Err(AssetDomainError::InvalidTransition {
                from: self.status,
                to: AssetStatus::InService,
            });
        }
        self.transition(
            AssetStatus::InService,
            assets::AssetCondition::Operational,
            at,
        )?;
        self.validate()
    }

    pub fn retire(&mut self, at: DateTime<Utc>) -> Result<(), AssetDomainError> {
        self.transition(AssetStatus::Retired, assets::AssetCondition::Retired, at)?;
        self.lifecycle.retired_at = Some(at);
        self.validate()
    }

    fn transition(
        &mut self,
        next: AssetStatus,
        condition: assets::AssetCondition,
        at: DateTime<Utc>,
    ) -> Result<(), AssetDomainError> {
        let allowed = matches!(
            (self.status, next),
            (AssetStatus::Draft, AssetStatus::InService)
                | (AssetStatus::InService, AssetStatus::UnderMaintenance)
                | (AssetStatus::UnderMaintenance, AssetStatus::InService)
                | (
                    AssetStatus::Draft | AssetStatus::InService,
                    AssetStatus::Retired
                )
        );
        if !allowed || at < self.updated_at {
            return Err(if !allowed {
                AssetDomainError::InvalidTransition {
                    from: self.status,
                    to: next,
                }
            } else {
                AssetDomainError::InvalidWindow
            });
        }
        self.status = next;
        self.change_condition(condition, at)
    }

    fn change_condition(
        &mut self,
        condition: assets::AssetCondition,
        at: DateTime<Utc>,
    ) -> Result<(), AssetDomainError> {
        if at < self.updated_at {
            return Err(AssetDomainError::InvalidWindow);
        }
        self.condition = condition;
        self.updated_at = at;
        self.bump_version()?;
        self.uncommitted_events.push(DomainEvent::ConditionChanged {
            asset_id: self.id,
            asset_code: self.code.clone(),
            name: self.name.clone(),
            location_id: self.location_id,
            condition,
            at,
        });
        Ok(())
    }

    fn bump_version(&mut self) -> Result<(), AssetDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AssetDomainError::VersionOverflow)?;
        Ok(())
    }

    pub const fn id(&self) -> AssetId {
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
    pub const fn scope(&self) -> &AssetScope {
        &self.scope
    }
    pub const fn location_id(&self) -> Uuid {
        self.location_id
    }
    pub const fn status(&self) -> AssetStatus {
        self.status
    }
    pub const fn condition(&self) -> assets::AssetCondition {
        self.condition
    }
    pub const fn lifecycle(&self) -> &AssetLifecycle {
        &self.lifecycle
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), AssetDomainError> {
    if value.is_nil() {
        Err(AssetDomainError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn require_text(value: &str, field: &'static str, maximum: usize) -> Result<(), AssetDomainError> {
    if value.trim().is_empty() {
        return Err(AssetDomainError::Required { field });
    }
    if value.len() > maximum {
        return Err(AssetDomainError::TooLong { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(AssetDomainError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
