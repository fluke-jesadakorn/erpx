//! Native Directory automation command vocabulary.
//!
//! These are transport/owner facts only. A copied action never authorizes a
//! provider write: the installed connector profile still decides whether the
//! requested operation is approved, and the command transaction decides
//! whether the caller holds current Company authority.
use crate::domain::user_management::UserManagementActor;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// The selected Company actor. Directory automation reuses the current IAM
/// actor shape: permissions are evaluated against the published role revision.
pub type DirectoryOperationActor = UserManagementActor;

pub const MANAGE_PERMISSION: &str = "iam.directory.manage";
pub const APPLY_PERMISSION: &str = "iam.directory.apply";
pub const READ_PERMISSION: &str = "iam.read";
pub const REVIEW_PERMISSION: &str = "iam.review";
pub const PEOPLE_MANAGE_PERMISSION: &str = "people.manage";

pub fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}
pub fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
pub fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}
pub fn request_invalid() -> BusinessError {
    invalid("DIRECTORY_REQUEST_INVALID", "The directory request is invalid.")
}
pub fn company_scope_required() -> BusinessError {
    forbidden(
        "COMPANY_SCOPE_REQUIRED",
        "Use People in a Company workspace.",
    )
}
pub fn connection_unavailable() -> BusinessError {
    conflict(
        "CONNECTION_UNAVAILABLE",
        "Choose an available directory connection.",
    )
}
pub fn connection_changed() -> BusinessError {
    conflict("CONNECTION_CHANGED", "The connection changed.")
}
pub fn directory_test_required() -> BusinessError {
    conflict(
        "DIRECTORY_TEST_REQUIRED",
        "Read and test the directory inventory first.",
    )
}
pub fn preview_changed() -> BusinessError {
    conflict(
        "DIRECTORY_PREVIEW_CHANGED",
        "Review the latest directory impact before submitting.",
    )
}
pub fn read_only() -> BusinessError {
    conflict(
        "DIRECTORY_READ_ONLY",
        "Enable tested write-back or keep this as a manual IT task.",
    )
}
pub fn version_conflict() -> BusinessError {
    conflict(
        "DIRECTORY_OPERATION_CHANGED",
        "This directory operation changed.",
    )
}
pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This command key was already used for a different request.",
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryObject {
    pub id: String,
    pub dn: String,
    pub name: String,
    pub kind: String,
    pub parent_id: Option<String>,
    pub version: String,
    pub module: String,
    #[serde(default)]
    pub properties: Value,
    #[serde(default)]
    pub member_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorField {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub read: bool,
    pub write: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryInventory {
    #[serde(default)]
    pub objects: Vec<DirectoryObject>,
    #[serde(default)]
    pub fields: Vec<ConnectorField>,
    pub provider_version: String,
    pub observed_at: i64,
    pub complete: bool,
}

impl DirectoryInventory {
    /// Parse the bounded installed-connector projection. Duplicate immutable
    /// identities or an incomplete observation never become a preview.
    pub fn checked(value: Value) -> Result<Self, BusinessError> {
        let inventory: Self = serde_json::from_value(value)
            .map_err(|_| invalid("DIRECTORY_INCOMPLETE", "The provider returned an invalid inventory."))?;
        if !inventory.complete
            || inventory.provider_version.is_empty()
            || inventory.objects.len() > 10_000
            || inventory
                .objects
                .iter()
                .any(|o| o.id.is_empty() || o.dn.is_empty() || o.version.is_empty())
        {
            return Err(invalid(
                "DIRECTORY_INCOMPLETE",
                "The provider returned an invalid inventory.",
            ));
        }
        let mut seen = BTreeSet::new();
        if inventory
            .objects
            .iter()
            .any(|object| !seen.insert(object.id.clone()))
        {
            return Err(invalid(
                "DIRECTORY_INCOMPLETE",
                "The provider returned an invalid inventory.",
            ));
        }
        Ok(inventory)
    }
    pub fn object(&self, id: &str) -> Option<&DirectoryObject> {
        self.objects.iter().find(|object| object.id == id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsDrive {
    pub id: String,
    pub letter: String,
    pub path: String,
    pub label: String,
    #[serde(default)]
    pub remove: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsPrinter {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub remove: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsPolicyDefinition {
    pub name: String,
    pub scope: String,
    pub target_ou_id: String,
    pub filter_group_id: String,
    #[serde(default)]
    pub computer_target_id: Option<String>,
    #[serde(default = "unchanged")]
    pub usb: String,
    #[serde(default)]
    pub lock_seconds: Option<i64>,
    #[serde(default)]
    pub wallpaper: Option<String>,
    #[serde(default = "default_wallpaper_style")]
    pub wallpaper_style: String,
    #[serde(default)]
    pub drives: Vec<WindowsDrive>,
    #[serde(default)]
    pub printers: Vec<WindowsPrinter>,
}
fn unchanged() -> String {
    "unchanged".to_owned()
}
fn default_wallpaper_style() -> String {
    "10".to_owned()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DirectoryAction {
    #[serde(rename = "account.create", rename_all = "camelCase")]
    AccountCreate {
        username: String,
        first_name: String,
        last_name: String,
        parent_id: String,
    },
    #[serde(rename = "account.update", rename_all = "camelCase")]
    AccountUpdate {
        object_id: String,
        fields: BTreeMap<String, Value>,
    },
    #[serde(rename = "account.disable", rename_all = "camelCase")]
    AccountDisable {
        object_id: String,
        #[serde(default)]
        case_id: Option<String>,
    },
    #[serde(rename = "account.enable", rename_all = "camelCase")]
    AccountEnable { object_id: String, case_id: String },
    #[serde(rename = "group.create", rename_all = "camelCase")]
    GroupCreate { name: String, parent_id: String },
    #[serde(rename = "group.members", rename_all = "camelCase")]
    GroupMembers {
        object_id: String,
        member_ids: Vec<String>,
        action: String,
    },
    #[serde(rename = "ou.create", rename_all = "camelCase")]
    OuCreate { name: String, parent_id: String },
    #[serde(rename = "ou.update", rename_all = "camelCase")]
    OuUpdate {
        object_id: String,
        #[serde(default)]
        description: String,
    },
    #[serde(rename = "object.move", rename_all = "camelCase")]
    ObjectMove { object_id: String, parent_id: String },
    #[serde(rename = "gpo.publish", rename_all = "camelCase")]
    GpoPublish {
        #[serde(default)]
        object_id: Option<String>,
        definition: WindowsPolicyDefinition,
    },
    #[serde(rename = "gpo.link", rename_all = "camelCase")]
    GpoLink {
        object_id: String,
        parent_id: String,
        #[serde(default)]
        filter_group_id: Option<String>,
        #[serde(default = "default_true")]
        enabled: bool,
        #[serde(default)]
        enforced: bool,
        #[serde(default)]
        order: Option<i64>,
        #[serde(default)]
        remove: bool,
    },
    #[serde(rename = "gpo.order", rename_all = "camelCase")]
    GpoOrder {
        object_id: String,
        gpo_ids: Vec<String>,
    },
    #[serde(rename = "gpo.inheritance", rename_all = "camelCase")]
    GpoInheritance { object_id: String, blocked: bool },
    #[serde(rename = "gpo.filter", rename_all = "camelCase")]
    GpoFilter {
        object_id: String,
        group_ids: Vec<String>,
    },
}
fn default_true() -> bool {
    true
}

impl DirectoryAction {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::AccountCreate { .. } => "account.create",
            Self::AccountUpdate { .. } => "account.update",
            Self::AccountDisable { .. } => "account.disable",
            Self::AccountEnable { .. } => "account.enable",
            Self::GroupCreate { .. } => "group.create",
            Self::GroupMembers { .. } => "group.members",
            Self::OuCreate { .. } => "ou.create",
            Self::OuUpdate { .. } => "ou.update",
            Self::ObjectMove { .. } => "object.move",
            Self::GpoPublish { .. } => "gpo.publish",
            Self::GpoLink { .. } => "gpo.link",
            Self::GpoOrder { .. } => "gpo.order",
            Self::GpoInheritance { .. } => "gpo.inheritance",
            Self::GpoFilter { .. } => "gpo.filter",
        }
    }
    pub fn object_id(&self) -> Option<&str> {
        match self {
            Self::AccountCreate { .. } | Self::GroupCreate { .. } | Self::OuCreate { .. } => None,
            Self::AccountUpdate { object_id, .. }
            | Self::AccountDisable { object_id, .. }
            | Self::AccountEnable { object_id, .. }
            | Self::GroupMembers { object_id, .. }
            | Self::OuUpdate { object_id, .. }
            | Self::ObjectMove { object_id, .. }
            | Self::GpoLink { object_id, .. }
            | Self::GpoOrder { object_id, .. }
            | Self::GpoInheritance { object_id, .. }
            | Self::GpoFilter { object_id, .. } => Some(object_id),
            Self::GpoPublish { object_id, .. } => object_id.as_deref(),
        }
    }
    pub fn parent_id(&self) -> Option<&str> {
        match self {
            Self::AccountCreate { parent_id, .. }
            | Self::GroupCreate { parent_id, .. }
            | Self::OuCreate { parent_id, .. }
            | Self::ObjectMove { parent_id, .. }
            | Self::GpoLink { parent_id, .. } => Some(parent_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryChangeInput {
    pub connection_id: String,
    pub expected_version: i64,
    pub action: DirectoryAction,
    #[serde(default)]
    pub additional_actions: Vec<DirectoryAction>,
    pub reason: String,
    #[serde(default)]
    pub affected_company_ids: Vec<String>,
}

impl DirectoryChangeInput {
    pub fn actions(&self) -> impl Iterator<Item = &DirectoryAction> {
        std::iter::once(&self.action).chain(self.additional_actions.iter())
    }
    pub fn validate(mut self) -> Result<Self, BusinessError> {
        self.reason = self.reason.trim().to_owned();
        if self.connection_id.is_empty() || self.connection_id.len() > 500 {
            return Err(request_invalid());
        }
        if self.expected_version < 1 || self.expected_version > i64::from(i32::MAX) {
            return Err(request_invalid());
        }
        if !(3..=1000).contains(&self.reason.chars().count())
            || self.affected_company_ids.len() > 100
            || self.additional_actions.len() > 19
        {
            return Err(request_invalid());
        }
        if self
            .affected_company_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 500)
        {
            return Err(request_invalid());
        }
        for action in self.actions() {
            validate_action(action)?;
        }
        Ok(self)
    }
}

fn id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 500
}
fn text_ok(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= 500
}
fn object_name_ok(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && value.chars().count() >= 2
        && value.chars().count() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'))
}
fn username_ok(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && (2..=64).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}
fn unc_ok(value: &str) -> bool {
    value.starts_with("\\\\")
        && value.len() <= 500
        && !value.split(['\\', '/']).any(|part| part.contains(char::is_whitespace))
        && value[2..].contains('\\')
        && !value.contains(['\r', '\n'])
}
fn validate_windows_policy(policy: &WindowsPolicyDefinition) -> Result<(), BusinessError> {
    let name_len = policy.name.trim().chars().count();
    if !(2..=120).contains(&name_len)
        || !id_ok(&policy.target_ou_id)
        || !id_ok(&policy.filter_group_id)
        || !matches!(policy.scope.as_str(), "user" | "computer")
        || !matches!(policy.usb.as_str(), "unchanged" | "deny" | "allow")
        || !matches!(policy.wallpaper_style.as_str(), "0" | "2" | "6" | "10")
        || policy.drives.len() > 20
        || policy.printers.len() > 20
    {
        return Err(invalid(
            "DIRECTORY_POLICY_INVALID",
            "Review the Windows policy fields.",
        ));
    }
    if let Some(seconds) = policy.lock_seconds {
        if !(60..=86_400).contains(&seconds) {
            return Err(invalid(
                "DIRECTORY_POLICY_INVALID",
                "Review the Windows policy fields.",
            ));
        }
    }
    if policy.scope == "computer" && policy.computer_target_id.is_none() {
        return Err(invalid(
            "DIRECTORY_POLICY_INVALID",
            "Choose an explicit computer OU or group.",
        ));
    }
    if policy.scope == "computer"
        && (policy.wallpaper.is_some() || !policy.drives.is_empty() || !policy.printers.is_empty())
    {
        return Err(invalid(
            "DIRECTORY_POLICY_INVALID",
            "Desktop, Drive and Printer preferences require a user policy.",
        ));
    }
    if let Some(wallpaper) = &policy.wallpaper {
        let lower = wallpaper.to_ascii_lowercase();
        if wallpaper.len() > 500
            || !(lower.ends_with(".bmp")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".jpe"))
            || !(wallpaper.starts_with("\\\\")
                || (wallpaper.len() > 2
                    && wallpaper.as_bytes()[1] == b':'
                    && wallpaper.as_bytes()[2] == b'\\'))
            || wallpaper.contains(['\r', '\n'])
        {
            return Err(invalid(
                "DIRECTORY_POLICY_INVALID",
                "Choose a local or UNC BMP/JPEG wallpaper path.",
            ));
        }
    }
    let letters = policy
        .drives
        .iter()
        .map(|drive| drive.letter.as_str())
        .collect::<BTreeSet<_>>();
    if letters.len() != policy.drives.len()
        || policy.drives.iter().any(|drive| {
            drive.letter.len() != 1
                || !drive.letter.as_bytes()[0].is_ascii_uppercase()
                || !(b'D'..=b'Z').contains(&drive.letter.as_bytes()[0])
                || !unc_ok(&drive.path)
                || !text_ok(&drive.label)
        })
    {
        return Err(invalid(
            "DIRECTORY_POLICY_INVALID",
            "Each drive letter must be unique.",
        ));
    }
    if policy
        .printers
        .iter()
        .filter(|printer| printer.is_default && !printer.remove)
        .count()
        > 1
        || policy.printers.iter().any(|printer| !unc_ok(&printer.path))
    {
        return Err(invalid(
            "DIRECTORY_POLICY_INVALID",
            "Choose only one default printer.",
        ));
    }
    Ok(())
}

pub fn validate_action(action: &DirectoryAction) -> Result<(), BusinessError> {
    match action {
        DirectoryAction::AccountCreate {
            username,
            first_name,
            last_name,
            parent_id,
        } => {
            if !username_ok(username)
                || !text_ok(first_name)
                || !text_ok(last_name)
                || !id_ok(parent_id)
            {
                return Err(request_invalid());
            }
        }
        DirectoryAction::AccountUpdate { object_id, fields } => {
            if !id_ok(object_id) || fields.is_empty() {
                return Err(request_invalid());
            }
            for value in fields.values() {
                let ok = value.as_bool().is_some()
                    || value
                        .as_str()
                        .is_some_and(|value| value.chars().count() <= 2000);
                if !ok {
                    return Err(request_invalid());
                }
            }
        }
        DirectoryAction::AccountDisable {
            object_id,
            case_id,
        } => {
            if !id_ok(object_id) || case_id.as_deref().is_some_and(|id| !id_ok(id)) {
                return Err(request_invalid());
            }
        }
        DirectoryAction::AccountEnable { object_id, case_id } => {
            if !id_ok(object_id) || !id_ok(case_id) {
                return Err(request_invalid());
            }
        }
        DirectoryAction::GroupCreate { name, parent_id }
        | DirectoryAction::OuCreate { name, parent_id } => {
            if !object_name_ok(name) || !id_ok(parent_id) {
                return Err(request_invalid());
            }
        }
        DirectoryAction::GroupMembers {
            object_id,
            member_ids,
            action,
        } => {
            if !id_ok(object_id)
                || member_ids.is_empty()
                || member_ids.len() > 100
                || member_ids.iter().any(|id| !id_ok(id))
                || !matches!(action.as_str(), "add" | "remove")
            {
                return Err(request_invalid());
            }
        }
        DirectoryAction::OuUpdate {
            object_id,
            description,
        } => {
            if !id_ok(object_id) || description.chars().count() > 500 {
                return Err(request_invalid());
            }
        }
        DirectoryAction::ObjectMove {
            object_id,
            parent_id,
        } => {
            if !id_ok(object_id) || !id_ok(parent_id) {
                return Err(request_invalid());
            }
        }
        DirectoryAction::GpoPublish {
            object_id,
            definition,
        } => {
            if object_id.as_deref().is_some_and(|id| !id_ok(id)) {
                return Err(request_invalid());
            }
            validate_windows_policy(definition)?;
        }
        DirectoryAction::GpoLink {
            object_id,
            parent_id,
            filter_group_id,
            order,
            ..
        } => {
            if !id_ok(object_id)
                || !id_ok(parent_id)
                || filter_group_id.as_deref().is_some_and(|id| !id_ok(id))
                || order.is_some_and(|order| !(1..=1000).contains(&order))
            {
                return Err(request_invalid());
            }
        }
        DirectoryAction::GpoOrder { object_id, gpo_ids } => {
            if !id_ok(object_id)
                || gpo_ids.len() > 1000
                || gpo_ids.iter().any(|id| !id_ok(id))
            {
                return Err(request_invalid());
            }
        }
        DirectoryAction::GpoInheritance { object_id, .. } => {
            if !id_ok(object_id) {
                return Err(request_invalid());
            }
        }
        DirectoryAction::GpoFilter {
            object_id,
            group_ids,
        } => {
            if !id_ok(object_id)
                || group_ids.len() > 100
                || group_ids.iter().any(|id| !id_ok(id))
            {
                return Err(request_invalid());
            }
        }
    }
    Ok(())
}

/// Sorted JSON used for stable preview tokens and evidence fingerprints.
pub fn canonical(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        Value::Object(object) => {
            let mut entries = object.iter().collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonical(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        other => other.clone(),
    }
}
pub fn hash_value(value: &Value) -> String {
    let mut digest = Sha256::new();
    digest.update(canonical(value).to_string().as_bytes());
    hex::encode(digest.finalize())
}

fn push_id(set: &mut BTreeSet<String>, value: Option<&str>) {
    if let Some(value) = value {
        if !value.is_empty() {
            set.insert(value.to_owned());
        }
    }
}

/// Transitive impact of one action inside the observed inventory.
pub fn affected_objects(action: &DirectoryAction, inventory: &DirectoryInventory) -> Vec<DirectoryObject> {
    let mut ids = BTreeSet::new();
    push_id(&mut ids, action.object_id());
    push_id(&mut ids, action.parent_id());
    match action {
        DirectoryAction::GroupMembers { member_ids, .. } => {
            for id in member_ids {
                ids.insert(id.clone());
            }
        }
        DirectoryAction::GpoPublish { object_id, definition } => {
            if object_id.is_none() {
                ids.insert(definition.target_ou_id.clone());
                ids.insert(definition.filter_group_id.clone());
            }
            push_id(&mut ids, definition.computer_target_id.as_deref());
        }
        DirectoryAction::GpoLink {
            filter_group_id, ..
        } => push_id(&mut ids, filter_group_id.as_deref()),
        DirectoryAction::GpoFilter { group_ids, .. } => {
            ids.extend(group_ids.iter().cloned());
        }
        DirectoryAction::GpoOrder { gpo_ids, .. } => {
            ids.extend(gpo_ids.iter().cloned());
        }
        _ => {}
    }
    let mut changed = true;
    while changed {
        changed = false;
        for object in &inventory.objects {
            if ids.contains(&object.id) {
                let mut members = object.member_ids.clone();
                if object.kind == "gpo" {
                    if let Some(values) = object
                        .properties
                        .get("securityFilterGroupIds")
                        .and_then(Value::as_array)
                    {
                        members.extend(
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned),
                        );
                    }
                }
                for id in members {
                    if ids.insert(id) {
                        changed = true;
                    }
                }
            } else if object
                .parent_id
                .as_deref()
                .is_some_and(|parent| ids.contains(parent))
                || (object.kind == "group"
                    && object.member_ids.iter().any(|id| ids.contains(id)))
            {
                ids.insert(object.id.clone());
                changed = true;
            }
        }
    }
    inventory
        .objects
        .iter()
        .filter(|object| ids.contains(&object.id))
        .cloned()
        .collect()
}

fn by_id<'a>(inventory: &'a DirectoryInventory, id: &str) -> Result<&'a DirectoryObject, BusinessError> {
    inventory.object(id).ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "DIRECTORY_OBJECT_MISSING",
            "Refresh the directory inventory.",
        )
    })
}
fn target_invalid(message: &'static str) -> BusinessError {
    invalid("DIRECTORY_TARGET_INVALID", message)
}
fn field_unsupported() -> BusinessError {
    invalid(
        "DIRECTORY_FIELD_UNSUPPORTED",
        "Choose fields supported for writing by this connector.",
    )
}

pub fn validate_object_target(
    action: &DirectoryAction,
    inventory: &DirectoryInventory,
) -> Result<(), BusinessError> {
    if let Some(id) = action.object_id() {
        by_id(inventory, id)?;
    }
    if let Some(parent) = action.parent_id() {
        if by_id(inventory, parent)?.kind != "ou" {
            return Err(target_invalid("Choose a directory OU or container."));
        }
    }
    match action {
        DirectoryAction::AccountUpdate { object_id, fields } => {
            if by_id(inventory, object_id)?.kind != "account" {
                return Err(target_invalid("Choose an account."));
            }
            if fields.is_empty()
                || fields.keys().any(|key| {
                    !inventory
                        .fields
                        .iter()
                        .any(|field| &field.name == key && field.write)
                })
            {
                return Err(field_unsupported());
            }
        }
        DirectoryAction::AccountDisable { object_id, .. }
        | DirectoryAction::AccountEnable { object_id, .. } => {
            if by_id(inventory, object_id)?.kind != "account" {
                return Err(target_invalid("Choose an account."));
            }
        }
        DirectoryAction::ObjectMove {
            object_id,
            parent_id,
        } => {
            if !["account", "computer", "group", "ou"]
                .contains(&by_id(inventory, object_id)?.kind.as_str())
            {
                return Err(target_invalid(
                    "Choose an account, computer, group or OU.",
                ));
            }
            let mut seen = BTreeSet::new();
            let mut target = Some(by_id(inventory, parent_id)?);
            while let Some(object) = target {
                if object.id == *object_id || !seen.insert(object.id.clone()) {
                    return Err(invalid(
                        "DIRECTORY_OU_CYCLE",
                        "An OU cannot be moved into itself or a descendant.",
                    ));
                }
                target = object
                    .parent_id
                    .as_deref()
                    .map(|id| by_id(inventory, id))
                    .transpose()?;
            }
        }
        DirectoryAction::OuUpdate { object_id, .. }
        | DirectoryAction::GpoInheritance { object_id, .. } => {
            if by_id(inventory, object_id)?.kind != "ou" {
                return Err(target_invalid("Choose an OU."));
            }
        }
        DirectoryAction::GpoOrder { object_id, gpo_ids } => {
            if by_id(inventory, object_id)?.kind != "ou" {
                return Err(target_invalid("Choose an OU."));
            }
            let links = by_id(inventory, object_id)?
                .properties
                .get("gpoLinks")
                .and_then(Value::as_array)
                .map(|links| {
                    links
                        .iter()
                        .filter_map(|link| link.get("gpoId").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if gpo_ids.iter().collect::<BTreeSet<_>>().len() != gpo_ids.len()
                || links.len() != gpo_ids.len()
                || links.iter().any(|link| !gpo_ids.contains(link))
            {
                return Err(invalid(
                    "DIRECTORY_LINKS_CHANGED",
                    "Include every current GPO link exactly once.",
                ));
            }
        }
        DirectoryAction::GpoFilter {
            object_id,
            group_ids,
        } => {
            if by_id(inventory, object_id)?.kind != "gpo"
                || group_ids
                    .iter()
                    .any(|id| by_id(inventory, id).is_ok_and(|group| group.kind != "group"))
            {
                return Err(target_invalid("Choose a GPO and security groups."));
            }
        }
        DirectoryAction::GroupMembers {
            object_id,
            member_ids,
            action,
        } => {
            if by_id(inventory, object_id)?.kind != "group" {
                return Err(target_invalid("Choose a group."));
            }
            for id in member_ids {
                let member = by_id(inventory, id)?;
                if !["account", "group", "computer"].contains(&member.kind.as_str()) {
                    return Err(target_invalid(
                        "Choose an account, computer, or nested group.",
                    ));
                }
                if action == "remove" {
                    continue;
                }
                let mut seen = BTreeSet::new();
                let mut todo = vec![id.clone()];
                while let Some(next) = todo.pop() {
                    if next == *object_id {
                        return Err(invalid(
                            "DIRECTORY_GROUP_CYCLE",
                            "Nested groups cannot form a cycle.",
                        ));
                    }
                    if seen.insert(next.clone()) {
                        if let Some(object) = inventory.object(&next) {
                            todo.extend(object.member_ids.iter().cloned());
                        }
                    }
                }
            }
        }
        DirectoryAction::GpoPublish {
            object_id,
            definition,
        } => {
            if object_id
                .as_deref()
                .is_some_and(|id| by_id(inventory, id).is_ok_and(|object| object.kind != "gpo"))
            {
                return Err(target_invalid("Choose a Windows GPO."));
            }
            if object_id.is_some() {
                if let Some(target) = &definition.computer_target_id {
                    if !by_id(inventory, target)
                        .is_ok_and(|object| matches!(object.kind.as_str(), "ou" | "group"))
                    {
                        return Err(target_invalid("Choose an explicit computer group or OU."));
                    }
                }
                return Ok(());
            }
            let target = by_id(inventory, &definition.target_ou_id)?;
            let filter = by_id(inventory, &definition.filter_group_id)?;
            if target.kind != "ou" || filter.kind != "group" {
                return Err(target_invalid(
                    "Choose an OU and a security filter group.",
                ));
            }
            if let Some(computer) = &definition.computer_target_id {
                if !by_id(inventory, computer)
                    .is_ok_and(|object| matches!(object.kind.as_str(), "ou" | "group"))
                {
                    return Err(target_invalid("Choose an explicit computer group or OU."));
                }
                if definition.scope == "computer"
                    && computer != &target.id
                    && computer != &filter.id
                {
                    return Err(target_invalid(
                        "The computer target must be the linked OU or security filter group.",
                    ));
                }
            }
        }
        DirectoryAction::GpoLink {
            object_id,
            parent_id,
            filter_group_id,
            ..
        } => {
            if by_id(inventory, object_id)?.kind != "gpo" {
                return Err(target_invalid("Choose a Windows GPO."));
            }
            let target = by_id(inventory, parent_id)?;
            let filter = filter_group_id
                .as_deref()
                .map(|id| by_id(inventory, id))
                .transpose()?;
            if target.kind != "ou" || filter.is_some_and(|group| group.kind != "group") {
                return Err(target_invalid(
                    "Choose an OU and a security filter group.",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorMapping {
    pub attribute_id: String,
    pub field: String,
    pub direction: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationConfiguration {
    pub connection_id: String,
    pub expected_version: i64,
    pub write_enabled: bool,
    #[serde(default)]
    pub mappings: Option<Vec<ConnectorMapping>>,
}




/// Evidence retained with the operation. The preview token binds the exact
/// input and the evidence the reviewer saw.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewEvidence {
    pub connection_version: i64,
    pub state_version: i64,
    pub inventory: DirectoryInventory,
    pub objects: Vec<String>,
    pub companies: Vec<String>,
    pub identity_links: Vec<(String, Option<String>, String)>,
}

impl PreviewEvidence {
    pub fn value(&self) -> Value {
        json!({
            "connectionVersion": self.connection_version,
            "stateVersion": self.state_version,
            "inventory": self.inventory,
            "objects": self.objects,
            "companies": self.companies,
            "identityLinks": self.identity_links,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PreparedPreview {
    pub token: String,
    pub result: Value,
    pub evidence: PreviewEvidence,
    pub state_version: i64,
    pub write_enabled: bool,
}

pub fn preview_token(input: &DirectoryChangeInput, evidence: &PreviewEvidence) -> String {
    hash_value(&json!({
        "input": input,
        "evidence": evidence.value(),
    }))
}

/// The observed provider identity of a linked alias, when the connector maps
/// the canonical UCS identity to an installed Samba identity.
pub fn windows_identity(object: &DirectoryObject) -> Option<&str> {
    object
        .properties
        .get("windowsIdentity")
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
}

pub fn canonical_uuid() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        static FALLBACK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let value = FALLBACK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return format!("fallback-{value}");
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
