#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_assets::public::asset_register::*;
use erpx_server_assets::public::retirement_work::{AssetRetirementWorkEffect, AssetRetirementWorkSource};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

type Error = PgTransactionError<BusinessError>;

struct FinanceCurrencyCatalogue;
impl AssetCurrencyCatalogue for FinanceCurrencyCatalogue {
    fn contains(&self, currency: &str) -> Result<bool, BusinessError> {
        erpx_server_finance::public::embedded_currency_rules()
            .map(|rules| rules.contains(currency))
            .map_err(|_| BusinessError::new(erpx_server_shared::public::FailureKind::Unavailable, "ASSET_CURRENCY_CATALOGUE_UNAVAILABLE", "The currency catalogue is unavailable."))
    }
    fn scale(&self, currency: &str, company_scale: Option<u32>) -> Result<Option<u32>, BusinessError> {
        let rules=erpx_server_finance::public::embedded_currency_rules()
            .map_err(|_| BusinessError::new(erpx_server_shared::public::FailureKind::Unavailable,"ASSET_CURRENCY_CATALOGUE_UNAVAILABLE","The currency catalogue is unavailable."))?;
        Ok(rules.scale(currency,company_scale.map(i64::from)).ok().map(u32::from))
    }
}



struct TestRetirementSource;
struct TestRetirementEffect { asset_id: String, clear: bool }
impl AssetRetirementWorkEffect for TestRetirementEffect {
    fn is_clear(&self) -> bool { self.clear }
    fn require_current<'a>(&'a self, c: &'a mut sqlx::PgConnection) -> AssetFuture<'a, (), Error> {
        Box::pin(async move {
            let clear: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM public.maintenance_work_orders WHERE asset_id=$1 AND status NOT IN ('completed','cancelled'))")
                .bind(&self.asset_id).fetch_one(c).await?;
            if clear != self.clear {
                return Err(BusinessError::new(FailureKind::Conflict, "ASSET_RETIREMENT_WORK_CHANGED", "The Maintenance work affecting the Asset changed.").into());
            }
            Ok(())
        })
    }
}
impl AssetRetirementWorkSource for TestRetirementSource {
    type Effect = Box<dyn AssetRetirementWorkEffect + 'static>;
    fn observe<'a>(&'a mut self, c: &'a mut sqlx::PgConnection, asset_id: &'a str) -> AssetFuture<'a, Self::Effect, Error> {
        Box::pin(async move {
            let clear: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM public.maintenance_work_orders WHERE asset_id=$1 AND status NOT IN ('completed','cancelled'))")
                .bind(asset_id).fetch_one(c).await?;
            Ok(Box::new(TestRetirementEffect { asset_id: asset_id.to_owned(), clear }) as Box<dyn AssetRetirementWorkEffect + 'static>)
        })
    }
}


macro_rules! register_retirement {
    ($($arg:expr),* $(,)?) => {
        PgAssetRegister::new($($arg),*).with_retirement_work_source(TestRetirementSource)
    };
}

async fn setup(pool: &DatabasePool) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(path, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES\
         ('company','company','company','Synthetic Company',NULL,'company',NULL,'active'),\
         ('branch','branch','branch','Synthetic Branch','company','company',NULL,'active'),\
         ('department','department','department','Synthetic Department','branch','company','branch','active'),\
         ('company-b','company','company-b','Foreign Company',NULL,'company-b',NULL,'active'),\
         ('branch-b','branch','branch-b','Foreign Branch','company-b','company-b',NULL,'active'),\
         ('department-b','department','department-b','Foreign Department','branch-b','company-b','branch-b','active');\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES\
         ('person-1','user-1','company','Custodian One','TH','active','fixture',0,0),\
         ('person-2','user-2','company','Reviewer Two','TH','active','fixture',0,0),\
         ('person-3','user-3','company-b','Foreign Person','TH','active','fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES\
         ('engagement-1','person-1','company','employee','salaried','active','2020-01-01','Custodian','THB','fixture',0,0),\
         ('engagement-2','person-2','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0);\
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,created_by,created_at,updated_at) VALUES('chart-1','company','Synthetic Chart','THB','active','fixture',0,0);\
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) VALUES('period-1','company','2026','2026-01-01','2026-12-31','open',0);\
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,created_at,updated_at) VALUES\
         ('account-1','chart-1','company','1000','Asset','asset',0,0),\
         ('account-2','chart-1','company','2000','Proceeds','asset',0,0),\
         ('account-3','chart-1','company','3000','Gain','other_income',0,0),\
         ('account-4','chart-1','company','4000','Loss','other_expense',0,0);\
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,created_by,created_at,updated_at) VALUES('journal-1','company','chart-1','period-1','J-1','2026-06-01','THB','posted','fixture',0,0);\
         INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,cost_minor,useful_life_months,in_service_date,status,currency,version,activation_journal_id,created_by,created_at,updated_at) VALUES\
         ('finance-1','company','FA-1','Finance Asset','account-1',1000,12,'2026-01-01','in_service','THB',2,'journal-1','fixture',0,0),\
         ('finance-2','company','FA-2','Finance Asset Two','account-1',1000,12,'2026-01-01','in_service','THB',1,NULL,'fixture',0,0),\
         ('finance-foreign','company-b','FA-F','Foreign Finance Asset','account-1',1000,12,'2026-01-01','in_service','THB',1,NULL,'fixture',0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor(user: &str, company: &str, branch: Option<&str>, department: Option<&str>) -> AssetActor {
    AssetActor {
        user_id: UserId::new(user).unwrap(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "finance.read".into(),
            "finance.write".into(),
            "records.review".into(),
        ],
        modules: vec!["assets".into()],
    }
}

async fn insert_asset(
    pool: &DatabasePool,
    id: &str,
    company: &str,
    branch: &str,
    department: Option<&str>,
    cost_minor: i64,
    finance_asset_id: Option<(&str, i64)>,
) {
    sqlx::query(
        "INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,acquired_date,finance_asset_id,finance_asset_version,created_by,created_at,updated_at) \
         VALUES($1,$2,$3,$4,$1,concat('Asset ',$1),'equipment','THB',$5,'2026-01-01',$6,$7,'fixture',0,0)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .bind(department)
    .bind(cost_minor)
    .bind(finance_asset_id.map(|value| value.0))
    .bind(finance_asset_id.map(|value| value.1))
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO operational_asset_events(id,asset_id,version,action,actor_id,reason,condition,lifecycle_status,placement,custodian_id,created_at)          VALUES($1,$2,1,'register','fixture','Asset registered','good','registered',NULL,NULL,0)",
    )
    .bind(format!("event-{id}"))
    .bind(id)
    .execute(pool.raw())
    .await
    .unwrap();
    if let Some((finance_asset_id, finance_asset_version)) = finance_asset_id {
        sqlx::query(
            "INSERT INTO operational_asset_finance_links(id,asset_id,finance_asset_id,asset_version,finance_asset_version,cost_minor,currency,reason,created_by,created_at) \
             VALUES($1,$2,$3,1,$4,$5,'THB','Fixture link','fixture',0)",
        )
        .bind(format!("link-{id}"))
        .bind(id)
        .bind(finance_asset_id)
        .bind(finance_asset_version)
        .bind(cost_minor)
        .execute(pool.raw())
        .await
        .unwrap();
    }
}

#[derive(Default)]
struct RecordingCost {
    calls: Arc<AtomicUsize>,
}
impl AcquisitionCostPort for RecordingCost {
    type Error = Error;
    fn prepare<'a>(&'a mut self, _: &'a str, _: &'a NewAsset) -> AssetFuture<'a, (), Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

async fn command<F>(pool: &DatabasePool, work: F) -> Result<(), Error>
where
    F: for<'c> FnOnce(
        &'c mut sqlx::PgConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), Error>> + Send + 'c>,
    >,
{
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    let result = work(&mut tx).await;
    match result {
        Ok(()) => {
            tx.commit().await.unwrap();
            Ok(())
        }
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    }
}

#[tokio::test]
async fn workspace_detail_history_and_maintenance_are_company_and_scope_bounded() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", Some("department"), 0, None).await;
        insert_asset(&pool, "asset-b", "company", "branch-b", None, 0, None).await;
        insert_asset(
            &pool,
            "asset-foreign",
            "company-b",
            "branch-b",
            None,
            0,
            None,
        )
        .await;
        sqlx::query(
            "INSERT INTO maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) \
             VALUES('work-a','company','branch','department','asset-a','Synthetic work','normal','scheduled','user-1','fixture',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let mut read = pool.raw().acquire().await.unwrap();
        let workspace = asset_workspace(
            &actor("user-1", "company", None, None),
            &mut PgAssetRegister::new(&mut read, actor("user-1", "company", None, None)),
        )
        .await
        .unwrap();
        assert_eq!(workspace.items.len(), 2);
        assert!(workspace.items.iter().all(|item| item.company_id == "company"));
        assert_eq!(workspace.finance_assets.len(), 2);
        assert!(workspace.can_create && workspace.can_link_finance);
        assert_eq!(workspace.people.len(), 2);
        drop(read);

        let mut read = pool.raw().acquire().await.unwrap();
        let detail = asset_detail(
            &actor("user-1", "company", None, None),
            &mut PgAssetRegister::new(&mut read, actor("user-1", "company", None, None)),
            "asset-a",
        )
        .await
        .unwrap();
        assert_eq!(detail.asset.id, "asset-a");
        assert_eq!(detail.history.len(), 1);
        assert_eq!(detail.history[0].action, "register");
        assert_eq!(detail.maintenance.len(), 1);
        assert_eq!(detail.maintenance[0].id, "work-a");
        assert!(detail.finance.is_none());
        assert!(matches!(
            asset_detail(
                &actor("user-1", "company", None, None),
                &mut PgAssetRegister::new(&mut read, actor("user-1", "company", None, None)),
                "asset-foreign"
            )
            .await,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_NOT_FOUND"
        ));
        drop(read);

        let mut read = pool.raw().acquire().await.unwrap();
        let branch = asset_workspace(
            &actor("user-1", "company", Some("branch-b"), None),
            &mut PgAssetRegister::new(&mut read, actor("user-1", "company", Some("branch-b"), None)),
        )
        .await
        .unwrap();
        assert_eq!(branch.items.len(), 1);
        assert_eq!(branch.items[0].id, "asset-b");
        assert_eq!(branch.finance_assets.len(), 2);
    })
    .await;
}

#[tokio::test]
async fn registration_is_scoped_uses_the_cost_handoff_and_persists_direct_readback() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let input = json!({
            "branchId": "branch-b",
            "departmentId": "department-b",
            "code": " NEW-1 ",
            "name": "  New Asset  ",
            "category": " equipment ",
            "currency": "thb",
            "costMinor": 1000,
            "acquiredDate": "2026-02-28",
            "financeAssetId": "finance-1",
            "financeAssetVersion": 2
        });
        let mut cost = RecordingCost::default();
        let calls = cost.calls.clone();
        command(&pool, |c| {
            Box::pin(async move {
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor("user-1", "company", Some("branch"), Some("department")));
                let outcome = create_asset(
                    &actor("user-1", "company", Some("branch"), Some("department")),
                    &mut repository,
                    &mut ledger,
                    &mut cost,
                    &FinanceCurrencyCatalogue,
                    &input,
                    "create-key",
                    500,
                )
                .await?;
                assert_eq!(outcome.result.version, 1);
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                Ok(())
            })
        })
        .await
        .unwrap();
        let row: (String, String, String, String, i64, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT branch_id,department_id,code,name,cost_minor,finance_asset_id,finance_asset_version FROM operational_assets WHERE id='asset-create-key'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "branch".into(),
                "department".into(),
                "NEW-1".into(),
                "New Asset".into(),
                1000,
                Some("finance-1".into()),
                Some(2)
            )
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operational_asset_events WHERE asset_id='asset-create-key' AND action='register' AND version=1"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operational_asset_finance_links WHERE asset_id='asset-create-key' AND asset_version=1"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn placement_condition_and_retirement_are_persisted_with_direct_readback() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", Some("department"), 0, None).await;
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                let placed = change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"place","expectedVersion":1,"placement":" HQ desk ","custodianId":"user-1","reason":" Assign custodian "}),
                    "place-key",
                    100,
                )
                .await?;
                assert_eq!(placed.result.version, 2);
                assert_eq!(placed.result.status, "in_service");

                let conditioned = change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"condition","expectedVersion":2,"condition":"degraded","reason":" Damaged "}),
                    "condition-key",
                    200,
                )
                .await?;
                assert_eq!(conditioned.result.version, 3);

                let requested = change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-request","expectedVersion":3,"reason":" End of life "}),
                    "retire-request-key",
                    300,
                )
                .await?;
                assert_eq!(requested.result.status, "retirement_pending");
                Ok(())
            })
        })
        .await
        .unwrap();

        let row: (String, String, Option<String>, Option<String>, Option<String>, i64) = sqlx::query_as(
            "SELECT lifecycle_status,condition,placement,custodian_id,retirement_reason,version FROM operational_assets WHERE id='asset-a'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "retirement_pending".into(),
                "degraded".into(),
                Some("HQ desk".into()),
                Some("user-1".into()),
                Some("End of life".into()),
                4
            )
        );
        let events: Vec<(String, i64)> = sqlx::query_as(
            "SELECT action,version FROM operational_asset_events WHERE asset_id='asset-a' ORDER BY version",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![
                ("register".into(), 1),
                ("place".into(), 2),
                ("condition".into(), 3),
                ("retire-request".into(), 4)
            ]
        );

        command(&pool, |c| {
            Box::pin(async move {
                let reviewer = actor("user-2", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, reviewer.clone());
                let retired = change_asset(
                    &reviewer,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-approve","expectedVersion":4,"reason":" Disposal approved "}),
                    "retire-approve-key",
                    400,
                )
                .await?;
                assert_eq!(retired.result.version, 5);
                assert_eq!(retired.result.status, "retired");
                Ok(())
            })
        })
        .await
        .unwrap();
        let row: (String, Option<String>, Option<String>, Option<String>, i64) = sqlx::query_as(
            "SELECT lifecycle_status,placement,custodian_id,retirement_reason,version FROM operational_assets WHERE id='asset-a'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "retired".into(),
                None,
                None,
                Some("End of life".into()),
                5
            )
        );
    })
    .await;
}

#[tokio::test]
async fn stale_versions_foreign_company_and_other_branch_refuse_without_writes() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        insert_asset(&pool, "asset-other-branch", "company", "branch-b", None, 0, None).await;

        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"condition","expectedVersion":9,"condition":"good","reason":" Stale "}),
                    "stale-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "CONFLICT"
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT version FROM operational_assets WHERE id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operational_asset_events WHERE asset_id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );

        let result = command(&pool, |c| {
            Box::pin(async move {
                let foreign = actor("user-1", "company-b", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, foreign.clone());
                change_asset(
                    &foreign,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"condition","expectedVersion":1,"condition":"good","reason":" Foreign "}),
                    "foreign-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_NOT_FOUND"
        ));

        let result = command(&pool, |c| {
            Box::pin(async move {
                let branch = actor("user-1", "company", Some("branch"), None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, branch.clone());
                change_asset(
                    &branch,
                    &mut repository,
                    &mut ledger,
                    "asset-other-branch",
                    &json!({"action":"condition","expectedVersion":1,"condition":"good","reason":" Other branch "}),
                    "branch-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_NOT_FOUND"
        ));
    })
    .await;
}

#[tokio::test]
async fn placement_requires_current_finance_recognition_and_an_eligible_custodian() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(
            &pool,
            "asset-cost",
            "company",
            "branch",
            None,
            1000,
            Some(("finance-2", 1)),
        )
        .await;
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-cost",
                    &json!({"action":"place","expectedVersion":1,"placement":"HQ","custodianId":"user-1","reason":" Assign "}),
                    "place-blocked-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "CONFLICT"
        ));
        sqlx::query(
            "UPDATE finance_assets SET activation_journal_id='journal-1' WHERE id='finance-2'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-cost",
                    &json!({"action":"place","expectedVersion":1,"placement":"HQ","custodianId":"user-3","reason":" Ineligible "}),
                    "place-custodian-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "CONFLICT"
        ));
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-cost",
                    &json!({"action":"place","expectedVersion":1,"placement":"HQ","custodianId":"user-2","reason":" Assign "}),
                    "place-ok-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT lifecycle_status FROM operational_assets WHERE id='asset-cost'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "in_service"
        );
    })
    .await;
}

#[tokio::test]
async fn retirement_requires_independent_review_and_a_posted_disposal() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(
            &pool,
            "asset-cost",
            "company",
            "branch",
            None,
            1000,
            Some(("finance-1", 2)),
        )
        .await;
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-cost",
                    &json!({"action":"retire-request","expectedVersion":1,"reason":" End of life "}),
                    "request-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();

        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-cost",
                    &json!({"action":"retire-approve","expectedVersion":2,"reason":" Self approve "}),
                    "self-key",
                    200,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_RETIREMENT_REVIEW_INDEPENDENT"
        ));

        let result = command(&pool, |c| {
            Box::pin(async move {
                let reviewer = actor("user-2", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, reviewer.clone());
                change_asset(
                    &reviewer,
                    &mut repository,
                    &mut ledger,
                    "asset-cost",
                    &json!({"action":"retire-approve","expectedVersion":2,"reason":" Approve "}),
                    "disposal-key",
                    200,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_FINANCE_DISPOSAL_REQUIRED"
        ));

        sqlx::raw_sql(
            "UPDATE finance_assets SET status='retired' WHERE id='finance-1';\
             INSERT INTO finance_asset_disposals(id,asset_id,company_id,asset_version,period_id,entry_date,proceeds_minor,gain_account_id,loss_account_id,evidence_ref,status,journal_id,created_by,decided_by,created_at,updated_at) \
             VALUES('disposal-1','finance-1','company',2,'period-1','2026-06-01',0,'account-3','account-4','evidence','posted','journal-1','fixture','fixture',0,0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        command(&pool, |c| {
            Box::pin(async move {
                let reviewer = actor("user-2", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, reviewer.clone());
                let retired = change_asset(
                    &reviewer,
                    &mut repository,
                    &mut ledger,
                    "asset-cost",
                    &json!({"action":"retire-approve","expectedVersion":2,"reason":" Approve "}),
                    "approve-key",
                    300,
                )
                .await?;
                assert_eq!(retired.result.status, "retired");
                Ok(())
            })
        })
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT lifecycle_status FROM operational_assets WHERE id='asset-cost'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "retired"
        );
    })
    .await;
}

#[tokio::test]
async fn retirement_refresh_is_limited_to_the_original_requester() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-request","expectedVersion":1,"reason":" First reason "}),
                    "first-request-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let other = actor("user-2", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, other.clone());
                change_asset(
                    &other,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-request","expectedVersion":2,"reason":" Hijack reason "}),
                    "hijack-key",
                    200,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_RETIREMENT_REFRESH_FORBIDDEN"
        ));
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                let renewed = change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-request","expectedVersion":2,"reason":" Renewed reason "}),
                    "renew-key",
                    300,
                )
                .await?;
                assert_eq!(renewed.result.version, 3);
                Ok(())
            })
        })
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT retirement_reason FROM operational_assets WHERE id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "Renewed reason"
        );
    })
    .await;
}

#[tokio::test]
async fn retired_condition_placement_during_maintenance_and_active_work_are_refused() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        sqlx::query(
            "INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) \
             VALUES('work-a','company','branch','asset-a','Active work','normal','in_progress','user-1','fixture',0,0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-request","expectedVersion":1,"reason":" Active work "}),
                    "active-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "CONFLICT"
        ));
        sqlx::query("UPDATE maintenance_work_orders SET status='completed',completion_evidence='done',resulting_condition='good' WHERE id='work-a'")
            .execute(pool.raw())
            .await
            .unwrap();
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-request","expectedVersion":1,"reason":" Ready "}),
                    "ready-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"place","expectedVersion":2,"placement":"HQ","custodianId":"user-1","reason":" Pending place "}),
                    "pending-place-key",
                    200,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_PLACEMENT_BLOCKED"
        ));
    })
    .await;
}

#[tokio::test]
async fn finance_link_is_immutable_and_checks_the_current_finance_version() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 1000, None).await;
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"link-finance","expectedVersion":1,"financeAssetId":"finance-1","financeAssetVersion":1,"reason":" Stale finance "}),
                    "stale-finance-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "CONFLICT"
        ));
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                let linked = change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"link-finance","expectedVersion":1,"financeAssetId":"finance-1","financeAssetVersion":2,"reason":" Link "}),
                    "link-key",
                    100,
                )
                .await?;
                assert_eq!(linked.result.version, 2);
                Ok(())
            })
        })
        .await
        .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"link-finance","expectedVersion":2,"financeAssetId":"finance-2","financeAssetVersion":1,"reason":" Replace "}),
                    "replace-key",
                    200,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_FINANCE_LINK_IMMUTABLE"
        ));
    })
    .await;
}

#[tokio::test]
async fn replay_returns_the_retained_receipt_without_touching_the_asset() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        struct PriorLedger;
        impl AssetCommandLedger for PriorLedger {
            type Error = Error;
            fn replay<'a>(
                &'a mut self,
                key: &'a str,
                operation: &'a str,
                record_id: &'a str,
                _: &'a Value,
            ) -> AssetFuture<'a, Option<Value>, Error> {
                assert_eq!(key, "replay-key");
                assert_eq!(operation, "asset.condition");
                assert_eq!(record_id, "asset-a");
                Box::pin(async { Ok(Some(json!({"id":"asset-a","version":4,"status":"in_service"}))) })
            }
        }
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let actor = actor("user-1", "company", None, None);
        let mut repository = PgAssetRegister::new(&mut tx, actor.clone());
        let mut ledger = PriorLedger;
        let replay = change_asset(
            &actor,
            &mut repository,
            &mut ledger,
            "asset-a",
            &json!({"action":"condition","expectedVersion":1,"condition":"unsafe","reason":" Replay "}),
            "replay-key",
            100,
        )
        .await
        .unwrap();
        assert_eq!(replay.result.version, 4);
        assert!(replay.receipt.is_none());
        tx.commit().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operational_asset_events WHERE asset_id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT version FROM operational_assets WHERE id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn a_deferred_trigger_rewrite_fails_the_receipt_and_rolls_back_all_owner_effects() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        sqlx::raw_sql(
            "CREATE FUNCTION synthetic_asset_rewrite() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE operational_assets SET name='deferred rewrite' WHERE id=NEW.asset_id; RETURN NEW; END $$;\
             CREATE CONSTRAINT TRIGGER synthetic_asset_rewrite AFTER INSERT ON operational_asset_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_asset_rewrite();",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let actor = actor("user-1", "company", None, None);
        let mut repository = PgAssetRegister::new(&mut tx, actor.clone());
        let outcome = change_asset(
            &actor,
            &mut repository,
            &mut NoCommandLedger::<Error>::default(),
            "asset-a",
            &json!({"action":"condition","expectedVersion":1,"condition":"degraded","reason":" Deferred "}),
            "deferred-key",
            100,
        )
        .await
        .unwrap();
        assert_eq!(outcome.result.version, 2);
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(matches!(
            outcome.receipt.unwrap().require_current(&mut tx).await,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_SOURCE_CHANGED"
        ));
        tx.rollback().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT version FROM operational_assets WHERE id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operational_asset_events WHERE asset_id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT name FROM operational_assets WHERE id='asset-a'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "Asset asset-a"
        );
    })
    .await;
}

#[tokio::test]
async fn conditional_retirement_rule_keeps_unsafe_assets_from_being_placed_and_retired_from_conditioning()
 {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"condition","expectedVersion":1,"condition":"unsafe","reason":" Unsafe "}),
                    "unsafe-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"place","expectedVersion":2,"placement":"HQ","custodianId":"user-1","reason":" Unsafe place "}),
                    "unsafe-place-key",
                    200,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_UNSAFE"
        ));
        command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-request","expectedVersion":2,"reason":" Retire unsafe "}),
                    "unsafe-retire-key",
                    200,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
        command(&pool, |c| {
            Box::pin(async move {
                let reviewer = actor("user-2", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, reviewer.clone());
                change_asset(
                    &reviewer,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"retire-approve","expectedVersion":3,"reason":" Approve "}),
                    "unsafe-approve-key",
                    250,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = register_retirement!(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"condition","expectedVersion":4,"condition":"good","reason":" Retired condition "}),
                    "retired-condition-key",
                    300,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSET_RETIRED"
        ));
    })
    .await;
}

#[tokio::test]
async fn registration_refuses_foreign_company_sources_and_unscoped_users() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let mut read = pool.raw().acquire().await.unwrap();
        let no_write = AssetActor {
            user_id: UserId::new("user-1").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            permissions: Vec::new(),
            modules: vec!["assets".into()],
        };
        assert!(matches!(
            asset_workspace(&no_write, &mut PgAssetRegister::new(&mut read, no_write.clone())).await,
            Err(PgTransactionError::Business(ref error)) if error.code == "ASSETS_ACCESS_CHANGED"
        ));
        drop(read);
        let input = json!({
            "branchId":"branch","code":"NEW-2","name":"No Finance","category":"equipment","currency":"THB",
            "costMinor":1000,"acquiredDate":"2026-02-28"
        });
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut repository = PgAssetRegister::new(c, actor.clone());
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut cost = RecordingCost::default();
                create_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    &mut cost,
                    &FinanceCurrencyCatalogue,
                    &input,
                    "foreign-finance-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        // A missing Finance link is valid for an owned acquisition cost; the
        // separate Finance handoff is not replaced by this owner adapter.
        assert!(result.is_ok());
    })
    .await;
}

#[tokio::test]
async fn a_soft_deleted_custodian_is_not_eligible_for_placement() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        sqlx::query("UPDATE hr_people SET status='inactive' WHERE user_id='user-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let result = command(&pool, |c| {
            Box::pin(async move {
                let actor = actor("user-1", "company", None, None);
                let mut ledger = NoCommandLedger::<Error>::default();
                let mut repository = PgAssetRegister::new(c, actor.clone());
                change_asset(
                    &actor,
                    &mut repository,
                    &mut ledger,
                    "asset-a",
                    &json!({"action":"place","expectedVersion":1,"placement":"HQ","custodianId":"user-1","reason":" Inactive "}),
                    "inactive-key",
                    100,
                )
                .await?;
                Ok(())
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(ref error)) if error.code == "CONFLICT"
        ));
    })
    .await;
}

#[tokio::test]
async fn read_only_workspace_uses_serializable_scope_arguments_without_effects() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", Some("department"), 0, None).await;
        insert_asset(&pool, "asset-b", "company", "branch", None, 0, None).await;
        let mut read = pool.raw().acquire().await.unwrap();
        let department = actor("user-1", "company", Some("branch"), Some("department"));
        let workspace = asset_workspace(
            &department,
            &mut PgAssetRegister::new(&mut read, department.clone()),
        )
        .await
        .unwrap();
        assert_eq!(workspace.items.len(), 1);
        assert_eq!(workspace.items[0].id, "asset-a");
        drop(read);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM operational_asset_events WHERE asset_id IN ('asset-a','asset-b')"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
    })
    .await;
}

#[tokio::test]
async fn deletion_and_event_mutation_remain_blocked_by_the_schema_guard() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool, "asset-a", "company", "branch", None, 0, None).await;
        let event = sqlx::query(
            "UPDATE operational_asset_events SET reason='tampered' WHERE asset_id='asset-a'",
        )
        .execute(pool.raw())
        .await;
        assert!(event.is_err());
        let delete = sqlx::query("DELETE FROM operational_assets WHERE id='asset-a'")
            .execute(pool.raw())
            .await;
        assert!(delete.is_err());
        let _ = Duration::from_millis(0);
    })
    .await;
}

#[tokio::test]
async fn asset_unicode_owner_rows_events_and_invalid_rollback() {
    // Owner SQL readback only: NoCommandLedger and RecordingCost are fixture
    // ports, not a claim about selected IAM, Workflow or Money handoff.
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        // Exercise the real Finance catalogue before any owner effect. A
        // scale-less ISO entry is valid with minor units, while unknown codes
        // are invalid regardless of which cost representation was supplied.
        assert!(validate_registration_currency(&FinanceCurrencyCatalogue, &json!({"currency":"XAU"})).is_ok());
        for (key,cost) in [("unknown-iso-minor",json!({"costMinor":0})),("unknown-iso-decimal",json!({"cost":"0"}))] {
            let mut input=json!({"branchId":"branch","code":key,"name":"Unknown currency","category":"equipment","currency":"ZZZ","acquiredDate":"2026-01-01"});
            input.as_object_mut().unwrap().extend(cost.as_object().unwrap().clone());
            let cost_calls=Arc::new(AtomicUsize::new(0));let observed=cost_calls.clone();
            let result=command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost{calls:cost_calls};create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,key,500).await?;Ok(())})).await;
            assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="VALIDATION_FAILED"));
            assert_eq!(observed.load(Ordering::SeqCst),0);
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM operational_assets WHERE id=$1").bind(format!("asset-{key}")).fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
        }
        let title="😀".repeat(160);let category="🧭".repeat(80);let placement="📍".repeat(200);let reason="🛠".repeat(2000);
        for (key,name) in [("unicode-asset-too-long","😀".repeat(161)),("unicode-asset-too-short","😀".to_owned())] {
            let input=json!({"branchId":"branch","code":format!("BAD-{key}"),"name":name,"category":"equipment","currency":"THB","costMinor":0,"acquiredDate":"2026-01-01"});
            let result=command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,key,500).await?;Ok(())})).await;
            assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="VALIDATION_FAILED"));
            let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM operational_assets WHERE id=$1),(SELECT count(*) FROM operational_asset_events WHERE asset_id=$1)").bind(format!("asset-{key}")).fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0));
        }
        let input=json!({"branchId":"branch","code":"unicode-asset","name":format!("\u{feff}{title}\u{feff}"),"category":category,"currency":"uſd","costMinor":0,"acquiredDate":"2026-01-01"});
        command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,"unicode-asset-create",500).await?;Ok(())})).await.unwrap();
        let before:(String,String,i64)=sqlx::query_as("SELECT name,category,version FROM operational_assets WHERE id='asset-unicode-asset-create'").fetch_one(pool.raw()).await.unwrap();assert_eq!(before,(title.clone(),category.clone(),1));
        let currency:String=sqlx::query_scalar("SELECT currency FROM operational_assets WHERE id='asset-unicode-asset-create'").fetch_one(pool.raw()).await.unwrap();assert_eq!(currency,"USD");
        let input=json!({"action":"place","expectedVersion":1,"placement":placement,"custodianId":"user-1","reason":reason});
        command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();change_asset(&a,&mut repo,&mut ledger,"asset-unicode-asset-create",&input,"unicode-asset-place",600).await?;Ok(())})).await.unwrap();
        let stored:(String,String,i64)=sqlx::query_as("SELECT placement,lifecycle_status,version FROM operational_assets WHERE id='asset-unicode-asset-create'").fetch_one(pool.raw()).await.unwrap();assert_eq!(stored,(placement,"in_service".into(),2));
        let events:Vec<(i64,String,String)>=sqlx::query_as("SELECT version,action,reason FROM operational_asset_events WHERE asset_id='asset-unicode-asset-create' ORDER BY version").fetch_all(pool.raw()).await.unwrap();assert_eq!(events.len(),2);assert_eq!(events[0].0,1);assert_eq!(events[0].1,"register");assert_eq!(events[1],(2,"place".into(),reason));
    }).await;
}

#[tokio::test]
async fn asset_request_ids_obey_selected_scope_and_foreign_sources_do_not_write() {
    // The actor and cost port are fixture facts; this exercises persisted
    // Assets scope and owner source checks, not composed IAM or Money.
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        for (key,branch,department) in [
            ("scope-long", "x".repeat(201), "y".repeat(201)),
            ("scope-control", "\u{0001}branch".to_owned(), "".to_owned()),
        ] {
            let input=json!({"branchId":branch,"departmentId":department,"code":key,"name":"Scoped Asset","category":"equipment","currency":"THB","costMinor":0,"acquiredDate":"2026-01-01"});
            command(&pool,|c|Box::pin(async move {
                let a=actor("user-1","company",Some("branch"),Some("department"));
                let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();
                create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,key,500).await?;Ok(())
            })).await.unwrap();
            let scope:(String,Option<String>)=sqlx::query_as("SELECT branch_id,department_id FROM operational_assets WHERE id=$1").bind(format!("asset-{key}")).fetch_one(pool.raw()).await.unwrap();
            assert_eq!(scope,("branch".into(),Some("department".into())));
        }
        for (key,finance) in [("foreign-finance-source","finance-foreign".to_owned()),("long-finance-source","f".repeat(201)),("control-finance-source","\u{0001}finance".to_owned())] {
            let input=json!({"branchId":"branch","code":key,"name":"Invalid Finance","category":"equipment","currency":"THB","costMinor":1000,"acquiredDate":"2026-01-01","financeAssetId":finance,"financeAssetVersion":1});
            let result=command(&pool,|c|Box::pin(async move {
                let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();
                create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,key,500).await?;Ok(())
            })).await;
            assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="CONFLICT"),"Finance source {key}: {result:?}");
            let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM operational_assets WHERE id=$1),(SELECT count(*) FROM operational_asset_events WHERE asset_id=$1)").bind(format!("asset-{key}")).fetch_one(pool.raw()).await.unwrap();
            assert_eq!(counts,(0,0));
        }
        insert_asset(&pool,"scope-custodian","company","branch",None,0,None).await;
        for (key,custodian) in [("foreign-custodian","user-3".to_owned()),("long-custodian","u".repeat(201)),("control-custodian","\u{0001}user".to_owned())] {
            let input=json!({"action":"place","expectedVersion":1,"placement":"Desk","custodianId":custodian,"reason":"Assign asset"});
            let result=command(&pool,|c|Box::pin(async move {
                let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();
                change_asset(&a,&mut repo,&mut ledger,"scope-custodian",&input,key,600).await?;Ok(())
            })).await;
            assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="CONFLICT"),"custodian {key}: {result:?}");
            let row:(i64,Option<String>)=sqlx::query_as("SELECT version,custodian_id FROM operational_assets WHERE id='scope-custodian'").fetch_one(pool.raw()).await.unwrap();assert_eq!(row,(1,None));
        }
    }).await;
}

#[tokio::test]
async fn registration_uses_iso_scale_and_replays_before_current_scale_lookup() {
    // This uses the real Finance catalogue and database scale, while the
    // command ledger/cost handoff are test ports, not composed authorization.
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO company_currency_scales(company_id,currency,scale,configured_by,configured_at) VALUES('company','THB',4,'fixture',0),('company','XAG',3,'fixture',0)").execute(pool.raw()).await.unwrap();
        for (key,currency,cost,expected) in [("fixed-usd","USD","1.23",123_i64),("fixed-thb","THB","1.23",123),("custom-xag","XAG","1.234",1234)] {
            let input=json!({"branchId":"branch","code":key,"name":"Scale Asset","category":"equipment","currency":currency,"cost":cost,"acquiredDate":"2026-01-01"});
            command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,key,500).await?;Ok(())})).await.unwrap();
            let actual:i64=sqlx::query_scalar("SELECT cost_minor FROM operational_assets WHERE id=$1").bind(format!("asset-{key}")).fetch_one(pool.raw()).await.unwrap();assert_eq!(actual,expected,"{key}");
        }
        let too_precise=json!({"branchId":"branch","code":"precise","name":"Precise Asset","category":"equipment","currency":"XAG","cost":"1.2345","acquiredDate":"2026-01-01"});
        let result=command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&too_precise,"too-precise",500).await?;Ok(())})).await;
        assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="MONEY_PRECISION_EXCEEDED"));
        for (key,amount) in [("unsafe-decimal","90071992547409.92".to_owned()),("huge-decimal","9".repeat(100))] {
            let input=json!({"branchId":"branch","code":key,"name":"Scale Asset","category":"equipment","currency":"USD","cost":amount,"acquiredDate":"2026-01-01"});
            let result=command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,key,500).await?;Ok(())})).await;
            assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="MONEY_OVERFLOW"));
        }
        sqlx::query("DELETE FROM company_currency_scales WHERE company_id='company' AND currency='XAG'").execute(pool.raw()).await.unwrap();
        let input=json!({"branchId":"branch","code":"custom-xag","name":"Scale Asset","category":"equipment","currency":"XAG","cost":"1.234","acquiredDate":"2026-01-01"});
        struct MatchingLedger { expected:Value }
        impl AssetCommandLedger for MatchingLedger {
            type Error=Error;
            fn replay<'a>(&'a mut self,key:&'a str,operation:&'a str,id:&'a str,input:&'a Value)->AssetFuture<'a,Option<Value>,Error>{
                Box::pin(async move {
                    assert_eq!((key,operation,id),("custom-xag","asset.create","asset-custom-xag"));
                    if input!=&self.expected { return Err(BusinessError::new(FailureKind::Conflict,"IDEMPOTENCY_CONFLICT","Changed request.").into()); }
                    Ok(Some(json!({"id":"asset-custom-xag","version":1,"status":"registered"})))
                })
            }
        }
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(&mut tx,a.clone());let mut ledger=MatchingLedger{expected:input.clone()};let mut cost=RecordingCost::default();
        let replay=create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&input,"custom-xag",600).await.unwrap();assert_eq!(replay.result.version,1);assert!(replay.receipt.is_none());assert_eq!(cost.calls.load(Ordering::SeqCst),0);
        let mut changed=input.clone();changed["cost"]=json!("1.235");let conflict=create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&changed,"custom-xag",600).await;
        assert!(matches!(conflict,Err(PgTransactionError::Business(ref e)) if e.code=="IDEMPOTENCY_CONFLICT"));
        let mut malformed=input.clone();malformed["cost"]=json!("1.2x");let rejected=create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&malformed,"custom-xag",600).await;
        assert!(matches!(rejected,Err(PgTransactionError::Business(ref e)) if e.code=="VALIDATION_FAILED"));drop(repo);tx.rollback().await.unwrap();
        let missing=json!({"branchId":"branch","code":"missing-scale","name":"Scale Asset","category":"equipment","currency":"XAG","cost":"1.234","acquiredDate":"2026-01-01"});
        let result=command(&pool,|c|Box::pin(async move {let a=actor("user-1","company",None,None);let mut repo=PgAssetRegister::new(c,a.clone());let mut ledger=NoCommandLedger::<Error>::default();let mut cost=RecordingCost::default();create_asset(&a,&mut repo,&mut ledger,&mut cost,&FinanceCurrencyCatalogue,&missing,"missing-scale",500).await?;Ok(())})).await;
        assert!(matches!(result,Err(PgTransactionError::Business(ref e)) if e.code=="ASSET_CURRENCY_SCALE_REQUIRED"));
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM operational_assets WHERE id IN ('asset-too-precise','asset-missing-scale','asset-unsafe-decimal','asset-huge-decimal')").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
    }).await;
}

#[tokio::test]
async fn asset_purchase_source_preserves_scope_image_and_writer_fence() {
    use erpx_server_assets::public::asset_purchase_source::*;
    use sqlx::Acquire;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool,"purchase-source","company","branch",Some("department"),1000,None).await;
        insert_asset(&pool,"purchase-linked","company","branch",Some("department"),1000,Some(("finance-1",2))).await;
        let company=CompanyId::new("company").unwrap();
        let foreign=CompanyId::new("company-b").unwrap();
        let mut conn=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TEMP TABLE operational_assets(id text,company_id text);CREATE TEMP TABLE operational_asset_finance_links(id text,asset_id text)").execute(&mut *conn).await.unwrap();
        let mut no_lock=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *no_lock).await.unwrap();assert!(observe_asset_purchase_source(&mut no_lock,&company,None,None,"purchase-source",false).await.is_err());no_lock.rollback().await.unwrap();
        let mut tx=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_asset_purchase_sources(&mut tx).await.unwrap();
        let command=observe_asset_purchase_source(&mut tx,&company,Some("branch"),Some("department"),"purchase-source",false).await.unwrap();
        let file=observe_asset_purchase_source(&mut tx,&company,Some("branch"),Some("department"),"purchase-source",true).await.unwrap();
        assert!(command.exists() && file.exists());assert_eq!(command.finance_asset_id(),None);
        let linked=observe_asset_purchase_source(&mut tx,&company,None,None,"purchase-linked",false).await.unwrap();assert_eq!(linked.finance_asset_id(),Some("finance-1"));
        for (scope,branch,department,id) in [(&foreign,None,None,"purchase-source"),(&company,Some("branch-b"),None,"purchase-source"),(&company,None,Some("department-b"),"purchase-source"),(&company,None,None,"purchase-missing")] {
            let absent=observe_asset_purchase_source(&mut tx,scope,branch,department,id,false).await.unwrap();assert!(!absent.exists());absent.require_current(&mut tx).await.unwrap();
        }
        let legacy=r#"SELECT jsonb_build_object('asset',CASE WHEN $5 THEN to_jsonb(r)-'version'-'updated_at' ELSE to_jsonb(r) END,'finance',to_jsonb(f),'links',(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM public.operational_asset_finance_links link WHERE link.asset_id=r.id))::text FROM public.operational_assets r LEFT JOIN public.finance_assets f ON f.id=r.finance_asset_id AND f.company_id=r.company_id WHERE r.id=$1 AND r.company_id=$2 AND ($3::text IS NULL OR r.branch_id=$3) AND ($4::text IS NULL OR r.department_id=$4)"#;
        let before_command:Option<String>=sqlx::query_scalar(legacy).bind("purchase-source").bind("company").bind(Some("branch")).bind(Some("department")).bind(false).fetch_optional(&mut *tx).await.unwrap();
        let before_file:Option<String>=sqlx::query_scalar(legacy).bind("purchase-source").bind("company").bind(Some("branch")).bind(Some("department")).bind(true).fetch_optional(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT asset_version").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.operational_assets SET version=version+1,updated_at=updated_at+1 WHERE id='purchase-source'").execute(&mut *tx).await.unwrap();
        let after_command:Option<String>=sqlx::query_scalar(legacy).bind("purchase-source").bind("company").bind(Some("branch")).bind(Some("department")).bind(false).fetch_optional(&mut *tx).await.unwrap();
        let after_file:Option<String>=sqlx::query_scalar(legacy).bind("purchase-source").bind("company").bind(Some("branch")).bind(Some("department")).bind(true).fetch_optional(&mut *tx).await.unwrap();
        assert_ne!(before_command,after_command);assert_eq!(before_file,after_file);assert!(command.require_current(&mut tx).await.is_err());file.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT asset_version").execute(&mut *tx).await.unwrap();command.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT asset_link").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.operational_assets SET finance_asset_id='finance-2',finance_asset_version=1 WHERE id='purchase-source'").execute(&mut *tx).await.unwrap();
        let before_link=observe_asset_purchase_source(&mut tx,&company,None,None,"purchase-source",false).await.unwrap();assert_eq!(before_link.finance_asset_id(),Some("finance-2"));
        let legacy_before_link:Option<String>=sqlx::query_scalar(legacy).bind("purchase-source").bind("company").bind(None::<&str>).bind(None::<&str>).bind(false).fetch_optional(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.operational_asset_finance_links(id,asset_id,finance_asset_id,asset_version,finance_asset_version,cost_minor,currency,reason,created_by,created_at) VALUES('purchase-link','purchase-source','finance-2',1,1,1000,'THB','fixture','fixture',0)").execute(&mut *tx).await.unwrap();
        let legacy_after_link:Option<String>=sqlx::query_scalar(legacy).bind("purchase-source").bind("company").bind(None::<&str>).bind(None::<&str>).bind(false).fetch_optional(&mut *tx).await.unwrap();
        assert_ne!(legacy_before_link,legacy_after_link);assert!(before_link.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT asset_link").execute(&mut *tx).await.unwrap();command.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
        sqlx::raw_sql("DROP TABLE pg_temp.operational_assets;DROP TABLE pg_temp.operational_asset_finance_links").execute(&mut *conn).await.unwrap();
        let mut other=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_asset_purchase_sources(&mut other).await.unwrap();assert!(command.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("UPDATE public.operational_assets SET name='changed' WHERE id='purchase-source'").execute(&mut *writer).await.unwrap();let mut blocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();let conflict=lock_asset_purchase_sources(&mut blocked).await;assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn retained_finance_cost_survives_asset_write_and_deferred_checks() {
    use erpx_server_finance::public::asset_acquisition_money::{RetainedAssetAcquisitionMoney,lock_asset_acquisition_money_sources,prepare_asset_acquisition_money};
    use erpx_server_shared::public::{Clock,TimestampMillis};
    struct FixedClock;
    impl Clock for FixedClock {fn now(&self)->TimestampMillis {TimestampMillis::new(1_800_000_000_000)}}
    struct FinanceEffect(RetainedAssetAcquisitionMoney);
    impl AcquisitionCostEffect for FinanceEffect {
        fn require_current<'a>(&'a self,c:&'a mut sqlx::PgConnection)->AssetFuture<'a,(),Error> {
            Box::pin(async move {self.0.require_current(c).await})
        }
    }
    struct FinanceProvider;
    impl RetainedAcquisitionCostProvider for FinanceProvider {
        fn prepare_retained<'a>(&'a self,c:&'a mut sqlx::PgConnection,id:&'a str,asset:&'a NewAsset)->AssetFuture<'a,Box<dyn AcquisitionCostEffect>,Error> {
            Box::pin(async move {
                let company=CompanyId::new("company").unwrap();
                let retained=prepare_asset_acquisition_money(c,&company,id,&asset.acquired_date,&asset.currency,asset.cost_minor,&FixedClock).await?.persist(c).await?;
                Ok(Box::new(FinanceEffect(retained)) as Box<dyn AcquisitionCostEffect>)
            })
        }
    }
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) VALUES('retained-cost-rate','company','THB','2026-01-01',2,'fixture',1)").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_asset_acquisition_money_sources(&mut tx).await.unwrap();
        let actor=actor("user-1","company",Some("branch"),Some("department"));
        let raw=json!({"branchId":"branch","departmentId":"department","code":"RETAIN-COST","name":"Retained cost","category":"equipment","currency":"THB","costMinor":101,"acquiredDate":"2026-01-15"});
        let mut ledger=NoCommandLedger::<Error>::default();
        let mut register=PgAssetRegister::new(&mut tx,actor.clone()).with_retained_cost_provider(Arc::new(FinanceProvider));
        let result=create_asset_in_unit(&actor,&mut register,&mut ledger,&FinanceCurrencyCatalogue,&raw,"retained-cost",1234).await.unwrap();
        let effect=register.take_acquisition_cost_effect().expect("new create must transfer the actual Finance witness");
        assert!(register.take_acquisition_cost_effect().is_none());drop(register);
        assert_eq!(result.result.id,"asset-retained-cost");
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await.unwrap();
        effect.require_current(&mut tx).await.unwrap();
        let stored:(i64,String,Option<String>)=sqlx::query_as("SELECT a.cost_minor,m.usd_units::text,m.fx_rate_id FROM public.operational_assets a JOIN public.money_transaction_snapshots m ON m.company_id=a.company_id AND m.source_type='operational_asset' AND m.source_id=a.id AND m.amount_kind='acquisition_cost' WHERE a.id='asset-retained-cost'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(stored,(101,"51".into(),Some("retained-cost-rate".into())));
        sqlx::query("SAVEPOINT changed_quote").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) VALUES('retained-cost-newer','company','THB','2026-01-02',4,'fixture',2)").execute(&mut *tx).await.unwrap();
        assert!(effect.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT changed_quote").execute(&mut *tx).await.unwrap();effect.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let absent:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM public.operational_assets WHERE id='asset-retained-cost'),(SELECT count(*) FROM public.money_transaction_snapshots WHERE source_type='operational_asset' AND source_id='asset-retained-cost')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(absent,(0,0));
    }).await;
}

#[tokio::test]
async fn typed_acquisition_cost_preserves_real_lock_conflict_without_legacy_fallback() {
    use erpx_server_finance::public::asset_acquisition_money::{RetainedAssetAcquisitionMoney,lock_asset_acquisition_money_sources,prepare_asset_acquisition_money};
    use erpx_server_shared::public::{Clock,TimestampMillis};
    struct FixedClock;
    impl Clock for FixedClock {fn now(&self)->TimestampMillis {TimestampMillis::new(1_800_000_000_000)}}
    struct FinanceEffect(RetainedAssetAcquisitionMoney);
    impl AcquisitionCostEffect for FinanceEffect {
        fn require_current<'a>(&'a self,c:&'a mut sqlx::PgConnection)->AssetFuture<'a,(),Error> {Box::pin(async move {self.0.require_current(c).await})}
    }
    struct FinanceProvider;
    impl RetainedAcquisitionCostProvider for FinanceProvider {
        fn prepare_retained<'a>(&'a self,c:&'a mut sqlx::PgConnection,id:&'a str,asset:&'a NewAsset)->AssetFuture<'a,Box<dyn AcquisitionCostEffect>,Error> {
            Box::pin(async move {
                // Deliberate contention probe for typed error propagation;
                // production must acquire this lock before its first snapshot.
                lock_asset_acquisition_money_sources(c).await?;
                let company=CompanyId::new("company").unwrap();
                let retained=prepare_asset_acquisition_money(c,&company,id,&asset.acquired_date,&asset.currency,asset.cost_minor,&FixedClock).await?.persist(c).await?;
                Ok(Box::new(FinanceEffect(retained)) as Box<dyn AcquisitionCostEffect>)
            })
        }
    }
    struct LegacySentinel(Arc<AtomicUsize>);
    impl AcquisitionCostProvider for LegacySentinel {
        fn prepare<'a>(&'a self,_c:&'a mut sqlx::PgConnection,_id:&'a str,_asset:&'a NewAsset)->AssetFuture<'a,(),BusinessError> {
            self.0.fetch_add(1,Ordering::SeqCst);
            Box::pin(async {Err(BusinessError::invalid("UNEXPECTED_LEGACY_FALLBACK","Legacy fallback was invoked."))})
        }
    }
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE public.company_fx_rates IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let actor=actor("user-1","company",Some("branch"),Some("department"));
        let input=json!({"branchId":"branch","code":"FX-CONTENDED","name":"Contended asset","category":"equipment","currency":"THB","costMinor":101,"acquiredDate":"2026-01-15"});
        let asset=NewAsset::parse(&input,101).unwrap();
        let calls=Arc::new(AtomicUsize::new(0));
        let mut register=PgAssetRegister::new(&mut tx,actor).with_cost_provider(Arc::new(LegacySentinel(calls.clone()))).with_retained_cost_provider(Arc::new(FinanceProvider));
        let result=AcquisitionCostPort::prepare(&mut register,"asset-contended",&asset).await;
        assert!(matches!(result,Err(PgTransactionError::Persistence(ref error)) if error.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && error.sql_state()==Some("55P03") && error.retryable()));
        assert!(register.take_acquisition_cost_effect().is_none());assert_eq!(calls.load(Ordering::SeqCst),0);
        drop(register);tx.rollback().await.unwrap();writer.rollback().await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM public.operational_assets WHERE id='asset-contended')+(SELECT count(*) FROM public.money_transaction_snapshots WHERE source_type='operational_asset' AND source_id='asset-contended')").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
    }).await;
}

#[tokio::test]
async fn asset_maintenance_join_matches_actual_ts_asset_image() {
    use erpx_server_assets::public::asset_purchase_source::{inspect_asset_maintenance_join,lock_asset_purchase_sources};
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        insert_asset(&pool,"asset-join-source","company","branch",Some("department"),1000,None).await;
        sqlx::query("INSERT INTO public.maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at) VALUES('asset-join-work','company','branch','department','asset-join-source','Inspect asset','normal','open','user-1','user-1',1,1)").execute(pool.raw()).await.unwrap();
        let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../.."))
            .args(["--conditions=react-server","-e","import {maintenancePurchaseSourceQuery as q} from './packages/server-maintenance/src/adapters/purchase-sources.ts'; let n=0; process.stdout.write(q('work',true).replace(/\\?/g,()=>'$'+(++n)))"])
            .output().unwrap();
        assert!(oracle.status.success(),"actual Maintenance TS source unavailable");
        let legacy=String::from_utf8(oracle.stdout).unwrap();
        let rewritten=legacy.replace("FROM maintenance_work_orders r JOIN operational_assets a", "FROM maintenance_asset_purchase_source r JOIN asset_maintenance_source a");
        assert_ne!(legacy,rewritten,"actual Maintenance source shape changed");
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_asset_maintenance_join(&mut unlocked).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT asset_source_locks").execute(&mut *tx).await.unwrap();
        lock_asset_purchase_sources(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE public.maintenance_work_orders,public.maintenance_preventive_plans,public.maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        let source=inspect_asset_maintenance_join(&mut tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        let projected=format!("WITH maintenance_asset_purchase_source AS (SELECT r.* FROM public.maintenance_work_orders r WHERE r.id='asset-join-work'), {} {}",source.cte_sql(),rewritten);
        async fn image(c:&mut sqlx::PgConnection,sql:&str,branch:&str,department:Option<&str>)->Option<Value>{
            let row=sqlx::query(sqlx::AssertSqlSafe(sql)).bind("asset-join-work").bind("company").bind(Some(branch)).bind(Some(branch)).bind(department).bind(department).fetch_optional(c).await.unwrap()?;
            Some(serde_json::from_str(sqlx::Row::try_get::<&str,_>(&row,"image").unwrap()).unwrap())
        }
        let original=image(&mut tx,&legacy,"branch",Some("department")).await;
        let joined=image(&mut tx,&projected,"branch",Some("department")).await;
        assert!(original.is_some());assert_eq!(joined,original);
        assert!(image(&mut tx,&projected,"branch-b",Some("department")).await.is_none());
        assert!(image(&mut tx,&projected,"branch",Some("department-b")).await.is_none());
        sqlx::query("SAVEPOINT asset_version_change").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE public.operational_assets SET version=version+1,updated_at=updated_at+1 WHERE id='asset-join-source'").execute(&mut *tx).await.unwrap();
        let changed=image(&mut tx,&projected,"branch",Some("department")).await;
        assert_ne!(changed,joined,"Maintenance file image retains the full Asset version");
        assert_eq!(changed,image(&mut tx,&legacy,"branch",Some("department")).await);
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT asset_version_change").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT asset_source_locks").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn selected_asset_purchase_ctes_match_actual_ts_source_query() {
    use erpx_server_assets::public::asset_purchase_source::*;
    use sqlx::{Postgres, QueryBuilder};
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        insert_asset(&pool,"selected-asset","company","branch",Some("department"),1000,None).await;
        insert_asset(&pool,"other-asset","company","branch",Some("department"),1000,None).await;
        // Capture only the query from the actual TS owner function; execution
        // and all data below are real PostgreSQL in the original fenced unit.
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const db={prepare(sql){return {bind(){return {async first(){return sql}}}}}};const actor={companyId:'company',branchId:'branch',departmentId:'department'};let sqls=[];for(const file of [false,true]){let n=0;const sql=await m.assetOwnerSourceImage(db,actor,{kind:'asset',id:'selected-asset'},file);sqls.push(sql.replace(/\?/g,()=>'$'+(++n)))}process.stdout.write(JSON.stringify(sqls));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/server-assets/src/adapters/purchases.ts");
        let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).args(["--conditions=react-server","-e",script]).arg(path).output().unwrap();
        assert!(out.status.success(),"actual Assets TS query unavailable");
        let legacy:Vec<String>=serde_json::from_slice(&out.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        lock_asset_purchase_sources(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        for (id,branch,department) in [("selected-asset",Some("branch"),Some("department")),("selected-asset",Some("wrong"),None),("missing",None,None)] {
            for file in [false,true] {
                let source=observe_asset_purchase_source(&mut tx,&company,branch,department,id,file).await.unwrap();
                assert_eq!(source.source_id(),id);
                assert_eq!(source.file_mode(),file);
                source.require_current(&mut tx).await.unwrap();
                let mut q=QueryBuilder::<Postgres>::new("WITH ");source.append_purchase_ctes(&mut q);
                q.push(" SELECT jsonb_build_object('asset',");
                if file {q.push("to_jsonb(r)-'version'-'updated_at'");}else{q.push("to_jsonb(r)");}
                q.push(",'finance',to_jsonb(f),'links',(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM asset_purchase_links link WHERE link.asset_id=r.id))::text FROM asset_purchase_selected r LEFT JOIN public.finance_assets f ON f.id=r.finance_asset_id AND f.company_id=r.company_id");
                let joined:Option<String>=q.build_query_scalar().fetch_optional(&mut *tx).await.unwrap();
                let original:Option<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(legacy[usize::from(file)].as_str())).bind(id).bind("company").bind(branch).bind(branch).bind(department).bind(department).fetch_optional(&mut *tx).await.unwrap();
                assert_eq!(joined,original,"source scope and file={file}");
                source.require_current(&mut tx).await.unwrap();
            }
        }
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn asset_files_descriptor_matches_actual_typescript_owner_sql() {
    use erpx_server_assets::public::asset_purchase_source::{lock_asset_purchase_sources,observe_asset_attachment_descriptor};
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        let long_id=format!("descriptor-{}","x".repeat(220));
        for (id,company,department,code) in [(long_id.as_str(),"company",Some("department"),"DESC-LONG"),("descriptor-null","company",None,"DESC-NULL"),("descriptor-foreign","foreign",None,"DESC-FOREIGN")] {
            sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,created_by,created_at,updated_at) VALUES($1,$2,'branch',$3,$4,'Synthetic','equipment','THB','fixture',0,0)").bind(id).bind(company).bind(department).bind(code).execute(pool.raw()).await.unwrap();
        }
        // Call actual TS owner adapter to get its fixed SQL and binding order.
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const db={prepare(sql){return {bind(...args){return {async first(){return {sql,args}}}}}}};const r=await m.OperationalAttachmentAssetsFacts(db).readModuleScoped({targetId:'sample',companyId:'company',targetType:'asset'});let n=0;process.stdout.write(JSON.stringify({sql:r.sql.replace(/\?/g,()=>'$'+(++n)),args:r.args}));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/attachments-operational-facts.ts");let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).arg("--conditions=react-server").arg("-e").arg(script).arg(path).output().unwrap();assert!(output.status.success(),"actual Assets operational descriptor oracle unavailable");let oracle:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(oracle["args"],json!(["sample","company"]));let sql=format!("SELECT row_to_json(d) FROM ({}) d",oracle["sql"].as_str().unwrap());
        let company=CompanyId::new("company").unwrap();let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_asset_purchase_sources(&mut tx).await.unwrap();
        for id in [long_id.as_str(),"descriptor-null","descriptor-foreign","descriptor-missing"] {
            let source=observe_asset_attachment_descriptor(&mut tx,&company,id).await.unwrap();let expected:Option<Value>=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(id).bind("company").fetch_optional(&mut *tx).await.unwrap();
            assert_eq!(source.data().map(|value|serde_json::to_value(value).unwrap()),expected,"{id}");source.require_current(&mut tx).await.unwrap();
            if id=="descriptor-null"{assert_eq!(source.data().unwrap().department_id,None);}
            if id=="descriptor-foreign"||id=="descriptor-missing"{assert!(source.data().is_none());}
        }
        let source=observe_asset_attachment_descriptor(&mut tx,&company,&long_id).await.unwrap();
        sqlx::query("SAVEPOINT descriptor_drift").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE operational_assets SET version=version+1 WHERE id=$1").bind(&long_id).execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT descriptor_drift").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}
