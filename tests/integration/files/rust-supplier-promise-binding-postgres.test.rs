#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_files_server::public::supplier_promise_binding::*;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::CompanyId;
use sqlx::{Connection, PgConnection};
async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1);\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at)          VALUES('upload-1','holding','company','delivery','proof.png','image/png',10,'staging-1','ready','fixture',4102444800000,0,0),         ('upload-2','holding','company','delivery','proof-2.png','image/png',10,'staging-2','ready','fixture',4102444800000,0,0);         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('attachment-1','upload-1','holding','company','delivery','original-1',1,'proof.png','image/png',10,repeat('a',64),'fixture',0,'company','image/png','available',1,1),\
         ('attachment-2','upload-2','holding','company','delivery','original-2',1,'proof-2.png','image/png',10,repeat('b',64),'fixture',0,'company','image/png','available',1,1);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-1','company',NULL,NULL,'purchase_order','order-1','action','supplier-promise','supplier_promise',1,1,'internal','[]',1,2,'draft','fixture',0,0),\
         ('set-2','company',NULL,NULL,'purchase_order','order-1','action','supplier-promise','supplier_promise',1,1,'internal','[]',1,2,'draft','fixture-2',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-1','set-1','attachment-1','draft','fixture',0,4102444800000),\
         ('link-2','set-2','attachment-2','draft','fixture',0,4102444800000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

struct Owner {
    facts: SupplierPromiseEvidenceFacts,
    frozen: std::sync::atomic::AtomicBool,
}
impl SupplierPromiseEvidenceOwner for Owner {
    fn features(&self) -> SupplierEvidenceFeatures {
        SupplierEvidenceFeatures {
            enabled: true,
            read_only: false,
            uploads_frozen: self.frozen.load(std::sync::atomic::Ordering::SeqCst),
        }
    }
    fn facts(&self) -> &SupplierPromiseEvidenceFacts {
        &self.facts
    }
    async fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> Result<(), PgTransactionError<SupplierPromiseEvidenceError>> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attachment_sets WHERE id='set-1' AND state='draft'",
        )
        .fetch_one(c)
        .await?;
        if n != 1 {
            return Err(PgTransactionError::Business(
                SupplierPromiseEvidenceError::Business(
                    erpx_platform_shared_server::public::BusinessError::invalid(
                        "TEST_SOURCE_CHANGED",
                        "test source changed",
                    ),
                ),
            ));
        }
        Ok(())
    }
    async fn require_after(
        &self,
        _c: &mut PgConnection,
    ) -> Result<(), PgTransactionError<SupplierPromiseEvidenceError>> {
        Ok(())
    }
}
#[tokio::test]
async fn supplier_evidence_preserves_order_deduplicates_and_rolls_back() {
    fixture::with_database(3,|pool| async move {
  setup(&pool).await;
  let mut c=pool.raw().acquire().await.unwrap(); let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  lock_supplier_promise_evidence_sources(&mut tx).await.unwrap();
  let owner=Owner{frozen:std::sync::atomic::AtomicBool::new(false),facts:SupplierPromiseEvidenceFacts{company_id:"company".into(),order_id:"order-1".into(),order_version:1,actor_id:"fixture".into(),command_key:"command-1".into(),promise_id:"supplier-promise-command-1".into()}};
  let company=CompanyId::new("company").unwrap();
  let ids=vec!["set-2".into(),"set-1".into(),"set-2".into()];
  sqlx::query("SAVEPOINT mixed_policy").execute(&mut *tx).await.unwrap();
  sqlx::query("UPDATE attachment_sets SET policy_version=2,required_count=2 WHERE id='set-1'").execute(&mut *tx).await.unwrap();
  let error=prepare_supplier_promise_evidence(&mut tx,&owner,&company,"order-1",1,&["set-1".into()],123).await.unwrap_err();
  assert!(matches!(error,PgTransactionError::Business(SupplierPromiseEvidenceError::RequiredCount{required:2})));
  sqlx::query("UPDATE attachment_sets SET required_count=1 WHERE id='set-1'").execute(&mut *tx).await.unwrap();
  let staged=prepare_supplier_promise_evidence(&mut tx,&owner,&company,"order-1",1,&["set-1".into()],123).await.unwrap();
  assert!(validate_supplier_promise_evidence_guards(&mut tx,&owner,&staged).await.is_err());
  sqlx::query("ROLLBACK TO SAVEPOINT mixed_policy").execute(&mut *tx).await.unwrap();
  let w=prepare_supplier_promise_evidence(&mut tx,&owner,&company,"order-1",1,&ids,123).await.unwrap();
  assert_eq!(w.manifest().iter().map(|x|x.link_id.as_str()).collect::<Vec<_>>(),vec!["link-2","link-1"]);
  let guarded=validate_supplier_promise_evidence_guards(&mut tx,&owner,&w).await.unwrap();
  owner.frozen.store(true,std::sync::atomic::Ordering::SeqCst);
  sqlx::query("SAVEPOINT apply_rollback").execute(&mut *tx).await.unwrap();
  let rolled_back=apply_supplier_promise_evidence(&mut tx,&owner,&guarded).await.unwrap();
  rolled_back.require_applied(&mut tx,&owner.facts,&ids).await.unwrap();
  sqlx::query("ROLLBACK TO SAVEPOINT apply_rollback").execute(&mut *tx).await.unwrap();
  assert!(rolled_back.require_applied(&mut tx,&owner.facts,&ids).await.is_err());
  sqlx::query("SAVEPOINT apply_release").execute(&mut *tx).await.unwrap();
  let retained=apply_supplier_promise_evidence(&mut tx,&owner,&guarded).await.unwrap();
  sqlx::query("RELEASE SAVEPOINT apply_release").execute(&mut *tx).await.unwrap();
  retained.require_applied(&mut tx,&owner.facts,&ids).await.unwrap();
  let mut wrong_facts=owner.facts.clone(); wrong_facts.command_key="other".into();
  assert!(retained.require_applied(&mut tx,&wrong_facts,&ids).await.is_err());
  assert!(retained.require_applied(&mut tx,&owner.facts,&["set-1".into()]).await.is_err());
  retained.require_current(&mut tx,&owner).await.unwrap();
  sqlx::query("SAVEPOINT source_guard").execute(&mut *tx).await.unwrap();
  assert!(sqlx::query("UPDATE attachments SET checksum_sha256=repeat('c',64) WHERE id='attachment-1'").execute(&mut *tx).await.is_err());
  sqlx::query("ROLLBACK TO SAVEPOINT source_guard").execute(&mut *tx).await.unwrap();
  owner.frozen.store(true,std::sync::atomic::Ordering::SeqCst);
  retained.require_current(&mut tx,&owner).await.unwrap();
  owner.frozen.store(false,std::sync::atomic::Ordering::SeqCst);
  retained.require_current(&mut tx,&owner).await.unwrap();
  let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE state='bound'),(SELECT count(*) FROM attachment_events WHERE action='set.bound')").fetch_one(&mut *tx).await.unwrap();assert_eq!(counts,(2,2));
  tx.rollback().await.unwrap();
  let mut next=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  assert!(retained.require_applied(&mut next,&owner.facts,&ids).await.is_err());
  next.rollback().await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_sets WHERE state='bound'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);
 }).await;
}
