//! Postgres proof of the native Workflow Inbox visibility delegate.
//!
//! Each gate mirrors its TypeScript oracle. Task alerts reuse the exact
//! operations task proof (cited, not re-derived). Action cards settle the
//! records role and the collaboration module before the frame-scoped card
//! read. Handoffs settle the project purchase with the Project authority,
//! then the handoff row and the project proof. Maintenance settles the
//! asset history with the Assets authority, the records role, the
//! maintenance module and the asset-joined read. Batches settle the
//! finance authority with the banking/inventory-valuation history, then
//! the Company-treasury frame and the batch read. Handbook settles the
//! work history, the membership, then the editor proof and the
//! article/revision read with the versioned source. Missing rows deny
//! with the exact oracle 404 codes and messages. The enforced-mode tests
//! pin modes through the delegate's explicit seams instead of mutating
//! process environment (unsafe env mutation is forbidden by workspace
//! lints).
//!
//! History decisions require company package history rights as well as the
//! grant and its event, exactly like the commercial decision port reads
//! them.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::inbox::{
    InboxTarget, InboxTargetPort, PgInboxTargets, WorkflowInboxTargets,
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
         ('br','branch','br','Br','co','co',NULL,'active'); \
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','wf_reader','Workflow reader',false,1,'active'), \
         ('*','no_read','No read',false,1,'active') ON CONFLICT DO NOTHING; \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','wf_reader',1,'[\"records.read\",\"finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0), \
         ('*','no_read',1,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0) ON CONFLICT DO NOTHING; \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','wf_reader','[\"maintenance\",\"projects\",\"collaboration\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-2','user-2','wf_reader','[\"maintenance\",\"projects\",\"collaboration\"]','co',0,9007199254740991,1,'holding'), \
         ('grant-3','user-3','no_read','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO staff_projects(id,company_id,title,owner_user_id,created_at,updated_at) VALUES \
         ('proj-1','co','Build','user-2',1,1); \
         INSERT INTO staff_tasks(id,company_id,branch_id,title,creator_user_id,assignee_user_id,due_at,created_at,updated_at) VALUES \
         ('t-1','co','br','Task','user-1','user-2',1,1,1); \
         INSERT INTO staff_project_handoffs(id,company_id,project_id,source_project_version,recipient_id,snapshot_json,snapshot_sha256,evidence,prepared_by,prepared_at) VALUES \
         ('hand-1','co','proj-1',1,'user-1','{}','abababababababababababababababababababababababababababababababab','ok','user-2',1); \
         INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,lifecycle_status,created_by,created_at,updated_at) VALUES \
         ('asset-1','co','br','A-1','Pump','equipment','USD','registered','maker',1,1); \
         INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,owner_id,created_by,created_at,updated_at) VALUES \
         ('wo-1','co','br','asset-1','Fix pump','normal','user-1','maker',1,1); \
         INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,created_by,edited_by,created_at,updated_at) VALUES \
         ('plan-1','co','br','asset-1','Inspect pump','normal','user-1',30,'UTC','2026-12-01','2026-12-31',1,'maker','maker',1,1); \
         INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at) SELECT 'plan-event-1',id,version,'create','maker','Seed plan created',to_jsonb(p),1 FROM maintenance_preventive_plans p WHERE id='plan-1'; \
         INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES \
         ('chart-1','co','Chart','USD','maker',1,1); \
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES \
         ('acct-1','chart-1','co','1000','Bank','asset',1,1); \
         INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,created_by,created_at,updated_at) VALUES \
         ('batch-1','co','USD','acct-1','2026-12-01','user-2',1,1); \
         INSERT INTO collaboration_action_cards(id,company_id,source_type,source_id,source_version,title,owner_id,due_at,created_by,created_at,updated_at) VALUES \
         ('card-1','co','staff_task','t-1',1,'Follow up','user-1',1,'maker',1,1); \
         INSERT INTO handbook_articles(id,company_id,draft_revision,published_revision,archived,created_by,created_at,updated_at) VALUES \
         ('art-1','co',2,2,0,'maker',1,1), \
         ('art-2','co',1,NULL,0,'maker',1,1); \
         INSERT INTO handbook_revisions(company_id,article_id,revision,title,category,body,created_by,created_at,published_by,published_at) VALUES \
         ('co','art-1',2,'Guide','general','Body','maker',1,'maker',1), \
         ('co','art-2',1,'Draft','general','Body','maker',1,NULL,NULL); \
         INSERT INTO handbook_editors(company_id,user_id,granted_by,granted_at) VALUES \
         ('co','user-1','maker',1)",
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
             ('co',$1,1,'active',0,NULL,1,'fixture','test:workflow-history',0)",
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
            "evidence": "test:workflow-history",
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
    projects_mode: Option<PackageEnforcementMode>,
    assets_mode: Option<PackageEnforcementMode>,
    finance_mode: Option<PackageEnforcementMode>,
    apps_mode: Option<PackageEnforcementMode>,
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
            let mut delegate = WorkflowInboxTargets::new(branch_id.as_deref(), None);
            if let Some(mode) = projects_mode {
                delegate = delegate.projects_enforcement_mode(mode);
            }
            if let Some(mode) = assets_mode {
                delegate = delegate.assets_enforcement_mode(mode);
            }
            if let Some(mode) = finance_mode {
                delegate = delegate.finance_enforcement_mode(mode);
            }
            if let Some(mode) = apps_mode {
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
        composed_target(
            pool, "co", user_id, source_type, source_id, None, None, None, None, None,
        )
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
async fn task_alert_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "staff_task_alert", "t-1")
            .await
            .expect("creator resolves their task alert");
        assert_eq!(target.href.as_deref(), Some("/staff-tasks/t-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn collaboration_action_resolves_through_the_retained_read() {
    // PgInboxTargets intercepts collaboration_action before any delegate
    // with its dedicated in-crate read (drop-when-invisible, versioned
    // source), so the workflow delegate defers it. These assertions pin
    // the retained behavior the composition actually serves.
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "collaboration_action", "card-1")
            .await
            .expect("scoped card resolves");
        assert_eq!(target.href.as_deref(), Some("/collaboration"));
        let source = target.source.expect("retained versioned source");
        assert_eq!(source["kind"], serde_json::json!("collaboration_action"));
        assert_eq!(source["id"], serde_json::json!("card-1"));
        assert_eq!(source["version"], serde_json::json!(1));
        assert_eq!(source["href"], serde_json::json!("/collaboration"));
    })
    .await;
}

#[tokio::test]
async fn handoff_visible_with_exact_fragment_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "project_handoff", "hand-1")
            .await
            .expect("scoped handoff resolves");
        assert_eq!(
            target.href.as_deref(),
            Some("/projects/proj-1#project-structure")
        );
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn maintenance_visible_with_exact_hrefs_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "maintenance_work", "wo-1")
            .await
            .expect("scoped work order resolves");
        assert_eq!(target.href.as_deref(), Some("/maintenance/wo-1"));
        assert_eq!(target.source, None);
        let target = visible(pool.clone(), "user-1", "maintenance_plan", "plan-1")
            .await
            .expect("scoped plan resolves");
        assert_eq!(target.href.as_deref(), Some("/maintenance/plans/plan-1"));
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn batch_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let target = visible(pool.clone(), "user-1", "supplier_batch", "batch-1")
            .await
            .expect("treasury reader resolves the batch");
        assert_eq!(
            target.href.as_deref(),
            Some("/finance/supplier-batches/batch-1")
        );
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn handbook_visible_with_versioned_source_and_editor_variant() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Editor: draft href with the query flag and the draft version.
        let target = visible(pool.clone(), "user-1", "handbook", "art-1")
            .await
            .expect("editor resolves the article");
        assert_eq!(target.href.as_deref(), Some("/handbook/art-1?edit=1"));
        let source = target.source.expect("versioned source");
        assert_eq!(source["kind"], serde_json::json!("handbook_article"));
        assert_eq!(source["id"], serde_json::json!("art-1"));
        assert_eq!(source["version"], serde_json::json!(2));
        assert_eq!(source["href"], serde_json::json!("/handbook/art-1?edit=1"));
        assert_eq!(source["label"], serde_json::json!("Handbook article"));
        // Plain member: published href and the published version.
        let target = visible(pool.clone(), "user-2", "handbook_article", "art-1")
            .await
            .expect("member resolves the published article");
        assert_eq!(target.href.as_deref(), Some("/handbook/art-1"));
        let source = target.source.expect("versioned source");
        assert_eq!(source["version"], serde_json::json!(2));
        assert_eq!(source["href"], serde_json::json!("/handbook/art-1"));
    })
    .await;
}

#[tokio::test]
async fn denied_rows_keep_oracle_codes() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // The retained collaboration_action read drops denied cards
        // instead of failing the page (Ok(None), not a denial).
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-3",
                "collaboration_action",
                "card-1",
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .is_none(),
            "retained read drops the card for a reader without the role"
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-3",
                    "maintenance_work",
                    "wo-1",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission."
            )
        );
        // Missing rows deny with each oracle's exact 404.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "staff_task_alert",
                    "missing",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            ("TASK_NOT_FOUND", "Task not found.")
        );
        // Missing cards also drop through the retained read.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "collaboration_action",
                "missing",
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap()
            .is_none(),
            "retained read drops a missing card"
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "project_handoff",
                    "missing",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            ("PROJECT_HANDOFF_NOT_FOUND", "Project handoff not found.")
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "maintenance_work",
                    "missing",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "MAINTENANCE_WORK_NOT_FOUND",
                "Maintenance work order not found in this scope."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "maintenance_plan",
                    "missing",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "MAINTENANCE_PLAN_NOT_FOUND",
                "Preventive plan not found in this scope."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "supplier_batch",
                    "missing",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            ("BATCH_NOT_FOUND", "Supplier batch not found.")
        );
        // Unpublished article without an editor: the article arm denies.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-2",
                    "handbook",
                    "art-2",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "ARTICLE_NOT_FOUND",
                "This article has not been published or is unavailable."
            )
        );
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "handbook_article",
                    "missing",
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "ARTICLE_NOT_FOUND",
                "This article has not been published or is unavailable."
            )
        );
        // Branch frame on a holding batch: the treasury frame denies.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    "supplier_batch",
                    "batch-1",
                    Some("br"),
                    None,
                    None,
                    None,
                    None,
                )
                .await
            ),
            (
                "COMPANY_TREASURY_REQUIRED",
                "Use a Company finance assignment for supplier bank batches."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_gates_deny_with_oracle_codes() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        // Empty purchase facts: each oracle's exact refusal surfaces.
        let (code, _) = denied(
            &composed_target(
                pool.clone(),
                "co",
                "user-1",
                "project_handoff",
                "hand-1",
                None,
                enforced,
                None,
                None,
                None,
            )
            .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
        let (code, _) = denied(
            &composed_target(
                pool.clone(),
                "co",
                "user-1",
                "maintenance_work",
                "wo-1",
                None,
                None,
                enforced,
                None,
                None,
            )
            .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
        let (code, _) = denied(
            &composed_target(
                pool.clone(),
                "co",
                "user-1",
                "supplier_batch",
                "batch-1",
                None,
                None,
                None,
                enforced,
                None,
            )
            .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
        let (code, _) = denied(
            &composed_target(
                pool.clone(),
                "co",
                "user-1",
                "handbook",
                "art-1",
                None,
                None,
                None,
                None,
                enforced,
            )
            .await,
        );
        assert_eq!(code, "PACKAGE_REQUIRED");
        // No assignment at all: the assets purchase gate denies before the
        // role proof, exactly like withAssetPurchasedRead checking the
        // capability history before the authority.
        assert_eq!(
            denied(
                &composed_target(
                    pool.clone(),
                    "co",
                    "user-9",
                    "maintenance_work",
                    "wo-1",
                    None,
                    None,
                    enforced,
                    None,
                    None,
                )
                .await
            ),
            (
                "PACKAGE_REQUIRED",
                "This package does not permit the requested work."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn enforced_mode_with_history_grants_stays_visible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_grants(
            &pool,
            &[
                "projects",
                "assets",
                "assets-preventive",
                "finance",
                "work",
            ],
        )
        .await;
        let enforced = Some(PackageEnforcementMode::Enforced);
        for (user_id, source_type, source_id, href) in [
            ("user-1", "project_handoff", "hand-1", "/projects/proj-1#project-structure"),
            ("user-1", "maintenance_work", "wo-1", "/maintenance/wo-1"),
            ("user-1", "maintenance_plan", "plan-1", "/maintenance/plans/plan-1"),
            ("user-1", "supplier_batch", "batch-1", "/finance/supplier-batches/batch-1"),
            ("user-1", "handbook", "art-1", "/handbook/art-1?edit=1"),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                user_id,
                source_type,
                source_id,
                None,
                enforced,
                enforced,
                enforced,
                enforced,
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("granted {source_type} {source_id} must resolve in enforced mode"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_type}");
        }
        // Handbook source stays versioned in enforced mode too.
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "handbook",
            "art-1",
            None,
            enforced,
            enforced,
            enforced,
            enforced,
        )
        .await
        .unwrap()
        .expect("granted handbook must resolve");
        assert_eq!(target.source.as_ref().and_then(|s| s.get("version")), Some(&serde_json::json!(2)));
        // Collaboration actions carry no purchase gate at all.
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "collaboration_action",
            "card-1",
            None,
            enforced,
            enforced,
            enforced,
            enforced,
        )
        .await
        .unwrap()
        .expect("action card resolves without purchase");
        assert_eq!(target.href.as_deref(), Some("/collaboration"));
    })
    .await;
}

#[tokio::test]
async fn unknown_and_out_of_scope_sources_fail_closed() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = WorkflowInboxTargets::new(None, None);
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
        // Field-service rows are explicitly out of scope for this slice.
        let mut owned = WorkflowInboxTargets::new(None, None);
        let code = run(pool.clone(), |connection| {
            Box::pin(async move {
                let error = owned
                    .target(connection, "co", "user-1", "field_service_order", "job-1", 0)
                    .await
                    .expect_err("field-service must defer");
                Ok(error.code)
            })
        })
        .await
        .unwrap();
        assert_eq!(code, "INBOX_TARGET_PORT_UNAVAILABLE");
        let mut targets =
            PgInboxTargets::with_delegate(None, None, WorkflowInboxTargets::new(None, None));
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
                            WorkflowInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "handbook", "art-1", 0)
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
            "SELECT (SELECT count(*) FROM staff_project_handoffs WHERE company_id='co'), \
             (SELECT count(*) FROM maintenance_work_orders WHERE company_id='co'), \
             (SELECT count(*) FROM finance_supplier_batches WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
    })
    .await;
}
