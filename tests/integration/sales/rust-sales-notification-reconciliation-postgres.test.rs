//! Native Sales customer-notice reconciliation on migrated PostgreSQL.
//!
//! Proves the exact uncertain attempt, independent current reviewer, provider
//! truth matching, immutable reconciliation/receipt/audit/outbox evidence,
//! draining settlement, late-authority rollback with exact retry and two-Company
//! concurrency without retry masking.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_sales::public::sales_notification_execution::PgNotificationExecution;
use erpx_server_sales::public::sales_notification_reconciliation::{
    PgNotificationReconciliation, ProviderEvent, ProviderObservation, ProviderObservationPort,
    ReconcileActor, ReconcileFuture, ReconcileInput, ReconcileReceipt,
    reconcile_customer_notification,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const NOW: i64 = 1_800_000_000_000;
const PROVIDER: &str = "provider-message-0001";
const REASON: &str = "Independent current approver verifies the original provider identity";

type Error = PgTransactionError<BusinessError>;

fn code(error: &Error) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        PgTransactionError::Persistence(_) => "PERSISTENCE",
    }
}

fn notice_id(company: &str) -> String {
    format!("notice-{company}")
}

fn attempt_id(company: &str) -> String {
    format!("attempt-{company}")
}

fn queue_command(company: &str) -> String {
    format!("queue-command-{company}")
}

fn actor(company: &str, user: &str) -> ReconcileActor {
    ReconcileActor {
        user_id: user.into(),
        person_user_ids: vec![user.into()],
        company_id: company.into(),
        branch_id: None,
        department_id: None,
        assignment_id: format!("assignment-{user}"),
        role: Some("sales_manager".into()),
        session_id: None,
        assignment_version: None,
        permissions: vec![
            "commerce.read".into(),
            "commerce.sales".into(),
            "commerce.approve".into(),
        ],
    }
}

struct Truth {
    observation: Option<ProviderObservation>,
    error: Option<BusinessError>,
    calls: AtomicUsize,
}

impl Truth {
    fn delivered() -> Self {
        Self {
            observation: Some(ProviderObservation {
                id: PROVIDER.into(),
                to: vec!["receiver@example.invalid".into()],
                subject: "Delivery recorded".into(),
                text: "Immutable delivery snapshot".into(),
                created_at: NOW - 500,
                last_event: ProviderEvent::Delivered,
            }),
            error: None,
            calls: AtomicUsize::new(0),
        }
    }
}

impl ProviderObservationPort for Truth {
    fn observe<'a>(
        &'a self,
        _: &'a str,
    ) -> ReconcileFuture<'a, ProviderObservation, BusinessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.error.as_ref() {
            let error = error.clone();
            return Box::pin(async move { Err(error) });
        }
        let value = self.observation.clone().expect("seeded observation");
        Box::pin(async move { Ok(value) })
    }
}

fn input() -> ReconcileInput {
    ReconcileInput {
        expected_version: 2,
        attempt_id: "attempt-company".into(),
        reason: REASON.into(),
    }
}

fn input_for(company: &str) -> ReconcileInput {
    ReconcileInput {
        attempt_id: attempt_id(company),
        ..input()
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

fn company_ids(company: &str) -> (String, String, String) {
    (
        format!("creator-{company}"),
        format!("reviewer-{company}"),
        format!("branch-{company}"),
    )
}

async fn seed_company(pool: &DatabasePool, company: &str) {
    let (creator, reviewer, branch) = company_ids(company);
    let sql = format!(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('{company}','company','{company}','Company',NULL,'{company}',NULL,'active',1),\
         ('{branch}','branch','{branch}','Branch',NULL,'{company}',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('{company}','{creator}','local:{creator}','active','local',1),\
         ('{company}','{reviewer}','local:{reviewer}','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-{creator}','{creator}','sales_manager','[]','{company}','{branch}',0,9000000000000,NULL,'branch','{branch}','manual'),\
         ('assignment-{reviewer}','{reviewer}','sales_manager','[]','{company}',NULL,0,9000000000000,NULL,'holding',NULL,'manual');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,notification_email,notification_consent,created_at,updated_at) VALUES \
         ('party-{company}','{company}','customer','C1','Customer','THB',30,0,'active',1,'receiver@example.invalid','granted',1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-{company}','{company}','{branch}','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES \
         ('order-{company}','{company}','{branch}',NULL,'sales','SO-1','party-{company}','warehouse-{company}','THB','2026-11-01','2026-11-08','','fulfilled',1000,0,NULL,3,'{creator}','{creator}',100,300);\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES \
         ('fulfillment-{company}','order-{company}','F-1','2026-11-02','{creator}',100,1);\
         INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) VALUES \
         ('{company}','resend',1,'{creator}',100);"
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(pool.raw())
        .await
        .unwrap();
}

async fn grant(pool: &DatabasePool, company: &str, state: &str) {
    let mut transaction = pool.raw().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) \
         VALUES($1,'sales',1,$2,0,NULL,1,'Actual isolated Sales notice purchase','test:sales-notification-reconciliation',$3)",
    )
    .bind(company)
    .bind(state)
    .bind(NOW)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
         SELECT 'grant-event-'||$1,company_id,package_id,'fixture','synthetic-reconciliation-grant',NULL, \
         jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version, \
           'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 \
         FROM company_package_grants WHERE company_id=$1 AND package_id='sales'",
    )
    .bind(company)
    .bind(NOW)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

/// Seed the exact durable uncertain attempt a lost provider response leaves.
async fn seed_notice(pool: &DatabasePool, company: &str, provider_id: Option<&str>) {
    let (creator, _, branch) = company_ids(company);
    sqlx::query(
        "INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,fulfillment_id,\
         party_id,party_version,recipient_email,subject,body_text,adapter,status,version,attempts,\
         next_attempt_at,lease_until,provider_message_id,created_by,created_at,updated_at) \
         VALUES($1,$2,$3,'order-'||$2,'fulfillment-'||$2,'party-'||$2,1,\
         'receiver@example.invalid','Delivery recorded','Immutable delivery snapshot','resend',\
         'unknown',2,1,$4,NULL,$5,$6,100,100)",
    )
    .bind(notice_id(company))
    .bind(company)
    .bind(&branch)
    .bind(NOW)
    .bind(provider_id)
    .bind(&creator)
    .execute(pool.raw())
    .await
    .unwrap();
    let worker = PgNotificationExecution::new(pool.clone(), PackageEnforcementMode::Disabled, false);
    let mut connection = pool.raw().acquire().await.unwrap();
    let source = worker
        .current_source(&mut connection, &notice_id(company))
        .await
        .unwrap()
        .expect("seeded notice source");
    assert!(source.release_ready);
    let authority = serde_json::json!({
        "userId": creator.clone(),
        "assignmentId": format!("assignment-{creator}"),
        "role": "sales_manager",
        "companyId": company,
        "branchId": branch.clone(),
        "iam": {},
    })
    .to_string();
    sqlx::query(
        "INSERT INTO sales_notification_execution_authorizations(command_id,notification_id,company_id,\
         kind,actor_id,assignment_id,authority_json,source_hash,notification_version,created_at) \
         VALUES($1,$2,$3,'queue',$4,$5,$6,$7,1,100)",
    )
    .bind(queue_command(company))
    .bind(notice_id(company))
    .bind(company)
    .bind(&creator)
    .bind(format!("assignment-{creator}"))
    .bind(&authority)
    .bind(&source.source_hash)
    .execute(&mut *connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,'commerce.customer-notification.queue',$2,$3,$4,$5,100,$6)",
    )
    .bind(queue_command(company))
    .bind(&creator)
    .bind("0".repeat(64))
    .bind(format!("fulfillment-{company}"))
    .bind(serde_json::json!({"id": notice_id(company)}).to_string())
    .bind(&authority)
    .execute(&mut *connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sales_notification_execution_attempts(id,notification_id,authorization_command_id,\
         company_id,attempt,lease_until,source_hash,state,provider_message_id,result_code,created_at,updated_at) \
         VALUES($1,$2,$3,$4,1,$5,$6,'unknown',$7,'CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN',$8,$8)",
    )
    .bind(attempt_id(company))
    .bind(notice_id(company))
    .bind(queue_command(company))
    .bind(company)
    .bind(NOW)
    .bind(&source.source_hash)
    .bind(provider_id)
    .bind(NOW - 1_000)
    .execute(&mut *connection)
    .await
    .unwrap();
}

async fn reconcile(
    pool: &DatabasePool,
    company: &str,
    reviewer: &str,
    key: &str,
    mode: PackageEnforcementMode,
    provider: Arc<dyn ProviderObservationPort>,
    input: ReconcileInput,
) -> Result<ReconcileReceipt, Error> {
    let reviewer = actor(company, reviewer);
    let notice = notice_id(company);
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let reviewer = reviewer.clone();
            let input = input.clone();
            let key = key.to_owned();
            let notice = notice.clone();
            let provider = Arc::clone(&provider);
            Box::pin(async move {
                let mut repository = PgNotificationReconciliation::new(connection, mode);
                reconcile_customer_notification(
                    &reviewer,
                    &notice,
                    input,
                    &key,
                    &mut repository,
                    provider.as_ref(),
                    NOW,
                )
                .await
            })
        })
        .await
}

async fn notice(pool: &DatabasePool, company: &str) -> (String, i64, Option<String>, String) {
    let row = sqlx::query(
        "SELECT status,version,provider_message_id,created_by FROM commerce_customer_notifications WHERE id=$1",
    )
    .bind(notice_id(company))
    .fetch_one(pool.raw())
    .await
    .unwrap();
    (
        row.get("status"),
        row.get("version"),
        row.get("provider_message_id"),
        row.get("created_by"),
    )
}

async fn attempt_snapshot(
    pool: &DatabasePool,
    company: &str,
) -> (String, Option<String>, String, String) {
    let row = sqlx::query(
        "SELECT to_jsonb(row)::text value,state,provider_message_id,source_hash \
         FROM sales_notification_execution_attempts row WHERE id=$1",
    )
    .bind(attempt_id(company))
    .fetch_one(pool.raw())
    .await
    .unwrap();
    (
        row.get("value"),
        row.get("provider_message_id"),
        row.get("source_hash"),
        row.get("state"),
    )
}

async fn counts(pool: &DatabasePool, key: &str) -> (i64, i64, i64, i64) {
    let row = sqlx::query(
        "SELECT (SELECT count(*) FROM sales_notification_reconciliations WHERE command_id=$1) reconciliations,\
         (SELECT count(*) FROM command_receipts WHERE command_id=$1) receipts,\
         (SELECT count(*) FROM audit_events WHERE command_id=$1) audits,\
         (SELECT count(*) FROM outbox_events WHERE command_id=$1) outbox",
    )
    .bind(key)
    .fetch_one(pool.raw())
    .await
    .unwrap();
    (
        row.get("reconciliations"),
        row.get("receipts"),
        row.get("audits"),
        row.get("outbox"),
    )
}

/// The retained package-obligation settlement predicate for one unknown attempt.
async fn settled(pool: &DatabasePool, company: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sales_notification_reconciliations resolution \
         JOIN command_receipts receipt ON receipt.command_id=resolution.command_id \
           AND receipt.record_id=$1 \
           AND receipt.operation='commerce.customer-notification.reconcile' \
           AND receipt.actor_id=resolution.actor_id \
           AND receipt.authority_json::jsonb->>'companyId'=resolution.company_id \
         JOIN audit_events audit ON audit.command_id=resolution.command_id \
           AND audit.record_id=$1 AND audit.record_version=resolution.notification_version \
           AND audit.action=receipt.operation \
         JOIN sales_notification_execution_authorizations authority \
           ON authority.command_id=$2 \
         JOIN sales_notification_execution_attempts attempt ON attempt.id=resolution.attempt_id \
         JOIN commerce_customer_notifications notice ON notice.id=resolution.notification_id \
         WHERE resolution.notification_id=$1 \
           AND resolution.source_hash=attempt.source_hash \
           AND resolution.provider_message_id=attempt.provider_message_id \
           AND resolution.provider_message_id=notice.provider_message_id \
           AND resolution.actor_id<>authority.actor_id AND resolution.actor_id<>notice.created_by \
           AND resolution.notification_version=notice.version AND resolution.outcome=notice.status \
           AND receipt.result_json::jsonb->>'reconciliationId'=resolution.id \
           AND receipt.result_json::jsonb->>'status'=resolution.outcome \
           AND receipt.result_json::jsonb->>'version'=resolution.notification_version::text \
           AND resolution.provider_observation_json::jsonb->>'id'=resolution.provider_message_id \
           AND resolution.provider_observation_json::jsonb->'to'=jsonb_build_array(notice.recipient_email) \
           AND resolution.provider_observation_json::jsonb->>'subject'=notice.subject \
           AND resolution.provider_observation_json::jsonb->>'text'=notice.body_text \
           AND resolution.provider_observation_json::jsonb->>'last_event'=resolution.provider_event)",
    )
    .bind(notice_id(company))
    .bind(queue_command(company))
    .fetch_one(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn independent_reviewer_settles_the_exact_uncertain_attempt_and_replays() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_company(&pool, "company").await;
        grant(&pool, "company", "draining").await;
        seed_notice(&pool, "company", Some(PROVIDER)).await;
        let provider = Arc::new(Truth::delivered());
        let (before_attempt, _, source_hash, _) = attempt_snapshot(&pool, "company").await;
        assert!(!settled(&pool, "company").await);

        let requester = reconcile(
            &pool,
            "company",
            "creator-company",
            "reconcile-requester-0001",
            PackageEnforcementMode::Enforced,
            provider.clone(),
            input_for("company"),
        )
        .await
        .unwrap_err();
        assert_eq!(code(&requester), "INDEPENDENT_NOTIFICATION_REVIEWER_REQUIRED");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            notice(&pool, "company").await,
            ("unknown".into(), 2, Some(PROVIDER.into()), "creator-company".into())
        );

        let receipt = reconcile(
            &pool,
            "company",
            "reviewer-company",
            "reconcile-reviewer-0001",
            PackageEnforcementMode::Enforced,
            provider.clone(),
            input_for("company"),
        )
        .await
        .unwrap();
        assert_eq!(receipt.status, "sent");
        assert_eq!(receipt.version, 3);
        assert_eq!(receipt.order_id, "order-company");
        assert_eq!(
            receipt.reconciliation_id,
            "sales-notification-reconciliation-reconcile-reviewer-0001"
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            notice(&pool, "company").await,
            ("sent".into(), 3, Some(PROVIDER.into()), "creator-company".into())
        );
        assert_eq!(
            attempt_snapshot(&pool, "company").await,
            (before_attempt, Some(PROVIDER.into()), source_hash, "unknown".into())
        );
        assert_eq!(
            counts(&pool, "reconcile-reviewer-0001").await,
            (1, 1, 1, 1)
        );
        assert!(settled(&pool, "company").await);
        let reconciliation = sqlx::query(
            "SELECT provider_event,outcome,expected_version,notification_version,reason \
             FROM sales_notification_reconciliations WHERE command_id='reconcile-reviewer-0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(reconciliation.get::<String, _>("provider_event"), "delivered");
        assert_eq!(reconciliation.get::<String, _>("outcome"), "sent");
        assert_eq!(reconciliation.get::<i64, _>("expected_version"), 2);
        assert_eq!(reconciliation.get::<i64, _>("notification_version"), 3);
        assert_eq!(reconciliation.get::<String, _>("reason"), REASON);

        let replay = reconcile(
            &pool,
            "company",
            "reviewer-company",
            "reconcile-reviewer-0001",
            PackageEnforcementMode::Enforced,
            provider.clone(),
            input_for("company"),
        )
        .await
        .unwrap();
        assert_eq!(replay, receipt);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        let conflict = reconcile(
            &pool,
            "company",
            "reviewer-company",
            "reconcile-reviewer-0001",
            PackageEnforcementMode::Enforced,
            provider,
            ReconcileInput { expected_version: 1, ..input_for("company") },
        )
        .await
        .unwrap_err();
        assert_eq!(code(&conflict), "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn provider_truth_and_scope_fail_closed_without_any_evidence() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;

        seed_company(&pool, "company").await;
        grant(&pool, "company", "active").await;
        seed_notice(&pool, "company", None).await;
        let provider = Arc::new(Truth::delivered());
        let missing = reconcile(
            &pool,
            "company",
            "reviewer-company",
            "reconcile-missing-0001",
            PackageEnforcementMode::Enforced,
            provider.clone(),
            input_for("company"),
        )
        .await
        .unwrap_err();
        assert_eq!(code(&missing), "CUSTOMER_NOTIFICATION_PROVIDER_ID_REQUIRED");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(counts(&pool, "reconcile-missing-0001").await, (0, 0, 0, 0));

        seed_company(&pool, "company-m").await;
        grant(&pool, "company-m", "active").await;
        seed_notice(&pool, "company-m", Some(PROVIDER)).await;
        for (label, mutate) in [
            (
                "id",
                Box::new(|value: &mut ProviderObservation| value.id = "different-provider".into())
                    as Box<dyn Fn(&mut ProviderObservation) + Send + Sync>,
            ),
            (
                "recipient",
                Box::new(|value: &mut ProviderObservation| {
                    value.to = vec!["other@example.invalid".into()]
                }),
            ),
            (
                "subject",
                Box::new(|value: &mut ProviderObservation| value.subject = "Changed subject".into()),
            ),
            (
                "text",
                Box::new(|value: &mut ProviderObservation| value.text = "Changed message".into()),
            ),
            (
                "window",
                Box::new(|value: &mut ProviderObservation| value.created_at = NOW + 5_000),
            ),
        ] {
            let mut observation = Truth::delivered().observation.unwrap();
            mutate(&mut observation);
            let provider = Arc::new(Truth {
                observation: Some(observation),
                error: None,
                calls: AtomicUsize::new(0),
            });
            let key = format!("reconcile-{label}-0001");
            let error = reconcile(
                &pool,
                "company-m",
                "reviewer-company-m",
                &key,
                PackageEnforcementMode::Enforced,
                provider,
                input_for("company-m"),
            )
            .await
            .unwrap_err();
            assert_eq!(code(&error), "CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH", "{label}");
            assert_eq!(counts(&pool, &key).await, (0, 0, 0, 0));
        }

        seed_company(&pool, "company-u").await;
        grant(&pool, "company-u", "active").await;
        seed_notice(&pool, "company-u", Some(PROVIDER)).await;
        let provider = Arc::new(Truth {
            observation: None,
            error: Some(erpx_server_sales::public::sales_notification_reconciliation::provider_unconfirmed()),
            calls: AtomicUsize::new(0),
        });
        let unconfirmed = reconcile(
            &pool,
            "company-u",
            "reviewer-company-u",
            "reconcile-unconfirmed-0001",
            PackageEnforcementMode::Enforced,
            provider,
            input_for("company-u"),
        )
        .await
        .unwrap_err();
        assert_eq!(code(&unconfirmed), "CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED");

        let mut foreign = actor("company-m", "reviewer-company-m");
        foreign.branch_id = Some("other-branch".into());
        let provider = Arc::new(Truth::delivered());
        let result = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), {
                let provider = provider.clone();
                let notice = notice_id("company-m");
                let input = input_for("company-m");
                move |connection| {
                    let foreign = foreign.clone();
                    let provider = Arc::clone(&provider);
                    let notice = notice.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        let mut repository = PgNotificationReconciliation::new(
                            connection,
                            PackageEnforcementMode::Enforced,
                        );
                        reconcile_customer_notification(
                            &foreign,
                            &notice,
                            input,
                            "reconcile-foreign-0001",
                            &mut repository,
                            provider.as_ref(),
                            NOW,
                        )
                        .await
                    })
                }
            })
            .await;
        assert_eq!(code(&result.unwrap_err()), "CUSTOMER_NOTIFICATION_NOT_FOUND");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(notice(&pool, "company").await.0, "unknown");
        assert_eq!(notice(&pool, "company-m").await.0, "unknown");
        assert!(!settled(&pool, "company").await);
        assert!(!settled(&pool, "company-m").await);
    })
    .await;
}

#[tokio::test]
async fn late_current_reviewer_revocation_rolls_back_then_the_exact_retry_settles() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_company(&pool, "company").await;
        grant(&pool, "company", "active").await;
        seed_notice(&pool, "company", Some(PROVIDER)).await;
        let provider = Arc::new(Truth::delivered());
        sqlx::raw_sql(
            "CREATE FUNCTION sales_reconcile_authority() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN UPDATE assignments SET revoked_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint,version=version+1 \
             WHERE id='assignment-reviewer-company';RETURN NEW;END $$;\
             CREATE CONSTRAINT TRIGGER sales_reconcile_authority AFTER INSERT ON sales_notification_reconciliations \
             DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='reconcile-late-0001') \
             EXECUTE FUNCTION sales_reconcile_authority();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let late = reconcile(
            &pool,
            "company",
            "reviewer-company",
            "reconcile-late-0001",
            PackageEnforcementMode::Enforced,
            provider.clone(),
            input_for("company"),
        )
        .await
        .unwrap_err();
        assert_eq!(code(&late), "QUEUED_AUTHORITY_CHANGED");
        sqlx::raw_sql(
            "DROP TRIGGER sales_reconcile_authority ON sales_notification_reconciliations;\
             DROP FUNCTION sales_reconcile_authority();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_eq!(notice(&pool, "company").await.0, "unknown");
        assert_eq!(counts(&pool, "reconcile-late-0001").await, (0, 0, 0, 0));
        assert!(
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT revoked_at FROM assignments WHERE id='assignment-reviewer-company'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap()
            .is_none()
        );

        let receipt = reconcile(
            &pool,
            "company",
            "reviewer-company",
            "reconcile-late-0001",
            PackageEnforcementMode::Enforced,
            provider,
            input_for("company"),
        )
        .await
        .unwrap();
        assert_eq!(receipt.status, "sent");
        assert_eq!(attempt_snapshot(&pool, "company").await.1, Some(PROVIDER.into()));
        assert_eq!(attempt_snapshot(&pool, "company").await.3, "unknown");
        assert!(settled(&pool, "company").await);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_company_observations_settle_without_deadlock_or_retry_masking() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        for company in ["company-a", "company-b"] {
            seed_company(&pool, company).await;
            grant(&pool, company, "active").await;
            seed_notice(&pool, company, Some(PROVIDER)).await;
        }
        let first = reconcile(
            &pool,
            "company-a",
            "reviewer-company-a",
            "reconcile-company-a-0001",
            PackageEnforcementMode::Enforced,
            Arc::new(Truth::delivered()),
            input_for("company-a"),
        );
        let second = reconcile(
            &pool,
            "company-b",
            "reviewer-company-b",
            "reconcile-company-b-0001",
            PackageEnforcementMode::Enforced,
            Arc::new(Truth::delivered()),
            input_for("company-b"),
        );
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.unwrap().status, "sent");
        assert_eq!(second.unwrap().status, "sent");
        assert_eq!(counts(&pool, "reconcile-company-a-0001").await, (1, 1, 1, 1));
        assert_eq!(counts(&pool, "reconcile-company-b-0001").await, (1, 1, 1, 1));
    })
    .await;
}
