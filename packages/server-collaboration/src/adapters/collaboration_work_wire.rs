//! HTTP/command JSON normalization. This adapter grants no mutation authority.
use crate::domain::collaboration_work::{CardAction, Change, RelationKind, js_trim, validate_text};
use erpx_server_shared::public::BusinessError;
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceReference};
use serde_json::{Value, json};

/// Normalized input is also the exact input used for the Workflow fingerprint.
/// Unknown JSON fields are discarded, matching the existing object schemas.
pub struct DecodedWorkCommand {
    pub change: Change,
    pub input: Value,
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COLLABORATION_INPUT_INVALID",
        "Provide a valid collaboration value.",
    )
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(invalid)
}
fn positive(value: &Value, key: &str) -> Result<i64, BusinessError> {
    let n = value.get(key).and_then(Value::as_f64).ok_or_else(invalid)?;
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=9_007_199_254_740_991.0).contains(&n) {
        return Err(invalid());
    }
    Ok(n as i64)
}
fn normalized(value: &Value, key: &str, max: usize) -> Result<String, BusinessError> {
    let v = text(value, key)?;
    validate_text(v, 3, max)?;
    Ok(js_trim(v).into())
}
fn reference(value: &Value, kind: &str, id: &str) -> Result<SourceReference, BusinessError> {
    Ok(SourceReference {
        kind: SourceKind::parse(text(value, kind)?)?,
        id: text(value, id)?.into(),
        expected_version: None,
    })
}
/// `operation` is selected by the registered route; `id` comes from its path.
/// Existing records still require classification from their persisted sources.
pub fn decode_work_command(
    operation: &str,
    id: Option<&str>,
    raw: &Value,
) -> Result<DecodedWorkCommand, BusinessError> {
    if !raw.is_object() {
        return Err(invalid());
    }
    let reason = normalized(raw, "reason", 3000)?;
    let (change, input) = match operation {
        "collaboration.relation.create" => {
            let left = reference(raw, "sourceType", "sourceId")?;
            let right = reference(raw, "targetType", "targetId")?;
            let relation_text = text(raw, "relation")?;
            let relation = match relation_text {
                "related" => RelationKind::Related,
                "duplicates" => RelationKind::Duplicates,
                "blocks" => RelationKind::Blocks,
                "depends_on" => RelationKind::DependsOn,
                _ => return Err(invalid()),
            };
            let input = json!({"sourceType":left.kind.as_str(),"sourceId":left.id,"targetType":right.kind.as_str(),"targetId":right.id,"relation":relation_text,"reason":reason});
            (
                Change::CreateRelation {
                    left,
                    right,
                    relation,
                    reason,
                },
                input,
            )
        }
        "collaboration.relation.remove" => {
            let id = id.filter(|v| !v.is_empty()).ok_or_else(invalid)?.to_owned();
            let expected_version = positive(raw, "expectedVersion")?;
            // The original removal validates trim but fingerprints/stores `why`
            // unchanged. Preserve that behavior for historical receipt replay.
            let reason = text(raw, "reason")?.to_owned();
            let input = json!({"expectedVersion":expected_version,"reason":reason});
            (
                Change::RemoveRelation {
                    id,
                    expected_version,
                    reason,
                },
                input,
            )
        }
        "collaboration.action.create" => {
            let source = reference(raw, "sourceType", "sourceId")?;
            let title = normalized(raw, "title", 160)?;
            let owner_id = text(raw, "ownerId")?.to_owned();
            let due_at = positive(raw, "dueAt")?;
            let input = json!({"sourceType":source.kind.as_str(),"sourceId":source.id,"title":title,"ownerId":owner_id,"dueAt":due_at,"reason":reason});
            (
                Change::CreateCard {
                    source,
                    title,
                    owner_id,
                    due_at,
                    reason,
                },
                input,
            )
        }
        "collaboration.action.complete"
        | "collaboration.action.reassign"
        | "collaboration.action.cancel" => {
            let id = id.filter(|v| !v.is_empty()).ok_or_else(invalid)?.to_owned();
            let action_text = text(raw, "action")?;
            if operation != format!("collaboration.action.{action_text}") {
                return Err(invalid());
            }
            let expected_version = positive(raw, "expectedVersion")?;
            let mut input =
                json!({"action":action_text,"expectedVersion":expected_version,"reason":reason});
            let (action, owner_id, due_at, evidence) = match action_text {
                "complete" => {
                    let evidence = normalized(raw, "evidence", 4000)?;
                    input["evidence"] = json!(evidence);
                    (CardAction::Complete, None, None, Some(evidence))
                }
                "reassign" => {
                    let owner = text(raw, "ownerId")?.to_owned();
                    let due = positive(raw, "dueAt")?;
                    input["ownerId"] = json!(owner);
                    input["dueAt"] = json!(due);
                    (CardAction::Reassign, Some(owner), Some(due), None)
                }
                "cancel" => (CardAction::Cancel, None, None, None),
                _ => return Err(invalid()),
            };
            (
                Change::ChangeCard {
                    id,
                    expected_version,
                    action,
                    owner_id,
                    due_at,
                    evidence,
                    reason,
                },
                input,
            )
        }
        _ => return Err(invalid()),
    };
    Ok(DecodedWorkCommand { change, input })
}
