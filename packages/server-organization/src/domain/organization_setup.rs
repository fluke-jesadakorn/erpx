//! Organization setup commands: the six staff levels, saved organization
//! templates and reviewed template application.
//!
//! These rules are the current retained TypeScript rules
//! (`positionSchema`, `templatePositionSchema`, `organizationTemplateSchema`,
//! `applyOrganizationTemplateSchema` and `validateTemplate`). The module
//! performs no I/O; adapters supply current sources.
use erpx_server_shared::public::BusinessError;
use serde::Serialize;
use serde_json::Value;

/// Retained `staffLevelTitles` order.
pub const STAFF_LEVEL_TITLES: [&str; 6] = [
    "CEO",
    "Executive / Director",
    "Branch / Department Manager",
    "Supervisor",
    "Senior Staff",
    "Staff",
];

pub const MAX_TEMPLATE_POSITIONS: usize = 100;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn js_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| {
            let value = number.as_f64()?;
            (value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER as f64)
                .then_some(value as i64)
        }),
        _ => None,
    }
}

fn string<'a>(value: &'a Value, message: &'static str) -> Result<&'a str, BusinessError> {
    value
        .as_str()
        .ok_or_else(|| invalid("VALIDATION_FAILED", message))
}

fn trimmed(
    value: Option<&Value>,
    minimum: usize,
    maximum: usize,
    message: &'static str,
) -> Result<String, BusinessError> {
    let value = value.ok_or_else(|| invalid("VALIDATION_FAILED", message))?;
    let value = string(value, message)?.trim();
    let length = js_len(value);
    if length < minimum || length > maximum {
        return Err(invalid("VALIDATION_FAILED", message));
    }
    Ok(value.to_owned())
}

fn optional_text(
    value: Option<&Value>,
    maximum: usize,
    message: &'static str,
) -> Result<Option<String>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            if text.is_empty() || js_len(text) > maximum {
                return Err(invalid("VALIDATION_FAILED", message));
            }
            Ok(Some(text.clone()))
        }
        Some(_) => Err(invalid("VALIDATION_FAILED", message)),
    }
}

/// One saved template position. `key` and `reportsToKey` are template-local
/// names, never database identifiers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePosition {
    pub key: String,
    pub title: String,
    pub level_number: i64,
    pub reports_to_key: Option<String>,
    pub is_ceo: bool,
    pub roster_required: bool,
}

impl TemplatePosition {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide a complete template position."))?;
        let key = object
            .get("key")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && js_len(value) <= 100)
            .ok_or_else(|| {
                invalid(
                    "VALIDATION_FAILED",
                    "Provide a template position key of at most 100 characters.",
                )
            })?
            .to_owned();
        let title = trimmed(
            object.get("title"),
            2,
            160,
            "Provide a position title of 2 to 160 characters.",
        )?;
        let level_number =
            integer(object.get("levelNumber").ok_or_else(|| {
                invalid("VALIDATION_FAILED", "Choose one of the six staff levels.")
            })?)
            .filter(|value| (1..=6).contains(value))
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Choose one of the six staff levels."))?;
        let reports_to_key = optional_text(
            object.get("reportsToKey"),
            4_096,
            "Choose a reporting template position.",
        )?;
        let is_ceo = object
            .get("isCeo")
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid("VALIDATION_FAILED", "Choose the CEO position."))?;
        let roster_required = match object.get("rosterRequired") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Choose whether the position requires a weekly roster.",
                ));
            }
        };
        Ok(Self {
            key,
            title,
            level_number,
            reports_to_key,
            is_ceo,
            roster_required,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    pub name: String,
    pub positions: Vec<TemplatePosition>,
    pub expected_version: i64,
}

impl TemplateInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let object = value.as_object().ok_or_else(|| {
            invalid(
                "VALIDATION_FAILED",
                "Provide a complete organization template.",
            )
        })?;
        let name = trimmed(
            object.get("name"),
            2,
            120,
            "Provide a template name of 2 to 120 characters.",
        )?;
        let positions = match object.get("positions") {
            Some(Value::Array(values))
                if !values.is_empty() && values.len() <= MAX_TEMPLATE_POSITIONS =>
            {
                values
                    .iter()
                    .map(TemplatePosition::parse)
                    .collect::<Result<Vec<_>, _>>()?
            }
            Some(Value::Array(_)) => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Provide 1 to 100 template positions.",
                ));
            }
            _ => {
                return Err(invalid(
                    "VALIDATION_FAILED",
                    "Provide 1 to 100 template positions.",
                ));
            }
        };
        let expected_version = integer(
            object
                .get("expectedVersion")
                .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the template version."))?,
        )
        .filter(|value| *value >= 0)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the template version."))?;
        Ok(Self {
            name,
            positions,
            expected_version,
        })
    }

    /// Exact retained `validateTemplate` order and error codes.
    pub fn validate(&self) -> Result<(), BusinessError> {
        validate_template_positions(&self.positions)
    }
}

/// Retained `validateTemplate` for positions loaded from a saved template.
pub fn validate_template_positions(positions: &[TemplatePosition]) -> Result<(), BusinessError> {
    if positions
        .iter()
        .map(|position| position.key.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != positions.len()
    {
        return Err(invalid(
            "DUPLICATE_POSITION",
            "Template position keys must be unique.",
        ));
    }
    for position in positions {
        if position.is_ceo && (position.level_number != 1 || position.reports_to_key.is_some()) {
            return Err(invalid(
                "CEO_POSITION_INVALID",
                "The CEO uses Level 1 with no reporting position.",
            ));
        }
        if !position.is_ceo && position.reports_to_key.is_none() {
            return Err(invalid(
                "REPORTING_POSITION_REQUIRED",
                "Each non-CEO position needs a reporting position.",
            ));
        }
        let mut seen = std::collections::BTreeSet::from([position.key.as_str()]);
        let mut parent = position.reports_to_key.as_deref();
        while let Some(key) = parent {
            if !seen.insert(key) {
                return Err(invalid(
                    "REPORTING_CYCLE",
                    "Correct the template reporting cycle.",
                ));
            }
            let next = positions
                .iter()
                .find(|candidate| candidate.key == key)
                .filter(|candidate| candidate.level_number < position.level_number)
                .ok_or_else(|| {
                    invalid(
                        "REPORTING_POSITION_INVALID",
                        "Template managers must exist and be more senior.",
                    )
                })?;
            parent = next.reports_to_key.as_deref();
        }
    }
    if positions.iter().filter(|position| position.is_ceo).count() != 1 {
        return Err(invalid(
            "CEO_POSITION_REQUIRED",
            "The organization template needs one CEO position.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyTemplateInput {
    pub expected_version: i64,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub reason: String,
}

impl ApplyTemplateInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let object = value.as_object().ok_or_else(|| {
            invalid(
                "VALIDATION_FAILED",
                "Provide a complete template application.",
            )
        })?;
        let expected_version = integer(
            object
                .get("expectedVersion")
                .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the template version."))?,
        )
        .filter(|value| *value >= 1)
        .ok_or_else(|| invalid("VALIDATION_FAILED", "Provide the template version."))?;
        let branch_id = optional_text(
            object.get("branchId"),
            200,
            "Choose an active branch in this company.",
        )?;
        let department_id = optional_text(
            object.get("departmentId"),
            200,
            "Choose a department in the selected branch.",
        )?;
        let reason = trimmed(
            object.get("reason"),
            3,
            1_000,
            "Provide a reason of 3 to 1000 characters.",
        )?;
        Ok(Self {
            expected_version,
            branch_id,
            department_id,
            reason,
        })
    }
}

/// The retained `checkScope` guard. An actor with a branch or department
/// assignment may only write inside that placement.
pub fn validate_scope(
    actor_branch: Option<&str>,
    actor_department: Option<&str>,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<(), BusinessError> {
    if actor_branch.is_some_and(|value| Some(value) != branch_id)
        || actor_department.is_some_and(|value| Some(value) != department_id)
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "SCOPE_FORBIDDEN",
            "Choose a position in your authorized workspace.",
        ));
    }
    Ok(())
}
