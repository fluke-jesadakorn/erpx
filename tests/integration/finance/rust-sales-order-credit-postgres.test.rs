//! Finance-owned sales-order credit guard on migrated PostgreSQL: exact
//! retained exposure formula, party lock, Company isolation and unlimited
//! credit.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_finance::public::sales_order_credit::sales_order_credit_allows;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::PgConnection;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign',NULL,'foreign',NULL,'active',1);\
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) \
         VALUES('chart','company','Chart','USD','active','fixture',0,0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,status,created_at,updated_at) \
         VALUES('receivable','chart','company','1100','Receivable','asset','USD','active',0,0),\
         ('revenue','chart','company','4000','Revenue','revenue','USD','active',0,0);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,credit_limit_minor,status,version,created_at,updated_at) \
         VALUES('party-limited','company','customer','C1','Limited customer','USD',100000,'active',1,0,0),\
         ('party-unlimited','company','customer','C2','Unlimited customer','USD',0,'active',1,0,0),\
         ('party-foreign','foreign','customer','C3','Foreign customer','USD',100000,'active',1,0,0);\
         INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) \
         VALUES('sales-confirmed','company','branch','sales','SO-1','party-limited','USD','2026-01-01','2026-02-01','','confirmed',20000,0,1,'fixture',0,0),\
         ('sales-draft','company','branch','sales','SO-2','party-limited','USD','2026-01-01','2026-02-01','','draft',50000,0,1,'fixture',0,0),\
         ('sales-other','company','branch','sales','SO-3','party-limited','USD','2026-01-01','2026-02-01','','confirmed',10000,0,1,'fixture',0,0);\
         INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) \
         VALUES('product','company','P1','Product','each',100,0,'active',1);\
         INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) \
         VALUES('line-confirmed','sales-confirmed','product','Product',1,20000,0,20000,0,0),\
         ('line-other','sales-other','product','Product',1,10000,0,10000,0,0);\
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at) \
         VALUES('invoice-draft','company','party-limited','receivable','INV-1','2026-01-01','2026-02-01','USD',30000,0,30000,30000,'receivable','revenue','draft','fixture',0,0),\
         ('invoice-posted','company','party-limited','receivable','INV-2','2026-01-01','2026-02-01','USD',20000,0,20000,10000,'receivable','revenue','posted','fixture',0,0),\
         ('invoice-void','company','party-limited','receivable','INV-3','2026-01-01','2026-02-01','USD',90000,0,90000,90000,'receivable','revenue','voided','fixture',0,0);\
         INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,invoice_id,version) \
         VALUES('fulfillment','sales-confirmed','F-1','2026-01-02','fixture',0,'invoice-posted',1);\
         INSERT INTO commerce_fulfillment_lines(id,fulfillment_id,order_line_id,quantity_milli,value_minor,subtotal_minor,tax_minor) \
         VALUES('fulfillment-line','fulfillment','line-confirmed',1,5000,5000,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn serial(c: &mut PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(c)
        .await
        .unwrap();
}

fn denied<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Persistence(error)) => assert!(error.to_string().contains(code)),
        _ => panic!("expected persistence {code}"),
    }
}

#[tokio::test]
async fn credit_guard_matches_the_retained_exposure_formula() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        // Exposure: draft 30000 + posted outstanding 10000 + confirmed order
        // 20000 with invoiced fulfillment 5000 (=15000) + confirmed 10000 =
        // 65000. The draft sales order 50000 and voided invoice 90000 do not
        // count.
        assert!(
            sales_order_credit_allows(&mut tx, &company(), "party-limited", 35_000)
                .await
                .unwrap()
        );
        assert!(
            !sales_order_credit_allows(&mut tx, &company(), "party-limited", 35_001)
                .await
                .unwrap()
        );
        assert!(
            sales_order_credit_allows(&mut tx, &company(), "party-unlimited", 9_000_000)
                .await
                .unwrap()
        );
        assert!(
            !sales_order_credit_allows(&mut tx, &company(), "missing", 0)
                .await
                .unwrap()
        );
        assert!(
            !sales_order_credit_allows(
                &mut tx,
                &CompanyId::new("foreign").unwrap(),
                "party-limited",
                0
            )
            .await
            .unwrap()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn credit_guard_requires_the_serializable_owner_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        denied(
            sales_order_credit_allows(&mut connection, &company(), "party-limited", 0).await,
            "FINANCE_SERIALIZABLE_REQUIRED",
        );
    })
    .await;
}
