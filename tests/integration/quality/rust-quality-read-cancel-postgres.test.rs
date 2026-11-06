//! Native Quality reads and cancellation over real PostgreSQL units.
#[path = "../../support/quality/quality-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_quality::public::quality_http::{
    PgQualityCancellation, PgQualityRead, apply_quality_cancellation, lock_quality_read_sources,
    lock_quality_cancellation_writes,
    read_quality_detail, read_quality_workspace, prepare_quality_cancellation,
    read_quality_cancellation_history_page, read_quality_detail_core,
};
use erpx_server_quality::public::quality_read::{
    CancellationAction, HoldStatus, QualityActor, QualityCancellationCommand, cancel_quality_hold,
    quality_detail, quality_detail_core_from_source, quality_workspace,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, RecordId, TimestampMillis, UserId,
};

// Test-only stand-in for Inventory's same-unit posted Count port. Production
// composition must use the actual Inventory public witness.
struct FixtureCount;
struct FixtureCountWitness { request: erpx_server_quality::public::purchase_lineage::CountLineageRequest, tx: String }

#[tokio::test]
async fn quality_inspection_attachment_reference_witness_retains_scope_and_absence() {
    use erpx_server_quality::public::quality_inspection_attachment_refs::{inspect_quality_inspection_attachment_refs, lock_quality_inspection_attachment_refs};
    fixture::with_database(3, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "refs-hold").await;
        sqlx::raw_sql("INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at) VALUES('refs-inspection','refs-hold',1,'pass','evidence','inspector',1),('refs-inspection-2','refs-hold',1,'pass','evidence','inspector',1); INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('refs-upload','quality_hold','refs-hold','quality_inspection_evidence','evidence.txt','text/plain',1,'refs-staging','ready','inspector',100,1,1),('refs-upload-2','quality_hold','refs-hold','quality_inspection_evidence','evidence2.txt','text/plain',1,'refs-staging-2','ready','inspector',100,1,1); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES('refs-file','refs-upload','quality_hold','refs-hold','quality_inspection_evidence','refs-object',1,'evidence.txt','text/plain',1,'checksum','inspector',1),('refs-file-2','refs-upload-2','quality_hold','refs-hold','quality_inspection_evidence','refs-object-2',1,'evidence2.txt','text/plain',1,'checksum','inspector',1)").execute(pool.raw()).await.unwrap();
        let hold = RecordId::new("refs-hold").unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(inspect_quality_inspection_attachment_refs(&mut tx, &company(), Some(&branch()), &hold).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_inspection_attachment_refs(&mut tx).await.unwrap();
        let empty = inspect_quality_inspection_attachment_refs(&mut tx, &company(), Some(&branch()), &hold).await.unwrap();
        assert!(empty.refs().is_empty());
        assert!(inspect_quality_inspection_attachment_refs(&mut tx, &CompanyId::new("foreign").unwrap(), None, &hold).await.is_err());
        assert!(inspect_quality_inspection_attachment_refs(&mut tx, &company(), Some(&RecordId::new("branch-other").unwrap()), &hold).await.is_err());
        sqlx::query("SAVEPOINT refs_before_insert").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO inventory_quality_inspection_attachments(inspection_id,attachment_id,ordinal) VALUES('refs-inspection','refs-file',-1)").execute(&mut *tx).await.unwrap();
        assert!(empty.require_current(&mut tx).await.is_err());
        let present = inspect_quality_inspection_attachment_refs(&mut tx, &company(), Some(&branch()), &hold).await.unwrap();
        assert_eq!(present.refs().len(), 1);
        assert_eq!(present.refs()[0].inspection_id, "refs-inspection");
        assert_eq!(present.refs()[0].attachment_id, "refs-file");
        assert_eq!(present.refs()[0].ordinal, -1);
        present.require_current(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO inventory_quality_inspection_attachments(inspection_id,attachment_id,ordinal) VALUES('refs-inspection-2','refs-file-2',-1)").execute(&mut *tx).await.unwrap();
        assert!(present.require_current(&mut tx).await.is_err());
        let tied = inspect_quality_inspection_attachment_refs(&mut tx, &company(), Some(&branch()), &hold).await.unwrap();
        assert_eq!(tied.refs().len(), 2);
        assert!(tied.refs().iter().any(|r| r.inspection_id == "refs-inspection" && r.attachment_id == "refs-file" && r.ordinal == -1));
        assert!(tied.refs().iter().any(|r| r.inspection_id == "refs-inspection-2" && r.attachment_id == "refs-file-2" && r.ordinal == -1));
        tied.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT refs_before_insert").execute(&mut *tx).await.unwrap();
        empty.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_quality_inspection_attachment_refs(&mut other).await.unwrap();
        assert!(empty.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[test]
fn different_parent_id_decompositions_cannot_share_both_lineage_suffixes() {
    // Both parents can produce the same child ID, but their Count suffixes
    // require incompatible source-ID suffixes (`-remaining-` vs `:remaining:`).
    let child_a=format!("{}-remaining-{}","p","a-remaining-b");
    let child_b=format!("{}-remaining-{}","p-remaining-a","b");
    assert_eq!(child_a,child_b);
    let source_a=format!("{}:remaining:{}","origin","a-remaining-b");
    assert!(!source_a.ends_with(":remaining:b"));
}
impl erpx_server_quality::public::purchase_lineage::QualityLineageCountPort for FixtureCount {
    type Witness = FixtureCountWitness;
    fn inspect<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,r:erpx_server_quality::public::purchase_lineage::CountLineageRequest)->erpx_server_runtime::public::postgres::PgWorkFuture<'a,Option<Self::Witness>,BusinessError> {
        Box::pin(async move {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_counts WHERE id=$1 AND company_id=$2 AND branch_id=$3 AND warehouse_id=$4 AND quality_hold_id=$5 AND status='posted')").bind(&r.count_id).bind(r.company_id.as_str()).bind(&r.branch_id).bind(&r.warehouse_id).bind(&r.hold_id).fetch_one(&mut *c).await?;
            let tx:String=sqlx::query_scalar("SELECT pg_current_xact_id()::text").fetch_one(c).await?;
            Ok(valid.then_some(FixtureCountWitness{request:r,tx}))
        })
    }
    fn require_current<'a>(&'a mut self,c:&'a mut sqlx::PgConnection,w:&'a Self::Witness)->erpx_server_runtime::public::postgres::PgWorkFuture<'a,(),BusinessError> {
        Box::pin(async move {
            let tx:String=sqlx::query_scalar("SELECT pg_current_xact_id()::text").fetch_one(&mut *c).await?;
            if tx!=w.tx || self.inspect(c,w.request.clone()).await?.is_none() { return Err(BusinessError::invalid("COUNT_CHANGED","Count changed.").into()); }
            Ok(())
        })
    }
}

#[tokio::test]
async fn purchase_lineage_follows_only_posted_reciprocal_count_and_disposition() {
    use erpx_server_quality::public::purchase_lineage::{QualityLineageRequest,inspect_quality_purchase_lineage,lock_quality_purchase_lineage};
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool).await;
        sqlx::raw_sql("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id) VALUES('root','company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0,'partial',3000,'goods_receipt','origin'); INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES('count-1','company','branch','warehouse','C1','2026-11-10','Scrap','posted','root','maker','maker',0,0); UPDATE inventory_quality_holds SET status='disposed',last_result='fail',disposition_kind='scrap',disposition_reason='Scrap',disposition_requested_by='reviewer',disposition_count_id='count-1',disposition_quantity_milli=1000,disposed_by='approver',disposed_at=1 WHERE id='root'; INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES('event-1','root','count-1','post','approver',1,'Scrap',1); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id) VALUES('root-remaining-count-1','company','branch','warehouse','product','Remaining','2026-11-10','held','maker',1,1,'partial',2000,'goods_receipt','origin:remaining:count-1')").execute(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES('count-2','company','branch','warehouse','C2','2026-11-10','Scrap','posted','root-remaining-count-1','maker','maker',0,0); UPDATE inventory_quality_holds SET status='disposed',last_result='fail',disposition_kind='scrap',disposition_reason='Scrap',disposition_requested_by='reviewer',disposition_count_id='count-2',disposition_quantity_milli=500,disposed_by='approver',disposed_at=2 WHERE id='root-remaining-count-1'; INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES('event-2','root-remaining-count-1','count-2','post','approver',1,'Scrap',2); INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id) VALUES('root-remaining-count-1-remaining-count-2','company','branch','warehouse','product','Remaining','2026-11-10','held','maker',2,2,'partial',1500,'goods_receipt','origin:remaining:count-1:remaining:count-2')").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_purchase_lineage(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE inventory_counts IN SHARE MODE").execute(&mut *tx).await.unwrap();
        let mut counts=FixtureCount;
        let request=QualityLineageRequest { company_id:company(),branch_id:Some("branch".into()),hold_id:"root-remaining-count-1-remaining-count-2".into() };
        let source=inspect_quality_purchase_lineage(&mut tx,&mut counts,request.clone()).await.unwrap();
        assert_eq!(source.hold()["id"],"root-remaining-count-1-remaining-count-2");
        assert_eq!(source.root()["id"],"root");
        source.require_current(&mut tx,&mut counts).await.unwrap();
        sqlx::query("UPDATE inventory_quality_holds SET reason=repeat('x',2097153) WHERE id='root-remaining-count-1-remaining-count-2'").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx,&mut counts).await.unwrap();
        let foreign=QualityLineageRequest { company_id:CompanyId::new("foreign").unwrap(),..request.clone() };
        assert!(inspect_quality_purchase_lineage(&mut tx,&mut counts,foreign).await.is_err());
        let other_branch=QualityLineageRequest { branch_id:Some("branch-other".into()),..request.clone() };
        assert!(inspect_quality_purchase_lineage(&mut tx,&mut counts,other_branch).await.is_err());
        sqlx::query("SAVEPOINT quality_negative").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_quality_holds SET held_quantity_milli=1499 WHERE id='root-remaining-count-1-remaining-count-2'").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx,&mut counts).await.is_err());
        let broken=inspect_quality_purchase_lineage(&mut tx,&mut counts,request.clone()).await.unwrap();
        assert_eq!(broken.root()["id"],"root-remaining-count-1-remaining-count-2");
        sqlx::query("ROLLBACK TO SAVEPOINT quality_negative").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_quality_holds SET source_id='wrong' WHERE id='root-remaining-count-1-remaining-count-2'").execute(&mut *tx).await.unwrap();
        let broken=inspect_quality_purchase_lineage(&mut tx,&mut counts,request.clone()).await.unwrap();
        assert_eq!(broken.root()["id"],"root-remaining-count-1-remaining-count-2");
        sqlx::query("ROLLBACK TO SAVEPOINT quality_negative").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE inventory_counts SET status='draft' WHERE id='count-1'").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx,&mut counts).await.is_err());
        let current=inspect_quality_purchase_lineage(&mut tx,&mut counts,request).await.unwrap();
        assert_eq!(current.root()["id"],"root-remaining-count-1");
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(source.require_current(&mut other,&mut counts).await.is_err());
        other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn purchase_lineage_stops_after_sixty_four_posted_edges() {
    use erpx_server_quality::public::purchase_lineage::{QualityLineageRequest,inspect_quality_purchase_lineage,lock_quality_purchase_lineage};
    fixture::with_database(2,|pool|async move {
        facts::setup(&pool).await;
        let mut seed=pool.raw().begin().await.unwrap();
        let mut id="depth-root".to_owned();
        let mut source="origin".to_owned();
        let mut expected_root=String::new();
        for depth in 0..=65 {
            sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,status,created_by,created_at,updated_at,quantity_scope,held_quantity_milli,source_type,source_id) VALUES($1,'company','branch','warehouse','product','Review','2026-11-10','held','maker',0,0,'partial',$2,'goods_receipt',$3)")
                .bind(&id).bind(66000i64-depth as i64*1000).bind(&source).execute(&mut *seed).await.unwrap();
            if depth==1 { expected_root=id.clone(); }
            if depth==65 { break; }
            let count=format!("depth-count-{depth}");
            sqlx::query("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,status,quality_hold_id,created_by,prepared_by,created_at,updated_at) VALUES($1,'company','branch','warehouse',$1,'2026-11-10','Scrap','posted',$2,'maker','maker',0,0)")
                .bind(&count).bind(&id).execute(&mut *seed).await.unwrap();
            sqlx::query("UPDATE inventory_quality_holds SET status='disposed',last_result='fail',disposition_kind='scrap',disposition_reason='Scrap',disposition_requested_by='reviewer',disposition_count_id=$1,disposition_quantity_milli=1000,disposed_by='approver',disposed_at=1 WHERE id=$2")
                .bind(&count).bind(&id).execute(&mut *seed).await.unwrap();
            sqlx::query("INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,'post','approver',1,'Scrap',1)")
                .bind(format!("event-{depth}")).bind(&id).bind(&count).execute(&mut *seed).await.unwrap();
            id=format!("{id}-remaining-{count}");
            source=format!("{source}:remaining:{count}");
        }
        seed.commit().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_purchase_lineage(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE inventory_counts IN SHARE MODE").execute(&mut *tx).await.unwrap();
        let mut counts=FixtureCount;
        let witness=inspect_quality_purchase_lineage(&mut tx,&mut counts,QualityLineageRequest {company_id:company(),branch_id:Some("branch".into()),hold_id:id}).await.unwrap();
        assert_eq!(witness.root()["id"],expected_root);
        witness.require_current(&mut tx,&mut counts).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

#[tokio::test]
async fn cancellation_history_pages_retain_every_row_in_one_scoped_unit() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        for (id, count) in [("history-0", 0_i64), ("history-200", 200), ("history-201", 201), ("history-5001", 5001)] {
            facts::hold(&pool, id).await;
            sqlx::query("UPDATE inventory_quality_holds SET version=$2 WHERE id=$1").bind(id).bind(count + 1).execute(pool.raw()).await.unwrap();
            sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) SELECT $1 || '-' || n::text,$1,n,'request','historical reason','maker',0 FROM generate_series(1,$2::bigint) n")
                .bind(id).bind(count).execute(pool.raw()).await.unwrap();
        }
        facts::hold(&pool, "history-byte-pages").await;
        sqlx::query("UPDATE inventory_quality_holds SET version=3 WHERE id='history-byte-pages'").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) SELECT 'byte-' || n::text,'history-byte-pages',n,'request',repeat('x',400000),'maker',0 FROM generate_series(1,2) n").execute(pool.raw()).await.unwrap();
        facts::hold(&pool, "history-oversized-row").await;
        sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) VALUES('oversized','history-oversized-row',1,'request',repeat('x',700000),'maker',0)").execute(pool.raw()).await.unwrap();
        facts::hold(&pool, "history-signed").await;
        sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) SELECT 'signed-' || n::text,'history-signed',n,'request','historical reason','maker',-1 FROM unnest(ARRAY[2::bigint,0,-1,-9007199254740991]) n").execute(pool.raw()).await.unwrap();
        facts::hold(&pool, "history-unsafe").await;
        sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) VALUES('unsafe','history-unsafe',9007199254740992,'request','historical reason','maker',0)").execute(pool.raw()).await.unwrap();
        let mut unlocked = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        let missing = read_quality_cancellation_history_page(&mut unlocked, &company(), Some(&branch()), "history-200", None, None, None, 200).await;
        assert!(matches!(missing, Err(PgTransactionError::Persistence(e)) if e.code == "QUALITY_HISTORY_READ_LOCKS_REQUIRED"));
        unlocked.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_read_sources(&mut tx).await.unwrap();
        let core = read_quality_detail_core(&mut tx, &company(), Some(&branch()), "history-5001").await.unwrap().unwrap();
        let core_view = quality_detail_core_from_source(&actor("manager", &["quality.read"]), core.data().clone()).unwrap();
        assert_eq!(core_view.hold.hold.id, "history-5001");
        core.require_current(&mut tx).await.unwrap();
        assert!(read_quality_detail(&mut tx, &company(), Some(&branch()), "history-5001").await.is_err());
        let foreign_actor = QualityActor { company_id: CompanyId::new("foreign").unwrap(), ..actor("manager", &["quality.read"]) };
        assert!(quality_detail_core_from_source(&foreign_actor, core.data().clone()).is_err());
        for (id, count) in [("history-0", 0_i64), ("history-200", 200), ("history-201", 201), ("history-5001", 5001)] {
            let mut before = None;
            let mut head = None;
            let mut revision: Option<String> = None;
            let mut versions = Vec::new();
            loop {
                let page = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), id, before, head, revision.as_deref(), 200).await.unwrap();
                page.require_current(&mut tx).await.unwrap();
                versions.extend(page.rows().iter().map(|row| row.version));
                head = Some(page.head_version());
                revision = Some(page.history_revision().to_owned());
                match page.next_before() { Some(next) => before = Some(next), None => break }
            }
            assert_eq!(versions, (1..=count).rev().collect::<Vec<_>>());
        }
        let mut cursor = None;
        let mut signed_head = None;
        let mut signed_revision: Option<String> = None;
        let mut signed_versions = Vec::new();
        loop {
            let page = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-signed", cursor, signed_head, signed_revision.as_deref(), 1).await.unwrap();
            signed_versions.extend(page.rows().iter().map(|row| row.version));
            assert!(page.rows().iter().all(|row| row.created_at == -1));
            signed_head = Some(page.head_version());
            signed_revision = Some(page.history_revision().to_owned());
            match page.next_before() { Some(next) => cursor = Some(next), None => break }
        }
        assert_eq!(signed_versions, vec![2, 0, -1, -9007199254740991]);
        let signed_detail = read_quality_detail(&mut tx, &company(), Some(&branch()), "history-signed").await.unwrap().unwrap();
        assert_eq!(signed_detail.data().cancellations.iter().map(|row| row.version).collect::<Vec<_>>(), signed_versions);
        assert!(read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-unsafe", None, None, None, 200).await.is_err());
        assert!(read_quality_cancellation_history_page(&mut tx, &CompanyId::new("foreign").unwrap(), Some(&branch()), "history-200", None, None, None, 200).await.is_err());
        assert!(read_quality_cancellation_history_page(&mut tx, &company(), Some(&RecordId::new("other-branch").unwrap()), "history-200", None, None, None, 200).await.is_err());
        assert!(read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-200", None, None, None, 201).await.is_err());
        assert!(read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-200", Some(1), None, None, 200).await.is_err());
        let first = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-201", None, None, None, 200).await.unwrap();
        let wrong_revision = format!("qch1:{}", "0".repeat(64));
        let tampered = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-201", first.next_before(), Some(first.head_version()), Some(&wrong_revision), 200).await;
        assert!(matches!(tampered, Err(PgTransactionError::Business(e)) if e.code == "QUALITY_HISTORY_CHANGED"));
        let wrong_head = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-201", first.next_before(), Some(first.head_version()+1), Some(first.history_revision()), 200).await;
        assert!(matches!(wrong_head, Err(PgTransactionError::Business(e)) if e.code == "QUALITY_HISTORY_CHANGED"));
        let byte_page = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-byte-pages", None, None, None, 200).await.unwrap();
        assert_eq!(byte_page.rows().len(), 1);
        let byte_next = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-byte-pages", byte_page.next_before(), Some(byte_page.head_version()), Some(byte_page.history_revision()), 200).await.unwrap();
        assert_eq!(byte_next.rows().len(), 1);
        assert_eq!(byte_next.next_before(), None);
        assert!(read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-oversized-row", None, None, None, 200).await.is_err());
        // Model a legacy/backfill insert that does not increment hold.version.
        // The complete-history revision must detect it even though the head is unchanged.
        sqlx::query("SAVEPOINT stale_history").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) VALUES('historical-zero','history-201',0,'request','backfilled reason','maker',0)").execute(&mut *tx).await.unwrap();
        let stale = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-201", first.next_before(), Some(first.head_version()), Some(first.history_revision()), 200).await;
        assert!(matches!(stale, Err(PgTransactionError::Business(e)) if e.code == "QUALITY_HISTORY_CHANGED"));
        sqlx::query("ROLLBACK TO SAVEPOINT stale_history").execute(&mut *tx).await.unwrap();
        sqlx::query("RELEASE SAVEPOINT stale_history").execute(&mut *tx).await.unwrap();
        first.require_current(&mut tx).await.unwrap();
        let saved = read_quality_cancellation_history_page(&mut tx, &company(), Some(&branch()), "history-200", None, None, None, 200).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_quality_read_sources(&mut other).await.unwrap();
        assert!(saved.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        // A later transaction can insert historical evidence without advancing
        // the hold head. A continuation from the old page must still restart.
        sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) VALUES('later-historical-zero','history-201',0,'request','backfilled reason','maker',0)").execute(pool.raw()).await.unwrap();
        let mut fresh = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();
        lock_quality_read_sources(&mut fresh).await.unwrap();
        let stale = read_quality_cancellation_history_page(&mut fresh, &company(), Some(&branch()), "history-201", first.next_before(), Some(first.head_version()), Some(first.history_revision()), 200).await;
        assert!(matches!(stale, Err(PgTransactionError::Business(e)) if e.code == "QUALITY_HISTORY_CHANGED"));
        fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn cancellation_streams_full_preimage_without_raising_detail_bound() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        for (id, count) in [("bounded-hold", 200_i64), ("over-bound-hold", 201_i64), ("many-hold", 5001_i64)] {
            facts::hold(&pool, id).await;
            sqlx::query("UPDATE inventory_quality_holds SET version=$2 WHERE id=$1")
                .bind(id).bind(count + 1).execute(pool.raw()).await.unwrap();
            sqlx::query("INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) SELECT $1 || '-' || n::text,$1,n,'request','historical reason','maker',0 FROM generate_series(1,$2::bigint) n")
                .bind(id).bind(count).execute(pool.raw()).await.unwrap();
        }
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_quality_cancellation_writes(&mut tx).await.unwrap();
        lock_quality_read_sources(&mut tx).await.unwrap();
        let over = command("quality-over-bound", "over-bound-hold", 202, CancellationAction::Request, "Review duplicate hold");
        let over = prepare_quality_cancellation(&mut tx, &over, &company(), Some(&branch()), &user("maker"), 1800000000000).await.unwrap();
        over.apply(&mut tx).await.unwrap().require_current(&mut tx).await.unwrap();
        let bounded = command("quality-bounded", "bounded-hold", 201, CancellationAction::Request, "Review duplicate hold");
        let prepared = prepare_quality_cancellation(&mut tx, &bounded, &company(), Some(&branch()), &user("maker"), 1800000000000).await.unwrap();
        let retained = prepared.apply(&mut tx).await.unwrap();
        retained.require_current(&mut tx).await.unwrap();
        assert!(read_quality_detail(&mut tx, &company(), Some(&branch()), "bounded-hold").await.is_err());
        let reject = command("quality-bounded-reject", "bounded-hold", 202, CancellationAction::Reject, "Independent review rejected cancellation");
        let reject = prepare_quality_cancellation(&mut tx, &reject, &company(), Some(&branch()), &user("manager"), 1800000000001).await.unwrap();
        reject.apply(&mut tx).await.unwrap().require_current(&mut tx).await.unwrap();
        assert!(retained.require_current(&mut tx).await.is_err());
        let many = command("quality-many-history", "many-hold", 5002, CancellationAction::Request, "Review duplicate hold");
        let many = prepare_quality_cancellation(&mut tx, &many, &company(), Some(&branch()), &user("maker"), 1800000000000).await.unwrap();
        many.apply(&mut tx).await.unwrap().require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='bounded-hold'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 200);
    }).await;
}

#[tokio::test]
async fn workflow_effect_is_transaction_bound_and_retains_exact_scoped_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        let command = command("quality-workflow-effect", "hold-a", 1, CancellationAction::Request, "Review the duplicate quality hold");
        // A shared read fence alone does not grant writer intent. Use a
        // separate unit so its snapshot cannot precede the real write locks.
        let mut read_only = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *read_only).await.unwrap();
        lock_quality_read_sources(&mut read_only).await.unwrap();
        let missing = prepare_quality_cancellation(&mut read_only, &command, &company(), Some(&branch()), &user("maker"), 1_800_000_000_000).await;
        assert!(matches!(missing, Err(PgTransactionError::Persistence(e)) if e.code == "QUALITY_CANCELLATION_WRITE_LOCKS_REQUIRED"));
        read_only.rollback().await.unwrap();
        // Locks acquired below a savepoint disappear when it rolls back;
        // an earlier prepared value must not continue to authorize an effect.
        let mut released = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *released).await.unwrap();
        sqlx::query("SAVEPOINT owner_locks").execute(&mut *released).await.unwrap();
        lock_quality_cancellation_writes(&mut released).await.unwrap();
        let prepared = prepare_quality_cancellation(&mut released, &command, &company(), Some(&branch()), &user("maker"), 1_800_000_000_000).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT owner_locks").execute(&mut *released).await.unwrap();
        let error = prepared.require_current(&mut released).await;
        assert!(matches!(error, Err(PgTransactionError::Persistence(e)) if e.code == "QUALITY_CANCELLATION_WRITE_LOCKS_REQUIRED"));
        released.rollback().await.unwrap();
        PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(), move |c| {
            let command = command.clone();
            Box::pin(async move {
                lock_quality_cancellation_writes(c).await?;
                lock_quality_read_sources(c).await?;
                let mut invalid = command.clone();
                invalid.reason = "x".into();
                assert!(prepare_quality_cancellation(c, &invalid, &company(), Some(&branch()), &user("maker"), 1_800_000_000_000).await.is_err());
                invalid.reason = "  valid reason  ".into();
                assert!(prepare_quality_cancellation(c, &invalid, &company(), Some(&branch()), &user("maker"), 1_800_000_000_000).await.is_err());
                let effect = prepare_quality_cancellation(c, &command, &company(), Some(&branch()), &user("maker"), 1_800_000_000_000).await?;
                assert_eq!(effect.receipt().version, 2);
                effect.require_current(c).await?;
                // A null-spec hold is not dependent on another specification's
                // metrics, even if those Company rows change in this unit.
                sqlx::query("INSERT INTO inventory_quality_specs(id,company_id,product_id,name,status,version,created_by,created_at,updated_at) VALUES('other-spec','company','product','Other limits','draft',1,'maker',0,0)")
                    .execute(&mut *c).await?;
                sqlx::query("INSERT INTO inventory_quality_spec_metrics(id,spec_id,code,label,unit,minimum_value,maximum_value,ordinal) VALUES('other-metric','other-spec','other','Other','u',0,1,0)")
                    .execute(&mut *c).await?;
                effect.require_current(c).await?;
                let retained = effect.apply(c).await?;
                retained.require_current(c).await?;
                sqlx::query("SAVEPOINT added_inspection").execute(&mut *c).await?;
                sqlx::query("INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at) VALUES('late-inspection','hold-a',2,'pass','late evidence','reviewer',1800000000000)")
                    .execute(&mut *c).await?;
                assert!(retained.require_current(c).await.is_err());
                sqlx::query("ROLLBACK TO SAVEPOINT added_inspection").execute(&mut *c).await?;
                sqlx::query("RELEASE SAVEPOINT added_inspection").execute(&mut *c).await?;
                retained.require_current(c).await?;
                sqlx::query("UPDATE inventory_quality_holds SET cancel_reason='tampered' WHERE id='hold-a'")
                    .execute(&mut *c).await?;
                assert!(retained.require_current(c).await.is_err());
                // Roll back tampering and effect, preserving the fixture.
                Err::<(), _>(BusinessError::invalid("TEST_ROLLBACK", "Rollback test effect.").into())
            })
        }).await.unwrap_err();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE id='quality-cancel-quality-workflow-effect'")
            .fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 0);
    }).await;
}

fn branch() -> RecordId {
    RecordId::new("branch").unwrap()
}

fn user(value: &str) -> UserId {
    UserId::new(value).unwrap()
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn actor(value: &str, permissions: &[&str]) -> QualityActor {
    QualityActor {
        user_id: user(value),
        company_id: company(),
        branch_id: Some(branch()),
        department_id: None,
        permissions: permissions
            .iter()
            .map(|permission| (*permission).into())
            .collect(),
    }
}

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}

fn denied<T: std::fmt::Debug>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    match result {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

async fn read_workspace_view(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    actor: QualityActor,
) -> Result<
    erpx_server_quality::public::quality_read::QualityWorkspace,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            Box::pin(async move {
                lock_quality_read_sources(c).await?;
                let mut repository = PgQualityRead {
                    connection: &mut *c,
                };
                Ok(quality_workspace(&actor, &mut repository).await?)
            })
        })
        .await
}

async fn read_detail_view(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    actor: QualityActor,
    id: &str,
) -> Result<
    erpx_server_quality::public::quality_read::QualityDetail,
    PgTransactionError<BusinessError>,
> {
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let id = id.clone();
            Box::pin(async move {
                lock_quality_read_sources(c).await?;
                let mut repository = PgQualityRead {
                    connection: &mut *c,
                };
                Ok(quality_detail(&actor, &id, &mut repository).await?)
            })
        })
        .await
}

async fn cancel(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    actor: QualityActor,
    command: QualityCancellationCommand,
) -> Result<
    erpx_server_quality::public::quality_read::QualityCancellationReceipt,
    PgTransactionError<BusinessError>,
> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let command = command.clone();
            Box::pin(async move {
                lock_quality_read_sources(c).await?;
                let mut repository = PgQualityCancellation {
                    connection: &mut *c,
                };
                Ok(cancel_quality_hold(&actor, command, &mut repository, &FixedClock).await?)
            })
        })
        .await
}

fn command(
    command_key: &str,
    hold: &str,
    version: i64,
    action: CancellationAction,
    reason: &str,
) -> QualityCancellationCommand {
    QualityCancellationCommand {
        command_key: key(command_key),
        hold_id: RecordId::new(hold).unwrap(),
        expected_version: version,
        action,
        reason: reason.into(),
    }
}

#[tokio::test]
async fn workspace_and_detail_read_only_quality_owned_facts_in_current_scope() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        facts::hold_in_branch(&pool, "hold-other", "branch-other").await;
        facts::specification(&pool, "spec").await;
        facts::inspection(&pool, "hold-a", "inspection", "pass").await;
        facts::count(&pool, "hold-a", "count-a").await;
        sqlx::raw_sql(
            "UPDATE inventory_quality_holds SET status='awaiting_release',last_result='pass',last_inspector='inspector',version=2,spec_id='spec',spec_version=2 WHERE id='hold-a';\
             INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at)VALUES('cancel-1','hold-a',1,'request','Mistaken scope','maker',0);\
             INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at)VALUES('disposition-1','hold-a','count-a','request','manager',2,'Controlled scrap review',0);\
             INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)VALUES('alert-1','hold-a','due_soon',2,'2026-11-10',0);",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let reviewer = actor("manager", &["quality.read", "quality.inspect", "quality.release", "quality.hold"]);
        let result = read_workspace_view(&pool, reviewer.clone()).await.unwrap();
        assert_eq!(result.holds.len(), 1);
        assert_eq!(result.holds[0].hold.id, "hold-a");
        assert_eq!(result.holds[0].hold.status, HoldStatus::AwaitingRelease);
        assert_eq!(result.holds[0].hold.inspection_count, 1);
        assert!(result.holds[0].decision.can_release);

        let readback = read_detail_view(&pool, reviewer.clone(), "hold-a").await.unwrap();
        assert_eq!(readback.hold.hold.id, "hold-a");
        assert_eq!(readback.inspections.len(), 1);
        assert_eq!(readback.measurements.len(), 2);
        assert!(readback.measurements[0].passed);
        assert_eq!(readback.cancellations.len(), 1);
        assert_eq!(readback.dispositions.len(), 1);
        assert_eq!(readback.spec_metrics.len(), 2);
        assert_eq!(readback.alerts.len(), 1);
        assert!(readback.hold.decision.can_release);

        // The Company has two metrics for another spec, but a hold without a
        // spec has no detail metrics. Workspace still returns Company metrics.
        facts::hold(&pool, "hold-no-spec").await;
        let no_spec = read_detail_view(&pool, reviewer.clone(), "hold-no-spec").await.unwrap();
        assert!(no_spec.spec_metrics.is_empty());
        let mut workspace_tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *workspace_tx).await.unwrap();
        lock_quality_read_sources(&mut workspace_tx).await.unwrap();
        let workspace = read_quality_workspace(&mut workspace_tx, &company(), Some(&branch())).await.unwrap();
        assert_eq!(workspace.data().specification_metrics.len(), 2);
        workspace.require_current(&mut workspace_tx).await.unwrap();
        workspace_tx.rollback().await.unwrap();

        // The creator cannot release their own passing inspection.
        let creator = actor("maker", &["quality.read", "quality.inspect", "quality.release", "quality.hold"]);
        let result = read_detail_view(&pool, creator, "hold-a").await.unwrap();
        assert!(!result.hold.decision.can_release);

        // A branch-scoped read must not see another branch or a foreign Company.
        let other = actor("manager", &["quality.read"]);
        let missing = read_detail_view(&pool, other, "hold-other").await;
        denied(missing, "QUALITY_HOLD_NOT_FOUND");

        let foreign = erpx_server_quality::public::quality_read::QualityActor {
            company_id: CompanyId::new("foreign").unwrap(),
            ..actor("manager", &["quality.read"])
        };
        let missing = read_detail_view(&pool, foreign, "hold-a").await;
        denied(missing, "QUALITY_HOLD_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn current_workspace_snapshot_rejects_a_changed_hold_in_the_same_transaction() {
    fixture::with_database(1, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_quality_read_sources(c).await?;
                    let current = read_quality_workspace(c, &company(), Some(&branch())).await?;
                    sqlx::query("UPDATE inventory_quality_holds SET reason='Changed after the read' WHERE id='hold-a'")
                        .execute(&mut *c)
                        .await?;
                    denied(
                        current.require_current(c).await,
                        "QUALITY_READ_CHANGED",
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn cancellation_requests_and_approves_once_with_local_idempotency() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        let maker = actor("maker", &["quality.read", "quality.hold"]);
        let request = command(
            "quality-cancel-request",
            "hold-a",
            1,
            CancellationAction::Request,
            "Duplicate hold created for the wrong receiving review",
        );
        let receipt = cancel(&pool, maker.clone(), request.clone()).await.unwrap();
        assert_eq!(receipt.status, HoldStatus::CancelPending);
        assert_eq!(receipt.version, 2);
        assert_eq!(
            cancel(&pool, maker.clone(), request.clone()).await.unwrap(),
            receipt
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='hold-a'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 1);

        let conflict = command(
            "quality-cancel-request",
            "hold-a",
            1,
            CancellationAction::Request,
            "A different reason must not replay the saved receipt",
        );
        denied(cancel(&pool, maker.clone(), conflict).await, "QUALITY_CANCELLATION_CONFLICT");
        let self_manager = actor("maker", &["quality.read", "quality.release"]);
        denied(
            cancel(
                &pool,
                self_manager,
                command(
                    "quality-cancel-self",
                    "hold-a",
                    2,
                    CancellationAction::Approve,
                    "Self approval attempt",
                ),
            )
            .await,
            "INDEPENDENT_CANCELLATION_REQUIRED",
        );

        sqlx::raw_sql(
            "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy)VALUES\
             ('company','maker','local:maker','person-1','active','local'),\
             ('company','alias','local:alias','person-1','active','local');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let alias = actor("alias", &["quality.read", "quality.release"]);
        denied(
            cancel(
                &pool,
                alias,
                command(
                    "quality-cancel-alias",
                    "hold-a",
                    2,
                    CancellationAction::Reject,
                    "Merged identity must not review its own request",
                ),
            )
            .await,
            "INDEPENDENT_CANCELLATION_REQUIRED",
        );

        let manager = actor("manager", &["quality.read", "quality.release"]);
        let approved = cancel(
            &pool,
            manager.clone(),
            command(
                "quality-cancel-approve",
                "hold-a",
                2,
                CancellationAction::Approve,
                "Confirmed duplicate of the other active hold",
            ),
        )
        .await
        .unwrap();
        assert_eq!(approved.status, HoldStatus::Cancelled);
        assert_eq!(approved.version, 3);

        let readback = read_detail_view(&pool, manager, "hold-a").await.unwrap();
        assert_eq!(readback.hold.hold.status, HoldStatus::Cancelled);
        assert_eq!(readback.cancellations.len(), 2);
        assert!(!readback.hold.decision.can_inspect);
        assert!(!readback.hold.decision.can_cancel);

        let saved = sqlx::query(
            "SELECT status,cancelled_by,cancelled_at,version FROM inventory_quality_holds WHERE id='hold-a'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(saved.get::<String, _>("status"), "cancelled");
        assert_eq!(saved.get::<Option<String>, _>("cancelled_by"), Some("manager".into()));
        assert_eq!(saved.get::<Option<i64>, _>("cancelled_at"), Some(1_800_000_000_000));
        assert_eq!(saved.get::<i64, _>("version"), 3);
    })
    .await;
}

#[tokio::test]
async fn cancellation_rejects_inspected_holds_and_a_late_failure_rolls_back() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-inspected").await;
        facts::specification(&pool, "spec").await;
        facts::inspection(&pool, "hold-inspected", "inspection", "fail").await;
        sqlx::query("UPDATE inventory_quality_holds SET last_result='fail',last_inspector='inspector',version=2,spec_id='spec',spec_version=2 WHERE id='hold-inspected'")
            .execute(pool.raw())
            .await
            .unwrap();
        let maker = actor("maker", &["quality.read", "quality.hold"]);
        denied(
            cancel(
                &pool,
                maker,
                command(
                    "quality-cancel-inspected",
                    "hold-inspected",
                    2,
                    CancellationAction::Request,
                    "Try to bypass a failed inspection",
                ),
            )
            .await,
            "UNINSPECTED_HOLD_REQUIRED",
        );
        let version: i64 = sqlx::query_scalar("SELECT version FROM inventory_quality_holds WHERE id='hold-inspected'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(version, 2);

        facts::hold(&pool, "hold-rollback").await;
        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_quality_read_sources(c).await?;
                    let command = command(
                        "quality-cancel-rollback",
                        "hold-rollback",
                        1,
                        CancellationAction::Request,
                        "Rollback after a controlled late failure",
                    );
                    apply_quality_cancellation(
                        c,
                        &command,
                        &company(),
                        Some(&branch()),
                        &user("maker"),
                        1,
                    )
                    .await?;
                    Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "QUALITY_TEST_LATE_FAILURE",
                        "Late failure after the Quality write.",
                    )
                    .into())
                })
            })
            .await;
        match result {
            Err(PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "QUALITY_TEST_LATE_FAILURE")
            }
            _ => panic!("expected the controlled late failure"),
        }
        let state = sqlx::query("SELECT status,version FROM inventory_quality_holds WHERE id='hold-rollback'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        use sqlx::Row;
        assert_eq!(state.get::<String, _>("status"), "held");
        assert_eq!(state.get::<i64, _>("version"), 1);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM inventory_quality_cancellations WHERE hold_id='hold-rollback'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}

#[tokio::test]
async fn cancellation_rejection_returns_to_held_and_keeps_the_hold_quarantined() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        let maker = actor("maker", &["quality.read", "quality.hold"]);
        cancel(
            &pool,
            maker,
            command(
                "quality-cancel-request",
                "hold-a",
                1,
                CancellationAction::Request,
                "Incorrect receiving hold",
            ),
        )
        .await
        .unwrap();
        let manager = actor(
            "manager",
            &["quality.read", "quality.inspect", "quality.release"],
        );
        let rejected = cancel(
            &pool,
            manager.clone(),
            command(
                "quality-cancel-reject",
                "hold-a",
                2,
                CancellationAction::Reject,
                "Verify supplier receipt before removing hold",
            ),
        )
        .await
        .unwrap();
        assert_eq!(rejected.status, HoldStatus::Held);
        assert_eq!(rejected.version, 3);
        let readback = read_detail_view(&pool, manager, "hold-a").await.unwrap();
        assert_eq!(readback.hold.hold.status, HoldStatus::Held);
        assert_eq!(readback.cancellations.len(), 2);
        assert!(readback.hold.decision.can_inspect);
    })
    .await;
}

#[tokio::test]
async fn quality_reads_require_an_owned_serializable_unit() {
    fixture::with_database(1, |pool| async move {
        facts::setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let error = read_quality_workspace(&mut connection, &company(), Some(&branch())).await;
        match error {
            Err(PgTransactionError::Persistence(error)) => {
                assert_eq!(error.code, "QUALITY_TRANSACTION_REQUIRED")
            }
            _ => panic!("expected the transaction requirement"),
        }
    })
    .await;
}

#[tokio::test]
async fn readback_uses_the_owner_transaction_and_persists_the_cancellation_event() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool).await;
        facts::hold(&pool, "hold-a").await;
        let event = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_quality_read_sources(c).await?;
                    let command = command(
                        "quality-cancel-readback",
                        "hold-a",
                        1,
                        CancellationAction::Request,
                        "Read back the persisted cancellation",
                    );
                    let receipt = apply_quality_cancellation(
                        c,
                        &command,
                        &company(),
                        Some(&branch()),
                        &user("maker"),
                        7,
                    )
                    .await?;
                    let current = read_quality_detail(c, &company(), Some(&branch()), "hold-a")
                        .await?
                        .expect("hold detail");
                    assert_eq!(current.data().hold.status, HoldStatus::CancelPending);
                    assert_eq!(current.data().cancellations.len(), 1);
                    current.require_current(c).await?;
                    Ok(receipt)
                })
            })
            .await
            .unwrap();
        assert_eq!(event.status, HoldStatus::CancelPending);
        let row = sqlx::query("SELECT action,reason,actor_id,version FROM inventory_quality_cancellations WHERE id=$1")
            .bind(&event.cancellation_id)
            .fetch_one(pool.raw())
            .await
            .unwrap();
        use sqlx::Row;
        assert_eq!(row.get::<String, _>("action"), "request");
        assert_eq!(row.get::<String, _>("reason"), "Read back the persisted cancellation");
        assert_eq!(row.get::<String, _>("actor_id"), "maker");
        assert_eq!(row.get::<i64, _>("version"), 2);
    })
    .await;
}
