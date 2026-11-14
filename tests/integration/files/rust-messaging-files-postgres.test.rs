//! Files-owned Messaging attachment port against PostgreSQL Files rows.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_collaboration::public::messaging_http::{
    MessagingFilesBindRequest, MessagingFilesContentRequest, MessagingFilesPort,
};
use erpx_server_collaboration::public::messaging_file_discard::{
    DiscardReceipt, MessagingFileDiscardFuture, MessagingFileDiscardPort,
    MessagingFileWithdrawRequest, discard_file,
};
use erpx_server_collaboration::public::messaging_native::MessagingActor;
use erpx_server_files::public::attachment_command::{
    AttachmentWithdrawAuthority, WithdrawAttachmentRequest, WithdrawTarget, withdraw_attachment,
};
use erpx_server_files::public::canonical_original::OriginalByteStore;
use erpx_server_files::public::messaging_files::{
    MessagingFilesActor, MessagingFilesAudience, MessagingFilesFuture, PgMessagingFiles,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PgWorkFuture, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;
use std::future::Future;
use std::sync::Arc;

const NOW: i64 = 1_800_000_000_000;
const HOST: &str = "acme.example.test";
const SLUG: &str = "acme";

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

struct Audience {
    member: bool,
}

impl MessagingFilesAudience for Audience {
    fn current<'a>(
        &'a self,
        _connection: &'a mut PgConnection,
        host: &'a str,
        holding_slug: &'a str,
        _headers: &'a HeaderMap,
    ) -> MessagingFilesFuture<'a, MessagingFilesActor> {
        Box::pin(async move {
            if host != HOST || holding_slug != SLUG {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "MESSAGING_FILE_NOT_FOUND",
                    "The conversation file is unavailable.",
                ));
            }
            Ok(MessagingFilesActor {
                company_id: company(),
                user_id: "owner".into(),
            })
        })
    }

    fn require_room<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        actor: &'a MessagingFilesActor,
        room_id: &'a str,
    ) -> MessagingFilesFuture<'a, ()> {
        Box::pin(async move {
            if !self.member {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "MESSAGING_FILE_NOT_FOUND",
                    "The conversation file is unavailable.",
                ));
            }
            let member: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM messaging_room_members WHERE room_id=$1 AND user_id=$2)",
            )
            .bind(room_id)
            .bind(&actor.user_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "MESSAGING_FILES_UNAVAILABLE",
                    "The conversation file is unavailable.",
                )
            })?;
            if !member {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "MESSAGING_FILE_NOT_FOUND",
                    "The conversation file is unavailable.",
                ));
            }
            Ok(())
        })
    }
}

struct Store;

impl OriginalByteStore for Store {
    fn read_verified(
        &self,
        key: &str,
        size: usize,
        checksum: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<u8>, BusinessError>> + Send {
        let key = key.to_owned();
        let checksum = checksum.to_owned();
        async move {
            if !(key.starts_with("original-") || key.starts_with("preview-"))
                || size != 5
                || limit < 5
            {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "FILE_ORIGINAL_STORAGE",
                    "The verified private file is unavailable.",
                ));
            }
            if checksum != "a".repeat(64) {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "FILE_ORIGINAL_STORAGE",
                    "The verified private file is unavailable.",
                ));
            }
            Ok(b"hello".to_vec())
        }
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
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
         INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,version,last_seq,created_at) \
         VALUES('room-1','company','group','Team','owner',5,1,0);\
         INSERT INTO messaging_room_members(room_id,user_id) VALUES('room-1','owner');\
         INSERT INTO messaging_messages(id,room_id,seq,sender_id,sender_name,body,created_at) \
         VALUES('message-1','room-1',1,'owner','Owner','Hello',1);\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) \
         VALUES('upload-1','messaging_room','room-1','message_attachment','plan.pdf','application/pdf',5,'staging-1','ready','owner',4102444800000,0,0),\
         ('upload-2','messaging_room','room-1','message_attachment','pending.pdf','application/pdf',5,'staging-2','ready','owner',4102444800000,0,0);\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at,preview_object_key,preview_bytes,preview_checksum_sha256,preview_media_type) \
         VALUES('attachment-1','upload-1','messaging_room','room-1','message_attachment','original-1',1,'plan.pdf','application/pdf',5,repeat('a',64),'owner',0,'company','application/pdf','available',1,1,'preview-1',5,repeat('a',64),'image/png'),\
         ('attachment-2','upload-2','messaging_room','room-1','message_attachment','original-2',1,'pending.pdf','application/pdf',5,repeat('b',64),'owner',0,'company','application/pdf','quarantined',NULL,NULL,NULL,NULL,NULL,NULL);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-1','company',NULL,NULL,'messaging_room','room-1','message','file-1','message_attachment',5,1,'confidential','[]',0,10,'draft','owner',0,0),\
         ('set-pending','company',NULL,NULL,'messaging_room','room-1','message','file-pending','message_attachment',5,1,'confidential','[]',0,10,'draft','owner',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-1','set-1','attachment-1','draft','owner',0,4102444800000),\
         ('link-pending','set-pending','attachment-2','draft','owner',0,4102444800000);\
         INSERT INTO messaging_files(id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,attachment_set_id,attachment_link_id,attachment_source_id) \
         VALUES('file-1','room-1','owner','plan.pdf','application/pdf',5,'original-1',repeat('a',64),0,'set-1','link-1','attachment-1'),\
         ('file-pending','room-1','owner','pending.pdf','application/pdf',5,'original-2',repeat('b',64),0,'set-pending','link-pending','attachment-2');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn port(pool: &DatabasePool, member: bool) -> PgMessagingFiles<Audience, Store> {
    PgMessagingFiles::new(
        pool.clone(),
        Arc::new(Audience { member }),
        Arc::new(Store),
        Arc::new(FixedClock),
    )
}

#[tokio::test]
async fn binds_scanned_message_files_and_serves_bounded_original_content() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let files: Arc<dyn MessagingFilesPort> = Arc::new(port(&pool, true));
        let ids = vec!["set-1".to_owned()];
        let headers = HeaderMap::new();
        let binding = files
            .bind(MessagingFilesBindRequest {
                host: HOST,
                holding_slug: SLUG,
                headers: &headers,
                room_id: "room-1",
                attachment_set_ids: &ids,
                message_id: Some("message-1"),
            })
            .await
            .unwrap();
        assert_eq!(binding.id, "message-1");
        assert_eq!(binding.version, 5);
        assert_eq!(binding.status, "bound");
        let state: (String, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT s.state,l.state,l.binding_type,l.binding_id FROM attachment_sets s \
             JOIN attachment_links l ON l.set_id=s.id WHERE s.id='set-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            state,
            (
                "bound".into(),
                "bound".into(),
                Some("message_attachment".into()),
                Some("message-1".into())
            )
        );
        let message_id: Option<String> =
            sqlx::query_scalar("SELECT message_id FROM messaging_files WHERE id='file-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(message_id.as_deref(), Some("message-1"));

        let content = files
            .content(MessagingFilesContentRequest {
                host: HOST,
                holding_slug: SLUG,
                headers: &headers,
                file_id: "file-1",
                variant: "original",
            })
            .await
            .unwrap();
        assert_eq!(content.media_type, "application/pdf");
        assert_eq!(content.filename, "plan.pdf");
        assert_eq!(content.body, b"hello");
        let preview = files
            .content(MessagingFilesContentRequest {
                host: HOST,
                holding_slug: SLUG,
                headers: &headers,
                file_id: "file-1",
                variant: "preview",
            })
            .await
            .unwrap();
        assert_eq!(preview.media_type, "image/png");
        assert_eq!(preview.body, b"hello");
        let audit: (String, String) = sqlx::query_as(
            "SELECT variant,outcome FROM attachment_access_events WHERE link_id='link-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, ("original".into(), "allowed".into()));
    })
    .await;
}

#[tokio::test]
async fn unscanned_files_and_revoked_membership_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let headers = HeaderMap::new();
        let files = port(&pool, true);
        let pending = vec!["set-pending".to_owned()];
        let error = files
            .bind(MessagingFilesBindRequest {
                host: HOST,
                holding_slug: SLUG,
                headers: &headers,
                room_id: "room-1",
                attachment_set_ids: &pending,
                message_id: Some("message-1"),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, "MESSAGING_ATTACHMENT_SCAN_REQUIRED");

        let revoked = port(&pool, false);
        let ids = vec!["set-1".to_owned()];
        let error = revoked
            .bind(MessagingFilesBindRequest {
                host: HOST,
                holding_slug: SLUG,
                headers: &headers,
                room_id: "room-1",
                attachment_set_ids: &ids,
                message_id: Some("message-1"),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, "MESSAGING_FILE_NOT_FOUND");
        let state: String =
            sqlx::query_scalar("SELECT state FROM attachment_sets WHERE id='set-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(state, "draft");
    })
    .await;
}

#[tokio::test]
async fn content_requires_current_membership_and_a_clean_bound_link() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let headers = HeaderMap::new();
        let files = port(&pool, true);
        let ids = vec!["set-1".to_owned()];
        files
            .bind(MessagingFilesBindRequest {
                host: HOST,
                holding_slug: SLUG,
                headers: &headers,
                room_id: "room-1",
                attachment_set_ids: &ids,
                message_id: Some("message-1"),
            })
            .await
            .unwrap();
        let revoked = port(&pool, false);
        let error = revoked
            .content(MessagingFilesContentRequest {
                host: HOST,
                holding_slug: SLUG,
                headers: &headers,
                file_id: "file-1",
                variant: "original",
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, "MESSAGING_FILE_NOT_FOUND");
    })
    .await;
}

struct TestWithdrawAuthority {
    company: CompanyId,
}

impl AttachmentWithdrawAuthority for TestWithdrawAuthority {
    fn authorize_withdraw<'c>(
        &'c self,
        _connection: &'c mut PgConnection,
        target: &'c WithdrawTarget<'c>,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if target.company_id != self.company.as_str()
                || target.target_type != "messaging_room"
            {
                return Err(PgTransactionError::Business(BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Synthetic authority denied the withdraw.",
                )));
            }
            Ok(())
        })
    }
}

struct TestDiscardPort {
    authority: TestWithdrawAuthority,
}

impl MessagingFileDiscardPort for TestDiscardPort {
    fn withdraw<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        request: MessagingFileWithdrawRequest<'a>,
    ) -> MessagingFileDiscardFuture<'a, ()> {
        Box::pin(async move {
            withdraw_attachment(
                connection,
                &self.authority,
                WithdrawAttachmentRequest {
                    actor_id: request.actor_id,
                    company_id: request.company_id,
                    link_id: request.link_id,
                    reason: request.reason,
                    command_key: request.command_key,
                    now: request.now,
                },
            )
            .await
            .map(|_| ())
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => BusinessError::new(
                    FailureKind::Unavailable,
                    "FILES_UNAVAILABLE",
                    "The Files owner transaction is unavailable.",
                ),
            })
        })
    }
}

fn discard_actor(user: &str) -> MessagingActor {
    MessagingActor {
        user_id: user.into(),
        display_name: user.into(),
        company_id: "company".into(),
    }
}

fn discard_code(error: PgTransactionError<BusinessError>) -> String {
    match error {
        PgTransactionError::Business(error) => error.code.to_owned(),
        PgTransactionError::Persistence(_) => "PERSISTENCE".into(),
    }
}

async fn discard(
    pool: &DatabasePool,
    actor: &MessagingActor,
    file_id: &str,
    reason: &str,
    key: &str,
) -> Result<DiscardReceipt, PgTransactionError<BusinessError>> {
    let unit = PgUnitOfWork::new(pool.clone());
    let port = TestDiscardPort {
        authority: TestWithdrawAuthority {
            company: company(),
        },
    };
    let actor = actor.clone();
    let file_id = file_id.to_owned();
    let reason = reason.to_owned();
    let key = key.to_owned();
    unit.run_once(TransactionOptions::default(), move |connection| {
        Box::pin(async move {
            discard_file(
                connection,
                &actor,
                "assignment-1",
                &key,
                &file_id,
                &reason,
                NOW,
                &port,
            )
            .await
        })
    })
    .await
}

#[tokio::test]
async fn discards_an_unsent_message_file_with_the_owner_receipt_and_files_audit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor = discard_actor("owner");
        let receipt = discard(
            &pool,
            &actor,
            "file-1",
            "Unsent duplicate evidence",
            "messaging-discard-0001",
        )
        .await
        .unwrap();
        assert_eq!(receipt.id, "file-1");
        assert_eq!(receipt.state, "withdrawn");

        let link: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT state,withdrawn_by,withdrawal_reason FROM attachment_links WHERE id='link-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            link,
            (
                "withdrawn".into(),
                Some("owner".into()),
                Some("Unsent duplicate evidence".into())
            )
        );
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attachment_events WHERE link_id='link-1' \
             AND action='link.withdrawn' AND actor_id='owner'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(events, 1);
        let (operation, record_id, result): (String, String, String) = sqlx::query_as(
            "SELECT operation,record_id,result_json FROM command_receipts \
             WHERE command_id='messaging-discard-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(operation, "messaging.file-discard");
        assert_eq!(record_id, "room-1");
        assert_eq!(result, r#"{"id":"file-1","state":"withdrawn"}"#);
        let audit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id='messaging-discard-0001' \
             AND action='messaging.file-discard' AND assignment_id='assignment-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, 1);
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id='messaging-discard-0001' \
             AND event_type='messaging.file-discard'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 1);
        let retained: String =
            sqlx::query_scalar("SELECT attachment_link_id FROM messaging_files WHERE id='file-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(retained, "link-1");
    })
    .await;
}

#[tokio::test]
async fn replays_the_identical_receipt_and_rejects_a_changed_payload() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let actor = discard_actor("owner");
        let reason = "Unsent duplicate evidence";
        let first = discard(&pool, &actor, "file-1", reason, "messaging-discard-0002")
            .await
            .unwrap();
        let replay = discard(&pool, &actor, "file-1", reason, "messaging-discard-0002")
            .await
            .unwrap();
        assert_eq!(first, replay);
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attachment_events WHERE link_id='link-1' \
             AND action='link.withdrawn'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(events, 1);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='messaging-discard-0002'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 1);

        let error = discard(
            &pool,
            &actor,
            "file-1",
            "A different discard reason",
            "messaging-discard-0002",
        )
        .await
        .unwrap_err();
        assert_eq!(discard_code(error), "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn requires_current_room_membership_and_an_unbound_own_file() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let stranger = discard_actor("stranger");
        let error = discard(
            &pool,
            &stranger,
            "file-1",
            "Unsent duplicate evidence",
            "messaging-discard-0003",
        )
        .await
        .unwrap_err();
        assert_eq!(discard_code(error), "MESSAGING_ROOM_FORBIDDEN");
        let state: String =
            sqlx::query_scalar("SELECT state FROM attachment_links WHERE id='link-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(state, "draft");

        sqlx::query("UPDATE messaging_files SET message_id='message-1' WHERE id='file-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let owner = discard_actor("owner");
        let error = discard(
            &pool,
            &owner,
            "file-1",
            "Unsent duplicate evidence",
            "messaging-discard-0004",
        )
        .await
        .unwrap_err();
        assert_eq!(discard_code(error), "MESSAGE_FILE_ALREADY_BOUND");
        let state: String =
            sqlx::query_scalar("SELECT state FROM attachment_links WHERE id='link-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(state, "draft");
    })
    .await;
}
