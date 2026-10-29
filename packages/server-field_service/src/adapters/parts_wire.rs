//! Exact original Field Service parts schema normalization; command data only.
use crate::application::parts::{
    EncodedFieldServiceInput, FieldServiceLotSelection, FieldServicePartsCommand,
    FieldServicePartsDetails, FieldServicePartsInput, NormalizedFieldServiceParts,
};
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldServiceIssuePath {
    Field(&'static str),
    Index(usize),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceWireIssue {
    pub code: &'static str,
    pub path: Vec<FieldServiceIssuePath>,
    pub message: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldServiceWireFailure {
    pub invalid_json: bool,
    pub issues: Vec<FieldServiceWireIssue>,
}
impl FieldServiceWireFailure {
    fn json() -> Self {
        Self {
            invalid_json: true,
            issues: vec![],
        }
    }
    fn validation(code: &'static str, path: Vec<FieldServiceIssuePath>, message: String) -> Self {
        Self {
            invalid_json: false,
            issues: vec![FieldServiceWireIssue {
                code,
                path,
                message,
            }],
        }
    }
}
fn field(name: &'static str) -> Vec<FieldServiceIssuePath> {
    vec![FieldServiceIssuePath::Field(name)]
}
fn document(bytes: &[u8]) -> Result<CommandJson, FieldServiceWireFailure> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let body = CommandJson::parse(text).map_err(|_| FieldServiceWireFailure::json())?;
    if !body.as_str().starts_with('{') {
        return Err(FieldServiceWireFailure::validation(
            "invalid_type",
            vec![],
            format!(
                "Invalid input: expected object, received {}",
                received(Some(&body))
            ),
        ));
    }
    Ok(body)
}
fn received(value: Option<&CommandJson>) -> &'static str {
    match value.map(CommandJson::as_str) {
        None => "undefined",
        Some("null") => "null",
        Some("true" | "false") => "boolean",
        Some(v) if v.starts_with('"') => "string",
        Some(v) if v.starts_with('[') => "array",
        Some(v) if v.starts_with('{') => "object",
        Some(_) => "number",
    }
}
fn property(v: &CommandJson, name: &str) -> Option<CommandJson> {
    v.property(name).ok().flatten()
}
fn whitespace(n: u16) -> bool {
    matches!(n,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}
#[derive(Default)]
struct Validation {
    issues: Vec<FieldServiceWireIssue>,
}
impl Validation {
    fn issue(&mut self, code: &'static str, path: &[FieldServiceIssuePath], message: String) {
        self.issues.push(FieldServiceWireIssue {
            code,
            path: path.to_vec(),
            message,
        });
    }
    fn string(
        &mut self,
        v: Option<CommandJson>,
        path: &[FieldServiceIssuePath],
        minimum: usize,
        maximum: Option<usize>,
        trim: bool,
    ) -> Option<CommandJson> {
        let Some(mut units) = v
            .as_ref()
            .and_then(|x| x.string_utf16_units().ok().flatten())
        else {
            self.issue(
                "invalid_type",
                path,
                format!(
                    "Invalid input: expected string, received {}",
                    received(v.as_ref())
                ),
            );
            return None;
        };
        if trim {
            let start = units
                .iter()
                .position(|u| !whitespace(*u))
                .unwrap_or(units.len());
            let end = units
                .iter()
                .rposition(|u| !whitespace(*u))
                .map_or(start, |n| n + 1);
            units = units[start..end].to_vec();
        }
        // Pinned B0 Zod4.6.7 counts Unicode code points; lone surrogate units count
        // individually. Preserve those exact units in normalized receipt identity.
        let length = char::decode_utf16(units.iter().copied()).count();
        if length < minimum {
            self.issue(
                "too_small",
                path,
                format!("Too small: expected string to have >={minimum} characters"),
            );
        }
        if maximum.is_some_and(|m| length > m) {
            self.issue(
                "too_big",
                path,
                format!(
                    "Too big: expected string to have <={} characters",
                    maximum.expect("checked maximum")
                ),
            );
        }
        CommandJson::from_utf16_string(&units).ok()
    }
    fn integer(
        &mut self,
        v: Option<CommandJson>,
        path: &[FieldServiceIssuePath],
        extra_safe: bool,
    ) -> Option<CommandJson> {
        self.integer_bound(v, path, extra_safe, false)
    }
    fn integer_bound(
        &mut self,
        v: Option<CommandJson>,
        path: &[FieldServiceIssuePath],
        extra_safe: bool,
        allow_zero: bool,
    ) -> Option<CommandJson> {
        let Some(value) = v
            .as_ref()
            .and_then(|x| x.as_str().parse::<f64>().ok())
            .filter(|x| x.is_finite())
        else {
            self.issue(
                "invalid_type",
                path,
                format!(
                    "Invalid input: expected number, received {}",
                    received(v.as_ref())
                ),
            );
            return None;
        };
        if value.fract() != 0.0 {
            self.issue(
                "invalid_type",
                path,
                "Invalid input: expected int, received number".into(),
            );
            return None;
        }
        const MAX: f64 = 9_007_199_254_740_991.0;
        if value > MAX {
            self.issue(
                "too_big",
                path,
                "Too big: expected int to be <=9007199254740991".into(),
            );
        } else if value < -MAX {
            self.issue(
                "too_small",
                path,
                "Too small: expected int to be >=-9007199254740991".into(),
            );
        }
        if value < 0.0 || (!allow_zero && value == 0.0) {
            self.issue(
                "too_small",
                path,
                "Too small: expected number to be >0".into(),
            );
        }
        if extra_safe && !(-MAX..=MAX).contains(&value) {
            self.issue(
                if value > 0.0 { "too_big" } else { "too_small" },
                path,
                if value > 0.0 {
                    "Too big: expected int to be <=9007199254740991"
                } else {
                    "Too small: expected int to be >=-9007199254740991"
                }
                .into(),
            );
        }
        v
    }
    fn date(
        &mut self,
        v: Option<CommandJson>,
        path: &[FieldServiceIssuePath],
    ) -> Option<CommandJson> {
        let normalized = self.string(v, path, 0, None, false);
        if let Some(n) = &normalized {
            let s = n.string_utf8_storage().ok().flatten().unwrap_or_default();
            if !iso_date(&s) {
                self.issue("invalid_format", path, "Invalid ISO date".into());
            }
        }
        normalized
    }
    fn allocations(&mut self, v: Option<CommandJson>) -> Option<CommandJson> {
        let path = field("allocations");
        let Some(v) = v else {
            return CommandJson::parse("[]").ok();
        };
        let Some(values) = v.array_elements().ok().flatten() else {
            self.issue(
                "invalid_type",
                &path,
                format!(
                    "Invalid input: expected array, received {}",
                    received(Some(&v))
                ),
            );
            return None;
        };
        // The pinned original array bound rejects before child validation.
        // Preserve that failure order and avoid traversing oversized input.
        if values.len() > 200 {
            self.issue(
                "too_big",
                &path,
                "Too big: expected array to have <=200 items".into(),
            );
            return None;
        }
        let mut normalized = Vec::with_capacity(values.len());
        for (index, lot) in values.iter().enumerate() {
            let mut prefix = path.clone();
            prefix.push(FieldServiceIssuePath::Index(index));
            if !lot.as_str().starts_with('{') {
                self.issue(
                    "invalid_type",
                    &prefix,
                    format!(
                        "Invalid input: expected object, received {}",
                        received(Some(lot))
                    ),
                );
                continue;
            }
            let mut fields = vec![];
            for (name, safe) in [
                ("lotId", false),
                ("quantityMilli", true),
                ("balanceVersion", false),
            ] {
                let mut p = prefix.clone();
                p.push(FieldServiceIssuePath::Field(name));
                let value = if name == "lotId" {
                    self.string(property(lot, name), &p, 1, None, false)
                } else {
                    self.integer(property(lot, name), &p, safe)
                };
                if let Some(value) = value {
                    fields.push((name, value));
                }
            }
            if let Ok(value) = object(&fields) {
                normalized.push(value);
            }
        }
        array(&normalized).ok()
    }
    fn finish(
        self,
        fields: &[(&'static str, CommandJson)],
    ) -> Result<CommandJson, FieldServiceWireFailure> {
        if !self.issues.is_empty() {
            return Err(FieldServiceWireFailure {
                invalid_json: false,
                issues: self.issues,
            });
        }
        object(fields)
    }
}
fn object(fields: &[(&str, CommandJson)]) -> Result<CommandJson, FieldServiceWireFailure> {
    CommandJson::parse(&format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(k, v)| format!("\"{k}\":{}", v.as_str()))
            .collect::<Vec<_>>()
            .join(",")
    ))
    .map_err(|_| FieldServiceWireFailure::json())
}
fn array(values: &[CommandJson]) -> Result<CommandJson, FieldServiceWireFailure> {
    CommandJson::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(CommandJson::as_str)
            .collect::<Vec<_>>()
            .join(",")
    ))
    .map_err(|_| FieldServiceWireFailure::json())
}
fn text(v: &CommandJson, name: &str) -> Result<String, FieldServiceWireFailure> {
    property(v, name)
        .and_then(|x| x.string_utf8_storage().ok().flatten())
        .ok_or_else(FieldServiceWireFailure::json)
}
fn int(v: &CommandJson, name: &str) -> Result<i64, FieldServiceWireFailure> {
    property(v, name)
        .and_then(|x| x.as_str().parse().ok())
        .ok_or_else(FieldServiceWireFailure::json)
}
fn iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let y = s[..4].parse::<u32>().unwrap_or(0);
    let m = s[5..7].parse::<u32>().unwrap_or(0);
    let d = s[8..].parse::<u32>().unwrap_or(0);
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let limit = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    d >= 1 && d <= limit
}
pub fn normalize_field_service_parts(
    bytes: &[u8],
) -> Result<NormalizedFieldServiceParts, FieldServiceWireFailure> {
    let body = document(bytes)?;
    let action = property(&body, "action")
        .and_then(|x| x.string_utf8_storage().ok().flatten())
        .filter(|s| ["reserve-part", "issue-part"].contains(&s.as_str()))
        .ok_or_else(|| FieldServiceWireFailure::validation("invalid_union", field("action"), "Invalid discriminator value. Expected 'schedule' | 'reserve-part' | 'issue-part' | 'dispatch' | 'start' | 'resume' | 'cancel' | 'exception' | 'complete' | 'release-billing'".into()))?;
    let mut check = Validation::default();
    let mut fields = vec![(
        "action",
        CommandJson::from_serializable(&action).map_err(|_| FieldServiceWireFailure::json())?,
    )];
    if let Some(v) = check.integer(
        property(&body, "expectedVersion"),
        &field("expectedVersion"),
        false,
    ) {
        fields.push(("expectedVersion", v));
    }
    if action == "reserve-part" {
        for name in ["warehouseId", "productId"] {
            if let Some(v) = check.string(property(&body, name), &field(name), 1, None, false) {
                fields.push((name, v));
            }
        }
        for name in ["quantityMilli", "expiresAt"] {
            if let Some(v) = check.integer(property(&body, name), &field(name), false) {
                fields.push((name, v));
            }
        }
    } else {
        if let Some(v) = check.string(
            property(&body, "reservationId"),
            &field("reservationId"),
            1,
            None,
            false,
        ) {
            fields.push(("reservationId", v));
        }
        if let Some(v) = check.integer(
            property(&body, "quantityMilli"),
            &field("quantityMilli"),
            false,
        ) {
            fields.push(("quantityMilli", v));
        }
        if let Some(raw) = property(&body, "entryDate")
            && let Some(v) = check.date(Some(raw), &field("entryDate"))
        {
            fields.push(("entryDate", v));
        }
        if let Some(v) = check.allocations(property(&body, "allocations")) {
            fields.push(("allocations", v));
        }
    }
    if let Some(v) = check.string(
        property(&body, "reason"),
        &field("reason"),
        3,
        Some(3000),
        true,
    ) {
        fields.push(("reason", v));
    }
    let json = check.finish(&fields)?;
    let expected_version = int(&json, "expectedVersion")?;
    let reason = text(&json, "reason")?;
    let input = if action == "reserve-part" {
        FieldServicePartsInput::Reserve {
            expected_version,
            warehouse_id: text(&json, "warehouseId")?,
            product_id: text(&json, "productId")?,
            quantity_milli: int(&json, "quantityMilli")?,
            expires_at: int(&json, "expiresAt")?,
            reason,
        }
    } else {
        let allocations = property(&json, "allocations")
            .and_then(|x| x.array_elements().ok().flatten())
            .ok_or_else(FieldServiceWireFailure::json)?
            .into_iter()
            .map(|lot| {
                Ok(FieldServiceLotSelection {
                    lot_id: text(&lot, "lotId")?,
                    quantity_milli: int(&lot, "quantityMilli")?,
                    balance_version: int(&lot, "balanceVersion")?,
                })
            })
            .collect::<Result<Vec<_>, FieldServiceWireFailure>>()?;
        FieldServicePartsInput::Issue {
            expected_version,
            reservation_id: text(&json, "reservationId")?,
            quantity_milli: int(&json, "quantityMilli")?,
            entry_date: property(&json, "entryDate")
                .map(|_| text(&json, "entryDate"))
                .transpose()?,
            allocations,
            reason,
        }
    };
    Ok(NormalizedFieldServiceParts {
        input,
        receipt_input: EncodedFieldServiceInput(json.as_str().to_owned()),
    })
}

/// Reconstruct the validated original without changing its UTF-16 string units.
pub fn field_service_parts_receipt_json(
    input: &EncodedFieldServiceInput,
) -> Result<CommandJson, BusinessError> {
    CommandJson::parse(input.as_str())
}
/// Encode the semantic transition at the original application boundary.
pub fn field_service_parts_details_json(
    command: &FieldServicePartsCommand,
    details: &FieldServicePartsDetails,
) -> Result<CommandJson, BusinessError> {
    let original = field_service_parts_receipt_json(command.receipt_input())?;
    let field = |name: &str| {
        original.property(name)?.ok_or_else(|| {
            BusinessError::invalid(
                "FIELD_SERVICE_COMMAND_INPUT",
                "Retain the normalized original Service command.",
            )
        })
    };
    let fields: Vec<(&str, CommandJson)> = match details {
        FieldServicePartsDetails::Reserve => vec![
            ("warehouseId", field("warehouseId")?),
            ("productId", field("productId")?),
            ("quantityMilli", field("quantityMilli")?),
        ],
        FieldServicePartsDetails::Issue {
            consumption_id,
            cost_minor,
            journal_id,
        } => vec![
            ("reservationId", field("reservationId")?),
            ("quantityMilli", field("quantityMilli")?),
            (
                "consumptionId",
                CommandJson::from_serializable(consumption_id)?,
            ),
            ("costMinor", CommandJson::from_serializable(cost_minor)?),
            ("journalId", CommandJson::from_serializable(journal_id)?),
        ],
    };
    let mut text = String::from("{");
    for (index, (name, value)) in fields.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(CommandJson::from_serializable(name)?.as_str());
        text.push(':');
        text.push_str(value.as_str());
    }
    text.push('}');
    CommandJson::parse(&text)
}

/// The request schema strips shared-edge `entity` and unknown fields before
/// Workflow fingerprints the exact B0-normalized input.
#[derive(Clone)]
pub struct NormalizedServiceRequest {
    pub agreement_id: String,
    pub agreement_version: i64,
    pub title: String,
    pub issue: String,
    pub site: String,
    pub technician_id: String,
    pub scheduled_at: Option<i64>,
    pub receipt_input: CommandJson,
}
pub fn normalize_service_request(
    bytes: &[u8],
) -> Result<NormalizedServiceRequest, FieldServiceWireFailure> {
    let body = document(bytes)?;
    let mut check = Validation::default();
    let mut fields = Vec::new();
    if let Some(v) = check.string(
        property(&body, "agreementId"),
        &field("agreementId"),
        1,
        None,
        false,
    ) {
        fields.push(("agreementId", v));
    }
    if let Some(v) = check.integer(
        property(&body, "agreementVersion"),
        &field("agreementVersion"),
        false,
    ) {
        fields.push(("agreementVersion", v));
    }
    for (name, max) in [("title", 160), ("issue", 3000), ("site", 300)] {
        if let Some(v) = check.string(property(&body, name), &field(name), 3, Some(max), true) {
            fields.push((name, v));
        }
    }
    if let Some(v) = check.string(
        property(&body, "technicianId"),
        &field("technicianId"),
        1,
        None,
        false,
    ) {
        fields.push(("technicianId", v));
    }
    if let Some(value) = property(&body, "scheduledAt") {
        if value.as_str() == "null" {
            fields.push(("scheduledAt", value));
        } else if let Some(v) = check.integer(Some(value), &field("scheduledAt"), false) {
            fields.push(("scheduledAt", v));
        }
    }
    let json = check.finish(&fields)?;
    let scheduled_at = match property(&json, "scheduledAt") {
        None => None,
        Some(value) if value.as_str() == "null" => None,
        Some(value) => Some(
            value
                .as_str()
                .parse()
                .map_err(|_| FieldServiceWireFailure::json())?,
        ),
    };
    Ok(NormalizedServiceRequest {
        agreement_id: text(&json, "agreementId")?,
        agreement_version: int(&json, "agreementVersion")?,
        title: text(&json, "title")?,
        issue: text(&json, "issue")?,
        site: text(&json, "site")?,
        technician_id: text(&json, "technicianId")?,
        scheduled_at,
        receipt_input: json,
    })
}

#[cfg(test)]
mod request_tests {
    use super::*;
    #[test]
    fn request_normalization_preserves_optional_null_and_strips_edge_fields() {
        let input=br#"{"entity":"request","agreementId":"a","agreementVersion":2,"title":"  Visit  ","issue":" Broken ","site":" Shop ","technicianId":"u","scheduledAt":null,"ignored":true}"#;
        let value = normalize_service_request(input).unwrap();
        assert_eq!(
            value.receipt_input.as_str(),
            r#"{"agreementId":"a","agreementVersion":2,"title":"Visit","issue":"Broken","site":"Shop","technicianId":"u","scheduledAt":null}"#
        );
        assert_eq!(value.scheduled_at, None);
        let no_date=normalize_service_request(br#"{"agreementId":"a","agreementVersion":2,"title":"Visit","issue":"Broken","site":"Shop","technicianId":"u"}"#).unwrap();
        assert_ne!(value.receipt_input.as_str(), no_date.receipt_input.as_str());
    }
    #[test]
    fn request_validation_rejects_short_title_and_noninteger_schedule() {
        let error=normalize_service_request(br#"{"agreementId":"a","agreementVersion":2,"title":" x ","issue":"Broken","site":"Shop","technicianId":"u","scheduledAt":1.5}"#).err().unwrap();
        assert_eq!(error.issues.len(), 2);
        assert_eq!(error.issues[0].path, field("title"));
        assert_eq!(error.issues[1].path, field("scheduledAt"));
    }
    #[test]
    fn order_create_fingerprint_distinguishes_null_from_omitted_fields() {
        let explicit = normalize_service_order_create(br#"{"ticketId":"t","ticketVersion":3,"agreementId":"a","agreementVersion":2,"assetId":null,"issue":" Broken ","site":" Shop ","technicianId":"u","scheduledAt":null,"ignored":true}"#).unwrap();
        let omitted = normalize_service_order_create(br#"{"ticketId":"t","ticketVersion":3,"agreementId":"a","agreementVersion":2,"issue":"Broken","site":"Shop","technicianId":"u"}"#).unwrap();
        assert_eq!(explicit.asset_id, None);
        assert_eq!(explicit.scheduled_at, None);
        assert_ne!(
            explicit.receipt_input.as_str(),
            omitted.receipt_input.as_str()
        );
        assert!(!explicit.receipt_input.as_str().contains("ignored"));
    }
}

#[derive(Clone)]
pub struct NormalizedServiceOrderCreate {
    pub ticket_id: String,
    pub ticket_version: i64,
    pub agreement_id: String,
    pub agreement_version: i64,
    pub asset_id: Option<String>,
    pub issue: String,
    pub site: String,
    pub technician_id: String,
    pub scheduled_at: Option<i64>,
    pub receipt_input: CommandJson,
}
pub fn normalize_service_order_create(
    bytes: &[u8],
) -> Result<NormalizedServiceOrderCreate, FieldServiceWireFailure> {
    let body = document(bytes)?;
    let mut check = Validation::default();
    let mut fields = Vec::new();
    for (name, version) in [
        ("ticketId", "ticketVersion"),
        ("agreementId", "agreementVersion"),
    ] {
        if let Some(v) = check.string(property(&body, name), &field(name), 1, None, false) {
            fields.push((name, v));
        }
        if let Some(v) = check.integer(property(&body, version), &field(version), false) {
            fields.push((version, v));
        }
    }
    if let Some(value) = property(&body, "assetId") {
        if value.as_str() == "null" {
            fields.push(("assetId", value));
        } else if let Some(v) = check.string(Some(value), &field("assetId"), 0, None, false) {
            fields.push(("assetId", v));
        }
    }
    for (name, max) in [("issue", 3000), ("site", 300)] {
        if let Some(v) = check.string(property(&body, name), &field(name), 3, Some(max), true) {
            fields.push((name, v));
        }
    }
    if let Some(v) = check.string(
        property(&body, "technicianId"),
        &field("technicianId"),
        1,
        None,
        false,
    ) {
        fields.push(("technicianId", v));
    }
    if let Some(value) = property(&body, "scheduledAt") {
        if value.as_str() == "null" {
            fields.push(("scheduledAt", value));
        } else if let Some(v) = check.integer(Some(value), &field("scheduledAt"), false) {
            fields.push(("scheduledAt", v));
        }
    }
    let json = check.finish(&fields)?;
    let asset_id = match property(&json, "assetId") {
        None => None,
        Some(v) if v.as_str() == "null" => None,
        Some(_) => Some(text(&json, "assetId")?),
    };
    let scheduled_at = match property(&json, "scheduledAt") {
        None => None,
        Some(v) if v.as_str() == "null" => None,
        Some(v) => Some(
            v.as_str()
                .parse()
                .map_err(|_| FieldServiceWireFailure::json())?,
        ),
    };
    Ok(NormalizedServiceOrderCreate {
        ticket_id: text(&json, "ticketId")?,
        ticket_version: int(&json, "ticketVersion")?,
        agreement_id: text(&json, "agreementId")?,
        agreement_version: int(&json, "agreementVersion")?,
        asset_id,
        issue: text(&json, "issue")?,
        site: text(&json, "site")?,
        technician_id: text(&json, "technicianId")?,
        scheduled_at,
        receipt_input: json,
    })
}

#[derive(Clone)]
pub struct NormalizedFieldServiceVisit {
    pub action: crate::domain::visit::VisitAction,
    pub expected_version: i64,
    pub technician_id: Option<String>,
    pub scheduled_at: Option<i64>,
    pub reason: String,
    pub labor_minutes: Option<i64>,
    pub evidence: Option<String>,
    pub customer_confirmation: Option<String>,
    pub receipt_input: CommandJson,
}
pub fn normalize_field_service_visit(
    bytes: &[u8],
) -> Result<NormalizedFieldServiceVisit, FieldServiceWireFailure> {
    use crate::domain::visit::VisitAction;
    let body = document(bytes)?;
    let action_json = property(&body, "action").ok_or_else(|| {
        FieldServiceWireFailure::validation(
            "invalid_union",
            field("action"),
            "Invalid input".into(),
        )
    })?;
    let action = match action_json.string_utf8_storage().ok().flatten().as_deref() {
        Some("schedule") => VisitAction::Schedule,
        Some("dispatch") => VisitAction::Dispatch,
        Some("start") => VisitAction::Start,
        Some("complete") => VisitAction::Complete,
        Some("exception") => VisitAction::Exception,
        Some("resume") => VisitAction::Resume,
        Some("cancel") => VisitAction::Cancel,
        _ => {
            return Err(FieldServiceWireFailure::validation(
                "invalid_union",
                field("action"),
                "Invalid input".into(),
            ));
        }
    };
    let mut check = Validation::default();
    let mut fields = vec![("action", action_json)];
    if let Some(v) = check.integer(
        property(&body, "expectedVersion"),
        &field("expectedVersion"),
        false,
    ) {
        fields.push(("expectedVersion", v));
    }
    if action == VisitAction::Schedule {
        if let Some(v) = check.string(
            property(&body, "technicianId"),
            &field("technicianId"),
            1,
            None,
            false,
        ) {
            fields.push(("technicianId", v));
        }
        if let Some(v) = check.integer(property(&body, "scheduledAt"), &field("scheduledAt"), false)
        {
            fields.push(("scheduledAt", v));
        }
    }
    if action == VisitAction::Complete {
        if let Some(v) = check.integer_bound(
            property(&body, "laborMinutes"),
            &field("laborMinutes"),
            false,
            true,
        ) {
            fields.push(("laborMinutes", v));
        }
        for name in ["evidence", "customerConfirmation"] {
            if let Some(v) = check.string(property(&body, name), &field(name), 3, Some(4000), true)
            {
                fields.push((name, v));
            }
        }
    }
    if let Some(v) = check.string(
        property(&body, "reason"),
        &field("reason"),
        3,
        Some(3000),
        true,
    ) {
        fields.push(("reason", v));
    }
    let json = check.finish(&fields)?;
    Ok(NormalizedFieldServiceVisit {
        action,
        expected_version: int(&json, "expectedVersion")?,
        technician_id: if action == VisitAction::Schedule {
            Some(text(&json, "technicianId")?)
        } else {
            None
        },
        scheduled_at: if action == VisitAction::Schedule {
            Some(int(&json, "scheduledAt")?)
        } else {
            None
        },
        reason: text(&json, "reason")?,
        labor_minutes: if action == VisitAction::Complete {
            Some(int(&json, "laborMinutes")?)
        } else {
            None
        },
        evidence: if action == VisitAction::Complete {
            Some(text(&json, "evidence")?)
        } else {
            None
        },
        customer_confirmation: if action == VisitAction::Complete {
            Some(text(&json, "customerConfirmation")?)
        } else {
            None
        },
        receipt_input: json,
    })
}
