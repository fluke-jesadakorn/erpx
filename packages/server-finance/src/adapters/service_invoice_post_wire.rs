//! Existing invoice-post schema order and logical/physical string separation.
use crate::domain::service_invoice_posting::validate_service_invoice_post_request;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Map, Value};
const KEYS: [&str; 7] = [
    "periodId",
    "entryDate",
    "expectedVersion",
    "fxRate",
    "attachmentSetIds",
    "fxGainAccountId",
    "fxLossAccountId",
];
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Posting period, date and a valid exchange rate are required.",
    )
}
/// Physical decoding never replaces the logical CommandJson used for receipts.
pub fn service_invoice_post_physical_input(doc: &CommandJson) -> Result<Value, BusinessError> {
    if doc.object_keys()?.is_none() {
        return Err(invalid());
    }
    let mut values = Map::new();
    for key in KEYS {
        let Some(value) = doc.property(key)? else {
            continue;
        };
        let physical = if key == "expectedVersion" {
            let value: Value = serde_json::from_str(value.as_str()).map_err(|_| invalid())?;
            let number = value.as_f64().ok_or_else(invalid)?;
            if !number.is_finite()
                || number.fract() != 0.0
                || !(1.0..=9_007_199_254_740_991.0).contains(&number)
            {
                return Err(invalid());
            }
            Value::from(number as i64)
        } else if key == "attachmentSetIds" {
            let items = value.array_elements()?.ok_or_else(invalid)?;
            if items.len() > 20 {
                return Err(invalid());
            }
            Value::Array(
                items
                    .into_iter()
                    .map(|item| {
                        let text = item.string_utf8_storage()?.ok_or_else(invalid)?;
                        if text.is_empty() {
                            return Err(invalid());
                        }
                        Ok(Value::String(text))
                    })
                    .collect::<Result<_, BusinessError>>()?,
            )
        } else {
            let text = value.string_utf8_storage()?.ok_or_else(invalid)?;
            if text.is_empty() {
                return Err(invalid());
            }
            Value::String(text)
        };
        values.insert(key.into(), physical);
    }
    let text = |k: &str| values.get(k).and_then(Value::as_str).ok_or_else(invalid);
    let attachments = values
        .get("attachmentSetIds")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .map(|s| s.as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    validate_service_invoice_post_request(
        text("periodId")?,
        text("entryDate")?,
        values.get("expectedVersion").and_then(Value::as_i64),
        values.get("fxRate").and_then(Value::as_str),
        &attachments,
    )?;
    Ok(Value::Object(values))
}
/// Public REST requires expectedVersion. Unknown keys are stripped in the same
/// schema order as the retained parser, without losing lone UTF-16 identity.
pub fn normalize_service_invoice_post(body: &[u8]) -> Result<CommandJson, BusinessError> {
    let text = String::from_utf8_lossy(body);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let doc = CommandJson::parse(text)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    let physical = service_invoice_post_physical_input(&doc)?;
    if physical.get("expectedVersion").is_none() {
        return Err(invalid());
    }
    let mut members = Vec::new();
    for key in KEYS {
        if let Some(value) = doc.property(key)? {
            members.push(format!("\"{key}\":{}", value.as_str()));
        }
    }
    CommandJson::parse(&format!("{{{}}}", members.join(",")))
}
