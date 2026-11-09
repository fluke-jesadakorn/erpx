#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::organization_nodes::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, TimestampMillis, UserId,
};
use serde_json::json;

#[derive(Clone, Copy)]
struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn actor(
    company: &str,
    user: &str,
    assignment: &str,
    scope_kind: OrganizationScopeKind,
    branch: Option<&str>,
    department: Option<&str>,
) -> OrganizationCommandActor {
    let command = OrganizationCommand {
        user_id: UserId::new(user).unwrap(),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: "custom_organization".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(str::to_owned),
        department_id: department.map(str::to_owned),
        authority: Some(OrganizationAuthority {
            session_id: format!("session-{user}"),
            revision: 7,
            assignment_version: 1,
        }),
        iam_json: json!({
            "principalId": format!("principal-{user}"),
            "permissions": ["organization.manage"],
            "grantablePermissions": [],
            "personId": format!("person-{user}"),
            "personUserIds": [user],
            "freshUntil": null
        })
        .to_string(),
        assignment_scope_kind: match scope_kind {
            OrganizationScopeKind::Holding => "holding",
            OrganizationScopeKind::Branch => "branch",
            OrganizationScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: department.or(branch).map(str::to_owned),
    };
    OrganizationCommandActor {
        holding_id: HoldingId::new("h001").unwrap(),
        command,
        modules: vec!["people".into()],
        permissions: vec!["organization.manage".into()],
    }
}

fn owner() -> OrganizationCommandActor {
    actor(
        "company",
        "owner",
        "assignment-owner",
        OrganizationScopeKind::Holding,
        None,
        None,
    )
}

fn reviewer() -> OrganizationCommandActor {
    actor(
        "company",
        "reviewer",
        "assignment-reviewer",
        OrganizationScopeKind::Holding,
        None,
        None,
    )
}

fn change(action: &str, kind: &str, slug: Option<&str>, display: &str, branch: Option<&str>) -> NodeChangeInput {
    NodeChangeInput::parse(&json!({
        "action": action,
        "kind": kind,
        "slug": slug,
        "displayName": display,
        "branchId": branch,
        "targetNodeId": null,
        "expectedNodeVersion": null,
        "expectedVersion": 0,
        "reason": "Reviewed organization change"
    }))
    .unwrap()
}

fn existing(
    action: &str,
    kind: &str,
    target: &str,
    display: &str,
    branch: Option<&str>,
    expected_node_version: i64,
    expected_version: i64,
) -> NodeChangeInput {
    NodeChangeInput::parse(&json!({
        "action": action,
        "kind": kind,
        "slug": null,
        "displayName": display,
        "branchId": branch,
        "targetNodeId": target,
        "expectedNodeVersion": expected_node_version,
        "expectedVersion": expected_version,
        "reason": "Reviewed lifecycle change"
    }))
    .unwrap()
}

async fn seed(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('company','company','acme','Synthetic Acme',NULL,'company',NULL,'active'), \
               ('branch','branch','north','North','company','company','branch','active'), \
               ('department','department','service','Service','branch','company','branch','active'), \
               ('destination','branch','destination','Destination','company','company','destination','active'), \
               ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active');\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','custom_organization','Synthetic organization authority',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_organization',1,'[\"organization.manage\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) \
         VALUES('person-owner','owner','company','Owner','TH','active',1,'fixture',0,0), \
               ('person-reviewer','reviewer','company','Reviewer','TH','active',1,'fixture',0,0), \
               ('person-member','member','company','Member','TH','active',1,'fixture',0,0);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) \
         VALUES('company','owner','local:owner','person-owner','active','local',1), \
               ('company','reviewer','local:reviewer','person-reviewer','active','local',1), \
               ('company','member','local:member','person-member','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('assignment-owner','owner','custom_organization','[\"people\"]','company',0,9999999999999,1,'holding'), \
               ('assignment-reviewer','reviewer','custom_organization','[\"people\"]','company',0,9999999999999,1,'holding'), \
               ('assignment-branch','member','custom_organization','[\"people\"]','company',0,9999999999999,1,'branch'), \
               ('assignment-member','member','custom_organization','[\"people\"]','company',0,9999999999999,1,'department');\
         UPDATE assignments SET branch_id='branch',scope_node_id='branch' WHERE id='assignment-branch';\
         UPDATE assignments SET branch_id='branch',scope_node_id='department' WHERE id='assignment-member';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run<T, F>(
    pool: erpx_server_runtime::public::postgres::DatabasePool,
    f: F,
) -> Result<T, PgTransactionError<BusinessError>>
where
    F: for<'c> FnOnce(
            &'c mut sqlx::PgConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, BusinessError>> + Send + 'c>,
        > + Send
        + 'static,
    T: Send + 'static,
{
    PgUnitOfWork::new(pool)
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move { f(connection).await.map_err(PgTransactionError::Business) })
        })
        .await
}

fn business<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        Err(PgTransactionError::Persistence(error)) => {
            panic!("expected business denial {code}, got persistence {error:?}")
        }
        Ok(_) => panic!("expected business denial {code}, got success"),
    }
}

#[tokio::test]
async fn node_lifecycle_is_independent_reviewed_replay_safe_and_immutable() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        let created = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-create-branch-0001",
                    None,
                    change("create", "branch", Some("alpha"), "Alpha branch", None),
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(created["state"], "submitted");
        assert_eq!(created["version"], 1);

        let replay = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-create-branch-0001",
                    None,
                    change("create", "branch", Some("alpha"), "Alpha branch", None),
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(replay, created);
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM organization_node_changes WHERE id='org-change-organization-create-branch-0001'), \
             (SELECT count(*) FROM organization_node_change_events WHERE change_id='org-change-organization-create-branch-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1));

        let self_review = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    review_change(
                        &mut ports,
                        owner(),
                        "org-change-organization-create-branch-0001",
                        NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Self review"})).unwrap(),
                        "organization-review-self-0001",
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(self_review, "INDEPENDENT_REVIEW_REQUIRED");

        let approved = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                review_change(
                    &mut ports,
                    reviewer(),
                    "org-change-organization-create-branch-0001",
                    NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Capacity and authority reviewed"})).unwrap(),
                    "organization-review-branch-0001",
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(approved["state"], "applied");
        assert_eq!(approved["nodeId"], "org-node-org-change-organization-create-branch-0001");
        let node: (String, String, i64) = sqlx::query_as(
            "SELECT display_name,status,hierarchy_revision FROM organization_nodes WHERE id='org-node-org-change-organization-create-branch-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(node, ("Alpha branch".into(), "active".into(), 1));
        let channel: String = sqlx::query_scalar(
            "SELECT name FROM messaging_rooms WHERE organization_node_id='org-node-org-change-organization-create-branch-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(channel, "Alpha branch");
        let audit: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM audit_events WHERE command_id='organization-review-branch-0001'), \
             (SELECT count(*) FROM outbox_events WHERE command_id='organization-review-branch-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, (1, 1));
        assert!(
            sqlx::query("UPDATE organization_node_change_events SET action='rejected' WHERE change_id='org-change-organization-create-branch-0001'")
                .execute(pool.raw())
                .await
                .is_err()
        );

        let department = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-create-department-0001",
                    None,
                    change("create", "department", Some("alpha-service"), "Alpha service", Some("org-node-org-change-organization-create-branch-0001")),
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let department_node = "org-node-org-change-organization-create-department-0001";
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                review_change(
                    &mut ports,
                    reviewer(),
                    department["id"].as_str().unwrap(),
                    NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Team placement reviewed"})).unwrap(),
                    "organization-review-department-0001",
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let empty = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-create-empty-0001",
                    None,
                    change("create", "department", Some("alpha-empty"), "Alpha empty", Some("org-node-org-change-organization-create-branch-0001")),
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let empty_node = "org-node-org-change-organization-create-empty-0001";
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                review_change(
                    &mut ports,
                    reviewer(),
                    empty["id"].as_str().unwrap(),
                    NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Empty team placement reviewed"})).unwrap(),
                    "organization-review-empty-0001",
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,version,created_by,created_at,updated_at) \
             VALUES('engagement-member','person-member','company','org-node-org-change-organization-create-branch-0001',$1,'employee','salaried','active','2026-01-01','Service staff','THB',1,'fixture',0,0)",
        )
        .bind(department_node)
        .execute(pool.raw())
        .await
        .unwrap();
        let suspend = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-suspend-branch-0001",
                    None,
                    existing("suspend", "branch", "org-node-org-change-organization-create-branch-0001", "Alpha branch", None, 1, 0),
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let suspend_id = suspend["id"].as_str().unwrap().to_owned();
        let suspend_for_review = suspend_id.clone();
        let blocked = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    review_change(
                        &mut ports,
                        reviewer(),
                        &suspend_for_review,
                        NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Closure evidence reviewed"})).unwrap(),
                        "organization-review-suspend-0001",
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(blocked, "ORGANIZATION_NODE_IN_USE");
        let state: String = sqlx::query_scalar("SELECT state FROM organization_node_changes WHERE id=$1")
            .bind(&suspend_id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(state, "submitted");

        let archive = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-archive-empty-0001",
                    None,
                    existing("archive", "department", empty_node, "Alpha empty", Some("org-node-org-change-organization-create-branch-0001"), 1, 0),
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let archived = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                review_change(
                    &mut ports,
                    reviewer(),
                    archive["id"].as_str().unwrap(),
                    NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Archive evidence reviewed"})).unwrap(),
                    "organization-review-archive-0001",
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(archived["state"], "applied");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM organization_nodes WHERE id=$1")
                .bind(empty_node)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "archived"
        );
        let reactivated = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-reactivate-0001",
                    None,
                    existing("reactivate", "department", empty_node, "Alpha empty", Some("org-node-org-change-organization-create-branch-0001"), 2, 0),
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                review_change(
                    &mut ports,
                    reviewer(),
                    reactivated["id"].as_str().unwrap(),
                    NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Reactivation evidence reviewed"})).unwrap(),
                    "organization-review-reactivate-0001",
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM organization_nodes WHERE id=$1")
                .bind(empty_node)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "active"
        );
    })
    .await;
}

#[tokio::test]
async fn company_isolation_scope_stale_versions_and_authority_revocation_fail_closed() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        let proposal = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-company-proposal-0001",
                    None,
                    change("create", "branch", Some("beta"), "Beta branch", None),
                    false,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(proposal["state"], "draft");
        let stale = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    save_change(
                        &mut ports,
                        owner(),
                        "organization-company-proposal-0002",
                        Some("org-change-organization-company-proposal-0001"),
                        change("create", "branch", Some("beta"), "Beta overwrite", None),
                        true,
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(stale, "CONFLICT");
        assert_eq!(
            sqlx::query_as::<_, (String, i64)>("SELECT display_name,version FROM organization_node_changes WHERE id='org-change-organization-company-proposal-0001'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ("Beta branch".into(), 1)
        );

        let foreign = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    review_change(
                        &mut ports,
                        actor(
                            "foreign",
                            "foreign-reviewer",
                            "assignment-reviewer",
                            OrganizationScopeKind::Holding,
                            None,
                            None,
                        ),
                        "org-change-organization-company-proposal-0001",
                        NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Foreign review"})).unwrap(),
                        "organization-foreign-review-0001",
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(foreign, "ORGANIZATION_CHANGE_NOT_FOUND");

        let scoped = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                let branch_actor = actor(
                    "company",
                    "member",
                    "assignment-branch",
                    OrganizationScopeKind::Branch,
                    Some("branch"),
                    None,
                );
                let workspace = read_workspace(&mut ports, &branch_actor, &ChangeFilters::default())
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })?;
                Ok(workspace)
            })
        })
        .await
        .unwrap();
        let ids = scoped.nodes.iter().map(|node| node.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids, vec!["branch", "department"]);
        assert!(scoped.node_changes.is_empty());
        assert!(!scoped.can_manage_units);

        sqlx::query("UPDATE assignments SET revoked_at=1,version=2 WHERE id='assignment-owner'")
            .execute(pool.raw())
            .await
            .unwrap();
        let revoked = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    save_change(
                        &mut ports,
                        owner(),
                        "organization-revoked-write-0001",
                        None,
                        change("create", "branch", Some("gamma"), "Gamma branch", None),
                        true,
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(revoked, "ORGANIZATION_AUTHORITY_CHANGED");
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM organization_node_changes WHERE id='org-change-organization-revoked-write-0001')")
                .fetch_one(pool.raw())
                .await
                .unwrap()
                .eq(&false)
        );
    })
    .await;
}

#[tokio::test]
async fn relocation_moves_placement_authority_channels_and_rejects_stale_evidence() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        sqlx::raw_sql(
            "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,position_id,worker_type,employment_type,status,start_date,job_title,currency,version,created_by,created_at,updated_at) \
             VALUES('engagement-member','person-member','company','branch','department',NULL,'employee','salaried','active','2026-01-01','Service staff','THB',1,'fixture',0,0);\
             INSERT INTO employees(id,user_id,company_id,branch_id,department_id,display_name,status,created_at) \
             VALUES('employee-member','member','company','branch','department','Member','active',0);\
             INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,version,created_at,updated_at) \
             VALUES('level-6','company','ORG_L6','Staff',-6,6,1,1,0,0);\
             INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at) \
             VALUES('position-member','company','branch','department','Service staff','level-6','fixture',0,0);\
             INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) \
             VALUES('company','head','department',2,NULL,'person-member',1);\
             INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) \
             VALUES('department','company','person-member',2,'owner',1);\
             INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) \
             VALUES('source-flow','company','department',2,'[{\"kind\":\"person\",\"personId\":\"person-member\"}]'::jsonb,'owner','Published route','source-flow-command',1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let preview = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                preview_change(
                    &mut ports,
                    owner(),
                    existing("relocate", "department", "department", "Service", Some("destination"), 1, 0),
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(preview.impact.staff, 1);
        assert_eq!(preview.impact.positions, 1);
        assert_eq!(preview.impact.assignments, 1);
        assert_eq!(preview.source_branch_id.as_deref(), Some("branch"));
        assert_eq!(preview.destination_branch_id.as_deref(), Some("destination"));

        let input = NodeChangeInput::parse(&json!({
            "action":"relocate","kind":"department","slug":null,"displayName":"Service",
            "branchId":"destination","targetNodeId":"department","expectedNodeVersion":1,
            "expectedVersion":0,"reason":"Move the team to its operating branch",
            "previewSnapshot":preview.preview_snapshot
        }))
        .unwrap();
        sqlx::query("UPDATE organization_positions SET version=version+1 WHERE id='position-member'")
            .execute(pool.raw())
            .await
            .unwrap();
        let stale_submission = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    save_change(
                        &mut ports,
                        owner(),
                        "organization-relocate-stale-0001",
                        None,
                        input,
                        true,
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(stale_submission, "ORGANIZATION_RELOCATION_CHANGED");
        assert_eq!(
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM organization_node_changes WHERE id='org-change-organization-relocate-stale-0001')")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            false
        );

        let fresh = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                preview_change(
                    &mut ports,
                    owner(),
                    existing("relocate", "department", "department", "Service", Some("destination"), 1, 0),
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let fresh_input = NodeChangeInput::parse(&json!({
            "action":"relocate","kind":"department","slug":null,"displayName":"Service",
            "branchId":"destination","targetNodeId":"department","expectedNodeVersion":1,
            "expectedVersion":0,"reason":"Move the team to its operating branch",
            "previewSnapshot":fresh.preview_snapshot
        }))
        .unwrap();
        let submitted = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-relocate-0001",
                    None,
                    fresh_input,
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        sqlx::query("UPDATE organization_positions SET version=version+1 WHERE id='position-member'")
            .execute(pool.raw())
            .await
            .unwrap();
        let submitted_id = submitted["id"].as_str().unwrap().to_owned();
        let submitted_for_review = submitted_id.clone();
        let stale = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    review_change(
                        &mut ports,
                        reviewer(),
                        &submitted_for_review,
                        NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Approve the reviewed relocation"})).unwrap(),
                        "organization-relocate-review-stale-0001",
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(stale, "ORGANIZATION_RELOCATION_CHANGED");
        assert_eq!(
            sqlx::query_as::<_, (String, Option<String>)>("SELECT state,target_node_id FROM organization_node_changes WHERE id=$1")
                .bind(&submitted_id)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ("submitted".into(), Some("department".into()))
        );
        let cancelled = run(pool.clone(), move |connection| {
            let submitted_id = submitted_id.clone();
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                cancel_change(
                    &mut ports,
                    owner(),
                    &submitted_id,
                    1,
                    "organization-relocate-cancel-0001",
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(cancelled["state"], "cancelled");

        let latest = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                preview_change(
                    &mut ports,
                    owner(),
                    existing("relocate", "department", "department", "Service", Some("destination"), 1, 0),
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let resubmit_input = NodeChangeInput::parse(&json!({
            "action":"relocate","kind":"department","slug":null,"displayName":"Service",
            "branchId":"destination","targetNodeId":"department","expectedNodeVersion":1,
            "expectedVersion":0,"reason":"Move the team to its operating branch",
            "previewSnapshot":latest.preview_snapshot
        }))
        .unwrap();
        let resubmitted = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                save_change(
                    &mut ports,
                    owner(),
                    "organization-relocate-0002",
                    None,
                    resubmit_input,
                    true,
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        let applied = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                review_change(
                    &mut ports,
                    reviewer(),
                    resubmitted["id"].as_str().unwrap(),
                    NodeDecisionInput::parse(&json!({"expectedVersion":1,"decision":"approve","reason":"Staff, positions and scoped access moved together"})).unwrap(),
                    "organization-relocate-review-0001",
                    &clock,
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    error => panic!("unexpected persistence {error:?}"),
                })
            })
        })
        .await
        .unwrap();
        assert_eq!(applied["state"], "applied");
        let moved = applied["nodeId"].as_str().unwrap().to_owned();
        assert_ne!(moved, "department");
        let old: (Option<String>, Option<String>, String) = sqlx::query_as("SELECT branch_id,parent_id,status FROM organization_nodes WHERE id='department'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(old, (Some("branch".into()), Some("branch".into()), "archived".into()));
        let new: (Option<String>, Option<String>, String) = sqlx::query_as("SELECT branch_id,parent_id,status FROM organization_nodes WHERE id=$1")
            .bind(&moved)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(new, (Some("destination".into()), Some("destination".into()), "active".into()));
        assert_eq!(
            sqlx::query_as::<_, (Option<String>, Option<String>)>("SELECT branch_id,department_id FROM hr_engagements WHERE id='engagement-member'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            (Some("destination".into()), Some(moved.clone()))
        );
        assert_eq!(
            sqlx::query_as::<_, (Option<String>, Option<String>)>("SELECT branch_id,department_id FROM employees WHERE id='employee-member'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            (Some("destination".into()), Some(moved.clone()))
        );
        assert_eq!(
            sqlx::query_as::<_, (Option<String>, Option<String>)>("SELECT branch_id,department_id FROM organization_positions WHERE id='position-member'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            (Some("destination".into()), Some(moved.clone()))
        );
        assert_eq!(
            sqlx::query_as::<_, (i64, Option<String>)>("SELECT version::bigint version,person_id FROM authority_seats WHERE company_id='company' AND kind='head' AND scope_id=$1")
                .bind(&moved)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            (1, Some("person-member".into()))
        );
        assert_eq!(
            sqlx::query_as::<_, (Option<String>, String)>("SELECT department_heads.person_id,authority_appointments.provenance FROM department_heads JOIN authority_appointments ON authority_appointments.scope_id=department_heads.department_id AND authority_appointments.company_id=department_heads.company_id WHERE department_heads.department_id=$1 AND authority_appointments.kind='head'")
                .bind(&moved)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            (Some("person-member".into()), "explicit".into())
        );
        let flow: (i64, serde_json::Value) = sqlx::query_as("SELECT version::bigint version,steps_json FROM department_flow_versions WHERE department_id=$1")
            .bind(&moved)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(flow.0, 1);
        assert_eq!(flow.1, json!([{"kind":"person","personId":"person-member"}]));

        let moved_for_workspace = moved.clone();
        let workspace = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                let member = actor(
                    "company",
                    "member",
                    "assignment-member",
                    OrganizationScopeKind::Department,
                    Some("destination"),
                    Some(&moved_for_workspace),
                );
                read_workspace(&mut ports, &member, &ChangeFilters::default())
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        let mut channel_nodes = workspace.authorized_channel_nodes.clone();
        channel_nodes.sort();
        let mut expected_channel_nodes = vec!["company".to_owned(), "destination".to_owned(), moved.clone()];
        expected_channel_nodes.sort();
        assert_eq!(channel_nodes, expected_channel_nodes);
        assert_eq!(workspace.channels.len(), 1);
        assert_eq!(workspace.channels[0].node_id, moved);
        assert_eq!(workspace.channels[0].room_id, format!("org-channel-{moved}"));
    })
    .await;
}

#[tokio::test]
async fn relocation_refuses_children_conflicts_and_rolls_back_on_a_late_failure() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let clock = TestClock;
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
             VALUES('child','team','child','Child','department','company','branch','active')",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let children = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    save_change(
                        &mut ports,
                        owner(),
                        "organization-relocate-children-0001",
                        None,
                        existing("relocate", "department", "department", "Service", Some("destination"), 1, 0),
                        true,
                        &clock,
                    )
                    .await
                })
            })
            .await;
        business(children, "RELOCATION_HAS_CHILDREN");
        assert_eq!(
            sqlx::query_as::<_, (String, Option<String>)>("SELECT status,branch_id FROM organization_nodes WHERE id='department'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            ("active".into(), Some("branch".into()))
        );

        let late_key = "organization-late-create-0001";
        sqlx::raw_sql(
            "CREATE FUNCTION organization_late_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'ORGANIZATION_LATE_FAILURE' USING ERRCODE='23514'; RETURN NEW; END $$;\
             CREATE CONSTRAINT TRIGGER organization_late_failure AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='organization-late-create-0001') EXECUTE FUNCTION organization_late_failure()",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let late = PgUnitOfWork::new(pool.clone())
            .run_once::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgOrganizationNodes::new(connection);
                    save_change(
                        &mut ports,
                        owner(),
                        late_key,
                        None,
                        change("create", "branch", Some("late"), "Late branch", None),
                        true,
                        &clock,
                    )
                    .await
                })
            })
            .await;
        assert!(matches!(late, Err(PgTransactionError::Persistence(_))));
        let rows: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM organization_node_changes WHERE id='org-change-organization-late-create-0001'), \
             (SELECT count(*) FROM organization_node_change_events WHERE change_id='org-change-organization-late-create-0001'), \
             (SELECT count(*) FROM command_receipts WHERE command_id='organization-late-create-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rows, (0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn page_read_returns_owned_catalog_facts_with_company_and_placement_isolation() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,active,version,created_at,updated_at) \
             VALUES('level-1','company','ORG_L1','CEO',-1,1,1,1,0,0), \
                   ('level-2','company','ORG_L2','Manager',-2,2,1,1,0,0), \
                   ('level-foreign','foreign','ORG_L1','Foreign',-1,1,1,1,0,0);\
             INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at) \
             VALUES('position-branch','company','branch',NULL,'Branch lead','level-1','fixture',0,0), \
                   ('position-department','company','branch','department','Service lead','level-2','fixture',0,0), \
                   ('position-other-department','company','branch','other-department','Other lead','level-2','fixture',0,0), \
                   ('position-other-branch','company','destination',NULL,'Destination lead','level-1','fixture',0,0), \
                   ('position-unplaced','company',NULL,NULL,'Company lead','level-1','fixture',0,0), \
                   ('position-foreign','foreign',NULL,NULL,'Foreign lead','level-foreign','fixture',0,0);\
             INSERT INTO organization_templates(id,company_id,name,positions_json,version,created_by,created_at,updated_at) \
             VALUES('template-company','company','Standard','[{\"key\":\"ceo\",\"title\":\"CEO\",\"levelNumber\":1,\"reportsToKey\":null,\"isCeo\":true}]',1,'fixture',0,0), \
                   ('template-foreign','foreign','Foreign','[]',1,'fixture',0,0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let owner_workspace = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                read_workspace(&mut ports, &owner(), &ChangeFilters::default())
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        let owner_positions = owner_workspace
            .positions
            .iter()
            .map(|position| position.id.as_str())
            .collect::<Vec<_>>();
        assert!(owner_positions.contains(&"position-branch"));
        assert!(owner_positions.contains(&"position-department"));
        assert!(owner_positions.contains(&"position-other-department"));
        assert!(owner_positions.contains(&"position-other-branch"));
        assert!(owner_positions.contains(&"position-unplaced"));
        assert!(!owner_positions.contains(&"position-foreign"));
        assert_eq!(
            owner_workspace
                .levels
                .iter()
                .map(|level| level.id.as_str())
                .collect::<Vec<_>>(),
            vec!["level-1", "level-2"]
        );
        assert_eq!(
            owner_workspace
                .templates
                .iter()
                .map(|template| template.id.as_str())
                .collect::<Vec<_>>(),
            vec!["template-company"]
        );
        assert_eq!(
            owner_workspace.templates[0].positions[0]["title"],
            json!("CEO")
        );
        assert!(owner_workspace.can_manage);
        assert!(owner_workspace.can_manage_units);
        assert!(owner_workspace.can_manage_spending);

        let department_workspace = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                let member = actor(
                    "company",
                    "member",
                    "assignment-member",
                    OrganizationScopeKind::Department,
                    Some("branch"),
                    Some("department"),
                );
                read_workspace(&mut ports, &member, &ChangeFilters::default())
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        let department_positions = department_workspace
            .positions
            .iter()
            .map(|position| position.id.as_str())
            .collect::<Vec<_>>();
        assert!(department_positions.contains(&"position-branch"));
        assert!(department_positions.contains(&"position-department"));
        assert!(department_positions.contains(&"position-unplaced"));
        assert!(!department_positions.contains(&"position-other-department"));
        assert!(!department_positions.contains(&"position-other-branch"));
        assert!(!department_workspace.can_manage_units);
        assert!(!department_workspace.can_manage_spending);

        let mut read_only = owner();
        read_only.permissions.clear();
        let read_only_workspace = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                read_workspace(&mut ports, &read_only, &ChangeFilters::default())
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        assert!(!read_only_workspace.can_manage);
        assert!(read_only_workspace.templates.is_empty());
        assert!(!read_only_workspace.can_manage_spending);
        assert_eq!(read_only_workspace.levels.len(), 2);
    })
    .await;
}

#[tokio::test]
async fn organization_authority_held_locks_cover_all_three_sources() {
    use erpx_server_organization::public::{lock_organization_authority_sources,require_organization_authority_sources_held};
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut outside=pool.raw().acquire().await.unwrap();assert!(require_organization_authority_sources_held(&mut outside).await.is_err());drop(outside);
        let mut partial=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE organization_nodes,authority_seats IN SHARE MODE").execute(&mut *partial).await.unwrap();
        assert!(matches!(require_organization_authority_sources_held(&mut partial).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="ORGANIZATION_AUTHORITY_LOCKS_REQUIRED"));partial.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_org_authority_locks").execute(&mut *tx).await.unwrap();
        lock_organization_authority_sources(&mut tx).await.unwrap();require_organization_authority_sources_held(&mut tx).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareLock' AND relation IN ('organization_nodes'::regclass,'authority_seats'::regclass,'organization_positions'::regclass)").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,3);
        sqlx::query("ROLLBACK TO SAVEPOINT before_org_authority_locks").execute(&mut *tx).await.unwrap();assert!(matches!(require_organization_authority_sources_held(&mut tx).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="ORGANIZATION_AUTHORITY_LOCKS_REQUIRED"));
        sqlx::query("LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_organization_authority_sources_held(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn organization_sales_authority_boundary_checks_canonical_sources_and_unit() {
    use erpx_server_organization::public::{lock_organization_authority_sources,inspect_organization_sales_authority_boundary};
    fixture::with_database(2,|pool|async move {
        seed(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        assert!(inspect_organization_sales_authority_boundary(&mut tx).await.is_err());
        sqlx::query("SAVEPOINT org_sources").execute(&mut *tx).await.unwrap();
        lock_organization_authority_sources(&mut tx).await.unwrap();
        let released=inspect_organization_sales_authority_boundary(&mut tx).await.unwrap();
        released.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT org_sources").execute(&mut *tx).await.unwrap();
        assert!(released.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("CREATE TEMP TABLE organization_nodes(id text)").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_organization_authority_sources(&mut tx).await.unwrap();
        let source=inspect_organization_sales_authority_boundary(&mut tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, pg_temp, public").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();tx.rollback().await.unwrap();
    }).await;
}
