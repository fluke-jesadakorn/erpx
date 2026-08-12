use erpx_identity_access::{PrincipalList, PrincipalView, RoleList, RoleView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalItem {
    pub id: String,
    pub display_name: String,
    pub status: String,
    pub assignment_count: usize,
}

impl From<&PrincipalView> for PrincipalItem {
    fn from(value: &PrincipalView) -> Self {
        Self {
            id: value.principal_id.as_uuid().to_string(),
            display_name: value.display_name.clone(),
            status: format!("{:?}", value.status).to_ascii_lowercase(),
            assignment_count: value.assignments.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleItem {
    pub id: String,
    pub code: String,
    pub display_name: String,
    pub permission_count: usize,
}

impl From<&RoleView> for RoleItem {
    fn from(value: &RoleView) -> Self {
        Self {
            id: value.role_id.as_uuid().to_string(),
            code: value.code.as_str().into(),
            display_name: value.display_name.clone(),
            permission_count: value.permissions.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityAccessContent {
    Principal(PrincipalItem),
    Principals {
        values: Vec<PrincipalItem>,
        next_cursor: Option<String>,
    },
    Role(RoleItem),
    Roles {
        values: Vec<RoleItem>,
        next_cursor: Option<String>,
    },
}

impl From<&PrincipalView> for IdentityAccessContent {
    fn from(value: &PrincipalView) -> Self {
        Self::Principal(value.into())
    }
}

impl From<&PrincipalList> for IdentityAccessContent {
    fn from(value: &PrincipalList) -> Self {
        Self::Principals {
            values: value.principals.iter().map(Into::into).collect(),
            next_cursor: value.next_cursor.clone(),
        }
    }
}

impl From<&RoleView> for IdentityAccessContent {
    fn from(value: &RoleView) -> Self {
        Self::Role(value.into())
    }
}

impl From<&RoleList> for IdentityAccessContent {
    fn from(value: &RoleList) -> Self {
        Self::Roles {
            values: value.roles.iter().map(Into::into).collect(),
            next_cursor: value.next_cursor.clone(),
        }
    }
}
