#[path = "../../support/rust-postgres.rs"]
mod fixture;

use base64::{Engine, engine::general_purpose::STANDARD};
use erpx_server_legal::public::{
    BoundedPdfVerifier, LegalInspectionDocument, LegalOriginalRead, PdfProcessLimits,
    PdfVerificationJob, PdfVerificationRepository, PdfVerifierProcess,
    PgLegalPdfVerificationBridge, PgPdfVerificationRepository, ValidationState, verify_company_pdf,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
    TimestampMillis, UserId,
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
        _: &mut PgConnection,
        _: &PdfVerificationJob,
        document: &LegalInspectionDocument,
        max: usize,
    ) -> Result<LegalOriginalRead, PgTransactionError<BusinessError>> {
        assert!(document.attachment_link_id.is_some());
        assert!(self.pdf.len() <= max);
        Ok(LegalOriginalRead {
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
