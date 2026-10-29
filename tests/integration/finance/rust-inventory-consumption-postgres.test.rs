#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_finance::public::inventory_consumption::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, HoldingId, RecordId, TimestampMillis,
};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use std::time::Duration;
struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}
fn id(s: &str) -> RecordId {
    RecordId::new(s).unwrap()
}
fn holding() -> HoldingId {
    HoldingId::new("h001").unwrap()
}
fn mapping() -> InventoryConsumptionMapping {
    InventoryConsumptionMapping {
        company_id: CompanyId::new("company").unwrap(),
        chart_id: id("chart"),
        inventory_account_id: id("inventory"),
        cogs_account_id: id("expense"),
    }
}
async fn setup(p: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(p, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('company','company','acme','Synthetic Company',NULL,'company',NULL,'active');INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','fixture',0,0);INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at)VALUES('inventory','company','chart','100','Inventory','asset','THB',0,0),('expense','company','chart','500','Expense','expense','THB',0,0);INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at)VALUES('period','company','Broad synthetic test period','2020-01-01','2030-12-31',0)").execute(p.raw()).await.unwrap();
}
async fn serial(c: &mut PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(c)
        .await
        .unwrap();
}
async fn rate(c: &mut PgConnection, name: &str, date: &str, value: &str, confirmed: i64) {
    sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES($1,'company','THB',$2::date,$3::numeric,'fixture',$4)").bind(name).bind(date).bind(value).bind(confirmed).execute(c).await.unwrap();
}
async fn read(c: &mut PgConnection) -> CurrentConsumptionFinance {
    read_inventory_consumption_finance(c, &holding(), &mapping(), "2026-01-01")
        .await
        .unwrap()
}
fn denied<T>(r: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match r {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, code),
        _ => panic!("expected {code}"),
    }
}
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../fixtures/finance/rust-instruments/conformance.v1.json"
    ))
    .unwrap()
}

#[tokio::test]
async fn config_is_finance_base_currency_and_mapping_validation_remains_a_separate_phase() {
    fixture::with_database(2,|p|async move{setup(&p).await;let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;
 sqlx::query("INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id)VALUES('company','chart','USD','inventory','inventory','inventory','inventory','inventory','expense','inventory','inventory','inventory')").execute(&mut *tx).await.unwrap();
 let source=read(&mut tx).await;
 // Execute the actual retained TypeScript owner's SQL template, rather than
 // inventing a new config rule in the Rust fixture. This is a test-only oracle.
 let finance_source=include_str!("../../../packages/server-finance/src/adapters/inventory-consumption-sources.ts");
 let commerce_source=include_str!("../../../packages/server-commerce/src/adapters/inventory-read-projections.ts");
 let projection=commerce_source.split("postingSettings: \"").nth(1).unwrap().split('"').next().unwrap();
 let sql=finance_source.split("db.prepare(`").nth(1).unwrap().split('`').next().unwrap().replace("${inventorycommerceProjection.postingSettings}",projection).replace('?',"$1");
 let original:(String,String,String,String,String)=sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind("company").fetch_one(&mut *tx).await.unwrap();
 assert_eq!(original,("chart".into(),"THB".into(),"inventory".into(),"expense".into(),"THB".into()));
 assert_eq!(source.config().unwrap().currency,"THB");assert_eq!(source.config().unwrap().base_currency,"THB");assert_eq!(source.period_ids(),[id("period")]);source.require_posting_mapping(&mut tx).await.unwrap();
 sqlx::query("UPDATE finance_accounts SET status='inactive'WHERE id='inventory'").execute(&mut *tx).await.unwrap();denied(source.require_current(&mut tx).await,"INVENTORY_FINANCE_SOURCE_CHANGED");let invalid=read(&mut tx).await;assert!(invalid.config().is_some(),"B0 config lookup must not move the later mapping denial before lot validation");denied(invalid.require_posting_mapping(&mut tx).await,"PART_FINANCE_MAPPING_CHANGED");
 let mut foreign=mapping();foreign.company_id=CompanyId::new("other").unwrap();let absent=read_inventory_consumption_finance(&mut tx,&holding(),&foreign,"2026-01-01").await.unwrap();assert!(absent.config().is_none());assert!(absent.period_ids().is_empty());tx.rollback().await.unwrap();}).await;
}

#[tokio::test]
async fn actual_dated_rates_and_utf16_source_oracle_preserve_quote_and_exact_rounding() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        let data = corpus();
        let mut checked = 0;
        for row in data["companyMoney"]["rates"].as_array().unwrap() {
            let mut tx = p.raw().begin().await.unwrap();
            serial(&mut tx).await;
            let currency = row["currency"].as_str().unwrap();
            sqlx::query("UPDATE finance_charts SET base_currency=$1 WHERE id='chart'")
                .bind(currency)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE finance_accounts SET currency=$1")
                .bind(currency)
                .execute(&mut *tx)
                .await
                .unwrap();
            for r in row["rates"].as_array().unwrap() {
                if r["currency"] == "THB" {
                    rate(
                        &mut tx,
                        r["id"].as_str().unwrap(),
                        r["effectiveDate"].as_str().unwrap(),
                        r["currencyPerUsd"].as_str().unwrap(),
                        r["confirmedAt"].as_i64().unwrap(),
                    )
                    .await;
                }
            }
            let source = read_inventory_consumption_finance(
                &mut tx,
                &holding(),
                &mapping(),
                row["date"].as_str().unwrap(),
            )
            .await
            .unwrap();
            let plan = prepare_consumption_stock_cost_money(
                &mut tx,
                &source,
                id("operational-part-quote"),
                100,
                &TestClock,
            )
            .await;
            if row["expected"]["value"].is_null() {
                denied(plan, "MANUAL_RATE_REQUIRED");
            } else {
                let plan = plan.unwrap();
                let expected = &row["expected"]["value"];
                let chosen = expected["rate"]["id"].as_str().unwrap();
                assert_eq!(plan.rate_id(), (chosen != "USD").then_some(chosen));
                assert_eq!(
                    plan.used_latest_fallback(),
                    expected["usedLatestFallback"].as_bool().unwrap()
                );
                let retained = plan.persist(&mut tx).await.unwrap();
                retained.require_current(&mut tx).await.unwrap();
            }
            tx.rollback().await.unwrap();
            checked += 1;
        }
        for row in data["companyMoney"]["conversions"].as_array().unwrap() {
            if row["sourceScale"] != 2
                || row["targetScale"] != 2
                || row["targetQuote"] != "1"
                || row["expected"]["value"].is_null()
            {
                continue;
            }
            let Ok(amount) = row["amount"].as_str().unwrap().parse::<i64>() else {
                continue;
            };
            if !(1..=9_007_199_254_740_991).contains(&amount) {
                continue;
            }
            let mut tx = p.raw().begin().await.unwrap();
            serial(&mut tx).await;
            rate(
                &mut tx,
                "precise",
                "2026-01-01",
                row["sourceQuote"].as_str().unwrap(),
                1,
            )
            .await;
            let source = read(&mut tx).await;
            let plan = prepare_consumption_stock_cost_money(
                &mut tx,
                &source,
                id("operational-part-rounding"),
                amount,
                &TestClock,
            )
            .await
            .unwrap();
            assert_eq!(plan.usd_units(), row["expected"]["value"].as_str().unwrap());
            let retained = plan.persist(&mut tx).await.unwrap();
            retained.require_current(&mut tx).await.unwrap();
            tx.rollback().await.unwrap();
            checked += 1;
        }
        assert_eq!(checked, 8);
    })
    .await;
}

#[tokio::test]
async fn money_is_immutable_exact_and_does_not_escape_its_owner_transaction() {
    fixture::with_database(1,|p|async move{setup(&p).await;let mut c=p.raw().acquire().await.unwrap();assert!(read_inventory_consumption_finance(&mut c,&holding(),&mapping(),"2026-01-01").await.is_err());let mut tx=c.begin().await.unwrap();serial(&mut tx).await;rate(&mut tx,"rate","2026-01-01","2",1).await;let source=read(&mut tx).await;let plan=prepare_consumption_stock_cost_money(&mut tx,&source,id("operational-part-owned"),101,&TestClock).await.unwrap();assert_eq!(plan.usd_units(),"51");let retained=plan.persist(&mut tx).await.unwrap();assert_eq!(retained.usd_units(),"51");retained.require_current(&mut tx).await.unwrap();tx.commit().await.unwrap();
 let mut tx=c.begin().await.unwrap();serial(&mut tx).await;denied(retained.require_current(&mut tx).await,"INVENTORY_FINANCE_SOURCE_CHANGED");let error=sqlx::query("UPDATE money_transaction_snapshots SET usd_units=0 WHERE source_id='operational-part-owned'").execute(&mut *tx).await.unwrap_err();assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();}).await;
}

#[tokio::test]
async fn final_quote_source_and_real_audit_denial_roll_back_money_without_success_evidence() {
    fixture::with_database(2,|p|async move{setup(&p).await;
 let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;rate(&mut tx,"old","2026-01-01","2",1).await;let source=read(&mut tx).await;let plan=prepare_consumption_stock_cost_money(&mut tx,&source,id("operational-part-change"),100,&TestClock).await.unwrap();rate(&mut tx,"new","2026-01-01","3",2).await;denied(plan.persist(&mut tx).await,"INVENTORY_FINANCE_SOURCE_CHANGED");tx.rollback().await.unwrap();
 sqlx::raw_sql("CREATE FUNCTION deny_actual_stock_cost_audit()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'synthetic audit failure' USING ERRCODE='23514';END$$;CREATE TRIGGER deny_actual_stock_cost_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION deny_actual_stock_cost_audit()").execute(p.raw()).await.unwrap();
 let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;rate(&mut tx,"audit-rate","2026-01-01","2",1).await;let source=read(&mut tx).await;prepare_consumption_stock_cost_money(&mut tx,&source,id("operational-part-audit"),100,&TestClock).await.unwrap().persist(&mut tx).await.unwrap();let failed=sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at)VALUES('audit','synthetic-command','fixture','fixture','inventory.consume','consumption','operational-part-audit',1,'test owner atomicity',1)").execute(&mut *tx).await.unwrap_err();assert_eq!(failed.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots").fetch_one(p.raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM audit_events").fetch_one(p.raw()).await.unwrap(),0);
 }).await;
}

#[tokio::test]
async fn utility_lock_observes_a_period_closed_before_the_first_snapshot() {
    fixture::with_database(3,|p|async move{setup(&p).await;let mut writer=p.raw().begin().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();sqlx::query("UPDATE finance_periods SET state='closed'WHERE id='period'").execute(&mut *writer).await.unwrap();let reader=p.clone();let task=tokio::spawn(async move{let mut tx=reader.raw().begin().await.unwrap();serial(&mut tx).await;let facts=read(&mut tx).await;assert!(facts.period_ids().is_empty());tx.rollback().await.unwrap();});let waiting=tokio::time::timeout(Duration::from_secs(5),async{loop{let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()AND $1=ANY(pg_blocking_pids(pid)))").bind(pid).fetch_one(p.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await;writer.commit().await.unwrap();waiting.expect("actual Finance utility lock wait not observed");tokio::time::timeout(Duration::from_secs(10),task).await.unwrap().unwrap();}).await;
}

#[tokio::test]
async fn scoped_resource_caps_and_late_scale_validation_reject_before_retaining_unbounded_sources()
{
    fixture::with_database(2,|p|async move{setup(&p).await;let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;sqlx::query("UPDATE finance_charts SET base_currency='JPY'WHERE id='chart'").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE finance_accounts SET currency='JPY'").execute(&mut *tx).await.unwrap();let source=read(&mut tx).await;denied(prepare_consumption_stock_cost_money(&mut tx,&source,id("operational-part-scale"),100,&TestClock).await,"CURRENCY_SCALE_MISMATCH");tx.rollback().await.unwrap();
 let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;rate(&mut tx,"oversize","2026-01-01","2",1).await;sqlx::query("UPDATE finance_charts SET name=repeat('x',70000)WHERE id='chart'").execute(&mut *tx).await.unwrap();let source=read(&mut tx).await;source.require_posting_mapping(&mut tx).await.unwrap();sqlx::query("UPDATE finance_accounts SET name=repeat('ignored private description',1500000)").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
 let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note)SELECT 'budget-'||i,'company','THB','2026-01-01',2,'fixture',i,'irrelevant note'FROM generate_series(1,20001)i").execute(&mut *tx).await.unwrap();let source=read(&mut tx).await;denied(prepare_consumption_stock_cost_money(&mut tx,&source,id("operational-part-budget"),100,&TestClock).await,"INVENTORY_FINANCE_SOURCE_LIMIT");tx.rollback().await.unwrap();}).await;
}

#[tokio::test]
async fn exact_numeric_evidence_rejects_a_one_unit_insert_mutation_beyond_f64_precision() {
    fixture::with_database(2,|p|async move{
  setup(&p).await;
  sqlx::raw_sql("CREATE FUNCTION mutate_stock_cost_money()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.source_type='operational_inventory_consumption'THEN NEW.usd_units:=NEW.usd_units+1;END IF;RETURN NEW;END$$;CREATE TRIGGER mutate_stock_cost_money BEFORE INSERT ON money_transaction_snapshots FOR EACH ROW EXECUTE FUNCTION mutate_stock_cost_money()").execute(p.raw()).await.unwrap();
  let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;
  rate(&mut tx,"tiny-exact-rate","2026-01-01","0.000000000000000001",1).await;
  let source=read(&mut tx).await;
  let plan=prepare_consumption_stock_cost_money(&mut tx,&source,id("operational-part-exact"),9_007_199_254_740_991,&TestClock).await.unwrap();
  assert_eq!(plan.usd_units(),"9007199254740991000000000000000000");
  denied(plan.persist(&mut tx).await,"INVENTORY_FINANCE_SOURCE_CHANGED");
  tx.rollback().await.unwrap();
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots").fetch_one(p.raw()).await.unwrap(),0);
 }).await;
}

#[tokio::test]
async fn service_parts_finance_witness_checks_mapping_defaults_and_same_unit_drift() {
    use erpx_server_finance::public::service_parts_posting::{
        ServicePartsPostingMapping, lock_service_parts_posting_references,
        read_service_parts_posting_references,
    };
    fixture::with_database(1, |p| async move {
        setup(&p).await;
        sqlx::raw_sql("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,created_at,updated_at) VALUES ('ar','company','chart','110','AR','asset','THB','receivable',0,0),('ap','company','chart','210','AP','liability','THB','payable',0,0),('revenue','company','chart','400','Revenue','revenue','THB','none',0,0),('clearing','company','chart','220','Clearing','liability','THB','none',0,0),('input','company','chart','120','Input','asset','THB','none',0,0),('output','company','chart','230','Output','liability','THB','none',0,0),('cash','company','chart','130','Cash','asset','THB','bank',0,0); INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,status,reason,created_by,reviewed_by,version,created_at,updated_at) VALUES ('sales-default','company','chart','THB','ar','revenue',NULL,'approved','reviewed','creator','reviewer',2,1,1)").execute(p.raw()).await.unwrap();
        let m = ServicePartsPostingMapping { chart_id:"chart".into(),currency:"THB".into(),receivable_account_id:"ar".into(),payable_account_id:"ap".into(),revenue_account_id:"revenue".into(),inventory_account_id:"inventory".into(),clearing_account_id:"clearing".into(),cogs_account_id:"expense".into(),input_tax_account_id:"input".into(),output_tax_account_id:"output".into(),cash_account_id:"cash".into() };
        let mut tx=p.raw().begin().await.unwrap(); serial(&mut tx).await;
        lock_service_parts_posting_references(&mut tx).await.unwrap();
        let witness=read_service_parts_posting_references(&mut tx,&CompanyId::new("company").unwrap(),None).await.unwrap();
        assert!(!witness.data().finance_history); assert!(!witness.data().defaults_blocked);
        assert!(witness.data().history_image.invoices.is_none() && witness.data().history_image.payments.is_none() && witness.data().history_image.journals.is_none());
        let charts:Vec<String>=sqlx::query_scalar("SELECT to_jsonb(c)::text FROM finance_charts c WHERE company_id='company' ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        let accounts:Vec<String>=sqlx::query_scalar("SELECT to_jsonb(a)::text FROM finance_accounts a WHERE company_id='company' ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        let defaults:Vec<String>=sqlx::query_scalar("SELECT to_jsonb(d)::text FROM finance_sales_defaults d WHERE company_id='company' ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        let money:String=sqlx::query_scalar("SELECT jsonb_build_object('settings',(SELECT to_jsonb(s) FROM company_money_settings s WHERE s.company_id='company'),'scales',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.currency) FROM company_currency_scales s WHERE s.company_id='company'),'[]'::jsonb))::text").fetch_one(&mut *tx).await.unwrap();
        let image=serde_json::to_string(&(serde_json::to_string(&(charts,accounts)).unwrap(),money,serde_json::to_string(&defaults).unwrap())).unwrap();
        assert_eq!(witness.data().reference_image,image);
        assert_eq!(witness.data().expected_sales_defaults.as_ref().unwrap().version,2);
        witness.validate_mapping(&m).unwrap(); witness.validate_defaults(&m,witness.data().expected_sales_defaults.as_ref()).unwrap();
        assert!(witness.validate_defaults(&m,None).is_err());
        let mut invalid=m.clone(); invalid.cash_account_id="revenue".into(); assert!(witness.validate_mapping(&invalid).is_err());
        sqlx::query("UPDATE finance_accounts SET name='Changed' WHERE id='cash'").execute(&mut *tx).await.unwrap();
        denied(witness.require_current(&mut tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED"); tx.rollback().await.unwrap();
        let mut tx=p.raw().begin().await.unwrap(); serial(&mut tx).await;
        sqlx::query("UPDATE finance_accounts SET status='inactive' WHERE id='ar'").execute(&mut *tx).await.unwrap();
        let blocked=read_service_parts_posting_references(&mut tx,&CompanyId::new("company").unwrap(),None).await.unwrap();
        assert!(blocked.data().defaults_blocked); assert!(blocked.data().expected_sales_defaults.is_none());
        assert_eq!(blocked.validate_defaults(&m,None).unwrap_err().code,"SALES_DEFAULTS_REQUIRED");
        assert!(blocked.validate_mapping(&m).is_err()); tx.rollback().await.unwrap();
        let mut tx=p.raw().begin().await.unwrap(); serial(&mut tx).await;
        sqlx::raw_sql("UPDATE finance_charts SET status='archived' WHERE id='chart'; INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('alternate-chart','company','Alternate','USD','fixture',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,created_at,updated_at)VALUES('alternate-ar','company','alternate-chart','110','Alternate AR','asset','USD','receivable',0,0),('alternate-revenue','company','alternate-chart','400','Alternate revenue','revenue','USD','none',0,0); INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,status,reason,created_by,reviewed_by,version,created_at,updated_at) VALUES ('alternate-default','company','alternate-chart','USD','alternate-ar','alternate-revenue',NULL,'approved','reviewed','creator','reviewer',3,2,2)").execute(&mut *tx).await.unwrap();
        let fallback=read_service_parts_posting_references(&mut tx,&CompanyId::new("company").unwrap(),Some(&m)).await.unwrap();
        assert_eq!(fallback.data().expected_sales_defaults.as_ref().unwrap().id,"alternate-default");
        assert_eq!(fallback.data().expected_sales_defaults.as_ref().unwrap().version,3);
        assert!(!fallback.data().defaults_blocked); assert!(fallback.validate_mapping(&m).is_err());
        let charts:Vec<String>=sqlx::query_scalar("SELECT to_jsonb(c)::text FROM finance_charts c WHERE company_id='company' ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        let accounts:Vec<String>=sqlx::query_scalar("SELECT to_jsonb(a)::text FROM finance_accounts a WHERE company_id='company' ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        let defaults:Vec<String>=sqlx::query_scalar("SELECT to_jsonb(d)::text FROM finance_sales_defaults d WHERE company_id='company' ORDER BY id").fetch_all(&mut *tx).await.unwrap();
        let money:String=sqlx::query_scalar("SELECT jsonb_build_object('settings',(SELECT to_jsonb(s) FROM company_money_settings s WHERE s.company_id='company'),'scales',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.currency) FROM company_currency_scales s WHERE s.company_id='company'),'[]'::jsonb))::text").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(fallback.data().reference_image,serde_json::to_string(&(serde_json::to_string(&(charts,accounts)).unwrap(),money,serde_json::to_string(&defaults).unwrap())).unwrap());
        sqlx::query("UPDATE finance_accounts SET name='Changed alternative' WHERE id='alternate-ar'").execute(&mut *tx).await.unwrap();
        denied(fallback.require_current(&mut tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED"); tx.rollback().await.unwrap();
        let mut tx=p.raw().begin().await.unwrap(); serial(&mut tx).await;
        sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('customer','company','customer','CUS','Customer','THB',1,1); INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,created_by,created_at,updated_at)VALUES('invoice-z','company','customer','receivable','INV-Z','2026-01-01','2026-01-02','THB',10,0,10,10,'ar','revenue','fixture',1,1),('invoice-a','company','customer','receivable','INV-A','2026-01-01','2026-01-02','THB',20,0,20,20,'ar','revenue','fixture',1,1); INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,created_by,created_at,updated_at)VALUES('payment-z','company','receipt','PAY-Z','2026-01-01','THB',10,10,'fixture',1,1),('payment-a','company','receipt','PAY-A','2026-01-01','THB',20,20,'fixture',1,1); INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,created_by,created_at,updated_at)VALUES('journal-z','company','chart','period','J-Z','2026-01-01','THB','fixture',1,1),('journal-a','company','chart','period','J-A','2026-01-01','THB','fixture',1,1)").execute(&mut *tx).await.unwrap();
        let history=read_service_parts_posting_references(&mut tx,&CompanyId::new("company").unwrap(),None).await.unwrap(); assert!(history.data().finance_history);
        let invoices:Option<String>=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id)::text FROM finance_invoices source WHERE source.company_id='company'").fetch_one(&mut *tx).await.unwrap();
        let payments:Option<String>=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id)::text FROM finance_payments source WHERE source.company_id='company'").fetch_one(&mut *tx).await.unwrap();
        let journals:Option<String>=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id)::text FROM finance_journals source WHERE source.company_id='company'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(history.data().history_image.invoices,invoices); assert_eq!(history.data().history_image.payments,payments); assert_eq!(history.data().history_image.journals,journals);
        sqlx::query("UPDATE finance_invoices SET invoice_number='INV-CHANGED' WHERE id='invoice-a'").execute(&mut *tx).await.unwrap();
        denied(history.require_current(&mut tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED"); tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn service_parts_prerequisite_witness_preserves_original_ordered_sources_and_drift() {
    use erpx_server_finance::public::service_parts_prerequisites::*;
    use sqlx::Row;
    fixture::with_database(2,|p|async move {
        setup(&p).await;
        sqlx::raw_sql("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('a-period','company','First period','2010-01-01','2011-12-31',0); INSERT INTO company_money_settings(company_id,default_currency,updated_by,updated_at) VALUES('company','THB','fixture',1) ON CONFLICT(company_id) DO UPDATE SET default_currency='THB'; INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES('company','THB',2,'fixture',1),('company','USD',2,'fixture',1)").execute(p.raw()).await.unwrap();
        let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;
        rate(&mut tx,"z-rate","2026-01-01","35.2500",2).await;rate(&mut tx,"a-rate","2025-01-01","34.1000",1).await;
        lock_service_parts_prerequisite_references(&mut tx).await.unwrap();
        let witness=read_service_parts_prerequisite_references(&mut tx,&CompanyId::new("company").unwrap()).await.unwrap();
        let legacy_absent=read_legacy_finance_command(&mut tx,"legacy-prerequisite").await.unwrap();assert!(!legacy_absent.exists());
        sqlx::query("INSERT INTO finance_commands(command_id,company_id,operation,request_hash,created_at) VALUES('legacy-prerequisite','company','finance.chart.create','synthetic',1)").execute(&mut *tx).await.unwrap();
        denied(legacy_absent.require_current(&mut tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED");let legacy=read_legacy_finance_command(&mut tx,"legacy-prerequisite").await.unwrap();assert!(legacy.exists());legacy.require_current(&mut tx).await.unwrap();
        let periods=sqlx::query("SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version version,to_jsonb(p)::text source FROM finance_periods p WHERE p.company_id=$1 ORDER BY p.id").bind("company").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(witness.data().periods.len(),periods.len());
        for (actual,row) in witness.data().periods.iter().zip(periods) {assert_eq!(actual.id,row.get::<String,_>("id"));assert_eq!(actual.name,row.get::<String,_>("name"));assert_eq!(actual.start_date,row.get::<String,_>("start_date"));assert_eq!(actual.end_date,row.get::<String,_>("end_date"));assert_eq!(actual.state,row.get::<String,_>("state"));assert_eq!(actual.version,row.get::<i64,_>("version"));assert_eq!(actual.source,row.get::<String,_>("source"));}
        let rates=sqlx::query("SELECT r.id,r.currency,r.effective_date::text effective_date,r.currency_per_usd::text currency_per_usd,r.confirmed_at,to_jsonb(r)::text source FROM company_fx_rates r WHERE r.company_id=$1 ORDER BY r.id").bind("company").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(witness.data().rates.len(),rates.len());
        for (actual,row) in witness.data().rates.iter().zip(rates) {assert_eq!(actual.id,row.get::<String,_>("id"));assert_eq!(actual.currency,row.get::<String,_>("currency"));assert_eq!(actual.effective_date,row.get::<String,_>("effective_date"));assert_eq!(actual.currency_per_usd,row.get::<String,_>("currency_per_usd"));assert_eq!(actual.confirmed_at,row.get::<i64,_>("confirmed_at"));assert_eq!(actual.source,row.get::<String,_>("source"));}
        let money=sqlx::query("SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=$1 UNION ALL SELECT 'scale:'||s.currency kind,to_jsonb(s)::text source FROM company_currency_scales s WHERE s.company_id=$1 ORDER BY kind").bind("company").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(witness.data().money.len(),money.len());for (actual,row) in witness.data().money.iter().zip(money){assert_eq!(actual.kind,row.get::<String,_>("kind"));assert_eq!(actual.source,row.get::<String,_>("source"));}
        let foreign=read_service_parts_prerequisite_references(&mut tx,&CompanyId::new("other").unwrap()).await.unwrap();assert!(foreign.data().periods.is_empty()&&foreign.data().rates.is_empty()&&foreign.data().money.is_empty());
        let mut other_tx=p.raw().begin().await.unwrap();serial(&mut other_tx).await;denied(witness.require_current(&mut other_tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED");other_tx.rollback().await.unwrap();
        let rate_witness=read_service_parts_prerequisite_references(&mut tx,&CompanyId::new("company").unwrap()).await.unwrap();
        rate(&mut tx,"new-rate","2027-01-01","36",3).await;denied(rate_witness.require_current(&mut tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED");
        let period_witness=read_service_parts_prerequisite_references(&mut tx,&CompanyId::new("company").unwrap()).await.unwrap();
        sqlx::query("UPDATE finance_periods SET name='changed' WHERE id='period'").execute(&mut *tx).await.unwrap();denied(period_witness.require_current(&mut tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED");
        let money_witness=read_service_parts_prerequisite_references(&mut tx,&CompanyId::new("company").unwrap()).await.unwrap();
        sqlx::query("UPDATE company_currency_scales SET scale=3 WHERE company_id='company' AND currency='THB'").execute(&mut *tx).await.unwrap();denied(money_witness.require_current(&mut tx).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED");
        sqlx::query("UPDATE company_money_settings SET updated_by=repeat('x',2097153) WHERE company_id='company'").execute(&mut *tx).await.unwrap();denied(read_service_parts_prerequisite_references(&mut tx,&CompanyId::new("company").unwrap()).await,"SERVICE_PARTS_POSTING_SOURCE_LIMIT");
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn service_parts_prerequisite_lock_waits_before_serializable_snapshot() {
    use erpx_server_finance::public::service_parts_prerequisites::*;
    use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
    fixture::with_database(3,|p|async move {
        setup(&p).await;let mut writer=p.raw().begin().await.unwrap();
        sqlx::query("UPDATE finance_periods SET name='Committed before read' WHERE id='period'").execute(&mut *writer).await.unwrap();
        let read_pool=p.clone();let task=tokio::spawn(async move {PgUnitOfWork::new(read_pool).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_service_parts_prerequisite_references(c).await?;
            let witness=read_service_parts_prerequisite_references(c,&CompanyId::new("company").unwrap()).await?;
            witness.require_current(c).await?;Ok(witness.data().periods[0].name.clone())
        })).await});
        tokio::time::timeout(Duration::from_secs(10),async {loop {let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareLock' AND relation='finance_periods'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(p.raw()).await.unwrap();if waiting{break}tokio::task::yield_now().await;}}).await.unwrap();
        writer.commit().await.unwrap();assert_eq!(task.await.unwrap().unwrap(),"Committed before read");
    }).await;
}

#[tokio::test]
async fn service_prerequisite_finance_writers_verify_insert_and_unrelated_source_drift() {
    use erpx_server_finance::public::{
        service_parts_posting::{
            lock_service_parts_prerequisite_finance, read_service_parts_prerequisite_finance,
        },
        service_parts_prerequisites::*,
    };
    use erpx_server_shared::public::UserId;
    fixture::with_database(2,|p|async move {
        setup(&p).await;
        let cases=[
            ServicePrerequisiteInput::Chart{name:"New chart".into(),base_currency:"THB".into(),accounting_basis:"IFRS".into()},
            ServicePrerequisiteInput::Account{chart_id:"chart".into(),code:"122".into(),name:"New bank".into(),category:"asset".into(),parent_id:None,currency:None,is_control:false,control_kind:"bank".into()},
            ServicePrerequisiteInput::Period{name:"Future period".into(),start_date:"2031-01-01".into(),end_date:"2031-12-31".into()},
            ServicePrerequisiteInput::Rate{currency:"THB".into(),effective_date:"2027-01-01".into(),currency_per_usd:"35.125".into(),note:"Reviewed rate".into()},
        ];
        for (n,input) in cases.into_iter().enumerate(){
            let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;
            lock_service_prerequisite_writer(&mut tx,input.variant()).await.unwrap();lock_service_parts_prerequisite_finance(&mut tx).await.unwrap();lock_service_parts_prerequisite_references(&mut tx).await.unwrap();
            let before_revision:i64=sqlx::query_scalar("SELECT revision FROM finance_reporting_revisions WHERE chart_id='chart'").fetch_one(&mut *tx).await.unwrap();
            let company=CompanyId::new("company").unwrap();let finance=read_service_parts_prerequisite_finance(&mut tx,&company).await.unwrap();let refs=read_service_parts_prerequisite_references(&mut tx,&company).await.unwrap();
            let plan=PreparedServicePrerequisite::prepare(&mut tx,ServicePrerequisiteIntent{company,actor:UserId::new("fixture").unwrap(),id:format!("new-{n}"),at:7,input},&finance,&refs).await.unwrap();
            plan.apply(&mut tx,&finance,&refs).await.unwrap();plan.require_current(&mut tx,&finance,&refs).await.unwrap();
            let after_revision:i64=sqlx::query_scalar("SELECT revision FROM finance_reporting_revisions WHERE chart_id='chart'").fetch_one(&mut *tx).await.unwrap();assert_eq!(after_revision,before_revision+i64::from(n==1));
            if n==0{let new_revision:i64=sqlx::query_scalar("SELECT revision FROM finance_reporting_revisions WHERE chart_id='new-0'").fetch_one(&mut *tx).await.unwrap();assert_eq!(new_revision,1);}
            sqlx::query("UPDATE company_money_settings SET updated_at=updated_at+1 WHERE company_id='company'").execute(&mut *tx).await.unwrap();
            denied(plan.require_current(&mut tx,&finance,&refs).await,"SERVICE_PARTS_POSTING_SOURCE_CHANGED");
            tx.rollback().await.unwrap();
        }
    }).await;
}

#[tokio::test]
async fn service_prerequisite_finance_ignores_unrelated_sales_defaults() {
    use erpx_server_finance::public::service_parts_posting::*;
    fixture::with_database(1,|p|async move {
        setup(&p).await;let mut tx=p.raw().begin().await.unwrap();serial(&mut tx).await;
        sqlx::query("INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,status,reason,created_by,version,created_at,updated_at) VALUES('large-unrelated','company','chart','THB','inventory','expense','draft',repeat('x',2097153),'fixture',1,1,1)").execute(&mut *tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();let source=read_service_parts_prerequisite_finance(&mut tx,&company).await.unwrap();assert_eq!(source.data().charts.len(),1);source.require_current(&mut tx).await.unwrap();
        denied(read_service_parts_posting_references(&mut tx,&company,None).await,"SERVICE_PARTS_POSTING_SOURCE_LIMIT");
        tx.rollback().await.unwrap();
    }).await;
}
