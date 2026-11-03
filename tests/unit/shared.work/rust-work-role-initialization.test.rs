use erpx_server_shared::public::AssignmentId;
use erpx_server_work_projection::public::work_role_store::*;
fn role(id: &str, version: i64) -> WorkRoleReference {
    WorkRoleReference {
        id: AssignmentId::new(id).unwrap(),
        version,
    }
}
#[test]
fn another_devices_preference_never_replaces_any_live_context() {
    let remembered = role("remembered", 1);
    let available = role("available", 2);
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::InitializeWorkspace,
            true,
            Some(&remembered),
            &[remembered.clone(), available.clone()],
            &[available]
        ),
        InitialWorkRole::PreserveCurrent
    );
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::InitializeWorkspace,
            true,
            None,
            &[],
            &[]
        ),
        InitialWorkRole::PreserveCurrent
    );
}
#[test]
fn new_session_restores_only_the_actual_original_assignment_version() {
    let old = role("remembered", 1);
    let current = role("remembered", 2);
    let other = role("other", 1);
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::InitializeWorkspace,
            false,
            Some(&old),
            &[current.clone(), other.clone()],
            &[other]
        ),
        InitialWorkRole::Unselected
    );
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::InitializeWorkspace,
            false,
            Some(&current),
            std::slice::from_ref(&current),
            &[]
        ),
        InitialWorkRole::Select {
            assignment: current,
            restoring: true
        }
    );
}
#[test]
fn one_eligible_scope_does_not_silently_choose_between_multiple_account_grants() {
    let a = role("company", 1);
    let b = role("branch", 1);
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::InitializeWorkspace,
            false,
            None,
            &[a.clone(), b.clone()],
            &[b]
        ),
        InitialWorkRole::Unselected
    );
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::InitializeWorkspace,
            false,
            None,
            std::slice::from_ref(&a),
            std::slice::from_ref(&a)
        ),
        InitialWorkRole::Select {
            assignment: a,
            restoring: false
        }
    );
}

#[test]
fn listing_role_options_restores_a_saved_role_but_never_auto_selects_a_fresh_role() {
    let a = role("only-grant", 1);
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::RestoreSaved,
            false,
            None,
            std::slice::from_ref(&a),
            std::slice::from_ref(&a)
        ),
        InitialWorkRole::Unselected
    );
    assert_eq!(
        initial_work_role(
            WorkRoleInitialization::RestoreSaved,
            false,
            Some(&a),
            std::slice::from_ref(&a),
            std::slice::from_ref(&a)
        ),
        InitialWorkRole::Select {
            assignment: a,
            restoring: true
        }
    );
}

#[test]
fn workspace_preferences_preserve_exact_holding_and_nested_scope_selectors(){
 use erpx_server_work_projection::public::workspace_preference::*;
 assert_eq!(workspace_preference_scope("/holdings/acme/home","acme").unwrap(),WorkspacePreferenceScope{branch_slug:None,department_slug:None});
 assert_eq!(workspace_preference_scope("/holdings/acme/branches/bangkok/departments/engineering/home","acme").unwrap(),WorkspacePreferenceScope{branch_slug:Some("bangkok".into()),department_slug:Some("engineering".into())});
 assert_eq!(workspace_preference_scope("/holdings/acme/departments/accounting/home","acme").unwrap().department_slug.as_deref(),Some("accounting"));
 for path in ["/holdings/northstar/home","/holdings/acme-extra/home","https://acme.example.test/holdings/acme/home","/holdings/acme/branches/../home","/holdings/acme/branches/%2e%2e/home","/holdings/acme//home","/holdings/acme/home?company=other","/holdings/acme/home#other","/platform/home"]{assert_eq!(workspace_preference_scope(path,"acme").unwrap_err().code,"INVALID_WORKSPACE");}
}
