//! Portable Identity Access role, independent-review and access-review tests.
use erpx_server_identity_access::public::iam_roles::{
    AccessReviewCampaignInput, AccessReviewCancelInput, AccessReviewCancelPlan,
    AccessReviewDecisionInput, AccessReviewDecisionPlan, AccessReviewStartPlan, CampaignRow,
    IamReceiptSnapshot, IamRoleActor, IamRolesFuture, IamRolesReviewRepository, ProposerAuthority,
    ReviewAssignment, ReviewDecisionInput, ReviewDecisionPlan, ReviewItemRow, ReviewRow,
    RevocationGuardFacts, RoleRow, RoleSaveInput, RoleSavePlan, cancel_access_review_campaign,
    decide_access_review_item, decide_review, save_role, start_access_review,
};
use erpx_server_identity_access::public::permission_catalog::permission_catalog;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use serde_json::{Value, json};

const NOW: i64 = 500;

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}

fn actor(permissions: &[&str], grantable: &[&str]) -> IamRoleActor {
    IamRoleActor {
        user_id: UserId::new("reviewer").unwrap(),
        session_id: id("session-reviewer"),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-reviewer").unwrap(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        role: Some("it_admin".into()),
        role_revision: Some(9),
        assignment_version: Some(2),
        identity_version: Some(1),
        authority_revision: Some(4),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        grantable_permissions: grantable.iter().map(|value| (*value).to_owned()).collect(),
        person_user_ids: vec![UserId::new("reviewer").unwrap()],
        person_id: Some(id("person-reviewer")),
        platform: false,
        emergency_until: None,
        expires_at: 1_000,
    }
}

fn manager() -> IamRoleActor {
    actor(
        &["iam.roles.manage", "iam.review"],
        &["iam.read", "iam.manage", "iam.roles.manage"],
    )
}

fn campaign_actor() -> IamRoleActor {
    actor(&["iam.review"], &[])
}

fn key() -> CommandKey {
    CommandKey::new("iam-role-key-1").unwrap()
}

fn role_input() -> RoleSaveInput {
    RoleSaveInput {
        id: None,
        name: "  Finance reader  ".into(),
        expected_version: 0,
        permissions: vec!["iam.manage".into(), "iam.read".into(), "iam.read".into()],
        grantable_permissions: vec!["iam.read".into()],
        scopes: vec!["holding".into(), "branch".into()],
        reason: "  Independent role revision  ".into(),
    }
}

#[derive(Default)]
struct Fake {
    authorized: bool,
    role: Option<RoleRow>,
    revision: i64,
    affected: Vec<String>,
    receipt: Option<IamReceiptSnapshot>,
    role_commits: Vec<RoleSavePlan>,
    review: Option<ReviewRow>,
    review_commits: Vec<ReviewDecisionPlan>,
    same_person: bool,
    branch_covers: bool,
    proposer: Option<ProposerAuthority>,
    allows: bool,
    affected_reviewer: bool,
    start_commits: Vec<AccessReviewStartPlan>,
    assignments: Vec<ReviewAssignment>,
    item: Option<ReviewItemRow>,
    current_assignment: Option<ReviewAssignment>,
    active: bool,
    guard: RevocationGuardFacts,
    decision_commits: Vec<AccessReviewDecisionPlan>,
    campaign: Option<CampaignRow>,
    decided: i64,
    cancel_commits: Vec<AccessReviewCancelPlan>,
    digest: Option<String>,
}

fn guard() -> RevocationGuardFacts {
    RevocationGuardFacts {
        same_person: false,
        recovery_member: false,
        other_recovery_administrator: true,
        other_administrator: true,
    }
}

fn assignment() -> ReviewAssignment {
    ReviewAssignment {
        id: "assignment-target".into(),
        user_id: "target".into(),
        role: "employee".into(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        source: "manual".into(),
        valid_until: 900,
        version: 3,
        revoked_at: None,
    }
}

fn item() -> ReviewItemRow {
    ReviewItemRow {
        id: "ar-item-iam-role-key-1-0".into(),
        campaign_id: "ar-campaign-iam-role-key-1".into(),
        company_id: "company".into(),
        assignment_id: "assignment-target".into(),
        assignment_version: 3,
        user_id: "target".into(),
        role: "employee".into(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        source: "manual".into(),
        valid_until: 900,
        decision: "pending".into(),
        campaign_status: "open".into(),
        campaign_version: 2,
        created_by: "creator".into(),
    }
}

fn review_payload() -> Value {
    json!({
        "id": "custom_role",
        "name": "Custom role",
        "revision": 2,
        "expectedVersion": 1,
        "permissions": ["iam.read"],
        "grantablePermissions": ["iam.read"],
        "affected": ["target"],
        "scopeKind": "holding",
        "scopeNodeId": null,
        "authorizingAssignmentId": "assignment-proposer",
    })
}

fn review() -> ReviewRow {
    ReviewRow {
        id: "iam-review-iam-role-key-1".into(),
        kind: "role.publish".into(),
        payload: review_payload(),
        created_by: "proposer".into(),
        created_person: "person-proposer".into(),
        status: "pending".into(),
        version: 1,
        reason: "Role review".into(),
    }
}

impl IamRolesReviewRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        _: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.authorized) })
    }
    fn digest<'a>(&'a mut self, _: &'a str) -> IamRolesFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok(self.digest.clone().unwrap_or_else(|| "a".repeat(64))) })
    }
    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> IamRolesFuture<'a, Option<IamReceiptSnapshot>, Self::Error> {
        Box::pin(async move { Ok(self.receipt.clone()) })
    }
    fn role_for_update<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, Option<RoleRow>, Self::Error> {
        Box::pin(async move { Ok(self.role.clone()) })
    }
    fn next_revision<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, i64, Self::Error> {
        Box::pin(async move { Ok(self.revision) })
    }
    fn affected_role_users<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, Vec<String>, Self::Error> {
        Box::pin(async move { Ok(self.affected.clone()) })
    }
    fn commit_role_save<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        plan: &'a RoleSavePlan,
        _: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.role_commits.push(plan.clone());
            Ok(serde_json::from_str(&plan.receipt_json()).unwrap())
        })
    }
    fn review_for_update<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewRow>, Self::Error> {
        Box::pin(async move { Ok(self.review.clone()) })
    }
    fn same_person<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.same_person) })
    }
    fn branch_covers<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.branch_covers) })
    }
    fn proposer_authority<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> IamRolesFuture<'a, Option<ProposerAuthority>, Self::Error> {
        Box::pin(async move { Ok(self.proposer.clone()) })
    }
    fn role_allows<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        _: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.allows) })
    }
    fn reviewer_affected_by_role<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        _: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.affected_reviewer) })
    }
    fn commit_review_decision<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        plan: &'a ReviewDecisionPlan,
        _: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.review_commits.push(plan.clone());
            Ok(serde_json::from_str(&plan.receipt_json()).unwrap())
        })
    }
    fn active_review_assignments<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: i64,
    ) -> IamRolesFuture<'a, Vec<ReviewAssignment>, Self::Error> {
        Box::pin(async move { Ok(self.assignments.clone()) })
    }
    fn commit_campaign_start<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        plan: &'a AccessReviewStartPlan,
        _: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.start_commits.push(plan.clone());
            Ok(serde_json::from_str(&plan.receipt_json()).unwrap())
        })
    }
    fn review_item_for_update<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewItemRow>, Self::Error> {
        Box::pin(async move { Ok(self.item.clone()) })
    }
    fn assignment_for_update<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewAssignment>, Self::Error> {
        Box::pin(async move { Ok(self.current_assignment.clone()) })
    }
    fn assignment_active<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        _: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.active) })
    }
    fn revocation_guard<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        _: i64,
    ) -> IamRolesFuture<'a, RevocationGuardFacts, Self::Error> {
        Box::pin(async move { Ok(self.guard.clone()) })
    }
    fn commit_campaign_decision<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        plan: &'a AccessReviewDecisionPlan,
        _: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.decision_commits.push(plan.clone());
            Ok(serde_json::from_str(&plan.receipt_json("completed")).unwrap())
        })
    }
    fn campaign_for_update<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, Option<CampaignRow>, Self::Error> {
        Box::pin(async move { Ok(self.campaign.clone()) })
    }
    fn decided_items<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
    ) -> IamRolesFuture<'a, i64, Self::Error> {
        Box::pin(async move { Ok(self.decided) })
    }
    fn commit_campaign_cancel<'a>(
        &'a mut self,
        _: &'a IamRoleActor,
        _: &'a str,
        plan: &'a AccessReviewCancelPlan,
        _: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.cancel_commits.push(plan.clone());
            Ok(serde_json::from_str(&plan.receipt_json()).unwrap())
        })
    }
}

fn ready() -> Fake {
    Fake {
        authorized: true,
        revision: 2,
        affected: vec!["target".into()],
        proposer: Some(ProposerAuthority {
            role: "it_admin".into(),
            grantable_permissions: vec!["iam.read".into(), "iam.manage".into()],
        }),
        allows: true,
        guard: guard(),
        ..Fake::default()
    }
}

#[tokio::test]
async fn role_save_gate_rejects_unlisted_preview_and_undelegated_permissions() {
    let service = permission_catalog().unwrap();
    let actor = manager();
    let mut fake = ready();
    for (name, permissions) in [
        ("unknown", vec!["invented.permission".to_string()]),
        ("preview", vec!["preview.brand".to_string()]),
        ("undelegated", vec!["finance.read".to_string()]),
    ] {
        let input = RoleSaveInput {
            permissions,
            ..role_input()
        };
        let error = save_role(&mut fake, &actor, service, &key(), &input, NOW)
            .await
            .unwrap_err();
        assert_eq!(error.code, "DELEGATION_EXCEEDED", "{name}");
    }
    let platform_scope = RoleSaveInput {
        scopes: vec!["platform".into()],
        ..role_input()
    };
    assert_eq!(
        save_role(&mut fake, &actor, service, &key(), &platform_scope, NOW)
            .await
            .unwrap_err()
            .code,
        "ROLE_SCOPE_INVALID"
    );
    let built_in = RoleSaveInput {
        id: Some("it_admin".into()),
        ..role_input()
    };
    assert_eq!(
        save_role(&mut fake, &actor, service, &key(), &built_in, NOW)
            .await
            .unwrap_err()
            .code,
        "PROTECTED_ROLE"
    );
    assert!(fake.role_commits.is_empty());
}

#[tokio::test]
async fn role_save_appends_revision_review_and_replays_the_exact_receipt() {
    let service = permission_catalog().unwrap();
    let actor = manager();
    let mut fake = ready();
    let value = save_role(&mut fake, &actor, service, &key(), &role_input(), NOW)
        .await
        .unwrap();
    assert_eq!(value["status"], "pending");
    assert_eq!(value["roleId"], "custom_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert_eq!(value["revision"], 2);
    assert_eq!(value["affected"], json!(["target"]));
    assert_eq!(fake.role_commits.len(), 1);
    let plan = &fake.role_commits[0];
    assert!(plan.created);
    assert_eq!(plan.name, "Finance reader");
    assert_eq!(plan.permissions, vec!["iam.manage", "iam.read"]);
    assert_eq!(plan.grantable_permissions, vec!["iam.read"]);
    assert_eq!(plan.scopes, vec!["branch", "holding"]);
    assert_eq!(plan.reason, "Independent role revision");
    assert_eq!(plan.review_id, "iam-review-iam-role-key-1");
    assert_eq!(
        plan.review_payload["authorizingAssignmentId"],
        "assignment-reviewer"
    );
    assert_eq!(plan.review_payload["revision"], 2);

    fake.receipt = Some(IamReceiptSnapshot {
        request_hash: "a".repeat(64),
        result_json: plan.receipt_json(),
        operation: "iam.role.save".into(),
        actor_id: actor.user_id.as_str().into(),
        record_id: plan.review_id.clone(),
        authority_json: actor.authority_json(),
    });
    let replay = save_role(&mut fake, &actor, service, &key(), &role_input(), NOW)
        .await
        .unwrap();
    assert_eq!(replay, value);
    assert_eq!(fake.role_commits.len(), 1);

    fake.digest = Some("b".repeat(64));
    let different = RoleSaveInput {
        name: "Different role".into(),
        ..role_input()
    };
    assert_eq!(
        save_role(&mut fake, &actor, service, &key(), &different, NOW)
            .await
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
}

#[tokio::test]
async fn role_save_enforces_current_authority_builtin_and_cas() {
    let service = permission_catalog().unwrap();
    let service_actor = manager();
    let mut fake = ready();
    fake.role = Some(RoleRow {
        company_id: "*".into(),
        id: "it_admin".into(),
        name: "IT".into(),
        built_in: true,
        published_revision: 1,
        status: "active".into(),
    });
    let input = RoleSaveInput {
        id: Some("custom_role".into()),
        expected_version: 1,
        ..role_input()
    };
    let built_in_input = RoleSaveInput {
        id: Some("it_admin".into()),
        expected_version: 1,
        ..role_input()
    };
    assert_eq!(
        save_role(
            &mut fake,
            &service_actor,
            service,
            &key(),
            &built_in_input,
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "PROTECTED_ROLE"
    );
    let mut custom = ready();
    custom.role = Some(RoleRow {
        company_id: "company".into(),
        id: "custom_role".into(),
        name: "Custom".into(),
        built_in: false,
        published_revision: 3,
        status: "active".into(),
    });
    let stale = RoleSaveInput {
        id: Some("custom_role".into()),
        expected_version: 2,
        ..role_input()
    };
    assert_eq!(
        save_role(&mut custom, &service_actor, service, &key(), &stale, NOW)
            .await
            .unwrap_err()
            .code,
        "ROLE_CHANGED"
    );
    let mut missing = ready();
    missing.role = None;
    assert_eq!(
        save_role(&mut missing, &service_actor, service, &key(), &input, NOW)
            .await
            .unwrap_err()
            .code,
        "ROLE_CHANGED"
    );
    let mut changed = ready();
    changed.authorized = false;
    assert_eq!(
        save_role(
            &mut changed,
            &service_actor,
            service,
            &key(),
            &role_input(),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "IAM_ACCESS_CHANGED"
    );
    assert_eq!(
        save_role(
            &mut ready(),
            &actor(&["iam.read"], &["iam.read"]),
            service,
            &key(),
            &role_input(),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "PERMISSION_REQUIRED"
    );
}

fn decision(decision: &str) -> ReviewDecisionInput {
    ReviewDecisionInput {
        expected_version: 1,
        decision: decision.into(),
        reason: "  Independent decision  ".into(),
    }
}

#[tokio::test]
async fn review_decision_enforces_independence_scope_delegation_and_publication() {
    let service = permission_catalog().unwrap();
    let service_actor = manager();
    let mut fake = ready();
    fake.review = Some(review());
    fake.same_person = true;
    assert_eq!(
        decide_review(
            &mut fake,
            &service_actor,
            service,
            &key(),
            "iam-review-iam-role-key-1",
            &decision("approve"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "INDEPENDENT_REVIEW_REQUIRED"
    );
    fake.same_person = false;
    fake.affected_reviewer = true;
    assert_eq!(
        decide_review(
            &mut fake,
            &service_actor,
            service,
            &key(),
            "iam-review-iam-role-key-1",
            &decision("approve"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "INDEPENDENT_REVIEW_REQUIRED"
    );
    fake.affected_reviewer = false;
    fake.proposer = None;
    assert_eq!(
        decide_review(
            &mut fake,
            &service_actor,
            service,
            &key(),
            "iam-review-iam-role-key-1",
            &decision("approve"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "PROPOSER_AUTHORITY_REVOKED"
    );
    fake.proposer = Some(ProposerAuthority {
        role: "it_admin".into(),
        grantable_permissions: vec!["iam.manage".into()],
    });
    assert_eq!(
        decide_review(
            &mut fake,
            &service_actor,
            service,
            &key(),
            "iam-review-iam-role-key-1",
            &decision("approve"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "DELEGATION_EXCEEDED"
    );
    fake.proposer = Some(ProposerAuthority {
        role: "it_admin".into(),
        grantable_permissions: vec!["iam.read".into()],
    });
    let branch = IamRoleActor {
        scope_kind: "branch".into(),
        scope_node_id: Some(id("branch-1")),
        ..manager()
    };
    assert_eq!(
        decide_review(
            &mut fake,
            &branch,
            service,
            &key(),
            "iam-review-iam-role-key-1",
            &decision("approve"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "REVIEW_OUTSIDE_SCOPE"
    );
    let value = decide_review(
        &mut fake,
        &service_actor,
        service,
        &key(),
        "iam-review-iam-role-key-1",
        &decision("approve"),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        value,
        json!({"id": "iam-review-iam-role-key-1", "status": "approved"})
    );
    assert_eq!(fake.review_commits.len(), 1);
    let plan = &fake.review_commits[0];
    assert_eq!(plan.reason, "Independent decision");
    assert_eq!(plan.publication.as_ref().unwrap().revision, 2);
    assert_eq!(plan.publication.as_ref().unwrap().id, "custom_role");
}

#[tokio::test]
async fn review_decision_rejects_unsupported_approval_and_allows_rejection() {
    let service = permission_catalog().unwrap();
    let service_actor = manager();
    let mut fake = ready();
    fake.review = Some(ReviewRow {
        kind: "assignment.grant".into(),
        payload: json!({
            "scopeKind": "holding",
            "scopeNodeId": null,
            "authorizingAssignmentId": "assignment-proposer"
        }),
        ..review()
    });
    assert_eq!(
        decide_review(
            &mut fake,
            &service_actor,
            service,
            &key(),
            "iam-review-iam-role-key-1",
            &decision("approve"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "REVIEW_UNSUPPORTED"
    );
    let value = decide_review(
        &mut fake,
        &service_actor,
        service,
        &key(),
        "iam-review-iam-role-key-1",
        &decision("reject"),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(value["status"], "rejected");
    assert!(fake.review_commits[0].publication.is_none());
    fake.review = Some(ReviewRow {
        version: 2,
        ..review()
    });
    assert_eq!(
        decide_review(
            &mut fake,
            &service_actor,
            service,
            &key(),
            "iam-review-iam-role-key-1",
            &decision("reject"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "REVIEW_CHANGED"
    );
}

fn campaign() -> AccessReviewCampaignInput {
    AccessReviewCampaignInput {
        name: "  Quarterly certification  ".into(),
        due_at: NOW + 86_400_000,
        reason: "  Quarterly review  ".into(),
    }
}

#[tokio::test]
async fn campaign_start_requires_holding_scope_due_date_and_active_assignments() {
    let actor = campaign_actor();
    let mut fake = ready();
    let platform = IamRoleActor {
        platform: true,
        ..campaign_actor()
    };
    assert_eq!(
        start_access_review(&mut fake, &platform, &key(), &campaign(), NOW)
            .await
            .unwrap_err()
            .code,
        "HOLDING_REVIEW_REQUIRED"
    );
    let branch = IamRoleActor {
        scope_kind: "branch".into(),
        scope_node_id: Some(id("branch-1")),
        ..campaign_actor()
    };
    assert_eq!(
        start_access_review(&mut fake, &branch, &key(), &campaign(), NOW)
            .await
            .unwrap_err()
            .code,
        "SCOPE_FORBIDDEN"
    );
    let mut invalid = campaign();
    invalid.due_at = NOW;
    assert_eq!(
        start_access_review(&mut fake, &actor, &key(), &invalid, NOW)
            .await
            .unwrap_err()
            .code,
        "REVIEW_DUE_DATE_INVALID"
    );
    assert_eq!(
        start_access_review(&mut fake, &actor, &key(), &campaign(), NOW)
            .await
            .unwrap_err()
            .code,
        "NO_ACTIVE_ASSIGNMENTS"
    );
    fake.assignments = vec![assignment()];
    let value = start_access_review(&mut fake, &actor, &key(), &campaign(), NOW)
        .await
        .unwrap();
    assert_eq!(
        value,
        json!({"id": "ar-campaign-iam-role-key-1", "status": "open", "version": 1, "itemCount": 1})
    );
    let plan = &fake.start_commits[0];
    assert_eq!(plan.name, "Quarterly certification");
    assert_eq!(plan.reason, "Quarterly review");
    assert_eq!(plan.started_event_id, "ar-event-iam-role-key-1");
}

fn campaign_decision(decision: &str) -> AccessReviewDecisionInput {
    AccessReviewDecisionInput {
        expected_campaign_version: 2,
        decision: decision.into(),
        reason: "  Certify access  ".into(),
    }
}

#[tokio::test]
async fn campaign_decision_enforces_independence_source_and_revocation_guards() {
    let actor = campaign_actor();
    let mut fake = ready();
    fake.item = Some(item());
    fake.campaign = Some(CampaignRow {
        id: "ar-campaign-iam-role-key-1".into(),
        status: "open".into(),
        version: 2,
        created_by: "creator".into(),
    });
    fake.current_assignment = Some(assignment());
    fake.active = true;

    let mut closed = item();
    closed.campaign_status = "completed".into();
    fake.item = Some(closed);
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("keep"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "ACCESS_REVIEW_ITEM_CHANGED"
    );
    fake.item = Some(item());
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &AccessReviewDecisionInput {
                expected_campaign_version: 1,
                ..campaign_decision("keep")
            },
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "ACCESS_REVIEW_CAMPAIGN_CHANGED"
    );
    fake.item = Some(ReviewItemRow {
        created_by: actor.user_id.as_str().into(),
        ..item()
    });
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("keep"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "INDEPENDENT_REVIEW_REQUIRED"
    );
    fake.item = Some(ReviewItemRow {
        user_id: actor.user_id.as_str().into(),
        ..item()
    });
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("keep"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "SELF_ACCESS_REVIEW"
    );
    fake.item = Some(item());
    fake.same_person = true;
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("keep"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "SELF_ACCESS_REVIEW"
    );
    fake.same_person = false;
    fake.current_assignment = None;
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("keep"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "REVIEW_SOURCE_CHANGED"
    );
    fake.current_assignment = Some(assignment());
    fake.active = false;
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("revoke"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "REVIEW_SOURCE_CHANGED"
    );
    fake.active = true;
    let value = decide_access_review_item(
        &mut fake,
        &actor,
        &key(),
        "ar-item-iam-role-key-1-0",
        &campaign_decision("keep"),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(value["decision"], "keep");
    assert_eq!(value["campaignVersion"], 3);
    assert!(fake.decision_commits[0].revocation.is_none());

    fake.guard = RevocationGuardFacts {
        recovery_member: true,
        ..guard()
    };
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("revoke"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "PERMISSION_REQUIRED"
    );
    fake.guard = RevocationGuardFacts {
        recovery_member: true,
        other_recovery_administrator: false,
        ..guard()
    };
    let recovery_actor = actor_with_recovery();
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &recovery_actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("revoke"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "LAST_RECOVERY_ADMINISTRATOR"
    );
    fake.guard = RevocationGuardFacts {
        other_administrator: false,
        ..guard()
    };
    let mut owner_assignment = assignment();
    owner_assignment.role = "holding_owner".into();
    fake.current_assignment = Some(owner_assignment);
    fake.item = Some(ReviewItemRow {
        role: "holding_owner".into(),
        ..item()
    });
    assert_eq!(
        decide_access_review_item(
            &mut fake,
            &recovery_actor,
            &key(),
            "ar-item-iam-role-key-1-0",
            &campaign_decision("revoke"),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "LAST_ADMINISTRATOR"
    );
    fake.guard = guard();
    fake.current_assignment = Some(assignment());
    fake.item = Some(item());
    let value = decide_access_review_item(
        &mut fake,
        &recovery_actor,
        &key(),
        "ar-item-iam-role-key-1-0",
        &campaign_decision("revoke"),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(value["decision"], "revoke");
    let plan = fake.decision_commits.last().unwrap();
    assert_eq!(plan.revocation.as_ref().unwrap().assignment_version, 3);
    assert_eq!(plan.revocation.as_ref().unwrap().reason, "Certify access");
}

fn actor_with_recovery() -> IamRoleActor {
    actor(&["iam.review", "iam.recovery"], &[])
}

#[tokio::test]
async fn campaign_cancel_requires_creator_current_version_and_no_decisions() {
    let actor = campaign_actor();
    let mut fake = ready();
    let input = AccessReviewCancelInput {
        expected_version: 2,
        reason: "  Superseded campaign  ".into(),
    };
    assert_eq!(
        cancel_access_review_campaign(
            &mut fake,
            &actor,
            &key(),
            "ar-campaign-iam-role-key-1",
            &input,
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "ACCESS_REVIEW_CAMPAIGN_NOT_FOUND"
    );
    fake.campaign = Some(CampaignRow {
        id: "ar-campaign-iam-role-key-1".into(),
        status: "open".into(),
        version: 2,
        created_by: "someone-else".into(),
    });
    assert_eq!(
        cancel_access_review_campaign(
            &mut fake,
            &actor,
            &key(),
            "ar-campaign-iam-role-key-1",
            &input,
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "CAMPAIGN_OWNER_REQUIRED"
    );
    fake.campaign = Some(CampaignRow {
        id: "ar-campaign-iam-role-key-1".into(),
        status: "open".into(),
        version: 3,
        created_by: actor.user_id.as_str().into(),
    });
    assert_eq!(
        cancel_access_review_campaign(
            &mut fake,
            &actor,
            &key(),
            "ar-campaign-iam-role-key-1",
            &input,
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "ACCESS_REVIEW_CAMPAIGN_CHANGED"
    );
    fake.campaign = Some(CampaignRow {
        version: 2,
        ..fake.campaign.clone().unwrap()
    });
    fake.decided = 1;
    assert_eq!(
        cancel_access_review_campaign(
            &mut fake,
            &actor,
            &key(),
            "ar-campaign-iam-role-key-1",
            &input,
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "CAMPAIGN_DECISIONS_EXIST"
    );
    fake.decided = 0;
    let value = cancel_access_review_campaign(
        &mut fake,
        &actor,
        &key(),
        "ar-campaign-iam-role-key-1",
        &input,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        value,
        json!({"id": "ar-campaign-iam-role-key-1", "status": "cancelled", "version": 3})
    );
    assert_eq!(fake.cancel_commits[0].reason, "Superseded campaign");
}

#[tokio::test]
async fn receipts_reject_foreign_authority_and_malformed_results() {
    let service = permission_catalog().unwrap();
    let actor = manager();
    let mut fake = ready();
    fake.receipt = Some(IamReceiptSnapshot {
        request_hash: "b".repeat(64),
        result_json: json!({"id": "iam-review-iam-role-key-1", "status": "pending"}).to_string(),
        operation: "iam.role.save".into(),
        actor_id: actor.user_id.as_str().into(),
        record_id: "iam-review-iam-role-key-1".into(),
        authority_json: actor.authority_json(),
    });
    assert_eq!(
        save_role(&mut fake, &actor, service, &key(), &role_input(), NOW)
            .await
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
    fake.receipt = Some(IamReceiptSnapshot {
        request_hash: "a".repeat(64),
        result_json: json!({"id": "iam-review-iam-role-key-1", "status": "approved"}).to_string(),
        ..fake.receipt.clone().unwrap()
    });
    assert_eq!(
        save_role(&mut fake, &actor, service, &key(), &role_input(), NOW)
            .await
            .unwrap_err()
            .code,
        "IAM_ROLES_REVIEWS_READBACK_INVALID"
    );
    let mut invalid = ready();
    invalid.digest = Some("not-a-digest".into());
    assert_eq!(
        save_role(&mut invalid, &actor, service, &key(), &role_input(), NOW)
            .await
            .unwrap_err()
            .code,
        "IAM_ROLES_REVIEWS_READBACK_INVALID"
    );
}
