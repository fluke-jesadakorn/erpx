//! Native auxiliary Legal reads and retention commands over leased PostgreSQL.
//! Every read and command runs in one serializable Holding transaction with the
//! current Company/branch/department/membership scope.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_legal::public::auxiliary::*;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, StatusCode, body::to_bytes, header,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgConnection};
use std::sync::{Arc, Mutex};

struct FixedClock(i64);

impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0)
    }
}

fn actor(user: &str, role: &str, permissions: &[&str]) -> LegalAuxiliaryActor {
    LegalAuxiliaryActor {
        user_id: UserId::new(user).unwrap(),
        assignment_id: erpx_server_shared::public::AssignmentId::new(format!("assignment-{user}"))
            .unwrap(),
        holding_id: HoldingId::new("holding-a").unwrap(),
        company_id: CompanyId::new("company-a").unwrap(),
        branch_id: None,
        department_id: None,
        role: role.into(),
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
    }
}

fn manager() -> LegalAuxiliaryActor {
    actor("user-one", "legal_manager", &["legal.read", "legal.manage", "legal.destroy"])
}

fn reviewer() -> LegalAuxiliaryActor {
    actor("user-two", "legal_manager", &["legal.manage", "legal.destroy"])
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}


async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)
         VALUES('company-a','company','company-a','Company A',NULL,'company-a',NULL,'active'),
               ('company-b','company','company-b','Company B',NULL,'company-b',NULL,'active');
         INSERT INTO legal_settings(company_id,enabled,ai_enabled) VALUES('company-a',true,true);
         INSERT INTO legal_items(id,company_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,state,version,created_at,updated_at,sla_due_at)
         VALUES
          ('matter-open','company-a','matter','Open matter','','internal','TH','TH','en','THB','Asia/Bangkok','normal','2026-01-10','user-one','user-one','open',1,1000,1000,2000),
          ('request-mine','company-a','request','My request','','internal','TH','TH','en','THB','Asia/Bangkok','normal','2026-02-01','user-one','user-one','submitted',1,1000,1000,2000),
          ('request-other','company-a','request','Other request','','confidential','TH','TH','en','THB','Asia/Bangkok','normal','2026-02-02','user-two','user-two','submitted',1,1000,1000,2000),
          ('contract-secret','company-a','contract','Secret contract','','confidential','TH','TH','en','THB','Asia/Bangkok','normal','2026-03-01','user-two','user-two','active',1,1000,1000,2000),
          ('matter-other','company-b','matter','Other company matter','','internal','TH','TH','en','THB','Asia/Bangkok','normal','2026-01-05','user-nine','user-nine','open',1,1000,1000,2000),
          ('rule-law','company-a','rule','Labour rule','','internal','TH','TH','th','THB','Asia/Bangkok','normal',NULL,'user-one','user-one','active',1,1000,1000,NULL),
          ('matter-archive','company-a','matter','Archived matter','','internal','TH','TH','en','THB','Asia/Bangkok','normal',NULL,'user-one','user-one','archived',1,1000,1000,NULL),
          ('matter-held','company-a','matter','Held matter','','internal','TH','TH','en','THB','Asia/Bangkok','normal',NULL,'user-one','user-one','archived',1,1000,1000,NULL);
         INSERT INTO legal_members(item_id,user_id,access) VALUES('contract-secret','user-one','read');
         INSERT INTO legal_matters(item_id,data) VALUES('matter-open','{}'),('matter-archive','{}'),('matter-held','{}');
         INSERT INTO legal_requests(item_id,data) VALUES('request-mine','{}'),('request-other','{}');
         INSERT INTO legal_contracts(item_id,data) VALUES('contract-secret','{}');
         INSERT INTO legal_rules(item_id,data) VALUES('rule-law','{}');
         INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,object_key,locked,created_by,created_at)
         VALUES('doc-law','rule-law',1,'labour.pdf','application/pdf','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',10,'',NULL,false,'user-one',1000),
               ('doc-archive','matter-archive',1,'evidence.txt','text/plain','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',10,'evidence',NULL,false,'user-one',1000),
               ('doc-review','matter-open',1,'review.pdf','application/pdf','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',10,'',NULL,false,'user-one',1000);
         INSERT INTO legal_text_reviews(id,company_id,item_id,document_id,document_checksum,extraction_method,state,version,created_by,created_at)
         VALUES('review-one','company-a','matter-open','doc-review','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','manual','approved',1,'user-one',1000);
         INSERT INTO legal_source_chunks(id,company_id,item_id,document_id,document_checksum,revision_id,text_review_id,page,page_mapping,section,text,text_checksum)
         VALUES('chunk-review','company-a','matter-open','doc-review','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',NULL,'review-one',2,true,'2','The written evidence clause was independently reviewed.','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');
         INSERT INTO legal_law_sources(id,company_id,rule_id,title,source_url,state,version,created_by,created_at)
         VALUES('source-law','company-a','rule-law','Labour Protection Act','https://law.example/labour.pdf','active',1,'user-one',1000);
         INSERT INTO legal_law_revisions(id,company_id,rule_id,source_id,document_id,document_checksum,revision_label,published_on,effective_from,effective_until,extraction_method,state,version,created_by,created_at)
         VALUES('revision-law','company-a','rule-law','source-law','doc-law','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','2020','2020-01-01','2020-01-01',NULL,'manual','approved',1,'user-one',1000);
         INSERT INTO legal_law_applicability(id,company_id,rule_id,revision_id,applies,basis,state,version,created_by,created_at)
         VALUES('applicability-law','company-a','rule-law','revision-law',true,'Company employs staff','approved',1,'user-one',1000);
         INSERT INTO legal_source_chunks(id,company_id,item_id,document_id,document_checksum,revision_id,text_review_id,page,page_mapping,section,text,text_checksum)
         VALUES('chunk-law','company-a','rule-law','doc-law','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','revision-law',NULL,1,true,'1','An employer must provide a written contract clause.','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');
         INSERT INTO legal_saved_views(company_id,user_id,name,query) VALUES('company-a','user-one','My view','{\"kind\":\"matter\"}'::jsonb);
         INSERT INTO legal_intake_forms(id,company_id,name,fields,sla_days,state,created_by,version)
         VALUES('form-one','company-a','Intake','[{\"key\":\"subject\",\"label\":\"Subject\",\"required\":true,\"type\":\"text\"}]'::jsonb,5,'approved','user-one',1);
         INSERT INTO legal_authorities(id,item_id,company_id,user_id,document_id,currency,limit_minor,valid_from,valid_until,state,created_by,version)
         VALUES('authority-one','contract-secret','company-a','user-one','doc-law','THB',1000,'2026-01-01','2026-12-31','proposed','user-one',1);
         INSERT INTO legal_ai_proposals(id,item_id,source_version,source_hash,question,state,created_by,created_at)
         VALUES('ai-one','matter-open',1,'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc','What applies?','proposed','user-one',1500);
         INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at)
         VALUES('hold-one','matter-held','Preserve','active','user-one',1000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn read(
    pool: &DatabasePool,
    actor: LegalAuxiliaryActor,
    read: LegalAuxiliaryRead,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let read = read.clone();
            Box::pin(async move {
                let mut ports = PgLegalAuxiliaryReads::new(connection);
                read_auxiliary(&mut ports, &actor, read, &FixedClock(1_794_330_000_000)).await
            })
        })
        .await
}

fn code(error: PgTransactionError<BusinessError>) -> String {
    match error {
        PgTransactionError::Business(error) => error.code.to_owned(),
        PgTransactionError::Persistence(_) => "PERSISTENCE".into(),
    }
}

#[tokio::test]
async fn document_citation_returns_full_source_and_audits_each_read() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;
        sqlx::raw_sql("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,object_key,locked,created_by,created_at) VALUES('doc-secret','contract-secret',1,'secret.txt','text/plain','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',6,'secret',NULL,false,'user-two',1000),('doc-mismatch','matter-open',2,'mismatch.pdf','application/pdf','cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',10,'',NULL,false,'user-one',1000),('doc-object','matter-open',3,'object.pdf','application/pdf','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',10,'','unavailable-object',false,'user-one',1000); INSERT INTO legal_text_reviews(id,company_id,item_id,document_id,document_checksum,extraction_method,state,version,created_by,created_at) VALUES('review-mismatch','company-a','matter-open','doc-mismatch','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','manual','approved',1,'user-one',1000); INSERT INTO legal_source_chunks(id,company_id,item_id,document_id,document_checksum,revision_id,text_review_id,page,page_mapping,section,text,text_checksum) VALUES('chunk-no-page','company-a','matter-open','doc-review','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',NULL,'review-one',2,false,'clause','Unmapped passage','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'),('chunk-mismatch','company-a','matter-open','doc-mismatch','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',NULL,'review-mismatch',1,true,'clause','Stale passage','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb')").execute(pool.raw()).await.unwrap();
        let citation = |id: &str| LegalAuxiliaryRead::Citation { id: id.into(), as_of: None };
        let actual = read(&pool, manager(), citation("document:chunk-review")).await.unwrap();
        assert_eq!(actual, json!({"kind":"document","title":"Open matter","name":"review.pdf","item_id":"matter-open","document_id":"doc-review","version":1,"checksum":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","page":2,"page_mapping":true,"section":"2","text":"The written evidence clause was independently reviewed.","notice":"Read this immutable original and verify that the cited passage supports the conclusion. Legal interpretation requires independent review."}));
        read(&pool, manager(), citation("document:chunk-review")).await.unwrap();
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM legal_access_events WHERE item_id='matter-open' AND document_id='doc-review' AND action='document.citation.read' AND actor_id='user-one'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(events, 2);
        let plain = read(&pool, manager(), citation("doc-archive")).await.unwrap();
        assert_eq!((plain["text"].as_str(), plain["page"].is_null(), plain["page_mapping"].as_bool()), (Some("evidence"), true, Some(false)));
        let unmapped = read(&pool, manager(), citation("document:chunk-no-page")).await.unwrap();
        assert_eq!((unmapped["page"].is_null(), unmapped["page_mapping"].as_bool(), unmapped["text"].as_str()), (true, Some(false), Some("Unmapped passage")));
        assert_eq!(code(read(&pool, manager(), citation("document:chunk-mismatch")).await.unwrap_err()), "LEGAL_CITATION_UNAVAILABLE");
        assert_eq!(code(read(&pool, manager(), citation("doc-object")).await.unwrap_err()), "LEGAL_CITATION_UNAVAILABLE");
        assert_eq!(code(read(&pool, manager(), citation("document:missing")).await.unwrap_err()), "LEGAL_CITATION_UNAVAILABLE");
        assert_eq!(code(read(&pool, manager(), citation("doc-review")).await.unwrap_err()), "LEGAL_CITATION_UNAVAILABLE");
        let outsider = actor("outsider", "legal_manager", &["legal.read"]);
        assert_eq!(code(read(&pool, outsider.clone(), citation("doc-secret")).await.unwrap_err()), "LEGAL_NOT_FOUND");
        assert_eq!(code(read(&pool, outsider, citation("missing-document")).await.unwrap_err()), "LEGAL_NOT_FOUND");
        sqlx::raw_sql("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at) VALUES('citation-upload','legal','matter-open','legal_document','revoked.pdf','application/pdf',10,10,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','citation-stage','citation-object','ready','user-one',9999999999999,0,0); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at,revoked_at) VALUES('citation-source','citation-upload','legal','matter-open','legal_document','citation-object',1,'revoked.pdf','application/pdf',10,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','user-one',0,'company-a','application/pdf','available',0,0,1); INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at,bound_at,bound_command_id) VALUES('citation-set','company-a','legal_item','matter-open','record','legal_document',1,1,'internal','{}','bound','user-one',0,0,0,'citation-binding'); INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) VALUES('citation-link','citation-set','citation-source','bound','user-one',0,9999999999999,0,'legal_document','citation-revoked'); INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,object_key,locked,created_by,created_at,attachment_link_id,attachment_source_id) VALUES('citation-revoked','matter-open',4,'revoked.pdf','application/pdf','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',10,'','citation-object',false,'user-one',0,'citation-link','citation-source')").execute(pool.raw()).await.unwrap();
        assert_eq!(code(read(&pool, manager(), citation("citation-revoked")).await.unwrap_err()), "LEGAL_CITATION_UNAVAILABLE");
        sqlx::raw_sql("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at) VALUES('citation-disposal','matter-archive','policy',1,'user-one','requested','fixture','{}',0); INSERT INTO legal_disposals(document_id,destruction_id,checksum,disposed_at) VALUES('doc-archive','citation-disposal','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',1)").execute(pool.raw()).await.unwrap();
        assert_eq!(code(read(&pool, manager(), citation("doc-archive")).await.unwrap_err()), "LEGAL_DOCUMENT_DISPOSED");
        let law = read(&pool, manager(), LegalAuxiliaryRead::Citation { id:"law:chunk-law".into(), as_of:Some("2026-11-15".into()) }).await.unwrap();
        assert_eq!(law["kind"], "law");
        assert_eq!(law["originalUrl"], "https://law.example/labour.pdf#page=1");
        read(&pool, manager(), LegalAuxiliaryRead::Citation { id:"law:chunk-law".into(), as_of:Some("2026-11-15".into()) }).await.unwrap();
        let law_events:i64=sqlx::query_scalar("SELECT count(*) FROM legal_access_events WHERE document_id='doc-law' AND actor_id='user-one' AND action='law.citation.read'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(law_events,2);
        let same_clock:i64=sqlx::query_scalar("SELECT count(*) FROM legal_access_events WHERE document_id='doc-law' AND action='law.citation.read' AND created_at=1794330000000").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(same_clock,2);
    }).await;
}

#[tokio::test]
async fn document_citation_retention_executing_uses_legacy_error() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await; seed(&pool).await;
        sqlx::query("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at) VALUES('citation-executing','matter-archive','policy',1,'user-one','executing','fixture','{}',0)").execute(pool.raw()).await.unwrap();
        let failure=read(&pool,manager(),LegalAuxiliaryRead::Citation{id:"doc-archive".into(),as_of:None}).await.unwrap_err();
        assert_eq!(code(failure),"LEGAL_RETENTION_FROZEN");
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM legal_access_events WHERE document_id='doc-archive'").fetch_one(pool.raw()).await.unwrap();assert_eq!(n,0);
        sqlx::query("UPDATE legal_destructions SET state='completed' WHERE id='citation-executing'").execute(pool.raw()).await.unwrap();
        let failure=read(&pool,manager(),LegalAuxiliaryRead::Citation{id:"doc-archive".into(),as_of:None}).await.unwrap_err();
        assert_eq!(code(failure),"LEGAL_CITATION_UNAVAILABLE");
        sqlx::query("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at) VALUES('law-executing','rule-law','policy',1,'user-one','executing','fixture','{}',0)").execute(pool.raw()).await.unwrap();
        let failure=read(&pool,manager(),LegalAuxiliaryRead::Citation{id:"law:chunk-law".into(),as_of:Some("2026-11-15".into())}).await.unwrap_err();assert_eq!(code(failure),"LEGAL_CITATION_UNAVAILABLE");
    }).await;
}

#[tokio::test]
async fn citation_source_rechecks_owner_drift_and_rolls_back_audit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;
        sqlx::query("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at) VALUES('doc-secret','contract-secret',1,'secret','text/plain','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',1,'secret','user-two',0)").execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE legal_items SET updated_at=9007199254740992 WHERE id='matter-open'").execute(pool.raw()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(inspect_legal_citation_source(&mut tx, &manager(), "document:chunk-review").await.is_err());
        lock_legal_citation_sources(&mut tx).await.unwrap();
        let doc = inspect_legal_citation_source(&mut tx, &manager(), "document:chunk-review").await.unwrap().unwrap();
        let law = inspect_legal_citation_source(&mut tx, &manager(), "law:chunk-law").await.unwrap().unwrap();
        assert_eq!((doc.item_id(), doc.document_id(), doc.kind()), ("matter-open", "doc-review", "matter"));
        assert!(inspect_legal_citation_source(&mut tx, &actor("outsider", "legal_manager", &["legal.read"]), "doc-secret").await.unwrap().is_none());
        let mut ports = PgLegalAuxiliaryReads::new(&mut tx);
        read_auxiliary(&mut ports, &manager(), LegalAuxiliaryRead::Citation { id:"document:chunk-review".into(), as_of:None }, &FixedClock(100)).await.unwrap();
        let audit = ports.take_citation_audit().await.unwrap();
        audit.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT audit_tamper").execute(&mut *tx).await.unwrap();
        assert!(sqlx::query("UPDATE legal_access_events SET action='forged' WHERE document_id='doc-review'").execute(&mut *tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT audit_tamper").execute(&mut *tx).await.unwrap();
        audit.require_current(&mut tx).await.unwrap();
        doc.require_current(&mut tx).await.unwrap();
        law.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT source_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE legal_items SET title='changed' WHERE id='matter-open'").execute(&mut *tx).await.unwrap();
        assert!(doc.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT source_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE legal_items SET updated_at=9007199254740993 WHERE id='matter-open'").execute(&mut *tx).await.unwrap();
        assert!(doc.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT source_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO legal_law_applicability(id,company_id,rule_id,revision_id,applies,basis,state,version,created_by,created_at) VALUES('new-applicability','company-a','rule-law','revision-law',false,'changed','approved',1,'user-one',2000)").execute(&mut *tx).await.unwrap();
        assert!(law.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let events:i64=sqlx::query_scalar("SELECT count(*) FROM legal_access_events WHERE document_id='doc-review' AND action='document.citation.read'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(events,0);
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT before_citation_lock").execute(&mut *tx).await.unwrap();
        lock_legal_citation_sources(&mut tx).await.unwrap();
        assert!(doc.require_current(&mut tx).await.is_err());
        assert!(audit.require_current(&mut tx).await.is_err());
        let source=inspect_legal_citation_source(&mut tx,&manager(),"document:chunk-review").await.unwrap().unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_citation_lock").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn citation_audit_rejects_suppressed_or_altered_insert() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await; seed(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_legal_citation_sources(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT before_trigger").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION pg_temp.suppress_citation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER test_suppress_citation BEFORE INSERT ON legal_access_events FOR EACH ROW EXECUTE FUNCTION pg_temp.suppress_citation()").execute(&mut *tx).await.unwrap();
        let mut ports=PgLegalAuxiliaryReads::new(&mut tx);
        assert!(read_auxiliary(&mut ports,&manager(),LegalAuxiliaryRead::Citation{id:"document:chunk-review".into(),as_of:None},&FixedClock(100)).await.is_err());
        assert!(ports.take_citation_audit().await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT before_trigger").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION pg_temp.alter_citation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.actor_id='forged'; RETURN NEW; END $$; CREATE TRIGGER test_alter_citation BEFORE INSERT ON legal_access_events FOR EACH ROW EXECUTE FUNCTION pg_temp.alter_citation()").execute(&mut *tx).await.unwrap();
        let mut ports=PgLegalAuxiliaryReads::new(&mut tx);
        assert!(read_auxiliary(&mut ports,&manager(),LegalAuxiliaryRead::Citation{id:"document:chunk-review".into(),as_of:None},&FixedClock(100)).await.is_err());
        let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM legal_access_events WHERE document_id='doc-review'").fetch_one(&mut *tx).await.unwrap();assert_eq!(remaining,0);
        tx.rollback().await.unwrap();
        let events:i64=sqlx::query_scalar("SELECT count(*) FROM legal_access_events WHERE document_id='doc-review'").fetch_one(pool.raw()).await.unwrap();assert_eq!(events,0);
    }).await;
}

#[tokio::test]
async fn auxiliary_reads_keep_company_membership_and_requester_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;

        let workspace = read(&pool, manager(), LegalAuxiliaryRead::Workspace)
            .await
            .unwrap();
        let ids: Vec<&str> = workspace["deadlines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["matter-open", "request-mine", "contract-secret"]);
        assert!(!ids.contains(&"matter-other"));
        assert!(!ids.contains(&"request-other"));

        let assigned: Vec<&str> = workspace["assigned"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert!(assigned.contains(&"matter-open"));
        assert!(!assigned.contains(&"contract-secret"));

        let requester = read(
            &pool,
            actor("user-one", "legal_requester", &["legal.read"]),
            LegalAuxiliaryRead::Workspace,
        )
        .await
        .unwrap();
        let requester_ids: Vec<&str> = requester["deadlines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(requester_ids, vec!["request-mine"]);

        let reports = read(&pool, manager(), LegalAuxiliaryRead::Reports)
            .await
            .unwrap();
        assert_eq!(reports["workflows"].as_array().unwrap().len(), 10);
        assert!(reports["asOf"].as_i64().unwrap() > 0);
        assert!(!reports["registers"].as_array().unwrap().is_empty());

        // A confidential record is readable through membership only.
        let retention = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::Retention {
                id: Some("contract-secret".into()),
            },
        )
        .await
        .unwrap();
        assert!(retention["assignment"].is_null());
        let outsider = read(
            &pool,
            actor("user-three", "legal_manager", &["legal.read"]),
            LegalAuxiliaryRead::Retention {
                id: Some("contract-secret".into()),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(code(outsider), "NOT_FOUND");

        let governance = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::Governance {
                id: Some("contract-secret".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(governance["forms"].as_array().unwrap().len(), 1);
        assert_eq!(governance["forms"][0]["fields"][0]["key"], "subject");
        assert_eq!(governance["authorities"].as_array().unwrap().len(), 1);
        assert_eq!(governance["settings"]["enabled"], true);

        let references = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::References {
                item_id: "matter-open".into(),
            },
        )
        .await
        .unwrap();
        assert!(references.as_array().unwrap().is_empty());

        assert!(read(&pool, manager(), LegalAuxiliaryRead::Trust)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        let views = read(&pool, manager(), LegalAuxiliaryRead::SavedViews)
            .await
            .unwrap();
        assert_eq!(views[0]["name"], "My view");
        assert_eq!(views[0]["query"]["kind"], "matter");
        let history = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::AiHistory {
                item_id: "matter-open".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(history[0]["id"], "ai-one");

        // The owner read requires its exact permission and a current Company.
        let denied = read(
            &pool,
            actor("user-one", "legal_manager", &[]),
            LegalAuxiliaryRead::Reports,
        )
        .await
        .unwrap_err();
        assert_eq!(code(denied), "PERMISSION_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn law_corpus_reads_require_approved_applicability_and_audit_the_access() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;

        let corpus = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::LawCorpus {
                rule_id: Some("rule-law".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(corpus["sources"].as_array().unwrap().len(), 1);
        assert_eq!(corpus["revisions"].as_array().unwrap().len(), 1);
        assert_eq!(corpus["applicability"].as_array().unwrap().len(), 1);
        assert_eq!(corpus["chunks"].as_array().unwrap().len(), 1);
        assert_eq!(corpus["chunks"][0]["id"], "chunk-law");
        assert!(corpus["notice"].as_str().unwrap().contains("independently reviewed"));

        let knowledge = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::Knowledge {
                query: "contract clause".into(),
                as_of: Some("2026-01-01".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(knowledge["conflicts"], 0);
        assert_eq!(knowledge["sources"][0]["kind"], "law");
        assert_eq!(knowledge["sources"][0]["item_id"], "rule-law");
        assert_eq!(knowledge["asOf"], "2026-01-01");

        let reviewed = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::Knowledge {
                query: "written evidence".into(),
                as_of: Some("2026-01-01".into()),
            },
        )
        .await
        .unwrap();
        assert!(reviewed["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["sourceId"] == "document:chunk-review" && source["kind"] == "document"));

        let authored = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::Knowledge {
                query: "evidence".into(),
                as_of: Some("2026-01-01".into()),
            },
        )
        .await
        .unwrap();
        assert!(authored["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|source| source["document_id"] == "doc-archive" && source["kind"] == "document"));

        let citation = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::LawCitation {
                id: "law:chunk-law".into(),
                as_of: Some("2026-01-01".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(citation["id"], "chunk-law");
        assert_eq!(citation["href"], "/legal?record=rule-law");
        assert!(citation["originalUrl"].as_str().unwrap().ends_with("#page=1"));

        let audited = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM legal_access_events WHERE action='law.citation.read' AND actor_id='user-one'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audited, 1);

        // An unapproved revision is not evidence, even when the chunk exists.
        sqlx::query("UPDATE legal_law_revisions SET state='pending' WHERE id='revision-law'")
            .execute(pool.raw())
            .await
            .unwrap();
        let excluded = read(
            &pool,
            manager(),
            LegalAuxiliaryRead::Knowledge {
                query: "contract clause".into(),
                as_of: Some("2026-01-01".into()),
            },
        )
        .await
        .unwrap();
        assert!(excluded["sources"].as_array().unwrap().is_empty());
        assert_eq!(excluded["conflicts"], 0);
    })
    .await;
}

#[tokio::test]
async fn retention_commands_retain_independence_replay_and_audit_effects() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;

        let create = json!({"action":"retention.policy","name":"Seven year retention","days":1,"reason":"Group retention policy"});
        let created = execute_retention(
            &pool,
            &manager(),
            &key("retention-policy-one"),
            &hash(&create),
            &create,
            1_000,
        )
        .await
        .unwrap();
        let policy_id = created["policyId"].as_str().unwrap().to_owned();

        let approve = json!({"action":"retention.approve-policy","policyId":policy_id,"policyVersion":1,"reason":"Independent approval"});
        let approved = execute_retention(
            &pool,
            &reviewer(),
            &key("retention-approve-one"),
            &hash(&approve),
            &approve,
            1_001,
        )
        .await
        .unwrap();
        assert_eq!(approved["policyId"], policy_id);

        // The creator cannot approve their own policy.
        let other = json!({"action":"retention.policy","name":"Second policy","days":1,"reason":"Group retention policy"});
        let second = execute_retention(
            &pool,
            &manager(),
            &key("retention-policy-two"),
            &hash(&other),
            &other,
            1_002,
        )
        .await
        .unwrap();
        let self_approve = json!({"action":"retention.approve-policy","policyId":second["policyId"],"policyVersion":1,"reason":"Not independent"});
        let error = execute_retention(
            &pool,
            &manager(),
            &key("retention-approve-two"),
            &hash(&self_approve),
            &self_approve,
            1_003,
        )
        .await
        .unwrap_err();
        assert_eq!(code(error), "LEGAL_INDEPENDENT_REVIEW");

        let assign = json!({"action":"retention.assign","id":"matter-archive","expectedVersion":1,"policyId":policy_id,"reason":"Closed matter"});
        let assigned = execute_retention(
            &pool,
            &manager(),
            &key("retention-assign-one"),
            &hash(&assign),
            &assign,
            1_004,
        )
        .await
        .unwrap();
        assert_eq!(assigned["version"], 2);
        // The same key and body replays the retained result without another write.
        let replay = execute_retention(
            &pool,
            &manager(),
            &key("retention-assign-one"),
            &hash(&assign),
            &assign,
            1_005,
        )
        .await
        .unwrap();
        assert_eq!(replay, assigned);

        let request = json!({"action":"retention.request","id":"matter-archive","expectedVersion":2,"reason":"Retention elapsed"});
        let requested = execute_retention(
            &pool,
            &manager(),
            &key("retention-request-one"),
            &hash(&request),
            &request,
            86_500_000,
        )
        .await
        .unwrap();
        assert_eq!(requested["version"], 3);
        let destruction: String = sqlx::query_scalar(
            "SELECT id FROM legal_destructions WHERE item_id='matter-archive'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(!destruction.is_empty());

        let approve_destruction = json!({"action":"retention.approve","id":"matter-archive","expectedVersion":3,"requestId":destruction,"confirmation":destruction,"reason":"Independent authorization"});
        let approved = execute_retention(
            &pool,
            &reviewer(),
            &key("retention-approve-destruction"),
            &hash(&approve_destruction),
            &approve_destruction,
            86_500_001,
        )
        .await
        .unwrap();
        assert_eq!(approved["version"], 4);
        let row: (String, Option<String>, i64) = sqlx::query_as(
            "SELECT state,approved_by,version::bigint FROM legal_destructions WHERE item_id='matter-archive'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("approved".to_owned(), Some("user-two".to_owned()), 2));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT version::bigint FROM legal_items WHERE id='matter-archive'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            4
        );
        // policy create, policy approve, policy create two, assign, request, approve
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM command_receipts")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            6
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_events")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            6
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox_events")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            6
        );
        // assign, request and approve advance the record and append one event.
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legal_events WHERE item_id='matter-archive'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            3
        );

        // An active hold blocks the destruction request.
        let assign_held = json!({"action":"retention.assign","id":"matter-held","expectedVersion":1,"policyId":policy_id,"reason":"Closed matter"});
        execute_retention(
            &pool,
            &manager(),
            &key("retention-assign-held"),
            &hash(&assign_held),
            &assign_held,
            1_006,
        )
        .await
        .unwrap();
        let request_held = json!({"action":"retention.request","id":"matter-held","expectedVersion":2,"reason":"Retention elapsed"});
        let error = execute_retention(
            &pool,
            &manager(),
            &key("retention-request-held"),
            &hash(&request_held),
            &request_held,
            86_500_002,
        )
        .await
        .unwrap_err();
        assert_eq!(code(error), "LEGAL_RETENTION_HOLD");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM legal_destructions WHERE item_id='matter-held'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );

        // An unrelated Company cannot read or act on the policy.
        let mut outsider = manager();
        outsider.company_id = CompanyId::new("company-b").unwrap();
        let error = execute_retention(
            &pool,
            &outsider,
            &key("retention-assign-other"),
            &hash(&assign),
            &assign,
            1_007,
        )
        .await
        .unwrap_err();
        assert_eq!(code(error), "NOT_FOUND");
    })
    .await;
}

struct RetentionBackend {
    calls: Mutex<Vec<(String, String, i64)>>,
}

impl LegalRetentionHttpBackend for RetentionBackend {
    fn resolve_actor<'a>(
        &'a self,
        _: LegalRetentionHttpSessionRequest<'a>,
    ) -> LegalRetentionActorFuture<'a> {
        Box::pin(async { Ok(manager()) })
    }

    fn command<'a>(
        &'a self,
        request: LegalRetentionHttpCommandRequest<'a>,
    ) -> LegalRetentionHttpFuture<'a> {
        self.calls.lock().unwrap().push((
            request.actor.user_id.as_str().to_owned(),
            request.command.action.operation().to_owned(),
            request.now,
        ));
        let actor = request.actor.clone();
        let key = request.key.clone();
        let request_hash = request.request_hash.to_owned();
        let raw = request.raw.clone();
        let pool = request.pool.clone();
        Box::pin(async move {
            execute_retention(&pool, &actor, &key, &request_hash, &raw, request.now)
                .await
                .map_err(legal_retention_business_error)
        })
    }
}

fn retention_headers(origin: Option<&str>, key: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, HeaderValue::from_static("acme.example"));
    if let Some(origin) = origin {
        headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
    }
    if let Some(key) = key {
        headers.insert(
            HeaderName::from_static("idempotency-key"),
            HeaderValue::from_str(key).unwrap(),
        );
    }
    headers
}

#[tokio::test]
async fn retention_router_receives_actor_and_pool_and_serves_the_native_family() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed(&pool).await;
        let backend = Arc::new(RetentionBackend {
            calls: Mutex::new(Vec::new()),
        });
        let service = LegalRetentionHttpService::new(vec![LegalRetentionHttpTarget {
            host: "acme.example".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example".into(),
            pool: pool.clone(),
            clock: Arc::new(FixedClock(1_794_330_000_000)),
            backend: backend.clone(),
        }])
        .unwrap();

        let body = json!({
            "action":"retention.policy",
            "name":"HTTP seven year policy",
            "days":1,
            "reason":"Group retention policy"
        })
        .to_string()
        .into_bytes();
        let response = service
            .command(
                "acme.example",
                "acme",
                &retention_headers(Some("https://acme.example"), Some("retention-http-one")),
                &body,
            )
            .await;
        let (parts, response_body) = response.into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        assert_eq!(parts.headers.get(header::CACHE_CONTROL).unwrap(), "private, no-store");
        let value: Value =
            serde_json::from_slice(&to_bytes(response_body, 8192).await.unwrap()).unwrap();
        assert_eq!(
            value["data"]["policyId"],
            "retention-retention-http-one"
        );
        // The backend received the proof-derived actor, the caller pool and the
        // service clock; the router itself never executes the command.
        assert_eq!(
            backend.calls.lock().unwrap().as_slice(),
            [(
                "user-one".to_owned(),
                "retention.policy".to_owned(),
                1_794_330_000_000
            )]
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_retention_policies WHERE id='retention-retention-http-one'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );

        // Same key + body replays the retained result through the same contract.
        let replay = service
            .command(
                "acme.example",
                "acme",
                &retention_headers(Some("https://acme.example"), Some("retention-http-one")),
                &body,
            )
            .await;
        let (parts, response_body) = replay.into_parts();
        assert_eq!(parts.status, StatusCode::OK);
        let value: Value =
            serde_json::from_slice(&to_bytes(response_body, 8192).await.unwrap()).unwrap();
        assert_eq!(value["data"]["policyId"], "retention-retention-http-one");

        // Transport failures never reach the backend.
        let cases = [
            (
                retention_headers(Some("https://other.example"), Some("retention-http-two")),
                body.clone(),
                StatusCode::FORBIDDEN,
            ),
            (
                retention_headers(Some("https://acme.example"), None),
                body.clone(),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                retention_headers(Some("https://acme.example"), Some("retention-http-three")),
                json!({"action":"trust.add","reason":"Not retention"}).to_string().into_bytes(),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ];
        for (headers, body, expected) in cases {
            let response = service.command("acme.example", "acme", &headers, &body).await;
            assert_eq!(response.into_parts().0.status, expected);
        }
        let unknown_holding = service
            .command(
                "acme.example",
                "other",
                &retention_headers(Some("https://acme.example"), Some("retention-http-four")),
                &body,
            )
            .await;
        assert_eq!(unknown_holding.into_parts().0.status, StatusCode::NOT_FOUND);
        assert_eq!(backend.calls.lock().unwrap().len(), 2);
    })
    .await;
}

fn hash(body: &serde_json::Value) -> String {
    erpx_server_legal::public::legal_records::sha256_hex(body.to_string().as_bytes())
}

#[allow(dead_code)]
fn _types(connection: &mut PgConnection) {
    let _: AssertSqlSafe<&str> = AssertSqlSafe("SELECT 1");
    let _ = connection;
    let _ = RecordId::new("id").unwrap();
}

mod destruction {
    //! Native-seeded destruction journal/retry tests. The bridge below is a test
    //! oracle for control/Files effects; it is not a production authority adapter.
    use super::fixture;

    use erpx_server_legal::public::{
        CurrentDestructionSource, DestructionLedgerEntry, LegalDestructionJournal,
        LegalDestructionManifest, LockedDestructionSource, PgLegalDestructionBridge,
        PgLegalDestructionManifestBridge, LegalQueuedIdentity, LegalDestructionExecution,
        LegalRetentionUnit, read_legal_queued_authority, execute_legal_destruction,
        execute_with_sources, journal_legal_destruction, prepare_legal_destruction_manifest,
    };
    use erpx_server_legal::public::auxiliary::{LegalRetentionPorts, PgLegalRetention};
    use erpx_server_runtime::public::postgres::{
        DatabasePool, MigrationTrack, PersistenceError, PgTransactionError, PgUnitOfWork,
        TransactionOptions, load_migrations, migrate_database,
    };
    use erpx_server_shared::public::{BusinessError, IdentifierSource, RecordId};
    use sqlx::PgConnection;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    type Error = PgTransactionError<BusinessError>;

    #[derive(Clone)]
    struct Bridge {
        deletes: Arc<AtomicUsize>,
        fail_seal: Arc<AtomicBool>,
    }
    impl PgLegalDestructionBridge for Bridge {
        async fn prepare(&mut self, _: &mut PgConnection, _: &str, _: &str) -> Result<(), Error> {
            Ok(())
        }
        async fn assert_current(
            &mut self,
            _: &mut PgConnection,
            _: &str,
            _: &str,
        ) -> Result<(), Error> {
            Ok(())
        }
        async fn locked_source(
            &mut self,
            _: &mut PgConnection,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<Option<LockedDestructionSource>, Error> {
            unreachable!("empty manifest")
        }
        async fn link_current(
            &mut self,
            _: &mut PgConnection,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<bool, Error> {
            unreachable!("empty manifest")
        }
        async fn source_shared(
            &mut self,
            _: &mut PgConnection,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<bool, Error> {
            unreachable!("empty manifest")
        }
        async fn retained_ledger(
            &mut self,
            _: &mut PgConnection,
            _: &str,
        ) -> Result<Vec<DestructionLedgerEntry>, Error> {
            unreachable!("empty manifest")
        }
        async fn object_shared(
            &mut self,
            _: &mut PgConnection,
            _: &str,
            _: &str,
        ) -> Result<bool, Error> {
            unreachable!("empty manifest")
        }
        async fn destroy(
            &mut self,
            _: &mut PgConnection,
            _: &str,
            _: &str,
            _: &LegalDestructionManifest,
        ) -> Result<(), Error> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn seal(
            &mut self,
            _: &mut PgConnection,
            _: &str,
            _: &str,
            _: &LegalDestructionManifest,
            _: i64,
        ) -> Result<(), Error> {
            if self.fail_seal.load(Ordering::SeqCst) {
                return Err(PersistenceError::new("TEST_FAILURE_AFTER_BYTES").into());
            }
            Ok(())
        }
    }

    impl PgLegalDestructionManifestBridge for Bridge {
        async fn prepare_manifest(&mut self, _: &mut PgConnection, _: &str, _: &str) -> Result<(), Error> { Ok(()) }
        async fn canonical_source(&mut self, _: &mut PgConnection, _: &str, _: &str, _: &str) -> Result<Option<CurrentDestructionSource>, Error> { unreachable!("legacy-only test") }
        async fn stored_ledger(&mut self, _: &mut PgConnection, _: &str) -> Result<Vec<DestructionLedgerEntry>, Error> { unreachable!("legacy-only test") }
    }

    async fn setup(pool: &DatabasePool) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
            .await
            .unwrap();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company-a','company','company-a','Company A',NULL,'company-a',NULL,'active');
            INSERT INTO legal_items(id,company_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,state,version,created_at,updated_at) VALUES('item-a','company-a','matter','Archived','','internal','TH','TH','en','THB','Asia/Bangkok','normal','requester','requester','archived',1,1,1);
            INSERT INTO legal_retention_policies(id,company_id,name,days,basis,state,created_by,approved_by,version,created_at) VALUES('policy-a','company-a','Policy',1,'Retention','approved','requester','reviewer',1,1);
            INSERT INTO legal_retention_assignments(item_id,policy_id,policy_version,eligible_at,assigned_by,created_at) VALUES('item-a','policy-a',1,1,'requester',1);
            INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,approved_by,state,reason,manifest,created_at,approved_at) VALUES('destruction-a','item-a','policy-a',1,'requester','reviewer','approved','expired','{\"documents\":[],\"uploads\":[],\"keys\":[],\"canonicalSources\":[]}',1,2)")
            .execute(pool.raw()).await.unwrap();
    }

    #[tokio::test]
    async fn queued_authority_reads_legacy_snapshot_and_rejects_source_or_transaction_drift() {
        use sha2::{Digest, Sha256};
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            // Exact source image used by the existing TypeScript producer;
            // hash it independently in Rust rather than using the port's SQL.
            let image: String = sqlx::query_scalar("SELECT jsonb_build_object('request',x.id,'item',i.id,'company',i.company_id,'branch',i.branch_id,'department',i.department_id,'kind',i.kind,'classification',i.classification,'state',i.state,'version',i.version,'requester',x.requested_by,'approver',x.approved_by,'manifest',x.manifest,'policyId',x.policy_id,'policyVersion',x.policy_version,'policy',to_jsonb(p))::text FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id JOIN legal_retention_policies p ON p.id=x.policy_id AND p.company_id=i.company_id WHERE x.id='destruction-a'")
                .fetch_one(pool.raw()).await.unwrap();
            let hash = format!("{:x}", Sha256::digest(image.as_bytes()));
            let legacy = serde_json::json!({"holdingId":"holding-a","companyId":"company-a","userId":"reviewer","assignmentId":"assignment-a","role":"manager","scope":{"kind":"holding"},"iam":{"permissions":["legal.destroy"]}}).to_string();
            sqlx::query("INSERT INTO legal_retention_execution_authorizations(command_id,destruction_id,item_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,item_version,created_at) VALUES('approve-key','destruction-a','item-a','company-a','approve','reviewer','assignment-a',$1,$2,1,10)")
                .bind(legacy).bind(hash).execute(pool.raw()).await.unwrap();
            let mut owner = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *owner).await.unwrap();
            let witness = read_legal_queued_authority(&mut owner, "company-a", "destruction-a").await.unwrap();
            assert!(matches!(witness.identity().unwrap(), LegalQueuedIdentity::LegacyWorkContext(_)));
            assert_eq!((witness.actor_id(), witness.assignment_id()), ("reviewer", "assignment-a"));
            witness.require_current(&mut owner).await.unwrap();
            let mut other = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
            assert!(witness.require_current(&mut other).await.is_err());
            other.rollback().await.unwrap();
            sqlx::query("UPDATE legal_items SET version=version+1 WHERE id='item-a'").execute(&mut *owner).await.unwrap();
            assert!(witness.require_current(&mut owner).await.is_err());
            owner.rollback().await.unwrap();
            assert!(read_legal_queued_authority(&mut *pool.raw().acquire().await.unwrap(), "company-a", "destruction-a").await.is_err(), "caller must hold a transaction");
        }).await;
    }

    #[tokio::test]
    async fn snapshot_includes_actual_documents_and_external_uploads() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            sqlx::raw_sql("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,object_key,locked,created_by,created_at) VALUES('doc-a','item-a',1,'doc','text/plain','document-hash',3,'abc','legal/company-a/item-a/doc',false,'requester',1);
                INSERT INTO legal_shares(id,item_id,document_id,email,expires_at,created_by,created_at) VALUES('share-a','item-a','doc-a','user@example.test',9999999999999,'requester',1);
                INSERT INTO legal_external_uploads(id,share_id,object_key,checksum,name,media_type,bytes,created_by,created_at) VALUES('upload-a','share-a','legal/company-a/item-a/upload','upload-hash','upload','text/plain',3,'guest',1)")
                .execute(pool.raw()).await.unwrap();
            let bridge = Bridge { deletes: Arc::new(AtomicUsize::new(0)), fail_seal: Arc::new(AtomicBool::new(false)) };
            let unit = PgUnitOfWork::new(pool.clone());
            let first_bridge = bridge.clone();
            let manifest = unit.run_once(TransactionOptions::default(), move |c| Box::pin(async move {
                let prepared = prepare_legal_destruction_manifest(c, &mut first_bridge.clone(), "company-a", "item-a").await?;
                let mut retention = PgLegalRetention::with_prepared_manifest(c, prepared);
                let wrong = retention.manifest("other-item", "company-a", 10).await.unwrap_err();
                assert!(matches!(wrong, PgTransactionError::Business(ref error) if error.code == "LEGAL_DESTRUCTION_MANIFEST_CHANGED"));
                retention.manifest("item-a", "company-a", 10).await
            })).await.unwrap();
            assert_eq!(manifest["documents"][0]["id"], "doc-a");
            assert_eq!(manifest["uploads"][0]["id"], "upload-a");
            assert_eq!(manifest["keys"], serde_json::json!(["legal/company-a/item-a/doc", "legal/company-a/item-a/upload"]));
            assert_eq!(manifest["canonicalSources"], serde_json::json!([]));
            let stale = unit.run_once(TransactionOptions::default(), move |c| Box::pin(async move {
                prepare_legal_destruction_manifest(c, &mut bridge.clone(), "company-a", "item-a").await
            })).await.unwrap();
            unit.run_once(TransactionOptions::default(), move |c| Box::pin(async move {
                let mut retention = PgLegalRetention::with_prepared_manifest(c, stale);
                let error = retention.manifest("item-a", "company-a", 10).await.unwrap_err();
                assert!(matches!(error, PgTransactionError::Business(ref error) if error.code == "LEGAL_DESTRUCTION_MANIFEST_CHANGED"));
                Ok::<_, Error>(())
            })).await.unwrap();
        }).await;
    }

    #[tokio::test]
    async fn committed_journal_survives_failure_after_byte_effect_and_retry_completes() {
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let deletes = Arc::new(AtomicUsize::new(0));
            let failure = Arc::new(AtomicBool::new(true));
            let bridge = Bridge { deletes: deletes.clone(), fail_seal: failure.clone() };
            let unit = PgUnitOfWork::new(pool.clone());
            let first = bridge.clone();
            let outcome = unit.run_once(TransactionOptions::default(), move |c| Box::pin(async move {
                let mut bridge = first;
                let journal = journal_legal_destruction(c, &mut bridge, "company-a", "destruction-a", "start-a", 10).await?;
                assert_eq!(journal, LegalDestructionJournal::Started);
                let error = execute_legal_destruction(c, &mut bridge, "company-a", "destruction-a", "end-invalid", 11).await.unwrap_err();
                assert!(matches!(error, PgTransactionError::Business(ref error) if error.code == "LEGAL_DESTRUCTION_JOURNAL_NOT_COMMITTED"));
                Ok::<_, Error>(())
            })).await;
            assert!(outcome.is_ok());
            assert_eq!(deletes.load(Ordering::SeqCst), 0);
            let second = bridge.clone();
            let failed = unit.run_once(TransactionOptions::default(), move |c| Box::pin(async move {
                execute_legal_destruction(c, &mut second.clone(), "company-a", "destruction-a", "end-failed", 12).await
            })).await;
            assert!(failed.is_err());
            assert_eq!(deletes.load(Ordering::SeqCst), 1);
            let state: String = sqlx::query_scalar("SELECT state FROM legal_destructions WHERE id='destruction-a'").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(state, "executing");
            let events: i64 = sqlx::query_scalar("SELECT count(*) FROM legal_events WHERE item_id='item-a' AND action='retention.started'").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(events, 1);
            failure.store(false, Ordering::SeqCst);
            let result = unit.run_once(TransactionOptions::default(), move |c| Box::pin(async move {
                execute_legal_destruction(c, &mut bridge.clone(), "company-a", "destruction-a", "end-ok", 13).await
            })).await.unwrap();
            assert!(result);
            let state: String = sqlx::query_scalar("SELECT state FROM legal_destructions WHERE id='destruction-a'").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(state, "completed");
            assert_eq!(deletes.load(Ordering::SeqCst), 2);
        }).await;
    }


    struct Ids(u64);
    impl IdentifierSource for Ids {
        fn next(&mut self) -> RecordId {
            self.0 += 1;
            RecordId::new(format!("native-destruction-{}", self.0)).unwrap()
        }
    }

    #[tokio::test]
    async fn native_execute_with_sources_requires_independent_reviewer_and_same_unit() {
        use serde_json::json;
        use sha2::{Digest, Sha256};
        fixture::with_database(2, |pool| async move {
            setup(&pool).await;
            let image: String = sqlx::query_scalar("SELECT jsonb_build_object('request',x.id,'item',i.id,'company',i.company_id,'branch',i.branch_id,'department',i.department_id,'kind',i.kind,'classification',i.classification,'state',i.state,'version',i.version,'requester',x.requested_by,'approver',x.approved_by,'manifest',x.manifest,'policyId',x.policy_id,'policyVersion',x.policy_version,'policy',to_jsonb(p))::text FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id JOIN legal_retention_policies p ON p.id=x.policy_id AND p.company_id=i.company_id WHERE x.id='destruction-a'")
                .fetch_one(pool.raw()).await.unwrap();
            let hash = format!("{:x}", Sha256::digest(image.as_bytes()));
            // A queued reviewer who is the original requester is refused by the
            // native iam_same_person parity gate before any owner phase starts.
            let same = json!({"holdingId":"holding-a","companyId":"company-a","userId":"requester","assignmentId":"assignment-a","role":"manager","scope":{"kind":"holding"}}).to_string();
            sqlx::query("INSERT INTO legal_retention_execution_authorizations(command_id,destruction_id,item_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,item_version,created_at) VALUES('same-person','destruction-a','item-a','company-a','approve','requester','assignment-a',$1,$2,1,10)")
                .bind(same).bind(&hash).execute(pool.raw()).await.unwrap();
            let mut control = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *control).await.unwrap();
            let denied = match LegalRetentionUnit::read(&mut control, "company-a", "destruction-a").await {
                Ok(_) => panic!("a reviewer who is the requester must be refused"),
                Err(error) => error,
            };
            assert!(matches!(denied, PgTransactionError::Business(ref error) if error.code == "LEGAL_INDEPENDENT_REVIEW"));
            control.rollback().await.unwrap();

            // A later, distinct queued reviewer is the current authority. The
            // native two-phase path journals, then deletes only after the unit
            // revalidates both authority and independent review at the byte gate.
            let reviewer = json!({"holdingId":"holding-a","companyId":"company-a","userId":"reviewer","assignmentId":"assignment-a","role":"manager","scope":{"kind":"holding"}}).to_string();
            sqlx::query("INSERT INTO legal_retention_execution_authorizations(command_id,destruction_id,item_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,item_version,created_at) VALUES('independent','destruction-a','item-a','company-a','approve','reviewer','assignment-a',$1,$2,1,20)")
                .bind(reviewer).bind(&hash).execute(pool.raw()).await.unwrap();
            let deletes = Arc::new(AtomicUsize::new(0));
            let bridge = Bridge { deletes: deletes.clone(), fail_seal: Arc::new(AtomicBool::new(false)) };
            let outcome = execute_with_sources(&pool, bridge, "company-a", "destruction-a", 10, &mut Ids(0)).await.unwrap();
            assert_eq!(outcome, LegalDestructionExecution::Completed);
            assert_eq!(deletes.load(Ordering::SeqCst), 1);
            let state: String = sqlx::query_scalar("SELECT state FROM legal_destructions WHERE id='destruction-a'").fetch_one(pool.raw()).await.unwrap();
            assert_eq!(state, "completed");
        }).await;
    }
}
