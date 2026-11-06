#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_organization::public::journal_flow_department::{
    journal_flow_team_name, lock_journal_flow_department_sources,
};
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId};

#[tokio::test]
async fn journal_flow_team_name_uses_active_same_company_node_or_company() {
    fixture::with_database(1, |pool| async move {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &files).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
             VALUES('company','company','acme','Acme',NULL,'company',NULL,'active'), \
                   ('department','department','service','Service','company','company',NULL,'active'), \
                   ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active'), \
                   ('archived','department','archived','Archived','company','company',NULL,'archived')",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let uow = PgUnitOfWork::new(pool.clone());

        // No department selected.
        let no_department = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    lock_journal_flow_department_sources(connection).await?;
                    journal_flow_team_name(connection, &CompanyId::new("company").unwrap(), None).await
                })
            })
            .await
            .unwrap();
        assert_eq!(no_department, "Company");

        // Active same-company node.
        let active = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    lock_journal_flow_department_sources(connection).await?;
                    journal_flow_team_name(
                        connection,
                        &CompanyId::new("company").unwrap(),
                        Some("department"),
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(active, "Service");

        // Missing, archived, or foreign node falls back to Company.
        for department in ["missing", "archived"] {
            let result = uow
                .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                    let department = department.to_owned();
                    Box::pin(async move {
                        lock_journal_flow_department_sources(connection).await?;
                        journal_flow_team_name(
                            connection,
                            &CompanyId::new("company").unwrap(),
                            Some(&department),
                        )
                        .await
                    })
                })
                .await
                .unwrap();
            assert_eq!(result, "Company", "department={department}");
        }
        let foreign = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    lock_journal_flow_department_sources(connection).await?;
                    journal_flow_team_name(
                        connection,
                        &CompanyId::new("company").unwrap(),
                        Some("foreign"),
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(foreign, "Company");

        // Calling outside a transaction is rejected.
        let mut connection = pool.raw().acquire().await.unwrap();
        let outside = journal_flow_team_name(
            &mut *connection,
            &CompanyId::new("company").unwrap(),
            None,
        )
        .await;
        assert!(outside.is_err());
    })
    .await;
}
