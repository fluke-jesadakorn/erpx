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
pub const CREATE_CONTACT_OPERATION: &str = "crm.identity.create-contact";
pub const LINK_GUEST_OPERATION: &str = "crm.identity.link-guest";
pub const GUEST_SHARING_OPERATION: &str = "crm.identity.guest-sharing";

/// `create-contact` wire input. Field names and validation mirror the TS
/// `crmIdentityCommand` schema exactly: an account id of at least one UTF-16
/// unit, a JS-trimmed name of 2..=160 units and JS-trimmed evidence of
/// 3..=2000 units.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactCreateInput {
    pub action: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    pub name: String,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContactCreatePlan {
    pub account_id: String,
    pub name: String,
    pub detail: String,
}

pub fn identity_input_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_IDENTITY_INPUT_INVALID",
        "Use a bounded CRM identity command.",
    )
}

fn valid_ref(value: &str) -> bool {
    value.encode_utf16().count() >= 1 && !value.chars().any(char::is_control)
}

fn valid_text(value: &str, min: usize, max: usize) -> bool {
    let units = value.encode_utf16().count();
    (min..=max).contains(&units)
}

pub fn validate_contact_create(input: &ContactCreateInput) -> Result<ContactCreatePlan, BusinessError> {
    if input.action != "create-contact" {
        return Err(BusinessError::invalid(
            "CRM_IDENTITY_ACTION_INVALID",
            "Only create-contact is supported by this slice.",
        ));
    }
    let name = js_trim(&input.name).to_owned();
    let detail = js_trim(&input.detail).to_owned();
    if !valid_ref(&input.account_id) || !valid_text(&name, 2, NAME_LIMIT) || !valid_text(&detail, 3, EVIDENCE_LIMIT) {
        return Err(identity_input_invalid());
    }
    Ok(ContactCreatePlan {
        account_id: input.account_id.clone(),
        name,
        detail,
    })
}

/// `link-guest` wire input. A granted sharing evidence also creates the
/// initial `granted` sharing event in the same owner transaction.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestLinkInput {
    pub action: String,
    #[serde(rename = "contactId")]
    pub contact_id: String,
    #[serde(rename = "guestId")]
    pub guest_id: String,
    #[serde(rename = "propertyId")]
    pub property_id: String,
    pub evidence: String,
    #[serde(rename = "sharingEvidence")]
    pub sharing_evidence: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GuestLinkPlan {
    pub contact_id: String,
    pub guest_id: String,
    pub property_id: String,
    pub evidence: String,
    pub sharing_evidence: Option<String>,
}

pub fn validate_guest_link(input: &GuestLinkInput) -> Result<GuestLinkPlan, BusinessError> {
    if input.action != "link-guest" {
        return Err(BusinessError::invalid(
            "CRM_IDENTITY_ACTION_INVALID",
            "Only link-guest is supported by this slice.",
        ));
    }
    let evidence = js_trim(&input.evidence).to_owned();
    let sharing_evidence = input.sharing_evidence.as_deref().map(|value| js_trim(value).to_owned());
    if !valid_ref(&input.contact_id)
        || !valid_ref(&input.guest_id)
        || !valid_ref(&input.property_id)
        || !valid_text(&evidence, 3, EVIDENCE_LIMIT)
        || sharing_evidence
            .as_deref()
            .is_some_and(|value| !valid_text(value, 3, EVIDENCE_LIMIT))
    {
        return Err(identity_input_invalid());
    }
    Ok(GuestLinkPlan {
        contact_id: input.contact_id.clone(),
        guest_id: input.guest_id.clone(),
        property_id: input.property_id.clone(),
        evidence,
        sharing_evidence,
    })
}

/// `guest-sharing` wire input. The expected version is the CAS guard on the
/// retained guest link; composition must first recheck the PMS source.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestSharingInput {
    pub action: String,
    #[serde(rename = "linkId")]
    pub link_id: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    pub state: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GuestSharingPlan {
    pub link_id: String,
    pub expected_version: i64,
    pub state: String,
    pub evidence: String,
    pub version: i64,
}

pub fn validate_guest_sharing(input: &GuestSharingInput) -> Result<GuestSharingPlan, BusinessError> {
    if input.action != "guest-sharing" {
        return Err(BusinessError::invalid(
            "CRM_IDENTITY_ACTION_INVALID",
            "Only guest-sharing is supported by this slice.",
        ));
    }
    let evidence = js_trim(&input.evidence).to_owned();
    if !valid_ref(&input.link_id)
        || !(1..=9_007_199_254_740_991).contains(&input.expected_version)
        || !matches!(input.state.as_str(), "granted" | "revoked")
        || !valid_text(&evidence, 3, EVIDENCE_LIMIT)
    {
        return Err(identity_input_invalid());
    }
    Ok(GuestSharingPlan {
        link_id: input.link_id.clone(),
        expected_version: input.expected_version,
        state: input.state.clone(),
        evidence,
        version: input.expected_version + 1,
    })
}

pub fn contact_record_id_for_key(key: &str) -> String {
    format!("contact-{key}")
}

pub fn guest_link_record_id_for_key(key: &str) -> String {
    format!("guest-link-{key}")
}

pub fn sharing_event_id_for_key(key: &str) -> String {
    format!("sharing-{key}")
}

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

    #[test]
    fn contact_create_trims_and_bounds_utf16_text() {
        let raw = json!({
            "action": "create-contact", "accountId": "acc-1",
            "name": "  ผู้รับ😀  ", "detail": "\u{feff}  note\u{00a0}  "
        });
        let plan = validate_contact_create(&serde_json::from_value(raw).unwrap()).unwrap();
        assert_eq!(plan.account_id, "acc-1");
        assert_eq!(plan.name, "ผู้รับ😀");
        assert_eq!(plan.detail, "note");
        let short = json!({
            "action": "create-contact", "accountId": "acc-1",
            "name": "a", "detail": "good"
        });
        let parsed: ContactCreateInput = serde_json::from_value(short).unwrap();
        assert!(validate_contact_create(&parsed).is_err());
    }

    #[test]
    fn guest_link_requires_all_refs_and_optional_grant_evidence() {
        let raw = json!({
            "action": "link-guest", "contactId": "c", "guestId": "g",
            "propertyId": "p", "evidence": "  linked  "
        });
        let plan = validate_guest_link(&serde_json::from_value(raw).unwrap()).unwrap();
        assert_eq!(plan.evidence, "linked");
        assert_eq!(plan.sharing_evidence, None);
        let granted = json!({
            "action": "link-guest", "contactId": "c", "guestId": "g",
            "propertyId": "p", "evidence": "linked", "sharingEvidence": "  granted  "
        });
        let plan = validate_guest_link(&serde_json::from_value(granted).unwrap()).unwrap();
        assert_eq!(plan.sharing_evidence.as_deref(), Some("granted"));
        let empty = json!({
            "action": "link-guest", "contactId": "", "guestId": "g",
            "propertyId": "p", "evidence": "linked"
        });
        let parsed: GuestLinkInput = serde_json::from_value(empty).unwrap();
        assert!(validate_guest_link(&parsed).is_err());
    }

    #[test]
    fn guest_sharing_enforces_state_version_and_evidence() {
        let raw = json!({
            "action": "guest-sharing", "linkId": "link-1",
            "expectedVersion": 4, "state": "revoked", "evidence": "  audited  "
        });
        let plan = validate_guest_sharing(&serde_json::from_value(raw).unwrap()).unwrap();
        assert_eq!(plan.state, "revoked");
        assert_eq!(plan.evidence, "audited");
        assert_eq!(plan.version, 5);
        for bad in [
            json!({"action":"guest-sharing","linkId":"l","expectedVersion":0,"state":"granted","evidence":"ok"}),
            json!({"action":"guest-sharing","linkId":"l","expectedVersion":1,"state":"pending","evidence":"ok"}),
            json!({"action":"guest-sharing","linkId":"l","expectedVersion":9007199254740992_i64,"state":"granted","evidence":"ok"}),
        ] {
            let parsed: GuestSharingInput = serde_json::from_value(bad).unwrap();
            assert!(validate_guest_sharing(&parsed).is_err());
        }
    }

    #[test]
    fn identity_record_ids_match_ts_namespaces() {
        assert_eq!(contact_record_id_for_key("k"), "contact-k");
        assert_eq!(guest_link_record_id_for_key("k"), "guest-link-k");
        assert_eq!(sharing_event_id_for_key("k"), "sharing-k");
        assert_eq!(CREATE_CONTACT_OPERATION, "crm.identity.create-contact");
        assert_eq!(LINK_GUEST_OPERATION, "crm.identity.link-guest");
        assert_eq!(GUEST_SHARING_OPERATION, "crm.identity.guest-sharing");
    }
}
