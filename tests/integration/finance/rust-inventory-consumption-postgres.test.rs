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
