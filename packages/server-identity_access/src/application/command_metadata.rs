use crate::domain::{
    command_metadata::CommandIamProjection,
    effective_authority::{AuthorizedWorkContext, authority_error},
};
use erpx_server_shared::public::BusinessError;
use std::collections::BTreeSet;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Borrow the source snapshot retained by the real current-authority coordinator.
/// No second query, environment source, default role or permission is introduced.
pub fn project_command_iam(
    context: &AuthorizedWorkContext,
) -> Result<CommandIamProjection, BusinessError> {
    project(context, None)
}

/// Explicit current queued-command metadata after the owner's original witness
/// has been checked by PgQueuedWorkAuthorizer. The copied projection itself is
/// never sufficient to perform or resume queued work.
pub fn project_queued_command_iam(
    context: &AuthorizedWorkContext,
) -> Result<CommandIamProjection, BusinessError> {
    project(context, Some(context.selection().assignment().version))
}
fn project(
    context: &AuthorizedWorkContext,
    queued: Option<i64>,
) -> Result<CommandIamProjection, BusinessError> {
    let facts = context.command_iam_metadata();
    let aliases: BTreeSet<_> = facts.person_user_ids.iter().collect();
    let safe = |value: i64, minimum: i64| (minimum..=MAX_SAFE_INTEGER).contains(&value);
    if !safe(facts.role_revision, 1)
        || !safe(facts.identity_version, 0)
        || facts
            .hr_appointment_version
            .is_some_and(|value| !safe(value, 1))
        || facts.emergency_until.is_some_and(|value| !safe(value, 0))
        || facts.fresh_until.is_some_and(|value| !safe(value, 0))
        || queued.is_some_and(|value| !safe(value, 1))
        || facts.role_revision != context.selection().role_revision()
        || facts.identity_version != context.identity_version()
        || facts.hr_appointment_version != context.hr_appointment_version()
        || facts.emergency_until != context.emergency_until()
        || facts.principal_id != context.principal_id()
        || facts.person_id.as_ref() != context.person_id()
        || facts.principal_id.is_empty()
        || facts.person_user_ids.first() != Some(context.user_id())
        || aliases.len() != facts.person_user_ids.len()
        || !facts
            .person_user_ids
            .iter()
            .all(|id| context.same_person_user(id))
        || facts.grantable_permissions != context.grantable_permissions()
        || facts.emergency_until.is_some() && facts.fresh_until.is_some()
        || facts
            .fresh_until
            .is_some_and(|deadline| deadline < context.expires_at())
    {
        return Err(authority_error(
            "COMMAND_IAM_METADATA_UNAVAILABLE",
            "The current command authority metadata is unavailable.",
        ));
    }
    Ok(CommandIamProjection {
        person_user_ids: facts.person_user_ids.clone(),
        emergency_until: facts.emergency_until,
        role_revision: facts.role_revision,
        identity_version: facts.identity_version,
        permissions: facts.permissions.clone(),
        hr_appointment_version: facts.hr_appointment_version,
        grantable_permissions: facts.grantable_permissions.clone(),
        principal_id: facts.principal_id.clone(),
        person_id: facts.person_id.clone(),
        fresh_until: facts.fresh_until,
        queued_assignment_version: queued,
    })
}
