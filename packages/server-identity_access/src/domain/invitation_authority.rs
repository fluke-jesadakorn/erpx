//! Invitation policy consumes retained owner facts; it neither opens databases
//! nor creates an interactive Work selection.
use super::account_credentials::signup_unavailable;
use erpx_server_shared::public::BusinessError;
use serde::Deserialize;

#[derive(Deserialize)]
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
