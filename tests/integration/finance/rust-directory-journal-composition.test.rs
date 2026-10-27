//! Candidate canonical target: tests/integration/finance/rust-directory-journal-composition.test.rs.
//! One actual Account/SSO fixture module; the LDAPS helper has no Account module.
#[allow(dead_code, reason = "shared real Finance/Auth command scenarios")]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
#[allow(dead_code, reason = "shared real bounded loopback protocol scenarios")]
#[path = "../../support/rust-directory-ldaps.rs"]
mod ldap;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::{
    SystemAuthenticationSecrets, directory::*, effective_authority::HoldingWorkAuthorizer,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, RecordId};
use erpx_service::{finance_journal_detail::*, finance_journal_review::*};
use facts::{Prepared, TestClock, account};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
struct DirectoryFinance {
    work: Arc<HoldingWorkAuthorizer>,
    server: ldap::Server,
    _profile: ldap::Fixture,
}
async fn directory(p: &Prepared, user: &str, pause: bool) -> DirectoryFinance {
    let server = ldap::Server::start(ldap::Scenario::Complete, pause, None).await;
    let profile = ldap::Fixture::new(
        server.port,
        false,
        Some(&ldap::material().ca),
        "ldap.fixture.test",
    )
    .await;
    let now = p.system.scenario.clock.now().get();
    sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id)VALUES($1,'h001','company')")
        .bind(ldap::CONNECTION).execute(p.system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,tested_version,created_by,created_at)VALUES($1,'company','Synthetic actual Finance Directory','univention',$2,'active',1,1,'fixture',0)")
        .bind(ldap::CONNECTION).bind(serde_json::to_value(&profile.descriptor.config).unwrap())
        .execute(p.system.holdings[0].raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,email,enabled,groups_json,observed_at,user_id)VALUES('finance-directory-identity','company',$1,'alice-id','alice','Before refresh','separate-contact@example.test',true,'[\"group-id\"]',$2,$3)")
        .bind(ldap::CONNECTION).bind(now-300000).bind(user)
        .execute(p.system.holdings[0].raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,connection_id,identity_id)VALUES('company',$1,'reviewed-finance-directory-principal','active','directory',$2,'finance-directory-identity')")
        .bind(user).bind(ldap::CONNECTION).execute(p.system.holdings[0].raw()).await.unwrap();
    let holdings = BTreeMap::from([
        (
            HoldingId::new("h001").unwrap(),
            DirectoryHoldingDatabase {
                audience: account::ACME.into(),
                write_binding: "HOLDING_H001_DB".into(),
                pool: p.system.holdings[0].clone(),
            },
        ),
        (
            HoldingId::new("h002").unwrap(),
            DirectoryHoldingDatabase {
                audience: account::NORTHSTAR.into(),
                write_binding: "HOLDING_H002_DB".into(),
                pool: p.system.holdings[1].clone(),
            },
        ),
    ]);
    let directory = Arc::new(
        PostgresDirectory::new(
            DirectoryDatabaseBindings {
                control: p.system.control.clone(),
                holdings,
                registry: account::fixture::account::holding_registry(),
                environment: "development".into(),
            },
            profile.profiles.clone(),
            p.system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
            account::invalidator(&p.system.holdings, p.system.scenario.clock.clone()),
        )
        .unwrap(),
    );
    let runtime = DirectoryRefreshRuntime::new(directory);
    // This must be the exact concrete type accepted by the ACTUAL service.
    let work: HoldingWorkAuthorizer = runtime
        .holding(
            HoldingId::new("h001").unwrap(),
            account::fixture::account::policy(),
        )
        .unwrap();
    DirectoryFinance {
        work: Arc::new(work),
        server,
        _profile: profile,
    }
}
fn review(p: &Prepared, d: &DirectoryFinance) -> FinanceJournalReviewAdministration<TestClock> {
    FinanceJournalReviewAdministration::new(
        d.work.clone(),
        p.configuration.clone(),
        p.system.scenario.clock.clone(),
    )
}
async fn counts(pool: &DatabasePool, key: &str) -> (i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1)")
        .bind(key).fetch_one(pool.raw()).await.unwrap()
}
async fn journal(pool: &DatabasePool, id: &str) -> (String, i64) {
    sqlx::query_as("SELECT status,version FROM finance_journals WHERE id=$1")
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}
async fn sessions(pool: &DatabasePool) -> i64 {
    sqlx::query_scalar("SELECT count(*)FROM session")
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn actual_finance_workspace_uses_configured_normal_authorizer_for_stale_directory_and_same_rendered_context()
 {
    account::with_system(|system| async move {
        let p = facts::setup(system).await;
        p.create("directory-read-origin", false).await.unwrap();
        let d = directory(&p, "fixture-a", false).await;
        let before = sessions(&p.system.control).await;
        let service = FinanceJournalDetailService::new(
            d.work.clone(),
            FinanceJournalReadConfiguration::new(
                FinanceJournalEnvironment::Development,
                HoldingId::new("h001").unwrap(),
                CompanyId::new("company").unwrap(),
                Arc::new(current_package_catalogue().unwrap()),
            )
            .unwrap(),
            p.system.scenario.clock.clone(),
        );
        let selection = p.maker_selection.clone();
        let result = service
            .read_workspace(
                "acme.example.test",
                p.maker.clone(),
                move || Ok(selection),
                RecordId::new("journal-directory-read-origin").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.context.user_id, "fixture-a");
        assert_eq!(result.context.assignment_id, "finance-fixture-a");
        assert_eq!(result.context.assignment_version.get(), 3);
        assert_eq!(result.context.revision.get(), 7);
        assert_eq!(
            result.data.journal["id"],
            erpx_server_finance::public::journal_detail::FinanceLedgerValue::Text(
                "journal-directory-read-origin".into()
            )
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT observed_at FROM iam_identities WHERE id='finance-directory-identity'"
            )
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap(),
            p.system.scenario.clock.now().get()
        );
        assert_eq!(sessions(&p.system.control).await, before);
        {
            let stats = d.server.stats.lock().unwrap();
            assert_eq!(stats.service_binds, 1);
            assert_eq!(stats.user_binds, 0);
        }
        d.server.finish().await;
    })
    .await;
}

#[tokio::test]
async fn actual_finance_post_after_stale_directory_refresh_retains_original_receipt_audit_and_independence()
 {
    account::with_system(|system|async move{
        let p=facts::setup(system).await;p.create("directory-post-origin",false).await.unwrap();
        let d=directory(&p,"fixture-b",false).await;
        let before=sessions(&p.system.control).await;
        let output=review(&p,&d).review("acme.example.test",p.reviewer.clone(),
            p.reviewer_selection.clone(),facts::command("journal-directory-post-origin","directory-post-command",facts::post())).await.unwrap();
        assert!(matches!(output,erpx_server_finance::public::journal_review::JournalReviewResult::Posted{version:2,..}));
        assert_eq!(journal(&p.system.holdings[0],"journal-directory-post-origin").await,("posted".into(),2));
        assert_eq!(counts(&p.system.holdings[0],"directory-post-command").await,(1,1,1));
        let (actor,metadata):(String,serde_json::Value)=sqlx::query_as("SELECT actor_id,authority_json::jsonb FROM command_receipts WHERE command_id='directory-post-command'")
            .fetch_one(p.system.holdings[0].raw()).await.unwrap();
        assert_eq!(actor,"fixture-b");
        assert_eq!(metadata["assignmentId"],"finance-fixture-b");
        assert_eq!(metadata["assignmentVersion"],3);
        assert_eq!(metadata["iam"]["principalId"],"reviewed-finance-directory-principal");
        assert_eq!(sessions(&p.system.control).await,before);
        {let stats=d.server.stats.lock().unwrap();assert_eq!(stats.service_binds,1);assert_eq!(stats.user_binds,0);}
        d.server.finish().await;
    }).await;
}

#[tokio::test]
async fn finance_authority_withdrawal_commits_during_actual_directory_io_and_blocks_all_posting_effects()
 {
    account::with_system(|system|async move{
        let p=facts::setup(system).await;p.create("directory-denied-origin",false).await.unwrap();
        let d=directory(&p,"fixture-b",true).await;
        let administration=review(&p,&d);let proof=p.reviewer.clone();let selection=p.reviewer_selection.clone();
        let operation=tokio::spawn(async move{
            administration.review("acme.example.test",proof,selection,
                facts::command("journal-directory-denied-origin","directory-denied-command",facts::post())).await
        });
        tokio::time::timeout(Duration::from_secs(3),d.server.started.notified()).await
            .expect("actual LDAPS reached the owned I/O barrier");
        // The actual business owner has no retained lease before observation.
        // This is an observable NOWAIT assertion rather than a timing guess.
        let mut unlocked=p.system.holdings[0].raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE finance_journals,command_receipts IN ACCESS EXCLUSIVE MODE NOWAIT")
            .execute(&mut*unlocked).await.unwrap();unlocked.rollback().await.unwrap();
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_finance_review',2,'[\"finance.read\",\"module.finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',1);UPDATE iam_roles SET published_revision=2 WHERE company_id='*'AND id='custom_finance_review'")
            .execute(p.system.holdings[0].raw()).await.unwrap();
        d.server.resume.notify_one();
        assert!(matches!(operation.await.unwrap(),Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error))if error.code=="PERMISSION_REQUIRED"));
        assert_eq!(journal(&p.system.holdings[0],"journal-directory-denied-origin").await,("draft".into(),1));
        assert_eq!(counts(&p.system.holdings[0],"directory-denied-command").await,(0,0,0));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM pg_stat_activity WHERE datname=current_database() AND state='idle in transaction'")
            .fetch_one(p.system.holdings[0].raw()).await.unwrap(),0);
        d.server.finish().await;
    }).await;
}
