//! Bounded manual-create transport. The normalized command is retained for
//! receipt identity independently of the storage representation.
use crate::application::journal::JournalDraft;
use crate::domain::journal::JournalLineInput;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
use serde::Deserialize;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ManualJournalCreateWire {
    draft: JournalDraft,
    normalized: CommandJson,
}

#[derive(Clone, Debug)]
pub struct ManualJournalEditWire {
    draft: JournalDraft,
    normalized: CommandJson,
    expected_version: i64,
    reason: String,
}
impl ManualJournalEditWire {
    pub fn draft(&self) -> &JournalDraft {
        &self.draft
    }
    pub fn normalized(&self) -> &CommandJson {
        &self.normalized
    }
    pub fn expected_version(&self) -> i64 {
        self.expected_version
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Strict top-level manual edit fields; line normalization is shared with creation.
pub fn normalize_manual_journal_edit(bytes: &[u8]) -> Result<ManualJournalEditWire, BusinessError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let doc = CommandJson::parse(text)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    struct StrictKeys;
    impl<'de> Deserialize<'de> for StrictKeys {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Keys;
            impl<'de> Visitor<'de> for Keys {
                type Value = StrictKeys;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("an edit object")
                }
                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                    while let Some(key) = map.next_key::<String>()? {
                        if !matches!(
                            key.as_str(),
                            "chartId"
                                | "periodId"
                                | "entryDate"
                                | "currency"
                                | "memo"
                                | "lines"
                                | "expectedVersion"
                                | "reason"
                        ) {
                            return Err(serde::de::Error::custom("unknown edit field"));
                        }
                        map.next_value::<IgnoredAny>()?;
                    }
                    Ok(StrictKeys)
                }
            }
            d.deserialize_map(Keys)
        }
    }
    serde_json::from_str::<StrictKeys>(doc.as_str()).map_err(|_| invalid())?;
    let number = field(&doc, "expectedVersion")?.ok_or_else(invalid)?;
    let number: f64 = number.as_str().parse().map_err(|_| invalid())?;
    if !(1.0..=9_007_199_254_740_990.0).contains(&number) || number.fract() != 0.0 {
        return Err(invalid());
    }
    let expected_version = number as i64;
    let reason_raw = field(&doc, "reason")?.ok_or_else(invalid)?;
    let units = reason_raw
        .string_utf16_units()
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    let whitespace = |u: &u16| matches!(*u, 0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff);
    let start = units
        .iter()
        .position(|u| !whitespace(u))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|u| !whitespace(u))
        .map_or(start, |i| i + 1);
    let trimmed = &units[start..end];
    if !(10..=2000).contains(&trimmed.len()) {
        return Err(invalid());
    }
    let reason = String::from_utf16_lossy(trimmed);
    let escaped_reason = format!(
        "\"{}\"",
        trimmed
            .iter()
            .map(|u| format!("\\u{u:04x}"))
            .collect::<String>()
    );
    let normalized_reason = CommandJson::parse(&escaped_reason)?;
    let create = normalize_manual_journal_create(bytes)?;
    let mut parts = Vec::new();
    for name in [
        "chartId",
        "periodId",
        "entryDate",
        "currency",
        "memo",
        "lines",
    ] {
        let value = field(create.normalized(), name)?.ok_or_else(invalid)?;
        parts.push(format!("\"{name}\":{}", value.as_str()));
    }
    parts.push(format!("\"expectedVersion\":{expected_version}"));
    parts.push(format!("\"reason\":{}", normalized_reason.as_str()));
    let normalized = CommandJson::parse(&format!("{{{}}}", parts.join(",")))?;
    Ok(ManualJournalEditWire {
        draft: create.draft,
        normalized,
        expected_version,
        reason,
    })
}
impl ManualJournalCreateWire {
    pub fn draft(&self) -> &JournalDraft {
        &self.draft
    }
    pub fn normalized(&self) -> &CommandJson {
        &self.normalized
    }
}
fn invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Invalid journal.")
}
fn field(doc: &CommandJson, name: &str) -> Result<Option<CommandJson>, BusinessError> {
    doc.property(name).map_err(|_| invalid())
}
fn string(v: &CommandJson, min: usize, max: usize) -> Result<String, BusinessError> {
    let units = v
        .string_utf16_units()
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    if units.len() < min || units.len() > max {
        return Err(invalid());
    }
    v.string_utf8_storage()
        .map_err(|_| invalid())?
        .ok_or_else(invalid)
}
fn required(
    doc: &CommandJson,
    name: &str,
    min: usize,
    max: usize,
) -> Result<(CommandJson, String), BusinessError> {
    let raw = field(doc, name)?.ok_or_else(invalid)?;
    let value = string(&raw, min, max)?;
    Ok((raw, value))
}
fn decimal(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(a, b)| (a, Some(b)));
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction
            .is_none_or(|v| !v.is_empty() && v.len() <= 6 && v.bytes().all(|b| b.is_ascii_digit()))
}
/// The journal editor supplies dimension codes and values from PostgreSQL
/// UTF-8 text. Unpaired UTF-16 in dimensions is outside this private profile;
/// valid dimension JSON remains source-ordered in the receipt identity.
pub fn normalize_manual_journal_create(
    bytes: &[u8],
) -> Result<ManualJournalCreateWire, BusinessError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let doc = CommandJson::parse(text)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    let (chart_raw, chart_id) = required(&doc, "chartId", 1, 128)?;
    let (period_raw, period_id) = required(&doc, "periodId", 1, 128)?;
    let (date_raw, entry_date) = required(&doc, "entryDate", 10, 10)?;
    if entry_date.len() != 10
        || time::Date::parse(
            &entry_date,
            time::macros::format_description!("[year]-[month]-[day]"),
        )
        .is_err()
    {
        return Err(invalid());
    }
    let (_, currency) = required(&doc, "currency", 3, 3)?;
    let currency = currency.to_uppercase();
    let currency_json = CommandJson::from_serializable(&currency)?;
    let fx_raw = field(&doc, "fxRate")?;
    let fx_rate = fx_raw
        .as_ref()
        .map(|v| string(v, 0, usize::MAX))
        .transpose()?;
    let memo_raw = field(&doc, "memo")?;
    let memo = memo_raw
        .as_ref()
        .map(|v| string(v, 0, 2000))
        .transpose()?
        .unwrap_or_default();
    let memo_json = match &memo_raw {
        Some(v) => v.clone(),
        None => CommandJson::from_serializable(&"")?,
    };
    if let Some(v) = field(&doc, "sourceType")? {
        string(&v, 0, 64)?;
    }
    if let Some(v) = field(&doc, "sourceId")? {
        string(&v, 0, 128)?;
    }
    if let Some(v) = field(&doc, "isOpening")?
        && !matches!(v.as_str(), "true" | "false")
    {
        return Err(invalid());
    }
    let raw_lines = field(&doc, "lines")?
        .ok_or_else(invalid)?
        .array_elements()
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    if !(2..=500).contains(&raw_lines.len()) {
        return Err(invalid());
    }
    let mut lines = Vec::with_capacity(raw_lines.len());
    let mut normalized_lines = Vec::with_capacity(raw_lines.len());
    for row in raw_lines {
        let (account_raw, account_id) = required(&row, "accountId", 1, 128)?;
        let description_raw = field(&row, "description")?;
        let description = description_raw
            .as_ref()
            .map(|v| string(v, 0, 500))
            .transpose()?;
        let (debit_raw, debit) = required(&row, "debit", 1, usize::MAX)?;
        let (credit_raw, credit) = required(&row, "credit", 1, usize::MAX)?;
        if !decimal(&debit) || !decimal(&credit) {
            return Err(invalid());
        }
        let dimensions_raw = field(&row, "dimensions")?;
        let dimensions: Option<BTreeMap<String, String>> = dimensions_raw
            .as_ref()
            .map(|v| serde_json::from_str(v.as_str()).map_err(|_| invalid()))
            .transpose()?;
        if dimensions.as_ref().is_some_and(|map| {
            map.iter()
                .any(|(k, v)| k.encode_utf16().count() > 80 || v.encode_utf16().count() > 160)
        }) {
            return Err(invalid());
        }
        let mut parts = vec![format!("\"accountId\":{}", account_raw.as_str())];
        if let Some(v) = &description_raw {
            parts.push(format!("\"description\":{}", v.as_str()));
        }
        parts.push(format!("\"debit\":{}", debit_raw.as_str()));
        parts.push(format!("\"credit\":{}", credit_raw.as_str()));
        if let Some(v) = &dimensions_raw {
            parts.push(format!("\"dimensions\":{}", v.as_str()));
        }
        normalized_lines.push(format!("{{{}}}", parts.join(",")));
        lines.push(JournalLineInput {
            account_id,
            description,
            debit,
            credit,
            dimensions,
        });
    }
    let fx = fx_raw
        .as_ref()
        .map(|v| format!(",\"fxRate\":{}", v.as_str()))
        .unwrap_or_default();
    let normalized = CommandJson::parse(&format!(
        "{{\"chartId\":{},\"periodId\":{},\"entryDate\":{},\"currency\":{}{},\"memo\":{},\"sourceType\":\"manual\",\"isOpening\":false,\"lines\":[{}]}}",
        chart_raw.as_str(),
        period_raw.as_str(),
        date_raw.as_str(),
        currency_json.as_str(),
        fx,
        memo_json.as_str(),
        normalized_lines.join(",")
    ))?;
    Ok(ManualJournalCreateWire {
        draft: JournalDraft {
            chart_id,
            period_id,
            entry_date,
            currency,
            fx_rate,
            is_opening: false,
            memo,
            source_type: "manual".into(),
            source_id: None,
            lines,
        },
        normalized,
    })
}
