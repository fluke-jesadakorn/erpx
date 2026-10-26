#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use auth::*;
use erpx_server_identity_access::public::effective_authority::{
    HoldingWorkAuthorizer, PgQueuedWorkAuthorizer, QueuedWorkBinding, WorkAuthorizationRequest,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId};
use erpx_service::legal_queue_source::LegalQueuedSource;
use std::sync::atomic::Ordering;
fn source() -> LegalQueuedSource {
    LegalQueuedSource::new(
        HoldingId::new("h001").unwrap(),
        CompanyId::new("company").unwrap(),
        RecordId::new("legal-job").unwrap(),
        RecordId::new("lease").unwrap(),
    )
}
fn legal_access() -> WorkAuthorizationRequest {
    let mut request = auth::access();
    request.permission = "legal.read".into();
    request.module = Some("legal".into());
    request.destination = "legal/processing".into();
    request
}
async fn seed_native_legal_job(authorizer: &HoldingWorkAuthorizer, pool: &DatabasePool) {
    use sha2::{Digest, Sha256};
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"finance.write\",\"module.finance.read\",\"legal.read\",\"module.legal.read\"]','[]','[\"holding\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'").execute(pool.raw()).await.unwrap();
    authorizer.execute("acme.example.test",proof(),input(),legal_access(),|connection,current|Box::pin(async move{
        sqlx::raw_sql("INSERT INTO legal_settings(company_id,enabled) VALUES('company',true);INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at) VALUES('legal-item','company','contract','Synthetic','internal','TH','TH','en','THB','Asia/Bangkok','normal','user','user',0,0)").execute(&mut *connection).await?;
        let checksum=format!("{:x}",Sha256::digest(b"x"));
        sqlx::query("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at) VALUES('legal-document','legal-item',1,'synthetic.pdf','application/pdf',$1,1,'x','user',0)").bind(&checksum).execute(&mut *connection).await?;
        let snapshot=QueuedWorkBinding::from_current(current).to_json()?;
        sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,attempts,lease_token,lease_until,actor_json,created_by,created_at) VALUES('legal-job','company','legal-item','legal-document',$1,'verify','running',1,'lease',$2,$3,'user',0)").bind(checksum).bind(NOW+10000).bind(snapshot).execute(connection).await?;Ok(())
    })).await.unwrap();
}
#[tokio::test]
async fn concrete_legal_owner_source_and_current_identity_are_both_required() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, clock) = setup(&control, &holding).await;
            seed_native_legal_job(authorizer.as_ref(), &holding).await;
            let queued = PgQueuedWorkAuthorizer::new(
                holding.clone(),
                HoldingId::new("h001").unwrap(),
                clock,
            );
            queued
                .execute(source(), legal_access(), |connection, current, owned| {
                    Box::pin(async move {
                        assert_eq!(owned.job().id.as_str(), "legal-job");
                        assert!(current.has_permission("legal.read"));
                        assert_eq!(current.user_id(), owned.user_id());
                        sqlx::query("INSERT INTO native_owner_effects VALUES('actual-source')")
                            .execute(connection)
                            .await?;
                        Ok(())
                    })
                })
                .await
                .unwrap();
            let original: String = sqlx::query_scalar(
                "SELECT actor_json FROM legal_processing_jobs WHERE id='legal-job'",
            )
            .fetch_one(holding.raw())
            .await
            .unwrap();
            for (field, value) in [
                ("holdingId", serde_json::json!("foreign")),
                ("companyId", serde_json::json!("foreign")),
                ("userId", serde_json::json!("foreign")),
                ("role", serde_json::json!("holding_owner")),
                ("roleRevision", serde_json::json!(99)),
            ] {
                let mut forged: serde_json::Value = serde_json::from_str(&original).unwrap();
                forged[field] = value;
                sqlx::query("UPDATE legal_processing_jobs SET actor_json=$1 WHERE id='legal-job'")
                    .bind(forged.to_string())
                    .execute(holding.raw())
                    .await
                    .unwrap();
                let denied = queued
                    .execute(source(), legal_access(), |_, _, _| {
                        Box::pin(async {
                            panic!("foreign owner snapshot reached worker action");
                            #[allow(unreachable_code)]
                            Ok(())
                        })
                    })
                    .await;
                assert!(
                    matches!(denied, Err(PgTransactionError::Business(_))),
                    "forged {field} should deny before worker action"
                );
            }
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM native_owner_effects")
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                1
            );
        })
        .await;
    })
    .await;
}
#[tokio::test]
async fn concrete_source_expiry_and_unstamped_completed_state_roll_back_cross_owner_effects() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
    let(authorizer,clock)=setup(&control,&holding).await;seed_native_legal_job(authorizer.as_ref(),&holding).await;
    let queued=PgQueuedWorkAuthorizer::new(holding.clone(),HoldingId::new("h001").unwrap(),clock.clone());
    let advancing=clock.clone();let expired=queued.execute(source(),legal_access(),move|connection,_,_|{let clock=advancing.clone();Box::pin(async move{sqlx::query("INSERT INTO native_owner_effects VALUES('expired')").execute(connection).await?;clock.0.store(NOW+10001,Ordering::SeqCst);Ok(())})}).await;assert!(matches!(expired,Err(PgTransactionError::Business(_))));clock.0.store(NOW,Ordering::SeqCst);
    let unstamped=queued.execute(source(),legal_access(),|connection,_,_|Box::pin(async move{sqlx::query("INSERT INTO native_owner_effects VALUES('unstamped')").execute(&mut *connection).await?;sqlx::query("UPDATE legal_processing_jobs SET state='completed',lease_token=NULL,lease_until=NULL WHERE id='legal-job'").execute(connection).await?;Ok(())})).await;assert!(matches!(unstamped,Err(PgTransactionError::Business(_))));
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT state FROM legal_processing_jobs WHERE id='legal-job'").fetch_one(holding.raw()).await.unwrap(),"running");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);
 }).await;}).await;
}

#[tokio::test]
async fn compute_between_short_transactions_retains_no_authority_leases_and_rejects_a_changed_role()
{
    use std::sync::Arc;
    use std::time::Duration;
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
        let(authorizer,clock)=setup(&control,&holding).await;seed_native_legal_job(authorizer.as_ref(),&holding).await;
        let queued=PgQueuedWorkAuthorizer::new(holding.clone(),HoldingId::new("h001").unwrap(),clock);
        // Prepared data is deliberately a job/input descriptor, not an opaque
        // owner witness, connection, authority context or previous file witness.
        let prepared=queued.execute(source(),legal_access(),|_,_,owned|Box::pin(async move{Ok((owned.job().id.clone(),owned.job().document.checksum_sha256.clone()))})).await.unwrap();
        let entered=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());
        let compute={let entered=entered.clone();let release=release.clone();tokio::spawn(async move{entered.notify_one();release.notified().await;prepared})};
        tokio::time::timeout(Duration::from_secs(3),entered.notified()).await.unwrap();
        let writes=async {
            sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',3,'[\"legal.read\",\"module.legal.read\"]','[]','[\"holding\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
            sqlx::query("UPDATE account_device_browsers SET revision=revision+1 WHERE id='browser'").execute(control.raw()).await.unwrap();
        };
        assert!(tokio::time::timeout(Duration::from_secs(3),writes).await.is_ok(),"a compute phase retained IAM or Control source locks");
        for pool in [&control,&holding]{assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND xact_start IS NOT NULL)").fetch_one(pool.raw()).await.unwrap(),"compute retained a database transaction");}
        release.notify_one();let computed=compute.await.unwrap();assert_eq!(computed.0.as_str(),"legal-job");
        let finalization=queued.execute(source(),legal_access(),|connection,_,_|Box::pin(async move{sqlx::query("INSERT INTO native_owner_effects VALUES('stale-computed-result')").execute(connection).await?;Ok(())})).await;
        assert!(matches!(finalization,Err(PgTransactionError::Business(_))),"a computed input descriptor became current authority");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);
    }).await;}).await;
}

#[cfg(feature = "legal-pdf-worker")]
mod split_worker {
    use super::*;
    use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
    use erpx_server_legal::public::*;
    use erpx_server_runtime::public::postgres::PgTransactionError;
    use erpx_server_shared::public::{
        BusinessError, CommandKey, FailureKind, IdentifierSource, TimestampMillis, UserId,
    };
    use erpx_service::legal_pdf_worker::{LegalPdfBridgeFactory, run_legal_pdf_short_transactions};
    use sqlx::PgConnection;
    use std::sync::{Arc, atomic::AtomicUsize};
    type Error = PgTransactionError<BusinessError>;
    fn unused() -> Error {
        BusinessError::new(
            FailureKind::Unavailable,
            "EXPLICIT_SYNTHETIC_PORT",
            "This fixture does not configure this owner operation.",
        )
        .into()
    }
    // Explicit synthetic record/purchase ports test orchestration only. They do
    // not attest to a production record policy, purchase or signature result.
    #[derive(Clone)]
    struct FixtureFactory {
        next: Arc<AtomicUsize>,
    }
    struct FixtureBridge<'c> {
        current: &'c AuthorizedWorkContext,
    }
    struct FixtureIds(Arc<AtomicUsize>);
    impl IdentifierSource for FixtureIds {
        fn next(&mut self) -> RecordId {
            RecordId::new(format!(
                "composition-inspection-{}",
                self.0.fetch_add(1, Ordering::SeqCst)
            ))
            .unwrap()
        }
    }
    impl LegalPdfBridgeFactory for FixtureFactory {
        type Bridge<'c> = FixtureBridge<'c>;
        type Identifiers = FixtureIds;
        fn bridge<'c>(&'c self, current: &'c AuthorizedWorkContext) -> Self::Bridge<'c> {
            FixtureBridge { current }
        }
        fn identifiers(&self) -> FixtureIds {
            FixtureIds(self.next.clone())
        }
    }
    impl PgLegalProcessingBridge for FixtureBridge<'_> {
        async fn authorize_queue(
            &mut self,
            _: &mut PgConnection,
            _: &LegalProcessingActor,
            _: &CommandKey,
            _: &LegalProcessingDocument,
            _: LegalProcessingKind,
        ) -> Result<LegalProcessingQueueAuthority, Error> {
            Err(unused())
        }
        async fn replay(
            &mut self,
            _: &mut PgConnection,
            _: &LegalProcessingActor,
            _: &CommandKey,
            _: &RecordId,
            _: LegalProcessingKind,
        ) -> Result<Option<LegalProcessingReceipt>, Error> {
            Err(unused())
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
            Err(unused())
        }
        async fn candidate_eligible(
            &mut self,
            _: &mut PgConnection,
            _: &LegalProcessingJob,
        ) -> Result<bool, Error> {
            Err(unused())
        }
        async fn authorize_status(
            &mut self,
            _: &mut PgConnection,
            _: &LegalProcessingActor,
            _: &RecordId,
        ) -> Result<(), Error> {
            Err(unused())
        }
        async fn authorize_completion(
            &mut self,
            _: &mut PgConnection,
            lease: &LegalProcessingLease,
        ) -> Result<(), Error> {
            self.current.require_permission("legal.read")?;
            if self.current.user_id() != &lease.job.created_by
                || self.current.selection().workspace().company_id != lease.job.company_id
            {
                return Err(unused());
            }
            Ok(())
        }
    }
    impl PgLegalPdfVerificationBridge for FixtureBridge<'_> {
        async fn authorize_document(
            &mut self,
            _: &mut PgConnection,
            job: &PdfVerificationJob,
            document: &LegalInspectionDocument,
        ) -> Result<String, Error> {
            self.current.require_permission("legal.read")?;
            if self.current.user_id() != &job.requested_by
                || self.current.selection().workspace().company_id != document.company_id
            {
                return Err(unused());
            }
            Ok(format!(
                "synthetic-current-role-{}",
                self.current.selection().role_revision()
            ))
        }
        async fn independent_person_review(
            &mut self,
            _: &mut PgConnection,
            _: &CompanyId,
            _: &UserId,
            _: &UserId,
        ) -> Result<bool, Error> {
            Err(unused())
        }
        async fn read_original(
            &mut self,
            _: &mut PgConnection,
            _: &PdfVerificationJob,
            _: &LegalInspectionDocument,
            _: usize,
        ) -> Result<LegalOriginalRead, Error> {
            Err(unused())
        }
        async fn revalidate_original(
            &mut self,
            _: &mut PgConnection,
            _: &PdfVerificationJob,
            _: &LegalInspectionDocument,
            _: &str,
        ) -> Result<(), Error> {
            Err(unused())
        }
    }
    struct ComputePort {
        holding: DatabasePool,
        control: DatabasePool,
        revoke_role: bool,
    }
    impl PdfVerifierProcess for ComputePort {
        async fn verify(
            &self,
            snapshot: AuthorizedPdfSnapshot,
            moment: TimestampMillis,
        ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
            for pool in [&self.holding, &self.control] {
                assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND xact_start IS NOT NULL)").fetch_one(pool.raw()).await.unwrap(),"the concrete worker ran inside an owner transaction");
            }
            if self.revoke_role {
                tokio::time::timeout(std::time::Duration::from_secs(3),sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',3,'[\"legal.read\",\"module.legal.read\"]','[]','[\"holding\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(self.holding.raw())).await.unwrap().unwrap();
            }
            assert!(snapshot.trust.is_empty());
            Ok(PdfVerificationEvidence {
                snapshot: PdfVerificationSnapshotStamp {
                    document_checksum_sha256: snapshot.document_checksum_sha256,
                    authorization_revision: snapshot.authorization_revision,
                    trust_revision: snapshot.trust_revision,
                    trust: vec![],
                },
                inspection_id: None,
                computation: None,
                report: PdfValidationReport {
                    status: ValidationState::Indeterminate,
                    signatures: vec![],
                    reasons: vec!["SYNTHETIC_COMPOSITION_COMPUTE".into()],
                    validated_at: time::OffsetDateTime::from_unix_timestamp_nanos(
                        i128::from(moment.get()) * 1000000,
                    )
                    .unwrap()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap(),
                    trust_store_digest: "0".repeat(64),
                    mode: "offline-strict-revocation".into(),
                    engine: "Explicit synthetic composition fixture".into(),
                },
            })
        }
    }
    #[tokio::test]
    async fn concrete_pdf_runner_prepares_computes_and_retains_in_separate_current_units() {
        fixture::with_database(2, |control| async move {
            fixture::with_database(2, move |holding| async move {
                let (authorizer, clock) = setup(&control, &holding).await;
                seed_native_legal_job(authorizer.as_ref(), &holding).await;
                let queued = PgQueuedWorkAuthorizer::new(
                    holding.clone(),
                    HoldingId::new("h001").unwrap(),
                    clock.clone(),
                );
                let result = run_legal_pdf_short_transactions(
                    &queued,
                    source(),
                    FixtureFactory {
                        next: Arc::new(AtomicUsize::new(0)),
                    },
                    &ComputePort {
                        holding: holding.clone(),
                        control,
                        revoke_role: false,
                    },
                    clock,
                )
                .await
                .unwrap();
                assert_eq!(result["status"], "indeterminate");
                assert_eq!(
                    sqlx::query_scalar::<_, String>(
                        "SELECT state FROM legal_processing_jobs WHERE id='legal-job'"
                    )
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                    "completed"
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legal_document_inspections")
                        .fetch_one(holding.raw())
                        .await
                        .unwrap(),
                    1
                );
            })
            .await;
        })
        .await;
    }
    #[tokio::test]
    async fn concrete_pdf_runner_cannot_retain_a_result_after_role_changes_during_compute() {
        fixture::with_database(2, |control| async move {
            fixture::with_database(2, move |holding| async move {
                let (authorizer, clock) = setup(&control, &holding).await;
                seed_native_legal_job(authorizer.as_ref(), &holding).await;
                let queued = PgQueuedWorkAuthorizer::new(
                    holding.clone(),
                    HoldingId::new("h001").unwrap(),
                    clock.clone(),
                );
                let result = run_legal_pdf_short_transactions(
                    &queued,
                    source(),
                    FixtureFactory {
                        next: Arc::new(AtomicUsize::new(0)),
                    },
                    &ComputePort {
                        holding: holding.clone(),
                        control,
                        revoke_role: true,
                    },
                    clock,
                )
                .await;
                assert!(matches!(result, Err(PgTransactionError::Business(_))));
                assert_eq!(
                    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legal_document_inspections")
                        .fetch_one(holding.raw())
                        .await
                        .unwrap(),
                    0
                );
                assert_eq!(
                    sqlx::query_scalar::<_, String>(
                        "SELECT state FROM legal_processing_jobs WHERE id='legal-job'"
                    )
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                    "running"
                );
            })
            .await;
        })
        .await;
    }
}

#[cfg(feature = "legal-pdf-worker")]
mod actual_owners {
    use super::*;
    use erpx_server_commercial::public::purchases::current_package_catalogue;
    use erpx_server_files::public::canonical_original::LocalOriginalStore;
    use erpx_server_legal::public::record_policy::LegalCommercialMode;
    use erpx_server_legal::public::{
        AuthorizedPdfSnapshot, PdfVerificationEvidence, PdfVerificationFailure, PdfVerifierProcess,
    };
    use erpx_server_platform_administration::public::legal_release::current_legal_release_catalogue;
    use erpx_server_pms::public::scheduled_legal::ScheduledReleaseMode;
    use erpx_server_shared::public::TimestampMillis;
    use erpx_service::{
        legal_owner_policy::LegalOwnerConfiguration, legal_pdf_bridge::run_current_legal_pdf,
    };
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        sync::{Arc, atomic::AtomicUsize},
        time::Duration,
    };
    const KEY: &str = "legal/item/original.pdf";
    const BODY: &[u8] = b"%PDF-1.7\nSynthetic owner composition fixture\n%%EOF";
    async fn seed(authorizer: &HoldingWorkAuthorizer, pool: &DatabasePool) {
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"legal.read\",\"module.legal.read\"]','[]','[\"holding\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'").execute(pool.raw()).await.unwrap();
        authorizer.execute("acme.example.test",proof(),input(),legal_access(),|c,current|Box::pin(async move{
            sqlx::raw_sql("INSERT INTO legal_settings(company_id,enabled)VALUES('company',true);INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)VALUES('legal-item','company','contract','Synthetic','confidential','TH','TH','en','THB','Asia/Bangkok','normal','user','user',0,0);INSERT INTO legal_contracts VALUES('legal-item','{}');INSERT INTO legal_members VALUES('legal-item','user','read')").execute(&mut *c).await?;
            let hash=hex::encode(Sha256::digest(BODY));
            sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id)VALUES('upload','legal_item','legal-item','legal_document','synthetic.pdf','application/pdf',$1,$1,$2,'staging/key',$3,'ready','user',$4,0,0,'company')").bind(BODY.len()as i64).bind(&hash).bind(KEY).bind(NOW+10000).execute(&mut *c).await?;
            sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state)VALUES('source','upload','legal_item','legal-item','legal_document',$1,1,'synthetic.pdf','application/pdf',$2,$3,'user',0,'company','quarantined')").bind(KEY).bind(BODY.len()as i64).bind(&hash).execute(&mut *c).await?;
            sqlx::raw_sql("UPDATE attachments SET source_state='scanning' WHERE id='source';UPDATE attachments SET source_state='available',detected_media_type='application/pdf',scanned_at=10,scanner_definition_at=9 WHERE id='source';INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,state,created_by,created_at,updated_at,bound_at,bound_command_id)VALUES('set','company','legal_item','legal-item','record','command','legal_document',1,1,'restricted','{}','legal-managed','bound','user',0,0,1,'command');INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id)VALUES('link','set','source','upload','bound','user',0,8000000000000000,1,'legal_document','legal-document');INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,scanner_definition_at,created_at,completed_at)VALUES('scan','source',1,'clean',9,0,10)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,created_by,created_at,attachment_link_id,attachment_source_id)VALUES('legal-document','legal-item',1,'synthetic.pdf','application/pdf',$1,$2,$3,'user',0,'link','source')").bind(&hash).bind(BODY.len()as i64).bind(KEY).execute(&mut *c).await?;
            sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,attempts,lease_token,lease_until,actor_json,created_by,created_at)VALUES('legal-job','company','legal-item','legal-document',$1,'verify','running',1,'lease',$2,$3,'user',0)").bind(hash).bind(NOW+10000).bind(QueuedWorkBinding::from_current(current).to_json()?).execute(c).await?;Ok(())
        })).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','legal',1,'active',0,$1,1,'Synthetic reviewed purchase','public-synthetic-evidence',0)").bind(NOW+3600000).execute(&mut *tx).await.unwrap();
        grant_event(&mut tx, None).await;
        tx.commit().await.unwrap();
    }
    async fn grant_image(c: &mut sqlx::PgConnection) -> serde_json::Value {
        sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='company'AND package_id='legal'").fetch_one(c).await.unwrap()
    }
    async fn grant_event(c: &mut sqlx::PgConnection, before: Option<serde_json::Value>) {
        let after = grant_image(c).await;
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)VALUES($1,'company','legal','synthetic-seed','public-synthetic-request',$2,$3,0)").bind(uuid::Uuid::new_v4().to_string()).bind(before).bind(after).execute(c).await.unwrap();
    }
    fn configuration() -> LegalOwnerConfiguration {
        LegalOwnerConfiguration {
            holding_id: HoldingId::new("h001").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            release_mode: ScheduledReleaseMode::Development,
            commercial_mode: LegalCommercialMode::Enforced,
            artifact_version: "unbuilt".into(),
            release_catalogue: Arc::new(current_legal_release_catalogue().unwrap()),
            purchase_catalogue: Arc::new(current_package_catalogue().unwrap()),
            engineering: None,
        }
    }
    fn store() -> (tempfile::TempDir, Arc<LocalOriginalStore>) {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(hex::encode(Sha256::digest(KEY)));
        fs::create_dir(&directory).unwrap();
        let version = uuid::Uuid::new_v4().to_string();
        fs::write(directory.join(&version), BODY).unwrap();
        fs::write(directory.join("current.json"),serde_json::to_vec(&serde_json::json!({"key":KEY,"version":version,"size":BODY.len(),"etag":hex::encode(Sha256::digest(BODY))})).unwrap()).unwrap();
        let store = Arc::new(LocalOriginalStore::open(root.path(), 1).unwrap());
        (root, store)
    }
    /// The only simulated effect is an unavailable verifier. Record policy,
    /// current IAM, purchase, readiness and canonical disk reads are actual.
    struct ToolFailure {
        holding: DatabasePool,
        control: DatabasePool,
        change: &'static str,
        calls: Arc<AtomicUsize>,
    }
    impl PdfVerifierProcess for ToolFailure {
        async fn verify(
            &self,
            snapshot: AuthorizedPdfSnapshot,
            _: TimestampMillis,
        ) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(snapshot.pdf, BODY);
            for pool in [&self.holding, &self.control] {
                assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND xact_start IS NOT NULL)").fetch_one(pool.raw()).await.unwrap());
            }
            tokio::time::timeout(Duration::from_secs(3),async{match self.change{
                "member"=>{sqlx::query("DELETE FROM legal_members WHERE item_id='legal-item'").execute(self.holding.raw()).await.unwrap();},
                "file"=>{sqlx::query("UPDATE attachments SET revoked_at=$1 WHERE id='source'").bind(NOW).execute(self.holding.raw()).await.unwrap();},
                "purchase"=>{let mut tx=self.holding.raw().begin().await.unwrap();let before=grant_image(&mut tx).await;sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company'AND package_id='legal'").execute(&mut *tx).await.unwrap();grant_event(&mut tx,Some(before)).await;tx.commit().await.unwrap();},_=>{}
            }}).await.expect("computation retained a current owner source lease");
            Err(PdfVerificationFailure::WorkerUnavailable)
        }
    }
    #[tokio::test]
    async fn actual_record_purchase_release_and_disk_ports_retain_only_current_tool_failure_evidence()
     {
        fixture::with_database(2, |control| async move {
            fixture::with_database(2, move |holding| async move {
                let (authorizer, clock) = setup(&control, &holding).await;
                seed(authorizer.as_ref(), &holding).await;
                let queued = PgQueuedWorkAuthorizer::new(
                    holding.clone(),
                    HoldingId::new("h001").unwrap(),
                    clock.clone(),
                );
                let (_root, store) = store();
                let calls = Arc::new(AtomicUsize::new(0));
                let result = run_current_legal_pdf(
                    &queued,
                    source(),
                    configuration(),
                    store,
                    &ToolFailure {
                        holding: holding.clone(),
                        control,
                        change: "",
                        calls: calls.clone(),
                    },
                    clock,
                )
                .await
                .unwrap();
                assert_eq!(result["status"], "indeterminate");
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert_eq!(
                    sqlx::query_scalar::<_, String>("SELECT state FROM legal_processing_jobs")
                        .fetch_one(holding.raw())
                        .await
                        .unwrap(),
                    "completed"
                );
                assert!(
                    sqlx::query_scalar::<_, i64>("SELECT count(*)FROM attachment_access_events")
                        .fetch_one(holding.raw())
                        .await
                        .unwrap()
                        >= 2
                );
            })
            .await;
        })
        .await;
    }
    #[tokio::test]
    async fn actual_owner_revocations_during_compute_deny_retention_without_leases() {
        for change in ["member", "purchase", "file"] {
            fixture::with_database(2, move |control| async move {
                fixture::with_database(2, move |holding| async move {
                    let (authorizer, clock) = setup(&control, &holding).await;
                    seed(authorizer.as_ref(), &holding).await;
                    let queued = PgQueuedWorkAuthorizer::new(
                        holding.clone(),
                        HoldingId::new("h001").unwrap(),
                        clock.clone(),
                    );
                    let (_root, store) = store();
                    let calls = Arc::new(AtomicUsize::new(0));
                    let result = run_current_legal_pdf(
                        &queued,
                        source(),
                        configuration(),
                        store,
                        &ToolFailure {
                            holding: holding.clone(),
                            control,
                            change,
                            calls: calls.clone(),
                        },
                        clock,
                    )
                    .await;
                    assert!(result.is_err(), "{change}");
                    assert_eq!(calls.load(Ordering::SeqCst), 1);
                    assert_eq!(
                        sqlx::query_scalar::<_, i64>(
                            "SELECT count(*)FROM legal_document_inspections"
                        )
                        .fetch_one(holding.raw())
                        .await
                        .unwrap(),
                        0
                    );
                    assert_eq!(
                        sqlx::query_scalar::<_, String>("SELECT state FROM legal_processing_jobs")
                            .fetch_one(holding.raw())
                            .await
                            .unwrap(),
                        "running"
                    );
                })
                .await;
            })
            .await;
        }
    }
    #[tokio::test]
    async fn actual_unbuilt_production_release_denies_before_any_compute_or_file_audit() {
        fixture::with_database(2, |control| async move {
            fixture::with_database(2, move |holding| async move {
                let (authorizer, clock) = setup(&control, &holding).await;
                seed(authorizer.as_ref(), &holding).await;
                let queued = PgQueuedWorkAuthorizer::new(
                    holding.clone(),
                    HoldingId::new("h001").unwrap(),
                    clock.clone(),
                );
                let (_root, store) = store();
                let calls = Arc::new(AtomicUsize::new(0));
                let mut config = configuration();
                config.release_mode = ScheduledReleaseMode::Production;
                let result = run_current_legal_pdf(
                    &queued,
                    source(),
                    config,
                    store,
                    &ToolFailure {
                        holding: holding.clone(),
                        control,
                        change: "",
                        calls: calls.clone(),
                    },
                    clock,
                )
                .await;
                assert!(matches!(result, Err(PgTransactionError::Business(_))));
                assert_eq!(calls.load(Ordering::SeqCst), 0);
                assert_eq!(
                    sqlx::query_scalar::<_, i64>("SELECT count(*)FROM attachment_access_events")
                        .fetch_one(holding.raw())
                        .await
                        .unwrap(),
                    0
                );
            })
            .await;
        })
        .await;
    }
}
