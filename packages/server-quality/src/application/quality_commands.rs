//! Native Quality specification and inspection command policy.
//!
//! These types are transport-independent. Current Account/IAM authority,
//! Commercial purchase admission, Commerce master witnesses and Workflow
//! receipts remain composition concerns. The retained TypeScript application
//! order and error codes are mirrored here so the native route can replace the
//! same owner effects without inventing a second policy.
use crate::domain::quality::{HoldStatus, InspectionResult, SpecStatus, validate_text};
use erpx_server_shared::public::{BusinessError, CommandKey, RecordId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualitySpecMetricInput {
    pub code: String,
    pub label: String,
    pub unit: String,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualitySpecInput {
    pub product_id: RecordId,
    pub name: String,
    pub expected_version: Option<i64>,
    pub metrics: Vec<QualitySpecMetricInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualitySpecCommand {
    pub key: CommandKey,
    pub id: Option<RecordId>,
    pub input: QualitySpecInput,
}

impl QualitySpecCommand {
    /// Retained TypeScript record identity. A new draft is keyed by command;
    /// an edit addresses the current draft row.
    pub fn record_id(&self) -> Result<RecordId, BusinessError> {
        match &self.id {
            Some(id) => Ok(id.clone()),
            None => RecordId::new(format!("quality-spec-{}", self.key.as_str())),
        }
    }

    pub fn operation(&self) -> &'static str {
        if self.id.is_some() {
            "quality.spec.update"
        } else {
            "quality.spec.create"
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualitySpecAction {
    Activate,
    Cancel,
}

impl QualitySpecAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "activate" => Some(Self::Activate),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualitySpecActionCommand {
    pub key: CommandKey,
    pub id: RecordId,
    pub expected_version: i64,
    pub action: QualitySpecAction,
    pub reason: String,
}

impl QualitySpecActionCommand {
    pub fn operation(&self) -> String {
        format!("quality.spec.{}", self.action.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualitySpecReceipt {
    pub id: String,
    pub status: SpecStatus,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityInspectionCommand {
    pub key: CommandKey,
    pub hold_id: RecordId,
    pub expected_version: i64,
    pub result: Option<InspectionResult>,
    pub evidence: String,
    pub measurements: Vec<crate::domain::quality::MeasurementInput>,
    pub attachment_ids: Vec<String>,
    pub attachment_set_ids: Vec<String>,
}

impl QualityInspectionCommand {
    pub fn operation(&self) -> &'static str {
        "quality.hold.inspect"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityInspectionReceipt {
    pub id: String,
    pub inspection_id: String,
    pub status: HoldStatus,
    pub result: InspectionResult,
    pub version: i64,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// A plain decimal exactly as accepted by the retained measurement schema.
pub fn is_measured_value(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut at = 0;
    if bytes.first() == Some(&b'-') {
        at = 1;
    }
    let integer_start = at;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
    }
    if !(1..=12).contains(&(at - integer_start)) {
        return false;
    }
    if at == bytes.len() {
        return true;
    }
    if bytes[at] != b'.' {
        return false;
    }
    at += 1;
    let fraction_start = at;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
    }
    at == bytes.len() && (1..=6).contains(&(at - fraction_start))
}

fn metric_code(value: &str) -> Result<String, BusinessError> {
    let value = validate_text(
        value,
        1,
        40,
        "QUALITY_METRIC_INVALID",
        "Use a measurement code between 1 and 40 characters.",
    )?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(invalid(
            "QUALITY_METRIC_INVALID",
            "Use letters, digits, underscore or hyphen for every measurement code.",
        ));
    }
    Ok(value)
}

fn limit(value: &str) -> Result<Option<String>, BusinessError> {
    if value.is_empty() {
        return Ok(None);
    }
    if !is_measured_value(value) {
        return Err(invalid(
            "QUALITY_LIMIT_INVALID",
            "Use a plain decimal inspection limit with at most six decimal places.",
        ));
    }
    let number = value
        .parse::<f64>()
        .map_err(|_| invalid("QUALITY_LIMIT_INVALID", "Use a valid inspection limit."))?;
    if !number.is_finite() {
        return Err(invalid(
            "QUALITY_LIMIT_INVALID",
            "Use a finite inspection limit.",
        ));
    }
    Ok(Some(value.to_owned()))
}

fn metric(input: QualitySpecMetricInput) -> Result<QualitySpecMetricInput, BusinessError> {
    let code = metric_code(&input.code)?;
    let label = validate_text(
        &input.label,
        2,
        120,
        "QUALITY_METRIC_INVALID",
        "Use a measurement label between 2 and 120 characters.",
    )?;
    let unit = validate_text(
        &input.unit,
        1,
        40,
        "QUALITY_METRIC_INVALID",
        "Use a measurement unit between 1 and 40 characters.",
    )?;
    let minimum = limit(&input.minimum.unwrap_or_default())?;
    let maximum = limit(&input.maximum.unwrap_or_default())?;
    if minimum.is_none() && maximum.is_none() {
        return Err(invalid(
            "QUALITY_LIMIT_REQUIRED",
            "Every measurement needs a minimum or maximum.",
        ));
    }
    if let (Some(minimum), Some(maximum)) = (&minimum, &maximum)
        && minimum
            .parse::<f64>()
            .ok()
            .zip(maximum.parse::<f64>().ok())
            .is_none_or(|(minimum, maximum)| minimum > maximum)
    {
        return Err(invalid(
            "QUALITY_LIMIT_INVALID",
            "A measurement minimum cannot exceed its maximum.",
        ));
    }
    Ok(QualitySpecMetricInput {
        code,
        label,
        unit,
        minimum,
        maximum,
    })
}

pub fn normalize_quality_spec_save(
    command: QualitySpecCommand,
) -> Result<QualitySpecCommand, BusinessError> {
    let name = validate_text(
        &command.input.name,
        3,
        160,
        "QUALITY_SPEC_NAME",
        "Enter a specification name between 3 and 160 characters.",
    )?;
    if command
        .input
        .expected_version
        .is_some_and(|version| version < 1)
    {
        return Err(invalid(
            "VERSION_REQUIRED",
            "Refresh the draft specification before editing.",
        ));
    }
    if command.input.metrics.is_empty() || command.input.metrics.len() > 50 {
        return Err(invalid(
            "QUALITY_METRICS_REQUIRED",
            "Add between one and fifty measurements.",
        ));
    }
    let mut codes = std::collections::BTreeSet::new();
    let mut metrics = Vec::with_capacity(command.input.metrics.len());
    for input in command.input.metrics {
        let metric = metric(input)?;
        if !codes.insert(metric.code.to_ascii_lowercase()) {
            return Err(invalid(
                "QUALITY_METRIC_DUPLICATE",
                "Each measurement code must be unique.",
            ));
        }
        metrics.push(metric);
    }
    Ok(QualitySpecCommand {
        input: QualitySpecInput {
            name,
            metrics,
            ..command.input
        },
        ..command
    })
}

pub fn normalize_quality_spec_action(
    command: QualitySpecActionCommand,
) -> Result<QualitySpecActionCommand, BusinessError> {
    if command.expected_version < 1 {
        return Err(invalid(
            "VERSION_REQUIRED",
            "Refresh the draft specification before editing.",
        ));
    }
    Ok(QualitySpecActionCommand {
        reason: validate_text(
            &command.reason,
            5,
            2000,
            "QUALITY_SPEC_REASON",
            "Enter a specification decision reason between 5 and 2000 characters.",
        )?,
        ..command
    })
}

pub fn normalize_quality_inspection(
    command: QualityInspectionCommand,
) -> Result<QualityInspectionCommand, BusinessError> {
    if command.expected_version < 1 {
        return Err(invalid(
            "VERSION_REQUIRED",
            "Refresh the quality hold before inspecting.",
        ));
    }
    if command.measurements.len() > 100 {
        return Err(invalid(
            "QUALITY_MEASUREMENTS_REQUIRED",
            "Record at most one hundred measurements.",
        ));
    }
    let mut metric_ids = std::collections::BTreeSet::new();
    for measurement in &command.measurements {
        if measurement.metric_id.is_empty()
            || !is_measured_value(&measurement.value)
            || !metric_ids.insert(measurement.metric_id.clone())
        {
            return Err(invalid(
                "QUALITY_MEASUREMENTS_REQUIRED",
                "Record one valid value for every measurement in the held specification.",
            ));
        }
    }
    if command.attachment_ids.len() > 10
        || command.attachment_set_ids.len() > 10
        || command
            .attachment_ids
            .iter()
            .try_fold(std::collections::BTreeSet::new(), |mut seen, id| {
                (seen.insert(id)).then_some(seen).ok_or(())
            })
            .is_err()
        || command
            .attachment_set_ids
            .iter()
            .try_fold(std::collections::BTreeSet::new(), |mut seen, id| {
                (seen.insert(id)).then_some(seen).ok_or(())
            })
            .is_err()
    {
        return Err(invalid(
            "QUALITY_ATTACHMENT_DUPLICATE",
            "Choose each inspection file once.",
        ));
    }
    Ok(QualityInspectionCommand {
        evidence: validate_text(
            &command.evidence,
            3,
            2000,
            "QUALITY_EVIDENCE_REQUIRED",
            "Describe the inspection evidence between 3 and 2000 characters.",
        )?,
        ..command
    })
}
