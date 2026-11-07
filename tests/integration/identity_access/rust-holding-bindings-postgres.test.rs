#[allow(
    dead_code,
    reason = "shared real Account/SSO fixture serves other lifecycle tests"
)]
#[path = "../../support/rust-account-queued-authority.rs"]
mod account_queue;
use account_queue::account;
use erpx_server_identity_access::public::{
    account_holding_bindings::*, account_sessions::verify_session_in_transaction, account_sso::*, *,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgRequestLease, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, HoldingId};
use sqlx::Connection;
use std::{sync::Arc, time::Duration};
fn audience() -> RequestAudience {
    RequestAudience::Holding {
        audience: account::ACME.into(),
    }
}
fn sso(system: &account::System) -> AccountSso {
    let policy = SsoPolicy::new(account::fixture::account::policy()).unwrap();
    AccountSso::new(
        policy.clone(),
        Arc::new(PostgresAccountSso::from_control_database(
            system.control.clone(),
            policy,
            system.scenario.clock.clone(),
            account::fixture::account::holding_registry(),
        )),
        Arc::new(SystemSsoCryptography),
        Arc::new(SystemAuthenticationSecrets),
        system.scenario.clock.clone(),
    )
}
async fn verified(
    pool: DatabasePool,
    proof: DeviceProof,
    now: i64,
) -> Result<bool, PgTransactionError<BusinessError>> {
    PgRequestLease::new(pool)
        .run(move |c| {
            Box::pin(async move {
                Ok(verify_session_in_transaction(
                    c,
                    &proof,
                    &audience(),
                    "development",
                    now,
                    &account::fixture::account::holding_registry(),
                )
                .await?
                .is_some())
            })
        })
        .await
}
#[tokio::test]
async fn live_account_child_and_sso_refuse_every_changed_configured_binding_without_following_a_new_pool()
 {
    account::with_system(|system|async move {
  let central=account::login(&system,"a@example.test",None).await;
  let child=account::child(&system,&central,account::ACME).await;
  let proof=DeviceProof{active:Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session,&child.token)),..Default::default()};
  let now=system.scenario.clock.now().get();
  assert!(verified(system.control.clone(),proof.clone(),now).await.unwrap());
  let registry=account::fixture::account::holding_registry();
  PgRequestLease::new(system.control.clone()).run(move|c|Box::pin(async move {lock_holding_binding_sources(c).await?;registry.verify_startup(c).await})).await.unwrap();
  for (column,changed,original) in [("status","suspended","active"),("slug","moved","acme"),("write_binding","NEW_WRITE","HOLDING_H001_DB"),("read_binding","NEW_READ","HOLDING_H001_READ_DB"),("files_binding","NEW_FILES","HOLDING_H001_FILES")] {
   let sql=match column{"status"=>"UPDATE holding_bindings SET status=$1 WHERE holding_id='h001'","slug"=>"UPDATE holding_bindings SET slug=$1 WHERE holding_id='h001'","write_binding"=>"UPDATE holding_bindings SET write_binding=$1 WHERE holding_id='h001'","read_binding"=>"UPDATE holding_bindings SET read_binding=$1 WHERE holding_id='h001'",_=>"UPDATE holding_bindings SET files_binding=$1 WHERE holding_id='h001'"};
   sqlx::query(sql).bind(changed).execute(system.control.raw()).await.unwrap();
   assert!(matches!(verified(system.control.clone(),proof.clone(),now).await,Err(PgTransactionError::Business(ref e))if e.code=="HOLDING_BINDING_UNAVAILABLE"),"{column}");
   assert!(sso(&system).start(account::ACME,None).await.is_err(),"SSO accepted {column}");
   // A suspended Holding never disables the independently proven central Account.
   assert!(system.scenario.auth.session(&central,RequestAudience::Account).await.unwrap().is_some());
   sqlx::query(sql).bind(original).execute(system.control.raw()).await.unwrap();
  }
  assert!(verified(system.control.clone(),proof,now).await.unwrap());
 }).await;
}
#[tokio::test]
async fn binding_withdrawal_after_grant_prevents_child_issuance_and_does_not_consume_the_one_use_grant()
 {
    account::with_system(|system| async move {
        let central = account::login(&system, "a@example.test", None).await;
        let service = sso(&system);
        let start = service.start(account::ACME, None).await.unwrap();
        let grant = service
            .authorize(
                start.transaction_id,
                &start.state,
                start.challenge,
                &central,
            )
            .await
            .unwrap()
            .unwrap();
        sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'")
            .execute(system.control.raw())
            .await
            .unwrap();
        assert!(
            service
                .consume(
                    account::ACME,
                    &grant.code,
                    &start.state,
                    start.browser_cookie.clone()
                )
                .await
                .is_err()
        );
        let children: i64 =
            sqlx::query_scalar("SELECT count(*)FROM sso_session_bindings WHERE audience=$1")
                .bind(account::ACME)
                .fetch_one(system.control.raw())
                .await
                .unwrap();
        assert_eq!(children, 0);
        sqlx::query("UPDATE holding_bindings SET status='active'WHERE holding_id='h001'")
            .execute(system.control.raw())
            .await
            .unwrap();
        service
            .consume(
                account::ACME,
                &grant.code,
                &start.state,
                start.browser_cookie,
            )
            .await
            .unwrap();
    })
    .await;
}
#[tokio::test]
async fn binding_utility_wait_precedes_first_snapshot_and_observes_committed_suspension() {
    account::with_system(|system|async move {
  let mut writer=system.control.raw().begin().await.unwrap();sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'").execute(&mut *writer).await.unwrap();
  let pool=system.control.clone();let reader=tokio::spawn(async move {PgRequestLease::new(pool).run(move|c|Box::pin(async move {lock_holding_binding_sources(c).await?;account::fixture::account::holding_registry().read_current(c,&audience()).await.map(|_|())})).await});
  let mut waiting=false;for _ in 0..200 {waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='holding_bindings'::regclass AND mode='ShareLock'AND NOT granted)").fetch_one(system.control.raw()).await.unwrap();if waiting{break}tokio::time::sleep(Duration::from_millis(10)).await;}
  assert!(waiting,"no actual Control source lock wait");writer.commit().await.unwrap();
  assert!(matches!(reader.await.unwrap(),Err(PgTransactionError::Business(ref e))if e.code=="HOLDING_BINDING_UNAVAILABLE"));
 }).await;
}
#[tokio::test]
async fn older_serializable_binding_snapshot_preserves_sqlstate_and_detached_or_changed_proofs_deny()
 {
    account::with_system(|system|async move {
  let registry=account::fixture::account::holding_registry();
  let mut old=system.control.raw().acquire().await.unwrap();let mut tx=old.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  sqlx::query("SELECT holding_id FROM holding_bindings WHERE holding_id='h001'").fetch_one(&mut *tx).await.unwrap();
  sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'").execute(system.control.raw()).await.unwrap();lock_holding_binding_sources(&mut tx).await.unwrap();
  let result=registry.read_current(&mut tx,&audience()).await;assert!(matches!(result,Err(PgTransactionError::Persistence(ref e))if e.sql_state()==Some("40001")));tx.rollback().await.unwrap();
  sqlx::query("UPDATE holding_bindings SET status='active'WHERE holding_id='h001'").execute(system.control.raw()).await.unwrap();
  let mut tx=old.begin().await.unwrap();lock_holding_binding_sources(&mut tx).await.unwrap();let proof=registry.read_current(&mut tx,&audience()).await.unwrap();assert_eq!(proof.holding_id(),&HoldingId::new("h001").unwrap());
  sqlx::query("UPDATE holding_bindings SET read_binding='CHANGED'WHERE holding_id='h001'").execute(&mut *tx).await.unwrap();assert!(proof.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
  let mut tx=old.begin().await.unwrap();lock_holding_binding_sources(&mut tx).await.unwrap();let proof=registry.read_current(&mut tx,&audience()).await.unwrap();tx.commit().await.unwrap();
  let mut next=old.begin().await.unwrap();assert!(proof.require_current(&mut next).await.is_err());next.rollback().await.unwrap();
 }).await;
}
#[tokio::test]
async fn actual_authorized_holding_unit_retains_binding_until_commit_and_next_unit_denies() {
    account::with_system(|system| async move {
        let p = account_queue::prepare(system).await;
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let authorizer = p.authorizer.clone();
        let proof = p.proof.clone();
        let selection = p.selection.clone();
        let access = p.access.clone();
        let signal = entered.clone();
        let gate = release.clone();
        let operation = tokio::spawn(async move {
            authorizer
                .execute(
                    "acme.example.test",
                    proof,
                    selection,
                    access,
                    move |_, _| {
                        let signal = signal.clone();
                        let gate = gate.clone();
                        Box::pin(async move {
                            signal.notify_one();
                            gate.notified().await;
                            Ok(())
                        })
                    },
                )
                .await
        });
        entered.notified().await;
        let pool = p.system.control.clone();
        let mut writer = tokio::spawn(async move {
            sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'")
                .execute(pool.raw())
                .await
                .unwrap();
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut writer)
                .await
                .is_err()
        );
        release.notify_one();
        operation.await.unwrap().unwrap();
        writer.await.unwrap();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = calls.clone();
        let denied = p
            .authorizer
            .execute(
                "acme.example.test",
                p.proof,
                p.selection,
                p.access,
                move |_, _| {
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            )
            .await;
        assert!(denied.is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    })
    .await;
}
#[tokio::test]
async fn deferred_transport_selection_never_runs_before_actual_session_authentication() {
    use erpx_server_identity_access::public::{
        account_inbound_http::CapturedHoldingMutation,
        effective_authority::HoldingWorkSourcePreparation,
    };
    use erpx_server_runtime::public::postgres::PgWorkFuture;
    #[derive(Clone)]
    struct NoAdditionalOwnerSources;
    impl HoldingWorkSourcePreparation for NoAdditionalOwnerSources {
        fn prepare<'c>(
            &'c self,
            _: &'c mut sqlx::PgConnection,
        ) -> PgWorkFuture<'c, (), BusinessError> {
            Box::pin(async { Ok(()) })
        }
    }
    account::with_system(|system|async move {
  let p=account_queue::prepare(system).await;let invoked=Arc::new(std::sync::atomic::AtomicUsize::new(0));
  let observed=invoked.clone();let failed=p.authorizer.execute_with_sources_deferred("acme.example.test",DeviceProof::default(),move||{observed.fetch_add(1,std::sync::atomic::Ordering::SeqCst);CapturedHoldingMutation::capture(&Default::default(),None,None).resolve()},p.access.clone(),NoAdditionalOwnerSources,|_,_|Box::pin(async{Ok(())})).await;
  assert!(matches!(failed,Err(PgTransactionError::Business(ref e))if e.code=="UNAUTHENTICATED"));assert_eq!(invoked.load(std::sync::atomic::Ordering::SeqCst),0);
  let observed=invoked.clone();let failed=p.authorizer.execute_with_sources_deferred("acme.example.test",p.proof,move||{observed.fetch_add(1,std::sync::atomic::Ordering::SeqCst);CapturedHoldingMutation::capture(&Default::default(),None,None).resolve()},p.access,NoAdditionalOwnerSources,|_,_|Box::pin(async{Ok(())})).await;
  assert!(matches!(failed,Err(PgTransactionError::Business(ref e))if e.code=="WORK_CONTEXT_CHANGED"));assert_eq!(invoked.load(std::sync::atomic::Ordering::SeqCst),1);
 }).await;
}
