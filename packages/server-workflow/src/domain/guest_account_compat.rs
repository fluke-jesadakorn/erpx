//! Narrow compatibility for immutable receipts predating Guest Accounts.
//! No routing, provider or business authority is derived from these aliases.
use super::command_json::{CommandJson, JsonString, OrderedJson};
use erpx_server_shared::public::BusinessError;
use std::collections::HashMap;
const NAMES: [(&str, &str); 8] = [
    ("folio", "guestAccount"),
    ("folios", "guestAccounts"),
    ("folioId", "guestAccountId"),
    ("toFolioId", "toGuestAccountId"),
    ("folioIds", "guestAccountIds"),
    ("folio_id", "guest_account_id"),
    ("folio_entry_id", "guest_account_entry_id"),
    ("folioEntryId", "guestAccountEntryId"),
];
fn operation_units(value: &JsonString, legacy: bool) -> JsonString {
    let mut units = value.units();
    for prefix in ["pms.", ""] {
        let (from, to) = if legacy {
            (format!("{prefix}guest_account."), format!("{prefix}folio."))
        } else {
            (format!("{prefix}folio."), format!("{prefix}guest_account."))
        };
        let from: Vec<_> = from.encode_utf16().collect();
        if units.starts_with(&from) {
            units.splice(..from.len(), to.encode_utf16());
            break;
        }
    }
    let (from, to) = if legacy {
        ("stay.route_guest_account", "stay.route_folio")
    } else {
        ("stay.route_folio", "stay.route_guest_account")
    };
    let from: Vec<_> = from.encode_utf16().collect();
    if let Some(at) = units.windows(from.len()).position(|s| s == from) {
        units.splice(at..at + from.len(), to.encode_utf16());
    }
    JsonString::from_units(units)
}
pub fn legacy_guest_account_operation(value: &str) -> String {
    operation_units(&JsonString::Unicode(value.into()), true)
        .as_str()
        .expect("ASCII replacement preserves valid Unicode")
        .into()
}
pub fn canonical_guest_account_operation(value: &str) -> String {
    operation_units(&JsonString::Unicode(value.into()), false)
        .as_str()
        .expect("ASCII replacement preserves valid Unicode")
        .into()
}
fn mapped_key(key: &JsonString, legacy: bool) -> JsonString {
    let Some(text) = key.as_str() else {
        return key.clone();
    };
    if let Some((a, b)) = NAMES
        .iter()
        .find(|(a, b)| if legacy { *b == text } else { *a == text })
    {
        return JsonString::Unicode(if legacy { a } else { b }.to_string());
    }
    // The captured JS mapping inherits Object.prototype. Retain its actual
    // ToPropertyKey result as data; Rust creates no prototype object/function.
    let inherited = match text {
        "__proto__" => Some("[object Object]".to_string()),
        "constructor" => Some("function Object() { [native code] }".to_string()),
        "__defineGetter__"
        | "__defineSetter__"
        | "hasOwnProperty"
        | "__lookupGetter__"
        | "__lookupSetter__"
        | "isPrototypeOf"
        | "propertyIsEnumerable"
        | "toString"
        | "valueOf"
        | "toLocaleString" => Some(format!("function {text}() {{ [native code] }}")),
        _ => None,
    };
    inherited
        .map(JsonString::Unicode)
        .unwrap_or_else(|| key.clone())
}
fn convert(value: OrderedJson, legacy: bool) -> Result<OrderedJson, BusinessError> {
    Ok(match value {
        OrderedJson::Array(values) => OrderedJson::Array(
            values
                .into_iter()
                .map(|v| convert(v, legacy))
                .collect::<Result<_, _>>()?,
        ),
        OrderedJson::Object(values) => {
            let mut output: Vec<(JsonString, OrderedJson)> = Vec::new();
            let mut positions = HashMap::<JsonString, usize>::new();
            for (key, item) in values {
                let name = mapped_key(&key, legacy);
                let next = match (&item, key.as_str()) {
                    (OrderedJson::String(text), Some("operation" | "eventType")) => {
                        OrderedJson::String(operation_units(text, legacy))
                    }
                    (OrderedJson::String(text), Some("kind" | "recordKind"))
                        if text.as_str()
                            == Some(if legacy { "guest_account" } else { "folio" }) =>
                    {
                        OrderedJson::text(if legacy { "folio" } else { "guest_account" })
                    }
                    (OrderedJson::String(text), Some("section"))
                        if text.as_str()
                            == Some(if legacy { "guest-accounts" } else { "folios" }) =>
                    {
                        OrderedJson::text(if legacy { "folios" } else { "guest-accounts" })
                    }
                    _ => convert(item, legacy)?,
                };
                if let Some(index) = positions.get(&name) {
                    if output[*index].1.encode() != next.encode() {
                        return Err(BusinessError::invalid(
                            "GUEST_ACCOUNT_REFERENCE_CONFLICT",
                            "Historical Guest Account references conflict.",
                        ));
                    }
                    output[*index] = (name, next);
                } else {
                    positions.insert(name.clone(), output.len());
                    output.push((name, next));
                }
            }
            OrderedJson::Object(output)
        }
        other => other,
    })
}
pub fn canonical_guest_account_json(value: &CommandJson) -> Result<CommandJson, BusinessError> {
    CommandJson::from_value(convert(value.value()?, false)?)
}
pub fn legacy_guest_account_json(value: &CommandJson) -> Result<CommandJson, BusinessError> {
    CommandJson::from_value(convert(value.value()?, true)?)
}
