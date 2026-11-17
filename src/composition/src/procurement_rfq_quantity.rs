//! Unbounded JavaScript-compatible quantity preparation for the Records
//! requestQuotationSource item map. This is not a full source or authority.
use erpx_server_procurement::public::rfq_request_allocations::RfqRequestAllocation;
use erpx_server_workflow::public::source_ordered_json::{
    SourceOrderedJson, SourceOrderedObject, SourceOrderedObjectValue,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RfqQuantityError {
    JsonSyntax,
    ItemsNull,
    ItemsNotArray,
    ItemNull,
    QuantityMissing,
    QuantityNull,
    QuantityNotString,
    DecimalInvalid,
    DecimalPrecision,
    DecimalRange,
    QuantityNonpositive,
}

fn whitespace(c: u16) -> bool {
    matches!(c,0x09..=0x0d|0x20|0xa0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}
fn decimal_quantity(raw: &[u16]) -> Result<i64, RfqQuantityError> {
    use RfqQuantityError::*;
    let start = raw
        .iter()
        .position(|&c| !whitespace(c))
        .unwrap_or(raw.len());
    let end = raw
        .iter()
        .rposition(|&c| !whitespace(c))
        .map(|n| n + 1)
        .unwrap_or(start);
    let raw = &raw[start..end];
    let negative = raw.first() == Some(&(b'-' as u16));
    let digits = if negative { &raw[1..] } else { raw };
    let dot = digits
        .iter()
        .position(|&c| c == b'.' as u16)
        .unwrap_or(digits.len());
    let whole = &digits[..dot];
    let fraction = if dot < digits.len() {
        &digits[dot + 1..]
    } else {
        &[]
    };
    let digit = |c: &u16| (b'0' as u16..=b'9' as u16).contains(c);
    if whole.is_empty()
        || !whole.iter().all(digit)
        || (dot < digits.len() && (fraction.is_empty() || !fraction.iter().all(digit)))
    {
        return Err(DecimalInvalid);
    }
    if fraction.iter().skip(3).any(|&v| v != b'0' as u16) {
        return Err(DecimalPrecision);
    }
    const MAX: u64 = 9_007_199_254_740_991;
    let mut major = 0_u64;
    for &value in whole {
        major = major
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(value - b'0' as u16)))
            .filter(|&n| n <= MAX)
            .unwrap_or(MAX + 1);
    }
    let mut minor_fraction = 0_u64;
    for index in 0..3 {
        minor_fraction = minor_fraction * 10
            + fraction
                .get(index)
                .map(|&x| u64::from(x - b'0' as u16))
                .unwrap_or(0);
    }
    let minor = major
        .checked_mul(1000)
        .and_then(|n| n.checked_add(minor_fraction))
        .filter(|&n| n <= MAX)
        .ok_or(DecimalRange)?;
    if negative || minor == 0 {
        return Err(QuantityNonpositive);
    }
    Ok(minor as i64)
}

/// Call at the original `detail.items.map` point, after all earlier owner
/// reads, access checks and allocation SQL have completed. Errors preserve
/// the distinction needed by the eventual JavaScript-compatible HTTP bridge.
pub(crate) fn rfq_quotation_quantities(items_json: &str) -> Result<Vec<i64>, RfqQuantityError> {
    use RfqQuantityError::*;
    let data = SourceOrderedJson::parse(items_json).map_err(|_| JsonSyntax)?;
    let root = data.root();
    if root.kind() == "null" {
        return Err(ItemsNull);
    }
    let len = root.array_len().ok_or(ItemsNotArray)?;
    let mut result = Vec::new();
    for index in 0..len {
        let item = root.element(index).expect("array index");
        if item.kind() == "null" {
            return Err(ItemNull);
        }
        let value = item.property("quantity").ok_or(QuantityMissing)?;
        if value.kind() == "null" {
            return Err(QuantityNull);
        }
        let text = value.utf16().ok_or(QuantityNotString)?;
        result.push(decimal_quantity(text)?);
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RfqQuantityAllocation {
    pub requested_milli: i64,
    pub allocated_milli: f64,
    pub remaining_milli: f64,
}
/// Apply the source's Number(ordinal)/Number(sum) and Map overwrite semantics.
/// Allocation rows must be the exact result of Procurement's original query;
/// PostgreSQL NUMERIC text has no NaN because it is a sum of bigint values.
pub(crate) fn rfq_quotation_allocation_amounts(
    quantities: &[i64],
    rows: &[RfqRequestAllocation],
) -> Result<Vec<RfqQuantityAllocation>, std::num::ParseFloatError> {
    let mut allocated = vec![0.0_f64; quantities.len()];
    for row in rows {
        let index = match row.ordinal {
            None => Some(0),
            Some(n) => usize::try_from(n).ok(),
        };
        let value: f64 = row.allocated_milli_decimal.parse()?;
        if let Some(slot) = index.and_then(|index| allocated.get_mut(index)) {
            *slot = value;
        }
    }
    Ok(quantities
        .iter()
        .zip(allocated)
        .map(
            |(&requested_milli, allocated_milli)| RfqQuantityAllocation {
                requested_milli,
                allocated_milli,
                remaining_milli: (requested_milli as f64 - allocated_milli).max(0.0),
            },
        )
        .collect())
}

#[derive(Debug)]
pub(crate) enum RfqItemMapError {
    Quantity(RfqQuantityError),
    AllocationNumber,
    DatabaseNumberOutOfRange,
    CodecInvariant,
}
pub(crate) struct RfqMappedItems {
    /// Complete JSON payload in JavaScript object property order, not a
    /// selected-fields projection. JSON.stringify number semantics apply.
    pub json: String,
    pub fully_allocated: bool,
    units: Vec<RfqEffectiveUnit>,
}
/// Keeps the JavaScript type/value needed for a strict string comparison
/// before JSON.stringify turns non-finite numbers into null. Object/array
/// contents remain in the original source and cannot equal a string.
#[derive(Debug, PartialEq)]
pub(crate) enum RfqEffectiveUnit {
    String(Vec<u16>),
    Number(f64),
    Boolean(bool),
    Array,
    Object,
}
impl RfqMappedItems {
    pub(crate) fn equivalent(&self, other: &Self) -> bool {
        self.json == other.json
            && self.fully_allocated == other.fully_allocated
            && self.units == other.units
    }
    pub(crate) fn effective_unit(&self, index: usize) -> Option<&RfqEffectiveUnit> {
        self.units.get(index)
    }
    /// Only JS strict string equality. The caller must preserve the source's
    /// separate truthiness short circuit for an omitted/empty requestUnit.
    pub(crate) fn unit_strictly_equals(&self, index: usize, expected: &[u16]) -> bool {
        matches!(self.units.get(index), Some(RfqEffectiveUnit::String(value)) if value == expected)
    }
}

/// Preserve the complete own-field spread, including UTF-16 property keys,
/// then apply unit and allocation overrides in the original insertion slots.
/// This does not perform access, scope, source fencing or a route cutover.
pub(crate) fn rfq_quotation_items_json(
    items_json: &str,
    rows: &[RfqRequestAllocation],
) -> Result<RfqMappedItems, RfqItemMapError> {
    use SourceOrderedObjectValue::{Borrowed, Number, Utf16};
    // NUMERIC is decoded by node-postgres safeNumber before any item map or
    // quantity evaluation, including rows whose ordinal has no matching item.
    for row in rows {
        let number: f64 = row.allocated_milli_decimal.parse().map_err(|_| RfqItemMapError::AllocationNumber)?;
        if !number.is_finite() || (number.fract() == 0.0 && number.abs() > 9_007_199_254_740_991.0) {
            return Err(RfqItemMapError::DatabaseNumberOutOfRange);
        }
    }
    let quantities = rfq_quotation_quantities(items_json).map_err(RfqItemMapError::Quantity)?;
    let amounts = rfq_quotation_allocation_amounts(&quantities, rows)
        .map_err(|_| RfqItemMapError::AllocationNumber)?;
    let input =
        SourceOrderedJson::parse(items_json).map_err(|_| RfqItemMapError::CodecInvariant)?;
    let root = input.root();
    let mut json = String::from("[");
    let mut units = Vec::new();
    let keys: Vec<Vec<u16>> = ["unit", "requestedMilli", "allocatedMilli", "remainingMilli"]
        .into_iter()
        .map(|v| v.encode_utf16().collect())
        .collect();
    for (index, amount) in amounts.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        let item = root.element(index).ok_or(RfqItemMapError::CodecInvariant)?;
        let unit = item.property("unit");
        units.push(match unit.filter(|value| value.truthy()) {
            None => RfqEffectiveUnit::String("each".encode_utf16().collect()),
            Some(value) => match value.kind() {
                "string" => RfqEffectiveUnit::String(value.utf16().expect("string kind").to_vec()),
                "number" => RfqEffectiveUnit::Number(value.number().expect("number kind")),
                "boolean" => RfqEffectiveUnit::Boolean(value.boolean().expect("boolean kind")),
                "array" => RfqEffectiveUnit::Array,
                "object" => RfqEffectiveUnit::Object,
                _ => return Err(RfqItemMapError::CodecInvariant),
            },
        });
        let fields = item
            .object_fields()
            .ok_or(RfqItemMapError::CodecInvariant)?;
        let mut output = SourceOrderedObject::new();
        let mut seen = [false; 4];
        for (key, value) in fields {
            let replacement = if key == keys[0] {
                seen[0] = true;
                if value.truthy() {
                    Borrowed(value)
                } else {
                    Utf16("each".encode_utf16().collect())
                }
            } else if key == keys[1] {
                seen[1] = true;
                Number(amount.requested_milli as f64)
            } else if key == keys[2] {
                seen[2] = true;
                Number(amount.allocated_milli)
            } else if key == keys[3] {
                seen[3] = true;
                Number(amount.remaining_milli)
            } else {
                Borrowed(value)
            };
            output
                .push_utf16(key.to_vec(), replacement)
                .map_err(|_| RfqItemMapError::CodecInvariant)?;
        }
        if !seen[0] {
            output
                .push("unit", Utf16("each".encode_utf16().collect()))
                .map_err(|_| RfqItemMapError::CodecInvariant)?;
        }
        if !seen[1] {
            output
                .push("requestedMilli", Number(amount.requested_milli as f64))
                .map_err(|_| RfqItemMapError::CodecInvariant)?;
        }
        if !seen[2] {
            output
                .push("allocatedMilli", Number(amount.allocated_milli))
                .map_err(|_| RfqItemMapError::CodecInvariant)?;
        }
        if !seen[3] {
            output
                .push("remainingMilli", Number(amount.remaining_milli))
                .map_err(|_| RfqItemMapError::CodecInvariant)?;
        }
        json.push_str(&output.stringify());
    }
    json.push(']');
    Ok(RfqMappedItems {
        json,
        fully_allocated: amounts.iter().all(|a| a.remaining_milli == 0.0),
        units,
    })
}
