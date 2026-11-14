#[path = "../../support/rust-postgres.rs"]
mod fixture;

use base64::{Engine, engine::general_purpose::STANDARD};
use erpx_server_legal::public::{
    AuthorizedPdfSnapshot, BoundedPdfVerifier, LegalInspectionDocument, LegalOriginalRead,
    LegalProcessingActor, LegalProcessingDocument, LegalProcessingJob, LegalProcessingKind,
    LegalProcessingLease, LegalProcessingQueueAuthority, LegalProcessingReceipt,
    LegalProcessingSourceRequest, PdfProcessLimits, PdfVerificationEvidence,
    PdfVerificationFailure, PdfVerificationJob, PdfVerificationRepository, PdfVerifierProcess,
    PgLegalPdfVerificationBridge, PgLegalProcessingBridge, PgLegalProcessingRepository,
    PgPdfVerificationRepository, ValidationState, compute_prepared_legal_pdf,
    inspect_current_legal_pdf, load_current_legal_processing_source, prepare_current_legal_pdf,
    retain_current_legal_pdf, verify_company_pdf,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis, UserId,
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_793_080_800_000)
    }
}
struct Identifiers {
    prefix: &'static str,
    next: u32,
}
impl IdentifierSource for Identifiers {
    fn next(&mut self) -> RecordId {
        self.next += 1;
        RecordId::new(format!("{}-{}", self.prefix, self.next)).unwrap()
    }
}

fn job() -> PdfVerificationJob {
    PdfVerificationJob {
        job_id: RecordId::new("job").unwrap(),
        holding_id: HoldingId::new("holding-a").unwrap(),
        company_id: CompanyId::new("company-a").unwrap(),
        document_version_id: RecordId::new("document").unwrap(),
        requested_by: UserId::new("requester").unwrap(),
    }
}
fn pdf() -> Vec<u8> {
    let j: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance.v1.json"
    ))
    .unwrap();
    hex::decode(j["cases"][0]["pdfHex"].as_str().unwrap()).unwrap()
}
fn private_directory() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().unwrap()
}

/// Explicit synthetic owner bridges. They do not establish production Identity,
/// Commercial or Files authority and perform no network/storage access.
struct Bridge {
    pdf: Vec<u8>,
    mode: Arc<AtomicU8>,
}
impl PgLegalProcessingBridge for Bridge {
    async fn authorize_queue(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
    ) -> Result<LegalProcessingQueueAuthority, PgTransactionError<BusinessError>> {
        Err(BusinessError::invalid(
            "TEST_PRODUCER_NOT_CONFIGURED",
            "This fixture only tests the worker.",
        )
        .into())
    }
    async fn replay(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &RecordId,
        _: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, PgTransactionError<BusinessError>> {
        Err(BusinessError::invalid(
            "TEST_PRODUCER_NOT_CONFIGURED",
            "This fixture only tests the worker.",
        )
        .into())
    }
    async fn record_queued(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
        _: &LegalProcessingReceipt,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        Err(BusinessError::invalid(
            "TEST_PRODUCER_NOT_CONFIGURED",
            "This fixture only tests the worker.",
        )
        .into())
    }
    async fn candidate_eligible(
        &mut self,
        _: &mut PgConnection,
        job: &LegalProcessingJob,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        assert_eq!(job.company_id.as_str(), "company-a");
        Ok(self.mode.load(Ordering::SeqCst) != 1)
    }
    async fn authorize_completion(
        &mut self,
        _: &mut PgConnection,
        lease: &LegalProcessingLease,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        assert_eq!(lease.job.created_by.as_str(), "requester");
        if self.mode.load(Ordering::SeqCst) == 1 {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "LEGAL_PERMISSION_REQUIRED",
                "Authority changed.",
            )
            .into())
        } else {
            Ok(())
        }
    }
    async fn authorize_status(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &RecordId,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        if self.mode.load(Ordering::SeqCst) == 1 {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "LEGAL_PERMISSION_REQUIRED",
                "Authority changed.",
            )
            .into())
        } else {
            Ok(())
        }
    }
}
impl PgLegalPdfVerificationBridge for Bridge {
    async fn authorize_document(
        &mut self,
        _: &mut PgConnection,
        job: &PdfVerificationJob,
        document: &LegalInspectionDocument,
    ) -> Result<String, PgTransactionError<BusinessError>> {
        assert_eq!(job.company_id, document.company_id);
        if self.mode.load(Ordering::SeqCst) == 1 {
            return Err(PgTransactionError::Business(BusinessError::new(
                FailureKind::Forbidden,
                "LEGAL_PERMISSION_REQUIRED",
                "Authority changed.",
            )));
        }
        Ok(if self.mode.load(Ordering::SeqCst) == 2 {
            "current-authority-2"
        } else {
            "current-authority-1"
        }
        .to_owned())
    }
    async fn independent_person_review(
        &mut self,
        _: &mut PgConnection,
        _: &CompanyId,
        created: &UserId,
        approved: &UserId,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        Ok(created != approved)
    }
    async fn read_original(
        &mut self,
        connection: &mut PgConnection,
        _: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        max: usize,
    ) -> Result<LegalOriginalRead, PgTransactionError<BusinessError>> {
        assert!(document.attachment_link_id.is_some());
        assert!(self.pdf.len() <= max);
        let (revision,state,destroyed_at):(i64,String,Option<i64>) = sqlx::query_as(
            "SELECT revision::bigint,source_state,destroyed_at FROM attachments WHERE id='source' FOR SHARE",
        )
        .fetch_one(connection)
        .await?;
        if state != "available" || destroyed_at.is_some() {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FILE_SOURCE_CHANGED",
                "The original source changed.",
            )
            .into());
        }
        Ok(LegalOriginalRead {
            source_data_sha256: hex::encode(Sha256::digest(
                format!("synthetic-source-data/v1:{revision}").as_bytes(),
            )),
            bytes: self.pdf.clone(),
            witness: "canonical-original-1".to_owned(),
        })
    }
    async fn revalidate_original(
        &mut self,
        connection: &mut PgConnection,
        _: &PdfVerificationJob,
        _: &LegalInspectionDocument,
        _: &str,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        match self.mode.load(Ordering::SeqCst) {
            3 => Err(PgTransactionError::Business(BusinessError::new(
                FailureKind::Conflict,
                "FILE_SOURCE_CHANGED",
                "The canonical source changed.",
            ))),
            4 => {
                sqlx::query("UPDATE legal_trust_material SET state='retired',version=version+1 WHERE company_id='company-a'").execute(connection).await?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

async fn setup(pool: &DatabasePool, canonical: bool, self_approved: bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    let bytes = pdf();
    let checksum = hex::encode(Sha256::digest(&bytes));
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES('company-a','company','synthetic','Synthetic Company','company-a','active');
        INSERT INTO legal_settings(company_id,enabled) VALUES('company-a',true);
        INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at) VALUES('item','company-a','contract','Synthetic contract','internal','TH','TH','en','THB','Asia/Bangkok','normal','requester','requester',0,0)")
        .execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at) VALUES('upload','legal','item','legal_document','fixture.pdf','application/pdf',$1,$1,$2,'fixture-stage','fixture-object','ready','requester',9999999999999,0,0)")
        .bind(bytes.len() as i64).bind(&checksum).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES('source','upload','legal','item','legal_document','fixture-object',1,'fixture.pdf','application/pdf',$1,$2,'requester',0,'company-a','application/pdf','available',0,0)")
        .bind(bytes.len() as i64).bind(&checksum).execute(pool.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at,bound_at,bound_command_id) VALUES('set','company-a','legal_item','item','record','legal_document',1,1,'internal','{}','bound','requester',0,0,0,'binding');
        INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) VALUES('link','set','source','bound','requester',0,9999999999999,0,'legal_document','document')")
        .execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,created_by,created_at,attachment_link_id,attachment_source_id) VALUES('document','item',1,'fixture.pdf','application/pdf',$1,$2,'fixture-object','requester',0,$3,$4)")
        .bind(checksum).bind(bytes.len() as i64).bind(canonical.then_some("link")).bind(canonical.then_some("source")).execute(pool.raw()).await.unwrap();
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/pdf-signatures/conformance.v1.json"
    ))
    .unwrap();
    for (index, item) in corpus["cases"][0]["trust"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let path = item["path"].as_str().unwrap();
        let mut bytes = hex::decode(item["hex"].as_str().unwrap()).unwrap();
        let kind = if path.starts_with("roots/") {
            bytes = openssl::x509::X509::from_pem(&bytes)
                .unwrap()
                .to_der()
                .unwrap();
            "root"
        } else {
            "crl"
        };
        sqlx::query("INSERT INTO legal_trust_material(id,company_id,kind,name,source,content,checksum,state,created_by,approved_by,created_at) VALUES($1,'company-a',$2,'Synthetic trust','https://unreachable.invalid',$3,$4,'approved','material-creator',$5,0)")
            .bind(format!("material-{index}")).bind(kind).bind(STANDARD.encode(&bytes)).bind(hex::encode(Sha256::digest(&bytes)))
            .bind(if self_approved {"material-creator"}else{"material-reviewer"}).execute(pool.raw()).await.unwrap();
    }
}

#[tokio::test]
async fn actual_rust_pdf_result_and_material_receipts_retain_with_the_owner_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, true, false).await;
        let directory = private_directory();
        let verifier = Arc::new(
            BoundedPdfVerifier::new(
                Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf")),
                directory.path(),
                PdfProcessLimits::default(),
            )
            .unwrap(),
        );
        let uow = PgUnitOfWork::new(pool.clone());
        let result = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let verifier = verifier.clone();
                Box::pin(async move {
                    let mut bridge = Bridge {
                        pdf: pdf(),
                        mode: Arc::new(AtomicU8::new(0)),
                    };
                    let mut ids = Identifiers {
                        prefix: "retained",
                        next: 0,
                    };
                    let mut repository = PgPdfVerificationRepository::new(
                        HoldingId::new("holding-a").unwrap(),
                        CompanyId::new("company-a").unwrap(),
                        connection,
                        &mut bridge,
                        &FixedClock,
                        &mut ids,
                    );
                    verify_company_pdf(&mut repository, verifier.as_ref(), &FixedClock, &job())
                        .await
                })
            })
            .await
            .unwrap();
        assert_eq!(result.report.status, ValidationState::Valid);
        assert!(result.inspection_id.is_some());
        let stored =
            sqlx::query_scalar::<_, String>("SELECT result_json FROM legal_document_inspections")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(stored["status"], "valid");
        assert_eq!(stored["signatures"][0]["policy"], "valid");
        assert_eq!(stored["trustMaterials"].as_array().unwrap().len(), 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_access_events WHERE action='document.verify'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert!(
            std::fs::read_dir(directory.path())
                .unwrap()
                .next()
                .is_none()
        );
    })
    .await;
}

#[tokio::test]
async fn authority_canonical_source_and_trust_changes_cannot_retain_a_valid_inspection() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, true, false).await;
        let directory = private_directory();
        let verifier = Arc::new(
            BoundedPdfVerifier::new(
                Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf")),
                directory.path(),
                PdfProcessLimits::default(),
            )
            .unwrap(),
        );
        let uow = PgUnitOfWork::new(pool.clone());
        for mode in [1, 2, 3, 4] {
            let verifier = verifier.clone();
            let result = uow
                .run::<(), BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let verifier = verifier.clone();
                    Box::pin(async move {
                        let change = Arc::new(AtomicU8::new(0));
                        let mut bridge = Bridge {
                            pdf: pdf(),
                            mode: change.clone(),
                        };
                        let mut ids = Identifiers {
                            prefix: "blocked",
                            next: 0,
                        };
                        let mut repository = PgPdfVerificationRepository::new(
                            HoldingId::new("holding-a").unwrap(),
                            CompanyId::new("company-a").unwrap(),
                            connection,
                            &mut bridge,
                            &FixedClock,
                            &mut ids,
                        );
                        let snapshot = repository.load_authorized(&job()).await?;
                        let evidence = verifier
                            .verify(snapshot, FixedClock.now())
                            .await
                            .map_err(PgTransactionError::<BusinessError>::from)?;
                        assert_eq!(evidence.report.status, ValidationState::Valid);
                        change.store(mode, Ordering::SeqCst);
                        repository.retain_if_current(&job(), evidence).await?;
                        Ok(())
                    })
                })
                .await;
            assert!(
                matches!(result, Err(PgTransactionError::Business(_))),
                "mode{mode}"
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legal_document_inspections")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legal_access_events")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_trust_material WHERE state='approved'"
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
async fn raw_object_key_and_self_approved_company_trust_are_refused_before_verification() {
    for (canonical, self_approved, expected) in [
        (false, false, "LEGAL_ATTACHMENT_LINK_REQUIRED"),
        (true, true, "TRUST_MATERIAL_NOT_INDEPENDENTLY_APPROVED"),
    ] {
        fixture::with_database(2,move |pool|async move {
            setup(&pool,canonical,self_approved).await;let uow=PgUnitOfWork::new(pool.clone());
            let result=uow.run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
                let mut bridge=Bridge{pdf:pdf(),mode:Arc::new(AtomicU8::new(0))};let mut ids=Identifiers{prefix:"never-retained",next:0};
                let mut repository=PgPdfVerificationRepository::new(HoldingId::new("holding-a").unwrap(),CompanyId::new("company-a").unwrap(),connection,&mut bridge,&FixedClock,&mut ids);
                repository.load_authorized(&job()).await?;Ok(())
            })).await;
            assert!(matches!(result,Err(PgTransactionError::Business(ref error)) if error.code==expected));
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_document_inspections").fetch_one(pool.raw()).await.unwrap(),0);
        }).await;
    }
}

struct UnavailableVerifier;
impl PdfVerifierProcess for UnavailableVerifier {
    async fn verify(
        &self,
        _: AuthorizedPdfSnapshot,
        _: TimestampMillis,
    ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
        Err(PdfVerificationFailure::WorkerUnavailable)
    }
}

#[tokio::test]
async fn committed_job_lease_actual_pdf_inspection_and_completion_share_the_current_owner_unit() {
    fixture::with_database(2,|pool|async move {
        setup(&pool,true,false).await;
        let directory=private_directory();
        let verifier=Arc::new(BoundedPdfVerifier::new(Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf")),directory.path(),PdfProcessLimits::default()).unwrap());
        let uow=PgUnitOfWork::new(pool.clone());
        for (name,tool_unavailable) in [("queued-pdf",false),("queued-failed-tool",true)] {
            sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,actor_json,created_by,created_at)SELECT $1,'company-a','item','document',checksum,'verify','queued','{}','requester',0 FROM legal_documents WHERE id='document'").bind(name).execute(pool.raw()).await.unwrap();
            let lease=uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
                let mut bridge=Bridge{pdf:pdf(),mode:Arc::new(AtomicU8::new(0))};let mut ids=Identifiers{prefix:"claimed-job",next:0};
                Ok(PgLegalProcessingRepository::new(HoldingId::new("holding-a").unwrap(),CompanyId::new("company-a").unwrap(),connection,&mut bridge,&FixedClock,&mut ids).claim(None).await?.lease.unwrap())
            })).await.unwrap();
            assert_eq!(lease.job.id.as_str(),name);
            let verifier=verifier.clone();
            let result=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|connection| {
                let lease=lease.clone();let verifier=verifier.clone();Box::pin(async move {
                    let mut bridge=Bridge{pdf:pdf(),mode:Arc::new(AtomicU8::new(0))};let mut ids=Identifiers{prefix:if tool_unavailable{"tool-failed"}else{"real-job"},next:0};
                    let source=load_current_legal_processing_source(connection,LegalProcessingSourceRequest{holding_id:HoldingId::new("holding-a").unwrap(),company_id:CompanyId::new("company-a").unwrap(),job_id:lease.job.id.clone(),lease_token:lease.token.clone()},FixedClock.now().get()).await?;
                    source.require_current_or_completed(connection,FixedClock.now().get()).await?;
                    let result=if tool_unavailable {inspect_current_legal_pdf(connection,&mut bridge,&UnavailableVerifier,&FixedClock,&mut ids,&source).await?}
                    else {inspect_current_legal_pdf(connection,&mut bridge,&*verifier,&FixedClock,&mut ids,&source).await?};
                    assert!(source.require_current(connection,FixedClock.now().get()).await.is_err(),"strict running guard must not broaden silently");
                    source.require_current_or_completed(connection,FixedClock.now().get()).await?;
                    Ok(result)
                })
            }).await.unwrap();
            assert_eq!(result["status"],if tool_unavailable{"indeterminate"}else{"valid"});
            assert_eq!(result["processingJobId"],name);
            let persisted:(String,String)=sqlx::query_as("SELECT state,result_json FROM legal_processing_jobs WHERE id=$1").bind(name).fetch_one(pool.raw()).await.unwrap();
            assert_eq!(persisted.0,"completed");assert_eq!(serde_json::from_str::<serde_json::Value>(&persisted.1).unwrap(),result);
            if tool_unavailable {assert!(result["signatures"].as_array().unwrap().is_empty());assert_eq!(result["trustStoreDigest"],"");assert!(result["reasons"].as_array().unwrap().iter().any(|reason|reason=="VERIFIER_UNAVAILABLE"));}
            else {assert_eq!(result["signatures"][0]["requiredCertificateChain"],"valid");}
        }
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_document_inspections").fetch_one(pool.raw()).await.unwrap(),2);
    }).await;
}

struct OffsetClock(i64);
impl Clock for OffsetClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(FixedClock.now().get() + self.0)
    }
}

struct WritingVerifier {
    inner: BoundedPdfVerifier,
    pool: DatabasePool,
    mutation: &'static str,
}
impl PdfVerifierProcess for WritingVerifier {
    async fn verify(
        &self,
        snapshot: AuthorizedPdfSnapshot,
        moment: TimestampMillis,
    ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
        let retained:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND state='idle in transaction'").fetch_one(self.pool.raw()).await.unwrap();
        assert_eq!(
            retained, 0,
            "no owner transaction may remain during computation"
        );
        let writer = async {
            match self.mutation {
                "trust" => {
                    sqlx::query("UPDATE legal_trust_material SET state='retired',version=version+1 WHERE kind='root'").execute(self.pool.raw()).await.unwrap();
                }
                "source" => {
                    sqlx::query("UPDATE attachments SET source_state='destroyed',destroyed_at=1 WHERE id='source'")
                        .execute(self.pool.raw())
                        .await
                        .unwrap();
                }
                _ => {}
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), writer)
            .await
            .expect("current source/trust writers were blocked by computation");
        self.inner.verify(snapshot, moment).await
    }
}

#[tokio::test]
async fn split_pdf_computation_releases_locks_and_remints_current_source_and_trust_before_retention()
 {
    for mutation in ["allowed-authority-token-change", "trust", "source"] {
        fixture::with_database(2,move |pool|async move {
            setup(&pool,true,false).await;
            sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,actor_json,created_by,created_at)SELECT 'split-job','company-a','item','document',checksum,'verify','queued','{}','requester',0 FROM legal_documents WHERE id='document'").execute(pool.raw()).await.unwrap();
            let uow=PgUnitOfWork::new(pool.clone());
            let lease=uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
                let mut bridge=Bridge{pdf:pdf(),mode:Arc::new(AtomicU8::new(0))};let mut ids=Identifiers{prefix:"split-lease",next:0};
                Ok(PgLegalProcessingRepository::new(HoldingId::new("holding-a").unwrap(),CompanyId::new("company-a").unwrap(),connection,&mut bridge,&FixedClock,&mut ids).claim(None).await?.lease.unwrap())
            })).await.unwrap();
            let prepared=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move |connection| {
                let lease=lease.clone();Box::pin(async move {
                    let source=load_current_legal_processing_source(connection,LegalProcessingSourceRequest{holding_id:lease.job.holding_id.clone(),company_id:lease.job.company_id.clone(),job_id:lease.job.id.clone(),lease_token:lease.token.clone()},FixedClock.now().get()).await?;
                    let mut bridge=Bridge{pdf:pdf(),mode:Arc::new(AtomicU8::new(0))};let mut ids=Identifiers{prefix:"prepared-only",next:0};
                    prepare_current_legal_pdf(connection,&mut bridge,&FixedClock,&mut ids,&source).await
                })
            }).await.unwrap();
            let directory=private_directory();
            let verifier=WritingVerifier{inner:BoundedPdfVerifier::new(Path::new(env!("CARGO_BIN_EXE_erpx-validate-pdf")),directory.path(),PdfProcessLimits::default()).unwrap(),pool:pool.clone(),mutation};
            // This call has no database/authority/source witness argument.
            let computed=Arc::new(compute_prepared_legal_pdf(prepared,&verifier,&OffsetClock(5_000)).await.unwrap());
            assert_eq!(computed.report().status,ValidationState::Valid,"dated computation uses its original supplied input");
            let retained=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move |connection| {
                let computed=computed.clone();Box::pin(async move {
                    let lease=computed.lease();
                    let source=load_current_legal_processing_source(connection,LegalProcessingSourceRequest{holding_id:lease.job.holding_id.clone(),company_id:lease.job.company_id.clone(),job_id:lease.job.id.clone(),lease_token:lease.token.clone()},OffsetClock(6_000).now().get()).await?;
                    let mut bridge=Bridge{pdf:pdf(),mode:Arc::new(AtomicU8::new(if mutation=="allowed-authority-token-change"{2}else{0}))};let mut ids=Identifiers{prefix:"split-retained",next:0};
                    let result=retain_current_legal_pdf(connection,&mut bridge,&OffsetClock(6_000),&mut ids,&source,&computed).await?;
                    source.require_current_or_completed(connection,OffsetClock(6_000).now().get()).await?;
                    Ok(result)
                })
            }).await;
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM legal_document_inspections").fetch_one(pool.raw()).await.unwrap();
            if mutation=="allowed-authority-token-change" {
                let retained=retained.unwrap();assert_eq!(count,1);assert_eq!(retained["status"],"valid");assert_eq!(retained["computedInput"]["preparedAt"],FixedClock.now().get());assert_eq!(retained["validatedAt"],"2026-10-27T06:00:05Z");
                assert!(!retained.to_string().contains("erpx-pdf-prepared-data-v1"));
                let created:i64=sqlx::query_scalar("SELECT created_at FROM legal_document_inspections").fetch_one(pool.raw()).await.unwrap();assert_eq!(created,OffsetClock(6_000).now().get());
            } else {
                assert!(matches!(retained,Err(PgTransactionError::Business(ref error))if ["PDF_VERIFICATION_SNAPSHOT_CHANGED","FILE_SOURCE_CHANGED"].contains(&error.code)));assert_eq!(count,0);
                assert_eq!(sqlx::query_scalar::<_,String>("SELECT state FROM legal_processing_jobs WHERE id='split-job'").fetch_one(pool.raw()).await.unwrap(),"running");
            }
        }).await;
    }
}
