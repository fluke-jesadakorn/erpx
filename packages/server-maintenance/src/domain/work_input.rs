//! Validated input data only; current Work and purchase/stock admission are separate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceLotInput {
    pub lot_id: String,
    pub quantity_milli: i64,
    pub balance_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaintenanceChangeAction {
    Schedule {
        owner_id: String,
        scheduled_at: i64,
    },
    Start,
    Block,
    Resume,
    Complete {
        resulting_condition: String,
        evidence: String,
    },
    IssuePart {
        warehouse_id: String,
        product_id: String,
        quantity_milli: i64,
        entry_date: String,
        allocations: Vec<MaintenanceLotInput>,
    },
    Cancel,
}
impl MaintenanceChangeAction {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Schedule { .. } => "schedule",
            Self::Start => "start",
            Self::Block => "block",
            Self::Resume => "resume",
            Self::Complete { .. } => "complete",
            Self::IssuePart { .. } => "issue-part",
            Self::Cancel => "cancel",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceChangeInput {
    pub expected_version: i64,
    pub reason: String,
    pub action: MaintenanceChangeAction,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceCreateInput {
    pub asset_id: String,
    pub asset_version: i64,
    pub title: String,
    pub priority: String,
    pub owner_id: String,
    pub scheduled_at: Option<i64>,
    pub reason: String,
}
