#[path = "rust-postgres.rs"]
pub mod database;
#[allow(dead_code)]
#[path = "rust-account-credentials.rs"]
pub mod fixture;
use erpx_server_identity_access::public::{account_revocation::*, account_sso::*, *};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::HoldingId;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, atomic::AtomicI64},
};
pub const ACME: &str = "holding:development:h001:acme.example.test";
pub const NORTHSTAR: &str = "holding:development:h002:northstar.example.test";
pub struct System {
    pub control: DatabasePool,
    pub holdings: [DatabasePool; 2],
    pub scenario: fixture::Scenario,
}
pub fn invalidator(
    holdings: &[DatabasePool; 2],
    clock: Arc<fixture::MutableClock>,
) -> Arc<dyn AccountWorkContextInvalidation> {
    Arc::new(
        PostgresAccountWorkContextInvalidation::new(
            BTreeMap::from([
                (
                    HoldingId::new("h001").unwrap(),
                    AccountHoldingWriter {
                        write_binding: "HOLDING_H001_DB".into(),
                        pool: holdings[0].clone(),
                    },
                ),
                (
                    HoldingId::new("h002").unwrap(),
                    AccountHoldingWriter {
                        write_binding: "HOLDING_H002_DB".into(),
                        pool: holdings[1].clone(),
                    },
                ),
            ]),
            clock,
        )
        .unwrap(),
    )
}
pub async fn with_system<F, Fut>(test: F)
where
    F: FnOnce(System) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    database::with_database(3,move |control|async move{
  database::with_database(3,move |a|async move{
   database::with_database(3,move |b|async move{
    let holdings=[a,b];
    for pool in &holdings {let files=load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),MigrationTrack::HoldingWrite).await.unwrap();migrate_database(pool,MigrationTrack::HoldingWrite,&files).await.unwrap();}
    let clock=Arc::new(fixture::MutableClock(AtomicI64::new(fixture::account::NOW)));
    let scenario=fixture::setup_with_clock_and_invalidation(&control,Arc::new(UnavailableInvitationCredentialAuthority),clock.clone(),invalidator(&holdings,clock)).await;
    sqlx::query("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding) VALUES('h002','northstar','Synthetic Northstar','HOLDING_H002_DB','HOLDING_H002_READ_DB','HOLDING_H002_FILES')").execute(control.raw()).await.unwrap();
    test(System{control,holdings,scenario}).await;
   }).await;
  }).await;
 }).await;
}
fn remember(previous: Option<PresentedSecrets>, signed: SignedIn) -> PresentedSecrets {
    let mut proof = previous.unwrap_or_else(fixture::account::empty);
    if let Some(browser) = signed.browser_secret {
        proof.browser = Some(browser);
    }
    proof.active = Some(signed.session_secret.clone());
    proof.saved.push((signed.handle, signed.session_secret));
    proof
}
pub async fn login(
    system: &System,
    email: &str,
    previous: Option<PresentedSecrets>,
) -> PresentedSecrets {
    let empty = fixture::account::empty();
    let proof = previous.as_ref().unwrap_or(&empty);
    let revision = if previous.is_some() {
        Some(system.scenario.auth.devices(proof).await.unwrap().revision)
    } else {
        None
    };
    let signed = system
        .scenario
        .auth
        .sign_in(email, fixture::PASSWORD.into(), proof, revision)
        .await
        .unwrap();
    remember(previous, signed)
}
pub async fn child(system: &System, proof: &PresentedSecrets, audience: &str) -> IssuedSso {
    let policy = SsoPolicy::new(fixture::account::policy()).unwrap();
    let service = AccountSso::new(
        policy.clone(),
        Arc::new(PostgresAccountSso::from_control_database(
            system.control.clone(),
            policy,
            system.scenario.clock.clone(),
        )),
        Arc::new(SystemSsoCryptography),
        Arc::new(SystemAuthenticationSecrets),
        system.scenario.clock.clone(),
    );
    let start = service.start(audience, None).await.unwrap();
    let grant = service
        .authorize(start.transaction_id, &start.state, start.challenge, proof)
        .await
        .unwrap()
        .unwrap();
    service
        .consume(audience, &grant.code, &start.state, start.browser_cookie)
        .await
        .unwrap()
}
pub async fn context(pool: &DatabasePool, child: &IssuedSso, user: &str) {
    sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,revision,expires_at,updated_at) VALUES($1,$2,'synthetic-company',1,$3,$4)").bind(&child.result.session_id).bind(user).bind(child.result.expires_at).bind(fixture::account::NOW).execute(pool.raw()).await.unwrap();
}
pub async fn children(system: &System, proof: &PresentedSecrets, user: &str) -> [IssuedSso; 2] {
    let a = child(system, proof, ACME).await;
    let b = child(system, proof, NORTHSTAR).await;
    context(&system.holdings[0], &a, user).await;
    context(&system.holdings[1], &b, user).await;
    [a, b]
}
pub async fn fenced(pool: &DatabasePool, child: &IssuedSso, user: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1 AND user_id=$2) AND NOT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=$1 AND user_id=$2)").bind(&child.result.session_id).bind(user).fetch_one(pool.raw()).await.unwrap()
}
pub async fn assert_fenced(system: &System, children: &[IssuedSso; 2], user: &str) {
    for (pool, child) in system.holdings.iter().zip(children) {
        assert!(fenced(pool, child, user).await);
    }
}
