//! Rust-owned Service Count transport contracts.
use crate::field_service_reservation_read::FieldServiceRenderedContext;
use serde::{Deserialize, Serialize};
// Rust-owned projections mirror the retained wire, checked before returning.
macro_rules! dto { ($name:ident { $($(#[$attr:meta])* $field:ident : $ty:ty),* $(,)? }) => {
 #[derive(Clone, Debug, Serialize, Deserialize)]
 #[cfg_attr(feature="contracts", derive(ts_rs::TS, utoipa::ToSchema))]
 #[serde(rename_all="camelCase")]
 pub struct $name { $($(#[$attr])* pub $field:$ty),* }
}; }
dto!(CountReadiness {
    schema_available: bool,
    posting_configured: bool,
    variance_account_available: bool,
    posting_period_available: bool
});
dto!(CountSelection { warehouse_id:Option<String>, product_id:Option<String>, count_date:Option<String> });
dto!(CountWarehouse {
    id: String,
    name: String,
    branch_id: String
});
dto!(CountBranch {
    id: String,
    name: String
});
dto!(CountProduct {
    id: String,
    sku: String,
    name: String,
    unit: String,
    tracking_mode: String
});
dto!(CountLot { lot_code:Option<String>, serial_number:Option<String>, manufactured_date:Option<String>, expiry_date:Option<String>, counted_quantity:String, unit_cost:Option<String> });
dto!(CountDecision { action:String, reason:Option<String>, actor_name:String, #[cfg_attr(feature="contracts", ts(type="number"), schema(minimum=-9007199254740991i64, maximum=9007199254740991i64))] created_at:i64 });
dto!(CountRecord { source_kind:CountSourceKind, id:String, number:String, status:String, #[cfg_attr(feature="contracts", ts(type="number"), schema(minimum=0, maximum=9007199254740991i64))] version:i64, count_date:String, reason:String, warehouse_id:String, warehouse_name:String, product_id:String, sku:String, product_name:String, unit:String, tracking_mode:String, counted_quantity:String, counted_value:String, unit_cost:Option<String>, lots:Vec<CountLot>, journal_id:Option<String>, can_submit:bool, can_approve:bool, can_post:bool, can_reject:bool, can_cancel:bool, decisions:Vec<CountDecision> });
dto!(CountWorkspace { company_id:String, available:bool, currency:Option<String>, document_currency:Option<String>, context_key:Option<String>, unavailable_reason:Option<CountUnavailableReason>, can_prepare:bool, can_create_warehouse:bool, can_create_product:bool, selection:CountSelection, readiness:CountReadiness, warehouses:Vec<CountWarehouse>, branches:Vec<CountBranch>, products:Vec<CountProduct>, record:Option<CountRecord> });
dto!(CountFlowPerson { user_id:String, name:String, team_id:Option<String>, team_name:String });
dto!(CountFlowStep { key:String, action:String, role:String, allowed_roles:Vec<String>, team_name:String, primary:Option<CountFlowPerson>, candidates:Vec<CountFlowPerson>, status:String, blocker:Option<String> });
dto!(CountFlowHistory {
    action: String,
    actor_name: String,
    #[cfg_attr(feature="contracts", ts(type="number"), schema(minimum=-9007199254740991i64, maximum=9007199254740991i64))]
    at: i64,
    reason: String,
    step: String
});
dto!(CountFlow { kind:CountFlowKind, record_id:String, #[cfg_attr(feature="contracts", ts(type="number"), schema(minimum=0, maximum=9007199254740991i64))] record_version:i64, #[cfg_attr(feature="contracts", ts(type="number"), schema(minimum=0, maximum=9007199254740991i64))] version:i64, #[cfg_attr(feature="contracts", ts(type="number"), schema(minimum=0, maximum=9007199254740991i64))] defaults_revision:i64, state:String, steps:Vec<CountFlowStep>, current:Option<CountFlowStep>, next:Option<CountFlowStep>, can_act:bool, can_manage:bool, blocker:Option<String>, history:Vec<CountFlowHistory> });
dto!(CountActionResult { id:String, status:String, #[cfg_attr(feature="contracts", ts(type="number"), schema(minimum=0, maximum=9007199254740991i64))] version:i64, journal_id:Option<String> });
dto!(CountCreateResult {
    id: String,
    status: String,
    #[cfg_attr(
        feature = "contracts",
        ts(type = "number"),
        schema(minimum = 0, maximum = 9007199254740991i64)
    )]
    version: i64
});
dto!(CountAssignmentResult {
    id: String,
    #[cfg_attr(
        feature = "contracts",
        ts(type = "number"),
        schema(minimum = 0, maximum = 9007199254740991i64)
    )]
    version: i64
});
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(untagged)]
pub enum ServiceInitialCountReadData {
    Workspace(CountWorkspace),
    Flow(CountFlow),
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(untagged)]
pub enum ServiceInitialCountCommandData {
    Create(CountCreateResult),
    Action(CountActionResult),
    Assign(CountAssignmentResult),
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceInitialCountReadResponse {
    pub data: ServiceInitialCountReadData,
    pub context: FieldServiceRenderedContext,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceInitialCountCommandResponse {
    pub data: ServiceInitialCountCommandData,
    pub context: FieldServiceRenderedContext,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum CountFlowKind {
    #[serde(rename = "stock_count")]
    StockCount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum CountSourceKind {
    #[serde(rename = "service-initial-physical-stock-count")]
    ServiceInitialPhysicalStockCount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "kebab-case")]
pub enum CountUnavailableReason {
    PurchaseRequired,
    StockAuthorityRequired,
    ServicePostingRequired,
    SelectionRequired,
    SelectionUnavailable,
    VarianceAccountRequired,
    PeriodRequired,
    PriorStockHistory,
    SchemaRequired,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CountCreateInput {
    #[cfg_attr(feature = "contracts", ts(type = "0"))]
    pub expected_version: u8,
    pub context_key: String,
    pub input: CountDraftInput,
}
dto!(CountDraftInput { warehouse_id:String, count_date:String, reason:String, lines:Vec<CountLineInput> });
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CountLineInput {
    pub product_id: String,
    pub counted_quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub unit_cost: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub allocations: Option<Vec<CountAllocationInput>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CountAllocationInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub lot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub lot_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub manufactured_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub expiry_date: Option<String>,
    pub counted_quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub unit_cost: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CountActionInput {
    #[cfg_attr(
        feature = "contracts",
        ts(type = "number"),
        schema(minimum = 0, maximum = 9007199254740991i64)
    )]
    pub expected_version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub attachment_set_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CountAssignmentInput {
    pub step_key: String,
    pub user_id: String,
    #[cfg_attr(
        feature = "contracts",
        ts(type = "number"),
        schema(minimum = 0, maximum = 9007199254740991i64)
    )]
    pub expected_version: i64,
    #[cfg_attr(
        feature = "contracts",
        ts(type = "number"),
        schema(minimum = 1, maximum = 9007199254740991i64)
    )]
    pub record_version: i64,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        feature = "contracts",
        ts(optional, type = "false"),
        schema(nullable = false)
    )]
    pub save_default: Option<bool>,
}
