//! Actual Account/SSO, IAM and owned LDAPS fixture; no manufactured Actor.
#[allow(dead_code, reason = "shared current authority scenario")]
#[path = "rust-account-queued-authority.rs"]
pub mod account_queue;
#[allow(dead_code, reason = "shared bounded loopback protocol scenario")]
#[path = "rust-directory-ldaps.rs"]
pub mod ldap;
pub use account_queue::account;
use erpx_server_identity_access::public::{directory::*, *};
use erpx_server_shared::public::{Clock, HoldingId};
use std::{collections::BTreeMap, sync::Arc};
pub struct Prepared {
    pub authority: account_queue::PreparedAccountQueuedAuthority,
    pub server: ldap::Server,
    pub profile: ldap::Fixture,
    pub directory: Arc<PostgresDirectory>,
    pub runtime: DirectoryRefreshRuntime,
    pub work: Arc<erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer>,
}
pub async fn prepare(
    system: account::System,
    scenario: ldap::Scenario,
    pause: bool,
    stale: bool,
) -> Prepared {
    let authority = account_queue::prepare(system).await;
    let server = ldap::Server::start(scenario, pause, None).await;
    let profile = ldap::Fixture::new(
        server.port,
        false,
        Some(&ldap::material().ca),
        "ldap.fixture.test",
    )
    .await;
    let system = &authority.system;
    let now = system.scenario.clock.now().get();
    sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id) VALUES($1,'h001','company')").bind(ldap::CONNECTION).execute(system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,tested_version,created_by,created_at) VALUES($1,'company','Synthetic refresh Directory','univention',$2,'active',1,1,'fixture',0)").bind(ldap::CONNECTION).bind(serde_json::to_value(&profile.descriptor.config).unwrap()).execute(system.holdings[0].raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,email,enabled,groups_json,observed_at,user_id) VALUES('directory-work-identity','company',$1,'alice-id','alice','Before refresh','separate-contact@example.test',true,'[\"group-id\"]',$2,'fixture-a')").bind(ldap::CONNECTION).bind(if stale{now-300000}else{now}).execute(system.holdings[0].raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,connection_id,identity_id) VALUES('company','fixture-a','reviewed-principal','active','directory',$1,'directory-work-identity')").bind(ldap::CONNECTION).execute(system.holdings[0].raw()).await.unwrap();
    sqlx::raw_sql("CREATE TABLE synthetic_directory_work_effects(id text PRIMARY KEY)")
        .execute(system.holdings[0].raw())
        .await
        .unwrap();
    let holdings = BTreeMap::from([
        (
            HoldingId::new("h001").unwrap(),
            DirectoryHoldingDatabase {
                audience: account::ACME.into(),
                write_binding: "HOLDING_H001_DB".into(),
                pool: system.holdings[0].clone(),
            },
        ),
        (
            HoldingId::new("h002").unwrap(),
            DirectoryHoldingDatabase {
                audience: account::NORTHSTAR.into(),
                write_binding: "HOLDING_H002_DB".into(),
                pool: system.holdings[1].clone(),
            },
        ),
    ]);
    let directory = Arc::new(
        PostgresDirectory::new(
            DirectoryDatabaseBindings {
                control: system.control.clone(),
                holdings,
                registry: account::fixture::account::holding_registry(),
                environment: "development".into(),
            },
            profile.profiles.clone(),
            system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
            account::invalidator(&system.holdings, system.scenario.clock.clone()),
        )
        .unwrap(),
    );
    let runtime = DirectoryRefreshRuntime::new(directory.clone());
    let work = Arc::new(
        runtime
            .holding(
                HoldingId::new("h001").unwrap(),
                account::fixture::account::policy(),
            )
            .unwrap(),
    );
    Prepared {
        authority,
        server,
        profile,
        directory,
        runtime,
        work,
    }
}
pub async fn execute(
    prepared: &Prepared,
) -> Result<
    String,
    erpx_server_runtime::public::postgres::PgTransactionError<
        erpx_server_shared::public::BusinessError,
    >,
> {
    prepared.work.execute("acme.example.test",prepared.authority.proof.clone(),prepared.authority.selection.clone(),prepared.authority.access.clone(),|c,context|Box::pin(async move{sqlx::query("INSERT INTO synthetic_directory_work_effects(id) VALUES('actual-authorized-effect')").execute(c).await?;Ok(context.user_id().as_str().to_owned())})).await
}
pub async fn effects(prepared: &Prepared) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM synthetic_directory_work_effects")
        .fetch_one(prepared.authority.system.holdings[0].raw())
        .await
        .unwrap()
}
pub async fn wait_provider(prepared: &Prepared) {
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        prepared.server.started.notified(),
    )
    .await
    .expect("actual provider reached the owned I/O barrier");
}

/// The synthetic durable effect owns a real utility fence, just as participating
/// business owners do; the IAM/session coordinator remains the authority issuer.
#[derive(Clone)]
pub struct EffectSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for EffectSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut sqlx::PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<
        'c,
        (),
        erpx_server_shared::public::BusinessError,
    > {
        Box::pin(async move {
            sqlx::query("LOCK TABLE synthetic_directory_work_effects IN SHARE ROW EXCLUSIVE MODE")
                .execute(c)
                .await?;
            Ok(())
        })
    }
}
