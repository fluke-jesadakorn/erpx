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
