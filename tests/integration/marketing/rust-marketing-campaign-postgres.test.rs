//! Native Marketing campaign/attribution reads over real migrated PostgreSQL.
//! These prove owner read SQL, Company/branch/department isolation, attribution
//! aggregation, serializable transaction guards, rollback and persisted-source
//! readback. They are not a Marketing command, package or browser journey.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_marketing::public::campaign_attribution::{
    MarketingCampaignActor, MarketingCampaignEntitlement, MarketingCampaignEntitlementPort,
    MarketingCampaignFuture, PgMarketingCampaignRead, marketing_campaign_report,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn actor(
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
) -> MarketingCampaignActor {
    MarketingCampaignActor {
        user_id: "owner".into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["marketing".into()],
        expires_at: NOW.get() + 60_000,
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('form-company','company','form','Lead form','{\"consentRequired\":true}','{}','active','owner','reviewer',1,10,10),\
         ('form-foreign','foreign','form','Foreign form','{\"consentRequired\":true}','{}','active','owner','reviewer',1,10,10);\
         INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES \
         ('campaign-autumn','company','AUTUMN','Autumn demand','owner',10),\
         ('campaign-summer','company','SUMMER','Summer demand','owner',11),\
         ('campaign-winter','company','WINTER','Winter demand','owner',12),\
         ('campaign-foreign','foreign','FOREIGN','Foreign campaign','owner',13);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) VALUES \
         ('candidate-1','company','branch',NULL,'form-company',1,'Synthetic one','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','pending',NULL,'owner',NULL,1,100,100),\
         ('candidate-2','company','branch',NULL,'form-company',1,'Synthetic two','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','handed_off',NULL,'owner','reviewer',1,101,101),\
         ('candidate-3','company','branch',NULL,'form-company',1,'Synthetic three','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','rejected',NULL,'owner','reviewer',1,102,102),\
         ('candidate-4','company','branch',NULL,'form-company',1,'Synthetic four','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"email\"}','Synthetic brief','pending',NULL,'owner',NULL,1,103,103),\
         ('candidate-5','company','other-branch',NULL,'form-company',1,'Synthetic five','Synthetic channel','Synthetic consent','{\"campaign\":\"AUTUMN\",\"source\":\"web\"}','Synthetic brief','pending',NULL,'owner',NULL,1,104,104),\
         ('candidate-6','company','branch','department','form-company',1,'Synthetic six','Synthetic channel','Synthetic consent','{\"campaign\":\"SUMMER\",\"utmSource\":\"social\"}','Synthetic brief','pending',NULL,'owner',NULL,1,105,105),\
         ('candidate-7','company','branch',NULL,'form-company',1,'Synthetic seven','Synthetic channel','Synthetic consent','{}','Synthetic brief','pending',NULL,'owner',NULL,1,106,106),\
         ('candidate-foreign','foreign','foreign-branch',NULL,'form-foreign',1,'Foreign one','Synthetic channel','Synthetic consent','{\"campaign\":\"FOREIGN\",\"source\":\"web\"}','Synthetic brief','pending',NULL,'owner',NULL,1,107,107);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

struct Packages;

impl MarketingCampaignEntitlementPort for Packages {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error> {
        Box::pin(async {
            Ok(MarketingCampaignEntitlement {
                enforced: true,
                history_allowed: true,
                new_work: true,
                settle_existing: true,
                revision: "7".into(),
            })
        })
    }
}

struct DeniedPackage;

impl MarketingCampaignEntitlementPort for DeniedPackage {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error> {
        Box::pin(async {
            Ok(MarketingCampaignEntitlement {
                enforced: true,
                history_allowed: false,
                new_work: false,
                settle_existing: false,
                revision: "9".into(),
            })
        })
    }
}

#[tokio::test]
async fn report_filters_company_branch_department_and_groups_attribution() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let company = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = company.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await?;
                    assert_eq!(
                        report
                            .campaigns
                            .iter()
                            .map(|row| row.code.as_str())
                            .collect::<Vec<_>>(),
                        vec!["AUTUMN", "SUMMER", "WINTER"]
                    );
                    assert!(
                        report
                            .campaigns
                            .iter()
                            .all(|row| row.company_id == "company")
                    );
                    assert_eq!(
                        report
                            .attribution
                            .iter()
                            .map(|row| (
                                row.campaign.as_str(),
                                row.source.as_str(),
                                row.captured,
                                row.pending,
                                row.handed_off,
                                row.rejected
                            ))
                            .collect::<Vec<_>>(),
                        vec![
                            ("AUTUMN", "web", 4, 2, 1, 1),
                            ("AUTUMN", "email", 1, 1, 0, 0),
                            ("SUMMER", "social", 1, 1, 0, 0),
                            ("Unspecified", "Unspecified", 1, 1, 0, 0),
                        ]
                    );
                    assert_eq!(report.purchase.unwrap().revision, "7");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let branch = actor("company", Some("branch"), None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = branch.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await?;
                    assert!(
                        report
                            .attribution
                            .iter()
                            .all(|row| row.campaign != "other-branch")
                    );
                    assert_eq!(
                        report
                            .attribution
                            .iter()
                            .map(|row| (row.campaign.as_str(), row.captured))
                            .collect::<Vec<_>>(),
                        vec![
                            ("AUTUMN", 3),
                            ("AUTUMN", 1),
                            ("SUMMER", 1),
                            ("Unspecified", 1)
                        ]
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();

        let department = actor(
            "company",
            Some("branch"),
            Some("department"),
            &["records.read"],
        );
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = department.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await?;
                    assert_eq!(report.attribution.len(), 1);
                    assert_eq!(report.attribution[0].campaign, "SUMMER");
                    assert_eq!(report.attribution[0].source, "social");
                    Ok(())
                })
            })
            .await
            .unwrap();

        let foreign = actor("foreign", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = foreign.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await?;
                    assert_eq!(report.campaigns.len(), 1);
                    assert_eq!(report.campaigns[0].id, "campaign-foreign");
                    assert_eq!(report.attribution.len(), 1);
                    assert_eq!(report.attribution[0].campaign, "FOREIGN");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn invalid_attribution_json_fails_closed_and_rolls_back() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) VALUES \
                         ('candidate-invalid','company','branch',NULL,'form-company',1,'Synthetic invalid','Synthetic channel','Synthetic consent','{not-json','Synthetic brief','pending',NULL,'owner',NULL,1,900,900);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let error =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await
                            .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "MARKETING_CAMPAIGN_SOURCE_INVALID");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let persisted: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM marketing_lead_candidates WHERE id='candidate-invalid'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(persisted, 0);
    })
    .await;
}

#[tokio::test]
async fn transaction_guards_rollback_and_persisted_readback_hold() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let owner = actor("company", None, None, &["records.read"]);
        let mut outside = pool.raw().acquire().await.unwrap();
        let error = PgMarketingCampaignRead::new(&mut outside)
            .campaigns(&CompanyId::new("company").unwrap(), 501)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "MARKETING_CAMPAIGN_READ_TRANSACTION_REQUIRED"
        ));
        drop(outside);

        let mut default_isolation = pool.raw().begin().await.unwrap();
        let error = PgMarketingCampaignRead::new(&mut default_isolation)
            .attribution(&CompanyId::new("company").unwrap(), None, None, 1001)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "MARKETING_CAMPAIGN_READ_SERIALIZABLE_REQUIRED"
        ));
        default_isolation.rollback().await.unwrap();

        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM marketing_campaigns")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let read_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = read_owner.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await?;
                    assert_eq!(report.campaigns.len(), 3);
                    Ok(())
                })
            })
            .await
            .unwrap();
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM marketing_campaigns")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(before, after);

        let rollback_owner = owner.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = rollback_owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES \
                         ('campaign-rollback','company','ROLLBACK','Rollback campaign','owner',9);",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let report =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await?;
                    assert!(
                        report
                            .campaigns
                            .iter()
                            .any(|row| row.id == "campaign-rollback")
                    );
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let persisted: i64 =
            sqlx::query_scalar("SELECT count(*) FROM marketing_campaigns WHERE id='campaign-rollback'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(persisted, 0);
    })
    .await;
}

#[tokio::test]
async fn permission_and_package_denial_fail_before_source_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let denied = actor("company", None, None, &[]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = denied.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "ALTER TABLE marketing_campaigns RENAME TO marketing_campaigns_hidden",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let error =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await
                            .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "MARKETING_CAMPAIGN_PERMISSION_REQUIRED");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
        let restored: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_class WHERE relname='marketing_campaigns')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(restored);

        let owner = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    sqlx::raw_sql(
                        "ALTER TABLE marketing_campaigns RENAME TO marketing_campaigns_hidden",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = DeniedPackage;
                    let error =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await
                            .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "PACKAGE_REQUIRED");
                    Err(PgTransactionError::Business(BusinessError::invalid(
                        "OWNER_ABORT",
                        "Rollback fixture",
                    )))
                })
            })
            .await
            .unwrap_err();
    })
    .await;
}

#[tokio::test]
async fn campaign_source_bounds_reject_excess_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) \
             SELECT 'bulk-'||n,'company','B'||lpad(n::text,4,'0'),'Bulk campaign '||n,'owner',100 \
             FROM generate_series(1,498) AS n;",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let owner = actor("company", None, None, &["records.read"]);
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                let current = owner.clone();
                Box::pin(async move {
                    let mut reader = PgMarketingCampaignRead::new(connection);
                    let mut packages = Packages;
                    let error =
                        marketing_campaign_report(&current, &mut reader, &mut packages, NOW)
                            .await
                            .unwrap_err();
                    let error = business(error);
                    assert_eq!(error.code, "MARKETING_CAMPAIGN_TOO_LARGE");
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}
