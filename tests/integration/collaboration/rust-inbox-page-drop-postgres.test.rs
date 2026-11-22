//! Postgres proof of the native Inbox page per-row drop mapping.
//!
//! A denied source (403/404 from its owner delegate) disappears from the
//! page instead of failing it — the native mirror of the retained
//! TypeScript per-row catch. Storage/transaction/Unavailable errors still
//! fail the whole page.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    InboxNotificationQuery, InboxScope, InboxTarget, InboxTargetFuture, InboxTargetPort,
    PgInboxTargets, PgInboxUnit,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

struct DropProbe;

impl InboxTargetPort for DropProbe {
    fn target<'a>(
        &'a mut self,
        _connection: &'a mut sqlx::PgConnection,
        _company_id: &'a str,
        _user_id: &'a str,
        source_type: &'a str,
        source_id: &'a str,
        _source_version: i64,
    ) -> InboxTargetFuture<'a> {
        Box::pin(async move {
            match source_type {
                "ok-type" => Ok(Some(InboxTarget {
                    href: Some(format!("/ok/{source_id}")),
                    source: None,
                })),
                "denied-type" => Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "PERMISSION_REQUIRED",
                    "Your selected role does not grant this permission.",
                )),
                "gone-type" => Err(BusinessError::new(
                    FailureKind::NotFound,
                    "GONE_NOT_FOUND",
                    "Gone.",
                )),
                // Unknown projection types drop like the retained
                // TypeScript `else continue`, by exact code (kind stays
                // Unavailable so port misconfiguration still fails loud).
                "mystery-type" => Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "INBOX_TARGET_SOURCE_UNSUPPORTED",
                    "This Inbox source is not part of the retained projection.",
                )),
                _ => Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "PROBE_UNAVAILABLE",
                    "Probe boiling.",
                )),
            }
        })
    }
}

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES \
         ('co','company','co','Co',NULL,'co',NULL,'active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','reader','Reader',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','reader',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','reader','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at) VALUES \
         ('evt-ok','co','key-ok','ok-type','ok-1',1,'work','ok',3), \
         ('evt-denied','co','key-denied','denied-type','denied-1',1,'work','denied',2), \
         ('evt-gone','co','key-gone','gone-type','gone-1',1,'work','gone',1), \
         ('evt-mystery','co','key-mystery','mystery-type','mystery-1',1,'work','mystery',4); \
         INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) VALUES \
         ('del-ok','evt-ok','co','user-1',3), \
         ('del-denied','evt-denied','co','user-1',2), \
         ('del-gone','evt-gone','co','user-1',1), \
         ('del-mystery','evt-mystery','co','user-1',4)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn page(
    pool: DatabasePool,
) -> Result<erpx_server_collaboration::public::inbox::InboxPageData, PgTransactionError<BusinessError>> {
    page_with_limit(pool, 10).await
}

async fn page_with_limit(
    pool: DatabasePool,
    limit: usize,
) -> Result<erpx_server_collaboration::public::inbox::InboxPageData, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool)
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move {
                let scope = InboxScope::new("co", "user-1", "grant-1", 1).map_err(PgTransactionError::Business)?;
                let mut targets = PgInboxTargets::with_delegate(None, None, DropProbe);
                let mut unit = PgInboxUnit::new(connection, scope, &mut targets);
                unit.notifications(InboxNotificationQuery {
                    view: None,
                    category: None,
                    search: String::new(),
                    cursor: None,
                    limit,
                })
                .await
            })
        })
        .await
}

#[tokio::test]
async fn denied_rows_drop_instead_of_failing_the_page() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let data = page(pool.clone()).await.unwrap();
        assert_eq!(data.items.len(), 1, "only the resolved row survives");
        assert_eq!(data.items[0].source_type, "ok-type");
        assert_eq!(data.items[0].source_id, "ok-1");
        assert_eq!(data.items[0].href.as_deref(), Some("/ok/ok-1"));
        assert!(!data.has_more);
    })
    .await;
}

#[tokio::test]
async fn unavailable_rows_still_fail_the_page() {    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at) VALUES \
             ('evt-broken','co','key-broken','broken-type','broken-1',1,'work','broken',4); \
             INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) VALUES \
             ('del-broken','evt-broken','co','user-1',4)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let error = page(pool.clone()).await.expect_err("unavailable must fail");
        match error {
            PgTransactionError::Business(error) => assert_eq!(error.code, "PROBE_UNAVAILABLE"),
            other => panic!("expected the probe business error, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn dropped_rows_refill_across_batches() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // 101 denied rows (newest) plus one visible row (oldest): with the
        // 100-row batch size the visible row only surfaces via refill.
        let mut events = String::from(
            "INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at) VALUES ",
        );
        let mut deliveries = String::from(
            "INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at) VALUES ",
        );
        for i in 0..101 {
            events.push_str(&format!(
                "('evt-d{i}','co','key-d{i}','denied-type','denied-{i}',1,'work','denied',{t}),",
                t = 1000 - i
            ));
            deliveries.push_str(&format!("('del-d{i}','evt-d{i}','co','user-1',{t}),", t = 1000 - i));
        }
        events.push_str("('evt-old','co','key-old','ok-type','old-1',1,'work','old',1);");
        deliveries.push_str("('del-old','evt-old','co','user-1',1);");
        sqlx::raw_sql(Box::leak(format!("{events} {deliveries}").into_boxed_str()) as &'static str)
            .execute(pool.raw())
            .await
            .unwrap();
        let data = page_with_limit(pool.clone(), 10).await.unwrap();
        assert_eq!(data.items.len(), 2, "refill fills past the denied batch");
        assert!(data.items.iter().any(|item| item.source_id == "old-1"));
        assert!(data.items.iter().any(|item| item.source_id == "ok-1"));
        assert!(!data.has_more);
    })
    .await;
}
