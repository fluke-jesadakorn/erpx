//! Native Sales customer-notice delivery on migrated PostgreSQL.
//!
//! Proves the durable claim, immutable attempt, source/consent re-check,
//! production release gate, at-most-once provider invocation, `unknown`
//! takeover and terminal/replay states of `PgNotificationExecution`.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_sales::public::sales_notification_execution::{
    DispatchOutcome, DispatchPlan, EmailDispatchPort, ExecutionFuture, OutboundEmail,
    PgNotificationExecution, deliver_notification, drain_notifications, not_configured,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_800_000_000_000;
const COMPANY: &str = "company";
const USER: &str = "creator";
const ASSIGNMENT: &str = "assignment-creator";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Behaviour {
    Accepted,
    Uncertain,
    NotConfigured,
}

type DispatchRecord = (String, String, String, DispatchPlan);

#[derive(Clone)]
struct TestDispatch {
    behaviour: Behaviour,
    calls: Arc<Mutex<Vec<DispatchRecord>>>,
}

impl TestDispatch {
    fn new(behaviour: Behaviour) -> Self {
        Self {
            behaviour,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl EmailDispatchPort for TestDispatch {
    fn prepare(&self, plan: DispatchPlan) -> Result<(), BusinessError> {
        if self.behaviour == Behaviour::NotConfigured && plan == DispatchPlan::Resend {
            return Err(not_configured());
        }
        Ok(())
    }

    fn dispatch<'a>(
        &'a self,
        email: OutboundEmail<'a>,
    ) -> ExecutionFuture<'a, DispatchOutcome, BusinessError> {
        self.calls.lock().unwrap().push((
            email.notification_id.to_owned(),
            email.recipient_email.to_owned(),
            email.subject.to_owned(),
            email.plan,
        ));
        let outcome = match self.behaviour {
            Behaviour::Accepted => DispatchOutcome::Accepted {
                provider_id: "declared-provider-message".into(),
            },
            Behaviour::Uncertain => DispatchOutcome::Uncertain,
            Behaviour::NotConfigured => return Box::pin(async { Err(not_configured()) }),
        };
        Box::pin(async move { Ok(outcome) })
    }
}

fn worker(pool: &DatabasePool, production: bool) -> PgNotificationExecution {
    worker_mode(pool, production, PackageEnforcementMode::Disabled)
}

fn worker_mode(
    pool: &DatabasePool,
    production: bool,
    mode: PackageEnforcementMode,
) -> PgNotificationExecution {
    PgNotificationExecution::new(pool.clone(), mode, production)
}

async fn run(
    pool: &DatabasePool,
    dispatch: &TestDispatch,
    notification: &str,
    production: bool,
) -> Result<Option<String>, PgTransactionError<BusinessError>> {
    run_mode(
        pool,
        dispatch,
        notification,
        production,
        PackageEnforcementMode::Disabled,
    )
    .await
}

async fn run_mode(
    pool: &DatabasePool,
    dispatch: &TestDispatch,
    notification: &str,
    production: bool,
    mode: PackageEnforcementMode,
) -> Result<Option<String>, PgTransactionError<BusinessError>> {
    deliver_notification(
        &worker_mode(pool, production, mode),
        Arc::new(dispatch.clone()),
        notification,
        NOW,
    )
    .await
}

async fn drain(
    pool: &DatabasePool,
    dispatch: &TestDispatch,
    production: bool,
) -> Result<u32, PgTransactionError<BusinessError>> {
    drain_mode(pool, dispatch, production, PackageEnforcementMode::Disabled).await
}

async fn drain_mode(
    pool: &DatabasePool,
    dispatch: &TestDispatch,
    production: bool,
    mode: PackageEnforcementMode,
) -> Result<u32, PgTransactionError<BusinessError>> {
    drain_notifications(
        &worker_mode(pool, production, mode),
        Arc::new(dispatch.clone()),
        NOW,
        10,
    )
    .await
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

async fn seed_source(pool: &DatabasePool, adapter: &str) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','creator','local:creator','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-creator','creator','sales_manager','[]','company','branch',0,9000000000000,NULL,'branch','branch','manual');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,notification_email,notification_consent,created_at,updated_at) VALUES \
         ('party-1','company','customer','C1','Customer','THB',30,0,'active',1,'receiver@example.invalid','granted',1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES \
         ('order-1','company','branch',NULL,'sales','SO-1','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',1000,0,NULL,3,'creator','creator',100,300);\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES \
         ('fulfillment-1','order-1','F-1','2026-11-02','creator',100,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) \
         VALUES($1,$2,1,$3,100)",
    )
    .bind(COMPANY)
    .bind(adapter)
    .bind(USER)
    .execute(pool.raw())
    .await
    .unwrap();
}

fn authority_json() -> String {
    serde_json::json!({
        "userId": USER,
        "assignmentId": ASSIGNMENT,
        "role": "sales_manager",
        "companyId": COMPANY,
        "branchId": "branch",
        "departmentId": null,
        "iam": {"queuedAssignmentVersion": 1},
    })
    .to_string()
}

/// Seed one immutable notice, its exact queued authority and receipt. The
/// authorization hash is the worker's current image hash, never a guess.
async fn seed_notice(
    pool: &DatabasePool,
    id: &str,
    adapter: &str,
    status: &str,
    attempts: i64,
    lease_until: Option<i64>,
    with_authorization: bool,
) {
    sqlx::query(
        "INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,fulfillment_id,\
         party_id,party_version,recipient_email,subject,body_text,adapter,status,attempts,\
         next_attempt_at,lease_until,created_by,created_at,updated_at) \
         VALUES($1,$2,'branch','order-1','fulfillment-1','party-1',1,'receiver@example.invalid',\
         'Delivery recorded','Immutable delivery snapshot',$3,$4,$5,$6,$7,$8,100,100)",
    )
    .bind(id)
    .bind(COMPANY)
    .bind(adapter)
    .bind(status)
    .bind(attempts)
    .bind(NOW)
    .bind(lease_until)
    .bind(USER)
    .execute(pool.raw())
    .await
    .unwrap();
    if !with_authorization {
        return;
    }
    let mut connection = pool.raw().acquire().await.unwrap();
    let source = worker(pool, false)
        .current_source(&mut connection, id)
        .await
        .unwrap()
        .expect("seeded notice source");
    assert!(source.release_ready);
    sqlx::query(
        "INSERT INTO sales_notification_execution_authorizations(command_id,notification_id,company_id,\
         kind,actor_id,assignment_id,authority_json,source_hash,notification_version,created_at) \
         VALUES($1,$2,$3,'queue',$4,$5,$6,$7,1,100)",
    )
    .bind(format!("command-{id}"))
    .bind(id)
    .bind(COMPANY)
    .bind(USER)
    .bind(ASSIGNMENT)
    .bind(authority_json())
    .bind(&source.source_hash)
    .execute(&mut *connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
         VALUES($1,'commerce.customer-notification.queue',$2,$3,'fulfillment-1',$4,100,$5)",
    )
    .bind(format!("command-{id}"))
    .bind(USER)
    .bind("0".repeat(64))
    .bind(serde_json::json!({"id": id}).to_string())
    .bind(authority_json())
    .execute(&mut *connection)
    .await
    .unwrap();
}

async fn notice(pool: &DatabasePool, id: &str) -> (String, i64, Option<String>, Option<i64>) {
    let row = sqlx::query(
        "SELECT status,attempts,provider_message_id,lease_until \
         FROM commerce_customer_notifications WHERE id=$1",
    )
    .bind(id)
    .fetch_one(pool.raw())
    .await
    .unwrap();
    (
        row.get("status"),
        row.get("attempts"),
        row.get("provider_message_id"),
        row.get("lease_until"),
    )
}

async fn attempt_states(pool: &DatabasePool, id: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT state FROM sales_notification_execution_attempts WHERE notification_id=$1 ORDER BY created_at,id")
        .bind(id)
        .fetch_all(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn sandbox_delivery_commits_one_attempt_and_replays_idempotently() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "sandbox").await;
        seed_notice(&pool, "notice-1", "sandbox", "pending", 0, None, true).await;
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        assert_eq!(
            drain_mode(&pool, &dispatch, false, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            1,
            "one delivered notice"
        );
        assert_eq!(dispatch.calls(), 0, "sandbox never contacts a provider");
        assert_eq!(
            notice(&pool, "notice-1").await,
            ("sent".into(), 1, Some("sandbox-notice-1".into()), None)
        );
        assert_eq!(attempt_states(&pool, "notice-1").await, vec!["sent"]);

        assert_eq!(
            drain_mode(&pool, &dispatch, false, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            0
        );
        assert_eq!(attempt_states(&pool, "notice-1").await, vec!["sent"]);
    })
    .await;
}

#[tokio::test]
async fn consent_revocation_cancels_before_any_provider_call() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "resend").await;
        seed_notice(&pool, "notice-2", "resend", "pending", 0, None, true).await;
        sqlx::query("UPDATE finance_parties SET notification_consent='revoked',version=version+1 WHERE id='party-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        assert_eq!(
            run_mode(
            &pool,
            &dispatch,
            "notice-2",
            false,
            PackageEnforcementMode::Shadow,
        )
        .await
        .unwrap(),
            None
        );
        assert_eq!(dispatch.calls(), 0);
        let (status, attempts, _, _) = notice(&pool, "notice-2").await;
        assert_eq!((status.as_str(), attempts), ("cancelled", 1));
        let error: String =
            sqlx::query_scalar("SELECT last_error FROM commerce_customer_notifications WHERE id='notice-2'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(error, "CUSTOMER_CONSENT_OR_EMAIL_CHANGED");
        assert_eq!(attempt_states(&pool, "notice-2").await, vec!["cancelled"]);
    })
    .await;
}

#[tokio::test]
async fn expired_sending_lease_becomes_uncertain_without_a_send() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "resend").await;
        seed_notice(
            &pool,
            "notice-3",
            "resend",
            "sending",
            8,
            Some(NOW - 1),
            true,
        )
        .await;
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        assert_eq!(
            drain_mode(&pool, &dispatch, false, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            0
        );
        assert_eq!(dispatch.calls(), 0);
        assert_eq!(
            notice(&pool, "notice-3").await,
            ("unknown".into(), 8, None, None)
        );
        assert_eq!(
            attempt_states(&pool, "notice-3").await,
            Vec::<String>::new()
        );
        let error: String = sqlx::query_scalar(
            "SELECT last_error FROM commerce_customer_notifications WHERE id='notice-3'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(error, "CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN");
    })
    .await;
}

#[tokio::test]
async fn missing_authorization_fails_closed_without_consuming_the_queue() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "sandbox").await;
        seed_notice(&pool, "notice-4", "sandbox", "pending", 0, None, false).await;
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        assert_eq!(
            drain_mode(&pool, &dispatch, false, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            notice(&pool, "notice-4").await,
            ("failed".into(), 0, None, None)
        );
        let error: String = sqlx::query_scalar(
            "SELECT last_error FROM commerce_customer_notifications WHERE id='notice-4'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(error, "CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED");
        assert_eq!(
            attempt_states(&pool, "notice-4").await,
            Vec::<String>::new()
        );
    })
    .await;
}

#[tokio::test]
async fn production_release_gate_holds_an_unverified_company_until_engineering_verification() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "sandbox").await;
        seed_notice(&pool, "notice-5", "sandbox", "pending", 0, None, true).await;
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        assert_eq!(
            drain_mode(&pool, &dispatch, true, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            notice(&pool, "notice-5").await,
            ("pending".into(), 0, None, None)
        );
        sqlx::query(
            "INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,\
             software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) \
             VALUES($1,'abcdefabcdef',$2,'release-'||$2,$2,true,$3,\
             'Declared isolated engineering verification',100)",
        )
        .bind(COMPANY)
        .bind("a".repeat(64))
        .bind(NOW + 60_000)
        .execute(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            drain_mode(&pool, &dispatch, true, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            notice(&pool, "notice-5").await,
            ("sent".into(), 1, Some("sandbox-notice-5".into()), None)
        );
    })
    .await;
}

#[tokio::test]
async fn disabled_development_boundary_delivers_without_a_purchased_authorization() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "sandbox").await;
        seed_notice(&pool, "notice-8", "sandbox", "pending", 0, None, false).await;
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        assert_eq!(drain(&pool, &dispatch, false).await.unwrap(), 1);
        assert_eq!(
            notice(&pool, "notice-8").await,
            ("sent".into(), 1, Some("sandbox-notice-8".into()), None)
        );
        assert_eq!(
            attempt_states(&pool, "notice-8").await,
            Vec::<String>::new(),
            "the disabled boundary writes no purchased attempt"
        );
    })
    .await;
}

#[tokio::test]
async fn uncertain_provider_response_is_never_resent() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "resend").await;
        seed_notice(&pool, "notice-6", "resend", "pending", 0, None, true).await;
        let dispatch = TestDispatch::new(Behaviour::Uncertain);

        let error = run_mode(
            &pool,
            &dispatch,
            "notice-6",
            false,
            PackageEnforcementMode::Shadow,
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN")
            }
            other => panic!("expected the owner uncertainty failure, got {other:?}"),
        }
        assert_eq!(dispatch.calls(), 1, "at most one provider invocation");
        assert_eq!(
            notice(&pool, "notice-6").await,
            ("unknown".into(), 1, None, None)
        );
        assert_eq!(attempt_states(&pool, "notice-6").await, vec!["unknown"]);

        assert_eq!(
            drain_mode(&pool, &dispatch, false, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            0
        );
        assert_eq!(dispatch.calls(), 1, "an uncertain attempt is never resent");
    })
    .await;
}

#[tokio::test]
async fn unconfigured_provider_fails_closed_before_marking_the_attempt_uncertain() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "resend").await;
        seed_notice(&pool, "notice-7", "resend", "pending", 0, None, true).await;
        let dispatch = TestDispatch::new(Behaviour::NotConfigured);

        assert!(
            run_mode(
                &pool,
                &dispatch,
                "notice-7",
                false,
                PackageEnforcementMode::Shadow,
            )
            .await
            .is_err()
        );
        assert_eq!(dispatch.calls(), 0);
        let (status, _, _, _) = notice(&pool, "notice-7").await;
        assert_eq!(status, "failed");
        let error: String = sqlx::query_scalar(
            "SELECT last_error FROM commerce_customer_notifications WHERE id='notice-7'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(error, "CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED");
        assert_eq!(attempt_states(&pool, "notice-7").await, vec!["failed"]);
    })
    .await;
}

#[tokio::test]
async fn post_send_commit_retry_never_invokes_the_provider_twice() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "resend").await;
        seed_notice(&pool, "notice-9", "resend", "pending", 0, None, true).await;
        sqlx::raw_sql(
            "CREATE FUNCTION sales_notice_commit_retry() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN RAISE EXCEPTION 'Declared post-send serialization fault' USING ERRCODE='40001'; END; $$;
             CREATE CONSTRAINT TRIGGER sales_notice_commit_retry AFTER UPDATE
             ON commerce_customer_notifications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
             WHEN (NEW.id='notice-9' AND NEW.status='sent')
             EXECUTE FUNCTION sales_notice_commit_retry();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        let error = run_mode(
            &pool,
            &dispatch,
            "notice-9",
            false,
            PackageEnforcementMode::Shadow,
        )
        .await
        .unwrap_err();
        match error {
            PgTransactionError::Business(error) => {
                assert_eq!(error.code, "CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN")
            }
            other => panic!("expected the owner uncertainty failure, got {other:?}"),
        }
        assert_eq!(
            dispatch.calls(),
            1,
            "one accepted provider message across the retried commit"
        );
        assert_eq!(
            notice(&pool, "notice-9").await,
            (
                "unknown".into(),
                1,
                Some("declared-provider-message".into()),
                None
            )
        );
        assert_eq!(attempt_states(&pool, "notice-9").await, vec!["unknown"]);
    })
    .await;
}

#[tokio::test]
async fn revoked_queued_assignment_fails_closed_before_any_provider_call() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool, "resend").await;
        seed_notice(&pool, "notice-10", "resend", "pending", 0, None, true).await;
        sqlx::query(
            "UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='assignment-creator'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let dispatch = TestDispatch::new(Behaviour::Accepted);

        assert_eq!(
            drain_mode(&pool, &dispatch, false, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            0
        );
        assert_eq!(dispatch.calls(), 0);
        assert_eq!(
            notice(&pool, "notice-10").await,
            ("failed".into(), 0, None, None)
        );
        assert_eq!(
            attempt_states(&pool, "notice-10").await,
            Vec::<String>::new()
        );
    })
    .await;
}
