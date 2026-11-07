//! Native CRM account create validation (owner-local rows only).
//!
//! Only the `create-account` slice is implemented here. `link-finance` and
//! `link-guest` remain IdentityAccess-owned in TypeScript per the CRM plan and
//! are intentionally not modelled. This module validates bounded input and
//! produces a normalized plan; it reads no table, performs no purchase or
//! cross-owner join, and emits no command receipt.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};

const NAME_LIMIT: usize = 160;

/// Normalized `create-account` wire input. Field names match the TS
/// `crmIdentityCommand` schema (`action`, `kind`, `name`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountCreateInput {
    pub action: String,
    pub kind: String,
    pub name: String,
}

/// Validated, normalized plan consumed by the PG adapter. `name` is the
/// trimmed value; `kind` is `business` or `individual`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccountCreatePlan {
    pub kind: String,
    pub name: String,
}

pub fn account_create_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_ACCOUNT_INPUT_INVALID",
        "Use a bounded CRM account name of 2 to 160 characters.",
    )
}

pub fn account_kind_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_ACCOUNT_KIND_INVALID",
        "Choose a business or individual account kind.",
    )
}

pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| {
        matches!(c,
            '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
            '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}

pub fn validate_account_create(
    input: &AccountCreateInput,
) -> Result<AccountCreatePlan, BusinessError> {
    if input.action != "create-account" {
        return Err(BusinessError::invalid(
            "CRM_ACCOUNT_ACTION_INVALID",
            "Only create-account is supported by this slice.",
        ));
    }
    if !matches!(input.kind.as_str(), "business" | "individual") {
        return Err(account_kind_invalid());
    }
    let name = js_trim(&input.name).to_owned();
    // Zod's string length uses JavaScript UTF-16 code units, not UTF-8 bytes
    // or Unicode scalar values. Preserve the existing identity contract.
    if !(2..=NAME_LIMIT).contains(&name.encode_utf16().count()) {
        return Err(account_create_invalid());
    }
    Ok(AccountCreatePlan {
        kind: input.kind.clone(),
        name,
    })
}

pub fn record_id_for_key(key: &str) -> String {
    format!("account-{key}")
}

pub const CREATE_ACCOUNT_OPERATION: &str = "crm.identity.create-account";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(value: serde_json::Value) -> AccountCreateInput {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn accepts_trimmed_business_and_individual_names() {
        let plan = validate_account_create(&input(json!({
            "action": "create-account", "kind": "business", "name": "  Acme Pty Ltd  "
        })))
        .unwrap();
        assert_eq!(plan.kind, "business");
        assert_eq!(plan.name, "Acme Pty Ltd");
    }

    #[test]
    fn rejects_empty_names() {
        let empty = validate_account_create(&input(json!({
            "action": "create-account", "kind": "individual", "name": "   "
        })));
        assert!(empty.is_err());
    }

    #[test]
    fn rejects_unknown_kind_and_action() {
        let kind = validate_account_create(&input(json!({
            "action": "create-account", "kind": "organization", "name": "Valid name"
        })));
        assert_eq!(kind.unwrap_err().code, "CRM_ACCOUNT_KIND_INVALID");
        let action = validate_account_create(&input(json!({
            "action": "link-finance", "kind": "business", "name": "Valid name"
        })));
        assert_eq!(action.unwrap_err().code, "CRM_ACCOUNT_ACTION_INVALID");
    }

    #[test]
    fn record_id_stays_in_ts_namespace() {
        assert_eq!(record_id_for_key("key-1"), "account-key-1");
        assert_eq!(CREATE_ACCOUNT_OPERATION, "crm.identity.create-account");
    }

    #[test]
    fn failure_kind_is_invalid() {
        use erpx_server_shared::public::FailureKind;
        let err = account_create_invalid();
        assert_eq!(err.kind, FailureKind::InvalidInput);
    }
}
