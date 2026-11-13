//! Actual Holding PostgreSQL Social rows with an explicit owner receipt/audit/
//! outbox harness. The authority/People/Records tables below are synthetic
//! composition ports, not production IAM or cross-owner evidence.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::social::*;
use erpx_server_collaboration::public::social_upload_files::{
    SocialUploadFilesFuture, SocialUploadFilesPort,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{PgConnection, Row};

const NOW: i64 = 1_700_000_000_000;
type Error = PgTransactionError<BusinessError>;

fn operation(change: &Change) -> &'static str {
    match change {
        Change::PublishPost { .. } => "social.post.publish",
        Change::PublishComment { .. } => "social.comment.publish",
        Change::EditComment { .. } => "social.comment.edit",
        Change::TombstoneComment { .. } => "social.comment.remove",
        Change::SetReaction { .. } => "social.post.appreciate",
        Change::SetFollow {
            target: FollowTarget::Post,
            ..
        } => "social.post.follow",
        Change::SetFollow {
            target: FollowTarget::Topic,
            ..
        } => "social.topic.follow",
        Change::ReportPost { .. } => "social.post.report",
        Change::SavePostDraft { .. } => "social.draft.save",
        Change::PublishDraft { .. } => "social.draft.publish",
        Change::EditPost { .. } => "social.post.edit",
        Change::RetractPost { .. } => "social.post.retract",
        Change::CommentUploadCreate { .. } => "social.comment.upload.create",
        Change::CommentUploadCancel { .. } => "social.comment.upload.cancel",
        Change::PostEditUploadCreate { .. } => "social.post.edit-upload.create",
        Change::PostEditUploadCancel { .. } => "social.post.edit-upload.cancel",
        Change::ModeratePost { .. } => "social.post.hide",
        Change::PublishHandoverPost { .. } => "social.handover.publish",
        Change::CreateStory { .. } => "social.story.publish",
        Change::ArchiveStory { .. } => "social.story.archive",
    }
}

fn record_id(key: &str, change: &Change) -> String {
    match change {
        Change::PublishPost { .. } => format!("post-{key}"),
        Change::PublishComment { .. } => format!("comment-{key}"),
        Change::EditComment { id, .. } | Change::TombstoneComment { id, .. } => id.clone(),
        Change::SetReaction { post_id, .. } => post_id.clone(),
        Change::SetFollow { target_id, .. } => target_id.clone(),
        Change::ReportPost { post_id, .. } => post_id.clone(),
        Change::SavePostDraft { id, .. } => id.clone().unwrap_or_else(|| format!("post-draft-{key}")),
        Change::PublishDraft { id, .. } => format!("post-{id}"),
        Change::EditPost { id, .. } | Change::RetractPost { id, .. } => id.clone(),
        Change::CommentUploadCreate { .. } => format!("comment-upload-{key}"),
        Change::CommentUploadCancel { id } | Change::PostEditUploadCancel { id } => id.clone(),
        Change::PostEditUploadCreate { .. } => format!("post-edit-upload-{key}"),
        Change::ModeratePost { id } => id.clone(),
        Change::PublishHandoverPost { .. } => format!("post-post-draft-{key}"),
        Change::CreateStory { .. } => format!("story-{key}"),
        Change::ArchiveStory { id, .. } => id.clone(),
    }
}

async fn authority(c: &mut PgConnection, user: &str, company: &str) -> Result<SocialActor, Error> {
    let row = sqlx::query(
        "SELECT display_name,branch_id,department_id,share_company,moderate FROM social_test_authority WHERE company_id=$1 AND user_id=$2 AND member=1",
    )
    .bind(company)
    .bind(user)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current company membership is required.",
        )
        .into());
    };
    Ok(SocialActor {
        user_id: user.into(),
        display_name: row.try_get(0)?,
        company_id: company.into(),
        branch_id: row.try_get(1)?,
        department_id: row.try_get(2)?,
        can_share_company: row.try_get::<i64, _>(3)? == 1,
        can_moderate: row.try_get::<i64, _>(4)? == 1,
        can_hide_posts: row.try_get::<i64, _>(4)? == 1,
    })
}

fn actor(user: &str, company: &str, branch: Option<&str>, moderate: bool) -> SocialActor {
    SocialActor {
        user_id: user.into(),
        display_name: format!("{user} name"),
        company_id: company.into(),
        branch_id: branch.map(str::to_owned),
        department_id: None,
        can_share_company: true,
        can_moderate: moderate,
        can_hide_posts: moderate,
    }
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOCIAL_SOURCE_CHANGED",
        "Synthetic current source changed.",
    )
    .into()
}

struct Harness<'a> {
    c: &'a mut PgConnection,
    user: String,
    company: String,
    key: CommandKey,
    operation: String,
    record_id: String,
    fingerprint: String,
    retained: Option<CurrentSocialEffects>,
    uploads: TestUploads,
    tamper: bool,
}

/// Composition's Files owner double: it proves the port receives exactly the
/// canceled draft identity on the caller's connection. The real SQL effects
/// are covered by the Files integration target.
#[derive(Default)]
struct TestUploads;

impl SocialUploadFilesPort for TestUploads {
    fn revoke_draft_uploads<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        draft_id: &'a str,
        now: i64,
    ) -> SocialUploadFilesFuture<'a> {
        Box::pin(async move {
            sqlx::query("INSERT INTO social_test_upload_cleanup(draft_id,now) VALUES($1,$2)")
                .bind(draft_id)
                .bind(now)
                .execute(&mut *connection)
                .await
                .map_err(|_| {
                    BusinessError::new(
                        FailureKind::Unavailable,
                        "TEST_FILES_PORT_UNAVAILABLE",
                        "Synthetic Files port failed.",
                    )
                })?;
            Ok(())
        })
    }
}

impl Harness<'_> {
    async fn receipt(&mut self) -> Result<Option<Receipt>, Error> {
        let row = sqlx::query(
            "SELECT operation,actor_id,record_id,request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR SHARE",
        )
        .bind(self.key.as_str())
        .fetch_optional(&mut *self.c)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let operation: String = row.try_get(0)?;
        let actor: String = row.try_get(1)?;
        let record_id: String = row.try_get(2)?;
        let fingerprint: String = row.try_get(3)?;
        if operation != self.operation
            || actor != self.user
            || record_id != self.record_id
            || fingerprint != self.fingerprint
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to another request.",
            )
            .into());
        }
        decode_receipt(&row.try_get::<String, _>(4)?).map(Some)
    }

    async fn write_receipt(&mut self, receipt: &Receipt) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(self.key.as_str())
        .bind(&self.operation)
        .bind(&self.user)
        .bind(&self.fingerprint)
        .bind(&self.record_id)
        .bind(serde_json::to_string(receipt).map_err(|_| changed())?)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        let (record_type, purpose) = if self.record_id.starts_with("comment-") {
            ("social_comment", "comment")
        } else {
            ("social_post", "social")
        };
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(format!("audit-{}", self.key.as_str()))
        .bind(self.key.as_str())
        .bind(&self.user)
        .bind(format!("{}-assignment", self.user))
        .bind(&self.operation)
        .bind(record_type)
        .bind(&self.record_id)
        .bind(receipt.version)
        .bind(purpose)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(format!("outbox-{}", self.key.as_str()))
        .bind(self.key.as_str())
        .bind(format!("{}.changed", self.operation))
        .bind(record_type)
        .bind(&self.record_id)
        .bind(receipt.version)
        .bind(serde_json::to_string(receipt).map_err(|_| changed())?)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        Ok(())
    }
}

impl SocialUnit for Harness<'_> {
    type Error = Error;
    async fn authorize(&mut self, _: Access) -> Result<SocialActor, Error> {
        authority(self.c, &self.user, &self.company).await
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, Error> {
        self.receipt().await
    }
    async fn post(&mut self, id: &str) -> Result<PostSnapshot, Error> {
        let company = CompanyId::new(self.company.clone())?;
        read_post(self.c, &company, id)
            .await?
            .ok_or_else(|| post_not_found().into())
    }
    async fn draft(&mut self, id: &str) -> Result<Option<DraftSnapshot>, Error> {
        let row = sqlx::query(
            "SELECT id,author_id,company_id,version,published_post_id,body,branch_id,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,mentions_json,content_json,rich_content_json,cancelled_at,comment_parent_post_id,published_comment_id,edit_post_id,edit_post_version,edit_saved_version FROM social_post_drafts WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *self.c)
        .await?;
        row.map(|row| {
            Ok(DraftSnapshot {
                id: row.try_get(0)?,
                author_id: row.try_get(1)?,
                company_id: row.try_get(2)?,
                version: row.try_get(3)?,
                published_post_id: row.try_get(4)?,
                body: row.try_get(5)?,
                branch_id: row.try_get(6)?,
                department_id: row.try_get(7)?,
                audience_kind: row.try_get(8)?,
                audience_id: row.try_get(9)?,
                participants_json: row.try_get(10)?,
                sources_json: row.try_get(11)?,
                purpose: row.try_get(12)?,
                mentions_json: row.try_get(13)?,
                content_json: row.try_get(14)?,
                rich_content_json: row.try_get(15)?,
                cancelled_at: row.try_get(16)?,
                comment_parent_post_id: row.try_get(17)?,
                published_comment_id: row.try_get(18)?,
                edit_post_id: row.try_get(19)?,
                edit_post_version: row.try_get(20)?,
                edit_saved_version: row.try_get(21)?,
            })
        })
        .transpose()
    }
    async fn comment(&mut self, id: &str, post_id: &str) -> Result<CommentSnapshot, Error> {
        read_comment(self.c, id, post_id).await?.ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "COMMENT_NOT_FOUND",
                "The comment is unavailable in this work scope.",
            )
            .into()
        })
    }
    async fn resolve_person(&mut self, id: &str) -> Result<Option<String>, Error> {
        Ok(sqlx::query_scalar(
            "SELECT display_name FROM social_test_people WHERE company_id=$1 AND user_id=$2",
        )
        .bind(&self.company)
        .bind(id)
        .fetch_optional(&mut *self.c)
        .await?)
    }
    async fn resolve_record(&mut self, module: &str, id: &str) -> Result<Option<String>, Error> {
        Ok(sqlx::query_scalar(
            "SELECT label FROM social_test_records WHERE company_id=$1 AND module=$2 AND id=$3",
        )
        .bind(&self.company)
        .bind(module)
        .bind(id)
        .fetch_optional(&mut *self.c)
        .await?)
    }
    async fn apply(
        &mut self,
        actor: &SocialActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, Error> {
        let receipt = apply_plan(
            self.c,
            actor,
            key,
            &plan,
            NOW,
            &mut self.uploads,
        )
        .await?;
        self.retained = Some(retain_effects(self.c, &plan, &receipt).await?);
        self.write_receipt(&receipt).await?;
        Ok(receipt)
    }
    async fn flush_deferred(&mut self) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.c)
            .await?;
        Ok(())
    }
    async fn require_unit_current(&mut self) -> Result<(), Error> {
        authority(self.c, &self.user, &self.company).await?;
        if let Some(retained) = &self.retained {
            retained.require_current(self.c).await?;
        }
        if self.receipt().await?.is_none() || self.tamper {
            return Err(changed());
        }
        Ok(())
    }
}

fn decode_receipt(raw: &str) -> Result<Receipt, Error> {
    let value: Value = serde_json::from_str(raw).map_err(|_| changed())?;
    let status = match value["status"].as_str() {
        Some("published") => "published",
        Some("active") => "active",
        Some("inactive") => "inactive",
        Some("followed") => "followed",
        Some("unfollowed") => "unfollowed",
        Some("tombstoned") => "tombstoned",
        Some("reported") => "reported",
        Some("draft") => "draft",
        Some("archived") => "archived",
        Some("cancelled") => "cancelled",
        Some("moderated") => "moderated",
        _ => return Err(changed()),
    };
    let id = value["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(changed)?;
    let version = value["version"]
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or_else(changed)?;
    Ok(Receipt {
        id: id.into(),
        version,
        status,
    })
}

async fn run(
    pool: &DatabasePool,
    user: &str,
    company: &str,
    key: &str,
    change: Change,
) -> Result<Receipt, Error> {
    run_observed(pool, user, company, key, change, false).await
}

async fn run_observed(
    pool: &DatabasePool,
    user: &str,
    company: &str,
    key: &str,
    change: Change,
    tamper: bool,
) -> Result<Receipt, Error> {
    let key = CommandKey::new(key)?;
    let operation = operation(&change).to_owned();
    let record_id = record_id(key.as_str(), &change);
    let fingerprint = format!("{change:?}");
    let user = user.to_owned();
    let company = company.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let key = key.clone();
            let change = change.clone();
            let user = user.clone();
            let company = company.clone();
            let operation = operation.clone();
            let record_id = record_id.clone();
            let fingerprint = fingerprint.clone();
            Box::pin(async move {
                lock_social_sources(c).await?;
                let mut harness = Harness {
                    c,
                    user,
                    company,
                    key: key.clone(),
                    operation,
                    record_id,
                    fingerprint,
                    retained: None,
                    uploads: TestUploads,
                    tamper,
                };
                execute(&mut harness, &key, change).await
            })
        })
        .await
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
        "CREATE TABLE social_test_authority(company_id text NOT NULL,user_id text NOT NULL,
           member bigint NOT NULL,share_company bigint NOT NULL,moderate bigint NOT NULL,
           branch_id text,department_id text,display_name text NOT NULL,
           PRIMARY KEY(company_id,user_id));
         CREATE TABLE social_test_people(company_id text NOT NULL,user_id text NOT NULL,
           display_name text NOT NULL,PRIMARY KEY(company_id,user_id));
         CREATE TABLE social_test_records(company_id text NOT NULL,module text NOT NULL,id text NOT NULL,
           label text NOT NULL,PRIMARY KEY(company_id,module,id));
         CREATE TABLE social_test_upload_cleanup(draft_id text NOT NULL,now bigint NOT NULL);
         INSERT INTO social_test_authority VALUES
           ('company','author',1,1,0,'branch-a',NULL,'Author'),
           ('company','branch-b',1,1,0,'branch-b',NULL,'Branch B'),
           ('company','moderator',1,1,1,'branch-a',NULL,'Moderator'),
           ('company','person-1',1,1,0,'branch-a',NULL,'Person One'),
           ('company','reader',1,0,0,'branch-a',NULL,'Reader'),
           ('other','other-author',1,1,0,'branch-a',NULL,'Other Author');
         INSERT INTO social_test_people VALUES
           ('company','person-1','Person One'),('other','other-author','Other Author');
         INSERT INTO social_test_records VALUES
           ('company','people_payroll','leave-1','LR-1 · Leave request');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn post_change(body: &str, audience: Audience) -> Change {
    Change::PublishPost {
        body: body.into(),
        audience,
        tags: vec!["Policy".into(), "ops".into()],
        mentions: vec![
            MentionInput {
                kind: MentionKind::Person,
                target_id: "person-1".into(),
            },
            MentionInput {
                kind: MentionKind::Record,
                target_id: "people_payroll/leave-1".into(),
            },
        ],
        linked_record: Some(LinkedRecordInput {
            module: "people_payroll".into(),
            id: "leave-1".into(),
        }),
    }
}

#[tokio::test]
async fn post_publish_topics_mentions_replay_feed_and_company_isolation() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let key = "social-post-0001";
        let change = post_change("  Hello #Policy  ", Audience::Branch);
        let receipt = run(&pool, "author", "company", key, change.clone())
            .await
            .unwrap();
        assert_eq!(receipt.id, format!("post-{key}"));
        assert_eq!((receipt.version, receipt.status), (1, "published"));
        let replay = run(&pool, "author", "company", key, change).await.unwrap();
        assert_eq!(replay, receipt);
        let counts: (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM social_posts),(SELECT count(*) FROM social_post_topics),(SELECT count(*) FROM social_mentions),(SELECT count(*) FROM staff_notifications),(SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 2, 2, 1, 1, 1, 1));

        let author = actor("author", "company", Some("branch-a"), false);
        let mut tx = pool.raw().begin().await.unwrap();
        let feed = read_feed(&mut tx, &author, FeedFilter::All, None, 30)
            .await
            .unwrap();
        assert_eq!(feed.len(), 1);
        assert_eq!(
            feed[0].topics,
            vec!["ops".to_owned(), "policy".to_owned()]
        );
        assert_eq!(feed[0].linked_record.as_ref().unwrap().label, "LR-1 · Leave request");
        assert!(feed[0].comments.is_empty());
        let other_branch = actor("branch-b", "company", Some("branch-b"), false);
        assert!(
            read_feed(&mut tx, &other_branch, FeedFilter::All, None, 30)
                .await
                .unwrap()
                .is_empty()
        );
        let other_company = actor("other-author", "other", Some("branch-a"), false);
        assert!(
            read_feed(&mut tx, &other_company, FeedFilter::All, None, 30)
                .await
                .unwrap()
                .is_empty()
        );
        tx.rollback().await.unwrap();

        let denied = run(
            &pool,
            "reader",
            "company",
            "social-post-0002",
            post_change("Company wide", Audience::Holding),
        )
        .await
        .unwrap_err();
        assert!(matches!(denied, PgTransactionError::Business(e) if e.code == "AUDIENCE_FORBIDDEN"));
        let missing = run(
            &pool,
            "author",
            "company",
            "social-post-0003",
            Change::PublishComment {
                post_id: receipt.id.clone(),
                body: "Hello".into(),
                parent_comment_id: None,
                mentions: vec![MentionInput {
                    kind: MentionKind::Person,
                    target_id: "other-author".into(),
                }],
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(missing, PgTransactionError::Business(e) if e.code == "MENTION_TARGET_NOT_FOUND"));
        let posts: i64 = sqlx::query_scalar("SELECT count(*) FROM social_posts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(posts, 1);
    })
    .await;
}

#[tokio::test]
async fn comments_edit_tombstone_moderation_cas_and_readback() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let post = run(
            &pool,
            "author",
            "company",
            "social-post-0101",
            post_change("Post", Audience::Branch),
        )
        .await
        .unwrap();
        let publish = run(
            &pool,
            "author",
            "company",
            "social-comment-0101",
            Change::PublishComment {
                post_id: post.id.clone(),
                body: "  Reply #Ops  ".into(),
                parent_comment_id: None,
                mentions: vec![MentionInput {
                    kind: MentionKind::Person,
                    target_id: "person-1".into(),
                }],
            },
        )
        .await
        .unwrap();
        assert_eq!((publish.version, publish.status), (1, "published"));
        let edit = run(
            &pool,
            "author",
            "company",
            "social-comment-0102",
            Change::EditComment {
                post_id: post.id.clone(),
                id: publish.id.clone(),
                expected_version: 1,
                body: "Edited".into(),
                mentions: vec![],
            },
        )
        .await
        .unwrap();
        assert_eq!((edit.version, edit.status), (2, "published"));
        let mentions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM social_mentions WHERE owner_type='comment' AND owner_id=$1",
        )
        .bind(&publish.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(mentions, 0);
        let stale = run(
            &pool,
            "author",
            "company",
            "social-comment-0103",
            Change::EditComment {
                post_id: post.id.clone(),
                id: publish.id.clone(),
                expected_version: 1,
                body: "Stale".into(),
                mentions: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(stale, PgTransactionError::Business(e) if e.code == "VERSION_CONFLICT"));
        let denied = run(
            &pool,
            "moderator",
            "company",
            "social-comment-0104",
            Change::EditComment {
                post_id: post.id.clone(),
                id: publish.id.clone(),
                expected_version: 2,
                body: "Moderator edit".into(),
                mentions: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(denied, PgTransactionError::Business(e) if e.code == "COMMENT_AUTHOR_REQUIRED")
        );
        let removed = run(
            &pool,
            "moderator",
            "company",
            "social-comment-0105",
            Change::TombstoneComment {
                post_id: post.id.clone(),
                id: publish.id.clone(),
                expected_version: 2,
            },
        )
        .await
        .unwrap();
        assert_eq!((removed.version, removed.status), (3, "tombstoned"));
        let mut tx = pool.raw().begin().await.unwrap();
        let comments = read_comments(&mut tx, &post.id, 30).await.unwrap();
        assert!(comments.is_empty());
        let feed = read_feed(
            &mut tx,
            &actor("author", "company", Some("branch-a"), false),
            FeedFilter::All,
            None,
            30,
        )
        .await
        .unwrap();
        assert_eq!(feed[0].comment_count, 0);
        tx.rollback().await.unwrap();
        let stale_remove = run(
            &pool,
            "moderator",
            "company",
            "social-comment-0106",
            Change::TombstoneComment {
                post_id: post.id.clone(),
                id: publish.id.clone(),
                expected_version: 2,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(stale_remove, PgTransactionError::Business(e) if e.code == "COMMENT_NOT_FOUND")
        );
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id LIKE 'social-comment-01%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 3);
    })
    .await;
}

#[tokio::test]
async fn reactions_follows_filters_and_tampered_effects_roll_back() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let post = run(
            &pool,
            "author",
            "company",
            "social-post-0201",
            post_change("Post", Audience::Holding),
        )
        .await
        .unwrap();
        let reaction = run(
            &pool,
            "author",
            "company",
            "social-reaction-0201",
            Change::SetReaction {
                post_id: post.id.clone(),
                active: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(reaction.status, "active");
        run(
            &pool,
            "author",
            "company",
            "social-reaction-0202",
            Change::SetReaction {
                post_id: post.id.clone(),
                active: true,
            },
        )
        .await
        .unwrap();
        let reactions: i64 = sqlx::query_scalar("SELECT count(*) FROM social_reactions")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(reactions, 1);
        run(
            &pool,
            "author",
            "company",
            "social-follow-0201",
            Change::SetFollow {
                target: FollowTarget::Post,
                target_id: post.id.clone(),
                active: true,
            },
        )
        .await
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        let following = read_feed(
            &mut tx,
            &actor("author", "company", Some("branch-a"), false),
            FeedFilter::Following,
            None,
            30,
        )
        .await
        .unwrap();
        assert_eq!(following.len(), 1);
        assert!(following[0].appreciated);
        assert!(following[0].followed);
        let records = read_feed(
            &mut tx,
            &actor("author", "company", Some("branch-a"), false),
            FeedFilter::Records,
            None,
            30,
        )
        .await
        .unwrap();
        assert_eq!(records.len(), 1);
        tx.rollback().await.unwrap();
        run(
            &pool,
            "author",
            "company",
            "social-follow-0202",
            Change::SetFollow {
                target: FollowTarget::Post,
                target_id: post.id.clone(),
                active: false,
            },
        )
        .await
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        assert!(
            read_feed(
                &mut tx,
                &actor("author", "company", Some("branch-a"), false),
                FeedFilter::Following,
                None,
                30,
            )
            .await
            .unwrap()
            .is_empty()
        );
        tx.rollback().await.unwrap();
        run(
            &pool,
            "author",
            "company",
            "social-follow-0203",
            Change::SetFollow {
                target: FollowTarget::Topic,
                target_id: "Policy".into(),
                active: true,
            },
        )
        .await
        .unwrap();
        let topic_follows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM social_follows WHERE target_type='topic' AND target_id='policy'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(topic_follows, 1);

        let before: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM social_posts),(SELECT count(*) FROM command_receipts)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let tampered = run_observed(
            &pool,
            "author",
            "company",
            "social-post-0202",
            post_change("Tampered", Audience::Holding),
            true,
        )
        .await;
        assert!(matches!(tampered, Err(PgTransactionError::Business(e)) if e.code == "SOCIAL_SOURCE_CHANGED"));
        let after: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM social_posts),(SELECT count(*) FROM command_receipts)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(before, after);
    })
    .await;
}

#[tokio::test]
async fn reports_are_idempotent_per_reporter_and_replay_without_duplicates() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let post = run(
            &pool,
            "author",
            "company",
            "social-post-0301",
            post_change("Report target", Audience::Holding),
        )
        .await
        .unwrap();
        let first = run(
            &pool,
            "reader",
            "company",
            "social-report-0301",
            Change::ReportPost {
                post_id: post.id.clone(),
                reason: "Spam".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!((first.id.as_str(), first.status), (post.id.as_str(), "reported"));
        let replay = run(
            &pool,
            "reader",
            "company",
            "social-report-0301",
            Change::ReportPost {
                post_id: post.id.clone(),
                reason: "Spam".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(replay, first);
        run(
            &pool,
            "reader",
            "company",
            "social-report-0302",
            Change::ReportPost {
                post_id: post.id.clone(),
                reason: "Harassment".into(),
            },
        )
        .await
        .unwrap();
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id,reason FROM social_reports WHERE post_id=$1 ORDER BY reporter_id",
        )
        .bind(&post.id)
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "Harassment");
        let missing = run(
            &pool,
            "reader",
            "company",
            "social-report-0303",
            Change::ReportPost {
                post_id: "post-missing".into(),
                reason: "Spam".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(missing, PgTransactionError::Business(e) if e.code == "POST_NOT_FOUND"));
    })
    .await;
}

#[tokio::test]
async fn drafts_save_update_replay_and_company_scope() {
    fixture::with_database(5, |pool| async move {
        setup(&pool).await;
        let draft = |id: Option<String>, body: &str, expected_version: i64| Change::SavePostDraft {
            id,
            body: body.into(),
            audience_kind: Some("branch".into()),
            audience_id: Some("branch-a".into()),
            participants: vec![],
            sources_json: "[]".into(),
            purpose: "update".into(),
            mentions: vec![],
            content_json: None,
            rich_content_json: None,
            expected_version,
        };
        let created = run(
            &pool,
            "author",
            "company",
            "social-draft-0401",
            draft(None, "Draft", 1),
        )
        .await
        .unwrap();
        assert_eq!((created.id.as_str(), created.version, created.status), ("post-draft-social-draft-0401", 1, "draft"));
        let row: (String, String, String, i64) = sqlx::query_as(
            "SELECT author_id,audience_kind,audience_id,version FROM social_post_drafts WHERE id=$1",
        )
        .bind(&created.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("author".into(), "branch".into(), "branch-a".into(), 1));
        let replay = run(
            &pool,
            "author",
            "company",
            "social-draft-0401",
            draft(None, "Draft", 1),
        )
        .await
        .unwrap();
        assert_eq!(replay, created);
        let updated = run(
            &pool,
            "author",
            "company",
            "social-draft-0402",
            draft(Some(created.id.clone()), "Updated", 1),
        )
        .await
        .unwrap();
        assert_eq!(updated.version, 2);
        let stale = run(
            &pool,
            "author",
            "company",
            "social-draft-0403",
            draft(Some(created.id.clone()), "Stale", 1),
        )
        .await
        .unwrap_err();
        assert!(matches!(stale, PgTransactionError::Business(e) if e.code == "VERSION_CONFLICT"));
        let foreign = run(
            &pool,
            "other-author",
            "other",
            "social-draft-0404",
            draft(Some(created.id.clone()), "Foreign", 2),
        )
        .await
        .unwrap_err();
        assert!(matches!(foreign, PgTransactionError::Business(e) if e.code == "DRAFT_NOT_FOUND"));
    })
    .await;
}

#[tokio::test]
async fn publish_draft_creates_the_social_post_participants_sources_and_binding() {
    fixture::with_database(6, |pool| async move {
        setup(&pool).await;
        let created = run(
            &pool,
            "author",
            "company",
            "social-draft-0501",
            Change::SavePostDraft {
                id: None,
                body: "Published from draft".into(),
                audience_kind: Some("participants".into()),
                audience_id: None,
                participants: vec!["person-1".into()],
                sources_json: r#"[{"kind":"staff_task","id":"task-1"}]"#.into(),
                purpose: "plan".into(),
                mentions: vec![MentionInput {
                    kind: MentionKind::Person,
                    target_id: "person-1".into(),
                }],
                content_json: None,
                rich_content_json: None,
                expected_version: 1,
            },
        )
        .await
        .unwrap();
        let published = run(
            &pool,
            "author",
            "company",
            "social-draft-0502",
            Change::PublishDraft {
                id: created.id.clone(),
                version: 1,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            (published.id.clone(), published.status),
            (format!("post-{}", created.id), "published")
        );
        let post: (String, String, String, i64) = sqlx::query_as(
            "SELECT body,audience_kind,purpose,version FROM social_posts WHERE id=$1",
        )
        .bind(&published.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            post,
            ("Published from draft".into(), "participants".into(), "plan".into(), 1)
        );
        let participants: i64 =
            sqlx::query_scalar("SELECT count(*) FROM social_post_participants WHERE post_id=$1")
                .bind(&published.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(participants, 2);
        let sources: (String, String) = sqlx::query_as(
            "SELECT source_kind,source_id FROM social_post_sources WHERE post_id=$1 ORDER BY ordinal",
        )
        .bind(&published.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(sources, ("staff_task".into(), "task-1".into()));
        let bound: (Option<String>, i64) = sqlx::query_as(
            "SELECT published_post_id,version FROM social_post_drafts WHERE id=$1",
        )
        .bind(&created.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(bound, (Some(published.id.clone()), 2));
        let replay = run(
            &pool,
            "author",
            "company",
            "social-draft-0502",
            Change::PublishDraft {
                id: created.id.clone(),
                version: 1,
            },
        )
        .await
        .unwrap();
        assert_eq!(replay, published);
        let again = run(
            &pool,
            "author",
            "company",
            "social-draft-0503",
            Change::PublishDraft {
                id: created.id.clone(),
                version: 2,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(again, PgTransactionError::Business(e) if e.code == "DRAFT_NOT_FOUND"));
    })
    .await;
}

#[tokio::test]
async fn edits_and_retracts_the_authors_post_with_immutable_revisions() {
    fixture::with_database(7, |pool| async move {
        setup(&pool).await;
        let post = run(
            &pool,
            "author",
            "company",
            "social-post-0601",
            Change::PublishPost {
                body: "Original".into(),
                audience: Audience::Holding,
                tags: vec![],
                mentions: vec![],
                linked_record: None,
            },
        )
        .await
        .unwrap();
        let edited = run(
            &pool,
            "author",
            "company",
            "social-post-0602",
            Change::EditPost {
                id: post.id.clone(),
                expected_version: 1,
                body: "Edited".into(),
                mentions: vec![],
                content_json: None,
                rich_content_json: None,
            },
        )
        .await
        .unwrap();
        assert_eq!((edited.version, edited.status), (2, "published"));
        let row: (String, i64) =
            sqlx::query_as("SELECT body,version FROM social_posts WHERE id=$1")
                .bind(&post.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(row, ("Edited".into(), 2));
        let revision: (String, i64, String) = sqlx::query_as(
            "SELECT action,prior_version,body FROM social_post_revisions WHERE post_id=$1 ORDER BY prior_version",
        )
        .bind(&post.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(revision, ("edit".into(), 1, "Original".into()));
        let stale = run(
            &pool,
            "author",
            "company",
            "social-post-0603",
            Change::EditPost {
                id: post.id.clone(),
                expected_version: 1,
                body: "Stale".into(),
                mentions: vec![],
                content_json: None,
                rich_content_json: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(stale, PgTransactionError::Business(e) if e.code == "VERSION_CONFLICT"));
        let foreign = run(
            &pool,
            "other-author",
            "other",
            "social-post-0604",
            Change::EditPost {
                id: post.id.clone(),
                expected_version: 2,
                body: "Foreign".into(),
                mentions: vec![],
                content_json: None,
                rich_content_json: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(foreign, PgTransactionError::Business(e) if e.code == "POST_NOT_FOUND"));
        let retracted = run(
            &pool,
            "author",
            "company",
            "social-post-0605",
            Change::RetractPost {
                id: post.id.clone(),
                expected_version: 2,
            },
        )
        .await
        .unwrap();
        assert_eq!((retracted.version, retracted.status), (3, "archived"));
        let state: (String, i64) =
            sqlx::query_as("SELECT state,version FROM social_posts WHERE id=$1")
                .bind(&post.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(state, ("archived".into(), 3));
        let retract_revision: (String, i64, String) = sqlx::query_as(
            "SELECT action,prior_version,body FROM social_post_revisions WHERE post_id=$1 AND action='retract'",
        )
        .bind(&post.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(retract_revision, ("retract".into(), 2, "Edited".into()));
    })
    .await;
}

#[tokio::test]
async fn upload_drafts_create_and_cancel_comment_and_edit_targets() {
    fixture::with_database(8, |pool| async move {
        setup(&pool).await;
        let post = run(
            &pool,
            "author",
            "company",
            "social-post-0701",
            Change::PublishPost {
                body: "Upload target".into(),
                audience: Audience::Holding,
                tags: vec![],
                mentions: vec![],
                linked_record: None,
            },
        )
        .await
        .unwrap();
        let comment = run(
            &pool,
            "author",
            "company",
            "social-comment-upload-0701",
            Change::CommentUploadCreate {
                post_id: post.id.clone(),
            },
        )
        .await
        .unwrap();
        let row: (Option<String>, Option<String>, i64) = sqlx::query_as(
            "SELECT comment_parent_post_id,published_comment_id,version FROM social_post_drafts WHERE id=$1",
        )
        .bind(&comment.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, (Some(post.id.clone()), None, 1));
        let cancelled = run(
            &pool,
            "author",
            "company",
            "social-comment-upload-0702",
            Change::CommentUploadCancel {
                id: comment.id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        let cancelled_row: (Option<i64>, i64) = sqlx::query_as(
            "SELECT cancelled_at,version FROM social_post_drafts WHERE id=$1",
        )
        .bind(&comment.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(cancelled_row.0.is_some());
        assert_eq!(cancelled_row.1, 2);
        let comment_cleanup: Vec<(String, i64)> =
            sqlx::query_as("SELECT draft_id,now FROM social_test_upload_cleanup ORDER BY draft_id")
                .fetch_all(pool.raw())
                .await
                .unwrap();
        assert_eq!(
            comment_cleanup,
            vec![(comment.id.clone(), NOW)],
            "the Files port must receive the canceled comment-upload draft"
        );
        let edit = run(
            &pool,
            "author",
            "company",
            "social-edit-upload-0703",
            Change::PostEditUploadCreate {
                post_id: post.id.clone(),
                expected_version: 1,
            },
        )
        .await
        .unwrap();
        let edit_row: (Option<String>, Option<i64>, i64) = sqlx::query_as(
            "SELECT edit_post_id,edit_post_version,version FROM social_post_drafts WHERE id=$1",
        )
        .bind(&edit.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(edit_row, (Some(post.id.clone()), Some(1), 1));
        let edit_cancelled = run(
            &pool,
            "author",
            "company",
            "social-edit-upload-0704",
            Change::PostEditUploadCancel {
                id: edit.id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(edit_cancelled.status, "cancelled");
        let cleanup: Vec<(String, i64)> =
            sqlx::query_as("SELECT draft_id,now FROM social_test_upload_cleanup ORDER BY draft_id")
                .fetch_all(pool.raw())
                .await
                .unwrap();
        let mut expected = vec![(comment.id.clone(), NOW), (edit.id.clone(), NOW)];
        expected.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            cleanup, expected,
            "the Files port must receive each canceled upload draft exactly once"
        );
    })
    .await;
}

#[tokio::test]
async fn moderators_hide_posts_but_other_roles_cannot() {
    fixture::with_database(9, |pool| async move {
        setup(&pool).await;
        let post = run(
            &pool,
            "author",
            "company",
            "social-post-0801",
            Change::PublishPost {
                body: "Moderate target".into(),
                audience: Audience::Holding,
                tags: vec![],
                mentions: vec![],
                linked_record: None,
            },
        )
        .await
        .unwrap();
        let denied = run(
            &pool,
            "branch-b",
            "company",
            "social-hide-0802",
            Change::ModeratePost {
                id: post.id.clone(),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(denied, PgTransactionError::Business(e) if e.code == "POST_MODERATOR_REQUIRED"));
        let hidden = run(
            &pool,
            "moderator",
            "company",
            "social-hide-0803",
            Change::ModeratePost {
                id: post.id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!((hidden.version, hidden.status), (2, "moderated"));
        let state: (String, i64) =
            sqlx::query_as("SELECT state,version FROM social_posts WHERE id=$1")
                .bind(&post.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(state, ("moderated".into(), 2));
    })
    .await;
}

#[tokio::test]
async fn handover_publish_creates_the_participant_post_and_sources() {
    fixture::with_database(10, |pool| async move {
        setup(&pool).await;
        let handover = Change::PublishHandoverPost {
            body: "Night handover".into(),
            recipient_ids: vec!["person-1".into()],
            sources: vec![PostSource {
                kind: "staff_task".into(),
                id: "task-1".into(),
            }],
        };
        let published = run(
            &pool,
            "author",
            "company",
            "social-handover-0901",
            handover.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            (published.id.as_str(), published.version, published.status),
            ("post-post-draft-social-handover-0901", 1, "published")
        );
        let post: (String, String, String) = sqlx::query_as(
            "SELECT purpose,audience_kind,body FROM social_posts WHERE id=$1",
        )
        .bind(&published.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            post,
            ("handover".into(), "participants".into(), "Night handover".into())
        );
        let participants: i64 =
            sqlx::query_scalar("SELECT count(*) FROM social_post_participants WHERE post_id=$1")
                .bind(&published.id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(participants, 2);
        let source: (String, String) = sqlx::query_as(
            "SELECT source_kind,source_id FROM social_post_sources WHERE post_id=$1",
        )
        .bind(&published.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(source, ("staff_task".into(), "task-1".into()));
        let operational: (String, String, String, String) = sqlx::query_as(
            "SELECT operation,actor_id,authority_json::jsonb->>'companyId',(result_json::jsonb->'sourceBindings')::text FROM command_receipts WHERE command_id=$1",
        )
        .bind("social-handover-0901-operations")
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            (operational.0.as_str(), operational.1.as_str(), operational.2.as_str()),
            ("work.handover.publish", "author", "company")
        );
        let expected_bindings: String = sqlx::query_scalar(
            "SELECT jsonb_agg(jsonb_build_object('kind',source_kind,'id',source_id) ORDER BY ordinal)::text FROM social_post_sources WHERE post_id=$1",
        )
        .bind(&published.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&operational.3).unwrap(),
            serde_json::from_str::<serde_json::Value>(&expected_bindings).unwrap()
        );
        let replay = run(
            &pool,
            "author",
            "company",
            "social-handover-0901",
            handover,
        )
        .await
        .unwrap();
        assert_eq!(replay, published);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn story_publish_and_archive_round_trip_with_replay_and_author_guard() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let key = "social-story-0001";
        let change = Change::CreateStory {
            title: "  News  ".into(),
            body: " Current story ".into(),
            theme: "sage".into(),
            audience: Audience::Branch,
        };
        let receipt = run(&pool, "author", "company", key, change.clone())
            .await
            .unwrap();
        assert_eq!(receipt.id, format!("story-{key}"));
        assert_eq!((receipt.version, receipt.status), (1, "published"));
        let replay = run(&pool, "author", "company", key, change).await.unwrap();
        assert_eq!(replay, receipt);
        let row: (String, i64, String, String) = sqlx::query_as(
            "SELECT author_id,version,state,title FROM social_stories WHERE id=$1",
        )
        .bind(&receipt.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row,
            ("author".into(), 1, "published".into(), "News".into())
        );

        let archive_key = "social-story-0002";
        let archive = Change::ArchiveStory {
            id: receipt.id.clone(),
            expected_version: 1,
        };
        let archived = run(&pool, "author", "company", archive_key, archive.clone())
            .await
            .unwrap();
        assert_eq!(archived.id, receipt.id);
        assert_eq!((archived.version, archived.status), (2, "archived"));
        let replay_archived = run(&pool, "author", "company", archive_key, archive)
            .await
            .unwrap();
        assert_eq!(replay_archived, archived);
        let state: (String, i64) = sqlx::query_as(
            "SELECT state,version FROM social_stories WHERE id=$1",
        )
        .bind(&archived.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state, ("archived".into(), 2));

        let denied = run(
            &pool,
            "reader",
            "company",
            "social-story-0003",
            Change::ArchiveStory {
                id: archived.id.clone(),
                expected_version: 2,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(denied, PgTransactionError::Business(e) if e.code == "STORY_CONFLICT"));
    })
    .await;
}
