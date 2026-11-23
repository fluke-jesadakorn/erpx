//! Native Brand attachment-source image over a leased PostgreSQL database:
//! holding/branch/company-storefront target parity with the retained
//! TypeScript read, unknown-target fail-closed, Company scoping and
//! transaction-required denial, all on the caller transaction.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_brand::public::brand_attachment_read::read_brand_attachment_image;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use serde_json::Value;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES \
         ('company','company','company','Company','company'), \
         ('branch','branch','branch','Branch','company'); \
         INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at) VALUES \
         ('holding','company',NULL,'holding','fixture',0), \
         ('branch','branch','company','both','fixture',0); \
         INSERT INTO storefront_app_brands(company_id,short_name,accent_color,updated_by,updated_at) VALUES \
         ('company','Store','#3659c9','fixture',0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn image_value(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw).expect("brand image must be JSON")
}

#[tokio::test]
async fn brand_attachment_images_match_the_retained_family() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, erpx_server_shared::public::BusinessError, _>(
                TransactionOptions::default(),
                |c| {
                    Box::pin(async move {
                        // holding_brand: holding-scope profile, no storefront row.
                        let holding =
                            read_brand_attachment_image(&mut *c, "company", "holding_brand", Some("company")).await?;
                        let holding = image_value(&holding);
                        assert_eq!(
                            holding.get("profile").and_then(|p| p.get("scope_kind")).and_then(Value::as_str),
                            Some("holding")
                        );
                        assert!(holding.get("storefront").is_some_and(Value::is_null));
                        // branch_brand: branch-scope profile, no storefront row.
                        let branch =
                            read_brand_attachment_image(&mut *c, "company", "branch_brand", Some("branch")).await?;
                        let branch = image_value(&branch);
                        assert_eq!(
                            branch.get("profile").and_then(|p| p.get("scope_kind")).and_then(Value::as_str),
                            Some("branch")
                        );
                        assert!(branch.get("storefront").is_some_and(Value::is_null));
                        // company_storefront_brand: no profile row, storefront row present.
                        let storefront = read_brand_attachment_image(
                            &mut *c,
                            "company",
                            "company_storefront_brand",
                            None,
                        )
                        .await?;
                        let storefront = image_value(&storefront);
                        assert!(storefront.get("profile").is_some_and(Value::is_null));
                        assert_eq!(
                            storefront
                                .get("storefront")
                                .and_then(|s| s.get("short_name"))
                                .and_then(Value::as_str),
                            Some("Store")
                        );
                        // Unknown targets fail closed; empty Company is rejected.
                        assert!(matches!(
                            read_brand_attachment_image(&mut *c, "company", "nope", None).await,
                            Err(PgTransactionError::Business(_))
                        ));
                        assert!(matches!(
                            read_brand_attachment_image(&mut *c, "", "holding_brand", Some("company")).await,
                            Err(PgTransactionError::Business(_))
                        ));
                        Ok(())
                    })
                },
            )
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn brand_attachment_read_requires_the_caller_transaction() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut direct = pool.raw().acquire().await.unwrap();
        let denied =
            read_brand_attachment_image(&mut direct, "company", "holding_brand", Some("company")).await;
        assert!(matches!(
            denied,
            Err(PgTransactionError::Persistence(_))
        ));
    })
    .await;
}
