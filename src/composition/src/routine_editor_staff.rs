//! Native routine-editor staff facts assembled from owner ports.
//!
//! This is composition only: People owns the staff shortlist and acting-staff
//! fact, Organization owns the level/department options, and this module
//! shapes them into the `staffPickerDisplayOptions(staffSettings(actor))`
//! shortlist. All reads use the caller's existing Holding transaction.
use erpx_server_organization::public::routine_editor_staff::{
    routine_editor_departments, routine_editor_levels,
};
use erpx_server_people_payroll::public::staff_facts::{active_staff, staff_picker_people};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, UserId};
use serde_json::{Value, json};
use sqlx::PgConnection;

/// Complete native routine-editor staff context: the exact picker options and
/// whether the acting user is current active staff. No owner SQL or fallback
/// lives here.
pub async fn routine_editor_staff(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let company = CompanyId::new(company_id).map_err(PgTransactionError::Business)?;
    let user = UserId::new(user_id).map_err(PgTransactionError::Business)?;
    let people = staff_picker_people(connection, company.as_str())
        .await
        .map_err(PgTransactionError::Business)?;
    let levels = routine_editor_levels(connection, company.as_str())
        .await
        .map_err(PgTransactionError::Business)?;
    let departments = routine_editor_departments(connection, company.as_str())
        .await
        .map_err(PgTransactionError::Business)?;
    let can_write = active_staff(connection, company.as_str(), user.as_str())
        .await
        .map_err(PgTransactionError::Business)?
        .is_some();
    Ok(json!({
        "options": {
            "people": people
                .into_iter()
                .map(|person| json!({
                    "user_id": person.user_id,
                    "legal_name": person.legal_name,
                    "department_id": person.department_id,
                    "level_id": person.level_id,
                    "level_name": person.level_name,
                }))
                .collect::<Vec<_>>(),
            "levels": levels
                .into_iter()
                .map(|level| json!({
                    "id": level.id,
                    "name": level.name,
                    "rank": level.rank,
                }))
                .collect::<Vec<_>>(),
            "departments": departments
                .into_iter()
                .map(|department| json!({
                    "id": department.id,
                    "display_name": department.display_name,
                }))
                .collect::<Vec<_>>(),
        },
        "canWrite": can_write,
    }))
}
