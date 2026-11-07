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

#[test]
fn workspace_branch_only_scope_and_rejections_are_exact(){
 use erpx_server_work_projection::public::workspace_preference::*;
 assert_eq!(workspace_preference_scope("/holdings/acme/branches/bangkok/home","acme").unwrap(),WorkspacePreferenceScope{branch_slug:Some("bangkok".into()),department_slug:None});
 // A 128-character slug is the longest accepted selector; 129 is rejected.
 let longest="a".repeat(128);
 let ok_path=format!("/holdings/acme/branches/{longest}/home");
 assert!(workspace_preference_scope(&ok_path,"acme").is_ok());
 let too_long=format!("/holdings/acme/branches/{}/home","a".repeat(129));
 let long_holding="a".repeat(129);
 let long_holding_path=format!("/holdings/{long_holding}/home");
 let overlong="/holdings/acme/branches/".to_owned()+&"a".repeat(5000);
 let rejected=[
  "".to_owned(),
  "/holdings/acme".to_owned(),
  "/holdings/acme2/home".to_owned(),
  "/holdings/acme/home/".to_owned(),
  "/holdings/acme/home/extra".to_owned(),
  "/holdings/acme/holdings/acme/home".to_owned(),
  "/holdings/acme/branches/home".to_owned(),
  "/holdings/acme/branches//home".to_owned(),
  "/holdings/acme/branches/Bangkok/home".to_owned(),
  "/holdings/acme/branches/bangkok.io/home".to_owned(),
  "/holdings/acme/BRANCHES/bangkok/home".to_owned(),
  "/holdings/acme/branches/bang\u{0}kok/home".to_owned(),
  too_long,
  overlong,
 ];
 for path in &rejected{assert_eq!(workspace_preference_scope(path,"acme").unwrap_err().code,"INVALID_WORKSPACE", "path: {path:?}");}
 assert_eq!(workspace_preference_scope(&long_holding_path,&long_holding).unwrap_err().code,"INVALID_WORKSPACE");
 assert_eq!(workspace_preference_scope("/holdings/ACME/home","ACME").unwrap_err().code,"INVALID_WORKSPACE");
}

#[test]
fn restore_saved_cannot_override_a_live_row_and_empty_eligible_stays_unselected(){
 let a=role("a",1);
 assert_eq!(
  initial_work_role(WorkRoleInitialization::RestoreSaved,true,Some(&a),std::slice::from_ref(&a),std::slice::from_ref(&a)),
  InitialWorkRole::PreserveCurrent
 );
 // A single account grant without an eligible scope never auto-selects.
 assert_eq!(
  initial_work_role(WorkRoleInitialization::InitializeWorkspace,false,None,std::slice::from_ref(&a),&[]),
  InitialWorkRole::Unselected
 );
 // No grants and no preference: unselected under both purposes.
 for purpose in [WorkRoleInitialization::RestoreSaved,WorkRoleInitialization::InitializeWorkspace]{
  assert_eq!(initial_work_role(purpose,false,None,&[],&[]),InitialWorkRole::Unselected);
 }
}
