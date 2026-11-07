//! Portable Identity Access assignment policy, workspace and withdrawal tests.
use erpx_server_identity_access::public::assignment::{
    AssignmentActor, AssignmentFuture, AssignmentGuardFacts, AssignmentReceipt,
    AssignmentRepository, AssignmentRevocation, AssignmentRoleRow, AssignmentRow, ReceiptSnapshot,
    RevocationPlan, assignment_workspace, revoke_assignment,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use serde_json::{Value, json};

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
fn actor_with(permissions: &[&str]) -> AssignmentActor {
    AssignmentActor {
        user_id: UserId::new("actor").unwrap(),
        session_id: id("session-1"),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-actor").unwrap(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        role: Some("custom_iam".into()),
        role_revision: Some(4),
        assignment_version: Some(3),
        identity_version: Some(2),
        authority_revision: Some(9),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        person_user_ids: vec![UserId::new("actor").unwrap()],
        emergency_until: None,
        expires_at: 1_000,
    }
}

fn row(version: i64) -> AssignmentRow {
    AssignmentRow {
        id: "assignment-target".into(),
        user_id: "target".into(),
        role: "employee".into(),
        modules_json: "[\"collaboration\"]".into(),
        company_id: "company".into(),
        branch_id: None,
        valid_from: 10,
        valid_until: 900,
        revoked_at: None,
        version,
        scope_kind: "holding".into(),
        scope_node_id: None,
        parent_id: None,
        source: "manual".into(),
        source_id: None,
        identity_status: Some("active".into()),
        login_policy: Some("local".into()),
        person_id: None,
        connection_id: None,
        identity_version: Some(1),
        group_name: None,
        mapping_connection_id: None,
        directory_enabled: None,
        observed_at: None,
        connection_status: None,
        active: true,
    }
}

fn role() -> AssignmentRoleRow {
    AssignmentRoleRow {
        id: "employee".into(),
        name: "Employee".into(),
        company_id: "*".into(),
        revision: 1,
        built_in: true,
        permissions: vec!["iam.self".into()],
        grantable_permissions: vec![],
        scopes: vec!["holding".into()],
        status: "active".into(),
    }
}

#[derive(Default)]
struct Fake {
    authorized: bool,
    rows: Vec<AssignmentRow>,
    roles: Vec<AssignmentRoleRow>,
    target: Option<AssignmentRow>,
    facts: Option<AssignmentGuardFacts>,
    receipt: Option<ReceiptSnapshot>,
    commits: Vec<RevocationPlan>,
    digest: Option<String>,
    fail_commit: bool,
}

fn facts() -> AssignmentGuardFacts {
    AssignmentGuardFacts {
        same_person: false,
        recovery_member: false,
        other_recovery_administrator: true,
        other_administrator: true,
    }
}

impl AssignmentRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a AssignmentActor,
        _: bool,
        _: i64,
    ) -> AssignmentFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.authorized) })
    }
    fn digest<'a>(&'a mut self, _: &'a str) -> AssignmentFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok(self.digest.clone().unwrap_or_else(|| "a".repeat(64))) })
    }
    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> AssignmentFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move { Ok(self.receipt.clone()) })
    }
    fn workspace_rows<'a>(
        &'a mut self,
        _: &'a AssignmentActor,
        _: i64,
    ) -> AssignmentFuture<'a, Vec<AssignmentRow>, Self::Error> {
        Box::pin(async move { Ok(self.rows.clone()) })
    }
    fn workspace_roles<'a>(
        &'a mut self,
        _: &'a AssignmentActor,
        _: &'a [String],
    ) -> AssignmentFuture<'a, Vec<AssignmentRoleRow>, Self::Error> {
        Box::pin(async move { Ok(self.roles.clone()) })
    }
    fn assignment_for_update<'a>(
        &'a mut self,
        _: &'a AssignmentActor,
        _: &'a str,
        _: i64,
    ) -> AssignmentFuture<'a, Option<AssignmentRow>, Self::Error> {
        Box::pin(async move { Ok(self.target.clone()) })
    }
    fn guard_facts<'a>(
        &'a mut self,
        _: &'a AssignmentActor,
        _: &'a AssignmentRow,
        _: i64,
    ) -> AssignmentFuture<'a, AssignmentGuardFacts, Self::Error> {
        Box::pin(async move { Ok(self.facts.clone().unwrap_or_else(facts)) })
    }
    fn commit_revoke<'a>(
        &'a mut self,
        _: &'a AssignmentActor,
        _: &'a str,
        plan: &'a RevocationPlan,
        _: &'a str,
    ) -> AssignmentFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            if self.fail_commit {
                return Err(erpx_server_identity_access::public::assignment::readback_invalid());
            }
            self.commits.push(plan.clone());
            Ok(AssignmentReceipt::revoked(plan.id.clone(), plan.assignment_version).to_wire())
        })
    }
}

fn command() -> (CommandKey, AssignmentRevocation) {
    (
        CommandKey::new("iam-revoke-key-1").unwrap(),
        AssignmentRevocation {
            expected_version: 3,
            reason: "  Reviewed withdrawal  ".into(),
        },
    )
}

fn prior(actor: &AssignmentActor, version: i64) -> ReceiptSnapshot {
    ReceiptSnapshot {
        request_hash: "a".repeat(64),
        result_json: json!({"id": "assignment-target", "status": "revoked", "version": version})
            .to_string(),
        operation: "iam.assignment.revoke".into(),
        actor_id: actor.user_id.as_str().into(),
        record_id: "assignment-target".into(),
        authority_json: actor.authority_json(),
    }
}

#[tokio::test]
async fn workspace_requires_current_read_and_returns_exact_scoped_rows_and_roles() {
    let actor = actor_with(&["iam.read"]);
    let mut fake = Fake {
        authorized: true,
        rows: vec![row(3)],
        roles: vec![role()],
        ..Fake::default()
    };
    let value = assignment_workspace(&mut fake, &actor, 500).await.unwrap();
    assert_eq!(value["assignments"][0]["id"], "assignment-target");
    assert_eq!(value["assignments"][0]["active"], true);
    assert_eq!(value["roles"][0]["name"], "Employee");
    assert_eq!(value["evaluatedAt"], 500);
    assert_eq!(value["currentAssignmentId"], "assignment-actor");
    assert_eq!(value["currentUserId"], "actor");
    assert_eq!(value["scopeKind"], "holding");

    let denied = assignment_workspace(&mut fake, &actor_with(&["iam.manage"]), 500)
        .await
        .err()
        .unwrap();
    assert_eq!(denied.code, "PERMISSION_REQUIRED");

    let expired = assignment_workspace(&mut fake, &actor, 1_000)
        .await
        .err()
        .unwrap();
    assert_eq!(expired.code, "WORK_CONTEXT_CHANGED");

    fake.authorized = false;
    let changed = assignment_workspace(&mut fake, &actor, 500)
        .await
        .err()
        .unwrap();
    assert_eq!(changed.code, "IAM_ACCESS_CHANGED");
}

#[tokio::test]
async fn workspace_rejects_foreign_rows_duplicates_and_unknown_sources() {
    let actor = actor_with(&["iam.read"]);
    let mutate = |change: fn(&mut AssignmentRow)| {
        let mut value = row(3);
        change(&mut value);
        Fake {
            authorized: true,
            rows: vec![value],
            ..Fake::default()
        }
    };
    for (name, fake) in [
        ("foreign", mutate(|row| row.company_id = "foreign".into())),
        ("duplicate", {
            let mut value = row(3);
            let copy = value.clone();
            value.active = copy.active;
            Fake {
                authorized: true,
                rows: vec![value, copy],
                ..Fake::default()
            }
        }),
        ("source", mutate(|row| row.source = "invented".into())),
        (
            "scope",
            mutate(|row| {
                row.scope_kind = "company".into();
                row.scope_node_id = None;
            }),
        ),
    ] {
        let error = assignment_workspace(&mut { fake }, &actor, 500)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "IAM_ASSIGNMENT_SOURCE_INVALID", "{name}");
    }
}

#[tokio::test]
async fn withdrawal_requires_manager_scope_version_and_commits_trimmed_reason() {
    let actor = actor_with(&["iam.manage"]);
    let mut fake = Fake {
        authorized: true,
        target: Some(row(3)),
        ..Fake::default()
    };
    let (key, input) = command();
    let value = revoke_assignment(&mut fake, &actor, &key, "assignment-target", &input, 500)
        .await
        .unwrap();
    assert_eq!(
        value,
        json!({"id": "assignment-target", "status": "revoked", "version": 4})
    );
    assert_eq!(fake.commits.len(), 1);
    let plan = &fake.commits[0];
    assert_eq!(plan.expected_version, 3);
    assert_eq!(plan.assignment_version, 4);
    assert_eq!(plan.reason, "Reviewed withdrawal");
    assert_eq!(plan.revoked_at, 500);

    let denied = revoke_assignment(
        &mut fake,
        &actor_with(&["iam.read"]),
        &key,
        "assignment-target",
        &input,
        500,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(denied.code, "PERMISSION_REQUIRED");

    let invalid = AssignmentRevocation {
        expected_version: 0,
        reason: "  ".into(),
    };
    let invalid = revoke_assignment(&mut fake, &actor, &key, "assignment-target", &invalid, 500)
        .await
        .err()
        .unwrap();
    assert_eq!(invalid.code, "IAM_ASSIGNMENT_REQUEST_INVALID");
}

#[tokio::test]
async fn withdrawal_replays_the_exact_receipt_and_rejects_a_different_body() {
    let actor = actor_with(&["iam.manage"]);
    let mut fake = Fake {
        authorized: true,
        target: Some(row(3)),
        receipt: Some(prior(&actor, 4)),
        ..Fake::default()
    };
    let (key, input) = command();
    let value = revoke_assignment(&mut fake, &actor, &key, "assignment-target", &input, 500)
        .await
        .unwrap();
    assert_eq!(value["version"], 4);
    assert!(fake.commits.is_empty());

    fake.receipt = Some(ReceiptSnapshot {
        request_hash: "b".repeat(64),
        ..prior(&actor, 4)
    });
    let conflict = revoke_assignment(&mut fake, &actor, &key, "assignment-target", &input, 500)
        .await
        .err()
        .unwrap();
    assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn withdrawal_refuses_self_same_person_stale_revoked_and_foreign_scope() {
    let actor = actor_with(&["iam.manage"]);
    let (key, input) = command();
    let mut same = row(3);
    same.user_id = "actor".into();
    let mut same_person = Fake {
        authorized: true,
        target: Some(row(3)),
        facts: Some(AssignmentGuardFacts {
            same_person: true,
            ..facts()
        }),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(
            &mut same_person,
            &actor,
            &key,
            "assignment-target",
            &input,
            500
        )
        .await
        .err()
        .unwrap()
        .code,
        "SELF_ACCESS_CHANGE"
    );
    let mut self_row = Fake {
        authorized: true,
        target: Some(same),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(
            &mut self_row,
            &actor,
            &key,
            "assignment-target",
            &input,
            500
        )
        .await
        .err()
        .unwrap()
        .code,
        "SELF_ACCESS_CHANGE"
    );
    let mut stale = Fake {
        authorized: true,
        target: Some(row(2)),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(&mut stale, &actor, &key, "assignment-target", &input, 500)
            .await
            .err()
            .unwrap()
            .code,
        "ASSIGNMENT_CHANGED"
    );
    let mut revoked = Fake {
        authorized: true,
        target: Some(AssignmentRow {
            revoked_at: Some(400),
            ..row(3)
        }),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(&mut revoked, &actor, &key, "assignment-target", &input, 500)
            .await
            .err()
            .unwrap()
            .code,
        "ASSIGNMENT_CHANGED"
    );
    let branch_actor = AssignmentActor {
        scope_kind: "branch".into(),
        scope_node_id: Some(id("branch-1")),
        ..actor_with(&["iam.manage"])
    };
    let mut foreign_scope = Fake {
        authorized: true,
        target: Some(AssignmentRow {
            scope_kind: "branch".into(),
            scope_node_id: Some("branch-2".into()),
            ..row(3)
        }),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(
            &mut foreign_scope,
            &branch_actor,
            &key,
            "assignment-target",
            &input,
            500
        )
        .await
        .err()
        .unwrap()
        .code,
        "SCOPE_FORBIDDEN"
    );
}

#[tokio::test]
async fn withdrawal_protects_the_last_recovery_and_company_administrator() {
    let actor = actor_with(&["iam.manage"]);
    let (key, input) = command();
    let mut recovery = Fake {
        authorized: true,
        target: Some(AssignmentRow {
            role: "holding_owner".into(),
            ..row(3)
        }),
        facts: Some(AssignmentGuardFacts {
            recovery_member: true,
            other_recovery_administrator: false,
            ..facts()
        }),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(
            &mut recovery,
            &actor,
            &key,
            "assignment-target",
            &input,
            500
        )
        .await
        .err()
        .unwrap()
        .code,
        "LAST_RECOVERY_ADMINISTRATOR"
    );
    let mut owner = Fake {
        authorized: true,
        target: Some(AssignmentRow {
            role: "holding_owner".into(),
            ..row(3)
        }),
        facts: Some(AssignmentGuardFacts {
            other_administrator: false,
            ..facts()
        }),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(&mut owner, &actor, &key, "assignment-target", &input, 500)
            .await
            .err()
            .unwrap()
            .code,
        "LAST_ADMINISTRATOR"
    );
    let mut failing = Fake {
        authorized: true,
        target: Some(row(3)),
        fail_commit: true,
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(&mut failing, &actor, &key, "assignment-target", &input, 500)
            .await
            .err()
            .unwrap()
            .code,
        "IAM_ASSIGNMENT_READBACK_INVALID"
    );
}

#[tokio::test]
async fn malformed_digest_and_missing_ids_fail_closed_before_commit() {
    let actor = actor_with(&["iam.manage"]);
    let (key, input) = command();
    let mut fake = Fake {
        authorized: true,
        target: Some(row(3)),
        digest: Some("not-a-digest".into()),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(&mut fake, &actor, &key, "assignment-target", &input, 500)
            .await
            .err()
            .unwrap()
            .code,
        "IAM_ASSIGNMENT_READBACK_INVALID"
    );
    assert!(fake.commits.is_empty());
    let mut missing = Fake {
        authorized: true,
        target: None,
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(&mut missing, &actor, &key, "assignment-target", &input, 500)
            .await
            .err()
            .unwrap()
            .code,
        "ASSIGNMENT_NOT_FOUND"
    );
    let mut invalid_id = Fake {
        authorized: true,
        target: Some(row(3)),
        ..Fake::default()
    };
    assert_eq!(
        revoke_assignment(&mut invalid_id, &actor, &key, "", &input, 500)
            .await
            .err()
            .unwrap()
            .code,
        "ASSIGNMENT_NOT_FOUND"
    );
}

#[tokio::test]
async fn duplicate_role_or_missing_role_is_never_silently_invented() {
    let actor = actor_with(&["iam.read"]);
    let mut fake = Fake {
        authorized: true,
        rows: vec![row(3)],
        roles: vec![role(), role()],
        ..Fake::default()
    };
    assert_eq!(
        assignment_workspace(&mut fake, &actor, 500)
            .await
            .err()
            .unwrap()
            .code,
        "IAM_ASSIGNMENT_SOURCE_INVALID"
    );
    let mut empty = Fake {
        authorized: true,
        rows: vec![row(3)],
        roles: Vec::new(),
        ..Fake::default()
    };
    assert!(assignment_workspace(&mut empty, &actor, 500).await.is_ok());
}
