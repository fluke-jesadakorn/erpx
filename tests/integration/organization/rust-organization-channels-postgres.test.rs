#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_organization_server::public::organization_nodes::{
    OrganizationChannelEnsureSummary, PgOrganizationNodes, ensure_organization_channels,
};
use erpx_platform_runtime_server::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId};

const NOW: i64 = 1_800_000_000_000;

async fn seed(pool: &erpx_platform_runtime_server::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES \
         ('company','company','acme','Acme',NULL,'company',NULL,'active'), \
         ('branch','branch','north','North','company','company','branch','active'), \
         ('department','department','service','Service','branch','company','branch','active'), \
         ('suspended','department','old','Old','branch','company','branch','suspended'), \
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active')",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn run<T, F>(
    pool: erpx_platform_runtime_server::public::postgres::DatabasePool,
    f: F,
) -> Result<T, PgTransactionError<BusinessError>>
where
    F: for<'c> FnOnce(
            &'c mut sqlx::PgConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T, BusinessError>> + Send + 'c>,
        > + Send
        + 'static,
    T: Send + 'static,
{
    PgUnitOfWork::new(pool)
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move { f(connection).await.map_err(PgTransactionError::Business) })
        })
        .await
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

#[tokio::test]
async fn company_ensure_provisions_team_and_rooms_idempotently_with_name_sync() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let first = run(pool.clone(), move |connection| {
            Box::pin(async move {
                ensure_organization_channels(connection, &company(), NOW)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        assert_eq!(
            first,
            OrganizationChannelEnsureSummary {
                team_id: "org-team-company".into(),
                team_created: true,
                rooms_created: 3,
                rooms_synced: 0,
            }
        );
        let team: (String, String, bool, i64) = sqlx::query_as(
            "SELECT id,name,organization_managed,created_at FROM messaging_teams WHERE id='org-team-company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(team, ("org-team-company".into(), "Organization".into(), true, NOW));
        let mut rooms = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT id,name,organization_node_id,created_at FROM messaging_rooms WHERE company_id='company' ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            rooms
                .iter()
                .map(|row| (row.0.as_str(), row.1.as_str(), row.2.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("org-channel-branch", "North", "branch"),
                ("org-channel-company", "Acme", "company"),
                ("org-channel-department", "Service", "department"),
            ]
        );
        assert!(rooms.iter().all(|row| row.3 == NOW));

        // Second call with a later timestamp creates nothing and rewrites no timestamps.
        let second = run(pool.clone(), move |connection| {
            Box::pin(async move {
                ensure_organization_channels(connection, &company(), NOW + 1_000)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        assert_eq!(
            second,
            OrganizationChannelEnsureSummary {
                team_id: "org-team-company".into(),
                team_created: false,
                rooms_created: 0,
                rooms_synced: 0,
            }
        );
        let stamps = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM messaging_rooms WHERE company_id='company' AND created_at=$1",
        )
        .bind(NOW)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stamps, 3);

        // Rename sync: existing rows take the current display name without touching created_at.
        sqlx::query("UPDATE organization_nodes SET display_name='Northside' WHERE id='branch'")
            .execute(pool.raw())
            .await
            .unwrap();
        let synced = run(pool.clone(), move |connection| {
            Box::pin(async move {
                let mut ports = PgOrganizationNodes::new(connection);
                ports
                    .ensure_channels(&company(), NOW + 2_000)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        assert!(!synced.team_created);
        assert_eq!(synced.rooms_created, 0);
        assert_eq!(synced.rooms_synced, 1);
        let renamed: (String, i64) = sqlx::query_as(
            "SELECT name,created_at FROM messaging_rooms WHERE organization_node_id='branch'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(renamed, ("Northside".into(), NOW));
        rooms = sqlx::query_as(
            "SELECT id,name,organization_node_id,created_at FROM messaging_rooms WHERE company_id='company' ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(rooms.len(), 3);

        // Suspended nodes never gain rooms, including after a later rename.
        sqlx::query("UPDATE organization_nodes SET display_name='Older' WHERE id='suspended'")
            .execute(pool.raw())
            .await
            .unwrap();
        let after = run(pool.clone(), move |connection| {
            Box::pin(async move {
                ensure_organization_channels(connection, &company(), NOW + 3_000)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        assert_eq!(after.rooms_created, 0);
        assert_eq!(after.rooms_synced, 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM messaging_rooms WHERE organization_node_id='suspended'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
        // No cross-company leakage.
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM messaging_rooms WHERE company_id='foreign'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn company_ensure_without_active_company_node_is_a_noop() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        sqlx::query("UPDATE organization_nodes SET status='suspended' WHERE id='company'")
            .execute(pool.raw())
            .await
            .unwrap();
        let summary = run(pool.clone(), move |connection| {
            Box::pin(async move {
                ensure_organization_channels(connection, &company(), NOW)
                    .await
                    .map_err(|error| match error {
                        PgTransactionError::Business(error) => error,
                        error => panic!("unexpected persistence {error:?}"),
                    })
            })
        })
        .await
        .unwrap();
        assert!(!summary.team_created);
        assert_eq!(summary.rooms_created, 0);
        assert_eq!(summary.rooms_synced, 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM messaging_teams")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM messaging_rooms")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn company_ensure_stays_inside_the_caller_transaction() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No ambient transaction: the port refuses instead of opening its own unit.
        let mut bare = pool.raw().acquire().await.unwrap();
        let refused =
            ensure_organization_channels(&mut bare, &company(), NOW).await;
        assert!(
            matches!(refused, Err(PgTransactionError::Persistence(_))),
            "ensure must require the caller's transaction"
        );
        // The caller owns commit/rollback: an outer failure rolls back the provisioned rows.
        let rolled_back: Result<OrganizationChannelEnsureSummary, _> =
            PgUnitOfWork::new(pool.clone())
            .run_once(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    ensure_organization_channels(connection, &company(), NOW).await?;
                    Err(BusinessError::new(
                        erpx_platform_shared_server::public::FailureKind::Conflict,
                        "SYNTHETIC_ROLLBACK",
                        "Synthetic caller failure.",
                    )
                    .into())
                })
            })
            .await;
        assert!(matches!(
            rolled_back,
            Err(PgTransactionError::Business(ref error)) if error.code == "SYNTHETIC_ROLLBACK"
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM messaging_teams")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM messaging_rooms")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}
