//! Pure domain contract for the native routine-template detail read.
//!
//! This is the native read half of the `routine-templates` route family. The
//! TypeScript `getTemplate` statement and export are retired; the Rust owner
//! owns the exact statement, the scope tail and the complete fifteen-field
//! row; no axum, sqlx or I/O lives in this module.

use crate::domain::routine_template_read::MAX_ID_BYTES;
use erpx_server_shared::public::{BusinessError, RecordId};

/// The retired alias-qualified projection with the canonical detail tail. The
/// `{scope}` marker accepts only an owner-owned static predicate produced by
/// [`crate::application::routine_template_read::TemplateScope`]; no request
/// value can reach the SQL text. Bind order is id, company, scope binds.
pub const TEMPLATE_DETAIL_SQL: &str = concat!(
    "SELECT t.* FROM routine_templates t WHERE t.id=$1 AND t.company_id=$2",
    " AND ({scope})",
);

/// Default owner scope. Composition may replace it with a purchased-capability
/// predicate via the application scope port.
pub const DEFAULT_SCOPE: &str = "TRUE";

/// Render the owner statement with a trusted static scope predicate.
pub fn render_detail_sql(scope_predicate: &str) -> String {
    TEMPLATE_DETAIL_SQL.replace("{scope}", scope_predicate)
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_ROUTINE_DETAIL_INVALID",
        "Use one bounded routine template id.",
    )
}

/// Template ids arrive from the URL path. They use the same bounded character
/// set as other Operations identities: no whitespace, SQL metacharacters or
/// placeholders, and never longer than the owner identity limit.
pub fn validate_template_id(value: &str) -> Result<(), BusinessError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || RecordId::new(value).is_err()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::routine_template_read::TemplateRow;

    #[test]
    fn detail_id_is_bounded() {
        assert!(validate_template_id("routine-1").is_ok());
        assert!(validate_template_id("routine:1_2.3").is_ok());
        assert!(validate_template_id("").is_err());
        assert!(validate_template_id("bad id").is_err());
        assert!(validate_template_id("x' OR 1=1").is_err());
        assert!(validate_template_id(&"x".repeat(MAX_ID_BYTES + 1)).is_err());
    }

    #[test]
    fn rendered_sql_keeps_the_static_scope_and_bind_order() {
        let sql = render_detail_sql(DEFAULT_SCOPE);
        assert_eq!(
            sql,
            "SELECT t.* FROM routine_templates t WHERE t.id=$1 AND t.company_id=$2 AND (TRUE)"
        );
        assert!(!sql.contains('?'));
        assert_eq!(sql, render_detail_sql(DEFAULT_SCOPE));
    }

    #[test]
    fn payload_shape_matches_the_retired_template_row() {
        let value = serde_json::to_value(TemplateRow {
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
        })
        .unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 15);
        assert_eq!(object["id"], serde_json::json!("routine-1"));
        assert_eq!(object["published"], serde_json::json!(1));
        assert_eq!(object["project_id"], serde_json::Value::Null);
    }
}
