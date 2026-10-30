//! Wire-only contracts for the private Field visit attachment intake API.
use serde::{Deserialize, Deserializer, Serialize};
macro_rules! dto { ($name:ident { $($(#[$attr:meta])* $field:ident : $ty:ty),* $(,)? }) => {
 #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
 #[cfg_attr(feature="contracts", derive(ts_rs::TS, utoipa::ToSchema))]
 #[serde(rename_all="camelCase", deny_unknown_fields)]
 pub struct $name { $($(#[$attr])* pub $field:$ty),* }
}; }
macro_rules! wire_enum { ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
 #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
 #[cfg_attr(feature="contracts", derive(ts_rs::TS, utoipa::ToSchema))]
 pub enum $name { $(#[serde(rename=$wire)] $variant),+ }
}; }
wire_enum!(FieldIntakeTarget { FieldVisit => "field_service_visit" });
wire_enum!(FieldIntakeComponent { Record => "record", Action => "action", Line => "line", Event => "event", Message => "message", Publication => "publication" });
wire_enum!(FieldIntakeState { Draft => "draft", Bound => "bound", Withdrawn => "withdrawn", Expired => "expired" });
wire_enum!(FieldIntakeClassification { Public => "public", Internal => "internal", Confidential => "confidential", Restricted => "restricted" });
wire_enum!(FieldIntakeSourceState { LegacyUnscanned => "legacy_unscanned", Quarantined => "quarantined", Scanning => "scanning", Available => "available", ScanError => "scan_error", MalwareRejected => "malware_rejected", Destroyed => "destroyed" });
fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
fn optional_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct FieldIntakeCreateInput {
    pub target_type: FieldIntakeTarget,
    pub target_id: String,
    pub component_kind: FieldIntakeComponent,
    #[serde(
        default,
        deserialize_with = "optional_string",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "contracts", ts(optional), schema(nullable = false))]
    pub component_ref: Option<String>,
    pub purpose_key: String,
    #[cfg_attr(
        feature = "contracts",
        ts(type = "number"),
        schema(minimum = 1, maximum = 9007199254740991i64)
    )]
    pub expected_target_version: i64,
}
dto!(FieldIntakeLink {
 id:String, #[serde(deserialize_with="required_nullable")] source_id:Option<String>, filename:String, media_type:String,
 #[serde(deserialize_with="required_nullable")] #[cfg_attr(feature="contracts", ts(type="number | null"))] bytes:Option<i64>,
 #[serde(deserialize_with="required_nullable")] source_state:Option<FieldIntakeSourceState>, link_state:FieldIntakeState, #[serde(deserialize_with="required_nullable")] caption:Option<String>, preview_available:bool,
 #[cfg_attr(feature="contracts", ts(type="number"))] created_at:i64,
 #[cfg_attr(feature="contracts", ts(type="number"))] expires_at:i64,
 #[serde(deserialize_with="required_nullable")] #[cfg_attr(feature="contracts", ts(type="number | null"))] bound_at:Option<i64>
});
dto!(FieldIntakeSet {
 id:String, target_type:FieldIntakeTarget, target_id:String, component_kind:FieldIntakeComponent, #[serde(deserialize_with="required_nullable")] component_ref:Option<String>,
 purpose_key:String,
 #[cfg_attr(feature="contracts", ts(type="number"))] expected_target_version:i64,
 #[cfg_attr(feature="contracts", ts(type="number"))] policy_version:i64,
 classification:FieldIntakeClassification,
 #[cfg_attr(feature="contracts", ts(type="number"))] required_count:i64,
 #[cfg_attr(feature="contracts", ts(type="number"))] max_files:i64,
 state:FieldIntakeState,
 #[cfg_attr(feature="contracts", ts(type="number"))] created_at:i64,
 #[cfg_attr(feature="contracts", ts(type="number"))] updated_at:i64,
 links:Vec<FieldIntakeLink>
});
dto!(FieldIntakeListResponse { data:Vec<FieldIntakeSet> });
dto!(FieldIntakeCreateResponse {
    data: FieldIntakeSet
});
dto!(FieldIntakeError {
    code: String,
    message: String
});
dto!(FieldIntakeErrorResponse {
    error: FieldIntakeError
});
#[derive(Debug)]
pub struct FieldIntakeWireError;

/// Validate a stored projection before its transaction can commit or be served.
pub fn checked_field_intake_set(
    value: serde_json::Value,
) -> Result<serde_json::Value, FieldIntakeWireError> {
    const LIMIT: i64 = 9_007_199_254_740_991;
    fn safe(value: i64) -> bool {
        (-LIMIT..=LIMIT).contains(&value)
    }
    let set: FieldIntakeSet = serde_json::from_value(value).map_err(|_| FieldIntakeWireError)?;
    if ![
        set.expected_target_version,
        set.policy_version,
        set.required_count,
        set.max_files,
        set.created_at,
        set.updated_at,
    ]
    .into_iter()
    .all(safe)
        || set.links.iter().any(|link| {
            !safe(link.created_at)
                || !safe(link.expires_at)
                || link.bytes.is_some_and(|v| !safe(v))
                || link.bound_at.is_some_and(|v| !safe(v))
        })
    {
        return Err(FieldIntakeWireError);
    }
    serde_json::to_value(set).map_err(|_| FieldIntakeWireError)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_field_set_wire_roundtrips_all_fields() {
        let value = serde_json::json!({"data":{"id":"s","targetType":"field_service_visit","targetId":"v","componentKind":"record","componentRef":null,"purposeKey":"site_photo","expectedTargetVersion":3,"policyVersion":1,"classification":"internal","requiredCount":0,"maxFiles":10,"state":"draft","createdAt":123,"updatedAt":456,"links":[{"id":"l","sourceId":null,"filename":"file","mediaType":"application/octet-stream","bytes":null,"sourceState":null,"linkState":"draft","caption":null,"previewAvailable":false,"createdAt":123,"expiresAt":456,"boundAt":null}]}});
        let parsed: FieldIntakeCreateResponse = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
        let mut extra = value.clone();
        extra["data"]["unknown"] = serde_json::json!(1);
        assert!(serde_json::from_value::<FieldIntakeCreateResponse>(extra).is_err());
        let mut missing = value;
        missing["data"]
            .as_object_mut()
            .unwrap()
            .remove("componentRef");
        assert!(serde_json::from_value::<FieldIntakeCreateResponse>(missing).is_err());
    }
    #[test]
    fn optional_component_reference_cannot_be_null() {
        let body = serde_json::json!({"targetType":"field_service_visit","targetId":"v","componentKind":"record","purposeKey":"site_photo","expectedTargetVersion":1});
        assert!(serde_json::from_value::<FieldIntakeCreateInput>(body.clone()).is_ok());
        let mut null = body;
        null["componentRef"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<FieldIntakeCreateInput>(null).is_err());
    }
}
