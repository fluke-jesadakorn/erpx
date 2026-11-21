//! Organization-owned staff placement port for native People workforce
//! commands. The caller owns the serializable Holding transaction; this
//! adapter checks Organization source facts and writes only the Organization
//! placement history row. People owns its own engagement columns.

use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::json;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const PLACEMENT_INVALID: BusinessError = BusinessError::new(
    FailureKind::InvalidInput,
    "PEOPLE_STAFF_PLACEMENT_INVALID",
    "The selected Organization placement or manager is no longer current.",
);

pub struct StaffPlacementRequest<'a> {
    pub company_id: &'a str,
    pub command_key: &'a str,
    pub person_id: &'a str,
    pub engagement_id: &'a str,
    pub branch_id: &'a str,
    pub department_id: Option<&'a str>,
    pub manager_person_id: Option<&'a str>,
    pub level_id: Option<&'a str>,
    pub position_id: Option<&'a str>,
    pub reason: &'a str,
    pub now: i64,
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_PLACEMENT_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PEOPLE_PLACEMENT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Validate the current Organization placement facts and retain the placement
/// change evidence in `staff_organization_history`.
pub async fn apply_staff_placement(
    connection: &mut PgConnection,
    request: &StaffPlacementRequest<'_>,
) -> Result<(), Error> {
    ready(connection).await?;
    let branch_ok: bool = sqlx::query_scalar(
        "EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 \
         AND kind='branch' AND status='active')",
    )
    .bind(request.branch_id)
    .bind(request.company_id)
    .fetch_one(&mut *connection)
    .await?;
    if !branch_ok {
        return Err(PLACEMENT_INVALID.into());
    }
    if let Some(department) = request.department_id {
        let department_ok: bool = sqlx::query_scalar(
            "EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 \
             AND branch_id=$3 AND kind='department' AND status='active')",
        )
        .bind(department)
        .bind(request.company_id)
        .bind(request.branch_id)
        .fetch_one(&mut *connection)
        .await?;
        if !department_ok {
            return Err(PLACEMENT_INVALID.into());
        }
    }
    if let Some(manager) = request.manager_person_id {
        let manager_ok: bool = sqlx::query_scalar(
            "EXISTS(SELECT 1 FROM hr_people WHERE id=$1 AND company_id=$2 \
             AND status='active' AND merged_into_id IS NULL)",
        )
        .bind(manager)
        .bind(request.company_id)
        .fetch_one(&mut *connection)
        .await?;
        if !manager_ok {
            return Err(PLACEMENT_INVALID.into());
        }
    }
    if let Some(level) = request.level_id {
        let level_ok: bool = sqlx::query_scalar(
            "EXISTS(SELECT 1 FROM staff_levels WHERE id=$1 AND company_id=$2 AND active=1)",
        )
        .bind(level)
        .bind(request.company_id)
        .fetch_one(&mut *connection)
        .await?;
        if !level_ok {
            return Err(PLACEMENT_INVALID.into());
        }
    }
    if let Some(position) = request.position_id {
        let position_ok: bool = sqlx::query_scalar(
            "EXISTS(SELECT 1 FROM organization_positions WHERE id=$1 AND company_id=$2 AND status='active')",
        )
        .bind(position)
        .bind(request.company_id)
        .fetch_one(&mut *connection)
        .await?;
        if !position_ok {
            return Err(PLACEMENT_INVALID.into());
        }
    }
    let before = json!({
        "personId": request.person_id,
        "engagementId": request.engagement_id,
        "branchId": request.branch_id,
        "departmentId": request.department_id,
        "levelId": request.level_id,
        "managerPersonId": request.manager_person_id,
        "positionId": request.position_id,
    });
    sqlx::query(
        "INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,\
         actor_id,reason,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(format!(
        "placement-{}-{}",
        request.command_key, request.engagement_id
    ))
    .bind(request.company_id)
    .bind(request.engagement_id)
    .bind(before.to_string())
    .bind(before.to_string())
    .bind(request.person_id)
    .bind(request.reason)
    .bind(request.command_key)
    .bind(request.now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub fn into_business(error: Error) -> BusinessError {
    match error {
        Error::Business(error) => error,
        Error::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "PEOPLE_STAFF_PLACEMENT_UNAVAILABLE",
            "The Organization staff placement port could not complete.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::PLACEMENT_INVALID;

    #[test]
    fn placement_failure_is_portable() {
        assert_eq!(PLACEMENT_INVALID.code, "PEOPLE_STAFF_PLACEMENT_INVALID");
    }
}
