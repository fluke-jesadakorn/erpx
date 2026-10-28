//! Bounded owner facts returned after current Service and purchase authorization.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceReserveAction {
    #[serde(rename = "reserve-part")]
    ReservePart,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceReserveInput {
    pub id: String,
    pub action: FieldServiceReserveAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub warehouse_id: String,
    pub product_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub quantity_milli: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expires_at: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceReserveResult {
    pub id: String,
    pub status: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub version: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceReservationRead {
    pub id: String,
    pub order_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
    pub quantity_milli: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
    pub consumed_milli: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
    pub released_milli: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expires_at: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceStockSnapshot {
    pub warehouse_id: String,
    pub product_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
    pub quantity_milli: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub version: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceReserveEvent {
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub version: i64,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
}
