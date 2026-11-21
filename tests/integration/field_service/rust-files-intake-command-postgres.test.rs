//! Disposable real Account, Field, Files and Workflow intake composition.
#[allow(dead_code, reason = "Shared real Account and Field Service fixture")]
#[expect(
    unused_imports,
    reason = "Shared fixture reexports helpers for other targets"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use erpx_server_files::public::field_visit_intake::{FieldIntakeAvailability, FieldSetRequest};
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId};
use erpx_service::field_attachment_intake_command::{
    FieldAttachmentIntakeCommand, FieldAttachmentIntakeCommandSources,
    FieldIntakeProviderReadiness, FieldIntakeProviderStatus, FieldSetCommandResponse,
    execute_field_attachment_intake, execute_field_attachment_intake_with_live_readiness,
};
use std::sync::Arc;

// A real command takes time between its receipt and owner effects. The
// outbox case must not depend on every clock read returning the same instant.
struct CommandClock<C> {
    source: Arc<C>,
    ticks: std::sync::atomic::AtomicI64,
    advancing: bool,
}
impl<C: erpx_server_shared::public::Clock> erpx_server_shared::public::Clock for CommandClock<C> {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        let offset = if self.advancing {
            self.ticks.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        } else { 0 };
        erpx_server_shared::public::TimestampMillis::new(self.source.now().get() + offset)
    }
}

async fn execute(
    p: &facts::Parts,
    key: &str,
    purpose: &str,
) -> Result<FieldSetCommandResponse, PgTransactionError<BusinessError>> {
    execute_as(
        p,
        key,
        purpose,
        p.review.maker.clone(),
        p.selection.clone(),
        "company",
    )
    .await
}
async fn execute_as(
    p: &facts::Parts,
    key: &str,
    purpose: &str,
    device: erpx_server_identity_access::public::DeviceProof,
    selection: erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,
    company_id: &str,
) -> Result<FieldSetCommandResponse, PgTransactionError<BusinessError>> {
    execute_with_provider(p, key, purpose, device, selection, company_id, None).await
}
async fn execute_with_provider(
    p: &facts::Parts,
    key: &str,
    purpose: &str,
    device: erpx_server_identity_access::public::DeviceProof,
    selection: erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,
    company_id: &str,
    provider: Option<Arc<dyn FieldIntakeProviderReadiness>>,
) -> Result<FieldSetCommandResponse, PgTransactionError<BusinessError>> {
    let company_id = company_id.to_owned();
    let key = key.to_owned();
    let purpose = purpose.to_owned();
    let clock = Arc::new(CommandClock {
        source: p.review.system.scenario.clock.clone(),
        ticks: std::sync::atomic::AtomicI64::new(0),
        advancing: key == "field-outbox-create",
    });
    let catalogue =
        Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap());
    p.review
        .authorizer
        .execute_with_typed_sources_deferred(
            "acme.example.test",
            device,
            move || Ok(selection),
            WorkAuthorizationRequest {
                permission: "records.read".into(),
                module: Some("field_service".into()),
                destination: "service/orders".into(),
                api: true,
            },
            FieldAttachmentIntakeCommandSources,
            move |c, actor, fence| {
                let key = key.clone();
                let purpose = purpose.clone();
                let clock = clock.clone();
                let catalogue = catalogue.clone();
                let company_id = company_id.clone();
                let provider = provider.clone();
                Box::pin(async move {
                    let holding = HoldingId::new("h001").unwrap();
                    let company = CompanyId::new(company_id).unwrap();
                    let input = FieldSetRequest {
                        target_type: "field_service_visit".into(),
                        target_id: "issue-order".into(),
                        component_kind: "record".into(),
                        component_ref: None,
                        purpose_key: purpose,
                        expected_target_version: if key == "provider-stale" { 2 } else { 3 },
                    };
                    let availability = FieldIntakeAvailability {
                        uploads_frozen: false,
                        scanner_ready: true,
                        preview_ready: true,
                    };
                    let command = FieldAttachmentIntakeCommand {
                        actor,
                        fence,
                        holding: &holding,
                        company: &company,
                        catalogue: &catalogue,
                        clock: clock.as_ref(),
                        input: &input,
                        key: &key,
                        availability: &availability,
                    };
                    if let Some(provider) = provider {
                        execute_field_attachment_intake_with_live_readiness(
                            c,
                            command,
                            provider.as_ref(),
                        )
                        .await
                    } else {
                        execute_field_attachment_intake(c, command).await
                    }
                })
            },
        )
        .await
}

struct BrokenProvider(std::sync::atomic::AtomicUsize);
impl FieldIntakeProviderReadiness for BrokenProvider {
    fn probe(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<FieldIntakeProviderStatus, BusinessError>>
                + Send
                + '_,
        >,
    > {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async {
            Ok(FieldIntakeProviderStatus {
                scanner_enabled: true,
                scanner_ready: false,
                preview_ready: true,
            })
        })
    }
}
#[tokio::test]
async fn readiness_is_deferred_past_recorded_replay_but_required_for_fresh_and_reuse() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let first = execute(&p, "provider-original", "site_photo").await.unwrap();
        let provider = Arc::new(BrokenProvider(std::sync::atomic::AtomicUsize::new(0)));
        let run = |key, purpose| execute_with_provider(&p, key, purpose, p.review.maker.clone(), p.selection.clone(), "company", Some(provider.clone()));
        let replay = run("provider-original", "site_photo").await.unwrap();
        assert!(replay.replayed); assert_eq!(replay.data, first.data);
        assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(matches!(run("provider-stale", "site_photo").await, Err(PgTransactionError::Business(e)) if e.code == "ATTACHMENT_TARGET_CHANGED"));
        assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        for (key, purpose) in [("provider-reuse", "site_photo"), ("provider-fresh", "technician_report")] {
            assert!(matches!(run(key, purpose).await, Err(PgTransactionError::Business(e)) if e.code == "ATTACHMENT_SCANNER_UNAVAILABLE"));
        }
        assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst), 2);
        let counts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.native-origin'),(SELECT count(*) FROM audit_events WHERE action='attachment.set.create'),(SELECT count(*) FROM outbox_events WHERE event_type='attachment.set.create')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(counts, (1,1,1,1));
    }).await;
}

#[tokio::test]
async fn invalid_wire_keys_leave_no_files_or_workflow_effects() {
    facts::account::with_system(|system| async move {
        let p = facts::setup_workspace(system).await;
        let keys = [String::new(), "short".into(), "a".repeat(181), "nonascii-é".into(), "forbidden/key".into()];
        for key in &keys {
            assert!(matches!(execute(&p, key, "site_photo").await, Err(PgTransactionError::Business(e)) if e.code == "COMMAND_KEY_REQUIRED"));
        }
        let counts: (i64, i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events),(SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events WHERE action='attachment.set.create'),(SELECT count(*) FROM outbox_events WHERE event_type='attachment.set.create')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(counts, (0, 0, 0, 0, 0));
        for key in ["a".repeat(8), "a".repeat(180)] {
            assert!(!execute(&p, &key, "site_photo").await.unwrap().replayed);
        }
    }).await;
}

#[tokio::test]
async fn native_create_replay_and_alias_each_record_exact_workflow_evidence() {
    facts::account::with_system(|system|async move {
        let p=facts::setup_workspace(system).await;
        let ledger=erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-a","native-create");
        let first=execute(&p,"native-create","site_photo").await.unwrap();assert!(!first.replayed);
        let replay=execute(&p,"native-create","site_photo").await.unwrap();assert!(replay.replayed);assert_eq!(first.data,replay.data);
        let alias=execute(&p,"native-alias","site_photo").await.unwrap();assert!(!alias.replayed);assert_eq!(first.data,alias.data);
        let alias_replay=execute(&p,"native-alias","site_photo").await.unwrap();assert!(alias_replay.replayed);assert_eq!(first.data,alias_replay.data);
        assert!(matches!(execute(&p,"native-create","technician_report").await,Err(PgTransactionError::Business(e)) if e.code=="IDEMPOTENCY_CONFLICT"));
        let counts:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.created'),(SELECT count(*) FROM attachment_events WHERE action='set.reused'),(SELECT count(*) FROM attachment_events WHERE action='set.native-origin'),(SELECT count(*) FROM audit_events WHERE action='attachment.set.create'),(SELECT count(*) FROM outbox_events WHERE event_type='attachment.set.create')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(counts,(1,1,1,2,2,2));
        let version:i64=sqlx::query_scalar("SELECT version FROM field_service_orders WHERE id='issue-order'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(version,3);
        let (saved_authority,saved_payload):(String,String)=sqlx::query_as("SELECT r.authority_json,o.payload_json FROM command_receipts r JOIN outbox_events o ON o.command_id=r.command_id WHERE r.command_id=$1").bind(&ledger).fetch_one(p.pool().raw()).await.unwrap();
        sqlx::query("UPDATE command_receipts SET authority_json=jsonb_set(authority_json::jsonb,'{iam}',jsonb_build_object('tampered',true),true)::text WHERE command_id=$1").bind(&ledger).execute(p.pool().raw()).await.unwrap();
        sqlx::query("UPDATE audit_events SET authority_json=(SELECT authority_json FROM command_receipts WHERE command_id=$1) WHERE command_id=$1").bind(&ledger).execute(p.pool().raw()).await.unwrap();
        sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET payload_json=jsonb_set(payload_json::jsonb,'{authority,iam}',jsonb_build_object('tampered',true),true)::text FROM runtime_owner WHERE command_id=$1").bind(&ledger).execute(p.pool().raw()).await.unwrap();
        assert!(matches!(execute(&p,"native-create","site_photo").await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_ATTACHMENT_CHANGED"));
        sqlx::query("UPDATE command_receipts SET authority_json=$1 WHERE command_id=$2").bind(&saved_authority).bind(&ledger).execute(p.pool().raw()).await.unwrap();
        sqlx::query("UPDATE audit_events SET authority_json=$1 WHERE command_id=$2").bind(&saved_authority).bind(&ledger).execute(p.pool().raw()).await.unwrap();
        sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET payload_json=$1 FROM runtime_owner WHERE command_id=$2").bind(&saved_payload).bind(&ledger).execute(p.pool().raw()).await.unwrap();
        let audit_id:String=sqlx::query_scalar("SELECT id FROM audit_events WHERE command_id=$1").bind(&ledger).fetch_one(p.pool().raw()).await.unwrap();
        sqlx::query("UPDATE audit_events SET id='tampered-audit-id' WHERE command_id=$1").bind(&ledger).execute(p.pool().raw()).await.unwrap();
        assert!(matches!(execute(&p,"native-create","site_photo").await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_ATTACHMENT_CHANGED"));
        sqlx::query("UPDATE audit_events SET id=$1 WHERE command_id=$2").bind(audit_id).bind(&ledger).execute(p.pool().raw()).await.unwrap();
        let outbox_id:String=sqlx::query_scalar("SELECT id FROM outbox_events WHERE command_id=$1").bind(&ledger).fetch_one(p.pool().raw()).await.unwrap();
        sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET id='tampered-outbox-id' FROM runtime_owner WHERE command_id=$1").bind(&ledger).execute(p.pool().raw()).await.unwrap();
        assert!(matches!(execute(&p,"native-create","site_photo").await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_ATTACHMENT_CHANGED"));
        sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET id=$1 FROM runtime_owner WHERE command_id=$2").bind(outbox_id).bind(&ledger).execute(p.pool().raw()).await.unwrap();
        sqlx::query("DELETE FROM command_receipts WHERE command_id=$1").bind(&ledger).execute(p.pool().raw()).await.unwrap();
        assert!(matches!(execute(&p,"native-create","site_photo").await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_ATTACHMENT_CHANGED"));
        let blocked=sqlx::query("DELETE FROM attachment_events WHERE action='set.native-origin'").execute(p.pool().raw()).await;
        assert_eq!(blocked.unwrap_err().as_database_error().and_then(|e| e.code()).as_deref(), Some("P0001"));
    }).await;
}

#[tokio::test]
async fn late_receipt_audit_outbox_origin_and_cleanup_corruption_rolls_back() {
    facts::account::with_system(|system|async move {
        let p=facts::setup_workspace(system).await;
        for (name, create, drop_sql) in [
            ("receipt", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.result_json := '{}'; RETURN NEW; END $$; CREATE TRIGGER intake_corruption BEFORE INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON command_receipts; DROP FUNCTION intake_corruption()"),
            ("audit", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.record_version := NEW.record_version + 1; RETURN NEW; END $$; CREATE TRIGGER intake_corruption BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON audit_events; DROP FUNCTION intake_corruption()"),
            ("outbox", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.aggregate_version := NEW.aggregate_version + 1; RETURN NEW; END $$; CREATE TRIGGER intake_corruption BEFORE INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON outbox_events; DROP FUNCTION intake_corruption()"),
            ("origin", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='set.native-origin' THEN RETURN NULL; END IF; RETURN NEW; END $$; CREATE TRIGGER intake_corruption BEFORE INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON attachment_events; DROP FUNCTION intake_corruption()"),
            ("malformed-origin", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='set.native-origin' THEN NEW.details_json := '{}'; END IF; RETURN NEW; END $$; CREATE TRIGGER intake_corruption BEFORE INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON attachment_events; DROP FUNCTION intake_corruption()"),
            ("field", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='set.native-origin' THEN UPDATE field_service_orders SET version=version+1 WHERE id='issue-order'; END IF; RETURN NEW; END $$; CREATE TRIGGER intake_corruption AFTER INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON attachment_events; DROP FUNCTION intake_corruption()"),
            ("authority", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.authority_json := '{}'; RETURN NEW; END $$; CREATE TRIGGER intake_corruption BEFORE INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON command_receipts; DROP FUNCTION intake_corruption()"),
            ("iam", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='set.native-origin' THEN UPDATE iam_members SET version=version+1 WHERE company_id='company' AND user_id='fixture-a'; END IF; RETURN NEW; END $$; CREATE TRIGGER intake_corruption AFTER INSERT ON attachment_events FOR EACH ROW EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON attachment_events; DROP FUNCTION intake_corruption()"),
            ("cleanup-receipt", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE command_receipts SET result_json='{}' WHERE command_id LIKE 'attachment.set.create:%'; RETURN NULL; END $$; CREATE TRIGGER intake_corruption AFTER DELETE ON command_assertions FOR EACH STATEMENT EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON command_assertions; DROP FUNCTION intake_corruption()"),
            ("cleanup-authority", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE command_receipts SET authority_json='{}' WHERE command_id LIKE 'attachment.set.create:%'; RETURN NULL; END $$; CREATE TRIGGER intake_corruption AFTER DELETE ON command_assertions FOR EACH STATEMENT EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON command_assertions; DROP FUNCTION intake_corruption()"),
            ("cleanup-audit", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE audit_events SET record_version=record_version+1 WHERE command_id LIKE 'attachment.set.create:%'; RETURN NULL; END $$; CREATE TRIGGER intake_corruption AFTER DELETE ON command_assertions FOR EACH STATEMENT EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON command_assertions; DROP FUNCTION intake_corruption()"),
            ("cleanup-outbox", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM set_config('erpx.outbox_runtime','native-rust',true); UPDATE outbox_events SET payload_json='{}' WHERE command_id LIKE 'attachment.set.create:%'; RETURN NULL; END $$; CREATE TRIGGER intake_corruption AFTER DELETE ON command_assertions FOR EACH STATEMENT EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON command_assertions; DROP FUNCTION intake_corruption()"),
            ("cleanup", "CREATE FUNCTION intake_corruption() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE attachment_sets SET classification='restricted' WHERE target_id='issue-order'; RETURN NULL; END $$; CREATE TRIGGER intake_corruption AFTER DELETE ON command_assertions FOR EACH STATEMENT EXECUTE FUNCTION intake_corruption()", "DROP TRIGGER intake_corruption ON command_assertions; DROP FUNCTION intake_corruption()"),
        ] {
            sqlx::raw_sql(create).execute(p.pool().raw()).await.unwrap();
            let key=format!("native-{name}-fail");let result=execute(&p,&key,"site_photo").await;
            match result { Err(PgTransactionError::Business(e)) if e.code == if name == "iam" { "WORK_AUTHORITY_CHANGED" } else { "SERVICE_ATTACHMENT_CHANGED" } => {}, Err(PgTransactionError::Business(e)) => panic!("{name} rejected as {}", e.code), Err(PgTransactionError::Persistence(e)) => panic!("{name} persistence {}", e.code), Ok(_) => panic!("{name} corruption was accepted") }
            let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-a",&key)).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(counts,(0,0,0,0),"{name}");
            sqlx::raw_sql(drop_sql).execute(p.pool().raw()).await.unwrap();
        }
        let control=execute(&p,"native-control","site_photo").await.unwrap();
        assert!(!control.replayed);
    }).await;
}

#[tokio::test]
async fn true_legacy_receipt_replays_without_synthetic_workflow_evidence() {
    use sha2::{Digest, Sha256};
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        let hash=hex::encode(Sha256::digest(b"company\0fixture-a\0legacy-create"));
        let id=format!("attachment-set-{}",&hash[..32]);
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES($1,'company','branch','field_service_visit','issue-order','record','site_photo',3,1,'internal','{}','fixture-a',1,1)").bind(&id).execute(p.pool().raw()).await.unwrap();
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES('legacy-event','company',$1,'fixture-a','set.created','{\"compatibilityAdapter\":false}',1)").bind(&id).execute(p.pool().raw()).await.unwrap();
        let provider=Arc::new(BrokenProvider(std::sync::atomic::AtomicUsize::new(0)));
        let value=execute_with_provider(&p,"legacy-create","site_photo",p.review.maker.clone(),p.selection.clone(),"company",Some(provider.clone())).await.unwrap();assert!(value.replayed);assert_eq!(value.data["id"],id);assert_eq!(provider.0.load(std::sync::atomic::Ordering::SeqCst),0);
        let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='legacy-create'),(SELECT count(*) FROM audit_events WHERE command_id='legacy-create'),(SELECT count(*) FROM outbox_events WHERE command_id='legacy-create'),(SELECT count(*) FROM attachment_events WHERE action='set.native-origin')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(counts,(0,0,0,0));
    }).await;
}

#[tokio::test]
async fn new_session_and_read_only_role_replay_but_other_users_get_their_own_keys() {
    use erpx_server_identity_access::public::work_context::ExpectedWorkSelection;
    use erpx_server_identity_access::public::{
        AuthenticationSecrets, DeviceProof, SecretPurpose, SystemAuthenticationSecrets,
    };
    use erpx_server_shared::public::{AssignmentId, Clock};
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('shared-wire-key','other.operation','fixture-a','unrelated','other','{}',1,'{}')").execute(p.pool().raw()).await.unwrap();
        let first=execute(&p,"shared-wire-key","site_photo").await.unwrap();
        let raw_key=erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-a","shared-wire-key");
        let now=p.review.system.scenario.clock.now().get();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','field_readonly','Synthetic read-only Service role',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','field_readonly',1,'[\"records.read\",\"module.field_service.read\"]','[]','[\"holding\"]','fixture',0)").execute(p.pool().raw()).await.unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('field-readonly-a','fixture-a','field_readonly','[\"field_service\"]','company',$1,$2,4,'holding')").bind(now-1000).bind(now+3600000).execute(p.pool().raw()).await.unwrap();
        let central=facts::account::login(&p.review.system,"a@example.test",None).await;
        let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-readonly-a',4,9,$2,$3)").bind(&child.result.session_id).bind(now+3600000).bind(now).execute(p.pool().raw()).await.unwrap();
        let new_device=DeviceProof{active:Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session,&child.token)),..Default::default()};
        let mut selected=p.selection.clone();selected.expected=Some(ExpectedWorkSelection{revision:9,assignment_id:AssignmentId::new("field-readonly-a").unwrap(),assignment_version:4});
        let replay=execute_as(&p,"shared-wire-key","site_photo",new_device.clone(),selected.clone(),"company").await.unwrap();assert!(replay.replayed);assert_eq!(replay.data,first.data);
        assert!(matches!(execute_as(&p,"new-readonly-key","site_photo",new_device.clone(),selected.clone(),"company").await,Err(PgTransactionError::Business(e)) if e.code=="PERMISSION_REQUIRED"));
        let unavailable=Arc::new(BrokenProvider(std::sync::atomic::AtomicUsize::new(0)));
        assert!(matches!(execute_with_provider(&p,"readonly-provider","site_photo",new_device.clone(),selected.clone(),"company",Some(unavailable.clone())).await,Err(PgTransactionError::Business(e)) if e.code=="PERMISSION_REQUIRED"));
        assert_eq!(unavailable.0.load(std::sync::atomic::Ordering::SeqCst),0);
        assert!(matches!(execute_as(&p,"shared-wire-key","site_photo",new_device,selected,"other-company").await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_ATTACHMENT_CHANGED"));
        facts::setup_agreement_review_facts(&p,now).await;
        sqlx::query("UPDATE active_work_contexts SET assignment_id='agreement-reviewer-b',assignment_version=3,revision=9 WHERE user_id='fixture-b'").execute(p.pool().raw()).await.unwrap();
        let mut reviewer=p.review.reviewer_selection.clone();reviewer.expected=Some(ExpectedWorkSelection{revision:9,assignment_id:AssignmentId::new("agreement-reviewer-b").unwrap(),assignment_version:3});
        let second=execute_as(&p,"shared-wire-key","site_photo",p.review.reviewer.clone(),reviewer,"company").await.unwrap();assert!(!second.replayed);assert_ne!(second.data["id"],first.data["id"]);
        let second_key=erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-b","shared-wire-key");assert_ne!(raw_key,second_key);
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id IN ($1,$2,'shared-wire-key')").bind(raw_key).bind(second_key).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,3);
    }).await;
}

#[tokio::test]
async fn native_field_intake_outbox_uses_files_identity_and_survives_corrupt_receipt(){
 facts::account::with_system(|system|async move{
  let p=facts::setup_workspace(system).await;
  let created=execute(&p,"field-outbox-create","technician_report").await.unwrap();
  let db=p.pool().raw();
  let ledger=erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-a","field-outbox-create");
  let times:(i64,i64)=sqlx::query_as("SELECT r.created_at,s.created_at FROM command_receipts r JOIN attachment_sets s ON s.id=r.result_json::jsonb->>'setId' WHERE r.command_id=$1").bind(&ledger).fetch_one(db).await.unwrap();
  assert!(times.1>times.0,"the owner effect occurs after the sampled command receipt");
  let mut c=db.acquire().await.unwrap();
  let q=erpx_server_files::public::field_outbox::QUARANTINE;
  let evidence=format!("({}) AND ({})",erpx_server_files::public::field_outbox::ELIGIBLE,erpx_server_workflow::public::field_outbox::FILES_COMMAND_EVIDENCE);
  let mut owned=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT count(*) FROM outbox_events e WHERE ");owned.push(q);assert_eq!(owned.build_query_scalar::<i64>().fetch_one(&mut *c).await.unwrap(),1);
  let fenced=sqlx::query("UPDATE outbox_events SET lease_token='legacy-files' WHERE command_id=$1").bind(&ledger).execute(&mut *c).await.unwrap_err();assert_eq!(fenced.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));
  use erpx_server_workflow::public::service_count_outbox::{claim,eligible,fail,complete};
  let now=9_000_000_000_000i64;
  let event=claim(&mut c,q,"files-native",now).await.unwrap().unwrap();
  assert!(eligible(&mut c,&evidence,&event).await.unwrap());
  let leaked:Option<String>=sqlx::query_scalar("SELECT nullif(current_setting('erpx.outbox_runtime',true),'')").fetch_one(&mut *c).await.unwrap();assert_eq!(leaked,None);
  let saved:String=sqlx::query_scalar("SELECT result_json FROM command_receipts WHERE command_id=$1").bind(&ledger).fetch_one(&mut *c).await.unwrap();
  sqlx::query("UPDATE command_receipts SET result_json='{}' WHERE command_id=$1").bind(&ledger).execute(&mut *c).await.unwrap();
  assert!(!eligible(&mut c,&evidence,&event).await.unwrap());
  assert!(fail(&mut c,&event,now,"SOURCE_INVALID").await.unwrap());
  let reclaimed=claim(&mut c,q,"files-native-retry",now+60_001).await.unwrap().unwrap();assert_eq!(event.id,reclaimed.id);
  assert!(!eligible(&mut c,&evidence,&reclaimed).await.unwrap());
  sqlx::query("UPDATE command_receipts SET result_json=$1 WHERE command_id=$2").bind(saved).bind(&ledger).execute(&mut *c).await.unwrap();
  assert!(eligible(&mut c,&evidence,&reclaimed).await.unwrap());assert!(complete(&mut c,&reclaimed,now+60_002).await.unwrap());
  // The same aggregate identifier in another Files owner's command does not
  // transfer ownership: a native set origin/bind command is also required.
  sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)VALUES('other-files-owner','other-files-owner','attachment.record.bind','attachment_record','issue-order',1,'{}',1)").execute(&mut *c).await.unwrap();
  assert!(claim(&mut c,q,"files-native-collision",now+120_000).await.unwrap().is_none());
  assert_eq!(sqlx::query("UPDATE outbox_events SET lease_token='legacy-other-owner' WHERE id='other-files-owner'").execute(&mut *c).await.unwrap().rows_affected(),1);
  assert!(created.data.get("id").is_some());
 }).await;
}

#[tokio::test]
async fn native_field_dead_letter_recovery_requires_current_source_and_preserves_failure_audit(){
 facts::account::with_system(|system|async move{
  use erpx_server_workflow::public::service_count_outbox::{claim,fail,eligible,complete,DeadLetterRetry,retry_owned_dead_letter};
  let p=facts::setup_workspace(system).await;
  execute(&p,"field-outbox-recovery","exception_evidence").await.unwrap();
  let ledger=erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-a","field-outbox-recovery");
  let mut c=p.pool().raw().acquire().await.unwrap();
  let q=erpx_server_files::public::field_outbox::QUARANTINE;
  let evidence=format!("({}) AND ({})",erpx_server_files::public::field_outbox::ELIGIBLE,erpx_server_workflow::public::field_outbox::FILES_COMMAND_EVIDENCE);
  let mut at=9_000_000_000_000i64;
  let mut last=None;
  for attempt in 1..=6 {
   let event=claim(&mut c,q,"before-native-recovery",at).await.unwrap().unwrap();
   assert_eq!(event.attempts,attempt);
   assert!(fail(&mut c,&event,at,"SOURCE_INVALID").await.unwrap());
   last=Some(event);if attempt<6{at+=60_001;}
  }
  let event=last.unwrap();
  let retry=|dead_at|DeadLetterRetry{event_id:&event.id,dead_letter_at:dead_at,now:at+1,holding_id:"h001",reason:"Retry after correcting native owner evidence",audit_id:"native-field-retry-test"};
  assert!(!retry_owned_dead_letter(&mut c,q,&evidence,retry(at-1)).await.unwrap());
  let saved:String=sqlx::query_scalar("SELECT result_json FROM command_receipts WHERE command_id=$1").bind(&ledger).fetch_one(&mut *c).await.unwrap();
  sqlx::query("UPDATE command_receipts SET result_json='{}' WHERE command_id=$1").bind(&ledger).execute(&mut *c).await.unwrap();
  assert!(!retry_owned_dead_letter(&mut c,q,&evidence,retry(at)).await.unwrap());
  sqlx::query("UPDATE command_receipts SET result_json=$1 WHERE command_id=$2").bind(saved).bind(&ledger).execute(&mut *c).await.unwrap();
  let duplicate:String=sqlx::query_scalar("SELECT id FROM audit_events WHERE command_id=$1").bind(&ledger).fetch_one(&mut *c).await.unwrap();
  let failed=retry_owned_dead_letter(&mut c,q,&evidence,DeadLetterRetry{audit_id:&duplicate,..retry(at)}).await.unwrap_err();assert_eq!(failed.as_database_error().and_then(|e|e.code()).as_deref(),Some("23505"));
  let unchanged:(i64,Option<i64>)=sqlx::query_as("SELECT attempts,dead_letter_at FROM outbox_events WHERE id=$1").bind(&event.id).fetch_one(&mut *c).await.unwrap();assert_eq!(unchanged,(6,Some(at)));
  assert!(retry_owned_dead_letter(&mut c,q,&evidence,retry(at)).await.unwrap());
  assert!(!retry_owned_dead_letter(&mut c,q,&evidence,retry(at)).await.unwrap());
  let audit:(i64,String,String,String)=sqlx::query_as("SELECT count(*) OVER (),actor_id,purpose,authority_json FROM audit_events WHERE action='runtime.job.retry' AND record_id=$1").bind(&event.id).fetch_one(&mut *c).await.unwrap();
  assert_eq!(audit.0,1);assert_eq!(audit.1,"runtime:field-dependency-worker");
  let purpose:serde_json::Value=serde_json::from_str(&audit.2).unwrap();assert_eq!(purpose["previousAttempts"],6);assert_eq!(purpose["previousError"],"SOURCE_INVALID");assert_eq!(purpose["deadLetterAt"],at);
  let authority:serde_json::Value=serde_json::from_str(&audit.3).unwrap();assert_eq!(authority["kind"],"runtime_service");assert_eq!(authority["holdingId"],"h001");assert_eq!(authority["companyId"],"company");assert!(authority.get("sessionId").is_none());assert!(authority.get("personId").is_none());
  let next=claim(&mut c,q,"after-native-recovery",at+2).await.unwrap().unwrap();assert_eq!(next.id,event.id);assert_eq!(next.attempts,1);assert!(eligible(&mut c,&evidence,&next).await.unwrap());assert!(complete(&mut c,&next,at+3).await.unwrap());
 }).await;
}

#[tokio::test]
async fn intake_create_and_replay_fail_closed_while_grant_writer_holds_fence() {
    use std::sync::atomic::Ordering;
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        let db=p.pool().raw();
        let key="intake-fence-contention";
        let ledger=erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-a",key);
        let provider=Arc::new(BrokenProvider(std::sync::atomic::AtomicUsize::new(0)));
        let mut writer=db.begin().await.unwrap();
        let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        assert_eq!(sqlx::query("UPDATE company_package_grants SET reason=reason WHERE company_id='company' AND package_id='service'").execute(&mut *writer).await.unwrap().rows_affected(),1);
        let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='company_package_grants'::regclass AND mode='RowExclusiveLock' AND granted)").bind(pid).fetch_one(db).await.unwrap();assert!(held);
        let blocked=tokio::time::timeout(std::time::Duration::from_secs(5),execute_with_provider(&p,key,"site_photo",p.review.maker.clone(),p.selection.clone(),"company",Some(provider.clone()))).await.unwrap();
        assert!(matches!(blocked,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        assert_eq!(provider.0.load(Ordering::SeqCst),0);
        let none:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action IN ('set.created','set.native-origin')),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(&ledger).fetch_one(db).await.unwrap();
        assert_eq!(none,(0,0,0,0,0));writer.rollback().await.unwrap();
        let first=execute(&p,key,"site_photo").await.unwrap();assert!(!first.replayed);
        let persisted:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action IN ('set.created','set.native-origin')),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(&ledger).fetch_one(db).await.unwrap();
        assert_eq!(persisted,(1,2,1,1,1));
        let mut writer=db.begin().await.unwrap();
        let replay_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        assert_eq!(sqlx::query("UPDATE company_package_grants SET reason=reason WHERE company_id='company' AND package_id='service'").execute(&mut *writer).await.unwrap().rows_affected(),1);
        let replay_held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='company_package_grants'::regclass AND mode='RowExclusiveLock' AND granted)").bind(replay_pid).fetch_one(db).await.unwrap();assert!(replay_held);
        let blocked_replay=tokio::time::timeout(std::time::Duration::from_secs(5),execute_with_provider(&p,key,"site_photo",p.review.maker.clone(),p.selection.clone(),"company",Some(provider.clone()))).await.unwrap();
        assert!(matches!(blocked_replay,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        assert_eq!(provider.0.load(Ordering::SeqCst),0);
        let during:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action IN ('set.created','set.native-origin')),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(&ledger).fetch_one(db).await.unwrap();assert_eq!(during,persisted);
        writer.rollback().await.unwrap();
        let replay=execute_with_provider(&p,key,"site_photo",p.review.maker.clone(),p.selection.clone(),"company",Some(provider.clone())).await.unwrap();assert!(replay.replayed);assert_eq!(replay.data,first.data);assert_eq!(provider.0.load(Ordering::SeqCst),0);
        let after:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action IN ('set.created','set.native-origin')),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(&ledger).fetch_one(db).await.unwrap();assert_eq!(after,persisted);
    }).await;
}
