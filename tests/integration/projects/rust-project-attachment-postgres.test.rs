//! Projects-owned attachment bind, version step and idempotency receipts
//! against PostgreSQL. Files owns the set/link/source ledger; Projects owns the
//! project version and the command receipt.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_files::public::project_attachment::{
    bind_project_attachment_with_current_facts, project_attachment_bound_manifest,
    project_attachment_link_ids, project_attachment_set_facts,
};
use erpx_server_projects::public::project_attachment::{
    PROJECT_ATTACHMENT_BIND_OPERATION, PgProjectAttachmentCommandReceipts, ProjectAttachmentActor,
    ProjectAttachmentBindingCommand, ProjectAttachmentBoundLink,
    ProjectAttachmentCommandRepository, ProjectAttachmentReceiptPlan,
    project_attachment_authority_json, project_attachment_command_receipt,
    project_attachment_fingerprint, replay_project_attachment_binding,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "project-attachment-bind-0001";

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => {
            BusinessError::invalid("TEST_PERSISTENCE_ERROR", "Unexpected persistence error.")
        }
    }
}

fn actor(user: &str) -> ProjectAttachmentActor {
    ProjectAttachmentActor {
        user_id: user.into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        permissions: vec!["records.write".into(), "records.read".into()],
        modules: vec!["projects".into()],
        expires_at: 4_102_444_800_000,
        active_staff: true,
        wbs_purchased: true,
    }
}

fn command(expected_version: i64, link: &str) -> ProjectAttachmentBindingCommand {
    ProjectAttachmentBindingCommand {
        target_type: "project".into(),
        target_id: "project-1".into(),
        expected_version,
        purpose: "new-work".into(),
        link_ids: vec![link.into()],
        caption: Some("Site evidence".into()),
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
         VALUES('upload-1','project','project-1','project_document','plan.pdf','application/pdf',10,'staging-1','ready','owner',4102444800000,0,0);\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('attachment-1','upload-1','project','project-1','project_document','original-1',1,'plan.pdf','application/pdf',10,repeat('a',64),'owner',0,'company','application/pdf','available',1,1);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-1','company',NULL,NULL,'project','project-1','record',NULL,'project_document',3,1,'internal','[]',0,50,'draft','owner',0,0),\
         ('set-stale','company',NULL,NULL,'project','project-1','record',NULL,'deliverable',2,1,'internal','[]',0,50,'draft','stale-fixture',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-1','set-1','attachment-1','draft','owner',0,4102444800000),\
         ('link-stale','set-stale','attachment-1','draft','owner',0,4102444800000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn serial(connection: &mut sqlx::PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(connection)
        .await
        .unwrap();
}

#[tokio::test]
async fn binds_once_steps_the_project_and_replays_the_retained_receipt() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        assert_eq!(
            project_attachment_link_ids(&mut tx, "company", "set-1")
                .await
                .unwrap(),
            ["link-1"]
        );
        let facts = project_attachment_set_facts(&mut tx, "company", "set-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(facts.target_type, "project");
        assert_eq!(facts.component_kind, "record");
        assert_eq!(facts.purpose_key, "project_document");
        assert_eq!(facts.expected_target_version, 3);
        assert_eq!(facts.state, "draft");

        let stale = bind_project_attachment_with_current_facts(
            &mut tx,
            &actor("owner"),
            &command(3, "link-stale"),
            TimestampMillis::new(NOW),
        )
        .await;
        assert_eq!(
            business(stale.unwrap_err()).code,
            "PROJECT_ATTACHMENT_CHANGED"
        );

        let binding = bind_project_attachment_with_current_facts(
            &mut tx,
            &actor("owner"),
            &command(3, "link-1"),
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        assert_eq!(binding.target_version, 4);
        assert!(
            project_attachment_link_ids(&mut tx, "company", "set-1")
                .await
                .unwrap()
                .is_empty()
        );
        let manifest = project_attachment_bound_manifest(&mut tx, "company", "set-1", "project-1")
            .await
            .unwrap();
        assert_eq!(manifest.len(), 1);
        assert_eq!(manifest[0].link_id, "link-1");
        assert_eq!(manifest[0].source_id, "attachment-1");
        let bound = manifest
            .iter()
            .map(|item| ProjectAttachmentBoundLink {
                link_id: item.link_id.clone(),
                source_id: item.source_id.clone(),
            })
            .collect::<Vec<_>>();
        let receipt = project_attachment_command_receipt(&binding, &bound).unwrap();
        assert_eq!(receipt.id, "attachment-1");
        assert_eq!(receipt.version, 4);
        assert_eq!(receipt.owner_version, 4);
        assert_eq!(receipt.status, "bound");
        assert_eq!(receipt.attachment_set_id, "set-1");
        assert_eq!(receipt.attachment_link_id, "link-1");

        let plan = ProjectAttachmentReceiptPlan {
            key: KEY.into(),
            operation: PROJECT_ATTACHMENT_BIND_OPERATION.into(),
            company_id: "company".into(),
            actor_id: "owner".into(),
            request_fingerprint: project_attachment_fingerprint(&actor("owner"), "set-1", 3)
                .unwrap(),
            set_id: "set-1".into(),
            receipt: receipt.clone(),
            authority_json: project_attachment_authority_json(&actor("owner")).unwrap(),
            now: NOW,
        };
        let mut receipts = PgProjectAttachmentCommandReceipts::new(&mut tx);
        receipts.record_receipt(&plan).await.unwrap();
        drop(receipts);
        tx.commit().await.unwrap();

        let version: i64 =
            sqlx::query_scalar("SELECT version FROM staff_projects WHERE id='project-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 4);

        let mut replay_tx = pool.raw().begin().await.unwrap();
        serial(&mut replay_tx).await;
        let stored = ProjectAttachmentCommandRepository::stored_receipt(
            &mut PgProjectAttachmentCommandReceipts::new(&mut replay_tx),
            KEY,
        )
        .await
        .unwrap();
        let replayed =
            replay_project_attachment_binding(stored.clone(), &actor("owner"), "set-1", 3)
                .unwrap()
                .unwrap();
        assert_eq!(replayed, receipt);
        assert_eq!(
            replay_project_attachment_binding(stored.clone(), &actor("owner"), "set-1", 4)
                .unwrap_err()
                .code,
            "ATTACHMENT_SET_CHANGED"
        );
        assert_eq!(
            replay_project_attachment_binding(stored.clone(), &actor("owner"), "set-2", 3)
                .unwrap_err()
                .code,
            "ATTACHMENT_SET_CHANGED"
        );
        assert_eq!(
            replay_project_attachment_binding(stored, &actor("other"), "set-1", 3)
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT"
        );
        replay_tx.rollback().await.unwrap();

        let after: i64 =
            sqlx::query_scalar("SELECT version FROM staff_projects WHERE id='project-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(after, 4);
    })
    .await;
}
