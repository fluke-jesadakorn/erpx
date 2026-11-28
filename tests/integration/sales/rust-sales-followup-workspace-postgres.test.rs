//! Scoped Sales follow-up workspace reads on disposable PostgreSQL.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_platform_runtime_server::public::postgres::{DatabasePool, MigrationTrack, load_migrations, migrate_database};
use erpx_sales_server::public::native_r3_followup_workspace::{read_workspace_on_connection, WorkspaceInput};
const OWNER: &str = "owner";
const MANAGER: &str = "manager";
const OTHER: &str = "other";
const ORDER: &str = "order-1";

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
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


#[tokio::test]
async fn scoped_workspace_filters_mixed_history_and_revalidates_recipient_sources() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES('branch-other','branch','branch-other','Other',NULL,'company',NULL,'active',1);
          INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES
          ('sale-1','company','branch',NULL,'sales','S-1','party-1','warehouse-1','THB','2026-09-02','2026-09-03','','confirmed',1000,0,NULL,3,'owner','owner',100,300),
          ('other-branch','company','branch-other',NULL,'quote','Q-2','party-1','warehouse-1','THB','2026-09-02','2026-09-03','','confirmed',1000,0,NULL,3,'owner','owner',100,300);
          INSERT INTO customer_followups(id,company_id,order_id,owner_id,due_date,next_action,status,version,created_by,created_at,updated_at) VALUES
          ('quote-followup','company','order-1','owner','2026-10-01','Call','open',1,'owner',1,1),
          ('sale-followup','company','sale-1','owner','2026-10-01','Call','open',1,'owner',1,1),
          ('hidden-followup','company','other-branch','owner','2026-10-01','Call','open',1,'owner',1,1)").execute(pool.raw()).await.unwrap();
        let input = WorkspaceInput { company_id:"company", branch_id:Some("branch"), department_id:None, order_id:None, user_id:"owner", can_read:true, can_sell:true, can_manage:false, quote_history:true, sales_history:false };
        let mut conn = pool.raw().acquire().await.unwrap();
        assert!(read_workspace_on_connection(&mut conn, input).await.is_err());
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let quotes = read_workspace_on_connection(&mut tx, input).await.unwrap();
        assert_eq!(quotes.items.len(), 1);
        assert_eq!(quotes.items[0]["id"], "quote-followup");
        let sales = read_workspace_on_connection(&mut tx, WorkspaceInput { quote_history:false, sales_history:true, ..input }).await.unwrap();
        assert_eq!(sales.items.len(), 1);
        assert_eq!(sales.items[0]["id"], "sale-followup");
        assert!(read_workspace_on_connection(&mut tx, WorkspaceInput { order_id:Some("sale-1"), ..input }).await.is_err());
        assert!(read_workspace_on_connection(&mut tx, WorkspaceInput { order_id:Some("other-branch"), ..input }).await.is_err());
        let selected = read_workspace_on_connection(&mut tx, WorkspaceInput { order_id:Some("order-1"), ..input }).await.unwrap();
        let order = selected.order.unwrap();
        assert_eq!(order["party_name"], "Customer");
        assert_eq!(order["party_code"], "C1");
        assert_eq!(order["warehouse_name"], "Main warehouse");
        assert!(order.get("notification_email").is_some());
        assert!(order.get("notification_consent").is_some());
        assert!(order.get("party_version").is_some());
        assert!(selected.recipients.iter().any(|r| r["id"] == "owner"));
        sqlx::query("UPDATE commerce_orders SET subtotal_minor=9007199254740992 WHERE id='order-1'").execute(&mut *tx).await.unwrap();
        assert!(read_workspace_on_connection(&mut tx, WorkspaceInput { order_id:Some("order-1"), ..input }).await.is_err());
        sqlx::query("UPDATE commerce_orders SET subtotal_minor=1000 WHERE id='order-1'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE customer_followups SET version=9007199254740992 WHERE id='quote-followup'").execute(&mut *tx).await.unwrap();
        assert!(read_workspace_on_connection(&mut tx, input).await.is_err());
        tx.rollback().await.unwrap();
        // Revoke/expire recipient authority and verify it is not projected.
        sqlx::query("UPDATE assignments SET revoked_at=1 WHERE user_id='owner' AND company_id='company'").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE hr_engagements SET end_date='2021-01-01' WHERE person_id='person-manager'").execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let rows = read_workspace_on_connection(&mut tx, WorkspaceInput { order_id:Some("order-1"), ..input }).await.unwrap();
        assert!(!rows.recipients.iter().any(|r| r["id"] == "owner" || r["id"] == "manager"));
        tx.rollback().await.unwrap();
        // Both references are protected by real Holding foreign keys. Exercise
        // the schema instead of constructing impossible missing-join rows.
        let missing_warehouse = sqlx::query("UPDATE commerce_orders SET warehouse_id='missing' WHERE id='order-1'").execute(pool.raw()).await.unwrap_err();
        assert_eq!(missing_warehouse.as_database_error().and_then(|e| e.code()).as_deref(), Some("23503"));
        let missing_party = sqlx::query("INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES('missing-party','company','branch',NULL,'quote','Q-404','missing','warehouse-1','THB','2026-09-02','2026-09-03','','confirmed',1000,0,NULL,3,'owner','owner',100,300)").execute(pool.raw()).await.unwrap_err();
        assert_eq!(missing_party.as_database_error().and_then(|e| e.code()).as_deref(), Some("23503"));
    }).await;
}
