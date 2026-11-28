//! Postgres proof of the native Payroll Inbox visibility delegate.
//!
//! The oracle defines no denial codes for these two source types: every
//! negative path is invisible (`Ok(None)`), in enforced and fallback layers
//! alike. The enforced-mode tests pin `PackageEnforcementMode::Enforced`
//! through the delegate's explicit mode seam instead of mutating process
//! environment (unsafe env mutation is forbidden by workspace lints).
//! Ambient mode stays disabled, so the fallback layer-2 path is exercised
//! exactly like non-enforced development.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_collaboration_server::public::inbox::{
    InboxTarget, InboxTargetPort, PayrollInboxTargets, PgInboxTargets,
};
use erpx_commercial_server::public::purchases::PackageEnforcementMode;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::json;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','payroll_reader','Payroll reader',false,1,'active'); \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','payroll_reader',1,'[\"payroll.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
         ('grant-1','user-1','payroll_reader','[]','co',0,9007199254740991,1,'holding'); \
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES \
         ('person-1','user-1','co','Worker','TH','maker',0,0), \
         ('person-3',NULL,'co','Contractor','TH','maker',0,0); \
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES \
         ('eng-1','person-1','co','br','employee','salaried','active','2026-01-01','Worker','THB','maker',0,0), \
         ('eng-3','person-3','co',NULL,'contractor','hourly','active','2026-01-01','Contractor','THB','maker',0,0); \
         INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,created_by,created_at,updated_at) VALUES \
         ('pg-1','co','Group','TH','THB','monthly','maker',0,0); \
         INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,created_by,created_at,updated_at) VALUES \
         ('run-1','co','pg-1','2026-11-01','2026-11-30','2026-12-05','THB','maker',1,1); \
         INSERT INTO hr_payroll_corrections(id,run_id,company_id,run_version,run_revision,input_digest,totals_json,reason,requested_by,created_at,updated_at) VALUES \
         ('cor-1','run-1','co',1,1,'digest','{}','Fix','user-1',1,1); \
         INSERT INTO hr_payroll_advances(id,company_id,engagement_id,person_id,amount_minor,installment_count,first_deduction_period,currency,reason,requested_by,created_at,updated_at) VALUES \
         ('adv-1','co','eng-1','person-1',100,2,'2026-12','THB','Advance','user-1',1,1), \
         ('adv-2','co','eng-3','person-3',100,2,'2026-12','THB','Advance','user-3',1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_people_grants(pool: &DatabasePool) {
    // History for people.payroll needs the people foundation beside the
    // people-payroll grant, exactly like the retained catalogue evaluation.
    // The grant events must land in the same transaction: the deferred audit
    // trigger rejects a grant without its matching event at commit.
    let mut tx = pool.raw().begin().await.unwrap();
    for (package, event) in [("people", "evt-people"), ("people-payroll", "evt-pp")] {
        sqlx::query(
            "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES \
             ('co',$1,1,'active',0,NULL,1,'fixture','test:people-history',0)",
        )
        .bind(package)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES \
             ($1,'co',$2,'fixture','test',NULL,$3::jsonb,0)",
        )
        .bind(event)
        .bind(package)
        .bind(
            serde_json::json!({
                "companyId": "co",
                "packageId": package,
                "packageVersion": 1,
                "state": "active",
                "startsAt": 0,
                "endsAt": None::<i64>,
                "version": 1,
                "reason": "fixture",
                "evidence": "test:people-history",
            })
            .to_string(),
        )
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
            let mut delegate = PayrollInboxTargets::new(branch_id.as_deref(), None);
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

fn fallback_visible(
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

#[tokio::test]
async fn correction_visible_with_exact_href_and_no_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Ambient disabled mode runs the layer-2 fallback: the holding
        // payroll.read grant resolves the run.
        let target = fallback_visible(pool.clone(), "user-1", "payroll_correction", "cor-1")
            .await
            .expect("held correction must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/people/payroll/runs/run-1#correction-cor-1")
        );
        // No source override: the composition final map applies the retained
        // correction default.
        assert_eq!(target.source, None);
        // Layer 2 has no frame check: a branch frame still resolves here.
        let framed = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "payroll_correction",
            "cor-1",
            Some("br"),
            None,
        )
        .await
        .unwrap()
        .expect("fallback ignores the selected frame");
        assert_eq!(
            framed.href.as_deref(),
            Some("/people/payroll/runs/run-1#correction-cor-1")
        );
    })
    .await;
}

#[tokio::test]
async fn advance_visible_for_self_and_privileged_without_source() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Self arm: user-1 is the advance's person.
        let target = fallback_visible(pool.clone(), "user-1", "payroll_advance", "adv-1")
            .await
            .expect("own advance must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/people/payroll/advances#payroll-advance-adv-1")
        );
        assert_eq!(target.source, None);
        // Privileged arm: user-1 holds payroll.read while person-3 has no
        // linked account, so only the holding grant can resolve adv-2.
        let target = fallback_visible(pool.clone(), "user-1", "payroll_advance", "adv-2")
            .await
            .expect("privileged advance must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/people/payroll/advances#payroll-advance-adv-2")
        );
        assert_eq!(target.source, None);
    })
    .await;
}

#[tokio::test]
async fn enforced_layer_with_grants_resolves_all_rows() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_people_grants(&pool).await;
        for (source_type, source_id, href) in [
            (
                "payroll_correction",
                "cor-1",
                "/people/payroll/runs/run-1#correction-cor-1",
            ),
            (
                "payroll_advance",
                "adv-1",
                "/people/payroll/advances#payroll-advance-adv-1",
            ),
            (
                "payroll_advance",
                "adv-2",
                "/people/payroll/advances#payroll-advance-adv-2",
            ),
        ] {
            let target = composed_target(
                pool.clone(),
                "co",
                "user-1",
                source_type,
                source_id,
                None,
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{source_id} must resolve in the enforced layer"));
            assert_eq!(target.href.as_deref(), Some(href), "{source_id}");
            assert_eq!(target.source, None, "{source_id}");
        }
    })
    .await;
}

#[tokio::test]
async fn enforced_branch_frame_keeps_only_frame_matched_self_reads() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        seed_people_grants(&pool).await;
        // Correction needs a holding frame: the branch frame hides it even
        // though the grant, run and purchase all hold.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "payroll_correction",
                "cor-1",
                Some("br"),
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .is_none(),
            "branch frame must hide the correction in the enforced layer"
        );
        // Self arm: user-1 is adv-1's person and the frame matches the
        // engagement scope, so it stays visible.
        let target = composed_target(
            pool.clone(),
            "co",
            "user-1",
            "payroll_advance",
            "adv-1",
            Some("br"),
            Some(PackageEnforcementMode::Enforced),
        )
        .await
        .unwrap()
        .expect("frame-matched self advance must resolve");
        assert_eq!(
            target.href.as_deref(),
            Some("/people/payroll/advances#payroll-advance-adv-1")
        );
        // Privileged arm needs a holding frame, and user-1 is not adv-2's
        // person: the branch frame hides it.
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "payroll_advance",
                "adv-2",
                Some("br"),
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .is_none(),
            "branch frame must hide the privileged-only advance"
        );
    })
    .await;
}

#[tokio::test]
async fn unpurchased_and_unauthorized_rows_are_invisible() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        // Enforced without purchase history: the retained predicate filters
        // the rows out instead of throwing.
        for (source_type, source_id) in [
            ("payroll_correction", "cor-1"),
            ("payroll_advance", "adv-1"),
        ] {
            assert!(
                composed_target(
                    pool.clone(),
                    "co",
                    "user-1",
                    source_type,
                    source_id,
                    None,
                    Some(PackageEnforcementMode::Enforced),
                )
                .await
                .unwrap()
                .is_none(),
                "{source_id} must stay invisible without purchase history"
            );
        }
        // No holding payroll.read grant: neither layer can resolve.
        for (source_type, source_id) in [
            ("payroll_correction", "cor-1"),
            ("payroll_advance", "adv-1"),
        ] {
            assert!(
                fallback_visible(pool.clone(), "user-2", source_type, source_id)
                    .await
                    .is_none(),
                "{source_id} must stay invisible without authority"
            );
        }
        // Missing rows are invisible in both layers.
        assert!(
            fallback_visible(pool.clone(), "user-1", "payroll_correction", "missing")
                .await
                .is_none()
        );
        seed_people_grants(&pool).await;
        assert!(
            composed_target(
                pool.clone(),
                "co",
                "user-1",
                "payroll_advance",
                "missing",
                None,
                Some(PackageEnforcementMode::Enforced),
            )
            .await
            .unwrap()
            .is_none()
        );
    })
    .await;
}

#[tokio::test]
async fn unknown_sources_fail_closed_without_inventing_links() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut delegate = PayrollInboxTargets::new(None, None);
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
            PgInboxTargets::with_delegate(None, None, PayrollInboxTargets::new(None, None));
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
                            PayrollInboxTargets::new(None, None),
                        );
                        let resolved = targets
                            .target(connection, "co", "user-1", "payroll_correction", "cor-1", 0)
                            .await?;
                        assert!(resolved.is_some());
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
        // The read-only port staged no rows: the seed image is unchanged.
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM hr_payroll_corrections WHERE company_id='co'), \
             (SELECT count(*) FROM hr_payroll_advances WHERE company_id='co')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 2));
    })
    .await;
}
