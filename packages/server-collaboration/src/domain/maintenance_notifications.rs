//! Original Maintenance notification data. None of these values is authority.
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceNotificationKind {
    Created,
    Updated,
}

/// Intended owner after-facts supplied by the actual retained Work effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceNotificationFacts {
    pub owner_key: CommandKey,
    pub kind: MaintenanceNotificationKind,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub work_id: RecordId,
    pub owner_user_id: UserId,
    pub work_version_after: i64,
    pub updated_at_after: TimestampMillis,
}

/// Derived notification data, with no persistence or authority constructor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceNotification {
    pub(crate) facts: MaintenanceNotificationFacts,
    pub(crate) id: RecordId,
    pub(crate) message: &'static str,
}
impl MaintenanceNotification {
    pub fn facts(&self) -> &MaintenanceNotificationFacts {
        &self.facts
    }
    pub fn id(&self) -> &RecordId {
        &self.id
    }
    pub fn message(&self) -> &'static str {
        self.message
    }
    pub fn event_id(&self) -> String {
        format!("legacy:{}", self.id.as_str())
    }
}
pub fn derive_maintenance_notification(
    facts: &MaintenanceNotificationFacts,
    key: &CommandKey,
    kind: MaintenanceNotificationKind,
) -> Result<MaintenanceNotification, BusinessError> {
    const SAFE: i64 = 9_007_199_254_740_991;
    if &facts.owner_key != key || facts.kind != kind {
        return Err(BusinessError::invalid(
            "MAINTENANCE_NOTIFICATION_BINDING_REQUIRED",
            "Use the exact owner command key and notification action.",
        ));
    }
    // A valid MAX_SAFE expectedVersion has a provisional next version one
    // greater. Preserve preparation error order; the actual owner apply gate
    // must reject unsafe output before any Work or notification insertion.
    if !(1..=SAFE + 1).contains(&facts.work_version_after)
        || !(0..=SAFE).contains(&facts.updated_at_after.get())
    {
        return Err(BusinessError::invalid(
            "MAINTENANCE_NOTIFICATION_INPUT_INVALID",
            "Use the exact bounded intended Work version and timestamp.",
        ));
    }
    Ok(MaintenanceNotification {
        facts: facts.clone(),
        id: RecordId::new(format!("maintenance-notice-{}", key.as_str()))?,
        message: match kind {
            MaintenanceNotificationKind::Created => {
                "New asset maintenance work: review schedule and condition"
            }
            MaintenanceNotificationKind::Updated => {
                "Asset maintenance work updated: review current next action"
            }
        },
    })
}
