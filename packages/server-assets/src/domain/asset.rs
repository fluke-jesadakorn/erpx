//! Native physical Asset register, condition, placement, retirement and
//! Finance-link policy.
//!
//! This module owns only the portable lifecycle facts. Current permission,
//! Commercial purchase admission, Workflow replay/audit and Money snapshots
//! are supplied by the enclosing composition; nothing here mints authority or
//! commits a transaction.
use crate::domain::maintenance::{MaintenanceAssetCondition, MaintenanceAssetLifecycle};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId, UserId};

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// ECMAScript `String.prototype.trim` removes WhiteSpace and LineTerminator
/// code points. Rust's `str::trim` uses the Unicode White_Space property,
/// which differs for example on U+0085 NEL. Parsed Asset text must match the
/// retained Zod/JavaScript boundary, so trim this exact closed set.
pub fn ecma_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(
            c,
            '\u{0009}'
                | '\u{000A}'
                | '\u{000B}'
                | '\u{000C}'
                | '\u{000D}'
                | '\u{0020}'
                | '\u{00A0}'
                | '\u{1680}'
                | '\u{2000}'
                ..='\u{200A}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202F}'
                    | '\u{205F}'
                    | '\u{3000}'
                    | '\u{FEFF}'
        )
    })
}

/// Count UTF-16 code units for JavaScript operations that explicitly need them.
pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

pub fn validate_trimmed_text(
    value: &str,
    minimum: usize,
    maximum: usize,
    code: &'static str,
    message: &'static str,
) -> Result<String, BusinessError> {
    let value = ecma_trim(value);
    // Zod 4 min/max count Unicode code points for valid UTF-8 strings.
    let length = value.chars().count();
    if length < minimum || length > maximum {
        return Err(BusinessError::new(FailureKind::InvalidInput, code, message));
    }
    Ok(value.to_owned())
}

pub fn validate_reason(value: &str) -> Result<String, BusinessError> {
    validate_trimmed_text(
        value,
        3,
        2000,
        "VALIDATION_FAILED",
        "Enter a reason between 3 and 2000 characters.",
    )
}

/// `z.string().date()` accepts a real proleptic Gregorian `YYYY-MM-DD` date.
pub fn validate_date(value: &str) -> Result<String, BusinessError> {
    let invalid =
        || BusinessError::invalid("VALIDATION_FAILED", "Enter a real date in YYYY-MM-DD form.");
    if !value.is_ascii()
        || value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || value.bytes().enumerate().any(|(index, byte)| {
            index != 4 && index != 7 && !byte.is_ascii_digit()
        })
    {
        return Err(invalid());
    }
    let year = value[0..4].parse::<i32>().map_err(|_| invalid())?;
    let month = value[5..7].parse::<u32>().map_err(|_| invalid())?;
    let day = value[8..10].parse::<u32>().map_err(|_| invalid())?;
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(invalid()),
    };
    if !(1..=days).contains(&day) {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

/// JSON integral value inside the JavaScript safe-integer range.
pub fn json_integer(value: &serde_json::Value, minimum: i64) -> Option<i64> {
    if let Some(integer) = value.as_i64() {
        return (integer >= minimum && integer <= MAX_SAFE_INTEGER).then_some(integer);
    }
    let float = value.as_f64()?;
    (float.is_finite()
        && float.fract() == 0.0
        && float >= minimum as f64
        && float <= MAX_SAFE_INTEGER as f64)
        .then_some(float as i64)
}

/// Parse the retained Zod cost shape without loading a Company scale. This is
/// done before replay; decimal conversion belongs after an authorized replay.
pub(crate) enum AssetCostInput {
    Minor(i64),
    Decimal(String),
}
pub(crate) fn parse_asset_cost(raw: &serde_json::Value) -> Result<AssetCostInput, BusinessError> {
    let invalid = || BusinessError::invalid("VALIDATION_FAILED", "Supply one valid acquisition cost.");
    let object = raw.as_object().ok_or_else(invalid)?;
    match (object.get("costMinor"), object.get("cost")) {
        (Some(value), None) => json_integer(value, 0).map(AssetCostInput::Minor).ok_or_else(invalid),
        (None, Some(serde_json::Value::String(value))) => {
            let mut parts = value.split('.');
            let whole = parts.next().unwrap_or_default();
            let fractional = parts.next();
            if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit())
                || fractional.is_some_and(|v| v.is_empty() || v.len() > 12 || !v.bytes().all(|b| b.is_ascii_digit()))
                || parts.next().is_some() { return Err(invalid()); }
            Ok(AssetCostInput::Decimal(value.clone()))
        }
        _ => Err(invalid()),
    }
}

pub fn validate_currency(value: &str) -> Result<String, BusinessError> {
    if value.chars().count() != 3 {
        return Err(BusinessError::invalid(
            "VALIDATION_FAILED",
            "Use a three-letter ISO currency.",
        ));
    }
    Ok(value.to_uppercase())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetState {
    pub version: i64,
    pub condition: MaintenanceAssetCondition,
    pub lifecycle: MaintenanceAssetLifecycle,
    pub placement: Option<String>,
    pub custodian_id: Option<String>,
    pub retirement_reason: Option<String>,
    pub finance_asset_id: Option<String>,
    pub finance_asset_version: Option<i64>,
    pub currency: String,
    pub cost_minor: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetAction {
    Place {
        placement: String,
        custodian_id: String,
    },
    Condition {
        condition: MaintenanceAssetCondition,
    },
    RetireRequest,
    RetireApprove,
    LinkFinance {
        finance_asset_id: String,
        finance_asset_version: i64,
    },
}
impl AssetAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Place { .. } => "place",
            Self::Condition { .. } => "condition",
            Self::RetireRequest => "retire-request",
            Self::RetireApprove => "retire-approve",
            Self::LinkFinance { .. } => "link-finance",
        }
    }
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Place { .. } => "asset.place",
            Self::Condition { .. } => "asset.condition",
            Self::RetireRequest => "asset.retire-request",
            Self::RetireApprove => "asset.retire-approve",
            Self::LinkFinance { .. } => "asset.link-finance",
        }
    }
}

/// The coordinator supplies this only after checking command key, operation,
/// actor and selected Company against an actual receipt. This is not replay
/// authorization or fingerprint validation and never comes from client labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetPurchaseDisposition { Fresh, MatchingReceipt }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetPurchasePurpose { NewWork, SettleExisting, History }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetPurchaseRequirements {
    pub operation: &'static str,
    pub capability: &'static str,
    pub purpose: AssetPurchasePurpose,
    pub permissions: &'static [&'static str],
    pub creating: bool,
}
fn purchase_purpose(fresh:AssetPurchasePurpose,disposition:AssetPurchaseDisposition)->AssetPurchasePurpose {
    match disposition {AssetPurchaseDisposition::Fresh=>fresh,AssetPurchaseDisposition::MatchingReceipt=>AssetPurchasePurpose::History}
}
/// Closed mapping from parsed owner input to the legacy purchase request.
/// It proves neither receipt provenance nor current Company/actor authority.
pub(crate) fn asset_create_purchase_from_validated(input:&NewAsset,disposition:AssetPurchaseDisposition)->AssetPurchaseRequirements {
    AssetPurchaseRequirements {operation:"asset.create",capability:"assets.register",purpose:purchase_purpose(AssetPurchasePurpose::NewWork,disposition),permissions:if input.finance_asset_id.is_some(){&["records.write","finance.write"]}else{&["records.write"]},creating:true}
}
/// Only a schema-parsed action can reach this mapping; invalid or unknown
/// wire actions must fail validation before any purchase decision.
pub(crate) fn asset_action_purchase_from_validated(action:&AssetAction,disposition:AssetPurchaseDisposition)->AssetPurchaseRequirements {
    let (capability,fresh,permissions):(&str,AssetPurchasePurpose,&[&str])=match action {
        AssetAction::Place{..}=>("assets.custody",AssetPurchasePurpose::NewWork,&["records.write"]),
        AssetAction::LinkFinance{..}=>("assets.register",AssetPurchasePurpose::SettleExisting,&["records.write","finance.write"]),
        AssetAction::RetireApprove=>("assets.register",AssetPurchasePurpose::SettleExisting,&["records.write","records.review"]),
        AssetAction::Condition{..}|AssetAction::RetireRequest=>("assets.register",AssetPurchasePurpose::SettleExisting,&["records.write"]),
    };
    AssetPurchaseRequirements {operation:action.operation(),capability,purpose:purchase_purpose(fresh,disposition),permissions,creating:false}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetChangeInput {
    pub expected_version: i64,
    pub action: AssetAction,
}
impl AssetChangeInput {
    pub fn parse(value: &serde_json::Value) -> Result<Self, BusinessError> {
        let invalid = || {
            BusinessError::invalid(
                "VALIDATION_FAILED",
                "Review the requested Asset action and its current version.",
            )
        };
        let object = value.as_object().ok_or_else(invalid)?;
        let expected_version = json_integer(object.get("expectedVersion").ok_or_else(invalid)?, 1)
            .ok_or_else(invalid)?;
        let action = match object.get("action").and_then(|value| value.as_str()) {
            Some("place") => {
                let placement = object
                    .get("placement")
                    .and_then(|value| value.as_str())
                    .ok_or_else(invalid)?;
                let placement = validate_trimmed_text(
                    placement,
                    2,
                    200,
                    "VALIDATION_FAILED",
                    "Enter a placement between 2 and 200 characters.",
                )?;
                let custodian = object
                    .get("custodianId")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(invalid)?;
                AssetAction::Place {
                    placement,
                    custodian_id: custodian.to_owned(),
                }
            }
            Some("condition") => AssetAction::Condition {
                condition: match object.get("condition").and_then(|value| value.as_str()) {
                    Some("good") => MaintenanceAssetCondition::Good,
                    Some("degraded") => MaintenanceAssetCondition::Degraded,
                    Some("unsafe") => MaintenanceAssetCondition::Unsafe,
                    _ => return Err(invalid()),
                },
            },
            Some("retire-request") => AssetAction::RetireRequest,
            Some("retire-approve") => AssetAction::RetireApprove,
            Some("link-finance") => {
                let finance_asset_id = object
                    .get("financeAssetId")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(invalid)?;
                let finance_asset_version =
                    object
                        .get("financeAssetVersion")
                        .ok_or_else(invalid)
                        .and_then(|value| json_integer(value, 1).ok_or_else(invalid))?;
                AssetAction::LinkFinance {
                    finance_asset_id: finance_asset_id.to_owned(),
                    finance_asset_version,
                }
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            expected_version,
            action,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetChange {
    pub expected_version: i64,
    pub action: AssetAction,
    pub actor_id: UserId,
    pub command_key: String,
    pub reason: String,
    pub at: i64,
}
impl AssetChange {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !(1..=MAX_SAFE_INTEGER).contains(&self.expected_version) {
            return Err(BusinessError::invalid(
                "VALIDATION_FAILED",
                "Provide the current positive Asset version.",
            ));
        }
        if !(0..=MAX_SAFE_INTEGER).contains(&self.at) {
            return Err(BusinessError::invalid(
                "VALIDATION_FAILED",
                "Use a current bounded command time.",
            ));
        }
        if self.command_key.is_empty() || self.command_key.len() > 180 {
            return Err(BusinessError::invalid(
                "VALIDATION_FAILED",
                "Use a valid command key for this action.",
            ));
        }
        validate_reason(&self.reason)?;
        Ok(())
    }
}

/// Explicit after-image owned by this action. `finance_asset_id` is only
/// present for `link-finance`; other actions retain the prior link.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetTransition {
    pub version: i64,
    pub condition: MaintenanceAssetCondition,
    pub lifecycle: MaintenanceAssetLifecycle,
    pub placement: Option<String>,
    pub custodian_id: Option<String>,
    pub retirement_reason: Option<String>,
    pub finance_asset_id: Option<String>,
    pub finance_asset_version: Option<i64>,
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// Derive the exact action after-image. The expected-version comparison stays
/// a separate late guard after authorization/current-source checks, matching
/// the retained command order.
pub fn asset_after(
    state: &AssetState,
    change: &AssetChange,
    prior_request_actor: Option<&UserId>,
    has_posted_disposal: bool,
) -> Result<AssetTransition, BusinessError> {
    let action = &change.action;
    let mut next = AssetTransition {
        version: state
            .version
            .checked_add(1)
            .filter(|n| *n <= MAX_SAFE_INTEGER)
            .ok_or_else(|| {
                BusinessError::invalid(
                    "ASSET_VERSION_INVALID",
                    "The physical Asset version cannot advance further.",
                )
            })?,
        condition: state.condition,
        lifecycle: state.lifecycle,
        placement: state.placement.clone(),
        custodian_id: state.custodian_id.clone(),
        retirement_reason: state.retirement_reason.clone(),
        finance_asset_id: state.finance_asset_id.clone(),
        finance_asset_version: state.finance_asset_version,
    };
    match action {
        AssetAction::LinkFinance {
            finance_asset_id,
            finance_asset_version,
        } => {
            if state.finance_asset_id.is_some() {
                return Err(conflict(
                    "ASSET_FINANCE_LINK_IMMUTABLE",
                    "The recorded Finance asset cannot be replaced.",
                ));
            }
            next.finance_asset_id = Some(finance_asset_id.as_str().to_owned());
            next.finance_asset_version = Some(*finance_asset_version);
        }
        AssetAction::Place {
            placement,
            custodian_id,
        } => {
            if !matches!(
                state.lifecycle,
                MaintenanceAssetLifecycle::Registered | MaintenanceAssetLifecycle::InService
            ) {
                return Err(conflict(
                    "ASSET_PLACEMENT_BLOCKED",
                    "Only available assets can be placed.",
                ));
            }
            if state.condition == MaintenanceAssetCondition::Unsafe {
                return Err(conflict(
                    "ASSET_UNSAFE",
                    "Resolve the unsafe condition before placing this asset.",
                ));
            }
            next.lifecycle = MaintenanceAssetLifecycle::InService;
            next.placement = Some(placement.clone());
            next.custodian_id = Some(custodian_id.as_str().to_owned());
        }
        AssetAction::Condition { condition } => {
            if state.lifecycle == MaintenanceAssetLifecycle::Retired {
                return Err(conflict(
                    "ASSET_RETIRED",
                    "A retired asset cannot receive a current condition.",
                ));
            }
            next.condition = *condition;
        }
        AssetAction::RetireRequest => {
            if state.lifecycle == MaintenanceAssetLifecycle::RetirementPending {
                match prior_request_actor {
                    Some(requester) => {
                        if requester != &change.actor_id {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "ASSET_RETIREMENT_REFRESH_FORBIDDEN",
                                "Only the original retirement requester can review and renew this pending request.",
                            ));
                        }
                    }
                    None => {
                        return Err(conflict(
                            "ASSET_RETIREMENT_REFRESH_FORBIDDEN",
                            "Only the original retirement requester can review and renew this pending request.",
                        ));
                    }
                }
            } else if !matches!(
                state.lifecycle,
                MaintenanceAssetLifecycle::Registered | MaintenanceAssetLifecycle::InService
            ) {
                return Err(conflict(
                    "ASSET_RETIREMENT_BLOCKED",
                    "Return the asset from maintenance before requesting retirement.",
                ));
            }
            next.lifecycle = MaintenanceAssetLifecycle::RetirementPending;
            next.retirement_reason = Some(validate_reason(&change.reason)?);
        }
        AssetAction::RetireApprove => {
            if state.lifecycle != MaintenanceAssetLifecycle::RetirementPending {
                return Err(conflict(
                    "ASSET_RETIREMENT_NOT_PENDING",
                    "Retirement is not awaiting review.",
                ));
            }
            if state.cost_minor > 0 && !has_posted_disposal {
                return Err(conflict(
                    "ASSET_FINANCE_DISPOSAL_REQUIRED",
                    "Finance must post the linked asset disposal before physical retirement can be approved.",
                ));
            }
            next.lifecycle = MaintenanceAssetLifecycle::Retired;
            next.placement = None;
            next.custodian_id = None;
        }
    }
    Ok(next)
}

pub fn require_independent_retirement_review(
    requester: &UserId,
    approver: &UserId,
) -> Result<(), BusinessError> {
    if requester == approver {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ASSET_RETIREMENT_REVIEW_INDEPENDENT",
            "A different reviewer must approve this retirement.",
        ));
    }
    Ok(())
}

/// Late generic stale-version guard. All current-state/action refusals must
/// precede it, as in the retained command order.
pub fn require_expected_version(
    state: &AssetState,
    expected_version: i64,
) -> Result<(), BusinessError> {
    if state.version != expected_version {
        return Err(conflict(
            "CONFLICT",
            "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAsset {
    pub branch_id: String,
    pub department_id: Option<String>,
    pub code: String,
    pub name: String,
    pub category: String,
    pub currency: String,
    pub cost_minor: i64,
    pub acquired_date: String,
    pub warranty_end: Option<String>,
    pub finance_asset_id: Option<String>,
    pub finance_asset_version: Option<i64>,
}
impl NewAsset {
    /// Parse the retained create schema. Decimal `cost` needs the current
    /// Finance currency scale, so the caller resolves it before this method.
    pub fn parse(value: &serde_json::Value, cost_minor: i64) -> Result<Self, BusinessError> {
        let invalid = || {
            BusinessError::invalid(
                "VALIDATION_FAILED",
                "Review the Asset registration details.",
            )
        };
        let object = value.as_object().ok_or_else(invalid)?;
        let text = |key: &str| {
            object
                .get(key)
                .and_then(|value| value.as_str())
                .ok_or_else(invalid)
        };
        let code = validate_trimmed_text(
            text("code")?,
            1,
            80,
            "VALIDATION_FAILED",
            "Enter an asset code of at most 80 characters.",
        )?;
        let name = validate_trimmed_text(
            text("name")?,
            2,
            160,
            "VALIDATION_FAILED",
            "Enter an asset name between 2 and 160 characters.",
        )?;
        let category = validate_trimmed_text(
            text("category")?,
            2,
            80,
            "VALIDATION_FAILED",
            "Enter an asset category between 2 and 80 characters.",
        )?;
        let currency = validate_currency(text("currency")?)?;
        let acquired_date = validate_date(text("acquiredDate")?)?;
        let warranty_end = match object.get("warrantyEnd") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(validate_date(value.as_str().ok_or_else(invalid)?)?),
        };
        let department_id = match object.get("departmentId") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => {
                Some(value.as_str().ok_or_else(invalid)?.to_owned())
            }
        };
        let finance_asset_id = object
            .get("financeAssetId")
            .map(|value| value.as_str().filter(|value| !value.is_empty()).map(str::to_owned).ok_or_else(invalid))
            .transpose()?;
        let finance_asset_version = object
            .get("financeAssetVersion")
            .map(|value| json_integer(value, 1).ok_or_else(invalid))
            .transpose()?;
        if finance_asset_id.is_some() != finance_asset_version.is_some() {
            return Err(invalid());
        }
        if !(0..=MAX_SAFE_INTEGER).contains(&cost_minor) {
            return Err(BusinessError::invalid(
                "VALIDATION_FAILED",
                "Acquisition cost must be a nonnegative safe integer.",
            ));
        }
        Ok(Self {
            branch_id: text("branchId").and_then(|value| (!value.is_empty()).then_some(value).ok_or_else(invalid))?.to_owned(),
            department_id,
            code,
            name,
            category,
            currency,
            cost_minor,
            acquired_date,
            warranty_end,
            finance_asset_id,
            finance_asset_version,
        })
    }

    /// The selected work scope wins over a submitted branch/department, as in
    /// the retained registration contract.
    pub fn scoped(mut self, branch: Option<&RecordId>, department: Option<&RecordId>) -> Self {
        if let Some(branch) = branch {
            self.branch_id = branch.as_str().to_owned();
        }
        if let Some(department) = department {
            self.department_id = Some(department.as_str().to_owned());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_only_ecmascript_whitespace() {
        assert_eq!(
            ecma_trim("\u{0085}reason\u{0085}"),
            "\u{0085}reason\u{0085}"
        );
        assert_eq!(ecma_trim("\u{FEFF} reason \u{00A0}"), "reason");
    }
}
