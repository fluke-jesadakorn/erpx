//! Bounded manual-create transport. The normalized command is retained for
//! receipt identity independently of the storage representation.
use crate::application::journal::JournalDraft;
use crate::domain::journal::JournalLineInput;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ManualJournalCreateWire {
    draft: JournalDraft,
    normalized: CommandJson,
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
