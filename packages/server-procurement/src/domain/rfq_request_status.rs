//! RFQ `sourceBlocked` comparison on the Records owner's raw source facts.
use erpx_server_records::public::rfq_request_source::RfqRequestFacts;
use erpx_server_workflow::public::source_ordered_json::SourceOrderedJson;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalJsonError {
    Syntax,
    NullProperty,
}

/// Mirrors the retained JS comparison and short-circuit order. Caller must
/// obtain facts from Records' same-unit witness and preserve the original
/// HTTP error mapping for invalid historical JSON; this does not authorize.
pub fn rfq_source_blocked(
    request_id: Option<&str>,
    status: &str,
    saved_evidence_version: Option<i64>,
    saved_items_json: Option<&str>,
    source: Option<&RfqRequestFacts>,
) -> Result<bool, ApprovalJsonError> {
    if request_id.is_none_or(str::is_empty) || status == "ordered" {
        return Ok(false);
    }
    let Some(source) = source else {
        return Ok(true);
    };
    if source.state != "in_progress"
        || (source.evidence_version as f64) != (saved_evidence_version.unwrap_or(0) as f64)
        || Some(source.items_json.as_str()) != saved_items_json
    {
        return Ok(true);
    }
    let json =
        SourceOrderedJson::parse(&source.approval_json).map_err(|_| ApprovalJsonError::Syntax)?;
    if json.root().kind() == "null" {
        return Err(ApprovalJsonError::NullProperty);
    }
    let approved: Vec<u16> = "approved".encode_utf16().collect();
    Ok(json
        .root()
        .property("decision")
        .and_then(|value| value.utf16())
        != Some(approved.as_slice()))
}
