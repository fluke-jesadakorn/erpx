//! Actual PostgreSQL qualification for the native Files attachment read.
//!
//! Only disposable leased development databases are used. The record authority
//! is a test port: production authority is owned by composition and the owning
//! business owner, never by this crate or a request DTO.
use erpx_server_files::public::attachment_read::{
    AttachmentListFilter, AttachmentReadPorts, AttachmentReader, AttachmentTargetAuthority,
    AttachmentTargetScope, PostgresAttachmentRead, attachment_link_status, list_attachment_sets,
};
use erpx_server_files::public::attachment_read::{
    inspect_attachment_ownership, lock_attachment_ownership_sources,
    revalidate_attachment_ownership,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, PgWorkFuture,
    TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Acquire, PgConnection};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[path = "../../support/rust-postgres.rs"]
mod fixture;

type Error = PgTransactionError<BusinessError>;

async fn wait_for_blocker(pool: &DatabasePool, waiting: i32, blocker: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT $2 = ANY(pg_blocking_pids($1))")
                .bind(waiting)
                .bind(blocker)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("expected exact backend to block");
}

#[tokio::test]
async fn ownership_source_locks_serialize_both_writer_orders() {
    fixture::with_database(3, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "race", "company", "asset-1", "manual", "draft", "draft", "actor", "available", 1000).await;
        let mut writer = pool.raw().acquire().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let mut tx = writer.begin().await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',purpose_key='site_photo' WHERE id='set-race'").execute(&mut *tx).await.unwrap();
        let (send_pid, recv_pid) = tokio::sync::oneshot::channel();
        let reader_pool = pool.clone();
        let reader = tokio::spawn(async move {
            let mut c = reader_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();
            send_pid.send(pid).unwrap();
            let mut t = c.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *t).await.unwrap();
            lock_attachment_ownership_sources(&mut t).await.unwrap();
            let witness = inspect_attachment_ownership(&mut t, &CompanyId::new("company").unwrap(), "link-race").await.unwrap().unwrap();
            assert_eq!(witness.kind(), erpx_server_files::public::attachment_read::AttachmentOwnerKind::FieldVisit);
            revalidate_attachment_ownership(&mut t, &witness).await.unwrap();
            t.commit().await.unwrap();
        });
        let reader_pid = recv_pid.await.unwrap();
        wait_for_blocker(&pool, reader_pid, writer_pid).await;
        tx.commit().await.unwrap();
        reader.await.unwrap();
        drop(writer);

        let mut reader = pool.raw().acquire().await.unwrap();
        let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let mut tx = reader.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_attachment_ownership_sources(&mut tx).await.unwrap();
        let witness = inspect_attachment_ownership(&mut tx, &CompanyId::new("company").unwrap(), "link-race").await.unwrap().unwrap();
        let (send_pid, recv_pid) = tokio::sync::oneshot::channel();
        let writer_pool = pool.clone();
        let writer = tokio::spawn(async move {
            let mut c = writer_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();
            send_pid.send(pid).unwrap();
            let mut t = c.begin().await.unwrap();
            sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-race'").execute(&mut *t).await.unwrap();
            t.commit().await.unwrap();
        });
        let writer_pid = recv_pid.await.unwrap();
        wait_for_blocker(&pool, writer_pid, reader_pid).await;
        revalidate_attachment_ownership(&mut tx, &witness).await.unwrap();
        tx.commit().await.unwrap();
        writer.await.unwrap();
        let mut fresh = reader.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();
        lock_attachment_ownership_sources(&mut fresh).await.unwrap();
        assert!(revalidate_attachment_ownership(&mut fresh, &witness).await.is_err());
        let after = inspect_attachment_ownership(&mut fresh, &CompanyId::new("company").unwrap(), "link-race").await.unwrap().unwrap();
        assert_eq!(after.kind(), erpx_server_files::public::attachment_read::AttachmentOwnerKind::FieldVisit);
        revalidate_attachment_ownership(&mut fresh, &after).await.unwrap();
        fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn ownership_witness_is_company_scoped_transaction_bound_and_rechecks_source() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(
            &pool,
            "ownership",
            "company",
            "asset-1",
            "manual",
            "draft",
            "draft",
            "actor",
            "available",
            1000,
        )
        .await;
        seed(&pool, "foreign-source", "foreign", "foreign-asset", "manual", "draft", "draft", "actor", "available", 1000).await;
        let company = CompanyId::new("company").unwrap();
        let foreign = CompanyId::new("foreign").unwrap();
        let mut c = pool.raw().acquire().await.unwrap();
        let mut tx = c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            inspect_attachment_ownership(&mut tx, &company, "link-ownership")
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        let mut tx = c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_attachment_ownership_sources(&mut tx).await.unwrap();
        assert!(
            inspect_attachment_ownership(&mut tx, &foreign, "link-ownership")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            inspect_attachment_ownership(&mut tx, &company, "missing")
                .await
                .unwrap()
                .is_none()
        );
        let witness = inspect_attachment_ownership(&mut tx, &company, "link-ownership")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            witness.kind(),
            erpx_server_files::public::attachment_read::AttachmentOwnerKind::Other
        );
        revalidate_attachment_ownership(&mut tx, &witness)
            .await
            .unwrap();
        sqlx::query("UPDATE attachment_sets SET target_id='changed' WHERE id='set-ownership'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            revalidate_attachment_ownership(&mut tx, &witness)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        sqlx::query(
            "UPDATE attachment_sets SET target_type='field_service_visit',purpose_key='site_photo' WHERE id='set-ownership'",
        )
        .execute(&mut *c)
        .await
        .unwrap();
        sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-ownership'")
            .execute(&mut *c)
            .await
            .unwrap();
        let mut tx = c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_attachment_ownership_sources(&mut tx).await.unwrap();
        let field = inspect_attachment_ownership(&mut tx, &company, "link-ownership")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            field.kind(),
            erpx_server_files::public::attachment_read::AttachmentOwnerKind::FieldVisit
        );
        revalidate_attachment_ownership(&mut tx, &field)
            .await
            .unwrap();
        assert!(erpx_server_files::public::attachment_read::stock_count_attachment_target(&mut tx, &field).await.unwrap().is_none());
        assert!(
            revalidate_attachment_ownership(&mut tx, &witness)
                .await
                .is_err()
        );
        sqlx::query("UPDATE attachment_sets SET purpose_key='not-a-field-purpose' WHERE id='set-ownership'")
            .execute(&mut *tx).await.unwrap();
        assert!(revalidate_attachment_ownership(&mut tx, &field).await.is_err());
        let invalid = inspect_attachment_ownership(&mut tx, &company, "link-ownership").await.unwrap().unwrap();
        assert_eq!(invalid.kind(), erpx_server_files::public::attachment_read::AttachmentOwnerKind::FieldVisit);
        revalidate_attachment_ownership(&mut tx, &invalid).await.unwrap();
        sqlx::query("UPDATE attachment_links SET attachment_id='source-foreign-source' WHERE id='link-ownership'").execute(&mut *tx).await.unwrap();
        assert!(revalidate_attachment_ownership(&mut tx, &invalid).await.is_err());
        assert!(inspect_attachment_ownership(&mut tx, &company, "link-ownership").await.unwrap().is_none());
        sqlx::query("SAVEPOINT missing_source").execute(&mut *tx).await.unwrap();
        let error = sqlx::query("UPDATE attachment_links SET attachment_id='missing-source' WHERE id='link-ownership'").execute(&mut *tx).await.unwrap_err();
        assert_eq!(error.as_database_error().and_then(|e| e.code()).as_deref(), Some("23503"));
        sqlx::query("ROLLBACK TO SAVEPOINT missing_source").execute(&mut *tx).await.unwrap();
        tx.rollback().await.unwrap();
        let long_target = "count-".to_owned() + &"x".repeat(512);
        sqlx::query("UPDATE attachment_sets SET target_type='stock_count',target_id=$1,component_kind='action',component_ref='submit',purpose_key='count_sheet' WHERE id='set-ownership'").bind(&long_target).execute(&mut *c).await.unwrap();
        let mut tx = c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_attachment_ownership_sources(&mut tx).await.unwrap();
        assert!(inspect_attachment_ownership(&mut tx, &foreign, "link-ownership").await.unwrap().is_none());
        let count = inspect_attachment_ownership(&mut tx, &company, "link-ownership").await.unwrap().unwrap();
        let target = erpx_server_files::public::attachment_read::stock_count_attachment_target(&mut tx, &count).await.unwrap().unwrap();
        assert_eq!((&target.target_id, target.purpose_key.as_str(), target.component_kind.as_str(), target.component_ref.as_deref()), (&long_target, "count_sheet", "action", Some("submit")));
        sqlx::query("UPDATE attachment_sets SET component_ref='different' WHERE id='set-ownership'").execute(&mut *tx).await.unwrap();
        assert!(erpx_server_files::public::attachment_read::stock_count_attachment_target(&mut tx, &count).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = c.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_attachment_ownership_sources(&mut tx).await.unwrap();
        assert!(erpx_server_files::public::attachment_read::stock_count_attachment_target(&mut tx, &count).await.is_err());
        tx.rollback().await.unwrap();
    })
    .await;
}

async fn migrate(pool: &DatabasePool) {
    let migrations = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

#[derive(Default)]
struct AuthorityInner {
    deny: bool,
    calls: AtomicUsize,
    seen: std::sync::Mutex<Vec<String>>,
}
#[derive(Clone, Default)]
struct Authority(Arc<AuthorityInner>);
impl Authority {
    fn allowed() -> Self {
        Self::default()
    }
    fn denied() -> Self {
        Self(Arc::new(AuthorityInner {
            deny: true,
            ..AuthorityInner::default()
        }))
    }
    fn calls(&self) -> usize {
        self.0.calls.load(Ordering::SeqCst)
    }
    fn seen(&self) -> Vec<String> {
        self.0.seen.lock().unwrap().clone()
    }
}
impl AttachmentTargetAuthority for Authority {
    fn authorize_read<'a>(
        &'a self,
        _: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        target: &'a erpx_server_files::public::attachment_read::AttachmentTarget<'a>,
    ) -> PgWorkFuture<'a, AttachmentTargetScope, BusinessError> {
        Box::pin(async move {
            self.0.calls.fetch_add(1, Ordering::SeqCst);
            self.0.seen.lock().unwrap().push(format!(
                "{}|{}|{}|{}",
                target.target_type, target.target_id, target.purpose_key, target.component_kind
            ));
            if self.0.deny {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ATTACHMENT_FORBIDDEN",
                    "Your current assignment cannot read files for this record.",
                )
                .into());
            }
            AttachmentTargetScope::new(reader.company_id().clone(), Some("branch".into()), None, 3)
                .map_err(Into::into)
        })
    }
}

fn reader_for(company: &str) -> AttachmentReader {
    AttachmentReader::new(
        "actor",
        CompanyId::new(company).unwrap(),
        Some("branch".into()),
        None,
        100_000,
    )
    .unwrap()
}

fn read_only() -> bool {
    false
}

#[allow(clippy::too_many_arguments)]
async fn seed(
    pool: &DatabasePool,
    prefix: &str,
    company: &str,
    target: &str,
    purpose: &str,
    set_state: &str,
    link_state: &str,
    creator: &str,
    source_state: &str,
    created_at: i64,
) {
    let attachment = format!("source-{prefix}");
    let upload = format!("upload-{prefix}");
    let set = format!("set-{prefix}");
    let link = format!("link-{prefix}");
    sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,revision,status,created_by,expires_at,created_at,updated_at,company_id) VALUES($1,'asset',$2,$3,'evidence.pdf','application/pdf',12,12,$4,$5,1,'uploaded',$6,$7,$8,$8,$9)")
        .bind(&upload)
        .bind(target)
        .bind(purpose)
        .bind("a".repeat(64))
        .bind(format!("staging/{prefix}"))
        .bind(creator)
        .bind(created_at + 10_000)
        .bind(created_at)
        .bind(company)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at,preview_object_key,preview_media_type,preview_checksum_sha256,preview_bytes) VALUES($1,$2,'asset',$3,$4,$5,1,'evidence.pdf','application/pdf',12,$6,$7,$8,$9,'application/pdf',$10,$8,$8,$11,'image/webp',$6,24) ON CONFLICT (id) DO NOTHING")
        .bind(&attachment)
        .bind(&upload)
        .bind(target)
        .bind(purpose)
        .bind(format!("origin/{prefix}"))
        .bind("a".repeat(64))
        .bind(creator)
        .bind(created_at)
        .bind(company)
        .bind(source_state)
        .bind(format!("preview/{prefix}"))
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,bound_at,bound_command_id,created_by,created_at,updated_at) VALUES($1,$2,'branch','asset',$3,'record',$4,3,1,'internal','{}',1,10,$5,$6,$7,$8,$9,$9)")
        .bind(&set)
        .bind(company)
        .bind(target)
        .bind(purpose)
        .bind(set_state)
        .bind((set_state == "bound").then_some(created_at + 5))
        .bind((set_state == "bound").then_some(format!("bind-{prefix}")))
        .bind(creator)
        .bind(created_at)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,binding_type,binding_id,bound_at,created_by,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(&link)
        .bind(&set)
        .bind(&attachment)
        .bind(link_state)
        .bind((link_state == "bound").then_some("record"))
        .bind((link_state == "bound").then_some(format!("record-{prefix}")))
        .bind((link_state == "bound").then_some(created_at + 5))
        .bind(creator)
        .bind(created_at)
        .bind(created_at + 10_000)
        .execute(pool.raw())
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,scanner_engine,scanner_signature,scanner_definition_at,completed_at,created_at) VALUES($1,$2,1,'clean','clamd','Signature',$3,$3,$4)")
        .bind(format!("scan-{prefix}"))
        .bind(&attachment)
        .bind(created_at + 1)
        .bind(created_at)
        .execute(pool.raw())
        .await
        .unwrap();
}

async fn run<T, F>(pool: &DatabasePool, work: F) -> Result<T, Error>
where
    T: Send + 'static,
    F: for<'a> FnOnce(&'a mut PgConnection) -> PgWorkFuture<'a, T, BusinessError> + Send,
{
    PgUnitOfWork::new(pool.clone())
        .run_once::<T, BusinessError, _>(TransactionOptions::default(), work)
        .await
}

#[tokio::test]
async fn list_returns_bound_sets_and_own_drafts_with_exact_company_scope() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(
            &pool,
            "bound",
            "company",
            "asset-1",
            "manual",
            "bound",
            "bound",
            "actor",
            "available",
            1_000,
        )
        .await;
        seed(
            &pool,
            "draft",
            "company",
            "asset-1",
            "specification",
            "draft",
            "draft",
            "actor",
            "available",
            2_000,
        )
        .await;
        seed(
            &pool,
            "other",
            "company",
            "asset-1",
            "manual",
            "draft",
            "draft",
            "someone-else",
            "available",
            3_000,
        )
        .await;
        seed(
            &pool,
            "foreign",
            "other",
            "asset-1",
            "manual",
            "bound",
            "bound",
            "actor",
            "available",
            4_000,
        )
        .await;
        let authority = Authority::allowed();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let filter = AttachmentListFilter::new("asset", "asset-1", None, None, None).unwrap();
        let reader = reader_for("company");
        let rows = run(&pool, move |c| {
            Box::pin(async move {
                list_attachment_sets(c, &ports, &reader, &filter, read_only(), 90_000).await
            })
        })
        .await
        .unwrap();
        assert_eq!(
            rows.len(),
            2,
            "foreign Company and other-person drafts stay hidden"
        );
        assert_eq!(rows[0].id, "set-draft");
        assert_eq!(rows[0].links.len(), 1);
        assert_eq!(rows[0].links[0].filename, "evidence.pdf");
        assert!(rows[0].links[0].preview_available);
        assert_eq!(rows[1].id, "set-bound");
        assert_eq!(rows[1].state, "bound");
        assert_eq!(rows[1].links[0].link_state, "bound");
        assert_eq!(authority.calls(), 2);
        assert_eq!(
            authority.seen(),
            vec![
                "asset|asset-1|specification|record",
                "asset|asset-1|manual|record",
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn list_filters_and_read_only_preserve_exact_visibility() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(
            &pool,
            "bound",
            "company",
            "asset-1",
            "manual",
            "bound",
            "bound",
            "actor",
            "available",
            1_000,
        )
        .await;
        seed(
            &pool,
            "draft",
            "company",
            "asset-1",
            "specification",
            "draft",
            "draft",
            "actor",
            "available",
            2_000,
        )
        .await;
        seed(
            &pool,
            "other",
            "company",
            "asset-1",
            "manual",
            "draft",
            "draft",
            "someone-else",
            "available",
            3_000,
        )
        .await;
        let authority = Authority::allowed();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let reader = reader_for("company");
        let by_purpose = AttachmentListFilter::new(
            "asset",
            "asset-1",
            Some("specification".into()),
            Some("record".into()),
            Some(3),
        )
        .unwrap();
        let rows = run(&pool, move |c| {
            Box::pin(async move {
                list_attachment_sets(c, &ports, &reader, &by_purpose, read_only(), 90_000).await
            })
        })
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "set-draft");

        let all = AttachmentListFilter::new("asset", "asset-1", None, None, None).unwrap();
        let reader = reader_for("company");
        let ports = PostgresAttachmentRead::new(authority.clone());
        let rows = run(&pool, move |c| {
            Box::pin(
                async move { list_attachment_sets(c, &ports, &reader, &all, true, 90_000).await },
            )
        })
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "set-bound");
    })
    .await;
}

#[tokio::test]
async fn status_reads_scan_retention_and_writes_one_allowed_access_audit() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "draft", "company", "asset-1", "specification", "draft", "draft", "actor", "available", 2_000).await;
        let authority = Authority::allowed();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let reader = reader_for("company");
        let view = run(&pool, move |c| {
            Box::pin(async move {
                attachment_link_status(c, &ports, &reader, "link-draft", read_only(), 90_000).await
            })
        })
        .await
        .unwrap();
        assert_eq!(view.link_id, "link-draft");
        assert_eq!(view.source_state.as_deref(), Some("available"));
        assert_eq!(view.scan.attempts, 1);
        assert_eq!(view.scan.last_result.as_deref(), Some("clean"));
        assert_eq!(view.scan.signature.as_deref(), Some("Signature"));
        assert!(view.preview_available);
        assert!(view.bindable);
        assert_eq!(view.expires_at, 12_000);
        let audit: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT attachment_id,actor_id,variant FROM attachment_access_events WHERE link_id='link-draft'",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            audit,
            vec![("source-draft".into(), "actor".into(), "status".into())]
        );
        assert_eq!(authority.calls(), 1);
    })
    .await;
}

#[tokio::test]
async fn status_hides_other_drafts_and_read_only_drafts_without_authority() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "other", "company", "asset-1", "specification", "draft", "draft", "someone-else", "available", 2_000).await;
        let authority = Authority::allowed();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let reader = reader_for("company");
        let error = run(&pool, move |c| {
            Box::pin(async move {
                attachment_link_status(c, &ports, &reader, "link-other", read_only(), 90_000).await
            })
        })
        .await
        .unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(ref error) if error.code == "ATTACHMENT_LINK_NOT_FOUND"));
        assert_eq!(authority.calls(), 0);
        let audit: i64 =
            sqlx::query_scalar("SELECT count(*) FROM attachment_access_events WHERE link_id='link-other'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(audit, 0);

        let reader = reader_for("company");
        let ports = PostgresAttachmentRead::new(authority.clone());
        let error = run(&pool, move |c| {
            Box::pin(async move {
                attachment_link_status(c, &ports, &reader, "link-other", true, 90_000).await
            })
        })
        .await
        .unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(_)));
    })
    .await;
}

#[tokio::test]
async fn authority_denial_aborts_the_whole_read_without_access_audit() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "draft", "company", "asset-1", "specification", "draft", "draft", "actor", "available", 2_000).await;
        let authority = Authority::denied();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let reader = reader_for("company");
        let filter = AttachmentListFilter::new("asset", "asset-1", None, None, None).unwrap();
        let error = run(&pool, move |c| {
            Box::pin(async move { list_attachment_sets(c, &ports, &reader, &filter, false, 90_000).await })
        })
        .await
        .unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(ref error) if error.code == "ATTACHMENT_FORBIDDEN"));

        let reader = reader_for("company");
        let ports = PostgresAttachmentRead::new(authority.clone());
        let error = run(&pool, move |c| {
            Box::pin(async move {
                attachment_link_status(c, &ports, &reader, "link-draft", false, 90_000).await
            })
        })
        .await
        .unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(ref error) if error.code == "ATTACHMENT_FORBIDDEN"));
        let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM attachment_access_events")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(audit, 0);
    })
    .await;
}

#[tokio::test]
async fn a_changed_projection_during_the_read_is_rejected_and_audit_rolls_back() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "draft", "company", "asset-1", "specification", "draft", "draft", "actor", "available", 2_000).await;
        sqlx::raw_sql("CREATE FUNCTION owned_status_rewrite() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE attachment_links SET expires_at=expires_at+1 WHERE id=NEW.link_id; RETURN NEW; END$$; CREATE TRIGGER owned_status_rewrite AFTER INSERT ON attachment_access_events FOR EACH ROW EXECUTE FUNCTION owned_status_rewrite();")
            .execute(pool.raw())
            .await
            .unwrap();
        let authority = Authority::allowed();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let reader = reader_for("company");
        let error = run(&pool, move |c| {
            Box::pin(async move {
                attachment_link_status(c, &ports, &reader, "link-draft", false, 90_000).await
            })
        })
        .await
        .unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(ref error) if error.code == "ATTACHMENT_READ_CHANGED"));
        let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM attachment_access_events")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(audit, 0);
        let expires: i64 = sqlx::query_scalar("SELECT expires_at FROM attachment_links WHERE id='link-draft'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(expires, 12_000);
    })
    .await;
}

#[tokio::test]
async fn caller_unit_must_be_an_active_serializable_transaction() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "draft", "company", "asset-1", "specification", "draft", "draft", "actor", "available", 2_000).await;
        let authority = Authority::allowed();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let filter = AttachmentListFilter::new("asset", "asset-1", None, None, None).unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(ports.list_sets(&mut connection, &reader_for("company"), &filter, false).await.is_err());
        drop(connection);
        let mut connection = pool.raw().acquire().await.unwrap();
        let mut transaction = connection.begin().await.unwrap();
        let error = ports
            .list_sets(&mut transaction, &reader_for("company"), &filter, false)
            .await
            .unwrap_err();
        assert!(matches!(error, PgTransactionError::Persistence(ref error) if error.code == "FILES_SERIALIZABLE_REQUIRED"));
        transaction.rollback().await.unwrap();
        assert_eq!(authority.calls(), 0);
    })
    .await;
}

#[tokio::test]
async fn oversized_list_fails_closed_instead_of_truncating_owner_evidence() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        for index in 0..=100 {
            let prefix = format!("bulk-{index:03}");
            let purpose = format!("purpose-{index:03}");
            seed(
                &pool,
                &prefix,
                "company",
                "asset-1",
                &purpose,
                if index % 2 == 0 { "bound" } else { "draft" },
                if index % 2 == 0 { "bound" } else { "draft" },
                "actor",
                "available",
                1_000 + index,
            )
            .await;
        }
        let authority = Authority::allowed();
        let ports = PostgresAttachmentRead::new(authority.clone());
        let reader = reader_for("company");
        let filter = AttachmentListFilter::new("asset", "asset-1", None, None, None).unwrap();
        let error = run(&pool, move |c| {
            Box::pin(async move { list_attachment_sets(c, &ports, &reader, &filter, false, 90_000).await })
        })
        .await
        .unwrap_err();
        assert!(matches!(error, PgTransactionError::Business(ref error) if error.code == "ATTACHMENT_LIST_LIMIT"));
    })
    .await;
}
