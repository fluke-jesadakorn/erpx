//! Files-owned Projects attachment port against PostgreSQL attachment sets and
//! the Projects version step.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_files::public::project_attachment::{
    PgProjectAttachmentFiles, bind_project_attachment_with_current_facts,
};
use erpx_server_projects::public::project_attachment::{
    ProjectAttachmentActor, ProjectAttachmentBindingCommand, ProjectAttachmentFactsRepository,
    ProjectAttachmentFilesPort, ProjectAttachmentFuture, ProjectAttachmentGrant,
    ProjectAttachmentPurpose, ProjectAttachmentTargetFacts, ProjectAttachmentTargetType,
    bind_project_attachment, witness,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => {
            BusinessError::invalid("TEST_PERSISTENCE_ERROR", "Unexpected persistence error.")
        }
    }
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn actor() -> ProjectAttachmentActor {
    ProjectAttachmentActor {
        user_id: "owner".into(),
        company_id: company(),
        branch_id: None,
        department_id: None,
        permissions: vec!["records.write".into(), "records.read".into()],
        modules: vec!["projects".into()],
        expires_at: 4_102_444_800_000,
        active_staff: true,
        wbs_purchased: true,
    }
}

fn facts() -> ProjectAttachmentTargetFacts {
    ProjectAttachmentTargetFacts {
        company_id: "company".into(),
        target_id: "project-1".into(),
        owner_user_id: "owner".into(),
        lifecycle_status: "active".into(),
        version: 3,
        is_owner: true,
        is_member: true,
    }
}

fn command(version: i64, link: &str) -> ProjectAttachmentBindingCommand {
    ProjectAttachmentBindingCommand {
        target_type: "project".into(),
        target_id: "project-1".into(),
        expected_version: version,
        purpose: "new-work".into(),
        link_ids: vec![link.into()],
        caption: Some("Site evidence".into()),
    }
}

fn grant(version: i64) -> ProjectAttachmentGrant {
    ProjectAttachmentGrant {
        company_id: company(),
        branch_id: None,
        department_id: None,
        target_type: ProjectAttachmentTargetType::Project,
        target_id: "project-1".into(),
        expected_version: version,
        actor_id: "owner".into(),
        capability: "projects.wbs",
        purpose: ProjectAttachmentPurpose::NewWork,
        witness: witness(&facts()),
    }
}

struct Repo {
    facts: Option<ProjectAttachmentTargetFacts>,
}

impl ProjectAttachmentFactsRepository for Repo {
    type Error = PgTransactionError<BusinessError>;

    fn target<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ProjectAttachmentFuture<'a, Option<ProjectAttachmentTargetFacts>, Self::Error> {
        Box::pin(async move { Ok(self.facts.clone()) })
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
         INSERT INTO staff_projects(id,company_id,title,description,owner_user_id,version,created_at,updated_at,lifecycle_status) \
         VALUES('project-1','company','Project','Project','owner',3,0,0,'active');\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) \
         VALUES('upload-1','project','project-1','project_document','plan.pdf','application/pdf',10,'staging-1','ready','owner',4102444800000,0,0),\
         ('upload-2','project','project-1','project_document','pending.pdf','application/pdf',10,'staging-2','ready','owner',4102444800000,0,0);\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('attachment-1','upload-1','project','project-1','project_document','original-1',1,'plan.pdf','application/pdf',10,repeat('a',64),'owner',0,'company','application/pdf','available',1,1),\
         ('attachment-2','upload-2','project','project-1','project_document','original-2',1,'pending.pdf','application/pdf',10,repeat('b',64),'owner',0,'company','application/pdf','quarantined',NULL,NULL);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-1','company',NULL,NULL,'project','project-1','record',NULL,'project_document',3,1,'internal','[]',0,50,'draft','owner',0,0),\
         ('set-pending','company',NULL,NULL,'project','project-1','record',NULL,'project_document',3,1,'internal','[]',0,50,'draft','pending-fixture',0,0),\
         ('set-stale','company',NULL,NULL,'project','project-1','record',NULL,'deliverable',2,1,'internal','[]',0,50,'draft','stale-fixture',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-1','set-1','attachment-1','draft','owner',0,4102444800000),\
         ('link-pending','set-pending','attachment-2','draft','owner',0,4102444800000),\
         ('link-stale','set-stale','attachment-1','draft','owner',0,4102444800000);",
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

#[tokio::test]
async fn binds_current_owner_links_and_advances_the_project_version() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut repo = Repo {
            facts: Some(facts()),
        };
        let mut files = PgProjectAttachmentFiles::new(&mut tx);
        let receipt = bind_project_attachment(
            &actor(),
            &mut repo,
            &mut files,
            &command(3, "link-1"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        assert_eq!(receipt.set_id, "set-1");
        assert_eq!(receipt.link_ids, ["link-1"]);
        assert_eq!(receipt.target_version, 4);
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM staff_projects WHERE id='project-1'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(version, 4);
        let state: (String, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT s.state,l.state,l.binding_type,l.binding_id FROM attachment_sets s \
             JOIN attachment_links l ON l.set_id=s.id WHERE s.id='set-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            state,
            (
                "bound".into(),
                "bound".into(),
                Some("record_document".into()),
                Some("project-1".into())
            )
        );
        tx.rollback().await.unwrap();
        let after: (i64, String) = sqlx::query_as(
            "SELECT p.version,s.state FROM staff_projects p CROSS JOIN attachment_sets s \
             WHERE p.id='project-1' AND s.id='set-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(after, (3, "draft".into()));
    })
    .await;
}

#[tokio::test]
async fn composition_facade_reads_current_facts_and_binds_in_one_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let receipt = bind_project_attachment_with_current_facts(
            &mut tx,
            &actor(),
            &command(3, "link-1"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        assert_eq!(receipt.set_id, "set-1");
        assert_eq!(receipt.target_version, 4);
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM staff_projects WHERE id='project-1'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(version, 4);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn quarantined_project_files_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut repo = Repo {
            facts: Some(facts()),
        };
        let mut files = PgProjectAttachmentFiles::new(&mut tx);
        let error = business(
            bind_project_attachment(
                &actor(),
                &mut repo,
                &mut files,
                &command(3, "link-pending"),
                TimestampMillis::new(NOW),
            )
            .await
            .unwrap_err(),
        );
        assert_eq!(error.code, "PROJECT_ATTACHMENT_CHANGED");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn unsupported_targets_and_stale_version_steps_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut files = PgProjectAttachmentFiles::new(&mut tx);
        let mut unsupported = command(3, "link-1");
        unsupported.target_type = "commerce_order".into();
        let error = business(files.bind(grant(3), &unsupported, NOW).await.unwrap_err());
        assert_eq!(error.code, "ATTACHMENT_TARGET_UNSUPPORTED");
        tx.rollback().await.unwrap();

        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut files = PgProjectAttachmentFiles::new(&mut tx);
        files
            .bind(grant(2), &command(2, "link-stale"), NOW)
            .await
            .unwrap();
        let error = business(files.advance_target(&grant(2), NOW).await.unwrap_err());
        assert_eq!(error.code, "PROJECT_ATTACHMENT_CHANGED");
        tx.rollback().await.unwrap();
    })
    .await;
}
