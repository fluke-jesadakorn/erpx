#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_organization::public::staff_department_route::{
    DepartmentPersonFact, DepartmentRouteCheckFact, DepartmentRouteFact, DepartmentRouteStepFact,
    staff_department_route,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::FailureKind;
use serde_json::{Value, json};

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES
           ('company','company','acme','Acme',NULL,'company',NULL,'active'),
           ('dept-head','department','head-route','Head Route','company','company',NULL,'active'),
           ('dept-legacy','department','legacy','Legacy','company','company',NULL,'active'),
           ('dept-policy','department','policy','Policy','company','company',NULL,'active'),
           ('dept-dup','department','dup','Duplicate','company','company',NULL,'active'),
           ('dept-self','department','self','Self','company','company',NULL,'active'),
           ('dept-ceo','department','ceo','Ceo','company','company',NULL,'active'),
           ('dept-nohead','department','nohead','No Head','company','company',NULL,'active');
         INSERT INTO staff_levels(id,company_id,code,name,rank,active,version,created_at,updated_at,level_number) VALUES
           ('l1','company','L1','Level 1',1,1,1,0,0,1),
           ('l2','company','L2','Level 2',2,1,1,0,0,2),
           ('l3','company','L3','Level 3',3,1,1,0,0,3),
           ('l4','company','L4','Level 4',4,1,1,0,0,4),
           ('l5','company','L5','Level 5',5,1,1,0,0,5),
           ('l6','company','L6','Level 6',6,1,1,0,0,6);
         INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,is_ceo,active,version,created_by,created_at,updated_at) VALUES
           ('pos-ceo','company',NULL,'company','CEO','l1',1,1,1,'fixture',0,0),
           ('pos-head','company',NULL,'dept-head','Head','l3',0,1,1,'fixture',0,0),
           ('pos-legacy','company',NULL,'dept-legacy','Legacy Head','l3',0,1,1,'fixture',0,0),
           ('pos-mgr','company',NULL,'dept-policy','Manager','l2',0,1,1,'fixture',0,0),
           ('pos-dup','company',NULL,'dept-dup','Duplicate','l3',0,1,1,'fixture',0,0),
           ('pos-self','company',NULL,'dept-self','Self','l4',0,1,1,'fixture',0,0),
           ('pos-ceo-head','company',NULL,'dept-ceo','Ceo Head','l4',0,1,1,'fixture',0,0);
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,merged_into_id,created_by,created_at,updated_at) VALUES
           ('person-requester','user-requester','company','Requester','TH','active',NULL,'fixture',0,0),
           ('person-head','user-head','company','Head','TH','active',NULL,'fixture',0,0),
           ('person-legacy','user-legacy','company','Legacy Head','TH','active',NULL,'fixture',0,0),
           ('person-mgr','user-mgr','company','Manager','TH','active',NULL,'fixture',0,0),
           ('person-senior','user-senior','company','Senior','TH','active',NULL,'fixture',0,0),
           ('person-top','user-top','company','Top','TH','active',NULL,'fixture',0,0),
           ('person-dup-a','user-dup-a','company','Dup A','TH','active',NULL,'fixture',0,0),
           ('person-dup-b','user-dup-b','company','Dup B','TH','active',NULL,'fixture',0,0),
           ('person-self','user-self','company','Self','TH','active',NULL,'fixture',0,0),
           ('person-ceo-head','user-ceo-head','company','Ceo Head','TH','active',NULL,'fixture',0,0),
           ('person-ceo','user-ceo','company','Ceo','TH','active',NULL,'fixture',0,0);
         INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES
           ('company','head','dept-head',3,'pos-head','person-head',0),
           ('company','head','dept-policy',4,'pos-head','person-head',0),
           ('company','head','dept-self',1,'pos-self','person-self',0),
           ('company','head','dept-ceo',1,'pos-ceo-head','person-ceo-head',0);
         UPDATE authority_seats SET version=2,position_id='pos-ceo',person_id='person-ceo',effective_at=0
         WHERE company_id='company' AND kind='ceo' AND scope_id='company';
         INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at) VALUES
           ('dept-legacy','company','person-legacy',1,'fixture',0);
         INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) VALUES
           ('policy-1','company','dept-policy',5,'[{\"kind\":\"person\",\"personId\":\"person-head\"},{\"kind\":\"position\",\"positionId\":\"pos-mgr\"}]'::jsonb,'user-head','Reviewed flow','command-policy-1',0),
           ('policy-dup','company','dept-dup',2,'[{\"kind\":\"position\",\"positionId\":\"pos-dup\"}]'::jsonb,'user-head','Reviewed flow','command-policy-dup',0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn person(
    user_id: &str,
    person_id: &str,
    department_id: Option<&str>,
    position_id: Option<&str>,
    level_number: Option<i64>,
    manager_person_id: Option<&str>,
    engagement_id: &str,
    engagement_version: i64,
) -> DepartmentPersonFact {
    DepartmentPersonFact {
        user_id: user_id.into(),
        person_id: person_id.into(),
        department_id: department_id.map(str::to_owned),
        position_id: position_id.map(str::to_owned),
        level_number,
        manager_person_id: manager_person_id.map(str::to_owned),
        engagement_id: engagement_id.into(),
        engagement_version,
    }
}

fn snapshot(step: &DepartmentRouteStepFact) -> Value {
    serde_json::from_str(step.snapshot_json.as_deref().unwrap()).unwrap()
}

fn check(kind: &str, id: &str, version: i64) -> DepartmentRouteCheckFact {
    DepartmentRouteCheckFact {
        kind: kind.into(),
        id: id.into(),
        company_id: "company".into(),
        version,
    }
}

#[tokio::test]
async fn missing_department_and_missing_selector_keep_retained_blockers() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();

        let people = vec![person(
            "user-requester",
            "person-requester",
            None,
            None,
            Some(5),
            None,
            "eng-requester",
            1,
        )];
        assert_eq!(
            staff_department_route(&mut connection, "company", "user-requester", &people)
                .await
                .unwrap(),
            DepartmentRouteFact {
                steps: vec![],
                checks: vec![],
                blocked_reason: Some("Assign the requester to a department.".into()),
            }
        );

        let people = vec![person(
            "user-requester",
            "person-requester",
            Some("dept-nohead"),
            None,
            Some(5),
            None,
            "eng-requester",
            1,
        )];
        assert_eq!(
            staff_department_route(&mut connection, "company", "user-requester", &people)
                .await
                .unwrap()
                .blocked_reason
                .as_deref(),
            Some("Appoint a department head or publish an acting flow.")
        );
    })
    .await;
}

#[tokio::test]
async fn head_seat_and_legacy_head_routes_record_snapshot_and_engagement_checks() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();

        let people = vec![
            person(
                "user-requester",
                "person-requester",
                Some("dept-head"),
                None,
                Some(5),
                None,
                "eng-requester",
                1,
            ),
            person(
                "user-head",
                "person-head",
                Some("dept-head"),
                Some("pos-head"),
                Some(3),
                None,
                "eng-head",
                7,
            ),
        ];
        let route = staff_department_route(&mut connection, "company", "user-requester", &people)
            .await
            .unwrap();
        assert_eq!(
            route,
            DepartmentRouteFact {
                steps: vec![DepartmentRouteStepFact {
                    user_id: "user-head".into(),
                    authority: "department".into(),
                    candidate_user_ids: vec!["user-head".into()],
                    stage: Some("department_approval".into()),
                    snapshot_json: route.steps[0].snapshot_json.clone(),
                }],
                checks: vec![check("staff-engagement", "eng-head", 7)],
                blocked_reason: None,
            }
        );
        assert_eq!(
            snapshot(&route.steps[0]),
            json!({
                "departmentId": "dept-head",
                "departmentPolicyId": null,
                "departmentPolicyVersion": 0,
                "selector": {"kind": "person", "personId": "person-head"},
                "selectedPersonId": "person-head",
                "selectedPositionId": "pos-head",
                "originalPersonId": "person-head",
            })
        );

        // No recorded head seat falls back to the legacy department head.
        let people = vec![
            person(
                "user-requester",
                "person-requester",
                Some("dept-legacy"),
                None,
                Some(5),
                None,
                "eng-requester",
                1,
            ),
            person(
                "user-legacy",
                "person-legacy",
                Some("dept-legacy"),
                Some("pos-legacy"),
                Some(3),
                None,
                "eng-legacy",
                2,
            ),
        ];
        let route = staff_department_route(&mut connection, "company", "user-requester", &people)
            .await
            .unwrap();
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].user_id, "user-legacy");
        assert_eq!(
            route.checks,
            vec![check("staff-engagement", "eng-legacy", 2)]
        );
        assert_eq!(
            snapshot(&route.steps[0])["selector"],
            json!({"kind": "person", "personId": "person-legacy"})
        );
    })
    .await;
}

#[tokio::test]
async fn policy_route_selects_person_and_position_steps_with_versions() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let people = vec![
            person(
                "user-requester",
                "person-requester",
                Some("dept-policy"),
                None,
                Some(5),
                None,
                "eng-requester",
                1,
            ),
            person(
                "user-head",
                "person-head",
                Some("dept-policy"),
                Some("pos-head"),
                Some(3),
                None,
                "eng-head",
                7,
            ),
            person(
                "user-mgr",
                "person-mgr",
                Some("dept-policy"),
                Some("pos-mgr"),
                Some(2),
                None,
                "eng-mgr",
                2,
            ),
        ];
        let route = staff_department_route(&mut connection, "company", "user-requester", &people)
            .await
            .unwrap();
        assert_eq!(
            route.steps.iter().map(|step| step.user_id.as_str()).collect::<Vec<_>>(),
            vec!["user-head", "user-mgr"]
        );
        assert_eq!(
            route.checks,
            vec![
                check("department-policy", "policy-1", 5),
                check("staff-engagement", "eng-head", 7),
                check("staff-engagement", "eng-mgr", 2),
            ]
        );
        assert_eq!(
            snapshot(&route.steps[0]),
            json!({
                "departmentId": "dept-policy",
                "departmentPolicyId": "policy-1",
                "departmentPolicyVersion": 5,
                "selector": {"kind": "person", "personId": "person-head"},
                "selectedPersonId": "person-head",
                "selectedPositionId": "pos-head",
                "originalPersonId": "person-head",
            })
        );
        assert_eq!(
            snapshot(&route.steps[1]),
            json!({
                "departmentId": "dept-policy",
                "departmentPolicyId": "policy-1",
                "departmentPolicyVersion": 5,
                "selector": {"kind": "position", "positionId": "pos-mgr"},
                "selectedPersonId": "person-mgr",
                "selectedPositionId": "pos-mgr",
                "originalPersonId": "person-mgr",
            })
        );
        assert_eq!(route.blocked_reason, None);
    })
    .await;
}

#[tokio::test]
async fn self_review_escalates_to_the_nearest_manager_then_the_ceo() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();

        let people = vec![
            person(
                "user-self",
                "person-self",
                Some("dept-self"),
                Some("pos-self"),
                Some(4),
                Some("person-senior"),
                "eng-self",
                3,
            ),
            person(
                "user-senior",
                "person-senior",
                Some("dept-self"),
                None,
                Some(3),
                Some("person-top"),
                "eng-senior",
                4,
            ),
            person(
                "user-top",
                "person-top",
                Some("company"),
                None,
                Some(2),
                None,
                "eng-top",
                5,
            ),
        ];
        let route = staff_department_route(&mut connection, "company", "user-self", &people)
            .await
            .unwrap();
        assert_eq!(route.steps.len(), 1);
        assert_eq!(route.steps[0].user_id, "user-senior");
        assert_eq!(
            route.checks,
            vec![check("staff-engagement", "eng-senior", 4)]
        );
        assert_eq!(
            snapshot(&route.steps[0]),
            json!({
                "departmentId": "dept-self",
                "departmentPolicyId": null,
                "departmentPolicyVersion": 0,
                "selector": {"kind": "person", "personId": "person-self"},
                "selectedPersonId": "person-senior",
                "selectedPositionId": null,
                "originalPersonId": "person-self",
            })
        );

        // A head with no manager escalates to the independent CEO seat.
        let people = vec![
            person(
                "user-ceo-head",
                "person-ceo-head",
                Some("dept-ceo"),
                Some("pos-ceo-head"),
                Some(4),
                None,
                "eng-ceo-head",
                1,
            ),
            person(
                "user-ceo",
                "person-ceo",
                Some("company"),
                Some("pos-ceo"),
                Some(1),
                None,
                "eng-ceo",
                9,
            ),
        ];
        let route = staff_department_route(&mut connection, "company", "user-ceo-head", &people)
            .await
            .unwrap();
        assert_eq!(route.steps[0].user_id, "user-ceo");
        assert_eq!(snapshot(&route.steps[0])["selectedPersonId"], "person-ceo");

        // A CEO who is the requester is not independent; the retained route blocks.
        sqlx::query(
            "UPDATE authority_seats SET person_id='person-ceo-head' \
             WHERE company_id='company' AND kind='ceo' AND scope_id='company'",
        )
        .execute(&mut *connection)
        .await
        .unwrap();
        let route = staff_department_route(&mut connection, "company", "user-ceo-head", &people)
            .await
            .unwrap();
        assert_eq!(route.steps, vec![]);
        assert_eq!(route.checks, vec![]);
        assert_eq!(
            route.blocked_reason.as_deref(),
            Some("Appoint a higher reporting manager or independent CEO for this request.")
        );
    })
    .await;
}

#[tokio::test]
async fn duplicate_active_holders_block_the_step() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let people = vec![
            person(
                "user-requester",
                "person-requester",
                Some("dept-dup"),
                None,
                Some(5),
                None,
                "eng-requester",
                1,
            ),
            person(
                "user-dup-a",
                "person-dup-a",
                Some("dept-dup"),
                Some("pos-dup"),
                Some(3),
                None,
                "eng-dup-a",
                1,
            ),
            person(
                "user-dup-b",
                "person-dup-b",
                Some("dept-dup"),
                Some("pos-dup"),
                Some(3),
                None,
                "eng-dup-b",
                1,
            ),
        ];
        assert_eq!(
            staff_department_route(&mut connection, "company", "user-requester", &people)
                .await
                .unwrap(),
            DepartmentRouteFact {
                steps: vec![],
                checks: vec![],
                blocked_reason: Some(
                    "Step 1 needs exactly one active position holder and account.".into()
                ),
            }
        );
    })
    .await;
}

#[tokio::test]
async fn requester_must_have_exactly_one_active_staff_row() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let missing = Vec::new();
        let error = staff_department_route(&mut connection, "company", "user-requester", &missing)
            .await
            .unwrap_err();
        assert_eq!(error.code, "STAFF_LINK_REQUIRED");
        assert_eq!(error.kind, FailureKind::Conflict);
        assert_eq!(
            error.message,
            "An active employee or contractor engagement and account are required in this company."
        );

        let duplicated = vec![
            person(
                "user-requester",
                "person-requester",
                Some("dept-head"),
                None,
                Some(5),
                None,
                "eng-one",
                1,
            ),
            person(
                "user-requester",
                "person-requester",
                Some("dept-head"),
                None,
                Some(5),
                None,
                "eng-two",
                2,
            ),
        ];
        let error = staff_department_route(&mut connection, "company", "user-requester", &duplicated)
            .await
            .unwrap_err();
        assert_eq!(error.code, "STAFF_LINK_REQUIRED");
        assert_eq!(
            error.message,
            "Multiple active engagements need correction before this action."
        );

        let valid = vec![person(
            "user-requester",
            "person-requester",
            Some("dept-head"),
            None,
            Some(5),
            None,
            "eng-one",
            1,
        )];
        let error = staff_department_route(&mut connection, "", "user-requester", &valid)
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
        let error = staff_department_route(&mut connection, "company", "", &valid)
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_IDENTIFIER");
    })
    .await;
}
