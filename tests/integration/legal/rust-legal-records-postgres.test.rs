//! Native Legal records, document versions, holds and archive over leased
//! PostgreSQL. Every command owns one serializable Legal transaction.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_legal::public::legal_records::*;
use erpx_server_runtime::public::http_support::{HeaderMap, StatusCode, body::to_bytes, header};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, UserId,
};
use sqlx::{AssertSqlSafe, PgConnection};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

struct Identity(Mutex<BTreeMap<String, String>>);

impl Default for Identity {
    fn default() -> Self {
        Self(Mutex::new(BTreeMap::new()))
    }
}

impl LegalIdentity for Identity {
    fn same_person<'a>(
        &'a self,
        _: &'a mut PgConnection,
        left: &'a str,
        right: &'a str,
        _: &'a CompanyId,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<bool, LegalRecordsError>> + Send + 'a>,
    > {
        let aliases = self.0.lock().unwrap();
        let canonical = |value: &str| {
            aliases
                .get(value)
                .cloned()
                .unwrap_or_else(|| value.to_owned())
        };
        let same = canonical(left) == canonical(right);
        Box::pin(async move { Ok(same) })
    }
}

struct Notifier {
    fail: bool,
}

impl LegalHoldNotifier for Notifier {
    fn notify<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        notices: &'a [LegalHoldNotice],
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), LegalRecordsError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if self.fail {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "SYNTHETIC_NOTIFIER_FAILURE",
                    "Synthetic notifier failure.",
                )
                .into());
            }
            for notice in notices {
                sqlx::query(
                    "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
                     VALUES($1,'company',$2,'legal',$3,$4,'A preservation notice requires acknowledgement',$5) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(format!(
                    "hold-{}-{}-{}",
                    notice.hold_id, notice.item_id, notice.user_id
                ))
                .bind(&notice.user_id)
                .bind(&notice.item_id)
                .bind(notice.notified_at)
                .bind(notice.notified_at)
                .execute(&mut *connection)
                .await?;
            }
            Ok(())
        })
    }
}

struct HttpBackend {
    pool: DatabasePool,
    actor: LegalRecordsActor,
    identity: Arc<Identity>,
    notifier: Arc<Notifier>,
}

fn rendered(actor: &LegalRecordsActor) -> LegalRenderedContext {
    LegalRenderedContext {
        user_id: actor.user_id.as_str().to_owned(),
        holding_id: actor.holding_id.as_str().to_owned(),
        company_id: actor.company_id.as_str().to_owned(),
        branch_id: actor
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        department_id: actor
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        assignment_id: actor.assignment_id.as_str().to_owned(),
        assignment_version: 1,
        revision: 1,
        role: actor.role.clone(),
        modules: vec!["legal".into()],
        permissions: actor.permissions.iter().cloned().collect(),
        expires_at: 9_007_199_254_740_991,
    }
}

impl LegalHttpBackend for HttpBackend {
    fn read<'a>(
        &'a self,
        request: LegalHttpReadRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalHttpReadReply> {
        Box::pin(async move {
            let context = rendered(&self.actor);
            match request.read {
                LegalHttpRead::Register(query) => {
                    Ok(LegalHttpReadReply::Register(Box::new(LegalRegisterReply {
                        data: read_legal_register(&self.pool, &self.actor, query)
                            .await
                            .map_err(legal_records_business_error)?,
                        context,
                    })))
                }
                LegalHttpRead::Detail(id) => {
                    Ok(LegalHttpReadReply::Detail(Box::new(LegalDetailReply {
                        data: read_legal_detail(&self.pool, &self.actor, &id)
                            .await
                            .map_err(legal_records_business_error)?,
                        context,
                    })))
                }
                LegalHttpRead::Export { id, format } => {
                    Ok(LegalHttpReadReply::Export(Box::new(LegalExportReply {
                        data: read_legal_export(&self.pool, &self.actor, &id, format.as_deref())
                            .await
                            .map_err(legal_records_business_error)?,
                        context,
                    })))
                }
            }
        })
    }

    fn command<'a>(
        &'a self,
        request: LegalHttpCommandRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalCommandReply> {
        Box::pin(async move {
            let key = CommandKey::new(request.key)?;
            Ok(LegalCommandReply {
                data: execute_legal_command(
                    &self.pool,
                    &self.actor,
                    &key,
                    request.request_hash,
                    request.body,
                    self.identity.clone(),
                    self.notifier.clone(),
                )
                .await
                .map_err(legal_records_business_error)?,
                context: rendered(&self.actor),
            })
        })
    }
}

fn actor(role: &str, permissions: &[&str]) -> LegalRecordsActor {
    LegalRecordsActor {
        holding_id: HoldingId::new("holding").unwrap(),
        holding_slug: "acme".into(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("editor-user").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
        branch_id: None,
        department_id: None,
        role: role.into(),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        actor_json: "{}".into(),
    }
}

fn requester() -> LegalRecordsActor {
    LegalRecordsActor {
        user_id: UserId::new("requester-user").unwrap(),
        role: "legal_requester".into(),
        permissions: ["legal.read", "legal.edit"].map(str::to_owned).into(),
        ..actor("legal_requester", &["legal.read", "legal.edit"])
    }
}

fn reviewer() -> LegalRecordsActor {
    LegalRecordsActor {
        user_id: UserId::new("reviewer-user").unwrap(),
        ..actor(
            "legal_manager",
            &[
                "legal.read",
                "legal.edit",
                "legal.hold",
                "legal.hold.release",
            ],
        )
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
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status)VALUES           ('company','company','acme','Synthetic Company',NULL,'company','active'),           ('foreign','company','foreign','Foreign Company',NULL,'foreign','active');         INSERT INTO legal_settings(company_id,enabled)VALUES('company',true),('foreign',true);\
         INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,state,version,created_at,updated_at)VALUES\
           ('contract-1','company',NULL,NULL,'contract','Service agreement','Current service agreement','internal','TH','Thailand','th','THB','Asia/Bangkok','normal',NULL,'editor-user','editor-user','draft',1,0,0),\
           ('contract-expired','company',NULL,NULL,'contract','Expired agreement','Expired service agreement','internal','TH','Thailand','th','THB','Asia/Bangkok','normal',NULL,'editor-user','editor-user','expired',1,0,0),\
           ('contract-approved','company',NULL,NULL,'contract','Approved agreement','Approved service agreement','internal','TH','Thailand','th','THB','Asia/Bangkok','normal',NULL,'editor-user','editor-user','approved',1,0,0),\
           ('request-1','company',NULL,NULL,'request','Requester question','Confidential question','confidential','TH','Thailand','th','THB','Asia/Bangkok','normal',NULL,'requester-user','requester-user','draft',1,0,0),\
           ('secret-contract','company',NULL,NULL,'contract','Secret agreement','Related confidential agreement','confidential','TH','Thailand','th','THB','Asia/Bangkok','normal',NULL,'editor-user','editor-user','draft',1,0,0),\
           ('foreign-contract','foreign',NULL,NULL,'contract','Foreign agreement','Other Company','internal','TH','Thailand','th','THB','Asia/Bangkok','normal',NULL,'editor-user','editor-user','draft',1,0,0);\
         INSERT INTO legal_contracts(item_id,data)VALUES\
           ('contract-1','{\"contractType\":\"service\"}'),\
           ('contract-expired','{\"contractType\":\"service\"}'),\
           ('contract-approved','{\"contractType\":\"service\"}'),\
           ('secret-contract','{\"contractType\":\"service\"}'),\
           ('foreign-contract','{\"contractType\":\"service\"}');\
         INSERT INTO legal_requests(item_id,data)VALUES('request-1','{\"requestType\":\"advice\"}');\
         INSERT INTO legal_members(item_id,user_id,access)VALUES\
           ('contract-1','editor-user','edit'),\
           ('contract-expired','editor-user','edit'),\
           ('contract-approved','editor-user','edit'),\
           ('request-1','requester-user','edit'),\
           ('secret-contract','secret-user','edit');\
         INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,locked,created_by,created_at)VALUES\
           ('contract-1-v1','contract-1',1,'Agreement','text/plain',repeat('a',64),11,'First text',false,'editor-user',0);\
         INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at)VALUES\
           ('secret-hold','secret-contract','Secret related preservation','active','secret-user',0);\
         INSERT INTO legal_hold_targets(hold_id,item_id)VALUES('secret-hold','contract-1');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn hash(value: &serde_json::Value) -> String {
    sha256_hex(serde_json::to_string(value).unwrap().as_bytes())
}

async fn command(
    pool: &DatabasePool,
    actor: &LegalRecordsActor,
    key: &CommandKey,
    raw: &serde_json::Value,
    notifier: Arc<Notifier>,
    identity: Arc<Identity>,
) -> Result<serde_json::Value, LegalRecordsError> {
    execute_legal_command(pool, actor, key, &hash(raw), raw, identity, notifier).await
}

fn code(error: LegalRecordsError) -> Option<&'static str> {
    match error {
        PgTransactionError::Business(error) => Some(error.code),
        PgTransactionError::Persistence(_) => None,
    }
}

async fn count(pool: &DatabasePool, table: &str) -> i64 {
    // Test-only table names are fixed literals from the assertions below.
    sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn legal_contract_approval_route_is_native_and_advances_the_contract() {
    fixture::with_database(6, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) \
             VALUES('approval-reviewer','reviewer-user','legal_manager','[\"legal\"]','company',NULL,0,9007199254740991,NULL,1,'holding',NULL,NULL,'legacy',NULL);\
             INSERT INTO legal_members(item_id,user_id,access) VALUES('contract-1','reviewer-user','edit');",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let identity = Arc::new(Identity::default());
        let notifier = Arc::new(Notifier { fail: false });
        let opener = actor("legal_counsel", &["legal.read", "legal.edit", "legal.review"]);
        let open_raw = serde_json::json!({
            "action":"approval.open",
            "id":"contract-1",
            "expectedVersion":1,
            "documentId":"contract-1-v1",
            "signerIds":["reviewer-user"],
            "approverIds":["reviewer-user"],
            "reason":"Open an independent approval route."
        });
        let opened = command(
            &pool,
            &opener,
            &key("legal-approval-open-0001"),
            &open_raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(opened["version"], 2);
        assert_eq!(opened["state"], "approval");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM approval_instances WHERE record_type='legal' AND record_id='contract-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "pending"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT approver_user_id FROM approval_steps WHERE instance_id LIKE 'approval-legal-%'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "reviewer-user"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT document_id FROM legal_approval_routes WHERE item_id='contract-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "contract-1-v1"
        );
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT locked FROM legal_documents WHERE id='contract-1-v1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap()
        );
        let reviewer = LegalRecordsActor {
            user_id: UserId::new("reviewer-user").unwrap(),
            ..actor("legal_manager", &["legal.read", "legal.approve"])
        };
        let decide_raw = serde_json::json!({
            "action":"approval.decide",
            "id":"contract-1",
            "expectedVersion":2,
            "decision":"approve",
            "reason":"Current reviewer approves this locked version."
        });
        let decided = command(
            &pool,
            &reviewer,
            &key("legal-approval-decide-0001"),
            &decide_raw,
            notifier,
            identity,
        )
        .await
        .unwrap();
        assert_eq!(decided["version"], 3);
        assert_eq!(decided["state"], "approved");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM approval_instances WHERE record_type='legal' AND record_id='contract-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "approved"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT decision FROM legal_approvals WHERE item_id='contract-1' AND cycle=1",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "approved"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM command_receipts WHERE record_id='contract-1' AND operation IN ('legal.approval.open','legal.approval.decide')"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
    })
    .await;
}

#[tokio::test]
async fn legal_export_records_access_without_advancing_the_record() {
    fixture::with_database(7, |pool| async move {
        setup(&pool).await;
        let exporter = actor("legal_counsel", &["legal.read", "legal.export"]);
        let original = read_legal_export(&pool, &exporter, "contract-1-v1", None)
            .await
            .unwrap();
        assert_eq!(original.item_id, "contract-1");
        assert_eq!(original.body, "First text");
        assert_eq!(original.object_key, None);
        assert!(!original.requires_provider);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_access_events WHERE document_id='contract-1-v1' AND action='document.export.original'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        let provider = read_legal_export(&pool, &exporter, "contract-1-v1", Some("pdf"))
            .await
            .unwrap();
        assert!(provider.requires_provider);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_access_events WHERE document_id='contract-1-v1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT version::bigint FROM legal_items WHERE id='contract-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            code(
                read_legal_export(
                    &pool,
                    &actor("legal_counsel", &["legal.read"]),
                    "contract-1-v1",
                    None
                )
                .await
                .unwrap_err()
            ),
            Some("PERMISSION_REQUIRED")
        );
        assert_eq!(
            code(
                read_legal_export(&pool, &exporter, "contract-1-v1", Some("xlsx"))
                    .await
                    .unwrap_err()
            ),
            Some("LEGAL_EXPORT_FORMAT")
        );
    })
    .await;
}

#[tokio::test]
async fn legal_executed_copy_review_is_native_and_replayable() {
    fixture::with_database(5, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "UPDATE legal_items SET state='approved' WHERE id='contract-1';\
             UPDATE legal_documents SET locked=true WHERE id='contract-1-v1';\
             INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,locked,created_by,created_at) \
             VALUES('contract-1-v2','contract-1',2,'Signed agreement','application/pdf',repeat('b',64),11,'Signed text',false,'importer-user',0);\
             INSERT INTO legal_approvals(item_id,cycle,document_id,document_hash,signers,reviewer_id,decision,created_at) \
             VALUES('contract-1',1,'contract-1-v1',repeat('a',64),'[]'::jsonb,'approver-user','approved',0);\
             INSERT INTO legal_members(item_id,user_id,access) VALUES('contract-1','reviewer-user','edit');\
             INSERT INTO legal_executed_copies(id,item_id,document_id,approved_cycle,approved_document_id,approved_hash,executed_hash,metadata,state,imported_by,imported_at,version) \
             VALUES('copy-1','contract-1','contract-1-v2',1,'contract-1-v1',repeat('a',64),repeat('b',64),'{\"approvedDocumentId\":\"contract-1-v1\",\"signedDate\":\"2026-11-08\",\"parties\":[\"Acme\",\"Customer\"],\"signers\":[{\"name\":\"A\",\"party\":\"Acme\",\"authorityDocumentIds\":[\"contract-1-v1\"]}],\"declaredPageCount\":1,\"notes\":\"\"}'::jsonb,'pending_review','importer-user',0,1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let identity = Arc::new(Identity::default());
        let notifier = Arc::new(Notifier { fail: false });
        let reviewer = LegalRecordsActor {
            user_id: UserId::new("reviewer-user").unwrap(),
            ..actor("legal_manager", &["legal.read", "legal.approve"])
        };
        let checks = serde_json::json!({
            "contentMatches":true,
            "pagesComplete":true,
            "partiesVerified":true,
            "signaturesComplete":true,
            "authorityVerified":true
        });
        let raw = serde_json::json!({
            "action":"executed.review",
            "id":"contract-1",
            "expectedVersion":1,
            "executedCopyId":"copy-1",
            "decision":"confirm",
            "checks":checks,
            "reason":"All pages and signatures match the approved draft."
        });
        let result = command(
            &pool,
            &reviewer,
            &key("legal-executed-review-0001"),
            &raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(result["version"], 2);
        assert_eq!(result["state"], "executed");
        assert_eq!(result["executedCopyId"], "copy-1");
        assert_eq!(result["executedCopyState"], "confirmed");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM legal_executed_copies WHERE id='copy-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "confirmed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT decision FROM legal_executed_reviews WHERE executed_copy_id='copy-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "confirm"
        );
        let replay = command(
            &pool,
            &reviewer,
            &key("legal-executed-review-0001"),
            &raw,
            notifier,
            identity,
        )
        .await
        .unwrap();
        assert_eq!(replay, result);
        assert_eq!(count(&pool, "legal_executed_reviews").await, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM command_receipts WHERE record_id='contract-1' AND operation='legal.executed.review'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn legal_access_share_and_membership_are_native_and_replayable() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) \
             VALUES('access-reviewer','reviewer-user','legal_manager','[\"legal\"]','company',NULL,0,9007199254740991,NULL,1,'holding',NULL,NULL,'legacy',NULL)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let identity = Arc::new(Identity::default());
        let notifier = Arc::new(Notifier { fail: false });
        let editor = actor(
            "legal_counsel",
            &["legal.read", "legal.edit", "legal.share"],
        );
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            + 86_400_000;
        let share_raw = serde_json::json!({
            "action":"share",
            "id":"contract-1",
            "expectedVersion":1,
            "documentId":"contract-1-v1",
            "email":"Person@Example.COM",
            "expiresAt":expires_at
        });
        let share = command(
            &pool,
            &editor,
            &key("legal-share-command-0001"),
            &share_raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(share["version"], 2);
        assert_eq!(share["state"], "draft");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT email FROM legal_shares WHERE item_id='contract-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "person@example.com"
        );
        assert_eq!(count(&pool, "legal_invitation_jobs").await, 1);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT holding_slug FROM legal_invitation_jobs LIMIT 1",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "acme"
        );
        let replay = command(
            &pool,
            &editor,
            &key("legal-share-command-0001"),
            &share_raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(replay, share);
        assert_eq!(count(&pool, "legal_shares").await, 1);
        assert_eq!(count(&pool, "legal_invitation_jobs").await, 1);

        let member_raw = serde_json::json!({
            "action":"member",
            "id":"contract-1",
            "expectedVersion":2,
            "userId":"reviewer-user",
            "access":"read"
        });
        let member = command(
            &pool,
            &editor,
            &key("legal-member-command-0001"),
            &member_raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(member["version"], 3);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT access FROM legal_members WHERE item_id='contract-1' AND user_id='reviewer-user'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "read"
        );
        let outsider_raw = serde_json::json!({
            "action":"member",
            "id":"contract-1",
            "expectedVersion":3,
            "userId":"outsider",
            "access":"read"
        });
        assert_eq!(
            code(
                command(
                    &pool,
                    &editor,
                    &key("legal-member-command-0002"),
                    &outsider_raw,
                    notifier.clone(),
                    identity.clone(),
                )
                .await
                .unwrap_err()
            ),
            Some("LEGAL_PERSON_UNAVAILABLE")
        );
        let owner_remove = serde_json::json!({
            "action":"member",
            "id":"contract-1",
            "expectedVersion":3,
            "userId":"editor-user",
            "access":"remove"
        });
        assert_eq!(
            code(
                command(
                    &pool,
                    &editor,
                    &key("legal-member-command-0003"),
                    &owner_remove,
                    notifier.clone(),
                    identity.clone(),
                )
                .await
                .unwrap_err()
            ),
            Some("LEGAL_OWNER_RETAINED")
        );

        let share_id: String =
            sqlx::query_scalar("SELECT id FROM legal_shares WHERE item_id='contract-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        let revoke_raw = serde_json::json!({
            "action":"share.revoke",
            "id":"contract-1",
            "expectedVersion":3,
            "shareId":share_id
        });
        let revoke = command(
            &pool,
            &editor,
            &key("legal-share-command-0002"),
            &revoke_raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(revoke["version"], 4);
        assert!(
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT revoked_at FROM legal_shares WHERE item_id='contract-1'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap()
            .is_some()
        );
        let replay = command(
            &pool,
            &editor,
            &key("legal-share-command-0002"),
            &revoke_raw,
            notifier,
            identity,
        )
        .await
        .unwrap();
        assert_eq!(replay, revoke);
        assert_eq!(count(&pool, "legal_shares").await, 1);
        assert_eq!(count(&pool, "legal_invitation_jobs").await, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM command_receipts WHERE record_id='contract-1' AND operation LIKE 'legal.%'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            3
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_events WHERE item_id='contract-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            3
        );
    })
    .await;
}

#[tokio::test]
async fn legal_records_register_detail_document_and_hold_lifecycle() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let identity = Arc::new(Identity::default());
        let notifier = Arc::new(Notifier { fail: false });
        let editor = actor("legal_counsel", &["legal.read", "legal.edit", "legal.hold", "legal.hold.release"]);

        let register = read_legal_register(
            &pool,
            &editor,
            LegalRegisterQuery::parse(Some("contract"), "", "", 1, false).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(register.total, 3);
        assert!(
            register
                .records
                .iter()
                .all(|record| record.company_id == "company" && record.kind == "contract")
        );
        assert!(register.records.iter().all(|record| record.id != "foreign-contract"));

        let requester_register = read_legal_register(
            &pool,
            &requester(),
            LegalRegisterQuery::parse(None, "", "", 1, false).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(requester_register.total, 1);
        assert_eq!(requester_register.records[0].id, "request-1");

        let detail = read_legal_detail(&pool, &editor, "contract-1")
            .await
            .unwrap();
        assert_eq!(detail.item.data["contractType"], "service");
        assert_eq!(detail.documents.len(), 1);
        assert_eq!(detail.documents[0].checksum, "a".repeat(64));
        assert!(!detail.holds[0].direct);
        assert_eq!(detail.holds[0].reason, "Protected by a related legal hold");
        assert_eq!(detail.access.len(), 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_access_events WHERE item_id='contract-1' AND actor_id='editor-user' AND action='record.read'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            code(read_legal_detail(&pool, &editor, "foreign-contract").await.unwrap_err()),
            Some("LEGAL_NOT_FOUND")
        );

        let raw = serde_json::json!({"action":"document","id":"contract-1","expectedVersion":1,"name":"Second agreement","body":"Second text"});
        let result = command(
            &pool,
            &editor,
            &key("legal-document-command-0001"),
            &raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(result["version"], 2);
        assert_eq!(result["state"], "draft");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT version::bigint FROM legal_items WHERE id='contract-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            2
        );
        let stored: (i64, String, i64) = sqlx::query_as(
            "SELECT version::bigint,checksum,bytes FROM legal_documents WHERE item_id='contract-1' AND version=2",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored.0, 2);
        assert_eq!(stored.1, sha256_hex(b"Second text"));
        assert_eq!(stored.2, 11);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT action FROM legal_events WHERE item_id='contract-1' ORDER BY version DESC LIMIT 1"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "document"
        );
        assert_eq!(count(&pool, "command_receipts").await, 1);
        assert_eq!(count(&pool, "audit_events").await, 1);
        assert_eq!(count(&pool, "outbox_events").await, 1);

        let replay = command(
            &pool,
            &editor,
            &key("legal-document-command-0001"),
            &raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        assert_eq!(replay, result);
        assert_eq!(count(&pool, "legal_documents").await, 2);
        assert_eq!(
            code(command(
                &pool,
                &editor,
                &key("legal-document-command-0001"),
                &serde_json::json!({"action":"document","id":"contract-1","expectedVersion":1,"name":"Changed","body":"Changed text"}),
                notifier.clone(),
                identity.clone(),
            )
            .await
            .unwrap_err()),
            Some("IDEMPOTENCY_CONFLICT")
        );
        assert_eq!(
            code(command(
                &pool,
                &editor,
                &key("legal-document-command-0002"),
                &serde_json::json!({"action":"document","id":"contract-1","expectedVersion":1,"name":"Stale","body":"Stale text"}),
                notifier.clone(),
                identity.clone(),
            )
            .await
            .unwrap_err()),
            Some("LEGAL_VERSION_CONFLICT")
        );

        let not_editor = LegalRecordsActor {
            user_id: UserId::new("reviewer-user").unwrap(),
            ..editor.clone()
        };
        assert_eq!(
            code(command(
                &pool,
                &not_editor,
                &key("legal-document-command-0003"),
                &serde_json::json!({"action":"document","id":"contract-1","expectedVersion":2,"name":"Denied","body":"Denied text"}),
                notifier.clone(),
                identity.clone(),
            )
            .await
            .unwrap_err()),
            Some("LEGAL_EDITOR_REQUIRED")
        );

        let hold_raw = serde_json::json!({"action":"hold.create","id":"contract-approved","expectedVersion":1,"reason":"Preserve the executed agreement"});
        let hold = command(
            &pool,
            &editor,
            &key("legal-hold-create-0001"),
            &hold_raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        let hold_id = hold["holdId"].as_str().unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM legal_holds WHERE id=$1")
                .bind(hold_id)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "active"
        );
        assert_eq!(count(&pool, "legal_hold_targets").await, 3);
        assert!(count(&pool, "legal_hold_notices").await >= 1);
        assert!(count(&pool, "staff_notifications").await >= 1);

        let request_raw = serde_json::json!({"action":"hold.request-release","id":"contract-approved","expectedVersion":2,"holdId":hold_id,"reason":"Release after review"});
        command(
            &pool,
            &editor,
            &key("legal-hold-release-request-0001"),
            &request_raw,
            notifier.clone(),
            identity.clone(),
        )
        .await
        .unwrap();
        let release_raw = serde_json::json!({"action":"hold.release","id":"contract-approved","expectedVersion":3,"holdId":hold_id,"reason":"Independent release review"});
        assert_eq!(
            code(command(
                &pool,
                &editor,
                &key("legal-hold-release-0001"),
                &release_raw,
                notifier.clone(),
                identity.clone(),
            )
            .await
            .unwrap_err()),
            Some("LEGAL_INDEPENDENT_REVIEW")
        );
        command(
            &pool,
            &reviewer(),
            &key("legal-hold-release-0002"),
            &release_raw,
            notifier,
            identity,
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM legal_holds WHERE id=$1")
                .bind(hold_id)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "released"
        );

        assert_eq!(
            code(command(
                &pool,
                &editor,
                &key("legal-archive-approved-0001"),
                &serde_json::json!({"action":"transition","id":"contract-approved","expectedVersion":4,"state":"archived","reason":"Archive after review"}),
                Arc::new(Notifier { fail: false }),
                Arc::new(Identity::default()),
            )
            .await
            .unwrap_err()),
            Some("LEGAL_TRANSITION")
        );
        command(
            &pool,
            &editor,
            &key("legal-archive-expired-0001"),
            &serde_json::json!({"action":"transition","id":"contract-expired","expectedVersion":1,"state":"archived","reason":"Archive expired agreement"}),
            Arc::new(Notifier { fail: false }),
            Arc::new(Identity::default()),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM legal_items WHERE id='contract-expired'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "archived"
        );

        sqlx::query(
            "INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at) \
             VALUES('destruction-1','contract-1','policy',1,'editor-user','executing','Synthetic freeze','[]',0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            code(command(
                &pool,
                &editor,
                &key("legal-document-command-0004"),
                &serde_json::json!({"action":"document","id":"contract-1","expectedVersion":2,"name":"Frozen","body":"Frozen text"}),
                Arc::new(Notifier { fail: false }),
                Arc::new(Identity::default()),
            )
            .await
            .unwrap_err()),
            Some("LEGAL_CONTENT_FROZEN")
        );
    })
    .await;
}

#[tokio::test]
async fn a_failed_hold_notifier_rolls_back_the_owner_command() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let editor = actor("legal_counsel", &["legal.read", "legal.edit", "legal.hold"]);
        let result = command(
            &pool,
            &editor,
            &key("legal-hold-create-failure-0001"),
            &serde_json::json!({"action":"hold.create","id":"contract-1","expectedVersion":1,"reason":"Preserve until review"}),
            Arc::new(Notifier { fail: true }),
            Arc::new(Identity::default()),
        )
        .await;
        assert_eq!(code(result.unwrap_err()), Some("SYNTHETIC_NOTIFIER_FAILURE"));
        assert_eq!(count(&pool, "legal_holds").await, 1);
        assert_eq!(count(&pool, "legal_hold_targets").await, 1);
        assert_eq!(count(&pool, "legal_hold_notices").await, 0);
        assert_eq!(count(&pool, "command_receipts").await, 0);
        assert_eq!(count(&pool, "audit_events").await, 0);
        assert_eq!(count(&pool, "outbox_events").await, 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT version::bigint FROM legal_items WHERE id='contract-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn native_legal_http_reads_and_commands_through_postgres() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let service = LegalHttpService::new(vec![LegalHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(HttpBackend {
                pool: pool.clone(),
                actor: actor(
                    "legal_counsel",
                    &["legal.read", "legal.edit", "legal.export", "legal.hold", "legal.hold.release"],
                ),
                identity: Arc::new(Identity::default()),
                notifier: Arc::new(Notifier { fail: false }),
            }),
        }])
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "acme.example.test".parse().unwrap());
        let response = service
            .register(
                "acme.example.test",
                "acme",
                &headers,
                LegalRegisterQuery::parse(Some("contract"), "", "", 1, false).unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["context"]["companyId"], "company");
        assert_eq!(body["context"]["assignmentId"], "assignment");

        let response = service
            .export("acme.example.test", "acme", &headers, "contract-1-v1", None)
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["id"], "contract-1-v1");
        assert_eq!(body["data"]["body"], "First text");
        assert_eq!(body["data"]["requires_provider"], false);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM legal_access_events WHERE document_id='contract-1-v1' AND action='document.export.original'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );

        let mut command_headers = headers.clone();
        command_headers.insert(
            header::ORIGIN,
            "https://acme.example.test".parse().unwrap(),
        );
        command_headers.insert(
            header::HeaderName::from_static("idempotency-key"),
            "legal-http-command-0001".parse().unwrap(),
        );
        let raw = serde_json::json!({"action":"document","id":"contract-1","expectedVersion":1,"name":"HTTP agreement","body":"HTTP text"});
        let response = service
            .command("acme.example.test", "acme", &command_headers, serde_json::to_vec(&raw).unwrap().as_slice())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["data"]["version"], 2);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT name FROM legal_documents WHERE item_id='contract-1' AND version=2"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "HTTP agreement"
        );

        let mut foreign = command_headers.clone();
        foreign.insert(header::ORIGIN, "https://other.example.test".parse().unwrap());
        let response = service
            .command("acme.example.test", "acme", &foreign, serde_json::to_vec(&raw).unwrap().as_slice())
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let mut missing = headers.clone();
        missing.insert(header::HOST, "other.example.test".parse().unwrap());
        let response = service
            .register(
                "other.example.test",
                "acme",
                &missing,
                LegalRegisterQuery::parse(None, "", "", 1, false).unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    })
    .await;
}
