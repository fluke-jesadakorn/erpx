//! Files-owned Social draft upload cleanup against actual Holding PostgreSQL
//! rows. Collaboration owns the canceled draft; this target proves the two
//! Files effects apply only to that draft's live attachments and ready uploads.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_files::public::social_draft_uploads::{
    SocialDraftUploadRevokeRequest, revoke_social_draft_uploads,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;

const NOW: i64 = 1_700_000_000_000;

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
        "INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,
           declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES
           ('u-ready-a','social_draft','draft-a','post_image','a.png','image/png',1,'staging-u-ready-a','ready','author',1,1,1),
           ('u-pending-a','social_draft','draft-a','post_image','b.png','image/png',1,'staging-u-pending-a','pending','author',1,1,1),
           ('u-ready-b','social_draft','draft-b','post_image','c.png','image/png',1,'staging-u-ready-b','ready','author',1,1,1),
           ('u-ready-post','social_post','post-a','post_image','d.png','image/png',1,'staging-u-ready-post','ready','author',1,1,1);
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,
           original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES
           ('a-live-a','u-ready-a','social_draft','draft-a','post_image','object-a-live-a',1,'a.png','image/png',1,'checksum-a-live-a','author',1),
           ('a-pending-a','u-pending-a','social_draft','draft-a','post_image','object-a-pending-a',1,'b.png','image/png',1,'checksum-a-pending-a','author',1),
           ('a-live-b','u-ready-b','social_draft','draft-b','post_image','object-a-live-b',1,'c.png','image/png',1,'checksum-a-live-b','author',1),
           ('a-post','u-ready-post','social_post','post-a','post_image','object-a-post',1,'d.png','image/png',1,'checksum-a-post','author',1);
         UPDATE attachments SET revoked_at=5 WHERE id='a-pending-a';",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn revokes_and_expires_only_the_canceled_draft() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        revoke_social_draft_uploads(
            &mut connection,
            SocialDraftUploadRevokeRequest {
                draft_id: "draft-a",
                now: NOW,
            },
        )
        .await
        .unwrap();

        let attachments: (Option<i64>, Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT (SELECT revoked_at FROM attachments WHERE id='a-live-a'),
                    (SELECT revoked_at FROM attachments WHERE id='a-pending-a'),
                    (SELECT revoked_at FROM attachments WHERE id='a-live-b'),
                    (SELECT revoked_at FROM attachments WHERE id='a-post')",
        )
        .fetch_one(&mut *connection)
        .await
        .unwrap();
        assert_eq!(
            attachments,
            (Some(NOW), Some(5), None, None),
            "only the canceled draft's live attachment is revoked"
        );

        let uploads: (String, String, String, String) = sqlx::query_as(
            "SELECT (SELECT status FROM upload_intents WHERE id='u-ready-a'),
                    (SELECT status FROM upload_intents WHERE id='u-pending-a'),
                    (SELECT status FROM upload_intents WHERE id='u-ready-b'),
                    (SELECT status FROM upload_intents WHERE id='u-ready-post')",
        )
        .fetch_one(&mut *connection)
        .await
        .unwrap();
        assert_eq!(
            uploads,
            (
                "expired".into(),
                "pending".into(),
                "ready".into(),
                "ready".into()
            ),
            "only the canceled draft's ready upload intent expires"
        );
        let updated_at: i64 =
            sqlx::query_scalar("SELECT updated_at FROM upload_intents WHERE id='u-ready-a'")
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(updated_at, NOW);
    })
    .await;
}

#[tokio::test]
async fn empty_draft_identity_makes_no_change() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = revoke_social_draft_uploads(
            &mut connection,
            SocialDraftUploadRevokeRequest {
                draft_id: "",
                now: NOW,
            },
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(BusinessError { code, .. }) => {
                assert_eq!(code, "SOCIAL_DRAFT_ID_REQUIRED")
            }
            _ => panic!("empty draft identity was persisted"),
        }
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM attachments WHERE revoked_at=$1),
                    (SELECT count(*) FROM upload_intents WHERE status='expired')",
        )
        .bind(NOW)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
        assert_eq!(counts, (0, 0));
    })
    .await;
}
