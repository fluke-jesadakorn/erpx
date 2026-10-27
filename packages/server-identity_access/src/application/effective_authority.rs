use crate::domain::{
    effective_authority::*,
    permission_catalog::PermissionCatalog,
    work_admission::{WorkModuleCatalog, beta_destination_allowed},
    work_context::WorkContextRequest,
};
use erpx_server_shared::public::BusinessError;
use std::collections::BTreeSet;

pub(crate) fn authorize_effective_work(
    request: &WorkContextRequest,
    access: &WorkAuthorizationRequest,
    facts: EffectiveAuthorityFacts,
    catalog: &PermissionCatalog,
    modules: &WorkModuleCatalog,
    now: i64,
) -> Result<AuthorizedWorkContext, BusinessError> {
    if facts.iam.role.id != facts.selection.assignment().role
        || facts.iam.role.revision != facts.selection.role_revision()
    {
        return Err(authority_error(
            "ROLE_UNAVAILABLE",
            "This role is unavailable.",
        ));
    }
    let evaluated = evaluate_current_iam_authority(&facts.iam, now)?;
    let expires_at = facts
        .selection
        .expires_at()
        .min(evaluated.fresh_until.unwrap_or(i64::MAX));
    let mut person_aliases = vec![request.user_id.clone()];
    for alias in &facts.iam.aliases {
        if !person_aliases.contains(alias) {
            person_aliases.push(alias.clone());
        }
    }
    let command_iam = CommandIamMetadata {
        person_user_ids: person_aliases,
        emergency_until: evaluated.emergency_until,
        role_revision: facts.iam.role.revision,
        identity_version: facts.iam.member.as_ref().map_or(0, |m| m.version),
        permissions: evaluated.permissions.clone(),
        hr_appointment_version: evaluated.hr_appointment_version,
        grantable_permissions: facts.iam.role.grantable_permissions.clone(),
        principal_id: facts
            .iam
            .member
            .as_ref()
            .map(|m| m.principal_id.clone())
            .unwrap_or_else(|| format!("local:{}", request.user_id.as_str())),
        person_id: facts
            .iam
            .member
            .as_ref()
            .and_then(|m| m.person_id.clone())
            .or(facts.iam.fallback_person_id.clone()),
        fresh_until: facts
            .iam
            .member
            .as_ref()
            .filter(|member| member.login_policy == LoginPolicy::Directory)
            .and(evaluated.fresh_until),
    };
    let permissions = evaluated.permissions;
    let effective_modules = modules.effective_modules(
        &facts.iam.role.id,
        &facts.selection.assignment().modules,
        &permissions,
        facts.beta,
    );
    if facts.beta && !beta_destination_allowed(&access.destination, access.api) {
        return Err(authority_error(
            "BETA_CAPABILITY_UNAVAILABLE",
            "This capability is not part of the public beta.",
        ));
    }
    if access
        .module
        .as_ref()
        .is_some_and(|module| !effective_modules.contains(module))
    {
        return Err(authority_error(
            "MODULE_UNAVAILABLE",
            "This module is unavailable for the current role.",
        ));
    }
    let scope = if facts.selection.workspace().department_id.is_some() {
        "department"
    } else if facts.selection.workspace().branch_id.is_some() {
        "branch"
    } else {
        "holding"
    };
    let raw_permissions: BTreeSet<String> = permissions.into_iter().collect();
    let permissions = catalog
        .definitions()
        .iter()
        .filter(|p| catalog.allows_current_permissions(&p.id, scope, true, &raw_permissions))
        .map(|p| p.id.clone())
        .collect::<BTreeSet<_>>();
    if !permissions.contains(&access.permission) {
        return Err(authority_error(
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    let mut person_user_ids: BTreeSet<_> = facts.iam.aliases.into_iter().collect();
    person_user_ids.insert(request.user_id.clone());
    let context = AuthorizedWorkContext {
        holding_id: request.holding_id.clone(),
        user_id: request.user_id.clone(),
        session_id: request.session_id.clone(),
        person_id: facts
            .iam
            .member
            .as_ref()
            .and_then(|m| m.person_id.clone())
            .or(facts.iam.fallback_person_id),
        principal_id: facts
            .iam
            .member
            .as_ref()
            .map(|m| m.principal_id.clone())
            .unwrap_or_else(|| format!("local:{}", request.user_id.as_str())),
        identity_version: facts.iam.member.as_ref().map_or(0, |m| m.version),
        hr_appointment_version: evaluated.hr_appointment_version,
        emergency_until: evaluated.emergency_until,
        grantable_permissions: facts.iam.role.grantable_permissions,
        selection: facts.selection,
        permissions,
        modules: effective_modules,
        person_user_ids,
        expires_at,
        command_iam,
    };
    context.require_current_time(now)?;
    Ok(context)
}

/// Pure current IAM evaluation for retained original-source operations that do
/// not own an interactive Work selection (for example reviewed invitations).
/// This produces facts, never a sealed permission-bearing Work context.
pub(crate) fn evaluate_current_iam_authority(
    facts: &CurrentIamAuthorityFacts,
    now: i64,
) -> Result<EvaluatedIamAuthority, BusinessError> {
    if facts.role.id.is_empty()
        || facts.role.revision < 1
        || facts
            .role
            .scopes
            .iter()
            .any(|s| !matches!(s.as_str(), "holding" | "branch" | "department" | "platform"))
    {
        return Err(authority_error(
            "ROLE_UNAVAILABLE",
            "This role is unavailable.",
        ));
    }
    let mut fresh_until: Option<i64> = None;
    if let Some(member) = &facts.member {
        if !member.active || member.version <= 0 {
            return Err(authority_error(
                "IDENTITY_SUSPENDED",
                "This identity no longer has access to this company.",
            ));
        }
        if member.login_policy == LoginPolicy::Directory {
            let deadline = member
                .directory_observed_at
                .checked_add(300000)
                .ok_or_else(|| {
                    authority_error(
                        "DIRECTORY_ACCESS_UNAVAILABLE",
                        "Your directory access needs a successful refresh.",
                    )
                })?;
            if !member.directory_enabled || !member.directory_connection_active || deadline <= now {
                return Err(authority_error(
                    "DIRECTORY_ACCESS_UNAVAILABLE",
                    "Your directory access needs a successful refresh.",
                ));
            }
            fresh_until = Some(fresh_until.map_or(deadline, |until| until.min(deadline)));
        }
        if member.login_policy == LoginPolicy::Emergency {
            let until = facts
                .emergency_until
                .filter(|until| *until > now)
                .ok_or_else(|| {
                    authority_error(
                        "EMERGENCY_ACCESS_REQUIRED",
                        "Verify MFA and activate a temporary emergency session.",
                    )
                })?;
            fresh_until = Some(fresh_until.map_or(until, |deadline| deadline.min(until)));
        }
    }
    let appointment = facts.hr.as_ref().filter(|hr| {
        hr.version > 0
            && hr.active_person
            && hr.unmerged_person
            && hr.active_engagement
            && hr.person_user_id.is_some()
            && hr.same_person_as_principal
    });
    let permissions = effective_authority_permissions(
        &facts.role.id,
        &facts.role.permissions,
        appointment.is_some(),
    );
    Ok(EvaluatedIamAuthority {
        permissions,
        hr_appointment_version: appointment.map(|hr| hr.version),
        emergency_until: facts
            .member
            .as_ref()
            .filter(|m| m.login_policy == LoginPolicy::Emergency)
            .and(facts.emergency_until),
        fresh_until,
    })
}
