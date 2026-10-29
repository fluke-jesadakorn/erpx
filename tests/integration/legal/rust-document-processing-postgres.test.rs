#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_legal::public::{document_processing::*, *};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis,
};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicU8, AtomicU64, Ordering},
    },
};
type Error = PgTransactionError<BusinessError>;
const NOW: i64 = 1_793_080_800_000;
struct TestClock(AtomicI64);
impl Clock for TestClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0.load(Ordering::SeqCst))
    }
}
#[derive(Default)]
struct Ids(u32);
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        self.0 += 1;
        id(&format!("effect-{}", self.0))
    }
}
fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
fn request() -> LegalProcessingSourceRequest {
    LegalProcessingSourceRequest {
        holding_id: HoldingId::new("holding-a").unwrap(),
        company_id: CompanyId::new("company-a").unwrap(),
        job_id: id("job"),
        lease_token: id("lease"),
    }
}
fn denied() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "TEST_CURRENT_AUTHORITY_REVOKED",
        "Synthetic owner bridge withdrew its current permission.",
    )
    .into()
}

/// Explicit synthetic foreign-owner ports. Real Identity/Files/Commercial
/// composition is separately tested by the service owner, never claimed here.
#[derive(Clone)]
struct Bridge {
    bytes: Arc<Vec<u8>>,
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
    ) -> Result<LegalProcessingQueueAuthority, Error> {
        Err(denied())
    }
    async fn replay(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &RecordId,
        _: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, Error> {
        Err(denied())
    }
    async fn record_queued(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
        _: &LegalProcessingReceipt,
    ) -> Result<(), Error> {
        Err(denied())
    }
    async fn candidate_eligible(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingJob,
    ) -> Result<bool, Error> {
        Ok(self.mode.load(Ordering::SeqCst) != 1)
    }
    async fn authorize_completion(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingLease,
    ) -> Result<(), Error> {
        if self.mode.load(Ordering::SeqCst) == 1 {
            Err(denied())
        } else {
            Ok(())
        }
    }
    async fn authorize_status(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &RecordId,
    ) -> Result<(), Error> {
        Err(denied())
    }
}
impl PgLegalDocumentExtractionBridge for Bridge {
    async fn authorize_document(
        &mut self,
        _: &mut PgConnection,
        source: &CurrentLegalProcessingSource,
        document: &LegalInspectionDocument,
    ) -> Result<String, Error> {
        assert_eq!(source.company_id(), &document.company_id);
        if self.mode.load(Ordering::SeqCst) == 1 {
            return Err(denied());
        }
        Ok(format!(
            "fresh-authority-{}",
            self.mode.load(Ordering::SeqCst)
        ))
    }
    async fn read_original(
        &mut self,
        c: &mut PgConnection,
        _: &CurrentLegalProcessingSource,
        _: &LegalInspectionDocument,
        max: usize,
    ) -> Result<LegalOriginalRead, Error> {
        assert!(self.bytes.len() <= max);
        let (revision, state): (i64, String) = sqlx::query_as(
            "SELECT revision::bigint,source_state FROM attachments WHERE id='source' FOR SHARE",
        )
        .fetch_one(c)
        .await?;
        if state != "available" {
            return Err(denied());
        }
        Ok(LegalOriginalRead {
            bytes: self.bytes.as_ref().clone(),
            witness: "local-files-proof".into(),
            source_data_sha256: hex::encode(Sha256::digest(format!(
                "synthetic-original/v1:{revision}:{state}"
            ))),
        })
    }
    async fn revalidate_original(
        &mut self,
        c: &mut PgConnection,
        _: &CurrentLegalProcessingSource,
        _: &LegalInspectionDocument,
        witness: &str,
    ) -> Result<(), Error> {
        assert_eq!(witness, "local-files-proof");
        let available: bool = sqlx::query_scalar(
            "SELECT source_state='available' FROM attachments WHERE id='source' FOR SHARE",
        )
        .fetch_one(c)
        .await?;
        if available { Ok(()) } else { Err(denied()) }
    }
}
fn docx() -> Vec<u8> {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/legal/documents/docx-conformance.v1.json"
    ))
    .unwrap();
    hex::decode(corpus["cases"][0]["docxHex"].as_str().unwrap()).unwrap()
}
async fn setup(pool: &DatabasePool, original: bool) -> Bridge {
    setup_with_raw(pool, original, false).await
}
async fn setup_with_raw(pool: &DatabasePool, original: bool, raw: bool) -> Bridge {
    let migrations = load_migrations(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    let bytes = if original {
        docx()
    } else {
        "ข้อความร่าง A & B 😀".as_bytes().to_vec()
    };
    let checksum = hex::encode(Sha256::digest(&bytes));
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status) VALUES('company-a','company','synthetic','Synthetic Company','company-a','active');INSERT INTO legal_settings(company_id,enabled) VALUES('company-a',true);INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at) VALUES('item','company-a','contract','Synthetic contract','internal','TH','TH','en','THB','Asia/Bangkok','normal','requester','requester',0,0)").execute(pool.raw()).await.unwrap();
    let mime = if original {
        LegalDocumentMediaType::Docx.as_mime()
    } else {
        "text/plain"
    };
    if original {
        sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at) VALUES('upload','legal','item','legal_document','fixture.docx',$1,$2,$2,$3,'fixture-stage','fixture-object','ready','requester',9999999999999,0,0)").bind(mime).bind(bytes.len() as i64).bind(&checksum).execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES('source','upload','legal','item','legal_document','fixture-object',1,'fixture.docx',$1,$2,$3,'requester',0,'company-a',$1,'available',0,0)").bind(mime).bind(bytes.len() as i64).bind(&checksum).execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at,bound_at,bound_command_id) VALUES('set','company-a','legal_item','item','record','legal_document',1,1,'internal','{}','bound','requester',0,0,0,'binding');INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) VALUES('link','set','source','bound','requester',0,9999999999999,0,'legal_document','document')").execute(pool.raw()).await.unwrap();
    }
    sqlx::query("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,object_key,created_by,created_at,attachment_link_id,attachment_source_id) VALUES('document','item',1,'fixture',$1,$2,$3,$4,$5,'requester',0,$6,$7)").bind(mime).bind(&checksum).bind(bytes.len() as i64).bind(if original{""}else{"ข้อความร่าง A & B 😀"}).bind(if original||raw{Some("fixture-object")}else{None}).bind(original.then_some("link")).bind(original.then_some("source")).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,attempts,lease_token,lease_until,actor_json,created_by,created_at) VALUES('job','company-a','item','document',$1,'ocr','running',1,'lease',$2,'{}','requester',$3)").bind(checksum).bind(NOW+600_000).bind(NOW).execute(pool.raw()).await.unwrap();
    Bridge {
        bytes: Arc::new(bytes),
        mode: Arc::new(AtomicU8::new(0)),
    }
}
async fn prepare(
    pool: &DatabasePool,
    bridge: Bridge,
    clock: Arc<TestClock>,
) -> PreparedLegalExtraction {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let mut bridge = bridge.clone();
            let clock = clock.clone();
            Box::pin(async move {
                let source =
                    load_current_legal_processing_source(c, request(), clock.now().get()).await?;
                prepare_current_legal_extraction(
                    c,
                    &mut bridge,
                    clock.as_ref(),
                    &mut Ids::default(),
                    &source,
                )
                .await
            })
        })
        .await
        .unwrap()
}
async fn retain(
    pool: &DatabasePool,
    bridge: Bridge,
    clock: Arc<TestClock>,
    computed: ComputedLegalExtraction,
) -> Result<LegalExtractionRetention, Error> {
    let computed = Arc::new(computed);
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let mut bridge = bridge.clone();
            let clock = clock.clone();
            let computed = computed.clone();
            Box::pin(async move {
                let source =
                    load_current_legal_processing_source(c, request(), clock.now().get()).await?;
                let result = retain_current_legal_extraction(
                    c,
                    &mut bridge,
                    clock.as_ref(),
                    &mut Ids::default(),
                    &source,
                    &computed,
                )
                .await?;
                if let LegalExtractionRetention::Failed { .. } = &result {
                    assert!(
                        source
                            .require_current_or_completed(c, clock.now().get())
                            .await
                            .is_err(),
                        "the completed-only guard must not silently accept failed rows"
                    );
                }
                source
                    .require_current_or_settled(c, clock.now().get())
                    .await?;
                Ok(result)
            })
        })
        .await
}
fn clock() -> Arc<TestClock> {
    Arc::new(TestClock(AtomicI64::new(NOW)))
}
fn extractor(root: &Path) -> BoundedDocumentExtractor {
    let unused = Path::new(env!("CARGO_BIN_EXE_erpx-extract-document")).to_path_buf();
    BoundedDocumentExtractor::new(
        &unused,
        root,
        DocumentToolConfiguration {
            pdftotext: unused.clone(),
            pdfinfo: unused.clone(),
            pdftoppm: unused.clone(),
            tesseract: unused.clone(),
            tessdata: root.into(),
            languages: "eng+tha".into(),
        },
        DocumentProcessLimits::default(),
    )
    .unwrap()
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
async fn counts(pool: &DatabasePool) -> (i64, i64, String) {
    sqlx::query_as("SELECT (SELECT count(*) FROM legal_document_inspections),(SELECT count(*) FROM legal_access_events),(SELECT state FROM legal_processing_jobs WHERE id='job')").fetch_one(pool.raw()).await.unwrap()
}
struct FailedExtractor {
    budget: Arc<AtomicU64>,
}
impl DocumentExtractor for FailedExtractor {
    async fn extract_with_budget(
        &self,
        _: DocumentExtractionInput,
        budget: DocumentComputeBudget,
    ) -> Result<DocumentExtractionReport, DocumentExtractionFailure> {
        self.budget.store(budget.millis(), Ordering::SeqCst);
        Err(DocumentExtractionFailure::ToolTimeout)
    }
}

#[tokio::test]
async fn actual_rust_docx_and_explicit_inline_inputs_retain_atomic_review_only_results() {
    for original in [false, true] {
        fixture::with_database(3,move|pool|async move{
        let bridge=setup(&pool,original).await;let clock=clock();let prepared=prepare(&pool,bridge.clone(),clock.clone()).await;
        let idle:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND xact_start IS NOT NULL AND pid<>pg_backend_pid()").fetch_one(pool.raw()).await.unwrap();assert_eq!(idle,0);
        // A writer commits between phases; the old transaction-local authority
        // token is not reused as the data identity of the final read.
        sqlx::query("UPDATE legal_settings SET enabled=true WHERE company_id='company-a'").execute(pool.raw()).await.unwrap();bridge.mode.store(2,Ordering::SeqCst);
        let directory=private_directory();let computed=compute_prepared_legal_extraction(prepared,&extractor(directory.path()),clock.as_ref()).await.unwrap();assert!(computed.report().unwrap().requires_review);
        clock.0.store(NOW+1_000,Ordering::SeqCst);
        let result=retain(&pool,bridge,clock,computed).await.unwrap();let LegalExtractionRetention::Completed(result)=result else{panic!("expected success")};
        assert_eq!(result["requiresReview"],true);assert_eq!(result["processingJobId"],"job");assert_eq!(result["computedInput"]["preparedAt"],NOW);assert_eq!(result["computedInput"]["computedAt"],NOW);assert_eq!(counts(&pool).await,(1,1,"completed".into()));
        let retained_at:i64=sqlx::query_scalar("SELECT created_at FROM legal_document_inspections").fetch_one(pool.raw()).await.unwrap();assert_eq!(retained_at,NOW+1_000);
        let action:String=sqlx::query_scalar("SELECT action FROM legal_access_events").fetch_one(pool.raw()).await.unwrap();assert_eq!(action,"document.ocr");
        let chunks:i64=sqlx::query_scalar("SELECT count(*) FROM legal_source_chunks").fetch_one(pool.raw()).await.unwrap();assert_eq!(chunks,0);
    }).await;
    }
}
#[tokio::test]
async fn remaining_lease_budget_and_terminal_failure_never_retain_partial_text_or_auto_retry() {
    fixture::with_database(3, |pool| async move {
        let bridge = setup(&pool, false).await;
        let clock = clock();
        let prepared = prepare(&pool, bridge.clone(), clock.clone()).await;
        clock.0.store(NOW + 535_000, Ordering::SeqCst);
        let budget = Arc::new(AtomicU64::new(0));
        let computed = compute_prepared_legal_extraction(
            prepared,
            &FailedExtractor {
                budget: budget.clone(),
            },
            clock.as_ref(),
        )
        .await
        .unwrap();
        assert_eq!(budget.load(Ordering::SeqCst), 5_000);
        assert!(computed.report().is_none());
        let result = retain(&pool, bridge.clone(), clock.clone(), computed)
            .await
            .unwrap();
        assert!(matches!(
            result,
            LegalExtractionRetention::Failed {
                retryable: true,
                ..
            }
        ));
        assert_eq!(counts(&pool).await, (0, 0, "failed".into()));
        let stored: (String, Option<String>) =
            sqlx::query_as("SELECT error,result_json FROM legal_processing_jobs WHERE id='job'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(
            stored,
            (DocumentExtractionFailure::ToolTimeout.code().into(), None)
        );
        let recovered = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let mut bridge = bridge.clone();
                let clock = clock.clone();
                Box::pin(async move {
                    PgLegalProcessingRepository::new(
                        HoldingId::new("holding-a").unwrap(),
                        CompanyId::new("company-a").unwrap(),
                        c,
                        &mut bridge,
                        clock.as_ref(),
                        &mut Ids::default(),
                    )
                    .recover_expired()
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(recovered, 0);
    })
    .await;
}
#[tokio::test]
async fn fresh_authority_source_and_preservation_changes_refuse_old_computation() {
    for change in ["authority", "source", "preservation"] {
        fixture::with_database(3,move|pool|async move{let bridge=setup(&pool,true).await;let clock=clock();let prepared=prepare(&pool,bridge.clone(),clock.clone()).await;let directory=private_directory();let computed=compute_prepared_legal_extraction(prepared,&extractor(directory.path()),clock.as_ref()).await.unwrap();
        match change{"authority"=>bridge.mode.store(1,Ordering::SeqCst),"source"=>{sqlx::query("UPDATE attachments SET source_state='destroyed',destroyed_at=$1 WHERE id='source'").bind(NOW).execute(pool.raw()).await.unwrap();},_=>{sqlx::query("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at)VALUES('destruction','item','synthetic-policy',1,'requester','executing','Synthetic denial','[]',$1)").bind(NOW).execute(pool.raw()).await.unwrap();}}
        assert!(retain(&pool,bridge,clock,computed).await.is_err());assert_eq!(counts(&pool).await,(0,0,"running".into()));
    }).await;
    }
}
#[tokio::test]
async fn raw_object_key_and_unproved_sql_failure_cannot_mint_an_owner_settlement() {
    fixture::with_database(3,|pool|async move{let bridge=setup_with_raw(&pool,false,true).await;let clock=clock();
        let forged=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{let source=load_current_legal_processing_source(c,request(),NOW).await?;sqlx::query("UPDATE legal_processing_jobs SET state='failed',error='DOCUMENT_EXTRACT_TOOL_TIMEOUT',result_json=NULL,lease_token=NULL,lease_until=NULL WHERE id='job'").execute(&mut *c).await?;source.require_current_or_settled(c,NOW).await})).await;assert!(forged.is_err());assert_eq!(counts(&pool).await.2,"running");
        let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let mut bridge=bridge.clone();let clock=clock.clone();Box::pin(async move{let source=load_current_legal_processing_source(c,request(),NOW).await?;prepare_current_legal_extraction(c,&mut bridge,clock.as_ref(),&mut Ids::default(),&source).await.map(|_|())})}).await;assert!(result.is_err());assert_eq!(counts(&pool).await,(0,0,"running".into()));
    }).await;
}
#[tokio::test]
async fn inspection_audit_and_completion_rollback_together_on_a_real_database_denial() {
    fixture::with_database(3,|pool|async move{let bridge=setup(&pool,false).await;let clock=clock();let prepared=prepare(&pool,bridge.clone(),clock.clone()).await;let directory=private_directory();let computed=compute_prepared_legal_extraction(prepared,&extractor(directory.path()),clock.as_ref()).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_deny_ocr_audit() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'fixture audit denial' USING ERRCODE='23514'; END$$;CREATE TRIGGER fixture_deny_ocr_audit BEFORE INSERT ON legal_access_events FOR EACH ROW EXECUTE FUNCTION fixture_deny_ocr_audit()").execute(pool.raw()).await.unwrap();
        assert!(retain(&pool,bridge,clock,computed).await.is_err());assert_eq!(counts(&pool).await,(0,0,"running".into()));
    }).await;
}
#[tokio::test]
async fn a_partial_or_indeterminate_provider_report_cannot_become_a_successful_ocr_inspection() {
    struct Partial;
    impl DocumentExtractor for Partial {
        async fn extract_with_budget(
            &self,
            input: DocumentExtractionInput,
            _: DocumentComputeBudget,
        ) -> Result<DocumentExtractionReport, DocumentExtractionFailure> {
            let mut report = unresolved_document_extraction(
                &input.checksum_sha256,
                DocumentExtractionFailure::ToolTimeout,
            );
            report.text = "first partial page".into();
            report.method = Some(ExtractionMethod::Inline);
            Ok(report)
        }
    }
    fixture::with_database(3, |pool| async move {
        let bridge = setup(&pool, false).await;
        let clock = clock();
        let prepared = prepare(&pool, bridge.clone(), clock.clone()).await;
        let computed = compute_prepared_legal_extraction(prepared, &Partial, clock.as_ref())
            .await
            .unwrap();
        assert_eq!(
            computed.failure(),
            Some(DocumentExtractionFailure::WorkerProtocol)
        );
        let result = retain(&pool, bridge, clock, computed).await.unwrap();
        assert!(matches!(result, LegalExtractionRetention::Failed { .. }));
        assert_eq!(counts(&pool).await, (0, 0, "failed".into()));
    })
    .await;
}
