//! Exact first message retained at its owner validation site.
use super::parts_wire::FieldServiceWireFailure;
pub fn field_service_first_wire_message(
    _bytes: &[u8],
    failure: &FieldServiceWireFailure,
) -> Option<String> {
    if failure.invalid_json {
        return None;
    }
    failure.issues.first().map(|issue| issue.message.clone())
}
