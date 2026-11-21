//! Files-owned Learning evidence port against PostgreSQL attachment sets.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_files::public::learning_evidence::PgLearningEvidence;
use erpx_server_learning::public::learning::{
    ASSIGNMENT_EVIDENCE_BINDING_TYPE, ASSIGNMENT_EVIDENCE_COMPONENT_KIND,
    ASSIGNMENT_EVIDENCE_PURPOSE, ASSIGNMENT_EVIDENCE_TARGET_TYPE, LearningEvidencePort,
    LearningEvidenceRequest,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{CompanyId, TimestampMillis};
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn request(component: &str) -> LearningEvidenceRequest {
    LearningEvidenceRequest {
        attachment_set_ids: vec!["set-completion".into()],
        target_id: "assignment-1".into(),
        target_type: ASSIGNMENT_EVIDENCE_TARGET_TYPE.into(),
        purpose_key: ASSIGNMENT_EVIDENCE_PURPOSE.into(),
        component_kind: ASSIGNMENT_EVIDENCE_COMPONENT_KIND.into(),
        component_ref: Some(component.into()),
        expected_target_version: 3,
        command_id: "learning-command-1".into(),
        binding_type: ASSIGNMENT_EVIDENCE_BINDING_TYPE.into(),
        binding_id: "assignment-1".into(),
    }
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1);\
         INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,status,created_by,version,created_at,updated_at) \
         VALUES('course-1','company','Safety','Safety course','Provider',30,'published','fixture',1,0,0);\
         INSERT INTO learning_assignments(id,company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,status,created_by,version,created_at,updated_at) \
         VALUES('assignment-1','company','company',NULL,'course-1',1,'learner','2026-12-01','assigned','fixture',3,0,0);\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) \
         VALUES('upload-1','learning_assignment','assignment-1','certificate','certificate.pdf','application/pdf',10,'staging-1','ready','fixture',4102444800000,0,0),\
         ('upload-2','learning_assignment','assignment-1','certificate','waiver.pdf','application/pdf',10,'staging-2','ready','fixture',4102444800000,0,0),\
         ('upload-3','learning_assignment','assignment-1','certificate','pending.pdf','application/pdf',10,'staging-3','ready','fixture',4102444800000,0,0);\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('attachment-1','upload-1','learning_assignment','assignment-1','certificate','original-1',1,'certificate.pdf','application/pdf',10,repeat('a',64),'fixture',0,'company','application/pdf','available',1,1),\
         ('attachment-2','upload-2','learning_assignment','assignment-1','certificate','original-2',1,'waiver.pdf','application/pdf',10,repeat('b',64),'fixture',0,'company','application/pdf','available',1,1),\
         ('attachment-3','upload-3','learning_assignment','assignment-1','certificate','original-3',1,'pending.pdf','application/pdf',10,repeat('c',64),'fixture',0,'company','application/pdf','quarantined',NULL,NULL);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-completion','company',NULL,NULL,'learning_assignment','assignment-1','action','completion','certificate',3,1,'confidential','[]',0,10,'draft','learner',0,0),\
         ('set-waiver','company',NULL,NULL,'learning_assignment','assignment-1','action','waiver','certificate',3,1,'confidential','[]',0,10,'draft','learner',0,0),\
         ('set-pending','company',NULL,NULL,'learning_assignment','assignment-1','action','completion','certificate',3,1,'confidential','[]',0,10,'draft','pending-fixture',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-completion','set-completion','attachment-1','draft','learner',0,4102444800000),\
         ('link-waiver','set-waiver','attachment-2','draft','learner',0,4102444800000),\
         ('link-pending','set-pending','attachment-3','draft','learner',0,4102444800000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn serial(connection: &mut PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(connection)
        .await
        .unwrap();
}

fn learning_port() -> PgLearningEvidence {
    learning_port_with_company(company())
}

fn learning_port_with_company(company: CompanyId) -> PgLearningEvidence {
    PgLearningEvidence::new(
        company,
        "learner".into(),
        None,
        None,
        TimestampMillis::new(NOW),
    )
}

#[tokio::test]
async fn binds_scanned_completion_evidence_and_rechecks_current_state() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = learning_port();
        let prepared = port
            .prepare(&mut tx, request("completion"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(prepared.set_ids, ["set-completion"]);
        assert_eq!(prepared.link_ids, ["link-completion"]);
        assert_eq!(prepared.policy_version, 1);
        assert_eq!(prepared.binding_type, ASSIGNMENT_EVIDENCE_BINDING_TYPE);
        assert_eq!(prepared.binding_id, "assignment-1");
        let state: (String, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT s.state,l.state,l.binding_type,l.binding_id FROM attachment_sets s \
             JOIN attachment_links l ON l.set_id=s.id WHERE s.id='set-completion'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            state,
            (
                "bound".into(),
                "bound".into(),
                Some("learning_completion".into()),
                Some("assignment-1".into())
            )
        );
        port.require_current(&mut tx, &prepared).await.unwrap();
        tx.rollback().await.unwrap();
        let draft: String =
            sqlx::query_scalar("SELECT state FROM attachment_sets WHERE id='set-completion'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(draft, "draft");
    })
    .await;
}

#[tokio::test]
async fn waiver_uses_the_waiver_component_and_stale_or_unscanned_sets_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = learning_port();
        let mut waiver = request("waiver");
        waiver.attachment_set_ids = vec!["set-waiver".into()];
        let prepared = port.prepare(&mut tx, waiver).await.unwrap().unwrap();
        assert_eq!(prepared.set_ids, ["set-waiver"]);
        tx.rollback().await.unwrap();

        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = learning_port();
        let mut pending = request("completion");
        pending.attachment_set_ids = vec!["set-pending".into()];
        let error = port.prepare(&mut tx, pending).await.err().unwrap();
        assert_eq!(error.code, "ATTACHMENT_SCAN_PENDING");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn context_and_company_mismatches_are_rejected() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = learning_port();
        let mut wrong = request("completion");
        wrong.attachment_set_ids = vec!["set-waiver".into()];
        let error = port.prepare(&mut tx, wrong).await.err().unwrap();
        assert_eq!(error.code, "ATTACHMENT_BINDING_CONTEXT");
        tx.rollback().await.unwrap();

        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut foreign = learning_port_with_company(CompanyId::new("foreign").unwrap());
        let error = foreign
            .prepare(&mut tx, request("completion"))
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "ATTACHMENT_SET_NOT_FOUND");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn require_current_detects_late_set_tampering() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = learning_port();
        let prepared = port
            .prepare(&mut tx, request("completion"))
            .await
            .unwrap()
            .unwrap();
        sqlx::query(
            "INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at)              VALUES('link-late','set-completion','attachment-2','draft','learner',1,4102444800000)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let error = port
            .require_current(&mut tx, &prepared)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "ATTACHMENT_BINDING_CHANGED");
        tx.rollback().await.unwrap();
    })
    .await;
}
