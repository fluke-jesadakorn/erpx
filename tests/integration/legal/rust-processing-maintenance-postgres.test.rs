#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_legal::public::{
    LegalProcessingCursor, LegalProcessingJob, processing_maintenance::*,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, HoldingId, IdentifierSource, RecordId, TimestampMillis,
};
use sqlx::PgConnection;
use std::sync::atomic::{AtomicI64, Ordering};
type Error = PgTransactionError<BusinessError>;
struct Time(AtomicI64);
impl Clock for Time {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0.load(Ordering::SeqCst))
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
fn scope() -> LegalMaintenanceScope {
    LegalMaintenanceScope {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
    }
}
/// Synthetic readiness/purchase port, explicitly no personal authority. Service
/// integration supplies the actual PMS/Commercial/owner source policy.
struct Policy;
impl PgLegalSystemClaimPolicy for Policy {
    async fn prepare_sources(&mut self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }
    async fn candidate_eligible(
        &mut self,
        c: &mut PgConnection,
        job: &LegalProcessingJob,
        _: i64,
    ) -> Result<bool, Error> {
        let facts = legal_system_candidate_facts(c, &scope(), job).await?;
        assert_eq!(facts.kind, "contract");
        Ok(facts.enabled && !job.id.as_str().starts_with("000-ineligible"))
    }
}
async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status)VALUES('company','company','acme','Synthetic',NULL,'company','active');INSERT INTO legal_settings(company_id,enabled)VALUES('company',true);INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)VALUES('item','company','contract','Synthetic','internal','TH','TH','en','THB','Asia/Bangkok','normal','retired-user','retired-user',0,0);INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at)VALUES('document','item',1,'synthetic.pdf','application/pdf',repeat('a',64),1,'x','retired-user',0)").execute(pool.raw()).await.unwrap();
}
async fn seed(pool: &DatabasePool, id: &str, state: &str, attempts: i32, until: Option<i64>) {
    sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,attempts,lease_token,lease_until,actor_json,created_by,created_at)VALUES($1,'company','item','document',repeat('a',64),'verify',$2,$3,CASE WHEN $2='running'THEN 'old-lease'ELSE NULL END,$4,'{\"invalid-native-selector\":true}','retired-user',0)").bind(id).bind(state).bind(attempts).bind(until).execute(pool.raw()).await.unwrap();
}
async fn claim(pool: &DatabasePool, after: Option<LegalProcessingCursor>) -> LegalMaintenanceScan {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let after = after.clone();
            Box::pin(async move {
                let (mut policy, mut ids) = (Policy, Ids);
                let clock = Time(AtomicI64::new(100));
                PgLegalProcessingMaintenance::new(scope(), c, &mut policy, &clock, &mut ids)
                    .claim(after.as_ref())
                    .await
            })
        })
        .await
        .unwrap()
}
#[tokio::test]
async fn opaque_system_claim_may_only_record_its_own_terminal_failure_without_personal_outputs() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool, "job", "queued", 0, None).await;
        let claimed = claim(&pool, None).await.claim.unwrap();
        assert_eq!(claimed.lease().attempts, 1);
        let handle = claimed.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let handle = handle.clone();
                Box::pin(async move {
                    let (mut policy, mut ids) = (Policy, Ids);
                    PgLegalProcessingMaintenance::new(
                        scope(),
                        c,
                        &mut policy,
                        &Time(AtomicI64::new(101)),
                        &mut ids,
                    )
                    .fail_claim(&handle, "WORK_AUTHORITY_REVOKED")
                    .await
                })
            })
            .await
            .unwrap();
        let row: (String, Option<String>, Option<String>, Option<String>) =
            sqlx::query_as("SELECT state,error,result_json,lease_token FROM legal_processing_jobs")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(
            row,
            (
                "failed".into(),
                Some("WORK_AUTHORITY_REVOKED".into()),
                None,
                None
            )
        );
        for table in [
            "legal_document_inspections",
            "legal_access_events",
            "command_receipts",
        ] {
            let count = match table {
                "legal_document_inspections" => {
                    sqlx::query_scalar::<_, i64>("SELECT count(*)FROM legal_document_inspections")
                        .fetch_one(pool.raw())
                        .await
                        .unwrap()
                }
                "legal_access_events" => {
                    sqlx::query_scalar::<_, i64>("SELECT count(*)FROM legal_access_events")
                        .fetch_one(pool.raw())
                        .await
                        .unwrap()
                }
                _ => sqlx::query_scalar::<_, i64>("SELECT count(*)FROM command_receipts")
                    .fetch_one(pool.raw())
                    .await
                    .unwrap(),
            };
            assert_eq!(count, 0);
        }
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let handle = claimed.clone();
                Box::pin(async move {
                    let (mut p, mut i) = (Policy, Ids);
                    PgLegalProcessingMaintenance::new(
                        scope(),
                        c,
                        &mut p,
                        &Time(AtomicI64::new(102)),
                        &mut i,
                    )
                    .fail_claim(&handle, "WORK_AUTHORITY_REVOKED")
                    .await
                })
            })
            .await;
        assert!(
            matches!(result, Err(Error::Business(_))),
            "a failed flag is not a fresh running lease proof"
        );
    })
    .await;
}
#[tokio::test]
async fn changed_token_attempt_scope_or_expired_lease_never_uses_an_old_claim() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool, "job", "queued", 0, None).await;
        let handle = claim(&pool, None).await.claim.unwrap();
        for variant in ["token", "attempt", "scope", "expired"] {
            let mut tx = pool.raw().begin().await.unwrap();
            match variant {
                "token" => {
                    sqlx::query("UPDATE legal_processing_jobs SET lease_token='new-lease'")
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                "attempt" => {
                    sqlx::query("UPDATE legal_processing_jobs SET attempts=attempts+1")
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                }
                _ => {}
            }
            tx.commit().await.unwrap();
            let old = handle.clone();
            let result = PgUnitOfWork::new(pool.clone())
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                    let old = old.clone();
                    Box::pin(async move {
                        let (mut p, mut i) = (Policy, Ids);
                        let mut selected = scope();
                        if variant == "scope" {
                            selected.company_id = CompanyId::new("foreign").unwrap();
                        }
                        let now = if variant == "expired" {
                            old.lease().until + 1
                        } else {
                            101
                        };
                        PgLegalProcessingMaintenance::new(
                            selected,
                            c,
                            &mut p,
                            &Time(AtomicI64::new(now)),
                            &mut i,
                        )
                        .fail_claim(&old, "WORK_AUTHORITY_REVOKED")
                        .await
                    })
                })
                .await;
            assert!(matches!(result, Err(Error::Business(_))), "{variant}");
            sqlx::query("UPDATE legal_processing_jobs SET lease_token=$1,attempts=$2")
                .bind(handle.lease().token.as_str())
                .bind(i32::try_from(handle.lease().attempts).unwrap())
                .execute(pool.raw())
                .await
                .unwrap();
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM legal_processing_jobs")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "running"
        );
    })
    .await;
}
#[tokio::test]
async fn cursor_continuation_reaches_work_after_more_than_sixty_four_ineligible_rows() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        for n in 0..70 {
            seed(&pool, &format!("000-ineligible-{n:03}"), "queued", 0, None).await;
        }
        seed(&pool, "999-eligible", "queued", 0, None).await;
        let first = claim(&pool, None).await;
        assert!(first.claim.is_none());
        assert!(first.next_cursor.is_some());
        let second = claim(&pool, first.next_cursor).await.claim.unwrap();
        assert_eq!(second.lease().job.id.as_str(), "999-eligible");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT sum(attempts)FROM legal_processing_jobs")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
    })
    .await;
}
#[tokio::test]
async fn only_expired_interrupted_running_jobs_requeue_with_bounded_three_attempt_policy() {
    fixture::with_database(2,|pool|async move{setup(&pool).await;for n in 0..103{seed(&pool,&format!("expired-{n:03}"),"running",if n==102{3}else{1},Some(99)).await;}seed(&pool,"terminal","failed",1,None).await;let count=|pool:DatabasePool|async move{PgUnitOfWork::new(pool).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{let(mut p,mut i)=(Policy,Ids);PgLegalProcessingMaintenance::new(scope(),c,&mut p,&Time(AtomicI64::new(100)),&mut i).recover_expired().await})).await.unwrap()};assert_eq!(count(pool.clone()).await,100);assert_eq!(count(pool.clone()).await,3);assert_eq!(count(pool.clone()).await,0);assert_eq!(sqlx::query_scalar::<_,String>("SELECT state FROM legal_processing_jobs WHERE id='terminal'").fetch_one(pool.raw()).await.unwrap(),"failed");assert_eq!(sqlx::query_scalar::<_,String>("SELECT state FROM legal_processing_jobs WHERE id='expired-102'").fetch_one(pool.raw()).await.unwrap(),"failed");assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM legal_processing_jobs WHERE state='queued'AND error='PROCESSING_INTERRUPTED'").fetch_one(pool.raw()).await.unwrap(),102);}).await;
}
