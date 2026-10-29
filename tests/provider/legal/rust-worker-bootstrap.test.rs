//! Explicit real-provider smoke: native tools, official private eng+tha models,
//! real ClamAV, actual Auth/SSO and leased disposable PostgreSQL only. No server
//! listener, installed storage, external mail or staging/production activation.
#[allow(
    dead_code,
    reason = "shared actual Account fixture also serves owner lifecycle suites"
)]
#[path = "../../support/rust-account-queued-authority.rs"]
mod account_queue;
use erpx_server_files::public::malware_scanner::{ClamAvConfig, ClamAvScanner, MalwareVerdict};
use erpx_server_legal::public::{DocumentToolConfiguration, LegalProcessingKind};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{Clock, TimestampMillis};
use sha2::{Digest, Sha256};
use sqlx::{ConnectOptions, postgres::PgConnectOptions};
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::atomic::Ordering,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(now())
    }
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn private_directory() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().unwrap()
}
fn selected(name: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var(name).expect("Explicit native provider configuration required"),
    );
    assert!(path.is_absolute());
    path.canonicalize().unwrap()
}
fn tools() -> DocumentToolConfiguration {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/worker-bootstrap-provider.v1.json"
    ))
    .unwrap();
    let models = selected("ERPX_LEGAL_TEST_TESSDATA");
    assert_eq!(
        models,
        root()
            .join(".scratch/dev/legal-native-provider/tessdata")
            .canonicalize()
            .unwrap(),
        "models belong only to this ignored private fixture"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&models).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    for resource in receipt["models"]["resources"].as_array().unwrap() {
        let file = models.join(resource["file"].as_str().unwrap());
        let bytes = fs::read(file).unwrap();
        assert_eq!(bytes.len() as u64, resource["bytes"].as_u64().unwrap());
        assert_eq!(
            hex::encode(Sha256::digest(bytes)),
            resource["sha256"].as_str().unwrap()
        );
    }
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/native-conformance.v1.json"
    ))
    .unwrap();
    for resource in frozen["oracle"]["toolReceipts"].as_array().unwrap() {
        assert_eq!(
            hex::encode(Sha256::digest(
                fs::read(resource["path"].as_str().unwrap()).unwrap()
            )),
            resource["sha256"].as_str().unwrap(),
            "captured native tool changed"
        );
    }
    let mut tools: DocumentToolConfiguration =
        serde_json::from_value(frozen["oracle"]["tools"].clone()).unwrap();
    tools.languages = "eng+tha".into();
    tools.tessdata = models;
    tools
}
fn source(kind: LegalProcessingKind) -> (&'static str, Vec<u8>) {
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/native-conformance.v1.json"
    ))
    .unwrap();
    let id = if kind == LegalProcessingKind::Ocr {
        "png"
    } else {
        "text-pdf"
    };
    let item = frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap();
    (
        if kind == LegalProcessingKind::Ocr {
            "image/png"
        } else {
            "application/pdf"
        },
        hex::decode(item["hex"].as_str().unwrap()).unwrap(),
    )
}
async fn scan(bytes: &[u8]) -> i64 {
    let scanner = ClamAvScanner::new(ClamAvConfig {
        socket: selected("ERPX_CLAMD_SOCKET"),
        database_directory: selected("ERPX_CLAMAV_DATABASE_DIR"),
        maximum_definition_age: Duration::from_secs(86400),
        command_timeout: Duration::from_secs(5),
        scan_timeout: Duration::from_secs(30),
        concurrent_scans: 1,
    })
    .unwrap();
    let result = scanner.scan(bytes, &SystemClock).await.unwrap();
    assert_eq!(result.verdict, MalwareVerdict::Clean);
    assert!(result.readiness.engine.starts_with("ClamAV "));
    result.readiness.definition_updated_at.get()
}
async fn seed(
    pool: &DatabasePool,
    binding: &str,
    kind: LegalProcessingKind,
    media: &str,
    bytes: &[u8],
    definition: i64,
) {
    let hash = hex::encode(Sha256::digest(bytes));
    let current = now();
    let key = "legal/cli/original";
    sqlx::raw_sql("INSERT INTO legal_settings(company_id,enabled)VALUES('company',true);INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)VALUES('cli-item','company','contract','Synthetic CLI provider','internal','TH','TH','th','THB','Asia/Bangkok','normal','fixture-a','fixture-a',0,0);INSERT INTO legal_contracts VALUES('cli-item','{}')").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id)VALUES('cli-upload','legal_item','cli-item','legal_document','synthetic-original',$1,$2,$2,$3,'staging/cli',$4,'ready','fixture-a',$5,$6,$6,'company')").bind(media).bind(bytes.len()as i64).bind(&hash).bind(key).bind(current+3_600_000).bind(current).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state)VALUES('cli-source','cli-upload','legal_item','cli-item','legal_document',$1,1,'synthetic-original',$2,$3,$4,'fixture-a',$5,'company','quarantined')").bind(key).bind(media).bind(bytes.len()as i64).bind(&hash).bind(current).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE attachments SET source_state='scanning'WHERE id='cli-source'")
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("UPDATE attachments SET source_state='available',detected_media_type=$1,scanned_at=$2,scanner_definition_at=$3 WHERE id='cli-source'").bind(media).bind(current).bind(definition).execute(pool.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,state,created_by,created_at,updated_at,bound_at,bound_command_id)VALUES('cli-set','company','legal_item','cli-item','record','cli-command','legal_document',1,1,'restricted','{}','legal-managed','bound','fixture-a',0,0,1,'cli-command');INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id)VALUES('cli-link','cli-set','cli-source','cli-upload','bound','fixture-a',0,8000000000000000,1,'legal_document','cli-document')").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,scanner_definition_at,created_at,completed_at)VALUES('cli-scan','cli-source',1,'clean',$1,$2,$2)").bind(definition).bind(current).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,created_by,created_at,attachment_link_id,attachment_source_id)VALUES('cli-document','cli-item',1,'synthetic-original',$1,$2,$3,$4,'fixture-a',$5,'cli-link','cli-source')").bind(media).bind(&hash).bind(bytes.len()as i64).bind(key).bind(current).execute(pool.raw()).await.unwrap();
    // The descriptor was captured by actual Account/SSO + current Holding IAM.
    // This fixture provisions queued work; the CLI must claim and authorize it.
    sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,actor_json,created_by,created_at)VALUES('cli-job','company','cli-item','cli-document',$1,$2,'queued',$3,'fixture-a',$4)").bind(hash).bind(kind.as_str()).bind(binding).bind(current).execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','legal',1,'active',$1,$2,1,'Synthetic reviewed provider fixture','public-synthetic-evidence',$1)").bind(current-1000).bind(current+3_600_000).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'cli-purchase','company','legal','fixture-a','public-synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company'AND package_id='legal'").bind(current).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
fn disk(root: &Path, bytes: &[u8]) {
    let key = "legal/cli/original";
    let directory = root.join(hex::encode(Sha256::digest(key)));
    fs::create_dir(&directory).unwrap();
    let version = uuid::Uuid::new_v4().to_string();
    fs::write(directory.join(&version), bytes).unwrap();
    fs::write(directory.join("current.json"),serde_json::to_vec(&serde_json::json!({"key":key,"version":version,"size":bytes.len(),"etag":hex::encode(Sha256::digest(bytes))})).unwrap()).unwrap();
}
async fn config(
    control: &DatabasePool,
    pool: &DatabasePool,
    tools: &DocumentToolConfiguration,
    files: &Path,
    temporary: &Path,
    path: &Path,
) {
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool.raw())
        .await
        .unwrap();
    let token = name.strip_prefix("erpx_dev_rust_").unwrap();
    let mut url = PgConnectOptions::from_str(&std::env::var("ERPX_TEST_ADMIN_URL").unwrap())
        .unwrap()
        .database(&name)
        .to_url_lossy();
    url.set_query(None);
    let control_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(control.raw())
        .await
        .unwrap();
    let control_token = control_name.strip_prefix("erpx_dev_rust_").unwrap();
    let mut control_url =
        PgConnectOptions::from_str(&std::env::var("ERPX_TEST_ADMIN_URL").unwrap())
            .unwrap()
            .database(&control_name)
            .to_url_lossy();
    control_url.set_query(None);
    let value = serde_json::json!({"controlDatabaseUrl":control_url.as_str(),"controlLeaseToken":control_token,"accountHost":"account.example.test","holdingBindings":[{"holdingId":"h001","slug":"acme","host":"acme.example.test","writeBinding":"HOLDING_H001_DB","readBinding":"HOLDING_H001_READ_DB","filesBinding":"HOLDING_H001_FILES"},{"holdingId":"h002","slug":"northstar","host":"northstar.example.test","writeBinding":"HOLDING_H002_DB","readBinding":"HOLDING_H002_READ_DB","filesBinding":"HOLDING_H002_FILES"}],"mode":"development","commercialMode":"enforced","holdingId":"h001","companyId":"company","databaseUrl":url.as_str(),"leaseToken":token,"filesRoot":files,"temporaryRoot":temporary,"documentBinary":selected("ERPX_DOCUMENT_WORKER_BIN"),"pdfBinary":selected("ERPX_PDF_WORKER_BIN"),"tools":tools});
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn original_is_computing(temporary: &Path, original: &[u8]) -> bool {
    fs::read_dir(temporary)
        .unwrap()
        .filter_map(Result::ok)
        .any(|directory| {
            ["input", "document.pdf"].into_iter().any(|name| {
                fs::read(directory.path().join(name)).is_ok_and(|bytes| bytes == original)
            })
        })
}
#[tokio::test]
async fn real_once_cli_claims_scanned_original_computes_without_database_leases_and_freshly_retains_or_denies()
 {
    let tools = tools();
    for scenario in [
        "cancel-during-probe",
        "ocr-success",
        "pdf-unsigned",
        "authority-denied",
        "source-revoked",
        "during-compute-authority",
        "during-compute-source",
        "audit-rollback",
    ] {
        let kind = if scenario == "pdf-unsigned" {
            LegalProcessingKind::Verify
        } else {
            LegalProcessingKind::Ocr
        };
        let (media, bytes) = source(kind);
        let definition = scan(&bytes).await;
        let tools = tools.clone();
        account_queue::account::with_system(move|system|async move{
            system.scenario.clock.0.store(now(),Ordering::SeqCst);let prepared=account_queue::prepare(system).await;let pool=&prepared.system.holdings[0];seed(pool,&prepared.binding.to_json().unwrap(),kind,media,&bytes,definition).await;
            let files=private_directory();disk(files.path(),&bytes);let temporary=private_directory();let configuration=private_directory();let path=configuration.path().join("worker.json");config(&prepared.system.control,pool,&tools,files.path(),temporary.path(),&path).await;
            match scenario{
                "authority-denied"=>{sqlx::query("UPDATE iam_roles SET status='archived'WHERE id='custom_queued_legal'").execute(pool.raw()).await.unwrap();},
                "source-revoked"=>{sqlx::query("UPDATE attachments SET revoked_at=$1 WHERE id='cli-source'").bind(now()).execute(pool.raw()).await.unwrap();},
                "audit-rollback"=>{sqlx::raw_sql("CREATE FUNCTION deny_cli_audit()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.action='document.ocr'THEN RAISE EXCEPTION 'synthetic audit denial' USING ERRCODE='23514';END IF;RETURN NEW;END$$;CREATE TRIGGER cli_audit_denial BEFORE INSERT ON legal_access_events FOR EACH ROW EXECUTE FUNCTION deny_cli_audit()").execute(pool.raw()).await.unwrap();},_=>{}
            }
            for database in [&prepared.system.control,pool]{
                let active:i64=sqlx::query_scalar("SELECT count(*)FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND backend_type='client backend'AND xact_start IS NOT NULL").fetch_one(database.raw()).await.unwrap();assert_eq!(active,0,"fixture left an application transaction open before CLI startup: {scenario}");
            }
            let mut child=tokio::process::Command::new(env!("CARGO_BIN_EXE_erpx-server")).env_clear().stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).args(["legal-worker","--once","--development-config"]).arg(&path).spawn().unwrap();
            // Keep the owned child unreaped until try_wait observes completion.
            // A signal can therefore target only this exact PID, never a reused
            // or unrelated process. Pipe reads happen after its bounded exit.
            let mut observed=false;let mut observed_compute=false;let mut retained_transaction=false;let mut withdrawn=false;let mut withdrawn_while_computing=false;let mut cancelled=false;let mut active_observed=false;let mut withdrawal_error=false;let mut slow_withdrawal=false;
            let status=loop{
                if let Some(status)=child.try_wait().unwrap(){break status;}
                if scenario=="cancel-during-probe"&&!cancelled&&fs::read_dir(temporary.path()).unwrap().next().is_some()&&let Some(pid)=child.id(){cancelled=nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32),nix::sys::signal::Signal::SIGTERM).is_ok();}
                // Startup probes deliberately use the same frozen inputs. An
                // input file is a computation witness only after this job's
                // actual claim has committed; a probe cannot satisfy it.
                let running=if original_is_computing(temporary.path(),&bytes){sqlx::query_scalar::<_,String>("SELECT state FROM legal_processing_jobs WHERE id='cli-job'").fetch_one(pool.raw()).await.unwrap()=="running"}else{false};
                if running&&original_is_computing(temporary.path(),&bytes){
                    let mut active=false;
                    for database in [&prepared.system.control,pool]{active|=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND backend_type='client backend'AND xact_start IS NOT NULL)").fetch_one(database.raw()).await.unwrap_or(true);}
                    if original_is_computing(temporary.path(),&bytes){
                        retained_transaction|=active;active_observed|=active;observed_compute=true;
                        if !withdrawn&&["during-compute-authority","during-compute-source"].contains(&scenario){
                            let start=std::time::Instant::now();
                            let result=if scenario=="during-compute-authority"{sqlx::query("UPDATE iam_roles SET status='archived'WHERE id='custom_queued_legal'").execute(pool.raw()).await}else{sqlx::query("UPDATE attachments SET revoked_at=$1 WHERE id='cli-source'").bind(now()).execute(pool.raw()).await};
                            withdrawal_error|=result.is_err();slow_withdrawal|=start.elapsed()>Duration::from_secs(3);retained_transaction|=withdrawal_error||slow_withdrawal;withdrawn=true;withdrawn_while_computing=original_is_computing(temporary.path(),&bytes);
                        }
                    }
                }
                observed=true;tokio::time::sleep(Duration::from_millis(10)).await;
            };
            use tokio::io::AsyncReadExt;
            let mut stdout=Vec::new();let mut stderr=Vec::new();child.stdout.take().unwrap().take(2*1024*1024+1).read_to_end(&mut stdout).await.unwrap();child.stderr.take().unwrap().take(2*1024*1024+1).read_to_end(&mut stderr).await.unwrap();assert!(stdout.len()<=2*1024*1024&&stderr.len()<=2*1024*1024);
            let output=std::process::Output{status,stdout,stderr};
            if ["during-compute-authority","during-compute-source"].contains(&scenario){assert!(withdrawn&&withdrawn_while_computing,"current withdrawal did not commit before actual original computation finished");}
            assert!(!retained_transaction,"actual computation witness failed: scenario={scenario}, clientTransaction={active_observed}, withdrawalError={withdrawal_error}, slowWithdrawal={slow_withdrawal}");assert!(observed);if ["ocr-success","audit-rollback"].contains(&scenario){assert!(observed_compute,"actual OCR computation was not observed independently of startup probes");}let(state,result,error,attempts):(String,Option<String>,Option<String>,i32)=sqlx::query_as("SELECT state,result_json,error,attempts FROM legal_processing_jobs WHERE id='cli-job'").fetch_one(pool.raw()).await.unwrap();let inspections:i64=sqlx::query_scalar("SELECT count(*)FROM legal_document_inspections").fetch_one(pool.raw()).await.unwrap();let access:i64=sqlx::query_scalar("SELECT count(*)FROM legal_access_events WHERE action IN('document.ocr','document.verify')").fetch_one(pool.raw()).await.unwrap();assert_eq!(attempts,if scenario=="cancel-during-probe"{0}else{1});
            match scenario{
                "cancel-during-probe"=>{assert!(cancelled);assert!(!output.status.success());assert_eq!(state,"queued");assert!(result.is_none()&&error.is_none());assert_eq!((inspections,access),(0,0));assert!(String::from_utf8_lossy(&output.stderr).contains("LEGAL_WORKER_CANCELLED"));},
                "ocr-success"|"pdf-unsigned"=>{assert!(output.status.success(),"native CLI failed: {}",String::from_utf8_lossy(&output.stderr));let summary:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(summary["state"],"completed");assert_eq!(summary["acceptance"],false);assert_eq!(state,"completed");assert!(error.is_none());assert_eq!((inspections,access),(1,1));let retained:serde_json::Value=serde_json::from_str(result.as_deref().unwrap()).unwrap();assert!(retained["computedInput"].is_object());if kind==LegalProcessingKind::Ocr{assert_eq!(retained["status"],"extracted");assert_eq!(retained["method"],"image-ocr");assert!(retained["text"].as_str().unwrap().contains("ERPX"));assert_eq!(retained["requiresReview"],true);}else{assert_eq!(retained["status"],"indeterminate");}},
                "audit-rollback"=>{assert!(!output.status.success());assert_eq!(state,"running");assert!(result.is_none());assert_eq!((inspections,access),(0,0));assert!(String::from_utf8_lossy(&output.stderr).contains("LEGAL_WORKER_PERSISTENCE_FAILED"));},
                _=>{assert!(output.status.success(),"maintenance failure must retain its own exact lease");assert_eq!(state,"failed");assert!(result.is_none());assert_eq!(error.as_deref(),Some("LEGAL_CURRENT_AUTHORITY_OR_SOURCE_CHANGED"));assert_eq!((inspections,access),(0,0));}
            }
            assert!(fs::read_dir(temporary.path()).unwrap().next().is_none());
        }).await;
    }
}
