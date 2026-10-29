#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_files::public::count_submission::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
struct Owner {
    facts: CountBindingFacts,
}
impl PgCountBindingOwner for Owner {
    fn facts(&self) -> &CountBindingFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM test_owner WHERE id=$1 AND version=$2 AND actor=$3 AND allowed AND state='draft')").bind(&self.facts.target_id).bind(self.facts.target_version).bind(&self.facts.actor_id).fetch_one(c).await?;
        if !ok {
            return Err(BusinessError::invalid("TEST_OWNER_DENIED", "Test owner changed.").into());
        }
        Ok(())
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM test_owner WHERE id=$1 AND version=$2+1 AND actor=$3 AND allowed AND state='submitted')").bind(&self.facts.target_id).bind(self.facts.target_version).bind(&self.facts.actor_id).fetch_one(c).await?;
        if !ok {
            return Err(BusinessError::invalid("TEST_OWNER_DENIED", "Test owner changed.").into());
        }
        Ok(())
    }
    async fn require_replay(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_after(c).await?;
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM test_receipt WHERE key=$1 AND actor=$2)",
        )
        .bind(&self.facts.command_id)
        .bind(&self.facts.actor_id)
        .fetch_one(c)
        .await?;
        if !ok {
            return Err(
                BusinessError::invalid("TEST_RECEIPT_REQUIRED", "Test receipt missing.").into(),
            );
        }
        Ok(())
    }
}
fn owner() -> Owner {
    Owner {
        facts: CountBindingFacts {
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            actor_id: "user".into(),
            target_type: "stock_count".into(),
            target_id: "count".into(),
            target_version: 1,
            command_id: "command-key".into(),
            set_ids: vec!["set-a".into(), "set-a".into(), "set-b".into()],
            features: CountBindingFeatures {
                enabled: true,
                read_only: false,
                uploads_frozen: false,
            },
            now: 1000,
        },
    }
}
async fn setup(p: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(p, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE test_owner(id text PRIMARY KEY,version bigint,actor text,allowed boolean,state text); CREATE TABLE test_receipt(key text PRIMARY KEY,actor text); INSERT INTO test_owner VALUES('count',1,'user',true,'draft'); INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('set-a','company','stock_count','count','action','submit','count_sheet',1,1,'internal','{}','user',1,1),('set-b','company','stock_count','count','action','submit','count_photo',1,1,'internal','{}','user',2,2); INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('upload-a','stock_count','count','count_sheet','a.pdf','application/pdf',100,'stage-a','ready','user',100000,1,1),('upload-b','stock_count','count','count_photo','b.png','image/png',200,'stage-b','ready','user',100000,2,2); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state) VALUES('a','upload-a','stock_count','count','count_sheet','object-a',1,'a.pdf','application/pdf',100,repeat('a',64),'user',1,'company','available'),('b','upload-b','stock_count','count','count_photo','object-b',1,'b.png','image/png',200,repeat('b',64),'user',2,'company','available'); INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('link<a','set-a','a','draft','user',1,100000),('link-b','set-b','b','draft','user',2,100000)").execute(p.raw()).await.unwrap();
}
async fn begin(p: &DatabasePool) -> sqlx::Transaction<'_, sqlx::Postgres> {
    let mut tx = p.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    lock_count_binding_sources(&mut tx).await.unwrap();
    tx
}
#[tokio::test]
async fn multi_set_binding_receipt_replay_and_atomic_rollback() {
    fixture::with_database(3,|p|async move{setup(&p).await;let o=owner();let mut tx=begin(&p).await;let targets=read_count_binding_target_types(&mut tx,&o.facts.company_id,&o.facts.set_ids).await.unwrap();assert_eq!(targets.len(),2);let plan=prepare_count_submission(&mut tx,&o).await.unwrap();let manifest=plan.manifest().to_vec();assert_eq!(manifest.len(),2);let retained=plan.apply(&mut tx).await.unwrap();sqlx::raw_sql("UPDATE test_owner SET state='submitted',version=2 WHERE id='count'; INSERT INTO test_receipt VALUES('command-key','user')").execute(&mut *tx).await.unwrap();retained.require_after(&mut tx).await.unwrap();let details:Vec<String>=sqlx::query_scalar("SELECT details_json FROM attachment_events WHERE action='set.bound' ORDER BY set_id").fetch_all(&mut *tx).await.unwrap();assert_eq!(details.len(),2);assert!(details[0].contains("link\\u003ca"));let first:serde_json::Value=serde_json::from_str(&details[0]).unwrap();let second:serde_json::Value=serde_json::from_str(&details[1]).unwrap();assert_eq!(first["manifest"].as_array().unwrap().len(),1);assert_eq!(second["manifest"].as_array().unwrap().len(),2);tx.commit().await.unwrap();let mut replay=begin(&p).await;verify_count_submission_replay(&mut replay,&o,&manifest).await.unwrap();replay.rollback().await.unwrap();}).await;
}

#[tokio::test]
async fn late_source_and_authority_changes_deny_without_binding() {
    fixture::with_database(3, |p| async move {
        setup(&p).await;
        let o = owner();
        let mut tx = begin(&p).await;
        let plan = prepare_count_submission(&mut tx, &o).await.unwrap();
        sqlx::query(
            "UPDATE attachment_links SET caption='changed after preparation' WHERE id='link-b'",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        match plan.apply(&mut tx).await {
            Err(PgTransactionError::Business(e)) => assert_eq!(e.code, "ATTACHMENT_SET_CHANGED"),
            _ => panic!("late link change was accepted"),
        }
        tx.rollback().await.unwrap();
        let mut tx = begin(&p).await;
        let plan = prepare_count_submission(&mut tx, &o).await.unwrap();
        sqlx::query("UPDATE test_owner SET allowed=false WHERE id='count'")
            .execute(&mut *tx)
            .await
            .unwrap();
        match plan.apply(&mut tx).await {
            Err(PgTransactionError::Business(e)) => assert_eq!(e.code, "TEST_OWNER_DENIED"),
            _ => panic!("late authority change was accepted"),
        }
        tx.rollback().await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM attachment_sets WHERE state='bound'")
                .fetch_one(p.raw())
                .await
                .unwrap();
        assert_eq!(count, 0);
    })
    .await;
}
#[tokio::test]
async fn unexpected_inserted_event_is_detected_and_all_mutations_rollback() {
    fixture::with_database(3,|p|async move{
        setup(&p).await;let o=owner();let mut tx=begin(&p).await;
        sqlx::raw_sql("CREATE FUNCTION test_tamper_bind() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='set.bound' THEN NEW.actor_id='tampered'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_tamper_bind BEFORE INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION test_tamper_bind()").execute(&mut *tx).await.unwrap();
        let plan=prepare_count_submission(&mut tx,&o).await.unwrap();
        match plan.apply(&mut tx).await {Err(PgTransactionError::Business(e))=>assert_eq!(e.code,"ATTACHMENT_SET_CHANGED"),_=>panic!("tampered event was accepted")}
        tx.rollback().await.unwrap();
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE state='bound'),(SELECT count(*) FROM attachment_links WHERE state='bound'),(SELECT count(*) FROM attachment_events WHERE action='set.bound')").fetch_one(p.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
    }).await;
}

#[tokio::test]
async fn target_version_and_compatibility_gates_use_real_current_rows() {
    fixture::with_database(3,|p|async move{
        setup(&p).await;
        let mut o=owner();
        let mut tx=begin(&p).await;
        sqlx::query("UPDATE attachment_sets SET expected_target_version=2 WHERE id='set-b'").execute(&mut *tx).await.unwrap();
        match prepare_count_submission(&mut tx,&o).await {Err(PgTransactionError::Business(e))=>assert_eq!(e.code,"ATTACHMENT_SET_STALE"),_=>panic!("stale version was accepted")}
        tx.rollback().await.unwrap();
        let mut tx=begin(&p).await;
        sqlx::query("UPDATE attachment_sets SET target_type='inventory_stock_count' WHERE id='set-b'").execute(&mut *tx).await.unwrap();
        match prepare_count_submission(&mut tx,&o).await {Err(PgTransactionError::Business(e))=>assert_eq!(e.code,"ATTACHMENT_SET_TARGET"),_=>panic!("mixed target was accepted")}
        tx.rollback().await.unwrap();
        o.facts.features.enabled=false;
        let mut tx=begin(&p).await;
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES('compat-a','company','set-a','user','set.created','{\"compatibilityAdapter\":true}',1)").execute(&mut *tx).await.unwrap();
        match prepare_count_submission(&mut tx,&o).await {Err(PgTransactionError::Business(e))=>assert_eq!(e.code,"ATTACHMENTS_DISABLED"),_=>panic!("partially marked compatibility input was accepted")}
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES('compat-b','company','set-b','user','set.created','{\"compatibilityAdapter\":true}',2)").execute(&mut *tx).await.unwrap();
        assert!(prepare_count_submission(&mut tx,&o).await.is_ok());
        tx.rollback().await.unwrap();
    }).await;
}
