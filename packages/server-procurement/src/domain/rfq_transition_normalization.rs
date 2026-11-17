use crate::domain::rfq_transition::RfqTransitionAction;
use erpx_server_workflow::public::source_ordered_json::{
    SourceOrderedJson, SourceOrderedObject, SourceOrderedObjectValue,
};

/// This identifies invalid input only; caller must preserve the Zod/HTTP error
/// mapping before mounting. It is not a permission or receipt witness.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct InvalidRfqTransition;

/// Constructed only by the source-order normalizer, never by request serde.
pub struct NormalizedRfqTransition {
    json: SourceOrderedJson,
    action: RfqTransitionAction,
}
impl NormalizedRfqTransition {
    pub fn json(&self) -> &SourceOrderedJson {
        &self.json
    }
    pub fn action(&self) -> RfqTransitionAction {
        self.action
    }
}

fn trim_units(v: &[u16]) -> &[u16] {
    fn whitespace(c: u16) -> bool {
        matches!(c,0x09..=0x0d|0x20|0xa0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
    }
    let start = v.iter().position(|&x| !whitespace(x)).unwrap_or(v.len());
    let end = v
        .iter()
        .rposition(|&x| !whitespace(x))
        .map(|x| x + 1)
        .unwrap_or(start);
    &v[start..end]
}
fn length(v: &[u16]) -> usize {
    char::decode_utf16(v.iter().copied()).count()
}

/// Build the Zod object in declaration order, drop unknown keys and retain
/// UTF-16 strings (including unpaired surrogates) without input size caps.
pub fn normalize_rfq_transition(
    raw: &SourceOrderedJson,
) -> Result<NormalizedRfqTransition, InvalidRfqTransition> {
    let root = raw.root();
    if !root.is_object() {
        return Err(InvalidRfqTransition);
    }
    let action = root.property("action").ok_or(InvalidRfqTransition)?;
    let action_text = action
        .utf16()
        .and_then(|v| String::from_utf16(v).ok())
        .ok_or(InvalidRfqTransition)?;
    let action_kind = match action_text.as_str() {
        "submit" => RfqTransitionAction::Submit,
        "approve" => RfqTransitionAction::Approve,
        "return" => RfqTransitionAction::Return,
        "cancel" => RfqTransitionAction::Cancel,
        _ => return Err(InvalidRfqTransition),
    };
    let version = root
        .property("expectedVersion")
        .ok_or(InvalidRfqTransition)?;
    let n = version.number().ok_or(InvalidRfqTransition)?;
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=9_007_199_254_740_991.0).contains(&n) {
        return Err(InvalidRfqTransition);
    }
    let offer = root.property("offerId");
    if let Some(value) = offer {
        let s = value.utf16().ok_or(InvalidRfqTransition)?;
        if s.is_empty() {
            return Err(InvalidRfqTransition);
        }
    }
    let reason = root
        .property("reason")
        .and_then(|v| v.utf16())
        .ok_or(InvalidRfqTransition)?;
    let reason = trim_units(reason);
    if !(3..=2000).contains(&length(reason)) {
        return Err(InvalidRfqTransition);
    }
    let mut output = SourceOrderedObject::new();
    output
        .push("action", SourceOrderedObjectValue::Borrowed(action))
        .map_err(|_| InvalidRfqTransition)?;
    output
        .push(
            "expectedVersion",
            SourceOrderedObjectValue::Borrowed(version),
        )
        .map_err(|_| InvalidRfqTransition)?;
    if let Some(value) = offer {
        output
            .push("offerId", SourceOrderedObjectValue::Borrowed(value))
            .map_err(|_| InvalidRfqTransition)?;
    }
    output
        .push("reason", SourceOrderedObjectValue::Utf16(reason.to_vec()))
        .map_err(|_| InvalidRfqTransition)?;
    let json = SourceOrderedJson::parse(&output.stringify()).map_err(|_| InvalidRfqTransition)?;
    Ok(NormalizedRfqTransition {
        json,
        action: action_kind,
    })
}
