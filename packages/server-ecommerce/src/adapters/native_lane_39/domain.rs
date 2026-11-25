//! Lane 39 Ecommerce domain: pure response types, validation and invariants
//! for the owner-executed workspace channel/cart list family.
//!
//! Both relations are Ecommerce-owned (`ecommerce_channels`, `ecommerce_carts`).
//! No cross-owner projection, lock, receipt or write authority is involved.

use erpx_server_shared::public::BusinessError;

pub const MAX_ID_LEN: usize = 200;
pub const MAX_ROWS: usize = 5000;

/// The caller's already-authorized workspace scope. Branch/department are
/// `None` for a Company-wide assignment and become SQL NULL bind predicates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceScope<'a> {
    pub company_id: &'a str,
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub channel_type: String,
    pub configuration_json: String,
    pub status: String,
    pub created_by: String,
    pub reviewed_by: Option<String>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CartRow {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
    pub channel_id: String,
    pub party_id: String,
    pub warehouse_id: String,
    pub currency: String,
    pub status: String,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceLists {
    pub channels: Vec<ChannelRow>,
    pub carts: Vec<CartRow>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Validate the scope before it becomes ordered bind values.
pub fn validate_scope(scope: &WorkspaceScope<'_>) -> Result<(), BusinessError> {
    if !valid_id(scope.company_id)
        || !scope.branch_id.map(valid_id).unwrap_or(true)
        || !scope.department_id.map(valid_id).unwrap_or(true)
    {
        return Err(invalid(
            "ECOMMERCE_WORKSPACE_LIST_REQUEST_INVALID",
            "Use one bounded Ecommerce workspace scope.",
        ));
    }
    Ok(())
}

fn valid_channel(row: &ChannelRow) -> bool {
    valid_id(&row.id)
        && row.company_id.len() <= MAX_ID_LEN
        && !row.name.is_empty()
        && matches!(row.channel_type.as_str(), "web" | "marketplace" | "social")
        && matches!(row.status.as_str(), "draft" | "active" | "rejected" | "retired")
        && row.reviewed_by.as_deref().map(valid_id).unwrap_or(true)
        && row.version >= 1
}

fn valid_cart(row: &CartRow) -> bool {
    valid_id(&row.id)
        && row.company_id.len() <= MAX_ID_LEN
        && valid_id(&row.branch_id)
        && row.department_id.as_deref().map(valid_id).unwrap_or(true)
        && matches!(row.status.as_str(), "open" | "checked_out" | "abandoned")
        && row.version >= 1
}

/// Refuse drifted or oversized rows instead of emitting a partial projection.
pub fn project(lists: WorkspaceLists) -> Result<WorkspaceLists, BusinessError> {
    if lists.channels.len() > MAX_ROWS || lists.carts.len() > MAX_ROWS {
        return Err(invalid(
            "ECOMMERCE_WORKSPACE_LIST_RESPONSE_INVALID",
            "The native Ecommerce workspace returned too many rows.",
        ));
    }
    if !lists.channels.iter().all(valid_channel) || !lists.carts.iter().all(valid_cart) {
        return Err(invalid(
            "ECOMMERCE_WORKSPACE_LIST_RESPONSE_INVALID",
            "The native Ecommerce workspace returned an invalid row.",
        ));
    }
    Ok(lists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_validation_is_bounded_and_company_is_required() {
        assert!(validate_scope(&WorkspaceScope {
            company_id: "co-1",
            branch_id: None,
            department_id: None,
        })
        .is_ok());
        assert_eq!(
            validate_scope(&WorkspaceScope {
                company_id: "",
                branch_id: None,
                department_id: None,
            })
            .unwrap_err()
            .code,
            "ECOMMERCE_WORKSPACE_LIST_REQUEST_INVALID"
        );
        assert!(validate_scope(&WorkspaceScope {
            company_id: "co-1",
            branch_id: Some("bad id"),
            department_id: None,
        })
        .is_err());
    }

    fn channel() -> ChannelRow {
        ChannelRow {
            id: "channel-1".into(),
            company_id: "co-1".into(),
            name: "Web".into(),
            channel_type: "web".into(),
            configuration_json: "{}".into(),
            status: "active".into(),
            created_by: "user-1".into(),
            reviewed_by: None,
            version: 1,
            created_at: 1,
            updated_at: 2,
        }
    }

    fn cart() -> CartRow {
        CartRow {
            id: "cart-1".into(),
            company_id: "co-1".into(),
            branch_id: "branch-1".into(),
            department_id: None,
            channel_id: "channel-1".into(),
            party_id: "party-1".into(),
            warehouse_id: "warehouse-1".into(),
            currency: "THB".into(),
            status: "open".into(),
            created_by: "user-1".into(),
            version: 1,
            created_at: 1,
            updated_at: 2,
        }
    }

    #[test]
    fn projection_accepts_valid_rows_and_refuses_drift() {
        assert_eq!(
            project(WorkspaceLists {
                channels: vec![channel()],
                carts: vec![cart()],
            })
            .unwrap()
            .channels
            .len(),
            1
        );
        let mut bad = cart();
        bad.status = "unknown".into();
        assert_eq!(
            project(WorkspaceLists {
                channels: vec![],
                carts: vec![bad],
            })
            .unwrap_err()
            .code,
            "ECOMMERCE_WORKSPACE_LIST_RESPONSE_INVALID"
        );
    }
}
