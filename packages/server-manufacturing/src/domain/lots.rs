//! Native Manufacturing production-lot posting policy.
//!
//! Pure decisions and derived posting identity for the retained material issue
//! and finished-output lot slices of a production order. The caller owns the
//! serializable Holding transaction, the current assignment and the command
//! receipt; this module never reads or writes storage.
//!
//! Parity source: the retained TypeScript lot application
//! (`packages/server-manufacturing/src/application/lots.ts`) and lot policy
//! (`packages/server-manufacturing/src/domain/lot-policy.ts`).
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const MATERIAL_LOT_UNAVAILABLE: &str = "A selected material lot is changed, expired, quarantined or unavailable.";
pub const LOT_ALLOCATION_REQUIRED: &str = "Allocate the entire issued material quantity to its available lots or serial numbers.";
pub const LOT_ALLOCATION_UNEXPECTED: &str = "This manufacturing material does not use lot tracking.";
pub const LOT_ALLOCATION_DUPLICATE: &str = "Use each material lot or serial once in this run.";
pub const SERIAL_QUANTITY_INVALID: &str = "Issue each material serial as exactly one unit.";
pub const OUTPUT_LOTS_REQUIRED: &str = "Allocate the accepted output quantity to new production lots or serial numbers.";
pub const OUTPUT_IDENTITY_INVALID: &str = "Use a unique lot code or serial number for each output allocation.";
pub const OUTPUT_EXPIRY_INVALID: &str = "New output cannot already be expired.";
pub const OUTPUT_SERIAL_QUANTITY_INVALID: &str = "Each output serial must identify one unit.";

pub fn lot_unavailable() -> BusinessError {
    BusinessError::new(FailureKind::Conflict, "MANUFACTURING_LOT_UNAVAILABLE", MATERIAL_LOT_UNAVAILABLE)
}
pub fn allocation_required() -> BusinessError {
    BusinessError::invalid("MANUFACTURING_LOT_ALLOCATION_REQUIRED", LOT_ALLOCATION_REQUIRED)
}
pub fn allocation_unexpected() -> BusinessError {
    BusinessError::invalid("LOT_ALLOCATION_UNEXPECTED", LOT_ALLOCATION_UNEXPECTED)
}
pub fn allocation_duplicate() -> BusinessError {
    BusinessError::invalid("LOT_ALLOCATION_DUPLICATE", LOT_ALLOCATION_DUPLICATE)
}
pub fn serial_quantity_invalid() -> BusinessError {
    BusinessError::invalid("SERIAL_QUANTITY_INVALID", SERIAL_QUANTITY_INVALID)
}
pub fn output_lots_required() -> BusinessError {
    BusinessError::invalid("MANUFACTURING_OUTPUT_LOTS_REQUIRED", OUTPUT_LOTS_REQUIRED)
}
pub fn output_identity_invalid() -> BusinessError {
    BusinessError::invalid("MANUFACTURING_OUTPUT_IDENTITY", OUTPUT_IDENTITY_INVALID)
}
pub fn output_expiry_invalid() -> BusinessError {
    BusinessError::invalid("MANUFACTURING_OUTPUT_EXPIRY", OUTPUT_EXPIRY_INVALID)
}
pub fn output_serial_quantity_invalid() -> BusinessError {
    BusinessError::invalid("SERIAL_QUANTITY_INVALID", OUTPUT_SERIAL_QUANTITY_INVALID)
}

/// Round-half-up proportional amount, matching the retained Commerce helper.
pub fn scaled_amount(amount: i64, numerator: i64, denominator: i64) -> Option<i64> {
    if denominator <= 0 {
        return None;
    }
    let value = (i128::from(amount) * i128::from(numerator) + i128::from(denominator / 2))
        / i128::from(denominator);
    i64::try_from(value).ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackingMode {
    None,
    Lot,
    Serial,
}

impl TrackingMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "lot" => Some(Self::Lot),
            "serial" => Some(Self::Serial),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Lot => "lot",
            Self::Serial => "serial",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialAllocation {
    pub product_id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub balance_version: i64,
}

/// Current Inventory balance facts for one selected lot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialLotFacts {
    pub status: String,
    pub held: bool,
    pub balance_version: i64,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub expiry_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialLotIssue {
    pub ordinal: usize,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    /// Balance version the caller selected; the atomic compare-and-set uses it.
    pub balance_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialLotPlan {
    pub issues: Vec<MaterialLotIssue>,
    pub value_minor: i64,
}

/// Validate the issue allocation and derive one lot issue per selected lot.
///
/// `facts` is aligned with `allocations`; the adapter reads it under the same
/// serializable transaction before this decision.
/// Validate allocation shape before any Inventory read; untracked materials
/// must not carry allocations.
pub fn validate_material_allocations(
    tracking_mode: TrackingMode,
    required_milli: i64,
    allocations: &[MaterialAllocation],
) -> Result<(), BusinessError> {
    if tracking_mode == TrackingMode::None {
        return if allocations.is_empty() {
            Ok(())
        } else {
            Err(allocation_unexpected())
        };
    }
    if allocations.is_empty()
        || allocations
            .iter()
            .try_fold(0_i64, |sum, allocation| sum.checked_add(allocation.quantity_milli))
            != Some(required_milli)
    {
        return Err(allocation_required());
    }
    if allocations.iter().enumerate().any(|(index, allocation)| {
        allocations[..index]
            .iter()
            .any(|prior| prior.lot_id == allocation.lot_id)
    }) {
        return Err(allocation_duplicate());
    }
    Ok(())
}

pub fn plan_material_lots(
    tracking_mode: TrackingMode,
    required_milli: i64,
    entry_date: &str,
    allocations: &[MaterialAllocation],
    facts: &[Option<MaterialLotFacts>],
) -> Result<Option<MaterialLotPlan>, BusinessError> {
    validate_material_allocations(tracking_mode, required_milli, allocations)?;
    if tracking_mode == TrackingMode::None {
        return Ok(None);
    }
    let mut value_minor = 0_i64;
    let mut issues = Vec::with_capacity(allocations.len());
    for (ordinal, allocation) in allocations.iter().enumerate() {
        let facts = facts.get(ordinal).and_then(Option::as_ref);
        let available = facts.filter(|facts| {
            facts.status == "active"
                && !facts.held
                && facts.balance_version == allocation.balance_version
                && facts.quantity_milli >= allocation.quantity_milli
                && facts.expiry_date.as_deref().is_none_or(|expiry| expiry >= entry_date)
        });
        let facts = match available {
            Some(facts) => facts,
            None => return Err(lot_unavailable()),
        };
        if tracking_mode == TrackingMode::Serial && allocation.quantity_milli != 1_000 {
            return Err(serial_quantity_invalid());
        }
        let value = if allocation.quantity_milli == facts.quantity_milli {
            facts.value_minor
        } else {
            scaled_amount(facts.value_minor, allocation.quantity_milli, facts.quantity_milli)
                .ok_or_else(lot_unavailable)?
        };
        value_minor = value_minor.checked_add(value).ok_or_else(lot_unavailable)?;
        issues.push(MaterialLotIssue {
            ordinal,
            lot_id: allocation.lot_id.clone(),
            quantity_milli: allocation.quantity_milli,
            value_minor: value,
            balance_version: allocation.balance_version,
        });
    }
    Ok(Some(MaterialLotPlan {
        issues,
        value_minor,
    }))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputAllocation {
    pub quantity_milli: i64,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub expiry_date: Option<String>,
}

/// Parse one wire material allocation; quantities use the retained
/// `<integer>[.<1-3 digits>]` scale.
pub fn parse_material_allocation(
    product_id: &str,
    lot_id: &str,
    quantity: &str,
    balance_version: i64,
) -> Option<MaterialAllocation> {
    let quantity_milli = crate::domain::specification::parse_quantity_milli(quantity)?;
    (quantity_milli > 0 && balance_version >= 1 && !product_id.is_empty() && !lot_id.is_empty())
        .then(|| MaterialAllocation {
            product_id: product_id.to_owned(),
            lot_id: lot_id.to_owned(),
            quantity_milli,
            balance_version,
        })
}

/// Parse one wire output allocation.
pub fn parse_output_allocation(
    quantity: &str,
    lot_code: Option<&str>,
    serial_number: Option<&str>,
    expiry_date: Option<&str>,
) -> Option<OutputAllocation> {
    let quantity_milli = crate::domain::specification::parse_quantity_milli(quantity)?;
    (quantity_milli > 0).then(|| OutputAllocation {
        quantity_milli,
        lot_code: lot_code.map(str::to_owned),
        serial_number: serial_number.map(str::to_owned),
        expiry_date: expiry_date.map(str::to_owned),
    })
}

/// Persisted material issue read back for output genealogy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialIssueRow {
    pub id: String,
    pub quantity_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputLot {
    pub ordinal: usize,
    pub lot_id: String,
    pub output_id: String,
    pub move_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub expiry_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Genealogy {
    pub id: String,
    pub issue_id: String,
    pub output_id: String,
    pub input_quantity_milli: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputLotPlan {
    pub lots: Vec<OutputLot>,
    pub genealogy: Vec<Genealogy>,
}

pub struct OutputLotInput<'a> {
    pub key: &'a str,
    pub order_id: &'a str,
    pub order_version: i64,
    pub tracking_mode: TrackingMode,
    pub output_milli: i64,
    pub total_minor: i64,
    pub entry_date: &'a str,
}

/// Validate accepted output allocations and derive the new lot, balance,
/// evidence, movement and genealogy rows.
pub fn plan_output_lots(
    input: &OutputLotInput<'_>,
    allocations: &[OutputAllocation],
    issues: &[MaterialIssueRow],
) -> Result<Option<OutputLotPlan>, BusinessError> {
    let total_allocated = allocations
        .iter()
        .try_fold(0_i64, |sum, allocation| sum.checked_add(allocation.quantity_milli));
    if input.tracking_mode == TrackingMode::None {
        return if allocations.is_empty() {
            Ok(None)
        } else {
            Err(allocation_unexpected())
        };
    }
    if allocations.is_empty() || total_allocated != Some(input.output_milli) {
        return Err(output_lots_required());
    }
    let mut identities = Vec::with_capacity(allocations.len());
    for allocation in allocations {
        let identity = if input.tracking_mode == TrackingMode::Serial {
            allocation.serial_number.as_deref()
        } else {
            allocation.lot_code.as_deref()
        };
        match identity {
            Some(identity)
                if !identity.is_empty()
                    && !identities.iter().any(|prior| *prior == identity) =>
            {
                identities.push(identity);
            }
            _ => return Err(output_identity_invalid()),
        }
    }
    let mut lots = Vec::with_capacity(allocations.len());
    let mut genealogy = Vec::new();
    let mut prior_input: Vec<(String, i64)> = Vec::new();
    let mut cumulative_quantity = 0_i64;
    let mut cumulative_value = 0_i64;
    for (ordinal, allocation) in allocations.iter().enumerate() {
        if input.tracking_mode == TrackingMode::Serial && allocation.quantity_milli != 1_000 {
            return Err(output_serial_quantity_invalid());
        }
        if allocation
            .expiry_date
            .as_deref()
            .is_some_and(|expiry| expiry < input.entry_date)
        {
            return Err(output_expiry_invalid());
        }
        cumulative_quantity = cumulative_quantity
            .checked_add(allocation.quantity_milli)
            .ok_or_else(output_lots_required)?;
        let cumulative = if ordinal + 1 == allocations.len() {
            input.total_minor
        } else {
            scaled_amount(input.total_minor, cumulative_quantity, input.output_milli)
                .ok_or_else(output_lots_required)?
        };
        let value = cumulative - cumulative_value;
        cumulative_value = cumulative;
        let lot_id = format!("mfg-output-lot-{}-{ordinal}", input.key);
        let output_id = format!("mfg-output-{}-{ordinal}", input.key);
        lots.push(OutputLot {
            ordinal,
            move_id: format!("mfg-output-move-{}-{ordinal}", input.key),
            lot_id,
            output_id: output_id.clone(),
            quantity_milli: allocation.quantity_milli,
            value_minor: value,
            lot_code: allocation.lot_code.clone(),
            serial_number: allocation.serial_number.clone(),
            expiry_date: allocation.expiry_date.clone(),
        });
        for (issue_index, issue) in issues.iter().enumerate() {
            let cumulative_input = if ordinal + 1 == allocations.len() {
                issue.quantity_milli
            } else {
                scaled_amount(issue.quantity_milli, cumulative_quantity, input.output_milli)
                    .ok_or_else(output_lots_required)?
            };
            let prior = prior_input
                .iter()
                .find(|(id, _)| id == &issue.id)
                .map(|(_, value)| *value)
                .unwrap_or(0);
            if let Some(entry) = prior_input.iter_mut().find(|(id, _)| id == &issue.id) {
                entry.1 = cumulative_input;
            } else {
                prior_input.push((issue.id.clone(), cumulative_input));
            }
            genealogy.push(Genealogy {
                id: format!("mfg-lot-genealogy-{}-{ordinal}-{issue_index}", input.key),
                issue_id: issue.id.clone(),
                output_id: output_id.clone(),
                input_quantity_milli: cumulative_input - prior,
            });
        }
    }
    Ok(Some(OutputLotPlan { lots, genealogy }))
}

/// Stable IDs for the retained material issue evidence and Inventory movement.
pub struct MaterialIds {
    pub issue_id: String,
    pub move_id: String,
    pub move_source_id: String,
}

pub fn material_ids(key: &str, order_id: &str, order_version: i64, product_id: &str, ordinal: usize) -> MaterialIds {
    MaterialIds {
        issue_id: format!("mfg-input-lot-{key}-{product_id}-{ordinal}"),
        move_id: format!("mfg-input-move-{key}-{product_id}-{ordinal}"),
        move_source_id: format!("{order_id}:v{order_version}:{product_id}"),
    }
}
