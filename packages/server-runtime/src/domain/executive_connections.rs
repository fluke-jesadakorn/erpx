//! Executive external-connection facts owned by Runtime.
//!
//! A connection is a hashed bearer secret bound to one current Work assignment
//! and organization scope. The secret is minted once and never replayed; only
//! this module decides the bounded request shape and derived status.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;

/// Retained connection cap per user and company, under one principal lock.
pub const EXECUTIVE_CONNECTION_LIMIT: i64 = 20;
/// Retained validity bound for a newly requested connection.
pub const EXECUTIVE_CONNECTION_DAYS_MAX: i64 = 90;
/// Retained listing bound; older connections remain revocable by id.
pub const EXECUTIVE_CONNECTION_LIST_MAX: i64 = 100;
pub const EXECUTIVE_CONNECTION_NAME_MAX: usize = 80;
pub const EXECUTIVE_CONNECTION_SCOPE_MAX: usize = 180;
pub const EXECUTIVE_CONNECTION_SECRET_PREFIX: &str = "erpx_exec_";
pub const EXECUTIVE_CONNECTION_SECRET_LENGTH: usize = 43;
pub const EXECUTIVE_CONNECTION_COMMAND_LENGTH: usize = 36;
pub const EXECUTIVE_CONNECTION_AUDIT_ACTION: &str = "connection.create";

const DAY_MILLIS: i64 = 86_400_000;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn scope_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CONNECTION_SCOPE_FORBIDDEN",
        "Choose a scope covered by the current workspace.",
    )
}

pub fn connection_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "EXECUTIVE_CONNECTION_FORBIDDEN",
        "An active Executive access grant and eligible executive work role are required.",
    )
}

pub fn access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "EXECUTIVE_ACCESS_CHANGED",
        "Select an active work role.",
    )
}

pub fn connection_limit() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONNECTION_LIMIT",
        "Revoke an existing connection before creating another (maximum 20 active connections).",
    )
}

pub fn already_created() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONNECTION_ALREADY_CREATED",
        "This request already created a connection. Its secret cannot be displayed again. Revoke it and create a new connection if the first response was lost.",
    )
}

pub fn connection_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CONNECTION_NOT_FOUND",
        "This connection does not exist.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "EXECUTIVE_UNAVAILABLE",
        "Connection storage is unavailable.",
    )
}

pub fn rate_limited() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "RATE_LIMITED",
        "Executive MCP allows 60 requests per minute per user and company. Retry after the current minute.",
    )
}

/// The exact persisted token row. No secret or hash is ever serialized.
#[derive(Clone, Debug)]
pub struct TokenRow {
    pub id: String,
    pub name: String,
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub assignment_id: String,
    pub assignment_version: i64,
    pub role: String,
    pub assignment_scope_kind: String,
    pub assignment_scope_node_id: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub scope_label: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

/// The current Work assignment a connection is bound to.
#[derive(Clone, Debug)]
pub struct ConnectionActor {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub assignment_id: String,
    pub assignment_version: i64,
    pub role: String,
    pub scope_kind: String,
    pub scope_node_id: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

impl ConnectionActor {
    pub fn principal_key(&self) -> String {
        format!("{}:{}", self.user_id, self.company_id)
    }
}

/// Current assignment facts used only to bound the new token's validity.
#[derive(Clone, Debug)]
pub struct GrantRow {
    pub role: String,
    pub valid_until: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionScope {
    pub id: String,
    pub label: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Active,
    Expired,
    Revoked,
    AccessChanged,
}

impl ConnectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::AccessChanged => "access_changed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalConnection {
    pub id: String,
    pub name: String,
    pub role: String,
    pub scope: String,
    pub status: ConnectionStatus,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalConnections {
    pub can_create: bool,
    pub scopes: Vec<ConnectionScope>,
    pub connections: Vec<ExternalConnection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedConnection {
    pub id: String,
    pub secret: String,
    pub expires_at: i64,
    pub scope: String,
    pub role: String,
}

/// A fully validated create request. `expires_days` defaults to 30.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateConnectionInput {
    pub name: String,
    pub scope_id: String,
    pub expires_days: i64,
}

/// Every column written once for a new connection and its final Control audit.
#[derive(Clone, Debug)]
pub struct NewConnection {
    pub id: String,
    pub secret_hash: String,
    pub name: String,
    pub actor: ConnectionActor,
    pub scope_label: String,
    pub created_at: i64,
    pub expires_at: i64,
}

fn object(
    value: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, BusinessError> {
    value
        .as_object()
        .ok_or_else(|| invalid("CONNECTION_INPUT_INVALID", "Use a connection object."))
}

/// Strict parse of the retained browser contract.
pub fn parse_create(value: &serde_json::Value) -> Result<CreateConnectionInput, BusinessError> {
    let object = object(value)?;
    for key in object.keys() {
        if !matches!(key.as_str(), "name" | "scopeId" | "expiresDays") {
            return Err(invalid(
                "CONNECTION_INPUT_INVALID",
                "Use only name, scopeId and expiresDays.",
            ));
        }
    }
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("CONNECTION_INPUT_INVALID", "Use a connection name."))?
        .trim();
    if name.is_empty() || name.encode_utf16().count() > EXECUTIVE_CONNECTION_NAME_MAX {
        return Err(invalid(
            "CONNECTION_INPUT_INVALID",
            "Use a connection name up to 80 characters.",
        ));
    }
    let scope_id = object
        .get("scopeId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("CONNECTION_INPUT_INVALID", "Choose a connection scope."))?;
    if scope_id.is_empty() || scope_id.encode_utf16().count() > EXECUTIVE_CONNECTION_SCOPE_MAX {
        return Err(invalid(
            "CONNECTION_INPUT_INVALID",
            "Choose a connection scope up to 180 characters.",
        ));
    }
    let expires_days = match object.get("expiresDays") {
        None => 30,
        Some(value) => value
            .as_i64()
            .filter(|days| (1..=EXECUTIVE_CONNECTION_DAYS_MAX).contains(days))
            .ok_or_else(|| {
                invalid(
                    "CONNECTION_INPUT_INVALID",
                    "Choose an expiry between 1 and 90 days.",
                )
            })?,
    };
    Ok(CreateConnectionInput {
        name: name.to_owned(),
        scope_id: scope_id.to_owned(),
        expires_days,
    })
}

/// Strict revoke branch of the same retained POST route.
pub fn parse_revoke(value: &serde_json::Value) -> Result<String, BusinessError> {
    let object = object(value)?;
    if object.len() != 1 || !object.contains_key("revokeId") {
        return Err(invalid("CONNECTION_INPUT_INVALID", "Use one revokeId."));
    }
    let id = object
        .get("revokeId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("CONNECTION_INPUT_INVALID", "Use a connection id."))?;
    validate_command_key(id)?;
    Ok(id.to_owned())
}

/// The retained UUID `Idempotency-Key` is also the token id.
pub fn validate_command_key(value: &str) -> Result<String, BusinessError> {
    let valid = value.len() == EXECUTIVE_CONNECTION_COMMAND_LENGTH
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        });
    if !valid {
        return Err(invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a UUID Idempotency-Key.",
        ));
    }
    Ok(value.to_ascii_lowercase())
}

/// Bearer format retained from the browser-created token contract.
pub fn token_secret(value: &str) -> Option<&str> {
    let secret = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    let body = secret.strip_prefix(EXECUTIVE_CONNECTION_SECRET_PREFIX)?;
    (body.len() == EXECUTIVE_CONNECTION_SECRET_LENGTH
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'))
    .then_some(secret)
}

/// The only status derivation: revocation and expiry are facts; a live token
/// whose current assignment/role/scope no longer resolves is `access_changed`.
pub fn status(token: &TokenRow, now: i64, access_current: bool) -> ConnectionStatus {
    if token.revoked_at.is_some() {
        ConnectionStatus::Revoked
    } else if token.expires_at <= now {
        ConnectionStatus::Expired
    } else if !access_current {
        ConnectionStatus::AccessChanged
    } else {
        ConnectionStatus::Active
    }
}

/// The retained scope label: Company-wide, Branch, or Branch · Department.
pub fn scope_label(kind: &str, display_name: &str, branch_name: Option<&str>) -> String {
    match kind {
        "company" => format!("{display_name} · Company-wide"),
        "branch" => display_name.to_owned(),
        _ => [branch_name.unwrap_or_default(), display_name]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// Connection scope id is a UUID in practice but never parsed as one; the
/// current organization source decides membership.
pub fn selected_scope<'a>(
    scopes: &'a [ConnectionScope],
    scope_id: &str,
) -> Option<&'a ConnectionScope> {
    scopes.iter().find(|scope| scope.id == scope_id)
}

pub fn expires_at(now: i64, days: i64, valid_until: i64) -> i64 {
    now.saturating_add(days.saturating_mul(DAY_MILLIS))
        .min(valid_until)
}

/// Retained one-minute window for the shared MCP request budget.
pub fn rate_window(now: i64) -> i64 {
    (now / 60_000) * 60_000
}

/// Bounded external connection payload; the order is created_at DESC.
pub fn external(token: &TokenRow, now: i64, access_current: bool) -> ExternalConnection {
    ExternalConnection {
        id: token.id.clone(),
        name: token.name.clone(),
        role: token.role.clone(),
        scope: token.scope_label.clone(),
        status: status(token, now, access_current),
        created_at: token.created_at,
        expires_at: token.expires_at,
        last_used_at: token.last_used_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> TokenRow {
        TokenRow {
            id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into(),
            name: "Agent".into(),
            user_id: "user".into(),
            holding_id: "h001".into(),
            company_id: "company".into(),
            assignment_id: "assignment".into(),
            assignment_version: 1,
            role: "holding_owner".into(),
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
            branch_id: None,
            department_id: None,
            scope_label: "Company · Company-wide".into(),
            created_at: 1,
            expires_at: 200,
            revoked_at: None,
            last_used_at: None,
        }
    }

    #[test]
    fn create_input_is_strict_and_bounded() {
        let value = serde_json::json!({"name":"  Agent  ","scopeId":"company","expiresDays":7});
        let input = parse_create(&value).unwrap();
        assert_eq!(input.name, "Agent");
        assert_eq!(input.scope_id, "company");
        assert_eq!(input.expires_days, 7);
        assert_eq!(
            parse_create(&serde_json::json!({"name":"A","scopeId":"c"}))
                .unwrap()
                .expires_days,
            30
        );
        for bad in [
            serde_json::json!({}),
            serde_json::json!({"name":"","scopeId":"c"}),
            serde_json::json!({"name":"A","scopeId":""}),
            serde_json::json!({"name":"A","scopeId":"c","expiresDays":0}),
            serde_json::json!({"name":"A","scopeId":"c","expiresDays":91}),
            serde_json::json!({"name":"A","scopeId":"c","expiresDays":1.5}),
            serde_json::json!({"name":"A","scopeId":"c","extra":true}),
            serde_json::json!({"name":"A","scopeId":"c","expiresDays":"7"}),
        ] {
            assert!(parse_create(&bad).is_err(), "accepted {bad}");
        }
        let long = "x".repeat(EXECUTIVE_CONNECTION_NAME_MAX + 1);
        assert!(parse_create(&serde_json::json!({"name":long,"scopeId":"c"})).is_err());
    }

    #[test]
    fn revoke_input_requires_exactly_one_uuid() {
        let id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        assert_eq!(
            parse_revoke(&serde_json::json!({"revokeId":id})).unwrap(),
            id
        );
        for bad in [
            serde_json::json!({}),
            serde_json::json!({"revokeId":"nope"}),
            serde_json::json!({"revokeId":id,"name":"A"}),
            serde_json::json!({"RevokeId":id}),
        ] {
            assert!(parse_revoke(&bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn command_key_accepts_only_retained_uuid_shape() {
        let id = "AAAAAAAA-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        assert_eq!(validate_command_key(id).unwrap(), id.to_ascii_lowercase());
        for bad in [
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaa",
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaaa",
            "gaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            "aaaaaaaa_aaaa_aaaa_aaaa_aaaaaaaaaaaa",
        ] {
            assert_eq!(
                validate_command_key(bad).unwrap_err().code,
                "COMMAND_KEY_REQUIRED"
            );
        }
    }

    #[test]
    fn bearer_secret_requires_the_exact_prefix_and_length() {
        let secret = format!("erpx_exec_{}", "a".repeat(43));
        assert_eq!(
            token_secret(&format!("Bearer {secret}")),
            Some(secret.as_str())
        );
        assert!(token_secret(&format!("bearer {secret}")).is_some());
        assert!(token_secret("Bearer session-cookie").is_none());
        assert!(token_secret(&format!("Bearer erpx_exec_{}", "a".repeat(42))).is_none());
        assert!(token_secret(&format!("Bearer erpx_exec_{}", "a".repeat(44))).is_none());
        assert!(token_secret(&format!("Bearer erpx_exec_{}", "a/".repeat(21) + "a")).is_none());
    }

    #[test]
    fn status_prefers_revocation_then_expiry_then_current_access() {
        let mut value = token();
        assert_eq!(status(&value, 100, true), ConnectionStatus::Active);
        assert_eq!(status(&value, 100, false), ConnectionStatus::AccessChanged);
        assert_eq!(status(&value, 200, true), ConnectionStatus::Expired);
        value.revoked_at = Some(5);
        assert_eq!(status(&value, 100, true), ConnectionStatus::Revoked);
        assert_eq!(status(&value, 100, false), ConnectionStatus::Revoked);
    }

    #[test]
    fn scope_labels_and_expiry_match_the_retained_payload() {
        assert_eq!(scope_label("company", "Acme", None), "Acme · Company-wide");
        assert_eq!(scope_label("branch", "Bangkok", Some("Acme")), "Bangkok");
        assert_eq!(
            scope_label("department", "Finance", Some("Bangkok")),
            "Bangkok · Finance"
        );
        assert_eq!(scope_label("department", "Finance", None), "Finance");
        assert_eq!(expires_at(10, 2, i64::MAX), 10 + 2 * DAY_MILLIS);
        assert_eq!(expires_at(10, 2, 20), 20);
        assert_eq!(rate_window(120_123), 120_000);
    }
}
