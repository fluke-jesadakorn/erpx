//! Native Sales customer-notification settings/queue/retry on PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_sales::public::notification_commands::{
    NotificationActor, NotificationHttpCommand, PgNotificationCommands, QueueNotification,
    RetryNotification, SettingsAdapter, UpdateSettings, queue_notification, retry_notification,
    update_settings,
};
use erpx_server_sales::public::sales_notification_execution::{
    NotificationExecutionUnit, PgNotificationExecution,
};
use erpx_server_shared::public::BusinessError;

const NOW: i64 = 1_800_000_000_000;
const COMPANY: &str = "company";

type Error = PgTransactionError<BusinessError>;

fn code(error: &Error) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        PgTransactionError::Persistence(_) => "PERSISTENCE",
    }
}

fn actor(permissions: &[&str]) -> NotificationActor {
    NotificationActor {
        user_id: "owner".into(),
        company_id: COMPANY.into(),
        branch_id: Some("branch".into()),
        department_id: None,
        assignment_id: "assignment-owner".into(),
        role: "sales_manager".into(),
        session_id: None,
        assignment_version: None,
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
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

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES \
         ('company','owner','local:owner','active','local',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES \
         ('assignment-owner','owner','sales_manager','[]','company','branch',0,9000000000000,NULL,'holding',NULL,'manual');\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,notification_email,notification_consent,created_at,updated_at) VALUES \
         ('party-1','company','customer','C1','Customer','THB',30,0,'active',1,'receiver@example.invalid','granted',1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,status,version) VALUES \
         ('product-1','company','SKU-1','Item','pcs',1000,'active',1);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES \
         ('order-1','company','branch',NULL,'sales','SO-1','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',1000,0,NULL,3,'owner','owner',100,300);\
         INSERT INTO commerce_order_lines(id,order_id,ordinal,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor) VALUES \
         ('line-1','order-1',0,'product-1','Item',2000,1000,0,2000,0);\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES \
         ('fulfillment-1','order-1','F-1','2026-11-02','owner',100,1);\
         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) VALUES \
         ('fulfillment-line-1','fulfillment-1','line-1',2000,2000,2000,0);"
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn grant(pool: &DatabasePool, state: &str) {
    let mut transaction = pool.raw().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) \
         VALUES('company','sales',1,$1,0,NULL,1,'Isolated Sales notice purchase','test:notification-command',$2)",
    )
    .bind(state)
    .bind(NOW)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
         SELECT 'grant-event',company_id,package_id,'fixture','synthetic-notification-grant',NULL,\
         jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,\
           'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 \
         FROM company_package_grants WHERE company_id='company' AND package_id='sales'",
    )
    .bind(NOW)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

async fn run<T, F>(
    pool: &DatabasePool,
    mode: PackageEnforcementMode,
    production: bool,
    action: F,
) -> std::result::Result<T, Error>
where
    T: Send + 'static,
    F: for<'c> Fn(
            &'c mut PgNotificationCommands<'c>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = std::result::Result<T, Error>> + Send + 'c>,
        > + Send
        + Clone
        + 'static,
{
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let action = action.clone();
            Box::pin(async move {
                let mut repository = PgNotificationCommands::new(connection, mode, production);
                action(&mut repository).await
            })
        })
        .await
}

#[tokio::test]
async fn settings_queue_and_retry_are_owner_commands() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        grant(&pool, "active").await;
        let owner = actor(&["commerce.read", "commerce.sales", "commerce.approve"]);

        let owner_call = owner.clone();
        let settings = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                update_settings(
                    &owner,
                    UpdateSettings { expected_version: 0, adapter: SettingsAdapter::Sandbox },
                    "notification-settings-0001",
                    repo,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        assert_eq!(settings.version, 1);
        assert_eq!(settings.adapter, "sandbox");
        let owner_call = owner.clone();
        let receipt = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                update_settings(
                    &owner,
                    UpdateSettings { expected_version: 0, adapter: SettingsAdapter::Sandbox },
                    "notification-settings-0001",
                    repo,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        assert_eq!(receipt, settings);
        let owner_call = owner.clone();
        let stale = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                update_settings(
                    &owner,
                    UpdateSettings { expected_version: 1, adapter: SettingsAdapter::Resend },
                    "notification-settings-0001",
                    repo,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap_err();
        assert_eq!(code(&stale), "IDEMPOTENCY_CONFLICT");

        let owner_call = owner.clone();
        let notice = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                queue_notification(
                    &owner,
                    "fulfillment-1",
                    QueueNotification {
                        expected_version: 1,
                        note: "Carrier confirmed the delivery window".into(),
                    },
                    "notification-queue-0001",
                    repo,
                    NOW,
                    "customer-notification-1",
                )
                .await
            })
        })
        .await
        .unwrap();
        assert_eq!(notice.status, "pending");
        assert_eq!(notice.order_id, "order-1");
        let row = sqlx::query_as::<_, (String, i64, String, String, String)>(
            "SELECT status,attempts,recipient_email,subject,body_text FROM commerce_customer_notifications WHERE id='customer-notification-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.0, "pending");
        assert_eq!(row.1, 0);
        assert_eq!(row.2, "receiver@example.invalid");
        assert_eq!(row.3, "Delivery F-1 for order SO-1");
        assert!(row.4.contains("SKU-1: 2 pcs"));
        let authorization = sqlx::query_as::<_, (String, String, String)>(
            "SELECT kind,source_hash,authority_json FROM sales_notification_execution_authorizations WHERE notification_id='customer-notification-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(authorization.0, "queue");
        assert_eq!(authorization.1.len(), 64);
        assert!(authorization.2.contains("\"companyId\":\"company\""));

        // The queued authorization is exactly what the native worker claims.
        let worker = PgNotificationExecution::new(pool.clone(), PackageEnforcementMode::Disabled, false);
        assert!(
            worker
                .claim(COMPANY, "customer-notification-1", NOW)
                .await
                .unwrap()
                .is_some(),
            "native worker claim"
        );
        // Release the claim so retry can be exercised on a failed notice.
        sqlx::raw_sql(
            "UPDATE commerce_customer_notifications SET status='failed',attempts=1,lease_until=NULL,last_error='CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED',updated_at=1800000000001 WHERE id='customer-notification-1';\
             UPDATE sales_notification_execution_attempts SET state='failed' WHERE notification_id='customer-notification-1';",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let owner_call = owner.clone();
        let retried = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                retry_notification(
                    &owner,
                    "customer-notification-1",
                    RetryNotification { expected_version: 1 },
                    "notification-retry-0001",
                    repo,
                    NOW + 1,
                )
                .await
            })
        })
        .await
        .unwrap();
        assert_eq!(retried.status, "pending");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM commerce_customer_notifications WHERE id='customer-notification-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "pending"
        );
        let owner_call = owner.clone();
        let not_failed = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                retry_notification(
                    &owner,
                    "customer-notification-1",
                    RetryNotification { expected_version: 1 },
                    "notification-retry-0002",
                    repo,
                    NOW + 2,
                )
                .await
            })
        })
        .await
        .unwrap_err();
        assert_eq!(code(&not_failed), "CUSTOMER_NOTIFICATION_NOT_FAILED");
    })
    .await;
}

#[tokio::test]
async fn purchase_and_production_gates_fail_closed() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let owner = actor(&["commerce.read", "commerce.sales", "commerce.approve"]);
        let owner_call = owner.clone();
        let no_purchase = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                update_settings(
                    &owner,
                    UpdateSettings { expected_version: 0, adapter: SettingsAdapter::Sandbox },
                    "notification-settings-gate-0001",
                    repo,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap_err();
        assert_eq!(code(&no_purchase), "PACKAGE_REQUIRED");
        grant(&pool, "active").await;
        let owner_call = owner.clone();
        let production = run(&pool, PackageEnforcementMode::Enforced, true, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                update_settings(
                    &owner,
                    UpdateSettings { expected_version: 0, adapter: SettingsAdapter::Sandbox },
                    "notification-settings-gate-0002",
                    repo,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap_err();
        assert_eq!(code(&production), "FEATURE_COMING_SOON");
        let owner_call = owner.clone();
        let disabled = run(&pool, PackageEnforcementMode::Disabled, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                update_settings(
                    &owner,
                    UpdateSettings { expected_version: 0, adapter: SettingsAdapter::Disabled },
                    "notification-settings-gate-0003",
                    repo,
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        assert_eq!(disabled.adapter, "disabled");
    })
    .await;
}

#[tokio::test]
async fn queue_refuses_missing_consent_and_existing_notice() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        grant(&pool, "active").await;
        sqlx::query("UPDATE finance_parties SET notification_consent='revoked' WHERE id='party-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::query("INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) VALUES('company','sandbox',1,'owner',100)")
            .execute(pool.raw())
            .await
            .unwrap();
        let owner = actor(&["commerce.read", "commerce.sales"]);
        let owner_call = owner.clone();
        let consent = run(&pool, PackageEnforcementMode::Enforced, false, move |repo| {
            let owner = owner_call.clone();
            Box::pin(async move {
                queue_notification(
                    &owner,
                    "fulfillment-1",
                    QueueNotification { expected_version: 1, note: "Call the customer".into() },
                    "notification-queue-consent-0001",
                    repo,
                    NOW,
                    "customer-notification-consent",
                )
                .await
            })
        })
        .await
        .unwrap_err();
        assert_eq!(code(&consent), "CUSTOMER_NOTIFICATION_CONSENT_REQUIRED");
        assert!(
            sqlx::query("SELECT 1 FROM commerce_customer_notifications WHERE fulfillment_id='fulfillment-1'")
                .fetch_optional(pool.raw())
                .await
                .unwrap()
                .is_none()
        );
    })
    .await;
}

#[test]
fn settings_adapter_parsing_is_exact() {
    assert_eq!(SettingsAdapter::parse("sandbox"), Some(SettingsAdapter::Sandbox));
    assert_eq!(SettingsAdapter::parse("resend"), Some(SettingsAdapter::Resend));
    assert_eq!(SettingsAdapter::parse("disabled"), Some(SettingsAdapter::Disabled));
    assert!(SettingsAdapter::parse("other").is_none());
}
