use erpx_server_organization::public::*;
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId};
fn node(id: &str, kind: NodeKind, parent: Option<&str>, branch: Option<&str>) -> OrganizationNode {
    OrganizationNode {
        id: RecordId::new(id).unwrap(),
        kind,
        slug: id.into(),
        display_name: id.into(),
        parent_id: parent.map(|id| RecordId::new(id).unwrap()),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|id| RecordId::new(id).unwrap()),
        status: NodeStatus::Active,
        hierarchy_revision: 1,
    }
}
fn lineage() -> HierarchyLineage {
    HierarchyLineage {
        company: node("company", NodeKind::Company, None, None),
        branch: Some(node(
            "branch",
            NodeKind::Branch,
            Some("company"),
            Some("branch"),
        )),
        department: Some(node(
            "department",
            NodeKind::Department,
            Some("branch"),
            Some("branch"),
        )),
    }
}
fn holding() -> HoldingId {
    HoldingId::new("h001").unwrap()
}
#[test]
fn active_company_branch_and_department_keep_their_owned_parent_lineage() {
    let facts = company_context(holding(), lineage()).unwrap();
    assert_eq!(facts.company().id.as_str(), "company");
    assert_eq!(
        facts
            .department()
            .unwrap()
            .parent_id
            .as_ref()
            .unwrap()
            .as_str(),
        "branch"
    );
    let mut facts = lineage();
    facts.branch.as_mut().unwrap().branch_id = None;
    assert!(
        company_context(holding(), facts).is_ok(),
        "new branch commands may store no redundant branch id"
    );
}
#[test]
fn suspended_archived_or_cross_company_lineage_fails_closed() {
    for field in 0..6 {
        let mut facts = lineage();
        match field {
            0 => facts.company.status = NodeStatus::Suspended,
            1 => facts.branch.as_mut().unwrap().status = NodeStatus::Archived,
            2 => facts.department.as_mut().unwrap().status = NodeStatus::Suspended,
            3 => {
                facts.department.as_mut().unwrap().company_id =
                    CompanyId::new("another-company").unwrap()
            }
            4 => {
                facts.branch.as_mut().unwrap().parent_id =
                    Some(RecordId::new("another-company").unwrap())
            }
            _ => {
                facts.department.as_mut().unwrap().parent_id =
                    Some(RecordId::new("company").unwrap())
            }
        };
        assert_eq!(
            company_context(holding(), facts).unwrap_err().code,
            "WORKSPACE_NOT_FOUND"
        );
    }
}
#[test]
fn company_level_department_has_no_branch_and_reports_to_the_company() {
    let mut facts = lineage();
    facts.branch = None;
    let department = facts.department.as_mut().unwrap();
    department.parent_id = Some(RecordId::new("company").unwrap());
    department.branch_id = None;
    assert!(company_context(holding(), facts).is_ok());
}
struct Facts {
    candidates: Vec<HierarchyLineage>,
    calls: usize,
    holding: HoldingId,
}
impl OrganizationReadPort for Facts {
    type Error = erpx_server_shared::public::BusinessError;
    fn holding_id(&self) -> &HoldingId {
        &self.holding
    }
    fn workspace_candidates<'a>(
        &'a mut self,
        _: &'a WorkspaceRouteQuery,
    ) -> OrganizationReadFuture<'a, Vec<HierarchyLineage>> {
        self.calls += 1;
        Box::pin(async move { Ok(self.candidates.clone()) })
    }
    fn company<'a>(
        &'a mut self,
        _: &'a CompanyId,
    ) -> OrganizationReadFuture<'a, Option<HierarchyLineage>> {
        Box::pin(async move { Ok(self.candidates.first().cloned()) })
    }
}
#[tokio::test]
async fn ambiguous_company_candidates_and_wrong_query_facts_cannot_select_an_arbitrary_company() {
    let query =
        WorkspaceRouteQuery::new(Some("branch".into()), Some("department".into()), None).unwrap();
    let mut facts = Facts {
        candidates: vec![lineage(), lineage()],
        calls: 0,
        holding: holding(),
    };
    assert_eq!(
        resolve_workspace(&mut facts, &query)
            .await
            .unwrap_err()
            .code,
        "WORKSPACE_NOT_FOUND"
    );
    facts.candidates.pop();
    facts.candidates[0].branch.as_mut().unwrap().slug = "wrong-branch".into();
    assert!(resolve_workspace(&mut facts, &query).await.is_err());
}
#[tokio::test]
async fn empty_route_is_not_an_implicit_default_company_or_a_permission_grant() {
    let mut facts = Facts {
        candidates: vec![lineage()],
        calls: 0,
        holding: holding(),
    };
    assert!(
        resolve_workspace(
            &mut facts,
            &WorkspaceRouteQuery::new(None, None, None).unwrap()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(facts.calls, 0);
    assert!(
        read_company_context(&mut facts, &CompanyId::new("foreign").unwrap())
            .await
            .is_err()
    );
}
#[test]
fn route_slugs_reject_paths_and_ambiguous_encoding() {
    for slug in [
        "../branch",
        "branch/department",
        "North",
        "-north",
        "north-",
        "north--main",
        "north%2fmain",
    ] {
        assert!(WorkspaceRouteQuery::new(Some(slug.into()), None, None).is_err());
    }
}
