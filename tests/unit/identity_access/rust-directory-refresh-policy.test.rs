use erpx_server_identity_access::public::{directory::refresh_rules::*, work_context::*};
use erpx_server_shared::public::{AssignmentId, CompanyId, RecordId, UserId};
const NOW: i64 = 1_800_000_000_000;
fn member() -> DirectoryRefreshMemberFacts {
    DirectoryRefreshMemberFacts {
        active: true,
        version: 3,
        connection_active: true,
        link_matches: true,
        identity_enabled: true,
        observed_at: NOW,
    }
}
fn fixture() -> (HoldingWorkspaceRequest, DirectoryRefreshSelectionFacts) {
    let company = CompanyId::new("company").unwrap();
    let assignment = AssignmentId::new("grant").unwrap();
    let user = UserId::new("actual-user").unwrap();
    let session = RecordId::new("actual-child").unwrap();
    (
        HoldingWorkspaceRequest {
            company_id: company.clone(),
            branch_slug: None,
            department_slug: None,
            expected: Some(ExpectedWorkSelection {
                revision: 7,
                assignment_id: assignment.clone(),
                assignment_version: 3,
            }),
            mutation: true,
        },
        DirectoryRefreshSelectionFacts {
            revoked: false,
            workspace: WorkspaceLineageFact {
                company_id: company.clone(),
                branch_id: None,
                department_id: None,
                branch_slug: None,
                department_slug: None,
            },
            selection: ActiveSelectionFact {
                session_id: session,
                user_id: user.clone(),
                company_id: company.clone(),
                assignment_id: Some(assignment.clone()),
                assignment_version: Some(3),
                revision: 7,
                expires_at: NOW + 1000,
            },
            assignment: WorkAssignmentFact {
                id: assignment,
                user_id: user,
                company_id: company.clone(),
                role: "actual-published-role".into(),
                modules: vec!["legal".into()],
                scope_kind: WorkScopeKind::Holding,
                scope_node_id: None,
                version: 3,
                valid_from: NOW - 1000,
                valid_until: NOW + 1000,
                revoked: false,
            },
            role: PublishedRoleFact {
                id: "actual-published-role".into(),
                company_id: company.as_str().into(),
                revision: 2,
                active: true,
                scopes: vec![WorkScopeKind::Holding],
            },
        },
    )
}
fn fence(
    input: &HoldingWorkspaceRequest,
    facts: &DirectoryRefreshSelectionFacts,
) -> Result<ExpectedWorkSelection, erpx_server_shared::public::BusinessError> {
    selection_fence(
        input,
        &RecordId::new("actual-child").unwrap(),
        &UserId::new("actual-user").unwrap(),
        NOW + 2000,
        facts,
        NOW,
    )
}
#[test]
fn exact_five_minute_cutoff_is_refreshable_while_current_observations_do_not_fetch() {
    let mut facts = member();
    assert!(matches!(
        classify_member(&facts, NOW).unwrap(),
        DirectoryRefreshDisposition::Current
    ));
    facts.observed_at = NOW - 299999;
    assert!(matches!(
        classify_member(&facts, NOW).unwrap(),
        DirectoryRefreshDisposition::Current
    ));
    facts.observed_at -= 1;
    assert!(matches!(
        classify_member(&facts, NOW).unwrap(),
        DirectoryRefreshDisposition::Observe
    ));
}
#[test]
fn a_stale_disabled_observation_can_discover_reenable_but_a_current_disabled_identity_cannot() {
    let mut facts = member();
    facts.identity_enabled = false;
    assert!(classify_member(&facts, NOW).is_err());
    facts.observed_at = NOW - 300000;
    assert!(matches!(
        classify_member(&facts, NOW).unwrap(),
        DirectoryRefreshDisposition::Observe
    ));
}
#[test]
fn explicit_member_connection_and_link_denials_and_future_source_time_do_not_schedule_refresh() {
    for branch in 0..5 {
        let mut facts = member();
        facts.observed_at = NOW - 300000;
        match branch {
            0 => facts.active = false,
            1 => facts.connection_active = false,
            2 => facts.link_matches = false,
            3 => facts.version = 0,
            _ => facts.observed_at = NOW + 1,
        }
        assert!(classify_member(&facts, NOW).is_err());
    }
}
#[test]
fn original_session_scope_and_positive_selection_cas_are_mandatory_before_provider_work() {
    let (input, mut facts) = fixture();
    assert!(fence(&input, &facts).is_ok());
    facts.revoked = true;
    assert_eq!(fence(&input, &facts).unwrap_err().code, "SESSION_REVOKED");
    facts.revoked = false;
    facts.selection.revision += 1;
    assert_eq!(
        fence(&input, &facts).unwrap_err().code,
        "WORK_CONTEXT_CHANGED"
    );
    facts.selection.revision -= 1;
    facts.selection.user_id = UserId::new("different-user").unwrap();
    assert!(fence(&input, &facts).is_err());
}
#[test]
fn missing_mutation_cas_cannot_refresh_and_read_cas_can_only_capture_the_actual_selected_grant() {
    let (mut input, facts) = fixture();
    input.expected = None;
    assert_eq!(
        fence(&input, &facts).unwrap_err().code,
        "WORK_CONTEXT_CHANGED"
    );
    input.mutation = false;
    let captured = fence(&input, &facts).unwrap();
    assert_eq!(captured.assignment_id, facts.assignment.id);
    assert_eq!(captured.assignment_version, 3);
    assert_eq!(captured.revision, 7);
}
#[test]
fn revoked_expired_or_republished_unavailable_grants_are_not_refresh_credentials() {
    let (input, mut facts) = fixture();
    facts.assignment.revoked = true;
    assert!(fence(&input, &facts).is_err());
    facts.assignment.revoked = false;
    facts.assignment.valid_until = NOW;
    assert!(fence(&input, &facts).is_err());
    facts.assignment.valid_until = NOW + 1;
    facts.role.active = false;
    assert_eq!(fence(&input, &facts).unwrap_err().code, "ROLE_UNAVAILABLE");
}
#[test]
fn ambiguous_holding_node_is_never_normalized_into_a_broader_refresh_scope() {
    let (input, mut facts) = fixture();
    facts.assignment.scope_node_id =
        Some(RecordId::new("branch-that-must-not-be-dropped").unwrap());
    assert_eq!(
        fence(&input, &facts).unwrap_err().code,
        "WORK_ROLE_UNAVAILABLE"
    );
}
