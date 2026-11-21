#[allow(
    dead_code,
    reason = "shared real leased/source facts also serve integration branches"
)]
#[path = "../../support/files/system-scan-facts.rs"]
mod system;
use erpx_server_files::public::{
    attachment_scan::{
        AttachmentScanRunOutcome, AttachmentScanSettlement, AttachmentScanTransactions,
    },
    malware_scanner::ClamAvConfig,
    preview::PreviewToolConfiguration,
};
use erpx_server_shared::public::Clock;
use erpx_service::{files_scan_native::*, files_system_scan::assemble_development_files_scan};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use system::{config, facts, fixture};
fn private() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
struct Providers {
    _root: tempfile::TempDir,
    storage: PathBuf,
    other: PathBuf,
    temporary: PathBuf,
    tool: PathBuf,
    entered: PathBuf,
    release: PathBuf,
    heartbeat: PathBuf,
}
impl Providers {
    fn new() -> Self {
        let r = private();
        let storage = r.path().join("files");
        let other = r.path().join("other");
        let temporary = r.path().join("work");
        let tool = r.path().join("tool");
        for p in [&storage, &other, &temporary, &tool] {
            fs::create_dir(p).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            entered: r.path().join("entered"),
            release: r.path().join("release"),
            heartbeat: r.path().join("heartbeat"),
            _root: r,
            storage,
            other,
            temporary,
            tool,
        }
    }
    fn configuration(&self, held: bool) -> NativeFilesScanConfiguration {
        let actual_pdf = PathBuf::from(std::env::var("ERPX_PDFTOPPM_BIN").unwrap());
        let pdf = if held {
            let helper = self.tool.join("pdftoppm");
            fs::copy(env!("CARGO_BIN_EXE_erpx-files-scan-held-tool"), &helper).unwrap();
            fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
            let d = serde_json::json!({"actual":actual_pdf,"actual_sha256":hex::encode(Sha256::digest(fs::read(&actual_pdf).unwrap())),"entered":self.entered,"release":self.release,"heartbeat":self.heartbeat});
            let file = self.tool.join("fixture.json");
            fs::write(&file, serde_json::to_vec(&d).unwrap()).unwrap();
            fs::set_permissions(file, fs::Permissions::from_mode(0o600)).unwrap();
            helper
        } else {
            actual_pdf
        };
        NativeFilesScanConfiguration {
            files_roots: BTreeMap::from([
                ("HOLDING_H001_FILES".into(), self.storage.clone()),
                ("HOLDING_H002_FILES".into(), self.other.clone()),
            ]),
            temporary_root: self.temporary.clone(),
            preview_binary: PathBuf::from(std::env::var("ERPX_FILES_PREVIEW_BIN").unwrap()),
            preview_tools: PreviewToolConfiguration::new(
                Path::new(&std::env::var("ERPX_HEIF_CONVERT").unwrap()),
                &pdf,
            )
            .unwrap(),
            scanner: ClamAvConfig {
                socket: PathBuf::from(std::env::var("ERPX_CLAMD_SOCKET").unwrap()),
                database_directory: PathBuf::from(
                    std::env::var("ERPX_CLAMAV_DATABASE_DIR").unwrap(),
                ),
                maximum_definition_age: Duration::from_secs(86400),
                command_timeout: Duration::from_secs(5),
                scan_timeout: Duration::from_secs(30),
                concurrent_scans: 1,
            },
        }
    }
    fn original(&self, name: &str, body: &[u8]) {
        let key = format!("quarantine/{name}/original");
        let dir = self
            .storage
            .join(hex::encode(Sha256::digest(key.as_bytes())));
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let version = uuid::Uuid::new_v4().to_string();
        let bodypath = dir.join(&version);
        fs::write(&bodypath, body).unwrap();
        fs::set_permissions(bodypath, fs::Permissions::from_mode(0o600)).unwrap();
        let current = dir.join("current.json");
        fs::write(&current,serde_json::to_vec(&serde_json::json!({"key":key,"version":version,"size":body.len(),"etag":hex::encode(Sha256::digest(body))})).unwrap()).unwrap();
        fs::set_permissions(current, fs::Permissions::from_mode(0o600)).unwrap();
    }
    async fn entered(&self) -> Vec<String> {
        for _ in 0..1000 {
            if self.entered.exists() && self.heartbeat.exists() {
                let ids = fs::read_to_string(&self.entered)
                    .unwrap()
                    .lines()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                assert_eq!(ids.len(), 2);
                return ids;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("actual decoder did not reach its held compute barrier")
    }
    fn release(&self) {
        fs::write(&self.release, b"release actual decoder").unwrap();
    }
    fn actual_orphan_preview(&self, name: &str, expected: &[u8]) {
        let key = format!(
            "quarantine/{name}/preview-{}.jpg",
            hex::encode(Sha256::digest(expected))
        );
        let directory = self
            .storage
            .join(hex::encode(Sha256::digest(key.as_bytes())));
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.join("current.json"))
                .expect("real decoder must finish immutable staging before final binding denial"),
        )
        .unwrap();
        assert_eq!(
            fs::read(directory.join(manifest["version"].as_str().unwrap())).unwrap(),
            expected
        );
    }
    fn no_preview_temporaries(&self) {
        assert_eq!(fs::read_dir(&self.temporary).unwrap().count(), 0);
    }
    fn assert_pids_gone(&self, ids: &[String]) {
        for id in ids {
            assert!(
                !std::process::Command::new("/bin/kill")
                    .arg("-0")
                    .arg(id)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .unwrap()
                    .success(),
                "owned tool descendant remains"
            )
        }
    }
}
impl Drop for Providers {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, b"test fixture cleanup");
    }
}
fn time() -> Arc<facts::TestClock> {
    Arc::new(facts::TestClock::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64,
    ))
}
async fn source(
    s: &fixture::account::System,
    name: &str,
    mime: &str,
    body: &[u8],
) -> erpx_server_shared::public::RecordId {
    facts::seed_source(&s.holdings[0], name, facts::TestOrigin::Managed, mime, body).await
}
async fn no_transactions(s: &fixture::account::System) {
    for pool in [&s.control, &s.holdings[0]] {
        let count:i64=sqlx::query_scalar("SELECT count(*)FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND xact_start IS NOT NULL").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 0);
    }
}
#[tokio::test]
async fn actual_joined_private_source_clamav_rust_preview_and_owned_sql_receipt_publish_exact_artifact()
 {
    fixture::account::with_system(|s|async move{
  let p=Providers::new();let bytes=include_bytes!("../../../fixtures/files/preview/png-alpha.input");let id=source(&s,"native-clean","image/png",bytes).await;p.original("native-clean",bytes);let clock=time();
  let worker=assemble_native_development_files_scan(config(&s,"company-owned").await,p.configuration(false),clock).await.unwrap();
  assert!(matches!(worker.run_next(Some(&id)).await.unwrap(),AttachmentScanRunOutcome::Settled(AttachmentScanSettlement::Available)));
  let record:(String,String,String,i64,i64)=sqlx::query_as("SELECT a.source_state,u.status,a.preview_object_key,(SELECT count(*)FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview'),(SELECT count(*)FROM attachment_events WHERE attachment_id=a.id AND action='scan.available')FROM attachments a JOIN upload_intents u ON u.id=a.upload_id WHERE a.id=$1").bind(id.as_str()).fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!((&record.0,&record.1,record.3,record.4),(&"available".to_owned(),&"ready".to_owned(),1,1));
  let directory=p.storage.join(hex::encode(Sha256::digest(record.2.as_bytes())));let manifest:serde_json::Value=serde_json::from_slice(&fs::read(directory.join("current.json")).unwrap()).unwrap();let actual=fs::read(directory.join(manifest["version"].as_str().unwrap())).unwrap();assert_eq!(actual,include_bytes!("../../../fixtures/files/preview/png-alpha.webp"));
  no_transactions(&s).await;p.no_preview_temporaries();worker.close().await;
 }).await;
}
#[tokio::test]
async fn actual_clamav_infection_and_actual_decoder_rejection_never_become_available_or_partial_success()
 {
    fixture::account::with_system(|s|async move{
  let p=Providers::new();let worker=assemble_native_development_files_scan(config(&s,"company-owned").await,p.configuration(false),time()).await.unwrap();
  // Public standard signature exists only in this disposable private source,
  // never the checkout, installed service data, provider definitions or logs.
  let eicar=concat!("X5O!P%@AP[4\\PZX54(P^)7CC)7}$","EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*").as_bytes();let id=source(&s,"native-infected","text/csv",eicar).await;p.original("native-infected",eicar);
  assert!(matches!(worker.run_next(Some(&id)).await.unwrap(),AttachmentScanRunOutcome::Settled(AttachmentScanSettlement::MalwareRejected)));let state:(String,i64)=sqlx::query_as("SELECT source_state,(SELECT count(*)FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview')FROM attachments a WHERE id=$1").bind(id.as_str()).fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!(state,("malware_rejected".into(),0));
  // This owned malformed PNG has a complete IHDR and IEND but no IDAT.
  // The earlier media envelope admits it; the actual decoder must reject it.
  // The frozen truncated invalid-png fixture correctly fails earlier admission.
  let valid=include_bytes!("../../../fixtures/files/preview/png-alpha.input");
  let bytes=[&valid[..33],&valid[valid.len()-12..]].concat();
  assert_eq!(erpx_server_files::public::media_signature::detected_attachment_media_type(&bytes,"image/png"),Some("image/png"));
  let id=source(&s,"native-invalid","image/png",&bytes).await;p.original("native-invalid",&bytes);
  assert!(matches!(worker.run_next(Some(&id)).await.unwrap(),AttachmentScanRunOutcome::Settled(AttachmentScanSettlement::Error{next_attempt:Some(2)})));
  let state:(String,String,i64)=sqlx::query_as("SELECT a.source_state,j.error_code,(SELECT count(*)FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview')FROM attachments a JOIN attachment_scan_attempts j ON j.attachment_id=a.id AND j.attempt=1 WHERE a.id=$1").bind(id.as_str()).fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!(state,("scan_error".into(),"ATTACHMENT_PREVIEW_DECODER_REJECTED".into(),0));worker.close().await;p.no_preview_temporaries();
 }).await;
}
#[tokio::test]
async fn actual_current_binding_withdrawal_commits_during_held_decoder_and_blocks_all_final_sql_effects()
 {
    fixture::account::with_system(|s|async move{
  let p=Providers::new();let bytes=include_bytes!("../../../fixtures/files/preview/pdf-first-page.input");let id=source(&s,"native-binding","application/pdf",bytes).await;p.original("native-binding",bytes);let worker=assemble_native_development_files_scan(config(&s,"company-owned").await,p.configuration(true),time()).await.unwrap();
  let running=worker.clone();let target=id.clone();let task=tokio::spawn(async move{running.run_next(Some(&target)).await});let ids=p.entered().await;no_transactions(&s).await;
  tokio::time::timeout(Duration::from_secs(2),sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'").execute(s.control.raw())).await.unwrap().unwrap();assert!(!task.is_finished());p.release();assert!(task.await.unwrap().is_err());
  p.actual_orphan_preview("native-binding",include_bytes!("../../../fixtures/files/preview/pdf-first-page.jpg"));
  assert_eq!(system::state(&s,id.as_str()).await,("scanning".into(),"running".into(),1,0));let count:i64=sqlx::query_scalar("SELECT count(*)FROM attachment_storage_ledger WHERE attachment_id=$1 AND variant='preview'").bind(id.as_str()).fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!(count,0);worker.close().await;p.no_preview_temporaries();p.assert_pids_gone(&ids);
 }).await;
}
#[tokio::test]
async fn actual_held_decoder_lease_reclaim_and_caller_cancellation_retain_no_stale_effect_or_descendant()
 {
    for cancel in [false, true] {
        fixture::account::with_system(move |s| async move {
            let p = Providers::new();
            let name = if cancel {
                "native-cancel"
            } else {
                "native-reclaim"
            };
            let bytes = include_bytes!("../../../fixtures/files/preview/pdf-first-page.input");
            let id = source(&s, name, "application/pdf", bytes).await;
            p.original(name, bytes);
            let clock = time();
            let worker = assemble_native_development_files_scan(
                config(&s, "company-owned").await,
                p.configuration(true),
                clock.clone(),
            )
            .await
            .unwrap();
            let running = worker.clone();
            let target = id.clone();
            let task = tokio::spawn(async move { running.run_next(Some(&target)).await });
            let ids = p.entered().await;
            no_transactions(&s).await;
            if cancel {
                task.abort();
                assert!(matches!(task.await, Err(error) if error.is_cancelled()));
                worker.shutdown_and_drain().await;
            } else {
                clock.set(clock.now().get() + 61_001);
                let reclaim =
                    assemble_development_files_scan(config(&s, "company-owned").await, clock)
                        .await
                        .unwrap();
                let current = reclaim
                    .claim_and_prepare(Some(id.as_str()))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(current.claim().attempt(), 1);
                assert!(!task.is_finished());
                p.release();
                assert!(task.await.unwrap().is_err());
                reclaim.close().await;
            }
            assert_eq!(
                system::state(&s, id.as_str()).await,
                ("scanning".into(), "running".into(), 1, 0)
            );
            worker.close().await;
            p.no_preview_temporaries();
            p.assert_pids_gone(&ids);
        })
        .await;
    }
}

#[tokio::test]
async fn field_runtime_loop_probes_real_providers_scans_and_shuts_down_without_open_transactions() {
 use erpx_service::field_attachment_intake_command::FieldIntakeProviderReadiness;
 use erpx_service::files_scan_worker::{FieldFilesScanTarget,assemble_field_files_scan_workers,run_field_files_scan_workers};
 fixture::account::with_system(|s|async move {
  let p=Providers::new();let bytes=include_bytes!("../../../fixtures/files/preview/png-alpha.input");
  let id=facts::seed_source(&s.holdings[0],"field-runtime",facts::TestOrigin::ManagedField,"image/png",bytes).await;p.original("field-runtime",bytes);
  let workers=assemble_field_files_scan_workers(vec![FieldFilesScanTarget{databases:config(&s,"company-owned").await,providers:p.configuration(false)}],time()).await.unwrap();
  let status=workers[0].probe().await.unwrap();assert!(status.scanner_ready&&status.preview_ready&&status.scanner_enabled);
  let (stop,shutdown)=tokio::sync::oneshot::channel::<()>();let task=tokio::spawn(run_field_files_scan_workers(workers,async move{let _=shutdown.await;}));
  let mut available=false;for _ in 0..300 {let state:String=sqlx::query_scalar("SELECT source_state FROM attachments WHERE id=$1").bind(id.as_str()).fetch_one(s.holdings[0].raw()).await.unwrap();if state=="available" {available=true;break;}tokio::time::sleep(Duration::from_millis(100)).await;}
  assert!(available,"real Field scanner did not settle");stop.send(()).unwrap();tokio::time::timeout(Duration::from_secs(10),task).await.unwrap().unwrap();
  let effects:(String,i64,i64)=sqlx::query_as("SELECT source_state,(SELECT count(*) FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview'),(SELECT count(*) FROM attachment_events WHERE attachment_id=a.id AND action='scan.available') FROM attachments a WHERE id=$1").bind(id.as_str()).fetch_one(s.holdings[0].raw()).await.unwrap();assert_eq!(effects,("available".into(),1,1));no_transactions(&s).await;p.no_preview_temporaries();
 }).await;
}
