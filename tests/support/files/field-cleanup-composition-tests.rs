//! Actual leased Control/Holding proof for the Field physical cleanup boundary.
use super::system::{clock, config, fixture};
use erpx_server_files::public::field_visit_upload::{
    FieldCleanupTransactions, LocalFieldQuarantineWriter, reconcile_field_upload_object,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, CompanyId};
use erpx_service::files_system_scan::{
    DevelopmentFilesScanTransactions, assemble_development_files_scan,
};
use sqlx::{PgConnection, PgPool};
use std::{
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    sync::atomic::{AtomicUsize, Ordering},
};
type Error = PgTransactionError<BusinessError>;
struct Interposed<C> {
    inner: DevelopmentFilesScanTransactions<C>,
    control: PgPool,
    holding: PgPool,
    steps: AtomicUsize,
    mode: u8,
}
impl<C: erpx_server_shared::public::Clock + Send + Sync + 'static> FieldCleanupTransactions
    for Interposed<C>
{
    async fn metadata<T, F>(&self, work: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: for<'a> Fn(&'a mut PgConnection, &'a CompanyId) -> PgWorkFuture<'a, T, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let value = self.inner.metadata(work).await?;
        if self.steps.fetch_add(1, Ordering::SeqCst) == 2 {
            if self.mode == 1 {
                sqlx::query(
                    "UPDATE holding_bindings SET status='suspended' WHERE holding_id='h001'",
                )
                .execute(&self.control)
                .await?;
            }
            if self.mode == 4 {
                sqlx::query(
                    "UPDATE organization_nodes SET status='suspended' WHERE id='company-owned'",
                )
                .execute(&self.holding)
                .await?;
            }
        }
        Ok(value)
    }
    async fn physical<T, F>(&self, work: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(
                &'a mut PgConnection,
                &'a CompanyId,
            ) -> PgWorkFuture<'a, T, BusinessError>
            + Send
            + 'static,
    {
        if self.mode == 2 || self.mode == 5 {
            let mut writer = self.holding.begin().await?;
            sqlx::query("INSERT INTO upload_intents(id,company_id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,quarantine_key,status,created_by,expires_at,created_at,updated_at) VALUES('gc-race','company-owned','field_service_visit','other','site_photo','a.png','image/png',24,'other-key','gc-object','expired','fixture',1,1,1)").execute(&mut *writer).await?;
            let inner = self.inner.clone();
            let require_repeatable_default = self.mode == 5;
            let task = tokio::spawn(async move {
                inner
                    .physical(move |c, company| {
                        Box::pin(async move {
                            if require_repeatable_default {
                                let setting: String =
                                    sqlx::query_scalar("SHOW default_transaction_isolation")
                                        .fetch_one(&mut *c)
                                        .await?;
                                let actual: String =
                                    sqlx::query_scalar("SHOW transaction_isolation")
                                        .fetch_one(&mut *c)
                                        .await?;
                                assert_eq!(setting, "repeatable read");
                                assert_eq!(actual, "read committed");
                            }
                            work(c, company).await
                        })
                    })
                    .await
            });
            // Confirm the physical transaction has reached its table lock while
            // the new reference remains uncommitted, then commit the reference.
            let mut waiting = false;
            for _ in 0..100 {
                waiting=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'LOCK TABLE attachment_links%')").fetch_one(&self.holding).await?;
                if waiting {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            assert!(
                waiting,
                "physical cleanup did not wait for the reference writer"
            );
            writer.commit().await?;
            return task.await.expect("physical task");
        }
        let value = self.inner.physical(work).await?;
        if self.mode == 3 {
            sqlx::query("UPDATE holding_bindings SET status='suspended' WHERE holding_id='h001'")
                .execute(&self.control)
                .await?;
        }
        Ok(value)
    }
}
async fn seed(pool: &PgPool, root: &std::path::Path) -> std::path::PathBuf {
    sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES('company-owned','company','gc-company','GC Company','company-owned','active') ON CONFLICT(id) DO UPDATE SET status='active'").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('gc-set','company-owned','field_service_visit','visit','record','site_photo',1,1,'internal','{}','fixture',1,1)").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO attachment_links(id,set_id,created_by,created_at,expires_at) VALUES('gc-link','gc-set','fixture',1,2)").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO upload_intents(id,company_id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,attachment_set_id,attachment_link_id) VALUES('gc-upload','company-owned','field_service_visit','visit','site_photo','a.png','image/png',24,'gc-object','expired','fixture',2,1,1,'gc-set','gc-link')").execute(pool).await.unwrap();
    sqlx::query("UPDATE attachment_links SET upload_intent_id='gc-upload' WHERE id='gc-link'")
        .execute(pool)
        .await
        .unwrap();
    use sha2::{Digest, Sha256};
    let body = b"\x89PNGabcdefgh\0\0\0\0IEND\xaeB`\x82";
    let digest = hex::encode(Sha256::digest(body));
    let folder = root.join(hex::encode(Sha256::digest(b"gc-object")));
    std::fs::create_dir(&folder).unwrap();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    let version = uuid::Uuid::new_v4().to_string();
    let manifest = serde_json::json!({"key":"gc-object","version":version,"size":body.len(),"etag":digest,"httpEtag":format!("\"{}\"",digest),"uploaded":"1970-01-01T00:00:01Z","httpMetadata":{"contentType":"image/png","cacheControl":"private, no-store"},"customMetadata":{"sha256":digest,"state":"quarantined"}});
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    for (name, bytes) in [
        (version.as_str(), body.as_slice()),
        ("current.json", manifest_bytes.as_slice()),
    ] {
        let path = folder.join(name);
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    folder
}
#[tokio::test]
async fn revoked_binding_and_reference_committed_while_waiting_preserve_objects_and_claims() {
    for mode in [1, 2, 3, 4, 5] {
        fixture::account::with_system(move |s| async move {
            let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();
            let folder=seed(s.holdings[0].raw(),root.path()).await;
            if mode==5 {
                let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(s.holdings[0].raw()).await.unwrap();
                assert!(db.starts_with("erpx_dev_rust_") && db.chars().all(|c| c.is_ascii_alphanumeric() || c=='_'));
                sqlx::query(sqlx::AssertSqlSafe(format!("ALTER ROLE erpx_test_admin IN DATABASE \"{db}\" SET default_transaction_isolation = 'repeatable read'")))
                    .execute(s.holdings[0].raw()).await.unwrap();
            }
            let clock=clock();
            let inner=assemble_development_files_scan(config(&s,"company-owned").await,clock.clone()).await.unwrap();
            let worker=Interposed{inner,control:s.control.raw().clone(),holding:s.holdings[0].raw().clone(),steps:AtomicUsize::new(0),mode};
            let store=LocalFieldQuarantineWriter::open(root.path(),2).unwrap();
            assert!(reconcile_field_upload_object(&store,&worker,"gc-upload",clock).await.is_err());
            assert_eq!(folder.join("current.json").exists(),mode!=3);
            let claims:i64=sqlx::query_scalar("SELECT count(*) FROM field_upload_object_cleanup_claims WHERE upload_intent_id='gc-upload'").fetch_one(s.holdings[0].raw()).await.unwrap();
            assert_eq!(claims,1,"failed phase must retain the claim");
            store.stop();store.drain().await;worker.inner.close().await;
        }).await;
    }
}

#[derive(Clone)]
struct RegressingClock(std::sync::Arc<AtomicUsize>);
impl erpx_server_shared::public::Clock for RegressingClock {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        let calls = self.0.fetch_add(1, Ordering::SeqCst);
        erpx_server_shared::public::TimestampMillis::new(if calls >= 4 { 400 } else { 1000 })
    }
}

#[tokio::test]
async fn abandoned_pin_recovers_only_after_object_lock_and_fresh_expiry() {
    use erpx_server_files::public::field_visit_upload::FieldCleanupOutcome;
    use nix::fcntl::{Flock, FlockArg};
    for mode in [0, 1, 2, 3] {
        fixture::account::with_system(move |s| async move {
            let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();
            let folder=seed(s.holdings[0].raw(),root.path()).await;
            let expires=if mode==1 { 1500 } else { 500 };
            let pin=uuid::Uuid::new_v4();
            sqlx::query("INSERT INTO field_upload_publication_pins(token,company_id,upload_intent_id,object_key,checksum_sha256,bytes,created_at,expires_at) VALUES($1,'company-owned','gc-upload','gc-object',$2,24,1,$3)")
                .bind(pin).bind("a".repeat(64)).bind(expires).execute(s.holdings[0].raw()).await.unwrap();
            let store=LocalFieldQuarantineWriter::open(root.path(),2).unwrap();
            if mode==3 {
                let time=std::sync::Arc::new(RegressingClock(std::sync::Arc::new(AtomicUsize::new(0))));
                let worker=assemble_development_files_scan(config(&s,"company-owned").await,time.clone()).await.unwrap();
                assert!(reconcile_field_upload_object(&store,&worker,"gc-upload",time).await.is_err());
                assert!(folder.join("current.json").exists());
                let pin_state:String=sqlx::query_scalar("SELECT state FROM field_upload_publication_pins WHERE token=$1").bind(pin).fetch_one(s.holdings[0].raw()).await.unwrap();
                assert_eq!(pin_state,"active","a backwards clock must roll back pin release");
                worker.close().await;
            } else {
                let time=clock();
                let worker=assemble_development_files_scan(config(&s,"company-owned").await,time.clone()).await.unwrap();
                if mode==2 {
                    let path=folder.join(".field-upload.lock");
                    let file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).unwrap();
                    let lock=Flock::lock(file,FlockArg::LockExclusiveNonblock).unwrap();
                    assert!(reconcile_field_upload_object(&store,&worker,"gc-upload",time.clone()).await.is_err());
                    let pin_state:String=sqlx::query_scalar("SELECT state FROM field_upload_publication_pins WHERE token=$1").bind(pin).fetch_one(s.holdings[0].raw()).await.unwrap();
                    assert_eq!(pin_state,"active");assert!(folder.join("current.json").exists());
                    drop(lock);
                }
                let result=reconcile_field_upload_object(&store,&worker,"gc-upload",time).await.unwrap();
                let pin_state:(String,Option<i64>)=sqlx::query_as("SELECT state,closed_at FROM field_upload_publication_pins WHERE token=$1").bind(pin).fetch_one(s.holdings[0].raw()).await.unwrap();
                if mode==1 { assert_eq!(result,FieldCleanupOutcome::NotEligible);assert_eq!(pin_state,("active".into(),None));assert!(folder.join("current.json").exists()); }
                else { assert_eq!(result,FieldCleanupOutcome::Cleaned);assert_eq!(pin_state,("released".into(),Some(1000)));assert!(!folder.join("current.json").exists()); }
                worker.close().await;
            }
            store.stop();store.drain().await;
        }).await;
    }
}

#[tokio::test]
async fn owned_publish_stages_are_reconciled_or_fail_closed() {
    use erpx_server_files::public::field_visit_upload::FieldCleanupOutcome;
    for mode in [0, 1, 2, 3, 4] {
        fixture::account::with_system(move |s| async move {
            let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();
            let folder=seed(s.holdings[0].raw(),root.path()).await;
            let manifest:serde_json::Value=serde_json::from_slice(&std::fs::read(folder.join("current.json")).unwrap()).unwrap();
            let version=manifest["version"].as_str().unwrap();let stage=folder.join(format!("{version}.json"));
            std::fs::hard_link(folder.join("current.json"),&stage).unwrap();
            if mode==0 || mode==3 || mode==4 { std::fs::remove_file(folder.join("current.json")).unwrap(); }
            if mode==4 { std::fs::remove_file(folder.join(version)).unwrap(); }
            if mode==2 { std::fs::write(folder.join("unknown"),b"unknown").unwrap(); }
            if mode==3 { std::fs::write(folder.join(version),b"partial").unwrap(); }
            let time=clock();let worker=assemble_development_files_scan(config(&s,"company-owned").await,time.clone()).await.unwrap();
            let store=LocalFieldQuarantineWriter::open(root.path(),2).unwrap();
            let result=reconcile_field_upload_object(&store,&worker,"gc-upload",time).await;
            if mode<=1 || mode==4 { assert_eq!(result.unwrap(),FieldCleanupOutcome::Cleaned);assert!(!folder.join("current.json").exists());assert!(!stage.exists());assert!(!folder.join(version).exists()); }
            else { assert!(result.is_err());assert!(stage.exists());assert!(folder.join(version).exists());let claims:i64=sqlx::query_scalar("SELECT count(*) FROM field_upload_object_cleanup_claims WHERE upload_intent_id='gc-upload'").fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!(claims,1); }
            worker.close().await;store.stop();store.drain().await;
        }).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_physical_poll_retains_database_guards_until_inline_work_finishes() {
    fixture::account::with_system(|s| async move {
        let root=tempfile::Builder::new().permissions(std::fs::Permissions::from_mode(0o700)).tempdir().unwrap();
        seed(s.holdings[0].raw(),root.path()).await;
        let worker=assemble_development_files_scan(config(&s,"company-owned").await,clock()).await.unwrap();
        let (started_tx,started_rx)=tokio::sync::oneshot::channel();
        let (release_tx,release_rx)=std::sync::mpsc::channel();
        let copy=worker.clone();
        let task=tokio::spawn(async move {
            copy.physical(move |_c,_company| Box::pin(async move {
                started_tx.send(()).unwrap();
                // Models the non-suspending filesystem critical section. An
                // abort cannot drop either transaction while its poll runs.
                tokio::task::block_in_place(|| release_rx.recv_timeout(std::time::Duration::from_secs(10)).expect("release watchdog"));
                Ok(())
            })).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10),started_rx).await.unwrap().unwrap();task.abort();
        let control=s.control.raw().clone();let holding=s.holdings[0].raw().clone();
        let binding=tokio::spawn(async move { sqlx::query("UPDATE holding_bindings SET status='suspended' WHERE holding_id='h001'").execute(&control).await });
        let reference=tokio::spawn(async move { sqlx::query("INSERT INTO upload_intents(id,company_id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('cancel-ref','company-owned','field_service_visit','other','site_photo','a.png','image/png',24,'cancel-key','expired','fixture',2,1,1)").execute(&holding).await });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!binding.is_finished());assert!(!reference.is_finished());
        release_tx.send(()).unwrap();
        let _=task.await;binding.await.unwrap().unwrap();reference.await.unwrap().unwrap();
        worker.close().await;
    }).await;
}
