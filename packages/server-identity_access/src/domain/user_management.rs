//! Portable user-management policy.
//!
//! Identity Access owns attribute definitions, local groups and memberships,
//! attribute values, access policies, policy attachments and the reviewed
//! policy-grant candidates derived from them. Nothing here reads a database,
//! HTTP request or clock; callers supply the current actor, facts and time.
//!
//! The reviewed command is a two-step preview/apply: the preview binds the
//! actor, the Company, the reviewed change and a fingerprint of the current
//! snapshot; apply re-reads the snapshot inside the serializable owner unit,
//! requires the same fingerprint and applies the change with its optimistic
//! version. Reconciliation turns eligible policy attachments into individually
//! reviewed grant candidates; it never grants access by itself.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub const MANAGE_PERMISSION: &str = "iam.manage";
pub const READ_PERMISSION: &str = "iam.read";
pub const DIRECTORY_MANAGE_PERMISSION: &str = "iam.directory.manage";
pub const ATTRIBUTE_MANAGE_PERMISSION: &str = "iam.attributes.manage";
pub const REVIEW_PERMISSION: &str = "iam.review";
pub const PERSON_ATTRIBUTE_WRITE_PERMISSION: &str = "people.custom_attributes.write";
pub const PEOPLE_MANAGE_PERMISSION: &str = "people.manage";
pub const ORGANIZATION_MANAGE_PERMISSION: &str = "organization.manage";
pub const MAX_PREVIEW_TOKEN: usize = 128;

const ATTRIBUTE_TYPES: [&str; 6] = [
    "text",
    "number",
    "date",
    "checkbox",
    "single-select",
    "multi-select",
];
const PERSON_RULE_FIELDS: [(&str, &str, &str); 6] = [
    ("worker_type", "Worker type", "text"),
    ("employment_type", "Employment type", "text"),
    ("job_title", "Job title", "text"),
    ("branch_id", "Branch", "text"),
    ("department_id", "Department", "text"),
    ("start_date", "Start date", "date"),
];

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn request_invalid() -> BusinessError {
    invalid(
        "USER_MANAGEMENT_REQUEST_INVALID",
        "Review the management change and try again.",
    )
}
pub fn changed() -> BusinessError {
    conflict(
        "MANAGEMENT_CHANGED",
        "This item changed. Refresh and preview again.",
    )
}
pub fn preview_changed() -> BusinessError {
    conflict(
        "USER_MANAGEMENT_PREVIEW_CHANGED",
        "The preview changed. Review the latest impact before saving.",
    )
}
pub fn scope_forbidden() -> BusinessError {
    forbidden("SCOPE_FORBIDDEN", "This object is outside your workspace.")
}
pub fn permission_required() -> BusinessError {
    forbidden(
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this operation.",
    )
}
pub fn company_scope_required() -> BusinessError {
    forbidden(
        "SCOPE_FORBIDDEN",
        "User management is available in Company workspaces.",
    )
}
pub fn active_unit_required() -> BusinessError {
    invalid(
        "SCOPE_NODE_INACTIVE",
        "Choose an active unit in this Company.",
    )
}
pub fn person_unavailable() -> BusinessError {
    forbidden("SCOPE_FORBIDDEN", "This person is outside your workspace.")
}
pub fn group_unavailable() -> BusinessError {
    invalid("GROUP_REQUIRED", "Choose a local group.")
}
pub fn unit_required() -> BusinessError {
    invalid("SCOPE_NODE_REQUIRED", "Choose a supported grant scope.")
}
pub fn grant_scope_mismatch() -> BusinessError {
    invalid("GRANT_SCOPE_MISMATCH", "Grant scope and unit must match.")
}
pub fn grant_expiry_invalid() -> BusinessError {
    invalid(
        "GRANT_EXPIRY_INVALID",
        "Choose an expiry within 90 days and your assignment validity.",
    )
}
pub fn grant_role_changed() -> BusinessError {
    invalid(
        "GRANT_ROLE_CHANGED",
        "The role changed or does not support this scope.",
    )
}
pub fn delegation_exceeded() -> BusinessError {
    forbidden(
        "DELEGATION_EXCEEDED",
        "The role exceeds your delegated permissions.",
    )
}
pub fn protected_grant() -> BusinessError {
    invalid(
        "PROTECTED_GRANT",
        "Ownership and recovery access must use individually reviewed direct assignments.",
    )
}
pub fn attribute_role_invalid() -> BusinessError {
    invalid(
        "ATTRIBUTE_ROLE_INVALID",
        "Choose an active role for field ownership and access.",
    )
}
pub fn attribute_acl_invalid() -> BusinessError {
    invalid(
        "ATTRIBUTE_ACL_INVALID",
        "Every field editor must also be allowed to view the field.",
    )
}
pub fn attribute_permanent() -> BusinessError {
    invalid(
        "ATTRIBUTE_TYPE_PERMANENT",
        "Field type and target are permanent. Create a new field instead.",
    )
}
pub fn attribute_in_use() -> BusinessError {
    conflict(
        "MANAGEMENT_CHANGED",
        "Update active group rules before archiving this field or changing its choices.",
    )
}
pub fn attribute_options_retained() -> BusinessError {
    invalid(
        "ATTRIBUTE_OPTIONS_RETAINED",
        "Existing choices are retained for history. Add choices or create a replacement field.",
    )
}
pub fn attribute_invalid(message: &'static str) -> BusinessError {
    invalid("ATTRIBUTE_INVALID", message)
}
pub fn group_cycle() -> BusinessError {
    invalid(
        "GROUP_CYCLE",
        "Groups cannot contain themselves or form a cycle.",
    )
}
pub fn group_parent_invalid() -> BusinessError {
    invalid(
        "GROUP_PARENT_INVALID",
        "Choose an active manual parent group.",
    )
}
pub fn group_parent_scope() -> BusinessError {
    invalid(
        "GROUP_PARENT_SCOPE",
        "A child group must stay within its parent's workspace.",
    )
}
pub fn group_children_scope() -> BusinessError {
    invalid(
        "GROUP_CHILDREN_SCOPE",
        "Move child groups before narrowing this group's workspace.",
    )
}
pub fn group_children_present() -> BusinessError {
    invalid(
        "GROUP_CHILDREN_PRESENT",
        "Move child groups before archiving or making this group automatic.",
    )
}
pub fn group_rule_field() -> BusinessError {
    invalid(
        "GROUP_RULE_FIELD",
        "Choose an active person field for each rule.",
    )
}
pub fn group_rule_value() -> BusinessError {
    invalid("GROUP_RULE_VALUE", "Each comparison needs a value.")
}
pub fn group_rule_order() -> BusinessError {
    invalid(
        "GROUP_RULE_ORDER",
        "Ordered comparisons require a number or date field.",
    )
}
pub fn group_rule_contains() -> BusinessError {
    invalid(
        "GROUP_RULE_CONTAINS",
        "Multi-select fields use contains; other fields use comparisons.",
    )
}
pub fn group_rule_text() -> BusinessError {
    invalid("GROUP_RULE_TEXT", "Choose a text value for this field.")
}
pub fn group_manual_required() -> BusinessError {
    invalid(
        "GROUP_MANUAL_REQUIRED",
        "Automatic groups use rules. Choose an active manual group.",
    )
}
pub fn membership_out_of_scope() -> BusinessError {
    invalid(
        "MEMBERSHIP_OUT_OF_SCOPE",
        "A person is outside the group's workspace.",
    )
}
pub fn values_changed() -> BusinessError {
    conflict("VALUES_CHANGED", "These values changed. Refresh first.")
}
pub fn inherit_invalid() -> BusinessError {
    invalid(
        "ATTRIBUTE_INHERIT_INVALID",
        "Only organization values support inheritance.",
    )
}
pub fn attribute_write_forbidden() -> BusinessError {
    forbidden(
        "ATTRIBUTE_WRITE_FORBIDDEN",
        "Your role cannot edit this field or override its inherited value.",
    )
}
pub fn policy_out_of_scope() -> BusinessError {
    forbidden("SCOPE_FORBIDDEN", "The policy is outside your workspace.")
}
pub fn policy_required() -> BusinessError {
    invalid(
        "POLICY_REQUIRED",
        "Choose an active policy within your workspace.",
    )
}
pub fn attachment_out_of_scope() -> BusinessError {
    forbidden(
        "SCOPE_FORBIDDEN",
        "The attachment is outside your workspace.",
    )
}
pub fn active_group_required() -> BusinessError {
    invalid("GROUP_REQUIRED", "Choose an active group.")
}
pub fn exclusion_out_of_scope() -> BusinessError {
    invalid(
        "EXCLUSION_OUT_OF_SCOPE",
        "Exclude only objects beneath the target.",
    )
}
pub fn review_changed() -> BusinessError {
    conflict(
        "REVIEW_CHANGED",
        "Eligibility changed. Refresh the proposal.",
    )
}
pub fn candidate_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CANDIDATE_NOT_FOUND",
        "The policy proposal is unavailable.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key was already used for a different identity command.",
    )
}
pub fn transaction_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "USER_MANAGEMENT_TRANSACTION_REQUIRED",
        "The user-management command requires its owner transaction.",
    )
}

/// Current reviewed-command authority captured by the composition root.
#[derive(Clone, Debug)]
pub struct UserManagementActor {
    pub user_id: String,
    pub session_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub assignment_id: String,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub role: String,
    pub permissions: Vec<String>,
    pub grantable_permissions: Vec<String>,
    pub platform: bool,
    pub expires_at: i64,
}

impl UserManagementActor {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
}

fn bool_value(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeInput {
    pub id: Option<String>,
    pub expected_version: i64,
    pub reason: String,
    pub label: String,
    #[serde(default)]
    pub help: String,
    pub target: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub archived: bool,
    pub owner_role: Option<String>,
    pub read_roles: Option<Vec<String>>,
    pub write_roles: Option<Vec<String>>,
    pub branch_override: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInput {
    pub id: Option<String>,
    pub expected_version: i64,
    pub reason: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub scope_node_id: Option<String>,
    #[serde(default)]
    pub rule: Option<Value>,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyGrantInput {
    pub role_id: String,
    pub expected_role_revision: i64,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub valid_until: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyInput {
    pub id: Option<String>,
    pub expected_version: i64,
    pub reason: String,
    pub name: String,
    pub grants: Vec<PolicyGrantInput>,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInput {
    pub id: Option<String>,
    pub expected_version: i64,
    pub reason: String,
    pub policy_id: String,
    pub target: String,
    pub target_id: String,
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipInput {
    pub group_id: String,
    pub expected_version: i64,
    pub person_ids: Vec<String>,
    pub action: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValuesInput {
    pub target: String,
    pub object_id: String,
    pub expected_version: i64,
    #[serde(default)]
    pub values: Map<String, Value>,
    pub reason: String,
    #[serde(default)]
    pub inherit: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", content = "input", rename_all = "lowercase")]
pub enum ManagementChange {
    Attribute(AttributeInput),
    Group(GroupInput),
    Policy(PolicyInput),
    Attachment(AttachmentInput),
    Membership(MembershipInput),
    Values(ValuesInput),
}

impl ManagementChange {
    pub fn parse(raw: Value) -> Result<Self, BusinessError> {
        let change: Self = serde_json::from_value(raw).map_err(|_| request_invalid())?;
        change.validate()?;
        Ok(change)
    }

    pub fn operation(&self) -> &'static str {
        match self {
            Self::Attribute(_) => "attribute",
            Self::Group(_) => "group",
            Self::Policy(_) => "policy",
            Self::Attachment(_) => "attachment",
            Self::Membership(_) => "membership",
            Self::Values(_) => "values",
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        match self {
            Self::Attribute(input) => {
                validate_id(input.id.as_deref())?;
                validate_version(input.expected_version)?;
                validate_reason(&input.reason)?;
                validate_name(&input.label, 2, 100)?;
                if input.help.chars().count() > 500 {
                    return Err(request_invalid());
                }
                if !["person", "group", "unit"].contains(&input.target.as_str())
                    || !ATTRIBUTE_TYPES.contains(&input.field_type.as_str())
                {
                    return Err(request_invalid());
                }
                if input.options.len() > 100
                    || input
                        .options
                        .iter()
                        .any(|option| option.trim().is_empty() || option.chars().count() > 100)
                {
                    return Err(request_invalid());
                }
                if input.field_type.ends_with("select") {
                    let unique: std::collections::BTreeSet<&str> =
                        input.options.iter().map(String::as_str).collect();
                    if input.options.is_empty() || unique.len() != input.options.len() {
                        return Err(attribute_invalid(
                            "Provide distinct choices for this field.",
                        ));
                    }
                }
                for role in input
                    .read_roles
                    .iter()
                    .chain(input.write_roles.iter())
                    .flatten()
                    .chain(input.owner_role.iter())
                {
                    validate_id(Some(role))?;
                }
                Ok(())
            }
            Self::Group(input) => {
                validate_id(input.id.as_deref())?;
                validate_version(input.expected_version)?;
                validate_reason(&input.reason)?;
                validate_name(&input.name, 2, 100)?;
                if !["manual", "automatic"].contains(&input.kind.as_str()) {
                    return Err(request_invalid());
                }
                validate_id(input.parent_id.as_deref())?;
                validate_id(input.scope_node_id.as_deref())?;
                if (input.kind == "automatic") != input.rule.is_some() {
                    return Err(invalid(
                        "GROUP_RULE_REQUIRED",
                        "Automatic groups require a rule; manual groups do not.",
                    ));
                }
                if let Some(rule) = &input.rule {
                    validate_rule(rule)?;
                }
                Ok(())
            }
            Self::Policy(input) => {
                validate_id(input.id.as_deref())?;
                validate_version(input.expected_version)?;
                validate_reason(&input.reason)?;
                validate_name(&input.name, 2, 100)?;
                if input.grants.is_empty() || input.grants.len() > 20 {
                    return Err(request_invalid());
                }
                for grant in &input.grants {
                    validate_id(Some(&grant.role_id))?;
                    validate_id(grant.scope_node_id.as_deref())?;
                    if grant.expected_role_revision < 1
                        || !["holding", "branch", "department"].contains(&grant.scope_kind.as_str())
                        || grant.valid_until <= 0
                    {
                        return Err(request_invalid());
                    }
                }
                Ok(())
            }
            Self::Attachment(input) => {
                validate_id(input.id.as_deref())?;
                validate_version(input.expected_version)?;
                validate_reason(&input.reason)?;
                validate_id(Some(&input.policy_id))?;
                validate_id(Some(&input.target_id))?;
                if !["group", "unit"].contains(&input.target.as_str())
                    || input.exclusions.len() > 100
                {
                    return Err(request_invalid());
                }
                for exclusion in &input.exclusions {
                    validate_id(Some(exclusion))?;
                }
                Ok(())
            }
            Self::Membership(input) => {
                validate_id(Some(&input.group_id))?;
                if input.expected_version < 1 {
                    return Err(request_invalid());
                }
                validate_reason(&input.reason)?;
                if input.person_ids.is_empty() || input.person_ids.len() > 200 {
                    return Err(request_invalid());
                }
                if !["add", "remove"].contains(&input.action.as_str()) {
                    return Err(request_invalid());
                }
                for person in &input.person_ids {
                    validate_id(Some(person))?;
                }
                Ok(())
            }
            Self::Values(input) => {
                validate_id(Some(&input.object_id))?;
                validate_version(input.expected_version)?;
                validate_reason(&input.reason)?;
                if !["person", "group", "unit"].contains(&input.target.as_str()) {
                    return Err(request_invalid());
                }
                if let Some(inherit) = &input.inherit {
                    if inherit.len() > 100 {
                        return Err(request_invalid());
                    }
                    for field in inherit {
                        validate_id(Some(field))?;
                    }
                }
                Ok(())
            }
        }
    }
}

fn validate_id(value: Option<&str>) -> Result<(), BusinessError> {
    match value {
        Some(value) if value.is_empty() || value.chars().count() > 180 => Err(request_invalid()),
        _ => Ok(()),
    }
}
fn validate_version(value: i64) -> Result<(), BusinessError> {
    if !(0..=i32::MAX as i64).contains(&value) {
        return Err(request_invalid());
    }
    Ok(())
}
fn validate_reason(value: &str) -> Result<(), BusinessError> {
    let length = value.trim().chars().count();
    if !(3..=1000).contains(&length) {
        return Err(request_invalid());
    }
    Ok(())
}
fn validate_name(value: &str, minimum: usize, maximum: usize) -> Result<(), BusinessError> {
    let length = value.trim().chars().count();
    if !(minimum..=maximum).contains(&length) {
        return Err(request_invalid());
    }
    Ok(())
}

fn validate_rule(rule: &Value) -> Result<(), BusinessError> {
    let mode = rule.get("mode").and_then(Value::as_str);
    if !matches!(mode, Some("all") | Some("any")) {
        return Err(request_invalid());
    }
    let conditions = rule
        .get("conditions")
        .and_then(Value::as_array)
        .ok_or_else(request_invalid)?;
    if conditions.is_empty() || conditions.len() > 20 {
        return Err(request_invalid());
    }
    for condition in conditions {
        let field = condition
            .get("field")
            .and_then(Value::as_str)
            .ok_or_else(request_invalid)?;
        if field.is_empty() || field.chars().count() > 180 {
            return Err(request_invalid());
        }
        let operation = condition
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(request_invalid)?;
        if ![
            "eq",
            "neq",
            "contains",
            "gt",
            "gte",
            "lt",
            "lte",
            "empty",
            "not-empty",
        ]
        .contains(&operation)
        {
            return Err(request_invalid());
        }
        if let Some(value) = condition.get("value") {
            let valid = value.is_string() || value.is_number() || value.is_boolean();
            if !valid {
                return Err(request_invalid());
            }
        }
    }
    Ok(())
}

fn has_permission(actor: &UserManagementActor, permission: &str) -> bool {
    actor.has(permission)
}

/// Field ACLs narrow existing object permissions; they never grant one.
pub fn attribute_access(field: &Value, actor: &UserManagementActor) -> (bool, bool) {
    let read_roles = field
        .get("readRoles")
        .and_then(Value::as_array)
        .filter(|roles| !roles.is_empty());
    let write_roles = field
        .get("writeRoles")
        .and_then(Value::as_array)
        .filter(|roles| !roles.is_empty());
    let contains = |roles: &Vec<Value>| {
        roles
            .iter()
            .any(|role| role.as_str() == Some(actor.role.as_str()))
    };
    let can_view = read_roles.is_none_or(contains);
    let branch_ok = actor.scope_kind == "holding"
        || field.get("branchOverride").and_then(Value::as_bool) != Some(false);
    (
        can_view,
        can_view && write_roles.is_none_or(contains) && branch_ok,
    )
}

/// Tagged permission required by one management change.
pub fn command_permission(change: &ManagementChange, actor: &UserManagementActor) -> &'static str {
    match change {
        ManagementChange::Group(_) | ManagementChange::Membership(_) => {
            if has_permission(actor, DIRECTORY_MANAGE_PERMISSION) {
                DIRECTORY_MANAGE_PERMISSION
            } else {
                MANAGE_PERMISSION
            }
        }
        ManagementChange::Values(input) => {
            if input.target == "group" && has_permission(actor, DIRECTORY_MANAGE_PERMISSION) {
                DIRECTORY_MANAGE_PERMISSION
            } else if input.target == "person" {
                PERSON_ATTRIBUTE_WRITE_PERMISSION
            } else if input.target == "unit" {
                ORGANIZATION_MANAGE_PERMISSION
            } else {
                MANAGE_PERMISSION
            }
        }
        ManagementChange::Attribute(_) => ATTRIBUTE_MANAGE_PERMISSION,
        ManagementChange::Policy(_) | ManagementChange::Attachment(_) => MANAGE_PERMISSION,
    }
}

pub fn validate_attribute_value(
    field: &Value,
    value: Option<&Value>,
) -> Result<Option<Value>, BusinessError> {
    let required = bool_value(field, "required");
    let empty = match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.is_empty(),
        Some(Value::Array(items)) => items.is_empty(),
        _ => false,
    };
    if empty {
        if required {
            return Err(attribute_invalid("This field is required."));
        }
        return Ok(None);
    }
    let value = value.expect("checked above");
    let field_type = field
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let options = || {
        field
            .get("options")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let valid = match field_type {
        "text" => value
            .as_str()
            .is_some_and(|text| text.chars().count() <= 2000),
        "number" => value.as_f64().is_some_and(f64::is_finite),
        "checkbox" => value.is_boolean(),
        "date" => value.as_str().is_some_and(valid_date),
        "single-select" => {
            let choices = options();
            value
                .as_str()
                .is_some_and(|text| choices.iter().any(|choice| choice.as_str() == Some(text)))
        }
        "multi-select" => {
            let choices = options();
            value.as_array().is_some_and(|items| {
                let mut seen = std::collections::BTreeSet::new();
                items.iter().all(|item| {
                    item.as_str().is_some_and(|text| {
                        choices.iter().any(|choice| choice.as_str() == Some(text))
                            && seen.insert(text)
                    })
                })
            })
        }
        _ => false,
    };
    if !valid {
        return Err(attribute_invalid("Choose a valid value for this field."));
    }
    Ok(Some(value.clone()))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let year: i32 = value[0..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day <= days
}

fn is_empty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.is_empty(),
        Some(Value::Array(items)) => items.is_empty(),
        _ => false,
    }
}

/// Evaluate one membership rule against person facts.
pub fn rule_matches(rule: &Value, facts: &Value) -> bool {
    let conditions = rule
        .get("conditions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let results: Vec<bool> = conditions
        .iter()
        .map(|condition| {
            let field = condition.get("field").and_then(Value::as_str).unwrap_or("");
            let op = condition.get("op").and_then(Value::as_str).unwrap_or("");
            let value = facts.get(field);
            let empty = is_empty(value);
            match op {
                "empty" => empty,
                "not-empty" => !empty,
                _ if empty => false,
                "eq" => value == condition.get("value"),
                "neq" => value != condition.get("value"),
                "contains" => value.and_then(Value::as_array).is_some_and(|items| {
                    items
                        .iter()
                        .any(|item| Some(item) == condition.get("value"))
                }),
                _ => {
                    let Some(current) = value else { return false };
                    let Some(target) = condition.get("value") else {
                        return false;
                    };
                    let comparison = match (current, target) {
                        (Value::Number(left), Value::Number(right)) => {
                            let (Some(left), Some(right)) = (left.as_f64(), right.as_f64()) else {
                                return false;
                            };
                            left - right
                        }
                        (Value::String(left), Value::String(right)) => {
                            if left == right {
                                0.0
                            } else if left < right {
                                -1.0
                            } else {
                                1.0
                            }
                        }
                        _ => return false,
                    };
                    match op {
                        "gt" => comparison > 0.0,
                        "gte" => comparison >= 0.0,
                        "lt" => comparison < 0.0,
                        "lte" => comparison <= 0.0,
                        _ => false,
                    }
                }
            }
        })
        .collect();
    match rule.get("mode").and_then(Value::as_str) {
        Some("any") => results.iter().any(|matched| *matched),
        _ => results.iter().all(|matched| *matched),
    }
}

pub fn content_hash(value: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(value).unwrap_or_default());
    format!("{:x}", hasher.finalize())
}

fn group_members(snapshot: &Value) -> Vec<String> {
    snapshot
        .get("groups")
        .and_then(Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .flat_map(|group| {
                    let name = group.get("name").and_then(Value::as_str).unwrap_or("");
                    group
                        .get("members")
                        .and_then(Value::as_array)
                        .map(|members| {
                            members
                                .iter()
                                .filter_map(Value::as_str)
                                .map(|person| {
                                    let person_name = snapshot
                                        .get("people")
                                        .and_then(Value::as_array)
                                        .and_then(|people| {
                                            people.iter().find(|row| {
                                                row.get("id").and_then(Value::as_str)
                                                    == Some(person)
                                            })
                                        })
                                        .and_then(|row| row.get("name"))
                                        .and_then(Value::as_str)
                                        .unwrap_or(person);
                                    format!("{person_name} → {name}")
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn person_name(snapshot: &Value, person: &str) -> String {
    snapshot
        .get("people")
        .and_then(Value::as_array)
        .and_then(|people| {
            people
                .iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(person))
        })
        .and_then(|row| row.get("name"))
        .and_then(Value::as_str)
        .unwrap_or(person)
        .to_owned()
}

/// Before/after impact for the reviewed workspace.
pub fn management_impact(before: &Value, after: &Value) -> Value {
    let old = group_members(before);
    let next = group_members(after);
    let members_added: Vec<&String> = next.iter().filter(|item| !old.contains(item)).collect();
    let members_removed: Vec<&String> = old.iter().filter(|item| !next.contains(item)).collect();
    let old_candidates = before
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let new_candidates = after
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut access_removed = Vec::new();
    for candidate in &old_candidates {
        let status = candidate
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !["active", "pending"].contains(&status) {
            continue;
        }
        let id = candidate.get("id").and_then(Value::as_str).unwrap_or("");
        let retained = new_candidates.iter().any(|next| {
            next.get("id").and_then(Value::as_str) == Some(id)
                && next.get("status").and_then(Value::as_str) == Some(status)
        });
        if !retained {
            let person = candidate
                .get("personId")
                .and_then(Value::as_str)
                .unwrap_or("");
            access_removed.push(format!(
                "{} · {}",
                person_name(before, person),
                candidate
                    .get("roleId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            ));
        }
    }
    let mut access_pending = Vec::new();
    let mut access_blocked = Vec::new();
    for candidate in &new_candidates {
        let id = candidate.get("id").and_then(Value::as_str).unwrap_or("");
        if old_candidates
            .iter()
            .any(|old| old.get("id").and_then(Value::as_str) == Some(id))
        {
            continue;
        }
        let person = candidate
            .get("personId")
            .and_then(Value::as_str)
            .unwrap_or("");
        match candidate.get("status").and_then(Value::as_str) {
            Some("pending") => access_pending.push(format!(
                "{} · {}",
                person_name(after, person),
                candidate
                    .get("roleId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )),
            Some("blocked") => access_blocked.push(format!(
                "{} · {}",
                person_name(after, person),
                candidate
                    .get("explanation")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )),
            _ => {}
        }
    }
    json!({
        "membersAdded": members_added,
        "membersRemoved": members_removed,
        "accessRemoved": access_removed,
        "accessPending": access_pending,
        "accessBlocked": access_blocked,
        "matchingPeople": Vec::<String>::new(),
    })
}

/// Explain which rule condition matched each member for a group command.
pub fn matching_people(snapshot: &Value, group_id: &str, rule: &Value) -> Vec<String> {
    let Some(group) = snapshot
        .get("groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups
                .iter()
                .find(|group| group.get("id").and_then(Value::as_str) == Some(group_id))
        })
    else {
        return Vec::new();
    };
    let conditions = rule
        .get("conditions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    group
        .get("members")
        .and_then(Value::as_array)
        .map(|members| {
            members
                .iter()
                .filter_map(Value::as_str)
                .map(|person| {
                    let Some(row) =
                        snapshot
                            .get("people")
                            .and_then(Value::as_array)
                            .and_then(|people| {
                                people.iter().find(|row| {
                                    row.get("id").and_then(Value::as_str) == Some(person)
                                })
                            })
                    else {
                        return person.to_owned();
                    };
                    let facts = row.get("facts").cloned().unwrap_or(Value::Null);
                    let matched: Vec<String> = conditions
                        .iter()
                        .filter(|condition| {
                            rule_matches(&json!({"mode": "all", "conditions": [condition]}), &facts)
                        })
                        .map(|condition| {
                            let field =
                                condition.get("field").and_then(Value::as_str).unwrap_or("");
                            let label = snapshot
                                .get("attributes")
                                .and_then(Value::as_array)
                                .and_then(|attributes| {
                                    attributes.iter().find(|attribute| {
                                        attribute
                                            .get("id")
                                            .and_then(Value::as_str)
                                            .map(|id| format!("custom:{id}"))
                                            .as_deref()
                                            == Some(field)
                                    })
                                })
                                .and_then(|attribute| attribute.get("label"))
                                .and_then(Value::as_str)
                                .or_else(|| {
                                    PERSON_RULE_FIELDS
                                        .iter()
                                        .find(|(id, _, _)| *id == field)
                                        .map(|(_, label, _)| *label)
                                })
                                .unwrap_or(field);
                            let value = condition
                                .get("value")
                                .map(|value| match value {
                                    Value::String(text) => text.clone(),
                                    other => other.to_string(),
                                })
                                .unwrap_or_default();
                            format!(
                                "{label}: {} {value}",
                                condition.get("op").and_then(Value::as_str).unwrap_or("")
                            )
                        })
                        .collect();
                    format!(
                        "{} — {}",
                        row.get("name").and_then(Value::as_str).unwrap_or(person),
                        matched.join(" · ")
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn person_rule_field_type(field: &str) -> Option<&'static str> {
    PERSON_RULE_FIELDS
        .iter()
        .find(|(id, _, _)| *id == field)
        .map(|(_, _, value_type)| *value_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn actor(role: &str, permissions: &[&str]) -> UserManagementActor {
        UserManagementActor {
            user_id: "user".into(),
            session_id: "session".into(),
            holding_id: "holding".into(),
            company_id: "company".into(),
            assignment_id: "assignment".into(),
            scope_kind: "holding".into(),
            scope_node_id: None,
            role: role.into(),
            permissions: permissions.iter().map(|value| (*value).into()).collect(),
            grantable_permissions: Vec::new(),
            platform: false,
            expires_at: 1_000_000,
        }
    }

    #[test]
    fn attribute_values_enforce_type_required_and_options() {
        let field = json!({
            "id": "note", "label": "Note", "type": "text", "required": true,
            "target": "person", "options": [],
        });
        assert_eq!(
            validate_attribute_value(&field, Some(&json!("hello"))).unwrap(),
            Some(json!("hello"))
        );
        assert_eq!(
            validate_attribute_value(&field, Some(&json!("")))
                .unwrap_err()
                .code,
            "ATTRIBUTE_INVALID"
        );
        let optional = json!({
            "id": "note", "label": "Note", "type": "text", "required": false,
            "target": "person", "options": [],
        });
        assert_eq!(
            validate_attribute_value(&optional, Some(&json!(""))).unwrap(),
            None
        );
        let select = json!({
            "id": "choice", "label": "Choice", "type": "single-select",
            "required": false, "target": "person", "options": ["a", "b"],
        });
        assert!(validate_attribute_value(&select, Some(&json!("c"))).is_err());
        let multi = json!({
            "id": "multi", "label": "Multi", "type": "multi-select",
            "required": false, "target": "person", "options": ["a", "b"],
        });
        assert!(validate_attribute_value(&multi, Some(&json!(["a", "b"]))).is_ok());
        assert!(validate_attribute_value(&multi, Some(&json!(["a", "a"]))).is_err());
    }

    #[test]
    fn dates_require_canonical_calendar_values() {
        let field = json!({
            "id": "start", "label": "Start", "type": "date", "required": false,
            "target": "person", "options": [],
        });
        assert!(validate_attribute_value(&field, Some(&json!("2024-02-29"))).is_ok());
        assert!(validate_attribute_value(&field, Some(&json!("2023-02-29"))).is_err());
        assert!(validate_attribute_value(&field, Some(&json!("2024-2-29"))).is_err());
    }

    #[test]
    fn membership_rules_follow_mode_comparisons_and_emptiness() {
        let facts = json!({
            "worker_type": "employee", "job_title": "Engineer",
            "start_date": "2024-01-01", "tags": ["a", "b"],
        });
        assert!(rule_matches(
            &json!({"mode": "all", "conditions": [
                {"field": "worker_type", "op": "eq", "value": "employee"},
                {"field": "start_date", "op": "gte", "value": "2024-01-01"},
            ]}),
            &facts
        ));
        assert!(!rule_matches(
            &json!({"mode": "all", "conditions": [
                {"field": "worker_type", "op": "neq", "value": "employee"},
            ]}),
            &facts
        ));
        assert!(rule_matches(
            &json!({"mode": "any", "conditions": [
                {"field": "missing", "op": "empty"},
                {"field": "tags", "op": "contains", "value": "b"},
            ]}),
            &facts
        ));
        assert!(rule_matches(
            &json!({"mode": "all", "conditions": [
                {"field": "tags", "op": "not-empty"},
                {"field": "missing", "op": "empty"},
            ]}),
            &facts
        ));
    }

    #[test]
    fn command_permissions_follow_operation_and_target() {
        let owner = actor("holding_owner", &[MANAGE_PERMISSION]);
        let directory = actor(
            "it_admin",
            &[MANAGE_PERMISSION, DIRECTORY_MANAGE_PERMISSION],
        );
        let hr = actor("hr_admin", &[PERSON_ATTRIBUTE_WRITE_PERMISSION]);
        let change: ManagementChange = ManagementChange::parse(json!({
            "operation": "group",
            "input": {
                "name": "Manual", "kind": "manual", "expectedVersion": 0,
                "reason": "Group setup",
            },
        }))
        .unwrap();
        assert_eq!(command_permission(&change, &owner), MANAGE_PERMISSION);
        assert_eq!(
            command_permission(&change, &directory),
            DIRECTORY_MANAGE_PERMISSION
        );
        let values: ManagementChange = ManagementChange::parse(json!({
            "operation": "values",
            "input": {
                "target": "person", "objectId": "person", "expectedVersion": 0,
                "values": {}, "reason": "Record note",
            },
        }))
        .unwrap();
        assert_eq!(
            command_permission(&values, &hr),
            PERSON_ATTRIBUTE_WRITE_PERMISSION
        );
    }

    #[test]
    fn impact_reports_membership_and_candidate_changes() {
        let before = json!({
            "people": [{"id": "person", "name": "Employee"}],
            "groups": [{"id": "group", "name": "Team", "members": []}],
            "candidates": [],
        });
        let after = json!({
            "people": [{"id": "person", "name": "Employee"}],
            "groups": [{"id": "group", "name": "Team", "members": ["person"]}],
            "candidates": [{
                "id": "candidate", "personId": "person", "roleId": "reader",
                "status": "pending", "explanation": "Review",
            }],
        });
        let impact = management_impact(&before, &after);
        assert_eq!(impact["membersAdded"], json!(["Employee → Team"]));
        assert_eq!(impact["membersRemoved"], json!([]));
        assert_eq!(impact["accessPending"], json!(["Employee · reader"]));
    }

    #[test]
    fn attribute_acl_narrows_without_granting() {
        let field = json!({
            "readRoles": ["hr_admin"], "writeRoles": ["hr_admin"],
        });
        assert_eq!(
            attribute_access(&field, &actor("hr_admin", &[])),
            (true, true)
        );
        assert_eq!(
            attribute_access(&field, &actor("auditor", &[])),
            (false, false)
        );
    }
}
