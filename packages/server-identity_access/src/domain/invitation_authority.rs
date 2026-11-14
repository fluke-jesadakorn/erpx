//! Invitation policy consumes retained owner facts; it neither opens databases
//! nor creates an interactive Work selection.
use super::account_credentials::signup_unavailable;
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffInvitationGrant {
    pub role: String,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_safe_i64")]
    pub expires: i64,
}
impl StaffInvitationGrant {
    pub fn supports_scope(&self, scope: &str) -> bool {
        matches!(scope, "holding" | "branch" | "department")
            && (self.role != "it_admin" || scope == "holding")
    }
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        let grant: Self = serde_json::from_str(value).map_err(|_| signup_unavailable())?;
        if grant.role.is_empty() || grant.role.len() > 180 || grant.expires <= 0 {
            return Err(signup_unavailable());
        }
        Ok(grant)
    }
}
/// A directly authorized People access invitation. The reviewed people-access
/// command already created the subject, assignment and review row, so
/// acceptance never links a Person or creates an assignment.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectInvitationGrant {
    pub role: String,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_safe_i64")]
    pub expires: i64,
    pub direct: bool,
    #[serde(rename = "existingUserId", default)]
    pub existing_user_id: Option<String>,
}
/// The two invitation families. `Staff` runs the reviewed person-link and grant
/// activation; `Direct` activates the People access relationship in place.
#[derive(Clone)]
pub enum InvitationGrant {
    Staff(StaffInvitationGrant),
    Direct(DirectInvitationGrant),
}
impl InvitationGrant {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        let raw: serde_json::Value =
            serde_json::from_str(value).map_err(|_| signup_unavailable())?;
        if raw.get("direct").and_then(serde_json::Value::as_bool) == Some(true) {
            let grant: DirectInvitationGrant =
                serde_json::from_value(raw).map_err(|_| signup_unavailable())?;
            if grant.role.is_empty()
                || grant.role.len() > 180
                || grant.expires <= 0
                || grant
                    .existing_user_id
                    .as_deref()
                    .is_some_and(|id| id.is_empty() || id.len() > 180)
            {
                return Err(signup_unavailable());
            }
            return Ok(Self::Direct(grant));
        }
        Ok(Self::Staff(StaffInvitationGrant::parse(value)?))
    }
    pub fn role(&self) -> &str {
        match self {
            Self::Staff(grant) => &grant.role,
            Self::Direct(grant) => &grant.role,
        }
    }
    pub fn expires(&self) -> i64 {
        match self {
            Self::Staff(grant) => grant.expires,
            Self::Direct(grant) => grant.expires,
        }
    }
    pub fn direct(&self) -> Option<&DirectInvitationGrant> {
        match self {
            Self::Direct(grant) => Some(grant),
            Self::Staff(_) => None,
        }
    }
    /// The account that accepts this invitation.
    pub fn target_user(&self, invitation_id: &str) -> String {
        self.direct()
            .and_then(|grant| grant.existing_user_id.clone())
            .unwrap_or_else(|| format!("usr_invite_{invitation_id}"))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewedInvitationGrant {
    pub invitation_id: String,
    pub user_id: String,
    pub role_id: String,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_safe_i64")]
    pub expected_role_revision: i64,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    #[serde(deserialize_with = "erpx_server_shared::public::json_numbers::deserialize_safe_i64")]
    pub valid_until: i64,
    pub parent_id: String,
    pub authorizing_assignment_id: String,
}
/// Explicit scope ancestry: an omitted parent node never acts as a wildcard.
pub fn invitation_scope_covers(
    parent_kind: &str,
    parent_node: Option<&str>,
    requested_kind: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> bool {
    let valid = match requested_kind {
        "holding" => branch.is_none() && department.is_none(),
        "branch" => branch.is_some() && department.is_none(),
        "department" => department.is_some(),
        _ => false,
    };
    valid
        && match parent_kind {
            "holding" => parent_node.is_none(),
            "branch" => {
                matches!(requested_kind, "branch" | "department")
                    && parent_node.is_some()
                    && parent_node == branch
            }
            "department" => {
                requested_kind == "department" && parent_node.is_some() && parent_node == department
            }
            _ => false,
        }
}
