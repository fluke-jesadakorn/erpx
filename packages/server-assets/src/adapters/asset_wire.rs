//! HTTP command normalization for the native Asset register.
//!
//! The route selects create or change from the body's optional `id`. All
//! business defaults and bounds stay in the owner domain/application; this
//! module only rejects an ambiguous transport shape before authorization.
use crate::domain::asset::{AssetChangeInput, NewAsset, validate_reason};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetHttpCommand {
    Create,
    Change { id: String, operation: String },
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ASSET_COMMAND_REQUIRED",
        "Choose a current Asset registration or lifecycle action.",
    )
}

/// Decode one registered Asset command body. `raw` must be a JSON object; an
/// absent/null `id` means registration, a non-empty string means a lifecycle
/// change. Unknown fields are discarded by the owner parsers.
pub fn decode_asset_command(raw: &Value) -> Result<AssetHttpCommand, BusinessError> {
    let object = raw.as_object().ok_or_else(invalid)?;
    match object.get("id") {
        None | Some(Value::Null) => {
            NewAsset::parse(raw, 0)?;
            Ok(AssetHttpCommand::Create)
        }
        Some(Value::String(id)) if !id.is_empty() => {
            let input = AssetChangeInput::parse(raw)?;
            let reason = raw.get("reason").and_then(Value::as_str).ok_or_else(|| {
                BusinessError::invalid(
                    "VALIDATION_FAILED",
                    "Enter a reason between 3 and 2000 characters.",
                )
            })?;
            validate_reason(reason)?;
            Ok(AssetHttpCommand::Change {
                id: id.clone(),
                operation: input.action.operation().to_owned(),
            })
        }
        Some(_) => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn create_and_change_are_selected_by_the_optional_id() {
        let create = json!({
            "branchId":"branch",
            "code":"A-1",
            "name":"Laptop",
            "category":"IT",
            "currency":"thb",
            "costMinor":0,
            "acquiredDate":"2026-01-01"
        });
        assert_eq!(
            decode_asset_command(&create).unwrap(),
            AssetHttpCommand::Create
        );
        assert_eq!(
            decode_asset_command(&json!({
                "id":"asset-1",
                "action":"place",
                "expectedVersion":2,
                "placement":"HQ desk",
                "custodianId":"user-1",
                "reason":"Assign custodian"
            }))
            .unwrap(),
            AssetHttpCommand::Change {
                id: "asset-1".into(),
                operation: "asset.place".into()
            }
        );
    }

    #[test]
    fn ambiguous_ids_and_invalid_actions_fail_before_authorization() {
        assert!(decode_asset_command(&json!({"id":7})).is_err());
        assert!(decode_asset_command(&json!({"id":""})).is_err());
        assert!(
            decode_asset_command(&json!({
                "id":"asset-1",
                "action":"delete",
                "expectedVersion":1,
                "reason":"Remove it"
            }))
            .is_err()
        );
        assert!(
            decode_asset_command(&json!({
                "id":"asset-1",
                "action":"condition",
                "expectedVersion":0,
                "condition":"good",
                "reason":"Inspect it"
            }))
            .is_err()
        );
    }

    #[test]
    fn change_requires_the_retained_reason_bounds() {
        assert!(
            decode_asset_command(&json!({
                "id":"asset-1",
                "action":"condition",
                "expectedVersion":1,
                "condition":"good",
                "reason":"  no  "
            }))
            .is_err()
        );
    }
}
