//! Validated preventive-action input only; current plan state, timezone
//! recurrence and asset authority are separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenancePreventiveAction {
    Activate,
    Pause,
    Cancel,
    Generate,
}

impl MaintenancePreventiveAction {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Pause => "pause",
            Self::Cancel => "cancel",
            Self::Generate => "generate",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenancePreventiveActionInput {
    pub expected_version: i64,
    pub reason: String,
    pub action: MaintenancePreventiveAction,
}
