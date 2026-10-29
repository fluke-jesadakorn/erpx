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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceIssueAction {
    #[serde(rename = "issue-part")]
    IssuePart,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceIssueAllocation {
    pub lot_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub quantity_milli: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub balance_version: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceIssueInput {
    pub id: String,
    pub action: FieldServiceIssueAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub reservation_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub quantity_milli: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub entry_date: Option<String>,
    pub allocations: Vec<FieldServiceIssueAllocation>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceIssueEvent {
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub version: i64,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub reservation_id: String,
    pub consumption_id: String,
    pub journal_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub quantity_milli: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub cost_minor: i64,
}

/// Retained Service request result. Version and status are the original result.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRequestResult {
    pub id: String,
    pub ticket_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts", schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub version: i64,
    pub status: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRequestOrderRead {
    pub id: String,
    pub ticket_id: String,
    pub agreement_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts", schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub agreement_version: i64,
    pub customer_id: String,
    pub status: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts", schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub version: i64,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRequestTicketRead {
    pub id: String,
    pub status: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts", schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub version: i64,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRequestEvent {
    pub actor_id: String,
    pub status: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts", schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub version: i64,
    pub issue: String,
    pub site: String,
    pub technician_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    #[cfg_attr(feature="contracts", schema(value_type=Option<i64>,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub scheduled_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceRequestEntity {
    #[serde(rename = "request")]
    Request,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRequestInput {
    pub entity: FieldServiceRequestEntity,
    pub agreement_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts", schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub agreement_version: i64,
    pub title: String,
    pub issue: String,
    pub site: String,
    pub technician_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional, type = "number | null"))]
    #[cfg_attr(feature="contracts", schema(value_type=Option<i64>,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub scheduled_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceOrderCreateInput {
    pub ticket_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub ticket_version: i64,
    pub agreement_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub agreement_version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    pub issue: String,
    pub site: String,
    pub technician_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional, type = "number | null"))]
    #[cfg_attr(feature="contracts",schema(value_type=Option<i64>,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub scheduled_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceCreatedOrderRead {
    pub id: String,
    pub ticket_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub ticket_version: i64,
    pub agreement_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub agreement_version: i64,
    pub customer_id: String,
    pub asset_id: Option<String>,
    pub status: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature="contracts",schema(value_type=i64,minimum=1,maximum=9_007_199_254_740_991_i64))]
    pub version: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceVisitEvent {
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
    pub technician_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number | null"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Option<i64>, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub scheduled_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional, type = "number | null"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Option<i64>, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
    pub labor_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    pub customer_confirmation: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceAgreementReviewEvent {
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub version: i64,
    pub action: String,
    pub actor_id: String,
    pub reason: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceAgreementReviewEntity {
    #[serde(rename = "agreement")]
    Agreement,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum FieldServiceAgreementReviewAction {
    Activate,
    Reject,
    Retire,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceAgreementReviewInput {
    pub entity: FieldServiceAgreementReviewEntity,
    pub id: String,
    pub action: FieldServiceAgreementReviewAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceAgreementCreateInput {
    pub entity: FieldServiceAgreementReviewEntity,
    pub branch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department_id: Option<String>,
    pub customer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    pub title: String,
    pub start_date: String,
    pub end_date: String,
    pub currency: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
    pub visit_rate_minor: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "contracts", ts(optional))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 10_000))]
    pub warranty_discount_bps: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceScheduleAction {
    #[serde(rename = "schedule")]
    Schedule,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceVisitStepAction {
    #[serde(rename = "dispatch")]
    Dispatch,
    #[serde(rename = "start")]
    Start,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceScheduleInput {
    pub id: String,
    pub action: FieldServiceScheduleAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub technician_id: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub scheduled_at: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceVisitStepInput {
    pub id: String,
    pub action: FieldServiceVisitStepAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceCompletionAction {
    #[serde(rename = "complete")]
    Complete,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceCompletionInput {
    pub id: String,
    pub action: FieldServiceCompletionAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 0, maximum = 9_007_199_254_740_991_i64))]
    pub labor_minutes: i64,
    pub evidence: String,
    pub customer_confirmation: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceRecoveryAction {
    #[serde(rename = "exception")]
    Exception,
    #[serde(rename = "resume")]
    Resume,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceRecoveryInput {
    pub id: String,
    pub action: FieldServiceRecoveryAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceCancelAction {
    #[serde(rename = "cancel")]
    Cancel,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceCancelInput {
    pub id: String,
    pub action: FieldServiceCancelAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub enum FieldServiceBillingAction {
    #[serde(rename = "release-billing")]
    ReleaseBilling,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldServiceBillingInput {
    pub id: String,
    pub action: FieldServiceBillingAction,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
    pub expected_version: i64,
    pub reason: String,
}
