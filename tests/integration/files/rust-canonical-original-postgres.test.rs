#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_files::public::canonical_original::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
    TimestampMillis, UserId,
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

type Error = PgTransactionError<BusinessError>;
struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(100)
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}

/// Explicit test policy, not a production authority adapter. The real parent
/// must assemble Identity, purchase, preservation and owner record policy.
#[derive(Default)]
struct Policy {
    deny: bool,
    authority: String,
    calls: usize,
}
impl PgOriginalReadPolicy for Policy {
    async fn prepare(
        &mut self,
        connection: &mut PgConnection,
        _actor: &OriginalReadActor,
        _query: &LegalOriginalQuery,
    ) -> Result<(), Error> {
        assert!(connection.is_in_transaction());
        Ok(())
    }
    async fn authorize(
        &mut self,
        _connection: &mut PgConnection,
        actor: &OriginalReadActor,
        query: &LegalOriginalQuery,
        facts: &CanonicalOriginalFacts,
    ) -> Result<String, Error> {
        self.calls += 1;
        assert_eq!(actor.company_id, query.company_id);
        assert_eq!(
            facts.binding_id.as_deref(),
            Some(query.document_id.as_str())
        );
        if self.deny {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "TEST_FILES_DENIED",
                "Denied by fixture.",
            )
            .into());
        }
        Ok(self.authority.clone())
    }
}
struct Bytes {
    reads: Arc<AtomicUsize>,
    corrupt: bool,
}
impl OriginalByteStore for Bytes {
    async fn read_verified(
        &self,
        key: &str,
        _size: usize,
        _checksum: &str,
        _limit: usize,
    ) -> Result<Vec<u8>, BusinessError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        assert_eq!(key, "legal/item/original.pdf");
        Ok(if self.corrupt {
            b"incorrect".to_vec()
        } else {
            body().to_vec()
        })
    }
}
fn body() -> &'static [u8] {
    b"%PDF-synthetic"
}
fn actor() -> OriginalReadActor {
    OriginalReadActor {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("user").unwrap(),
        selection_revision: "selection-1".into(),
    }
}
fn query() -> LegalOriginalQuery {
    LegalOriginalQuery {
        holding_id: actor().holding_id,
        company_id: actor().company_id,
        item_id: RecordId::new("item").unwrap(),
        document_id: RecordId::new("document").unwrap(),
        link_id: RecordId::new("link").unwrap(),
        expected_checksum_sha256: hex::encode(Sha256::digest(body())),
        expected_media_type: LegalOriginalMediaType::Pdf,
        max_bytes: ORIGINAL_LIMIT,
    }
}

async fn setup(pool: &DatabasePool) {
    let migrations = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES('upload','legal_item','item','legal_document','synthetic.pdf','application/pdf',$1,$1,$2,'staging/key','legal/item/original.pdf','ready','user',10000,0,0,'company')")
        .bind(body().len() as i64).bind(query().expected_checksum_sha256).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state) VALUES('source','upload','legal_item','item','legal_document','legal/item/original.pdf',1,'synthetic.pdf','application/pdf',$1,$2,'user',0,'company','quarantined')")
        .bind(body().len() as i64).bind(query().expected_checksum_sha256).execute(pool.raw()).await.unwrap();
    sqlx::raw_sql("UPDATE attachments SET source_state='scanning' WHERE id='source'; UPDATE attachments SET source_state='available',detected_media_type='application/pdf',scanned_at=10,scanner_definition_at=9 WHERE id='source';
        INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,state,created_by,created_at,updated_at,bound_at,bound_command_id) VALUES('set','company','legal_item','item','record','command','legal_document',1,1,'restricted','{}','legal-managed','bound','user',0,0,1,'command');
        INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) VALUES('link','set','source','upload','bound','user',0,10000,1,'legal_document','document');
        INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,scanner_definition_at,created_at,completed_at) VALUES('scan','source',1,'clean',9,0,10)")
        .execute(pool.raw()).await.unwrap();
}

#[tokio::test]
async fn canonical_original_retains_audit_only_in_callers_transaction_and_detects_later_changes() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await; let reads=Arc::new(AtomicUsize::new(0)); let owned=reads.clone();
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|connection| {
            let reads=owned.clone(); Box::pin(async move {
                let mut policy=Policy {authority:"authority-1".into(),..Default::default()}; let bytes=Bytes { reads,corrupt:false }; let mut ids=Ids;
                let mut reader=PgCanonicalOriginalReader::new(actor().holding_id,actor().company_id,connection,&mut policy,&bytes,&FixedClock,&mut ids);
                let read=reader.read(&actor(),&query()).await?; assert_eq!(read.bytes,body()); assert_eq!(read.witness.as_str().len(),64);
                reader.revalidate(&actor(),&query(),&read.witness).await?; drop(reader);
                assert_eq!(policy.calls,3);
                policy.authority="authority-2".into();
                {
                    let mut reader=PgCanonicalOriginalReader::new(actor().holding_id,actor().company_id,connection,&mut policy,&bytes,&FixedClock,&mut ids);
                    assert!(matches!(reader.revalidate(&actor(),&query(),&read.witness).await,Err(PgTransactionError::Business(ref error)) if error.code=="FILE_ORIGINAL_CHANGED"));
                }
                policy.authority="authority-1".into();
                sqlx::query("UPDATE attachments SET revoked_at=100 WHERE id='source'").execute(&mut *connection).await?;
                let mut reader=PgCanonicalOriginalReader::new(actor().holding_id,actor().company_id,connection,&mut policy,&bytes,&FixedClock,&mut ids);
                assert!(reader.revalidate(&actor(),&query(),&read.witness).await.is_err());
                Err(BusinessError::invalid("TEST_ABORT","Caller rolls back.").into())
            })
        }).await;
        assert!(result.is_err()); assert_eq!(reads.load(Ordering::SeqCst),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_access_events").fetch_one(pool.raw()).await.unwrap(),0);
        let reads=reads.clone();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|connection| { let reads=reads.clone(); Box::pin(async move {
            let mut policy=Policy {authority:"authority-1".into(),..Default::default()}; let bytes=Bytes {reads,corrupt:false}; let mut ids=Ids;
            let mut reader=PgCanonicalOriginalReader::new(actor().holding_id,actor().company_id,connection,&mut policy,&bytes,&FixedClock,&mut ids);
            let read=reader.read(&actor(),&query()).await?; reader.revalidate(&actor(),&query(),&read.witness).await?; Ok(())
        }) }).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_access_events WHERE outcome='allowed' AND variant='original'").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}

#[tokio::test]
async fn denied_wrong_company_unready_or_corrupt_sources_cannot_certify_originals() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        for case in ["denied","company","unready","corrupt"] {
            let reads=Arc::new(AtomicUsize::new(0)); let check_reads=reads.clone();
            let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|connection| { let reads=reads.clone(); Box::pin(async move {
                if case=="unready" { sqlx::query("UPDATE attachments SET source_state='destroyed',destroyed_at=100 WHERE id='source'").execute(&mut *connection).await?; }
                let mut policy=Policy {deny:case=="denied",authority:"authority-1".into(),calls:0}; let bytes=Bytes {reads,corrupt:case=="corrupt"}; let mut ids=Ids;
                let mut a=actor(); let mut q=query(); if case=="company" { a.company_id=CompanyId::new("foreign").unwrap(); q.company_id=a.company_id.clone(); }
                let mut reader=PgCanonicalOriginalReader::new(a.holding_id.clone(),a.company_id.clone(),connection,&mut policy,&bytes,&FixedClock,&mut ids);
                assert!(reader.read(&a,&q).await.is_err(),"{case}");
                Err(BusinessError::invalid("TEST_ABORT","Fixture rollback.").into())
            }) }).await;
            assert!(result.is_err()); assert_eq!(check_reads.load(Ordering::SeqCst),usize::from(case=="corrupt"));
        }
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_access_events").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn witness_cannot_cross_transaction_even_on_the_same_database_backend() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut policy = Policy {
            authority: "authority-1".into(),
            ..Default::default()
        };
        let bytes = Bytes {
            reads: Arc::new(AtomicUsize::new(0)),
            corrupt: false,
        };
        let mut ids = Ids;
        {
            let mut reader = PgCanonicalOriginalReader::new(
                actor().holding_id,
                actor().company_id,
                &mut connection,
                &mut policy,
                &bytes,
                &FixedClock,
                &mut ids,
            );
            assert!(reader.read(&actor(), &query()).await.is_err());
        }
        let mut tx = connection.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let previous = {
            let mut reader = PgCanonicalOriginalReader::new(
                actor().holding_id,
                actor().company_id,
                &mut tx,
                &mut policy,
                &bytes,
                &FixedClock,
                &mut ids,
            );
            reader.read(&actor(), &query()).await.unwrap()
        };
        tx.commit().await.unwrap();
        policy.authority = "authority-2".into();
        let mut tx = connection.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        {
            let mut reader = PgCanonicalOriginalReader::new(
                actor().holding_id,
                actor().company_id,
                &mut tx,
                &mut policy,
                &bytes,
                &FixedClock,
                &mut ids,
            );
            assert!(
                reader
                    .revalidate(&actor(), &query(), &previous.witness)
                    .await
                    .is_err()
            );
            let fresh = reader.read(&actor(), &query()).await.unwrap();
            assert_eq!(fresh.input_identity, previous.input_identity);
            assert_ne!(fresh.witness.as_str(), previous.witness.as_str());
            reader
                .revalidate(&actor(), &query(), &fresh.witness)
                .await
                .unwrap();
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn a_source_changed_after_the_owner_snapshot_requires_a_fresh_retry() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        // Establish an earlier owner snapshot, then let a real other backend
        // commit before the Files source locks are acquired.
        sqlx::query("SELECT id FROM attachment_links WHERE id='link'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE attachments SET revoked_at=100 WHERE id='source'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut policy = Policy {
            authority: "authority-1".into(),
            ..Default::default()
        };
        let reads = Arc::new(AtomicUsize::new(0));
        let bytes = Bytes {
            reads: reads.clone(),
            corrupt: false,
        };
        let mut ids = Ids;
        let result = PgCanonicalOriginalReader::new(
            actor().holding_id,
            actor().company_id,
            &mut tx,
            &mut policy,
            &bytes,
            &FixedClock,
            &mut ids,
        )
        .read(&actor(), &query())
        .await;
        assert!(
            result.is_err(),
            "An earlier SERIALIZABLE snapshot must not certify superseded evidence"
        );
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn utility_source_lease_waits_for_revocation_before_the_first_fresh_snapshot() {
    use std::time::Duration;
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE attachments SET revoked_at=100 WHERE id='source'").execute(&mut *writer).await.unwrap();
        let fresh_pool=pool.clone();let reads=Arc::new(AtomicUsize::new(0));let observed=reads.clone();
        let fresh=tokio::spawn(async move {
            PgUnitOfWork::new(fresh_pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|connection|{
                let reads=reads.clone();Box::pin(async move {
                    lock_canonical_original_sources(connection).await?;
                    let mut policy=Policy{authority:"authority-1".into(),..Default::default()};let bytes=Bytes{reads,corrupt:false};let mut ids=Ids;
                    PgCanonicalOriginalReader::new(actor().holding_id,actor().company_id,connection,&mut policy,&bytes,&FixedClock,&mut ids).read(&actor(),&query()).await?;Ok(())
                })
            }).await
        });
        let waiting=tokio::time::timeout(Duration::from_secs(5),async {
            loop {let waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND usename=current_user AND pid<>pg_backend_pid() AND backend_type='client backend' AND wait_event_type='Lock')").fetch_one(pool.raw()).await.unwrap();if waiting{break;}tokio::time::sleep(Duration::from_millis(10)).await;}
        }).await.is_ok();
        writer.commit().await.unwrap();let outcome=fresh.await.unwrap();
        assert!(waiting);assert!(matches!(outcome,Err(Error::Business(ref e))if e.code=="FILE_ORIGINAL_UNAVAILABLE"));
        assert_eq!(observed.load(Ordering::SeqCst),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_access_events").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}
