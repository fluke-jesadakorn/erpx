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

#[tokio::test]
async fn quality_inspection_descriptors_retain_exact_ids_and_revocation_in_one_unit() {
    use erpx_server_files::public::quality_inspection_descriptors::{inspect_quality_inspection_descriptors, lock_quality_inspection_descriptors};
    fixture::with_database(3, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "quality-visible", "company", "hold-visible", "manual", "draft", "draft", "actor", "available", 1000).await;
        seed(&pool, "quality-revoked", "company", "hold-revoked", "manual", "draft", "draft", "actor", "available", 1000).await;
        sqlx::query("UPDATE attachments SET revoked_at=2000 WHERE id='source-quality-revoked'").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('quality-large-revoked-upload','asset','large-revoked','manual','file','text/plain',1,'quality-large-revoked-staging','ready','actor',10000,1,1),('quality-large-visible-upload','asset','large-visible','manual','file','text/plain',1,'quality-large-visible-staging','ready','actor',10000,1,1); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,revoked_at) VALUES('quality-large-revoked','quality-large-revoked-upload','asset','large-revoked','manual','quality-large-revoked-object',1,repeat('x',4194305),'text/plain',1,'checksum','actor',1,2000),('quality-large-visible','quality-large-visible-upload','asset','large-visible','manual','quality-large-visible-object',1,repeat('x',4194305),'text/plain',1,'checksum','actor',1,NULL)").execute(pool.raw()).await.unwrap();
        let ids = vec!["missing-quality-file".to_owned(), "source-quality-revoked".to_owned(), "quality-large-revoked".to_owned(), "source-quality-visible".to_owned()];
        let mut no_lock = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *no_lock).await.unwrap();
        assert!(inspect_quality_inspection_descriptors(&mut no_lock, &ids).await.is_err());
        no_lock.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_inspection_descriptors(&mut tx).await.unwrap();
        let witness = inspect_quality_inspection_descriptors(&mut tx, &ids).await.unwrap();
        assert_eq!(witness.descriptors().len(), 1);
        assert_eq!(witness.descriptors()[0].id, "source-quality-visible");
        assert_eq!(witness.descriptors()[0].original_name, "evidence.pdf");
        assert_eq!(witness.descriptors()[0].media_type, "application/pdf");
        assert_eq!(witness.descriptors()[0].bytes, 12);
        assert_eq!(witness.descriptors()[0].checksum_sha256, "a".repeat(64));
        witness.require_current(&mut tx).await.unwrap();
        assert!(inspect_quality_inspection_descriptors(&mut tx, &["quality-large-visible".into()]).await.is_err());
        sqlx::query("SAVEPOINT files_descriptor_before_insert").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('missing-quality-upload','asset','missing','manual','file','text/plain',1,'missing-quality-staging','ready','actor',10000,1,1); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES('missing-quality-file','missing-quality-upload','asset','missing','manual','missing-quality-object',1,'file','text/plain',1,'checksum','actor',1)").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT files_descriptor_before_insert").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT files_descriptor_before_revoke").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachments SET revoked_at=3000 WHERE id='source-quality-visible'").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT files_descriptor_before_revoke").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        assert!(inspect_quality_inspection_descriptors(&mut tx, &["source-quality-visible".into(), "source-quality-visible".into()]).await.is_err());
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_quality_inspection_descriptors(&mut other).await.unwrap();
        assert!(witness.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_inspection_descriptor_locks_serialize_actual_revocation_both_orders() {
    use erpx_server_files::public::quality_inspection_descriptors::{inspect_quality_inspection_descriptors, lock_quality_inspection_descriptors};
    fixture::with_database(3, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "quality-writer-first", "company", "hold-writer", "manual", "draft", "draft", "actor", "available", 1000).await;
        seed(&pool, "quality-reader-first", "company", "hold-reader", "manual", "draft", "draft", "actor", "available", 1000).await;
        let mut writer = pool.raw().acquire().await.unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let mut tx = writer.begin().await.unwrap();
        sqlx::query("UPDATE attachments SET revoked_at=2000 WHERE id='source-quality-writer-first'").execute(&mut *tx).await.unwrap();
        let (send_pid, recv_pid) = tokio::sync::oneshot::channel();
        let reader_pool = pool.clone();
        let reader = tokio::spawn(async move {
            let mut c = reader_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();
            send_pid.send(pid).unwrap();
            let mut t = c.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *t).await.unwrap();
            lock_quality_inspection_descriptors(&mut t).await.unwrap();
            let witness = inspect_quality_inspection_descriptors(&mut t, &["source-quality-writer-first".into()]).await.unwrap();
            assert!(witness.descriptors().is_empty());
            witness.require_current(&mut t).await.unwrap();
            t.commit().await.unwrap();
        });
        wait_for_blocker(&pool, recv_pid.await.unwrap(), writer_pid).await;
        tx.commit().await.unwrap();
        reader.await.unwrap();
        drop(writer);

        let mut reader = pool.raw().acquire().await.unwrap();
        let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let mut tx = reader.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_inspection_descriptors(&mut tx).await.unwrap();
        let witness = inspect_quality_inspection_descriptors(&mut tx, &["source-quality-reader-first".into()]).await.unwrap();
        assert_eq!(witness.descriptors().len(), 1);
        let (send_pid, recv_pid) = tokio::sync::oneshot::channel();
        let writer_pool = pool.clone();
        let writer = tokio::spawn(async move {
            let mut c = writer_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();
            send_pid.send(pid).unwrap();
            sqlx::query("UPDATE attachments SET revoked_at=3000 WHERE id='source-quality-reader-first'").execute(&mut *c).await.unwrap();
        });
        wait_for_blocker(&pool, recv_pid.await.unwrap(), reader_pid).await;
        witness.require_current(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        drop(reader);
        writer.await.unwrap();
        let mut fresh = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();
        lock_quality_inspection_descriptors(&mut fresh).await.unwrap();
        let after = inspect_quality_inspection_descriptors(&mut fresh, &["source-quality-reader-first".into()]).await.unwrap();
        assert!(after.descriptors().is_empty());
        fresh.rollback().await.unwrap();
    }).await;
}

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
async fn field_attachment_ancestry_retains_scoped_link_and_global_source_candidates() {
    use erpx_server_files::public::field_attachment_ancestry::{inspect_field_attachment_ancestry,lock_field_attachment_ancestry};
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"ancestry","company","asset-ancestry","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"ancestry-foreign","foreign","asset-foreign","manual","draft","draft","actor","available",1000).await;
        sqlx::query("UPDATE upload_intents SET attachment_set_id='set-ancestry-foreign' WHERE id IN ('upload-ancestry','upload-ancestry-foreign')").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE attachment_links SET attachment_id='source-ancestry',upload_intent_id='upload-ancestry-foreign',state='withdrawn',withdrawn_at=1100,withdrawn_by='actor',withdrawal_reason='fixture' WHERE id='link-ancestry-foreign'").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE attachments SET revoked_at=1100 WHERE id='source-ancestry'").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('link-ancestry-null','set-ancestry',NULL,'draft','actor',1000,11000)").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let foreign=CompanyId::new("foreign").unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry").await.is_err());
        tx.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_field_attachment_ancestry(&mut tx).await.unwrap();
        assert!(inspect_field_attachment_ancestry(&mut tx,&foreign,"link-ancestry").await.unwrap().is_none());
        assert!(inspect_field_attachment_ancestry(&mut tx,&company,"missing").await.unwrap().is_none());
        let witness=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry").await.unwrap().unwrap();
        let relation=erpx_server_workflow::public::record_origin::FieldPaymentTargets::relation_identifier(&witness);
        let nonce=relation.strip_prefix("pg_temp.\"erpx_files_ancestry_").and_then(|v|v.strip_suffix('"')).unwrap();
        assert_eq!(nonce.len(),32);assert!(nonce.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        erpx_server_workflow::public::record_origin::FieldPaymentTargets::require_current(&witness,&mut tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        let routing=witness.field_visit_routing(&mut tx).await.unwrap();assert!(!routing.direct_set && !routing.direct && !routing.source);
        sqlx::query("SAVEPOINT field_visit_paths").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',target_id='visit' WHERE id='set-ancestry'").execute(&mut *tx).await.unwrap();
        assert!(witness.field_visit_routing(&mut tx).await.is_err());
        let direct_only=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry-null").await.unwrap().unwrap();
        let direct=direct_only.field_visit_routing(&mut tx).await.unwrap();assert!(direct.direct_set && direct.direct);assert!(!direct.source);
        let linked=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry").await.unwrap().unwrap();
        let result=linked.field_visit_routing(&mut tx).await.unwrap();assert!(result.direct_set && result.direct && result.source);
        let legacy:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned('source-ancestry')").fetch_one(&mut *tx).await.unwrap();assert_eq!(result.source,legacy);
        sqlx::query("ROLLBACK TO SAVEPOINT field_visit_paths").execute(&mut *tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();
        // A selected link with no attachment can still have Field upload
        // lineage; neither arm alone creates an attachment-source predicate.
        sqlx::query("SAVEPOINT visit_upload_owner").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE upload_intents SET owner_type='field_service_visit',owner_id='visit-upload' WHERE id='upload-ancestry-foreign'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_links SET upload_intent_id='upload-ancestry-foreign' WHERE id='link-ancestry-null'").execute(&mut *tx).await.unwrap();
        let upload=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry-null").await.unwrap().unwrap();let flags=upload.field_visit_routing(&mut tx).await.unwrap();assert!(!flags.direct_set && flags.direct && !flags.source);
        sqlx::query("ROLLBACK TO SAVEPOINT visit_upload_owner").execute(&mut *tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT visit_upload_set").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',target_id='visit-upload-set' WHERE id='set-ancestry-foreign'").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_links SET upload_intent_id='upload-ancestry' WHERE id='link-ancestry-null'").execute(&mut *tx).await.unwrap();
        let upload_set=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry-null").await.unwrap().unwrap();let flags=upload_set.field_visit_routing(&mut tx).await.unwrap();assert!(!flags.direct_set && flags.direct && !flags.source);
        // The foreign link to the same attachment makes only global source
        // ancestry Field-owned for the original selected link.
        let foreign_only=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry").await.unwrap().unwrap();let flags=foreign_only.field_visit_routing(&mut tx).await.unwrap();assert!(!flags.direct_set && !flags.direct && flags.source);
        let legacy:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned('source-ancestry')").fetch_one(&mut *tx).await.unwrap();assert_eq!(flags.source,legacy);
        sqlx::query("ROLLBACK TO SAVEPOINT visit_upload_set").execute(&mut *tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();
        let mut candidates=Vec::new();let mut n=0;
        while let Some(candidate)=witness.candidate(&mut tx,n).await.unwrap(){candidates.push(candidate);n+=1;}
        let mut actual=candidates.into_iter().map(|v|(v.path,v.kind,v.target)).collect::<Vec<_>>();
        actual.sort();
        let mut expected=vec![
            ("direct-set",Some("asset"),Some("asset-ancestry")),
            ("direct-upload",None,None), ("direct-upload-set",None,None),
            ("source-attachment",Some("asset"),Some("asset-ancestry")),
            ("source-upload",Some("asset"),Some("asset-ancestry")),
            ("source-upload-set",Some("asset"),Some("asset-foreign")),
            ("linked-set",Some("asset"),Some("asset-ancestry")),
            ("linked-set",Some("asset"),Some("asset-foreign")),
            ("linked-upload",None,None), ("linked-upload",Some("asset"),Some("asset-foreign")),
            ("linked-upload-set",None,None), ("linked-upload-set",Some("asset"),Some("asset-foreign")),
        ].into_iter().map(|(p,k,t)|(p.to_owned(),k.map(str::to_owned),t.map(str::to_owned))).collect::<Vec<_>>();
        expected.sort();assert_eq!(actual,expected);
        sqlx::query("SAVEPOINT ancestry_change").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_id='changed-ancestry' WHERE id='set-ancestry'").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT ancestry_change").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        // Changing the retained relation collation changes downstream join
        // semantics even if the text bytes themselves are unchanged.
        sqlx::query("SAVEPOINT ancestry_collation").execute(&mut *tx).await.unwrap();
        sqlx::query("CREATE COLLATION fixture_ancestry_nondeterministic (provider=icu, locale='und-u-ks-level2', deterministic=false)").execute(&mut *tx).await.unwrap();
        let relation=erpx_server_workflow::public::record_origin::FieldPaymentTargets::relation_identifier(&witness);
        let ddl=format!("ALTER TABLE {} ALTER COLUMN target TYPE text COLLATE fixture_ancestry_nondeterministic",relation);sqlx::query(sqlx::AssertSqlSafe(ddl)).execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT ancestry_collation").execute(&mut *tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();
        // A same-named replacement or altered temporary relation is never the
        // snapshot originally bound to this witness.
        let table:String=sqlx::query_scalar("SELECT relname::text FROM pg_class WHERE relnamespace=pg_my_temp_schema() AND relname LIKE 'erpx_files_ancestry_%'").fetch_one(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT ancestry_temp_tamper").execute(&mut *tx).await.unwrap();
        let tamper=format!("UPDATE pg_temp.\"{table}\" SET target='forged' WHERE path='direct-set'");
        sqlx::query(sqlx::AssertSqlSafe(tamper)).execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT ancestry_temp_tamper").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT ancestry_search_path").execute(&mut *tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog,public").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT ancestry_search_path").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT ancestry_shape").execute(&mut *tx).await.unwrap();
        let alter=format!("ALTER TABLE pg_temp.\"{table}\" ADD COLUMN extra text");
        sqlx::query(sqlx::AssertSqlSafe(alter)).execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT ancestry_shape").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT ancestry_recreate").execute(&mut *tx).await.unwrap();
        let drop_sql=format!("DROP TABLE pg_temp.\"{table}\"");
        sqlx::query(sqlx::AssertSqlSafe(drop_sql)).execute(&mut *tx).await.unwrap();
        let create_sql=format!("CREATE TEMP TABLE \"{table}\" (path text,link_id text,attachment_id text,upload_intent_id text,set_id text,upload_set_id text,kind text,target text) ON COMMIT DROP");
        sqlx::query(sqlx::AssertSqlSafe(create_sql)).execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT ancestry_recreate").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        // Canonically equivalent Unicode spellings are distinct retained bytes,
        // regardless of the database's default collation.
        sqlx::query("SAVEPOINT ancestry_unicode").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_id='é' WHERE id='set-ancestry'").execute(&mut *tx).await.unwrap();
        let unicode=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry").await.unwrap().unwrap();
        sqlx::query("UPDATE attachment_sets SET target_id='é' WHERE id='set-ancestry'").execute(&mut *tx).await.unwrap();
        assert!(unicode.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT ancestry_unicode").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        let null_source=inspect_field_attachment_ancestry(&mut tx,&company,"link-ancestry-null").await.unwrap().unwrap();
        let mut direct=Vec::new();let mut index=0;
        while let Some(row)=null_source.candidate(&mut tx,index).await.unwrap(){direct.push(row.path);index+=1;}
        direct.sort();assert_eq!(direct,vec!["direct-set","direct-upload","direct-upload-set"]);
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_field_attachment_ancestry(&mut other).await.unwrap();
        assert!(witness.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        let mut released=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *released).await.unwrap();
        sqlx::query("SAVEPOINT before_ancestry_locks").execute(&mut *released).await.unwrap();
        lock_field_attachment_ancestry(&mut released).await.unwrap();
        let lost=inspect_field_attachment_ancestry(&mut released,&company,"link-ancestry").await.unwrap().unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_ancestry_locks").execute(&mut *released).await.unwrap();
        assert!(lost.require_current(&mut released).await.is_err());
        released.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn ancestry_nowait_and_writer_wait_cover_both_lock_orders() {
    use erpx_server_files::public::field_attachment_ancestry::lock_field_attachment_ancestry;
    fixture::with_database(3, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"ancestry-race","company","asset-race","manual","draft","draft","actor","available",1000).await;
        let mut writer=pool.raw().acquire().await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let mut tx=writer.begin().await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_id='held-first' WHERE id='set-ancestry-race'").execute(&mut *tx).await.unwrap();
        let holds:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='attachment_sets'::regclass AND mode='RowExclusiveLock' AND granted)").bind(writer_pid).fetch_one(pool.raw()).await.unwrap();assert!(holds);
        let mut candidate=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *candidate).await.unwrap();
        let denied=lock_field_attachment_ancestry(&mut candidate).await;
        assert!(matches!(&denied,Err(PgTransactionError::Persistence(e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()),"{denied:?}");
        candidate.rollback().await.unwrap();tx.rollback().await.unwrap();drop(writer);
        let mut reader=pool.raw().acquire().await.unwrap();
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let mut retained=reader.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *retained).await.unwrap();
        lock_field_attachment_ancestry(&mut retained).await.unwrap();
        let (send,recv)=tokio::sync::oneshot::channel();let cloned=pool.clone();
        let mut task=tokio::spawn(async move {
            let mut c=cloned.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();
            let mut tx=c.begin().await.unwrap();sqlx::query("UPDATE attachment_sets SET target_id='writer-after-reader' WHERE id='set-ancestry-race'").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        });
        let blocked_pid=recv.await.unwrap();
        let blocked=tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {let yes:bool=sqlx::query_scalar("SELECT $2=ANY(pg_blocking_pids($1))").bind(blocked_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();if yes{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
        }).await;
        retained.rollback().await.unwrap();drop(reader);
        let joined=tokio::time::timeout(std::time::Duration::from_secs(5),&mut task).await;
        if joined.is_err(){task.abort();let _=task.await;}
        assert!(blocked.is_ok(),"expected exact writer PID to wait for ancestry reader");
        joined.expect("writer did not finish after reader rollback").unwrap();
        let target:String=sqlx::query_scalar("SELECT target_id FROM attachment_sets WHERE id='set-ancestry-race'").fetch_one(pool.raw()).await.unwrap();assert_eq!(target,"writer-after-reader");
        let mut fresh=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();
        lock_field_attachment_ancestry(&mut fresh).await.unwrap();
        let witness=erpx_server_files::public::field_attachment_ancestry::inspect_field_attachment_ancestry(&mut fresh,&CompanyId::new("company").unwrap(),"link-ancestry-race").await.unwrap().unwrap();
        let mut found=false;let mut index=0;
        while let Some(row)=witness.candidate(&mut fresh,index).await.unwrap(){if row.path=="direct-set" && row.target.as_deref()==Some("writer-after-reader"){found=true;}index+=1;}
        assert!(found,"fresh witness must read the committed writer target");witness.require_current(&mut fresh).await.unwrap();fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn files_ancestry_feeds_every_workflow_payment_receipt_path_in_one_unit() {
    use erpx_server_files::public::field_attachment_ancestry::{inspect_field_attachment_ancestry,lock_field_attachment_ancestry};
    use erpx_server_workflow::public::record_origin::{inspect_field_payment_receipt_batch,lock_payment_origin_candidate_sources};
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        for (prefix,company) in [("batch-root","company"),("batch-source-set","foreign"),("batch-link","foreign"),("batch-link-set","foreign")] {
            seed(&pool,prefix,company,prefix,"manual","draft","draft","actor","available",1000).await;
        }
        sqlx::raw_sql("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES('upload-payment-batch','finance_payment','pay-source-upload','manual','file','application/pdf',12,'staging/payment-batch','uploaded','actor',11000,1000,1000,'company'); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES('source-payment-batch','upload-payment-batch','finance_payment','pay-attachment','manual','origin/payment-batch',1,'file','application/pdf',12,'checksum','actor',1000,'company','application/pdf','available',1000,1000); UPDATE attachment_links SET attachment_id='source-payment-batch' WHERE id='link-batch-root'; UPDATE upload_intents SET attachment_set_id='set-batch-source-set' WHERE id='upload-payment-batch'; UPDATE attachment_sets SET target_type='finance_payment',target_id='pay-source-set' WHERE id='set-batch-source-set'; UPDATE attachment_sets SET target_type='finance_payment',target_id='pay-link-set' WHERE id='set-batch-link'; UPDATE upload_intents SET owner_type='finance_payment',owner_id='pay-link-upload',attachment_set_id='set-batch-link-set' WHERE id='upload-batch-link'; UPDATE attachment_sets SET target_type='finance_payment',target_id='pay-upload-set' WHERE id='set-batch-link-set'; UPDATE attachment_links SET attachment_id='source-payment-batch',upload_intent_id='upload-batch-link' WHERE id='link-batch-link'").execute(pool.raw()).await.unwrap();
        for (key,record,result) in [("batch-a","pay-attachment",r#"{"settlementInvoiceId":"inv-a"}"#),("batch-a-null","pay-attachment",r#"{"settlementInvoiceId":null}"#),("batch-b","pay-source-upload",r#"{"settlementInvoiceId":"inv-b"}"#),("batch-c","other",r#"{"id":"pay-source-set","settlementInvoiceId":"inv-c"}"#),("batch-d","pay-link-set",r#"{"settlementInvoiceId":"inv-d"}"#),("batch-e","pay-link-upload",r#"{"settlementInvoiceId":"inv-e"}"#),("batch-f","pay-upload-set",r#"{"settlementInvoiceId":"inv-f"}"#)] {
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,'finance.payment.create','actor','fixture',$2,$3,0)").bind(key).bind(record).bind(result).execute(pool.raw()).await.unwrap();
        }
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_payment_origin_candidate_sources(&mut tx).await.unwrap();lock_field_attachment_ancestry(&mut tx).await.unwrap();
        let ancestry=inspect_field_attachment_ancestry(&mut tx,&CompanyId::new("company").unwrap(),"link-batch-root").await.unwrap().unwrap();
        let batch=inspect_field_payment_receipt_batch(&mut tx,&ancestry).await.unwrap();batch.require_current(&mut tx).await.unwrap();
        let query=format!("SELECT path,target,command_id,invoice_id FROM {} ORDER BY path COLLATE \"C\",command_id COLLATE \"C\"",batch.relation_identifier());
        let actual:Vec<(String,String,String,Option<String>)>=sqlx::query_as(sqlx::AssertSqlSafe(query)).fetch_all(&mut *tx).await.unwrap();
        let expected=vec![("linked-set","pay-link-set","batch-d",Some("inv-d")),("linked-upload","pay-link-upload","batch-e",Some("inv-e")),("linked-upload-set","pay-upload-set","batch-f",Some("inv-f")),("source-attachment","pay-attachment","batch-a",Some("inv-a")),("source-attachment","pay-attachment","batch-a-null",None),("source-upload","pay-source-upload","batch-b",Some("inv-b")),("source-upload-set","pay-source-set","batch-c",Some("inv-c"))].into_iter().map(|(a,b,c,d)|(a.to_owned(),b.to_owned(),c.to_owned(),d.map(str::to_owned))).collect::<Vec<_>>();assert_eq!(actual,expected);
        sqlx::query("SAVEPOINT batch_new_receipt").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('batch-late','finance.payment.create','actor','fixture','pay-attachment','{}',0)").execute(&mut *tx).await.unwrap();assert!(batch.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT batch_new_receipt").execute(&mut *tx).await.unwrap();batch.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT batch_tamper").execute(&mut *tx).await.unwrap();let sql=format!("DELETE FROM {} WHERE command_id='batch-a-null'",batch.relation_identifier());sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&mut *tx).await.unwrap();assert!(batch.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT batch_tamper").execute(&mut *tx).await.unwrap();batch.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT batch_collation").execute(&mut *tx).await.unwrap();sqlx::query("CREATE COLLATION fixture_batch_nondeterministic (provider=icu, locale='und-u-ks-level2', deterministic=false)").execute(&mut *tx).await.unwrap();let sql=format!("ALTER TABLE {} ALTER COLUMN invoice_id TYPE text COLLATE fixture_batch_nondeterministic",batch.relation_identifier());sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&mut *tx).await.unwrap();assert!(batch.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT batch_collation").execute(&mut *tx).await.unwrap();batch.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT batch_trigger").execute(&mut *tx).await.unwrap();sqlx::raw_sql("CREATE FUNCTION pg_temp.fixture_batch_trigger() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$").execute(&mut *tx).await.unwrap();let sql=format!("CREATE TRIGGER injected BEFORE INSERT ON {} FOR EACH ROW EXECUTE FUNCTION pg_temp.fixture_batch_trigger()",batch.relation_identifier());sqlx::query(sqlx::AssertSqlSafe(sql)).execute(&mut *tx).await.unwrap();assert!(batch.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT batch_trigger").execute(&mut *tx).await.unwrap();batch.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_payment_origin_candidate_sources(&mut other).await.unwrap();lock_field_attachment_ancestry(&mut other).await.unwrap();assert!(batch.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
    }).await;
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

#[tokio::test]
async fn finance_target_metadata_is_bound_to_actual_files_witness() {
    use erpx_server_files::public::attachment_read::finance_attachment_target;
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"finance-target","company","asset-1","manual","draft","draft","actor","available",1000).await;
        let company=CompanyId::new("company").unwrap();
        let long_id=format!("journal-{}","x".repeat(512));
        sqlx::query("UPDATE attachment_sets SET target_type='finance_journal',target_id=$1,purpose_key='finance_support',component_kind='record',component_ref=NULL WHERE id='set-finance-target'").bind(&long_id).execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_attachment_ownership_sources(&mut tx).await.unwrap();
        let witness=inspect_attachment_ownership(&mut tx,&company,"link-finance-target").await.unwrap().unwrap();
        let target=finance_attachment_target(&mut tx,&witness).await.unwrap().unwrap();
        assert_eq!((&target.target_type,&target.target_id,target.purpose_key.as_str(),target.component_kind.as_str(),target.component_ref.as_deref()),(&"finance_journal".to_owned(),&long_id,"finance_support","record",None));
        sqlx::query("UPDATE attachment_sets SET target_type='finance_payment',component_kind='action',component_ref='bank-confirmation' WHERE id='set-finance-target'").execute(&mut *tx).await.unwrap();
        assert!(finance_attachment_target(&mut tx,&witness).await.is_err());
        let payment=inspect_attachment_ownership(&mut tx,&company,"link-finance-target").await.unwrap().unwrap();
        let target=finance_attachment_target(&mut tx,&payment).await.unwrap().unwrap();assert_eq!(target.target_type,"finance_payment");assert_eq!(target.target_id,long_id);assert_eq!(target.component_ref.as_deref(),Some("bank-confirmation"));
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_attachment_ownership_sources(&mut other).await.unwrap();assert!(finance_attachment_target(&mut other,&payment).await.is_err());other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn field_visit_link_requires_and_retains_all_owner_locks() {
    use erpx_server_files::public::field_visit_link::{lock_field_visit_link_sources,read_field_visit_link,require_field_visit_link_sources_held};
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"field-lock","company","visit-1","site_photo","draft","draft","actor","available",1000).await;
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',component_kind='record',purpose_key='site_photo' WHERE id='set-field-lock'").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let mut outside=pool.raw().acquire().await.unwrap();assert!(require_field_visit_link_sources_held(&mut outside).await.is_err());drop(outside);
        let mut insufficient=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *insufficient).await.unwrap();
        lock_attachment_ownership_sources(&mut insufficient).await.unwrap();
        assert!(require_field_visit_link_sources_held(&mut insufficient).await.is_err());
        assert!(read_field_visit_link(&mut insufficient,&company,"link-field-lock").await.is_err());insufficient.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_field_link_locks").execute(&mut *tx).await.unwrap();
        lock_field_visit_link_sources(&mut tx).await.unwrap();
        require_field_visit_link_sources_held(&mut tx).await.unwrap();
        let modes:(i64,i64)=sqlx::query_as("SELECT (SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareRowExclusiveLock' AND relation IN ('attachment_links'::regclass,'attachment_sets'::regclass)),(SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareLock' AND relation IN ('attachments'::regclass,'attachment_scan_attempts'::regclass))").fetch_one(&mut *tx).await.unwrap();assert_eq!(modes,(2,2));
        let source=read_field_visit_link(&mut tx,&company,"link-field-lock").await.unwrap();assert_eq!(source.source().target_id,"visit-1");source.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_field_link_locks").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());assert!(require_field_visit_link_sources_held(&mut tx).await.is_err());
        sqlx::query("LOCK TABLE attachment_links,attachment_sets,attachments,attachment_scan_attempts IN EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_field_visit_link_sources_held(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn retained_field_withdrawal_checks_deferred_drift_replay_and_existing_event() {
    use erpx_server_files::public::field_visit_link::{lock_field_visit_link_sources,read_field_visit_link};
    use sha2::{Digest,Sha256};
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"withdraw-retained","company","visit-1","site_photo","draft","draft","actor","available",1000).await;
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',component_kind='record',purpose_key='site_photo' WHERE id='set-withdraw-retained'").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION files_deferred_withdraw_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE attachment_links SET caption='deferred tamper' WHERE id=NEW.link_id; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER files_deferred_withdraw_drift AFTER INSERT ON attachment_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.action='link.withdrawn') EXECUTE FUNCTION files_deferred_withdraw_drift()").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_visit_link_sources(&mut tx).await.unwrap();
        let source=read_field_visit_link(&mut tx,&company,"link-withdraw-retained").await.unwrap();
        let retained=source.withdraw_retained(&mut tx,"actor","No longer needed","deferred-key",20).await.unwrap();retained.require_current(&mut tx).await.unwrap();
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await.unwrap();assert!(retained.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
        let restored:(String,Option<String>,i64)=sqlx::query_as("SELECT state,caption,(SELECT count(*) FROM attachment_events WHERE link_id='link-withdraw-retained') FROM attachment_links WHERE id='link-withdraw-retained'").fetch_one(pool.raw()).await.unwrap();assert_eq!(restored,("draft".into(),None,0));
        sqlx::raw_sql("DROP TRIGGER files_deferred_withdraw_drift ON attachment_events; DROP FUNCTION files_deferred_withdraw_drift()").execute(pool.raw()).await.unwrap();
        // A pre-existing event with the deterministic key remains unchanged; it
        // is retained as its actual image, not manufactured as our new event.
        let hash=Sha256::digest(b"company\0link-withdraw-retained\0existing-key");let event=format!("attachment-withdraw-{}",&hex::encode(hash)[..32]);
        sqlx::query("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,'company',NULL,'link-withdraw-retained','set-withdraw-retained','other','link.withdrawn','{}',7)").bind(&event).execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_visit_link_sources(&mut tx).await.unwrap();
        let source=read_field_visit_link(&mut tx,&company,"link-withdraw-retained").await.unwrap();let retained=source.withdraw_retained(&mut tx,"actor","No longer needed","existing-key",20).await.unwrap();sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await.unwrap();retained.require_current(&mut tx).await.unwrap();tx.commit().await.unwrap();
        let event_actor:String=sqlx::query_scalar("SELECT actor_id FROM attachment_events WHERE id=$1").bind(&event).fetch_one(pool.raw()).await.unwrap();assert_eq!(event_actor,"other");
        let mut replay=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *replay).await.unwrap();lock_field_visit_link_sources(&mut replay).await.unwrap();
        let source=read_field_visit_link(&mut replay,&company,"link-withdraw-retained").await.unwrap();let retained=source.withdraw_retained(&mut replay,"actor","No longer needed","different-key",21).await.unwrap();retained.require_current(&mut replay).await.unwrap();replay.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn retained_status_access_checks_same_unit_exact_event_and_absent_attachment() {
    use erpx_server_files::public::field_visit_link::{lock_field_visit_link_sources,read_field_visit_link,write_status_access_retained};
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"status-retained","company","visit-1","site_photo","draft","draft","actor","available",1000).await;
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',component_kind='record',purpose_key='site_photo' WHERE id='set-status-retained'").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        sqlx::raw_sql("CREATE FUNCTION status_access_reason_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.reason='unexpected reason'; RETURN NEW; END $$; CREATE TRIGGER status_access_reason_tamper BEFORE INSERT ON attachment_access_events FOR EACH ROW EXECUTE FUNCTION status_access_reason_tamper()").execute(pool.raw()).await.unwrap();
        let mut tampered=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tampered).await.unwrap();lock_field_visit_link_sources(&mut tampered).await.unwrap();
        let source=read_field_visit_link(&mut tampered,&company,"link-status-retained").await.unwrap();
        let error=write_status_access_retained(&mut tampered,&company,source.source(),"actor",1019).await;
        assert!(matches!(error,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(ref e)) if e.code=="ATTACHMENT_LINK_LOCKED"));
        tampered.rollback().await.unwrap();let count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_access_events WHERE link_id='link-status-retained'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
        sqlx::raw_sql("DROP TRIGGER status_access_reason_tamper ON attachment_access_events; DROP FUNCTION status_access_reason_tamper()").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_visit_link_sources(&mut tx).await.unwrap();
        let link=read_field_visit_link(&mut tx,&company,"link-status-retained").await.unwrap();
        sqlx::query("SAVEPOINT before_status_access").execute(&mut *tx).await.unwrap();
        let retained=write_status_access_retained(&mut tx,&company,link.source(),"actor",1020).await.unwrap();
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await.unwrap();retained.require_current(&mut tx).await.unwrap();
        let event:(String,String,String,String,Option<String>,i64)=sqlx::query_as("SELECT attachment_id,link_id,actor_id,outcome,reason,created_at FROM attachment_access_events WHERE link_id='link-status-retained' AND variant='status'").fetch_one(&mut *tx).await.unwrap();assert_eq!(event,("source-status-retained".into(),"link-status-retained".into(),"actor".into(),"allowed".into(),None,1020));
        sqlx::query("ROLLBACK TO SAVEPOINT before_status_access").execute(&mut *tx).await.unwrap();assert!(retained.require_current(&mut tx).await.is_err());
        sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-status-retained'").execute(&mut *tx).await.unwrap();
        let empty_link=read_field_visit_link(&mut tx,&company,"link-status-retained").await.unwrap();let empty=write_status_access_retained(&mut tx,&company,empty_link.source(),"actor",1021).await.unwrap();sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await.unwrap();empty.require_current(&mut tx).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_access_events WHERE link_id='link-status-retained'").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,0);tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_field_visit_link_sources(&mut other).await.unwrap();assert!(empty.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn no_temp_files_workflow_routing_uses_public_sources_without_temp_relations() {
    use erpx_server_files::public::field_attachment_routing::{inspect_field_attachment_routing,lock_field_attachment_routing_sources};
    use erpx_server_workflow::public::record_origin::{inspect_field_payment_routing,lock_field_payment_routing_sources};
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"no-temp-routing","company","asset-routing","manual","draft","draft","actor","available",1000).await;
        sqlx::query("UPDATE upload_intents SET owner_type='finance_payment',owner_id='routing-payment' WHERE id='upload-no-temp-routing'").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('routing-receipt','finance.payment.create','actor','fixture','routing-payment','{\"settlementInvoiceId\":\"routing-invoice\"}',0)").execute(pool.raw()).await.unwrap();
        // The existing leased-database role lacks CREATEROLE; this verifies
        // no TEMP relation is created, not runtime-role privilege qualification.
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_field_payment_routing_sources(&mut tx).await.unwrap();lock_field_attachment_routing_sources(&mut tx).await.unwrap();
        let temp_before:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();
        let source=inspect_field_attachment_routing(&mut tx,&CompanyId::new("company").unwrap(),"link-no-temp-routing").await.unwrap().unwrap();
        let route=inspect_field_payment_routing(&mut tx,&source).await.unwrap();
        let sql=format!("{} SELECT count(*) FROM payment_receipts WHERE path='source-upload' AND command_id='routing-receipt' AND invoice_id='routing-invoice'",route.payment_cte());
        let count:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(route.selected_link_id()).bind(route.company_id()).fetch_one(&mut *tx).await.unwrap();assert_eq!(count,1);
        source.require_current(&mut tx).await.unwrap();route.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT selected_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE public.attachment_sets SET target_id='changed' WHERE id='set-no-temp-routing'").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT selected_drift").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        // Workflow retains a query, not a receipt image. A consumer must
        // recompute its complete result after same-unit effects.
        sqlx::query("SAVEPOINT late_receipt").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('routing-late','finance.payment.create','actor','fixture','routing-payment','{\"settlementInvoiceId\":\"routing-invoice\"}',0)").execute(&mut *tx).await.unwrap();
        route.require_current(&mut tx).await.unwrap();
        let sql=format!("{} SELECT count(*) FROM payment_receipts WHERE path='source-upload' AND invoice_id='routing-invoice'",route.payment_cte());
        let count:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(route.selected_link_id()).bind(route.company_id()).fetch_one(&mut *tx).await.unwrap();assert_eq!(count,2);
        sqlx::query("ROLLBACK TO SAVEPOINT late_receipt").execute(&mut *tx).await.unwrap();route.require_current(&mut tx).await.unwrap();
        let temp_after:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(temp_after,temp_before);
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn no_temp_routing_ignores_pg_temp_shadow_before_and_after_capture() {
    use erpx_server_files::public::field_attachment_routing::{inspect_field_attachment_routing,lock_field_attachment_routing_sources};
    use erpx_server_workflow::public::record_origin::{inspect_field_payment_routing,lock_field_payment_routing_sources};
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"shadow-routing","company","asset-shadow","manual","draft","draft","actor","available",1000).await;
        let mut c=pool.raw().acquire().await.unwrap();
        // Pre-existing empty shadow persists across transaction boundaries.
        sqlx::query("CREATE TEMP TABLE attachment_sets (id text,company_id text)").execute(&mut *c).await.unwrap();
        let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_field_payment_routing_sources(&mut tx).await.unwrap();lock_field_attachment_routing_sources(&mut tx).await.unwrap();
        let source=inspect_field_attachment_routing(&mut tx,&CompanyId::new("company").unwrap(),"link-shadow-routing").await.unwrap().unwrap();
        let route=inspect_field_payment_routing(&mut tx,&source).await.unwrap();
        assert!(!source.field_visit_routing().direct);source.require_current(&mut tx).await.unwrap();route.require_current(&mut tx).await.unwrap();
        // Also shadow a second source after the witness was captured. The
        // search_path setting itself remains unchanged by implicit pg_temp.
        sqlx::query("CREATE TEMP TABLE attachment_links (id text,set_id text)").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();route.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn no_temp_selected_link_observation_retains_scoped_absence_and_present_routing() {
    use erpx_server_files::public::field_attachment_routing::{observe_field_attachment_routing,lock_field_attachment_routing_sources};
    fixture::with_database(3, |pool| async move {
        migrate(&pool).await;
        seed(&pool,"observation","company","asset-observation","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"observation-foreign","foreign","asset-foreign","manual","draft","draft","actor","available",1000).await;
        let company=CompanyId::new("company").unwrap();
        let mut missing_lock=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *missing_lock).await.unwrap();
        assert!(observe_field_attachment_routing(&mut missing_lock,&company,"missing-observation").await.is_err());missing_lock.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_attachment_routing_sources(&mut tx).await.unwrap();
        let present=observe_field_attachment_routing(&mut tx,&company,"link-observation").await.unwrap();
        assert_eq!(present.company_id(),"company");assert_eq!(present.selected_link_id(),"link-observation");assert!(present.routing().is_some());
        // Obtain the query and parameter order by calling the real TS owner
        // function; the local test substitutes SQLx parameters, never user SQL.
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);const db={prepare(sql){return {bind(...args){return {async first(){return {sql,args}}}}}}};const x=await m.attachmentReadRoute(db,'company','link-observation');let n=0;process.stdout.write(JSON.stringify({sql:x.sql.replace(/\?/g,()=>'$'+(++n)),args:x.args}));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/private-attachment-reads.ts");
        let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).arg("--conditions=react-server").arg("-e").arg(script).arg(path).output().unwrap();
        assert!(output.status.success(),"actual Files attachmentReadRoute oracle unavailable");
        let oracle:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(oracle["args"],serde_json::json!(["link-observation","company"]));
        let route_sql=format!("SELECT row_to_json(route) FROM ({}) route",oracle["sql"].as_str().unwrap());
        let real:Option<serde_json::Value>=sqlx::query_scalar(sqlx::AssertSqlSafe(route_sql.as_str())).bind("link-observation").bind("company").fetch_optional(&mut *tx).await.unwrap();
        assert_eq!(real,Some(serde_json::to_value(present.routing().unwrap().read_route()).unwrap()));
        for absent_id in ["missing-observation","link-observation-foreign"] {
            let absent:Option<serde_json::Value>=sqlx::query_scalar(sqlx::AssertSqlSafe(route_sql.as_str())).bind(absent_id).bind("company").fetch_optional(&mut *tx).await.unwrap();assert!(absent.is_none());
        }
        present.require_current(&mut tx).await.unwrap();
        let missing=observe_field_attachment_routing(&mut tx,&company,"missing-observation").await.unwrap();assert!(missing.routing().is_none());
        let foreign=observe_field_attachment_routing(&mut tx,&company,"link-observation-foreign").await.unwrap();assert!(foreign.routing().is_none());
        sqlx::query("SAVEPOINT observation_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('missing-observation','set-observation',NULL,'draft','actor',1000,11000)").execute(&mut *tx).await.unwrap();
        assert!(missing.require_current(&mut tx).await.is_err());foreign.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE public.attachment_sets SET target_id='changed' WHERE id='set-observation'").execute(&mut *tx).await.unwrap();assert!(present.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT observation_drift").execute(&mut *tx).await.unwrap();missing.require_current(&mut tx).await.unwrap();foreign.require_current(&mut tx).await.unwrap();present.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut later=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *later).await.unwrap();lock_field_attachment_routing_sources(&mut later).await.unwrap();assert!(missing.require_current(&mut later).await.is_err());later.rollback().await.unwrap();
        let mut save=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *save).await.unwrap();sqlx::query("SAVEPOINT before_routing_locks").execute(&mut *save).await.unwrap();lock_field_attachment_routing_sources(&mut save).await.unwrap();let saved=observe_field_attachment_routing(&mut save,&company,"missing-observation").await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_routing_locks").execute(&mut *save).await.unwrap();assert!(saved.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn global_visit_link_marker_matches_actual_typescript_without_finance_markers() {
    use erpx_server_files::public::field_attachment_routing::{observe_global_field_visit_link,observe_unscoped_attachment_link,lock_field_attachment_routing_sources};
    fixture::with_database(3,|pool|async move {
        migrate(&pool).await;
        seed(&pool,"global-local","company","asset-local","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"global-foreign","foreign","visit-foreign","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"global-null-root","company","asset-null","manual","draft","draft","actor","available",1000).await;
        sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-global-null-root'").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',purpose_key='site_photo',component_kind='record' WHERE id='set-global-foreign'").execute(pool.raw()).await.unwrap();
        // Call the original TS function, observing its third query after both
        // earlier Finance branches return false. No mocked SQL is invented.
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);let i=0,query='';const db={prepare(sql){return {bind(...args){return {async first(){i++;if(i===3)query=sql;return {marker:false,scoped:false}}}}}}};await m.fieldFileWriterOwnerInDatabase(db,{companyId:'company'},{kind:'link',id:'sample'});if(i!==3||!query)throw Error('VISIT_BRANCH_MISSING');let n=0;process.stdout.write(query.replace(/\?/g,()=>'$'+(++n)));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/field-file-writer-owner.ts");
        let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).arg("--conditions=react-server").arg("-e").arg(script).arg(path).output().unwrap();
        assert!(output.status.success(),"actual TS Field link marker unavailable");let sql=String::from_utf8(output.stdout).unwrap();
        let company=CompanyId::new("company").unwrap();let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_field_attachment_routing_sources(&mut tx).await.unwrap();
        // The installed TS-oracle function also reads Finance, Workflow and
        // Inventory. Protect those real sources before the first snapshot.
        sqlx::query("LOCK TABLE public.finance_invoices,public.finance_allocations,public.finance_journals,public.command_receipts,public.operational_inventory_consumptions IN SHARE MODE NOWAIT").execute(&mut *tx).await.unwrap();
        for (id,marker,scoped) in [("link-global-local",false,true),("link-global-foreign",true,false),("link-global-null-root",false,true),("missing-global",false,false)] {
            let witness=observe_global_field_visit_link(&mut tx,&company,id).await.unwrap();
            let root=observe_unscoped_attachment_link(&mut tx,&company,id).await.unwrap();
            let ts:(bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str())).bind(id).bind(id).bind("company").fetch_one(&mut *tx).await.unwrap();
            let joined=format!("WITH {} SELECT EXISTS(SELECT 1 FROM file_unscoped_link l WHERE l.set_type='field_service_visit' OR l.upload_type='field_service_visit' OR l.upload_set_type='field_service_visit' OR erpx_files_field_attachment_owned(l.attachment_id)),(SELECT scoped FROM file_link_scope)",root.cte_sql());
            let raw:(bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(joined.as_str())).bind(root.selected_link_id()).bind(root.company_id()).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(raw,ts,"{id}");assert_eq!((witness.facts().marker,witness.facts().scoped),ts,"{id}");assert_eq!(ts,(marker,scoped));assert_eq!(root.scoped(),scoped);assert_eq!(root.exists(),id!="missing-global");
            if id=="link-global-null-root" {let q=format!("WITH {} SELECT attachment_id IS NULL FROM file_unscoped_link",root.cte_sql());let is_null:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(q.as_str())).bind(root.selected_link_id()).bind(root.company_id()).fetch_one(&mut *tx).await.unwrap();assert!(is_null);}
            root.require_current(&mut tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();
        }
        let foreign=observe_global_field_visit_link(&mut tx,&company,"link-global-foreign").await.unwrap();
        let root=observe_unscoped_attachment_link(&mut tx,&company,"link-global-foreign").await.unwrap();
        sqlx::query("SAVEPOINT global_visit_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_type='asset',purpose_key='manual' WHERE id='set-global-foreign'").execute(&mut *tx).await.unwrap();
        assert!(foreign.require_current(&mut tx).await.is_err());assert!(root.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT global_visit_drift").execute(&mut *tx).await.unwrap();foreign.require_current(&mut tx).await.unwrap();root.require_current(&mut tx).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(observe_global_field_visit_link(&mut other,&company,"link-global-local").await.is_err());other.rollback().await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn global_attachment_ancestry_matches_installed_visit_marker_and_null() {
    use erpx_server_files::public::field_attachment_routing::{inspect_field_attachment_routing,observe_global_attachment_ancestry,lock_field_attachment_routing_sources};
    fixture::with_database(3,|pool|async move {
        migrate(&pool).await;
        seed(&pool,"global-source","company","asset-one","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"global-related","foreign","visit-one","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"global-null","company","asset-null","manual","draft","draft","actor","available",1000).await;
        // A different Company's link supplies the actual Field ancestry.
        sqlx::query("UPDATE attachment_links SET attachment_id='source-global-source' WHERE id='link-global-related'").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',purpose_key='site_photo',component_kind='record' WHERE id='set-global-related'").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-global-null'").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_attachment_routing_sources(&mut tx).await.unwrap();
        assert!(inspect_field_attachment_routing(&mut tx,&company,"missing-global-root").await.unwrap().is_none());
        for (id,expected_id,expected) in [("link-global-source",Some("source-global-source"),true),("link-global-null",None,false)] {
            let selected=inspect_field_attachment_routing(&mut tx,&company,id).await.unwrap().unwrap();let source=observe_global_attachment_ancestry(&mut tx,&selected).await.unwrap();
            assert_eq!(source.attachment_id(),expected_id);
            let query=format!("WITH {} SELECT EXISTS(SELECT 1 FROM file_global_attachment_source a WHERE a.attachment_owner_type='field_service_visit' OR a.upload_owner_type='field_service_visit' OR a.upload_set_type='field_service_visit') OR EXISTS(SELECT 1 FROM file_global_related_links l WHERE l.set_type='field_service_visit' OR l.upload_owner_type='field_service_visit' OR l.upload_set_type='field_service_visit')",source.cte_sql());
            let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str())).bind(source.selected_link_id()).bind(source.company_id()).fetch_one(&mut *tx).await.unwrap();
            let oracle:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned($1)").bind(source.attachment_id()).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(actual,oracle,"{id}");assert_eq!(actual,expected,"{id}");source.require_current(&mut tx).await.unwrap();
            if expected {
                sqlx::query("SAVEPOINT global_related_drift").execute(&mut *tx).await.unwrap();
                sqlx::query("UPDATE attachment_sets SET target_type='asset',purpose_key='manual' WHERE id='set-global-related'").execute(&mut *tx).await.unwrap();
                assert!(source.require_current(&mut tx).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT global_related_drift").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
            }
        }
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn unscoped_global_ancestry_matches_real_ts_for_foreign_related_and_null() {
    use erpx_server_files::public::field_attachment_routing::{observe_unscoped_attachment_link,observe_unscoped_global_attachment_ancestry,lock_field_attachment_routing_sources};
    fixture::with_database(3,|pool|async move {
        migrate(&pool).await;
        seed(&pool,"unscoped-root","foreign","asset-foreign","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"unscoped-related","company","visit-related","manual","draft","draft","actor","available",1000).await;
        seed(&pool,"unscoped-null","company","asset-null","manual","draft","draft","actor","available",1000).await;
        sqlx::query("UPDATE attachment_links SET attachment_id='source-unscoped-root' WHERE id='link-unscoped-related'").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE attachment_sets SET target_type='field_service_visit',purpose_key='site_photo',component_kind='record' WHERE id='set-unscoped-related'").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE attachment_links SET attachment_id=NULL WHERE id='link-unscoped-null'").execute(pool.raw()).await.unwrap();
        let script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const m=await import(process.argv[1]);let i=0,query='';const db={prepare(sql){return {bind(...args){return {async first(){i++;if(i===3)query=sql;return {marker:false,scoped:false}}}}}}};await m.fieldFileWriterOwnerInDatabase(db,{companyId:'company'},{kind:'link',id:'sample'});if(i!==3||!query)throw Error('VISIT_BRANCH_MISSING');let n=0;process.stdout.write(query.replace(/\?/g,()=>'$'+(++n)));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters/field-file-writer-owner.ts");let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).arg("--conditions=react-server").arg("-e").arg(script).arg(path).output().unwrap();assert!(output.status.success(),"actual TS Field link marker unavailable");let ts_sql=String::from_utf8(output.stdout).unwrap();
        let company=CompanyId::new("company").unwrap();let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_field_attachment_routing_sources(&mut tx).await.unwrap();sqlx::query("LOCK TABLE public.finance_invoices,public.finance_allocations,public.finance_journals,public.command_receipts,public.operational_inventory_consumptions IN SHARE MODE NOWAIT").execute(&mut *tx).await.unwrap();
        for (id,exists,scoped,marker) in [("link-unscoped-root",true,false,true),("link-unscoped-null",true,true,false),("missing-unscoped",false,false,false)] {
            let root=observe_unscoped_attachment_link(&mut tx,&company,id).await.unwrap();let ancestry=observe_unscoped_global_attachment_ancestry(&mut tx,&root).await.unwrap();
            assert_eq!((ancestry.link_exists(),ancestry.scoped()),(exists,scoped));
            let query=format!("WITH {}, {} SELECT EXISTS(SELECT 1 FROM file_unscoped_link l WHERE l.set_type='field_service_visit' OR l.upload_type='field_service_visit' OR l.upload_set_type='field_service_visit') OR EXISTS(SELECT 1 FROM file_global_attachment_source a WHERE a.attachment_owner_type='field_service_visit' OR a.upload_owner_type='field_service_visit' OR a.upload_set_type='field_service_visit') OR EXISTS(SELECT 1 FROM file_global_related_links l WHERE l.set_type='field_service_visit' OR l.upload_owner_type='field_service_visit' OR l.upload_set_type='field_service_visit'),(SELECT scoped FROM file_link_scope)",ancestry.source_cte_sql(),ancestry.cte_sql());
            let actual:(bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(query.as_str())).bind(ancestry.selected_link_id()).bind(ancestry.company_id()).fetch_one(&mut *tx).await.unwrap();
            let expected:(bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(ts_sql.as_str())).bind(id).bind(id).bind("company").fetch_one(&mut *tx).await.unwrap();assert_eq!(actual,expected,"{id}");assert_eq!(actual,(marker,scoped));ancestry.require_current(&mut tx).await.unwrap();
            if id=="link-unscoped-null" {let q=format!("WITH {} SELECT attachment_id IS NULL FROM file_unscoped_link",ancestry.source_cte_sql());let null:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(q.as_str())).bind(id).bind("company").fetch_one(&mut *tx).await.unwrap();assert!(null);}
            if id=="link-unscoped-root" {
                sqlx::query("SAVEPOINT related_source_drift").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE attachment_sets SET target_type='asset',purpose_key='manual' WHERE id='set-unscoped-related'").execute(&mut *tx).await.unwrap();
                // Root is unchanged; only recomputing the global marker finds
                // this unrelated-link effect, so no full-image claim is made.
                ancestry.require_current(&mut tx).await.unwrap();let after:(bool,bool)=sqlx::query_as(sqlx::AssertSqlSafe(query.as_str())).bind(id).bind("company").fetch_one(&mut *tx).await.unwrap();assert_eq!(after,(false,false));
                sqlx::query("ROLLBACK TO SAVEPOINT related_source_drift").execute(&mut *tx).await.unwrap();
            }
        }
        tx.rollback().await.unwrap();
    }).await;
}
