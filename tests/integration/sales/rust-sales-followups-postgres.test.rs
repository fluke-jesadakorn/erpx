//! Native Sales customer follow-up commands on migrated PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_sales::public::customer_followup_commands::{
    ChangeFollowup, CreateFollowup, FollowupAction, FollowupActor, FollowupReceipt, PgFollowups,
    change_customer_followup, create_customer_followup,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;
const COMPANY: &str = "company";
const OWNER: &str = "owner";
const MANAGER: &str = "manager";
const OTHER: &str = "other";
const ORDER: &str = "order-1";

type Error = PgTransactionError<BusinessError>;

fn code(error: &Error) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        PgTransactionError::Persistence(_) => "PERSISTENCE",
    }
}

fn actor(user: &str, permissions: &[&str]) -> FollowupActor {
    FollowupActor {
        user_id: user.into(),
        company_id: COMPANY.into(),
        branch_id: Some("branch".into()),
        department_id: None,
        assignment_id: format!("assignment-{user}"),
        role: "sales_manager".into(),
        session_id: None,
        assignment_version: None,
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn input() -> CreateFollowup {
    CreateFollowup {
        order_id: ORDER.into(),
        expected_version: 1,
        owner_id: OWNER.into(),
        due_date: "2026-09-22".into(),
        next_action: "Call the customer about installation requirements".into(),
    }
}

fn change(action: FollowupAction, version: i64) -> ChangeFollowup {
    ChangeFollowup {
        action,
        expected_version: version,
        owner_id: (action == FollowupAction::Reassign).then(|| MANAGER.to_owned()),
        due_date: (action == FollowupAction::Reassign).then(|| "2026-09-23".to_owned()),
        reason: "Customer confirmed the installation outcome".into(),
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
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    for user in [OWNER, MANAGER, OTHER] {
        sqlx::query(
            "INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) \
             VALUES('company',$1,$2,'active','local',1)",
        )
        .bind(user)
        .bind(format!("local:{user}"))
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) \
             VALUES($1,$2,CASE WHEN $2='other' THEN 'sales_operator' ELSE 'sales_manager' END,'[]','company','branch',0,9000000000000,NULL,'holding',NULL,'manual')",
        )
        .bind(format!("assignment-{user}"))
        .bind(user)
        .execute(pool.raw())
        .await
        .unwrap();
        let person = format!("person-{user}");
        sqlx::query(
            "INSERT INTO hr_people(id,company_id,user_id,legal_name,country_code,status,version,created_by,created_at,updated_at,merged_into_id) \
             VALUES($1,'company',$2,$3,'TH','active',1,'fixture',1,1,NULL)",
        )
        .bind(&person)
        .bind(user)
        .bind(format!("Person {user}"))
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO hr_engagements(id,company_id,person_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at) \
             VALUES($1,'company',$2,'employee','salaried','active','2020-01-01',NULL,'Sales','THB',1,'fixture',1,1)",
        )
        .bind(format!("engagement-{user}"))
        .bind(&person)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) \
         VALUES('party-1','company','customer','C1','Customer','THB',30,0,'active',1,1,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) \
         VALUES('warehouse-1','company','branch','WH1','Main warehouse','active',1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) \
         VALUES($1,'company','branch',NULL,'quote','Q-1','party-1','warehouse-1','THB','2026-09-02','2026-09-03','','confirmed',1000,0,NULL,3,'owner','owner',100,300)",
    )
    .bind(ORDER)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn grant(pool: &DatabasePool, state: &str) {
    let mut transaction = pool.raw().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) \
         VALUES('company','sales',1,$1,0,NULL,1,'Isolated Sales follow-up purchase','test:followup',$2)",
    )
    .bind(state)
    .bind(NOW)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) \
         SELECT 'grant-event',company_id,package_id,'fixture','synthetic-followup-grant',NULL,\
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

async fn run_create(
    pool: &DatabasePool,
    actor: &FollowupActor,
    input: CreateFollowup,
    key: &str,
    mode: PackageEnforcementMode,
) -> Result<FollowupReceipt, Error> {
    let actor = actor.clone();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.clone();
            Box::pin(async move {
                let mut repository = PgFollowups::new(connection, mode);
                create_customer_followup(&actor, input, &key, &mut repository, NOW).await
            })
        })
        .await
}

async fn run_change(
    pool: &DatabasePool,
    actor: &FollowupActor,
    id: &str,
    input: ChangeFollowup,
    key: &str,
    mode: PackageEnforcementMode,
) -> Result<FollowupReceipt, Error> {
    let actor = actor.clone();
    let key = key.to_owned();
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let input = input.clone();
            let key = key.clone();
            let id = id.clone();
            Box::pin(async move {
                let mut repository = PgFollowups::new(connection, mode);
                change_customer_followup(&actor, &id, input, &key, &mut repository, NOW).await
            })
        })
        .await
}

async fn ledger(pool: &DatabasePool, key: &str) -> (i64, i64, i64, i64) {
    let row = sqlx::query(
        "SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1) receipts,\
         (SELECT count(*) FROM audit_events WHERE command_id=$1) audits,\
         (SELECT count(*) FROM outbox_events WHERE command_id=$1) outbox,\
         (SELECT count(*) FROM staff_notifications WHERE id='followup-notice-'||$1) notices",
    )
    .bind(key)
    .fetch_one(pool.raw())
    .await
    .unwrap();
    (
        row.get("receipts"),
        row.get("audits"),
        row.get("outbox"),
        row.get("notices"),
    )
}

#[tokio::test]
async fn creates_and_replays_one_immutable_source_linked_contact() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        grant(&pool, "active").await;
        let key = "customer-followup-0001";
        let receipt = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            key,
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap();
        assert_eq!(
            receipt,
            FollowupReceipt {
                id: format!("followup-{key}"),
                version: 1,
                status: "open".into()
            }
        );
        let row = sqlx::query(
            "SELECT company_id,order_id,owner_id,due_date,next_action,status,version,created_by \
             FROM customer_followups WHERE id=$1",
        )
        .bind(format!("followup-{key}"))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("company_id"), COMPANY);
        assert_eq!(row.get::<String, _>("order_id"), ORDER);
        assert_eq!(row.get::<String, _>("owner_id"), OWNER);
        assert_eq!(row.get::<String, _>("due_date"), "2026-09-22");
        assert_eq!(row.get::<String, _>("status"), "open");
        assert_eq!(row.get::<i64, _>("version"), 1);
        assert_eq!(row.get::<String, _>("created_by"), OWNER);
        let event: (String, i64, String) = sqlx::query_as(
            "SELECT action,version,owner_id FROM customer_followup_events WHERE followup_id=$1",
        )
        .bind(format!("followup-{key}"))
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(event, ("create".into(), 1, OWNER.into()));
        assert_eq!(ledger(&pool, key).await, (1, 1, 1, 1));

        let replay = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            key,
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap();
        assert_eq!(replay, receipt);
        assert_eq!(ledger(&pool, key).await, (1, 1, 1, 1));

        let conflict = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            CreateFollowup {
                next_action: "A changed uncertain original contact".into(),
                ..input()
            },
            key,
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&conflict), "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn ownership_version_and_owner_eligibility_are_enforced() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        grant(&pool, "active").await;
        let id = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            "customer-followup-0002",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap()
        .id;
        let foreign = run_change(
            &pool,
            &actor(MANAGER, &["commerce.read", "commerce.sales", "commerce.approve"]),
            &id,
            change(FollowupAction::Complete, 1),
            "customer-followup-0003",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&foreign), "CUSTOMER_FOLLOWUP_OWNER_REQUIRED");

        let owner_cancel = run_change(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            &id,
            change(FollowupAction::Cancel, 1),
            "customer-followup-0004",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap();
        assert_eq!(owner_cancel.status, "cancelled");
        let stale = run_change(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            &id,
            change(FollowupAction::Cancel, 1),
            "customer-followup-0005",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&stale), "CONFLICT");

        // A manager reassignment hands the open contact to another eligible owner.
        let receipt = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            "customer-followup-0006",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap();
        let reassigned = run_change(
            &pool,
            &actor(MANAGER, &["commerce.read", "commerce.sales", "commerce.approve"]),
            &receipt.id,
            change(FollowupAction::Reassign, 1),
            "customer-followup-0007",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap();
        assert_eq!(reassigned.status, "open");
        assert_eq!(reassigned.version, 2);

        sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='assignment-owner'")
            .bind(NOW)
            .execute(pool.raw())
            .await
            .unwrap();
        let ineligible = run_create(
            &pool,
            &actor(MANAGER, &["commerce.read", "commerce.sales", "commerce.approve"]),
            input(),
            "customer-followup-0008",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&ineligible), "CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn purchase_new_work_and_draining_are_enforced() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let no_purchase = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            "customer-followup-purchase-0001",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&no_purchase), "PACKAGE_REQUIRED");

        grant(&pool, "draining").await;
        let draining = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            "customer-followup-purchase-0002",
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&draining), "PACKAGE_DRAINING");

    })
    .await;
}

#[tokio::test]
async fn late_authority_and_source_substitution_roll_back_then_retry() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        grant(&pool, "active").await;
        let key = "customer-followup-authority-0001";
        let fault = "followup_authority_revoke";
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE FUNCTION {fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
             UPDATE assignments SET revoked_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint,version=version+1 \
             WHERE id='assignment-owner';RETURN NEW;END $$;\
             CREATE CONSTRAINT TRIGGER {fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED \
             FOR EACH ROW WHEN(NEW.command_id='{key}') EXECUTE FUNCTION {fault}();"
        )))
        .execute(pool.raw())
        .await
        .unwrap();
        let late = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            key,
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&late), "FOLLOWUP_ACCESS_CHANGED");
        sqlx::raw_sql("DROP TRIGGER followup_authority_revoke ON command_receipts;DROP FUNCTION followup_authority_revoke();")
            .execute(pool.raw())
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT revoked_at FROM assignments WHERE id='assignment-owner'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            None
        );
        assert_eq!(ledger(&pool, key).await, (0, 0, 0, 0));
        assert_eq!(
            run_create(
                &pool,
                &actor(OWNER, &["commerce.read", "commerce.sales"]),
                input(),
                key,
                PackageEnforcementMode::Enforced
            )
            .await
            .unwrap()
            .version,
            1
        );

        let substitution = "customer-followup-source-0001";
        let other_order = "order-2";
        sqlx::query(
            "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) \
             VALUES($1,'company','branch',NULL,'quote','Q-2','party-1','warehouse-1','THB','2026-09-02','2026-09-03','','confirmed',1000,0,NULL,3,'owner','owner',100,300)",
        )
        .bind(other_order)
        .execute(pool.raw())
        .await
        .unwrap();
        let fault = "followup_source_substitute";
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE FUNCTION {fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
             UPDATE customer_followups SET order_id='{other_order}' WHERE id='followup-{substitution}';RETURN NEW;END $$;\
             CREATE CONSTRAINT TRIGGER {fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED \
             FOR EACH ROW WHEN(NEW.command_id='{substitution}') EXECUTE FUNCTION {fault}();"
        )))
        .execute(pool.raw())
        .await
        .unwrap();
        let error = run_create(
            &pool,
            &actor(OWNER, &["commerce.read", "commerce.sales"]),
            input(),
            substitution,
            PackageEnforcementMode::Enforced,
        )
        .await
        .unwrap_err();
        assert_eq!(code(&error), "FOLLOWUP_SOURCE_CHANGED");
        sqlx::raw_sql("DROP TRIGGER followup_source_substitute ON command_receipts;DROP FUNCTION followup_source_substitute();")
            .execute(pool.raw())
            .await
            .unwrap();
        assert!(
            sqlx::query("SELECT 1 FROM customer_followups WHERE id=$1")
                .bind(format!("followup-{substitution}"))
                .fetch_optional(pool.raw())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(ledger(&pool, substitution).await, (0, 0, 0, 0));
    })
    .await;
}
