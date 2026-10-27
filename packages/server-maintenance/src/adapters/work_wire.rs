//! Original B0 Maintenance schema normalization, separate from current authority.
use crate::application::work_commands::{NormalizedMaintenanceChange, NormalizedMaintenanceCreate};
use crate::domain::work_input::*;
use erpx_server_workflow::public::commands::CommandJson;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaintenanceIssuePath {
    Field(&'static str),
    Index(usize),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceWireIssue {
    pub code: &'static str,
    pub path: Vec<MaintenanceIssuePath>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceWireFailure {
    pub invalid_json: bool,
    pub issues: Vec<MaintenanceWireIssue>,
}
impl MaintenanceWireFailure {
    fn json() -> Self {
        Self {
            invalid_json: true,
            issues: vec![],
        }
    }
    fn validation(code: &'static str, path: Vec<MaintenanceIssuePath>) -> Self {
        Self {
            invalid_json: false,
            issues: vec![MaintenanceWireIssue { code, path }],
        }
    }
}
fn field(name: &'static str) -> Vec<MaintenanceIssuePath> {
    vec![MaintenanceIssuePath::Field(name)]
}
fn document(bytes: &[u8]) -> Result<CommandJson, MaintenanceWireFailure> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let body = CommandJson::parse(text).map_err(|_| MaintenanceWireFailure::json())?;
    if !body.as_str().starts_with('{') {
        return Err(MaintenanceWireFailure::validation("invalid_type", vec![]));
    }
    Ok(body)
}
fn property(v: &CommandJson, name: &str) -> Option<CommandJson> {
    v.property(name).ok().flatten()
}
fn whitespace(n: u16) -> bool {
    matches!(n,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}
#[derive(Default)]
struct Validation {
    issues: Vec<MaintenanceWireIssue>,
}
impl Validation {
    fn issue(&mut self, code: &'static str, path: &[MaintenanceIssuePath]) {
        self.issues.push(MaintenanceWireIssue {
            code,
            path: path.to_vec(),
        });
    }
    fn string(
        &mut self,
        v: Option<CommandJson>,
        path: &[MaintenanceIssuePath],
        minimum: usize,
        maximum: Option<usize>,
        trim: bool,
    ) -> Option<CommandJson> {
        let Some(mut units) = v.and_then(|x| x.string_utf16_units().ok().flatten()) else {
            self.issue("invalid_type", path);
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
            self.issue("too_small", path);
        }
        if maximum.is_some_and(|m| length > m) {
            self.issue("too_big", path);
        }
        CommandJson::from_utf16_string(&units).ok()
    }
    fn enumeration(
        &mut self,
        v: Option<CommandJson>,
        path: &[MaintenanceIssuePath],
        allowed: &[&str],
    ) -> Option<CommandJson> {
        let value = v
            .as_ref()
            .and_then(|x| x.string_utf8_storage().ok().flatten());
        if !value.as_deref().is_some_and(|x| allowed.contains(&x)) {
            self.issue("invalid_value", path);
            None
        } else {
            v
        }
    }
    fn integer(
        &mut self,
        v: Option<CommandJson>,
        path: &[MaintenanceIssuePath],
        extra_safe: bool,
    ) -> Option<CommandJson> {
        let Some(value) = v
            .as_ref()
            .and_then(|x| x.as_str().parse::<f64>().ok())
            .filter(|x| x.is_finite() && x.fract() == 0.0)
        else {
            self.issue("invalid_type", path);
            return None;
        };
        const MAX: f64 = 9_007_199_254_740_991.0;
        if value > MAX {
            self.issue("too_big", path);
        } else if value < -MAX {
            self.issue("too_small", path);
        }
        if value <= 0.0 {
            self.issue("too_small", path);
        }
        if extra_safe && !(-MAX..=MAX).contains(&value) {
            self.issue(if value > 0.0 { "too_big" } else { "too_small" }, path);
        }
        v
    }
    fn date(
        &mut self,
        v: Option<CommandJson>,
        path: &[MaintenanceIssuePath],
    ) -> Option<CommandJson> {
        let normalized = self.string(v, path, 0, None, false);
        if let Some(n) = &normalized {
            let s = n.string_utf8_storage().ok().flatten().unwrap_or_default();
            if !iso_date(&s) {
                self.issue("invalid_format", path);
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
            self.issue("invalid_type", &path);
            return None;
        };
        // The pinned original array bound rejects before child validation.
        // Preserve that failure order and avoid traversing oversized input.
        if values.len() > 200 {
            self.issue("too_big", &path);
            return None;
        }
        let mut normalized = Vec::with_capacity(values.len());
        for (index, lot) in values.iter().enumerate() {
            let mut prefix = path.clone();
            prefix.push(MaintenanceIssuePath::Index(index));
            if !lot.as_str().starts_with('{') {
                self.issue("invalid_type", &prefix);
                continue;
            }
            let mut fields = vec![];
            for (name, safe) in [
                ("lotId", false),
                ("quantityMilli", true),
                ("balanceVersion", false),
            ] {
                let mut p = prefix.clone();
                p.push(MaintenanceIssuePath::Field(name));
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
    ) -> Result<CommandJson, MaintenanceWireFailure> {
        if !self.issues.is_empty() {
            return Err(MaintenanceWireFailure {
                invalid_json: false,
                issues: self.issues,
            });
        }
        object(fields)
    }
}
fn object(fields: &[(&str, CommandJson)]) -> Result<CommandJson, MaintenanceWireFailure> {
    CommandJson::parse(&format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(k, v)| format!("\"{k}\":{}", v.as_str()))
            .collect::<Vec<_>>()
            .join(",")
    ))
    .map_err(|_| MaintenanceWireFailure::json())
}
fn array(values: &[CommandJson]) -> Result<CommandJson, MaintenanceWireFailure> {
    CommandJson::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(CommandJson::as_str)
            .collect::<Vec<_>>()
            .join(",")
    ))
    .map_err(|_| MaintenanceWireFailure::json())
}
fn text(v: &CommandJson, name: &str) -> Result<String, MaintenanceWireFailure> {
    property(v, name)
        .and_then(|x| x.string_utf8_storage().ok().flatten())
        .ok_or_else(MaintenanceWireFailure::json)
}
fn int(v: &CommandJson, name: &str) -> Result<i64, MaintenanceWireFailure> {
    property(v, name)
        .and_then(|x| x.as_str().parse().ok())
        .ok_or_else(MaintenanceWireFailure::json)
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
pub fn normalize_maintenance_change(
    bytes: &[u8],
) -> Result<NormalizedMaintenanceChange, MaintenanceWireFailure> {
    let body = document(bytes)?;
    let action = property(&body, "action")
        .and_then(|x| x.string_utf8_storage().ok().flatten())
        .filter(|s| {
            [
                "schedule",
                "start",
                "block",
                "resume",
                "complete",
                "issue-part",
                "cancel",
            ]
            .contains(&s.as_str())
        })
        .ok_or_else(|| MaintenanceWireFailure::validation("invalid_union", field("action")))?;
    let mut check = Validation::default();
    let mut fields = vec![(
        "action",
        CommandJson::from_serializable(&action).map_err(|_| MaintenanceWireFailure::json())?,
    )];
    if let Some(v) = check.integer(
        property(&body, "expectedVersion"),
        &field("expectedVersion"),
        false,
    ) {
        fields.push(("expectedVersion", v));
    }
    match action.as_str() {
        "schedule" => {
            if let Some(v) = check.string(
                property(&body, "ownerId"),
                &field("ownerId"),
                1,
                None,
                false,
            ) {
                fields.push(("ownerId", v));
            }
            if let Some(v) =
                check.integer(property(&body, "scheduledAt"), &field("scheduledAt"), false)
            {
                fields.push(("scheduledAt", v));
            }
        }
        "complete" => {
            if let Some(v) = check.enumeration(
                property(&body, "resultingCondition"),
                &field("resultingCondition"),
                &["good", "degraded", "unsafe"],
            ) {
                fields.push(("resultingCondition", v));
            }
            if let Some(v) = check.string(
                property(&body, "evidence"),
                &field("evidence"),
                3,
                Some(4000),
                true,
            ) {
                fields.push(("evidence", v));
            }
        }
        "issue-part" => {
            for name in ["warehouseId", "productId"] {
                if let Some(v) = check.string(property(&body, name), &field(name), 1, None, false) {
                    fields.push((name, v));
                }
            }
            if let Some(v) = check.integer(
                property(&body, "quantityMilli"),
                &field("quantityMilli"),
                true,
            ) {
                fields.push(("quantityMilli", v));
            }
            if let Some(v) = check.date(property(&body, "entryDate"), &field("entryDate")) {
                fields.push(("entryDate", v));
            }
            if let Some(v) = check.allocations(property(&body, "allocations")) {
                fields.push(("allocations", v));
            }
        }
        _ => {}
    }
    if let Some(v) = check.string(
        property(&body, "reason"),
        &field("reason"),
        3,
        Some(2000),
        true,
    ) {
        fields.push(("reason", v));
    }
    let json = check.finish(&fields)?;
    let action = match action.as_str() {
        "schedule" => MaintenanceChangeAction::Schedule {
            owner_id: text(&json, "ownerId")?,
            scheduled_at: int(&json, "scheduledAt")?,
        },
        "start" => MaintenanceChangeAction::Start,
        "block" => MaintenanceChangeAction::Block,
        "resume" => MaintenanceChangeAction::Resume,
        "cancel" => MaintenanceChangeAction::Cancel,
        "complete" => MaintenanceChangeAction::Complete {
            resulting_condition: text(&json, "resultingCondition")?,
            evidence: text(&json, "evidence")?,
        },
        "issue-part" => {
            let values = property(&json, "allocations")
                .and_then(|x| x.array_elements().ok().flatten())
                .ok_or_else(MaintenanceWireFailure::json)?;
            let mut allocations = vec![];
            for lot in values {
                allocations.push(MaintenanceLotInput {
                    lot_id: text(&lot, "lotId")?,
                    quantity_milli: int(&lot, "quantityMilli")?,
                    balance_version: int(&lot, "balanceVersion")?,
                });
            }
            MaintenanceChangeAction::IssuePart {
                warehouse_id: text(&json, "warehouseId")?,
                product_id: text(&json, "productId")?,
                quantity_milli: int(&json, "quantityMilli")?,
                entry_date: text(&json, "entryDate")?,
                allocations,
            }
        }
        _ => return Err(MaintenanceWireFailure::json()),
    };
    Ok(NormalizedMaintenanceChange::new(
        MaintenanceChangeInput {
            expected_version: int(&json, "expectedVersion")?,
            reason: text(&json, "reason")?,
            action,
        },
        json.as_str().to_owned(),
    ))
}
pub fn normalize_maintenance_create(
    bytes: &[u8],
) -> Result<NormalizedMaintenanceCreate, MaintenanceWireFailure> {
    let body = document(bytes)?;
    let mut check = Validation::default();
    let mut fields = vec![];
    for name in [
        "assetId",
        "assetVersion",
        "title",
        "priority",
        "ownerId",
        "scheduledAt",
        "reason",
    ] {
        let raw = property(&body, name);
        let value = match name {
            "assetVersion" => check.integer(raw, &field(name), false),
            "scheduledAt" => match raw {
                None => None,
                Some(v) if v.as_str() == "null" => Some(v),
                Some(v) => check.integer(Some(v), &field(name), false),
            },
            "priority" => check.enumeration(raw, &field(name), &["normal", "high", "critical"]),
            "title" => check.string(raw, &field(name), 3, Some(160), true),
            "reason" => check.string(raw, &field(name), 3, Some(2000), true),
            _ => check.string(raw, &field(name), 1, None, false),
        };
        if let Some(value) = value {
            fields.push((name, value));
        }
    }
    let json = check.finish(&fields)?;
    let scheduled_at = match property(&json, "scheduledAt") {
        None => None,
        Some(v) if v.as_str() == "null" => None,
        Some(_) => Some(int(&json, "scheduledAt")?),
    };
    Ok(NormalizedMaintenanceCreate::new(
        MaintenanceCreateInput {
            asset_id: text(&json, "assetId")?,
            asset_version: int(&json, "assetVersion")?,
            title: text(&json, "title")?,
            priority: text(&json, "priority")?,
            owner_id: text(&json, "ownerId")?,
            scheduled_at,
            reason: text(&json, "reason")?,
        },
        json.as_str().to_owned(),
    ))
}
