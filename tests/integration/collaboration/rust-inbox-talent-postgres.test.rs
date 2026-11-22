//! Postgres proof of the native Talent, Marketing and Manufacturing Inbox
//! visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle. Talent settles the Recruitment
//! role, the Talent module and the recruit history before the scoped
//! application read; marketing settles the module, the reader role, the
//! consented-candidate image and the marketing history before the scoped
//! candidate read; manufacturing settles the module, the reader role, the
//! order-chain image and the execution history before the scoped order
//! read. With enforcement off the purchase path is skipped and the
//! readable check authorizes the scoped read directly. Missing rows deny
//! with the exact oracle 403/404 codes and messages. The enforced-mode
//! tests pin `PackageEnforcementMode::Enforced` through the delegate's
//! explicit mode seam instead of mutating process environment (unsafe env
//! mutation is forbidden by workspace lints).
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    InboxTarget, InboxTargetPort, PgInboxTargets, TalentInboxTargets,
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;

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
         ('co','company','co','Co',NULL,'co',NULL,'active'), \
         ('br','branch','br','Br','co','co',NULL,'active'), \
         ('dep-1','department','dep-1','Dep','br','co','br','active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','talent_reader','Talent reader',false,1,'active'), \
         ('*','talent_noreview','Talent viewer',false,1,'active'), \
         ('*','marketing_reader','Marketing reader',false,1,'active'), \
         ('*','mfg_reader','Manufacturing reader',false,1,'active'), \
         ('*','no_talent','No talent',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','talent_reader',1,'[\"records.read\",\"records.review\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','talent_noreview',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','marketing_reader',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','mfg_reader',1,'[\"records.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','no_talent',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','talent_reader','[\"talent\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','marketing_reader','[\"marketing\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','mfg_reader','[\"manufacturing\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-4','user-4','no_talent','[]','co',0,9007199254740991,1,'holding'), \
         ('grant-5','user-5','talent_noreview','[\"talent\"]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO talent_requisitions(id,company_id,branch_id,department_id,job_title,openings,target_start_date,description,created_by,created_at,updated_at) VALUES \
         ('req-1','co','br','dep-1','Engineer',1,'2026-12-01','Build things','maker',1,1); \
         INSERT INTO talent_candidates(id,company_id,legal_name,email,consent_evidence,consented_at,retention_until,created_by,created_at,updated_at) VALUES \
         ('cand-1','co','Alex','alex@example.com','consent',1,'2027-01-01','maker',1,1); \
         INSERT INTO talent_applications(id,company_id,requisition_id,requisition_version,candidate_id,owner_id,created_by,created_at,updated_at) VALUES \
         ('app-1','co','req-1',1,'cand-1','user-1','maker',1,1); \
         INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,created_by,created_at,updated_at) VALUES \
         ('form-1','co','form','Hiring','{}','{}','maker',0,0); \
         INSERT INTO marketing_asset_events(id,asset_id,version,action,actor_id,reason,status,created_at) VALUES \
         ('ev-1','form-1',1,'activate','maker','Launch','active',0); \
         INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,created_by,created_at,updated_at) VALUES \
         ('mc-1','co','br','dep-1','form-1',1,'Sam','sam@example.com','yes, I consent','{}','Hello','maker',1,1); \
         INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) VALUES \
         ('ee-1','mc-1',1,'form_submit','web','submit','maker',1); \
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES \
         ('prod-1','co','SKU-1','Widget','each',100,0,'active',1); \
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('wh-1','co','br','W1','Warehouse','active',1); \
         INSERT INTO manufacturing_specifications(id,company_id,name,mode,output_product_id,batch_quantity_milli,expected_yield_bps,created_by,created_at,updated_at) VALUES \
         ('sp-1','co','Assembly','bom','prod-1',1000,10000,'maker',1,1); \
         INSERT INTO planning_scenarios(id,company_id,branch_id,warehouse_id,specification_id,specification_version,demand_quantity_milli,due_date,capacity_minutes,shortages_json,context_json,created_by,created_at,updated_at) VALUES \
         ('sc-1','co','br','wh-1','sp-1',1,1000,'2026-12-01',0,'[]','{}','maker',1,1), \
         ('sc-2','co','br','wh-1','sp-1',1,1000,'2026-12-01',0,'[]','{\"comparisonScenarioId\":\"sc-1\",\"comparison\":{\"scenarioId\":\"sc-1\",\"sourceVersion\":1}}','maker',1,1); \
         INSERT INTO planning_recommendations(id,scenario_id,scenario_version,recommended_quantity_milli,required_minutes,feasible_date,created_by,created_at,updated_at) VALUES \
         ('rec-1','sc-1',1,1000,0,'2026-12-01','maker',1,1), \
         ('rec-2','sc-2',1,1000,0,'2026-12-01','maker',1,1); \
         INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,created_by,created_at,updated_at) VALUES \
         ('mo-1','co','br','wh-1','rec-1','sp-1',1,1000,'maker',1,1), \
         ('mo-2','co','br','wh-1','rec-2','sp-1',1,1000,'maker',1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_grants(pool: &DatabasePool, packages: &[&str]) {
    let mut tx = pool.raw().begin().await.unwrap();
    for package in packages {
        let event = format!("evt-{package}");
        sqlx::query(
            "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES \
             ('co',$1,1,'active',0,NULL,1,'fixture','test:talent-history',0)",
        )
        .bind(package)
        .execute(&mut *tx)
        .await
        .unwrap();
        let after = serde_json::json!({
            "companyId": "co",
            "packageId": package,
            "packageVersion": 1,
            "state": "active",
            "startsAt": 0,
            "endsAt": None::<i64>,
            "version": 1,
            "reason": "fixture",
            "evidence": "test:talent-history",
        })
        .to_string();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES \
             ($1,'co',$2,'fixture','test',NULL,$3::jsonb,0)",
        )
        .bind(event.clone())
        .bind(package)
        .bind(after)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at) VALUES \
             ('co',$1,1,$2,'after',1,0)",
        )
        .bind(package)
        .bind(&event)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

async fn run<T, F>(
    pool: DatabasePool,
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

async fn composed_target(
    pool: DatabasePool,
    company_id: &str,
    user_id: &str,
    source_type: &str,
    source_id: &str,
    branch_id: Option<&str>,
    mode: Option<PackageEnforcementMode>,
) -> Result<Option<InboxTarget>, PgTransactionError<BusinessError>> {
    let (company_id, user_id, source_type, source_id, branch_id) = (
        company_id.to_owned(),
        user_id.to_owned(),
        source_type.to_owned(),
        source_id.to_owned(),
        branch_id.map(str::to_owned),
    );
    run(pool, move |connection| {
        Box::pin(async move {
            let mut delegate = TalentInboxTargets::new(branch_id.as_deref(), None);
            if let Some(mode) = mode {
                delegate = delegate.enforcement_mode(mode);
            }
            let mut targets = PgInboxTargets::with_delegate(None, None, delegate);
            targets
                .target(connection, &company_id, &user_id, &source_type, &source_id, 0)
                .await
        })
    })
    .await
}

fn visible(
    pool: DatabasePool,
    user_id: &str,
    source_type: &str,
    source_id: &str,
) -> impl std::future::Future<Output = Option<InboxTarget>> {
    async move {
        composed_target(pool, "co", user_id, source_type, source_id, None, None)
            .await
            .unwrap()
    }
}

fn denied(
    result: &Result<Option<InboxTarget>, PgTransactionError<BusinessError>>,
) -> (&'static str, &'static str) {
    match result {
        Err(PgTransactionError::Business(error)) => (error.code, error.message),
        _ => panic!("expected an oracle denial, got {result:?}"),
    }
}

#[tokio::test]
async fn talent_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "talent_application", "app-1")
            .await
            .expect("scoped application must resolve");
        assert_eq!(target.href.as_deref(), Some("/talent/app-1"));
        // No source override: the composition final map applies the
        // retained default ({kind:"talent_application", id, href, label}).
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn marketing_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-2", "marketing_candidate", "mc-1")
            .await
            .expect("scoped candidate must resolve");
        assert_eq!(target.href.as_deref(), Some("/marketing/mc-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn manufacturing_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-3", "manufacturing_order", "mo-1")
            .await
            .expect("scoped order must resolve");
        assert_eq!(target.href.as_deref(), Some("/manufacturing/mo-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn readers_without_the_role_or_module_are_denied() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // No records.read: the readable check denies first.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-4", "talent_application", "app-1", None, None)
                    .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-4", "marketing_candidate", "mc-1", None, None)
                    .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-4", "manufacturing_order", "mo-1", None, None)
                    .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // records.read without the module denies with the module code.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-2", "talent_application", "app-1", None, None)
                    .await
            ),
            ("MODULE_FORBIDDEN", "Your current role cannot access Talent.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "marketing_candidate", "mc-1", None, None)
                    .await
            ),
            ("MODULE_FORBIDDEN", "Your current role cannot access Marketing.")
        );
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "manufacturing_order", "mo-1", None, None)
                    .await
            ),
            (
                "MODULE_FORBIDDEN",
                "Your current role cannot access manufacturing."
            )
        );
        // Talent reviewers only: records.read without records.review
        // denies with the privacy code (and the authority code when
        // enforced).
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-5", "talent_application", "app-1", None, None)
                    .await
            ),
            (
                "TALENT_PRIVACY_FORBIDDEN",
                "Candidate data requires Talent reviewer access."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-5",
                    "talent_application",
                    "app-1",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "TALENT_ACCESS_CHANGED",
                "Choose a current authorized Recruitment work role."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_authority_denies_before_the_purchase_gate() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // No recruitment role: the assignment authority denies with the
        // oracle code before any purchase decision.
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-4", "talent_application", "app-1", None, enforced)
                    .await
            ),
            (
                "TALENT_ACCESS_CHANGED",
                "Choose a current authorized Recruitment work role."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-4",
                    "marketing_candidate",
                    "mc-1",
                    None,
                    enforced,
                )
                .await
            ),
            ("MODULE_FORBIDDEN", "Your current role cannot access Marketing.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-4",
                    "manufacturing_order",
                    "mo-1",
                    None,
                    enforced,
                )
                .await
            ),
            (
                "MANUFACTURING_ACCESS_CHANGED",
                "Choose a current original production assignment."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn missing_rows_keep_their_oracle_codes_and_messages() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(
                &composed_target(pool.clone(), "co", "user-1", "talent_application", "missing", None, None)
                    .await
            ),
            ("TALENT_APPLICATION_NOT_FOUND", "Application not found.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-2",
                    "marketing_candidate",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            ("MARKETING_CANDIDATE_NOT_FOUND", "Lead candidate not found.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "manufacturing_order",
                    "missing",
                    None,
                    None,
                )
                .await
            ),
            ("MANUFACTURING_ORDER_NOT_FOUND", "Order not found.")
        );
        // Out-of-frame rows read as missing, exactly like the retained
        // scopes.
        for (user_id, source_type, source_id, code) in [
            ("user-1", "talent_application", "app-1", "TALENT_APPLICATION_NOT_FOUND"),
            ("user-2", "marketing_candidate", "mc-1", "MARKETING_CANDIDATE_NOT_FOUND"),
            ("user-3", "manufacturing_order", "mo-1", "MANUFACTURING_ORDER_NOT_FOUND"),
        ] {
            let (actual_code, _) = denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    user_id,
                    source_type,
                    source_id,
                    Some("elsewhere"),
                    None,
                )
                .await,
            );
            assert_eq!(actual_code, code, "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn unpurchased_company_is_denied_with_the_decision_code() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Empty purchase facts in enforced mode: each oracle's exact
        // refusal surfaces for its own capability.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "talent_application",
                    "app-1",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "This package does not permit the requested work."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-2",
                    "marketing_candidate",
                    "mc-1",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "The Marketing addon or retained history is required."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "manufacturing_order",
                    "mo-1",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "The original production capability or retained history is required."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_mode_with_history_grant_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(&pool, &["recruit", "sales", "sales-marketing", "manufacturing"]).await;
        // One grant per owner covers the capability behind its slice.
        for (user_id, source_type, source_id, href) in [
            ("user-1", "talent_application", "app-1", "/talent/app-1"),
            ("user-2", "marketing_candidate", "mc-1", "/marketing/mc-1"),
            ("user-3", "manufacturing_order", "mo-1", "/manufacturing/mo-1"),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                user_id,
                source_type,
                source_id,
                None,
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("granted {source_type} must resolve in enforced mode"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_type}");
            assert_eq!(target.source, None, "{source_type}");
        }
    })
    .await;
}

#[tokio::test]
async fn advanced_plan_requires_the_advanced_history() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The comparison scenario resolves the advanced flag: execution
        // history alone denies, exactly like the oracle capabilities.
        seed_grants(&pool, &["manufacturing"]).await;
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "manufacturing_order",
                    "mo-2",
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "The original production capability or retained history is required."
            )
        );
        seed_grants(&pool, &["manufacturing-mrp"]).await;
        let target = composed_target(
            pool.clone(),
            "co",
            "user-3",
            "manufacturing_order",
            "mo-2",
            None,
            Some(PackageEnforcementMode::Enforced),
        )
        .await
        .unwrap()
        .expect("advanced order resolves with both histories");
        assert_eq!(target.href.as_deref(), Some("/manufacturing/mo-2"));
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = TalentInboxTargets::new(None, None);
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let error = delegate
                    .target(connection, "co", "user-1", "future_source", "x", 0)
                    .await
                    .expect_err("unknown source must fail closed");
                Ok(error.code)
            })
        })
        .await
        .unwrap();
        assert_eq!(code, "INBOX_TARGET_SOURCE_UNSUPPORTED");
        let mut targets =
            PgInboxTargets::with_delegate(None, None, TalentInboxTargets::new(None, None));
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let error = targets
                    .target(connection, "co", "user-1", "support_ticket", "t-1", 0)
                    .await
                    .expect_err("unowned source must fail closed");
                Ok(error.code)
            })
        })
        .await
        .unwrap();
        assert_eq!(code, "INBOX_TARGET_PORT_UNAVAILABLE");
    })
    .await;
}

#[tokio::test]
async fn port_commits_nothing_and_rolls_back_with_the_caller() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let rolled_back: Result<Option<InboxTarget>, _> =
            PgUnitOfWork::new(pool.clone())
                .run_once(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let mut targets = PgInboxTargets::with_delegate(
                            None,
                            None,
                            TalentInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "talent_application", "app-1", 0)
                            .await?;
                        assert!(resolved.is_some());
                        Err(BusinessError::new(
                            erpx_server_shared::public::FailureKind::Conflict,
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
        // The read-only port staged no rows: the seed image is unchanged.
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM talent_applications WHERE company_id='co'), \
             (SELECT count(*) FROM marketing_lead_candidates WHERE company_id='co'), \
             (SELECT count(*) FROM manufacturing_orders WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 2));
    })
    .await;
}
