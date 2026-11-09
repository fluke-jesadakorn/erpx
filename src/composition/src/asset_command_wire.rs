//! Lossless request identity for the Assets command transport.
//!
//! Storage text follows Node/PostgreSQL's UTF-8 replacement behavior; receipt
//! identity retains the original UTF-16 units after the legacy Zod transforms.
use erpx_server_assets::public::asset_http::normalized_asset_command_input;
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Map, Value};

const CREATE: &[&str] = &[
    "id",
    "branchId",
    "departmentId",
    "code",
    "name",
    "category",
    "currency",
    "costMinor",
    "cost",
    "acquiredDate",
    "warrantyEnd",
    "financeAssetId",
    "financeAssetVersion",
];
const PLACE: &[&str] = &[
    "id",
    "action",
    "expectedVersion",
    "placement",
    "custodianId",
    "reason",
];
const CONDITION: &[&str] = &["id", "action", "expectedVersion", "condition", "reason"];
const RETIRE: &[&str] = &["id", "action", "expectedVersion", "reason"];
const LINK: &[&str] = &[
    "id",
    "action",
    "expectedVersion",
    "financeAssetId",
    "financeAssetVersion",
    "reason",
];
const IDENTITIES: &[&str] = &[
    "id",
    "branchId",
    "departmentId",
    "code",
    "financeAssetId",
    "custodianId",
];
const TRIMMED: &[&str] = &["code", "name", "category", "placement", "reason"];

fn invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Review the requested Asset command.")
}
fn trimmed(units: &[u16]) -> &[u16] {
    let white = |c: &u16| matches!(*c, 0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff);
    let first = units.iter().position(|c| !white(c)).unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|c| !white(c))
        .map_or(first, |i| i + 1);
    &units[first..end]
}

pub(crate) struct AssetCommandWire {
    pub storage: Value,
    pub input: CommandJson,
    pub id_utf16: Option<Vec<u16>>,
    pub unsafe_identity: bool,
}

pub(crate) fn parse_asset_command_wire(body: &[u8]) -> Result<AssetCommandWire, BusinessError> {
    let text = std::str::from_utf8(body).map_err(|_| invalid())?;
    let source = CommandJson::parse(text)?;
    let id = source.property("id")?;
    let id_units = id
        .as_ref()
        .map(CommandJson::string_utf16_units)
        .transpose()?
        .flatten();
    let is_create = id_units.is_none();
    let fields = if is_create {
        CREATE
    } else {
        let action = source
            .property("action")?
            .ok_or_else(invalid)?
            .string_utf8_storage()?
            .ok_or_else(invalid)?;
        match action.as_str() {
            "place" => PLACE,
            "condition" => CONDITION,
            "retire-request" | "retire-approve" => RETIRE,
            "link-finance" => LINK,
            _ => return Err(invalid()),
        }
    };
    let mut storage = Map::new();
    let mut unsafe_identity = false;
    let mut id_utf16 = None;
    for key in fields {
        // The legacy route dispatches non-string IDs to create; Zod strips
        // `id` from that command instead of validating the selector as input.
        if is_create && *key == "id" {
            continue;
        }
        let Some(value) = source.property(key)? else {
            continue;
        };
        let converted = if let Some(units) = value.string_utf16_units()? {
            if *key == "id" {
                id_utf16 = Some(units.clone());
            }
            if IDENTITIES.contains(key) {
                unsafe_identity |= units.contains(&0)
                    || char::decode_utf16(units.iter().copied()).any(|item| item.is_err());
            }
            Value::String(String::from_utf16_lossy(&units))
        } else {
            serde_json::from_str(value.as_str()).map_err(|_| invalid())?
        };
        storage.insert((*key).to_owned(), converted);
    }
    let storage = Value::Object(storage);
    let normalized = CommandJson::parse(&normalized_asset_command_input(&storage)?)?;
    let keys = normalized.object_keys()?.ok_or_else(invalid)?;
    let mut encoded = String::from("{");
    for (index, key) in keys.iter().enumerate() {
        if index != 0 {
            encoded.push(',');
        }
        encoded.push_str(&serde_json::to_string(key).map_err(|_| invalid())?);
        encoded.push(':');
        let original = source.property(key)?;
        let original_units = original
            .as_ref()
            .map(CommandJson::string_utf16_units)
            .transpose()?
            .flatten();
        if let Some(units) = original_units.filter(|_| key != "currency") {
            let units = if TRIMMED.contains(&key.as_str()) {
                trimmed(&units)
            } else {
                &units
            };
            encoded.push_str(CommandJson::from_utf16_string(units)?.as_str());
        } else {
            encoded.push_str(normalized.property(key)?.ok_or_else(invalid)?.as_str());
        }
    }
    encoded.push('}');
    Ok(AssetCommandWire {
        storage,
        input: CommandJson::parse(&encoded)?,
        id_utf16,
        unsafe_identity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_surrogates_in_receipt_and_replacement_in_storage() {
        let wire = parse_asset_command_wire(br#"{"branchId":"b","code":"A","name":"  \ud800xy  ","category":"IT","currency":"usd","costMinor":0,"acquiredDate":"2026-01-01","ignored":"\ud800"}"#).unwrap();
        assert_eq!(wire.storage["name"], "  �xy  ");
        assert_eq!(
            wire.input
                .property("name")
                .unwrap()
                .unwrap()
                .string_utf16_units()
                .unwrap()
                .unwrap(),
            vec![0xd800, b'x' as u16, b'y' as u16]
        );
        assert!(!wire.unsafe_identity);
    }
    #[test]
    fn identifies_lossy_lookup_fields_without_rejecting_replay() {
        let wire = parse_asset_command_wire(br#"{"branchId":"\ud800","code":"A","name":"Laptop","category":"IT","currency":"USD","costMinor":0,"acquiredDate":"2026-01-01"}"#).unwrap();
        assert!(wire.unsafe_identity);
    }
    #[test]
    fn ignores_other_variant_fields_even_when_they_contain_surrogates() {
        let wire = parse_asset_command_wire(br#"{"branchId":"b","code":"A","name":"Laptop","category":"IT","currency":"USD","costMinor":0,"acquiredDate":"2026-01-01","custodianId":"\ud800","action":{"x":"\ud800"}}"#).unwrap();
        assert!(!wire.unsafe_identity);
        assert!(wire.input.property("action").unwrap().is_none());
    }
    #[test]
    fn non_string_selector_is_stripped_from_create_input() {
        let wire = parse_asset_command_wire(br#"{"id":{"nested":"\ud800"},"branchId":"b","code":"A","name":"Laptop","category":"IT","currency":"USD","costMinor":0,"acquiredDate":"2026-01-01"}"#).unwrap();
        assert!(wire.storage.get("id").is_none());
        assert!(wire.input.property("id").unwrap().is_none());
        assert!(!wire.unsafe_identity);
    }
    #[test]
    fn selectors_and_duplicate_keys_follow_legacy_dispatch() {
        for id in ["false", "0", "[]", "null"] {
            let raw = format!(
                r#"{{"id":{id},"branchId":"b","code":"A","name":"Laptop","category":"IT","currency":"USD","costMinor":0,"acquiredDate":"2026-01-01"}}"#
            );
            assert!(
                parse_asset_command_wire(raw.as_bytes())
                    .unwrap()
                    .storage
                    .get("id")
                    .is_none()
            );
        }
        let change = parse_asset_command_wire(br#"{"id":false,"id":"asset-1","action":"place","expectedVersion":1,"placement":"Desk","custodianId":"u","reason":"Place it","branchId":"\ud800"}"#).unwrap();
        assert_eq!(
            change.id_utf16.unwrap(),
            "asset-1".encode_utf16().collect::<Vec<_>>()
        );
        assert!(!change.unsafe_identity);
        assert!(change.input.property("branchId").unwrap().is_none());
        let empty = parse_asset_command_wire(
            br#"{"id":"","action":"retire-request","expectedVersion":1,"reason":"Retire it"}"#,
        )
        .unwrap();
        assert_eq!(empty.id_utf16.unwrap(), Vec::<u16>::new());
    }
}
