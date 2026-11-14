//! Files-owned Projects attachment intake against PostgreSQL.
//!
//! Covers the native draft set, reservation, byte completion and the scan
//! receipt gate. Files owns the set/link/source ledger, the private quarantine
//! object and the pending native scan attempt; Projects owns the current
//! project version and writer authority.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_files::public::field_visit_upload::LocalFieldQuarantineWriter;
use erpx_server_files::public::project_attachment_intake::{
    ProjectAttachmentFileRequest, ProjectAttachmentIntakeAvailability,
    ProjectAttachmentSetRequest, complete_project_attachment_upload,
    create_project_attachment_set, project_attachment_upload_status,
    reserve_project_attachment_file,
};
use erpx_server_files::public::project_attachment::{
    ProjectAttachmentScanRead, bind_project_attachment_with_current_facts,
};
use erpx_server_projects::public::project_attachment::{
    ProjectAttachmentActor, ProjectAttachmentBindingCommand,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};

const NOW: i64 = 1_800_000_000_000;
const HOLDING: &str = "h001";
const PDF: &[u8] = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\ntrailer\n<<>>\n%%EOF";

fn availability() -> ProjectAttachmentIntakeAvailability {
    ProjectAttachmentIntakeAvailability {
        uploads_frozen: false,
        scanner_ready: true,
        preview_ready: true,
    }
}

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

fn set_request(version: i64) -> ProjectAttachmentSetRequest {
    ProjectAttachmentSetRequest {
        target_type: "project".into(),
        target_id: "project-1".into(),
        component_kind: "record".into(),
        component_ref: Some(" attachments ".into()),
        purpose_key: "project_document".into(),
        expected_target_version: version,
    }
}

fn file_request(bytes: i64) -> ProjectAttachmentFileRequest {
    ProjectAttachmentFileRequest {
        filename: " plan.pdf ".into(),
        media_type: "application/pdf".into(),
        bytes,
        caption: Some(" Site plan ".into()),
        supersedes_link_id: None,
    }
}

async fn serial(connection: &mut sqlx::PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(connection)
        .await
        .unwrap();
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
         VALUES('project-1','company','Project','Project','owner',3,0,0,'active'),\
         ('project-frozen','company','Frozen','Frozen','owner',3,0,0,'completed');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn create_and_reserve(
    connection: &mut sqlx::PgConnection,
    key: &str,
) -> (String, String, i64) {
    let set = create_project_attachment_set(
        connection,
        &actor("owner"),
        &set_request(3),
        key,
        availability(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(set.state, "draft");
    assert_eq!(set.policy_version, 1);
    assert_eq!(set.max_files, 50);
    assert_eq!(set.component_ref.as_deref(), Some("attachments"));
    let reservation = reserve_project_attachment_file(
        connection,
        &actor("owner"),
        HOLDING,
        &set.id,
        &file_request(PDF.len() as i64),
        key,
        availability(),
        None,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(reservation.replay, false);
    (set.id, reservation.upload_intent_id, reservation.expires_at)
}

#[tokio::test]
async fn stages_a_project_file_and_gates_availability_on_the_scan_receipt() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let writer = LocalFieldQuarantineWriter::open(directory.path(), 2).unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let (set_id, upload_id, _) = create_and_reserve(&mut tx, "project-intake-key-0001").await;

        let completion = complete_project_attachment_upload(
            &mut tx,
            &writer,
            &actor("owner"),
            &upload_id,
            "application/pdf",
            PDF.to_vec(),
            availability(),
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(completion.attachment_set_id, set_id);
        assert_eq!(completion.state, "quarantined");
        assert_eq!(completion.status, "scan_pending");
        assert_eq!(
            completion.scan,
            ProjectAttachmentScanRead {
                attempts: 1,
                last_result: Some("pending".into()),
                last_error: None,
                signature: None,
                definition_updated_at: None,
                scanned_at: None,
            }
        );

        let pending = project_attachment_upload_status(
            &mut tx,
            &actor("owner"),
            &upload_id,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(pending.status, "scan_pending");
        assert!(!pending.bindable);
        assert_eq!(pending.source_state.as_deref(), Some("quarantined"));

        // An available source without the exact clean attempt completion is not
        // a valid receipt and must never surface as bindable.
        sqlx::query("UPDATE attachments SET source_state='scanning' WHERE id=$1")
            .bind(format!("attachment-{upload_id}"))
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE attachments SET source_state='available',scanned_at=100,scanner_definition_at=99 \
             WHERE id=$1",
        )
        .bind(format!("attachment-{upload_id}"))
        .execute(&mut *tx)
        .await
        .unwrap();
        let invalid = project_attachment_upload_status(
            &mut tx,
            &actor("owner"),
            &upload_id,
            NOW,
        )
        .await;
        assert_eq!(
            business(invalid.unwrap_err()).code,
            "ATTACHMENT_SCAN_RECEIPT_INVALID"
        );

        // The exact native receipt (clean attempt completed at scanned_at with
        // the recorded definition) unlocks bindability.
        sqlx::query(
            "UPDATE attachment_scan_attempts SET state='clean',scanner_definition_at=99,\
             completed_at=100,result_code='OK' WHERE attachment_id=$1 AND attempt=1",
        )
        .bind(format!("attachment-{upload_id}"))
        .execute(&mut *tx)
        .await
        .unwrap();
        let available = project_attachment_upload_status(
            &mut tx,
            &actor("owner"),
            &upload_id,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(available.status, "available");
        assert!(available.bindable);
        assert_eq!(available.scanned_at, Some(100));

        let binding = bind_project_attachment_with_current_facts(
            &mut tx,
            &actor("owner"),
            &ProjectAttachmentBindingCommand {
                target_type: "project".into(),
                target_id: "project-1".into(),
                expected_version: 3,
                purpose: "new-work".into(),
                link_ids: vec![completion.attachment_link_id.clone()],
                caption: None,
            },
            TimestampMillis::new(NOW),
        )
        .await
        .unwrap();
        assert_eq!(binding.target_version, 4);
        tx.commit().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn rejects_foreign_authority_oversize_and_cross_key_reuse() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;

        // Not the project owner.
        let foreign = create_project_attachment_set(
            &mut tx,
            &actor("other"),
            &set_request(3),
            "project-intake-foreign-0001",
            availability(),
            NOW,
        )
        .await;
        assert_eq!(
            business(foreign.unwrap_err()).code,
            "ATTACHMENT_PROJECT_OWNER_REQUIRED"
        );

        // A frozen project never stages new documents.
        let frozen = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &ProjectAttachmentSetRequest {
                target_id: "project-frozen".into(),
                ..set_request(3)
            },
            "project-intake-frozen-0001",
            availability(),
            NOW,
        )
        .await;
        assert_eq!(business(frozen.unwrap_err()).code, "PROJECT_STRUCTURE_FROZEN");

        // An old expected version is a conflict, not a silent retry.
        let stale = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &set_request(2),
            "project-intake-stale-0001",
            availability(),
            NOW,
        )
        .await;
        assert_eq!(
            business(stale.unwrap_err()).code,
            "ATTACHMENT_TARGET_CHANGED"
        );

        let set = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &set_request(3),
            "project-intake-key-0002",
            availability(),
            NOW,
        )
        .await
        .unwrap();

        // Oversize reservations are refused before an intent exists.
        let oversize = reserve_project_attachment_file(
            &mut tx,
            &actor("owner"),
            HOLDING,
            &set.id,
            &file_request(10 * 1024 * 1024 + 1),
            "project-intake-key-0002",
            availability(),
            None,
            NOW,
        )
        .await;
        assert_eq!(business(oversize.unwrap_err()).code, "ATTACHMENT_SIZE");

        // Unknown purpose for the closed project policy.
        let unknown = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &ProjectAttachmentSetRequest {
                purpose_key: "payment_slip".into(),
                ..set_request(3)
            },
            "project-intake-unknown-0001",
            availability(),
            NOW,
        )
        .await;
        assert_eq!(
            business(unknown.unwrap_err()).code,
            "ATTACHMENT_POLICY_UNKNOWN"
        );

        // Frozen upload intake availability is a fail-closed service error.
        let frozen_intake = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &set_request(3),
            "project-intake-frozen-service-0001",
            ProjectAttachmentIntakeAvailability {
                uploads_frozen: true,
                scanner_ready: true,
                preview_ready: true,
            },
            NOW,
        )
        .await;
        assert_eq!(
            business(frozen_intake.unwrap_err()).code,
            "ATTACHMENT_UPLOAD_FREEZE"
        );

        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn replays_set_reservation_and_completion_without_duplicate_effects() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let writer = LocalFieldQuarantineWriter::open(directory.path(), 2).unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;

        let first = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &set_request(3),
            "project-intake-replay-0001",
            availability(),
            NOW,
        )
        .await
        .unwrap();
        let replay = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &set_request(3),
            "project-intake-replay-0001",
            availability(),
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(first.id, replay.id);
        let conflict = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &ProjectAttachmentSetRequest {
                purpose_key: "deliverable".into(),
                ..set_request(3)
            },
            "project-intake-replay-0001",
            availability(),
            NOW,
        )
        .await;
        assert_eq!(
            business(conflict.unwrap_err()).code,
            "IDEMPOTENCY_CONFLICT"
        );

        // A different key for the same exact slot reuses the draft set.
        let reused = create_project_attachment_set(
            &mut tx,
            &actor("owner"),
            &set_request(3),
            "project-intake-replay-0002",
            availability(),
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(first.id, reused.id);

        let request = file_request(PDF.len() as i64);
        let reserved = reserve_project_attachment_file(
            &mut tx,
            &actor("owner"),
            HOLDING,
            &first.id,
            &request,
            "project-intake-replay-0003",
            availability(),
            None,
            NOW,
        )
        .await
        .unwrap();
        let reserved_again = reserve_project_attachment_file(
            &mut tx,
            &actor("owner"),
            HOLDING,
            &first.id,
            &request,
            "project-intake-replay-0003",
            availability(),
            None,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(reserved.upload_intent_id, reserved_again.upload_intent_id);
        assert_eq!(reserved.attachment_link_id, reserved_again.attachment_link_id);
        assert_eq!(reserved.expires_at, reserved_again.expires_at);
        assert!(!reserved.replay);
        assert!(reserved_again.replay);
        let reserve_conflict = reserve_project_attachment_file(
            &mut tx,
            &actor("owner"),
            HOLDING,
            &first.id,
            &ProjectAttachmentFileRequest {
                filename: "other.pdf".into(),
                ..request.clone()
            },
            "project-intake-replay-0003",
            availability(),
            None,
            NOW,
        )
        .await;
        assert_eq!(
            business(reserve_conflict.unwrap_err()).code,
            "IDEMPOTENCY_CONFLICT"
        );

        let completion = complete_project_attachment_upload(
            &mut tx,
            &writer,
            &actor("owner"),
            &reserved.upload_intent_id,
            "application/pdf",
            PDF.to_vec(),
            availability(),
            NOW,
        )
        .await
        .unwrap();
        let completion_again = complete_project_attachment_upload(
            &mut tx,
            &writer,
            &actor("owner"),
            &reserved.upload_intent_id,
            "application/pdf",
            PDF.to_vec(),
            availability(),
            NOW,
        )
        .await
        .unwrap();
        assert!(completion_again.replay);
        assert_eq!(completion.source_id, completion_again.source_id);
        assert_eq!(
            business(
                complete_project_attachment_upload(
                    &mut tx,
                    &writer,
                    &actor("owner"),
                    &reserved.upload_intent_id,
                    "application/pdf",
                    PDF.iter()
                        .map(|byte| if *byte == b'4' { b'5' } else { *byte })
                        .collect(),
                    availability(),
                    NOW,
                )
                .await
                .unwrap_err()
            )
            .code,
            "UPLOAD_CONFLICT"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
