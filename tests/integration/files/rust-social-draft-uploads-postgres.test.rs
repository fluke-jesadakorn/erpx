//! Files-owned Social draft images and cleanup against actual Holding
//! PostgreSQL. Collaboration supplies the current draft or post identity.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::social_upload_files::lock_social_comment_draft;
use erpx_files_server::public::social_draft_uploads::{
    SocialCommentFilesRequest, SocialDraftUploadRevokeRequest, read_social_home_images,
    retain_social_comment_images, revoke_social_draft_uploads,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;

const NOW: i64 = 1_700_000_000_000;

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO social_posts(id,author_id,author_name,company_id,body,created_at,updated_at) VALUES ('post-a','author','Author','company-a','Post',1,1);
         INSERT INTO social_post_drafts(id,author_id,company_id,body,created_at,updated_at,comment_parent_post_id) VALUES ('draft-a','author','company-a','',1,1,'post-a');
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,
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
async fn home_image_projection_uses_published_drafts_and_saved_edits_only() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "UPDATE social_post_drafts SET published_post_id='post-a',comment_parent_post_id=NULL WHERE id='draft-a';
             INSERT INTO social_post_drafts(id,author_id,company_id,body,created_at,updated_at,edit_post_id,edit_post_version,edit_saved_version)
               VALUES ('draft-b','author','company-b','Other company',1,1,'post-a',3,4);
             INSERT INTO social_post_drafts(id,author_id,company_id,body,created_at,updated_at,edit_post_id,edit_post_version,edit_saved_version)
               VALUES ('edit-saved','author','company-a','Saved edit',2,2,'post-a',1,2),
                      ('edit-open','author','company-a','Unpublished edit',2,2,'post-a',2,NULL);
             INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,
               declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES
               ('u-saved','social_draft','edit-saved','post_image','edit.png','image/png',1,'staging-edit-saved','ready','author',2,2,2),
               ('u-open','social_draft','edit-open','post_image','open.png','image/png',1,'staging-edit-open','ready','author',2,2,2);
             INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,
               original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES
               ('a-saved','u-saved','social_draft','edit-saved','post_image','object-a-saved',1,'edit.png','image/png',1,'checksum-a-saved','author',2),
               ('a-open','u-open','social_draft','edit-open','post_image','object-a-open',1,'open.png','image/png',1,'checksum-a-open','author',2);",
        )
        .execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx).await.unwrap();
        let images = read_social_home_images(&mut tx, "company-a", "post-a").await.unwrap();
        assert_eq!(images.iter().map(|image| image.id.as_str()).collect::<Vec<_>>(), ["a-live-a", "a-saved"]);
        assert_eq!(images[1].original_name, "edit.png");
        assert!(images.iter().all(|image| image.post_id == "post-a"));
        assert!(read_social_home_images(&mut tx, "company-b", "post-a").await.unwrap().is_empty());
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn selected_comment_images_are_retained_and_other_draft_files_revoked_atomically() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES ('u-extra','social_draft','draft-a','post_image','extra.png','image/png',1,'staging-extra','ready','author',1,1,1); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES ('a-extra','u-extra','social_draft','draft-a','post_image','object-extra',1,'extra.png','image/png',1,'checksum-extra','author',1)")
            .execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let witness = lock_social_comment_draft(&mut tx, "draft-a", "company-a", "author", "post-a").await.unwrap();
        let selected = vec!["a-live-a".to_owned()];
        retain_social_comment_images(&mut tx, SocialCommentFilesRequest { draft: &witness, selected_attachment_ids: &selected, now: NOW }).await.unwrap();
        let rows: (Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as("SELECT (SELECT revoked_at FROM attachments WHERE id='a-live-a'),(SELECT revoked_at FROM attachments WHERE id='a-extra'),(SELECT revoked_at FROM attachments WHERE id='a-live-b')").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(rows, (None, Some(NOW), None));
        tx.rollback().await.unwrap();
        let revoked: Option<i64> = sqlx::query_scalar("SELECT revoked_at FROM attachments WHERE id='a-extra'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(revoked, None, "caller rollback also rolls back Files effects");
    }).await;
}

#[tokio::test]
async fn comment_image_validation_rejects_foreign_duplicate_and_revoked_files() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        for selected in [
            vec!["a-live-b".to_owned()],
            vec!["a-live-a".to_owned(), "a-live-a".to_owned()],
            vec!["a-pending-a".to_owned()],
        ] {
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *tx)
                .await
                .unwrap();
            let witness =
                lock_social_comment_draft(&mut tx, "draft-a", "company-a", "author", "post-a")
                    .await
                    .unwrap();
            assert!(
                retain_social_comment_images(
                    &mut tx,
                    SocialCommentFilesRequest {
                        draft: &witness,
                        selected_attachment_ids: &selected,
                        now: NOW
                    }
                )
                .await
                .is_err()
            );
            tx.rollback().await.unwrap();
        }
        let revoked: Option<i64> =
            sqlx::query_scalar("SELECT revoked_at FROM attachments WHERE id='a-live-a'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(revoked, None);
    })
    .await;
}

#[tokio::test]
async fn draft_witness_rejects_wrong_actor_and_a_different_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut owner = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *owner)
            .await
            .unwrap();
        assert!(
            lock_social_comment_draft(&mut owner, "draft-a", "company-a", "other-author", "post-a")
                .await
                .is_err()
        );
        assert!(
            lock_social_comment_draft(&mut owner, "draft-a", "other-company", "author", "post-a")
                .await
                .is_err()
        );
        assert!(
            lock_social_comment_draft(&mut owner, "draft-a", "company-a", "author", "other-post")
                .await
                .is_err()
        );
        let witness =
            lock_social_comment_draft(&mut owner, "draft-a", "company-a", "author", "post-a")
                .await
                .unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *other)
            .await
            .unwrap();
        let ids = vec!["a-live-a".to_owned()];
        assert!(
            retain_social_comment_images(
                &mut other,
                SocialCommentFilesRequest {
                    draft: &witness,
                    selected_attachment_ids: &ids,
                    now: NOW
                }
            )
            .await
            .is_err()
        );
        other.rollback().await.unwrap();
        sqlx::query("UPDATE social_post_drafts SET version=version+1 WHERE id='draft-a'")
            .execute(&mut *owner)
            .await
            .unwrap();
        assert!(
            retain_social_comment_images(
                &mut owner,
                SocialCommentFilesRequest {
                    draft: &witness,
                    selected_attachment_ids: &ids,
                    now: NOW,
                },
            )
            .await
            .is_err()
        );
        owner.rollback().await.unwrap();
    })
    .await;
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
