//! Target-neutral Assets view models. Transport/domain mapping lives in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetItem {
    pub asset_id: String,
    pub version: u64,
    pub code: String,
    pub name: String,
    pub scope: String,
    pub location_id: String,
    pub status: String,
    pub condition: String,
    pub placed_in_service_at: Option<String>,
    pub retired_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetItems {
    pub assets: Vec<AssetItem>,
    pub next_cursor: Option<String>,
}
