//! Native Learning commands and reads over real migrated PostgreSQL: current
//! IAM/People eligibility, reviewed courses, versions/CAS, idempotency,
//! audit/outbox, certificate handoff, rollback and persisted readback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_learning::public::eligibility_facts::{
    EligibilityQuery, EligibilityScope, learning_eligibility_consumer_facts,
    learning_eligibility_facts,
};
use erpx_server_learning::public::learning::{
    AssignmentAction, AssignmentChange, AssignmentRequest, Attendance, CertificateRevocation,
    CourseAction, CourseChange, CourseRequest, LearningActor, PgLearning, change_assignment,
    change_course, create_assignment, create_course, learning_detail, learning_workspace,
    revoke_certificate, utc_date,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use serde_json::Value;
use sqlx::{AssertSqlSafe, Row};

const NOW: i64 = 1_800_000_000_000;

fn actor(
    user: &str,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
) -> LearningActor {
    LearningActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        assignment_id: AssignmentId::new(format!("assignment-{user}")).unwrap(),
        role: Some("employee".into()),
        authority_revision: Some(3),
        assignment_version: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["learning".into()],
        expires_at: NOW + 300_000,
    }
}

fn reviewer() -> LearningActor {
    actor(
        "reviewer",
        "company",
        None,
        None,
        &["records.read", "records.write", "records.review"],
    )
}

fn author() -> LearningActor {
    actor(
        "author",
        "company",
        None,
        None,
        &["records.read", "records.write", "records.review"],
    )
}

fn learner() -> LearningActor {
    actor(
        "learner",
        "company",
        None,
        None,
        &["records.read", "records.write"],
    )
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn course_input() -> CourseRequest {
    CourseRequest {
        title: "Native safe equipment operation".into(),
        description: "Practical operating and emergency-stop procedure.".into(),
        provider: "Internal academy".into(),
        duration_minutes: 90,
        required_for: Some("equipment_operator".into()),
    }
}

fn change(action: CourseAction, expected_version: i64) -> CourseChange {
    CourseChange {
        action,
        expected_version,
        reason: "Independent reviewer checked the current course source".into(),
    }
}

fn assignment_input(course: &str, learner: &str) -> AssignmentRequest {
    AssignmentRequest {
        course_id: course.into(),
        course_version: 2,
        learner_id: learner.into(),
        due_date: "2026-11-30".into(),
        reason: "Required before operating production equipment".into(),
    }
}

fn complete(expected_version: i64) -> AssignmentChange {
    AssignmentChange {
        action: AssignmentAction::Complete,
        expected_version,
        reason: "Course and assessment completed".into(),
        attendance: Some(Attendance::Attended),
        score_bps: Some(9_500),
        evidence: Some("Provider result SIM-9500 and observed practical check".into()),
        certificate_valid_from: Some("2026-11-04".into()),
        certificate_expires_on: Some("2028-11-04".into()),
        attachment_set_ids: Vec::new(),
    }
}

fn action(action: AssignmentAction, expected_version: i64) -> AssignmentChange {
    AssignmentChange {
        action,
        expected_version,
        reason: match action {
            AssignmentAction::Start => "Learner opened the assigned course".into(),
            AssignmentAction::Waive => "Equivalent external certification verified".into(),
            AssignmentAction::Cancel => "Learner left before completion".into(),
            AssignmentAction::Complete => "Course and assessment completed".into(),
        },
        attendance: None,
        score_bps: None,
        evidence: None,
        certificate_valid_from: None,
        certificate_expires_on: None,
        attachment_set_ids: Vec::new(),
    }
}

fn business<T>(result: Result<T, PgTransactionError<BusinessError>>) -> Result<T, BusinessError> {
    match result {
        Ok(value) => Ok(value),
        Err(PgTransactionError::Business(error)) => Err(error),
        Err(other) => panic!("expected an owner business result, got {other:?}"),
    }
}

async fn run_create_course(
    pool: &DatabasePool,
    actor: LearningActor,
    key_value: &str,
    input: CourseRequest,
) -> Result<Value, BusinessError> {
    let key_value = key(key_value);
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let key_value = key_value.clone();
                let input = input.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    create_course(&mut repository, &actor, &key_value, &input, NOW).await
                })
            })
            .await,
    )
}

async fn run_change_course(
    pool: &DatabasePool,
    actor: LearningActor,
    id: &str,
    key_value: &str,
    input: CourseChange,
) -> Result<Value, BusinessError> {
    let id = id.to_owned();
    let key_value = key(key_value);
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let id = id.clone();
                let key_value = key_value.clone();
                let input = input.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    change_course(&mut repository, &actor, &key_value, &id, &input, NOW).await
                })
            })
            .await,
    )
}

async fn run_create_assignment(
    pool: &DatabasePool,
    actor: LearningActor,
    key_value: &str,
    input: AssignmentRequest,
) -> Result<Value, BusinessError> {
    let key_value = key(key_value);
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let key_value = key_value.clone();
                let input = input.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    create_assignment(&mut repository, &actor, &key_value, &input, NOW).await
                })
            })
            .await,
    )
}

async fn run_change_assignment(
    pool: &DatabasePool,
    actor: LearningActor,
    id: &str,
    key_value: &str,
    input: AssignmentChange,
) -> Result<Value, BusinessError> {
    let id = id.to_owned();
    let key_value = key(key_value);
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let id = id.clone();
                let key_value = key_value.clone();
                let input = input.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    change_assignment(&mut repository, &actor, &key_value, &id, &input, NOW).await
                })
            })
            .await,
    )
}

async fn run_revoke(
    pool: &DatabasePool,
    actor: LearningActor,
    id: &str,
    key_value: &str,
    input: CertificateRevocation,
) -> Result<Value, BusinessError> {
    let id = id.to_owned();
    let key_value = key(key_value);
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let id = id.clone();
                let key_value = key_value.clone();
                let input = input.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    revoke_certificate(&mut repository, &actor, &key_value, &id, &input, NOW).await
                })
            })
            .await,
    )
}

async fn run_workspace(
    pool: &DatabasePool,
    actor: LearningActor,
    personal: bool,
) -> Result<Value, BusinessError> {
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    learning_workspace(&mut repository, &actor, personal, NOW).await
                })
            })
            .await,
    )
}

async fn run_detail(
    pool: &DatabasePool,
    actor: LearningActor,
    id: &str,
) -> Result<Value, BusinessError> {
    let id = id.to_owned();
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let id = id.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    learning_detail(&mut repository, &actor, &id, NOW).await
                })
            })
            .await,
    )
}

async fn run_eligibility(
    pool: &DatabasePool,
    actor: LearningActor,
    person_id: &str,
    eligibility_code: &str,
    effective_date: &str,
) -> Result<Value, BusinessError> {
    let query = EligibilityQuery {
        person_id: person_id.into(),
        eligibility_code: eligibility_code.into(),
        effective_date: effective_date.into(),
    };
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let query = query.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    learning_eligibility_facts(&mut repository, &actor, &query, NOW).await
                })
            })
            .await,
    )
}

async fn run_consumer_eligibility(
    pool: &DatabasePool,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    person_id: &str,
    eligibility_code: &str,
    effective_date: &str,
) -> Result<Value, BusinessError> {
    let scope = EligibilityScope::new(company_id, branch_id, department_id).unwrap();
    let query = EligibilityQuery {
        person_id: person_id.into(),
        eligibility_code: eligibility_code.into(),
        effective_date: effective_date.into(),
    };
    business(
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let scope = scope.clone();
                let query = query.clone();
                Box::pin(async move {
                    let mut repository = PgLearning::new(connection);
                    learning_eligibility_consumer_facts(&mut repository, &scope, &query).await
                })
            })
            .await,
    )
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','learner','local:learner','active','local',1),\
         ('company','peer','local:peer','active','local',1),\
         ('company','reviewer','local:reviewer','active','local',1),\
         ('company','author','local:author','active','local',1);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES \
         ('person-learner','learner','company','Learner','TH','active','fixture',0,0),\
         ('person-peer','peer','company','Peer Learner','TH','active','fixture',0,0),\
         ('person-ineligible','ineligible','company','Ineligible Worker','TH','active','fixture',0,0),\
         ('person-outside','outside','company','Outside Worker','TH','active','fixture',0,0),\
         ('person-reviewer','reviewer','company','Reviewer','TH','active','fixture',0,0),\
         ('person-author','author','company','Author','TH','active','fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('engagement-learner','person-learner','company','branch','department','employee','salaried','active','2020-01-01','Learner','THB','fixture',0,0),\
         ('engagement-peer','person-peer','company','branch','other-department','employee','salaried','active','2020-01-01','Peer','THB','fixture',0,0),\
         ('engagement-ineligible','person-ineligible','company','branch','department','employee','salaried','active','2020-01-01','Worker','THB','fixture',0,0),\
         ('engagement-outside','person-outside','company','other-branch','outside','employee','salaried','active','2020-01-01','Outside','THB','fixture',0,0),\
         ('engagement-reviewer','person-reviewer','company','branch','department','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0),\
         ('engagement-author','person-author','company','branch','department','employee','salaried','active','2020-01-01','Author','THB','fixture',0,0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,parent_id,source) VALUES \
         ('assignment-learner','learner','employee','[\"learning\"]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-peer','peer','employee','[\"learning\"]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-reviewer','reviewer','employee','[\"learning\"]','company',NULL,0,9000000000000,'holding',NULL,'manual'),\
         ('assignment-author','author','employee','[\"learning\"]','company',NULL,0,9000000000000,'holding',NULL,'manual');\
         INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,required_for,status,created_by,reviewed_by,reviewed_at,version,created_at,updated_at) VALUES \
         ('course-required','company','Safe equipment operation','Practical operating and emergency-stop procedure.','Internal academy',90,'equipment_operator','published','author','reviewer',1799999990000,2,1799999700000,1799999990000),\
         ('course-plain','company','General Company induction','General Company induction and policy review.','Internal academy',45,NULL,'published','author','reviewer',1799999990000,2,1799999700000,1799999990000),\
         ('course-draft','company','Draft course','Draft course awaiting independent review.','Internal academy',30,NULL,'draft','author',NULL,NULL,1,1799999700000,1799999990000),\
         ('course-foreign','foreign','Foreign course','Foreign ineligible course description.','Foreign academy',30,NULL,'published','foreign-author','foreign-reviewer',1799999990000,2,1799999700000,1799999990000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor_branch(actor: LearningActor, branch: &str, department: Option<&str>) -> LearningActor {
    LearningActor {
        branch_id: Some(RecordId::new(branch).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        ..actor
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn course_unicode_and_safe_duration_round_trip_through_postgres() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let input = CourseRequest {
            title: "ก".repeat(160), description: "😀".repeat(1_500),
            provider: "😀".repeat(80), duration_minutes: 1_000_001,
            required_for: Some("ช".repeat(120)),
        };
        let created = run_create_course(&pool, author(), "learning-unicode-0001", input.clone()).await.unwrap();
        let id = created["id"].as_str().unwrap();
        let row = sqlx::query("SELECT title,description,provider,duration_minutes,required_for FROM learning_courses WHERE id=$1")
            .bind(id).fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row.get::<String,_>("title"), input.title);
        assert_eq!(row.get::<String,_>("description"), input.description);
        assert_eq!(row.get::<String,_>("provider"), input.provider);
        assert_eq!(row.get::<i64,_>("duration_minutes"), 1_000_001);
        assert_eq!(row.get::<Option<String>,_>("required_for"), input.required_for);
        let workspace = run_workspace(&pool, author(), false).await.unwrap();
        let saved = workspace["courses"].as_array().unwrap().iter().find(|row| row["id"] == id).unwrap();
        assert_eq!(saved["title"], input.title);
        assert_eq!(saved["description"], input.description);
        assert_eq!(saved["duration_minutes"], 1_000_001);
        for after_replay in [false, true] {
            if after_replay {
                assert_eq!(run_create_course(&pool, author(), "learning-unicode-0001", input.clone()).await.unwrap(), created);
            }
            let effects: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM learning_courses WHERE id=$1),(SELECT count(*) FROM learning_course_events WHERE course_id=$1 AND action='create'),(SELECT count(*) FROM command_receipts WHERE command_id=$2),(SELECT count(*) FROM audit_events WHERE command_id=$2),(SELECT count(*) FROM outbox_events WHERE command_id=$2)")
                .bind(id).bind("learning-unicode-0001").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(effects, (1,1,1,1,1), "after replay: {after_replay}");
        }
        let too_long = CourseRequest { title: format!("{}ก", "😀".repeat(80)), ..input.clone() };
        assert!(run_create_course(&pool, author(), "learning-unicode-invalid", too_long).await.is_err());
        let too_large = CourseRequest { duration_minutes: 9_007_199_254_740_992, ..input };
        assert!(run_create_course(&pool, author(), "learning-duration-invalid", too_large).await.is_err());
        let invalid_effects: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM learning_courses WHERE id IN ('learning-course-learning-unicode-invalid','learning-course-learning-duration-invalid')),(SELECT count(*) FROM learning_course_events WHERE course_id IN ('learning-course-learning-unicode-invalid','learning-course-learning-duration-invalid')),(SELECT count(*) FROM command_receipts WHERE command_id IN ('learning-unicode-invalid','learning-duration-invalid')),(SELECT count(*) FROM audit_events WHERE command_id IN ('learning-unicode-invalid','learning-duration-invalid')),(SELECT count(*) FROM outbox_events WHERE command_id IN ('learning-unicode-invalid','learning-duration-invalid'))").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(invalid_effects, (0,0,0,0,0));
        let controls = CourseRequest { title: "\u{0085}Learn\nSafety".into(), description: "First\nsecond".into(), provider: "Team".into(), duration_minutes: 30, required_for: None };
        let saved_control = run_create_course(&pool, author(), "learning-control-0001", controls.clone()).await.unwrap();
        let control_id = saved_control["id"].as_str().unwrap();
        let row: (String,String) = sqlx::query_as("SELECT title,description FROM learning_courses WHERE id=$1").bind(control_id).fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row, (controls.title.clone(),controls.description.clone()));
        let workspace = run_workspace(&pool, author(), false).await.unwrap();
        let row = workspace["courses"].as_array().unwrap().iter().find(|row| row["id"] == control_id).unwrap();
        assert_eq!(row["title"], controls.title);
        assert_eq!(row["description"], controls.description);
        assert!(run_create_course(&pool, author(), "learning-empty-code", CourseRequest { required_for: Some(" \u{feff}".into()), ..controls }).await.is_err());
    }).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn course_catalogue_is_independently_reviewed_versioned_and_evidenced() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let created = run_create_course(&pool, author(), "learning-course-0001", course_input())
            .await
            .unwrap();
        assert_eq!(
            created,
            serde_json::json!({"id":"learning-course-learning-course-0001","status":"draft","version":1})
        );
        let replay = run_create_course(&pool, author(), "learning-course-0001", course_input())
            .await
            .unwrap();
        assert_eq!(replay, created);
        let row = sqlx::query(
            "SELECT title,description,provider,duration_minutes,required_for,status,created_by,version FROM learning_courses WHERE id=$1",
        )
        .bind("learning-course-learning-course-0001")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("status"), "draft");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(row.get::<String, _>("required_for"), "equipment_operator");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM learning_course_events WHERE course_id=$1 AND action='create'"
            )
            .bind("learning-course-learning-course-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            let count = sqlx::query(AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE command_id=$1"
            )))
            .bind("learning-course-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap()
            .get::<i64, _>("count");
            assert_eq!(count, 1, "{table}");
        }

        let self_review = run_change_course(
            &pool,
            author(),
            "learning-course-learning-course-0001",
            "learning-course-0002",
            change(CourseAction::Publish, 1),
        )
        .await
        .unwrap_err();
        assert_eq!(self_review.code, "LEARNING_REVIEW_REQUIRED");
        let published = run_change_course(
            &pool,
            reviewer(),
            "learning-course-learning-course-0001",
            "learning-course-0003",
            change(CourseAction::Publish, 1),
        )
        .await
        .unwrap();
        assert_eq!(
            published,
            serde_json::json!({"id":"learning-course-learning-course-0001","status":"published","version":2})
        );
        let stale = run_change_course(
            &pool,
            reviewer(),
            "learning-course-learning-course-0001",
            "learning-course-0004",
            change(CourseAction::Publish, 1),
        )
        .await
        .unwrap_err();
        assert_eq!(stale.code, "LEARNING_COURSE_CHANGED");
        let foreign = run_change_course(
            &pool,
            actor(
                "foreign-reviewer",
                "foreign",
                None,
                None,
                &["records.read", "records.write", "records.review"],
            ),
            "course-required",
            "learning-course-0005",
            change(CourseAction::Retire, 2),
        )
        .await
        .unwrap_err();
        assert_eq!(foreign.code, "LEARNING_COURSE_NOT_FOUND");
        let workspace = run_workspace(&pool, learner(), false).await.unwrap();
        assert!(workspace["courses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "learning-course-learning-course-0001"
                && row["status"] == "published"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM learning_course_events WHERE course_id=$1"
            )
            .bind("learning-course-learning-course-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn assignment_creation_requires_current_published_course_and_eligible_worker() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let input = assignment_input("course-required", "learner");
        let created =
            run_create_assignment(&pool, reviewer(), "learning-assignment-0001", input.clone())
                .await
                .unwrap();
        assert_eq!(
            created,
            serde_json::json!({"id":"learning-assignment-learning-assignment-0001","status":"assigned","version":1})
        );
        let replay =
            run_create_assignment(&pool, reviewer(), "learning-assignment-0001", input.clone())
                .await
                .unwrap();
        assert_eq!(replay, created);
        let row = sqlx::query(
            "SELECT company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,status,version FROM learning_assignments WHERE id=$1",
        )
        .bind("learning-assignment-learning-assignment-0001")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("branch_id"), "branch");
        assert_eq!(row.get::<String, _>("department_id"), "department");
        assert_eq!(row.get::<String, _>("status"), "assigned");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM learning_assignment_events WHERE assignment_id=$1 AND action='assign'"
            )
            .bind("learning-assignment-learning-assignment-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            let count = sqlx::query(AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE command_id=$1"
            )))
            .bind("learning-assignment-0001")
            .fetch_one(pool.raw())
            .await
            .unwrap()
            .get::<i64, _>("count");
            assert_eq!(count, 1, "{table}");
        }

        let stale = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0002",
            AssignmentRequest {
                course_version: 3,
                ..input.clone()
            },
        )
        .await
        .unwrap_err();
        assert_eq!(stale.code, "LEARNING_SOURCE_CHANGED");
        let draft = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0003",
            assignment_input("course-draft", "learner"),
        )
        .await
        .unwrap_err();
        assert_eq!(draft.code, "LEARNING_SOURCE_CHANGED");
        let ineligible = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0004",
            assignment_input("course-required", "ineligible"),
        )
        .await
        .unwrap_err();
        assert_eq!(ineligible.code, "LEARNING_LEARNER_INELIGIBLE");
        let branch_scope =
            run_create_assignment(
                &pool,
                actor_branch(reviewer(), "other-branch", Some("outside")),
                "learning-assignment-0005",
                assignment_input("course-required", "learner"),
            )
            .await
            .unwrap_err();
        assert_eq!(branch_scope.code, "LEARNING_LEARNER_INELIGIBLE");
        let conflict = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0001",
            assignment_input("course-required", "peer"),
        )
        .await
        .unwrap_err();
        assert_eq!(conflict.code, "LEARNING_IDEMPOTENCY_CONFLICT");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM learning_assignments WHERE learner_id IN ('ineligible','peer')"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        let branch_reviewer = actor_branch(reviewer(), "branch", None);
        assert!(run_create_assignment(
            &pool,
            branch_reviewer,
            "learning-assignment-0006",
            assignment_input("course-plain", "peer"),
        )
        .await
        .is_ok());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn learner_progress_completion_and_certificate_are_atomic_and_replayable() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let created = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0101",
            assignment_input("course-required", "learner"),
        )
        .await
        .unwrap();
        let id = created["id"].as_str().unwrap().to_owned();
        let not_learner = run_change_assignment(
            &pool,
            reviewer(),
            &id,
            "learning-assignment-0102",
            action(AssignmentAction::Start, 1),
        )
        .await
        .unwrap_err();
        assert_eq!(not_learner.code, "LEARNING_LEARNER_REQUIRED");
        let started = run_change_assignment(
            &pool,
            learner(),
            &id,
            "learning-assignment-0103",
            action(AssignmentAction::Start, 1),
        )
        .await
        .unwrap();
        assert_eq!(
            started,
            serde_json::json!({"id":id,"status":"in_progress","version":2})
        );
        let invalid_dates = run_change_assignment(
            &pool,
            learner(),
            &id,
            "learning-assignment-0104",
            AssignmentChange {
                certificate_expires_on: Some("2025-01-01".into()),
                ..complete(2)
            },
        )
        .await
        .unwrap_err();
        assert_eq!(invalid_dates.code, "LEARNING_CERTIFICATE_DATES_INVALID");
        let completed = run_change_assignment(
            &pool,
            learner(),
            &id,
            "learning-assignment-0105",
            complete(2),
        )
        .await
        .unwrap();
        assert_eq!(
            completed,
            serde_json::json!({"id":id,"status":"completed","version":3})
        );
        assert_eq!(
            run_change_assignment(
                &pool,
                learner(),
                &id,
                "learning-assignment-0105",
                complete(2)
            )
            .await
            .unwrap(),
            completed
        );
        let stale = run_change_assignment(
            &pool,
            learner(),
            &id,
            "learning-assignment-0106",
            complete(2),
        )
        .await
        .unwrap_err();
        assert_eq!(stale.code, "LEARNING_STATE_CHANGED");
        let row = sqlx::query(
            "SELECT status,attendance,score_bps,result_evidence,completion_reason,version FROM learning_assignments WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("status"), "completed");
        assert_eq!(row.get::<String, _>("attendance"), "attended");
        assert_eq!(row.get::<i64, _>("score_bps"), 9_500);
        assert_eq!(row.get::<i64, _>("version"), 3);
        let handoff = sqlx::query(
            "SELECT person_id,eligibility_code,source_assignment_version,status FROM learning_eligibility_handoffs WHERE assignment_id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(handoff.get::<String, _>("person_id"), "person-learner");
        assert_eq!(handoff.get::<String, _>("eligibility_code"), "equipment_operator");
        assert_eq!(handoff.get::<i64, _>("source_assignment_version"), 3);
        let certificate = sqlx::query(
            "SELECT source_assignment_version,valid_from,expires_on,completion_kind,evidence_ref FROM learning_certificates WHERE assignment_id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(certificate.get::<i64, _>("source_assignment_version"), 3);
        assert_eq!(certificate.get::<String, _>("valid_from"), "2026-11-04");
        assert_eq!(certificate.get::<String, _>("expires_on"), "2028-11-04");
        assert_eq!(certificate.get::<String, _>("completion_kind"), "completed");
        let detail = run_detail(&pool, learner(), &id).await.unwrap();
        assert_eq!(detail["assignment"]["status"], "completed");
        assert_eq!(detail["assignment"]["attendance"], "attended");
        assert_eq!(detail["assignment"]["score_bps"], 9_500);
        assert_eq!(
            detail["assignment"]["result_evidence"],
            "Provider result SIM-9500 and observed practical check"
        );
        assert_eq!(
            detail["assignment"]["completion_reason"],
            "Course and assessment completed"
        );
        let workspace = run_workspace(&pool, learner(), false).await.unwrap();
        let assignment = workspace["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap();
        assert_eq!(assignment["attendance"], "attended");
        assert_eq!(assignment["score_bps"], 9_500);
        assert_eq!(detail["history"].as_array().unwrap().len(), 3);
        assert_eq!(detail["certificate"]["source_assignment_version"], 3);
        assert!(detail["certificate"]["revocation_id"].is_null());
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            let count = sqlx::query(AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE command_id=$1"
            )))
            .bind("learning-assignment-0105")
            .fetch_one(pool.raw())
            .await
            .unwrap()
            .get::<i64, _>("count");
            assert_eq!(count, 1, "{table}");
        }

        let second = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0201",
            assignment_input("course-plain", "peer"),
        )
        .await
        .unwrap();
        let second_id = second["id"].as_str().unwrap().to_owned();
        run_change_assignment(
            &pool,
            actor(
                "peer",
                "company",
                None,
                None,
                &["records.read", "records.write"],
            ),
            &second_id,
            "learning-assignment-0202",
            action(AssignmentAction::Start, 1),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE assignments SET revoked_at=$1,version=version+1 WHERE id='assignment-peer'")
            .bind(NOW)
            .execute(pool.raw())
            .await
            .unwrap();
        let ineligible = run_change_assignment(
            &pool,
            actor(
                "peer",
                "company",
                None,
                None,
                &["records.read", "records.write"],
            ),
            &second_id,
            "learning-assignment-0203",
            AssignmentChange {
                action: AssignmentAction::Complete,
                expected_version: 2,
                reason: "Completed after access ended".into(),
                attendance: Some(Attendance::Attended),
                score_bps: Some(9_000),
                evidence: Some("Provider result after access ended".into()),
                certificate_valid_from: Some("2026-11-04".into()),
                certificate_expires_on: None,
                attachment_set_ids: Vec::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(ineligible.code, "LEARNING_LEARNER_REQUIRED");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM learning_assignments WHERE id=$1")
                .bind(&second_id)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "in_progress"
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reviewer_waiver_cancellation_and_certificate_revocation_are_bounded() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let created = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0301",
            assignment_input("course-required", "learner"),
        )
        .await
        .unwrap();
        let id = created["id"].as_str().unwrap().to_owned();
        let waived = run_change_assignment(
            &pool,
            reviewer(),
            &id,
            "learning-assignment-0302",
            action(AssignmentAction::Waive, 1),
        )
        .await
        .unwrap();
        assert_eq!(
            waived,
            serde_json::json!({"id":id,"status":"waived","version":2})
        );
        let certificate = sqlx::query(
            "SELECT id,source_assignment_version,completion_kind,evidence_ref,valid_from FROM learning_certificates WHERE assignment_id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let certificate_id = certificate.get::<String, _>("id");
        assert_eq!(certificate.get::<i64, _>("source_assignment_version"), 2);
        assert_eq!(certificate.get::<String, _>("completion_kind"), "waived");
        assert_eq!(
            certificate.get::<String, _>("evidence_ref"),
            "Equivalent external certification verified"
        );
        assert_eq!(certificate.get::<String, _>("valid_from"), utc_date(NOW));

        let cancellation = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0401",
            assignment_input("course-plain", "peer"),
        )
        .await
        .unwrap();
        let cancellation_id = cancellation["id"].as_str().unwrap().to_owned();
        let cancelled = run_change_assignment(
            &pool,
            reviewer(),
            &cancellation_id,
            "learning-assignment-0402",
            action(AssignmentAction::Cancel, 1),
        )
        .await
        .unwrap();
        assert_eq!(
            cancelled,
            serde_json::json!({"id":cancellation_id,"status":"cancelled","version":2})
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM learning_certificates WHERE assignment_id=$1"
            )
            .bind(&cancellation_id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );

        let revoke = CertificateRevocation {
            expected_assignment_version: 2,
            reason: "Independent owner retires this retained certificate".into(),
        };
        let learner_revoke =
            run_revoke(&pool, learner(), &certificate_id, "learning-certificate-0501", revoke.clone())
                .await
                .unwrap_err();
        assert_eq!(learner_revoke.code, "LEARNING_REVIEW_REQUIRED");
        let revoked = run_revoke(
            &pool,
            reviewer(),
            &certificate_id,
            "learning-certificate-0502",
            revoke.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            revoked,
            serde_json::json!({"id":certificate_id,"status":"revoked","version":2})
        );
        assert_eq!(
            run_revoke(
                &pool,
                reviewer(),
                &certificate_id,
                "learning-certificate-0502",
                revoke.clone()
            )
            .await
            .unwrap(),
            revoked
        );
        let again = run_revoke(
            &pool,
            reviewer(),
            &certificate_id,
            "learning-certificate-0503",
            revoke.clone(),
        )
        .await
        .unwrap_err();
        assert_eq!(again.code, "LEARNING_CERTIFICATE_REVOKED");
        let stale = run_revoke(
            &pool,
            reviewer(),
            &certificate_id,
            "learning-certificate-0504",
            CertificateRevocation {
                expected_assignment_version: 1,
                ..revoke
            },
        )
        .await
        .unwrap_err();
        assert_eq!(stale.code, "LEARNING_CERTIFICATE_NOT_FOUND");
        let foreign = run_revoke(
            &pool,
            actor(
                "foreign-reviewer",
                "foreign",
                None,
                None,
                &["records.read", "records.write", "records.review"],
            ),
            &certificate_id,
            "learning-certificate-0505",
            CertificateRevocation {
                expected_assignment_version: 2,
                reason: "Foreign reviewer must not revoke this evidence".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(foreign.code, "LEARNING_CERTIFICATE_NOT_FOUND");
        let detail = run_detail(&pool, reviewer(), &id).await.unwrap();
        assert_eq!(detail["certificate"]["revocation_id"], "certificate-revoke-learning-certificate-0502");
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            let count = sqlx::query(AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE command_id=$1"
            )))
            .bind("learning-certificate-0502")
            .fetch_one(pool.raw())
            .await
            .unwrap()
            .get::<i64, _>("count");
            assert_eq!(count, 1, "{table}");
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn workspace_and_detail_preserve_company_branch_department_and_personal_scope() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let learner_assignment = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0601",
            assignment_input("course-required", "learner"),
        )
        .await
        .unwrap();
        let learner_id = learner_assignment["id"].as_str().unwrap().to_owned();
        let peer_assignment = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0602",
            assignment_input("course-plain", "peer"),
        )
        .await
        .unwrap();
        let peer_id = peer_assignment["id"].as_str().unwrap().to_owned();

        let workspace = run_workspace(&pool, reviewer(), false).await.unwrap();
        let ids = workspace["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(ids.contains(&learner_id.as_str()));
        assert!(ids.contains(&peer_id.as_str()));
        assert_eq!(workspace["canWrite"], true);
        assert_eq!(workspace["canReview"], true);
        assert_eq!(workspace["actorId"], "reviewer");
        assert!(
            workspace["people"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["id"] == "learner")
        );
        let personal_learner = run_workspace(&pool, learner(), true).await.unwrap();
        assert_eq!(personal_learner["assignments"].as_array().unwrap().len(), 1);
        assert_eq!(personal_learner["assignments"][0]["id"], learner_id);
        assert!(personal_learner["people"].as_array().unwrap().is_empty());
        let personal_reviewer = run_workspace(&pool, reviewer(), true).await.unwrap();
        assert!(
            personal_reviewer["assignments"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let branch_reviewer = actor_branch(reviewer(), "other-branch", Some("outside"));
        let branch_workspace = run_workspace(&pool, branch_reviewer.clone(), false)
            .await
            .unwrap();
        assert!(
            branch_workspace["assignments"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            run_detail(&pool, branch_reviewer, &learner_id)
                .await
                .unwrap_err()
                .code,
            "LEARNING_ASSIGNMENT_NOT_FOUND"
        );
        let department_reviewer = actor_branch(reviewer(), "branch", Some("department"));
        let department_workspace = run_workspace(&pool, department_reviewer, false)
            .await
            .unwrap();
        let department_ids = department_workspace["assignments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(department_ids.contains(&learner_id.as_str()));
        assert!(!department_ids.contains(&peer_id.as_str()));
        let foreign = actor(
            "foreign-reviewer",
            "foreign",
            None,
            None,
            &["records.read", "records.write", "records.review"],
        );
        assert_eq!(
            run_detail(&pool, foreign, &learner_id)
                .await
                .unwrap_err()
                .code,
            "LEARNING_ASSIGNMENT_NOT_FOUND"
        );
        let detail = run_detail(&pool, learner(), &learner_id).await.unwrap();
        assert_eq!(detail["assignment"]["learner_name"], "Learner");
        assert_eq!(detail["assignment"]["learner_eligible"], 1);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn transaction_guards_and_late_effects_leave_no_partial_learning_evidence() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let created = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0701",
            assignment_input("course-required", "learner"),
        )
        .await
        .unwrap();
        let id = created["id"].as_str().unwrap().to_owned();
        run_change_assignment(
            &pool,
            learner(),
            &id,
            "learning-assignment-0702",
            action(AssignmentAction::Start, 1),
        )
        .await
        .unwrap();

        let mut raw = pool.raw().acquire().await.unwrap();
        let no_transaction = {
            let mut repository = PgLearning::new(&mut raw);
            learning_workspace(&mut repository, &learner(), false, NOW)
                .await
                .unwrap_err()
        };
        assert!(matches!(no_transaction, PgTransactionError::Persistence(_)));
        drop(raw);

        let mut raw = pool.raw().acquire().await.unwrap();
        sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *raw)
            .await
            .unwrap();
        let read_committed = {
            let mut repository = PgLearning::new(&mut raw);
            learning_workspace(&mut repository, &learner(), false, NOW)
                .await
                .unwrap_err()
        };
        assert!(matches!(read_committed, PgTransactionError::Persistence(_)));
        sqlx::query("ROLLBACK").execute(&mut *raw).await.unwrap();
        drop(raw);

        sqlx::raw_sql(AssertSqlSafe(format!(
            "CREATE FUNCTION learning_fail_certificate() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN IF NEW.assignment_id = '{id}' THEN \
             RAISE EXCEPTION 'LEARNING_CERTIFICATE_FAILED' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; \
             CREATE TRIGGER learning_fail_certificate BEFORE INSERT ON learning_certificates \
             FOR EACH ROW EXECUTE FUNCTION learning_fail_certificate()"
        )))
        .execute(pool.raw())
        .await
        .unwrap();
        let failed = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), {
                let actor = learner();
                let id = id.clone();
                move |connection| {
                    let actor = actor.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        let mut repository = PgLearning::new(connection);
                        change_assignment(
                            &mut repository,
                            &actor,
                            &key("learning-assignment-0703"),
                            &id,
                            &complete(2),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await;
        if !matches!(failed, Err(PgTransactionError::Persistence(_))) {
            panic!("expected a persistence failure from the certificate trigger, got {failed:?}");
        }
        sqlx::query("DROP TRIGGER learning_fail_certificate ON learning_certificates")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION learning_fail_certificate()")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM learning_assignments WHERE id=$1")
                .bind(&id)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "in_progress"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM command_receipts WHERE command_id='learning-assignment-0703'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM learning_eligibility_handoffs WHERE assignment_id=$1"
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            run_change_assignment(
                &pool,
                learner(),
                &id,
                "learning-assignment-0703",
                complete(2)
            )
            .await
            .unwrap(),
            serde_json::json!({"id":id,"status":"completed","version":3})
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM learning_certificates WHERE assignment_id=$1"
            )
            .bind(&id)
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn eligibility_handoff_facts_are_current_scoped_and_consumption_preserving() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let created = run_create_assignment(
            &pool,
            reviewer(),
            "learning-assignment-0801",
            assignment_input("course-required", "learner"),
        )
        .await
        .unwrap();
        let id = created["id"].as_str().unwrap().to_owned();
        run_change_assignment(
            &pool,
            learner(),
            &id,
            "learning-assignment-0802",
            action(AssignmentAction::Start, 1),
        )
        .await
        .unwrap();
        run_change_assignment(
            &pool,
            learner(),
            &id,
            "learning-assignment-0803",
            complete(2),
        )
        .await
        .unwrap();

        let certificate = sqlx::query(
            "SELECT c.id,h.id handoff_id FROM learning_certificates c \
             JOIN learning_eligibility_handoffs h ON h.id=c.handoff_id \
             WHERE c.assignment_id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let certificate_id = certificate.get::<String, _>("id");
        let handoff_id = certificate.get::<String, _>("handoff_id");

        sqlx::raw_sql(
            "INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,\
             employee_deduction_minor,employer_cost_minor,effective_from,status,created_by,version,created_at,updated_at) \
             VALUES('plan-learning','company','Learning plan','Provider','employee','THB',0,0,'2026-01-01','published','reviewer',2,0,0);\
             INSERT INTO benefit_enrollment_windows(id,company_id,name,opens_on,closes_on,effective_on,status,created_by,version,created_at,updated_at) \
             VALUES('window-learning','company','Window','2026-01-01','2026-12-31','2026-01-01','open','reviewer',2,0,0);\
             INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,\
             coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,status,created_by,version,created_at,updated_at) \
             VALUES('election-learning','company','person-learner','engagement-learner','plan-learning',2,'window-learning',2,\
             'employee','[]','THB',0,0,'2026-01-01','elected','reviewer',1,0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO learning_benefit_consumption_receipts(id,handoff_id,certificate_id,election_id,\
             company_id,person_id,source_assignment_version,eligibility_code,source_digest,purpose,actor_id,command_id,created_at) \
             SELECT 'learning-benefit-learning',h.id,c.id,'election-learning',c.company_id,c.person_id,\
             c.source_assignment_version,c.eligibility_code,repeat('a',64),\
             'Benefits eligibility at election creation','benefits-reviewer','benefit-election-learning',$1 \
             FROM learning_certificates c JOIN learning_eligibility_handoffs h ON h.id=c.handoff_id \
             WHERE c.assignment_id=$2",
        )
        .bind(NOW)
        .bind(&id)
        .execute(pool.raw())
        .await
        .unwrap();

        let counts = |table: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                    .fetch_one(pool.raw())
                    .await
                    .unwrap()
            }
        };

        let facts = run_eligibility(
            &pool,
            learner(),
            "person-learner",
            "equipment_operator",
            "2026-11-04",
        )
        .await
        .unwrap();
        assert_eq!(facts["person_id"], "person-learner");
        assert_eq!(facts["eligibility_code"], "equipment_operator");
        assert_eq!(facts["effective_date"], "2026-11-04");
        assert_eq!(facts["eligible"], true);
        assert_eq!(facts["certificates"].as_array().unwrap().len(), 1);
        let current = &facts["certificates"][0];
        assert_eq!(current["id"], certificate_id);
        assert_eq!(current["handoff_id"], handoff_id);
        assert_eq!(current["company_id"], "company");
        assert_eq!(current["person_id"], "person-learner");
        assert_eq!(current["assignment_id"], id);
        assert_eq!(current["source_assignment_version"], 3);
        assert_eq!(current["handoff_source_assignment_version"], 3);
        assert_eq!(current["assignment_version"], 3);
        assert_eq!(current["assignment_status"], "completed");
        assert_eq!(current["completion_kind"], "completed");
        assert_eq!(current["person_status"], "active");
        assert_eq!(current["current"], true);
        assert!(current["revocation_id"].is_null());
        assert_eq!(current["consumptions"].as_array().unwrap().len(), 1);
        assert_eq!(
            current["consumptions"][0]["election_id"],
            "election-learning"
        );
        assert_eq!(
            current["consumptions"][0]["source_digest"],
            "a".repeat(64)
        );
        assert_eq!(
            current["consumptions"][0]["source_assignment_version"],
            3
        );

        let consumer = run_consumer_eligibility(
            &pool,
            "company",
            None,
            None,
            "person-learner",
            "equipment_operator",
            "2026-11-04",
        )
        .await
        .unwrap();
        assert_eq!(consumer["eligible"], true);
        assert_eq!(
            consumer["certificates"][0]["consumptions"][0]["election_id"],
            "election-learning"
        );
        assert_eq!(
            run_consumer_eligibility(
                &pool,
                "company",
                Some("branch"),
                Some("department"),
                "person-learner",
                "equipment_operator",
                "2026-11-04",
            )
            .await
            .unwrap()["eligible"],
            true
        );
        assert_eq!(
            run_consumer_eligibility(
                &pool,
                "company",
                Some("other-branch"),
                None,
                "person-learner",
                "equipment_operator",
                "2026-11-04",
            )
            .await
            .unwrap_err()
            .code,
            "LEARNING_ELIGIBILITY_NOT_FOUND"
        );
        assert_eq!(
            run_consumer_eligibility(
                &pool,
                "foreign",
                None,
                None,
                "person-learner",
                "equipment_operator",
                "2026-11-04",
            )
            .await
            .unwrap_err()
            .code,
            "LEARNING_ELIGIBILITY_NOT_FOUND"
        );

        let before_date = run_eligibility(
            &pool,
            learner(),
            "person-learner",
            "equipment_operator",
            "2026-11-03",
        )
        .await
        .unwrap();
        assert_eq!(before_date["eligible"], false);
        assert_eq!(before_date["certificates"][0]["current"], false);
        let after_expiry = run_eligibility(
            &pool,
            learner(),
            "person-learner",
            "equipment_operator",
            "2028-11-05",
        )
        .await
        .unwrap();
        assert_eq!(after_expiry["eligible"], false);

        assert_eq!(
            run_eligibility(
                &pool,
                learner(),
                "person-learner",
                "other_code",
                "2026-11-04"
            )
            .await
            .unwrap_err()
            .code,
            "LEARNING_ELIGIBILITY_NOT_FOUND"
        );
        assert_eq!(
            run_eligibility(
                &pool,
                learner(),
                "person-learner",
                "equipment_operator",
                "2026-02-30"
            )
            .await
            .unwrap_err()
            .code,
            "LEARNING_REQUEST_INVALID"
        );
        assert_eq!(
            run_eligibility(
                &pool,
                actor(
                    "foreign-reviewer",
                    "foreign",
                    None,
                    None,
                    &["records.read", "records.write", "records.review"],
                ),
                "person-learner",
                "equipment_operator",
                "2026-11-04"
            )
            .await
            .unwrap_err()
            .code,
            "LEARNING_ELIGIBILITY_NOT_FOUND"
        );
        for branch in [
            actor_branch(reviewer(), "other-branch", Some("outside")),
            actor_branch(reviewer(), "branch", Some("other-department")),
        ] {
            assert_eq!(
                run_eligibility(
                    &pool,
                    branch,
                    "person-learner",
                    "equipment_operator",
                    "2026-11-04"
                )
                .await
                .unwrap_err()
                .code,
                "LEARNING_ELIGIBILITY_NOT_FOUND"
            );
        }
        let in_scope = actor_branch(reviewer(), "branch", Some("department"));
        assert_eq!(
            run_eligibility(
                &pool,
                in_scope,
                "person-learner",
                "equipment_operator",
                "2026-11-04"
            )
            .await
            .unwrap()["eligible"],
            true
        );

        run_revoke(
            &pool,
            reviewer(),
            &certificate_id,
            "learning-certificate-0801",
            CertificateRevocation {
                expected_assignment_version: 3,
                reason: "Independent owner retires this certificate".into(),
            },
        )
        .await
        .unwrap();
        let revoked = run_eligibility(
            &pool,
            learner(),
            "person-learner",
            "equipment_operator",
            "2026-11-04",
        )
        .await
        .unwrap();
        assert_eq!(revoked["eligible"], false);
        assert_eq!(revoked["certificates"][0]["current"], false);
        assert_eq!(
            revoked["certificates"][0]["revocation_id"].as_str().unwrap(),
            "certificate-revoke-learning-certificate-0801"
        );
        assert_eq!(
            revoked["certificates"][0]["consumptions"].as_array().unwrap().len(),
            1
        );

        sqlx::query("UPDATE hr_people SET status='inactive' WHERE id='person-learner'")
            .execute(pool.raw())
            .await
            .unwrap();
        let before = (
            counts("command_receipts").await,
            counts("audit_events").await,
            counts("outbox_events").await,
            counts("learning_benefit_consumption_receipts").await,
        );
        let inactive = run_eligibility(
            &pool,
            learner(),
            "person-learner",
            "equipment_operator",
            "2026-11-04",
        )
        .await
        .unwrap();
        assert_eq!(inactive["eligible"], false);
        assert_eq!(inactive["certificates"][0]["person_status"], "inactive");
        assert_eq!(inactive["certificates"][0]["current"], false);

        sqlx::query("UPDATE hr_people SET status='active',merged_into_id='person-peer' WHERE id='person-learner'")
            .execute(pool.raw())
            .await
            .unwrap();
        let merged = run_eligibility(
            &pool,
            learner(),
            "person-learner",
            "equipment_operator",
            "2026-11-04",
        )
        .await
        .unwrap();
        assert_eq!(merged["eligible"], false);
        assert_eq!(merged["certificates"][0]["person_status"], "active");
        assert_eq!(
            merged["certificates"][0]["person_merged_into_id"],
            "person-peer"
        );
        assert_eq!(merged["certificates"][0]["current"], false);

        let after = (
            counts("command_receipts").await,
            counts("audit_events").await,
            counts("outbox_events").await,
            counts("learning_benefit_consumption_receipts").await,
        );
        assert_eq!(before, after);

        let mut raw = pool.raw().acquire().await.unwrap();
        let no_transaction = {
            let mut repository = PgLearning::new(&mut raw);
            learning_eligibility_facts(
                &mut repository,
                &learner(),
                &EligibilityQuery {
                    person_id: "person-learner".into(),
                    eligibility_code: "equipment_operator".into(),
                    effective_date: "2026-11-04".into(),
                },
                NOW,
            )
            .await
            .unwrap_err()
        };
        assert!(matches!(no_transaction, PgTransactionError::Persistence(_)));
        drop(raw);

        let mut raw = pool.raw().acquire().await.unwrap();
        sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *raw)
            .await
            .unwrap();
        let read_committed = {
            let mut repository = PgLearning::new(&mut raw);
            learning_eligibility_facts(
                &mut repository,
                &learner(),
                &EligibilityQuery {
                    person_id: "person-learner".into(),
                    eligibility_code: "equipment_operator".into(),
                    effective_date: "2026-11-04".into(),
                },
                NOW,
            )
            .await
            .unwrap_err()
        };
        assert!(matches!(read_committed, PgTransactionError::Persistence(_)));
        sqlx::query("ROLLBACK").execute(&mut *raw).await.unwrap();
        drop(raw);
    })
    .await;
}
