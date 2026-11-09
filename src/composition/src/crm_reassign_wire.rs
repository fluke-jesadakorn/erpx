//! Lossless reassign receipt identity before Workflow replay. This does not
//! establish actor, source, recipient eligibility, purchase or write authority.
use erpx_server_crm::public::activities_native::normalize_activity_reassign_utf16;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;

pub struct CrmReassignWire {
    pub target_utf16: Vec<u16>,
    pub input: CommandJson,
    /// A physical text lookup could alias after UTF-8 replacement; authorize and
    /// check historical replay before deciding how to reject such a new write.
    pub unsafe_lookup: bool,
}
fn invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Review the CRM activity input.")
}
fn string(source: &CommandJson, key: &str) -> Result<Vec<u16>, BusinessError> {
    source
        .property(key)?
        .ok_or_else(invalid)?
        .string_utf16_units()?
        .ok_or_else(invalid)
}
pub fn parse_crm_reassign_wire(body: &[u8]) -> Result<CrmReassignWire, BusinessError> {
    // Preserve the existing Workflow input resource bound before decoding.
    // This intentionally remains a documented mismatch with legacy Request.json.
    if body.len() > 2 * 1024 * 1024 {
        return Err(invalid());
    }
    // Pinned Node Request.json decodes with replacement and accepts up to two leading BOMs.
    let decoded = String::from_utf8_lossy(body);
    let text = decoded.strip_prefix('\u{feff}').unwrap_or(&decoded);
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let source = CommandJson::parse(text)?;
    let target = string(&source, "id")?;
    let action = string(&source, "action")?;
    let owner = string(&source, "ownerId")?;
    let date = string(&source, "dueDate")?;
    let reason = string(&source, "reason")?;
    let version_value = source.property("expectedVersion")?.ok_or_else(invalid)?;
    let version: serde_json::Value =
        serde_json::from_str(version_value.as_str()).map_err(|_| invalid())?;
    let n = version.as_f64().ok_or_else(invalid)?;
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=9_007_199_254_740_991.0).contains(&n) {
        return Err(invalid());
    }
    let normalized = normalize_activity_reassign_utf16(&action, n as i64, &owner, &date, &reason)?;
    let owner_json = CommandJson::from_utf16_string(normalized.owner_id())?;
    let date_json = CommandJson::from_utf16_string(normalized.due_date())?;
    let reason_json = CommandJson::from_utf16_string(normalized.reason())?;
    let input = CommandJson::parse(&format!(
        r#"{{"action":"reassign","expectedVersion":{},"ownerId":{},"dueDate":{},"reason":{}}}"#,
        normalized.expected_version(),
        owner_json.as_str(),
        date_json.as_str(),
        reason_json.as_str()
    ))?;
    let unsafe_lookup = target.contains(&0)
        || owner.contains(&0)
        || char::decode_utf16(target.iter().copied()).any(|x| x.is_err())
        || char::decode_utf16(owner.iter().copied()).any(|x| x.is_err());
    Ok(CrmReassignWire {
        target_utf16: target,
        input,
        unsafe_lookup,
    })
}
