//! Public surface for native migration lane 27 (projects).
pub use crate::adapters::native_lane_27::*;

use erpx_platform_shared_server::public::BusinessError;
use sqlx::PgConnection;

/// Read the current Holding actor's routine Project options on the caller's
/// already-connected connection. Visibility is the exact `listProjects`
/// Company/owner-or-member predicate with `updated_at DESC` ordering; the
/// purchase and scheduled-capability policy is bound by composition, not here.
pub async fn read_routine_project_options(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
) -> Result<Vec<RoutineProjectOption>, BusinessError> {
    let scope = company_scope(company_id, user_id)?;
    PgProjectActualsRead::routine_project_options(connection, &scope.company, &scope.user).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routine_option_wire_shape_is_only_id_and_title() {
        let value = serde_json::to_value(RoutineProjectOption {
            id: "project-1".into(),
            title: "Alpha".into(),
        })
        .unwrap();
        assert_eq!(value, json!({"id":"project-1","title":"Alpha"}));
        assert_eq!(value.as_object().unwrap().len(), 2);
    }
}
