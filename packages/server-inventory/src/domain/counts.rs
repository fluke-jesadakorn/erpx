//! Portable Inventory stock-count rules for the generic Count writer.
//!
//! A Count snapshots the current stock and lot evidence, records the physical
//! result and later transitions it through submission, review and posting.
//! This planner owns the save/admission rules and the status transition gate;
//! current IAM/Commercial authority, the command ledger, Finance journals and
//! Files evidence stay at the composition boundary. It never writes by itself.
use super::service_initial_physical_count::{
    CountAction, CountAuthority, CountStatus, js_trim, transition_count, valid_date,
};
use super::valuation::{MAX_SAFE_UNITS, safe, scaled_inventory_value};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_LINES: usize = 50;
pub const MAX_ALLOCATIONS: usize = 500;
pub const MAX_REASON: usize = 2_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountAllocationInput {
    #[serde(default)]
    pub lot_id: Option<String>,
    #[serde(default)]
    pub lot_code: Option<String>,
    #[serde(default)]
    pub serial_number: Option<String>,
    #[serde(default)]
    pub manufactured_date: Option<String>,
    #[serde(default)]
    pub expiry_date: Option<String>,
    pub counted_quantity: String,
    #[serde(default)]
    pub unit_cost: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountLineInput {
    pub product_id: String,
    pub counted_quantity: String,
    #[serde(default)]
    pub unit_cost: Option<String>,
    #[serde(default)]
    pub allocations: Vec<CountAllocationInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountSaveInput {
    pub warehouse_id: String,
    pub count_date: String,
    pub reason: String,
    #[serde(default)]
    pub expected_version: Option<i64>,
    pub lines: Vec<CountLineInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountRecord {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub number: String,
    pub count_date: String,
    pub reason: String,
    pub status: String,
    pub version: i64,
    pub net_value_minor: i64,
    pub created_by: String,
    pub prepared_by: String,
    pub approved_by: Option<String>,
    pub quality_hold_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountWarehouse {
    pub id: String,
    pub branch_id: String,
    pub version: i64,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountProduct {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub version: i64,
    pub tracking_mode: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountStock {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountLot {
    pub id: String,
    pub product_id: String,
    pub tracking_mode: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CountSource {
    pub current: Option<CountRecord>,
    pub warehouse: Option<CountWarehouse>,
    pub products: BTreeMap<String, CountProduct>,
    pub stocks: BTreeMap<String, CountStock>,
    pub lots: BTreeMap<String, Vec<CountLot>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountSaveLotPlan {
    pub id: String,
    pub count_line_id: String,
    pub source_lot_id: Option<String>,
    pub result_lot_id: String,
    pub tracking_mode: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub snapshot_quantity_milli: i64,
    pub snapshot_value_minor: i64,
    pub snapshot_version: i64,
    pub counted_quantity_milli: i64,
    pub counted_value_minor: i64,
    pub unit_cost_minor: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountSaveLinePlan {
    pub id: String,
    pub product_id: String,
    pub product_version: i64,
    pub description: String,
    pub unit: String,
    pub tracking_mode: String,
    pub snapshot_quantity_milli: i64,
    pub snapshot_value_minor: i64,
    pub snapshot_version: i64,
    pub counted_quantity_milli: i64,
    pub counted_value_minor: i64,
    pub unit_cost_minor: Option<i64>,
    pub ordinal: i64,
    pub lots: Vec<CountSaveLotPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountSavePlan {
    pub record_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub number: String,
    pub count_date: String,
    pub reason: String,
    pub net_value_minor: i64,
    pub version: i64,
    pub revision: bool,
    pub lines: Vec<CountSaveLinePlan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountTransitionPlan {
    pub next: CountStatus,
    pub posting: bool,
    pub reason: Option<String>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

/// Exact nonnegative decimal with the retained Zod precision. The wire regex
/// admits 1-12 whole digits and up to `scale` fractional digits.
fn decimal(value: &str, scale: usize, code: &'static str) -> Result<i64, BusinessError> {
    let failed = || invalid(code, "Use a bounded count quantity or unit cost.");
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if whole.is_empty()
        || whole.len() > 12
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || parts.next().is_some()
        || fraction.is_some_and(|f| {
            f.is_empty() || f.len() > scale || !f.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return Err(failed());
    }
    let fraction = fraction.unwrap_or_default();
    let whole: i64 = whole.parse().map_err(|_| failed())?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().map_err(|_| failed())?
            * 10_i64.pow((scale - fraction.len()) as u32)
    };
    safe(whole * 10_i64.pow(scale as u32) + fraction)
}

fn positive_version(value: Option<i64>) -> Result<i64, BusinessError> {
    match value {
        None => Ok(0),
        Some(value) if value > 0 => Ok(value),
        Some(_) => Err(conflict(
            "COUNT_VERSION_CHANGED",
            "The physical count changed.",
        )),
    }
}

fn optional_date(value: Option<&str>, code: &'static str) -> Result<Option<String>, BusinessError> {
    match value {
        None | Some("") => Ok(None),
        Some(date) if valid_date(date) => Ok(Some(date.to_owned())),
        Some(_) => Err(invalid(code, "Use a valid ISO count date.")),
    }
}

fn found_lot_id(company_id: &str, product_id: &str, identity: &str) -> String {
    let digest = Sha256::digest(format!("{company_id}:{product_id}:{identity}").as_bytes());
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut suffix = String::with_capacity(32);
    for byte in digest.iter().take(16) {
        suffix.push(HEX[(byte >> 4) as usize] as char);
        suffix.push(HEX[(byte & 0x0f) as usize] as char);
    }
    format!("lot-{suffix}")
}

fn plan_lot(
    company_id: &str,
    product: &CountProduct,
    warehouse_id: &str,
    record_id: &str,
    line_index: usize,
    lot_index: usize,
    current: &[CountLot],
    seen: &mut BTreeSet<String>,
    identities: &mut BTreeSet<String>,
    allocation: &CountAllocationInput,
) -> Result<CountSaveLotPlan, BusinessError> {
    let counted = decimal(
        &allocation.counted_quantity,
        3,
        "INVALID_COUNT_QUANTITY",
    )?;
    let source = match allocation.lot_id.as_deref() {
        Some(id) => Some(
            current
                .iter()
                .find(|lot| lot.id == id)
                .ok_or_else(|| {
                    invalid(
                        "COUNT_LOT_INVALID",
                        "Choose a current lot or serial from this warehouse.",
                    )
                })?,
        ),
        None => None,
    };
    if allocation
        .lot_id
        .as_deref()
        .is_some_and(|id| !seen.insert(id.to_owned()))
    {
        return Err(invalid(
            "DUPLICATE_COUNT_LOT",
            "Count each lot or serial once.",
        ));
    }
    let (lot_code, serial_number) = match product.tracking_mode.as_str() {
        "lot" => (
            source
                .and_then(|lot| lot.lot_code.as_deref())
                .map(|value| js_trim(value).to_owned())
                .or_else(|| {
                    allocation
                        .lot_code
                        .as_deref()
                        .map(|value| js_trim(value).to_owned())
                }),
            None,
        ),
        "serial" => (
            None,
            source
                .and_then(|lot| lot.serial_number.as_deref())
                .map(|value| js_trim(value).to_owned())
                .or_else(|| {
                    allocation
                        .serial_number
                        .as_deref()
                        .map(|value| js_trim(value).to_owned())
                }),
        ),
        _ => {
            return Err(invalid(
                "LOT_ALLOCATION_NOT_ALLOWED",
                "Lot allocations are only valid for tracked products.",
            ));
        }
    };
    let identity = lot_code
        .as_deref()
        .or(serial_number.as_deref())
        .map(js_trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            invalid(
                "TRACKING_IDENTITY_REQUIRED",
                if product.tracking_mode == "serial" {
                    "Enter the serial number found during the count."
                } else {
                    "Enter the lot code found during the count."
                },
            )
        })?
        .to_owned();
    if !identities.insert(identity.to_lowercase()) {
        return Err(invalid(
            "DUPLICATE_TRACKING_IDENTITY",
            "Each lot code or serial number may appear once per product.",
        ));
    }
    if product.tracking_mode == "serial" && ![0, 1_000].contains(&counted) {
        return Err(invalid(
            "SERIAL_QUANTITY_INVALID",
            "Each serial number must be counted as zero or one unit.",
        ));
    }
    let (snapshot_quantity, snapshot_value, snapshot_version) = source
        .map(|lot| {
            (
                lot.quantity_milli,
                lot.value_minor,
                lot.balance_version,
            )
        })
        .unwrap_or((0, 0, 0));
    let new_unit_cost = allocation
        .unit_cost
        .as_deref()
        .map(|value| decimal(value, 2, "VALUATION_REQUIRED"))
        .transpose()?;
    if source.is_none() && counted == 0 {
        return Err(invalid(
            "FOUND_LOT_QUANTITY_REQUIRED",
            "A newly found lot or serial must have a positive counted quantity.",
        ));
    }
    if source.is_none() && new_unit_cost.unwrap_or(0) <= 0 {
        return Err(invalid(
            "VALUATION_REQUIRED",
            "Enter a verified unit cost for a newly found lot or serial.",
        ));
    }
    let counted_value = if counted == 0 {
        0
    } else if source.is_some() {
        scaled_inventory_value(snapshot_value, counted, snapshot_quantity)?
    } else {
        scaled_inventory_value(new_unit_cost.unwrap_or(0), counted, 1_000)?
    };
    if counted > 0 && counted_value == 0 {
        return Err(invalid(
            "ZERO_VALUATION",
            "A positive lot count requires a positive inventory valuation.",
        ));
    }
    let result_lot_id = source
        .map(|lot| lot.id.clone())
        .unwrap_or_else(|| found_lot_id(company_id, &product.id, &identity));
    if result_lot_id.is_empty() || warehouse_id.is_empty() || record_id.is_empty() {
        return Err(invalid(
            "COUNT_LOT_INVALID",
            "Use a current count and warehouse selection.",
        ));
    }
    Ok(CountSaveLotPlan {
        id: format!("{record_id}-{line_index}-lot-{lot_index}"),
        count_line_id: format!("{record_id}-{line_index}"),
        source_lot_id: source.map(|lot| lot.id.clone()),
        result_lot_id,
        tracking_mode: product.tracking_mode.clone(),
        lot_code,
        serial_number,
        manufactured_date: source
            .and_then(|lot| lot.manufactured_date.clone())
            .or(optional_date(allocation.manufactured_date.as_deref(), "COUNT_DATE_INVALID")?),
        expiry_date: source
            .and_then(|lot| lot.expiry_date.clone())
            .or(optional_date(allocation.expiry_date.as_deref(), "COUNT_DATE_INVALID")?),
        snapshot_quantity_milli: snapshot_quantity,
        snapshot_value_minor: snapshot_value,
        snapshot_version,
        counted_quantity_milli: counted,
        counted_value_minor: counted_value,
        unit_cost_minor: if source.is_none() { new_unit_cost } else { None },
    })
}

/// Plan a draft create or revision from current owner facts. `record_id` is the
/// command-derived Count identity; `company_id` is the selected Company.
pub fn plan_count_save(
    source: &CountSource,
    input: &CountSaveInput,
    record_id: &str,
    company_id: &str,
) -> Result<CountSavePlan, BusinessError> {
    if record_id.is_empty() || company_id.is_empty() {
        return Err(invalid("COUNT_INVALID", "Use a current Count command."));
    }
    let reason = js_trim(&input.reason);
    if !(5..=MAX_REASON).contains(&reason.chars().count()) {
        return Err(invalid(
            "REASON_REQUIRED",
            "Record why this physical count is being prepared.",
        ));
    }
    if !valid_date(&input.count_date) {
        return Err(invalid("COUNT_DATE_INVALID", "Use a valid count date."));
    }
    if input.lines.is_empty() || input.lines.len() > MAX_LINES {
        return Err(invalid(
            "COUNT_LINES_REQUIRED",
            "Count between one and fifty products.",
        ));
    }
    let expected = positive_version(input.expected_version)?;
    let revision = source.current.is_some();
    if let Some(current) = &source.current {
        if current.version != expected
            || !matches!(current.status.as_str(), "draft" | "rejected")
        {
            return Err(conflict(
                "COUNT_VERSION_CHANGED",
                "The physical count changed.",
            ));
        }
        if current.warehouse_id != input.warehouse_id {
            return Err(invalid(
                "WAREHOUSE_LOCKED",
                "Create a separate count for another warehouse.",
            ));
        }
    }
    let warehouse = source
        .warehouse
        .as_ref()
        .filter(|warehouse| warehouse.active && warehouse.id == input.warehouse_id)
        .ok_or_else(|| {
            invalid(
                "WAREHOUSE_INVALID",
                "Choose an active warehouse in your scope.",
            )
        })?;
    let mut products = BTreeSet::new();
    if input
        .lines
        .iter()
        .any(|line| !products.insert(line.product_id.as_str()))
    {
        return Err(invalid(
            "DUPLICATE_PRODUCT",
            "Count each product once.",
        ));
    }
    let mut lines = Vec::with_capacity(input.lines.len());
    let mut net_value = 0_i64;
    for (index, item) in input.lines.iter().enumerate() {
        let product = source.products.get(&item.product_id).ok_or_else(|| {
            invalid(
                "PRODUCT_INVALID",
                "Choose an active stock item in this company.",
            )
        })?;
        let quantity = decimal(
            &item.counted_quantity,
            3,
            "INVALID_COUNT_QUANTITY",
        )?;
        let unit_cost = item
            .unit_cost
            .as_deref()
            .map(|value| decimal(value, 2, "VALUATION_REQUIRED"))
            .transpose()?;
        let stock = source.stocks.get(&item.product_id);
        let old_quantity = stock.map(|stock| stock.quantity_milli).unwrap_or(0);
        let old_value = stock.map(|stock| stock.value_minor).unwrap_or(0);
        let (value, lots) = match product.tracking_mode.as_str() {
            "none" => {
                if !item.allocations.is_empty() {
                    return Err(invalid(
                        "LOT_ALLOCATION_NOT_ALLOWED",
                        "Lot allocations are only valid for tracked products.",
                    ));
                }
                if old_quantity == 0 && quantity > 0 && unit_cost.unwrap_or(0) <= 0 {
                    return Err(invalid(
                        "VALUATION_REQUIRED",
                        "Enter a verified unit cost when counting stock without an existing valuation.",
                    ));
                }
                let value = if quantity == 0 {
                    0
                } else if old_quantity > 0 {
                    scaled_inventory_value(old_value, quantity, old_quantity)?
                } else {
                    scaled_inventory_value(unit_cost.unwrap_or(0), quantity, 1_000)?
                };
                (value, Vec::new())
            }
            "lot" | "serial" => {
                if item.allocations.len() > MAX_ALLOCATIONS {
                    return Err(invalid(
                        "COUNT_LOT_INVALID",
                        "Use at most 500 lot allocations.",
                    ));
                }
                let current = source
                    .lots
                    .get(&item.product_id)
                    .cloned()
                    .unwrap_or_default();
                let mut seen = BTreeSet::new();
                let mut identities = BTreeSet::new();
                let mut lots = Vec::with_capacity(item.allocations.len());
                for (lot_index, allocation) in item.allocations.iter().enumerate() {
                    lots.push(plan_lot(
                        company_id,
                        product,
                        &warehouse.id,
                        record_id,
                        index,
                        lot_index,
                        &current,
                        &mut seen,
                        &mut identities,
                        allocation,
                    )?);
                }
                if current.iter().any(|lot| !seen.contains(&lot.id)) {
                    return Err(invalid(
                        "COUNT_LOT_MISSING",
                        "Count every current lot or serial, including zero when it is missing physically.",
                    ));
                }
                let total = lots.iter().try_fold(0_i64, |sum, lot| {
                    safe(sum + lot.counted_quantity_milli)
                })?;
                if total != quantity {
                    return Err(invalid(
                        "COUNT_LOT_TOTAL_MISMATCH",
                        "The lot or serial quantities must equal the product count.",
                    ));
                }
                let value = lots.iter().try_fold(0_i64, |sum, lot| {
                    safe(sum + lot.counted_value_minor)
                })?;
                (value, lots)
            }
            _ => {
                return Err(invalid(
                    "PRODUCT_INVALID",
                    "Choose an active stock item in this company.",
                ));
            }
        };
        if quantity > 0 && value == 0 {
            return Err(invalid(
                "ZERO_VALUATION",
                "A positive count requires a positive inventory valuation.",
            ));
        }
        let snapshot_quantity = old_quantity;
        let snapshot_value = old_value;
        let snapshot_version = stock.map(|stock| stock.version).unwrap_or(0);
        net_value = safe(
            net_value
                + (value - snapshot_value),
        )?;
        lines.push(CountSaveLinePlan {
            id: format!("{record_id}-{index}"),
            product_id: product.id.clone(),
            product_version: product.version,
            description: product.name.clone(),
            unit: product.unit.clone(),
            tracking_mode: product.tracking_mode.clone(),
            snapshot_quantity_milli: snapshot_quantity,
            snapshot_value_minor: snapshot_value,
            snapshot_version,
            counted_quantity_milli: quantity,
            counted_value_minor: value,
            unit_cost_minor: (product.tracking_mode == "none" && old_quantity == 0)
                .then_some(unit_cost)
                .flatten(),
            ordinal: index as i64,
            lots,
        });
    }
    Ok(CountSavePlan {
        record_id: record_id.to_owned(),
        company_id: company_id.to_owned(),
        branch_id: warehouse.branch_id.clone(),
        warehouse_id: warehouse.id.clone(),
        number: if revision {
            source
                .current
                .as_ref()
                .map(|current| current.number.clone())
                .unwrap_or_default()
        } else {
            let digest = Sha256::digest(record_id.as_bytes());
            let mut suffix = String::with_capacity(16);
            for byte in digest.iter().take(8) {
                suffix.push_str(&format!("{byte:02X}"));
            }
            format!("COUNT-{suffix}")
        },
        count_date: input.count_date.clone(),
        reason: reason.to_owned(),
        net_value_minor: net_value,
        version: if revision { expected + 1 } else { 1 },
        revision,
        lines,
    })
}

/// Parse the retained wire action name.
pub fn count_action(value: &str) -> Option<CountAction> {
    match value {
        "submit" => Some(CountAction::Submit),
        "approve" => Some(CountAction::Approve),
        "post" => Some(CountAction::Post),
        "reject" => Some(CountAction::Reject),
        "cancel" => Some(CountAction::Cancel),
        _ => None,
    }
}

pub fn count_status(value: &str) -> Option<CountStatus> {
    match value {
        "draft" => Some(CountStatus::Draft),
        "submitted" => Some(CountStatus::Submitted),
        "approved" => Some(CountStatus::Approved),
        "rejected" => Some(CountStatus::Rejected),
        "cancelled" => Some(CountStatus::Cancelled),
        "posted" => Some(CountStatus::Posted),
        _ => None,
    }
}

/// Admission for submit/review/cancel. Posting effects and Finance journal are
/// planned by the composition boundary; this only proves the transition.
pub fn plan_count_transition(
    status: &str,
    version: i64,
    expected: i64,
    action: CountAction,
    authority: &CountAuthority,
    reason: Option<&str>,
    attachments: usize,
) -> Result<CountTransitionPlan, BusinessError> {
    let status = count_status(status)
        .ok_or_else(|| conflict("COUNT_STATUS_CHANGED", "The Count status is unavailable."))?;
    if !(1..=MAX_SAFE_UNITS).contains(&expected) {
        return Err(conflict(
            "COUNT_VERSION_CHANGED",
            "The physical count changed.",
        ));
    }
    let next = transition_count(status, version, expected, action, authority, reason, attachments)?;
    Ok(CountTransitionPlan {
        next,
        posting: action == CountAction::Post,
        reason: reason.map(|value| js_trim(value).to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(tracking: &str) -> CountProduct {
        CountProduct {
            id: "product".into(),
            name: "Widget".into(),
            unit: "pcs".into(),
            version: 3,
            tracking_mode: tracking.into(),
        }
    }

    fn source(tracking: &str) -> CountSource {
        let mut products = BTreeMap::new();
        products.insert("product".into(), product(tracking));
        let mut stocks = BTreeMap::new();
        stocks.insert(
            "product".into(),
            CountStock {
                quantity_milli: 10_000,
                value_minor: 5_000,
                version: 4,
            },
        );
        let mut lots = BTreeMap::new();
        if tracking != "none" {
            lots.insert(
                "product".into(),
                vec![
                    CountLot {
                        id: "lot-a".into(),
                        product_id: "product".into(),
                        tracking_mode: tracking.into(),
                        lot_code: (tracking == "lot").then(|| "L-A".into()),
                        serial_number: (tracking == "serial").then(|| "SN-A".into()),
                        manufactured_date: None,
                        expiry_date: None,
                        quantity_milli: 10_000,
                        value_minor: 5_000,
                        balance_version: 2,
                    },
                ],
            );
        }
        CountSource {
            current: None,
            warehouse: Some(CountWarehouse {
                id: "warehouse".into(),
                branch_id: "branch".into(),
                version: 2,
                active: true,
            }),
            products,
            stocks,
            lots,
        }
    }

    fn input(tracking: &str) -> CountSaveInput {
        CountSaveInput {
            warehouse_id: "warehouse".into(),
            count_date: "2026-11-10".into(),
            reason: "Monthly physical count".into(),
            expected_version: None,
            lines: vec![CountLineInput {
                product_id: "product".into(),
                counted_quantity: "8.000".into(),
                unit_cost: None,
                allocations: if tracking == "none" {
                    vec![]
                } else {
                    vec![CountAllocationInput {
                        lot_id: Some("lot-a".into()),
                        lot_code: None,
                        serial_number: None,
                        manufactured_date: Some("".into()),
                        expiry_date: Some("".into()),
                        counted_quantity: "8.000".into(),
                        unit_cost: None,
                    }]
                },
            }],
        }
    }

    #[test]
    fn creates_and_revises_a_draft_with_exact_snapshot_values() {
        let plan = plan_count_save(&source("none"), &input("none"), "count-key", "company").unwrap();
        assert!(!plan.revision);
        assert_eq!(plan.version, 1);
        assert_eq!(plan.number, "COUNT-B039874C0806B8E6");
        assert_eq!(plan.net_value_minor, -1_000);
        assert_eq!(plan.lines[0].snapshot_quantity_milli, 10_000);
        assert_eq!(plan.lines[0].counted_value_minor, 4_000);

        let mut revision = source("none");
        revision.current = Some(CountRecord {
            id: "count-key".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            warehouse_id: "warehouse".into(),
            number: "COUNT-1".into(),
            count_date: "2026-11-10".into(),
            reason: "Monthly physical count".into(),
            status: "rejected".into(),
            version: 2,
            net_value_minor: 0,
            created_by: "user".into(),
            prepared_by: "user".into(),
            approved_by: None,
            quality_hold_id: None,
        });
        let mut edit = input("none");
        edit.expected_version = Some(2);
        let plan = plan_count_save(&revision, &edit, "count-key", "company").unwrap();
        assert!(plan.revision);
        assert_eq!(plan.version, 3);
        assert_eq!(plan.number, "COUNT-1");
    }

    #[test]
    fn tracked_counts_require_every_current_lot_and_exact_quantities() {
        let plan = plan_count_save(&source("lot"), &input("lot"), "count-key", "company").unwrap();
        assert_eq!(plan.lines[0].lots.len(), 1);
        assert_eq!(plan.lines[0].lots[0].source_lot_id.as_deref(), Some("lot-a"));
        assert_eq!(plan.lines[0].lots[0].snapshot_version, 2);
        assert_eq!(plan.lines[0].lots[0].counted_value_minor, 4_000);

        let mut missing = input("lot");
        missing.lines[0].allocations.clear();
        assert_eq!(
            plan_count_save(&source("lot"), &missing, "count-key", "company")
                .unwrap_err()
                .code,
            "COUNT_LOT_MISSING"
        );
        let mut mismatch = input("lot");
        mismatch.lines[0].allocations[0].counted_quantity = "7.000".into();
        assert_eq!(
            plan_count_save(&source("lot"), &mismatch, "count-key", "company")
                .unwrap_err()
                .code,
            "COUNT_LOT_TOTAL_MISMATCH"
        );
    }

    #[test]
    fn found_lots_need_a_positive_quantity_and_cost() {
        let mut found = source("lot");
        found.lots.clear();
        let mut input = input("lot");
        input.lines[0].counted_quantity = "1.000".into();
        input.lines[0].allocations = vec![CountAllocationInput {
            lot_id: None,
            lot_code: Some("NEW".into()),
            serial_number: None,
            manufactured_date: None,
            expiry_date: None,
            counted_quantity: "1.000".into(),
            unit_cost: Some("2.50".into()),
        }];
        let plan = plan_count_save(&found, &input, "count-key", "company").unwrap();
        assert!(plan.lines[0].lots[0].result_lot_id.starts_with("lot-"));
        assert_eq!(plan.lines[0].lots[0].counted_value_minor, 250);
        let mut no_cost = input;
        no_cost.lines[0].allocations[0].unit_cost = None;
        assert_eq!(
            plan_count_save(&found, &no_cost, "count-key", "company")
                .unwrap_err()
                .code,
            "VALUATION_REQUIRED"
        );
    }

    #[test]
    fn save_rejects_locked_scopes_and_duplicate_products() {
        let mut revision = source("none");
        revision.current = Some(CountRecord {
            id: "count-key".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            warehouse_id: "other".into(),
            number: "COUNT-2".into(),
            count_date: "2026-11-10".into(),
            reason: "Monthly physical count".into(),
            status: "draft".into(),
            version: 2,
            net_value_minor: 0,
            created_by: "user".into(),
            prepared_by: "user".into(),
            approved_by: None,
            quality_hold_id: None,
        });
        let mut edit = input("none");
        edit.expected_version = Some(2);
        assert_eq!(
            plan_count_save(&revision, &edit, "count-key", "company")
                .unwrap_err()
                .code,
            "WAREHOUSE_LOCKED"
        );
        let mut duplicate = input("none");
        duplicate.lines.push(duplicate.lines[0].clone());
        assert_eq!(
            plan_count_save(&source("none"), &duplicate, "count-key", "company")
                .unwrap_err()
                .code,
            "DUPLICATE_PRODUCT"
        );
    }

    fn authority(same_creator: bool, can_prepare: bool, can_approve: bool) -> CountAuthority {
        CountAuthority {
            same_as_creator: same_creator,
            same_as_preparer: same_creator,
            company_scope: true,
            department_scope: false,
            can_prepare,
            can_approve,
        }
    }

    #[test]
    fn transitions_keep_preparation_and_review_independent() {
        let plan = plan_count_transition(
            "draft",
            1,
            1,
            CountAction::Submit,
            &authority(true, true, false),
            None,
            0,
        )
        .unwrap();
        assert_eq!(plan.next, CountStatus::Submitted);
        assert!(!plan.posting);
        assert_eq!(
            plan_count_transition(
                "submitted",
                2,
                2,
                CountAction::Approve,
                &authority(true, false, true),
                Some("Approved"),
                0,
            )
            .unwrap_err()
            .code,
            "COUNT_ACTION_FORBIDDEN"
        );
        let plan = plan_count_transition(
            "submitted",
            2,
            2,
            CountAction::Approve,
            &authority(false, false, true),
            Some("Approved"),
            0,
        )
        .unwrap();
        assert_eq!(plan.next, CountStatus::Approved);
        assert_eq!(
            plan_count_transition(
                "approved",
                3,
                3,
                CountAction::Post,
                &authority(false, false, true),
                Some("Posted"),
                0,
            )
            .unwrap()
            .posting,
            true
        );
    }
}
