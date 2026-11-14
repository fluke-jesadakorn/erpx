//! Native CRM account identity validation (owner-local rows only).
//!
//! `create-account` and `link-finance` are modelled here. `create-contact`,
//! `link-guest` and `guest-sharing` remain TypeScript-owned for now. This
//! module validates bounded input and produces normalized plans; it reads no
//! table, performs no Finance/PMS cross-owner join, and emits no command
//! receipt.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

const NAME_LIMIT: usize = 160;
const EVIDENCE_LIMIT: usize = 2_000;
const ID_LIMIT: usize = 200;

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

/// Normalized `link-finance` wire input. The composition supplies the selected
/// reviewer context and the Finance customer witness; this DTO is not
/// authorization.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountLinkFinanceInput {
    pub action: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "customerId")]
    pub customer_id: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    pub evidence: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountLinkFinanceActor {
    pub user_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub can_review: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccountLinkFinancePlan {
    pub account_id: String,
    pub customer_id: String,
    pub expected_version: i64,
    pub evidence: String,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= ID_LIMIT && !value.chars().any(char::is_control)
}

pub fn validate_account_link_finance(
    actor: &AccountLinkFinanceActor,
    input: &AccountLinkFinanceInput,
) -> Result<AccountLinkFinancePlan, BusinessError> {
    if input.action != "link-finance" {
        return Err(BusinessError::invalid(
            "CRM_ACCOUNT_ACTION_INVALID",
            "Only link-finance is supported by this slice.",
        ));
    }
    if !actor.can_review {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_ACCOUNT_REVIEW_REQUIRED",
            "An independent reviewer must link the Finance customer.",
        ));
    }
    let evidence = js_trim(&input.evidence).to_owned();
    if !valid_id(&input.account_id)
        || !valid_id(&input.customer_id)
        || !(1..=9_007_199_254_740_991).contains(&input.expected_version)
        || !(3..=EVIDENCE_LIMIT).contains(&evidence.chars().count())
    {
        return Err(BusinessError::invalid(
            "CRM_ACCOUNT_INPUT_INVALID",
            "Use a bounded Finance customer link with review evidence.",
        ));
    }
    Ok(AccountLinkFinancePlan {
        account_id: input.account_id.clone(),
        customer_id: input.customer_id.clone(),
        expected_version: input.expected_version,
        evidence,
    })
}

pub fn record_id_for_key(key: &str) -> String {
    format!("account-{key}")
}

pub const CREATE_ACCOUNT_OPERATION: &str = "crm.identity.create-account";
pub const LINK_FINANCE_OPERATION: &str = "crm.identity.link-finance";

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
