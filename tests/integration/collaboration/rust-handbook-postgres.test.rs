//! Actual Holding PostgreSQL owner rows with an explicit owner receipt/audit/
//! outbox harness. The authority table below is a synthetic composition port; it
//! is not production IAM or purchase evidence. Workflow-ledger mounting remains a
//! composition gate rather than something this owner crate can claim.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::handbook::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

const NOW: i64 = 1_700_000_000_000;
type Error = PgTransactionError<BusinessError>;

async fn authority(
    c: &mut PgConnection,
    user: &str,
    company: &str,
) -> Result<AuthorizedHandbookActor, Error> {
    let row = sqlx::query(
        "SELECT member,editor,owner FROM handbook_test_authority WHERE company_id=$1 AND user_id=$2",
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
    let member: i64 = row.try_get(0)?;
    let editor: i64 = row.try_get(1)?;
    let owner: i64 = row.try_get(2)?;
    if member != 1 {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current company membership is required.",
        )
        .into());
    }
    Ok(AuthorizedHandbookActor {
        user_id: user.into(),
        company_id: company.into(),
        can_read: true,
        can_edit: editor == 1 || owner == 1,
        can_manage_editors: owner == 1,
    })
}

fn actor(user: &str, company: &str, editor: bool, owner: bool) -> AuthorizedHandbookActor {
    AuthorizedHandbookActor {
        user_id: user.into(),
        company_id: company.into(),
        can_read: true,
        can_edit: editor || owner,
        can_manage_editors: owner,
    }
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "HANDBOOK_SOURCE_CHANGED",
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
    input_json: String,
    retained: Option<CurrentHandbookEffects>,
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
        let request_hash: String = row.try_get(3)?;
        if operation != self.operation
            || actor != self.user
            || record_id != self.record_id
            || request_hash != self.input_json
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
        let result = serde_json::to_string(&json!({
            "id": receipt.id,
            "version": receipt.version,
            "revision": receipt.revision,
            "status": receipt.status
        }))
        .map_err(|_| changed())?;
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(self.key.as_str())
        .bind(&self.operation)
        .bind(&self.user)
        .bind(&self.input_json)
        .bind(&self.record_id)
        .bind(&result)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        let record_type = if self.record_id.starts_with("article-") {
            "handbook_article"
        } else {
            "handbook_editor"
        };
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'handbook',$9)",
        )
        .bind(format!("audit-{}", self.key.as_str()))
        .bind(self.key.as_str())
        .bind(&self.user)
        .bind(format!("{}-assignment", self.user))
        .bind(&self.operation)
        .bind(record_type)
        .bind(&self.record_id)
        .bind(receipt.version)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,'handbook.changed',$3,$4,$5,$6,$7)",
        )
        .bind(format!("outbox-{}", self.key.as_str()))
        .bind(self.key.as_str())
        .bind(record_type)
        .bind(&self.record_id)
        .bind(receipt.version)
        .bind(&result)
        .bind(NOW)
        .execute(&mut *self.c)
        .await?;
        Ok(())
    }
}
impl HandbookUnit for Harness<'_> {
    type Error = Error;
    async fn authorize(&mut self, _: Access) -> Result<AuthorizedHandbookActor, Error> {
        authority(self.c, &self.user, &self.company).await
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, Error> {
        self.receipt().await
    }
    async fn article(&mut self, id: &str) -> Result<ArticleSnapshot, Error> {
        let company = CompanyId::new(self.company.clone())?;
        read_article(self.c, &company, id).await?.ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "ARTICLE_NOT_FOUND",
                "Article not found.",
            )
            .into()
        })
    }
    async fn settings_version(&mut self) -> Result<i64, Error> {
        let company = CompanyId::new(self.company.clone())?;
        read_settings_version(self.c, &company).await
    }
    async fn is_company_member(&mut self, user_id: &str) -> Result<bool, Error> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM handbook_test_members WHERE company_id=$1 AND user_id=$2)",
        )
        .bind(&self.company)
        .bind(user_id)
        .fetch_one(&mut *self.c)
        .await?)
    }
    async fn apply(
        &mut self,
        actor: &AuthorizedHandbookActor,
        key: &CommandKey,
        plan: OwnerPlan,
    ) -> Result<Receipt, Error> {
        let receipt = apply_plan(self.c, actor, key, &plan, NOW).await?;
        self.retained = Some(retain_effects(self.c, actor, key, &plan, &receipt, NOW).await?);
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
        if self.receipt().await?.is_none() {
            return Err(changed());
        }
        if self.tamper {
            return Err(changed());
        }
        Ok(())
    }
}

fn decode_receipt(raw: &str) -> Result<Receipt, Error> {
    let value: Value = serde_json::from_str(raw).map_err(|_| changed())?;
    let status = match value["status"].as_str() {
        Some("draft") => "draft",
        Some("published") => "published",
        Some("archived") => "archived",
        Some("restored") => "restored",
        Some("active") => "active",
        Some("inactive") => "inactive",
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
    let revision = value["revision"]
        .as_i64()
        .filter(|value| *value >= 0)
        .ok_or_else(changed)?;
    Ok(Receipt {
        id: id.into(),
        version,
        revision,
        status,
    })
}

fn record_id(key: &str, change: &Change) -> String {
    match change {
        Change::CreateArticle { .. } => format!("article-{key}"),
        Change::UpdateArticle { id, .. } | Change::ArticleAction { id, .. } => id.clone(),
        Change::SetEditor { user_id, .. } => user_id.clone(),
    }
}

async fn run(
    pool: &DatabasePool,
    user: &str,
    company: &str,
    key: &str,
    operation: &str,
    id: Option<&str>,
    raw: Value,
) -> Result<Receipt, Error> {
    run_observed(pool, user, company, key, operation, id, raw, false).await
}

#[allow(clippy::too_many_arguments)]
async fn run_observed(
    pool: &DatabasePool,
    user: &str,
    company: &str,
    key: &str,
    operation: &str,
    id: Option<&str>,
    raw: Value,
    tamper: bool,
) -> Result<Receipt, Error> {
    let key = CommandKey::new(key)?;
    let decoded = decode_handbook_command(operation, id, &raw)?;
    let record_id = record_id(key.as_str(), &decoded.change);
    let input_json = serde_json::to_string(&decoded.input).map_err(|_| changed())?;
    let change = decoded.change;
    let user = user.to_owned();
    let company = company.to_owned();
    let operation = operation.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let key = key.clone();
            let change = change.clone();
            let user = user.clone();
            let company = company.clone();
            let operation = operation.clone();
            let record_id = record_id.clone();
            let input_json = input_json.clone();
            Box::pin(async move {
                let mut harness = Harness {
                    c,
                    user,
                    company,
                    key: key.clone(),
                    operation,
                    record_id,
                    input_json,
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
        "CREATE TABLE handbook_test_authority(company_id text NOT NULL,user_id text NOT NULL,
           member bigint NOT NULL,editor bigint NOT NULL,owner bigint NOT NULL,
           PRIMARY KEY(company_id,user_id));
         CREATE TABLE handbook_test_members(company_id text NOT NULL,user_id text NOT NULL,
           PRIMARY KEY(company_id,user_id));
         INSERT INTO handbook_test_authority VALUES
           ('company','editor',1,1,0),('company','owner',1,1,1),('company','plain',1,0,0),
           ('company','target',1,1,0),('other','editor',1,1,0),('other','target',1,1,0);
         INSERT INTO handbook_test_members VALUES
           ('company','editor'),('company','owner'),('company','target'),
           ('other','editor'),('other','target');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn article_id(key: &str) -> String {
    format!("article-{key}")
}

#[tokio::test]
async fn article_lifecycle_replay_company_isolation_and_current_readback() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let create_key = "handbook-create-0001";
        let id = article_id(create_key);
        let create = run(
            &pool,
            "editor",
            "company",
            create_key,
            "handbook.article.create",
            None,
            json!({"title":"  Policy  ","category":"policy","body":"Original body"}),
        )
        .await
        .unwrap();
        assert_eq!(
            create,
            Receipt {
                id: id.clone(),
                version: 1,
                revision: 1,
                status: "draft"
            }
        );
        let replay = run(
            &pool,
            "editor",
            "company",
            create_key,
            "handbook.article.create",
            None,
            json!({"title":"  Policy  ","category":"policy","body":"Original body"}),
        )
        .await
        .unwrap();
        assert_eq!(replay, create);
        let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM handbook_articles),(SELECT count(*) FROM handbook_revisions),(SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1, 1, 1));

        let company = CompanyId::new("company").unwrap();
        let editor = actor("editor", "company", true, false);
        let plain = actor("plain", "company", false, false);
        let mut tx = pool.raw().begin().await.unwrap();
        assert!(matches!(
            read_list(&mut tx, &plain, &company, "", true, None, 30).await,
            Err(PgTransactionError::Business(e)) if e.code == "HANDBOOK_EDITOR_REQUIRED"
        ));
        assert!(matches!(
            read_editor_options(&mut tx, &plain, &company).await,
            Err(PgTransactionError::Business(e)) if e.code == "HANDBOOK_OWNER_REQUIRED"
        ));
        let drafts = read_list(&mut tx, &editor, &company, "", true, None, 30)
            .await
            .unwrap();
        assert_eq!(drafts.items.len(), 1);
        assert_eq!(drafts.items[0].version, 1);
        assert_eq!(drafts.items[0].revision, 1);
        tx.rollback().await.unwrap();

        let update = run(
            &pool,
            "editor",
            "company",
            "handbook-update-0001",
            "handbook.article.update",
            Some(&id),
            json!({"title":"Policy v2","category":"procedure","body":"Body v2","expectedVersion":1}),
        )
        .await
        .unwrap();
        assert_eq!((update.version, update.revision, update.status), (2, 2, "draft"));
        let conflict = run(
            &pool,
            "editor",
            "company",
            "handbook-stale-0001",
            "handbook.article.update",
            Some(&id),
            json!({"title":"Stale","category":"policy","body":"Stale body","expectedVersion":1}),
        )
        .await;
        assert!(matches!(conflict, Err(PgTransactionError::Business(e)) if e.code == "VERSION_CONFLICT"));
        let stale_receipts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='handbook-stale-0001'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(stale_receipts, 0);

        let publish = run(
            &pool,
            "editor",
            "company",
            "handbook-publish-0001",
            "handbook.article.publish",
            Some(&id),
            json!({"action":"publish","expectedVersion":2}),
        )
        .await
        .unwrap();
        assert_eq!((publish.version, publish.revision, publish.status), (3, 2, "published"));
        let publish_replay = run(
            &pool,
            "editor",
            "company",
            "handbook-publish-0001",
            "handbook.article.publish",
            Some(&id),
            json!({"action":"publish","expectedVersion":2}),
        )
        .await
        .unwrap();
        assert_eq!(publish_replay, publish);

        let other = CompanyId::new("other").unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        let published = read_article_detail(&mut tx, &actor("editor", "company", true, false), &company, &id, false, None)
            .await
            .unwrap();
        assert_eq!(published.revision.revision, 2);
        assert_eq!(published.revision.title, "Policy v2");
        assert_eq!(published.revision.published_by.as_deref(), Some("editor"));
        assert_eq!(published.history.len(), 1);
        assert_eq!(published.history[0].revision, 2);
        assert!(matches!(
            read_article_detail(&mut tx, &actor("other-editor", "other", true, false), &other, &id, false, None).await,
            Err(PgTransactionError::Business(e)) if e.code == "ARTICLE_NOT_FOUND"
        ));
        let list = read_list(&mut tx, &actor("editor", "company", true, false), &company, "", false, None, 30).await.unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].revision, 2);
        assert_eq!(list.items[0].version, 2);
        let none = read_list(&mut tx, &actor("other-editor", "other", true, false), &other, "", false, None, 30).await.unwrap();
        assert!(none.items.is_empty());
        tx.rollback().await.unwrap();

        let archive = run(
            &pool,
            "editor",
            "company",
            "handbook-archive-0001",
            "handbook.article.archive",
            Some(&id),
            json!({"action":"archive","expectedVersion":3}),
        )
        .await
        .unwrap();
        assert_eq!((archive.version, archive.status), (4, "archived"));
        let mut tx = pool.raw().begin().await.unwrap();
        assert!(matches!(
            read_article_detail(&mut tx, &actor("editor", "company", true, false), &company, &id, false, None).await,
            Err(PgTransactionError::Business(e)) if e.code == "ARTICLE_NOT_FOUND"
        ));
        let editing = read_article_detail(&mut tx, &actor("editor", "company", true, false), &company, &id, true, None)
            .await
            .unwrap();
        assert_eq!(editing.article.archived, 1);
        assert_eq!(editing.history.len(), 2);
        tx.rollback().await.unwrap();

        let restore = run(
            &pool,
            "editor",
            "company",
            "handbook-restore-0001",
            "handbook.article.restore",
            Some(&id),
            json!({"action":"restore","expectedVersion":4}),
        )
        .await
        .unwrap();
        assert_eq!((restore.version, restore.status), (5, "restored"));
        let mut tx = pool.raw().begin().await.unwrap();
        assert_eq!(
            read_list(&mut tx, &actor("editor", "company", true, false), &company, "", false, None, 30).await.unwrap().items.len(),
            1
        );
        tx.rollback().await.unwrap();

        let evidence: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id LIKE 'handbook-%'),(SELECT count(*) FROM audit_events WHERE command_id LIKE 'handbook-%'),(SELECT count(*) FROM outbox_events WHERE command_id LIKE 'handbook-%')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        // create, update, publish, archive and restore each commit exactly one
        // receipt/audit/outbox row; replays and the stale conflict add none.
        assert_eq!(evidence, (5, 5, 5));
        let stored: (i64, i64, Option<i64>, i64) = sqlx::query_as(
            "SELECT version,draft_revision,published_revision,archived FROM handbook_articles WHERE id=$1 AND company_id='company'",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored, (5, 2, Some(2), 0));
    })
    .await;
}

#[tokio::test]
async fn late_tampering_and_current_authority_revocation_roll_back_every_owner_effect() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let tampered = run_observed(
            &pool,
            "editor",
            "company",
            "handbook-tamper-0001",
            "handbook.article.create",
            None,
            json!({"title":"Tampered draft","body":"Body"}),
            true,
        )
        .await;
        assert!(
            matches!(tampered, Err(PgTransactionError::Business(ref e)) if e.code == "HANDBOOK_SOURCE_CHANGED")
        );
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM handbook_articles WHERE id='article-handbook-tamper-0001'),(SELECT count(*) FROM handbook_revisions WHERE article_id='article-handbook-tamper-0001'),(SELECT count(*) FROM command_receipts WHERE command_id='handbook-tamper-0001'),(SELECT count(*) FROM outbox_events WHERE command_id='handbook-tamper-0001')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0, 0));

        let create = run(
            &pool,
            "editor",
            "company",
            "handbook-authority-0001",
            "handbook.article.create",
            None,
            json!({"title":"Authority draft","body":"Body"}),
        )
        .await
        .unwrap();
        assert_eq!(create.version, 1);
        sqlx::query("UPDATE handbook_test_authority SET editor=0 WHERE company_id='company' AND user_id='editor'")
            .execute(pool.raw())
            .await
            .unwrap();
        let denied = run(
            &pool,
            "editor",
            "company",
            "handbook-revoked-0001",
            "handbook.article.update",
            Some("article-handbook-authority-0001"),
            json!({"title":"Revoked update","body":"Body","expectedVersion":1}),
        )
        .await;
        assert!(
            matches!(denied, Err(PgTransactionError::Business(ref e)) if e.code == "HANDBOOK_EDITOR_REQUIRED")
        );
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='handbook-revoked-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}

#[tokio::test]
async fn editor_appointment_requires_owner_exact_version_and_current_membership() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let denied = run(
            &pool,
            "plain",
            "company",
            "handbook-editor-grant",
            "handbook.editor.update",
            None,
            json!({"userId":"target","active":true,"expectedVersion":0}),
        )
        .await;
        assert!(
            matches!(denied, Err(PgTransactionError::Business(ref e)) if e.code == "HANDBOOK_OWNER_REQUIRED")
        );
        let grant = run(
            &pool,
            "owner",
            "company",
            "handbook-editor-grant",
            "handbook.editor.update",
            None,
            json!({"userId":"target","active":true,"expectedVersion":0}),
        )
        .await
        .unwrap();
        assert_eq!(
            grant,
            Receipt {
                id: "target".into(),
                version: 1,
                revision: 0,
                status: "active"
            }
        );
        let replay = run(
            &pool,
            "owner",
            "company",
            "handbook-editor-grant",
            "handbook.editor.update",
            None,
            json!({"userId":"target","active":true,"expectedVersion":0}),
        )
        .await
        .unwrap();
        assert_eq!(replay, grant);
        let options = {
            let mut tx = pool.raw().begin().await.unwrap();
            let options = read_editor_options(
                &mut tx,
                &actor("owner", "company", true, true),
                &CompanyId::new("company").unwrap(),
            )
                .await
                .unwrap();
            tx.rollback().await.unwrap();
            options
        };
        assert_eq!(options.version, 1);
        assert_eq!(options.editors.len(), 1);
        assert_eq!(options.editors[0].user_id, "target");

        let missing = run(
            &pool,
            "owner",
            "company",
            "handbook-editor-missing",
            "handbook.editor.update",
            None,
            json!({"userId":"missing","active":true,"expectedVersion":1}),
        )
        .await;
        assert!(
            matches!(missing, Err(PgTransactionError::Business(ref e)) if e.code == "EDITOR_MEMBER_REQUIRED")
        );
        let revoke = run(
            &pool,
            "owner",
            "company",
            "handbook-editor-revoke",
            "handbook.editor.update",
            None,
            json!({"userId":"target","active":false,"expectedVersion":1}),
        )
        .await
        .unwrap();
        assert_eq!((revoke.version, revoke.status), (2, "inactive"));
        let editors: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM handbook_editors WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(editors, 0);
        let evidence: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id LIKE 'handbook-editor-%'),(SELECT count(*) FROM audit_events WHERE command_id LIKE 'handbook-editor-%'),(SELECT count(*) FROM outbox_events WHERE command_id LIKE 'handbook-editor-%')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, (2, 2, 2));
    })
    .await;
}

#[tokio::test]
async fn concurrent_updates_with_the_same_expected_version_commit_once() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let id = article_id("handbook-concurrent-0001");
        run(
            &pool,
            "editor",
            "company",
            "handbook-concurrent-0001",
            "handbook.article.create",
            None,
            json!({"title":"Concurrent","body":"Body"}),
        )
        .await
        .unwrap();
        let left = run(
            &pool,
            "editor",
            "company",
            "handbook-concurrent-a",
            "handbook.article.update",
            Some(&id),
            json!({"title":"Left","body":"Left body","expectedVersion":1}),
        );
        let right = run(
            &pool,
            "editor",
            "company",
            "handbook-concurrent-b",
            "handbook.article.update",
            Some(&id),
            json!({"title":"Right","body":"Right body","expectedVersion":1}),
        );
        let (left, right) = tokio::join!(left, right);
        let successes = [&left, &right].iter().filter(|result| result.is_ok()).count();
        assert_eq!(successes, 1, "left={left:?} right={right:?}");
        let failed = if left.is_err() { &left } else { &right };
        assert!(
            matches!(failed, Err(PgTransactionError::Business(e)) if e.code == "VERSION_CONFLICT"),
            "unexpected loser: {failed:?}"
        );
        let state: (i64, i64) = sqlx::query_as(
            "SELECT version,draft_revision FROM handbook_articles WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state, (2, 2));
        let revisions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM handbook_revisions WHERE article_id=$1")
                .bind(&id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(revisions, 2);
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ('handbook-concurrent-a','handbook-concurrent-b')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 1);
    })
    .await;
}
