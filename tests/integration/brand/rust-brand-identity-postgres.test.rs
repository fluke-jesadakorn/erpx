//! Native Brand identity lifecycle over a leased PostgreSQL database: draft
//! CAS, approved category locks, independent publication, media guards,
//! rollback, cancellation, Company isolation, replay and retained evidence.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_brand::public::identity::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, UserId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
use serde_json::{Value, json};

const NOW: i64 = 1_000;

fn command_actor(company: &str, user: &str, assignment: &str, role: &str) -> CommandActor {
    CommandActor {
        user_id: UserId::new(user).unwrap(),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: role.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: CommandField::Omitted,
        department_id: CommandField::Omitted,
        authority: CommandField::Value(WorkAuthority {
            session_id: "session".into(),
            revision: 1,
            assignment_version: 1,
        }),
        iam: CommandField::Value(
            CommandJson::parse(
                r#"{"roleRevision":1,"identityVersion":1,"permissions":[],"grantablePermissions":[],"principalId":"principal","personId":null,"personUserIds":[],"freshUntil":null}"#,
            )
            .unwrap(),
        ),
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    }
}

fn actor(
    company: &str,
    user: &str,
    assignment: &str,
    role: &str,
    permissions: &[&str],
    modules: &[&str],
) -> BrandActor {
    BrandActor::new(
        "h001",
        command_actor(company, user, assignment, role),
        modules.iter().map(|value| (*value).to_owned()).collect(),
        permissions.iter().map(|value| (*value).to_owned()).collect(),
    )
}

#[derive(Clone)]
struct People {
    owner: BrandActor,
    head: BrandActor,
    reviewer: BrandActor,
    claimant: BrandActor,
    foreign: BrandActor,
}

fn people() -> People {
    People {
        owner: actor(
            "company",
            "owner",
            "assign-owner",
            "holding_owner",
            &["organization.manage", "company.owner", "records.read"],
            &["brand"],
        ),
        head: actor(
            "company",
            "head",
            "assign-head",
            "hr_admin",
            &["organization.manage", "records.read"],
            &["brand"],
        ),
        reviewer: actor(
            "company",
            "reviewer",
            "assign-reviewer",
            "hr_admin",
            &["organization.manage", "records.read"],
            &["brand"],
        ),
        claimant: actor(
            "company",
            "claimant",
            "assign-claimant",
            "employee",
            &["records.read"],
            &["brand"],
        ),
        foreign: actor(
            "foreign",
            "owner",
            "assign-foreign",
            "holding_owner",
            &["organization.manage", "company.owner", "records.read"],
            &["brand"],
        ),
    }
}

fn identity(short_name: &str, accent: &str, logo: Option<&str>) -> Value {
    json!({
        "identity": {"shortName": short_name, "tagline": "Approved company identity"},
        "visual": {"accentColor": accent, "logoPublicationId": logo},
    })
}

fn draft(short_name: &str, accent: &str, logo: Option<&str>, published: i64) -> Value {
    json!({
        "expectedPublishedVersion": published,
        "identity": identity(short_name, accent, logo),
        "reason": "Reviewable identity draft with approved values",
    })
}

fn code(error: BrandError) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        PgTransactionError::Persistence(error) => {
            panic!("unexpected persistence failure: {error}")
        }
    }
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status) VALUES\
         ('company','company','acme','Acme',NULL,'company','active'),\
         ('foreign','company','foreign','Foreign',NULL,'foreign','active');\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES\
         ('assign-owner','owner','holding_owner','[\"brand\"]','company',0,9007199254740000,'holding'),\
         ('assign-head','head','hr_admin','[\"brand\"]','company',0,9007199254740000,'holding'),\
         ('assign-reviewer','reviewer','hr_admin','[\"brand\"]','company',0,9007199254740000,'holding'),\
         ('assign-claimant','claimant','employee','[\"brand\"]','company',0,9007199254740000,'holding'),\
         ('assign-foreign','owner','holding_owner','[\"brand\"]','foreign',0,9007199254740000,'holding'),\
         ('assign-orphan','orphan','hr_admin','[\"brand\"]','orphan',0,9007199254740000,'holding');\
         INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES\
         ('set-logo','company','company_storefront_brand','company','publication','logo',1,1,'public','[]','owner',1,1),\
         ('set-foreign','foreign','company_storefront_brand','foreign','publication','logo',1,1,'public','[]','owner',1,1);\
         INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES\
         ('link-logo','set-logo','draft','owner',1,9007199254740000),\
         ('link-foreign','set-foreign','draft','owner',1,9007199254740000);\
         INSERT INTO attachment_publications(id,company_id,source_link_id,target_type,target_id,derivative_object_key,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,revision,state,created_by,created_at,approved_by,approved_at) VALUES\
         ('logo-approved','company','link-logo','company_storefront_brand','company','logo-approved.webp','image/webp','checksum',10,'Approved logo',1,'approved','owner',1,'reviewer',2),\
         ('logo-foreign','foreign','link-foreign','company_storefront_brand','foreign','logo-foreign.webp','image/webp','checksum',10,'Foreign logo',1,'approved','owner',1,'reviewer',2);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn save(
    pool: &DatabasePool,
    actor: &BrandActor,
    raw: Value,
    key: &str,
    now: i64,
) -> Result<Value, BrandError> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let mut ports = PgBrandIdentity::new(c, &actor, now);
                save_brand_identity_draft(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn publish(
    pool: &DatabasePool,
    actor: &BrandActor,
    draft_id: &str,
    raw: Value,
    key: &str,
    now: i64,
) -> Result<Value, BrandError> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            let draft_id = draft_id.to_owned();
            Box::pin(async move {
                let mut ports = PgBrandIdentity::new(c, &actor, now);
                publish_brand_identity(&mut ports, &draft_id, &raw, &key).await
            })
        })
        .await
}

async fn rollback(
    pool: &DatabasePool,
    actor: &BrandActor,
    raw: Value,
    key: &str,
    now: i64,
) -> Result<Value, BrandError> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let mut ports = PgBrandIdentity::new(c, &actor, now);
                rollback_brand_identity(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn change_lock(
    pool: &DatabasePool,
    actor: &BrandActor,
    raw: Value,
    key: &str,
    now: i64,
) -> Result<Value, BrandError> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let mut ports = PgBrandIdentity::new(c, &actor, now);
                change_brand_category_lock(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn cancel(
    pool: &DatabasePool,
    actor: &BrandActor,
    draft_id: &str,
    raw: Value,
    key: &str,
    now: i64,
) -> Result<Value, BrandError> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            let draft_id = draft_id.to_owned();
            Box::pin(async move {
                let mut ports = PgBrandIdentity::new(c, &actor, now);
                cancel_brand_draft(&mut ports, &draft_id, &raw, &key).await
            })
        })
        .await
}

async fn workspace(
    pool: &DatabasePool,
    actor: &BrandActor,
    now: i64,
) -> Result<BrandIdentityWorkspace, BrandError> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut ports = PgBrandIdentityRead::new(c, &actor, now);
                read_brand_identity_workspace(&mut ports).await
            })
        })
        .await
}

async fn scalar<T>(pool: &DatabasePool, sql: &'static str) -> T
where
    T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
{
    sqlx::query_scalar(sql).fetch_one(pool.raw()).await.unwrap()
}

async fn exercise(pool: &DatabasePool, pool_people: &People) {
    // --- draft create, replay and CAS ---
    let storefront_before: Option<(String, String)> = sqlx::query_as(
        "SELECT short_name,accent_color FROM storefront_app_brands WHERE company_id='company'",
    )
    .fetch_optional(pool.raw())
    .await
    .unwrap();
    assert!(
        storefront_before.is_none(),
        "the fixture starts without a storefront brand row"
    );
    let first = save(
        pool,
        &pool_people.owner,
        draft("Engineering demo", "#224488", None, 0),
        "brand-draft-0001",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        first,
        json!({"id": "brand-draft-brand-draft-0001", "status": "draft", "version": 1})
    );
    assert_eq!(
        save(
            pool,
            &pool_people.owner,
            draft("Engineering demo", "#224488", None, 0),
            "brand-draft-0001",
            NOW,
        )
        .await
        .unwrap(),
        first,
        "an exact replay returns the retained result"
    );
    let draft_effects: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM brand_identity_drafts WHERE company_id='company'),\
         (SELECT count(*) FROM brand_identity_versions WHERE company_id='company'),\
         (SELECT count(*) FROM brand_identity_events WHERE company_id='company')",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    assert_eq!(
        draft_effects,
        (1, 0, 1),
        "a draft save and its replay create exactly one draft and one event without publishing"
    );
    let storefront_after_draft: Option<(String, String)> = sqlx::query_as(
        "SELECT short_name,accent_color FROM storefront_app_brands WHERE company_id='company'",
    )
    .fetch_optional(pool.raw())
    .await
    .unwrap();
    assert_eq!(
        storefront_after_draft, storefront_before,
        "an unpublished draft never changes the storefront projection"
    );
    assert_eq!(
        code(
            save(
                pool,
                &pool_people.owner,
                draft("Different name", "#224488", None, 0),
                "brand-draft-0001",
                NOW,
            )
            .await
            .unwrap_err()
        ),
        "IDEMPOTENCY_CONFLICT"
    );
    let after_conflict: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM brand_identity_drafts WHERE company_id='company'),\
         (SELECT count(*) FROM brand_identity_versions WHERE company_id='company'),\
         (SELECT count(*) FROM brand_identity_events WHERE company_id='company')",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    assert_eq!(
        after_conflict, draft_effects,
        "a changed-body replay conflict leaves the draft and evidence unchanged"
    );
    let initial = workspace(pool, &pool_people.owner, NOW + 100).await.unwrap();
    assert_eq!(initial.drafts.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&initial.drafts[0].identity_json).unwrap(),
        identity("Engineering demo", "#224488", None),
        "the retained draft row exposes the exact canonical identity payload"
    );
    assert!(initial.active.is_none());
    assert_eq!(initial.initial.identity.short_name, "Acme");
    assert_eq!(initial.initial.visual.accent_color, "#3659c9");
    assert!(initial.can_manage && initial.can_lock);
    assert_eq!(initial.media.len(), 1, "approved Company media is listed");
    assert_eq!(initial.media[0].id, "logo-approved");
    let scoped = workspace(pool, &pool_people.claimant, NOW + 100)
        .await
        .unwrap();
    assert!(scoped.drafts.is_empty(), "non-managers only see their own drafts");
    assert!(!scoped.can_manage && !scoped.can_lock);
    assert!(scoped.history.is_empty());

    let saved = save(
        pool,
        &pool_people.owner,
        json!({
            "id": "brand-draft-brand-draft-0001",
            "expectedVersion": 1,
            "expectedPublishedVersion": 0,
            "identity": identity("Reviewed demo name", "#224488", None),
            "reason": "Correct the name before publication",
        }),
        "brand-save-0002",
        NOW + 200,
    )
    .await
    .unwrap();
    assert_eq!(saved["version"], 2);
    assert_eq!(
        code(
            save(
                pool,
                &pool_people.owner,
                json!({
                    "id": "brand-draft-brand-draft-0001",
                    "expectedVersion": 1,
                    "expectedPublishedVersion": 0,
                    "identity": identity("Obsolete name", "#224488", None),
                    "reason": "Stale editor must not overwrite",
                }),
                "brand-save-0003",
                NOW + 300,
            )
            .await
            .unwrap_err()
        ),
        "CONFLICT"
    );

    // --- independent publication ---
    let publish_input = || {
        json!({
            "expectedVersion": 2,
            "expectedPublishedVersion": 0,
            "reason": "Independent review of customer-facing identity",
        })
    };
    assert_eq!(
        code(
            publish(
                pool,
                &pool_people.owner,
                "brand-draft-brand-draft-0001",
                publish_input(),
                "brand-publish-0004",
                NOW + 400,
            )
            .await
            .unwrap_err()
        ),
        "BRAND_INDEPENDENT_PUBLICATION"
    );
    let denied_effects: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM brand_identity_versions WHERE company_id='company'),\
         (SELECT count(*) FROM brand_identity_active WHERE company_id='company'),\
         (SELECT count(*) FROM command_receipts WHERE command_id='brand-publish-0004'),\
         (SELECT count(*) FROM audit_events WHERE command_id='brand-publish-0004'),\
         (SELECT count(*) FROM outbox_events WHERE command_id='brand-publish-0004')",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    assert_eq!(
        denied_effects,
        (0, 0, 0, 0, 0),
        "a maker self-publication attempt writes no version, active identity or command effects"
    );
    let published = publish(
        pool,
        &pool_people.head,
        "brand-draft-brand-draft-0001",
        publish_input(),
        "brand-publish-0004",
        NOW + 500,
    )
    .await
    .unwrap();
    assert_eq!(
        published,
        json!({
            "id": "brand-draft-brand-draft-0001",
            "status": "published",
            "version": 3,
            "publishedId": "brand-version-brand-publish-0004",
            "revision": 1,
            "publishedVersion": 1,
        })
    );
    assert_eq!(
        publish(
            pool,
            &pool_people.head,
            "brand-draft-brand-draft-0001",
            publish_input(),
            "brand-publish-0004",
            NOW + 500,
        )
        .await
        .unwrap(),
        published
    );
    let after = workspace(pool, &pool_people.owner, NOW + 600)
        .await
        .unwrap();
    let active = after.active.as_ref().expect("active identity");
    assert_eq!(active.revision, 1);
    assert_eq!(active.active_version, 1);
    assert_eq!(active.identity.identity.short_name, "Reviewed demo name");
    assert_eq!(after.versions.len(), 1);
    assert_eq!(after.drafts[0].status, "published");
    assert_eq!(after.drafts[0].version, 3);
    assert_eq!(
        after
            .history
            .iter()
            .map(|event| event.action.as_str())
            .collect::<Vec<_>>(),
        ["publish", "save", "draft"]
    );
    let storefront: (String, String) =
        sqlx::query_as("SELECT short_name,accent_color FROM storefront_app_brands WHERE company_id='company'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
    assert_eq!(storefront, ("Reviewed demo name".into(), "#224488".into()));
    let active_row: (String, i64) = sqlx::query_as(
        "SELECT identity_version_id,version FROM brand_identity_active WHERE company_id='company'",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    assert_eq!(
        active_row,
        ("brand-version-brand-publish-0004".into(), 1),
        "the active pointer selects the exact immutable published version"
    );
    let receipt: String =
        scalar(pool, "SELECT operation FROM command_receipts WHERE command_id='brand-publish-0004'").await;
    assert_eq!(receipt, "brand.identity.publish");
    let audit: i64 = scalar(
        pool,
        "SELECT count(*) FROM audit_events WHERE command_id='brand-publish-0004' AND action='brand.identity.publish'",
    )
    .await;
    assert_eq!(audit, 1);
    let outbox: i64 = scalar(
        pool,
        "SELECT count(*) FROM outbox_events WHERE command_id='brand-publish-0004' AND event_type='brand.identity.publish'",
    )
    .await;
    assert_eq!(outbox, 1);
    assert_eq!(
        code(
            publish(
                pool,
                &pool_people.head,
                "brand-draft-brand-draft-0001",
                publish_input(),
                "brand-publish-0005",
                NOW + 700,
            )
            .await
            .unwrap_err()
        ),
        "BRAND_DRAFT_UNAVAILABLE"
    );

    assert_eq!(
        code(
            save(
                pool,
                &pool_people.head,
                draft("Stale publication base", "#224488", None, 0),
                "brand-publish-0005b",
                NOW + 750,
            )
            .await
            .unwrap_err()
        ),
        "BRAND_PUBLICATION_CHANGED"
    );

    // --- approved media guards ---
    save(
        pool,
        &pool_people.head,
        draft("Approved logo identity", "#224488", Some("logo-approved"), 1),
        "brand-media-0006",
        NOW + 800,
    )
    .await
    .unwrap();
    for (key, logo, now) in [
        ("brand-media-0007", "logo-foreign", NOW + 900),
        ("brand-media-0008", "missing-publication", NOW + 1_000),
    ] {
        assert_eq!(
            code(
                save(
                    pool,
                    &pool_people.head,
                    draft("Rejected logo identity", "#224488", Some(logo), 1),
                    key,
                    now,
                )
                .await
                .unwrap_err()
            ),
            "CONFLICT",
            "unapproved media {logo} is rejected"
        );
        let rejected: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM brand_identity_drafts WHERE id=$1),\
             (SELECT count(*) FROM brand_identity_events WHERE id=$2)",
        )
        .bind(format!("brand-draft-{key}"))
        .bind(format!("brand-event-{key}"))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            rejected,
            (0, 0),
            "a rejected media save writes no draft or event"
        );
    }

    // --- category locks ---
    let lock_input = |locked: bool, expected: i64, published: i64, reason: &str| {
        json!({
            "category": "visual",
            "locked": locked,
            "expectedVersion": expected,
            "expectedPublishedVersion": published,
            "reason": reason,
        })
    };
    assert_eq!(
        code(
            change_lock(
                pool,
                &pool_people.head,
                lock_input(true, 0, 1, "Reviewer cannot release the owner lock"),
                "brand-lock-0009",
                NOW + 1_100,
            )
            .await
            .unwrap_err()
        ),
        "PERMISSION_REQUIRED"
    );
    let locked = change_lock(
        pool,
        &pool_people.owner,
        lock_input(true, 0, 1, "Protect independently approved colors"),
        "brand-lock-0009",
        NOW + 1_200,
    )
    .await
    .unwrap();
    assert_eq!(
        locked,
        json!({"category": "visual", "locked": true, "version": 1})
    );
    assert_eq!(
        change_lock(
            pool,
            &pool_people.owner,
            lock_input(true, 0, 1, "Protect independently approved colors"),
            "brand-lock-0009",
            NOW + 1_200,
        )
        .await
        .unwrap(),
        locked
    );
    assert_eq!(
        code(
            save(
                pool,
                &pool_people.head,
                draft("New palette", "#778899", None, 1),
                "brand-lock-0010",
                NOW + 1_300,
            )
            .await
            .unwrap_err()
        ),
        "BRAND_CATEGORY_LOCKED"
    );
    let locked_rejection: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM brand_identity_drafts WHERE id='brand-draft-brand-lock-0010'),\
         (SELECT count(*) FROM brand_identity_events WHERE id='brand-event-brand-lock-0010')",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    assert_eq!(
        locked_rejection,
        (0, 0),
        "a locked-category save writes no draft or event"
    );
    let permitted = save(
        pool,
        &pool_people.head,
        draft("New short name only", "#224488", None, 1),
        "brand-lock-0011",
        NOW + 1_400,
    )
    .await
    .unwrap();
    assert_eq!(permitted["version"], 1);
    publish(
        pool,
        &pool_people.reviewer,
        "brand-draft-brand-lock-0011",
        json!({
            "expectedVersion": 1,
            "expectedPublishedVersion": 1,
            "reason": "Name revised while approved visual values remain locked",
        }),
        "brand-lock-0012",
        NOW + 1_500,
    )
    .await
    .unwrap();
    let active = workspace(pool, &pool_people.owner, NOW + 1_600)
        .await
        .unwrap()
        .active
        .unwrap();
    assert_eq!(active.revision, 2);
    assert_eq!(active.active_version, 2);
    assert_eq!(active.identity.visual.accent_color, "#224488");
    change_lock(
        pool,
        &pool_people.owner,
        lock_input(false, 1, 2, "Owner reviewed the planned new palette"),
        "brand-lock-0013",
        NOW + 1_700,
    )
    .await
    .unwrap();
    let changed = save(
        pool,
        &pool_people.head,
        draft("Unlocked palette", "#778899", None, 2),
        "brand-lock-0014",
        NOW + 1_800,
    )
    .await
    .unwrap();
    assert_eq!(changed["version"], 1);
    publish(
        pool,
        &pool_people.reviewer,
        "brand-draft-brand-lock-0014",
        json!({
            "expectedVersion": 1,
            "expectedPublishedVersion": 2,
            "reason": "Independently approved the unlocked visual revision",
        }),
        "brand-lock-0015",
        NOW + 1_900,
    )
    .await
    .unwrap();
    let active = workspace(pool, &pool_people.owner, NOW + 2_000)
        .await
        .unwrap();
    assert_eq!(active.active.unwrap().identity.visual.accent_color, "#778899");
    assert_eq!(
        active.locks[0].category, "visual",
        "the unlocked category remains retained evidence"
    );
    assert_eq!(active.locks[0].version, 2);
    assert!(!active.locks[0].locked);
}

#[tokio::test]
async fn retained_draft_lock_publication_and_media_behaviour() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let people = people();
        exercise(&pool, &people).await;
    })
    .await;
}

fn rollback_input(published_id: &str, expected: i64, reason: &str) -> Value {
    json!({
        "publishedId": published_id,
        "expectedPublishedVersion": expected,
        "reason": reason,
    })
}

#[tokio::test]
async fn rollback_cas_cancel_and_immutable_evidence() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let people = people();
        let first = save(
            &pool,
            &people.owner,
            draft("First identity", "#224488", None, 0),
            "brand-rollback-0001",
            1_000,
        )
        .await
        .unwrap();
        let first_id = "brand-draft-brand-rollback-0001".to_owned();
        publish(
            &pool,
            &people.head,
            &first_id,
            json!({
                "expectedVersion": 1,
                "expectedPublishedVersion": 0,
                "reason": "First identity independently reviewed",
            }),
            "brand-rollback-0002",
            1_100,
        )
        .await
        .unwrap();
        let first_published = "brand-version-brand-rollback-0002".to_owned();
        assert_eq!(first["version"], 1);
        save(
            &pool,
            &people.owner,
            draft("Second identity", "#667788", None, 1),
            "brand-rollback-0003",
            1_200,
        )
        .await
        .unwrap();
        publish(
            &pool,
            &people.head,
            "brand-draft-brand-rollback-0003",
            json!({
                "expectedVersion": 1,
                "expectedPublishedVersion": 1,
                "reason": "Second identity independently reviewed",
            }),
            "brand-rollback-0004",
            1_300,
        )
        .await
        .unwrap();
        change_lock(
            &pool,
            &people.owner,
            json!({
                "category": "visual",
                "locked": true,
                "expectedVersion": 0,
                "expectedPublishedVersion": 2,
                "reason": "Protect the new approved palette",
            }),
            "brand-rollback-0005",
            1_400,
        )
        .await
        .unwrap();
        let active_before_locked_rollback: (String, i64) = sqlx::query_as(
            "SELECT v.id,a.version FROM brand_identity_active a JOIN brand_identity_versions v \
             ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            active_before_locked_rollback.1, 2,
            "the second publication is active before the blocked restore"
        );
        assert_eq!(
            code(
                rollback(
                    &pool,
                    &people.owner,
                    rollback_input(&first_published, 2, "Restore the previously approved identity"),
                    "brand-rollback-0006",
                    1_500,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_CATEGORY_LOCKED"
        );
        let locked_rollback_effects: (String, i64, i64, i64) = sqlx::query_as(
            "SELECT v.id,(SELECT count(*) FROM command_receipts WHERE command_id='brand-rollback-0006'),\
             (SELECT count(*) FROM audit_events WHERE command_id='brand-rollback-0006'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='brand-rollback-0006') \
             FROM brand_identity_active a JOIN brand_identity_versions v \
             ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            locked_rollback_effects,
            (active_before_locked_rollback.0.clone(), 0, 0, 0),
            "a locked restore leaves the active identity and command effects unchanged"
        );
        change_lock(
            &pool,
            &people.owner,
            json!({
                "category": "visual",
                "locked": false,
                "expectedVersion": 1,
                "expectedPublishedVersion": 2,
                "reason": "Owner reviewed the previous approved palette",
            }),
            "brand-rollback-0007",
            1_600,
        )
        .await
        .unwrap();
        let body = rollback_input(
            &first_published,
            2,
            "Restore the previously approved identity",
        );
        let restored = rollback(
            &pool,
            &people.owner,
            body.clone(),
            "brand-rollback-0008",
            1_700,
        )
        .await
        .unwrap();
        assert_eq!(
            restored,
            json!({
                "id": first_published,
                "revision": 1,
                "publishedVersion": 3,
            })
        );
        assert_eq!(
            rollback(&pool, &people.owner, body.clone(), "brand-rollback-0008", 1_700)
                .await
                .unwrap(),
            restored
        );
        let data = workspace(&pool, &people.owner, 1_800).await.unwrap();
        assert_eq!(data.active.as_ref().unwrap().active_version, 3);
        assert_eq!(data.active.as_ref().unwrap().revision, 1);
        assert_eq!(
            data.active.as_ref().unwrap().identity.identity.short_name,
            "First identity"
        );
        let rolled_back_storefront: (String, String) = sqlx::query_as(
            "SELECT short_name,accent_color FROM storefront_app_brands WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            rolled_back_storefront,
            ("First identity".into(), "#224488".into()),
            "rollback activates the exact retained storefront projection"
        );
        assert_eq!(data.versions.len(), 2);
        assert_eq!(
            data.history
                .iter()
                .filter(|event| event.action == "rollback")
                .count(),
            1
        );
        assert_eq!(
            code(
                rollback(
                    &pool,
                    &people.owner,
                    rollback_input(&first_published, 2, "Stale active version must not restore"),
                    "brand-rollback-0009",
                    1_900,
                )
                .await
                .unwrap_err()
            ),
            "CONFLICT"
        );
        assert_eq!(
            code(
                rollback(
                    &pool,
                    &people.owner,
                    rollback_input("brand-version-missing", 3, "Choose another retained version"),
                    "brand-rollback-0010",
                    2_000,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_VERSION_UNAVAILABLE"
        );
        let rejected_rollbacks: (i64, i64, i64, String) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('brand-rollback-0009','brand-rollback-0010')),\
             (SELECT count(*) FROM audit_events WHERE command_id IN ('brand-rollback-0009','brand-rollback-0010')),\
             (SELECT count(*) FROM outbox_events WHERE command_id IN ('brand-rollback-0009','brand-rollback-0010')),\
             (SELECT v.id FROM brand_identity_active a JOIN brand_identity_versions v \
              ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id='company')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            rejected_rollbacks,
            (0, 0, 0, first_published.clone()),
            "stale or unavailable restores write no effects and keep the restored publication active"
        );

        // Cancellation retains both the draft row and its event.
        save(
            &pool,
            &people.owner,
            draft("Withdrawn identity", "#224488", None, 3),
            "brand-cancel-0011",
            2_100,
        )
        .await
        .unwrap();
        assert_eq!(
            code(
                save(
                    &pool,
                    &people.owner,
                    json!({
                        "id": "brand-draft-brand-cancel-0011",
                        "expectedPublishedVersion": 3,
                        "identity": identity("Missing expected version", "#224488", None),
                        "reason": "Missing expected draft version",
                    }),
                    "brand-cancel-0011b",
                    2_150,
                )
                .await
                .unwrap_err()
            ),
            "EXPECTED_VERSION_REQUIRED"
        );
        let cancelled = cancel(
            &pool,
            &people.owner,
            "brand-draft-brand-cancel-0011",
            json!({"expectedVersion": 1, "reason": "Withdraw the draft before review"}),
            "brand-cancel-0012",
            2_200,
        )
        .await
        .unwrap();
        assert_eq!(
            cancelled,
            json!({"id": "brand-draft-brand-cancel-0011", "status": "cancelled", "version": 2})
        );
        assert_eq!(
            cancel(
                &pool,
                &people.owner,
                "brand-draft-brand-cancel-0011",
                json!({"expectedVersion": 1, "reason": "Withdraw the draft before review"}),
                "brand-cancel-0012",
                2_200,
            )
            .await
            .unwrap(),
            cancelled
        );
        let draft_row = workspace(&pool, &people.owner, 2_300).await.unwrap();
        let cancelled_row = draft_row
            .drafts
            .iter()
            .find(|row| row.id == "brand-draft-brand-cancel-0011")
            .unwrap();
        assert_eq!(cancelled_row.status, "cancelled");
        assert_eq!(cancelled_row.version, 2);
        assert_eq!(
            code(
                save(
                    &pool,
                    &people.owner,
                    json!({
                        "id": "brand-draft-brand-cancel-0011",
                        "expectedVersion": 2,
                        "expectedPublishedVersion": 3,
                        "identity": identity("Cannot overwrite", "#224488", None),
                        "reason": "Cannot overwrite a withdrawn draft",
                    }),
                    "brand-cancel-0013",
                    2_400,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_DRAFT_UNAVAILABLE"
        );
        let cancel_effects: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='brand-cancel-0012'),\
             (SELECT count(*) FROM audit_events WHERE command_id='brand-cancel-0012'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='brand-cancel-0012'),\
             (SELECT count(*) FROM brand_identity_events WHERE record_id='brand-draft-brand-cancel-0011' AND action='cancel')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            cancel_effects,
            (1, 1, 1, 1),
            "cancel replay retains one receipt, audit, outbox and cancel event"
        );
        let unavailable_save_effects: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='brand-cancel-0013'),\
             (SELECT count(*) FROM brand_identity_events WHERE id='brand-event-brand-cancel-0013')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            unavailable_save_effects,
            (0, 0),
            "a withdrawn draft cannot be overwritten and the refusal writes no effects"
        );
        assert!(
            sqlx::query("DELETE FROM brand_identity_events WHERE company_id='company'")
                .execute(pool.raw())
                .await
                .is_err(),
            "identity events are immutable"
        );
        assert!(
            sqlx::query("UPDATE brand_identity_versions SET identity_json='{}'")
                .execute(pool.raw())
                .await
                .is_err(),
            "published versions are immutable"
        );
    })
    .await;
}

#[tokio::test]
async fn company_isolation_and_current_authority() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let people = people();
        save(
            &pool,
            &people.owner,
            draft("Company identity", "#224488", None, 0),
            "brand-scope-0001",
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(
            code(
                save(
                    &pool,
                    &people.claimant,
                    draft("Claimant identity", "#224488", None, 0),
                    "brand-scope-0002",
                    1_100,
                )
                .await
                .unwrap_err()
            ),
            "PERMISSION_REQUIRED"
        );
        let no_module = BrandActor::new(
            "h001",
            command_actor("company", "owner", "assign-owner", "holding_owner"),
            Vec::new(),
            vec!["organization.manage".into()],
        );
        assert_eq!(
            code(
                save(
                    &pool,
                    &no_module,
                    draft("Without module", "#224488", None, 0),
                    "brand-scope-0003",
                    1_200,
                )
                .await
                .unwrap_err()
            ),
            "MODULE_FORBIDDEN"
        );
        // A foreign Company actor cannot edit this Company's draft by identifier.
        assert_eq!(
            code(
                save(
                    &pool,
                    &people.foreign,
                    json!({
                        "id": "brand-draft-brand-scope-0001",
                        "expectedVersion": 1,
                        "expectedPublishedVersion": 0,
                        "identity": identity("Foreign overwrite", "#224488", None),
                        "reason": "Different Company must not edit this draft",
                    }),
                    "brand-scope-0004",
                    1_300,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_DRAFT_UNAVAILABLE"
        );
        // A Company actor cannot borrow a foreign assignment.
        let borrowed = actor(
            "foreign",
            "owner",
            "assign-owner",
            "holding_owner",
            &["organization.manage", "company.owner"],
            &["brand"],
        );
        assert_eq!(
            code(
                save(
                    &pool,
                    &borrowed,
                    draft("Borrowed assignment", "#224488", None, 0),
                    "brand-scope-0005",
                    1_400,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_AUTHORITY_CHANGED"
        );
        // The foreign Company retains its own isolated lifecycle.
        save(
            &pool,
            &people.foreign,
            draft("Foreign identity", "#123456", None, 0),
            "brand-scope-0006",
            1_500,
        )
        .await
        .unwrap();
        let company = workspace(&pool, &people.owner, 1_600).await.unwrap();
        assert_eq!(company.drafts.len(), 1);
        assert_eq!(company.drafts[0].company_id, "company");
        let foreign = workspace(&pool, &people.foreign, 1_600).await.unwrap();
        assert_eq!(foreign.drafts.len(), 1);
        assert_eq!(foreign.drafts[0].company_id, "foreign");
        // A foreign actor still cannot publish this Company's draft.
        assert_eq!(
            code(
                publish(
                    &pool,
                    &people.foreign,
                    "brand-draft-brand-scope-0001",
                    json!({
                        "expectedVersion": 1,
                        "expectedPublishedVersion": 0,
                        "reason": "Different Company cannot publish",
                    }),
                    "brand-scope-0007",
                    1_700,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_DRAFT_UNAVAILABLE"
        );
        assert_eq!(
            code(
                change_lock(
                    &pool,
                    &people.foreign,
                    json!({
                        "category": "visual",
                        "locked": true,
                        "expectedVersion": 0,
                        "expectedPublishedVersion": 1,
                        "reason": "No active published identity exists yet",
                    }),
                    "brand-scope-0008a",
                    1_850,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_PUBLISHED_IDENTITY_REQUIRED"
        );
        sqlx::query("UPDATE organization_nodes SET status='suspended' WHERE id='foreign'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            code(
                save(
                    &pool,
                    &people.foreign,
                    draft("Inactive Company", "#123456", None, 0),
                    "brand-scope-0009",
                    1_860,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_MANAGEMENT_FORBIDDEN"
        );
        sqlx::query("UPDATE organization_nodes SET status='active' WHERE id='foreign'")
            .execute(pool.raw())
            .await
            .unwrap();
        let orphan = actor(
            "orphan",
            "orphan",
            "assign-orphan",
            "hr_admin",
            &["organization.manage", "records.read"],
            &["brand"],
        );
        assert_eq!(
            code(workspace(&pool, &orphan, 1_870).await.unwrap_err()),
            "COMPANY_REQUIRED"
        );
        sqlx::query("UPDATE assignments SET revoked_at=1000 WHERE id='assign-owner'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            code(workspace(&pool, &people.owner, 1_800).await.unwrap_err()),
            "BRAND_AUTHORITY_CHANGED"
        );
        assert_eq!(
            code(
                save(
                    &pool,
                    &people.owner,
                    draft("Revoked identity", "#224488", None, 0),
                    "brand-scope-0010",
                    1_900,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_AUTHORITY_CHANGED"
        );
        let isolation_effects: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM brand_identity_drafts WHERE company_id='company'),\
             (SELECT count(*) FROM brand_identity_versions WHERE company_id='company'),\
             (SELECT count(*) FROM command_receipts WHERE command_id IN ('brand-scope-0004','brand-scope-0005','brand-scope-0007','brand-scope-0008a','brand-scope-0010')),\
             (SELECT count(*) FROM brand_identity_events WHERE company_id='company' AND id LIKE 'brand-event-brand-scope-00%')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            isolation_effects,
            (1, 0, 0, 1),
            "scope refusals add no draft, version, receipt or event beyond the one legitimate owner draft"
        );
    })
    .await;
}

#[tokio::test]
async fn concurrent_publication_rebases_the_losing_draft() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let people = people();
        save(
            &pool,
            &people.owner,
            draft("Owner draft", "#224488", None, 0),
            "brand-concurrent-0001",
            1_000,
        )
        .await
        .unwrap();
        save(
            &pool,
            &people.head,
            draft("Head draft", "#224488", None, 0),
            "brand-concurrent-0002",
            1_100,
        )
        .await
        .unwrap();
        let owner_publishes = publish(
            &pool,
            &people.head,
            "brand-draft-brand-concurrent-0001",
            json!({
                "expectedVersion": 1,
                "expectedPublishedVersion": 0,
                "reason": "Independent review of owner draft",
            }),
            "brand-concurrent-0003",
            1_200,
        );
        let head_publishes = publish(
            &pool,
            &people.owner,
            "brand-draft-brand-concurrent-0002",
            json!({
                "expectedVersion": 1,
                "expectedPublishedVersion": 0,
                "reason": "Independent review of head draft",
            }),
            "brand-concurrent-0004",
            1_200,
        );
        let (first, second) = tokio::join!(owner_publishes, head_publishes);
        let fulfilled = [first.is_ok(), second.is_ok()]
            .into_iter()
            .filter(|ok| *ok)
            .count();
        assert_eq!(fulfilled, 1, "only one concurrent publication may commit");
        let data = workspace(&pool, &people.owner, 1_300).await.unwrap();
        assert_eq!(data.versions.len(), 1);
        assert_eq!(data.active.as_ref().unwrap().active_version, 1);
        let remaining = data
            .drafts
            .iter()
            .find(|row| row.status == "draft")
            .expect("the losing draft remains editable")
            .clone();
        let reviewer = if remaining.created_by == "owner" {
            &people.head
        } else {
            &people.owner
        };
        assert_eq!(
            code(
                publish(
                    &pool,
                    reviewer,
                    &remaining.id,
                    json!({
                        "expectedVersion": 1,
                        "expectedPublishedVersion": 1,
                        "reason": "Draft still based on obsolete identity",
                    }),
                    "brand-concurrent-0005",
                    1_400,
                )
                .await
                .unwrap_err()
            ),
            "BRAND_DRAFT_REBASE_REQUIRED"
        );
        let maker = if remaining.created_by == "owner" {
            &people.owner
        } else {
            &people.head
        };
        save(
            &pool,
            maker,
            json!({
                "id": remaining.id,
                "expectedVersion": 1,
                "expectedPublishedVersion": 1,
                "identity": serde_json::from_str::<Value>(&remaining.identity_json).unwrap(),
                "reason": "Review differences against current publication",
            }),
            "brand-concurrent-0006",
            1_500,
        )
        .await
        .unwrap();
        publish(
            &pool,
            reviewer,
            &remaining.id,
            json!({
                "expectedVersion": 2,
                "expectedPublishedVersion": 1,
                "reason": "Independently reviewed rebased identity",
            }),
            "brand-concurrent-0007",
            1_600,
        )
        .await
        .unwrap();
        assert_eq!(
            workspace(&pool, &people.owner, 1_700)
                .await
                .unwrap()
                .versions
                .len(),
            2
        );
    })
    .await;
}
