//! HTTP command normalization for the native Asset register.
//!
//! The route selects create or change from the body's optional `id`. All
//! business defaults and bounds stay in the owner domain/application; this
//! module only rejects an ambiguous transport shape before authorization.
use crate::domain::asset::{
    AssetAction, AssetChangeInput, AssetCostInput, NewAsset, parse_asset_cost, validate_reason,
};
use erpx_server_shared::public::BusinessError;
use serde::Serializer;
use serde::ser::SerializeMap;
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

/// Decode the retained route selector: any string `id` selects change;
/// missing, null or a non-string value selects registration. Unknown fields
/// are discarded by the owner parsers.
pub fn decode_asset_command(raw: &Value) -> Result<AssetHttpCommand, BusinessError> {
    let object = raw.as_object().ok_or_else(invalid)?;
    match object.get("id") {
        Some(Value::String(id)) => {
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
        _ => {
            parse_asset_cost(raw)?;
            NewAsset::parse(raw, 0)?;
            Ok(AssetHttpCommand::Create)
        }
    }
}

/// Serialize the retained Zod-parsed command input in schema order. The HTTP
/// selector `id` and unknown fields are not part of the command fingerprint.
/// This accepts valid Unicode JSON values; the transport must separately
/// preserve lone UTF-16 code units if it accepts them.
pub fn normalized_asset_command_input(raw: &Value) -> Result<String, BusinessError> {
    let command = decode_asset_command(raw)?;
    let invalid =
        || BusinessError::invalid("VALIDATION_FAILED", "Review the requested Asset command.");
    let object = raw.as_object().ok_or_else(invalid)?;
    let mut result = Vec::new();
    let mut serializer = serde_json::Serializer::new(&mut result);
    let mut map = serializer.serialize_map(None).map_err(|_| invalid())?;
    match command {
        AssetHttpCommand::Create => {
            let cost = parse_asset_cost(raw)?;
            let asset = NewAsset::parse(
                raw,
                match &cost {
                    AssetCostInput::Minor(value) => *value,
                    AssetCostInput::Decimal(_) => 0,
                },
            )?;
            map.serialize_entry("branchId", asset.branch_id.as_str())
                .map_err(|_| invalid())?;
            if object.contains_key("departmentId") {
                map.serialize_entry(
                    "departmentId",
                    &asset.department_id.as_ref().map(|v| v.as_str()),
                )
                .map_err(|_| invalid())?;
            }
            map.serialize_entry("code", &asset.code)
                .map_err(|_| invalid())?;
            map.serialize_entry("name", &asset.name)
                .map_err(|_| invalid())?;
            map.serialize_entry("category", &asset.category)
                .map_err(|_| invalid())?;
            map.serialize_entry("currency", &asset.currency)
                .map_err(|_| invalid())?;
            match &cost {
                AssetCostInput::Minor(value) => map
                    .serialize_entry("costMinor", value)
                    .map_err(|_| invalid())?,
                AssetCostInput::Decimal(value) => {
                    map.serialize_entry("cost", value).map_err(|_| invalid())?
                }
            }
            map.serialize_entry("acquiredDate", &asset.acquired_date)
                .map_err(|_| invalid())?;
            if object.contains_key("warrantyEnd") {
                map.serialize_entry("warrantyEnd", &asset.warranty_end)
                    .map_err(|_| invalid())?;
            }
            if let Some(value) = &asset.finance_asset_id {
                map.serialize_entry("financeAssetId", value.as_str())
                    .map_err(|_| invalid())?;
            }
            if let Some(value) = asset.finance_asset_version {
                map.serialize_entry("financeAssetVersion", &value)
                    .map_err(|_| invalid())?;
            }
        }
        AssetHttpCommand::Change { .. } => {
            let change = AssetChangeInput::parse(raw)?;
            let reason = validate_reason(
                object
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?,
            )?;
            map.serialize_entry("action", change.action.name())
                .map_err(|_| invalid())?;
            map.serialize_entry("expectedVersion", &change.expected_version)
                .map_err(|_| invalid())?;
            match change.action {
                AssetAction::Place {
                    placement,
                    custodian_id,
                } => {
                    map.serialize_entry("placement", &placement)
                        .map_err(|_| invalid())?;
                    map.serialize_entry("custodianId", custodian_id.as_str())
                        .map_err(|_| invalid())?;
                }
                AssetAction::Condition { .. } => map
                    .serialize_entry("condition", object.get("condition").ok_or_else(invalid)?)
                    .map_err(|_| invalid())?,
                AssetAction::LinkFinance {
                    finance_asset_id,
                    finance_asset_version,
                } => {
                    map.serialize_entry("financeAssetId", finance_asset_id.as_str())
                        .map_err(|_| invalid())?;
                    map.serialize_entry("financeAssetVersion", &finance_asset_version)
                        .map_err(|_| invalid())?;
                }
                AssetAction::RetireRequest | AssetAction::RetireApprove => {}
            }
            map.serialize_entry("reason", &reason)
                .map_err(|_| invalid())?;
        }
    }
    map.end().map_err(|_| invalid())?;
    String::from_utf8(result).map_err(|_| invalid())
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
