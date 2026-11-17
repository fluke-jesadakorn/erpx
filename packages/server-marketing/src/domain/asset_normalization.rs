//! Scoped Marketing asset input normalization; no command or IAM authority.
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::source_ordered_json::{
    SourceOrderedJson, SourceOrderedObject, SourceOrderedObjectValue, SourceOrderedValue,
};
fn invalid(message: &'static str) -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", message)
}
fn presentation(message: &'static str) -> BusinessError {
    BusinessError::invalid("MARKETING_PRESENTATION_INVALID", message)
}
fn trim_units(v: &[u16]) -> &[u16] {
    fn space(c: u16) -> bool {
        matches!(c,0x09..=0x0d|0x20|0xa0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
    }
    let start = v.iter().position(|&c| !space(c)).unwrap_or(v.len());
    let end = v
        .iter()
        .rposition(|&c| !space(c))
        .map(|x| x + 1)
        .unwrap_or(start);
    &v[start..end]
}
fn zod_string_length(v: &[u16]) -> usize {
    char::decode_utf16(v.iter().copied()).count()
}
fn type_error(value: Option<SourceOrderedValue<'_>>, target: &str) -> BusinessError {
    let kind = value.map(|v| v.kind()).unwrap_or("undefined");
    let message = match (target, kind) {
        ("string", "undefined") => "Invalid input: expected string, received undefined",
        ("string", "number") => "Invalid input: expected string, received number",
        ("string", "null") => "Invalid input: expected string, received null",
        ("string", "boolean") => "Invalid input: expected string, received boolean",
        ("string", "array") => "Invalid input: expected string, received array",
        ("string", _) => "Invalid input: expected string, received object",
        ("record", "undefined") => "Invalid input: expected record, received undefined",
        ("record", "array") => "Invalid input: expected record, received array",
        ("record", "null") => "Invalid input: expected record, received null",
        ("record", "number") => "Invalid input: expected record, received number",
        ("record", "boolean") => "Invalid input: expected record, received boolean",
        _ => "Invalid input: expected record, received string",
    };
    invalid(message)
}
fn presentation_validation(v: SourceOrderedValue<'_>) -> Result<(), BusinessError> {
    for key in ["contentAssetId", "contentVersion", "mediaPublicationId"] {
        if let Some(x) = v.property(key) {
            if key == "contentVersion" {
                let n = x.number().ok_or_else(|| {
                    presentation(match x.kind() {
                        "null" => "Invalid input: expected number, received null",
                        "string" => "Invalid input: expected number, received string",
                        "boolean" => "Invalid input: expected number, received boolean",
                        "array" => "Invalid input: expected number, received array",
                        _ => "Invalid input: expected number, received object",
                    })
                })?;
                if n.fract() != 0.0 {
                    return Err(presentation("Invalid input: expected int, received number"));
                }
                if n > 9_007_199_254_740_991.0 {
                    return Err(presentation(
                        "Too big: expected int to be <=9007199254740991",
                    ));
                }
                if n < -9_007_199_254_740_991.0 {
                    return Err(presentation(
                        "Too small: expected int to be >=-9007199254740991",
                    ));
                }
                if n <= 0.0 {
                    return Err(presentation("Too small: expected number to be >0"));
                }
            } else {
                let text = x.utf16().ok_or_else(|| {
                    presentation(match x.kind() {
                        "null" => "Invalid input: expected string, received null",
                        "number" => "Invalid input: expected string, received number",
                        "boolean" => "Invalid input: expected string, received boolean",
                        "array" => "Invalid input: expected string, received array",
                        _ => "Invalid input: expected string, received object",
                    })
                })?;
                if text.is_empty() {
                    return Err(presentation(
                        "Too small: expected string to have >=1 characters",
                    ));
                }
                if zod_string_length(text) > 180 {
                    return Err(presentation(
                        "Too big: expected string to have <=180 characters",
                    ));
                }
            }
        }
    }
    let has_content = v.property("contentAssetId").is_some();
    if has_content != v.property("contentVersion").is_some() {
        return Err(presentation("Choose an exact content revision."));
    }
    if v.property("mediaPublicationId").is_some() && !has_content {
        return Err(presentation(
            "Choose the content owner of the public creative.",
        ));
    }
    Ok(())
}
/// Normalize only after caller performs the original authorization/purchase order.
/// Returns Zod-shaped serialized JSON. The returned value grants no authority.
pub fn normalize_asset_input(raw: &SourceOrderedJson) -> Result<String, BusinessError> {
    let root = raw.root();
    if !root.is_object() {
        return Err(invalid(match root.kind() {
            "array" => "Invalid input: expected object, received array",
            "string" => "Invalid input: expected object, received string",
            "number" => "Invalid input: expected object, received number",
            "boolean" => "Invalid input: expected object, received boolean",
            _ => "Invalid input: expected object, received null",
        }));
    }
    let kind = root.property("kind").and_then(|x| x.utf16());
    let kind = kind
        .and_then(|x| String::from_utf16(x).ok())
        .filter(|x| matches!(x.as_str(), "form" | "content" | "domain" | "provider"))
        .ok_or_else(|| {
            invalid("Invalid option: expected one of \"form\"|\"content\"|\"domain\"|\"provider\"")
        })?;
    let name = root.property("name");
    let name = trim_units(
        name.and_then(|x| x.utf16())
            .ok_or_else(|| type_error(name, "string"))?,
    );
    if zod_string_length(name) < 2 {
        return Err(invalid("Too small: expected string to have >=2 characters"));
    }
    if zod_string_length(name) > 160 {
        return Err(invalid("Too big: expected string to have <=160 characters"));
    }
    let configuration = root.property("configuration");
    let configuration = configuration
        .filter(|v| v.is_object())
        .ok_or_else(|| type_error(configuration, "record"))?;
    let preview = root.property("preview");
    let preview = preview
        .filter(|v| v.is_object())
        .ok_or_else(|| type_error(preview, "record"))?;
    if kind == "form" {
        presentation_validation(configuration)?;
        if configuration
            .property("consentRequired")
            .and_then(|x| x.boolean())
            != Some(true)
        {
            return Err(BusinessError::invalid(
                "MARKETING_CONSENT_REQUIRED",
                "Lead capture forms must require consent before they can be saved.",
            ));
        }
    }
    let mut output = SourceOrderedObject::new();
    output.push(
        "kind",
        SourceOrderedObjectValue::Utf16(kind.encode_utf16().collect()),
    )?;
    output.push("name", SourceOrderedObjectValue::Utf16(name.to_vec()))?;
    output.push(
        "configuration",
        SourceOrderedObjectValue::Borrowed(configuration),
    )?;
    output.push("preview", SourceOrderedObjectValue::Borrowed(preview))?;
    Ok(output.stringify())
}
