//! Actual Holding PostgreSQL Social rows with an explicit owner receipt/audit/
//! outbox harness. The authority/People/Records tables below are synthetic
//! composition ports, not production IAM or cross-owner evidence.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::social::*;
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
    tamper: bool,
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
            "SELECT id,author_id,company_id,version,published_post_id FROM social_post_drafts WHERE id=$1",
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
        let receipt = apply_plan(self.c, actor, key, &plan, NOW).await?;
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
