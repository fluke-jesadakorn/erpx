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

#[tokio::test]
async fn legacy_sales_approval_orders_credit_assertion_before_version_guard() {
    // Invoke the actual TS application and Finance statement builder. Only the
    // statement capture is substituted; credit assertions execute against real
    // migrated PostgreSQL below, without changing production guards.
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let app=root.join("packages/server-commerce/src/application/commerce.ts");
    let finance=root.join("packages/server-finance/src/adapters/commerce-facts.ts");
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const c=await import(process.argv[1]);const f=await import(process.argv[2]);const db={prepare(sql){return {bind(...values){return {sql,values}}}}};const finance=f.CommerceFinanceFacts(db);const writer={replayCommand:async()=>null,readCommerceOrders1:async()=>({id:'sales-draft',kind:'sales',state:'submitted',version:3,created_by:'creator',discount_approval_required:0,party_id:'party-limited',subtotal_minor:35001,tax_minor:0}),guardRule1:()=>({marker:'version'}),prepareCommerceOrders4:()=>({marker:'update'}),prepareFinanceParties1:finance.prepareFinanceParties1,guardRule20:finance.guardRule20,executeCommand:async(...a)=>a[7]};const actor={userId:'reviewer',companyId:'company',branchId:null,departmentId:null,iam:{permissions:['commerce.sales','commerce.approve'],personUserIds:[]}};const application=c.createCommerceApplication({clock:{now:()=>1},require:a=>a,error:(status,code,message)=>Object.assign(new Error(message),{status,code}),hash:async()=>'',forActor:async()=>writer});const steps=await application.transitionOrder(actor,'sales-draft',{action:'approve',expectedVersion:2},'credit-oracle');process.stdout.write(JSON.stringify(steps));"#;
    let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(app).arg(finance).output().unwrap();
    assert!(oracle.status.success(),"actual Commerce/Finance TS oracle: {}",String::from_utf8_lossy(&oracle.stderr));
    let steps:serde_json::Value=serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(steps.as_array().unwrap().len(),4);
    assert!(steps[0]["sql"].as_str().unwrap().contains("finance_parties"));
    assert_eq!(steps[1]["values"],serde_json::json!(["credit-oracle","customer-credit-limit","party-limited","company",35001]));
    assert_eq!(steps[2]["marker"],"version");assert_eq!(steps[3]["marker"],"update");
    let sql=steps[1]["sql"].as_str().unwrap();assert_eq!(sql.matches('?').count(),5);
    let mut n=0;let sql:String=sql.chars().flat_map(|c|if c=='?'{n+=1;format!("${n}").chars().collect::<Vec<_>>()}else{vec![c]}).collect();
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;let mut tx=pool.raw().begin().await.unwrap();serial(&mut tx).await;
        for (amount,allowed) in [(35_000_i64,true),(35_001_i64,false)] {
            assert_eq!(sales_order_credit_allows(&mut tx,&company(),"party-limited",amount).await.unwrap(),allowed);
            sqlx::query("SAVEPOINT legacy_credit").execute(&mut *tx).await.unwrap();
            let result=sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind("credit-oracle").bind("customer-credit-limit").bind("party-limited").bind("company").bind(amount).execute(&mut *tx).await;
            if allowed {assert_eq!(result.unwrap().rows_affected(),1)} else {let error=result.unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));}
            sqlx::query("ROLLBACK TO SAVEPOINT legacy_credit").execute(&mut *tx).await.unwrap();
        }
        let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='credit-oracle'),(SELECT count(*) FROM audit_events WHERE command_id='credit-oracle'),(SELECT count(*) FROM outbox_events WHERE command_id='credit-oracle')").fetch_one(&mut *tx).await.unwrap();assert_eq!(effects,(0,0,0));tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commerce_order_party_guard_matches_actual_typescript_sql() {
    use erpx_server_finance::public::commerce_order_party_guard::{lock_commerce_order_party_source,observe_commerce_order_party_guard,CommerceOrderPartyKind};
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let finance=root.join("packages/server-finance/src/adapters/commerce-facts.ts");
    let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const f=await import(process.argv[1]);const db={prepare(sql){return{bind(...values){return{sql,values}}}}};const s=f.CommerceFinanceFacts(db).guardRule17({key:'party-oracle',guardId:'party',values:['party-limited','company','customer','USD']});process.stdout.write(JSON.stringify(s));"#;
    let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(root).args(["--conditions=react-server","-e",script]).arg(finance).output().unwrap();
    assert!(oracle.status.success(),"actual Finance TS oracle: {}",String::from_utf8_lossy(&oracle.stderr));
    let step:serde_json::Value=serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(step["values"],serde_json::json!(["party-oracle","party","party-limited","company","customer","USD"]));
    let raw=step["sql"].as_str().unwrap();assert_eq!(raw.matches('?').count(),6);
    let mut n=0;let sql:String=raw.chars().flat_map(|ch|if ch=='?'{n+=1;format!("${n}").chars().collect::<Vec<_>>()}else{vec![ch]}).collect();
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO hr_people(id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('party-person','company','Person','TH','fixture',0,0); INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,person_id,created_at,updated_at) VALUES('party-supplier','company','supplier','S1','Supplier','USD','active',NULL,0,0),('party-personal','company','customer','C4','Personal','USD','active','party-person',0,0)").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();serial(&mut tx).await;
        lock_commerce_order_party_source(&mut tx).await.unwrap();
        for (id,kind,currency,expected) in [("party-limited",CommerceOrderPartyKind::Customer,"USD",true),("party-supplier",CommerceOrderPartyKind::Supplier,"USD",true),("party-personal",CommerceOrderPartyKind::Customer,"USD",false),("party-foreign",CommerceOrderPartyKind::Customer,"USD",false),("missing",CommerceOrderPartyKind::Customer,"USD",false),("party-limited",CommerceOrderPartyKind::Supplier,"USD",false),("party-limited",CommerceOrderPartyKind::Customer,"THB",false)] {
            let guard=observe_commerce_order_party_guard(&mut tx,&company(),id,kind,currency).await.unwrap();
            assert_eq!(guard.matched(),expected);guard.require_current(&mut tx).await.unwrap();
            sqlx::query("SAVEPOINT party_oracle").execute(&mut *tx).await.unwrap();
            let result=sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind("party-oracle").bind("party").bind(id).bind("company").bind(match kind{CommerceOrderPartyKind::Customer=>"customer",CommerceOrderPartyKind::Supplier=>"supplier"}).bind(currency).execute(&mut *tx).await;
            if expected {assert_eq!(result.unwrap().rows_affected(),1)} else {assert_eq!(result.unwrap_err().as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));}
            sqlx::query("ROLLBACK TO SAVEPOINT party_oracle").execute(&mut *tx).await.unwrap();
        }
        let original=observe_commerce_order_party_guard(&mut tx,&company(),"party-limited",CommerceOrderPartyKind::Customer,"USD").await.unwrap();
        sqlx::query("UPDATE public.finance_parties SET status='blocked' WHERE id='party-limited'").execute(&mut *tx).await.unwrap();
        assert!(original.require_current(&mut tx).await.is_err());
        let blocked=observe_commerce_order_party_guard(&mut tx,&company(),"party-limited",CommerceOrderPartyKind::Customer,"USD").await.unwrap();assert!(!blocked.matched());
        let limit="x".repeat(8192);
        assert!(!observe_commerce_order_party_guard(&mut tx,&company(),&limit,CommerceOrderPartyKind::Customer,"USD").await.unwrap().matched());
        assert!(observe_commerce_order_party_guard(&mut tx,&company(),&(limit.clone()+"x"),CommerceOrderPartyKind::Customer,"USD").await.is_err());
        assert!(!observe_commerce_order_party_guard(&mut tx,&company(),"missing",CommerceOrderPartyKind::Customer,&limit).await.unwrap().matched());
        assert!(observe_commerce_order_party_guard(&mut tx,&company(),"missing",CommerceOrderPartyKind::Customer,&(limit+"x")).await.is_err());
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();
        let (pid_tx,pid_rx)=tokio::sync::oneshot::channel();let writer_pool=pool.clone();
        let writer=tokio::spawn(async move {
            let mut other=writer_pool.raw().begin().await.unwrap();
            sqlx::query("SET LOCAL lock_timeout='3s'").execute(&mut *other).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *other).await.unwrap();pid_tx.send(pid).unwrap();
            let error=sqlx::query("UPDATE public.finance_parties SET name='concurrent writer' WHERE id='party-unlimited'").execute(&mut *other).await.unwrap_err();
            assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03"));other.rollback().await.unwrap();
        });
        let writer_pid=pid_rx.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2),async { loop {
            let waiting:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2)) AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=$2 AND relation='public.finance_parties'::regclass AND mode='RowExclusiveLock' AND NOT granted)").bind(reader_pid).bind(writer_pid).fetch_one(&mut *tx).await.unwrap();
            if waiting { break; } tokio::task::yield_now().await;
        }}).await.expect("the exact writer waits on the retained table fence");
        writer.await.unwrap();
        tx.rollback().await.unwrap();
        let mut fresh=pool.raw().begin().await.unwrap();serial(&mut fresh).await;lock_commerce_order_party_source(&mut fresh).await.unwrap();
        assert!(original.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut save=pool.raw().begin().await.unwrap();serial(&mut save).await;
        sqlx::query("SELECT pg_current_xact_id()").execute(&mut *save).await.unwrap();
        sqlx::query("SAVEPOINT before_party_lock").execute(&mut *save).await.unwrap();lock_commerce_order_party_source(&mut save).await.unwrap();
        let held=observe_commerce_order_party_guard(&mut save,&company(),"party-limited",CommerceOrderPartyKind::Customer,"USD").await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_party_lock").execute(&mut *save).await.unwrap();assert!(held.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
        let mut shadow=pool.raw().begin().await.unwrap();serial(&mut shadow).await;
        sqlx::raw_sql("CREATE TEMP TABLE finance_parties(id text,company_id text,kind text,status text,currency text,person_id text); INSERT INTO finance_parties VALUES('missing','company','customer','active','USD',NULL)").execute(&mut *shadow).await.unwrap();
        lock_commerce_order_party_source(&mut shadow).await.unwrap();let proof=observe_commerce_order_party_guard(&mut shadow,&company(),"missing",CommerceOrderPartyKind::Customer,"USD").await.unwrap();assert!(!proof.matched());
        sqlx::query("SET LOCAL search_path=pg_catalog,public").execute(&mut *shadow).await.unwrap();assert!(proof.require_current(&mut shadow).await.is_err());shadow.rollback().await.unwrap();
        let mut rls=pool.raw().begin().await.unwrap();serial(&mut rls).await;
        let bypass:bool=sqlx::query_scalar("SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&mut *rls).await.unwrap();assert!(!bypass,"leased test role must not bypass RLS");
        sqlx::raw_sql("ALTER TABLE public.finance_parties ENABLE ROW LEVEL SECURITY; ALTER TABLE public.finance_parties FORCE ROW LEVEL SECURITY").execute(&mut *rls).await.unwrap();
        let active:bool=sqlx::query_scalar("SELECT row_security_active('public.finance_parties'::regclass)").fetch_one(&mut *rls).await.unwrap();assert!(active,"RLS must be active for the selected role");
        lock_commerce_order_party_source(&mut rls).await.unwrap();assert!(observe_commerce_order_party_guard(&mut rls,&company(),"party-limited",CommerceOrderPartyKind::Customer,"USD").await.is_err());rls.rollback().await.unwrap();
    }).await;
}
