//! Minimal Organization picker facts for the native routine editor.
//!
//! These are the retained `staffSettings` level/department reads, reduced to
//! the columns `staffPickerDisplayOptions` keeps. Both statements are
//! SELECT-only, Company keyed on the caller's connection and keep the original
//! SQL ordering; this owner opens no transaction and reads no other owner's
//! tables.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::PgConnection;

/// One retained staff-settings level option.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutineEditorLevelFact {
    pub id: String,
    pub name: String,
    pub rank: i64,
}

/// One retained active department option.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutineEditorDepartmentFact {
    pub id: String,
    pub display_name: String,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_ROUTINE_EDITOR_UNAVAILABLE",
        "The Organization routine editor fact is unavailable.",
    )
}

/// `listLevels`: all Company levels in the original
/// `level_number ASC NULLS LAST,rank DESC,code` order.
pub async fn routine_editor_levels(
    c: &mut PgConnection,
    company_id: &str,
) -> Result<Vec<RoutineEditorLevelFact>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let rows = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT id,name,rank FROM staff_levels WHERE company_id=$1 \
         ORDER BY level_number ASC NULLS LAST,rank DESC,code",
    )
    .bind(company.as_str())
    .fetch_all(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(rows
        .into_iter()
        .map(|(id, name, rank)| RoutineEditorLevelFact { id, name, rank })
        .collect())
}

/// The retained active `kind='department'` nodes in `display_name` order.
pub async fn routine_editor_departments(
    c: &mut PgConnection,
    company_id: &str,
) -> Result<Vec<RoutineEditorDepartmentFact>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT n.id,n.display_name FROM organization_nodes n \
         WHERE n.company_id=$1 AND n.kind='department' AND n.status='active' \
         ORDER BY n.display_name",
    )
    .bind(company.as_str())
    .fetch_all(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(rows
        .into_iter()
        .map(|(id, display_name)| RoutineEditorDepartmentFact { id, display_name })
        .collect())
}
