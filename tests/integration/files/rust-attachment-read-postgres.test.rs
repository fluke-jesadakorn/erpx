//! Actual PostgreSQL qualification for the native Files attachment read.
//!
//! Only disposable leased development databases are used. The record authority
//! is a test port: production authority is owned by composition and the owning
//! business owner, never by this crate or a request DTO.
use erpx_server_files::public::attachment_read::{
    AttachmentListFilter, AttachmentReadPorts, AttachmentReader, AttachmentTargetAuthority,
    AttachmentTargetScope, PostgresAttachmentRead, attachment_link_status, list_attachment_sets,
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
