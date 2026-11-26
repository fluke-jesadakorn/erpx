//! Pure domain types, SQL contract and invariants for the native routine
//! template list read. No I/O, no axum and no sqlx live in this module.
//!
//! This is the read half of the `routine-templates` route family whose command
//! half is already native. The retired TypeScript authority executed the same
//! alias-qualified projection against `routine_templates`; the Rust owner now
//! owns the statement, the row mapping and the response shape.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde::Serialize;

/// Maximum accepted identity bytes. Matches the rest of the Operations owner
/// and keeps unbounded values away from Postgres.
pub const MAX_ID_BYTES: usize = 200;

/// The exact retired projection head (`operationsAliasSelectAll('t',
/// 'routine_templates', …)`) followed by the canonical visibility rule. The
/// `{scope}` marker is replaced only by an owner-owned static predicate from
/// [`crate::application::routine_template_read::TemplateScope`]; no wire value
/// can ever reach the SQL text. Bind order is company, creator, assignment,
/// then the composition scope binds.
pub const TEMPLATE_LIST_SQL: &str = concat!(
    "SELECT t.* FROM routine_templates t WHERE t.company_id=$1",
    " AND (t.published=1 OR t.creator_user_id=$2 OR EXISTS(SELECT 1 FROM assignments a",
    " WHERE a.id=$3 AND a.company_id=t.company_id AND a.revoked_at IS NULL",
    " AND a.role='holding_owner'",
    " AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)))",
    " AND ({scope}) ORDER BY t.updated_at DESC,t.id",
);

/// Deterministic 64-bit FNV-1a used by parity tests as a dependency-free
/// rendered-SQL oracle, mirroring the other native Operations read slices.
pub fn sql_oracle(sql: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in sql.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Render the owner statement with a trusted static scope predicate. The
/// predicate is never sourced from a request body or query string.
pub fn render_list_sql(scope_predicate: &str) -> String {
    TEMPLATE_LIST_SQL.replace("{scope}", scope_predicate)
}

/// Actor facts resolved by the caller's session and selected work context.
/// They are transport facts only; the domain never grants authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateActor {
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub revision: i64,
    pub assignment_version: i64,
}

fn invalid(field: &'static str) -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_ROUTINE_READ_INVALID",
        match field {
            "company" => "Use one bounded Operations company identity.",
            "user" => "Use one bounded Operations user identity.",
            _ => "Use one bounded Operations assignment identity.",
        },
    )
}

/// Identities arriving from headers follow the same bounded character set as
/// the rest of the owner: no whitespace, SQL metacharacters or placeholders.
pub fn validate_identity(value: &str, field: &'static str) -> Result<(), BusinessError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || RecordId::new(value).is_err()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'@')
        })
    {
        return Err(invalid(field));
    }
    Ok(())
}

/// Complete retired `TemplateRow` payload. Keys are deliberately snake_case:
/// the TypeScript consumer forwarded these rows unchanged to the presenter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TemplateRow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub creator_user_id: String,
    pub assignee_user_id: String,
    pub project_id: Option<String>,
    pub start_approvers_json: String,
    pub completion_approvers_json: String,
    pub published: i32,
    pub version: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_bounded() {
        assert!(validate_identity("acme:001", "company").is_ok());
        assert!(validate_identity("user-42", "user").is_ok());
        assert!(validate_identity("assign_1", "assignment").is_ok());
        assert!(validate_identity("", "company").is_err());
        assert!(validate_identity("bad company", "company").is_err());
        assert!(validate_identity("x' OR 1=1", "company").is_err());
        assert!(validate_identity(&"x".repeat(MAX_ID_BYTES + 1), "user").is_err());
    }

    #[test]
    fn rendered_sql_keeps_the_static_scope_and_bind_order() {
        let sql = render_list_sql("TRUE");
        assert!(sql.starts_with("SELECT t.* FROM routine_templates t WHERE t.company_id=$1"));
        assert!(sql.contains("t.creator_user_id=$2"));
        assert!(sql.contains("a.id=$3"));
        assert!(sql.contains("AND (TRUE) ORDER BY t.updated_at DESC,t.id"));
        assert!(!sql.contains('?'));
        // Oracle pinned from the byte-identical retired projection rendering.
        assert_eq!(sql_oracle(&sql), 0x7f4f9bc4e6205b44);
        assert_eq!(sql, render_list_sql("TRUE"));
    }

    #[test]
    fn payload_keys_match_the_retired_row() {
        let row = TemplateRow {
            id: "routine-1".into(),
            title: "Weekly close".into(),
            description: String::new(),
            company_id: "acme".into(),
            branch_id: None,
            department_id: None,
            creator_user_id: "u-1".into(),
            assignee_user_id: "u-2".into(),
            project_id: None,
            start_approvers_json: "[]".into(),
            completion_approvers_json: "[]".into(),
            published: 1,
            version: 2,
            created_at: 100,
            updated_at: 200,
        };
        let value = serde_json::to_value(&row).unwrap();
        let object = value.as_object().unwrap();
        for key in [
            "id",
            "title",
            "description",
            "company_id",
            "branch_id",
            "department_id",
            "creator_user_id",
            "assignee_user_id",
            "project_id",
            "start_approvers_json",
            "completion_approvers_json",
            "published",
            "version",
            "created_at",
            "updated_at",
        ] {
            assert!(object.contains_key(key), "missing response field {key}");
        }
        assert_eq!(object.len(), 15);
        assert_eq!(object["published"], serde_json::json!(1));
        assert_eq!(object["project_id"], serde_json::Value::Null);
    }
}
