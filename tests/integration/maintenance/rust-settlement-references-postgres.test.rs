//! Historical reference fixtures are direct database setup, not Finance posting evidence.
#[allow(dead_code, reason = "Shared Account and actual Workflow fixture")]
#[path = "../../support/finance/maintenance-parts.rs"]
mod settlement_facts;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_maintenance::public::work_wire::normalize_maintenance_change;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey, IdentifierSource, RecordId};
use erpx_service::maintenance_transitions::{
    MaintenanceTransitionsSourcePreparation, execute_maintenance_transition,
};

struct Ids(u64);
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        self.0 += 1;
        RecordId::new(format!("historical-close-event-{}", self.0)).unwrap()
    }
}
fn command(version: i64) -> erpx_server_maintenance::public::work_commands::MaintenanceWorkCommand {
    let body = serde_json::json!({"action":"complete","expectedVersion":version,"resultingCondition":"good","evidence":"Historical fixture close","reason":"Historical fixture close reason"});
    normalize_maintenance_change(&serde_json::to_vec(&body).unwrap())
        .unwrap()
        .into_command(
            CommandKey::new("historical-close").unwrap(),
            RecordId::new("work").unwrap(),
        )
}
async fn close(
    p: &settlement_facts::Parts,
    version: i64,
) -> Result<(), PgTransactionError<BusinessError>> {
    let clock = p.reviewer.system.scenario.clock.clone();
    p.reviewer
        .authorizer
        .execute_with_sources(
            "acme.example.test",
            p.reviewer.maker.clone(),
            p.issuer_selection.clone(),
            settlement_facts::issue_request(),
            MaintenanceTransitionsSourcePreparation,
            move |c, current| {
                let clock = clock.clone();
                Box::pin(async move {
                    execute_maintenance_transition(
                        c,
                        current,
                        &command(version),
                        PackageEnforcementMode::Enforced,
                        clock.as_ref(),
                        clock.as_ref(),
                        &mut Ids(0),
                    )
                    .await?;
                    Ok(())
                })
            },
        )
        .await
}
async fn original_predicate(p: &settlement_facts::Parts) -> bool {
    let original = include_str!("../../../packages/server-maintenance/src/adapters/work.ts");
    let predicate = original
        .split("`NOT EXISTS(SELECT 1 FROM (")
        .nth(1)
        .unwrap()
        .split('`')
        .next()
        .unwrap();
    fn projection(source: &str, name: &str) -> String {
        let line = source
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{name}: ")))
            .unwrap();
        serde_json::from_str(
            line.trim()
                .strip_prefix(&format!("{name}: "))
                .unwrap()
                .trim_end_matches(','),
        )
        .unwrap()
    }
    let inventory = projection(
        include_str!(
            "../../../packages/server-inventory/src/adapters/final-root-owner-projection-sources.ts"
        ),
        "operational_inventory_consumptions",
    );
    let finance = projection(
        include_str!(
            "../../../packages/server-finance/src/adapters/final-root-owner-projection-sources.ts"
        ),
        "finance_journals",
    );
    let sql = format!("SELECT NOT EXISTS(SELECT 1 FROM ({predicate}")
        .replace(
            "${inventoryProjectionSources.operational_inventory_consumptions}",
            &inventory,
        )
        .replace("${financeProjectionSources.finance_journals}", &finance)
        .replace('?', "$1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind("work")
        .fetch_one(p.pool().raw())
        .await
        .unwrap()
}
async fn historical_fixture(p: &settlement_facts::Parts, journal: &str, posted: bool) {
    // Rewrite only disposable fixture rows, bypassing evidence triggers to represent
    // pre-existing historical data; all TEXT columns and ordinary constraints remain.
    let consumption = format!("historical-{}\u{1}tail", "x".repeat(240));
    let reference = format!("historical-journal-{}\u{2}tail", "y".repeat(240));
    let mut tx = p.pool().raw().begin().await.unwrap();
    sqlx::raw_sql("ALTER TABLE operational_inventory_consumptions DISABLE TRIGGER USER; ALTER TABLE finance_journals DISABLE TRIGGER USER").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO finance_journals SELECT (jsonb_populate_record(NULL::finance_journals,to_jsonb(j)||jsonb_build_object('id',$2::text,'journal_number',$2::text,'status',$3::text,'source_id',$4::text))).* FROM finance_journals j WHERE id=$1")
        .bind(journal).bind(&reference).bind(if posted {"posted"} else {"draft"}).bind(&consumption).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "UPDATE operational_inventory_consumptions SET id=$1,journal_id=$2 WHERE journal_id=$3",
    )
    .bind(&consumption)
    .bind(&reference)
    .bind(journal)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::raw_sql("ALTER TABLE operational_inventory_consumptions ENABLE TRIGGER USER; ALTER TABLE finance_journals ENABLE TRIGGER USER").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
#[tokio::test]
async fn long_control_historical_posted_reference_closes_and_replays() {
    settlement_facts::account::with_system(|system| async move {
        let p=settlement_facts::setup_parts(system).await;
        let journal=p.issue("historical-seed-posted",1).await;
        historical_fixture(&p,&journal,true).await;
        assert!(original_predicate(&p).await);
        close(&p,2).await.unwrap();
        close(&p,2).await.unwrap();
        let (status, version, receipts):(String,i64,i64)=sqlx::query_as("SELECT status,version,(SELECT count(*) FROM command_receipts WHERE command_id='historical-close') FROM maintenance_work_orders WHERE id='work'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!((status,version,receipts),("completed".into(),3,1));
    }).await;
}
#[tokio::test]
async fn long_control_historical_draft_follows_work_cas_and_blocks() {
    settlement_facts::account::with_system(|system| async move {
        let p=settlement_facts::setup_parts(system).await;
        let journal=p.issue("historical-seed-draft",1).await;
        historical_fixture(&p,&journal,false).await;
        assert!(!original_predicate(&p).await);
        // An ordinary stale version reaches the late CAS; the numeric boundary has separate coverage.
        for version in [3,2] {
            assert!(matches!(close(&p,version).await,Err(PgTransactionError::Business(ref e)) if e.code=="CONFLICT"));
        }
        let (status,version,receipts):(String,i64,i64)=sqlx::query_as("SELECT status,version,(SELECT count(*) FROM command_receipts WHERE command_id='historical-close') FROM maintenance_work_orders WHERE id='work'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!((status,version,receipts),("in_progress".into(),2,0));
    }).await;
}

#[tokio::test]
async fn deferred_journal_rewrite_rolls_back_close_and_all_workflow_effects() {
    settlement_facts::account::with_system(|system| async move {
        let p = settlement_facts::setup_parts(system).await;
        let journal = p.issue("historical-seed-deferred", 1).await;
        historical_fixture(&p, &journal, true).await;
        let before: (i64,i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events),(SELECT count(*) FROM maintenance_work_events),(SELECT count(*) FROM operational_asset_events),(SELECT count(*) FROM staff_notifications)").fetch_one(p.pool().raw()).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION synthetic_close_journal_rewrite() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_journals SET memo='deferred fixture rewrite' WHERE id=(SELECT journal_id FROM operational_inventory_consumptions WHERE source_id='work'); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER synthetic_close_journal_rewrite AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_close_journal_rewrite()") .execute(p.pool().raw()).await.unwrap();
        assert!(matches!(close(&p,2).await,Err(PgTransactionError::Business(ref e)) if e.code=="JOURNAL_SETTLEMENT_SOURCE_CHANGED"));
        let after: (i64,i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events),(SELECT count(*) FROM maintenance_work_events),(SELECT count(*) FROM operational_asset_events),(SELECT count(*) FROM staff_notifications)").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(after,before);
        let state: (String,i64,bool) = sqlx::query_as("SELECT w.status,w.version,j.memo<>'deferred fixture rewrite' FROM maintenance_work_orders w JOIN operational_inventory_consumptions p ON p.source_id=w.id JOIN finance_journals j ON j.id=p.journal_id WHERE w.id='work'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(state,("in_progress".into(),2,true));
    }).await;
}
