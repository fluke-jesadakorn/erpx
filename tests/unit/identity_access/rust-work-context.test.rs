use erpx_server_identity_access::public::work_context::*;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1000)
    }
}
fn request() -> WorkContextRequest {
    WorkContextRequest {
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: Some("north".into()),
        department_slug: Some("engineering".into()),
        session_id: RecordId::new("session").unwrap(),
        user_id: UserId::new("user").unwrap(),
        session_expires_at: 2000,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new("grant").unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    }
}
fn snapshot() -> WorkContextSnapshot {
    let request = request();
    WorkContextSnapshot {
        workspace: Some(WorkspaceLineageFact {
            company_id: request.company_id.clone(),
            branch_id: Some(RecordId::new("branch").unwrap()),
            department_id: Some(RecordId::new("department").unwrap()),
            branch_slug: request.branch_slug,
            department_slug: request.department_slug,
        }),
        selection: Some(ActiveSelectionFact {
            session_id: request.session_id,
            user_id: request.user_id.clone(),
            company_id: request.company_id.clone(),
            assignment_id: Some(AssignmentId::new("grant").unwrap()),
            assignment_version: Some(3),
            revision: 7,
            expires_at: 2000,
        }),
        assignment: Some(WorkAssignmentFact {
            id: AssignmentId::new("grant").unwrap(),
            user_id: request.user_id,
            company_id: request.company_id,
            role: "finance_preparer".into(),
            scope_kind: WorkScopeKind::Branch,
            scope_node_id: Some(RecordId::new("branch").unwrap()),
            version: 3,
            valid_from: 0,
            valid_until: 2000,
            revoked: false,
        }),
        role: Some(PublishedRoleFact {
            id: "finance_preparer".into(),
            company_id: "*".into(),
            revision: 2,
            active: true,
            scopes: vec![
                WorkScopeKind::Holding,
                WorkScopeKind::Branch,
                WorkScopeKind::Department,
            ],
        }),
        session_revoked: false,
        directory_stale: false,
        principal_active: true,
        assignment_active: true,
    }
}
struct Port {
    holding: HoldingId,
    facts: WorkContextSnapshot,
    calls: usize,
}
impl Port {
    fn new() -> Self {
        Self {
            holding: request().holding_id,
            facts: snapshot(),
            calls: 0,
        }
    }
}
impl CurrentWorkContextPort for Port {
    type Error = BusinessError;
    fn holding_id(&self) -> &HoldingId {
        &self.holding
    }
    fn snapshot<'a>(
        &'a mut self,
        _: &'a WorkContextRequest,
        _: i64,
    ) -> WorkContextFuture<'a, WorkContextSnapshot> {
        self.calls += 1;
        Box::pin(async move { Ok(self.facts.clone()) })
    }
}
#[tokio::test]
async fn current_selected_branch_grant_covers_its_department_without_a_fallback_role() {
    let mut port = Port::new();
    let result = inspect_current_work_selection(&mut port, &FixedClock, &request())
        .await
        .unwrap();
    assert_eq!(result.assignment().id.as_str(), "grant");
    assert_eq!(result.revision(), 7);
    assert_eq!(result.role_revision(), 2);
    assert_eq!(port.calls, 1);
}
#[tokio::test]
async fn revoked_session_and_stale_directory_deny_before_selected_grant_is_usable() {
    for stale in [false, true] {
        let mut port = Port::new();
        port.facts.session_revoked = !stale;
        port.facts.directory_stale = stale;
        assert_eq!(
            inspect_current_work_selection(&mut port, &FixedClock, &request())
                .await
                .unwrap_err()
                .code,
            if stale {
                "DIRECTORY_ACCESS_UNAVAILABLE"
            } else {
                "SESSION_REVOKED"
            }
        );
    }
}
#[tokio::test]
async fn stale_selection_assignment_and_lineage_never_choose_another_available_role() {
    for field in 0..8 {
        let mut port = Port::new();
        match field {
            0 => port.facts.selection = None,
            1 => port.facts.assignment = None,
            2 => port.facts.assignment.as_mut().unwrap().revoked = true,
            3 => port.facts.assignment.as_mut().unwrap().version = 4,
            4 => {
                port.facts.assignment.as_mut().unwrap().company_id =
                    CompanyId::new("foreign").unwrap()
            }
            5 => {
                port.facts.assignment.as_mut().unwrap().scope_node_id =
                    Some(RecordId::new("other-branch").unwrap())
            }
            6 => port.facts.principal_active = false,
            _ => port.facts.assignment_active = false,
        };
        assert!(
            inspect_current_work_selection(&mut port, &FixedClock, &request())
                .await
                .is_err(),
            "acceptedfield {field}"
        );
    }
}
#[tokio::test]
async fn mutating_request_requires_the_exact_rendered_context_revision_and_assignment() {
    for field in 0..4 {
        let mut request = request();
        match field {
            0 => request.expected = None,
            1 => request.expected.as_mut().unwrap().revision = 6,
            2 => request.expected.as_mut().unwrap().assignment_version = 2,
            _ => {
                request.expected.as_mut().unwrap().assignment_id =
                    AssignmentId::new("other").unwrap()
            }
        };
        assert_eq!(
            inspect_current_work_selection(&mut Port::new(), &FixedClock, &request)
                .await
                .unwrap_err()
                .code,
            "WORK_CONTEXT_CHANGED"
        );
    }
}
#[tokio::test]
async fn archived_foreign_or_scope_mismatched_published_role_is_unavailable() {
    for field in 0..5 {
        let mut port = Port::new();
        match field {
            0 => port.facts.role = None,
            1 => port.facts.role.as_mut().unwrap().active = false,
            2 => port.facts.role.as_mut().unwrap().scopes = vec![WorkScopeKind::Department],
            3 => port.facts.role.as_mut().unwrap().company_id = "foreign".into(),
            _ => port.facts.role.as_mut().unwrap().revision = 0,
        };
        assert_eq!(
            inspect_current_work_selection(&mut port, &FixedClock, &request())
                .await
                .unwrap_err()
                .code,
            "ROLE_UNAVAILABLE"
        );
    }
}
#[tokio::test]
async fn expired_control_proof_and_foreign_holding_do_not_query_business_sources() {
    for expiry in [true, false] {
        let mut request = request();
        let mut port = Port::new();
        if expiry {
            request.session_expires_at = 1000;
        } else {
            request.holding_id = HoldingId::new("h002").unwrap();
        }
        assert!(
            inspect_current_work_selection(&mut port, &FixedClock, &request)
                .await
                .is_err()
        );
        assert_eq!(port.calls, 0);
    }
}
