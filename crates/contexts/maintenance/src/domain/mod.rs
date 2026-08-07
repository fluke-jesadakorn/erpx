use chrono::{DateTime, Utc};
use erpx_integration_contracts::{ReferenceCode, assets};
use erpx_kernel::{EventId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_MAINTENANCE_TITLE_BYTES: usize = 255;
pub const MAX_MAINTENANCE_REASON_BYTES: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkOrderId(Uuid);

impl WorkOrderId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, MaintenanceDomainError> {
        require_id(value, "work_order_id")?;
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for WorkOrderId {
    type Error = MaintenanceDomainError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

impl From<WorkOrderId> for Uuid {
    fn from(value: WorkOrderId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkOrderStatus {
    Planned,
    Scheduled,
    InProgress,
    Blocked,
    Completed,
    Cancelled,
}

impl WorkOrderStatus {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Planned, Self::Scheduled | Self::Cancelled)
                | (Self::Scheduled, Self::InProgress | Self::Cancelled)
                | (Self::InProgress, Self::Blocked | Self::Completed)
                | (Self::Blocked, Self::InProgress | Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceRequest {
    pub source_event_id: EventId,
    pub source_aggregate_version: u64,
    pub asset: assets::AssetRef,
    pub request_code: String,
}

impl MaintenanceRequest {
    pub fn try_new(
        source_event_id: EventId,
        source_aggregate_version: u64,
        asset: assets::AssetRef,
        request_code: impl Into<String>,
    ) -> Result<Self, MaintenanceDomainError> {
        if source_aggregate_version == 0 {
            return Err(MaintenanceDomainError::InvalidSourceVersion);
        }
        let request_code = request_code.into();
        ReferenceCode::try_new(request_code.clone())
            .map_err(|_| MaintenanceDomainError::InvalidReference)?;
        Ok(Self {
            source_event_id,
            source_aggregate_version,
            asset,
            request_code,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Downtime {
    pub downtime_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub reason: String,
}

impl Downtime {
    pub fn validate(&self) -> Result<(), MaintenanceDomainError> {
        require_id(self.downtime_id, "downtime_id")?;
        require_text(
            &self.reason,
            "downtime_reason",
            MAX_MAINTENANCE_REASON_BYTES,
        )?;
        if self.ended_at <= self.started_at {
            return Err(MaintenanceDomainError::InvalidWindow);
        }
        Ok(())
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.started_at < other.ended_at && other.started_at < self.ended_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEvent {
    Opened {
        work_order_id: WorkOrderId,
        asset: assets::AssetRef,
        request_code: String,
        at: DateTime<Utc>,
    },
    StatusChanged {
        work_order_id: WorkOrderId,
        asset: assets::AssetRef,
        status: WorkOrderStatus,
        scheduled_for: Option<DateTime<Utc>>,
        at: DateTime<Utc>,
    },
    DowntimeRecorded {
        work_order_id: WorkOrderId,
        downtime_id: Uuid,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MaintenanceDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("maintenance requires an exact company or branch scope")]
    InvalidScope,
    #[error("maintenance request must be sourced from a nonzero Assets event version")]
    InvalidSourceVersion,
    #[error("asset fact does not match the Assets maintenance request")]
    AssetFactMismatch,
    #[error("a retired asset cannot receive a work order")]
    AssetRetired,
    #[error("maintenance reference is invalid")]
    InvalidReference,
    #[error("work order transition is not allowed: {from:?} -> {to:?}")]
    InvalidTransition {
        from: WorkOrderStatus,
        to: WorkOrderStatus,
    },
    #[error("maintenance timestamp window is invalid")]
    InvalidWindow,
    #[error("downtime overlaps an existing interval")]
    OverlappingDowntime,
    #[error("duplicate downtime identifier")]
    DuplicateDowntime,
    #[error("work order version overflowed")]
    VersionOverflow,
}

/// Maintenance-owned aggregate. Its Assets dependency is a copied typed fact,
/// never an Assets aggregate or repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOrder {
    id: WorkOrderId,
    version: u64,
    code: String,
    scope: TenantScope,
    request: MaintenanceRequest,
    asset_fact: assets::AssetConditionFact,
    title: String,
    status: WorkOrderStatus,
    scheduled_for: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    downtime: Vec<Downtime>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl WorkOrder {
    pub fn open(
        id: WorkOrderId,
        scope: TenantScope,
        request: MaintenanceRequest,
        asset_fact: assets::AssetConditionFact,
        title: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, MaintenanceDomainError> {
        validate_scope(&scope)?;
        if request.asset != asset_fact.asset {
            return Err(MaintenanceDomainError::AssetFactMismatch);
        }
        if asset_fact.condition == assets::AssetCondition::Retired {
            return Err(MaintenanceDomainError::AssetRetired);
        }
        let title = title.into();
        require_text(&title, "maintenance_title", MAX_MAINTENANCE_TITLE_BYTES)?;
        let code = format!("MWO-{}", id.as_uuid().simple());
        let mut value = Self {
            id,
            version: 1,
            code,
            scope,
            request,
            asset_fact,
            title,
            status: WorkOrderStatus::Planned,
            scheduled_for: None,
            started_at: None,
            completed_at: None,
            downtime: Vec::new(),
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.uncommitted_events.push(DomainEvent::Opened {
            work_order_id: id,
            asset: value.asset_fact.asset,
            request_code: value.request.request_code.clone(),
            at,
        });
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MaintenanceDomainError> {
        require_id(self.id.as_uuid(), "work_order_id")?;
        validate_scope(&self.scope)?;
        require_text(&self.code, "work_order_code", 128)?;
        require_text(
            &self.title,
            "maintenance_title",
            MAX_MAINTENANCE_TITLE_BYTES,
        )?;
        if self.request.asset != self.asset_fact.asset || self.version == 0 {
            return Err(MaintenanceDomainError::AssetFactMismatch);
        }
        if self.updated_at < self.created_at {
            return Err(MaintenanceDomainError::InvalidWindow);
        }
        match self.status {
            WorkOrderStatus::Planned
                if self.scheduled_for.is_some()
                    || self.started_at.is_some()
                    || self.completed_at.is_some() =>
            {
                Err(MaintenanceDomainError::InvalidWindow)
            }
            WorkOrderStatus::Scheduled
                if self.scheduled_for.is_none()
                    || self.started_at.is_some()
                    || self.completed_at.is_some() =>
            {
                Err(MaintenanceDomainError::InvalidWindow)
            }
            WorkOrderStatus::InProgress | WorkOrderStatus::Blocked
                if self.scheduled_for.is_none()
                    || self.started_at.is_none()
                    || self.completed_at.is_some() =>
            {
                Err(MaintenanceDomainError::InvalidWindow)
            }
            WorkOrderStatus::Completed
                if self.started_at.is_none() || self.completed_at.is_none() =>
            {
                Err(MaintenanceDomainError::InvalidWindow)
            }
            _ => {
                for (index, current) in self.downtime.iter().enumerate() {
                    current.validate()?;
                    if self.downtime[..index]
                        .iter()
                        .any(|existing| existing.overlaps(current))
                    {
                        return Err(MaintenanceDomainError::OverlappingDowntime);
                    }
                }
                Ok(())
            }
        }
    }

    pub fn schedule(
        &mut self,
        scheduled_for: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<(), MaintenanceDomainError> {
        if scheduled_for < at {
            return Err(MaintenanceDomainError::InvalidWindow);
        }
        let mut candidate = self.clone();
        candidate.scheduled_for = Some(scheduled_for);
        candidate.transition(WorkOrderStatus::Scheduled, at)?;
        *self = candidate;
        Ok(())
    }

    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), MaintenanceDomainError> {
        if self.scheduled_for.is_some_and(|scheduled| at < scheduled) {
            return Err(MaintenanceDomainError::InvalidWindow);
        }
        let mut candidate = self.clone();
        candidate.started_at = Some(at);
        candidate.transition(WorkOrderStatus::InProgress, at)?;
        *self = candidate;
        Ok(())
    }

    pub fn block(&mut self, at: DateTime<Utc>) -> Result<(), MaintenanceDomainError> {
        self.transition(WorkOrderStatus::Blocked, at)
    }

    pub fn resume(&mut self, at: DateTime<Utc>) -> Result<(), MaintenanceDomainError> {
        self.transition(WorkOrderStatus::InProgress, at)
    }

    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), MaintenanceDomainError> {
        if self.started_at.is_none_or(|started| at <= started) {
            return Err(MaintenanceDomainError::InvalidWindow);
        }
        let mut candidate = self.clone();
        candidate.completed_at = Some(at);
        candidate.transition(WorkOrderStatus::Completed, at)?;
        *self = candidate;
        Ok(())
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), MaintenanceDomainError> {
        self.transition(WorkOrderStatus::Cancelled, at)
    }

    pub fn record_downtime(
        &mut self,
        downtime: Downtime,
        at: DateTime<Utc>,
    ) -> Result<(), MaintenanceDomainError> {
        if !matches!(
            self.status,
            WorkOrderStatus::InProgress | WorkOrderStatus::Blocked
        ) {
            return Err(MaintenanceDomainError::InvalidTransition {
                from: self.status,
                to: self.status,
            });
        }
        downtime.validate()?;
        if self
            .downtime
            .iter()
            .any(|existing| existing.downtime_id == downtime.downtime_id)
        {
            return Err(MaintenanceDomainError::DuplicateDowntime);
        }
        if self
            .downtime
            .iter()
            .any(|existing| existing.overlaps(&downtime))
        {
            return Err(MaintenanceDomainError::OverlappingDowntime);
        }
        if at < self.updated_at {
            return Err(MaintenanceDomainError::InvalidWindow);
        }
        let downtime_id = downtime.downtime_id;
        self.downtime.push(downtime);
        self.updated_at = at;
        self.bump_version()?;
        self.uncommitted_events.push(DomainEvent::DowntimeRecorded {
            work_order_id: self.id,
            downtime_id,
            at,
        });
        self.validate()
    }

    fn transition(
        &mut self,
        next: WorkOrderStatus,
        at: DateTime<Utc>,
    ) -> Result<(), MaintenanceDomainError> {
        if !self.status.can_transition_to(next) {
            return Err(MaintenanceDomainError::InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        if at < self.updated_at {
            return Err(MaintenanceDomainError::InvalidWindow);
        }
        self.status = next;
        self.updated_at = at;
        self.bump_version()?;
        self.uncommitted_events.push(DomainEvent::StatusChanged {
            work_order_id: self.id,
            asset: self.asset_fact.asset,
            status: next,
            scheduled_for: self.scheduled_for,
            at,
        });
        self.validate()
    }

    fn bump_version(&mut self) -> Result<(), MaintenanceDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(MaintenanceDomainError::VersionOverflow)?;
        Ok(())
    }

    pub const fn id(&self) -> WorkOrderId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn request(&self) -> &MaintenanceRequest {
        &self.request
    }
    pub const fn asset_fact(&self) -> &assets::AssetConditionFact {
        &self.asset_fact
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> WorkOrderStatus {
        self.status
    }
    pub const fn scheduled_for(&self) -> Option<DateTime<Utc>> {
        self.scheduled_for
    }
    pub const fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }
    pub const fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
    pub fn downtime(&self) -> &[Downtime] {
        &self.downtime
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

fn validate_scope(scope: &TenantScope) -> Result<(), MaintenanceDomainError> {
    if !matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) || scope.validate().is_err()
    {
        Err(MaintenanceDomainError::InvalidScope)
    } else {
        Ok(())
    }
}

fn require_id(value: Uuid, field: &'static str) -> Result<(), MaintenanceDomainError> {
    if value.is_nil() {
        Err(MaintenanceDomainError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn require_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), MaintenanceDomainError> {
    if value.trim().is_empty() {
        return Err(MaintenanceDomainError::Required { field });
    }
    if value.len() > maximum {
        return Err(MaintenanceDomainError::TooLong { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(MaintenanceDomainError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
