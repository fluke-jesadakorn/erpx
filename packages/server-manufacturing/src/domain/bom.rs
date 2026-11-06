//! Native Manufacturing BOM/formula register facts.
//!
//! The register is a bounded owner read projection over the manufacturing
//! specification, component-line and routing-operation relations. It does not
//! create, review or freeze a specification. Current IAM authority, the
//! Holding transaction and the selected Company scope are supplied by the
//! composition; this module enforces the read policy and every source fact
//! before presentation.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;
use std::collections::BTreeSet;

pub const MAX_SPECIFICATIONS: usize = 200;
pub const MAX_LINES_PER_SPECIFICATION: usize = 100;
pub const MAX_OPERATIONS_PER_SPECIFICATION: usize = 50;
pub const MAX_TOTAL_LINES: usize = 20_000;
pub const MAX_TOTAL_OPERATIONS: usize = 10_000;
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BomActor {
    pub user_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl BomActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    fn has_module(&self) -> bool {
        self.modules.iter().any(|value| value == "manufacturing")
    }

    pub fn can_read(&self) -> bool {
        self.has("records.read") && self.has_module()
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("records.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MANUFACTURING_BOM_PERMISSION_REQUIRED",
                "Your selected role does not grant Manufacturing read access.",
            ));
        }
        if !self.has_module() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "MANUFACTURING_BOM_MODULE_FORBIDDEN",
                "Your current role cannot access Manufacturing.",
            ));
        }
        if self.expires_at <= 0 || now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }
}

pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_BOM_SOURCE_INVALID",
        "The current manufacturing bill of materials source is invalid.",
    )
}

pub(crate) fn limit_exceeded() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_BOM_LIMIT",
        "This manufacturing bill of materials register exceeds its bounded read.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BomLine {
    pub id: String,
    pub specification_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub substitution_group: Option<String>,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BomOperation {
    pub id: String,
    pub specification_id: String,
    pub resource_id: String,
    pub resource_name: String,
    pub name: String,
    pub setup_minutes: i64,
    pub run_minutes_per_batch: i64,
    pub ordinal: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BomSpecification {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub mode: String,
    pub output_product_id: String,
    pub batch_quantity_milli: i64,
    pub expected_yield_bps: i64,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub lines: Vec<BomLine>,
    pub operations: Vec<BomOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BomRegister {
    pub specifications: Vec<BomSpecification>,
}

fn identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

/// ECMAScript `String.prototype.trim` edge whitespace: White_Space plus
/// Line_Terminator, including U+FEFF. Rust `str::trim` additionally strips
/// U+0085 (White_Space in Unicode but not trimmed by the TypeScript
/// `text` validator) and keeps U+FEFF, so the exact set is spelled out.
fn js_trim(value: &str) -> &str {
    value.trim_matches(|character| {
        matches!(character,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}')
    })
}

fn text(value: &str, minimum: usize, maximum: usize) -> bool {
    // TypeScript measures UTF-16 code units (`String.length`), not UTF-8
    // bytes or code points: a non-BMP character counts 2.
    let length = value.encode_utf16().count();
    // `char::is_control` is intentionally the full superset (including C1
    // controls) that the TypeScript transport permits; this native read
    // fails closed there instead of accepting it.
    length >= minimum
        && length <= maximum
        && js_trim(value) == value
        && !value.chars().any(char::is_control)
}

fn nonnegative(value: i64) -> bool {
    (0..=MAX_SAFE_INTEGER).contains(&value)
}

fn positive(value: i64) -> bool {
    (1..=MAX_SAFE_INTEGER).contains(&value)
}

fn valid_mode(value: &str) -> bool {
    matches!(value, "bom" | "formula")
}

fn valid_status(value: &str) -> bool {
    matches!(value, "draft" | "approved" | "rejected" | "retired")
}

fn valid_line(line: &BomLine, specification: &BomSpecification) -> bool {
    identifier(&line.id)
        && line.specification_id == specification.id
        && identifier(&line.product_id)
        && line.product_id != specification.output_product_id
        && positive(line.quantity_milli)
        && nonnegative(line.ordinal)
        && line
            .substitution_group
            .as_deref()
            .is_none_or(|value| text(value, 1, 80))
}

fn valid_operation(operation: &BomOperation, specification: &BomSpecification) -> bool {
    identifier(&operation.id)
        && operation.specification_id == specification.id
        && identifier(&operation.resource_id)
        && text(&operation.resource_name, 2, 160)
        && text(&operation.name, 2, 160)
        && nonnegative(operation.setup_minutes)
        && positive(operation.run_minutes_per_batch)
        && nonnegative(operation.ordinal)
}

fn valid_specification(specification: &BomSpecification, company: &CompanyId) -> bool {
    identifier(&specification.id)
        && specification.company_id == company.as_str()
        && text(&specification.name, 2, 160)
        && valid_mode(&specification.mode)
        && identifier(&specification.output_product_id)
        && positive(specification.batch_quantity_milli)
        && (1..=20_000).contains(&specification.expected_yield_bps)
        && valid_status(&specification.status)
        && identifier(&specification.created_by)
        && specification.reviewed_by.as_deref().is_none_or(|value| {
            identifier(value)
                && match specification.status.as_str() {
                    "draft" => false,
                    _ => value != specification.created_by,
                }
        })
        && match specification.status.as_str() {
            "draft" => specification.reviewed_by.is_none(),
            _ => specification.reviewed_by.is_some(),
        }
        && (1..=MAX_SAFE_INTEGER).contains(&specification.version)
        && nonnegative(specification.created_at)
        && nonnegative(specification.updated_at)
        && specification.updated_at >= specification.created_at
        && (1..=MAX_LINES_PER_SPECIFICATION).contains(&specification.lines.len())
        && specification.operations.len() <= MAX_OPERATIONS_PER_SPECIFICATION
}

pub fn validate_register(
    actor: &BomActor,
    specifications: Vec<BomSpecification>,
) -> Result<BomRegister, BusinessError> {
    if specifications.len() > MAX_SPECIFICATIONS {
        return Err(limit_exceeded());
    }
    let mut specification_ids = BTreeSet::new();
    let mut line_ids = BTreeSet::new();
    let mut operation_ids = BTreeSet::new();
    let mut total_lines = 0usize;
    let mut total_operations = 0usize;
    for specification in &specifications {
        if specification.lines.len() > MAX_LINES_PER_SPECIFICATION
            || specification.operations.len() > MAX_OPERATIONS_PER_SPECIFICATION
        {
            return Err(limit_exceeded());
        }
        if !valid_specification(specification, &actor.company_id)
            || !specification_ids.insert(specification.id.clone())
        {
            return Err(source_invalid());
        }
        total_lines += specification.lines.len();
        total_operations += specification.operations.len();
        if total_lines > MAX_TOTAL_LINES || total_operations > MAX_TOTAL_OPERATIONS {
            return Err(limit_exceeded());
        }
        let mut products = BTreeSet::new();
        let mut line_ordinals = BTreeSet::new();
        for line in &specification.lines {
            if !valid_line(line, specification)
                || !line_ids.insert(line.id.clone())
                || !products.insert(line.product_id.clone())
                || !line_ordinals.insert(line.ordinal)
            {
                return Err(source_invalid());
            }
        }
        let mut operation_ordinals = BTreeSet::new();
        for operation in &specification.operations {
            if !valid_operation(operation, specification)
                || !operation_ids.insert(operation.id.clone())
                || !operation_ordinals.insert(operation.ordinal)
            {
                return Err(source_invalid());
            }
        }
    }
    Ok(BomRegister { specifications })
}
