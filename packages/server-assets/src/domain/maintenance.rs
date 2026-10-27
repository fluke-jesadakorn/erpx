use erpx_server_shared::public::{CompanyId, HoldingId, RecordId};
/// Minimal physical-Asset relationship for actual Maintenance source assembly.
/// This does not grant record access, purchases, cost visibility or stock writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceAssetFacts {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub asset_id: RecordId,
    pub branch_id: RecordId,
    pub department_id: Option<RecordId>,
    pub version: i64,
}

/// Physical lifecycle data, never permission or a current source witness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceAssetCondition {
    Good,
    Degraded,
    Unsafe,
}
impl MaintenanceAssetCondition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Degraded => "degraded",
            Self::Unsafe => "unsafe",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceAssetLifecycle {
    Registered,
    InService,
    Maintenance,
    RetirementPending,
    Retired,
}
impl MaintenanceAssetLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::InService => "in_service",
            Self::Maintenance => "maintenance",
            Self::RetirementPending => "retirement_pending",
            Self::Retired => "retired",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceAssetState {
    pub version: i64,
    pub condition: MaintenanceAssetCondition,
    pub lifecycle: MaintenanceAssetLifecycle,
    /// PostgreSQL NULL, rather than JavaScript truthiness: empty placement is
    /// still present for the original complete/cancel lifecycle expression.
    pub placement_present: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaintenancePhysicalAction {
    Start,
    Complete {
        condition: MaintenanceAssetCondition,
    },
    Cancel {
        downtime_started_at: Option<i64>,
    },
}
impl MaintenancePhysicalAction {
    pub fn event_action(&self) -> &'static str {
        match self {
            Self::Start => "maintenance-start",
            Self::Complete { .. } => "maintenance-complete",
            Self::Cancel { .. } => "maintenance-cancel",
        }
    }
}
/// Expected data only. Start lifecycle/version admission belongs to its
/// original late effect position after the Work CAS/update. This derivation
/// must not move a deferred Asset denial ahead of the Work version refusal.
pub fn maintenance_asset_after(
    before: &MaintenanceAssetState,
    action: &MaintenancePhysicalAction,
) -> Result<Option<MaintenanceAssetState>, erpx_server_shared::public::BusinessError> {
    use erpx_server_shared::public::BusinessError;
    if matches!(
        action,
        MaintenancePhysicalAction::Cancel {
            downtime_started_at: None | Some(0)
        }
    ) {
        return Ok(None);
    }
    let version = before
        .version
        .checked_add(1)
        .filter(|n| *n > 1 && *n <= 9_007_199_254_740_992)
        .ok_or_else(|| {
            BusinessError::invalid(
                "ASSET_MAINTENANCE_VERSION_INVALID",
                "Retain a bounded current physical Asset version.",
            )
        })?;
    let condition = match action {
        MaintenancePhysicalAction::Complete { condition } => *condition,
        _ => before.condition,
    };
    let lifecycle = match action {
        MaintenancePhysicalAction::Start => MaintenanceAssetLifecycle::Maintenance,
        MaintenancePhysicalAction::Complete {
            condition: MaintenanceAssetCondition::Unsafe,
        } => MaintenanceAssetLifecycle::Maintenance,
        _ if before.placement_present => MaintenanceAssetLifecycle::InService,
        _ => MaintenanceAssetLifecycle::Registered,
    };
    Ok(Some(MaintenanceAssetState {
        version,
        condition,
        lifecycle,
        placement_present: before.placement_present,
    }))
}
/// This is a late factual guard, not operational/purchase authorization.
pub fn require_maintenance_start_source(
    before: &MaintenanceAssetState,
    expected_version: i64,
) -> Result<(), erpx_server_shared::public::BusinessError> {
    use erpx_server_shared::public::{BusinessError, FailureKind};
    if before.version != expected_version
        || !matches!(
            before.lifecycle,
            MaintenanceAssetLifecycle::Registered | MaintenanceAssetLifecycle::InService
        )
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        ));
    }
    Ok(())
}
