//! Native Files read side for Project attachment sets, links, status and
//! verified private content against PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_files::public::canonical_original::OriginalByteStore;
use erpx_server_files::public::project_attachment::{
    ProjectAttachmentContentVariant, project_attachment_content,
    project_attachment_content_by_link, project_attachment_link_status,
    project_attachment_link_status_by_link, project_attachment_links,
    project_attachment_links_by_set, project_attachment_sets,
};
use erpx_server_projects::public::project_attachment::ProjectAttachmentActor;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use std::future::Future;

const NOW: i64 = 1_800_000_000_000;
const BODY: &[u8] = b"hello";

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn actor(company_id: CompanyId, user: &str) -> ProjectAttachmentActor {
    ProjectAttachmentActor {
        user_id: user.into(),
        company_id,
        branch_id: None,
        department_id: None,
        permissions: vec!["records.write".into(), "records.read".into()],
        modules: vec!["projects".into()],
        expires_at: 4_102_444_800_000,
        active_staff: true,
        wbs_purchased: true,
    }
}

fn now() -> TimestampMillis {
    TimestampMillis::new(NOW)
}

struct Store;

impl OriginalByteStore for Store {
    fn read_verified(
        &self,
        key: &str,
        size: usize,
        _checksum: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<u8>, BusinessError>> + Send {
        let key = key.to_owned();
        async move {
            if !(key.starts_with("original-") || key.starts_with("preview-"))
                || size != BODY.len()
                || limit < BODY.len()
            {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "FILE_ORIGINAL_STORAGE",
                    "The verified private file is unavailable.",
                ));
            }
            Ok(BODY.to_vec())
        }
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
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('other-company','company','other-company','Other Company',NULL,'other-company',NULL,'active',1);\
         INSERT INTO staff_projects(id,company_id,title,description,owner_user_id,version,created_at,updated_at,lifecycle_status) \
         VALUES('project-1','company','Project','Project','owner',3,0,0,'active'),\
         ('project-other','other-company','Other','Other','other-owner',3,0,0,'active');\
         INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) \
         VALUES('project-1','another-member','owner',0);\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) \
         VALUES('upload-1','project','project-1','project_document','plan.pdf','application/pdf',5,'staging-1','ready','owner',4102444800000,0,0),\
         ('upload-2','project','project-1','project_document','pending.pdf','application/pdf',5,'staging-2','ready','owner',4102444800000,0,0),\
         ('upload-3','project','project-1','project_document','revoked.pdf','application/pdf',5,'staging-3','ready','owner',4102444800000,0,0),\
         ('upload-4','project','project-1','project_document','oversize.pdf','application/pdf',5,'staging-4','ready','owner',4102444800000,0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at,preview_object_key,preview_bytes,preview_checksum_sha256,preview_media_type,revoked_at) \
         VALUES('attachment-1','upload-1','project','project-1','project_document','original-1',1,'plan.pdf','application/pdf',5,encode(sha256(convert_to('hello','UTF8')),'hex'),'owner',0,'company','application/pdf','available',1,1,'preview-1',5,encode(sha256(convert_to('hello','UTF8')),'hex'),'image/png',NULL),\
         ('attachment-2','upload-2','project','project-1','project_document','original-2',1,'pending.pdf','application/pdf',5,repeat('b',64),'owner',0,'company','application/pdf','quarantined',NULL,NULL,NULL,NULL,NULL,NULL,NULL),\
         ('attachment-3','upload-3','project','project-1','project_document','original-3',1,'revoked.pdf','application/pdf',5,encode(sha256(convert_to('hello','UTF8')),'hex'),'owner',0,'company','application/pdf','available',1,1,NULL,NULL,NULL,NULL,1),\
         ('attachment-4','upload-4','project','project-1','project_document','original-4',1,'oversize.pdf','application/pdf',10485761,encode(sha256(convert_to('hello','UTF8')),'hex'),'owner',0,'company','application/pdf','available',1,1,NULL,NULL,NULL,NULL,NULL);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-1','company',NULL,NULL,'project','project-1','record',NULL,'project_document',3,1,'internal','[]',0,50,'draft','owner',10,10),\
         ('set-pending','company',NULL,NULL,'project','project-1','record',NULL,'handoff',3,1,'internal','[]',0,50,'draft','owner',30,30),\
         ('set-revoked','company',NULL,NULL,'project','project-1','record',NULL,'deliverable',3,1,'internal','[]',0,50,'draft','owner',40,40),\
         ('set-private','company',NULL,NULL,'project','project-1','record',NULL,'project_document',3,1,'internal','[]',0,50,'draft','another-member',50,50);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at,bound_at,bound_command_id) \
         VALUES('set-bound','company',NULL,NULL,'project','project-1','record',NULL,'handoff',3,1,'internal','[]',0,50,'bound','another-member',20,20,20,'bind-1');\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) \
         VALUES('link-1','set-1','attachment-1','draft','owner',10,4102444800000,NULL,NULL,NULL),\
         ('link-bound','set-bound','attachment-1','bound','another-member',20,4102444800000,20,'record_document','project-1'),\
         ('link-pending','set-pending','attachment-2','draft','owner',30,4102444800000,NULL,NULL,NULL),\
         ('link-revoked','set-revoked','attachment-3','draft','owner',40,4102444800000,NULL,NULL,NULL),\
         ('link-oversize','set-1','attachment-4','draft','owner',10,4102444800000,NULL,NULL,NULL),\
         ('link-private','set-private','attachment-1','draft','another-member',50,4102444800000,NULL,NULL,NULL);\
         INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,scanner_engine,scanner_signature,scanner_definition_at,created_at,completed_at) \
         VALUES('scan-1','attachment-1',1,'clean','clamd','sig-1',1,1,2);"
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn code(error: BusinessError) -> String {
    error.code.to_string()
}

#[tokio::test]
async fn lists_visible_project_sets_links_and_exact_status() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        let owner = actor(company(), "owner");
        let sets = project_attachment_sets(&mut tx, &owner, "project-1", now())
            .await
            .unwrap();
        let ids = sets.iter().map(|set| set.id.as_str()).collect::<Vec<_>>();
        assert!(ids.contains(&"set-1"));
        assert!(ids.contains(&"set-bound"));
        assert!(ids.contains(&"set-pending"));
        assert!(ids.contains(&"set-revoked"));
        assert!(!ids.contains(&"set-private"));

        let first = sets.iter().find(|set| set.id == "set-1").unwrap();
        assert_eq!(first.target_type, "project");
        assert_eq!(first.target_id, "project-1");
        assert_eq!(first.component_kind, "record");
        assert_eq!(first.purpose_key, "project_document");
        assert_eq!(first.expected_target_version, 3);
        assert_eq!(first.policy_version, 1);
        assert_eq!(first.classification, "internal");
        assert_eq!(first.required_count, 0);
        assert_eq!(first.max_files, 50);
        assert_eq!(first.state, "draft");
        assert_eq!(first.links.len(), 2);
        let link = first.links.iter().find(|link| link.id == "link-1").unwrap();
        assert_eq!(link.id, "link-1");
        assert_eq!(link.source_id.as_deref(), Some("attachment-1"));
        assert_eq!(link.filename, "plan.pdf");
        assert_eq!(link.media_type, "application/pdf");
        assert_eq!(link.bytes, Some(5));
        assert_eq!(link.source_state.as_deref(), Some("available"));
        assert_eq!(link.link_state, "draft");
        assert!(link.preview_available);
        assert_eq!(link.bound_at, None);

        let links = project_attachment_links(&mut tx, &owner, "project-1", "set-1", now())
            .await
            .unwrap();
        assert_eq!(links, first.links);

        let status = project_attachment_link_status(&mut tx, &owner, "project-1", "link-1", now())
            .await
            .unwrap();
        assert_eq!(status.link_id, "link-1");
        assert_eq!(status.set_id, "set-1");
        assert_eq!(status.source_id.as_deref(), Some("attachment-1"));
        assert_eq!(status.source_state.as_deref(), Some("available"));
        assert_eq!(status.link_state, "draft");
        assert!(status.preview_available);
        assert!(status.bindable);
        assert_eq!(status.bound_at, None);
        assert_eq!(status.withdrawn_at, None);
        assert_eq!(status.scan.attempts, 1);
        assert_eq!(status.scan.last_result.as_deref(), Some("clean"));
        assert_eq!(status.scan.signature.as_deref(), Some("sig-1"));
        assert_eq!(status.scan.definition_updated_at, Some(1));
        assert_eq!(status.scan.scanned_at, Some(2));

        let bound = project_attachment_link_status(&mut tx, &owner, "project-1", "link-bound", now())
            .await
            .unwrap();
        assert_eq!(bound.link_state, "bound");
        assert!(!bound.bindable);
        assert_eq!(bound.bound_at, Some(20));

        let variant: String = sqlx::query_scalar(
            "SELECT variant FROM attachment_access_events WHERE link_id='link-1' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(variant, "status");

        let other = actor(CompanyId::new("other-company").unwrap(), "owner");
        assert_eq!(
            code(project_attachment_sets(&mut tx, &other, "project-1", now())
                .await
                .unwrap_err()),
            "ATTACHMENT_TARGET_NOT_FOUND"
        );
        assert_eq!(
            code(project_attachment_links(&mut tx, &owner, "project-1", "set-private", now())
                .await
                .unwrap_err()),
            "ATTACHMENT_SET_NOT_FOUND"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn serves_checked_original_and_preview_bytes_with_access_events() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        let owner = actor(company(), "owner");
        let store = Store;

        let original = project_attachment_content(
            &mut tx,
            &store,
            &owner,
            "project-1",
            "link-1",
            ProjectAttachmentContentVariant::Original,
            now(),
        )
        .await
        .unwrap();
        assert_eq!(original.media_type, "application/pdf");
        assert_eq!(original.filename, "plan.pdf");
        assert_eq!(original.body, BODY);
        assert!(!original.inline);

        let preview = project_attachment_content(
            &mut tx,
            &store,
            &owner,
            "project-1",
            "link-1",
            ProjectAttachmentContentVariant::Preview,
            now(),
        )
        .await
        .unwrap();
        assert_eq!(preview.media_type, "image/png");
        assert_eq!(preview.filename, "plan.pdf");
        assert_eq!(preview.body, BODY);
        assert!(preview.inline);

        let variants: Vec<String> = sqlx::query_scalar(
            "SELECT variant FROM attachment_access_events WHERE link_id='link-1' ORDER BY created_at,variant",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(variants, ["original", "preview"]);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn content_fails_closed_for_scan_revocation_foreign_scope_and_oversize() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        let owner = actor(company(), "owner");
        let store = Store;

        assert_eq!(
            code(
                project_attachment_content(
                    &mut tx,
                    &store,
                    &owner,
                    "project-1",
                    "link-pending",
                    ProjectAttachmentContentVariant::Original,
                    now(),
                )
                .await
                .unwrap_err()
            ),
            "ATTACHMENT_NOT_AVAILABLE"
        );
        assert_eq!(
            code(
                project_attachment_content(
                    &mut tx,
                    &store,
                    &owner,
                    "project-1",
                    "link-revoked",
                    ProjectAttachmentContentVariant::Original,
                    now(),
                )
                .await
                .unwrap_err()
            ),
            "ATTACHMENT_NOT_AVAILABLE"
        );

        assert_eq!(
            code(
                project_attachment_content(
                    &mut tx,
                    &store,
                    &owner,
                    "project-1",
                    "link-missing",
                    ProjectAttachmentContentVariant::Original,
                    now(),
                )
                .await
                .unwrap_err()
            ),
            "ATTACHMENT_LINK_NOT_FOUND"
        );

        let other = actor(CompanyId::new("other-company").unwrap(), "owner");
        assert_eq!(
            code(
                project_attachment_content(
                    &mut tx,
                    &store,
                    &other,
                    "project-1",
                    "link-1",
                    ProjectAttachmentContentVariant::Original,
                    now(),
                )
                .await
                .unwrap_err()
            ),
            "ATTACHMENT_TARGET_NOT_FOUND"
        );

        assert_eq!(
            code(
                project_attachment_content(
                    &mut tx,
                    &store,
                    &owner,
                    "project-1",
                    "link-oversize",
                    ProjectAttachmentContentVariant::Original,
                    now(),
                )
                .await
                .unwrap_err()
            ),
            "ATTACHMENT_NOT_AVAILABLE"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn resolves_project_target_by_set_and_by_link_and_hides_other_targets() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        let owner = actor(company(), "owner");
        let store = Store;

        let by_set = project_attachment_links_by_set(&mut tx, &owner, "set-1", now())
            .await
            .unwrap();
        let by_project = project_attachment_links(&mut tx, &owner, "project-1", "set-1", now())
            .await
            .unwrap();
        assert_eq!(by_set, by_project);

        let status = project_attachment_link_status_by_link(&mut tx, &owner, "link-1", now())
            .await
            .unwrap();
        assert_eq!(status.link_id, "link-1");
        assert_eq!(status.set_id, "set-1");
        assert_eq!(status.scan.attempts, 1);
        assert!(status.preview_available);

        let original = project_attachment_content_by_link(
            &mut tx,
            &store,
            &owner,
            "link-1",
            ProjectAttachmentContentVariant::Original,
            now(),
        )
        .await
        .unwrap();
        assert_eq!(original.media_type, "application/pdf");
        assert_eq!(original.filename, "plan.pdf");
        assert_eq!(original.body, BODY);
        assert!(!original.inline);

        sqlx::raw_sql(
            "INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
             VALUES('set-other','company',NULL,NULL,'commitment','commitment-1','record',NULL,'project_document',3,1,'internal','[]',0,50,'draft','owner',60,60);\
             INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) \
             VALUES('link-other','set-other',NULL,'draft','owner',60,4102444800000,NULL,NULL,NULL);",
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        assert_eq!(
            code(project_attachment_links_by_set(&mut tx, &owner, "set-other", now())
                .await
                .unwrap_err()),
            "ATTACHMENT_SET_NOT_FOUND"
        );
        assert_eq!(
            code(project_attachment_links_by_set(&mut tx, &owner, "set-missing", now())
                .await
                .unwrap_err()),
            "ATTACHMENT_SET_NOT_FOUND"
        );
        assert_eq!(
            code(
                project_attachment_link_status_by_link(&mut tx, &owner, "link-other", now())
                    .await
                    .unwrap_err()
            ),
            "ATTACHMENT_LINK_NOT_FOUND"
        );
        assert_eq!(
            code(
                project_attachment_link_status_by_link(&mut tx, &owner, "link-missing", now())
                    .await
                    .unwrap_err()
            ),
            "ATTACHMENT_LINK_NOT_FOUND"
        );
        assert_eq!(
            code(
                project_attachment_content_by_link(
                    &mut tx,
                    &store,
                    &owner,
                    "link-other",
                    ProjectAttachmentContentVariant::Original,
                    now(),
                )
                .await
                .unwrap_err()
            ),
            "ATTACHMENT_LINK_NOT_FOUND"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn private_drafts_and_non_members_are_hidden() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        let owner = actor(company(), "owner");
        let another = actor(company(), "another-member");
        let sets = project_attachment_sets(&mut tx, &another, "project-1", now())
            .await
            .unwrap();
        let ids = sets.iter().map(|set| set.id.as_str()).collect::<Vec<_>>();
        assert!(ids.contains(&"set-private"));
        assert!(!ids.contains(&"set-1"));
        assert_eq!(
            code(project_attachment_link_status(
                &mut tx,
                &owner,
                "project-1",
                "link-private",
                now(),
            )
            .await
            .unwrap_err()),
            "ATTACHMENT_LINK_NOT_FOUND"
        );
        let outsider = actor(company(), "outsider");
        assert_eq!(
            code(
                project_attachment_sets(&mut tx, &outsider, "project-1", now())
                    .await
                    .unwrap_err()
            ),
            "ATTACHMENT_TARGET_NOT_FOUND"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
