#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_finance::public::initial_count_finance::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId};

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
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','acme','Company',NULL,'company',NULL,'active'); INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('chart','company','Primary','THB','fixture',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('inventory','company','chart','100','Inventory','asset','THB',0,0),('variance','company','chart','5400','Variance','expense','THB',0,0); INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('old','company','Old','2026-01-01','2026-12-31',0)").execute(p.raw()).await.unwrap();
}
fn basis() -> InitialCountFinanceBasis {
    InitialCountFinanceBasis {
        company_id: CompanyId::new("company").unwrap(),
        chart_id: "chart".into(),
        inventory_account_id: "inventory".into(),
        variance_account_id: "variance".into(),
        currency: "THB".into(),
    }
}
fn denied<T>(r: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match r {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, code),
        _ => panic!("expected {code}"),
    }
}
#[tokio::test]
async fn unique_current_period_and_full_source_witness_are_transaction_bound() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        let h = HoldingId::new("h001").unwrap();
        let mut tx = p.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_finance_sources(&mut tx).await.unwrap();
        let source = read_initial_count_finance(&mut tx, &h, &basis(), "2026-05-01").await.unwrap();
        assert_eq!(source.period_id(), "old");
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES('company','THB',2,'fixture',0)").execute(&mut *tx).await.unwrap();
        denied(source.require_current(&mut tx).await, "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
        sqlx::query("DELETE FROM company_currency_scales WHERE company_id='company' AND currency='THB'").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE finance_accounts SET name='Changed full row' WHERE id='variance'").execute(&mut *tx).await.unwrap();
        denied(source.require_current(&mut tx).await, "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
        sqlx::raw_sql("UPDATE finance_periods SET state='closed' WHERE id='old'; INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('replacement','company','Replacement','2026-02-01','2026-11-30',0)").execute(&mut *tx).await.unwrap();
        let replacement = read_initial_count_finance(&mut tx, &h, &basis(), "2026-05-01").await.unwrap();
        assert_eq!(replacement.period_id(), "replacement", "period is selected at posting, not from the original proof");
        sqlx::query("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('overlap','company','Overlap','2026-03-01','2026-10-31',0)").execute(&mut *tx).await.unwrap();
        denied(read_initial_count_finance(&mut tx, &h, &basis(), "2026-05-01").await, "PERIOD_UNAVAILABLE");
        tx.rollback().await.unwrap();
    }).await;
}

// Test-only source; production composition must supply actual Inventory,
// Commerce, Workflow, IAM and purchase witnesses on the same transaction.
struct TestSource {
    finance: CurrentInitialCountFinance,
    facts: InitialCountPostingFacts,
}
impl PgInitialCountPostingSource for TestSource {
    fn facts(&self) -> &InitialCountPostingFacts {
        &self.facts
    }
    fn finance(&self) -> &CurrentInitialCountFinance {
        &self.finance
    }
    async fn require_before(
        &self,
        c: &mut sqlx::PgConnection,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        self.finance.require_current(c).await
    }
    async fn require_after(
        &self,
        c: &mut sqlx::PgConnection,
        _: Option<&str>,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        self.finance.require_current(c).await
    }
}
struct TestClock;
impl erpx_server_shared::public::Clock for TestClock {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        erpx_server_shared::public::TimestampMillis::new(1800000000000)
    }
}
fn actor() -> erpx_server_finance::public::FinanceActor {
    use erpx_server_shared::public::{AssignmentId, UserId};
    erpx_server_finance::public::FinanceActor {
        user_id: UserId::new("user").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
    }
}
#[tokio::test]
async fn original_journal_effects_zero_and_transaction_rollback() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        for (key, delta) in [("gain-count",125), ("loss-count",-125), ("zero-count",0)] {
            let mut tx=p.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            lock_initial_count_finance_sources(&mut tx).await.unwrap();
            let finance=read_initial_count_finance(&mut tx,&actor().holding_id,&basis(),"2026-05-01").await.unwrap();
            let source=TestSource { finance, facts:InitialCountPostingFacts { actor:actor(), command_key:erpx_server_shared::public::CommandKey::new(key).unwrap(), count_id:format!("count-{key}"), count_number:"COUNT-1".into(), reason:"Verified physical count".into(), actual_cost_delta_minor:delta } };
            let plan=prepare_initial_count_posting(&mut tx,&source,&TestClock).await.unwrap();
            assert_eq!(plan.journal_id().is_some(),delta!=0);
            let retained=plan.persist(&mut tx).await.unwrap();
            retained.flush_and_require_after(&mut tx).await.unwrap();
            let rows:Vec<(String,i64,i64)>=sqlx::query_as("SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=$1 ORDER BY ordinal").bind(format!("journal-{key}")).fetch_all(&mut *tx).await.unwrap();
            let expected=match delta { 125=>vec![("inventory".to_string(),125,0),("variance".to_string(),0,125)], -125=>vec![("variance".to_string(),125,0),("inventory".to_string(),0,125)], _=>vec![] };
            assert_eq!(rows,expected);
            let events:i64=sqlx::query_scalar("SELECT count(*) FROM finance_financial_events WHERE source_type='stock_count' AND source_id=$1").bind(format!("count-{key}")).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(events,if delta==0 {0} else {1});
            let money:i64=sqlx::query_scalar("SELECT count(*) FROM money_transaction_snapshots WHERE company_id='company' AND source_type='finance_journal' AND source_id=$1").bind(format!("journal-{key}")).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(money,0);
            tx.rollback().await.unwrap();
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM finance_journals WHERE id=$1").bind(format!("journal-{key}")).fetch_one(p.raw()).await.unwrap();
            assert_eq!(count,0);
        }
    }).await;
}

#[tokio::test]
async fn deferred_book_projection_rejects_inactive_mapping_and_rolls_back() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('secondary','company','Secondary','THB','fixture',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('inventory2','company','secondary','100','Inventory','asset','THB',0,0),('variance2','company','secondary','5400','Variance','expense','THB',0,0); INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,created_at) VALUES('rule','company','chart','secondary','2020-01-01','author','fixture','{}','hash',0); INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) VALUES('rule','inventory','inventory2'),('rule','variance','variance2'); UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer' WHERE id='rule'; SET CONSTRAINTS ALL IMMEDIATE; UPDATE finance_accounts SET status='inactive' WHERE id='variance2'").execute(p.raw()).await.unwrap();
        let mut tx=p.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_finance_sources(&mut tx).await.unwrap();
        let finance=read_initial_count_finance(&mut tx,&actor().holding_id,&basis(),"2026-05-01").await.unwrap();
        let source=TestSource { finance, facts:InitialCountPostingFacts { actor:actor(), command_key:erpx_server_shared::public::CommandKey::new("missing-map").unwrap(), count_id:"count-map".into(), count_number:"COUNT-2".into(), reason:"Verified physical count".into(), actual_cost_delta_minor:125 } };
        let retained=prepare_initial_count_posting(&mut tx,&source,&TestClock).await.unwrap().persist(&mut tx).await.unwrap();
        assert!(retained.flush_and_require_after(&mut tx).await.is_err());
        drop(retained);
        tx.rollback().await.unwrap();
        let rows:i64=sqlx::query_scalar("SELECT count(*) FROM finance_journals WHERE source_id='count-map'").fetch_one(p.raw()).await.unwrap();
        assert_eq!(rows,0);
    }).await;
}

#[tokio::test]
async fn approved_book_uses_mapped_accounts_and_captures_both_events() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('secondary','company','Secondary','THB','fixture',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('inventory2','company','secondary','100','Inventory','asset','THB',0,0),('variance2','company','secondary','5400','Variance','expense','THB',0,0); INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,created_at) VALUES('rule','company','chart','secondary','2020-01-01','author','fixture','{}','hash',0); INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) VALUES('rule','inventory','inventory2'),('rule','variance','variance2'); UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer' WHERE id='rule'").execute(p.raw()).await.unwrap();
        let mut tx=p.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_finance_sources(&mut tx).await.unwrap();
        let finance=read_initial_count_finance(&mut tx,&actor().holding_id,&basis(),"2026-05-01").await.unwrap();
        let source=TestSource { finance, facts:InitialCountPostingFacts { actor:actor(), command_key:erpx_server_shared::public::CommandKey::new("book-count").unwrap(), count_id:"count-book".into(), count_number:"COUNT-3".into(), reason:"Verified physical count".into(), actual_cost_delta_minor:125 } };
        let retained=prepare_initial_count_posting(&mut tx,&source,&TestClock).await.unwrap().persist(&mut tx).await.unwrap();
        retained.flush_and_require_after(&mut tx).await.unwrap();
        let lines:Vec<(String,i64,i64)>=sqlx::query_as("SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id='book-rule-journal-book-count' ORDER BY ordinal").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(lines,vec![("inventory2".into(),125,0),("variance2".into(),0,125)]);
        let events:i64=sqlx::query_scalar("SELECT count(*) FROM finance_financial_events WHERE journal_id IN ('journal-book-count','book-rule-journal-book-count')").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(events,2);
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn oversized_finance_witness_is_rejected_before_transfer() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        let mut tx = p.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_finance_sources(&mut tx).await.unwrap();
        sqlx::query("UPDATE finance_charts SET name=repeat('x',70000) WHERE id='chart'").execute(&mut *tx).await.unwrap();
        denied(read_initial_count_finance(&mut tx,&actor().holding_id,&basis(),"2026-05-01").await,"COUNT_FINANCE_SOURCE_LIMIT");
        denied(read_initial_count_readiness(&mut tx,&actor().holding_id,&basis().company_id,Some("chart"),Some("inventory"),"2026-05-01").await,"COUNT_FINANCE_SOURCE_LIMIT");
        sqlx::query("UPDATE finance_charts SET name='Primary' WHERE id='chart'").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) SELECT 'bulk-'||i,'company',repeat('x',62000),(DATE '2026-01-01'+i)::text,'2026-12-31',0 FROM generate_series(1,70) i").execute(&mut *tx).await.unwrap();
        denied(read_initial_count_finance(&mut tx,&actor().holding_id,&basis(),"2026-05-01").await,"COUNT_FINANCE_SOURCE_LIMIT");
        denied(read_initial_count_readiness(&mut tx,&actor().holding_id,&basis().company_id,Some("chart"),Some("inventory"),"2026-05-01").await,"COUNT_FINANCE_SOURCE_LIMIT");
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn trigger_timestamp_tampering_is_rejected_and_rolls_back() {
    fixture::with_database(2, |p| async move {
        setup(&p).await;
        sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('secondary','company','Secondary','THB','fixture',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,created_at,updated_at) VALUES('inventory2','company','secondary','100','Inventory','asset','THB',0,0),('variance2','company','secondary','5400','Variance','expense','THB',0,0); INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,created_at) VALUES('rule','company','chart','secondary','2020-01-01','author','fixture','{}','hash',0); INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) VALUES('rule','inventory','inventory2'),('rule','variance','variance2'); UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer' WHERE id='rule'").execute(p.raw()).await.unwrap();
        for (key, function) in [
            ("bad-primary", "CREATE FUNCTION test_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_type='stock_count' THEN NEW.recorded_at:=9007199254740992; END IF; RETURN NEW; END $$; CREATE TRIGGER test_tamper BEFORE INSERT ON finance_financial_events FOR EACH ROW EXECUTE FUNCTION test_tamper()"),
            ("bad-secondary", "CREATE FUNCTION test_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_type='book_projection' THEN NEW.recorded_at:=9007199254740992; END IF; RETURN NEW; END $$; CREATE TRIGGER test_tamper BEFORE INSERT ON finance_financial_events FOR EACH ROW EXECUTE FUNCTION test_tamper()"),
            ("bad-book-time", "CREATE FUNCTION test_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.created_at:=9007199254740992; RETURN NEW; END $$; CREATE TRIGGER test_tamper BEFORE INSERT ON finance_book_postings FOR EACH ROW EXECUTE FUNCTION test_tamper()"),
            ("bad-old-event", "CREATE FUNCTION test_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_type='stock_count' THEN NEW.recorded_at:=1; END IF; RETURN NEW; END $$; CREATE TRIGGER test_tamper BEFORE INSERT ON finance_financial_events FOR EACH ROW EXECUTE FUNCTION test_tamper()"),
            ("bad-event-kind", "CREATE FUNCTION test_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_type='stock_count' THEN NEW.event_kind:='settlement'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_tamper BEFORE INSERT ON finance_financial_events FOR EACH ROW EXECUTE FUNCTION test_tamper()"),
            ("bad-book-version", "CREATE FUNCTION test_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.source_version:=2; RETURN NEW; END $$; CREATE TRIGGER test_tamper BEFORE INSERT ON finance_book_postings FOR EACH ROW EXECUTE FUNCTION test_tamper()"),
        ] {
            let mut tx=p.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            sqlx::raw_sql(function).execute(&mut *tx).await.unwrap();
            lock_initial_count_finance_sources(&mut tx).await.unwrap();
            let finance=read_initial_count_finance(&mut tx,&actor().holding_id,&basis(),"2026-05-01").await.unwrap();
            let source=TestSource { finance, facts:InitialCountPostingFacts { actor:actor(), command_key:erpx_server_shared::public::CommandKey::new(key).unwrap(), count_id:format!("count-{key}"), count_number:"COUNT-4".into(), reason:"Verified physical count".into(), actual_cost_delta_minor:125 } };
            let retained=prepare_initial_count_posting(&mut tx,&source,&TestClock).await.unwrap().persist(&mut tx).await.unwrap();
            denied(retained.flush_and_require_after(&mut tx).await,"COUNT_FINANCE_SOURCE_CHANGED");
            drop(retained);
            tx.rollback().await.unwrap();
            let rows:i64=sqlx::query_scalar("SELECT count(*) FROM finance_journals WHERE source_id=$1").bind(format!("count-{key}")).fetch_one(p.raw()).await.unwrap();
            assert_eq!(rows,0);
        }
    }).await;
}

#[tokio::test]
async fn waiting_finance_lock_precedes_serializable_snapshot() {
    use sqlx::Connection;
    use std::time::Duration;
    fixture::with_database(3, |p| async move {
        setup(&p).await;
        let mut reader = p.raw().acquire().await.unwrap();
        // The PID read occurs before BEGIN; no reader snapshot exists yet.
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let mut writer = p.raw().begin().await.unwrap();
        sqlx::raw_sql("UPDATE finance_accounts SET name='committed writer' WHERE id='variance'; UPDATE finance_periods SET state='closed' WHERE id='old'; INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('new-period','company','New','2026-02-01','2026-11-30',0)").execute(&mut *writer).await.unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let read = tokio::spawn(async move {
            let mut tx = reader.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            started_tx.send(()).unwrap();
            lock_initial_count_finance_sources(&mut tx).await.unwrap();
            let current = read_initial_count_finance(&mut tx, &actor().holding_id, &basis(), "2026-05-01").await.unwrap();
            assert_eq!(current.period_id(), "new-period");
            // Changing the account back to its old image must invalidate the
            // witness, proving it captured the writer's committed account row.
            sqlx::query("UPDATE finance_accounts SET name='Variance' WHERE id='variance'").execute(&mut *tx).await.unwrap();
            denied(current.require_current(&mut tx).await, "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
            tx.rollback().await.unwrap();
        });
        started_rx.await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event_type='Lock')").bind(pid).fetch_one(&mut *writer).await.unwrap();
                if waiting { break; }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("reader did not wait on the Finance utility lock");
        writer.commit().await.unwrap();
        read.await.unwrap();
    }).await;
}

fn bun_fallback(ids: &[String]) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let exe = std::env::var("ERPX_BUN_BIN").expect("pinned ERPX_BUN_BIN required");
    let mut child = Command::new(exe)
        .arg("-e")
        .arg("console.log(JSON.stringify([null,JSON.parse(await Bun.stdin.text())]))")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let rows: Vec<_> = ids.iter().map(|id| serde_json::json!({"id":id})).collect();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&rows).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .trim_end_matches('\n')
        .to_owned()
}
async fn retained_finance_image(c: &mut sqlx::PgConnection) -> String {
    // Execute the actual retained TypeScript SQL, preserving PostgreSQL jsonb
    // rendering rather than reproducing its property ordering in Rust.
    let source = include_str!(
        "../../../packages/server-finance/src/adapters/initial-physical-stock-count-sources.ts"
    );
    let original = source
        .split("database.prepare(`")
        .nth(1)
        .unwrap()
        .split('`')
        .next()
        .unwrap();
    let mut index = 0;
    let sql = original
        .replace('?', "\u{1}")
        .split('\u{1}')
        .enumerate()
        .map(|(i, s)| {
            if i == 0 {
                s.to_owned()
            } else {
                index += 1;
                format!("${index}{s}")
            }
        })
        .collect::<String>();
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind("2026-05-01")
        .bind("2026-05-01")
        .bind("inventory")
        .bind("variance")
        .bind("chart")
        .bind("company")
        .fetch_one(c)
        .await
        .unwrap()
}
async fn current_period_ids(c: &mut sqlx::PgConnection) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' AND start_date<=$2 AND end_date>=$3").bind("company").bind("2026-05-01").bind("2026-05-01").fetch_all(c).await.unwrap()
}
#[tokio::test]
async fn discovery_matches_retained_images_and_reports_period_readiness() {
    fixture::with_database(2,|p|async move{
    setup(&p).await;let mut tx=p.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_initial_count_finance_sources(&mut tx).await.unwrap();
    let h=HoldingId::new("h001").unwrap();let company=CompanyId::new("company").unwrap();
    let ready=read_initial_count_readiness(&mut tx,&h,&company,Some("chart"),Some("inventory"),"2026-05-01").await.unwrap();
    assert_eq!(ready.basis().unwrap().variance_account_id,"variance");assert_eq!(ready.period_id(),Some("old"));assert_eq!(ready.period_count(),1);assert_eq!(ready.context_image(),retained_finance_image(&mut tx).await);ready.require_current(&mut tx).await.unwrap();
    sqlx::query("UPDATE finance_accounts SET name='writer changed account' WHERE id='variance'").execute(&mut *tx).await.unwrap();denied(ready.require_current(&mut tx).await,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
    sqlx::query("UPDATE finance_accounts SET status='inactive' WHERE id='variance'").execute(&mut *tx).await.unwrap();
    let absent=read_initial_count_readiness(&mut tx,&h,&company,Some("chart"),Some("inventory"),"2026-05-01").await.unwrap();assert!(absent.basis().is_none());assert_eq!(absent.period_id(),Some("old"));assert_eq!(absent.context_image(),bun_fallback(&current_period_ids(&mut tx).await));
    sqlx::query("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES($1,'company','Overlap','2026-02-01','2026-11-30',0)").bind("period-\"-ไทย").execute(&mut *tx).await.unwrap();
    let overlap=read_initial_count_readiness(&mut tx,&h,&company,Some("chart"),Some("inventory"),"2026-05-01").await.unwrap();assert!(overlap.basis().is_none());assert_eq!(overlap.period_count(),2);assert_eq!(overlap.period_id(),None);assert_eq!(overlap.context_image(),bun_fallback(&current_period_ids(&mut tx).await));denied(absent.require_current(&mut tx).await,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED");
    sqlx::query("UPDATE finance_accounts SET status='active' WHERE id='variance'").execute(&mut *tx).await.unwrap();
    let configured=read_initial_count_readiness(&mut tx,&h,&company,Some("chart"),Some("inventory"),"2026-05-01").await.unwrap();assert!(configured.basis().is_some());assert_eq!(configured.period_id(),None);assert_eq!(configured.context_image(),retained_finance_image(&mut tx).await);
    sqlx::query("UPDATE finance_periods SET state='closed' WHERE company_id='company'").execute(&mut *tx).await.unwrap();
    let none=read_initial_count_readiness(&mut tx,&h,&company,Some("chart"),Some("inventory"),"2026-05-01").await.unwrap();assert!(none.basis().is_some());assert_eq!(none.period_count(),0);assert_eq!(none.period_id(),None);assert_eq!(none.context_image(),retained_finance_image(&mut tx).await);
    let no_mapping=read_initial_count_readiness(&mut tx,&h,&company,None,None,"2026-05-01").await.unwrap();assert!(no_mapping.basis().is_none());assert_eq!(no_mapping.context_image(),bun_fallback(&[]));tx.rollback().await.unwrap();
 }).await;
}
