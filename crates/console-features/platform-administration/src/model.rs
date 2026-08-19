#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingItem {
    pub id: String,
    pub code: String,
    pub name: String,
    pub status: String,
    pub environment_id: String,
    pub version: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorItem {
    pub id: String,
    pub key: String,
    pub kind: String,
    pub state: String,
    pub capabilities: Vec<String>,
    pub version: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminContextItem {
    pub id: String,
    pub holding_id: String,
    pub principal_ref: String,
    pub reason: String,
    pub status: String,
    pub expires_at: String,
    pub version: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformAdministrationContent {
    Holding(HoldingItem),
    Holdings(Vec<HoldingItem>),
    Connector(ConnectorItem),
    Connectors(Vec<ConnectorItem>),
    AdminContext(AdminContextItem),
}
