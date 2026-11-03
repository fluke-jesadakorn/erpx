//! Actual Account SSO/selected IAM and Workflow ledger; historical Field rows
//! are the existing explicit synthetic prerequisite, not workflow acceptance.
#[allow(dead_code)]
#[expect(
    unused_imports,
    reason = "Shared existing Field fixture reexports other owner helpers"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
#[path = "../../support/field_service/service-posting.rs"]
mod service_posting_fixture;
#[path = "../../support/rust-postgres.rs"]
mod projection_fixture;
#[allow(dead_code)]
#[path="../../support/runtime/private-session-config.rs"]
mod runtime_configuration;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
use erpx_service::field_collaboration_command::*;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn native_service_commercial_authority_retains_holding_account_and_mfa() {
    use erpx_server_identity_access::public::{platform_commercial_authority::*,account_security::*,*};
    use erpx_server_runtime::public::postgres::PgTransactionError;
    facts::account::with_system(|system|async move{
        let db=system.control.raw();let now=system.scenario.clock.now().get();
        let policy=facts::account::fixture::account::policy();
        let registry=facts::account::fixture::account::holding_registry();
        // Published operator policy/assignment are explicit fixture sources.
        // Account login, Holding SSO and MFA below use actual native services.
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('platform','platform_field_commercial','Field Commercial reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('platform','platform_field_commercial',1,'[\"platform.commercial.read\",\"platform.commercial.manage\"]','[]','[\"platform\"]','fixture',0);INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version)VALUES('platform','fixture-a','local:fixture-a','active','local',1)").execute(db).await.unwrap();
        sqlx::query("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until)VALUES('field-commercial-operator','fixture-a','platform_field_commercial','[]',$1,$2)").bind(now-1000).bind(now+3600000).execute(db).await.unwrap();
        let central=facts::account::login(&system,"a@example.test",None).await;
        let child=facts::account::child(&system,&central,facts::account::ACME).await;
        let device=DeviceProof{active:Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session,&child.token)),..Default::default()};
        let actor=PlatformCommercialSelection{user_id:"fixture-a".into(),session_id:child.result.session_id.clone(),assignment_id:"field-commercial-operator".into(),role:"platform_field_commercial".into(),role_revision:1};
        // This fixture explicitly configures Acme as its provider entry. Real
        // runtime must supply its exact registered provider-entry Holding.
        let provider=HoldingId::new("h001").unwrap();
        let mut tx=db.begin().await.unwrap();
        let read=verify_platform_commercial_authority(&mut tx,"acme.example.test",device.clone(),policy.clone(),registry.clone(),&provider,actor.clone(),PlatformCommercialOperation::Read,now).await.unwrap();assert!(read.can_manage());read.require_current(&mut tx,now).await.unwrap();
        let resolved=verify_current_platform_commercial_authority(&mut tx,"acme.example.test",device.clone(),policy.clone(),registry.clone(),&provider,Some("stale-cookie-preference"),PlatformCommercialOperation::Read,now).await.unwrap();assert_eq!(resolved.selected(),&actor);resolved.require_current(&mut tx,now).await.unwrap();tx.rollback().await.unwrap();
        let mut tx=db.begin().await.unwrap();
        let missing=verify_platform_commercial_authority(&mut tx,"acme.example.test",device.clone(),policy.clone(),registry.clone(),&provider,actor.clone(),PlatformCommercialOperation::Apply,now).await;
        assert!(matches!(missing,Err(PgTransactionError::Business(e))if e.code=="FRESH_MFA_REQUIRED"));tx.rollback().await.unwrap();
        let keys=Arc::new(AccountMfaKeyring::new("fixture".into(),vec![("fixture".into(),[7;32])]).unwrap());
        let mfa=PostgresAccountMfa::new(system.control.clone(),"development".into(),keys.clone(),system.scenario.clock.clone(),registry.clone()).unwrap();
        let audience=RequestAudience::Holding{audience:facts::account::ACME.into()};
        mfa.enroll(device.clone(),audience.clone()).await.unwrap();
        let envelope:String=sqlx::query_scalar("SELECT secret FROM iam_mfa WHERE user_id='fixture-a'").fetch_one(db).await.unwrap();
        let code=keys.open("fixture-a",&envelope).unwrap().code(now/30000).unwrap();
        mfa.verify(device.clone(),audience,code).await.unwrap();
        let mut tx=db.begin().await.unwrap();
        let apply=verify_platform_commercial_authority(&mut tx,"acme.example.test",device.clone(),policy.clone(),registry.clone(),&provider,actor.clone(),PlatformCommercialOperation::Apply,now).await.unwrap();apply.require_current(&mut tx,now).await.unwrap();
        // Actual concurrent source revocation cannot race this retained lease.
        let mut competitor=db.begin().await.unwrap();sqlx::query("SET LOCAL lock_timeout='100ms'").execute(&mut *competitor).await.unwrap();
        let locked=sqlx::query("UPDATE platform_assignments SET revoked_at=1 WHERE id='field-commercial-operator'").execute(&mut *competitor).await.unwrap_err();assert_eq!(locked.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03"));competitor.rollback().await.unwrap();
        let expired=apply.require_current(&mut tx,now+300000).await;assert!(matches!(expired,Err(PgTransactionError::Business(e))if e.code=="FRESH_MFA_REQUIRED"));
        sqlx::query("INSERT INTO admin_provider_contexts(session_id,user_id,assignment_id,revision,updated_at)VALUES($1,'fixture-a','another-selected-assignment',1,$2)").bind(&actor.session_id).bind(now).execute(&mut *tx).await.unwrap();
        let changed=apply.require_current(&mut tx,now).await;assert!(matches!(changed,Err(PgTransactionError::Business(e))if e.code=="PROVIDER_CONTEXT_CHANGED"));
        let no_fallback=verify_current_platform_commercial_authority(&mut tx,"acme.example.test",device.clone(),policy.clone(),registry.clone(),&provider,None,PlatformCommercialOperation::Apply,now).await;assert!(matches!(no_fallback,Err(PgTransactionError::Business(e))if e.code=="PROVIDER_CONTEXT_CHANGED"));tx.rollback().await.unwrap();
        let mut other=db.begin().await.unwrap();assert!(apply.require_current(&mut other,now).await.is_err());other.rollback().await.unwrap();
        for (holding,selection,expected) in [(HoldingId::new("h002").unwrap(),actor.clone(),"PLATFORM_AUTHORITY_REQUIRED"),(provider.clone(),PlatformCommercialSelection{user_id:"fixture-b".into(),..actor.clone()},"PLATFORM_AUTHORITY_REQUIRED"),(provider.clone(),PlatformCommercialSelection{role_revision:2,..actor.clone()},"POLICY_CHANGED")]{
            let mut tx=db.begin().await.unwrap();let result=verify_platform_commercial_authority(&mut tx,"acme.example.test",device.clone(),policy.clone(),registry.clone(),&holding,selection,PlatformCommercialOperation::Apply,now).await;assert!(matches!(result,Err(PgTransactionError::Business(e))if e.code==expected));tx.rollback().await.unwrap();
        }
    }).await;
}

#[tokio::test]
async fn selected_records_actor_executes_all_field_collaboration_commands_with_exact_replay() {
    facts::account::with_system(|system| async move {
        let mut p = facts::setup_workspace(system).await;
        let clock = p.review.system.scenario.clock.clone();
        let pool = p.pool().clone();
        let db = pool.raw();
        let native_id:String=sqlx::query_scalar("SELECT id FROM field_service_orders WHERE company_id='company' ORDER BY id LIMIT 1").fetch_one(db).await.unwrap();
        let alias=sqlx::query("INSERT INTO workspace_records(module_id,id,section_id,title,fields_json,company_id,created_by,updated_by,created_at,updated_at)VALUES('operations',$1,'exceptions','Aliased native Field source','[]','company','fixture-a','fixture-a',0,0)").bind(native_id).execute(db).await.unwrap_err();
        assert_eq!(alias.as_database_error().and_then(|e|e.constraint()),Some("native_field_records_required"));
        sqlx::raw_sql(r#"
INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','native_collaboration_records','Synthetic Records contributor',false,1,'active');
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','native_collaboration_records',1,'["records.read","records.write","module.field_service.read","module.collaboration.read","module.operations.read"]','[]','["holding","branch","department"]','synthetic',0);
UPDATE assignments SET role='native_collaboration_records',modules_json='["field_service","collaboration","operations"]',version=4 WHERE id='field-fixture-a';
UPDATE active_work_contexts SET assignment_version=4,revision=9 WHERE user_id='fixture-a';
"#).execute(db).await.unwrap();
        let expected = p.selection.expected.as_mut().unwrap();
        expected.assignment_version = 4; expected.revision = 9;
        let target = FieldCollaborationTarget {
            holding: HoldingId::new("h001").unwrap(), company: CompanyId::new("company").unwrap(), clock: clock.clone(),
            catalogue: Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap()),
        };
        let create = FieldCollaborationInput { key: "native-field-card-create".into(), operation: "collaboration.action.create".into(), id: None, body: serde_json::to_vec(&json!({"sourceType":"field_service_order","sourceId":"reserve-order","title":"Review visit evidence","ownerId":"fixture-a","dueAt":clock.now().get()+60000,"reason":"Review the actual source"})).unwrap() };
        for _ in 0..2 {
            let selection = p.selection.clone();
            let receipt = execute_selected_field_collaboration(&p.review.authorizer, "acme.example.test", p.review.maker.clone(), move || Ok(selection), target.clone(), create.clone()).await.unwrap();
            assert_eq!(receipt.id, "action-card-native-field-card-create");
            assert_eq!(receipt.version, 1); assert_eq!(receipt.status, "open");
        }
        let denied = FieldCollaborationInput { key: "native-field-card-cancel-denied".into(), operation: "collaboration.action.cancel".into(), id: Some("action-card-native-field-card-create".into()), body: serde_json::to_vec(&json!({"action":"cancel","expectedVersion":1,"reason":"Reviewer required"})).unwrap() };
        let selection = p.selection.clone();
        let result = execute_selected_field_collaboration(&p.review.authorizer, "acme.example.test", p.review.maker.clone(), move || Ok(selection), target.clone(), denied).await;
        assert!(matches!(result, Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code == "ACTION_CARD_REVIEW_REQUIRED"));
        let complete = FieldCollaborationInput { key: "native-field-card-complete".into(), operation: "collaboration.action.complete".into(), id: Some("action-card-native-field-card-create".into()), body: serde_json::to_vec(&json!({"action":"complete","expectedVersion":1,"evidence":"The visit evidence was reviewed","reason":"Completed source review"})).unwrap() };
        for _ in 0..2 {
            let selection = p.selection.clone();
            let receipt = execute_selected_field_collaboration(&p.review.authorizer, "acme.example.test", p.review.maker.clone(), move || Ok(selection), target.clone(), complete.clone()).await.unwrap();
            assert_eq!(receipt.version, 2); assert_eq!(receipt.status, "completed");
        }
        for (operation, key, id, raw, status, version) in [
            ("collaboration.relation.create", "native-field-relation-create", None, json!({"sourceType":"field_service_order","sourceId":"reserve-order","targetType":"field_service_order","targetId":"issue-order","relation":"related","reason":"Link actual Field sources"}), "active", 1),
            ("collaboration.relation.remove", "native-field-relation-remove", Some("record-relation-native-field-relation-create"), json!({"expectedVersion":1,"reason":"  Remove obsolete relation  "}), "removed", 2),
            ("collaboration.action.create", "native-field-card-review", None, json!({"sourceType":"field_service_order","sourceId":"reserve-order","title":"Review routing","ownerId":"fixture-a","dueAt":clock.now().get()+60000,"reason":"Review routing"}), "open", 1),
        ] {
            for _ in 0..2 {
                let selection = p.selection.clone();
                let command = FieldCollaborationInput { operation:operation.into(),key:key.into(),id:id.map(Into::into),body:serde_json::to_vec(&raw).unwrap() };
                let receipt=execute_selected_field_collaboration(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),command).await.unwrap();
                assert_eq!((receipt.status,receipt.version),(status,version));
            }
        }
        let reason:String=sqlx::query_scalar("SELECT reason FROM collaboration_relation_events WHERE id='record-relation-event-native-field-relation-remove'").fetch_one(db).await.unwrap();
        assert_eq!(reason,"  Remove obsolete relation  ");
        // A published fixture role change, rather than a forged actor or a
        // permissive port, establishes current reviewer authority.
        sqlx::raw_sql(r#"
INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','native_collaboration_reviewer','Synthetic Records reviewer',false,1,'active');
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','native_collaboration_reviewer',1,'["records.read","records.write","records.review","module.field_service.read","module.collaboration.read","module.operations.read"]','[]','["holding","branch","department"]','synthetic',0);
UPDATE assignments SET role='native_collaboration_reviewer',version=5 WHERE id='field-fixture-a';
UPDATE active_work_contexts SET assignment_version=5,revision=10 WHERE user_id='fixture-a';
"#).execute(db).await.unwrap();
        let expected=p.selection.expected.as_mut().unwrap();expected.assignment_version=5;expected.revision=10;
        for (operation,key,raw,status,version) in [
            ("collaboration.action.reassign","native-field-card-reassign",json!({"action":"reassign","expectedVersion":1,"ownerId":"fixture-a","dueAt":clock.now().get()+120000,"reason":"Reviewer updated routing"}),"open",2),
            ("collaboration.action.cancel","native-field-card-cancel",json!({"action":"cancel","expectedVersion":2,"reason":"Reviewer cancelled routing"}),"cancelled",3),
        ] {
            for _ in 0..2 {
                let selection=p.selection.clone();
                let command=FieldCollaborationInput{operation:operation.into(),key:key.into(),id:Some("action-card-native-field-card-review".into()),body:serde_json::to_vec(&raw).unwrap()};
                let receipt=execute_selected_field_collaboration(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),command).await.unwrap();
                assert_eq!((receipt.status,receipt.version),(status,version));
            }
        }
        let total:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id LIKE 'native-field-%'),(SELECT count(*) FROM audit_events WHERE command_id LIKE 'native-field-%'),(SELECT count(*) FROM outbox_events WHERE command_id LIKE 'native-field-%')").fetch_one(db).await.unwrap();
        assert_eq!(total,(7,7,7));
        let effects: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('native-field-card-create','native-field-card-complete')),(SELECT count(*) FROM audit_events WHERE command_id IN ('native-field-card-create','native-field-card-complete')),(SELECT count(*) FROM outbox_events WHERE command_id IN ('native-field-card-create','native-field-card-complete')),(SELECT count(*) FROM collaboration_action_events WHERE card_id='action-card-native-field-card-create'),(SELECT count(*) FROM command_receipts WHERE command_id='native-field-card-cancel-denied')").fetch_one(db).await.unwrap();
        assert_eq!(effects, (2,2,2,2,0));
        #[cfg(feature="field-service-http")]
        {
            use tower::ServiceExt;
            let central=facts::account::login(&p.review.system,"a@example.test",None).await;
            let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;
            sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',5,10,$2,$3)").bind(&child.result.session_id).bind(clock.now().get()+3_600_000).bind(clock.now().get()).execute(db).await.unwrap();
            let cookie=format!("__Host-erpx-product={}",child.token.expose());
            let mut api_config=runtime_configuration::document(&p.review.system.control,&p.review.system.holdings).await;
            api_config["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);api_config["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);
            let runtime=erpx_service::private_development_api::assemble_development_api(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&api_config.to_string()).unwrap(),clock.clone(),Arc::new(erpx_server_identity_access::public::SystemAuthenticationSecrets)).await.unwrap();
            let router=runtime.router();
            let path="/api/v1/holdings/acme/field-service/collaboration";
            let request=|method:&str,cookie:&str,revision:&str,origin:&str,body:String|axum::http::Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin",origin).header("cookie",cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version","5").header("idempotency-key","http-field-card-create").body(axum::body::Body::from(body)).unwrap();
            let body=serde_json::to_string(&json!({"sourceType":"field_service_order","sourceId":"reserve-order","title":"HTTP source review","ownerId":"fixture-a","dueAt":clock.now().get()+60000,"reason":"Actual authenticated HTTP"})).unwrap();
            for _ in 0..2 {
                let response=router.clone().oneshot(request("POST",&cookie,"10","https://acme.example.test",body.clone())).await.unwrap();
                assert_eq!(response.status(),axum::http::StatusCode::OK);
                assert_eq!(response.headers()["cache-control"],"private, no-store");
                let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
                assert_eq!(value["data"]["id"],"action-card-http-field-card-create");assert_eq!(value["context"]["revision"],10);
            }
            let response=router.clone().oneshot(request("GET",&cookie,"10","https://acme.example.test",String::new())).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::OK);
            let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
            assert_eq!(value["data"]["cards"].as_array().unwrap().len(),3);assert_eq!(value["data"]["sourceTypes"],json!(["field_service_order","support_ticket"]));assert_eq!(value["data"]["people"],json!([{"id":"fixture-a","name":"PRIVATE_SYNTHETIC_WORKER"}]));assert_eq!(value["context"]["companyId"],"company");assert_eq!(value["data"]["work"].as_array().unwrap().len(),3);assert!(value["data"]["work"].as_array().unwrap().iter().all(|w|w["type"]=="collaboration_action"&&w["href"]=="/field-service/collaboration"));
            for (cookie,revision,origin,status) in [(&cookie[..],"9","https://acme.example.test",axum::http::StatusCode::CONFLICT),("","10","https://acme.example.test",axum::http::StatusCode::UNAUTHORIZED),(&cookie[..],"10","https://evil.example.test",axum::http::StatusCode::FORBIDDEN)] {
                let response=router.clone().oneshot(request("POST",cookie,revision,origin,"not json".into())).await.unwrap();assert_eq!(response.status(),status);
            }
            // Field entry readback preserves Support ownership and its purchase gate.
            let support_request = |id:&str,cookie:&str,revision:&str| axum::http::Request::builder().method("GET").uri(format!("/api/v1/holdings/acme/field-service/support/{id}")).header("host","acme.example.test").header("cookie",cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version","5").body(axum::body::Body::empty()).unwrap();
            let response=router.clone().oneshot(support_request("ticket-reserve-order",&cookie,"10")).await.unwrap();
            assert_eq!(response.status(),axum::http::StatusCode::FORBIDDEN);
            let denied:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
            assert!(denied.get("data").is_none());
            let mixed=FieldCollaborationInput{key:"field-support-denied".into(),operation:"collaboration.relation.create".into(),id:None,body:serde_json::to_vec(&json!({"sourceType":"field_service_order","sourceId":"reserve-order","targetType":"support_ticket","targetId":"ticket-reserve-order","relation":"blocks","reason":"Actual cross-owner handoff"})).unwrap()};
            let selection=p.selection.clone();
            assert!(execute_selected_field_collaboration(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),mixed).await.is_err());
            let denied_effects:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='field-support-denied'").fetch_one(db).await.unwrap();assert_eq!(denied_effects,0);
            let mut grant=db.begin().await.unwrap();
            sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company','support',1,'active',0,1,'Synthetic Support readback purchase','public synthetic evidence',$1)").bind(clock.now().get()).execute(&mut *grant).await.unwrap();
            sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'field-support-readback',company_id,package_id,'fixture-a','synthetic-support-readback',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company'AND package_id='support'").bind(clock.now().get()).execute(&mut *grant).await.unwrap();
            grant.commit().await.unwrap();
            let response=router.clone().oneshot(support_request("ticket-reserve-order",&cookie,"10")).await.unwrap();
            assert_eq!(response.status(),axum::http::StatusCode::OK);assert_eq!(response.headers()["cache-control"],"private, no-store");
            let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
            assert_eq!(value["data"]["row"]["id"],"ticket-reserve-order");assert_eq!(value["data"]["row"]["service_order_id"],"reserve-order");assert_eq!(value["data"]["managedInFieldService"],true);assert_eq!(value["data"]["canManage"],false);assert_eq!(value["context"]["revision"],10);
            for (id,credential,revision,status) in [("ticket-reserve-order","","10",axum::http::StatusCode::UNAUTHORIZED),("ticket-reserve-order",cookie.as_str(),"9",axum::http::StatusCode::CONFLICT),("missing",cookie.as_str(),"10",axum::http::StatusCode::NOT_FOUND)] {
                let response=router.clone().oneshot(support_request(id,credential,revision)).await.unwrap();assert_eq!(response.status(),status);
            }
            // Both immutable endpoints select the native partition. Purchase,
            // recursive Records guards, ledger and retries stay in one owner unit.
            for reverse in [false,true]{
                let key=if reverse{"support-field-relation"}else{"field-support-relation"};
                let (source_type,source_id,target_type,target_id)=if reverse{("support_ticket","ticket-reserve-order","field_service_order","reserve-order")}else{("field_service_order","reserve-order","support_ticket","ticket-reserve-order")};
                let input=FieldCollaborationInput{key:key.into(),operation:"collaboration.relation.create".into(),id:None,body:serde_json::to_vec(&json!({"sourceType":source_type,"sourceId":source_id,"targetType":target_type,"targetId":target_id,"relation":"blocks","reason":"Actual cross-owner handoff"})).unwrap()};
                for _ in 0..2{let selection=p.selection.clone();let result=execute_selected_field_collaboration(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),input.clone()).await.unwrap();assert_eq!(result.id,format!("record-relation-{key}"));assert_eq!(result.version,1);}
                let id=format!("record-relation-{key}");let remove=FieldCollaborationInput{key:format!("{key}-remove"),operation:"collaboration.relation.remove".into(),id:Some(id),body:serde_json::to_vec(&json!({"expectedVersion":1,"reason":"Completed source handoff"})).unwrap()};
                for _ in 0..2{let selection=p.selection.clone();let result=execute_selected_field_collaboration(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),remove.clone()).await.unwrap();assert_eq!(result.version,2);assert_eq!(result.status,"removed");}
                let evidence:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1 OR command_id=$2),(SELECT count(*) FROM audit_events WHERE command_id=$1 OR command_id=$2),(SELECT count(*) FROM outbox_events WHERE command_id=$1 OR command_id=$2)").bind(key).bind(format!("{key}-remove")).fetch_one(db).await.unwrap();assert_eq!(evidence,(2,2,2));
                let fenced=sqlx::query("UPDATE outbox_events SET lease_token='legacy-mixed-relation' WHERE command_id=$1").bind(key).execute(db).await.unwrap_err();assert_eq!(fenced.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));
            }
            // Existing database writer fence rejects a generic Support edit. No guard is disabled.
            let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let company=CompanyId::new("company").unwrap();
            let read=erpx_server_support::public::field_ticket_read::read_field_ticket(&mut tx,&company,"ticket-reserve-order",Some("branch"),None).await.unwrap().unwrap();
            assert!(read.require_reader("fixture-a",false,true).is_ok());assert!(read.require_reader("unrelated",false,true).is_err());assert!(read.require_reader("unrelated",true,false).is_ok());
            assert!(erpx_server_support::public::field_ticket_read::read_field_ticket(&mut tx,&company,"ticket-reserve-order",Some("another-branch"),None).await.unwrap().is_none());
            let denied=sqlx::query("UPDATE support_tickets SET title='Injected stale title' WHERE id='ticket-reserve-order'").execute(&mut *tx).await.unwrap_err();assert!(denied.to_string().contains("SERVICE_CASE_MANAGED_BY_ORDER"));tx.rollback().await.unwrap();
            let mut another=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *another).await.unwrap();assert!(read.require_current(&mut another).await.is_err());another.rollback().await.unwrap();
            let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='http-field-card-create'),(SELECT count(*) FROM audit_events WHERE command_id='http-field-card-create'),(SELECT count(*) FROM outbox_events WHERE command_id='http-field-card-create')").fetch_one(db).await.unwrap();assert_eq!(counts,(1,1,1));
            runtime.close().await;
        }
        // Actual native policy/open/lifecycle composition. Historical Field
        // source rows remain explicit fixture prerequisites, not browser acceptance.
        {
            use erpx_service::field_operations_command::{execute_selected_field_operations,FieldOperationsTransition,execute_selected_field_operations_setup,FieldOperationsSetup};
            facts::setup_agreement_review_facts(&p,clock.now().get()).await;
            sqlx::raw_sql(r#"UPDATE assignments SET role='native_collaboration_reviewer',modules_json='["field_service","operations"]',version=4 WHERE id='agreement-reviewer-b';UPDATE active_work_contexts SET assignment_id='agreement-reviewer-b',assignment_version=4,revision=8 WHERE user_id='fixture-b';"#).execute(db).await.unwrap();
            let mut reviewer=p.review.reviewer_selection.clone();let expected=reviewer.expected.as_mut().unwrap();expected.assignment_id=erpx_server_shared::public::AssignmentId::new("agreement-reviewer-b").unwrap();expected.assignment_version=4;expected.revision=8;

            let draft=FieldOperationsSetup{key:"ops-field-policy-draft".into(),id:None,body:json!({"entity":"policy","area":"service","name":"Native Field exception policy","sourceKinds":["field_service_order","field_service_order"],"targetMinutes":60})};
            for _ in 0..2{let selected=p.selection.clone();let result=execute_selected_field_operations_setup(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),draft.clone()).await.unwrap();assert_eq!(result,json!({"id":"queue-policy-ops-field-policy-draft","version":1,"status":"draft"}));}
            let publish=FieldOperationsSetup{key:"ops-field-policy-publish".into(),id:Some("queue-policy-ops-field-policy-draft".into()),body:json!({"entity":"policy","action":"publish","expectedVersion":1,"reason":"Independent actual policy review"})};
            let selected=p.selection.clone();let mut own=publish.clone();own.key="ops-field-policy-self-review".into();let denied=execute_selected_field_operations_setup(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),own).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="CONFLICT"));
            for _ in 0..2{let selected=reviewer.clone();let result=execute_selected_field_operations_setup(&p.review.authorizer,"acme.example.test",p.review.reviewer.clone(),move||Ok(selected),target.clone(),publish.clone()).await.unwrap();assert_eq!(result,json!({"id":"queue-policy-ops-field-policy-draft","version":2,"status":"published"}));}
            let selected=p.selection.clone();let result=execute_selected_field_operations_setup(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),draft).await.unwrap();assert_eq!(result["status"],"draft");assert_eq!(result["version"],1);
            let open=FieldOperationsSetup{key:"ops-field-open".into(),id:None,body:json!({"entity":"exception","source":{"kind":"field_service_order","id":"reserve-order","version":1},"policyId":"queue-policy-ops-field-policy-draft","policyVersion":2,"title":"Native Field operational exception","issue":"Visit needs independent review","priority":"high","ownerId":"fixture-a"})};
            let selected=p.selection.clone();let mut future=open.clone();future.key="ops-field-open-future".into();let denied=execute_selected_field_operations_setup(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),future).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="OPERATIONS_RECIPIENT_UNAVAILABLE"));
            sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
            for _ in 0..2{let selected=p.selection.clone();let result=execute_selected_field_operations_setup(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),open.clone()).await.unwrap();assert_eq!(result,json!({"id":"operations-exception-ops-field-open","version":1,"status":"open"}));}
            let facts:(i64,i64,String,String)=sqlx::query_as("SELECT due_at,source_version,source_kinds_json,source_reference_json FROM operations_exceptions e JOIN operations_queue_policies p ON p.id=e.policy_id WHERE e.id='operations-exception-ops-field-open'").fetch_one(db).await.unwrap();assert_eq!(facts.0,clock.now().get()+60*60000);assert_eq!(facts.1,1);assert_eq!(facts.2,"[\"field_service_order\"]");let source:serde_json::Value=serde_json::from_str(&facts.3).unwrap();assert_eq!(source["id"],"reserve-order");assert_eq!(source["companyId"],"company");assert_eq!(source["branchId"],"branch");
            // Preserve the distinct future-dated transition regression as well.
            sqlx::query("UPDATE hr_engagements SET start_date='2100-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
            let run=|action:&str,key:&str,version:i64|FieldOperationsTransition{id:"operations-exception-ops-field-open".into(),key:key.into(),body:json!({"action":action,"expectedVersion":version,"reason":"Actual selected Operations","resolution":"The visit evidence was reviewed"})};
            // Existing Field/Collaboration fixture intentionally starts in 2100.
            // Keep that rule and prove Operations rejects it before using dated facts.
            let selected=p.selection.clone();let future=execute_selected_field_operations(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),run("acknowledge","ops-field-future-worker",1)).await;
            assert!(matches!(future,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="OPERATIONS_RECIPIENT_UNAVAILABLE"));
            sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
            for _ in 0..2{let selected=p.selection.clone();let receipt=execute_selected_field_operations(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),run("acknowledge","ops-field-ack",1)).await.unwrap();assert_eq!(receipt,json!({"id":"operations-exception-ops-field-open","version":2,"status":"acknowledged"}));}
            // Collaboration deliberately accepts undated engagements; Operations
            // must independently reject an employment which ended before today.
            sqlx::query("UPDATE hr_engagements SET end_date=start_date WHERE person_id='field-person'").execute(db).await.unwrap();
            let selected=p.selection.clone();let denied=execute_selected_field_operations(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),run("start","ops-field-expired-worker",2)).await;
            assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="OPERATIONS_RECIPIENT_UNAVAILABLE"));
            sqlx::query("UPDATE hr_engagements SET end_date=NULL WHERE person_id='field-person'").execute(db).await.unwrap();
            for (action,key,version,status) in [("start","ops-field-start",2,"in_progress"),("resolve","ops-field-resolve",3,"awaiting_review")]{for _ in 0..2{let selected=p.selection.clone();let receipt=execute_selected_field_operations(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),run(action,key,version)).await.unwrap();assert_eq!(receipt,json!({"id":"operations-exception-ops-field-open","version":version+1,"status":status}));}}
            for (proof,selection,allowed) in [(p.review.maker.clone(),p.selection.clone(),false),(p.review.reviewer.clone(),reviewer.clone(),true)]{
                let reply=erpx_service::field_operations_command::read_selected_field_operations_workspace(&p.review.authorizer,"acme.example.test",proof,move||Ok(selection),target.clone()).await.unwrap();
                let work=&reply["data"]["work"][0];assert_eq!(work["status"],"awaiting_review");assert_eq!(work["canAct"],allowed);assert_eq!(work["queue"],if allowed{"todo"}else{"waiting"});
            }
            let selected=p.selection.clone();let denied=execute_selected_field_operations(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),run("approve","ops-field-self-review",4)).await;
            assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="OPERATIONS_INDEPENDENT_REVIEW"));
            for _ in 0..2{let selected=reviewer.clone();let receipt=execute_selected_field_operations(&p.review.authorizer,"acme.example.test",p.review.reviewer.clone(),move||Ok(selected),target.clone(),run("approve","ops-field-approve",4)).await.unwrap();assert_eq!(receipt,json!({"id":"operations-exception-ops-field-open","version":5,"status":"resolved"}));}
            for _ in 0..2{let selected=p.selection.clone();let receipt=execute_selected_field_operations(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),run("close","ops-field-close",5)).await.unwrap();assert_eq!(receipt,json!({"id":"operations-exception-ops-field-open","version":6,"status":"closed"}));}
            let effects:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id LIKE 'ops-field-%'),(SELECT count(*) FROM audit_events WHERE command_id LIKE 'ops-field-%'),(SELECT count(*) FROM outbox_events WHERE command_id LIKE 'ops-field-%'),(SELECT count(*) FROM operations_exception_events WHERE exception_id='operations-exception-ops-field-open'),(SELECT count(*) FROM staff_notifications WHERE record_id='operations-exception-ops-field-open'),(SELECT count(*) FROM command_receipts WHERE command_id IN ('ops-field-self-review','ops-field-expired-worker','ops-field-future-worker','ops-field-open-future','ops-field-policy-self-review'))").fetch_one(db).await.unwrap();assert_eq!(effects,(8,8,8,6,6,0));
            #[cfg(feature="field-service-http")]
            {
                use tower::ServiceExt;
                let central=facts::account::login(&p.review.system,"a@example.test",None).await;let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;
                sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',5,10,$2,$3)").bind(&child.result.session_id).bind(clock.now().get()+3_600_000).bind(clock.now().get()).execute(db).await.unwrap();
                let cookie=format!("__Host-erpx-product={}",child.token.expose());
                let mut api_config=runtime_configuration::document(&p.review.system.control,&p.review.system.holdings).await;
            api_config["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);api_config["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);
            let runtime=erpx_service::private_development_api::assemble_development_api(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&api_config.to_string()).unwrap(),clock.clone(),Arc::new(erpx_server_identity_access::public::SystemAuthenticationSecrets)).await.unwrap();
            let router=runtime.router();
                let request=|method:&str,cookie:&str,revision:&str,origin:&str,body:&str|axum::http::Request::builder().method(method).uri("/api/v1/holdings/acme/field-service/operations/operations-exception-ops-field-open").header("host","acme.example.test").header("cookie",cookie).header("origin",origin).header("content-type","application/json").header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version","5").header("idempotency-key","ops-field-http-reopen").body(axum::body::Body::from(body.to_owned())).unwrap();
                let response=router.clone().oneshot(request("GET",&cookie,"10","https://acme.example.test","")).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::OK);
                let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"]["row"]["status"],"closed");assert_eq!(value["data"]["history"].as_array().unwrap().len(),6);assert_eq!(value["data"]["source"]["id"],"reserve-order");assert_eq!(value["data"]["source"]["kind"],"field_service_order");
                for _ in 0..2{let response=router.clone().oneshot(request("POST",&cookie,"10","https://acme.example.test",r#"{"action":"reopen","expectedVersion":6,"reason":"Actual native HTTP retry"}"#)).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"],json!({"id":"operations-exception-ops-field-open","version":7,"status":"in_progress"}));}
                for (credential,revision,origin,status) in [("","10","https://acme.example.test",axum::http::StatusCode::UNAUTHORIZED),(cookie.as_str(),"9","https://acme.example.test",axum::http::StatusCode::CONFLICT),(cookie.as_str(),"10","https://evil.example.test",axum::http::StatusCode::FORBIDDEN)]{let response=router.clone().oneshot(request("POST",credential,revision,origin,"not json")).await.unwrap();assert_eq!(response.status(),status);}
                let root_request=|method:&str,key:&str,body:&str|{let mut r=request(method,&cookie,"10","https://acme.example.test",body);*r.uri_mut()="/api/v1/holdings/acme/field-service/operations".parse().unwrap();r.headers_mut().insert("idempotency-key",key.parse().unwrap());r};
                let response=router.clone().oneshot(root_request("GET","unused","")).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"]["actorId"],"fixture-a");assert_eq!(value["data"]["exceptions"].as_array().unwrap().len(),1);assert_eq!(value["data"]["summary"]["active"],1);let work=&value["data"]["work"][0];assert_eq!(work["id"],"operations-exception-ops-field-open");assert_eq!(work["type"],"operational_exception");assert_eq!(work["href"],"/field-service/operations/operations-exception-ops-field-open");assert_eq!(work["canAct"],true);assert_eq!(work["source"]["id"],"reserve-order");assert_eq!(work["source"]["label"],format!("Field service order · {}",work["source"]["title"].as_str().unwrap()));assert!(value["data"]["people"].as_array().unwrap().iter().any(|p|p["id"]=="fixture-b"));
                let draft=r#"{"entity":"policy","area":"http_field","name":"HTTP Field policy","targetMinutes":30,"sourceKinds":["field_service_order"]}"#;
                for _ in 0..2{let response=router.clone().oneshot(root_request("POST","ops-field-http-draft",draft)).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"],json!({"id":"queue-policy-ops-field-http-draft","status":"draft","version":1}));}
                let publish=r#"{"entity":"policy","id":"queue-policy-ops-field-http-draft","action":"publish","expectedVersion":1,"reason":"Independent HTTP policy review"}"#;
                for _ in 0..2{let mut r=root_request("POST","ops-field-http-publish",publish);r.headers_mut().insert("cookie",format!("__Host-erpx-product={}",p.review.reviewer_token.expose()).parse().unwrap());r.headers_mut().insert("x-erpx-context-assignment","agreement-reviewer-b".parse().unwrap());r.headers_mut().insert("x-erpx-context-revision","8".parse().unwrap());r.headers_mut().insert("x-erpx-context-assignment-version","4".parse().unwrap());let response=router.clone().oneshot(r).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"],json!({"id":"queue-policy-ops-field-http-draft","status":"published","version":2}));}
                let open=r#"{"entity":"exception","source":{"kind":"field_service_order","id":"reserve-order","version":1},"policyId":"queue-policy-ops-field-http-draft","policyVersion":2,"title":"Actual HTTP exception","issue":"Review Field evidence","ownerId":"fixture-a"}"#;
                for _ in 0..2{let response=router.clone().oneshot(root_request("POST","ops-field-http-open",open)).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"],json!({"id":"operations-exception-ops-field-http-open","status":"open","version":1}));}
                let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN('ops-field-http-draft','ops-field-http-publish','ops-field-http-open')),(SELECT count(*) FROM audit_events WHERE command_id IN('ops-field-http-draft','ops-field-http-publish','ops-field-http-open')),(SELECT count(*) FROM outbox_events WHERE command_id IN('ops-field-http-draft','ops-field-http-publish','ops-field-http-open')),(SELECT count(*) FROM staff_notifications WHERE id='operations-notice-ops-field-http-open')").fetch_one(db).await.unwrap();assert_eq!(counts,(3,3,3,1));
                // Existing mixed policy belongs to the remaining Operations consumers.
                // Native Field publication must neither replace nor retire it.
                sqlx::raw_sql(r#"INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,status,created_by,reviewed_by,version,created_at,updated_at)VALUES('mixed-existing','company','mixed_queue','Existing mixed owner policy',1,'["field_service_order","support_ticket"]',60,'published','fixture-a','fixture-b',2,0,0);"#).execute(db).await.unwrap();
                for command in [FieldOperationsSetup{key:"ops-field-mixed-native-draft".into(),id:None,body:json!({"entity":"policy","area":"mixed_queue","name":"Do not replace mixed policy","sourceKinds":["field_service_order"],"targetMinutes":30})},FieldOperationsSetup{key:"ops-field-mixed-native-retire".into(),id:Some("mixed-existing".into()),body:json!({"entity":"policy","action":"retire","expectedVersion":2,"reason":"Do not retire mixed policy"})}]{let selected=p.selection.clone();let result=execute_selected_field_operations_setup(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selected),target.clone(),command).await;assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="OPERATIONS_POLICY_SHARED_AREA"));}
                let preserved:(String,i64,String)=sqlx::query_as("SELECT status,version,source_kinds_json FROM operations_queue_policies WHERE id='mixed-existing'").fetch_one(db).await.unwrap();assert_eq!(preserved,("published".into(),2,"[\"field_service_order\",\"support_ticket\"]".into()));
                let legacy_policies:Vec<String>=sqlx::query_scalar("SELECT id FROM operations_queue_policies WHERE company_id='company' AND NOT (jsonb_typeof(source_kinds_json::jsonb)='array' AND source_kinds_json::jsonb<>'[]'::jsonb AND source_kinds_json::jsonb <@ '[\"field_service_order\"]'::jsonb) ORDER BY id").fetch_all(db).await.unwrap();assert_eq!(legacy_policies,vec!["mixed-existing"]);
                let legacy_exceptions:i64=sqlx::query_scalar("SELECT count(*) FROM operations_exceptions WHERE company_id='company' AND source_kind<>'field_service_order'").fetch_one(db).await.unwrap();assert_eq!(legacy_exceptions,0);
                let denied:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id LIKE 'ops-field-mixed-native-%'").fetch_one(db).await.unwrap();assert_eq!(denied,0);
                let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='ops-field-http-reopen'),(SELECT count(*) FROM audit_events WHERE command_id='ops-field-http-reopen'),(SELECT count(*) FROM outbox_events WHERE command_id='ops-field-http-reopen'),(SELECT count(*) FROM staff_notifications WHERE id='operations-notice-ops-field-http-reopen')").fetch_one(db).await.unwrap();assert_eq!(counts,(1,1,1,1));
                runtime.close().await;
            }

        }
        // Real same-unit recursive Support reads over historical typed source
        // fixtures. These are source-guard checks, not migrated POS/PMS writers.
        sqlx::raw_sql(r#"
INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('recursive-asset','company','branch','RECURSIVE','Source asset','equipment','THB','fixture-a',0,0),('recursive-foreign','other-company','branch','RECURSIVE','Foreign asset','equipment','THB','fixture-a',0,0);
INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode)VALUES('recursive-register','company','branch','warehouse','Recursive source','retail');
INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor)VALUES('recursive-shift','recursive-register','company','branch','fixture-a',0,0);
INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,accounts_json)VALUES('recursive-sale','company','branch','recursive-register','recursive-shift','SOURCE-1','THB','2026-10-31','fixture-a',0,0,'{}');
INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at)VALUES('recursive-property','company','branch','Source hotel','2026-10-31','{}','fixture-a',0);
INSERT INTO pms_records(id,property_id,kind,data_json,updated_at)VALUES('recursive-room','recursive-property','room','{"state":"clean","number":"101"}',0),('recursive-refund-own','recursive-property','refund','{"state":"requested","requestedBy":"fixture-a"}',0),('recursive-refund-other','recursive-property','refund','{"state":"requested","requestedBy":"fixture-b"}',0);
"#).execute(db).await.unwrap();
        for (id,kind,source) in [("asset","asset","recursive-asset"),("sale","pos_sale","recursive-sale"),("room","pms_record","recursive-room"),("refund-own","pms_record","recursive-refund-own"),("refund-other","pms_record","recursive-refund-other"),("foreign","asset","recursive-foreign")]{
            sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,title,issue,status,source_kind,source_record_json,created_by,created_at,updated_at)VALUES($1,'company','branch','Typed source case','Historical source fixture','triage',$2,$3,'fixture-a',0,0)").bind(format!("recursive-ticket-{id}")).bind(kind).bind(json!({"kind":kind,"id":source,"version":1}).to_string()).execute(db).await.unwrap();
        }
        for id in ["asset","sale","room"]{
            let selection=p.selection.clone();let denied=erpx_service::field_support_read::read_selected_field_support(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),format!("recursive-ticket-{id}")).await;
            assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.kind==erpx_server_shared::public::FailureKind::NotFound));
        }
        sqlx::raw_sql(r#"
INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_recursive_reader','Synthetic recursive source reader',false,1,'active');
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_recursive_reader',1,'["records.read","module.field_service.read","module.assets.read","pos.read","pms.read","pms.housekeeping","pms.cashier"]','[]','["holding","branch","department"]','synthetic',0);
UPDATE assignments SET role='native_recursive_reader',modules_json='["field_service","assets"]',version=6 WHERE id='field-fixture-a';
UPDATE active_work_contexts SET assignment_id='field-fixture-a',assignment_version=6,revision=11 WHERE user_id='fixture-a';
"#).execute(db).await.unwrap();
        let mut recursive_selection=p.selection.clone();let expected=recursive_selection.expected.as_mut().unwrap();expected.assignment_version=6;expected.revision=11;
        // Personal permissions alone do not replace any owning purchase.
        for id in ["asset","sale","room"]{let selection=recursive_selection.clone();let denied=erpx_service::field_support_read::read_selected_field_support(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),format!("recursive-ticket-{id}")).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.kind==erpx_server_shared::public::FailureKind::NotFound));}
        for package in ["assets","pos","pms"]{
            let mut tx=db.begin().await.unwrap();
            sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company',$1,1,'active',0,1,'Synthetic recursive source purchase','Synthetic public fixture',$2)").bind(package).bind(clock.now().get()).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT $1,company_id,package_id,'fixture-a',$1,NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$3 FROM company_package_grants WHERE company_id='company' AND package_id=$2").bind(format!("recursive-grant-{package}")).bind(package).bind(clock.now().get()).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        }
        // Historical stored origin v1 does not require the parent to stay at v1.
        sqlx::query("UPDATE operational_assets SET version=2,name='Current source asset' WHERE id='recursive-asset'").execute(db).await.unwrap();
        for id in ["asset","sale","room","refund-own"]{
            let selection=recursive_selection.clone();let read=erpx_service::field_support_read::read_selected_field_support(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),format!("recursive-ticket-{id}")).await.unwrap();assert_eq!(read.data["row"]["id"],format!("recursive-ticket-{id}"));assert_eq!(read.data["managedInFieldService"],false);assert_eq!(read.data["canManage"],false);
        }
        sqlx::query("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','null'::jsonb)::text,version=2 WHERE id='recursive-room'").execute(db).await.unwrap();
        let selection=recursive_selection.clone();erpx_service::field_support_read::read_selected_field_support(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),"recursive-ticket-room".into()).await.unwrap();
        for id in ["refund-other","foreign"]{
            let selection=recursive_selection.clone();let denied=erpx_service::field_support_read::read_selected_field_support(&p.review.authorizer,"acme.example.test",p.review.maker.clone(),move||Ok(selection),target.clone(),format!("recursive-ticket-{id}")).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.kind==erpx_server_shared::public::FailureKind::NotFound));
        }
        // Retained owning facts cannot be reused in another unit or after a
        // source/branch mutation. Roll back all intentional fixture tampering.
        let company=CompanyId::new("company").unwrap();let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let source=erpx_server_assets::public::record_source::read_record_source(&mut tx,&company,"recursive-asset").await.unwrap().unwrap();sqlx::query("UPDATE operational_assets SET name='Changed inside consumer unit' WHERE id='recursive-asset'").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
        let mut another=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *another).await.unwrap();assert!(source.require_current(&mut another).await.is_err());
        let pms=erpx_server_pms::public::record_source::read_record_source(&mut another,&company,"recursive-room").await.unwrap().unwrap();assert_eq!(pms.data().status,"open");
        let branch=erpx_server_organization::public::pms_source_branch::read_pms_source_branch(&mut another,&company,"branch").await.unwrap().unwrap();sqlx::query("UPDATE organization_nodes SET status='suspended' WHERE id='branch'").execute(&mut *another).await.unwrap();assert!(branch.require_current(&mut another).await.is_err());another.rollback().await.unwrap();
        // Distinct leased read DB and the actual command events above. This
        // proves projection/claim ownership, not whole Field browser acceptance.
        let write=db.clone();let now=clock.now().get()+10_000;
        projection_fixture::with_database(2,move|read|async move{
            use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
            use erpx_server_workflow::public::service_count_outbox::{claim,project,complete,fail,eligible};
            let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
            let files=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&files).await.unwrap();
            let quarantine=format!("({}) OR ({}) OR ({})",erpx_server_field_service::public::field_outbox::QUARANTINE,erpx_server_operations::public::field_outbox::QUARANTINE,erpx_server_collaboration::public::field_outbox::QUARANTINE);
            let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT count(*) FROM outbox_events e WHERE ");q.push(&quarantine);
            let owned:i64=q.build_query_scalar().fetch_one(&write).await.unwrap();assert!(owned>=12);
            let mut legacy=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT count(*) FROM outbox_events e WHERE e.command_id LIKE 'ops-field-%' AND NOT (");legacy.push(&quarantine).push(")");
            assert_eq!(legacy.build_query_scalar::<i64>().fetch_one(&write).await.unwrap(),0);
            // Omitting launcher filters cannot transfer native jobs back to TS.
            let direct=sqlx::query("UPDATE outbox_events SET lease_token='legacy-unfiltered' WHERE command_id='ops-field-open'").execute(&write).await.unwrap_err();assert_eq!(direct.as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));assert_eq!(direct.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));
            let mut c=write.acquire().await.unwrap();let first=claim(&mut c,&quarantine,"field-first",now).await.unwrap().unwrap();
            let native_evidence=format!("(({}) OR ({}) OR ({})) AND ({})",erpx_server_field_service::public::field_outbox::ELIGIBLE,erpx_server_operations::public::field_outbox::ELIGIBLE,erpx_server_collaboration::public::field_outbox::ELIGIBLE,erpx_server_workflow::public::field_outbox::COMMAND_EVIDENCE);
            assert!(eligible(&mut c,&native_evidence,&first).await.unwrap());
            let leaked:Option<String>=sqlx::query_scalar("SELECT nullif(current_setting('erpx.outbox_runtime',true),'')").fetch_one(&mut *c).await.unwrap();assert_eq!(leaked,None);

            {let mut tx=read.raw().begin().await.unwrap();project(&mut tx,&first,now).await.unwrap();tx.commit().await.unwrap();}
            // Crash after read commit, before write acknowledgement. The new
            // worker reclaims exactly the expired lease, with one read receipt.
            if let Some(early)=claim(&mut c,&quarantine,"field-early",now+1).await.unwrap(){assert_ne!(early.id,first.id);assert!(fail(&mut c,&early,now+1,"TEST_RETRY").await.unwrap());}
            assert!(erpx_service::field_dependency_worker::run_once(&write,read.raw(),"field-recovery",now+90_001).await.unwrap());
            assert!(!complete(&mut c,&first,now+90_002).await.unwrap());
            let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=$1").bind(&first.id).fetch_one(read.raw()).await.unwrap();assert_eq!(receipts,1);
            drop(c);
            let mut drained=1;
            while erpx_service::field_dependency_worker::run_once(&write,read.raw(),"field-drain",now+90_002).await.unwrap(){drained+=1;assert!(drained<=owned+1);}
            assert_eq!(drained,owned);
            let mut processed=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT count(*) FROM outbox_events e WHERE e.processed_at IS NOT NULL AND (");processed.push(&quarantine).push(")");assert_eq!(processed.build_query_scalar::<i64>().fetch_one(&write).await.unwrap(),owned);
            let receipt_count:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts").fetch_one(read.raw()).await.unwrap();assert_eq!(receipt_count,owned);
            let work_count:i64=sqlx::query_scalar("SELECT count(*) FROM work_items").fetch_one(read.raw()).await.unwrap();assert_eq!(work_count,0);
            // Unrecognized Field families are quarantined, never acknowledged
            // or handed to TS. Other owners' events are still claimable there.
            sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)VALUES('field-unknown','field-unknown','field.order.unrecognized','field_order','missing',1,'{}',1),('other-owner','other-owner','support.ticket.create','support_ticket','other',1,'{}',1)").execute(&write).await.unwrap();
            let mut time=now+100_000;
            for _ in 0..6{assert!(erpx_service::field_dependency_worker::run_once(&write,read.raw(),"field-invalid",time).await.unwrap());time+=60_001;}
            let row:(Option<i64>,Option<i64>,Option<String>)=sqlx::query_as("SELECT processed_at,dead_letter_at,last_error FROM outbox_events WHERE id='field-unknown'").fetch_one(&write).await.unwrap();assert_eq!(row.0,None);assert!(row.1.is_some());assert_eq!(row.2.as_deref(),Some("SOURCE_INVALID"));
            let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT e.id FROM outbox_events e WHERE e.processed_at IS NULL AND e.dead_letter_at IS NULL AND NOT (");q.push(&quarantine).push(") ORDER BY e.id");
            let legacy:Vec<String>=q.build_query_scalar().fetch_all(&write).await.unwrap();assert!(legacy.iter().any(|v|v=="other-owner"));assert!(!legacy.iter().any(|v|v=="field-unknown"));
        }).await;
    }).await;
}

#[tokio::test]
async fn native_field_push_cursor_fence_recovery_and_ack_are_partitioned() {
    use tower::ServiceExt;
    use erpx_server_collaboration::public::web_push::{claim, fanout, settle, subscriptions};
    facts::account::with_system(|system| async move {
        let p=facts::setup_workspace(system).await;
        let db=p.pool().raw();
        let central=facts::account::login(&p.review.system,"a@example.test",None).await;
        let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;
        let now=p.review.system.scenario.clock.now().get();
        use erpx_server_identity_access::public::push_session::{current_account,current_company};
        let mut control=p.review.system.control.raw().begin().await.unwrap();
        assert!(current_account(&mut control,&child.result.session_id,"fixture-a",facts::account::ACME,"development",now).await.unwrap());
        assert!(!current_account(&mut control,&child.result.session_id,"fixture-a",facts::account::ACME,"production",now).await.unwrap());
        assert!(!current_account(&mut control,&child.result.session_id,"fixture-b",facts::account::ACME,"development",now).await.unwrap());
        assert!(!current_account(&mut control,&child.result.session_id,"fixture-a",facts::account::NORTHSTAR,"development",now).await.unwrap());
        control.commit().await.unwrap();
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) SELECT $1,user_id,company_id,id,version,1,$2,$3 FROM assignments WHERE id='field-fixture-a'").bind(&child.result.session_id).bind(now+3600000).bind(now).execute(db).await.unwrap();
        let mut tx=db.begin().await.unwrap();
        assert!(current_company(&mut tx,&child.result.session_id,"fixture-a","company",now).await.unwrap());
        assert!(!current_company(&mut tx,&child.result.session_id,"fixture-a","foreign-company",now).await.unwrap());
        assert!(!current_company(&mut tx,&child.result.session_id,"fixture-a","company",now+3600000).await.unwrap());
        tx.rollback().await.unwrap();

        sqlx::raw_sql(r#"
INSERT INTO push_preferences(company_id,user_id,enabled,updated_at)VALUES('company','fixture-a',true,0);
INSERT INTO push_subscriptions(id,company_id,user_id,session_id,origin,inbox_path,endpoint_hash,sealed_subscription,created_at,updated_at,scan_at,cursor_at,cursor_id)VALUES('native-push','company','fixture-a','synthetic-session','https://acme.example.test','/work/inbox','synthetic-endpoint','synthetic-sealed',0,0,9999999999999,9999999999999,'legacy-ahead');
INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at)VALUES
('native-push-event','company','native-push-event','field_service_order','reserve-order',1,'work','Synthetic Field notification',10),
('legacy-push-event','company','legacy-push-event','other-owner','other',1,'work','Synthetic other notification',20);
INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at)VALUES('native-push-delivery','native-push-event','company','fixture-a',10),('legacy-push-delivery','legacy-push-event','company','fixture-a',20);
"#).execute(db).await.unwrap();
        let mut tx=db.begin().await.unwrap();
        let s=subscriptions(&mut tx).await.unwrap();assert_eq!(s.len(),1);
        assert_eq!(fanout(&mut tx,&s[0],30).await.unwrap(),1);
        assert_eq!(fanout(&mut tx,&s[0],31).await.unwrap(),0);
        tx.commit().await.unwrap();
        let cursor:(i64,i64)=sqlx::query_as("SELECT s.cursor_at,n.cursor_at FROM push_subscriptions s JOIN native_field_push_cursors n ON n.subscription_id=s.id WHERE s.id='native-push'").fetch_one(db).await.unwrap();
        assert_eq!(cursor,(9999999999999,10));
        let error=sqlx::query("UPDATE push_delivery_jobs SET attempts=99 WHERE subscription_id='native-push'").execute(db).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().constraint(),Some("native_field_push_runtime_required"));
        let error=sqlx::query("UPDATE push_delivery_jobs SET inbox_delivery_id='legacy-push-delivery' WHERE subscription_id='native-push'").execute(db).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().constraint(),Some("native_field_push_runtime_required"));
        #[cfg(feature="field-push-worker")]
        {
            use erpx_service::field_push_worker::{FieldPushTarget,PushTurn,run_once};
            let target=FieldPushTarget{control:p.review.system.control.raw().clone(),holding:db.clone(),audience:facts::account::ACME.into(),origin:"https://acme.example.test".into()};
            assert_eq!(run_once(&target,None,p.review.system.scenario.clock.as_ref()).await.unwrap(),PushTurn::Disabled);
            let untouched:(i32,Option<String>)=sqlx::query_as("SELECT attempts,lease_token FROM push_delivery_jobs WHERE subscription_id='native-push'").fetch_one(db).await.unwrap();assert_eq!(untouched,(0,None));
        }
        let mut tx=db.begin().await.unwrap();
        let (id,attempts)=claim(&mut tx,"claim-1",40).await.unwrap().unwrap();assert_eq!(attempts,1);tx.commit().await.unwrap();
        let mut tx=db.begin().await.unwrap();
        let row=erpx_server_collaboration::public::web_push::delivery(&mut tx,&id,"claim-1",41).await.unwrap().unwrap();assert!(row.visible);assert!(row.preferences.permits("work"));assert_eq!(row.subscription.inbox_path,"/work/inbox");
        // Acknowledgements are owned native commands, including in this queue
        // regression. Direct legacy updates remain fenced by the real schema.
        let mut document=runtime_configuration::document(&p.review.system.control,&p.review.system.holdings).await;
        document["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);document["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);
        let runtime=erpx_service::private_development_api::assemble_development_api(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&document.to_string()).unwrap(),p.review.system.scenario.clock.clone(),Arc::new(erpx_server_identity_access::public::SystemAuthenticationSecrets)).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",child.token.expose());
        for (read,key) in [(true,"native-push-notice-read"),(false,"native-push-notice-unread")] {
         let response=runtime.router().oneshot(axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/inbox/native-push-delivery/state").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").header("idempotency-key",key).body(axum::body::Body::from(json!({"read":read}).to_string())).unwrap()).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));
         assert_eq!(erpx_server_collaboration::public::web_push::delivery(&mut tx,&id,"claim-1",41).await.unwrap().unwrap().visible,!read);
        }
        tx.rollback().await.unwrap();runtime.close().await;
        let mut tx=db.begin().await.unwrap();assert!(claim(&mut tx,"claim-2",41).await.unwrap().is_none());tx.commit().await.unwrap();
        let mut tx=db.begin().await.unwrap();let recovered=claim(&mut tx,"claim-2",30041).await.unwrap().unwrap();assert_eq!(recovered,(id.clone(),2));
        assert!(!settle(&mut tx,&id,"claim-1",30042,Some(201),1).await.unwrap());
        assert!(settle(&mut tx,&id,"claim-2",30042,Some(503),2).await.unwrap());tx.commit().await.unwrap();
        let mut tx=db.begin().await.unwrap();assert!(claim(&mut tx,"claim-3",30043).await.unwrap().is_none());tx.commit().await.unwrap();
        let mut tx=db.begin().await.unwrap();assert_eq!(claim(&mut tx,"claim-3",40000).await.unwrap(),Some((id.clone(),3)));
        assert!(settle(&mut tx,&id,"claim-3",40001,Some(201),3).await.unwrap());tx.commit().await.unwrap();
        let result:(String,i32,i64)=sqlx::query_as("SELECT status,attempts,sent_at FROM push_delivery_jobs WHERE id=$1").bind(id).fetch_one(db).await.unwrap();assert_eq!(result,("sent".into(),3,40001));
        #[cfg(feature="field-push-worker")]
        {
            use erpx_service::field_push_worker::{FieldPushTarget,PushTurn,run_once};
            use erpx_server_collaboration::public::web_push::NativePushProvider;
            let target=FieldPushTarget{control:p.review.system.control.raw().clone(),holding:db.clone(),audience:facts::account::ACME.into(),origin:"https://acme.example.test".into()};
            let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../fixtures/collaboration/web-push/native-provider.v1.json")).unwrap();
            let provider=NativePushProvider::new("BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8","yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw","mailto:synthetic@example.test",fixture["dataKey"].as_str().unwrap()).unwrap();
            sqlx::query("UPDATE push_subscriptions SET session_id=$1,sealed_subscription='broken-synthetic-envelope' WHERE id='native-push'").bind(&child.result.session_id).execute(db).await.unwrap();
            sqlx::raw_sql("INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at)VALUES('quiet-native-event','company','quiet-native-event','field_service_order','reserve-order',1,'work','Synthetic quiet notification',50); INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at)VALUES('quiet-native-delivery','quiet-native-event','company','fixture-a',50);").execute(db).await.unwrap();
            let minute=(now.div_euclid(60000)+420).rem_euclid(1440) as i32;
            sqlx::query("UPDATE push_preferences SET quiet_start_minute=$1,quiet_end_minute=$2 WHERE company_id='company' AND user_id='fixture-a'").bind(minute).bind((minute+1)%1440).execute(db).await.unwrap();
            assert_eq!(run_once(&target,Some(&provider),p.review.system.scenario.clock.as_ref()).await.unwrap(),PushTurn::Enabled);
            let state:(String,i32,i64)=sqlx::query_as("SELECT status,attempts,next_attempt_at FROM push_delivery_jobs WHERE inbox_delivery_id='quiet-native-delivery'").fetch_one(db).await.unwrap();assert_eq!((state.0.as_str(),state.1),("pending",0));assert!(state.2>now);
            p.review.system.scenario.clock.advance(60001);
            assert_eq!(run_once(&target,Some(&provider),p.review.system.scenario.clock.as_ref()).await.unwrap(),PushTurn::Enabled);
            // Malformed local ciphertext fails preparation before any HTTP send.
            let state:(String,i32,Option<i32>)=sqlx::query_as("SELECT status,attempts,last_status FROM push_delivery_jobs WHERE inbox_delivery_id='quiet-native-delivery'").fetch_one(db).await.unwrap();assert_eq!(state,("dead".into(),1,Some(400)));
        }
        // A late commit below the diagnostic cursor is still observed; old
        // pre-registration Inbox and disabled categories never become pushes.
        sqlx::raw_sql("UPDATE push_subscriptions SET created_at=5 WHERE id='native-push'; UPDATE push_preferences SET notice_enabled=false WHERE company_id='company' AND user_id='fixture-a'; INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at)VALUES('late-native','company','late-native','field_service_order','reserve-order',1,'work','Synthetic',6),('old-native','company','old-native','field_service_order','reserve-order',1,'work','Synthetic',4),('muted-native','company','muted-native','field_service_order','reserve-order',1,'notice','Synthetic',7); INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at)VALUES('late-native','late-native','company','fixture-a',6),('old-native','old-native','company','fixture-a',4),('muted-native','muted-native','company','fixture-a',7);").execute(db).await.unwrap();
        let mut tx=db.begin().await.unwrap();
        let s=subscriptions(&mut tx).await.unwrap();
        assert_eq!(fanout(&mut tx,&s[0],60000).await.unwrap(),1);
        assert_eq!(fanout(&mut tx,&s[0],60001).await.unwrap(),0);
        tx.commit().await.unwrap();
        let status:String=sqlx::query_scalar("SELECT status FROM push_delivery_jobs WHERE inbox_delivery_id='muted-native'").fetch_one(db).await.unwrap();assert_eq!(status,"suppressed");
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM push_delivery_jobs WHERE inbox_delivery_id='old-native'").fetch_one(db).await.unwrap();assert_eq!(count,0);
        let at:i64=sqlx::query_scalar("SELECT cursor_at FROM native_field_push_cursors WHERE subscription_id='native-push'").fetch_one(db).await.unwrap();assert!(at>=10);
        sqlx::query("UPDATE push_preferences SET notice_enabled=true WHERE company_id='company' AND user_id='fixture-a'").execute(db).await.unwrap();
        let mut tx=db.begin().await.unwrap();let s=subscriptions(&mut tx).await.unwrap();assert_eq!(fanout(&mut tx,&s[0],60002).await.unwrap(),0);tx.commit().await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM push_delivery_jobs WHERE inbox_delivery_id='legacy-push-delivery'").fetch_one(db).await.unwrap();assert_eq!(count,0);
    }).await;
}

#[tokio::test]
async fn selected_field_customer_http_keeps_creation_receipt_and_current_confirmation_separate(){
 use tower::ServiceExt;
 facts::account::with_system(|system|async move{
  let p=facts::review::setup_service_customer_without_finance_references(system).await;let db=p.system.holdings[0].raw();let clock=p.system.scenario.clock.clone();
  sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_service_customer','Synthetic customer contributor',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_service_customer',1,'[\"records.read\",\"finance.read\",\"finance.write\",\"module.field_service.read\",\"module.finance.read\"]','[]','[\"holding\"]','synthetic',0); UPDATE assignments SET role='native_service_customer',modules_json='[\"field_service\",\"finance\"]',version=4 WHERE id='finance-fixture-a';").execute(db).await.unwrap();
  let central=facts::account::login(&p.system,"a@example.test",None).await;
  let child=facts::account::child(&p.system,&central,facts::account::ACME).await;
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','finance-fixture-a',4,9,$2,$3)").bind(&child.result.session_id).bind(clock.now().get()+3600000).bind(clock.now().get()).execute(db).await.unwrap();
  let mut grant=db.begin().await.unwrap();
  sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','service',1,'active',0,$1,1,'Synthetic Service-only startup','Synthetic public source',$2)").bind(clock.now().get()+120000).bind(clock.now().get()).execute(&mut *grant).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'native-customer-service-grant',company_id,package_id,'fixture-a','native-customer-service-grant',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(clock.now().get()).execute(&mut *grant).await.unwrap();grant.commit().await.unwrap();
  let cookie=format!("__Host-erpx-product={}",child.token.expose());
  use erpx_service::private_development_api::{DevelopmentApiConfiguration,assemble_development_api};
  use erpx_server_identity_access::public::SystemAuthenticationSecrets;
  let mut document=runtime_configuration::document(&p.system.control,&p.system.holdings).await;
  document["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);
  document["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);
  document["fieldServiceStockSetups"]=json!(["h001"]);
  let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&document.to_string()).unwrap(),clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let router=runtime.router();
  let path="/api/v1/holdings/acme/field-service/customer-preparation";
  let request=|method:&str,path:&str,body:String,key:&str,company:&str,origin:&str|axum::http::Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin",origin).header("cookie",&cookie).header("x-erpx-context-company",company).header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-revision","9").header("x-erpx-context-assignment-version","4").header("idempotency-key",key).body(axum::body::Body::from(body)).unwrap();
  let response=router.clone().oneshot(request("GET",path,String::new(),"native-customer-http","company","https://acme.example.test")).await.unwrap();
  assert_eq!(response.status(),200);
  let view:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(view["data"]["available"],true);assert_eq!(view["data"]["canCreate"],true);
  let body=json!({"expectedVersion":0,"contextKey":view["data"]["contextKey"],"input":{"kind":"customer","code":"NATIVE-FIELD-CUSTOMER","name":"Original native customer","currency":"usd","paymentTermsDays":30,"creditLimit":"12.34"}}).to_string();
  // Match JSON.parse's duplicate-key normalization before schema validation.
  let duplicate_body=body.replace("\"kind\":\"customer\"","\"kind\":\"supplier\",\"kind\":\"customer\"");
  let duplicate=router.clone().oneshot(request("POST",path,duplicate_body,"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(duplicate.status(),201);
  let mut created=None;
  for _ in 0..2{
   let response=router.clone().oneshot(request("POST",path,body.clone(),"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(response.status(),201);
   assert_eq!(response.headers()["cache-control"],"private, no-store");
   let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"]["currency"],"USD");assert_eq!(value["data"]["creditLimitMinor"],1234);
   if let Some(ref prior)=created{assert_eq!(prior,&value["data"]);}else{created=Some(value["data"].clone());}
  }
  let created=created.unwrap();let id=created["id"].as_str().unwrap();
  sqlx::query("UPDATE finance_parties SET status='blocked',name='Current blocked customer',version=2 WHERE id=$1").bind(id).execute(db).await.unwrap();
  let response=router.clone().oneshot(request("POST",&format!("{path}/confirmations"),body.clone(),"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(response.status(),200);
  let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"]["name"],"Current blocked customer");assert_eq!(value["data"]["status"],"blocked");
  let response=router.clone().oneshot(request("POST",path,body.clone(),"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(response.status(),201);
  let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(value["data"],created);
  for (key,company,origin,status)in[("native-customer-http","foreign-company","https://acme.example.test",403),("native-customer-http","company","https://foreign.example.test",403),("native-customer-stale","company","https://acme.example.test",409)]{
   let response=router.clone().oneshot(request("POST",path,body.clone(),key,company,origin)).await.unwrap();assert_eq!(response.status(),status);
  }
  let missing=router.clone().oneshot(request("POST",&format!("{path}/confirmations"),body.clone(),"native-customer-no-receipt","company","https://acme.example.test")).await.unwrap();assert_eq!(missing.status(),404);
  let supplier:serde_json::Value=serde_json::from_str(&body).unwrap();let mut supplier=supplier;supplier["input"]["kind"]=json!("supplier");
  let invalid=router.clone().oneshot(request("POST",path,supplier.to_string(),"native-customer-supplier","company","https://acme.example.test")).await.unwrap();assert_eq!(invalid.status(),422);let invalid:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(invalid.into_body(),65536).await.unwrap()).unwrap();assert_eq!(invalid["error"]["code"],"VALIDATION_FAILED");
  let mut changed:serde_json::Value=serde_json::from_str(&body).unwrap();changed["input"]["name"]=json!("Another original request");
  let conflict=router.clone().oneshot(request("POST",path,changed.to_string(),"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(conflict.status(),409);
  // Existing robustness assertions now exercise the native transaction,
  // including effects scheduled for the deferred-constraint boundary.
  let image_sql="SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text) FROM finance_parties t),(SELECT jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text) FROM command_receipts t),(SELECT jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text) FROM audit_events t),(SELECT jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text) FROM outbox_events t),(SELECT jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text) FROM assignments t),(SELECT jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text) FROM company_package_grants t))::text";
  let response=router.clone().oneshot(request("GET",path,String::new(),"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(response.status(),200);
  let view:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
  let mut fault_input:serde_json::Value=serde_json::from_str(&body).unwrap();fault_input["contextKey"]=view["data"]["contextKey"].clone();fault_input["input"]["code"]=json!("NATIVE-CUSTOMER-FAULT");let fault_body=fault_input.to_string();
  for (target,mutation,status) in [
   ("receipt","NEW.request_hash:=repeat('0',64)",409),
   ("party","UPDATE finance_parties SET name=name||' deferred drift' WHERE id=NEW.id",409),
   ("IAM","UPDATE assignments SET version=version+1,revoked_at=1 WHERE id='finance-fixture-a'",403),
   ("purchase","UPDATE company_package_grants SET version=version+1 WHERE company_id='company' AND package_id='service'",409),
  ]{
   let before:String=sqlx::query_scalar(image_sql).fetch_one(db).await.unwrap();
   // All SQL fragments are closed literals from the synthetic fault table above.
   let ddl=if target=="receipt"{format!("CREATE FUNCTION native_customer_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='native-customer-fault' THEN {mutation}; END IF; RETURN NEW; END $$; CREATE TRIGGER native_customer_fault BEFORE INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION native_customer_fault()") }else{format!("CREATE FUNCTION native_customer_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.code='NATIVE-CUSTOMER-FAULT' THEN {mutation}; END IF; RETURN NULL; END $$; CREATE CONSTRAINT TRIGGER native_customer_fault AFTER INSERT ON finance_parties DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION native_customer_fault()")};
   sqlx::raw_sql(sqlx::AssertSqlSafe(ddl)).execute(db).await.unwrap();
   let response=router.clone().oneshot(request("POST",path,fault_body.clone(),"native-customer-fault","company","https://acme.example.test")).await.unwrap();let actual=response.status();let detail:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(actual,status,"{target}: {detail}");
   let after:String=sqlx::query_scalar(image_sql).fetch_one(db).await.unwrap();assert_eq!(after,before,"{target} must roll back every owner effect");
   let table=if target=="receipt"{"command_receipts"}else{"finance_parties"};sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER native_customer_fault ON {table}; DROP FUNCTION native_customer_fault()"))).execute(db).await.unwrap();
  }
  let retry=router.clone().oneshot(request("POST",path,fault_body,"native-customer-fault","company","https://acme.example.test")).await.unwrap();assert_eq!(retry.status(),201);
  // The same native projector owns the new customer outbox. Legacy direct
  // claim attempts are fenced even when a launcher predicate is omitted.
  let error=sqlx::query("UPDATE outbox_events SET attempts=99 WHERE command_id='native-customer-http'").execute(db).await.unwrap_err();assert_eq!(error.as_database_error().unwrap().constraint(),Some("native_outbox_runtime_required"));
  let write=db.clone();let now=p.system.scenario.clock.now().get();
  projection_fixture::with_database(2,move|read|async move{
   use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
   let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&files).await.unwrap();
   for _ in 0..2{assert!(erpx_service::field_dependency_worker::run_once(&write,read.raw(),"native-customer-worker",now).await.unwrap());}
   let receipt:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=(SELECT last_event_id FROM projection_checkpoints WHERE projector='work-items')").fetch_one(read.raw()).await.unwrap();assert_eq!(receipt,1);
  }).await;
  let published:bool=sqlx::query_scalar("SELECT processed_at IS NOT NULL FROM outbox_events WHERE command_id='native-customer-http'").fetch_one(db).await.unwrap();assert!(published);
  let prerequisites:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_charts),(SELECT count(*) FROM finance_accounts),(SELECT count(*) FROM finance_periods),(SELECT count(*) FROM company_fx_rates),(SELECT count(*) FROM money_transaction_snapshots),(SELECT count(*) FROM company_package_grants WHERE package_id<>'service')").fetch_one(db).await.unwrap();assert_eq!(prerequisites,(0,0,0,0,0,0));
  clock.advance(120001);
  let response=router.clone().oneshot(request("GET",path,String::new(),"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(response.status(),200);
  let view:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(view["data"]["available"],true);assert_eq!(view["data"]["canCreate"],false);
  let historical=router.clone().oneshot(request("POST",path,body.clone(),"native-customer-http","company","https://acme.example.test")).await.unwrap();assert_eq!(historical.status(),201);
  let historical:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(historical.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(historical["data"],created);
  let denied=router.clone().oneshot(request("POST",path,body.clone(),"native-customer-expired","company","https://acme.example.test")).await.unwrap();assert_eq!(denied.status(),403);
  let effects:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_parties WHERE code='NATIVE-FIELD-CUSTOMER'),(SELECT count(*) FROM command_receipts WHERE command_id='native-customer-http'),(SELECT count(*) FROM audit_events WHERE command_id='native-customer-http'),(SELECT count(*) FROM outbox_events WHERE command_id='native-customer-http')").fetch_one(db).await.unwrap();assert_eq!(effects,(1,1,1,1));
  runtime.close().await;
 }).await;
}

#[tokio::test]
async fn scheduled_field_reservation_alerts_keep_original_phase_identity_and_rollback_sources(){
 use tower::ServiceExt;
 facts::account::with_system(|system|async move{
  let p=facts::setup_workspace(system).await;let db=p.pool().raw();let now=p.review.system.scenario.clock.now().get();
  sqlx::query("UPDATE field_service_part_reservations SET expires_at=$1,created_by='fixture-b' WHERE id='reserved-part'").bind(now+1000).execute(db).await.unwrap();
  assert_eq!(erpx_service::field_reservation_alerts::run_once(db,now).await.unwrap(),(2,0));
  assert_eq!(erpx_service::field_reservation_alerts::run_once(db,now).await.unwrap(),(0,0));
  let old:(String,i64)=sqlx::query_as("SELECT id,created_at FROM staff_notifications WHERE id LIKE 'inventory-reservation-expiring-%' AND user_id='fixture-a'").fetch_one(db).await.unwrap();assert_eq!(old.1,now);
  let recipients:Vec<(String,String)>=sqlx::query_as("SELECT user_id,record_type FROM staff_notifications WHERE id LIKE 'inventory-reservation-expiring-%' ORDER BY user_id").fetch_all(db).await.unwrap();assert_eq!(recipients,vec![("fixture-a".into(),"field_service_order".into()),("fixture-b".into(),"field_service_order".into())]);
  let expected_id:String=sqlx::query_scalar("SELECT 'inventory-reservation-expiring-'||md5('field:reserved-part:'||$1::bigint::text||':fixture-a')").bind(now+1000).fetch_one(db).await.unwrap();assert_eq!(old.0,expected_id);
  // Legacy callers cannot recreate or alter this producer's immutable rows.
  let error=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES('inventory-reservation-expired-legacy','company','fixture-a','field_service_order','issue-order',3,'Legacy bypass',0)").execute(db).await.unwrap_err();assert_eq!(error.as_database_error().unwrap().constraint(),Some("native_field_reservation_notice_required"));
  let error=sqlx::query("UPDATE staff_notifications SET message='Legacy rewrite' WHERE id=$1").bind(&old.0).execute(db).await.unwrap_err();assert_eq!(error.as_database_error().unwrap().constraint(),Some("native_field_reservation_notice_required"));
  // Field recipient/retention assertions moved from the retained legacy worker
  // suite keep the real native producer and acknowledgement ownership together.
  p.review.system.scenario.clock.advance(1);
  let central=facts::account::login(&p.review.system,"a@example.test",None).await;let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let mut document=runtime_configuration::document(&p.review.system.control,&p.review.system.holdings).await;document["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);document["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);
  let runtime=erpx_service::private_development_api::assemble_development_api(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&document.to_string()).unwrap(),p.review.system.scenario.clock.clone(),Arc::new(erpx_server_identity_access::public::SystemAuthenticationSecrets)).await.unwrap();
  let selected=runtime.router().oneshot(axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(axum::body::Body::from(r#"{"assignmentId":"field-fixture-a","assignmentVersion":3,"revision":0}"#)).unwrap()).await.unwrap();assert_eq!(selected.status(),200);
  let response=runtime.router().oneshot(axum::http::Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/field-service/inbox/{}/state",old.0)).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key","native-reservation-notice-state").header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").body(axum::body::Body::from(r#"{"read":true,"archived":true}"#)).unwrap()).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));runtime.close().await;

  assert_eq!(erpx_service::field_reservation_alerts::run_once(db,now+2).await.unwrap(),(0,0));
  let preserved:(Option<i64>,Option<i64>)=sqlx::query_as("SELECT read_at,archived_at FROM inbox_deliveries WHERE id=$1").bind(&old.0).fetch_one(db).await.unwrap();assert_eq!(preserved,(Some(now+1),Some(now+1)));
  sqlx::raw_sql("CREATE FUNCTION field_alert_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id LIKE 'inventory-reservation-expired-%' THEN UPDATE field_service_part_reservations SET released_milli=quantity_milli-consumed_milli WHERE id='reserved-part'; END IF; RETURN NULL; END $$; CREATE CONSTRAINT TRIGGER field_alert_fault AFTER INSERT ON staff_notifications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION field_alert_fault()").execute(db).await.unwrap();
  let result=erpx_service::field_reservation_alerts::run_once(db,now+1001).await;
  assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="FIELD_RESERVATION_ALERT_SOURCE_CHANGED"));
  let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM staff_notifications WHERE id LIKE 'inventory-reservation-expired-%'),(SELECT released_milli FROM field_service_part_reservations WHERE id='reserved-part')").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0));
  sqlx::raw_sql("DROP TRIGGER field_alert_fault ON staff_notifications; DROP FUNCTION field_alert_fault()").execute(db).await.unwrap();
  assert_eq!(erpx_service::field_reservation_alerts::run_once(db,now+1001).await.unwrap(),(0,2));
  assert_eq!(erpx_service::field_reservation_alerts::run_once(db,now+1002).await.unwrap(),(0,0));
  sqlx::query("UPDATE field_service_part_reservations SET released_milli=quantity_milli-consumed_milli,expires_at=$1 WHERE id='reserved-part'").bind(now-1).execute(db).await.unwrap();
  assert_eq!(erpx_service::field_reservation_alerts::run_once(db,now+1003).await.unwrap(),(0,0));
  sqlx::query("UPDATE field_service_part_reservations SET released_milli=0 WHERE id='reserved-part'").execute(db).await.unwrap();
  // A coherent historical terminal fixture, including the required source
  // event and Support projection; this is scheduled eligibility, not a native
  // completion journey or permission bypass.
  let mut terminal=db.begin().await.unwrap();
  sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) SELECT 'synthetic-terminal-alert',id,version+1,'complete',technician_id,'Synthetic terminal source','completed',technician_id,scheduled_at,'{}',$1 FROM field_service_orders WHERE id='issue-order'").bind(now+1003).execute(&mut *terminal).await.unwrap();
  sqlx::query("UPDATE field_service_orders SET status='completed',version=version+1,visit_completed_at=$1,completion_evidence='Synthetic completed',customer_confirmation='Synthetic confirmed',labor_minutes=0 WHERE id='issue-order'").bind(now+1003).execute(&mut *terminal).await.unwrap();terminal.commit().await.unwrap();
  assert_eq!(erpx_service::field_reservation_alerts::run_once(db,now+1003).await.unwrap(),(0,0));
 }).await;
}

#[tokio::test]
async fn native_account_role_selection_preserves_live_context_and_fences_atomic_preferences_audit() {
    use erpx_server_identity_access::public::work_context::HoldingWorkContextInspector;
    use erpx_server_shared::public::AssignmentId;
    use erpx_service::native_work_roles::{execute, WorkRoleAction};
    facts::account::with_system(|system| async move {
        let p = facts::review::setup_service_customer_without_finance_references(system).await;
        let db = p.system.holdings[0].raw();
        let clock = p.system.scenario.clock.clone();
        let inspector = HoldingWorkContextInspector::new(
            p.system.control.clone(), p.system.holdings[0].clone(), HoldingId::new("h001").unwrap(),
            facts::account::fixture::account::policy(), clock.clone(),
            facts::account::fixture::account::holding_registry(),
        ).unwrap();
        let request = {let mut r=p.maker_selection.clone();r.expected=None;r.mutation=false;r};
        let read = || WorkRoleAction::Restore;
        let select = |id:&str, revision:i64, remember:bool| WorkRoleAction::Select {
            assignment:AssignmentId::new(id).unwrap(), revision, expected_version:None, remember,
        };
        let initial=execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),read()).await.unwrap();
        assert!(initial.valid);assert_eq!(initial.selection.as_ref().unwrap().revision,7);
        assert_eq!(initial.options.len(),1);
        let session=initial.session_id.clone();
        // Re-selecting a live role is the original no-op: no preference, audit or revision churn.
        let same=execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),select("finance-fixture-a",7,true)).await.unwrap();
        assert_eq!(same.selection.unwrap().revision,7);
        let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM work_role_preferences WHERE user_id='fixture-a'),(SELECT count(*) FROM audit_events WHERE action='work.role.select')").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0));
        // Even a null/expired live pointer is preserved; another device's preference cannot replace it.
        sqlx::query("INSERT INTO work_role_preferences(user_id,company_id,assignment_id,assignment_version,updated_at)VALUES('fixture-a','company','finance-fixture-a',3,$1)").bind(clock.now().get()).execute(db).await.unwrap();
        sqlx::query("UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,expires_at=$1 WHERE session_id=$2").bind(clock.now().get()-1).bind(&session).execute(db).await.unwrap();
        let existing=execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),WorkRoleAction::Initialize).await.unwrap();
        assert!(!existing.valid);assert_eq!(existing.selection.unwrap().revision,7);
        let before:String=sqlx::query_scalar("SELECT to_jsonb(c)::text FROM active_work_contexts c WHERE session_id=$1").bind(&session).fetch_one(db).await.unwrap();
        facts::review::business(execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),select("finance-fixture-a",6,true)).await,"WORK_CONTEXT_CHANGED");
        facts::review::business(execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),select("finance-fixture-b",7,true)).await,"ASSIGNMENT_UNAVAILABLE");
        let after:String=sqlx::query_scalar("SELECT to_jsonb(c)::text FROM active_work_contexts c WHERE session_id=$1").bind(&session).fetch_one(db).await.unwrap();assert_eq!(before,after);
        // An actual audit failure must roll back both Identity's pointer and Work Projection's preference.
        sqlx::raw_sql("CREATE FUNCTION reject_native_role_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='work.role.select' THEN RAISE EXCEPTION 'synthetic role audit failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_native_role_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION reject_native_role_audit();").execute(db).await.unwrap();
        assert!(execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),select("finance-fixture-a",7,true)).await.is_err());
        let after:String=sqlx::query_scalar("SELECT to_jsonb(c)::text FROM active_work_contexts c WHERE session_id=$1").bind(&session).fetch_one(db).await.unwrap();assert_eq!(before,after);
        sqlx::raw_sql("DROP TRIGGER reject_native_role_audit ON audit_events; DROP FUNCTION reject_native_role_audit();").execute(db).await.unwrap();
        facts::review::business(execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),WorkRoleAction::Select{assignment:AssignmentId::new("finance-fixture-a").unwrap(),revision:7,expected_version:Some(2),remember:true}).await,"WORK_CONTEXT_CHANGED");
        let selected=execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),select("finance-fixture-a",7,true)).await.unwrap();assert!(selected.valid);assert_eq!(selected.selection.unwrap().revision,8);
        let audit:serde_json::Value=sqlx::query_scalar("SELECT authority_json::jsonb FROM audit_events WHERE action='work.role.select'").fetch_one(db).await.unwrap();assert_eq!(audit["assignmentVersion"],3);assert_eq!(audit["revision"],8);
        // Deferred tampering after the audit is observed before commit and the full unit rolls back.
        sqlx::query("UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL WHERE session_id=$1").bind(&session).execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION alter_native_role_after_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='work.role.select' THEN UPDATE assignments SET version=version+1 WHERE id='finance-fixture-a'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER alter_native_role_after_audit AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION alter_native_role_after_audit();").execute(db).await.unwrap();
        facts::review::business(execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),select("finance-fixture-a",8,true)).await,"WORK_CONTEXT_CHANGED");
        let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM assignments WHERE id='finance-fixture-a'),(SELECT revision FROM active_work_contexts WHERE session_id=$1),(SELECT count(*) FROM audit_events WHERE action='work.role.select')").bind(&session).fetch_one(db).await.unwrap();assert_eq!(facts,(3,8,1));
        sqlx::raw_sql("DROP TRIGGER alter_native_role_after_audit ON audit_events; DROP FUNCTION alter_native_role_after_audit();").execute(db).await.unwrap();
        // A genuinely new session may restore the saved original version, but a listing without a saved choice never auto-selects.
        sqlx::query("DELETE FROM active_work_contexts WHERE session_id=$1").bind(&session).execute(db).await.unwrap();
        let (first_restore,second_restore)=tokio::join!(
            execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),read()),
            execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),read())
        );
        for restored in [first_restore,second_restore]{let restored=restored.unwrap();assert!(restored.valid);assert_eq!(restored.selection.unwrap().revision,1);}
        let audits:i64=sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE action='work.role.select' AND record_version=1").fetch_one(db).await.unwrap();assert_eq!(audits,1);
        let purpose:String=sqlx::query_scalar("SELECT purpose FROM audit_events WHERE action='work.role.select' AND record_version=1").fetch_one(db).await.unwrap();assert_eq!(purpose,"Restored saved work assignment for a new session");
        sqlx::query("DELETE FROM active_work_contexts WHERE session_id=$1").bind(&session).execute(db).await.unwrap();
        sqlx::query("DELETE FROM work_role_preferences WHERE user_id='fixture-a'").execute(db).await.unwrap();
        let unselected=execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),read()).await.unwrap();assert!(unselected.selection.is_none());assert!(!unselected.valid);
        let initialized=execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),WorkRoleAction::Initialize).await.unwrap();assert!(initialized.valid);assert_eq!(initialized.selection.unwrap().revision,1);
        sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='finance-fixture-a'").bind(clock.now().get()).execute(db).await.unwrap();
        facts::review::business(execute(&inspector,"acme.example.test",p.maker.clone(),request.clone(),clock.clone(),read()).await,"WORKSPACE_UNAVAILABLE");
        assert!(execute(&inspector,"northstar.example.test",p.maker.clone(),request,clock,read()).await.is_err());
    }).await;
}

#[tokio::test]
async fn native_role_http_uses_normal_child_session_and_current_company_in_the_assembled_runtime(){
 use tower::ServiceExt;
 facts::account::with_system(|system|async move{
  let p=facts::review::setup_service_customer_without_finance_references(system).await;
  let clock=p.system.scenario.clock.clone();
  let central=facts::account::login(&p.system,"a@example.test",None).await;
  let child=facts::account::child(&p.system,&central,facts::account::ACME).await;
  let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let mut document=runtime_configuration::document(&p.system.control,&p.system.holdings).await;
  document["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);
  let runtime=erpx_service::private_development_api::assemble_development_api(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&document.to_string()).unwrap(),clock.clone(),Arc::new(erpx_server_identity_access::public::SystemAuthenticationSecrets)).await.unwrap();
  let router=runtime.router();let path="/api/v1/holdings/acme/field-service/work-roles";
  let company_request=||axum::http::Request::builder().uri("/api/v1/holdings/acme/field-service/work-roles/companies").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","untrusted-foreign-company").body(axum::body::Body::empty()).unwrap();
  let company_reply=router.clone().oneshot(company_request()).await.unwrap();assert_eq!(company_reply.status(),200);
  let company_state:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(company_reply.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(company_state["data"]["userId"],"fixture-a");assert_eq!(company_state["data"]["sessionId"],child.result.session_id);assert_eq!(company_state["data"]["companies"],json!([{"id":"company","slug":"acme","name":"Synthetic Acme"}]));
  // Account identity alone cannot reveal a Company after its grant expires.
  sqlx::query("UPDATE assignments SET valid_until=$1 WHERE id='finance-fixture-a'").bind(clock.now().get()-1).execute(p.system.holdings[0].raw()).await.unwrap();
  let company_reply=router.clone().oneshot(company_request()).await.unwrap();assert_eq!(company_reply.status(),200);let company_state:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(company_reply.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(company_state["data"]["companies"],json!([]));
  sqlx::query("UPDATE assignments SET valid_until=$1 WHERE id='finance-fixture-a'").bind(clock.now().get()+3600000).execute(p.system.holdings[0].raw()).await.unwrap();


  let preferences_request=|method:&str,body:&str,origin:&str|axum::http::Request::builder().method(method).uri("/api/v1/holdings/acme/field-service/work-roles/preferences").header("host","acme.example.test").header("cookie",&cookie).header("origin",origin).body(axum::body::Body::from(body.to_owned())).unwrap();
  let pref=router.clone().oneshot(preferences_request("GET","","https://acme.example.test")).await.unwrap();assert_eq!(pref.status(),200);let pref:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(pref.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(pref["data"]["userId"],"fixture-a");assert_eq!(pref["data"]["company"],serde_json::Value::Null);
  sqlx::query("INSERT INTO account_workspace_preferences(user_id,platform_assignment_id,updated_at)VALUES('fixture-a','synthetic-retained-platform',0)ON CONFLICT(user_id)DO UPDATE SET platform_assignment_id=excluded.platform_assignment_id").execute(p.system.control.raw()).await.unwrap();
  let selected_company=router.clone().oneshot(preferences_request("POST",r#"{"companyId":"company"}"#,"https://acme.example.test")).await.unwrap();assert_eq!(selected_company.status(),200);let pref:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(selected_company.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(pref["data"]["company"]["companyId"],"company");assert_eq!(pref["data"]["workspace"]["workspacePath"],"/holdings/acme/home");assert_eq!(pref["data"]["workspace"]["platformAssignmentId"],"synthetic-retained-platform");
  let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT to_jsonb(w) FROM account_workspace_preferences w WHERE user_id='fixture-a'),(SELECT to_jsonb(c) FROM account_company_preferences c WHERE user_id='fixture-a' AND holding_id='h001'))").fetch_one(p.system.control.raw()).await.unwrap();
  let denied=router.clone().oneshot(preferences_request("POST",r#"{"companyId":"foreign-company"}"#,"https://acme.example.test")).await.unwrap();assert_eq!(denied.status(),403);
  let denied=router.clone().oneshot(preferences_request("POST",r#"{"companyId":"company"}"#,"https://northstar.example.test")).await.unwrap();assert_eq!(denied.status(),403);

  sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('native-preference-branch','branch','native-preference-branch','Synthetic current branch','company','company','native-preference-branch','active')").execute(p.system.holdings[0].raw()).await.unwrap();
  let scoped_request=|authenticated:bool|{let mut request=axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles/preferences").header("host","acme.example.test").header("origin","https://acme.example.test").header("x-erpx-context-company","company").header("x-erpx-workspace-branch","native-preference-branch");if authenticated{request=request.header("cookie",&cookie);}request.body(axum::body::Body::from("invalid-body-must-not-skip-lineage-authentication")).unwrap()};
  let denied=router.clone().oneshot(scoped_request(false)).await.unwrap();assert_eq!(denied.status(),401);
  let denied=router.clone().oneshot(scoped_request(true)).await.unwrap();assert_eq!(denied.status(),409);let denied:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(denied.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(denied["error"]["code"],"COMPANY_DERIVED_FROM_LINEAGE");
  // Both Control preference rows and current Account sources are retained after
  // deferred effects; a failed save must never leave a partial navigation choice.
  sqlx::raw_sql("CREATE FUNCTION poison_native_company_preference() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE account_workspace_preferences SET workspace_path='/unexpected-path' WHERE user_id=NEW.user_id; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER poison_native_company_preference AFTER INSERT OR UPDATE ON account_company_preferences DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION poison_native_company_preference();").execute(p.system.control.raw()).await.unwrap();
  let failed=router.clone().oneshot(preferences_request("POST",r#"{"companyId":"company"}"#,"https://acme.example.test")).await.unwrap();assert_eq!(failed.status(),409);
  let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT to_jsonb(w) FROM account_workspace_preferences w WHERE user_id='fixture-a'),(SELECT to_jsonb(c) FROM account_company_preferences c WHERE user_id='fixture-a' AND holding_id='h001'))").fetch_one(p.system.control.raw()).await.unwrap();assert_eq!(before,after);
  sqlx::raw_sql("DROP TRIGGER poison_native_company_preference ON account_company_preferences;DROP FUNCTION poison_native_company_preference();").execute(p.system.control.raw()).await.unwrap();
  let original_name:String=sqlx::query_scalar("SELECT name FROM \"user\" WHERE id='fixture-a'").fetch_one(p.system.control.raw()).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION alter_native_preference_account() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE \"user\" SET name='unexpected-deferred-account-name' WHERE id=NEW.user_id; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER alter_native_preference_account AFTER INSERT OR UPDATE ON account_company_preferences DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION alter_native_preference_account();").execute(p.system.control.raw()).await.unwrap();
  let failed=router.clone().oneshot(preferences_request("POST",r#"{"companyId":"company"}"#,"https://acme.example.test")).await.unwrap();assert_eq!(failed.status(),401);
  let name:String=sqlx::query_scalar("SELECT name FROM \"user\" WHERE id='fixture-a'").fetch_one(p.system.control.raw()).await.unwrap();assert_eq!(name,original_name);
  sqlx::raw_sql("DROP TRIGGER alter_native_preference_account ON account_company_preferences;DROP FUNCTION alter_native_preference_account();").execute(p.system.control.raw()).await.unwrap();
  let request=|method:&str,body:&str,company:&str,origin:&str,authenticated:bool|{let mut b=axum::http::Request::builder().method(method).uri(path).header("host","acme.example.test").header("x-erpx-context-company",company).header("x-erpx-context-assignment","forged-picker-role").header("x-erpx-context-revision","999");if authenticated{b=b.header("cookie",&cookie);}if !origin.is_empty(){b=b.header("origin",origin);}b.body(axum::body::Body::from(body.to_owned())).unwrap()};
  let first=router.clone().oneshot(request("GET","","company","",true)).await.unwrap();assert_eq!(first.status(),200);
  let state:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(first.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(state["data"]["selection"],serde_json::Value::Null);assert_eq!(state["data"]["valid"],false);assert_eq!(state["data"]["options"][0]["id"],"finance-fixture-a");
  let denied=router.clone().oneshot(request("POST","{\"action\":\"initialize\"}","company","https://northstar.example.test",true)).await.unwrap();assert_eq!(denied.status(),403);
  let denied=router.clone().oneshot(request("GET","","company","",false)).await.unwrap();assert_eq!(denied.status(),401);
  let chosen=router.clone().oneshot(request("POST","{\"assignmentId\":\"finance-fixture-a\",\"revision\":0}","company","https://acme.example.test",true)).await.unwrap();assert_eq!(chosen.status(),200);assert_eq!(chosen.headers()["cache-control"],"private, no-store");
  let state:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(chosen.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(state["data"]["selection"]["revision"],1);assert_eq!(state["data"]["valid"],true);
  let stale=router.clone().oneshot(request("POST","{\"assignmentId\":\"finance-fixture-a\",\"revision\":0}","company","https://acme.example.test",true)).await.unwrap();assert_eq!(stale.status(),409);
  let denied=router.clone().oneshot(request("POST","{\"assignmentId\":\"finance-fixture-b\",\"revision\":1}","company","https://acme.example.test",true)).await.unwrap();assert_eq!(denied.status(),403);
  let foreign=router.clone().oneshot(request("GET","","foreign-company","",true)).await.unwrap();assert!(!foreign.status().is_success());
  let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM active_work_contexts WHERE session_id=$1),(SELECT count(*) FROM audit_events WHERE action='work.role.select')").bind(&child.result.session_id).fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(effects,(1,1));
  // SSR access resolves structural selectors in Rust; it never initializes a
  // role or returns effective authority. Context binds the current native role.
  let access_request=|mode:&str,company:Option<&str>,branch:Option<&str>,revision:Option<&str>|{
   let mut b=axum::http::Request::builder().uri(format!("{path}/{mode}")).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-api","false").header("x-erpx-destination","home");
   if let Some(c)=company{b=b.header("x-erpx-requested-company",c);}
   if let Some(bslug)=branch{b=b.header("x-erpx-workspace-branch",bslug);}
   if let Some(rev)=revision{b=b.header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision",rev);}
   b.body(axum::body::Body::empty()).unwrap()
  };
  let missing=router.clone().oneshot(access_request("access",None,None,None)).await.unwrap();assert_eq!(missing.status(),409);
  let access=router.clone().oneshot(access_request("access",Some("company"),None,None)).await.unwrap();assert_eq!(access.status(),200);let access:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(access.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(access["data"]["companySlug"],"acme");assert_eq!(access["data"]["context"],serde_json::Value::Null);assert!(access["data"]["workspaces"].as_array().unwrap().iter().any(|w|w["branchSlug"]=="native-preference-branch"));
  let context=router.clone().oneshot(access_request("context",Some("company"),None,Some("1"))).await.unwrap();assert_eq!(context.status(),200);let context:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(context.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(context["data"]["context"]["iam"]["roleRevision"].as_i64().unwrap()>0,true);assert!(context["data"]["context"]["iam"]["permissions"].is_array());assert_eq!(context["data"]["selection"]["revision"],1);
  let branch=router.clone().oneshot(access_request("context",None,Some("native-preference-branch"),Some("1"))).await.unwrap();assert_eq!(branch.status(),200);let branch:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(branch.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(branch["data"]["branchName"],"Synthetic current branch");assert_eq!(branch["data"]["companyId"],"company");
  let stale=router.clone().oneshot(access_request("context",Some("company"),None,Some("0"))).await.unwrap();assert_eq!(stale.status(),409);
  // A typed Company id must not be accepted as the current Company's slug.
  let foreign=router.clone().oneshot(access_request("access",Some("acme"),None,None)).await.unwrap();assert!(!foreign.status().is_success());
  let workspace_request=|path:&str,revision:&str|axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles/workspace-preference").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision",revision).body(axum::body::Body::from(json!({"path":path,"companyId":"company"}).to_string())).unwrap();
  let branch_path="/holdings/acme/branches/native-preference-branch/home";
  let saved=router.clone().oneshot(workspace_request(branch_path,"1")).await.unwrap();assert_eq!(saved.status(),200);let saved:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(saved.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(saved["data"]["workspace"]["workspacePath"],branch_path);assert_eq!(saved["data"]["workspace"]["platformAssignmentId"],"synthetic-retained-platform");
  let stale=router.clone().oneshot(workspace_request("/holdings/acme/home","0")).await.unwrap();assert_eq!(stale.status(),409);
  let foreign=router.clone().oneshot(workspace_request("/holdings/northstar/home","1")).await.unwrap();assert_eq!(foreign.status(),422);
  sqlx::query("UPDATE assignments SET version=4 WHERE id='finance-fixture-a'").execute(p.system.holdings[0].raw()).await.unwrap();
  let stale=router.clone().oneshot(workspace_request("/holdings/acme/home","1")).await.unwrap();assert_eq!(stale.status(),409);
  let unchanged:String=sqlx::query_scalar("SELECT workspace_path FROM account_workspace_preferences WHERE user_id='fixture-a'").fetch_one(p.system.control.raw()).await.unwrap();assert_eq!(unchanged,branch_path);
  runtime.close().await;
 }).await;
}

#[tokio::test]
async fn native_field_inbox_candidates_keep_source_visibility_and_seek_past_hidden_rows(){
 use tower::ServiceExt;
 facts::account::with_system(|system|async move{
  let p=facts::setup_workspace(system).await;let clock=p.review.system.scenario.clock.clone();
  sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_linked_notifications','Synthetic native Field notifications',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_linked_notifications',1,'[\"records.read\",\"records.write\",\"records.review\",\"module.field_service.read\",\"module.operations.read\",\"module.collaboration.read\"]','[]','[\"holding\"]','synthetic',0);UPDATE assignments SET role='native_linked_notifications',modules_json='[\"field_service\",\"operations\",\"collaboration\"]',version=4 WHERE id='field-fixture-a';").execute(p.pool().raw()).await.unwrap();
  // Explicit historical owner rows: identical cross-owner IDs must retain
  // distinct witnesses, and a completed card remains readable outside Work.
  sqlx::query("INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,status,created_by,reviewed_by,created_at,updated_at)VALUES('native-inbox-policy','company','field','Synthetic Field queue',1,'[\"field_service_order\"]',60,'published','fixture-a','fixture-b',$1,$1)").bind(clock.now().get()).execute(p.pool().raw()).await.unwrap();
  sqlx::query("INSERT INTO operations_exceptions(id,company_id,branch_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,owner_id,due_at,created_by,created_at,updated_at)VALUES('native-linked-source','company','branch','field','field_service_order','reserve-order',1,'{}','native-inbox-policy','{}','Synthetic Field exception','Synthetic source','normal','fixture-a',$1,'fixture-a',$1,$1)").bind(clock.now().get()).execute(p.pool().raw()).await.unwrap();
  sqlx::query("INSERT INTO collaboration_action_cards(id,company_id,branch_id,source_type,source_id,source_version,title,owner_id,due_at,status,completion_evidence,created_by,version,created_at,updated_at)VALUES('native-linked-source','company','branch','field_service_order','reserve-order',1,'Synthetic completed Field action','fixture-a',$1,'completed','Synthetic retained completion','fixture-a',2,$1,$1)").bind(clock.now().get()).execute(p.pool().raw()).await.unwrap();
  let central=facts::account::login(&p.review.system,"a@example.test",None).await;
  let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;
  let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let mut document=runtime_configuration::document(&p.review.system.control,&p.review.system.holdings).await;
  document["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);
  document["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);
  let runtime=erpx_service::private_development_api::assemble_development_api(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&document.to_string()).unwrap(),clock.clone(),Arc::new(erpx_server_identity_access::public::SystemAuthenticationSecrets)).await.unwrap();let router=runtime.router();
  let selected=router.clone().oneshot(axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(axum::body::Body::from(r#"{"assignmentId":"field-fixture-a","assignmentVersion":4,"revision":0}"#)).unwrap()).await.unwrap();assert_eq!(selected.status(),200);
  // Historical source versions do not replace current source visibility. Long
  // native notice ids must remain usable as seek cursors.
  let long="n".repeat(201);let now=clock.now().get();
  for(id,source,user,message,created)in[("native-inbox-hidden","not-an-order","fixture-a","PRIVATE_UNRESOLVED_NOTIFICATION",now-1),(long.as_str(),"reserve-order","fixture-a","Synthetic current Field notice",now-2),("native-inbox-next","reserve-order","fixture-a","Synthetic older Field notice",now-3),("native-inbox-other-account","reserve-order","fixture-b","PRIVATE_OTHER_ACCOUNT_NOTIFICATION",now)]{
   sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,'company',$2,'field_service_order',$3,1,$4,$5)").bind(id).bind(user).bind(source).bind(message).bind(created).execute(p.pool().raw()).await.unwrap();
  }
  for(id,kind,message,created)in[("native-linked-ops-notice","operational_exception","Synthetic Field exception notice",now-4),("native-linked-card-notice","collaboration_action","Synthetic completed action notice",now-5)]{
   sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,'company','fixture-a',$2,'native-linked-source',1,$3,$4)").bind(id).bind(kind).bind(message).bind(created).execute(p.pool().raw()).await.unwrap();
  }
  for(id,created)in[("native-time-nine",9_i64),("native-time-ten",10_i64)]{sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,'company','fixture-a','field_service_order','reserve-order',1,$3,$2)").bind(id).bind(created).bind(format!("Synthetic historical timestamp {id}")).execute(p.pool().raw()).await.unwrap();}
  sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES('native-support-notice','company','fixture-a','support_ticket','ticket-reserve-order',1,'Synthetic Field-managed Support notice',$1)").bind(now-6).execute(p.pool().raw()).await.unwrap();
  let classified:bool=sqlx::query_scalar("SELECT erpx_field_push_source('support_ticket','ticket-reserve-order','company')").fetch_one(p.pool().raw()).await.unwrap();assert!(classified);
  let path="/api/v1/holdings/acme/field-service/inbox-candidates";
  let request=|query:&str,origin:&str,company:&str|axum::http::Request::builder().uri(format!("{path}?{query}")).header("host","acme.example.test").header("origin",origin).header("cookie",&cookie).header("x-erpx-context-company",company).header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","4").header("x-erpx-context-revision","1").body(axum::body::Body::empty()).unwrap();
  let page=router.clone().oneshot(request("limit=1","https://acme.example.test","company")).await.unwrap();assert_eq!(page.status(),200);let body=axum::body::to_bytes(page.into_body(),2*1024*1024).await.unwrap();assert!(!String::from_utf8_lossy(&body).contains("PRIVATE_"));let page:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(page["data"].as_array().unwrap().len(),1);assert_eq!(page["data"][0]["id"],long);assert_eq!(page["data"][0]["source_version"],1);assert_eq!(page["data"][0]["native_field_item"]["href"],"/field-service/reserve-order");assert_eq!(page["context"]["revision"],1);
  let query=format!("limit=1&beforeAt={}&beforeId={}",now-2,long);let next=router.clone().oneshot(request(&query,"https://acme.example.test","company")).await.unwrap();assert_eq!(next.status(),200);let next:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(next.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(next["data"][0]["id"],"native-inbox-next");
  let foreign=router.clone().oneshot(request("limit=1","https://northstar.example.test","company")).await.unwrap();assert_eq!(foreign.status(),403);
  let foreign=router.clone().oneshot(request("limit=1","https://acme.example.test","foreign-company")).await.unwrap();assert!(!foreign.status().is_success());
  let linked=router.clone().oneshot(request("all=true&limit=100","https://acme.example.test","company")).await.unwrap();assert_eq!(linked.status(),200);let linked:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(linked.into_body(),2*1024*1024).await.unwrap()).unwrap();let rows=linked["data"].as_array().unwrap();let ops=rows.iter().find(|v|v["id"]=="native-linked-ops-notice").unwrap();let card=rows.iter().find(|v|v["id"]=="native-linked-card-notice").unwrap();assert_eq!(ops["native_field_item"]["href"],"/operations/native-linked-source");assert_eq!(card["native_field_item"]["href"],"/collaboration");assert_eq!(card["source_version"],1);assert!(card["native_field_item"]["source"].is_null());
  // Executive source reads and aggregate decisions are native too. Default
  // counts are complete; evidence paging never becomes a partial aggregate.
  let report_request=|query:&str,origin:&str|{let mut req=request("",origin,"company");*req.uri_mut()=format!("/api/v1/holdings/acme/field-service/executive?{query}").parse().unwrap();req};
  let dates="fromAt=0&toAt=4102444800000";
  for (metric,breakdown) in [("open-service-orders","default"),("field-visit-completion","default"),("service-billing-releases","month"),("open-service-orders","branch")] {
   let response=router.clone().oneshot(report_request(&format!("metric={metric}&breakdown={breakdown}&{dates}"),"https://acme.example.test")).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));let body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let raw=body["data"]["raw"].as_array().unwrap();
   match metric {
    "field-visit-completion"=>assert_eq!(raw,&vec![json!({"label":"Field visits","numerator":"0","denominator":"2"})]),
    "service-billing-releases"=>{assert!(raw.is_empty());assert!(body["data"]["sources"].as_array().unwrap().is_empty());},
    _=>{assert_eq!(raw.iter().map(|r|r["value"].as_str().unwrap().parse::<i64>().unwrap()).sum::<i64>(),2);if breakdown=="branch"{let name:String=sqlx::query_scalar("SELECT coalesce((SELECT display_name FROM organization_nodes WHERE id='branch' AND company_id='company'),'Branch')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(raw,&vec![json!({"label":name,"value":"2"})]);}}
   }
  }
  let response=router.clone().oneshot(report_request(&format!("metric=field-visit-completion&{dates}&evidenceLimit=1"),"https://acme.example.test")).await.unwrap();assert_eq!(response.status(),200);let body:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();let rows=body["data"]["evidence"].as_array().unwrap();assert_eq!(rows.len(),2);assert!(rows.iter().all(|r|r.as_object().unwrap().len()==5));let first=&rows[0];
  let response=router.clone().oneshot(report_request(&format!("metric=field-visit-completion&{dates}&evidenceLimit=1&beforeAt={}&beforeId={}",first["scheduled_at"],first["id"].as_str().unwrap()),"https://acme.example.test")).await.unwrap();assert_eq!(response.status(),200);let body:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(body["data"]["evidence"].as_array().unwrap(),&rows[1..]);
  let forbidden=router.clone().oneshot(report_request(&format!("metric=open-service-orders&{dates}"),"https://northstar.example.test")).await.unwrap();assert_eq!(forbidden.status(),403);
  let invalid=router.clone().oneshot(report_request(&format!("metric=field-visit-completion&{dates}&evidenceLimit=0"),"https://acme.example.test")).await.unwrap();assert_eq!(invalid.status(),422);
  let state_request=|id:&str,key:&str,body:&str,revision:&str|axum::http::Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/field-service/inbox/{id}/state")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","4").header("x-erpx-context-revision",revision).body(axum::body::Body::from(body.to_owned())).unwrap();
  let ack=router.clone().oneshot(state_request(&long,"native-inbox-read",r#"{"read":true}"#,"1")).await.unwrap();let status=ack.status();let ack=axum::body::to_bytes(ack.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&ack));let ack:serde_json::Value=serde_json::from_slice(&ack).unwrap();assert_eq!(ack["data"]["id"],long);assert_eq!(ack["data"]["readAt"],now);
  // An exact retry retains the original acknowledgement time.
  let retry=router.clone().oneshot(state_request(&long,"native-inbox-read",r#"{"read":true}"#,"1")).await.unwrap();assert_eq!(retry.status(),200);let retry:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(retry.into_body(),65536).await.unwrap()).unwrap();assert_eq!(retry["data"],ack["data"]);
  for(id,key)in[("native-linked-ops-notice","native-inbox-ops"),("native-linked-card-notice","native-inbox-card")]{
   let response=router.clone().oneshot(state_request(id,key,r#"{"read":true}"#,"1")).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));
   let denied=sqlx::query("UPDATE staff_notifications SET read_at=NULL WHERE id=$1").bind(id).execute(p.pool().raw()).await.unwrap_err();assert_eq!(denied.as_database_error().and_then(|v|v.constraint()),Some("native_field_staff_state_required"));
  }
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='native-inbox-read'),(SELECT count(*) FROM audit_events WHERE command_id='native-inbox-read'),(SELECT count(*) FROM outbox_events WHERE command_id='native-inbox-read')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(counts,(1,1,1));
  let times:(Option<i64>,Option<i64>)=sqlx::query_as("SELECT d.read_at,n.read_at FROM inbox_deliveries d JOIN staff_notifications n ON n.id=d.id WHERE d.id=$1").bind(&long).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(times,(Some(now),Some(now)));
  assert_eq!(router.clone().oneshot(state_request(&long,"native-inbox-read",r#"{"read":false}"#,"1")).await.unwrap().status(),409);
  assert_eq!(router.clone().oneshot(state_request(&long,"native-inbox-stale",r#"{"read":false}"#,"0")).await.unwrap().status(),409);
  assert_eq!(router.clone().oneshot(state_request("native-inbox-other-account","native-inbox-foreign",r#"{"read":true}"#,"1")).await.unwrap().status(),404);
  assert_eq!(router.clone().oneshot(state_request("native-inbox-hidden","native-inbox-hidden",r#"{"read":true}"#,"1")).await.unwrap().status(),404);
  assert_eq!(router.clone().oneshot(state_request(&long,"native-inbox-invalid",r#"{"read":true,"archived":null}"#,"1")).await.unwrap().status(),422);
  let archive=router.clone().oneshot(state_request(&long,"native-inbox-archive",r#"{"archived":true}"#,"1")).await.unwrap();assert_eq!(archive.status(),200);
  let unread=router.clone().oneshot(request("limit=100","https://acme.example.test","company")).await.unwrap();let unread:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(unread.into_body(),65536).await.unwrap()).unwrap();assert!(!unread["data"].as_array().unwrap().iter().any(|v|v["id"]==long));
  let staff=router.clone().oneshot(request("projection=staff-list&all=true&limit=100","https://acme.example.test","company")).await.unwrap();let status=staff.status();let bytes=axum::body::to_bytes(staff.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));let staff:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let rows=staff["data"].as_array().unwrap();assert!(rows.iter().any(|v|v["id"]==long&&v["archived_at"].is_i64()));let ten=rows.iter().position(|v|v["id"]=="native-time-ten").unwrap();let nine=rows.iter().position(|v|v["id"]=="native-time-nine").unwrap();assert!(ten<nine);
  let staff=router.clone().oneshot(request("projection=staff-page&all=true&limit=100","https://acme.example.test","company")).await.unwrap();assert_eq!(staff.status(),200);let staff:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(staff.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(staff["data"][0]["id"],"native-time-nine");
  let seek=format!("projection=staff-page&all=true&reverse=true&beforeAt=0&beforeKey={}&beforeId=native-inbox-next&limit=1",now-3);let previous=router.clone().oneshot(request(&seek,"https://acme.example.test","company")).await.unwrap();assert_eq!(previous.status(),200);let previous:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(previous.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(previous["data"][0]["id"],long);
  // Neither generic SQL callers nor the legacy Staff read writer can mutate
  // this partition after the candidate migration, even without route filters.
  for sql in ["UPDATE inbox_deliveries SET read_at=NULL WHERE id=$1","UPDATE staff_notifications SET read_at=NULL WHERE id=$1"]{let error=sqlx::query(sql).bind(&long).execute(p.pool().raw()).await.unwrap_err();assert_eq!(error.as_database_error().and_then(|v|v.code()).as_deref(),Some("23514"));}
  let a=router.clone().oneshot(state_request(&long,"native-inbox-reopen",r#"{"archived":false,"read":false}"#,"1"));let b=router.clone().oneshot(state_request(&long,"native-inbox-reopen",r#"{"archived":false,"read":false}"#,"1"));let(a,b)=tokio::join!(a,b);assert_eq!(a.unwrap().status(),200);assert_eq!(b.unwrap().status(),200);
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='native-inbox-reopen'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,1);
  sqlx::raw_sql("CREATE FUNCTION poison_native_inbox_after_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='native-inbox-poison' THEN UPDATE inbox_events SET preview='POISONED_NATIVE_MESSAGE' WHERE id=(SELECT event_id FROM inbox_deliveries WHERE id=NEW.record_id); END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER poison_native_inbox_after_audit AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION poison_native_inbox_after_audit();").execute(p.pool().raw()).await.unwrap();
  let poisoned=router.clone().oneshot(state_request(&long,"native-inbox-poison",r#"{"read":true}"#,"1")).await.unwrap();assert_eq!(poisoned.status(),409);
  let unchanged:(Option<i64>,String,i64,i64,i64)=sqlx::query_as("SELECT d.read_at,e.preview,(SELECT count(*) FROM command_receipts WHERE command_id='native-inbox-poison'),(SELECT count(*) FROM audit_events WHERE command_id='native-inbox-poison'),(SELECT count(*) FROM outbox_events WHERE command_id='native-inbox-poison') FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id WHERE d.id=$1").bind(&long).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(unchanged,(None,"Synthetic current Field notice".into(),0,0,0));
  sqlx::raw_sql("DROP TRIGGER poison_native_inbox_after_audit ON audit_events;DROP FUNCTION poison_native_inbox_after_audit();").execute(p.pool().raw()).await.unwrap();
  let direct=sqlx::query("UPDATE outbox_events SET lease_token='legacy-unfiltered' WHERE command_id='native-inbox-read'").execute(p.pool().raw()).await.unwrap_err();assert_eq!(direct.as_database_error().and_then(|v|v.constraint()),Some("native_outbox_runtime_required"));
  let projected_ids:Vec<String>=sqlx::query_scalar("SELECT id FROM outbox_events WHERE command_id LIKE 'native-inbox-%'").fetch_all(p.pool().raw()).await.unwrap();
  let projected_write=p.pool().raw().clone();
  projection_fixture::with_database(2,move|read|async move{
   use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
   let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&files).await.unwrap();
   for _ in 0..5{assert!(erpx_service::field_dependency_worker::run_once(&projected_write,read.raw(),"native-inbox-project",now).await.unwrap());}
   let projected:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=ANY($1)").bind(&projected_ids).fetch_one(read.raw()).await.unwrap();assert_eq!(projected,5);
  }).await;
  // Support visibility belongs to Support, including a reader with neither a
  // Field module nor records.read. Its parent order is navigation, not a grant.
  let mut grant=p.pool().raw().begin().await.unwrap();
  sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company','support',1,'active',0,1,'Synthetic native notification purchase','public synthetic evidence',$1)").bind(now).execute(&mut *grant).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'native-notification-support',company_id,package_id,'fixture-a','synthetic-notification-support',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company'AND package_id='support'").bind(now).execute(&mut *grant).await.unwrap();grant.commit().await.unwrap();
  for (role,permission,version,previous,revision) in [("native_support_notifications","support.read",5,1,2),("native_reporter_notifications","records.read",6,2,3)] {
   sqlx::query("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*',$1,$1,false,1,'active')").bind(role).execute(p.pool().raw()).await.unwrap();
   sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*',$1,1,$2::jsonb,'[]','[\"holding\"]','synthetic',0)").bind(role).bind(json!([permission]).to_string()).execute(p.pool().raw()).await.unwrap();
   sqlx::query("UPDATE assignments SET role=$1,modules_json='[]',version=$2 WHERE id='field-fixture-a'").bind(role).bind(version).execute(p.pool().raw()).await.unwrap();
   let selected=router.clone().oneshot(axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(axum::body::Body::from(json!({"assignmentId":"field-fixture-a","assignmentVersion":version,"revision":previous}).to_string())).unwrap()).await.unwrap();assert_eq!(selected.status(),200);
   let request=|path:&str,post:bool|{let mut r=axum::http::Request::builder().uri(path).method(if post{"POST"}else{"GET"}).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version",version.to_string()).header("x-erpx-context-revision",revision.to_string());if post{r=r.header("idempotency-key",format!("native-support-state-{version}"));}r.body(axum::body::Body::from(if post{r#"{"read":true}"#}else{""})).unwrap()};
   let notices=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/inbox-candidates?all=true&limit=100",false)).await.unwrap();let status=notices.status();let bytes=axum::body::to_bytes(notices.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));let notices:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let rows=notices["data"].as_array().unwrap();assert_eq!(rows.len(),1);assert_eq!(rows[0]["id"],"native-support-notice");assert_eq!(rows[0]["native_field_item"]["href"],"/field-service/support/ticket-reserve-order");
   let ticket=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/support/ticket-reserve-order",false)).await.unwrap();assert_eq!(ticket.status(),200);
   let denied=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/executive?metric=open-service-orders&fromAt=0&toAt=0",false)).await.unwrap();assert_eq!(denied.status(),403);

   let ack=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/inbox/native-support-notice/state",true)).await.unwrap();let status=ack.status();let bytes=axum::body::to_bytes(ack.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));
  }
  let error=sqlx::query("UPDATE staff_notifications SET read_at=NULL WHERE id='native-support-notice'").execute(p.pool().raw()).await.unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.constraint()),Some("native_field_staff_state_required"));
  runtime.close().await;
 }).await;
}

#[tokio::test]
async fn selected_native_service_startup_customer_agreement_needs_no_finance_ledger(){
 use tower::ServiceExt;
 use service_invoice_posting_cases::no_charge_response as native;
 use axum::http::StatusCode;
 facts::account::with_system(|system|async move{
  let wall=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(wall,std::sync::atomic::Ordering::SeqCst);
  let p=facts::review::setup_service_customer_without_finance_references(system).await;let db=p.system.holdings[0].raw();let clock=p.system.scenario.clock.clone();let now=clock.now().get();
  sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_service_startup','Synthetic native Service contributor',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_service_startup',1,'[\"records.read\",\"records.write\",\"records.review\",\"finance.read\",\"finance.write\",\"module.field_service.read\",\"module.finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0);UPDATE assignments SET role='native_service_startup',modules_json='[\"field_service\",\"finance\"]',version=4 WHERE id IN('finance-fixture-a','finance-fixture-b');INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('startup-branch','branch','startup','Synthetic Service branch','company','company','startup-branch','active');").execute(db).await.unwrap();
  // Existing Field reviewer policy requires a current Person/engagement as
  // well as IAM. These are explicit owner fixture facts, never injected actors.
  for user in ["fixture-a","fixture-b"]{
   sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES($1,$2,'company',$2,'TH','active','fixture-a',0,0)").bind(format!("startup-person-{user}")).bind(user).execute(db).await.unwrap();
   sqlx::query("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES($1,$2,'company','employee','salaried','active','2020-01-01','Service contributor','USD','fixture-a',0,0)").bind(format!("startup-engagement-{user}")).bind(format!("startup-person-{user}")).execute(db).await.unwrap();
   sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company',$1,$2,$3,'active','local',1)").bind(user).bind(format!("local:{user}")).bind(format!("startup-person-{user}")).execute(db).await.unwrap();
  }
   // Both Companies are declared before the one native runtime is assembled.
   // Distinct normal sessions must select the correct target on the same host.
   sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('startup-company-two','company','intake-two','Second native intake Company',NULL,'startup-company-two',NULL,'active'),('startup-two-branch','branch','intake-two-branch','Second native branch','startup-company-two','startup-company-two','startup-two-branch','active')").execute(db).await.unwrap();
   // Distinct real fixture accounts respect Holding-wide unique People user
   // links. Reuse only the existing synthetic credential hash, then normal SSO.
   for(user,email)in[("fixture-c","c@example.test"),("fixture-d","d@example.test")]{
    sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\")VALUES($1,$1,$2,1,$3,$3)").bind(user).bind(email).bind(now).execute(p.system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\")SELECT $1,$1,'credential','local:credential',$1,password,$2,$2 FROM account WHERE id='fixture-a'").bind(user).bind(now).execute(p.system.control.raw()).await.unwrap();
   }
   for user in ["fixture-c","fixture-d"]{
    sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES($1,$2,'startup-company-two',$2,'TH','active','fixture-c',0,0)").bind(format!("startup-two-person-{user}")).bind(user).execute(db).await.unwrap();
    sqlx::query("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES($1,$2,'startup-company-two','employee','salaried','active','2020-01-01','Second Company technician','USD','fixture-c',0,0)").bind(format!("startup-two-engagement-{user}")).bind(format!("startup-two-person-{user}")).execute(db).await.unwrap();
    sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('startup-company-two',$1,$2,$3,'active','local',1)").bind(user).bind(format!("local:{user}")).bind(format!("startup-two-person-{user}")).execute(db).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,'native_service_startup',$3,'startup-company-two',$4,$5,1,'holding')").bind(format!("startup-two-{user}")).bind(user).bind(r#"["field_service","finance"]"#).bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
   }
   let mut tx=db.begin().await.unwrap();sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('startup-company-two','service',1,'active',0,1,'Synthetic second Company native concurrency','Public synthetic fixture',$1)").bind(now).execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'startup-two-service',company_id,package_id,'fixture-c','startup-two-service',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='startup-company-two' AND package_id='service'").bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();

  let mut document=runtime_configuration::document(&p.system.control,&p.system.holdings).await;
  document["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"},{"holdingId":"h001","companyId":"startup-company-two"}]);document["fieldServiceWorkspaceReads"]=json!(["h001"]);document["fieldServiceWorkReads"]=json!(["h001"]);document["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);document["fieldServiceStockSetups"]=json!(["h001"]);document["fieldServiceAgreementCreates"]=json!(["h001"]);document["fieldServiceAgreementReviews"]=json!(["h001"]);document["fieldServiceRequests"]=json!(["h001"]);for flag in ["fieldServiceDetailReads","fieldServiceVisits","fieldServiceVisitCompletions","fieldServiceBillingReleases","fieldServiceOrderCancellations"]{document[flag]=json!(["h001"]);}
  let mut duplicate=document.clone();duplicate["fieldServiceReads"].as_array_mut().unwrap().push(json!({"holdingId":"h001","companyId":"company"}));assert!(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&duplicate.to_string()).is_err(),"Duplicate Holding/Company targets remain invalid");
  let mut issue_scope=document.clone();issue_scope["fieldServiceIssues"]=json!(["h001"]);issue_scope["fieldServiceReservations"]=json!(["h001"]);issue_scope["financeJournalReads"]=json!([{"holdingId":"h001","companyId":"company"}]);assert!(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&issue_scope.to_string()).is_err(),"Every Company with enabled parts effects needs its Finance target");issue_scope["financeJournalReads"].as_array_mut().unwrap().push(json!({"holdingId":"h001","companyId":"startup-company-two"}));assert!(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&issue_scope.to_string()).is_ok());
  let runtime=erpx_service::private_development_api::assemble_development_api(erpx_service::private_development_api::DevelopmentApiConfiguration::parse(&document.to_string()).unwrap(),clock.clone(),Arc::new(erpx_server_identity_access::public::SystemAuthenticationSecrets)).await.unwrap();let router=runtime.router();
  let mut cookies=Vec::new();
  for(email,assignment)in[("a@example.test","finance-fixture-a"),("b@example.test","finance-fixture-b")]{
   let central=facts::account::login(&p.system,email,None).await;let child=facts::account::child(&p.system,&central,facts::account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
   let response=router.clone().oneshot(axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(axum::body::Body::from(json!({"assignmentId":assignment,"assignmentVersion":4,"revision":0}).to_string())).unwrap()).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));cookies.push(cookie);
  }
  let request=|path:&str,body:Option<String>,key:&str,reviewer:bool|axum::http::Request::builder().method(if body.is_some(){"POST"}else{"GET"}).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookies[usize::from(reviewer)]).header("x-erpx-context-company","company").header("x-erpx-context-assignment",if reviewer{"finance-fixture-b"}else{"finance-fixture-a"}).header("x-erpx-context-assignment-version","4").header("x-erpx-context-revision","1").header("idempotency-key",key).body(axum::body::Body::from(body.unwrap_or_default())).unwrap();
  let absent=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/customer-preparation",None,"startup-missing-grant",false)).await.unwrap();assert_eq!(absent.status(),200);let absent:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(absent.into_body(),65536).await.unwrap()).unwrap();assert_eq!(absent["data"]["available"],false);assert_eq!(absent["data"]["unavailableReason"],"purchase-required");
  let absent_input=json!({"expectedVersion":0,"contextKey":"a".repeat(64),"input":{"kind":"customer","code":"NO-GRANT","name":"Must not be created","currency":"USD","paymentTermsDays":0,"creditLimit":"0"}});
  let absent=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/customer-preparation",Some(absent_input.to_string()),"startup-missing-grant",false)).await.unwrap();assert_eq!(absent.status(),403);let count:i64=sqlx::query_scalar("SELECT count(*) FROM finance_parties").fetch_one(db).await.unwrap();assert_eq!(count,0);
  let absent_workspace=native(router.clone(),request("/api/v1/holdings/acme/field-service/workspace",None,"startup-no-service",false),StatusCode::FORBIDDEN).await;assert_eq!(absent_workspace["error"]["code"],"PACKAGE_REQUIRED");
  let absent_work=native(router.clone(),request("/api/v1/holdings/acme/field-service/work",None,"startup-no-service",false),StatusCode::OK).await;assert_eq!(absent_work["data"],json!([]));
  let absent_agreement=json!({"entity":"agreement","branchId":"startup-branch","customerId":"not-created","title":"No-purchase draft must fail first","startDate":"2026-01-01","endDate":"2027-12-31","currency":"USD","visitRateMinor":125075,"warrantyDiscountBps":0});
  let refused=native(router.clone(),request("/api/v1/holdings/acme/field-service",Some(absent_agreement.to_string()),"startup-no-service",false),StatusCode::FORBIDDEN).await;assert_eq!(refused["error"]["code"],"PACKAGE_REQUIRED");
  let no_drafts:i64=sqlx::query_scalar("SELECT count(*) FROM field_service_agreements WHERE company_id='company'").fetch_one(db).await.unwrap();assert_eq!(no_drafts,0);
  let mut tx=db.begin().await.unwrap();
  sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','service',1,'active',0,$1,1,'Synthetic Service-only startup','Synthetic fixture',$2)").bind(now+3_600_000).bind(now).execute(&mut *tx).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'native-startup-service-grant',company_id,package_id,'fixture-a','native-startup-service-grant',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
  let path="/api/v1/holdings/acme/field-service/customer-preparation";let response=router.clone().oneshot(request(path,None,"startup-read",false)).await.unwrap();assert_eq!(response.status(),200);let view:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),65536).await.unwrap()).unwrap();
  let customer=json!({"expectedVersion":0,"contextKey":view["data"]["contextKey"],"input":{"kind":"customer","code":"STARTUP","name":"Native startup customer","currency":"USD","paymentTermsDays":0,"creditLimit":"0"}});
  let created=router.clone().oneshot(request(path,Some(customer.to_string()),"startup-customer",false)).await.unwrap();let status=created.status();let bytes=axum::body::to_bytes(created.into_body(),65536).await.unwrap();assert_eq!(status,201,"{}",String::from_utf8_lossy(&bytes));let created:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
  let workspace=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/workspace",None,"startup-workspace",false)).await.unwrap();let status=workspace.status();let bytes=axum::body::to_bytes(workspace.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));let workspace:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(workspace["data"]["customers"],json!([{"id":created["data"]["id"],"name":"Native startup customer","currency":"USD"}]));
  let refreshed=router.clone().oneshot(request(path,None,"startup-current",false)).await.unwrap();assert_eq!(refreshed.status(),200);let refreshed:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(refreshed.into_body(),65536).await.unwrap()).unwrap();assert_eq!(refreshed["data"]["customers"][0]["id"],created["data"]["id"]);
  let mut duplicate=customer.clone();duplicate["contextKey"]=refreshed["data"]["contextKey"].clone();let duplicate=router.clone().oneshot(request(path,Some(duplicate.to_string()),"startup-duplicate-code",false)).await.unwrap();assert_eq!(duplicate.status(),409);let duplicate:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(duplicate.into_body(),65536).await.unwrap()).unwrap();assert_eq!(duplicate["error"]["code"],"SERVICE_CUSTOMER_CODE_EXISTS");
  let mut stale=customer.clone();stale["input"]["code"]=json!("STALE-CONTEXT");let stale=router.clone().oneshot(request(path,Some(stale.to_string()),"startup-stale-context",false)).await.unwrap();assert_eq!(stale.status(),409);let stale:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(stale.into_body(),65536).await.unwrap()).unwrap();assert_eq!(stale["error"]["code"],"SERVICE_CUSTOMER_CONTEXT_CHANGED");
  let agreement=json!({"entity":"agreement","branchId":"startup-branch","customerId":created["data"]["id"],"title":"Native startup agreement","startDate":"2026-01-01","endDate":"2027-12-31","currency":"USD","visitRateMinor":12345,"warrantyDiscountBps":500});
  let root="/api/v1/holdings/acme/field-service";
  for _ in 0..2{let response=router.clone().oneshot(request(root,Some(agreement.to_string()),"startup-agreement",false)).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));}
  let review=json!({"entity":"agreement","id":"service-agreement-startup-agreement","action":"activate","expectedVersion":1,"reason":"Independent native startup approval"});
  let own=router.clone().oneshot(request(root,Some(review.to_string()),"startup-self-review",false)).await.unwrap();assert_eq!(own.status(),409);
  let response=router.clone().oneshot(request(root,Some(review.to_string()),"startup-approve",true)).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));
  let response=router.clone().oneshot(request("/api/v1/holdings/acme/field-service/agreements/creates/startup-agreement",None,"startup-read",false)).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));let saved:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(saved["money"]["amountUnits"],"12345");assert_eq!(saved["money"]["usdUnits"],"12345");assert_eq!(saved["money"]["fxRateId"],serde_json::Value::Null);
  let state:(String,i64,i64,i64,i64)=sqlx::query_as("SELECT status,(SELECT count(*) FROM field_service_agreement_events WHERE agreement_id=g.id),(SELECT count(*) FROM command_receipts WHERE command_id IN('startup-customer','startup-agreement','startup-approve')),(SELECT count(*) FROM audit_events WHERE command_id IN('startup-customer','startup-agreement','startup-approve')),(SELECT count(*) FROM outbox_events WHERE command_id IN('startup-customer','startup-agreement','startup-approve')) FROM field_service_agreements g WHERE id='service-agreement-startup-agreement'").fetch_one(db).await.unwrap();assert_eq!(state,("active".into(),2,3,3,3));
  let prerequisites:(i64,i64,i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_charts),(SELECT count(*) FROM finance_accounts),(SELECT count(*) FROM finance_periods),(SELECT count(*) FROM company_fx_rates),(SELECT count(*) FROM finance_journals),(SELECT count(*) FROM company_package_grants WHERE package_id<>'service'),(SELECT count(*) FROM commerce_settings),(SELECT count(*) FROM finance_invoices),(SELECT count(*) FROM finance_payments)").fetch_one(db).await.unwrap();assert_eq!(prerequisites,(0,0,0,0,0,0,0,0,0));
  sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('startup-department','department','startup-dept','Synthetic department','startup-branch','company','startup-branch','active');INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_service_no_finance','Synthetic non-Finance user',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_service_no_finance',1,'[\"records.read\",\"records.write\",\"module.field_service.read\"]','[]','[\"holding\"]','synthetic',0);").execute(db).await.unwrap();
  sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at)VALUES('startup-blocked','company','customer','BLOCKED','Blocked Service customer','USD','blocked',$1,$1)").bind(now).execute(db).await.unwrap();
  sqlx::query("UPDATE assignments SET scope_kind='department',scope_node_id='startup-department' WHERE id='finance-fixture-b'").execute(db).await.unwrap();
  let original:String=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id)::text FROM finance_parties p").fetch_one(db).await.unwrap();
  for(kind,node,branch,department,role)in[("branch",Some("startup-branch"),Some("startup"),None,"native_service_startup"),("department",Some("startup-department"),Some("startup"),Some("startup-dept"),"native_service_startup"),("holding",None,Some("startup"),Some("startup-dept"),"native_service_no_finance")]{
   let assignment=format!("startup-restricted-{kind}");sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id)VALUES($1,'fixture-a',$2,$7,'company',$3,$4,1,$5,$6)").bind(&assignment).bind(role).bind(now-1000).bind(now+3_600_000).bind(kind).bind(node).bind(if role=="native_service_no_finance"{r#"["field_service"]"#}else{r#"["field_service","finance"]"#}).execute(db).await.unwrap();
   let central=facts::account::login(&p.system,"a@example.test",None).await;let child=facts::account::child(&p.system,&central,facts::account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
   let scoped=|method:&str,path:&str,body:String,selected:bool|{let mut req=axum::http::Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("idempotency-key",format!("startup-restricted-{kind}"));if let Some(branch)=branch{req=req.header("x-erpx-workspace-branch",branch);}if let Some(department)=department{req=req.header("x-erpx-workspace-department",department);}if selected{req=req.header("x-erpx-context-assignment",&assignment).header("x-erpx-context-assignment-version","1").header("x-erpx-context-revision","1");}req.body(axum::body::Body::from(body)).unwrap()};
   let selected=router.clone().oneshot(scoped("POST","/api/v1/holdings/acme/field-service/work-roles",json!({"assignmentId":assignment,"assignmentVersion":1,"revision":0}).to_string(),false)).await.unwrap();let status=selected.status();let bytes=axum::body::to_bytes(selected.into_body(),65536).await.unwrap();assert_eq!(status,200,"{kind}: {}",String::from_utf8_lossy(&bytes));
   let view=router.clone().oneshot(scoped("GET",path,String::new(),true)).await.unwrap();let status=view.status();let bytes=axum::body::to_bytes(view.into_body(),65536).await.unwrap();assert_eq!(status,200,"{kind}: {}",String::from_utf8_lossy(&bytes));let view:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(view["data"]["available"],false);assert_eq!(view["data"]["unavailableReason"],"authority-required");
   let denied=router.clone().oneshot(scoped("POST",path,customer.to_string(),true)).await.unwrap();assert_eq!(denied.status(),403,"{kind}");
   let unchanged:String=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id)::text FROM finance_parties p").fetch_one(db).await.unwrap();assert_eq!(unchanged,original);
   let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(format!("startup-restricted-{kind}")).fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0));
   if kind=="holding"{
    let workspace=native(router.clone(),scoped("GET","/api/v1/holdings/acme/field-service/workspace",String::new(),true),StatusCode::OK).await;
    let data=&workspace["data"];assert_eq!(data["referenceScope"],json!({"branchId":"startup-branch","departmentId":"startup-department"}));
    let mut nodes=data["nodes"].as_array().unwrap().iter().map(|r|r["id"].as_str().unwrap()).collect::<Vec<_>>();nodes.sort();assert_eq!(nodes,vec!["startup-branch","startup-department"]);
    assert_eq!(data["customers"],json!([{"id":created["data"]["id"],"name":"Native startup customer","currency":"USD"}]));assert!(data["assets"].as_array().unwrap().is_empty());
    assert!(data["people"].as_array().unwrap().iter().all(|r|r["branch_id"]=="startup-branch"&&r["department_id"]=="startup-department"));
    let person=data["people"].as_array().unwrap().iter().filter(|r|r["id"]=="fixture-b").collect::<Vec<_>>();assert_eq!(person.len(),1);
    let input=json!({"entity":"agreement","branchId":"startup-branch","departmentId":"startup-department","customerId":created["data"]["id"],"assetId":null,"title":"Dollar customer agreement","startDate":"2026-01-01","endDate":"2027-12-31","currency":"USD","visitRateMinor":125075,"warrantyDiscountBps":0});
    let draft=native(router.clone(),scoped("POST",root,input.to_string(),true),StatusCode::OK).await;assert_eq!(draft["data"]["status"],"draft");assert_eq!(draft["data"]["version"],1);
    let stored:(String,i64,String)=sqlx::query_as("SELECT currency,visit_rate_minor,department_id FROM field_service_agreements WHERE id=$1").bind(draft["data"]["id"].as_str().unwrap()).fetch_one(db).await.unwrap();assert_eq!(stored,("USD".into(),125075,"startup-department".into()));
   }
  }
  let request_body=json!({"entity":"request","agreementId":"service-agreement-startup-agreement","agreementVersion":2,"title":"Out of scope visit","issue":"Inspect the equipment","site":"Customer reception","technicianId":"fixture-b","scheduledAt":null});
  let denied=native(router.clone(),request(root,Some(request_body.to_string()),"startup-out-of-scope-request",false),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"CONFLICT");
  let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id='service-order-startup-out-of-scope-request'),(SELECT count(*) FROM support_tickets WHERE service_order_id='service-order-startup-out-of-scope-request'),(SELECT count(*) FROM command_receipts WHERE command_id='startup-out-of-scope-request')").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0));
  for(engagement,revoked,expected)in[("ended",false,false),("active",false,true),("active",true,false)]{
   sqlx::query("UPDATE hr_engagements SET status=$1 WHERE id='startup-engagement-fixture-b'").bind(engagement).execute(db).await.unwrap();
   if revoked{sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='finance-fixture-b'").bind(now).execute(db).await.unwrap();}
   let workspace=native(router.clone(),request("/api/v1/holdings/acme/field-service/workspace",None,"startup-eligibility",false),StatusCode::OK).await;
   let people=workspace["data"]["people"].as_array().unwrap();assert_eq!(people.iter().any(|r|r["id"]=="fixture-b"),expected);assert!(people.iter().any(|r|r["id"]=="fixture-a"));
  }
  // Fresh direct intake has no sales document or pre-existing case. Reuse
  // this fixture's actual native Agreement and the normal selected sessions.
  Box::pin(async {
   let empty:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_orders WHERE company_id='company'),(SELECT count(*) FROM support_tickets WHERE company_id='company')").fetch_one(db).await.unwrap();assert_eq!(empty,(0,0));
   sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('startup-direct-reviewer','fixture-b','native_service_startup','[\"field_service\",\"finance\"]','company',$1,$2,1,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
   let select=axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookies[1]).header("x-erpx-context-company","company").body(axum::body::Body::from(json!({"assignmentId":"startup-direct-reviewer","assignmentVersion":1,"revision":1}).to_string())).unwrap();native(router.clone(),select,StatusCode::OK).await;
   let direct=|body:serde_json::Value,key:&str,reviewer:bool|{let mut req=request(root,Some(body.to_string()),key,reviewer);if reviewer{req.headers_mut().insert("x-erpx-context-assignment","startup-direct-reviewer".parse().unwrap());req.headers_mut().insert("x-erpx-context-assignment-version","1".parse().unwrap());req.headers_mut().insert("x-erpx-context-revision","2".parse().unwrap());}req};
   let input=json!({"entity":"request","agreementId":"service-agreement-startup-agreement","agreementVersion":2,"title":"Direct customer visit request","issue":"Inspect the customer equipment on site","site":"Customer reception","technicianId":"fixture-a","scheduledAt":now+60000});
   sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('startup-other-branch','branch','intake-other','Other valid intake branch','company','company','startup-other-branch','active')").execute(db).await.unwrap();
   // Original source refusals must create no case, order or command effects.
   for(kind,status,code)in[("module",StatusCode::FORBIDDEN,"MODULE_FORBIDDEN"),("branch",StatusCode::NOT_FOUND,"FIELD_SERVICE_AGREEMENT_NOT_FOUND"),("stale",StatusCode::CONFLICT,"FIELD_SERVICE_AGREEMENT_CHANGED"),("technician",StatusCode::CONFLICT,"CONFLICT")]{
    if kind=="module"{sqlx::query("UPDATE assignments SET modules_json='[]' WHERE id='finance-fixture-a'").execute(db).await.unwrap();}
    let mut invalid=input.clone();if kind=="stale"{invalid["agreementVersion"]=json!(1);}if kind=="technician"{invalid["technicianId"]=json!("unavailable-technician");}
    let key=format!("startup-denied-{kind}");let mut req=direct(invalid,&key,false);if kind=="branch"{req.headers_mut().insert("x-erpx-workspace-branch","intake-other".parse().unwrap());}
    let denied=native(router.clone(),req,status).await;assert_eq!(denied["error"]["code"],code,"{kind}");
    if kind=="module"{sqlx::query("UPDATE assignments SET modules_json=$1 WHERE id='finance-fixture-a'").bind(r#"["field_service","finance"]"#).execute(db).await.unwrap();}
    let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE company_id='company'),(SELECT count(*) FROM support_tickets WHERE company_id='company'),(SELECT count(*) FROM command_receipts WHERE command_id=$1),(SELECT count(*) FROM audit_events WHERE command_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id=$1)").bind(&key).fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0,0,0),"{kind}");
   }
   // Original deferred source and selected-authority faults are reached at
   // outbox, then rolled back along with both owners and the command ledger.
   for fault in ["source","authority"]{
    let agreement_before:String=sqlx::query_scalar("SELECT to_jsonb(g)::text FROM field_service_agreements g WHERE id='service-agreement-startup-agreement'").fetch_one(db).await.unwrap();
    let assignment_before:String=sqlx::query_scalar("SELECT to_jsonb(a)::text FROM assignments a WHERE id='finance-fixture-a'").fetch_one(db).await.unwrap();
    sqlx::raw_sql("CREATE SEQUENCE startup_intake_final_attempt;CREATE FUNCTION startup_intake_final_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('startup_intake_final_attempt');IF TG_ARGV[0]='source' THEN UPDATE field_service_agreements SET end_date='2026-01-02' WHERE id='service-agreement-startup-agreement';ELSE UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='finance-fixture-a';END IF;RETURN NEW;END $$").execute(db).await.unwrap();
    if fault=="source"{sqlx::raw_sql("CREATE CONSTRAINT TRIGGER startup_intake_final_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='startup-direct-intake') EXECUTE FUNCTION startup_intake_final_fault('source')").execute(db).await.unwrap();}else{sqlx::raw_sql("CREATE CONSTRAINT TRIGGER startup_intake_final_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='startup-direct-intake') EXECUTE FUNCTION startup_intake_final_fault('authority')").execute(db).await.unwrap();}
    let denied=native(router.clone(),direct(input.clone(),"startup-direct-intake",false),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],if fault=="source"{"FIELD_SERVICE_SOURCE_CHANGED"}else{"WORK_ROLE_UNAVAILABLE"},"{fault}");
    assert!(sqlx::query_scalar::<_,bool>("SELECT is_called FROM startup_intake_final_attempt").fetch_one(db).await.unwrap());
    let agreement_after:String=sqlx::query_scalar("SELECT to_jsonb(g)::text FROM field_service_agreements g WHERE id='service-agreement-startup-agreement'").fetch_one(db).await.unwrap();assert_eq!(agreement_after,agreement_before);
    let assignment_after:String=sqlx::query_scalar("SELECT to_jsonb(a)::text FROM assignments a WHERE id='finance-fixture-a'").fetch_one(db).await.unwrap();assert_eq!(assignment_after,assignment_before);
    let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE company_id='company'),(SELECT count(*) FROM support_tickets WHERE company_id='company'),(SELECT count(*) FROM command_receipts WHERE command_id='startup-direct-intake'),(SELECT count(*) FROM audit_events WHERE command_id='startup-direct-intake'),(SELECT count(*) FROM outbox_events WHERE command_id='startup-direct-intake')").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0,0,0));
    sqlx::raw_sql("DROP TRIGGER startup_intake_final_fault ON outbox_events;DROP FUNCTION startup_intake_final_fault();DROP SEQUENCE startup_intake_final_attempt").execute(db).await.unwrap();
   }
   // Native Agreement owner retires under its actual source lease while a
   // second native intake waits before its first source snapshot.
   native(router.clone(),direct(agreement.clone(),"startup-lease-agreement",false),StatusCode::OK).await;
   native(router.clone(),direct(json!({"entity":"agreement","id":"service-agreement-startup-lease-agreement","action":"activate","expectedVersion":1,"reason":"Independent source lease fixture approval"}),"startup-lease-approve",true),StatusCode::OK).await;
   sqlx::raw_sql("CREATE FUNCTION startup_retire_pause() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(7812345);RETURN NEW;END $$;CREATE TRIGGER startup_retire_pause AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='startup-lease-retire') EXECUTE FUNCTION startup_retire_pause()").execute(db).await.unwrap();
   let mut barrier=db.acquire().await.unwrap();sqlx::query("SELECT pg_advisory_lock(7812345)").execute(&mut *barrier).await.unwrap();
   let retire_request=direct(json!({"entity":"agreement","id":"service-agreement-startup-lease-agreement","action":"retire","expectedVersion":2,"reason":"Actual owner retires before queued intake reads"}),"startup-lease-retire",true);let retire_router=router.clone();let retire_task=tokio::spawn(async move{native(retire_router,retire_request,StatusCode::OK).await});
   let mut writer=None;for _ in 0..200{writer=sqlx::query_scalar::<_,i32>("SELECT pid FROM pg_locks WHERE locktype='advisory' AND objid=7812345 AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) LIMIT 1").fetch_optional(db).await.unwrap();if writer.is_some(){break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
   let mut lease_input=input.clone();lease_input["agreementId"]=json!("service-agreement-startup-lease-agreement");let intake_request=direct(lease_input,"startup-lease-intake",false);let intake_router=router.clone();let intake_task=tokio::spawn(async move{native(intake_router,intake_request,StatusCode::CONFLICT).await});
   let mut blocked=false;if let Some(pid)=writer{for _ in 0..200{blocked=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid)))").bind(pid).fetch_one(db).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}
   sqlx::query("SELECT pg_advisory_unlock(7812345)").execute(&mut *barrier).await.unwrap();drop(barrier);
   retire_task.await.unwrap();let denied=intake_task.await.unwrap();assert!(writer.is_some(),"Actual native retire must reach outbox barrier");assert!(blocked,"Native intake must wait for actual Agreement writer");assert_eq!(denied["error"]["code"],"FIELD_SERVICE_AGREEMENT_CHANGED");
   sqlx::raw_sql("DROP TRIGGER startup_retire_pause ON outbox_events;DROP FUNCTION startup_retire_pause()").execute(db).await.unwrap();
   let retired:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_agreements WHERE id='service-agreement-startup-lease-agreement'").fetch_one(db).await.unwrap();assert_eq!(retired,("retired".into(),3));
   let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id='service-order-startup-lease-intake'),(SELECT count(*) FROM support_tickets WHERE service_order_id='service-order-startup-lease-intake'),(SELECT count(*) FROM command_receipts WHERE command_id='startup-lease-intake'),(SELECT count(*) FROM audit_events WHERE command_id='startup-lease-intake'),(SELECT count(*) FROM outbox_events WHERE command_id='startup-lease-intake')").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0,0,0));
   let second_router=router.clone();
   let mut second_cookies=Vec::new();for(email,user)in[("c@example.test","fixture-c"),("d@example.test","fixture-d")]{let central=facts::account::login(&p.system,email,None).await;let child=facts::account::child(&p.system,&central,facts::account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());let select=axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","startup-company-two").body(axum::body::Body::from(json!({"assignmentId":format!("startup-two-{user}"),"assignmentVersion":1,"revision":0}).to_string())).unwrap();native(second_router.clone(),select,StatusCode::OK).await;second_cookies.push(cookie);}
   let second_request=|path:&str,body:Option<serde_json::Value>,key:&str,reviewer:bool|axum::http::Request::builder().method(if body.is_some(){"POST"}else{"GET"}).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&second_cookies[usize::from(reviewer)]).header("x-erpx-context-company","startup-company-two").header("x-erpx-context-assignment",if reviewer{"startup-two-fixture-d"}else{"startup-two-fixture-c"}).header("x-erpx-context-assignment-version","1").header("x-erpx-context-revision","1").header("idempotency-key",key).body(axum::body::Body::from(body.map(|b|b.to_string()).unwrap_or_default())).unwrap();
   let preparation=native(second_router.clone(),second_request(path,None,"startup-two-preparation",false),StatusCode::OK).await;assert_eq!(preparation["data"]["available"],true);
   let customer=native(second_router.clone(),second_request(path,Some(json!({"expectedVersion":0,"contextKey":preparation["data"]["contextKey"],"input":{"kind":"customer","code":"TWO","name":"Second Company native customer","currency":"USD","paymentTermsDays":0,"creditLimit":"0"}})),"startup-two-customer",false),StatusCode::CREATED).await;
   native(second_router.clone(),second_request(root,Some(json!({"entity":"agreement","branchId":"startup-two-branch","customerId":customer["data"]["id"],"title":"Second Company native agreement","startDate":"2026-01-01","endDate":"2027-12-31","currency":"USD","visitRateMinor":12345,"warrantyDiscountBps":500})),"startup-two-agreement",false),StatusCode::OK).await;
   native(second_router.clone(),second_request(root,Some(json!({"entity":"agreement","id":"service-agreement-startup-two-agreement","action":"activate","expectedVersion":1,"reason":"Independent second Company approval"})),"startup-two-approve",true),StatusCode::OK).await;
   let mut second_input=input.clone();second_input["agreementId"]=json!("service-agreement-startup-two-agreement");second_input["technicianId"]=json!("fixture-c");
   sqlx::raw_sql("CREATE FUNCTION startup_concurrent_intake() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);RETURN NEW;END $$;CREATE TRIGGER startup_concurrent_intake BEFORE INSERT ON field_service_orders FOR EACH ROW WHEN(NEW.id IN('service-order-startup-direct-intake','service-order-startup-two-intake')) EXECUTE FUNCTION startup_concurrent_intake()").execute(db).await.unwrap();
   let(one,two,other)=tokio::join!(native(router.clone(),direct(input.clone(),"startup-direct-intake",false),StatusCode::OK),native(router.clone(),direct(input.clone(),"startup-direct-intake",false),StatusCode::OK),native(second_router.clone(),second_request(root,Some(second_input),"startup-two-intake",false),StatusCode::OK));assert_eq!(one,two);
   sqlx::raw_sql("DROP TRIGGER startup_concurrent_intake ON field_service_orders;DROP FUNCTION startup_concurrent_intake()").execute(db).await.unwrap();
   for(company,agreement,result,key,actor)in[("company","service-agreement-startup-agreement",&one,"startup-direct-intake","fixture-a"),("startup-company-two","service-agreement-startup-two-agreement",&other,"startup-two-intake","fixture-c")]{
    let id=result["data"]["id"].as_str().unwrap();let ticket=result["data"]["ticketId"].as_str().unwrap();let order:(String,String,i64)=sqlx::query_as("SELECT company_id,agreement_id,version FROM field_service_orders WHERE id=$1").bind(id).fetch_one(db).await.unwrap();assert_eq!(order,(company.into(),agreement.into(),1));let source:(String,String,String)=sqlx::query_as("SELECT company_id,service_order_id,service_agreement_id FROM support_tickets WHERE id=$1").bind(ticket).fetch_one(db).await.unwrap();assert_eq!(source,(company.into(),id.into(),agreement.into()));
    let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts r WHERE command_id=$1 AND actor_id=$3 AND authority_json::jsonb->>'companyId'=$2),(SELECT count(*) FROM audit_events a JOIN command_receipts r USING(command_id) WHERE a.command_id=$1 AND r.actor_id=$3 AND r.authority_json::jsonb->>'companyId'=$2),(SELECT count(*) FROM outbox_events o JOIN command_receipts r USING(command_id) WHERE o.command_id=$1 AND r.actor_id=$3 AND r.authority_json::jsonb->>'companyId'=$2)").bind(key).bind(company).bind(actor).fetch_one(db).await.unwrap();assert_eq!(effects,(1,1,1));
   }
   // Both targets share one router. Neither may read or write the sibling's
   // source even when a real Company2 session supplies its exact source ID.
   let other_id=other["data"]["id"].as_str().unwrap();let first_id=one["data"]["id"].as_str().unwrap();
   let denied=native(router.clone(),request(&format!("{root}/orders/{other_id}/detail"),None,"startup-one-foreign-detail",false),StatusCode::NOT_FOUND).await;assert!(denied.get("data").is_none());
   let denied=native(second_router.clone(),second_request(&format!("{root}/orders/{first_id}/detail"),None,"startup-two-foreign-detail",false),StatusCode::NOT_FOUND).await;assert!(denied.get("data").is_none());
   let denied=native(second_router.clone(),second_request(root,Some(input.clone()),"startup-two-foreign-agreement",false),StatusCode::NOT_FOUND).await;assert_eq!(denied["error"]["code"],"FIELD_SERVICE_AGREEMENT_NOT_FOUND");
   let refused:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='startup-two-foreign-agreement'),(SELECT count(*) FROM audit_events WHERE command_id='startup-two-foreign-agreement'),(SELECT count(*) FROM outbox_events WHERE command_id='startup-two-foreign-agreement')").fetch_one(db).await.unwrap();assert_eq!(refused,(0,0,0));
   // Retained managed-case readback is native and read-only. Reviewed
   // schedule/cancel still belong to Field and project Support atomically.
   let other_order=other["data"]["id"].as_str().unwrap();let other_ticket=other["data"]["ticketId"].as_str().unwrap();
   native(second_router.clone(),second_request(root,Some(json!({"id":other_order,"action":"schedule","expectedVersion":1,"technicianId":"fixture-d","scheduledAt":now+120000,"reason":"Reviewed technician reassignment"})),"startup-two-schedule",false),StatusCode::OK).await;
   let projection:(String,String,i64,i64)=sqlx::query_as("SELECT owner_id,status,service_order_version,version FROM support_tickets WHERE id=$1").bind(other_ticket).fetch_one(db).await.unwrap();assert_eq!(projection,("fixture-d".into(),"open".into(),2,2));
   let mut tx=db.begin().await.unwrap();sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('startup-company-two','support',1,'active',0,1,'Synthetic native Support readback','Public synthetic fixture',$1)").bind(now).execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'startup-two-support',company_id,package_id,'fixture-c','startup-two-support',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='startup-company-two' AND package_id='support'").bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
   let support_path=format!("{root}/support/{other_ticket}");let detail=native(second_router.clone(),second_request(&support_path,None,"startup-two-support-read",false),StatusCode::OK).await;assert_eq!(detail["data"]["managedInFieldService"],true);assert_eq!(detail["data"]["row"]["service_order_id"],other_order);for flag in ["canWork","canManage","canReport","canConfirm","canReopen","canChange","canShare"]{assert_eq!(detail["data"][flag],false,"{flag}");}assert_eq!(detail["data"]["slaPolicies"],json!([]));
   let error=sqlx::query("UPDATE support_tickets SET status='cancelled',version=version+1 WHERE id=$1").bind(other_ticket).execute(db).await.unwrap_err();assert!(error.to_string().contains("SERVICE_CASE_MANAGED_BY_ORDER"));
   let mut tx=db.begin().await.unwrap();let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='startup-company-two' AND package_id='service'").fetch_one(&mut *tx).await.unwrap();sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='startup-company-two' AND package_id='service'").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'startup-two-draining',company_id,package_id,'fixture-c','startup-two-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='startup-company-two' AND package_id='service'").bind(before).bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
   native(second_router.clone(),second_request(root,Some(json!({"id":other_order,"action":"cancel","expectedVersion":2,"reason":"Customer cancelled the existing appointment"})),"startup-two-cancel",false),StatusCode::OK).await;
   let cancelled:(String,i64,i64)=sqlx::query_as("SELECT status,service_order_version,version FROM support_tickets WHERE id=$1").bind(other_ticket).fetch_one(db).await.unwrap();assert_eq!(cancelled,("cancelled".into(),3,3));
   let history:Vec<String>=sqlx::query_scalar("SELECT action FROM support_ticket_events WHERE ticket_id=$1 ORDER BY version").bind(other_ticket).fetch_all(db).await.unwrap();assert_eq!(history,vec!["service-intake","service-schedule","service-cancel"]);
   let readback=native(second_router.clone(),second_request(&support_path,None,"startup-two-support-cancelled",false),StatusCode::OK).await;assert_eq!(readback["data"]["row"]["status"],"cancelled");assert_eq!(readback["data"]["row"]["version"],3);


   let order=one["data"]["id"].as_str().unwrap();let ticket=one["data"]["ticketId"].as_str().unwrap();
   let source:(String,Option<String>,String,String,String)=sqlx::query_as("SELECT source_kind,order_id,service_agreement_id,service_order_id,status FROM support_tickets WHERE id=$1").bind(ticket).fetch_one(db).await.unwrap();assert_eq!(source,("field_service".into(),None,"service-agreement-startup-agreement".into(),order.into(),"open".into()));
   let counts:(i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id=$1),(SELECT count(*) FROM support_tickets WHERE id=$2),(SELECT count(*) FROM support_ticket_events WHERE ticket_id=$2),(SELECT count(*) FROM field_service_order_events WHERE order_id=$1),(SELECT count(*) FROM command_receipts WHERE command_id='startup-direct-intake'),(SELECT count(*) FROM audit_events WHERE command_id='startup-direct-intake'),(SELECT count(*) FROM outbox_events WHERE command_id='startup-direct-intake')").bind(order).bind(ticket).fetch_one(db).await.unwrap();assert_eq!(counts,(1,1,1,1,1,1,1));
   let grants:Vec<String>=sqlx::query_scalar("SELECT package_id FROM company_package_grants WHERE company_id='company' ORDER BY package_id").fetch_all(db).await.unwrap();assert_eq!(grants,vec!["service"]);
   let mut changed=input.clone();changed["site"]=json!("Different command content");let denied=native(router.clone(),direct(changed,"startup-direct-intake",false),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"IDEMPOTENCY_CONFLICT");
   let immutable=sqlx::query("UPDATE support_tickets SET service_agreement_id='different' WHERE id=$1").bind(ticket).execute(db).await.unwrap_err();assert!(immutable.to_string().contains("SUPPORT_SOURCE_IMMUTABLE"));
   let managed=sqlx::query("UPDATE support_tickets SET status='cancelled',version=version+1 WHERE id=$1").bind(ticket).execute(db).await.unwrap_err();assert!(managed.to_string().contains("SERVICE_CASE_MANAGED_BY_ORDER"));
   // Expiry reached after the actual outbox stage cannot leave an orphan
   // intake. Advance the existing fixture clock only after the sequence proves
   // entry into the original retained-write delay; no business guard is mocked.
   let mut tx=db.begin().await.unwrap();let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='company' AND package_id='service'").fetch_one(&mut *tx).await.unwrap();sqlx::query("UPDATE company_package_grants SET ends_at=$1,version=version+1 WHERE company_id='company' AND package_id='service'").bind(now+1000).execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'startup-direct-expiry','company',package_id,'fixture-a','synthetic-direct-expiry',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(before).bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
   sqlx::raw_sql("CREATE SEQUENCE startup_intake_expiry_attempt;CREATE FUNCTION startup_intake_expiry() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('startup_intake_expiry_attempt');PERFORM pg_sleep(2.25);RETURN NEW;END $$;CREATE TRIGGER startup_intake_expiry AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='startup-expired-intake') EXECUTE FUNCTION startup_intake_expiry()").execute(db).await.unwrap();
   let expiry_request=direct(input.clone(),"startup-expired-intake",false);let expiry_router=router.clone();let expiry_task=tokio::spawn(async move{native(expiry_router,expiry_request,StatusCode::CONFLICT).await});
   let mut reached=false;for _ in 0..200{reached=sqlx::query_scalar("SELECT is_called FROM startup_intake_expiry_attempt").fetch_one(db).await.unwrap();if reached{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}clock.0.store(now+2000,std::sync::atomic::Ordering::SeqCst);
   let denied=expiry_task.await.unwrap();assert!(reached,"Actual outbox stage must be reached before fixture time advances");assert_eq!(denied["error"]["code"],"PACKAGE_CAPABILITY_CHANGED");
   let order_unchanged:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_orders WHERE id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(order_unchanged,("scheduled".into(),1));
   let ticket_unchanged:(String,i64,i64)=sqlx::query_as("SELECT status,service_order_version,version FROM support_tickets WHERE id=$1").bind(ticket).fetch_one(db).await.unwrap();assert_eq!(ticket_unchanged,("open".into(),1,1));
   let effects:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id='service-order-startup-expired-intake'),(SELECT count(*) FROM support_tickets WHERE id='ticket-startup-expired-intake'),(SELECT count(*) FROM staff_notifications WHERE record_id='service-order-startup-expired-intake'),(SELECT count(*) FROM command_receipts WHERE command_id='startup-expired-intake'),(SELECT count(*) FROM audit_events WHERE command_id='startup-expired-intake'),(SELECT count(*) FROM outbox_events WHERE command_id='startup-expired-intake')").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0,0,0,0));
   sqlx::raw_sql("DROP TRIGGER startup_intake_expiry ON outbox_events;DROP FUNCTION startup_intake_expiry();DROP SEQUENCE startup_intake_expiry_attempt").execute(db).await.unwrap();
   let workspace=native(router.clone(),request("/api/v1/holdings/acme/field-service/workspace",None,"startup-expired-workspace",false),StatusCode::OK).await;assert_eq!(workspace["data"]["canCreateOrder"],false);
   // Finance's retained primary references are fixture prerequisites; invoice
   // decisions and writes below still run only through native owner ports.
   sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('direct-chart','company','Direct primary','USD','fixture-a',0,0);INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at)VALUES('direct-ar','company','direct-chart','120','Receivable','asset','USD',1,'receivable',0,0),('direct-revenue','company','direct-chart','400','Revenue','revenue','USD',0,'none',0,0)").execute(db).await.unwrap();
   // Real Support projection failure: no owner/notification/command partial writes.
   let before:String=sqlx::query_scalar("SELECT to_jsonb(t)::text FROM support_tickets t WHERE id=$1").bind(ticket).fetch_one(db).await.unwrap();
   sqlx::raw_sql("CREATE FUNCTION startup_case_projection_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SERVICE_CASE_PROJECTION_FAILURE' USING ERRCODE='23514';END;$$;CREATE TRIGGER startup_case_projection_failure BEFORE INSERT ON support_ticket_events FOR EACH ROW WHEN(NEW.action='service-dispatch') EXECUTE FUNCTION startup_case_projection_failure()").execute(db).await.unwrap();
   native(router.clone(),direct(json!({"id":order,"action":"dispatch","expectedVersion":1,"reason":"Complete the contracted visit"}),"startup-direct-dispatch",false),StatusCode::INTERNAL_SERVER_ERROR).await;
   let after:String=sqlx::query_scalar("SELECT to_jsonb(t)::text FROM support_tickets t WHERE id=$1").bind(ticket).fetch_one(db).await.unwrap();assert_eq!(after,before);
   let order_before:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_orders WHERE id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(order_before,("scheduled".into(),1));
   let rolled:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='startup-direct-dispatch'),(SELECT count(*) FROM audit_events WHERE command_id='startup-direct-dispatch'),(SELECT count(*) FROM outbox_events WHERE command_id='startup-direct-dispatch'),(SELECT count(*) FROM staff_notifications WHERE record_id=$1 AND record_version=2),(SELECT count(*) FROM field_service_order_events WHERE order_id=$1)").bind(order).fetch_one(db).await.unwrap();assert_eq!(rolled,(0,0,0,0,1));
   sqlx::raw_sql("DROP TRIGGER startup_case_projection_failure ON support_ticket_events;DROP FUNCTION startup_case_projection_failure()").execute(db).await.unwrap();
   // The expired active grant still permits the original contracted visit's
   // settlement before the separate explicit draining profile is applied.
   native(router.clone(),direct(json!({"id":order,"action":"dispatch","expectedVersion":1,"reason":"Complete the contracted visit"}),"startup-direct-dispatch",false),StatusCode::OK).await;
   let dispatched:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_orders WHERE id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(dispatched,("dispatched".into(),2));
   let mut tx=db.begin().await.unwrap();let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='company' AND package_id='service'").fetch_one(&mut *tx).await.unwrap();sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='service'").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'startup-direct-draining','company',package_id,'fixture-a','synthetic-direct-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(before).bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
   assert_eq!(native(router.clone(),direct(input.clone(),"startup-direct-intake",false),StatusCode::OK).await,one);
   for(action,version)in[("start",2),("complete",3)]{let mut body=json!({"id":order,"action":action,"expectedVersion":version,"reason":"Complete the contracted visit"});if action=="complete"{body["laborMinutes"]=json!(30);body["evidence"]=json!("Inspected the equipment safely");body["customerConfirmation"]=json!("Customer confirmed restored operation");}native(router.clone(),direct(body,&format!("startup-direct-{action}"),false),StatusCode::OK).await;}
   let retire=json!({"entity":"agreement","id":"service-agreement-startup-agreement","action":"retire","expectedVersion":2,"reason":"Every visit must complete its billing handoff"});let denied=native(router.clone(),direct(retire.clone(),"startup-direct-early-retire",true),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"CONFLICT");
   // Two different normal logins linked to one Person cannot become an
   // independent Agreement or billing reviewer; no invoice may be produced.
   sqlx::query("UPDATE iam_members SET person_id='startup-person-fixture-a' WHERE company_id='company' AND user_id='fixture-b'").execute(db).await.unwrap();
   assert!(sqlx::query_scalar::<_,bool>("SELECT iam_same_person('fixture-a','fixture-b','company')").fetch_one(db).await.unwrap());
   for path in [format!("{root}/orders/{order}/detail"),format!("{root}/work")]{let mut req=request(&path,None,"startup-linked-read",true);req.headers_mut().insert("x-erpx-context-assignment","startup-direct-reviewer".parse().unwrap());req.headers_mut().insert("x-erpx-context-assignment-version","1".parse().unwrap());req.headers_mut().insert("x-erpx-context-revision","2".parse().unwrap());let value=native(router.clone(),req,StatusCode::OK).await;if path.ends_with("/work"){let item=value["data"].as_array().unwrap().iter().find(|row|row["id"]==order).unwrap();assert_eq!(item["canAct"],false);assert_eq!(item["queue"],"waiting");}else{assert_eq!(value["data"]["order"]["can_release_billing"],0);}}
   let denied=native(router.clone(),direct(json!({"id":order,"action":"release-billing","expectedVersion":4,"reason":"A linked login is the same person who completed the visit"}),"startup-linked-billing",true),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"INDEPENDENT_BILLING_RELEASE_REQUIRED");
   let invoices:i64=sqlx::query_scalar("SELECT count(*) FROM finance_invoices WHERE source_type='field_service' AND source_id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(invoices,0);
   let denied=native(router.clone(),direct(retire.clone(),"startup-linked-retire",true),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"CONFLICT");
   sqlx::query("UPDATE iam_members SET person_id='startup-person-fixture-b' WHERE company_id='company' AND user_id='fixture-b'").execute(db).await.unwrap();
   native(router.clone(),direct(json!({"id":order,"action":"release-billing","expectedVersion":4,"reason":"Independent reviewer accepted the completed visit"}),"startup-direct-billing",true),StatusCode::OK).await;
   let closed:(String,String,String)=sqlx::query_as("SELECT status,confirmed_by,service_order_id FROM support_tickets WHERE id=$1").bind(ticket).fetch_one(db).await.unwrap();assert_eq!(closed,("closed".into(),"fixture-b".into(),order.into()));
   let totals:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_orders WHERE company_id='company'),(SELECT count(*) FROM support_tickets WHERE company_id='company')").fetch_one(db).await.unwrap();assert_eq!(totals,(0,1));
   native(router.clone(),direct(retire,"startup-direct-retire",true),StatusCode::OK).await;
   sqlx::query("UPDATE assignments SET revoked_at=1 WHERE id='finance-fixture-a'").execute(db).await.unwrap();
   let denied=native(router.clone(),direct(input,"startup-direct-intake",false),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"WORK_ROLE_UNAVAILABLE");
   let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='startup-direct-intake'").fetch_one(db).await.unwrap();assert_eq!(receipts,1);
  }).await;
  runtime.close().await;
  // Native pools flush their PostgreSQL counters on shutdown. Successful
  // requests alone would not prove that the bounded retry hid no deadlock.
  sqlx::query("SELECT pg_stat_clear_snapshot()").execute(db).await.unwrap();
  let deadlocks:i64=sqlx::query_scalar("SELECT deadlocks FROM pg_stat_database WHERE datname=current_database()").fetch_one(db).await.unwrap();assert_eq!(deadlocks,0);
 }).await;
}

#[tokio::test]
async fn native_service_package_obligations_preserve_raw_history_scope_and_unit(){
 facts::account::with_system(|system|async move{
  use erpx_service::field_service_package_settlement::*;
  let p=facts::setup_workspace(system).await;let company=CompanyId::new("company").unwrap();let now=p.review.system.scenario.clock.now().get();
  let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  lock_field_package_obligation_sources(&mut tx).await.unwrap();
  let base=Box::pin(read_field_package_obligations(&mut tx,&company,"service",1,now)).await.unwrap();let addon=Box::pin(read_field_package_obligations(&mut tx,&company,"service-parts-assets",1,now)).await.unwrap();
  assert!(base.observation().outstanding>0);assert!(base.observation().outstanding>=addon.observation().outstanding);
  let support=read_service_support_package_obligations(&mut tx,&company,&base).await.unwrap();assert_eq!(support.observation().outstanding,0);
  // Explicit historical alert/ack fixture only: this proves raw settlement
  // visibility, not native SLA detection or authorized acknowledgement.
  sqlx::query("INSERT INTO support_sla_alerts(id,ticket_id,source_version,breached_at,detected_at)VALUES('historical-service-sla','ticket-reserve-order',1,$1,$1)").bind(now).execute(&mut *tx).await.unwrap();
  assert!(support.require_current(&mut tx,&base).await.is_err());
  let pending=read_service_support_package_obligations(&mut tx,&company,&base).await.unwrap();assert_eq!(pending.observation().outstanding,1);
  sqlx::query("UPDATE support_sla_alerts SET acknowledged_by='fixture-a',acknowledged_at=$1,reason='Historical acknowledgement fixture',version=2 WHERE id='historical-service-sla'").bind(now).execute(&mut *tx).await.unwrap();
  assert!(pending.require_current(&mut tx,&base).await.is_err());let settled=read_service_support_package_obligations(&mut tx,&company,&base).await.unwrap();assert_eq!(settled.observation().outstanding,0);
  // Keep the actual managed-projection fence, without disabling a trigger.
  sqlx::query("SAVEPOINT support_managed_projection").execute(&mut *tx).await.unwrap();
  let denied=sqlx::query("UPDATE support_tickets SET service_order_version=service_order_version+1 WHERE id='ticket-reserve-order'").execute(&mut *tx).await.unwrap_err();assert_eq!(denied.as_database_error().map(|e|e.message()),Some("SERVICE_CASE_MANAGED_BY_ORDER"));
  sqlx::query("ROLLBACK TO SAVEPOINT support_managed_projection").execute(&mut *tx).await.unwrap();sqlx::query("RELEASE SAVEPOINT support_managed_projection").execute(&mut *tx).await.unwrap();
  // Explicit uncommittable staged orphan: deferred source constraints remain
  // enabled. Raw review must see its duty before rollback, not silently drop it.
  sqlx::query("SAVEPOINT support_staged_orphan").execute(&mut *tx).await.unwrap();
  sqlx::query("INSERT INTO support_tickets SELECT(jsonb_populate_record(NULL::support_tickets,to_jsonb(t)||jsonb_build_object('id','uncommittable-service-orphan','service_order_id','uncommittable-missing-order'))).* FROM support_tickets t WHERE t.id='ticket-reserve-order'").execute(&mut *tx).await.unwrap();
  assert!(settled.require_current(&mut tx,&base).await.is_err());let orphan=read_service_support_package_obligations(&mut tx,&company,&base).await.unwrap();assert_eq!(orphan.observation().outstanding,1);
  sqlx::query("ROLLBACK TO SAVEPOINT support_staged_orphan").execute(&mut *tx).await.unwrap();sqlx::query("RELEASE SAVEPOINT support_staged_orphan").execute(&mut *tx).await.unwrap();
  let serialized=serde_json::to_string(base.observation()).unwrap();assert!(!serialized.contains("customer_id")&&!serialized.contains("currency")&&!serialized.contains("source_id"));assert_eq!(base.observation().module_id,"field_service");
  let foreign=Box::pin(read_field_package_obligations(&mut tx,&CompanyId::new("unrelated-company").unwrap(),"service",1,now)).await.unwrap();assert_eq!(foreign.observation().outstanding,0);assert_ne!(foreign.observation().revision,base.observation().revision);
  // Privileged negative fixture: retained inactive placement identity remains
  // a valid original reference, while the old snapshot must detect its drift.
  sqlx::query("UPDATE organization_nodes SET status='suspended' WHERE company_id='company' AND kind IN('branch','headquarters','department')").execute(&mut *tx).await.unwrap();
  assert!(base.require_current(&mut tx).await.is_err());
  let inactive=Box::pin(read_field_package_obligations(&mut tx,&company,"service",1,now)).await.unwrap();assert_eq!(inactive.observation(),base.observation());
  // Unregistered snapshots never open a settlement scope.
  let unregistered=Box::pin(read_field_package_obligations(&mut tx,&company,"service",2,now)).await;
  assert!(matches!(unregistered,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_PACKAGE_OBLIGATION_SCOPE_REQUIRED"));
  tx.rollback().await.unwrap();
  let mut other=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_field_package_obligation_sources(&mut other).await.unwrap();assert!(base.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn native_service_asset_settlement_preserves_customer_equipment(){
 facts::account::with_system(|system|async move{
  use erpx_service::field_service_package_settlement::*;
  use erpx_server_assets::public::field_service_workspace::*;
  let p=facts::setup_without_finance_history_covered(system).await;let company=CompanyId::new("company").unwrap();let now=p.review.system.scenario.clock.now().get();let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  erpx_server_finance::public::field_service_workspace::lock_service_workspace_customers(&mut tx).await.unwrap();erpx_server_inventory::public::field_service_consumption_finance::lock_consumption_finance_sources(&mut tx).await.unwrap();lock_service_asset_settlement(&mut tx).await.unwrap();erpx_server_organization::public::field_service_workspace::lock_service_workspace_locations(&mut tx).await.unwrap();erpx_server_field_service::public::package_settlement::lock_service_package_settlement(&mut tx).await.unwrap();
  let field=Box::pin(read_field_package_obligations(&mut tx,&company,"service",1,now)).await.unwrap();let sources=field.asset_sources();assert!(!sources.is_empty());
  let before:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(a) FROM operational_assets a WHERE id=$1").bind(&sources[0].asset_id).fetch_one(&mut *tx).await.unwrap();
  let assets=read_service_asset_settlement(&mut tx,&company,&sources).await.unwrap();assert!(assets.facts().is_empty());assets.require_current(&mut tx).await.unwrap();
  let unchanged:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(a) FROM operational_assets a WHERE id=$1").bind(&sources[0].asset_id).fetch_one(&mut *tx).await.unwrap();assert_eq!(unchanged,before);
  let mut missing=sources[0].clone();missing.asset_id="missing-private-equipment".into();let invalid=read_service_asset_settlement(&mut tx,&company,&[missing]).await.unwrap();assert_eq!(invalid.facts().len(),1);
  let mut misplaced=sources[0].clone();misplaced.branch_id="another-branch".into();assert_eq!(read_service_asset_settlement(&mut tx,&company,&[misplaced]).await.unwrap().facts().len(),1);
  let mut foreign=sources[0].clone();foreign.company_id="foreign-company".into();assert!(read_service_asset_settlement(&mut tx,&company,&[foreign]).await.is_err());
  // Privileged negative source-drift fixture; all mutations roll back below.
  sqlx::query("UPDATE operational_assets SET name=name||' changed source' WHERE id=$1").bind(&sources[0].asset_id).execute(&mut *tx).await.unwrap();assert!(assets.require_current(&mut tx).await.is_err());assert!(read_service_asset_settlement(&mut tx,&company,&sources).await.unwrap().facts().is_empty());
  tx.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn native_service_inventory_settlement_disposes_no_unrelated_stock(){
 facts::account::with_system(|system|async move{
  use erpx_service::field_service_package_settlement::*;
  let p=facts::setup_workspace(system).await;let company=CompanyId::new("company").unwrap();let now=p.review.system.scenario.clock.now().get();
  let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_package_obligation_sources(&mut tx).await.unwrap();
  let field=Box::pin(read_field_package_obligations(&mut tx,&company,"service-parts-assets",1,now)).await.unwrap();let before=read_service_inventory_package_obligations(&mut tx,&company,&field).await.unwrap();assert_eq!(before.observation().outstanding,1);before.require_current(&mut tx,&field).await.unwrap();tx.rollback().await.unwrap();
  // Real selected native commands, not a counter or terminal-status fixture.
  p.issue("settlement-native-issue-first").await.unwrap();
  p.execute("issue-order","settlement-native-issue-rest",r#"{"action":"issue-part","expectedVersion":4,"reservationId":"reserved-part","quantityMilli":2000,"entryDate":"2026-01-10","allocations":[],"reason":"Consume the actual remaining original reserved spare"}"#).await.unwrap();
  let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_package_obligation_sources(&mut tx).await.unwrap();
  let field=Box::pin(read_field_package_obligations(&mut tx,&company,"service-parts-assets",1,now)).await.unwrap();let after=read_service_inventory_package_obligations(&mut tx,&company,&field).await.unwrap();assert_eq!(after.observation().outstanding,0);assert_eq!(field.observation().outstanding,0);after.require_current(&mut tx,&field).await.unwrap();
  let base=Box::pin(read_field_package_obligations(&mut tx,&company,"service",1,now)).await.unwrap();assert!(base.observation().outstanding>0);
  let stock:(i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(&mut *tx).await.unwrap();assert_eq!(stock,(2000,400));
  let actor=erpx_server_finance::public::FinanceActor{holding_id:HoldingId::new("h001").unwrap(),company_id:company.clone(),user_id:erpx_server_shared::public::UserId::new("fixture-a").unwrap(),assignment_id:erpx_server_shared::public::AssignmentId::new("field-fixture-a").unwrap()};
  // Technical raw reads confer no Finance or Commercial permission. The real
  // issuer scope is used here; consuming grant authority is a separate gate.
  let finance=read_service_finance_package_obligations(&mut tx,&actor,&field).await.unwrap();assert_eq!(finance.observation().outstanding,2);finance.require_current(&mut tx,&field).await.unwrap();
  let journals:i64=sqlx::query_scalar("SELECT count(*) FROM finance_journals WHERE source_type='field_service_parts' AND status='draft'").fetch_one(&mut *tx).await.unwrap();assert_eq!(journals,2);tx.rollback().await.unwrap();
 }).await;
}

// Existing invoice cohort moved here to avoid linking the unrelated broad HTTP
// matrix under the VM disk floor. Original read assertions are retained.
mod service_invoice_posting_cases {
use serde_json::json;
use super::facts::*;
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use erpx_server_identity_access::public::{SystemAuthenticationSecrets,AuthenticationSecrets,SecretPurpose};
use erpx_server_runtime::public::postgres::PgTransactionError;

use erpx_service::private_development_api::{assemble_development_api,DevelopmentApiConfiguration};
use std::sync::{Arc,atomic::Ordering};
use tower::ServiceExt;
use super::runtime_configuration as private_configuration;
async fn send_raw(
    router: &axum::Router,
    cookie: &str,
    key: &str,
    body: &str,
) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/field-service")
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie)
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-revision", "8")
        .header("x-erpx-context-assignment", "field-fixture-a")
        .header("x-erpx-context-assignment-version", "3")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
async fn send(
    router: &axum::Router,
    cookie: &str,
    key: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let (status, raw) = send_raw(router, cookie, key, body).await;
    (status, serde_json::from_str(&raw).unwrap())
}

async fn send_as(
    router: &axum::Router,
    cookie: &str,
    assignment: &str,
    key: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/field-service")
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie)
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-revision", "8")
        .header("x-erpx-context-assignment", assignment)
        .header("x-erpx-context-assignment-version", "3")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[cfg(feature="files-scan-worker")]
async fn actual_payment_proof(
    p:&Parts,device:erpx_server_identity_access::public::DeviceProof,
    selected:erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,
    config:erpx_service::finance_journal_create::FinanceJournalCreateConfiguration,
    cookie:&str,router:axum::Router,clock:Arc<review::TestClock>,payment:&str,directory:tempfile::TempDir,
)->(String,String,erpx_server_files::public::attachment_scan::LocalSanitizationStore,tempfile::TempDir,erpx_service::field_files_cleanup_worker::FieldFilesCleanupWorker<review::TestClock>) {
 use erpx_server_files::public::{field_visit_intake::*,field_visit_upload::*,attachment_scan::*,preview::PreviewToolConfiguration,malware_scanner::ClamAvConfig};
 use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
 use erpx_service::{finance_service_receipt_files::{FinancePaymentIntakeSources,FinancePaymentIntakeAuthority},files_scan_native::*,files_system_scan::DevelopmentFilesScanConfiguration};
 use erpx_server_runtime::public::postgres::LeasedDevelopmentDatabase;
 use erpx_server_shared::public::{HoldingId,CompanyId,Clock,BusinessError};
 use std::{collections::BTreeMap,path::{Path,PathBuf},time::Duration};
 let root=directory.path().join("objects");let temporary=directory.path().join("compute");
 let doc=private_configuration::document(&p.review.system.control,&p.review.system.holdings).await;
 let lease=|d:&serde_json::Value|LeasedDevelopmentDatabase::parse(d["url"].as_str().unwrap(),d["leaseToken"].as_str().unwrap()).unwrap();
 let db=||DevelopmentFilesScanConfiguration::new(lease(&doc["control"]),doc["holdings"].as_array().unwrap().iter().map(|h|(h["writeBinding"].as_str().unwrap().to_owned(),lease(&h["database"]))).collect(),account::fixture::account::holding_registry(),AttachmentScanScope{holding_id:HoldingId::new("h001").unwrap(),company_id:CompanyId::new("company").unwrap()}).unwrap();
 let worker=Arc::new(assemble_native_field_files_scan(db(),NativeFilesScanConfiguration {
  files_roots:BTreeMap::from([("HOLDING_H001_FILES".into(),root.clone())]),temporary_root:temporary,
  preview_binary:PathBuf::from(std::env::var("ERPX_NATIVE_PREVIEW_BIN").expect("qualified preview CLI required")),
  preview_tools:PreviewToolConfiguration::new(Path::new("/usr/bin/heif-convert"),Path::new("/usr/bin/pdftoppm")).unwrap(),
  scanner:ClamAvConfig{socket:PathBuf::from("/erpx-clam/clamd.sock"),database_directory:PathBuf::from("/erpx-definitions"),maximum_definition_age:Duration::from_secs(7*86400),command_timeout:Duration::from_secs(5),scan_timeout:Duration::from_secs(30),concurrent_scans:1},
 },clock.clone()).await.expect("actual Files providers"));
 use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
 use tower::ServiceExt;
 use erpx_service::{field_upload_http::*,field_attachment_intake_http::{FieldIntakeTarget,FieldIntakePolicy,FieldIntakeMode},field_service_reservation_read_http::FieldServiceReadScopeHeaders};
 let intake=FieldIntakeTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),holding:HoldingId::new("h001").unwrap(),company:CompanyId::new("company").unwrap().into(),companies:None,authorizer:p.review.authorizer.clone(),catalogue:Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap()),clock:clock.clone(),policy:FieldIntakePolicy{mode:FieldIntakeMode::Enabled,uploads_frozen:false},readiness:worker.clone()};

 let old_router=field_upload_router(Arc::new(account::fixture::account::policy()),account::fixture::account::holding_registry(),Arc::new(FieldServiceReadScopeHeaders),vec![FieldUploadTarget{intake:Arc::new(intake),pool:p.pool().raw().clone(),writer:LocalFieldQuarantineWriter::open(&root,1).unwrap(),store:LocalSanitizationStore::open(&root,1).unwrap()}]).unwrap();
 let req=|method:&str,path:&str,key:&str,body:Vec<u8>|{
  let e=selected.expected.as_ref().unwrap();
  Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("content-type",if method=="PUT"{"image/png"}else{"application/json"}).header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-assignment",e.assignment_id.as_str()).header("x-erpx-context-assignment-version",e.assignment_version.to_string()).header("x-erpx-context-revision",e.revision.to_string()).body(Body::from(body)).unwrap()
 };
 let mut selected=selected.clone();selected.mutation=true;
 let request=FieldSetRequest{target_type:"finance_payment".into(),target_id:payment.into(),component_kind:"action".into(),component_ref:Some("bank-confirmation".into()),purpose_key:"payment_proof".into(),expected_target_version:3};
 let create=|input:FieldSetRequest,key:&str|{
  let selected=selected.clone();let device=device.clone();let config=config.clone();let clock=clock.clone();let worker=worker.clone();let key=key.to_owned();
  p.review.authorizer.execute_with_sources_deferred("acme.example.test",device,move||Ok(selected),WorkAuthorizationRequest{permission:"module.finance.read".into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},FinancePaymentIntakeSources,move|c,current|{let(config,clock,worker,input,key)=(config.clone(),clock.clone(),worker.clone(),input.clone(),key.clone());Box::pin(async move {
   erpx_service::finance_field_files_intake_command::create_finance_payment_proof_set(c,current,&config,clock.as_ref(),&input,&key,&FieldIntakeAvailability{uploads_frozen:false,scanner_ready:false,preview_ready:false},worker.as_ref()).await
  })})
 };
 let mut wrong_owner=request.clone();wrong_owner.target_type="field_service_visit".into();wrong_owner.component_kind="record".into();wrong_owner.purpose_key="site_photo".into();assert!(matches!(create(wrong_owner,"native-proof-owner-mismatch").await,Err(PgTransactionError::Business(e)) if e.code=="ATTACHMENT_POLICY_UNKNOWN"));
 let mut stale=request.clone();stale.expected_target_version=2;assert!(matches!(create(stale,"native-receipt-proof-stale").await,Err(PgTransactionError::Business(e)) if e.code=="ATTACHMENT_TARGET_CHANGED"));
 let set_path="/api/v1/holdings/acme/finance/payment-files/attachment-sets";
 let set_body=serde_json::to_vec(&serde_json::json!({"targetType":"finance_payment","targetId":payment,"componentKind":"action","componentRef":"bank-confirmation","purposeKey":"payment_proof","expectedTargetVersion":3.0,"ignored":true})).unwrap();
 let created_http=router.clone().oneshot(req("POST",set_path,"native-receipt-proof",set_body.clone())).await.unwrap();let status=created_http.status();let response=to_bytes(created_http.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&response));let created_http:serde_json::Value=serde_json::from_slice(&response).unwrap();
 let created=create(request.clone(),"native-receipt-proof").await.expect("native Finance HTTP proof set replay");assert!(created.replayed);let repeated=create(request.clone(),"native-receipt-proof").await.unwrap();assert!(repeated.replayed);assert_eq!(created.data,repeated.data);assert_eq!(created_http["data"],created.data);
 assert_eq!(router.clone().oneshot(req("POST",set_path,"native-receipt-proof",set_body)).await.unwrap().status(),StatusCode::CREATED);
 let list=format!("{set_path}?targetType=finance_payment&targetId={payment}&purposeKey=payment_proof&expectedTargetVersion=3");let r=router.clone().oneshot(req("GET",&list,"ignored",vec![])).await.unwrap();let status=r.status();let response=to_bytes(r.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&response));let rows:serde_json::Value=serde_json::from_slice(&response).unwrap();assert_eq!(rows["data"],serde_json::json!([created.data.clone()]));
 assert_eq!(router.clone().oneshot(req("GET",&list.replace("targetType=finance_payment","targetType=field_service_visit"),"ignored",vec![])).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
 let set=created.data["id"].as_str().unwrap().to_owned();
 struct Quota;
 impl FieldStorageQuota for Quota {
  fn storage_limit<'a>(&'a self,c:&'a mut sqlx::PgConnection,company:&'a str)->erpx_server_runtime::public::postgres::PgWorkFuture<'a,Option<i64>,BusinessError>{Box::pin(async move {erpx_server_manage_erpx::public::company_storage_limit(c,&CompanyId::new(company)?).await})}
 }
 let bytes=include_bytes!("../../../fixtures/files/preview/png-alpha.input").to_vec();
 let file=FieldFileReservation {filename:"native-payment-proof.png".into(),media_type:"image/png".into(),bytes:bytes.len() as i64,caption:None,supersedes_link_id:None};
 let reserve=||{
  let(selected,device,config,clock,set,file)=(selected.clone(),device.clone(),config.clone(),clock.clone(),set.clone(),file.clone());
  p.review.authorizer.execute_with_sources_deferred("acme.example.test",device,move||Ok(selected),WorkAuthorizationRequest{permission:"module.finance.read".into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},FinancePaymentIntakeSources,move|c,current|{let(config,clock,set,file)=(config.clone(),clock.clone(),set.clone(),file.clone());Box::pin(async move {
   let authority=FinancePaymentIntakeAuthority::for_read(current,&config,clock.as_ref());
   reserve_field_file(c,&authority,&Quota,current,current.holding_id(),&set,&file,"native-payment-proof-file",clock.now().get()).await
  })})
 };
 let reservation_path=format!("/api/v1/holdings/acme/finance/payment-files/attachment-sets/{set}/files");
 let raw=serde_json::to_vec(&serde_json::json!({"filename":file.filename,"mediaType":file.media_type,"bytes":file.bytes,"caption":file.caption,"supersedesLinkId":file.supersedes_link_id})).unwrap();
 let mut anonymous=req("POST",&reservation_path,"native-payment-proof-file",raw.clone());anonymous.headers_mut().remove("cookie");assert_eq!(router.clone().oneshot(anonymous).await.unwrap().status(),StatusCode::UNAUTHORIZED);
 let mut stale=req("POST",&reservation_path,"native-payment-proof-file",raw.clone());stale.headers_mut().insert("x-erpx-context-revision","999".parse().unwrap());assert_eq!(router.clone().oneshot(stale).await.unwrap().status(),StatusCode::CONFLICT);
 let mut wrong_origin=req("POST",&reservation_path,"native-payment-proof-file",raw.clone());wrong_origin.headers_mut().insert("origin","https://northstar.example.test".parse().unwrap());assert_eq!(router.clone().oneshot(wrong_origin).await.unwrap().status(),StatusCode::FORBIDDEN);
 // The previous Visit ingress cannot acquire Finance ownership, even for the
 // same actual selected session and a persisted Finance set identifier.
 let old=old_router.oneshot(req("POST",&reservation_path.replace("finance/payment-files","field-service"),"native-proof-owner-mismatch",raw.clone())).await.unwrap();assert_eq!(old.status(),StatusCode::FORBIDDEN);
 let reserved=router.clone().oneshot(req("POST",&reservation_path,"native-payment-proof-file",raw.clone())).await.unwrap();let status=reserved.status();let response=to_bytes(reserved.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&response));let reserved:serde_json::Value=serde_json::from_slice(&response).unwrap();
 let reservation=reserve().await.expect("native Finance HTTP reservation replay");assert!(reservation.replay);assert!(reserve().await.unwrap().replay);assert_eq!(reserved["data"]["uploadIntentId"],reservation.upload_intent_id);
 // A mistaken staging reservation can be withdrawn only by its creator
 // through the same native current Finance source. No underlying data is deleted.
 let spare=router.clone().oneshot(req("POST",&reservation_path,"native-payment-proof-spare",raw.clone())).await.unwrap();assert_eq!(spare.status(),StatusCode::CREATED);let spare:serde_json::Value=serde_json::from_slice(&to_bytes(spare.into_body(),65536).await.unwrap()).unwrap();let spare_id=spare["data"]["attachmentLinkId"].as_str().unwrap();
 let withdraw_path=format!("/api/v1/holdings/acme/finance/payment-files/attachment-links/{spare_id}/withdraw");
 for _ in 0..2 {
  let r=router.clone().oneshot(req("POST",&withdraw_path,"native-proof-withdraw",br#"{"reason":"Withdraw mistaken duplicate proof"}"#.to_vec())).await.unwrap();let status=r.status();let body=to_bytes(r.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&body));let body:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(body["data"]["linkState"],"withdrawn");
 }
 assert_eq!(router.clone().oneshot(req("POST",&withdraw_path,"native-proof-other-reason",br#"{"reason":"Different withdrawal reason"}"#.to_vec())).await.unwrap().status(),StatusCode::CONFLICT);
 let withdrawals:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_events WHERE link_id=$1 AND action='link.withdrawn'").bind(spare_id).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(withdrawals,1);
 let upload=reservation.upload_intent_id.clone();let link=reservation.attachment_link_id.clone();let url=reserved["data"]["uploadUrl"].as_str().unwrap();assert!(url.contains("/finance/payment-files/uploads/"));
 let completed=router.clone().oneshot(req("PUT",url,"ignored",bytes.clone())).await.unwrap();let status=completed.status();let response=to_bytes(completed.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::ACCEPTED,"{}",String::from_utf8_lossy(&response));let completed:serde_json::Value=serde_json::from_slice(&response).unwrap();assert_eq!(completed["data"]["state"],"quarantined");
 assert_eq!(router.clone().oneshot(req("PUT",url,"ignored",bytes.clone())).await.unwrap().status(),StatusCode::ACCEPTED);
 let replay={let(selected,device,config,clock,upload,bytes)=(selected.clone(),device.clone(),config.clone(),clock.clone(),upload.clone(),bytes.clone());
  p.review.authorizer.execute_with_sources_deferred("acme.example.test",device,move||Ok(selected),WorkAuthorizationRequest{permission:"module.finance.read".into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},FinancePaymentIntakeSources,move|c,current|{let(config,clock,upload,bytes)=(config.clone(),clock.clone(),upload.clone(),bytes.clone());Box::pin(async move {let authority=FinancePaymentIntakeAuthority::for_read(current,&config,clock.as_ref());prepare_field_upload(c,&authority,current,&upload,"image/png",bytes,clock.now().get()).await})}).await.unwrap()
 };assert!(matches!(replay,FieldUploadPreparation::Replay(_)));
 let source=completed["data"]["sourceId"].as_str().unwrap();
 for (kind,id) in [("set",set.as_str()),("upload",upload.as_str()),("link",link.as_str()),("attachment",source)] {
  let error=sqlx::query("SELECT erpx_files_assert_legacy_writer($1,$2)").bind(kind).bind(id).execute(p.pool().raw()).await.expect_err("native Finance file must deny legacy writer");
  assert_eq!(error.as_database_error().and_then(|e|e.constraint()),Some("field_file_native_writer"));
 }
 let owned:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned($1)").bind(source).fetch_one(p.pool().raw()).await.unwrap();assert!(owned);
 let legacy_claim:i64=sqlx::query_scalar("SELECT count(*) FROM erpx_files_claim_legacy_scan($1,$2,'forbidden-legacy-lease',$3)").bind(clock.now().get()).bind(clock.now().get()+60000).bind(source).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(legacy_claim,0);
 let candidate=worker.next_field_candidate().await.expect("automatic native Finance scan discovery").expect("pending proof");assert_eq!(candidate.as_str(),source);
 let outcome=worker.run_next(Some(&candidate)).await.expect("actual scan and preview");assert!(matches!(outcome,AttachmentScanRunOutcome::Settled(_)));
 let available:(String,i64)=sqlx::query_as("SELECT source_state,(SELECT count(*) FROM attachment_storage_ledger WHERE attachment_id=a.id AND variant='preview') FROM attachments a WHERE id=$1").bind(source).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(available,("available".into(),1));
 let store=LocalSanitizationStore::open(&root,1).unwrap();
 // Read the bytes using a new actual selected-actor transaction before and
 // after private I/O; a mere direct filesystem read is not acceptance.
 let status={let(selected,device,config,clock,link)=(selected.clone(),device.clone(),config.clone(),clock.clone(),link.clone());
  p.review.authorizer.execute_with_sources_deferred("acme.example.test",device,move||Ok(selected),WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},FinancePaymentIntakeSources,move|c,current|{let(config,clock,link)=(config.clone(),clock.clone(),link.clone());Box::pin(async move {let authority=FinancePaymentIntakeAuthority::for_read(current,&config,clock.as_ref());read_field_link_status(c,&authority,current,&link,false,clock.now().get()).await})}).await.unwrap()
 };assert_eq!(status["sourceState"],"available");assert_eq!(status["bindable"],true);assert_eq!(status["previewAvailable"],true);
 let witness={let(selected,device,config,clock,link)=(selected.clone(),device.clone(),config.clone(),clock.clone(),link.clone());
  p.review.authorizer.execute_with_sources_deferred("acme.example.test",device,move||Ok(selected),WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},FinancePaymentIntakeSources,move|c,current|{let(config,clock,link)=(config.clone(),clock.clone(),link.clone());Box::pin(async move {let authority=FinancePaymentIntakeAuthority::for_read(current,&config,clock.as_ref());prepare_field_content(c,&authority,current,&link,FieldContentVariant::Original,false,clock.now().get()).await})}).await.unwrap()
 };
 let content=read_field_content_bytes(&store,witness).await.unwrap();
 let response={let(selected,device,config,clock)=(selected.clone(),device.clone(),config.clone(),clock.clone());
  p.review.authorizer.execute_with_sources_deferred("acme.example.test",device,move||Ok(selected),WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},FinancePaymentIntakeSources,move|c,current|{let(config,clock,content)=(config.clone(),clock.clone(),content.clone());Box::pin(async move {let authority=FinancePaymentIntakeAuthority::for_read(current,&config,clock.as_ref());finish_field_content(c,&authority,current,content,false,clock.now().get()).await})}).await.unwrap()
 };
 assert_eq!(response.bytes,bytes);assert_eq!(response.media_type,"image/png");
 let status_path=format!("/api/v1/holdings/acme/finance/payment-files/attachment-links/{link}/status");
 let r=router.clone().oneshot(req("GET",&status_path,"ignored",vec![])).await.unwrap();assert_eq!(r.status(),StatusCode::OK);let status:serde_json::Value=serde_json::from_slice(&to_bytes(r.into_body(),65536).await.unwrap()).unwrap();assert_eq!(status["data"]["sourceState"],"available");assert_eq!(status["data"]["previewAvailable"],true);
 let content_path=format!("/api/v1/holdings/acme/finance/payment-files/attachment-links/{link}/content");
 let r=router.clone().oneshot(req("GET",&content_path,"ignored",vec![])).await.unwrap();assert_eq!(r.status(),StatusCode::OK);assert_eq!(r.headers()["content-type"],"image/png");assert_eq!(to_bytes(r.into_body(),10485760).await.unwrap().as_ref(),bytes.as_slice());
 drop(router);
 worker.shutdown_and_drain().await;Arc::try_unwrap(worker).ok().expect("owned worker clones released").close().await;
 let transactions=erpx_service::files_system_scan::assemble_development_files_scan(db(),clock.clone()).await.unwrap();
 let cleanup=erpx_service::field_files_cleanup_worker::FieldFilesCleanupWorker::new(transactions,LocalFieldQuarantineWriter::open(&root,1).unwrap(),clock);
 (set,link,store,directory,cleanup)
}

// Explicit privileged corruption fixtures, never a normal user/native journey.
// The lower default writer fence must be crossed deliberately to keep testing
// the consuming source guard against a persisted corrupt source. No trigger or
// assertion is disabled, and nothing reaches operator/shared databases.
async fn inject_receipt_fixture_change(p:&Parts,payment:&str,sql:&str){
    let mut tx=p.pool().raw().begin().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(sql)).bind(payment).execute(&mut *tx).await.unwrap();
    sqlx::query(include_str!("../../../packages/server-finance/src/adapters/native_service_receipt_marker.sql")).bind(payment).bind("company").bind(Vec::<String>::new()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

#[test]
fn receipt_files_owner_boundary_keeps_visit_policy_closed() {
 use erpx_server_files::public::{field_visit_intake::*,field_visit_upload::*};
 assert!(!erpx_server_files::public::field_visit_link::valid_field_purpose("finance_payment","action","payment_proof"));
 assert!(owner_classification(IntakeOwner::FieldVisit,"finance_payment","action","payment_proof").is_err());
 assert!(owner_classification(IntakeOwner::FinancePayment,"field_service_visit","record","site_photo").is_err());
 let request=FieldFileReservation{filename:"evidence.pdf".into(),media_type:"application/pdf".into(),bytes:42,caption:None,supersedes_link_id:None};
 assert!(normalize_fresh_field_file("payment_proof",&request).is_err());
 assert_eq!(normalize_fresh_owner_file(IntakeOwner::FinancePayment,"payment_proof",&request).unwrap().bytes,42);
}

#[test]
fn receipt_confirm_wire_preserves_trimmed_utf16_nested_identity_and_defaults() {
    use erpx_service::finance_service_receipt_confirm::normalize_service_receipt_confirm as parse;
    let body=br#"{"proof":"  bank \ud800  ","bankReference":"  confirmed  ","bankVersion":1.0,"bankTransactionId":"bank","periodId":"period","expectedVersion":3,"allocations":[{"amount":"100.00","invoiceId":"invoice","ignored":true}],"ignored":true}"#;
    let first=parse(body).unwrap();let second=parse(String::from_utf8(body.to_vec()).unwrap().replace("\\ud800","\\ufffd").as_bytes()).unwrap();
    assert_eq!(first.input().proof,second.input().proof);assert_ne!(first.normalized(),second.normalized());assert_eq!(first.input().bank_reference,"confirmed");assert!(!first.input().proof.starts_with(' '));assert!(first.input().attachment_set_ids.is_empty());
    assert_eq!(first.normalized().object_keys().unwrap().unwrap(),vec!["expectedVersion","periodId","bankTransactionId","bankVersion","bankReference","proof","allocations","attachmentSetIds"]);
    assert!(parse(&serde_json::to_vec(first.input()).unwrap()).is_ok());
    for (key,value) in [("bankVersion",serde_json::json!(1.5)),("fxRate",serde_json::Value::Null),("bankReference",serde_json::json!(" x ")),("attachmentSetIds",serde_json::Value::Null),("allocations",serde_json::json!([]))] {let mut invalid=serde_json::to_value(first.input()).unwrap();invalid[key]=value;assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err(),"{key}");}
}

#[test]
fn receipt_bank_wire_retains_nested_utf16_and_validates_calendar_types() {
    use erpx_service::finance_service_receipt_bank::normalize_service_receipt_bank as parse;
    let body=br#"{"settlementPaymentId":"pay-r","bankAccount":"bank","statementNumber":"S","statementDate":"2026-11-01","opening":"0","closing":"100","sourceHash":"synthetic-hash","rows":[{"externalId":"row","transactionDate":"2026-11-01","description":"\ud800","amount":"100","currency":"THB","ignored":true}]}"#;
    let first=parse(body).unwrap();let replacement=String::from_utf8(body.to_vec()).unwrap().replace("\\ud800","\\ufffd");let second=parse(replacement.as_bytes()).unwrap();
    assert_eq!(first.input().rows[0].description,second.input().rows[0].description);assert_ne!(first.normalized(),second.normalized());assert_eq!(first.input().settlement_payment_id,"pay-r");assert!(parse(&serde_json::to_vec(first.input()).unwrap()).is_ok());
    for (key,value) in [("statementDate",serde_json::json!("2026-02-30")),("sourceHash",serde_json::json!("short")),("rows",serde_json::Value::Null)] {let mut bad=serde_json::to_value(first.input()).unwrap();bad[key]=value;assert!(parse(&serde_json::to_vec(&bad).unwrap()).is_err());}
}

#[test]
fn receipt_create_wire_retains_schema_order_utf16_and_optional_setup() {
    use erpx_service::finance_service_receipt_create::normalize_service_receipt_create as parse;
    let raw=parse(br#"{"amount":"100","currency":"thb","paymentDate":"2026-11-01","paymentNumber":"\ud800","kind":"receipt","settlementInvoiceId":"field-invoice","ignored":true}"#).unwrap();
    let replaced=parse(br#"{"kind":"receipt","paymentNumber":"\ufffd","paymentDate":"2026-11-01","currency":"thb","amount":"100","settlementInvoiceId":"field-invoice"}"#).unwrap();
    assert_eq!(raw.input().payment_number,replaced.input().payment_number);assert_ne!(raw.normalized(),replaced.normalized());assert_eq!(raw.input().currency,"thb");assert_eq!(raw.input().bank_account,"");
    assert_eq!(raw.normalized().object_keys().unwrap().unwrap(),vec!["kind","paymentNumber","paymentDate","currency","amount","settlementInvoiceId"]);
    assert!(parse(br#"{"kind":"receipt","paymentNumber":"x","paymentDate":"2026-02-30","currency":"THB","amount":"100"}"#).is_err());
    assert!(parse(br#"{"kind":"receipt","paymentNumber":"x","paymentDate":"2026-11-01","currency":"THB","amount":100}"#).is_err());
}

#[test]
fn receipt_review_wire_preserves_utf16_identity_and_original_proof_policy() {
    use erpx_service::finance_service_receipt_review::normalize_service_receipt_decision as parse;
    let raw=parse(br#"{"unknown":true,"proof":"  bank \ud800  ","expectedVersion":2.0}"#,true).unwrap();
    let replaced=parse(br#"{"expectedVersion":2,"proof":"  bank \ufffd  "}"#,true).unwrap();
    assert_eq!(raw.input().expected_version,2);
    assert_eq!(raw.input().proof,replaced.input().proof);
    assert_ne!(raw.normalized(),replaced.normalized());
    assert_eq!(raw.normalized().object_keys().unwrap().unwrap(),vec!["expectedVersion","proof"]);
    for invalid in [r#"{"expectedVersion":0,"proof":"x"}"#,r#"{"expectedVersion":1.5,"proof":"x"}"#,r#"{"expectedVersion":1,"proof":null}"#,r#"[]"#] {assert_eq!(parse(invalid.as_bytes(),true).err().unwrap().code,"VALIDATION_FAILED");}
    // Blank is valid transport input; current authorization/replay precedes the business rejection.
    assert_eq!(parse(br#"{"expectedVersion":1,"proof":"  "}"#,false).unwrap().input().proof,"  ");
}

#[tokio::test]
async fn finance_service_invoice_workspace_uses_real_included_history() {
    receipt_workspace_case(false,false,false,false,false).await;
}
#[tokio::test]
async fn finance_service_invoice_workspace_secondary_book_settlement() {
    receipt_workspace_case(true,false,false,false,false).await;
}
#[tokio::test]
async fn finance_service_invoice_workspace_optional_evidence_http_confirmation() {
    receipt_workspace_case(false,true,false,false,false).await;
}
#[tokio::test]
async fn finance_service_invoice_workspace_rejects_foreign_field_billing() {
    receipt_workspace_case(false,false,true,false,false).await;
}
#[cfg(feature="files-scan-worker")]
#[tokio::test]
async fn finance_service_invoice_workspace_actual_payment_files() {
    receipt_workspace_case(false,false,false,true,false).await;
}
#[tokio::test]
async fn fully_covered_field_invoice_uses_native_independent_confirmation(){Box::pin(receipt_workspace_case(false,false,false,false,true)).await;}
#[tokio::test]
async fn service_only_visit_drains_and_hands_off_native_invoice_without_helpdesk(){
 for exercise_parts in [false,true]{
 account::with_system(move |system|Box::pin(async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);
  let p=crate::facts::setup_service_only_without_finance(system).await;setup_agreement_review_facts(&p,now).await;let system=&p.review.system;let db=p.pool().raw();
  sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
  sqlx::raw_sql("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at)VALUES('service-only-ar','company','chart','120','Receivable','asset','THB',1,'receivable',0,0),('service-only-revenue','company','chart','420','Revenue','revenue','THB',0,'none',0,0);INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('service-only-sale','company','branch','sales','SERVICE-ONLY','customer','warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',0,0);INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,version)VALUES('service-only-ticket','company','branch','service-only-sale','customer','Retained source ticket','Original problem','fixture-a','2026-12-31','fixture-a',0,0,3)").execute(db).await.unwrap();
            sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\")VALUES('monetary-c','Independent monetary reviewer','monetary-c@example.test',1,$1,$1)").bind(now).execute(system.control.raw()).await.unwrap();
            sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\")SELECT 'monetary-c','monetary-c','credential','local:credential','monetary-c',password,$1,$1 FROM account WHERE id='fixture-a'").bind(now).execute(system.control.raw()).await.unwrap();
            sqlx::raw_sql(r#"INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','monetary_review','Independent monetary reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','monetary_review',1,'["finance.read","finance.write","finance.approve","module.finance.read","workflow.finance_approver"]','[]','["holding"]','synthetic',0);INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('monetary-person','monetary-c','company','Independent monetary reviewer','TH','active','fixture-a',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('monetary-engagement','monetary-person','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture-a',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','monetary-c','local:monetary-c','monetary-person','active','local',1)"#).execute(db).await.unwrap();
            sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('finance-monetary-c','monetary-c','monetary_review','[\"finance\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
            sqlx::query("UPDATE finance_periods SET start_date='2026-01-01',end_date='2026-12-31' WHERE id='period' AND company_id='company'").execute(db).await.unwrap();
  let mut config=private_configuration::document(&system.control,&system.holdings).await;config["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);
  for flag in ["fieldServiceWorkspaceReads","fieldServiceReservations","fieldServiceDetailReads","fieldServiceAgreementCreates","fieldServiceAgreementReviews","fieldServiceOrderCreates","fieldServiceVisits","fieldServiceOrderCancellations","fieldServiceVisitCompletions","fieldServiceBillingReleases"]{config[flag]=json!(["h001"]);}
  config["financeJournalReads"]=config["fieldServiceReads"].clone();for flag in ["financeFieldServiceInvoiceReads","financeFieldServiceInvoicePosts"]{config[flag]=json!(["h001"]);}
  let runtime=Box::pin(assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets))).await.unwrap();
  let mut cookies=Vec::new();for(email,assignment)in[("a@example.test","field-fixture-a"),("b@example.test","agreement-reviewer-b"),("monetary-c@example.test","finance-monetary-c")]{let central=account::login(system,email,None).await;let child=account::child(system,&central,account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());let select=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(Body::from(json!({"assignmentId":assignment,"assignmentVersion":3,"revision":0}).to_string())).unwrap();no_charge_response(runtime.router(),select,StatusCode::OK).await;cookies.push(cookie);}
  let call=|method:&str,path:&str,actor:usize,key:&str,body:serde_json::Value|Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookies[actor]).header("x-erpx-context-company","company").header("x-erpx-context-revision","1").header("x-erpx-context-assignment",["field-fixture-a","agreement-reviewer-b","finance-monetary-c"][actor]).header("x-erpx-context-assignment-version","3").header("idempotency-key",key).body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap();let root="/api/v1/holdings/acme/field-service";
  let active=no_charge_response(runtime.router(),call("GET",&format!("{root}/workspace"),0,"",json!(null)),StatusCode::OK).await;for(key,value)in[("canCreateAgreement",true),("canCreateOrder",true),("canUseParts",false),("canUseAssetReferences",false)]{assert_eq!(active["data"][key],value);}assert_eq!(active["data"]["assets"],json!([]));assert!(active["data"]["tickets"].as_array().unwrap().iter().any(|r|r["id"]=="service-only-ticket"));
  let ticket=active["data"]["tickets"].as_array().unwrap().iter().find(|r|r["id"]=="service-only-ticket").unwrap();let mut keys=ticket.as_object().unwrap().keys().map(String::as_str).collect::<Vec<_>>();keys.sort();assert_eq!(keys,vec!["branch_id","customer_id","department_id","id","title","version"]);assert!(!active["data"]["tickets"].to_string().contains("Original problem"));
  sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('service-hidden-branch','branch','service-hidden','Other selected branch','company','company','service-hidden-branch','active')").execute(db).await.unwrap();
  let mut foreign=call("GET",&format!("{root}/workspace"),0,"",json!(null));foreign.headers_mut().insert("x-erpx-workspace-branch","service-hidden".parse().unwrap());let scoped=no_charge_response(runtime.router(),foreign,StatusCode::OK).await;assert_eq!(scoped["data"]["tickets"],json!([]));
  let agreement_input=json!({"entity":"agreement","branchId":"branch","customerId":"customer","assetId":null,"title":"Service-only obligation","startDate":"2026-01-01","endDate":"2027-12-31","currency":"THB","visitRateMinor":150000,"warrantyDiscountBps":0});
  let agreement=no_charge_response(runtime.router(),call("POST",root,0,"service-only-agreement",agreement_input.clone()),StatusCode::OK).await;let agreement=agreement["data"]["id"].as_str().unwrap();
  no_charge_response(runtime.router(),call("POST",root,1,"service-only-agreement-approve",json!({"entity":"agreement","id":agreement,"action":"activate","expectedVersion":1,"reason":"Independent Service-only agreement approval"})),StatusCode::OK).await;
  let created=no_charge_response(runtime.router(),call("POST",root,0,"service-only-order",json!({"ticketId":"service-only-ticket","ticketVersion":3,"agreementId":agreement,"agreementVersion":2,"assetId":null,"issue":"Inspect retained customer equipment","site":"Customer reception","technicianId":"fixture-a","scheduledAt":now+60000})),StatusCode::OK).await;let order=created["data"]["id"].as_str().unwrap();
  if exercise_parts{
   sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('service-bound-asset','company','branch','BOUND','Bound equipment','equipment','THB','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
   let bound_input=json!({"entity":"agreement","branchId":"branch","customerId":"customer","assetId":"service-bound-asset","title":"Original bound-asset agreement","startDate":"2026-01-01","endDate":"2027-12-31","currency":"THB","visitRateMinor":150000,"warrantyDiscountBps":0});
   let absent=no_charge_response(runtime.router(),call("POST",root,0,"service-bound-no-addon",bound_input.clone()),StatusCode::FORBIDDEN).await;assert_eq!(absent["error"]["code"],"PACKAGE_REQUIRED");
   let reserve=json!({"id":order,"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3_600_000,"reason":"Reserve the contractual replacement part"});
   let denied=no_charge_response(runtime.router(),call("POST",root,0,"service-only-parts-denied",reserve.clone()),StatusCode::FORBIDDEN).await;assert_eq!(denied["error"]["code"],"PACKAGE_REQUIRED");
   let absent:i64=sqlx::query_scalar("SELECT count(*) FROM field_service_part_reservations WHERE order_id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(absent,0);
   // Explicit approved purchase facts in the existing disposable fixture;
   // activation/MFA correctness belongs to the qualified Commercial cases.
   let mut tx=db.begin().await.unwrap();
   sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at)VALUES('company','service-parts-assets',1,'active',0,1,'Synthetic addon approval','Synthetic public evidence',$1)").bind(now).execute(&mut *tx).await.unwrap();
   sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'service-only-addon','company',package_id,'fixture-a','synthetic-addon',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='service-parts-assets'").bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
   no_charge_response(runtime.router(),call("POST",root,0,"service-only-parts-enabled",reserve),StatusCode::OK).await;
   let committed:i64=sqlx::query_scalar("SELECT count(*) FROM field_service_part_reservations WHERE order_id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(committed,1);
   let guards:i64=sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id LIKE '%:service-purchase:%'").fetch_one(db).await.unwrap();assert_eq!(guards,0);
   let bound=no_charge_response(runtime.router(),call("POST",root,0,"service-bound-agreement",bound_input),StatusCode::OK).await;let bound=bound["data"]["id"].as_str().unwrap();
   no_charge_response(runtime.router(),call("POST",root,1,"service-bound-review",json!({"entity":"agreement","id":bound,"action":"activate","expectedVersion":1,"reason":"Independent reviewer confirms bound equipment"})),StatusCode::OK).await;
   let mut bound_order=json!({"ticketId":"service-only-ticket","ticketVersion":3,"agreementId":bound,"agreementVersion":2,"issue":"Inspect the bound asset","site":"Customer plant","technicianId":"fixture-a","scheduledAt":now+60000});
   let missing=no_charge_response(runtime.router(),call("POST",root,0,"service-bound-omitted",bound_order.clone()),StatusCode::CONFLICT).await;assert_eq!(missing["error"]["code"],"CONFLICT");
   let absent:i64=sqlx::query_scalar("SELECT count(*) FROM field_service_orders WHERE id='service-order-service-bound-omitted'").fetch_one(db).await.unwrap();assert_eq!(absent,0);
   bound_order["assetId"]=json!("service-bound-asset");let created=no_charge_response(runtime.router(),call("POST",root,0,"service-bound-order",bound_order.clone()),StatusCode::OK).await;let bound_order_id=created["data"]["id"].as_str().unwrap();
   crate::facts::drain(&p,"service-parts-assets").await;
   let workspace=no_charge_response(runtime.router(),call("GET",&format!("{root}/workspace"),0,"",json!(null)),StatusCode::OK).await;assert_eq!(workspace["data"]["canUseParts"],true);assert_eq!(workspace["data"]["canUseAssetReferences"],false);assert_eq!(workspace["data"]["assets"],json!([]));
   let refused=no_charge_response(runtime.router(),call("POST",root,0,"service-bound-new-while-draining",bound_order),StatusCode::CONFLICT).await;assert_eq!(refused["error"]["code"],"PACKAGE_CAPABILITY_CHANGED");
   no_charge_response(runtime.router(),call("POST",root,0,"service-bound-cancel",json!({"id":bound_order_id,"action":"cancel","expectedVersion":1,"reason":"Finish the existing asset-related obligation"})),StatusCode::OK).await;
   runtime.close().await;return;
  }
  // The existing owned fixture changes purchase facts; this is not a normal
  // Commercial/MFA journey or a whole-owner acceptance claim.
  crate::facts::drain(&p,"service").await;
  let draining=no_charge_response(runtime.router(),call("GET",&format!("{root}/workspace"),0,"",json!(null)),StatusCode::OK).await;for key in ["canCreateAgreement","canCreateOrder","canUseParts"]{assert_eq!(draining["data"][key],false);}for key in ["tickets","customers","nodes"]{assert_eq!(draining["data"][key],json!([]));}
  let refused=no_charge_response(runtime.router(),call("POST",root,0,"service-only-new-while-draining",agreement_input),StatusCode::CONFLICT).await;assert_eq!(refused["error"]["code"],"PACKAGE_CAPABILITY_CHANGED");
  for(action,version)in[("dispatch",1),("start",2),("complete",3),("release-billing",4)]{let mut body=json!({"id":order,"action":action,"expectedVersion":version,"reason":"Complete the existing contractual obligation"});if action=="complete"{body["laborMinutes"]=json!(45);body["evidence"]=json!("Diagnosed the equipment safely");body["customerConfirmation"]=json!("Customer confirmed restored operation");}no_charge_response(runtime.router(),call("POST",root,usize::from(action=="release-billing"),&format!("service-only-{action}"),body),StatusCode::OK).await;}
  let detail=no_charge_response(runtime.router(),call("GET",&format!("{root}/orders/{order}/detail"),0,"",json!(null)),StatusCode::OK).await;assert_eq!(detail["data"]["order"]["status"],"billed");assert_eq!(detail["data"]["order"]["version"],5);assert_eq!(detail["data"]["order"]["ticket_id"],"service-only-ticket");
  let invoice:(String,String,String)=sqlx::query_as("SELECT status,source_type,source_id FROM finance_invoices WHERE source_type='field_service' AND source_id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(invoice,("draft".into(),"field_service".into(),order.into()));
  let invoice="field-invoice-service-only-release-billing";
  let quote_sql="SELECT jsonb_build_object('currency',currency,'scale',currency_scale,'amount',amount_units::text,'usd',usd_units::text,'rate',fx_rate_id,'fallback',used_latest_fallback) FROM money_transaction_snapshots WHERE company_id='company' AND source_type=$1 AND source_id=$2 AND amount_kind=$3";
  let frozen:serde_json::Value=sqlx::query_scalar(quote_sql).bind("field_service_billing_release").bind("field-billing-service-only-release-billing").bind("invoice_total").fetch_one(db).await.unwrap();
  let draft:serde_json::Value=sqlx::query_scalar(quote_sql).bind("finance_invoice").bind(invoice).bind("draft:v1").fetch_one(db).await.unwrap();assert_eq!(draft,frozen);
  let date=time::OffsetDateTime::from_unix_timestamp(now/1000).unwrap().date().to_string();
  sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('service-only-later-rate','company','THB',$1::date,72,'monetary-c',$2)").bind(&date).bind(now).execute(db).await.unwrap();
  sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('monetary-secondary','company','Monetary secondary','THB','fixture-a',0,0);INSERT INTO finance_accounts SELECT (jsonb_populate_record(NULL::finance_accounts,to_jsonb(a)||jsonb_build_object('id','monetary-secondary-'||a.id,'chart_id','monetary-secondary'))).* FROM finance_accounts a WHERE a.company_id='company' AND a.chart_id='chart';INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,created_at)VALUES('monetary-book-rule','company','chart','monetary-secondary','2020-01-01','fixture-a','Synthetic independently reviewed rule','{}','synthetic-book-hash',0);INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id)SELECT 'monetary-book-rule',a.id,'monetary-secondary-'||a.id FROM finance_accounts a WHERE a.company_id='company' AND a.chart_id='chart';UPDATE finance_book_rules SET status='approved',reviewed_by='monetary-c' WHERE id='monetary-book-rule'").execute(db).await.unwrap();
  let input=json!({"periodId":"period","entryDate":date,"expectedVersion":1});let post_path=format!("/api/v1/holdings/acme/finance/invoices/{invoice}/post");
  let posted=no_charge_response(runtime.router(),call("POST",&post_path,2,"service-only-native-invoice-post",input.clone()),StatusCode::OK).await;
  assert_eq!(no_charge_response(runtime.router(),call("POST",&post_path,2,"service-only-native-invoice-post",input),StatusCode::OK).await,posted);
  let current:serde_json::Value=sqlx::query_scalar(quote_sql).bind("finance_invoice").bind(invoice).bind("total").fetch_one(db).await.unwrap();assert_eq!(current,frozen);
  let journal=posted["data"]["journalId"].as_str().unwrap();let secondary:String=sqlx::query_scalar("SELECT secondary_journal_id FROM finance_book_postings WHERE primary_journal_id=$1 AND rule_id='monetary-book-rule'").bind(journal).fetch_one(db).await.unwrap();
  for id in [journal,&secondary]{let read=no_charge_response(runtime.router(),call("GET",&format!("/api/v1/holdings/acme/finance/journals/{id}/workspace"),2,"",json!(null)),StatusCode::OK).await;assert_eq!(read["data"]["journal"]["id"],id);assert_eq!(read["data"]["journal"]["status"],"posted");}
  let sums:(i64,i64)=sqlx::query_as("SELECT sum(debit_minor)::bigint,sum(credit_minor)::bigint FROM finance_journal_lines WHERE journal_id=$1").bind(&secondary).fetch_one(db).await.unwrap();assert_eq!(sums,(150000,150000));
  let guards:i64=sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id LIKE '%:service-purchase:%'").fetch_one(db).await.unwrap();assert_eq!(guards,0);
  let other_grants:i64=sqlx::query_scalar("SELECT count(*) FROM company_package_grants WHERE package_id<>'service'").fetch_one(db).await.unwrap();assert_eq!(other_grants,0);
  runtime.close().await;
 })).await;
 }
}
// The original partial/non-warranty refusals need actual native invoice
// lineage, not a forged zero invoice or a pure policy boolean.
#[tokio::test]
async fn monetary_field_invoice_rejects_no_charge_without_mutating_its_draft(){
    for discount in [5000_i64,0] {
        account::with_system(move |system|Box::pin(async move{
            let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
            system.scenario.clock.0.store(now,Ordering::SeqCst);
            let p=crate::facts::setup_without_finance_history_covered(system).await;
            setup_agreement_review_facts(&p,now).await;
            let system=&p.review.system;let db=p.pool().raw();
            sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\")VALUES('monetary-c','Independent monetary reviewer','monetary-c@example.test',1,$1,$1)").bind(now).execute(system.control.raw()).await.unwrap();
            sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\")SELECT 'monetary-c','monetary-c','credential','local:credential','monetary-c',password,$1,$1 FROM account WHERE id='fixture-a'").bind(now).execute(system.control.raw()).await.unwrap();
            sqlx::raw_sql(r#"INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','monetary_review','Independent monetary reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','monetary_review',1,'["finance.read","finance.write","finance.approve","module.finance.read","workflow.finance_approver"]','[]','["holding"]','synthetic',0);INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('monetary-person','monetary-c','company','Independent monetary reviewer','TH','active','fixture-a',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('monetary-engagement','monetary-person','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture-a',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','monetary-c','local:monetary-c','monetary-person','active','local',1)"#).execute(db).await.unwrap();
            sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('finance-monetary-c','monetary-c','monetary_review','[\"finance\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
            sqlx::query("UPDATE finance_periods SET start_date='2026-01-01',end_date='2026-12-31' WHERE id='period' AND company_id='company'").execute(db).await.unwrap();
            sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
            sqlx::raw_sql("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('invoice-ar','company','chart','120','Receivable','asset','THB',1,'receivable',0,0),('invoice-revenue','company','chart','420','Revenue','revenue','THB',0,'none',0,0)").execute(db).await.unwrap();
            // Retained physical opening balance, matching the original8000-cost
            // assertion. All reservation/issue/cost decisions below are native.
            sqlx::raw_sql("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('monetary-warehouse','company','branch','ORIGINAL','Original scoped physical stock');INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('monetary-warehouse','product',1000,8000)").execute(db).await.unwrap();
            let mut config=private_configuration::document(&system.control,&system.holdings).await;
            config["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);
            config["financeJournalReads"]=config["fieldServiceReads"].clone();
            for flag in ["fieldServiceWorkspaceReads","fieldServiceDetailReads","fieldServiceVisitRecoveries","fieldServiceWorkReads","fieldServiceReservations","fieldServiceIssues","financeJournalPosts","financeFieldServiceJournalPosts","fieldServiceAgreementCreates","fieldServiceAgreementReviews","fieldServiceRequests","fieldServiceVisits","fieldServiceVisitCompletions","fieldServiceBillingReleases","financeFieldServiceInvoiceReads","financeFieldServiceInvoicePosts","financeFieldServiceReceiptWrites"]{config[flag]=json!(["h001"]);}
            let runtime=Box::pin(assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets))).await.unwrap();
            let mut cookies=Vec::new();
            for (email,assignment) in [("a@example.test","field-fixture-a"),("b@example.test","agreement-reviewer-b"),("monetary-c@example.test","finance-monetary-c")]{
                let central=account::login(system,email,None).await;let child=account::child(system,&central,account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
                let select=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(Body::from(json!({"assignmentId":assignment,"assignmentVersion":3,"revision":0}).to_string())).unwrap();
                no_charge_response(runtime.router(),select,StatusCode::OK).await;cookies.push(cookie);
            }
            let call=|method:&str,path:&str,actor:usize,key:&str,body:serde_json::Value|Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookies[actor]).header("x-erpx-context-company","company").header("x-erpx-context-revision","1").header("x-erpx-context-assignment",["field-fixture-a","agreement-reviewer-b","finance-monetary-c"][actor]).header("x-erpx-context-assignment-version","3").header("idempotency-key",key).body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap();
            let root="/api/v1/holdings/acme/field-service";
            let agreement=no_charge_response(runtime.router(),call("POST",root,0,"monetary-agreement",json!({"entity":"agreement","branchId":"branch","customerId":"customer","assetId":"covered-asset","title":"Actual independently approved monetary warranty","startDate":"2026-01-01","endDate":"2027-12-31","currency":"THB","visitRateMinor":150000,"warrantyDiscountBps":discount})),StatusCode::OK).await;
            let agreement=agreement["data"]["id"].as_str().unwrap();
            no_charge_response(runtime.router(),call("POST",root,1,"monetary-agreement-approve",json!({"entity":"agreement","id":agreement,"action":"activate","expectedVersion":1,"reason":"Independent approval of exact partial or ordinary customer demand"})),StatusCode::OK).await;
            let created=no_charge_response(runtime.router(),call("POST",root,0,"monetary-request",json!({"entity":"request","agreementId":agreement,"agreementVersion":2,"title":"Actual monetary warranty refusal","issue":"Inspect original equipment","site":"Customer site","technicianId":"fixture-a","scheduledAt":now+60000})),StatusCode::OK).await;
            let order=created["data"]["id"].as_str().unwrap();
            let work=no_charge_response(runtime.router(),call("GET",&format!("{root}/work"),0,"",json!(null)),StatusCode::OK).await;
            let item=work["data"].as_array().unwrap().iter().find(|r|r["id"]==order).unwrap();assert_eq!(item["queue"],"todo");assert_eq!(item["canAct"],true);
            let competitor=if discount==0{Some(no_charge_response(runtime.router(),call("POST",root,0,"monetary-competing-request",json!({"entity":"request","agreementId":agreement,"agreementVersion":2,"title":"Actual competing last spare","issue":"Inspect original equipment","site":"Customer site","technicianId":"fixture-a","scheduledAt":now+60000})),StatusCode::OK).await)}else{None};
            let reserve=|id:&str,key:&str|call("POST",root,0,key,json!({"id":id,"action":"reserve-part","expectedVersion":1,"warehouseId":"monetary-warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3_600_000,"reason":"Original confirmed appointment stock"}));
            let order=if let Some(other)=&competitor{
                let other_id=other["data"]["id"].as_str().unwrap();
                let(first,second)=tokio::join!(runtime.router().oneshot(reserve(order,"monetary-reserve-part")),runtime.router().oneshot(reserve(other_id,"monetary-reserve-competing")));
                let(first,second)=(first.unwrap(),second.unwrap());
                assert_eq!(usize::from(first.status()==StatusCode::OK)+usize::from(second.status()==StatusCode::OK),1);
                if first.status()==StatusCode::OK{assert_eq!(second.status(),StatusCode::CONFLICT);order}else{assert_eq!(first.status(),StatusCode::CONFLICT);other_id}
            }else{no_charge_response(runtime.router(),reserve(order,"monetary-reserve-part"),StatusCode::OK).await;order};
            if discount==0{
                let foreign=Request::builder().uri(format!("{root}/orders/{order}/detail")).header("host","acme.example.test").header("cookie",&cookies[0]).header("x-erpx-workspace-branch","foreign").body(Body::empty()).unwrap();
                no_charge_response(runtime.router(),foreign,StatusCode::NOT_FOUND).await;
                let stale=no_charge_response(runtime.router(),call("POST",root,0,"monetary-stale-dispatch",json!({"id":order,"action":"dispatch","expectedVersion":1,"reason":"Stale reserved order must fail"})),StatusCode::CONFLICT).await;assert_eq!(stale["error"]["code"],"CONFLICT");
                let immutable=sqlx::query("DELETE FROM field_service_order_events WHERE order_id=$1").bind(order).execute(db).await.unwrap_err();assert!(immutable.to_string().contains("FIELD_SERVICE_HISTORY_IMMUTABLE"));
                let immutable=sqlx::query("UPDATE field_service_orders SET ticket_id='different' WHERE id=$1").bind(order).execute(db).await.unwrap_err();assert!(immutable.to_string().contains("FIELD_SERVICE_ORDER_SOURCE_IMMUTABLE"));
            }
            let reservation:String=sqlx::query_scalar("SELECT id FROM field_service_part_reservations WHERE order_id=$1").bind(order).fetch_one(db).await.unwrap();
            let reserved:i64=sqlx::query_scalar("SELECT quantity_milli FROM inventory_active_reservations WHERE source_kind='field_service' AND source_id=$1").bind(&reservation).fetch_one(db).await.unwrap();assert_eq!(reserved,1000);
            for (action,version) in [("dispatch",2),("start",3),("exception",4),("resume",5),("issue-part",6),("complete",7),("release-billing",8)]{
                let mut body=json!({"id":order,"action":action,"expectedVersion":version,"reason":"Actual native monetary visit progression"});
                if action=="exception"{body["reason"]=json!("Power isolation was not ready at arrival");}
                if action=="issue-part"{body["reservationId"]=json!(reservation);body["quantityMilli"]=json!(1000);body["entryDate"]=json!(time::OffsetDateTime::from_unix_timestamp(now/1000).unwrap().date().to_string());}
                if action=="complete"{body["laborMinutes"]=json!(90);body["evidence"]=json!("Original equipment tested");body["customerConfirmation"]=json!("Customer accepted actual work");}
                if action=="release-billing"{
                    let denied=no_charge_response(runtime.router(),call("POST",root,0,"monetary-self-billing-denied",body.clone()),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"INDEPENDENT_BILLING_RELEASE_REQUIRED");
                    if discount==0{
                        sqlx::raw_sql("CREATE FUNCTION monetary_final_event_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'FIELD_SERVICE_FINAL_EVENT_FAILURE' USING ERRCODE='23514';END $$;CREATE TRIGGER monetary_final_event_failure BEFORE INSERT ON field_service_order_events FOR EACH ROW WHEN (NEW.id='service-order-event-monetary-release-billing') EXECUTE FUNCTION monetary_final_event_failure()").execute(db).await.unwrap();
                        no_charge_response(runtime.router(),call("POST",root,1,"monetary-release-billing",body.clone()),StatusCode::INTERNAL_SERVER_ERROR).await;
                        let original:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_orders WHERE id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(original,("completed".into(),8));
                        let rollback:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_invoices WHERE source_type='field_service' AND source_id=$1),(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='field_service_billing_release' AND source_id='field-billing-monetary-release-billing'),(SELECT count(*) FROM field_service_billing_releases WHERE order_id=$1),(SELECT count(*) FROM command_receipts WHERE command_id='monetary-release-billing'),(SELECT count(*) FROM audit_events WHERE command_id='monetary-release-billing'),(SELECT count(*) FROM outbox_events WHERE command_id='monetary-release-billing')").bind(order).fetch_one(db).await.unwrap();assert_eq!(rollback,(0,0,0,0,0,0));
                        sqlx::raw_sql("DROP TRIGGER monetary_final_event_failure ON field_service_order_events;DROP FUNCTION monetary_final_event_failure()").execute(db).await.unwrap();
                    }
                }
                no_charge_response(runtime.router(),call("POST",root,usize::from(action=="release-billing"),&format!("monetary-{action}"),body),StatusCode::OK).await;
                if action=="exception"{
                    let work=no_charge_response(runtime.router(),call("GET",&format!("{root}/work"),0,"",json!(null)),StatusCode::OK).await;
                    let item=work["data"].as_array().unwrap().iter().find(|r|r["id"]==order).unwrap();assert!(item["blocker"].as_str().unwrap().contains("Power isolation"));
                }
                if action=="issue-part"{
                    let stock:(i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='monetary-warehouse' AND product_id='product'").fetch_one(db).await.unwrap();assert_eq!(stock,(0,0));
                    let consumption:(String,i64,i64)=sqlx::query_as("SELECT journal_id,quantity_milli,value_minor FROM operational_inventory_consumptions WHERE source_kind='field_service' AND source_id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!((consumption.1,consumption.2),(1000,8000));
                    let draft:String=sqlx::query_scalar("SELECT status FROM finance_journals WHERE id=$1").bind(&consumption.0).fetch_one(db).await.unwrap();assert_eq!(draft,"draft");
                    let detail=no_charge_response(runtime.router(),call("GET",&format!("{root}/orders/{order}/detail"),0,"",json!(null)),StatusCode::OK).await;
                    let original=&detail["data"]["consumptions"][0];assert_eq!(original["source_kind"],"field_service");assert_eq!(original["quantity_milli"],1000);assert_eq!(original["value_minor"],8000);assert_eq!(original["journal_status"],"draft");
                    let incomplete=json!({"id":order,"action":"complete","expectedVersion":7,"laborMinutes":90,"evidence":"Original equipment tested","customerConfirmation":"Customer accepted actual work","reason":"Unposted real spare cost must block completion"});
                    let denied=no_charge_response(runtime.router(),call("POST",root,0,"monetary-unposted-cost-denied",incomplete),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"CONFLICT");
                    let posted=no_charge_response(runtime.router(),call("POST",&format!("/api/v1/holdings/acme/finance/journals/{}/post",consumption.0),2,"monetary-cost-post",json!({"expectedVersion":1})),StatusCode::OK).await;assert_eq!(posted["data"]["status"],"posted");
                    let lines:Vec<(String,i64,i64)>=sqlx::query_as("SELECT account_id,base_debit_minor,base_credit_minor FROM finance_journal_lines WHERE journal_id=$1 ORDER BY ordinal").bind(&consumption.0).fetch_all(db).await.unwrap();assert_eq!(lines,vec![("expense".into(),8000,0),("inventory".into(),0,8000)]);
                    let movement:(i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor FROM inventory_ledger WHERE source_type='field_service' AND source_id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(movement,(-1000,-8000));
                }
            }
            let completed:(String,i64,i64)=sqlx::query_as("SELECT status,version,labor_minutes FROM field_service_orders WHERE id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(completed,("billed".into(),9,90));
            let history:Vec<String>=sqlx::query_scalar("SELECT action FROM field_service_order_events WHERE order_id=$1 ORDER BY version").bind(order).fetch_all(db).await.unwrap();assert_eq!(history,vec!["create","reserve-part","dispatch","start","exception","resume","issue-part","complete","release-billing"]);
            let billing:(String,i64,i64,String)=sqlx::query_as("SELECT currency,amount_minor,source_order_version,status FROM field_service_billing_releases WHERE order_id=$1").bind(order).fetch_one(db).await.unwrap();assert_eq!(billing,("THB".into(),if discount==5000{75000}else{150000},9,"released".into()));
            if discount==0{
                let retry=json!({"id":order,"action":"release-billing","expectedVersion":8,"reason":"Actual native monetary visit progression"});
                let replay=no_charge_response(runtime.router(),call("POST",root,1,"monetary-release-billing",retry),StatusCode::OK).await;assert_eq!(replay["data"]["id"],order);assert_eq!(replay["data"]["version"],9);assert_eq!(replay["data"]["status"],"billed");
                let effects:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_invoices WHERE source_type='field_service' AND source_id=$1),(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='field_service_billing_release' AND source_id='field-billing-monetary-release-billing'),(SELECT count(*) FROM command_receipts WHERE command_id='monetary-release-billing'),(SELECT count(*) FROM audit_events WHERE command_id='monetary-release-billing'),(SELECT count(*) FROM outbox_events WHERE command_id='monetary-release-billing'),(SELECT count(*) FROM staff_notifications WHERE record_id=$1 AND record_version=9)").bind(order).fetch_one(db).await.unwrap();assert_eq!(effects,(1,1,1,1,1,1));
            }
            let invoice="field-invoice-monetary-release-billing";
            let view=no_charge_response(runtime.router(),call("GET",&format!("/api/v1/holdings/acme/finance/invoices/{invoice}/workspace"),2,"",json!(null)),StatusCode::OK).await;
            assert!(view["noCharge"].is_null());assert_eq!(view["data"]["invoice"]["total_minor"],if discount==5000{75000}else{150000});assert_eq!(view["data"]["invoice"]["status"],"draft");
            let before:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id=$1").bind(invoice).fetch_one(db).await.unwrap();
            assert_eq!(before["kind"],"receivable");assert_eq!(before["source_type"],"field_service");assert_eq!(before["source_id"],order);assert_eq!(before["debit_account_id"],"invoice-ar");// The original Sales fixture has no THB currency; this explicit THB revenue
            // reference is the eligible account selected by the native Finance port.
            assert_eq!(before["credit_account_id"],"invoice-revenue");
            let detail=no_charge_response(runtime.router(),call("GET",&format!("{root}/orders/{order}/detail"),0,"",json!(null)),StatusCode::OK).await;
            assert_eq!(detail["data"]["order"]["status"],"billed");assert_eq!(detail["data"]["order"]["version"],9);assert_eq!(detail["data"]["order"]["labor_minutes"],90);assert_eq!(detail["data"]["billing"]["currency"],"THB");assert_eq!(detail["data"]["billing"]["amount_minor"],if discount==5000{75000}else{150000});assert_eq!(detail["data"]["billing"]["source_order_version"],9);assert_eq!(detail["data"]["billing"]["status"],"released");
            let actions:Vec<&str>=detail["data"]["history"].as_array().unwrap().iter().map(|e|e["action"].as_str().unwrap()).collect();assert_eq!(actions,vec!["create","reserve-part","dispatch","start","exception","resume","issue-part","complete","release-billing"]);
            let denied=no_charge_response(runtime.router(),call("POST",&format!("/api/v1/holdings/acme/finance/invoices/{invoice}/confirm-no-charge"),2,"monetary-no-charge-denied",json!({"expectedVersion":1,"contextKey":"f".repeat(64),"reason":"Monetary demand cannot become no charge"})),StatusCode::CONFLICT).await;
            assert_eq!(denied["error"]["code"],"SERVICE_NO_CHARGE_SOURCE_CHANGED");
            let after:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id=$1").bind(invoice).fetch_one(db).await.unwrap();assert_eq!(after,before);
            let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_service_no_charge_confirmations WHERE invoice_id=$1),(SELECT count(*) FROM command_receipts WHERE command_id='monetary-no-charge-denied'),(SELECT count(*) FROM audit_events WHERE command_id='monetary-no-charge-denied'),(SELECT count(*) FROM outbox_events WHERE command_id='monetary-no-charge-denied'),(SELECT count(*) FROM flow_events WHERE command_id='monetary-no-charge-denied')").bind(invoice).fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0,0,0));
            let date=time::OffsetDateTime::from_unix_timestamp(now/1000).unwrap().date().to_string();
            if discount==5000 {
                let release:(i64,bool,i32,String,String)=sqlx::query_as("SELECT amount_minor,warranty_applied,warranty_discount_bps,warranty_end_snapshot,pricing_date::text FROM field_service_billing_releases WHERE id='field-billing-monetary-release-billing'").fetch_one(db).await.unwrap();
                assert_eq!(release,(75000,true,5000,"2099-12-31".into(),date.clone()));
            }
            // Original released quote survives a later confirmed FX fact and
            // actual approved secondary-book projection. These are explicit
            // fixture prerequisites, not FX/book administration qualification.
            let quote_sql="SELECT jsonb_build_object('currency',currency,'currencyScale',currency_scale,'amountUnits',amount_units::text,'usdUnits',usd_units::text,'fxRateId',fx_rate_id,'usedLatestFallback',used_latest_fallback) FROM money_transaction_snapshots WHERE company_id='company' AND source_type=$1 AND source_id=$2 AND amount_kind=$3";
            let frozen:serde_json::Value=sqlx::query_scalar(quote_sql).bind("field_service_billing_release").bind("field-billing-monetary-release-billing").bind("invoice_total").fetch_one(db).await.unwrap();
            let draft:serde_json::Value=sqlx::query_scalar(quote_sql).bind("finance_invoice").bind(invoice).bind("draft:v1").fetch_one(db).await.unwrap();assert_eq!(draft,frozen);
            sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('monetary-later-rate','company','THB',$1::date,72,'monetary-c',$2)").bind(&date).bind(now).execute(db).await.unwrap();
            let newer:String=sqlx::query_scalar("SELECT id FROM company_fx_rates WHERE company_id='company' AND currency='THB' ORDER BY effective_date DESC,confirmed_at DESC,id DESC LIMIT 1").fetch_one(db).await.unwrap();assert_eq!(newer,"monetary-later-rate");assert_ne!(frozen["fxRateId"],newer);
            sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('monetary-secondary','company','Monetary secondary','THB','fixture-a',0,0);INSERT INTO finance_accounts SELECT (jsonb_populate_record(NULL::finance_accounts,to_jsonb(a)||jsonb_build_object('id','monetary-secondary-'||a.id,'chart_id','monetary-secondary'))).* FROM finance_accounts a WHERE a.company_id='company' AND a.chart_id='chart';INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,created_at)VALUES('monetary-book-rule','company','chart','monetary-secondary','2020-01-01','fixture-a','Synthetic independently reviewed rule','{}','synthetic-book-hash',0);INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id)SELECT 'monetary-book-rule',a.id,'monetary-secondary-'||a.id FROM finance_accounts a WHERE a.company_id='company' AND a.chart_id='chart';UPDATE finance_book_rules SET status='approved',reviewed_by='monetary-c' WHERE id='monetary-book-rule'").execute(db).await.unwrap();
            let input=json!({"periodId":"period","entryDate":date,"expectedVersion":1});let path=format!("/api/v1/holdings/acme/finance/invoices/{invoice}/post");
            // A real derived-book failure must roll primary, secondary and
            // command effects back before the identical post/replay can succeed.
            sqlx::raw_sql("CREATE SEQUENCE monetary_book_attempt;CREATE FUNCTION monetary_book_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('monetary_book_attempt');RAISE EXCEPTION 'MONETARY_BOOK_PROJECTION_FAILURE' USING ERRCODE='23514';END $$;CREATE TRIGGER monetary_book_failure BEFORE INSERT ON finance_book_postings FOR EACH ROW WHEN(NEW.rule_id='monetary-book-rule') EXECUTE FUNCTION monetary_book_failure()").execute(db).await.unwrap();
            no_charge_response(runtime.router(),call("POST",&path,2,"monetary-invoice-post",input.clone()),StatusCode::INTERNAL_SERVER_ERROR).await;
            assert!(sqlx::query_scalar::<_,bool>("SELECT is_called FROM monetary_book_attempt").fetch_one(db).await.unwrap());
            let rollback_invoice:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id=$1").bind(invoice).fetch_one(db).await.unwrap();assert_eq!(rollback_invoice,before);
            let rollback:(i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_journals WHERE id IN('journal-monetary-invoice-post','book-monetary-book-rule-journal-monetary-invoice-post')),(SELECT count(*) FROM finance_book_postings WHERE primary_journal_id='journal-monetary-invoice-post'),(SELECT count(*) FROM finance_financial_events WHERE journal_id IN('journal-monetary-invoice-post','book-monetary-book-rule-journal-monetary-invoice-post')),(SELECT count(*) FROM money_transaction_snapshots WHERE company_id='company' AND source_type='finance_invoice' AND source_id=$1 AND amount_kind='total'),(SELECT count(*) FROM command_receipts WHERE command_id='monetary-invoice-post'),(SELECT count(*) FROM audit_events WHERE command_id='monetary-invoice-post'),(SELECT count(*) FROM outbox_events WHERE command_id='monetary-invoice-post')").bind(invoice).fetch_one(db).await.unwrap();assert_eq!(rollback,(0,0,0,0,0,0,0));
            sqlx::raw_sql("DROP TRIGGER monetary_book_failure ON finance_book_postings;DROP FUNCTION monetary_book_failure();DROP SEQUENCE monetary_book_attempt").execute(db).await.unwrap();
            let posted=no_charge_response(runtime.router(),call("POST",&path,2,"monetary-invoice-post",input.clone()),StatusCode::OK).await;assert_eq!(posted["data"]["status"],"posted");
            let replay=no_charge_response(runtime.router(),call("POST",&path,2,"monetary-invoice-post",input),StatusCode::OK).await;assert_eq!(replay,posted);
            let posted_quote:serde_json::Value=sqlx::query_scalar(quote_sql).bind("finance_invoice").bind(invoice).bind("total").fetch_one(db).await.unwrap();assert_eq!(posted_quote,frozen);
            let carried:(String,String,String)=sqlx::query_as("SELECT carried_source_type,carried_source_id,carried_amount_kind FROM money_transaction_snapshots WHERE company_id='company' AND source_type='finance_invoice' AND source_id=$1 AND amount_kind='total'").bind(invoice).fetch_one(db).await.unwrap();assert_eq!(carried,("finance_invoice".into(),invoice.into(),"draft:v1".into()));
            let journal=posted["data"]["journalId"].as_str().unwrap();
            let secondary:String=sqlx::query_scalar("SELECT secondary_journal_id FROM finance_book_postings WHERE primary_journal_id=$1 AND rule_id='monetary-book-rule'").bind(journal).fetch_one(db).await.unwrap();
            let secondary_read=no_charge_response(runtime.router(),call("GET",&format!("/api/v1/holdings/acme/finance/journals/{secondary}/workspace"),2,"",json!(null)),StatusCode::OK).await;assert_eq!(secondary_read["data"]["journal"]["id"],secondary);assert_eq!(secondary_read["data"]["journal"]["status"],"posted");assert_eq!(secondary_read["data"]["journal"]["chart_id"],"monetary-secondary");
            let primary_read=no_charge_response(runtime.router(),call("GET",&format!("/api/v1/holdings/acme/finance/journals/{journal}"),2,"",json!(null)),StatusCode::OK).await;assert_eq!(primary_read["data"]["journal"]["id"],journal);
            let mut spoof=call("GET","/api/v1/holdings/acme/finance/journals/book-monetary-fabricated/workspace",2,"",json!(null));spoof.headers_mut().insert("x-erpx-source-kind","field_service_invoice".parse().unwrap());let denied=no_charge_response(runtime.router(),spoof,StatusCode::FORBIDDEN).await;assert_eq!(denied["error"]["code"],"PACKAGE_REQUIRED");assert!(denied.get("data").is_none());
            let mut foreign=call("GET",&format!("/api/v1/holdings/acme/finance/journals/{secondary}/workspace"),2,"",json!(null));foreign.headers_mut().insert("x-erpx-context-company","foreign".parse().unwrap());let denied=no_charge_response(runtime.router(),foreign,StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"WORK_CONTEXT_CHANGED");assert!(denied.get("data").is_none());
            let expected_book=if discount==5000{75000}else{150000};let books:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT sum(debit_minor)::bigint FROM finance_journal_lines WHERE journal_id=$1),(SELECT sum(credit_minor)::bigint FROM finance_journal_lines WHERE journal_id=$1),(SELECT count(*) FROM finance_financial_events WHERE journal_id IN($1,$2))").bind(&secondary).bind(journal).fetch_one(db).await.unwrap();assert_eq!(books,(expected_book,expected_book,2));
            let expected=if discount==5000{75000}else{150000};
            let ledger:(String,i64,i64,i64,i64)=sqlx::query_as("SELECT i.status,i.total_minor,i.outstanding_minor,sum(l.debit_minor)::bigint,sum(l.credit_minor)::bigint FROM finance_invoices i JOIN finance_journal_lines l ON l.journal_id=i.journal_id WHERE i.id=$1 GROUP BY i.id").bind(invoice).fetch_one(db).await.unwrap();assert_eq!(ledger,("posted".into(),expected,expected,expected,expected));
            if discount==0{
                let reassignment=no_charge_response(runtime.router(),call("POST",root,0,"monetary-reassignment-request",json!({"entity":"request","agreementId":agreement,"agreementVersion":2,"title":"Actual reviewed technician reassignment","issue":"Inspect original equipment","site":"Customer site","technicianId":"fixture-a","scheduledAt":now+60000})),StatusCode::OK).await;
                let reassignment=reassignment["data"]["id"].as_str().unwrap();
                sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='field-fixture-a'").bind(now).execute(db).await.unwrap();
                for path in [format!("{root}/workspace"),format!("{root}/orders/{reassignment}/detail"),format!("{root}/work")]{
                    let denied=no_charge_response(runtime.router(),call("GET",&path,0,"",json!(null)),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"WORK_ROLE_UNAVAILABLE");assert!(denied.get("data").is_none());
                }
                let work=no_charge_response(runtime.router(),call("GET",&format!("{root}/work"),1,"",json!(null)),StatusCode::OK).await;
                let item=work["data"].as_array().unwrap().iter().find(|r|r["id"]==reassignment).unwrap();assert!(item["blocker"].as_str().unwrap().contains("no longer eligible"));
                no_charge_response(runtime.router(),call("POST",root,1,"monetary-reassignment",json!({"id":reassignment,"action":"schedule","expectedVersion":1,"technicianId":"fixture-b","scheduledAt":now+120000,"reason":"Independent reviewer reassigns after technician access ended"})),StatusCode::OK).await;
                let detail=no_charge_response(runtime.router(),call("GET",&format!("{root}/orders/{reassignment}/detail"),1,"",json!(null)),StatusCode::OK).await;assert_eq!(detail["data"]["order"]["technician_id"],"fixture-b");
                sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('monetary-department','department','monetary-dept','Scoped department','branch','company','branch','active')").execute(db).await.unwrap();
                let scoped=|mut request:Request<Body>|{request.headers_mut().insert("x-erpx-workspace-branch","north".parse().unwrap());request.headers_mut().insert("x-erpx-workspace-department","monetary-dept".parse().unwrap());request};
                let workspace=no_charge_response(runtime.router(),scoped(call("GET",&format!("{root}/workspace"),1,"",json!(null))),StatusCode::OK).await;assert!(!workspace["data"]["assets"].as_array().unwrap().iter().any(|r|r["id"]=="covered-asset"));
                let denied=no_charge_response(runtime.router(),scoped(call("POST",root,1,"monetary-foreign-department-asset",json!({"entity":"agreement","branchId":"branch","departmentId":"monetary-department","customerId":"customer","assetId":"covered-asset","title":"Cross department asset agreement","startDate":"2026-01-01","endDate":"2027-12-31","currency":"THB","visitRateMinor":150000,"warrantyDiscountBps":0}))),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"CONFLICT");
                let refused:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_agreements WHERE id='service-agreement-monetary-foreign-department-asset'),(SELECT count(*) FROM command_receipts WHERE command_id='monetary-foreign-department-asset')").fetch_one(db).await.unwrap();assert_eq!(refused,(0,0));
            }
            runtime.close().await;
        })).await;
    }
}
#[cfg(feature="files-scan-worker")]
#[tokio::test]
async fn fully_covered_field_invoice_retains_actual_visit_files(){Box::pin(receipt_workspace_case(false,false,false,true,true)).await;}
pub(super) async fn no_charge_response(router:axum::Router,request:Request<Body>,expected:StatusCode)->serde_json::Value{
    let label=format!("{} {}",request.method(),request.uri());let response=tokio::spawn(async move{router.oneshot(request).await}).await.unwrap().unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,expected,"{label}: {}",String::from_utf8_lossy(&bytes));serde_json::from_slice(&bytes).unwrap()
}
async fn native_service_commercial_withdrawal(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime){
    use erpx_server_shared::public::Clock;
    use erpx_server_identity_access::public::{platform_commercial_authority::*,account_security::*,*};
    use erpx_server_commercial::public::{purchases::*,package_review::*};
    use erpx_service::field_service_package_settlement::{preview_service_package_batch,apply_service_package_batch};
    let system=&p.review.system;let db=system.control.raw();let clock=system.scenario.clock.as_ref();let now=clock.now().get();
    let policy=account::fixture::account::policy();let registry=account::fixture::account::holding_registry();
    // Operator/role publication is explicit fixture data. Sign-in, SSO, MFA,
    // raw five-owner settlement, review and every grant write are native.
    sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('platform','platform_field_settlement','Service settlement reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('platform','platform_field_settlement',1,'[\"platform.commercial.read\",\"platform.commercial.manage\"]','[]','[\"platform\"]','fixture',0);INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version)VALUES('platform','fixture-a','local:fixture-a','active','local',1)").execute(db).await.unwrap();
    sqlx::query("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until)VALUES('native-service-settlement','fixture-a','platform_field_settlement','[]',$1,$2)").bind(now-1000).bind(now+3600000).execute(db).await.unwrap();
    let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
    let device=DeviceProof{active:Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session,&child.token)),..Default::default()};
    let actor=PlatformCommercialSelection{user_id:"fixture-a".into(),session_id:child.result.session_id.clone(),assignment_id:"native-service-settlement".into(),role:"platform_field_settlement".into(),role_revision:1};
    let commercial=runtime.router();
    let commercial_cookie=format!("__Host-erpx-product={}",child.token.expose());
    let request=|method:&str,path:&str,key:&str,body:serde_json::Value|Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&commercial_cookie).header("content-type","application/json").header("idempotency-key",key).body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap();
    // Public lifecycle admission precedes shape validation. A client-provided
    // package label never substitutes an authenticated Platform authority.
    let anonymous=Request::builder().method("POST").uri("/api/v1/platform/commercial/packages/preview").header("host","acme.example.test").header("origin","https://acme.example.test").header("content-type","application/json").body(Body::from("{}")).unwrap();
    let denied=no_charge_response(commercial.clone(),anonymous,StatusCode::UNAUTHORIZED).await;assert_eq!(denied["error"]["code"],"UNAUTHENTICATED");
    let foreign=Request::builder().method("POST").uri("/api/v1/platform/commercial/packages/preview").header("host","acme.example.test").header("origin","https://foreign.example.test").header("content-type","application/json").body(Body::from("invalid-json")).unwrap();
    let denied=no_charge_response(commercial.clone(),foreign,StatusCode::FORBIDDEN).await;assert_eq!(denied["error"]["code"],"ORIGIN_FORBIDDEN");
    let companies=no_charge_response(commercial.clone(),request("GET","/api/v1/platform/commercial","",json!(null)),StatusCode::OK).await;
    assert_eq!(companies["data"]["canApply"],true);
    assert!(companies["data"]["companies"].as_array().unwrap().iter().any(|row|row["id"]=="company"&&row["holdingId"]=="h001"&&row["name"].as_str().is_some_and(|s|!s.is_empty())&&row["holding"].as_str().is_some_and(|s|!s.is_empty())));
    let enrolled=no_charge_response(commercial.clone(),request("POST","/api/v1/account/iam/mfa/enroll","",json!({})),StatusCode::OK).await;assert_eq!(enrolled["data"]["secret"].as_str().unwrap().len(),32);
    let keys=Arc::new(AccountMfaKeyring::new("fixture".into(),vec![("fixture".into(),[7;32])]).unwrap());
    let envelope:String=sqlx::query_scalar("SELECT secret FROM iam_mfa WHERE user_id='fixture-a'").fetch_one(db).await.unwrap();
    let code=keys.open("fixture-a",&envelope).unwrap().code(now/30000).unwrap();
    let verified=no_charge_response(commercial.clone(),request("POST","/api/v1/account/iam/mfa/verify","",json!({"code":code})),StatusCode::OK).await;assert!(verified["data"]["verifiedUntil"].as_i64().unwrap()>now);
    let holding=erpx_server_shared::public::HoldingId::new("h001").unwrap();let company=erpx_server_shared::public::CompanyId::new("company").unwrap();
    let signer=Arc::new(PackageLifecycleReviewSigner::new("fixture-only-secret-32-characters-minimum").unwrap());
    for state in ["draining","inactive"]{
        let catalogue=current_package_catalogue().unwrap();
        let mut read=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *read).await.unwrap();lock_commercial_authority_sources(&mut read).await.unwrap();
        let facts=PgCompanyPurchaseReader::new(holding.clone(),company.clone(),&mut read,&catalogue).read(&company).await.unwrap();read.rollback().await.unwrap();
        let changes:Vec<_>=facts.grants.iter().filter(|g|matches!(g.package_id.as_str(),"service"|"service-parts-assets")&&g.state!=state).map(|g|BatchChange{expected_version:g.version,grant:CompanyPackageGrant{state:state.into(),version:g.version+1,reason:"Reviewed native Service settlement".into(),evidence:"test:actual-native-service-settlement".into(),..g.clone()}}).collect();
        if changes.is_empty(){continue;}
        let input=GrantBatch{company_id:company.as_str().into(),expected_revision:package_grant_revision(company.as_str(),&facts.grants).unwrap(),idempotency_key:format!("native-service-{state}-batch"),changes};
        let mut control=db.begin().await.unwrap();
        let read_authority=verify_platform_commercial_authority(&mut control,"acme.example.test",device.clone(),policy.clone(),registry.clone(),&holding,actor.clone(),PlatformCommercialOperation::Read,clock.now().get()).await.unwrap();
        let mut unit=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unit).await.unwrap();
        let preview=Box::pin(preview_service_package_batch(&mut unit,&mut control,&read_authority,clock,holding.clone(),&input,&signer)).await.unwrap();
        let forbidden=Box::pin(apply_service_package_batch(&mut unit,&mut control,&read_authority,clock,holding.clone(),&input,&signer,&preview.review.token)).await;
        assert!(matches!(forbidden,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="PLATFORM_COMMERCIAL_REQUIRED"));unit.rollback().await.unwrap();control.rollback().await.unwrap();
        let view=no_charge_response(commercial.clone(),request("GET","/api/v1/platform/commercial/packages?holdingId=h001&companyId=company","",json!(null)),StatusCode::OK).await;
        assert_eq!(view["data"]["companyId"],"company");assert_eq!(view["data"]["revision"],input.expected_revision);assert_eq!(view["data"]["billingMode"],"sandbox");
        let mut wire=input.clone();for change in &mut wire.changes{change.grant.reason=format!("\u{feff}  {}  \u{202f}",change.grant.reason);change.grant.evidence=format!("  {}  ",change.grant.evidence);}
        let http_preview=no_charge_response(commercial.clone(),request("POST","/api/v1/platform/commercial/packages/preview","",json!({"holdingId":"h001","request":wire})),StatusCode::OK).await;
        assert_eq!(http_preview["data"]["comparison"],serde_json::to_value(&preview.comparison).unwrap());
        let token=http_preview["data"]["review"]["token"].as_str().unwrap();
        let apply_body=json!({"holdingId":"h001","request":input,"token":token});
        if state=="inactive" {
            let before:(String,i64,i64)=sqlx::query_as("SELECT (SELECT name FROM finance_parties WHERE id='customer'),(SELECT count(*) FROM company_package_grant_events WHERE company_id='company'),(SELECT count(*) FROM company_package_history_rights WHERE company_id='company')").fetch_one(p.pool().raw()).await.unwrap();
            sqlx::raw_sql("CREATE SEQUENCE native_settlement_fault_attempt;CREATE FUNCTION native_settlement_fault() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM nextval('native_settlement_fault_attempt');UPDATE finance_parties SET name=name||' late source' WHERE id='customer';RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER native_settlement_fault AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.id='native-service-inactive-batch') EXECUTE FUNCTION native_settlement_fault()").execute(p.pool().raw()).await.unwrap();
            let result=no_charge_response(commercial.clone(),request("POST","/api/v1/platform/commercial/packages/apply",&input.idempotency_key,apply_body.clone()),StatusCode::CONFLICT).await;
            assert_eq!(result["error"]["code"],"FIELD_SERVICE_CUSTOMERS_CHANGED");
            let attempted:bool=sqlx::query_scalar("SELECT is_called FROM native_settlement_fault_attempt").fetch_one(p.pool().raw()).await.unwrap();assert!(attempted,"the actual deferred mutation must execute");
            let after:(String,i64,i64)=sqlx::query_as("SELECT (SELECT name FROM finance_parties WHERE id='customer'),(SELECT count(*) FROM company_package_grant_events WHERE company_id='company'),(SELECT count(*) FROM company_package_history_rights WHERE company_id='company')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(after,before);
            let staged:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM company_package_change_sets WHERE id='native-service-inactive-batch'),(SELECT count(*) FROM company_package_review_evidence WHERE change_set_id='native-service-inactive-batch'),(SELECT count(*) FROM company_package_grants WHERE company_id='company' AND package_id IN('service','service-parts-assets') AND state<>'draining')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(staged,(0,0,0));
            sqlx::raw_sql("DROP TRIGGER native_settlement_fault ON company_package_change_sets;DROP FUNCTION native_settlement_fault();DROP SEQUENCE native_settlement_fault_attempt").execute(p.pool().raw()).await.unwrap();
        }
        let applied=no_charge_response(commercial.clone(),request("POST","/api/v1/platform/commercial/packages/apply",&input.idempotency_key,apply_body.clone()),StatusCode::OK).await;
        assert_eq!(applied["data"]["receipt"],serde_json::to_value(&preview.comparison).unwrap());
        let recovered=no_charge_response(commercial.clone(),request("POST","/api/v1/platform/commercial/packages/apply",&input.idempotency_key,apply_body),StatusCode::OK).await;assert_eq!(recovered,applied);
        let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM company_package_change_sets WHERE id=$1),(SELECT count(*) FROM company_package_review_evidence WHERE change_set_id=$1 AND actor_id='fixture-a' AND assignment_id='native-service-settlement')").bind(&input.idempotency_key).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(counts,(1,1));
    }
}
async fn no_charge_native_invoice(runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,field:&str,reviewer:&str,suffix:&str,now:i64)->String{
    let request=json!({"entity":"request","agreementId":"agreement","agreementVersion":2,"title":"Actual covered routing review","issue":"Inspect covered equipment","site":"Customer site","technicianId":"fixture-a","scheduledAt":now+60000});
    let created=send(&runtime.router(),field,&format!("{suffix}-request"),&request.to_string()).await;assert_eq!(created.0,StatusCode::OK,"{}",created.1);let order=created.1["data"]["id"].as_str().unwrap();
    for (action,version) in [("dispatch",1),("start",2),("complete",3)]{
        let mut body=json!({"id":order,"action":action,"expectedVersion":version,"reason":"Actual covered visit progression"});if action=="complete"{body["laborMinutes"]=json!(5);body["evidence"]=json!("Actual completed warranty work");body["customerConfirmation"]=json!("Customer accepted covered work");}
        let result=send(&runtime.router(),field,&format!("{suffix}-{action}"),&body.to_string()).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);
    }
    let released=send_as(&runtime.router(),reviewer,"agreement-reviewer-b",&format!("{suffix}-billing"),&json!({"id":order,"action":"release-billing","expectedVersion":4,"reason":"Independent covered billing release"}).to_string()).await;assert_eq!(released.0,StatusCode::OK,"{}",released.1);format!("field-invoice-{suffix}-billing")
}
async fn no_charge_native_routing(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,now:i64,field:&str,reviewer:&str,cookie_c:&str)->String{
    let system=&p.review.system;let db=p.pool().raw();
    let original_grants:serde_json::Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(g) ORDER BY package_id),'[]'::jsonb) FROM company_package_grants g WHERE company_id='company'").fetch_one(db).await.unwrap();
    // Authority/roster facts only. All responsibilities/defaults, invoices and
    // confirmations below are actual native public commands under normal SSO.
    sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\")VALUES('fixture-d','Independent No Charge D','d@example.test',1,$1,$1)").bind(now).execute(system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") SELECT 'fixture-d','fixture-d','credential','local:credential','fixture-d',password,$1,$1 FROM account WHERE id='fixture-a'").bind(now).execute(system.control.raw()).await.unwrap();
    sqlx::raw_sql(r#"INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','no_charge_flow_admin','Company invoice administrator',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','no_charge_flow_admin',1,'["finance.read","module.finance.read","organization.manage"]','[]','["holding"]','synthetic',0);INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('no-charge-person-d','fixture-d','company','Independent reviewer D','TH','active','fixture-a',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('no-charge-engagement-d','no-charge-person-d','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture-a',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-d','local:fixture-d','no-charge-person-d','active','local',1)"#).execute(db).await.unwrap();
    for (id,user,role) in [("finance-no-charge-admin","fixture-a","no_charge_flow_admin"),("finance-fixture-d","fixture-d","native_no_charge_review")]{sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,$3,'[\"finance\"]','company',$4,$5,3,'holding')").bind(id).bind(user).bind(role).bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();}
    let mut cookies=Vec::new();for (email,assignment) in [("a@example.test","finance-no-charge-admin"),("d@example.test","finance-fixture-d")]{
        let central=account::login(system,email,None).await;let child=account::child(system,&central,account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let request=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(Body::from(json!({"assignmentId":assignment,"assignmentVersion":3,"revision":0}).to_string())).unwrap();no_charge_response(runtime.router(),request,StatusCode::OK).await;cookies.push(cookie);
    }
    let call=|method:&str,id:&str,action:&str,cookie:&str,assignment:&str,key:&str,body:serde_json::Value|Request::builder().method(method).uri(format!("/api/v1/holdings/acme/finance/invoices/{id}/{action}")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("x-erpx-context-company","company").header("x-erpx-context-revision","1").header("x-erpx-context-assignment",assignment).header("x-erpx-context-assignment-version","3").header("idempotency-key",key).body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap();
    let seed=no_charge_native_invoice(runtime,field,reviewer,"no-charge-default-seed",now).await;
    let view=no_charge_response(runtime.router(),call("GET",&seed,"workspace",&cookies[0],"finance-no-charge-admin","",json!(null)),StatusCode::OK).await;assert_eq!(view["flow"]["canManage"],true);
    let assigned=no_charge_response(runtime.router(),call("POST",&seed,"flow",&cookies[0],"finance-no-charge-admin","no-charge-default-c",json!({"stepKey":"draft","userId":"fixture-c","expectedVersion":view["flow"]["version"],"recordVersion":1,"reason":"Normal default for independent covered review","saveDefault":true})),StatusCode::OK).await;
    let revision:i64=sqlx::query_scalar("SELECT max(revision) FROM flow_defaults WHERE company_id='company' AND kind='invoice'").fetch_one(db).await.unwrap();
    let original_default:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(d) FROM flow_defaults d WHERE company_id='company' AND kind='invoice' AND revision=$1").bind(revision).fetch_one(db).await.unwrap();
    let captured_seed=no_charge_response(runtime.router(),call("GET",&seed,"workspace",cookie_c,"finance-fixture-c","",json!(null)),StatusCode::OK).await;
    let inherited=no_charge_native_invoice(runtime,field,reviewer,"no-charge-inherited",now).await;
    let captured=no_charge_response(runtime.router(),call("GET",&inherited,"workspace",cookie_c,"finance-fixture-c","",json!(null)),StatusCode::OK).await;assert_eq!(captured["noCharge"]["canConfirm"],true);assert_eq!(captured["flow"]["version"],0);assert_eq!(captured["flow"]["defaultsRevision"],revision);
    let input=json!({"expectedVersion":1,"contextKey":captured["noCharge"]["contextKey"],"reason":"Retain the actual captured Company default"});
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SEQUENCE no_charge_default_attempt;CREATE FUNCTION no_charge_default_fault() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM nextval('no_charge_default_attempt');UPDATE flow_defaults SET primary_user_id='fixture-d' WHERE company_id='company' AND revision={revision};RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER no_charge_default_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='no-charge-inherited-confirm') EXECUTE FUNCTION no_charge_default_fault()"))).execute(db).await.unwrap();
    let failed=no_charge_response(runtime.router(),call("POST",&inherited,"confirm-no-charge",cookie_c,"finance-fixture-c","no-charge-inherited-confirm",input.clone()),StatusCode::CONFLICT).await;assert_eq!(failed["error"]["code"],"FLOW_ROUTING_CHANGED");
    let attempted:bool=sqlx::query_scalar("SELECT is_called FROM no_charge_default_attempt").fetch_one(db).await.unwrap();assert!(attempted);
    let primary:String=sqlx::query_scalar("SELECT primary_user_id FROM flow_defaults WHERE company_id='company' AND revision=$1").bind(revision).fetch_one(db).await.unwrap();assert_eq!(primary,"fixture-c");
    let after_default:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(d) FROM flow_defaults d WHERE company_id='company' AND kind='invoice' AND revision=$1").bind(revision).fetch_one(db).await.unwrap();assert_eq!(after_default,original_default);
    let rolled_back:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM finance_invoices WHERE id=$1),(SELECT count(*) FROM flow_routes WHERE kind='invoice' AND record_id=$1),(SELECT count(*) FROM finance_service_no_charge_confirmations WHERE command_id='no-charge-inherited-confirm'),(SELECT count(*) FROM command_receipts WHERE command_id='no-charge-inherited-confirm'),(SELECT count(*) FROM audit_events WHERE command_id='no-charge-inherited-confirm'),(SELECT count(*) FROM outbox_events WHERE command_id='no-charge-inherited-confirm')").bind(&inherited).fetch_one(db).await.unwrap();assert_eq!(rolled_back,(1,0,0,0,0,0));
    sqlx::raw_sql("DROP TRIGGER no_charge_default_fault ON outbox_events;DROP FUNCTION no_charge_default_fault();DROP SEQUENCE no_charge_default_attempt").execute(db).await.unwrap();
    let confirmed=no_charge_response(runtime.router(),call("POST",&inherited,"confirm-no-charge",cookie_c,"finance-fixture-c","no-charge-inherited-confirm",input.clone()),StatusCode::OK).await;
    let route:(i64,i64)=sqlx::query_as("SELECT version,defaults_revision FROM flow_routes WHERE company_id='company' AND kind='invoice' AND record_id=$1").bind(&inherited).fetch_one(db).await.unwrap();assert_eq!(route,(1,revision));
    // Existing-route CAS is distinct from default materialization above.
    // Create/route/confirm through native public commands; late SQL is only the
    // existing disposable fault mechanism, never a source/authority shortcut.
    let routed=Box::pin(async {
        let routed=Box::pin(no_charge_native_invoice(runtime,field,reviewer,"no-charge-existing-route",now)).await;
        let view=no_charge_response(runtime.router(),call("GET",&routed,"workspace",cookie_c,"finance-fixture-c","",json!(null)),StatusCode::OK).await;
        let assigned=no_charge_response(runtime.router(),call("POST",&routed,"flow",&cookies[0],"finance-no-charge-admin","no-charge-existing-route-primary",json!({"stepKey":"draft","userId":"fixture-c","expectedVersion":view["flow"]["version"],"recordVersion":1,"reason":"Explicit current independent reviewer","saveDefault":false})),StatusCode::OK).await;
        let controls=no_charge_response(runtime.router(),call("GET",&routed,"workspace",cookie_c,"finance-fixture-c","",json!(null)),StatusCode::OK).await;assert_eq!(assigned["data"]["version"].as_i64().unwrap(),view["flow"]["version"].as_i64().unwrap().max(1)+1);assert_eq!(controls["flow"]["version"],assigned["data"]["version"]);assert_eq!(controls["noCharge"]["canConfirm"],true);
        let input=json!({"expectedVersion":1,"contextKey":controls["noCharge"]["contextKey"],"reason":"Retain the captured existing route"});
        let before_route:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(r) FROM flow_routes r WHERE company_id='company' AND kind='invoice' AND record_id=$1").bind(&routed).fetch_one(db).await.unwrap();
        let before_invoice:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id=$1").bind(&routed).fetch_one(db).await.unwrap();
        assert!(routed.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'));
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SEQUENCE no_charge_route_attempt;CREATE FUNCTION no_charge_route_fault() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM nextval('no_charge_route_attempt');UPDATE flow_routes SET steps_json=jsonb_set(steps_json::jsonb,'{{draft,primaryUserId}}',to_jsonb('fixture-d'::text))::text,version=version+1 WHERE company_id='company' AND kind='invoice' AND record_id='{routed}';RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER no_charge_route_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='no-charge-existing-route-confirm') EXECUTE FUNCTION no_charge_route_fault()"))).execute(db).await.unwrap();
        let failed=no_charge_response(runtime.router(),call("POST",&routed,"confirm-no-charge",cookie_c,"finance-fixture-c","no-charge-existing-route-confirm",input.clone()),StatusCode::CONFLICT).await;assert_eq!(failed["error"]["code"],"FLOW_ROUTING_CHANGED");
        assert!(sqlx::query_scalar::<_,bool>("SELECT is_called FROM no_charge_route_attempt").fetch_one(db).await.unwrap());
        let after_route:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(r) FROM flow_routes r WHERE company_id='company' AND kind='invoice' AND record_id=$1").bind(&routed).fetch_one(db).await.unwrap();assert_eq!(after_route,before_route);
        let after_invoice:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id=$1").bind(&routed).fetch_one(db).await.unwrap();assert_eq!(after_invoice,before_invoice);
        let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_service_no_charge_confirmations WHERE command_id='no-charge-existing-route-confirm'),(SELECT count(*) FROM command_receipts WHERE command_id='no-charge-existing-route-confirm'),(SELECT count(*) FROM audit_events WHERE command_id='no-charge-existing-route-confirm'),(SELECT count(*) FROM outbox_events WHERE command_id='no-charge-existing-route-confirm'),(SELECT count(*) FROM flow_events WHERE command_id='no-charge-existing-route-confirm')").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0,0,0,0));
        sqlx::raw_sql("DROP TRIGGER no_charge_route_fault ON outbox_events;DROP FUNCTION no_charge_route_fault();DROP SEQUENCE no_charge_route_attempt").execute(db).await.unwrap();
        let confirmed=no_charge_response(runtime.router(),call("POST",&routed,"confirm-no-charge",cookie_c,"finance-fixture-c","no-charge-existing-route-confirm",input.clone()),StatusCode::OK).await;assert_eq!(confirmed["data"]["id"],routed);assert_eq!(confirmed["data"]["version"],2);
        let replay=no_charge_response(runtime.router(),call("POST",&routed,"confirm-no-charge",cookie_c,"finance-fixture-c","no-charge-existing-route-confirm",input),StatusCode::OK).await;assert_eq!(replay,confirmed);
        let event:i64=sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id='no-charge-existing-route-confirm' AND kind='invoice' AND record_id=$1 AND action='post'").bind(&routed).fetch_one(db).await.unwrap();assert_eq!(event,1);routed
    }).await;
    no_charge_response(runtime.router(),call("POST",&seed,"flow",&cookies[0],"finance-no-charge-admin","no-charge-default-d",json!({"stepKey":"draft","userId":"fixture-d","expectedVersion":assigned["data"]["version"],"recordVersion":1,"reason":"Normal reassignment and future-only Company default","saveDefault":true})),StatusCode::OK).await;
    let next_revision:i64=sqlx::query_scalar("SELECT max(revision) FROM flow_defaults WHERE company_id='company' AND kind='invoice'").fetch_one(db).await.unwrap();assert!(next_revision>revision);
    let replay=no_charge_response(runtime.router(),call("POST",&inherited,"confirm-no-charge",cookie_c,"finance-fixture-c","no-charge-inherited-confirm",input),StatusCode::OK).await;assert_eq!(replay,confirmed);
    let denied=no_charge_response(runtime.router(),call("POST",&seed,"confirm-no-charge",cookie_c,"finance-fixture-c","no-charge-wrong-primary",json!({"expectedVersion":1,"contextKey":captured_seed["noCharge"]["contextKey"],"reason":"Original captured controls do not override reassignment"})),StatusCode::FORBIDDEN).await;assert_eq!(denied["error"]["code"],"FLOW_PRIMARY_REQUIRED");
    let old=no_charge_response(runtime.router(),call("GET",&seed,"workspace",cookie_c,"finance-fixture-c","",json!(null)),StatusCode::OK).await;assert_eq!(old["noCharge"]["canConfirm"],false);assert_eq!(old["flow"]["canAct"],false);
    let refused:(String,i64,i64,i64)=sqlx::query_as("SELECT status,version,(SELECT count(*) FROM finance_service_no_charge_confirmations WHERE invoice_id=$1),(SELECT count(*) FROM command_receipts WHERE command_id='no-charge-wrong-primary') FROM finance_invoices WHERE id=$1").bind(&seed).fetch_one(db).await.unwrap();assert_eq!(refused,("draft".into(),1,0,0));
    let next=no_charge_response(runtime.router(),call("GET",&seed,"workspace",&cookies[1],"finance-fixture-d","",json!(null)),StatusCode::OK).await;assert_eq!(next["noCharge"]["canConfirm"],true);
    assert_eq!(next["flow"]["canAct"],true);
    let result=no_charge_response(runtime.router(),call("POST",&seed,"confirm-no-charge",&cookies[1],"finance-fixture-d","no-charge-reassigned-confirm",json!({"expectedVersion":1,"contextKey":next["noCharge"]["contextKey"],"reason":"Independent current reviewer accepts actual zero demand"})),StatusCode::OK).await;assert_eq!(result["data"]["status"],"no_charge_confirmed");
    let replay=no_charge_response(runtime.router(),call("POST",&seed,"confirm-no-charge",&cookies[1],"finance-fixture-d","no-charge-reassigned-confirm",json!({"expectedVersion":1,"contextKey":next["noCharge"]["contextKey"],"reason":"Independent current reviewer accepts actual zero demand"})),StatusCode::OK).await;assert_eq!(replay,result);
    let events:i64=sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id='no-charge-reassigned-confirm' AND kind='invoice' AND record_id=$1 AND action='post' AND actor_user_id='fixture-d'").bind(&seed).fetch_one(db).await.unwrap();assert_eq!(events,1);
    let default_events:i64=sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id='no-charge-inherited-confirm' AND kind='invoice' AND record_id=$1 AND action='post' AND actor_user_id='fixture-c'").bind(&inherited).fetch_one(db).await.unwrap();assert_eq!(default_events,1);
    let monetary:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_journals WHERE source_type='invoice' AND source_id=ANY($1)),(SELECT count(*) FROM finance_allocations WHERE invoice_id=ANY($1))").bind(vec![seed,inherited,routed]).fetch_one(db).await.unwrap();assert_eq!(monetary,(0,0));
    let after_grants:serde_json::Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(to_jsonb(g) ORDER BY package_id),'[]'::jsonb) FROM company_package_grants g WHERE company_id='company'").fetch_one(db).await.unwrap();assert_eq!(after_grants,original_grants);
    cookies[1].clone()
}

// Reference/IAM fixtures only; the second Company's order, billing and
// confirmation below are actual native commands, with normal Account/SSO.
async fn no_charge_second_company(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,now:i64)->(Request<Body>,String){
    let system=&p.review.system;let db=p.pool().raw();
    sqlx::raw_sql(r#"INSERT INTO "user" SELECT(jsonb_populate_record(NULL::"user",to_jsonb(u)||jsonb_build_object('id','c2-'||id,'email','c2-'||email))).* FROM "user" u WHERE id IN('fixture-a','fixture-b','fixture-c');
    INSERT INTO account SELECT(jsonb_populate_record(NULL::account,to_jsonb(a)||jsonb_build_object('id','c2-'||id,'accountId','c2-'||"accountId",'userId','c2-'||"userId"))).* FROM account a WHERE id IN('fixture-a','fixture-b','fixture-c')"#).execute(system.control.raw()).await.unwrap();
    let mut tx=db.begin().await.unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('company-two','company','second','Second Company',NULL,'company-two',NULL,'active'),('branch-two','branch','second-north','Second North','company-two','company-two','branch-two','active')").execute(&mut *tx).await.unwrap();
    // Fixed fixture tables/JSON fields, never arbitrary caller identifiers.
    for (table,filter,changes) in [
        ("finance_charts","id='chart'",r#"{"id":"c2-chart","company_id":"company-two"}"#),
        ("finance_periods","id='period'",r#"{"id":"c2-period","company_id":"company-two"}"#),
        ("finance_parties","id='customer'",r#"{"id":"c2-customer","company_id":"company-two"}"#),
        ("company_fx_rates","id='rate'",r#"{"id":"c2-rate","company_id":"company-two"}"#),
        ("operational_assets","id='covered-asset'",r#"{"id":"c2-asset","company_id":"company-two","branch_id":"branch-two"}"#),
        ("field_service_agreements","id='agreement'",r#"{"id":"c2-agreement","company_id":"company-two","branch_id":"branch-two","customer_id":"c2-customer","asset_id":"c2-asset","created_by":"c2-fixture-a","reviewed_by":"c2-fixture-b"}"#),
        ("field_service_agreement_events","id='agreement-review'",r#"{"id":"c2-agreement-review","agreement_id":"c2-agreement","actor_id":"c2-fixture-b"}"#),
        ("money_transaction_snapshots","company_id='company' AND source_type='field_service_agreement' AND source_id='agreement'",r#"{"company_id":"company-two","source_id":"c2-agreement","fx_rate_id":"c2-rate"}"#),
    ]{
        let sql=format!("INSERT INTO {table} SELECT (jsonb_populate_record(NULL::{table},to_jsonb(src)||$1::jsonb)).* FROM {table} src WHERE {filter}");
        sqlx::query(sqlx::AssertSqlSafe(sql)).bind(serde_json::from_str::<serde_json::Value>(changes).unwrap()).execute(&mut *tx).await.unwrap();
    }
    sqlx::raw_sql(r#"INSERT INTO finance_accounts SELECT(jsonb_populate_record(NULL::finance_accounts,to_jsonb(a)||jsonb_build_object('id','c2-'||id,'company_id','company-two','chart_id','c2-chart'))).* FROM finance_accounts a WHERE company_id='company' AND chart_id='chart';
    INSERT INTO commerce_settings SELECT(jsonb_populate_record(NULL::commerce_settings,to_jsonb(s)||jsonb_build_object('company_id','company-two','chart_id','c2-chart','receivable_account_id','c2-'||receivable_account_id,'payable_account_id','c2-'||payable_account_id,'revenue_account_id','c2-'||revenue_account_id,'inventory_account_id','c2-'||inventory_account_id,'clearing_account_id','c2-'||clearing_account_id,'cogs_account_id','c2-'||cogs_account_id,'input_tax_account_id','c2-'||input_tax_account_id,'output_tax_account_id','c2-'||output_tax_account_id,'cash_account_id','c2-'||cash_account_id))).* FROM commerce_settings s WHERE company_id='company';
    INSERT INTO hr_people SELECT(jsonb_populate_record(NULL::hr_people,to_jsonb(p)||jsonb_build_object('id','c2-'||id,'user_id','c2-'||user_id,'company_id','company-two'))).* FROM hr_people p WHERE company_id='company';
    INSERT INTO hr_engagements SELECT(jsonb_populate_record(NULL::hr_engagements,to_jsonb(e)||jsonb_build_object('id','c2-'||id,'person_id','c2-'||person_id,'company_id','company-two'))).* FROM hr_engagements e WHERE company_id='company';
    INSERT INTO iam_members SELECT(jsonb_populate_record(NULL::iam_members,to_jsonb(m)||jsonb_build_object('company_id','company-two','user_id','c2-'||user_id,'principal_id','local:c2-'||user_id,'person_id','c2-'||person_id))).* FROM iam_members m WHERE company_id='company';
    "#).execute(&mut *tx).await.unwrap();
    for (id,user,role,module) in [("field-two-a","c2-fixture-a","custom_field_parts","field_service"),("field-two-b","c2-fixture-b","field_agreement_reviewer","field_service"),("finance-two-c","c2-fixture-c","native_no_charge_review","finance")]{
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,$3,$4,'company-two',$5,$6,3,'holding')").bind(id).bind(user).bind(role).bind(json!([module]).to_string()).bind(now-1000).bind(now+3_600_000).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    crate::facts::seed_initial_service_purchases(p,"company-two",true).await.unwrap();
    let grants_before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id='company-two'").fetch_one(p.pool().raw()).await.unwrap();
    let duplicate=crate::facts::seed_initial_service_purchases(p,"company-two",true).await;
    assert!(matches!(duplicate,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e))if e.code=="STALE_PACKAGE_GRANT"));
    let grants_after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id='company-two'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(grants_before,grants_after);
    // A single-Company Field composition suffices to create this independent
    // source. Finance on the candidate above mounts both declared Companies.
    let mut config=private_configuration::document(&system.control,&system.holdings).await;
    config["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company-two"}]);
    config["financeJournalReads"]=config["fieldServiceReads"].clone();
    for flag in ["fieldServiceRequests","fieldServiceVisits","fieldServiceVisitCompletions","fieldServiceBillingReleases","financeFieldServiceInvoiceReads"]{config[flag]=json!(["h001"]);}
    let field=Box::pin(assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets))).await.unwrap();
    let mut cookies=Vec::new();
    for (email,assignment) in [("c2-a@example.test","field-two-a"),("c2-b@example.test","field-two-b"),("c2-c@example.test","finance-two-c")]{
        let central=account::login(system,email,None).await;let child=account::child(system,&central,account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let request=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company-two").body(Body::from(json!({"assignmentId":assignment,"assignmentVersion":3,"revision":0}).to_string())).unwrap();
        no_charge_response(field.router(),request,StatusCode::OK).await;cookies.push(cookie);
    }
    let command=|cookie:&str,assignment:&str,key:&str,body:serde_json::Value|Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("x-erpx-context-company","company-two").header("x-erpx-context-revision","1").header("x-erpx-context-assignment",assignment).header("x-erpx-context-assignment-version","3").header("idempotency-key",key).body(Body::from(body.to_string())).unwrap();
    let created=no_charge_response(field.router(),command(&cookies[0],"field-two-a","c2-request",json!({"entity":"request","agreementId":"c2-agreement","agreementVersion":2,"title":"Second Company covered visit","issue":"Inspect covered equipment","site":"Second Company site","technicianId":"c2-fixture-a","scheduledAt":now+60000})),StatusCode::OK).await;
    let order=created["data"]["id"].as_str().unwrap();
    for (action,version) in [("dispatch",1),("start",2),("complete",3),("release-billing",4)]{
        let mut body=json!({"id":order,"action":action,"expectedVersion":version,"reason":"Actual second Company progression"});
        if action=="complete"{body["laborMinutes"]=json!(5);body["evidence"]=json!("Second Company actual completed work");body["customerConfirmation"]=json!("Second Company customer acceptance");}
        let actor=if action=="release-billing"{1}else{0};let assignment=if actor==1{"field-two-b"}else{"field-two-a"};
        no_charge_response(field.router(),command(&cookies[actor],assignment,&format!("c2-{action}"),body),StatusCode::OK).await;
    }
    field.close().await;
    let invoice="field-invoice-c2-release-billing";
    let read=Request::builder().uri(format!("/api/v1/holdings/acme/finance/invoices/{invoice}/workspace")).header("host","acme.example.test").header("cookie",&cookies[2]).header("x-erpx-context-company","company-two").header("x-erpx-context-assignment","finance-two-c").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").body(Body::empty()).unwrap();
    let view=no_charge_response(runtime.router(),read,StatusCode::OK).await;assert_eq!(view["noCharge"]["canConfirm"],true);assert_eq!(view["data"]["invoice"]["company_id"],"company-two");
    let request=Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/finance/invoices/{invoice}/confirm-no-charge")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookies[2]).header("x-erpx-context-company","company-two").header("x-erpx-context-revision","1").header("x-erpx-context-assignment","finance-two-c").header("x-erpx-context-assignment-version","3").header("idempotency-key","native-no-charge-two").body(Body::from(json!({"expectedVersion":1,"contextKey":view["noCharge"]["contextKey"],"reason":"Independent second Company zero-demand confirmation"}).to_string())).unwrap();
    (request,invoice.into())
}

async fn no_charge_real_parts_journal(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,now:i64,cookie_d:&str,field_cookie:&str){
    let system=&p.review.system;let db=p.pool().raw();
    eprintln!("native Field cost: issue original spare");
    // This is a separate native Parts application operation, not a nested
    // No-Charge owner transaction. Schedule its owned command as the runtime
    // would, avoiding a debug-only nested cohort/Parts poll stack.
    let parts=erpx_service::field_service_parts::FieldServicePartsAdministration::new(
        p.review.authorizer.clone(),
        erpx_service::field_service_parts::FieldServicePartsConfiguration::new(
            erpx_service::field_service_parts::FieldServicePartsEnvironment::Development,
            erpx_server_shared::public::HoldingId::new("h001").unwrap(),
            erpx_server_shared::public::CompanyId::new("company").unwrap(),
            erpx_server_commercial::public::purchases::PackageEnforcementMode::Enforced,
        ).unwrap(),
        Arc::new(crate::facts::WriteClock(std::sync::atomic::AtomicI64::new(now))),
        system.scenario.clock.clone(),
    );
    let device=p.review.maker.clone();let selection=p.selection.clone();
    tokio::spawn(async move{
        Box::pin(parts.execute("acme.example.test",device,selection,"issue-order".into(),br#"{"action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":3000,"entryDate":"2026-01-10","reason":"Original covered equipment consumes a real valued spare"}"#.to_vec(),"no-charge-original-spare".into())).await
    }).await.unwrap().unwrap();
    let journal:String=sqlx::query_scalar("SELECT journal_id FROM operational_inventory_consumptions WHERE id='operational-part-no-charge-original-spare' AND company_id='company'").fetch_one(db).await.unwrap();
    let blocked=send(&runtime.router(),field_cookie,"no-charge-cost-unposted",r#"{"id":"issue-order","action":"complete","expectedVersion":4,"laborMinutes":5,"evidence":"Covered replacement tested","customerConfirmation":"Customer accepted covered work","reason":"Cost must be independently posted first"}"#).await;
    assert_eq!(blocked.0,StatusCode::CONFLICT,"{}",blocked.1);
    let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;let admin=format!("__Host-erpx-product={}",child.token.expose());
    let select=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&admin).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"finance-no-charge-admin","assignmentVersion":3,"revision":0}"#)).unwrap();no_charge_response(runtime.router(),select,StatusCode::OK).await;
    let call=|method:&str,action:&str,cookie:&str,assignment:&str,key:&str,body:serde_json::Value|Request::builder().method(method).uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/{action}")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("x-erpx-context-company","company").header("x-erpx-context-revision","1").header("x-erpx-context-assignment",assignment).header("x-erpx-context-assignment-version","3").header("idempotency-key",key).body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap();
    eprintln!("native Field cost: read current responsibility");
    let captured=no_charge_response(runtime.router(),call("GET","flow",&admin,"finance-no-charge-admin","",json!(null)),StatusCode::OK).await;
    assert_eq!(captured["data"]["kind"],"journal");assert_eq!(captured["data"]["canManage"],true);assert_eq!(captured["data"]["canAct"],false);
    assert_eq!(captured["runtime"],"rust");assert_eq!(captured["workspace"]["journal"]["source_type"],"field_service_parts");
    assert_eq!(captured["access"]["sourceOnly"],true);assert_eq!(captured["access"]["canPost"],false);
    assert_eq!(captured["access"]["purchaseControls"]["finance.accounting"]["history"],false);
    assert_eq!(captured["access"]["purchaseControls"]["finance.service-invoice"]["history"],true);

    let input=json!({"stepKey":"draft","userId":"fixture-d","expectedVersion":captured["data"]["version"],"recordVersion":1,"reason":"Independent current review of the original real covered spare cost","saveDefault":false});
    let assigned=no_charge_response(runtime.router(),call("POST","flow",&admin,"finance-no-charge-admin","no-charge-cost-assign",input.clone()),StatusCode::OK).await;
    let repeated=no_charge_response(runtime.router(),call("POST","flow",&admin,"finance-no-charge-admin","no-charge-cost-assign",input),StatusCode::OK).await;assert_eq!(repeated,assigned);
    let work_request=||{let mut request=call("GET","flow",cookie_d,"finance-fixture-d","",json!(null));*request.uri_mut()="/api/v1/holdings/acme/finance/field-work".parse().unwrap();request};
    let work=no_charge_response(runtime.router(),work_request(),StatusCode::OK).await;
    let item=work["data"].as_array().unwrap().iter().find(|item|item["type"]=="journal"&&item["id"]==journal).expect("native Field journal responsibility remains in personal Work");
    assert_eq!(item["canAct"],true);assert_eq!(item["queue"],"todo");assert_eq!(item["flow"]["current"]["primary"]["userId"],"fixture-d");assert_eq!(work["context"]["assignmentId"],"finance-fixture-d");
    let mut stale_work=work_request();stale_work.headers_mut().insert("x-erpx-context-revision","0".parse().unwrap());no_charge_response(runtime.router(),stale_work,StatusCode::CONFLICT).await;
    let mut foreign_work=work_request();foreign_work.headers_mut().insert("x-erpx-context-company","company-without-selection".parse().unwrap());let denied=runtime.router().oneshot(foreign_work).await.unwrap();assert!(matches!(denied.status().as_u16(),403|404|409));
    let legacy=sqlx::query("UPDATE flow_routes SET version=version+1 WHERE company_id='company' AND kind='journal' AND record_id=$1").bind(&journal).execute(db).await.unwrap_err();
    assert_eq!(legacy.as_database_error().and_then(|e|e.constraint()),Some("native_field_journal_flow_required"));
    let legacy_notice=sqlx::query("UPDATE staff_notifications SET message='Legacy overwritten responsibility' WHERE id='field-journal-flow-notice-no-charge-cost-assign'").execute(db).await.unwrap_err();
    assert_eq!(legacy_notice.as_database_error().and_then(|e|e.constraint()),Some("native_field_journal_notice_required"));

    let selected=no_charge_response(runtime.router(),call("GET","flow",cookie_d,"finance-fixture-d","",json!(null)),StatusCode::OK).await;
    assert_eq!(selected["data"]["current"]["primary"]["userId"],"fixture-d");assert_eq!(selected["data"]["canAct"],true);
    assert_eq!(selected["access"]["canPost"],true);
    let mut foreign=call("POST","post",cookie_d,"finance-fixture-d","no-charge-cost-wrong-company",json!({"expectedVersion":1}));
    foreign.headers_mut().insert("x-erpx-context-company","company-two".parse().unwrap());
    no_charge_response(runtime.router(),foreign,StatusCode::CONFLICT).await;
    let posted=no_charge_response(runtime.router(),call("POST","post",cookie_d,"finance-fixture-d","no-charge-cost-post",json!({"expectedVersion":1})),StatusCode::OK).await;
    assert_eq!(posted["data"]["status"],"posted");
    let after=no_charge_response(runtime.router(),call("GET","flow",cookie_d,"finance-fixture-d","",json!(null)),StatusCode::OK).await;assert_eq!(after["data"]["current"],serde_json::Value::Null);
    assert_eq!(after["workspace"]["journal"]["status"],"posted");assert_eq!(after["access"]["canPost"],false);
    let cost:(String,i64,i64)=sqlx::query_as("SELECT j.status,sum(l.debit_minor)::bigint,sum(l.credit_minor)::bigint FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.id=$1 GROUP BY j.status").bind(&journal).fetch_one(db).await.unwrap();assert_eq!(cost,("posted".into(),600,600));
    let stock:(i64,i64)=sqlx::query_as("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap();assert_eq!(stock,(0,0));
    let mut settlement=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *settlement).await.unwrap();
    erpx_server_inventory::public::field_service_consumption_finance::lock_consumption_finance_sources(&mut settlement).await.unwrap();
    let raw=erpx_server_inventory::public::field_service_consumption_finance::read_service_settlement_consumptions(&mut settlement,&erpx_server_shared::public::CompanyId::new("company").unwrap(),&["issue-order".into()]).await.unwrap();
    assert_eq!(raw.data().len(),1);assert_eq!(raw.data()[0].id,"operational-part-no-charge-original-spare");assert_eq!(raw.data()[0].journal_id,journal);assert_eq!((raw.data()[0].quantity_milli,raw.data()[0].value_minor),(3000,600));raw.require_current(&mut settlement).await.unwrap();settlement.rollback().await.unwrap();

    let eligible:Vec<(String,bool)>=sqlx::query_as("SELECT command_id,erpx_native_field_journal_outbox_eligible(e) FROM outbox_events e WHERE command_id IN('no-charge-cost-assign','no-charge-cost-post') ORDER BY command_id").fetch_all(db).await.unwrap();
    if eligible.iter().any(|(_,ok)|!ok){
        let facts:Vec<serde_json::Value>=sqlx::query_scalar("SELECT jsonb_build_object('command',e.command_id,'type',e.aggregate_type,'version',e.aggregate_version,'payload',e.payload_json::jsonb-'authority','operation',r.operation,'record',r.record_id,'result',r.result_json::jsonb,'source',j.source_id,'consumption',p.id,'sourceOrder',p.source_id,'sourceActor',p.created_by,'issue',issue.command_id,'issueActor',issue.actor_id,'issueOrder',issue.record_id,'sameCompany',issue.authority_json::jsonb->>'companyId'=p.company_id,'flowKind',f.kind,'flowAction',f.action,'flowVersion',f.record_version,'flowNext',f.next_step_key,'receiptAt',r.created_at,'eventAt',e.created_at,'flowAt',f.created_at,'audit',(SELECT jsonb_agg(jsonb_build_object('action',a.action,'type',a.record_type,'record',a.record_id,'version',a.record_version,'at',a.created_at,'sameAuthority',a.authority_json=r.authority_json)) FROM audit_events a WHERE a.command_id=e.command_id)) FROM outbox_events e LEFT JOIN command_receipts r ON r.command_id=e.command_id LEFT JOIN finance_journals j ON j.id=r.record_id LEFT JOIN operational_inventory_consumptions p ON p.journal_id=j.id LEFT JOIN command_receipts issue ON p.id='operational-part-'||issue.command_id LEFT JOIN flow_events f ON f.command_id=e.command_id WHERE e.command_id IN('no-charge-cost-assign','no-charge-cost-post') ORDER BY e.command_id").fetch_all(db).await.unwrap();
        panic!("native cost eligibility failed: {facts:?}");
    }
    assert_eq!(eligible,vec![("no-charge-cost-assign".into(),true),("no-charge-cost-post".into(),true)]);
    let decisions:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='no-charge-cost-assign'),(SELECT count(*) FROM flow_events WHERE command_id='no-charge-cost-assign'),(SELECT count(*) FROM staff_notifications WHERE id='field-journal-flow-notice-no-charge-cost-assign')").fetch_one(db).await.unwrap();assert_eq!(decisions,(1,1,1));
    // The native journal notice remains source-authorized for the Finance-only
    // recipient after posting; legacy state writes and queue claims are fenced.
    for (statement,constraint) in [
        ("UPDATE staff_notifications SET read_at=1 WHERE id='field-journal-flow-notice-no-charge-cost-assign'","native_field_staff_state_required"),
        ("UPDATE inbox_deliveries SET read_at=1 WHERE id='field-journal-flow-notice-no-charge-cost-assign'","native_field_inbox_state_required"),
        ("UPDATE outbox_events SET attempts=99 WHERE command_id='no-charge-cost-assign'","native_outbox_runtime_required"),
        ("UPDATE outbox_events SET attempts=99 WHERE command_id='no-charge-cost-post'","native_outbox_runtime_required"),
    ]{
        let error=sqlx::query(statement).execute(db).await.unwrap_err();
        assert_eq!(error.as_database_error().and_then(|e|e.constraint()),Some(constraint));
    }
    for command in ["no-charge-cost-assign","no-charge-cost-post"]{
        let mut malformed=db.begin().await.unwrap();
        sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)SELECT 'cost-malformed-'||command_id,command_id,'unrelated.event','unrelated','other-record',aggregate_version,payload_json,created_at FROM outbox_events WHERE command_id=$1").bind(command).execute(&mut *malformed).await.unwrap();
        let owned:bool=sqlx::query_scalar("SELECT erpx_native_outbox_owned(e) FROM outbox_events e WHERE id='cost-malformed-'||$1").bind(command).fetch_one(&mut *malformed).await.unwrap();assert!(owned,"persisted cost receipt prevents legacy downgrade: {command}");
        let eligible:bool=sqlx::query_scalar("SELECT erpx_native_field_journal_outbox_eligible(e) FROM outbox_events e WHERE id='cost-malformed-'||$1").bind(command).fetch_one(&mut *malformed).await.unwrap();assert!(!eligible);
        let claim=sqlx::query("UPDATE outbox_events SET attempts=99 WHERE id='cost-malformed-'||$1").bind(command).execute(&mut *malformed).await.unwrap_err();assert_eq!(claim.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));malformed.rollback().await.unwrap();
    }
    let inbox=no_charge_response(runtime.router(),Request::builder().uri("/api/v1/holdings/acme/field-service/inbox-candidates?all=true&limit=100").header("host","acme.example.test").header("cookie",cookie_d).body(Body::empty()).unwrap(),StatusCode::OK).await;
    let item=inbox["data"].as_array().unwrap().iter().find(|n|n["id"]=="field-journal-flow-notice-no-charge-cost-assign").expect("native Finance-only recipient sees its source-qualified Field cost notice");
    assert_eq!(item["source_type"],"journal");assert_eq!(item["source_id"],journal);assert_eq!(item["native_field_item"]["href"],format!("/finance/journals/{}",journal.replace(':',"%3A")));
    // Queue only: use the real selected native session and independent cursor.
    // No push provider is configured or invoked by this verification.
    let sessions:Vec<String>=sqlx::query_scalar("SELECT session_id FROM active_work_contexts WHERE user_id='fixture-d' AND company_id='company' AND assignment_id='finance-fixture-d'").fetch_all(db).await.unwrap();assert_eq!(sessions.len(),1);
    sqlx::query("INSERT INTO push_preferences(company_id,user_id,enabled,updated_at)VALUES('company','fixture-d',true,0) ON CONFLICT(company_id,user_id)DO UPDATE SET enabled=true").execute(db).await.unwrap();
    sqlx::query("INSERT INTO push_subscriptions(id,company_id,user_id,session_id,origin,inbox_path,endpoint_hash,sealed_subscription,created_at,updated_at,scan_at,cursor_at,cursor_id)VALUES('journal-native-push','company','fixture-d',$1,'https://acme.example.test','/work/inbox','synthetic-journal-endpoint','synthetic-journal-sealed',0,0,9999999999999,9999999999999,'legacy-ahead')").bind(&sessions[0]).execute(db).await.unwrap();
    let mut tx=db.begin().await.unwrap();let subscriptions=erpx_server_collaboration::public::web_push::subscriptions(&mut tx).await.unwrap();let subscription=subscriptions.iter().find(|s|s.id=="journal-native-push").unwrap();
    assert!(erpx_server_collaboration::public::web_push::fanout(&mut tx,subscription,now).await.unwrap()>=1);assert_eq!(erpx_server_collaboration::public::web_push::fanout(&mut tx,subscription,now+1).await.unwrap(),0);tx.commit().await.unwrap();
    let jobs:i64=sqlx::query_scalar("SELECT count(*) FROM push_delivery_jobs WHERE subscription_id='journal-native-push' AND inbox_delivery_id='field-journal-flow-notice-no-charge-cost-assign'").fetch_one(db).await.unwrap();assert_eq!(jobs,1);
    let cursor:i64=sqlx::query_scalar("SELECT cursor_at FROM push_subscriptions WHERE id='journal-native-push'").fetch_one(db).await.unwrap();assert_eq!(cursor,9999999999999);
    let claim=sqlx::query("UPDATE push_delivery_jobs SET attempts=99 WHERE subscription_id='journal-native-push'").execute(db).await.unwrap_err();assert_eq!(claim.as_database_error().and_then(|e|e.constraint()),Some("native_field_push_runtime_required"));
    let read=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/inbox/field-journal-flow-notice-no-charge-cost-assign/state").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie_d).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-d").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").header("idempotency-key","no-charge-cost-read").body(Body::from(r#"{"read":true}"#)).unwrap();
    no_charge_response(runtime.router(),read,StatusCode::OK).await;
    let read:(Option<i64>,Option<i64>)=sqlx::query_as("SELECT n.read_at,d.read_at FROM staff_notifications n JOIN inbox_deliveries d ON d.id=n.id WHERE n.id='field-journal-flow-notice-no-charge-cost-assign'").fetch_one(db).await.unwrap();assert_eq!(read,(Some(now),Some(now)));
    // A retained non-Field reference selects legacy explicitly, without
    // returning journal data or granting its outstanding purchase permission.
    sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,created_by,created_at,updated_at)VALUES('non-field-flow-partition','company','chart','period','NON-FIELD-REFERENCE','2026-01-10','THB','manual','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
    let mut read=call("GET","flow",cookie_d,"finance-fixture-d","",json!(null));*read.uri_mut()="/api/v1/holdings/acme/finance/journals/non-field-flow-partition/flow".parse().unwrap();
    let legacy=no_charge_response(runtime.router(),read,StatusCode::OK).await;
    assert_eq!(legacy["runtime"],"legacy");for key in ["data","workspace","access"]{assert_eq!(legacy[key],serde_json::Value::Null);}
}


#[cfg(feature="files-scan-worker")]
struct CoveredVisitEvidence{set:String,link:String,source:String,bytes:Vec<u8>}
#[cfg(feature="files-scan-worker")]
fn covered_visit_request(method:&str,path:&str,cookie:&str,key:&str,bytes:Vec<u8>)->Request<Body>{
    Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("content-type",if method=="PUT"{"image/png"}else{"application/json"}).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key",key).body(Body::from(bytes)).unwrap()
}
#[cfg(feature="files-scan-worker")]
async fn no_charge_visit_evidence(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,directory:&tempfile::TempDir,cookie:&str,finance_cookie:&str)->CoveredVisitEvidence{
    use erpx_service::{files_scan_native::*,files_system_scan::DevelopmentFilesScanConfiguration};
    use erpx_server_runtime::public::postgres::LeasedDevelopmentDatabase;
    use erpx_server_files::public::{attachment_scan::*,preview::PreviewToolConfiguration,malware_scanner::ClamAvConfig};
    use erpx_server_shared::public::{HoldingId,CompanyId};
    use std::{collections::BTreeMap,path::{Path,PathBuf},time::Duration};
    let doc=private_configuration::document(&p.review.system.control,&p.review.system.holdings).await;
    let lease=|d:&serde_json::Value|LeasedDevelopmentDatabase::parse(d["url"].as_str().unwrap(),d["leaseToken"].as_str().unwrap()).unwrap();
    let databases=DevelopmentFilesScanConfiguration::new(lease(&doc["control"]),doc["holdings"].as_array().unwrap().iter().map(|h|(h["writeBinding"].as_str().unwrap().to_owned(),lease(&h["database"]))).collect(),account::fixture::account::holding_registry(),AttachmentScanScope{holding_id:HoldingId::new("h001").unwrap(),company_id:CompanyId::new("company").unwrap()}).unwrap();
    // Existing real native scan composition and the runtime's same private roots.
    let worker=Box::pin(assemble_native_field_files_scan(databases,NativeFilesScanConfiguration{
        files_roots:BTreeMap::from([("HOLDING_H001_FILES".into(),directory.path().join("objects"))]),temporary_root:directory.path().join("compute"),preview_binary:PathBuf::from(std::env::var("ERPX_NATIVE_PREVIEW_BIN").expect("qualified preview CLI required")),preview_tools:PreviewToolConfiguration::new(Path::new("/usr/bin/heif-convert"),Path::new("/usr/bin/pdftoppm")).unwrap(),scanner:ClamAvConfig{socket:PathBuf::from("/erpx-clam/clamd.sock"),database_directory:PathBuf::from("/erpx-definitions"),maximum_definition_age:Duration::from_secs(7*86400),command_timeout:Duration::from_secs(5),scan_timeout:Duration::from_secs(30),concurrent_scans:1}
    },p.review.system.scenario.clock.clone())).await.unwrap();
    Box::pin(no_charge_journal_evidence(p,runtime,&worker,finance_cookie)).await;
    let base="/api/v1/holdings/acme/field-service/attachment-sets";
    let create=json!({"targetType":"field_service_visit","targetId":"issue-order","componentKind":"record","purposeKey":"signed_job_sheet","expectedTargetVersion":4});
    let set=no_charge_response(runtime.router(),covered_visit_request("POST",base,cookie,"covered-visit-set",serde_json::to_vec(&create).unwrap()),StatusCode::CREATED).await;
    let set=set["data"]["id"].as_str().unwrap();
    let bytes=include_bytes!("../../../fixtures/files/preview/png-alpha.input").to_vec();
    let file=json!({"filename":"actual-covered-job-sheet.png","mediaType":"image/png","bytes":bytes.len()});
    let reserve=no_charge_response(runtime.router(),covered_visit_request("POST",&format!("{base}/{set}/files"),cookie,"covered-visit-file",serde_json::to_vec(&file).unwrap()),StatusCode::CREATED).await;
    let link=reserve["data"]["attachmentLinkId"].as_str().unwrap().to_owned();
    let upload=no_charge_response(runtime.router(),covered_visit_request("PUT",reserve["data"]["uploadUrl"].as_str().unwrap(),cookie,"ignored",bytes.clone()),StatusCode::ACCEPTED).await;
    let source=upload["data"]["sourceId"].as_str().unwrap().to_owned();
    let candidate=worker.next_field_candidate().await.unwrap().expect("actual pending covered visit evidence");assert_eq!(candidate.as_str(),source);
    assert!(matches!(Box::pin(worker.run_next(Some(&candidate))).await.unwrap(),AttachmentScanRunOutcome::Settled(_)));
    let state:String=sqlx::query_scalar("SELECT source_state FROM attachments WHERE id=$1").bind(&source).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(state,"available");
    let bind=json!({"expectedTargetVersion":4});
    no_charge_response(runtime.router(),covered_visit_request("POST",&format!("/api/v1/holdings/acme/attachment-sets/{set}/bind"),cookie,"covered-visit-bind",serde_json::to_vec(&bind).unwrap()),StatusCode::OK).await;
    sqlx::query("UPDATE attachments SET legal_hold=1 WHERE id=$1").bind(&source).execute(p.pool().raw()).await.unwrap();
    CoveredVisitEvidence{set:set.into(),link,source,bytes}
}
#[cfg(feature="files-scan-worker")]
async fn no_charge_journal_evidence<C:erpx_server_shared::public::Clock+Send+Sync+'static>(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,worker:&erpx_service::files_scan_native::NativeFilesScanWorker<C>,cookie:&str){
    use erpx_server_shared::public::Clock;
    use erpx_server_files::public::attachment_scan::AttachmentScanRunOutcome;
    let db=p.pool().raw();

    let (journal,version):(String,i64)=sqlx::query_as("SELECT j.id,j.version FROM finance_journals j JOIN operational_inventory_consumptions c ON c.journal_id=j.id WHERE c.id='operational-part-no-charge-original-spare' AND j.company_id='company'").fetch_one(db).await.unwrap();
    let request=|method:&str,path:&str,key:&str,bytes:Vec<u8>|Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("content-type",if method=="PUT"{"image/png"}else{"application/json"}).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-d").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").header("idempotency-key",key).body(Body::from(bytes)).unwrap();
    let base="/api/v1/holdings/acme/finance/journal-files/attachment-sets";
    let create=json!({"targetType":"finance_journal","targetId":journal,"componentKind":"record","purposeKey":"journal_support","expectedTargetVersion":version});
    let created=no_charge_response(runtime.router(),request("POST",base,"covered-journal-set",serde_json::to_vec(&create).unwrap()),StatusCode::CREATED).await;
    assert_eq!(created["data"]["maxFiles"],50);assert_eq!(created["data"]["classification"],"confidential");
    let repeated=no_charge_response(runtime.router(),request("POST",base,"covered-journal-set",serde_json::to_vec(&create).unwrap()),StatusCode::CREATED).await;assert_eq!(repeated,created);
    let set=created["data"]["id"].as_str().unwrap();
    let bytes=include_bytes!("../../../fixtures/files/preview/png-alpha.input").to_vec();
    let file=json!({"filename":"actual-field-cost-support.png","mediaType":"image/png","bytes":bytes.len()});
    let reserve=no_charge_response(runtime.router(),request("POST",&format!("{base}/{set}/files"),"covered-journal-file",serde_json::to_vec(&file).unwrap()),StatusCode::CREATED).await;
    let link=reserve["data"]["attachmentLinkId"].as_str().unwrap();
    let upload=no_charge_response(runtime.router(),request("PUT",reserve["data"]["uploadUrl"].as_str().unwrap(),"ignored",bytes.clone()),StatusCode::ACCEPTED).await;
    let source=upload["data"]["sourceId"].as_str().unwrap();
    let legacy:i64=sqlx::query_scalar("SELECT count(*) FROM erpx_files_claim_legacy_scan($1,$2,'forbidden-journal-legacy-lease',$3)").bind(p.review.system.scenario.clock.now().get()).bind(p.review.system.scenario.clock.now().get()+60000).bind(source).fetch_one(db).await.unwrap();assert_eq!(legacy,0);
    let candidate=worker.next_field_candidate().await.unwrap().expect("native journal scan discovery");assert_eq!(candidate.as_str(),source);
    assert!(matches!(Box::pin(worker.run_next(Some(&candidate))).await.unwrap(),AttachmentScanRunOutcome::Settled(_)));
    let bind_path=format!("{base}/{set}/bind");let body=serde_json::to_vec(&json!({"expectedTargetVersion":version})).unwrap();
    let mut foreign=request("POST",&bind_path,"covered-journal-foreign",body.clone());foreign.headers_mut().insert("x-erpx-context-company","company-two".parse().unwrap());
    no_charge_response(runtime.router(),foreign,StatusCode::CONFLICT).await;
    no_charge_response(runtime.router(),request("POST",&bind_path,"covered-journal-stale",serde_json::to_vec(&json!({"expectedTargetVersion":version+1})).unwrap()),StatusCode::CONFLICT).await;
    sqlx::raw_sql("CREATE FUNCTION reject_journal_bind_fixture() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.command_id='covered-journal-rollback' THEN RAISE EXCEPTION 'synthetic journal bind outbox failure';END IF;RETURN NEW;END$$;CREATE TRIGGER reject_journal_bind_fixture BEFORE INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION reject_journal_bind_fixture()").execute(db).await.unwrap();
    no_charge_response(runtime.router(),request("POST",&bind_path,"covered-journal-rollback",body.clone()),StatusCode::INTERNAL_SERVER_ERROR).await;
    sqlx::raw_sql("DROP TRIGGER reject_journal_bind_fixture ON outbox_events;DROP FUNCTION reject_journal_bind_fixture()").execute(db).await.unwrap();
    let failed:(String,i64,i64,i64,i64)=sqlx::query_as("SELECT s.state,j.version,(SELECT count(*) FROM command_receipts WHERE command_id='covered-journal-rollback'),(SELECT count(*) FROM audit_events WHERE command_id='covered-journal-rollback'),(SELECT count(*) FROM outbox_events WHERE command_id='covered-journal-rollback') FROM attachment_sets s JOIN finance_journals j ON j.id=s.target_id WHERE s.id=$1").bind(set).fetch_one(db).await.unwrap();assert_eq!(failed,("draft".into(),version,0,0,0));
    // Even mutable Finance metadata must remain exact after deferred effects,
    // not merely at the initial read or immediately after the metadata CAS.
    let before:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(j) FROM finance_journals j WHERE id=$1").bind(&journal).fetch_one(db).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION change_journal_bind_source_fixture() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE finance_journals SET updated_at=updated_at+1 WHERE id=NEW.aggregate_id;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER change_journal_bind_source_fixture AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='covered-journal-source-drift') EXECUTE FUNCTION change_journal_bind_source_fixture()").execute(db).await.unwrap();
    no_charge_response(runtime.router(),request("POST",&bind_path,"covered-journal-source-drift",body.clone()),StatusCode::CONFLICT).await;
    sqlx::raw_sql("DROP TRIGGER change_journal_bind_source_fixture ON outbox_events;DROP FUNCTION change_journal_bind_source_fixture()").execute(db).await.unwrap();
    let after:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(j) FROM finance_journals j WHERE id=$1").bind(&journal).fetch_one(db).await.unwrap();assert_eq!(after,before);
    let escaped:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='covered-journal-source-drift'").fetch_one(db).await.unwrap();assert_eq!(escaped,0);
    let (first,competing)=tokio::join!(no_charge_response(runtime.router(),request("POST",&bind_path,"covered-journal-bind",body.clone()),StatusCode::OK),no_charge_response(runtime.router(),request("POST",&bind_path,"covered-journal-bind",body.clone()),StatusCode::OK));
    let bound=first;assert_eq!(competing,bound);
    assert_eq!(bound["data"]["version"],version+1);
    let replay=no_charge_response(runtime.router(),request("POST",&bind_path,"covered-journal-bind",body),StatusCode::OK).await;assert_eq!(replay,bound);
    let current:(String,i64,i64,i64)=sqlx::query_as("SELECT j.status,j.version,sum(l.debit_minor)::bigint,sum(l.credit_minor)::bigint FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.id=$1 GROUP BY j.id").bind(&journal).fetch_one(db).await.unwrap();assert_eq!(current,("posted".into(),version+1,600,600));
    let denied=sqlx::query("SELECT erpx_files_assert_legacy_writer('set',$1)").bind(set).execute(db).await.unwrap_err();assert_eq!(denied.as_database_error().and_then(|e|e.constraint()),Some("field_file_native_writer"));
    let cleanup:bool=sqlx::query_scalar("SELECT erpx_files_legacy_cleanup_set_allowed($1)").bind(set).fetch_one(db).await.unwrap();assert!(!cleanup);
    let eligible=erpx_server_files::public::field_outbox::ELIGIBLE;
    let mut query=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT command_id,erpx_native_outbox_owned(e),(");query.push(eligible).push(") FROM outbox_events e WHERE command_id IN(").push_bind(erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-d","covered-journal-set")).push(",'covered-journal-bind') ORDER BY command_id");let events:Vec<(String,bool,bool)>=query.build_query_as().fetch_all(db).await.unwrap();assert_eq!(events.len(),2);for(command,owned,eligible)in events{assert!(owned&&eligible,"native journal Files event: {command}");}
    // Retain the original Finance journal legal-hold/private-byte assertion
    // on the real scanned native source rather than a generic TS attachment.
    let held_source:String=sqlx::query_scalar("SELECT attachment_id FROM attachment_links WHERE id=$1").bind(link).fetch_one(db).await.unwrap();
    sqlx::query("UPDATE attachments SET legal_hold=1 WHERE id=$1").bind(&held_source).execute(db).await.unwrap();
    let held:i64=sqlx::query_scalar("SELECT legal_hold FROM attachments WHERE id=$1").bind(&held_source).fetch_one(db).await.unwrap();assert_eq!(held,1);
    let path=format!("/api/v1/holdings/acme/finance/journal-files/attachment-links/{link}/content?variant=original");
    let response=runtime.router().oneshot(request("GET",&path,"",vec![])).await.unwrap();let status=response.status();let actual=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&actual));assert_eq!(actual.as_ref(),bytes.as_slice());
}
#[cfg(feature="files-scan-worker")]
async fn no_charge_visit_history(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,cookie:&str,file:CoveredVisitEvidence,version:i64){
    let path=format!("/api/v1/holdings/acme/field-service/attachment-links/{}/content?variant=original",file.link);
    let router=runtime.router();let request=covered_visit_request("GET",&path,cookie,"ignored",vec![]);
    let response=tokio::spawn(async move{router.oneshot(request).await}).await.unwrap().unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["content-type"],"image/png");assert_eq!(to_bytes(response.into_body(),10485760).await.unwrap().as_ref(),file.bytes.as_slice());
    let held:i64=sqlx::query_scalar("SELECT legal_hold FROM attachments WHERE id=$1").bind(&file.source).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(held,1);
    let before:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_sets").fetch_one(p.pool().raw()).await.unwrap();
    let late=json!({"targetType":"field_service_visit","targetId":"issue-order","componentKind":"record","purposeKey":"technician_report","expectedTargetVersion":version});
    let refused=no_charge_response(runtime.router(),covered_visit_request("POST","/api/v1/holdings/acme/field-service/attachment-sets",cookie,"covered-visit-late-set",serde_json::to_vec(&late).unwrap()),StatusCode::CONFLICT).await;assert_eq!(refused["error"]["code"],"SERVICE_ATTACHMENT_HISTORY_ONLY");
    let after:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_sets").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(after,before);
    // Privileged invalid-staging fixture, matching the retained original test.
    // The native positive upload/scan/bind above is genuine. This cloned draft
    // is not a successful user stage or an authorization grant; it proves that
    // a current History-only target cannot be bound even if staging exists.
    sqlx::query("INSERT INTO attachment_sets SELECT (jsonb_populate_record(NULL::attachment_sets,to_jsonb(s)||jsonb_build_object('id','covered-history-only-set','purpose_key','technician_report','expected_target_version',$2::bigint,'state','draft','bound_at',NULL,'bound_command_id',NULL))).* FROM attachment_sets s WHERE s.id=$1").bind(&file.set).bind(version).execute(p.pool().raw()).await.unwrap();
    sqlx::query("INSERT INTO attachment_links SELECT (jsonb_populate_record(NULL::attachment_links,to_jsonb(l)||jsonb_build_object('id','covered-history-only-link','set_id','covered-history-only-set','state','draft','bound_at',NULL,'binding_type',NULL,'binding_id',NULL))).* FROM attachment_links l WHERE l.id=$1").bind(&file.link).execute(p.pool().raw()).await.unwrap();
    let late_bind=no_charge_response(runtime.router(),covered_visit_request("POST","/api/v1/holdings/acme/attachment-sets/covered-history-only-set/bind",cookie,"covered-history-only-bind",serde_json::to_vec(&json!({"expectedTargetVersion":version})).unwrap()),StatusCode::CONFLICT).await;
    assert_eq!(late_bind["error"]["code"],"SERVICE_ATTACHMENT_HISTORY_ONLY");
    let late_state:String=sqlx::query_scalar("SELECT state FROM attachment_sets WHERE id='covered-history-only-set'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(late_state,"draft");
    let order:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_orders WHERE id='issue-order'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(order,("billed".into(),version));
}

async fn native_no_charge_case(p:&Parts,runtime:&erpx_service::private_development_api::DevelopmentApiRuntime,now:i64,original_finance:&str,field_cookie:&str,release_cookie:&str,visit_directory:Option<&tempfile::TempDir>){
    let system=&p.review.system;let db=p.pool().raw();
    sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\")VALUES('fixture-c','Independent No Charge C','c@example.test',1,$1,$1)").bind(now).execute(system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") SELECT 'fixture-c','fixture-c','credential','local:credential','fixture-c',password,$1,$1 FROM account WHERE id='fixture-a'").bind(now).execute(system.control.raw()).await.unwrap();
    sqlx::raw_sql(r#"INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_no_charge_review','Independent no-charge review',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_no_charge_review',1,'["finance.read","finance.write","finance.approve","module.finance.read","workflow.finance_approver"]','[]','["holding"]','synthetic',0);INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('no-charge-person','fixture-c','company','Independent reviewer','TH','active','fixture-a',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('no-charge-engagement','no-charge-person','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture-a',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-c','local:fixture-c','no-charge-person','active','local',1)"#).execute(db).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('finance-fixture-c','fixture-c','native_no_charge_review','[\"finance\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
    let central=account::login(system,"c@example.test",None).await;let child=account::child(system,&central,account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
    let request=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"finance-fixture-c","assignmentVersion":3,"revision":0}"#)).unwrap();
    let response=runtime.router().oneshot(request).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
    let get=|cookie:&str,assignment:&str,revision:&str|Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/workspace").header("host","acme.example.test").header("cookie",cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment",assignment).header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision",revision).body(Body::empty()).unwrap();
    let response=runtime.router().oneshot(get(&cookie,"finance-fixture-c","1")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let current:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
    assert_eq!(current["noCharge"]["canConfirm"],true);assert_eq!(current["data"]["invoice"]["total_minor"],0);
    // Preserve the original full-warranty draft and exact zero reporting quote.
    let zero_draft:(i64,String)=sqlx::query_as("SELECT total_minor,status FROM finance_invoices WHERE id='field-invoice-included-billing'").fetch_one(db).await.unwrap();assert_eq!(zero_draft,(0,"draft".into()));
    let zero_quote:(String,String)=sqlx::query_as("SELECT amount_units::text,usd_units::text FROM money_transaction_snapshots WHERE company_id='company' AND source_type='finance_invoice' AND source_id='field-invoice-included-billing' AND amount_kind='draft:v1'").fetch_one(db).await.unwrap();assert_eq!(zero_quote,("0".into(),"0".into()));
    let before_zero_post:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('invoice',(SELECT to_jsonb(i) FROM finance_invoices i WHERE id='field-invoice-included-billing'),'release',(SELECT to_jsonb(r) FROM field_service_billing_releases r WHERE id='field-billing-included-billing'),'money',(SELECT jsonb_agg(to_jsonb(m) ORDER BY source_type,amount_kind) FROM money_transaction_snapshots m WHERE company_id='company' AND source_id IN('field-invoice-included-billing','field-billing-included-billing')))").fetch_one(db).await.unwrap();
    let ordinary=Request::builder().method("POST").uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/post").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key","covered-ordinary-post-denied").header("x-erpx-context-company","company").header("x-erpx-context-revision","1").header("x-erpx-context-assignment","finance-fixture-c").header("x-erpx-context-assignment-version","3").body(Body::from(r#"{"periodId":"period","entryDate":"2026-01-10","expectedVersion":1}"#)).unwrap();
    let denied=no_charge_response(runtime.router(),ordinary,StatusCode::UNPROCESSABLE_ENTITY).await;assert_eq!(denied["error"]["code"],"INVALID_JOURNAL_LINE");
    let after_zero_post:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('invoice',(SELECT to_jsonb(i) FROM finance_invoices i WHERE id='field-invoice-included-billing'),'release',(SELECT to_jsonb(r) FROM field_service_billing_releases r WHERE id='field-billing-included-billing'),'money',(SELECT jsonb_agg(to_jsonb(m) ORDER BY source_type,amount_kind) FROM money_transaction_snapshots m WHERE company_id='company' AND source_id IN('field-invoice-included-billing','field-billing-included-billing')))").fetch_one(db).await.unwrap();assert_eq!(after_zero_post,before_zero_post);
    let zero_effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='covered-ordinary-post-denied'),(SELECT count(*) FROM audit_events WHERE command_id='covered-ordinary-post-denied'),(SELECT count(*) FROM outbox_events WHERE command_id='covered-ordinary-post-denied'),(SELECT count(*) FROM finance_journals WHERE source_type='invoice' AND source_id='field-invoice-included-billing'),(SELECT count(*) FROM finance_allocations WHERE invoice_id='field-invoice-included-billing')").fetch_one(db).await.unwrap();assert_eq!(zero_effects,(0,0,0,0,0));
    let body=serde_json::json!({"expectedVersion":1,"contextKey":current["noCharge"]["contextKey"],"reason":"Independent covered service fulfillment"}).to_string();
    let post=|cookie:&str,key:&str,body:String,assignment:&str,revision:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/confirm-no-charge").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-revision",revision).header("x-erpx-context-assignment",assignment).header("x-erpx-context-assignment-version","3").body(Body::from(body)).unwrap();
    let response=runtime.router().oneshot(get(original_finance,"finance-fixture-b","7")).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();let release_actor:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(release_actor["noCharge"]["canConfirm"],false);
    let denied=serde_json::json!({"expectedVersion":1,"contextKey":release_actor["noCharge"]["contextKey"],"reason":"Releaser cannot confirm own covered service"}).to_string();
    let response=runtime.router().oneshot(post(original_finance,"no-charge-releaser",denied,"finance-fixture-b","7")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::FORBIDDEN,"{}",String::from_utf8_lossy(&bytes));assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["error"]["code"],"INDEPENDENT_APPROVER_REQUIRED");
    for (fault,expected,code) in [("json",StatusCode::UNPROCESSABLE_ENTITY,"INVALID_JSON"),("key",StatusCode::UNPROCESSABLE_ENTITY,"IDEMPOTENCY_KEY_REQUIRED"),("origin",StatusCode::FORBIDDEN,"ORIGIN_FORBIDDEN"),("path",StatusCode::NOT_FOUND,"NOT_FOUND")]{
        let mut invalid=post(&cookie,"no-charge-ingress-invalid",if fault=="json"{"{".into()}else{body.clone()},"finance-fixture-c","1");
        if fault=="key"{invalid.headers_mut().remove("idempotency-key");}if fault=="origin"{invalid.headers_mut().insert("origin","https://foreign.test".parse().unwrap());}if fault=="path"{*invalid.uri_mut()="/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/confirm-no-charge/unregistered".parse().unwrap();}
        if fault=="path" {
            // The private admission guard's unregistered path is its existing text404;
            // the public Next route's JSON NOT_FOUND remains asserted separately.
            let response=runtime.router().oneshot(invalid).await.unwrap();assert_eq!(response.status(),expected);assert_eq!(to_bytes(response.into_body(),65536).await.unwrap().as_ref(),b"Not Found");
        }else{let error=no_charge_response(runtime.router(),invalid,expected).await;assert_eq!(error["error"]["code"],code);}
    }
    for (key,input) in [("no-charge-stale-version",json!({"expectedVersion":2,"contextKey":current["noCharge"]["contextKey"],"reason":"A later version cannot approve the original source"})),("no-charge-bad-context",json!({"expectedVersion":1,"contextKey":"f".repeat(64),"reason":"A forged review context cannot approve the source"}))]{
        let denied=no_charge_response(runtime.router(),post(&cookie,key,input.to_string(),"finance-fixture-c","1"),StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"SERVICE_NO_CHARGE_CONTEXT_CHANGED");
        let effects:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id=$1").bind(key).fetch_one(db).await.unwrap();assert_eq!(effects,0);
    }
    let unprivileged_central=Box::pin(account::login(system,"a@example.test",None)).await;let unprivileged_child=Box::pin(account::child(system,&unprivileged_central,account::ACME)).await;let unprivileged_cookie=format!("__Host-erpx-product={}",unprivileged_child.token.expose());
    let select=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&unprivileged_cookie).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"field-fixture-a","assignmentVersion":3,"revision":0}"#)).unwrap();no_charge_response(runtime.router(),select,StatusCode::OK).await;
    no_charge_response(runtime.router(),post(&unprivileged_cookie,"no-charge-field-denied",body.clone(),"field-fixture-a","1"),StatusCode::FORBIDDEN).await;
    no_charge_response(runtime.router(),get(&unprivileged_cookie,"field-fixture-a","1"),StatusCode::FORBIDDEN).await;
    // A real selected Finance preparer may read Finance, but cannot turn its
    // write permission into independent approval or bypass that gate on replay.
    sqlx::raw_sql(r#"INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_no_charge_preparer','Read/write without approval',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_no_charge_preparer',1,'["finance.read","finance.write","module.finance.read"]','[]','["holding"]','synthetic',0)"#).execute(db).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('no-charge-preparer-a','fixture-a','native_no_charge_preparer','[\"finance\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
    let preparer_central=Box::pin(account::login(system,"a@example.test",None)).await;let preparer_child=Box::pin(account::child(system,&preparer_central,account::ACME)).await;let preparer=format!("__Host-erpx-product={}",preparer_child.token.expose());
    let select=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&preparer).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"no-charge-preparer-a","assignmentVersion":3,"revision":0}"#)).unwrap();no_charge_response(runtime.router(),select,StatusCode::OK).await;
    no_charge_response(runtime.router(),post(&preparer,"no-charge-preparer-denied",body.clone(),"no-charge-preparer-a","1"),StatusCode::FORBIDDEN).await;
    let denied_effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_service_no_charge_confirmations WHERE invoice_id='field-invoice-included-billing'),(SELECT count(*) FROM command_receipts WHERE command_id IN('no-charge-field-denied','no-charge-preparer-denied','no-charge-releaser','no-charge-stale-version','no-charge-bad-context'))").fetch_one(db).await.unwrap();assert_eq!(denied_effects,(0,0));
    let original_invoice:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id='field-invoice-included-billing'").fetch_one(db).await.unwrap();
    let original_order:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(o) FROM field_service_orders o WHERE id='reserve-order'").fetch_one(db).await.unwrap();
    let original_assignment:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(a) FROM assignments a WHERE id='finance-fixture-c'").fetch_one(db).await.unwrap();
    for (fault,mutation,expected,code) in [
        ("source","UPDATE field_service_orders SET customer_confirmation='Contradictory late signoff' WHERE id='reserve-order'",StatusCode::CONFLICT,"SERVICE_NO_CHARGE_PROOF_REQUIRED"),
        ("iam","UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='finance-fixture-c'",StatusCode::FORBIDDEN,"FINANCE_ACCESS_CHANGED"),
        ("route","UPDATE flow_routes SET version=version+1 WHERE kind='invoice' AND record_id='field-invoice-included-billing'",StatusCode::CONFLICT,"FLOW_ROUTING_CHANGED"),
    ]{
        // DDL is generated solely from these finite test constants; no input,
        // credentials or external SQL can reach this existing fault mechanism.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SEQUENCE no_charge_final_fault_attempt;CREATE FUNCTION no_charge_final_fault() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM nextval('no_charge_final_fault_attempt');{mutation};RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER no_charge_final_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='no-charge-final-{fault}') EXECUTE FUNCTION no_charge_final_fault()"))).execute(db).await.unwrap();
        let response=runtime.router().oneshot(post(&cookie,&format!("no-charge-final-{fault}"),body.clone(),"finance-fixture-c","1")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,expected,"{fault}: {}",String::from_utf8_lossy(&bytes));assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["error"]["code"],code);
        let attempted:bool=sqlx::query_scalar("SELECT is_called FROM no_charge_final_fault_attempt").fetch_one(db).await.unwrap();assert!(attempted,"the real deferred {fault} fault must execute");
        sqlx::raw_sql("DROP TRIGGER no_charge_final_fault ON outbox_events;DROP FUNCTION no_charge_final_fault();DROP SEQUENCE no_charge_final_fault_attempt").execute(db).await.unwrap();
        let invoice:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id='field-invoice-included-billing'").fetch_one(db).await.unwrap();assert_eq!(invoice,original_invoice);
        let order:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(o) FROM field_service_orders o WHERE id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(order,original_order);
        let counts:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_service_no_charge_confirmations),(SELECT count(*) FROM command_receipts WHERE command_id LIKE 'no-charge-final-%'),(SELECT count(*) FROM audit_events WHERE command_id LIKE 'no-charge-final-%'),(SELECT count(*) FROM outbox_events WHERE command_id LIKE 'no-charge-final-%'),(SELECT count(*) FROM flow_routes WHERE kind='invoice' AND record_id='field-invoice-included-billing')").fetch_one(db).await.unwrap();assert_eq!(counts,(0,0,0,0,0));
        let assignment:(Option<i64>,i64)=sqlx::query_as("SELECT revoked_at,version FROM assignments WHERE id='finance-fixture-c'").fetch_one(db).await.unwrap();assert_eq!(assignment,(None,3));
        let assignment_image:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(a) FROM assignments a WHERE id='finance-fixture-c'").fetch_one(db).await.unwrap();assert_eq!(assignment_image,original_assignment);
    }
    // A failure after all Finance/Flow effects must roll the entire command back.
    sqlx::raw_sql("CREATE FUNCTION reject_no_charge_fixture() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.event_type='finance.service-no-charge.confirm' THEN RAISE EXCEPTION 'synthetic outbox failure';END IF;RETURN NEW;END$$;CREATE TRIGGER reject_no_charge_fixture BEFORE INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION reject_no_charge_fixture()").execute(db).await.unwrap();
    let response=runtime.router().oneshot(post(&cookie,"native-no-charge-rollback",body.clone(),"finance-fixture-c","1")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::INTERNAL_SERVER_ERROR,"{}",String::from_utf8_lossy(&bytes));
    sqlx::raw_sql("DROP TRIGGER reject_no_charge_fixture ON outbox_events;DROP FUNCTION reject_no_charge_fixture()").execute(db).await.unwrap();
    let rows:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_service_no_charge_confirmations),(SELECT count(*) FROM command_receipts WHERE command_id='native-no-charge-rollback'),(SELECT version FROM finance_invoices WHERE id='field-invoice-included-billing')").fetch_one(db).await.unwrap();assert_eq!(rows,(0,0,1));
    let (second_company,second_invoice)=Box::pin(no_charge_second_company(p,runtime,now)).await;
    let mut unspecified=get(&cookie,"finance-fixture-c","1");unspecified.headers_mut().remove("x-erpx-context-company");
    let missing=no_charge_response(runtime.router(),unspecified,StatusCode::CONFLICT).await;assert_eq!(missing["error"]["code"],"WORK_COMPANY_REQUIRED");
    let mut foreign=post(&cookie,"no-charge-cross-company",body.clone(),"finance-fixture-c","1");*foreign.uri_mut()=format!("/api/v1/holdings/acme/finance/invoices/{second_invoice}/confirm-no-charge").parse().unwrap();
    let denied=no_charge_response(runtime.router(),foreign,StatusCode::CONFLICT).await;assert_eq!(denied["error"]["code"],"SERVICE_NO_CHARGE_SOURCE_CHANGED");
    let (first,competing,second_company)=tokio::join!(runtime.router().oneshot(post(&cookie,"native-no-charge",body.clone(),"finance-fixture-c","1")),runtime.router().oneshot(post(&cookie,"native-no-charge",body.clone(),"finance-fixture-c","1")),runtime.router().oneshot(second_company));
    let second=second_company.unwrap();let second_status=second.status();let second_bytes=to_bytes(second.into_body(),65536).await.unwrap();assert_eq!(second_status,StatusCode::OK,"{}",String::from_utf8_lossy(&second_bytes));
    let second_result:serde_json::Value=serde_json::from_slice(&second_bytes).unwrap();assert_eq!(second_result["data"]["status"],"no_charge_confirmed");assert_eq!(second_result["data"]["id"],second_invoice);assert_eq!(second_result["data"]["version"],2);
    let decisions:Vec<(String,String,String,String)>=sqlx::query_as("SELECT company_id,invoice_id,service_order_id,command_id FROM finance_service_no_charge_confirmations WHERE command_id IN('native-no-charge','native-no-charge-two') ORDER BY company_id").fetch_all(db).await.unwrap();
    let second_order:String=sqlx::query_scalar("SELECT result_json::jsonb->>'id' FROM command_receipts WHERE command_id='c2-request' AND authority_json::jsonb->>'companyId'='company-two'").fetch_one(db).await.unwrap();
    assert_eq!(decisions,vec![("company".into(),"field-invoice-included-billing".into(),"reserve-order".into(),"native-no-charge".into()),("company-two".into(),second_invoice.clone(),second_order,"native-no-charge-two".into())]);
    let no_monetary:i64=sqlx::query_scalar("SELECT count(*) FROM finance_journals WHERE source_type='invoice' AND source_id IN('field-invoice-included-billing',$1)").bind(&second_invoice).fetch_one(db).await.unwrap();assert_eq!(no_monetary,0);
    let companies:Vec<String>=sqlx::query_scalar("SELECT company_id FROM finance_service_no_charge_confirmations WHERE command_id IN('native-no-charge','native-no-charge-two') ORDER BY company_id").fetch_all(db).await.unwrap();assert_eq!(companies,vec!["company","company-two"]);
    let escaped:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='no-charge-cross-company'").fetch_one(db).await.unwrap();assert_eq!(escaped,0);
    let response=first.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let result:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(result["data"]["status"],"no_charge_confirmed");assert_eq!(result["data"]["id"],"field-invoice-included-billing");assert_eq!(result["data"]["version"],2);
    let competing=competing.unwrap();let status=competing.status();let bytes=to_bytes(competing.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),result);
    let response=runtime.router().oneshot(post(&cookie,"native-no-charge",body.clone(),"finance-fixture-c","1")).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(serde_json::from_slice::<serde_json::Value>(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap(),result);
    let facts:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_service_no_charge_completed WHERE command_id='native-no-charge'),(SELECT count(*) FROM audit_events WHERE command_id='native-no-charge'),(SELECT count(*) FROM outbox_events WHERE command_id='native-no-charge'),(SELECT count(*) FROM finance_journals WHERE source_type='invoice' AND source_id='field-invoice-included-billing'),(SELECT count(*) FROM finance_allocations WHERE invoice_id='field-invoice-included-billing')").fetch_one(db).await.unwrap();assert_eq!(facts,(1,1,1,0,0));
    let changed=body.replace("Independent covered service fulfillment","Changed retained confirmation reason");
    assert_eq!(runtime.router().oneshot(post(&cookie,"native-no-charge",changed,"finance-fixture-c","1")).await.unwrap().status(),StatusCode::CONFLICT);
    assert_eq!(runtime.router().oneshot(post(&cookie,"native-no-charge-stale",body,"finance-fixture-c","1")).await.unwrap().status(),StatusCode::CONFLICT);
    let response=runtime.router().oneshot(get(&cookie,"finance-fixture-c","1")).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();let terminal:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(terminal["noCharge"]["confirmed"],true);assert_eq!(terminal["noCharge"]["canConfirm"],false);assert_eq!(terminal["flow"]["current"],serde_json::Value::Null);
    // Original retained supplier-evidence refusal now consumes genuine native
    // invoice lineage and current Finance authority after No-Charge confirmation.
    let before:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id='field-invoice-included-billing'").fetch_one(db).await.unwrap();
    let mut supplier=get(&cookie,"finance-fixture-c","1");*supplier.uri_mut()="/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/workspace?purpose=payment-evidence".parse().unwrap();
    let refused=no_charge_response(runtime.router(),supplier,StatusCode::UNPROCESSABLE_ENTITY).await;assert_eq!(refused["error"]["code"],"PAYABLE_REQUIRED");
    let after:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(i) FROM finance_invoices i WHERE id='field-invoice-included-billing'").fetch_one(db).await.unwrap();assert_eq!(before,after);
    let cookie_d=Box::pin(no_charge_native_routing(p,runtime,now,field_cookie,release_cookie,&cookie)).await;
    eprintln!("native No-Charge: transfer real parts-cost path");
    Box::pin(no_charge_real_parts_journal(p,runtime,now,&cookie_d,field_cookie)).await;
    #[cfg(feature="files-scan-worker")]
    let visit_file=if let Some(directory)=visit_directory{
        crate::facts::drain(p,"service").await;crate::facts::drain(p,"service-parts-assets").await;
        Some(Box::pin(no_charge_visit_evidence(p,runtime,directory,field_cookie,&cookie_d)).await)
    }else{None};
    #[cfg(not(feature="files-scan-worker"))]
    assert!(visit_directory.is_none(),"actual covered Visit Files needs its provider profile");
    #[cfg(feature="files-scan-worker")]
    let completed_version=if visit_file.is_some(){5}else{4};
    #[cfg(not(feature="files-scan-worker"))]
    let completed_version=4;

    // A lawful later identity link must not rewrite the original independent
    // decision. Replay still needs current permission, scope and terminal proof.
    sqlx::query("UPDATE iam_members SET person_id='agreement-review-person' WHERE company_id='company' AND user_id='fixture-c'").execute(db).await.unwrap();
    let aliases:bool=sqlx::query_scalar("SELECT iam_same_person('fixture-b','fixture-c','company')").fetch_one(db).await.unwrap();assert!(aliases);
    let completed=send(&runtime.router(),field_cookie,"no-charge-linked-complete",&json!({"id":"issue-order","action":"complete","expectedVersion":completed_version,"laborMinutes":5,"evidence":"Second actual warranty visit","customerConfirmation":"Second accepted visit","reason":"Complete covered visit"}).to_string()).await;assert_eq!(completed.0,StatusCode::OK,"{}",completed.1);
    let release=send_as(&runtime.router(),release_cookie,"agreement-reviewer-b","no-charge-linked-billing",&json!({"id":"issue-order","action":"release-billing","expectedVersion":completed_version+1,"reason":"Independent second billing release"}).to_string()).await;assert_eq!(release.0,StatusCode::OK,"{}",release.1);
    let mut read=get(&cookie,"finance-fixture-c","1");*read.uri_mut()="/api/v1/holdings/acme/finance/invoices/field-invoice-no-charge-linked-billing/workspace".parse().unwrap();let response=runtime.router().oneshot(read).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let linked:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(linked["noCharge"]["canConfirm"],false);
    let mut denied=post(&cookie,"no-charge-linked-denied",serde_json::json!({"expectedVersion":1,"contextKey":linked["noCharge"]["contextKey"],"reason":"A linked releaser must not approve a fresh source"}).to_string(),"finance-fixture-c","1");*denied.uri_mut()="/api/v1/holdings/acme/finance/invoices/field-invoice-no-charge-linked-billing/confirm-no-charge".parse().unwrap();let response=runtime.router().oneshot(denied).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::FORBIDDEN,"{}",String::from_utf8_lossy(&bytes));assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["error"]["code"],"INDEPENDENT_APPROVER_REQUIRED");
    let response=runtime.router().oneshot(post(&cookie,"native-no-charge",serde_json::json!({"expectedVersion":1,"contextKey":current["noCharge"]["contextKey"],"reason":"Independent covered service fulfillment"}).to_string(),"finance-fixture-c","1")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),result);
    // Finish the fresh source with the other independent current reviewer,
    // then retain history through normal Agreement retirement and grant expiry.
    let mut read=get(&cookie_d,"finance-fixture-d","1");*read.uri_mut()="/api/v1/holdings/acme/finance/invoices/field-invoice-no-charge-linked-billing/workspace".parse().unwrap();let next=no_charge_response(runtime.router(),read,StatusCode::OK).await;assert_eq!(next["noCharge"]["canConfirm"],true);
    let mut finish=post(&cookie_d,"no-charge-linked-independent",json!({"expectedVersion":1,"contextKey":next["noCharge"]["contextKey"],"reason":"Independent current reviewer settles the retained zero demand"}).to_string(),"finance-fixture-d","1");*finish.uri_mut()="/api/v1/holdings/acme/finance/invoices/field-invoice-no-charge-linked-billing/confirm-no-charge".parse().unwrap();no_charge_response(runtime.router(),finish,StatusCode::OK).await;
    // The shared review fixture also retains two unrelated draft Agreements.
    // Company-wide settlement must not hide them behind completed visits.
    // Discharge each through the real independent native review command.
    for id in ["review-agreement","review-reject-agreement"] {
        let closed=send_as(&runtime.router(),release_cookie,"agreement-reviewer-b",&format!("no-charge-close-{id}"),&json!({"entity":"agreement","id":id,"action":"reject","expectedVersion":1,"reason":"Independent review declines the unused fixture proposal before Company settlement"}).to_string()).await;
        assert_eq!(closed.0,StatusCode::OK,"{}",closed.1);
    }
    let retired=send_as(&runtime.router(),release_cookie,"agreement-reviewer-b","no-charge-agreement-retire",r#"{"entity":"agreement","id":"agreement","action":"retire","expectedVersion":2,"reason":"Original covered work fulfilled with retained independent confirmations"}"#).await;assert_eq!(retired.0,StatusCode::OK,"{}",retired.1);
    // All five raw owner observations must see the actual completed native
    // visit, reviewed stock cost and zero-demand confirmations before withdrawal.
    {
        use erpx_service::field_service_package_settlement::*;
        let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_package_obligation_sources(&mut tx).await.unwrap();
        let company=erpx_server_shared::public::CompanyId::new("company").unwrap();let field=Box::pin(read_field_package_obligations(&mut tx,&company,"service",1,now)).await.unwrap();
        assert_eq!(field.observation().outstanding,0);
        let support=read_service_support_package_obligations(&mut tx,&company,&field).await.unwrap();assert_eq!(support.observation().outstanding,0);
        let inventory=read_service_inventory_package_obligations(&mut tx,&company,&field).await.unwrap();assert_eq!(inventory.observation().outstanding,0);
        let assets=erpx_server_assets::public::field_service_workspace::read_service_asset_settlement(&mut tx,&company,&field.asset_sources()).await.unwrap();assert!(assets.facts().is_empty());
        let actor=erpx_server_finance::public::FinanceActor{holding_id:erpx_server_shared::public::HoldingId::new("h001").unwrap(),company_id:company.clone(),user_id:erpx_server_shared::public::UserId::new("fixture-d").unwrap(),assignment_id:erpx_server_shared::public::AssignmentId::new("finance-fixture-d").unwrap()};
        // The real consuming batch reuses this prepared query enough to choose
        // a generic plan. Qualify that path directly after the planner OOM;
        // never force custom plans in the production adapter to hide it.
        sqlx::query("SET LOCAL plan_cache_mode='force_generic_plan'").execute(&mut *tx).await.unwrap();
        let finance=Box::pin(read_service_finance_package_obligations(&mut tx,&actor,&field)).await.unwrap();assert_eq!(finance.observation().outstanding,0);finance.require_current(&mut tx,&field).await.unwrap();assets.require_current(&mut tx).await.unwrap();inventory.require_current(&mut tx,&field).await.unwrap();support.require_current(&mut tx,&field).await.unwrap();tx.rollback().await.unwrap();
    }
    // Withdrawal preview must refuse while a real native duty remains; the
    // shared inspect() obligations gate also guards apply (proven by the
    // deferred-drift apply rollback in native_service_commercial_withdrawal).
    // Drain first with the live draft below (mirroring the original draining
    // setup), then deny the inactive batch. The draft is a genuine native
    // command effect, rejected afterwards. A separate Platform identity
    // avoids the withdrawal fixture rows owned later in this case.
    {
        use erpx_server_shared::public::Clock;
        use erpx_server_identity_access::public::account_security::*;
        use erpx_server_commercial::public::purchases::*;
        use erpx_service::field_service_package_settlement::{read_field_package_obligations,lock_field_package_obligation_sources};
        let system=&p.review.system;let control_raw=system.control.raw();let clock=system.scenario.clock.as_ref();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('platform','platform_field_settlement_preview','Settlement denial reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('platform','platform_field_settlement_preview',1,'[\"platform.commercial.read\",\"platform.commercial.manage\"]','[]','[\"platform\"]','fixture',0);INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version)VALUES('platform','fixture-b','local:fixture-b','active','local',1)").execute(control_raw).await.unwrap();
        sqlx::query("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until)VALUES('native-service-settlement-preview','fixture-b','platform_field_settlement_preview','[]',$1,$2)").bind(clock.now().get()-1000).bind(clock.now().get()+3_600_000).execute(control_raw).await.unwrap();
        let central_b=account::login(system,"b@example.test",None).await;let child_b=account::child(system,&central_b,account::ACME).await;
        let commercial_cookie_b=format!("__Host-erpx-product={}",child_b.token.expose());
        let keys=Arc::new(AccountMfaKeyring::new("fixture".into(),vec![("fixture".into(),[7;32])]).unwrap());
        let _enrolled=no_charge_response(runtime.router(),Request::builder().method("POST").uri("/api/v1/account/iam/mfa/enroll").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&commercial_cookie_b).header("content-type","application/json").header("idempotency-key","native-obligated-mfa").body(Body::from(json!({}).to_string())).unwrap(),StatusCode::OK).await;
        let envelope:String=sqlx::query_scalar("SELECT secret FROM iam_mfa WHERE user_id='fixture-b'").fetch_one(control_raw).await.unwrap();
        let code=keys.open("fixture-b",&envelope).unwrap().code(clock.now().get()/30000).unwrap();
        let verified=no_charge_response(runtime.router(),Request::builder().method("POST").uri("/api/v1/account/iam/mfa/verify").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&commercial_cookie_b).header("content-type","application/json").header("idempotency-key","native-obligated-mfa-verify").body(Body::from(json!({"code":code}).to_string())).unwrap(),StatusCode::OK).await;assert!(verified["data"]["verifiedUntil"].as_i64().unwrap()>clock.now().get());
        let holding=erpx_server_shared::public::HoldingId::new("h001").unwrap();let company=erpx_server_shared::public::CompanyId::new("company").unwrap();
        let catalogue=current_package_catalogue().unwrap();
        let commercial_post=|method:&str,uri:String,cookie:&str,key:&str,body:Vec<u8>|Request::builder().method(method).uri(uri).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("content-type","application/json").header("idempotency-key",key).body(Body::from(body)).unwrap();
        let read_batch=||async{
            let mut read=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *read).await.unwrap();lock_commercial_authority_sources(&mut read).await.unwrap();
            let facts=PgCompanyPurchaseReader::new(holding.clone(),company.clone(),&mut read,&catalogue).read(&company).await.unwrap();read.rollback().await.unwrap();facts
        };
        let refs:(String,String,String)=sqlx::query_as("SELECT branch_id,customer_id,currency FROM field_service_agreements WHERE id='agreement'").fetch_one(db).await.unwrap();
        let draft_body=json!({"entity":"agreement","branchId":refs.0,"customerId":refs.1,"assetId":null,"title":"Native obligated denial draft","startDate":"2026-01-01","endDate":"2027-12-31","currency":refs.2,"visitRateMinor":150000,"warrantyDiscountBps":0}).to_string();
        let created=send(&runtime.router(),field_cookie,"native-obligated-draft",&draft_body).await;assert_eq!(created.0,StatusCode::OK,"{}",created.1);
        let draft_id=created.1["data"]["id"].as_str().unwrap().to_owned();
        let mut duty=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *duty).await.unwrap();lock_field_package_obligation_sources(&mut duty).await.unwrap();
        let live=Box::pin(read_field_package_obligations(&mut duty,&company,"service",1,clock.now().get())).await.unwrap();assert!(live.observation().outstanding>0,"the native draft must be a real duty");duty.rollback().await.unwrap();
        let facts=read_batch().await;
        assert!(["service","service-parts-assets"].iter().all(|id|facts.grants.iter().any(|g|&g.package_id==id&&g.state=="active")),"both Service packages must be active");
        let drain_input=GrantBatch{company_id:company.as_str().into(),expected_revision:package_grant_revision(company.as_str(),&facts.grants).unwrap(),idempotency_key:"native-service-obligated-drain".into(),changes:facts.grants.iter().filter(|g|matches!(g.package_id.as_str(),"service"|"service-parts-assets")).map(|g|BatchChange{expected_version:g.version,grant:CompanyPackageGrant{state:"draining".into(),version:g.version+1,reason:"Native obligated denial drain".into(),evidence:"test:native-service-obligated-denial".into(),..g.clone()}}).collect()};
        let drain_value=serde_json::to_value(&drain_input).unwrap();
        let drain_preview=no_charge_response(runtime.router(),commercial_post("POST","/api/v1/platform/commercial/packages/preview".into(),&commercial_cookie_b,"native-obligated-drain-preview",serde_json::to_vec(&json!({"holdingId":"h001","request":drain_value.clone()})).unwrap()),StatusCode::OK).await;
        let drain_token=drain_preview["data"]["review"]["token"].as_str().unwrap().to_owned();
        no_charge_response(runtime.router(),commercial_post("POST","/api/v1/platform/commercial/packages/apply".into(),&commercial_cookie_b,"native-service-obligated-drain",serde_json::to_vec(&json!({"holdingId":"h001","request":drain_value,"token":drain_token})).unwrap()),StatusCode::OK).await;
        let drained:Vec<(String,String)>=sqlx::query_as("SELECT package_id,state FROM company_package_grants WHERE company_id='company' AND package_id IN('service','service-parts-assets') ORDER BY package_id").fetch_all(p.pool().raw()).await.unwrap();assert!(drained.iter().all(|(_,s)|s=="draining"));
        let facts=read_batch().await;
        let inactive_input=GrantBatch{company_id:company.as_str().into(),expected_revision:package_grant_revision(company.as_str(),&facts.grants).unwrap(),idempotency_key:"native-service-obligated-inactive".into(),changes:facts.grants.iter().filter(|g|matches!(g.package_id.as_str(),"service"|"service-parts-assets")&&g.state!="inactive").map(|g|BatchChange{expected_version:g.version,grant:CompanyPackageGrant{state:"inactive".into(),version:g.version+1,reason:"Native obligated denial withdrawal".into(),evidence:"test:native-service-obligated-denial".into(),..g.clone()}}).collect()};
        assert_eq!(inactive_input.changes.len(),2);
        let inactive_value=serde_json::to_value(&inactive_input).unwrap();
        let denied=no_charge_response(runtime.router(),commercial_post("POST","/api/v1/platform/commercial/packages/preview".into(),&commercial_cookie_b,"native-obligated-inactive-denied",serde_json::to_vec(&json!({"holdingId":"h001","request":inactive_value})).unwrap()),StatusCode::CONFLICT).await;
        assert_eq!(denied["error"]["code"],"PACKAGE_OBLIGATIONS_REMAIN");
        let staged:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM company_package_change_sets WHERE id='native-service-obligated-inactive'),(SELECT count(*) FROM company_package_review_evidence WHERE change_set_id='native-service-obligated-inactive')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(staged,(0,0));
        let states:Vec<(String,String)>=sqlx::query_as("SELECT package_id,state FROM company_package_grants WHERE company_id='company' AND package_id IN('service','service-parts-assets') ORDER BY package_id").fetch_all(p.pool().raw()).await.unwrap();assert!(states.iter().all(|(_,s)|s=="draining"));
        let draft_state:String=sqlx::query_scalar("SELECT status FROM field_service_agreements WHERE id=$1").bind(&draft_id).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(draft_state,"draft");
        let reject=json!({"entity":"agreement","id":draft_id,"action":"reject","expectedVersion":1,"reason":"Native obligated denial draft discharged after denial proof"}).to_string();
        let rejected=send_as(&runtime.router(),release_cookie,"agreement-reviewer-b","native-obligated-draft-reject",&reject).await;assert_eq!(rejected.0,StatusCode::OK,"{}",rejected.1);
        let mut duty=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *duty).await.unwrap();lock_field_package_obligation_sources(&mut duty).await.unwrap();
        let restored=Box::pin(read_field_package_obligations(&mut duty,&company,"service",1,clock.now().get())).await.unwrap();assert_eq!(restored.observation().outstanding,0);duty.rollback().await.unwrap();
    }
    Box::pin(native_service_commercial_withdrawal(p,runtime)).await;
    let retained:(i64,i64)=sqlx::query_as("SELECT count(*),max(retained_at) FROM company_package_history_rights WHERE company_id='company' AND package_id IN('service','service-parts-assets')").fetch_one(db).await.unwrap();assert_eq!(retained.0,2);
    // Commercial retention uses PostgreSQL wall time. Advance the existing
    // injected fixture clock to that actual instant; never backdate rights.
    let current_time:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(db).await.unwrap();assert!(current_time>=retained.1);p.review.system.scenario.clock.0.store(current_time,Ordering::SeqCst);
    let history=no_charge_response(runtime.router(),get(&cookie,"finance-fixture-c","1"),StatusCode::OK).await;assert_eq!(history["noCharge"]["confirmed"],true);assert_eq!(history["sourcePurchase"]["history"],true);assert_eq!(history["sourcePurchase"]["settleExisting"],false);
    let old_input=json!({"expectedVersion":1,"contextKey":current["noCharge"]["contextKey"],"reason":"Independent covered service fulfillment"}).to_string();let replay=no_charge_response(runtime.router(),post(&cookie,"native-no-charge",old_input,"finance-fixture-c","1"),StatusCode::OK).await;assert_eq!(replay,result);
    let mutation=sqlx::query("UPDATE field_service_orders SET completion_evidence='Contradictory historical evidence' WHERE id='reserve-order'").execute(db).await.unwrap_err();assert_eq!(mutation.as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));
    let retained_evidence:String=sqlx::query_scalar("SELECT completion_evidence FROM field_service_orders WHERE id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(Some(retained_evidence.as_str()),original_order["completion_evidence"].as_str());
    #[cfg(feature="files-scan-worker")]
    if let Some(file)=visit_file{
        no_charge_visit_history(p,runtime,field_cookie,file,completed_version+2).await;
        // Original source History also retains independently reviewed cost
        // evidence for a Finance-only actor, without allowing new intake.
        let (link,journal,version):(String,String,i64)=sqlx::query_as("SELECT l.id,s.target_id,j.version FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN finance_journals j ON j.id=s.target_id WHERE s.company_id='company' AND s.target_type='finance_journal' AND s.purpose_key='journal_support' AND s.state='bound'").fetch_one(db).await.unwrap();
        let request=|method:&str,path:String,key:&str,body:Vec<u8>|Request::builder().method(method).uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie_d).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-d").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").header("idempotency-key",key).header("content-type","application/json").body(Body::from(body)).unwrap();
        let content=runtime.router().oneshot(request("GET",format!("/api/v1/holdings/acme/finance/journal-files/attachment-links/{link}/content?variant=original"),"",vec![])).await.unwrap();let status=content.status();let bytes=to_bytes(content.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(bytes.as_ref(),include_bytes!("../../../fixtures/files/preview/png-alpha.input"));
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_sets WHERE target_type='finance_journal'").fetch_one(db).await.unwrap();
        no_charge_response(runtime.router(),request("POST","/api/v1/holdings/acme/finance/journal-files/attachment-sets".into(),"journal-history-new-intake",serde_json::to_vec(&json!({"targetType":"finance_journal","targetId":journal,"componentKind":"record","purposeKey":"journal_support","expectedTargetVersion":version})).unwrap()),StatusCode::FORBIDDEN).await;
        let unchanged:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_sets WHERE target_type='finance_journal'").fetch_one(db).await.unwrap();assert_eq!(unchanged,count);

    }
    // Count actual native application callbacks, without a new metrics harness.
    // Replayed decisions must not hide a re-execution of the selected unit.
    let callbacks=Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let device=erpx_server_identity_access::public::DeviceProof{active:Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session,&child.token)),..Default::default()};
    let mut selected=p.review.reviewer_selection.clone();selected.expected.as_mut().unwrap().assignment_id=erpx_server_shared::public::AssignmentId::new("finance-fixture-c").unwrap();selected.expected.as_mut().unwrap().revision=1;
    let config=erpx_service::finance_journal_create::FinanceJournalCreateConfiguration::new(erpx_service::finance_journal_review::FinanceJournalEnvironment::Development,erpx_server_shared::public::HoldingId::new("h001").unwrap(),erpx_server_shared::public::CompanyId::new("company").unwrap(),Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap())).unwrap();
    let logical=erpx_server_finance::public::service_no_charge::normalize_service_no_charge(&json!({"expectedVersion":1,"contextKey":current["noCharge"]["contextKey"],"reason":"Independent covered service fulfillment"}).to_string().into_bytes()).unwrap().normalized().clone();
    let replay_native=||{let selected=selected.clone();let callbacks=callbacks.clone();let logical=logical.clone();erpx_service::finance_service_no_charge::execute_service_no_charge(&p.review.authorizer,"acme.example.test",device.clone(),move||Ok(selected),config.clone(),p.review.system.scenario.clock.clone(),move||{callbacks.fetch_add(1,Ordering::SeqCst);Ok(erpx_server_workflow::public::commands::WorkflowRequest{key:"native-no-charge".into(),operation:"finance.service-no-charge.confirm".into(),record_id:"field-invoice-included-billing".into(),input:logical.clone()})})};
    let (first,second)=tokio::join!(replay_native(),replay_native());let first=first.unwrap();assert_eq!(first,second.unwrap());assert_eq!(callbacks.load(Ordering::SeqCst),2,"two concurrent replay calls must execute their native callback once each");assert_eq!(serde_json::from_str::<serde_json::Value>(first.as_str()).unwrap(),result["data"]);
    let event=sqlx::query_scalar::<_,bool>("SELECT erpx_native_outbox_owned(e) FROM outbox_events e WHERE command_id='native-no-charge'").fetch_one(db).await.unwrap();assert!(event);
    let eligible=sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(format!("SELECT EXISTS(SELECT 1 FROM outbox_events e WHERE command_id='native-no-charge' AND ({}))",erpx_server_finance::public::field_outbox::ELIGIBLE))).fetch_one(db).await.unwrap();assert!(eligible);
    let denied=sqlx::query("UPDATE outbox_events SET attempts=99 WHERE command_id='native-no-charge'").execute(db).await.unwrap_err();assert_eq!(denied.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));
    let mut malformed=db.begin().await.unwrap();
    sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)SELECT 'no-charge-malformed',command_id,'unrelated.event','unrelated','other-record',aggregate_version,payload_json,created_at FROM outbox_events WHERE command_id='native-no-charge'").execute(&mut *malformed).await.unwrap();
    let quarantine:bool=sqlx::query_scalar("SELECT erpx_native_outbox_owned(e) FROM outbox_events e WHERE id='no-charge-malformed'").fetch_one(&mut *malformed).await.unwrap();assert!(quarantine);
    let claim=sqlx::query("UPDATE outbox_events SET attempts=99 WHERE id='no-charge-malformed'").execute(&mut *malformed).await.unwrap_err();assert_eq!(claim.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));malformed.rollback().await.unwrap();
    let mut cost_commands=vec!["no-charge-cost-assign".to_owned(),"no-charge-cost-post".into(),"no-charge-cost-read".into()];
    if visit_directory.is_some(){cost_commands.push(erpx_server_files::public::field_visit_intake::field_set_command_id("company","fixture-d","covered-journal-set"));cost_commands.push("covered-journal-bind".into());}
    let expected_cost_events=cost_commands.len();
    let write=db.clone();crate::projection_fixture::with_database(2,move|read|async move{
        use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&files).await.unwrap();
        let mut attempts=0;while erpx_service::field_dependency_worker::run_once(&write,read.raw(),"native-no-charge-worker",now).await.unwrap(){attempts+=1;assert!(attempts<60);}
        let cost_events:Vec<(String,String,Option<i64>)>=sqlx::query_as("SELECT id,command_id,processed_at FROM outbox_events WHERE command_id=ANY($1) ORDER BY command_id").bind(&cost_commands).fetch_all(&write).await.unwrap();
        assert_eq!(cost_events.len(),expected_cost_events);
        for (event,command,processed) in cost_events{
            assert!(processed.is_some(),"native cost event must be acknowledged: {command}");
            let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=$1").bind(event).fetch_one(read.raw()).await.unwrap();assert_eq!(receipts,1,"native cost projection: {command}");
        }
        let event:String=sqlx::query_scalar("SELECT id FROM outbox_events WHERE command_id='native-no-charge' AND processed_at IS NOT NULL").fetch_one(&write).await.unwrap();
        let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=$1").bind(event).fetch_one(read.raw()).await.unwrap();assert_eq!(receipts,1);
        let work:i64=sqlx::query_scalar("SELECT count(*) FROM work_items WHERE source_id='field-invoice-included-billing'").fetch_one(read.raw()).await.unwrap();assert_eq!(work,0);
    }).await;


}
async fn receipt_workspace_case(secondary_book: bool, http_confirmation: bool, foreign_billing: bool, actual_files: bool,zero_demand:bool) {
    use erpx_server_shared::public::Clock;
    account::with_system(move |system| Box::pin(async move {
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(now,Ordering::SeqCst);
        let p=if zero_demand{crate::facts::setup_without_finance_history_covered(system).await}else if foreign_billing{setup_without_finance_history_in_currency(system,"USD").await}else if actual_files{crate::facts::setup_service_only_without_finance(system).await}else{setup_without_finance_history(system).await};setup_agreement_review_facts(&p,now).await;
        let system=&p.review.system;let db=p.pool().raw();
        sqlx::raw_sql("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement';INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('invoice-ar','company','chart','120','Receivable','asset','THB',1,'receivable',0,0),('invoice-revenue','company','chart','420','Revenue','revenue','THB',0,'none',0,0)").execute(db).await.unwrap();
        let currency=if foreign_billing {"USD"}else{"THB"};
        if foreign_billing {
            // Original fixture source is configured before any native commands;
            // the genuine native billing request must reject a chart-currency mismatch.
            sqlx::raw_sql("UPDATE finance_accounts SET currency='USD' WHERE id IN('invoice-ar','invoice-revenue');").execute(db).await.unwrap();
        }
        let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let central=account::login(system,"b@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-b','company','agreement-reviewer-b',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let reviewer=format!("__Host-erpx-product={}",child.token.expose());
        let finance=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());
        let mut config=private_configuration::document(&system.control,&system.holdings).await;config["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);for flag in ["fieldServiceVisits","fieldServiceVisitCompletions","fieldServiceBillingReleases"]{config[flag]=serde_json::json!(["h001"]);}config["financeJournalReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);config["financeFieldServiceInvoiceReads"]=serde_json::json!(["h001"]);config["financeFieldServiceInvoicePosts"]=serde_json::json!(["h001"]);config["financeFieldServiceReceiptWrites"]=serde_json::json!(["h001"]);if actual_files||zero_demand{config["fieldServiceSourceRecordReads"]=serde_json::json!([{"holdingId":"h001","mode":"enforced"}]);}if actual_files {config["fieldServiceAgreementReviews"]=json!(["h001"]);config["fieldServiceOrderCancellations"]=json!(["h001"]);config["fieldServiceVisitRecoveries"]=json!(["h001"]);}if zero_demand || actual_files {
            config["accountSecurityKeys"]=json!({"current":"fixture","keys":[{"id":"fixture","keyHex":"07".repeat(32)}]});
            config["servicePackageLifecycle"]=json!({"providerHoldingId":"h001","holdingIds":["h001"],"reviewSecret":"fixture-only-secret-32-characters-minimum"});
            let mut no_keys=config.clone();no_keys.as_object_mut().unwrap().remove("accountSecurityKeys");assert!(DevelopmentApiConfiguration::parse(&no_keys.to_string()).is_err());
            let mut shadow=config.clone();shadow["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"shadow"}]);assert!(DevelopmentApiConfiguration::parse(&shadow.to_string()).is_err());
        }let mut no_post=config.clone();no_post["financeFieldServiceInvoicePosts"]=serde_json::json!([]);assert!(DevelopmentApiConfiguration::parse(&no_post.to_string()).is_err());let mut duplicate=config.clone();duplicate["financeFieldServiceReceiptWrites"]=serde_json::json!(["h001","h001"]);assert!(DevelopmentApiConfiguration::parse(&duplicate.to_string()).is_err());
        if zero_demand {for flag in ["fieldServiceReservations","fieldServiceIssues","financeJournalPosts","financeFieldServiceJournalPosts","fieldServiceAgreementCreates"]{config[flag]=json!(["h001"]);}config["fieldServiceRequests"]=json!(["h001"]);config["fieldServiceAgreementReviews"]=json!(["h001"]);config["financeJournalReads"]=json!([{"holdingId":"h001","companyId":"company"},{"holdingId":"h001","companyId":"company-two"}]);}
        #[cfg(feature="files-scan-worker")]
        let mut native_directory=if actual_files {
            use std::os::unix::fs::DirBuilderExt;
            let directory=tempfile::tempdir().unwrap();let objects=directory.path().join("objects");let compute=directory.path().join("compute");std::fs::DirBuilder::new().mode(0o700).create(&objects).unwrap();std::fs::DirBuilder::new().mode(0o700).create(&compute).unwrap();
            config["fieldFilesProviders"]=serde_json::json!({"targets":[{"holdingId":"h001","companyId":"company","filesBinding":"HOLDING_H001_FILES","filesRoot":objects,"temporaryRoot":compute,"previewBinary":"/target/debug/erpx-render-preview","heifConvert":"/usr/bin/heif-convert","pdftoppm":"/usr/bin/pdftoppm","clamavSocket":"/erpx-clam/clamd.sock","clamavDatabaseDirectory":"/erpx-definitions","maximumDefinitionAgeMs":604800000,"commandTimeoutMs":5000,"scanTimeoutMs":30000,"concurrentScans":1}]});
            config["fieldFilesPolicy"]=serde_json::json!({"mode":"enabled","uploadsFrozen":false});
            config["fieldAttachmentLinks"]=serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);config["fieldAttachmentBinds"]=serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);Some(directory)
        }else{None};
        let runtime=Box::pin(assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets))).await.unwrap();
        for (key,body) in [("included-dispatch",r#"{"id":"reserve-order","action":"dispatch","expectedVersion":1,"reason":"Dispatch"}"#),("included-start",r#"{"id":"reserve-order","action":"start","expectedVersion":2,"reason":"Start"}"#),("included-complete",r#"{"id":"reserve-order","action":"complete","expectedVersion":3,"laborMinutes":5,"evidence":"Actual work","customerConfirmation":"Accepted","reason":"Complete"}"#)] {let r=send(&runtime.router(),&cookie,key,body).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1)}
        let billing=r#"{"id":"reserve-order","action":"release-billing","expectedVersion":4,"reason":"Independent release"}"#;let r=send_as(&runtime.router(),&reviewer,"agreement-reviewer-b","included-billing",billing).await;
        if foreign_billing {
            assert_eq!(r.0,StatusCode::UNPROCESSABLE_ENTITY,"{}",r.1);assert_eq!(r.1["error"]["code"],"FIELD_SERVICE_FINANCE_SETUP_REQUIRED");
            let absent:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_invoices WHERE id='field-invoice-included-billing'),(SELECT count(*) FROM command_receipts WHERE command_id='included-billing'),(SELECT count(*) FROM audit_events WHERE command_id='included-billing'),(SELECT count(*) FROM outbox_events WHERE command_id='included-billing')").fetch_one(db).await.unwrap();assert_eq!(absent,(0,0,0,0));
            let order:(String,i64)=sqlx::query_as("SELECT status,version FROM field_service_orders WHERE id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(order,("completed".into(),4));
            runtime.close().await;return;
        }
        assert_eq!(r.0,StatusCode::OK,"{}",r.1);
        if zero_demand {
            #[cfg(feature="files-scan-worker")]
            let visit_directory=native_directory.as_ref();
            #[cfg(not(feature="files-scan-worker"))]
            let visit_directory=None;
            Box::pin(native_no_charge_case(&p,&runtime,now,&finance,&cookie,&reviewer,visit_directory)).await;runtime.close().await;return;
        }
        let get=||Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/workspace").header("host","acme.example.test").header("cookie",&finance).body(Body::empty()).unwrap();let r=runtime.router().oneshot(get()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(body["sourceOnly"],true);assert_eq!(body["data"]["source"]["releaseId"],"field-billing-included-billing");assert_eq!(body["data"]["postingPreview"]["state"],"ready");assert_eq!(body["data"]["postingPreview"]["posted"],false);assert_eq!(body["data"]["postingPreview"]["debit"],10000);assert_eq!(body["data"]["postingPreview"]["credit"],10000);assert_eq!(body["sourcePurchase"]["capability"],"finance.service-invoice");assert_eq!(body["sourcePurchase"]["settleExisting"],true);assert_eq!(body["canPost"],true);assert_eq!(body["purchaseControls"]["finance.accounting"]["history"],false);

        assert_eq!(body["flow"]["kind"],"invoice");
        assert_eq!(body["flow"]["recordId"],"field-invoice-included-billing");
        assert_eq!(body["flow"]["recordVersion"],1);
        assert_eq!(body["flow"]["state"],"draft");
        if actual_files {
            // Authority/roster fixture only. The real command below creates
            // routing/default/event/notice/receipt/audit/outbox effects itself.
            sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_invoice_flow_admin','Synthetic Company Flow administrator',false,1,'active'),('*','native_invoice_flow_review','Synthetic current Finance reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_invoice_flow_admin',1,'[\"finance.read\",\"module.finance.read\",\"organization.manage\"]','[]','[\"holding\"]','synthetic',0),('*','native_invoice_flow_review',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','synthetic',0)").execute(db).await.unwrap();
            for(id,user,role)in [("invoice-flow-admin-a","fixture-a","native_invoice_flow_admin"),("invoice-flow-review-a","fixture-a","native_invoice_flow_review"),("invoice-flow-review-b","fixture-b","native_invoice_flow_review")]{
                sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES($1,$2,$3,'[\"finance\"]','company',$4,$5,3,'holding')").bind(id).bind(user).bind(role).bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
            }
            let central=account::login(system,"a@example.test",None).await;
            let admin=account::child(system,&central,account::ACME).await;
            let admin_cookie=format!("__Host-erpx-product={}",admin.token.expose());
            let select=runtime.router().oneshot(Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&admin_cookie).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"invoice-flow-admin-a","assignmentVersion":3,"revision":0}"#)).unwrap()).await.unwrap();
            let status=select.status();let bytes=to_bytes(select.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
            let admin_get=||Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/workspace").header("host","acme.example.test").header("cookie",&admin_cookie).body(Body::empty()).unwrap();
            let response=runtime.router().oneshot(admin_get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
            let view:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(view["flow"]["canManage"],true);
            assert_eq!(view["flow"]["steps"][0]["candidates"].as_array().unwrap().len(),1);
            let target=view["flow"]["steps"][0]["candidates"][0]["userId"].as_str().unwrap();
            assert_ne!(Some(target),view["data"]["invoice"]["created_by"].as_str());
            let payload=serde_json::json!({"stepKey":"draft","userId":target,"expectedVersion":0,"recordVersion":1,"reason":"Independent invoice responsibility","saveDefault":true}).to_string();
            let assign=|payload:&str,key:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/flow").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&admin_cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","invoice-flow-admin-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").header("idempotency-key",key).body(Body::from(payload.to_owned())).unwrap();
            let mut stale=assign(&payload,"native-invoice-flow");stale.headers_mut().insert("x-erpx-context-revision","999".parse().unwrap());
            assert_eq!(runtime.router().oneshot(stale).await.unwrap().status(),StatusCode::CONFLICT);
            let mut anonymous=assign(&payload,"native-invoice-flow");anonymous.headers_mut().remove("cookie");assert_eq!(runtime.router().oneshot(anonymous).await.unwrap().status(),StatusCode::UNAUTHORIZED);
            sqlx::raw_sql("CREATE FUNCTION native_invoice_flow_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_parties SET name='Deferred poisoned Flow customer' WHERE id='customer' AND company_id=NEW.company_id;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER native_invoice_flow_fault AFTER INSERT ON flow_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='native-invoice-flow') EXECUTE FUNCTION native_invoice_flow_fault()").execute(db).await.unwrap();
            let response=runtime.router().oneshot(assign(&payload,"native-invoice-flow")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::NOT_FOUND,"{}",String::from_utf8_lossy(&bytes));
            let counts:(i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_routes WHERE kind='invoice' AND record_id='field-invoice-included-billing'),(SELECT count(*) FROM flow_defaults WHERE kind='invoice'),(SELECT count(*) FROM flow_events WHERE command_id='native-invoice-flow'),(SELECT count(*) FROM staff_notifications WHERE id='invoice-flow-notice-native-invoice-flow'),(SELECT count(*) FROM command_receipts WHERE command_id='native-invoice-flow'),(SELECT count(*) FROM audit_events WHERE command_id='native-invoice-flow'),(SELECT count(*) FROM outbox_events WHERE command_id='native-invoice-flow')").fetch_one(db).await.unwrap();assert_eq!(counts,(0,0,0,0,0,0,0));
            sqlx::raw_sql("DROP TRIGGER native_invoice_flow_fault ON flow_events;DROP FUNCTION native_invoice_flow_fault()").execute(db).await.unwrap();
            let response=runtime.router().oneshot(assign(&payload,"native-invoice-flow")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
            let acknowledgement:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(acknowledgement["data"],serde_json::json!({"id":"field-invoice-included-billing","version":2}));
            let replay=runtime.router().oneshot(assign(&payload,"native-invoice-flow")).await.unwrap();let status=replay.status();let bytes=to_bytes(replay.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),acknowledgement);
            let changed=payload.replace("Independent invoice responsibility","Changed responsibility reason");
            assert_eq!(runtime.router().oneshot(assign(&changed,"native-invoice-flow")).await.unwrap().status(),StatusCode::CONFLICT);
            let response=runtime.router().oneshot(admin_get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let after:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
            assert_eq!(after["flow"]["version"],2);assert_eq!(after["flow"]["current"]["primary"]["userId"],target);assert_eq!(after["flow"]["recordVersion"],1);assert_eq!(after["flow"]["history"].as_array().unwrap().len(),1);
            let counts:(i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_routes WHERE kind='invoice' AND record_id='field-invoice-included-billing'),(SELECT count(*) FROM flow_defaults WHERE kind='invoice'),(SELECT count(*) FROM flow_events WHERE command_id='native-invoice-flow'),(SELECT count(*) FROM staff_notifications WHERE id='invoice-flow-notice-native-invoice-flow'),(SELECT count(*) FROM command_receipts WHERE command_id='native-invoice-flow'),(SELECT count(*) FROM audit_events WHERE command_id='native-invoice-flow'),(SELECT count(*) FROM outbox_events WHERE command_id='native-invoice-flow')").fetch_one(db).await.unwrap();assert_eq!(counts,(1,1,1,1,1,1,1));
            for(sql,constraint)in [
                ("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at)SELECT 'legacy-invoice-flow-event','legacy-invoice-flow',company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at FROM flow_events WHERE command_id='native-invoice-flow'","native_service_invoice_flow_required"),
                ("UPDATE flow_routes SET version=version+1 WHERE kind='invoice' AND record_id='field-invoice-included-billing'","native_service_invoice_flow_required"),
                ("UPDATE flow_routes SET kind='journal' WHERE kind='invoice' AND record_id='field-invoice-included-billing'","native_service_invoice_flow_required"),
                ("UPDATE flow_events SET reason='legacy edit' WHERE command_id='native-invoice-flow'","flow_event_identity_immutable"),
                ("DELETE FROM flow_events WHERE command_id='native-invoice-flow'","flow_event_identity_immutable"),
                ("UPDATE staff_notifications SET message='legacy edit' WHERE id='invoice-flow-notice-native-invoice-flow'","native_service_invoice_notice_required"),
                ("UPDATE staff_notifications SET read_at=1 WHERE id='invoice-flow-notice-native-invoice-flow'","native_field_staff_state_required"),
                ("UPDATE inbox_deliveries SET read_at=1 WHERE id='invoice-flow-notice-native-invoice-flow'","native_field_inbox_state_required"),
            ]{
                let failure=sqlx::query(sql).execute(db).await.unwrap_err();
                assert_eq!(failure.as_database_error().and_then(|e|e.constraint()),Some(constraint),"{failure}");
            }
            let owned:bool=sqlx::query_scalar("SELECT erpx_native_outbox_owned(e) FROM outbox_events e WHERE command_id='native-invoice-flow'").fetch_one(db).await.unwrap();assert!(owned);
            // Normal native Account/role entry for the actual recipient. Its
            // Finance role has no generic Records permission.
            let address=match target{"fixture-a"=>"a@example.test","fixture-b"=>"b@example.test",_=>panic!("unexpected synthetic recipient")};
            let central=account::login(system,address,None).await;let recipient=account::child(system,&central,account::ACME).await;let recipient_cookie=format!("__Host-erpx-product={}",recipient.token.expose());let assignment=format!("finance-{target}");
            let request=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&recipient_cookie).header("x-erpx-context-company","company").body(Body::from(serde_json::json!({"assignmentId":assignment,"assignmentVersion":3,"revision":0}).to_string())).unwrap();
            let response=runtime.router().oneshot(request).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
            let response=runtime.router().oneshot(Request::builder().uri("/api/v1/holdings/acme/field-service/inbox-candidates?all=true&limit=100").header("host","acme.example.test").header("cookie",&recipient_cookie).body(Body::empty()).unwrap()).await.unwrap();
            let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let inbox:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
            let permissions=inbox["context"]["permissions"].as_array().unwrap();assert!(permissions.iter().any(|p|p=="finance.read"));assert!(!permissions.iter().any(|p|p=="records.read"));
            let item=inbox["data"].as_array().unwrap().iter().find(|n|n["id"]=="invoice-flow-notice-native-invoice-flow").expect("native Finance-only recipient must see its current source-qualified invoice notice");
            assert_eq!(item["source_type"],"invoice");assert_eq!(item["source_id"],"field-invoice-included-billing");assert_eq!(item["native_field_item"]["href"],"/finance/invoices/field-invoice-included-billing");
            // Queue admission uses the existing independent native cursor and
            // current persisted source. No provider is configured or invoked.
            sqlx::query("INSERT INTO push_preferences(company_id,user_id,enabled,updated_at)VALUES('company',$1,true,0) ON CONFLICT(company_id,user_id)DO UPDATE SET enabled=true").bind(target).execute(db).await.unwrap();
            sqlx::query("INSERT INTO push_subscriptions(id,company_id,user_id,session_id,origin,inbox_path,endpoint_hash,sealed_subscription,created_at,updated_at,scan_at,cursor_at,cursor_id)VALUES('invoice-native-push','company',$1,$2,'https://acme.example.test','/work/inbox','synthetic-invoice-endpoint','synthetic-invoice-sealed',0,0,9999999999999,9999999999999,'legacy-ahead')").bind(target).bind(&recipient.result.session_id).execute(db).await.unwrap();
            // Four existing native Field notices remain in this fixture; the new invoice adds one.
            let mut tx=db.begin().await.unwrap();let subscriptions=erpx_server_collaboration::public::web_push::subscriptions(&mut tx).await.unwrap();let subscription=subscriptions.iter().find(|s|s.id=="invoice-native-push").unwrap();assert_eq!(erpx_server_collaboration::public::web_push::fanout(&mut tx,subscription,now).await.unwrap(),5);assert_eq!(erpx_server_collaboration::public::web_push::fanout(&mut tx,subscription,now+1).await.unwrap(),0);tx.commit().await.unwrap();
            let cursor:i64=sqlx::query_scalar("SELECT cursor_at FROM push_subscriptions WHERE id='invoice-native-push'").fetch_one(db).await.unwrap();assert_eq!(cursor,9999999999999);
            let jobs:i64=sqlx::query_scalar("SELECT count(*) FROM push_delivery_jobs WHERE subscription_id='invoice-native-push' AND inbox_delivery_id='invoice-flow-notice-native-invoice-flow'").fetch_one(db).await.unwrap();assert_eq!(jobs,1);
            let failure=sqlx::query("UPDATE push_delivery_jobs SET attempts=99 WHERE subscription_id='invoice-native-push'").execute(db).await.unwrap_err();assert_eq!(failure.as_database_error().and_then(|e|e.constraint()),Some("native_field_push_runtime_required"));
            let response=runtime.router().oneshot(Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/inbox/invoice-flow-notice-native-invoice-flow/state").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&recipient_cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment",&assignment).header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").header("idempotency-key","native-invoice-flow-read").body(Body::from(r#"{"read":true}"#)).unwrap()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
            let read:(Option<i64>,Option<i64>)=sqlx::query_as("SELECT n.read_at,d.read_at FROM staff_notifications n JOIN inbox_deliveries d ON d.id=n.id WHERE n.id='invoice-flow-notice-native-invoice-flow'").fetch_one(db).await.unwrap();assert_eq!(read,(Some(now),Some(now)));

        }
        // Fresh normal Account/SSO session selects Finance through the native
        // role command; no injected role or bypass authorizer is used here.
        let central=account::login(system,"a@example.test",None).await;let poster=account::child(system,&central,account::ACME).await;
        let poster_cookie=format!("__Host-erpx-product={}",poster.token.expose());
        let selected=runtime.router().oneshot(Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&poster_cookie).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"finance-fixture-a","assignmentVersion":3,"revision":0}"#)).unwrap()).await.unwrap();
        let status=selected.status();let bytes=to_bytes(selected.into_body(),65536).await.unwrap();assert_eq!(status,200,"{}",String::from_utf8_lossy(&bytes));
        sqlx::query("UPDATE finance_periods SET start_date='2026-01-01',end_date='2026-12-31' WHERE company_id='company' AND id='period'").execute(db).await.unwrap();
        use erpx_server_identity_access::public::{AuthenticationSecrets,SecretPurpose};
        let proof=erpx_server_identity_access::public::DeviceProof {active:Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session,&poster.token)),..Default::default()};
        let mut selection=p.review.maker_selection.clone();selection.expected.as_mut().unwrap().revision=1;
        let clock=system.scenario.clock.clone();
        let post_config=erpx_service::finance_journal_create::FinanceJournalCreateConfiguration::new(erpx_service::finance_journal_review::FinanceJournalEnvironment::Development,erpx_server_shared::public::HoldingId::new("h001").unwrap(),erpx_server_shared::public::CompanyId::new("company").unwrap(),Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap())).unwrap();
        let input=erpx_server_finance::public::service_invoice_posting::normalize_service_invoice_post(&serde_json::to_vec(&serde_json::json!({"periodId":"period","entryDate":body["data"]["invoice"]["invoice_date"],"expectedVersion":1})).unwrap()).unwrap();
        let post=||{let selection=selection.clone();let input=input.clone();erpx_service::finance_service_invoice_post::execute_service_invoice_post(&p.review.authorizer,"acme.example.test",proof.clone(),move||Ok(selection),post_config.clone(),clock.clone(),move||Ok(erpx_server_workflow::public::commands::WorkflowRequest{key:"native-field-invoice-post".into(),operation:"finance.invoice.post".into(),record_id:"field-invoice-included-billing".into(),input:input.clone()}))};
        let legacy=sqlx::query("UPDATE finance_invoices SET status='posted',version=version+1 WHERE id='field-invoice-included-billing'").execute(db).await.unwrap_err();assert_eq!(legacy.as_database_error().and_then(|e|e.constraint()),Some("native_service_invoice_post_required"));
        let alias=sqlx::query("UPDATE finance_invoices SET source_type='standalone' WHERE id='field-invoice-included-billing'").execute(db).await.unwrap_err();assert_eq!(alias.as_database_error().and_then(|e|e.constraint()),Some("native_service_invoice_post_required"));
        // A late source drift after invoice/journal/Money writes must roll back
        // the entire native unit, including that deferred source mutation.
        let original_name:String=sqlx::query_scalar("SELECT name FROM finance_parties WHERE id='customer' AND company_id='company'").fetch_one(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION native_invoice_post_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_parties SET name='Deferred poisoned customer' WHERE id=NEW.party_id AND company_id=NEW.company_id; RETURN NULL; END $$;CREATE CONSTRAINT TRIGGER native_invoice_post_fault AFTER UPDATE ON finance_invoices DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.id='field-invoice-included-billing' AND NEW.status='posted') EXECUTE FUNCTION native_invoice_post_fault()").execute(db).await.unwrap();
        let failed=post().await;assert!(matches!(&failed,Err(PgTransactionError::Business(e)) if e.code=="INVOICE_NOT_FOUND"),"{failed:?}");
        let rolled_back:(String,i64,i64,i64)=sqlx::query_as("SELECT status,version::bigint,(SELECT count(*) FROM finance_journals WHERE id='journal-native-field-invoice-post'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='finance_invoice' AND source_id='field-invoice-included-billing' AND amount_kind='total') FROM finance_invoices WHERE id='field-invoice-included-billing'").fetch_one(db).await.unwrap();assert_eq!(rolled_back,("draft".into(),1,0,0));
        let failed_effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='native-field-invoice-post'),(SELECT count(*) FROM audit_events WHERE command_id='native-field-invoice-post'),(SELECT count(*) FROM outbox_events WHERE command_id='native-field-invoice-post')").fetch_one(db).await.unwrap();assert_eq!(failed_effects,(0,0,0));
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT name FROM finance_parties WHERE id='customer' AND company_id='company'").fetch_one(db).await.unwrap(),original_name);
        sqlx::raw_sql("DROP TRIGGER native_invoice_post_fault ON finance_invoices;DROP FUNCTION native_invoice_post_fault()").execute(db).await.unwrap();
        let http_post=|origin:&str,company:&str,revision:&str,payload:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/post").header("host","acme.example.test").header("origin",origin).header("cookie",&poster_cookie).header("x-erpx-context-company",company).header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision",revision).header("idempotency-key","native-field-invoice-post").body(Body::from(payload.to_owned())).unwrap();
        for (origin,company,revision) in [("https://evil.example.test","company","1"),("https://acme.example.test","other-company","1"),("https://acme.example.test","company","0")] {
            let denied=runtime.router().oneshot(http_post(origin,company,revision,"malformed body must not precede identity")).await.unwrap();
            assert!(matches!(denied.status().as_u16(),401|403|409),"unexpected identity denial {}",denied.status());
        }
        let source_runtime=|id:&str|Request::builder().uri(format!("/api/v1/holdings/acme/finance/invoices/{id}/post-runtime")).header("host","acme.example.test").header("cookie",&poster_cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1").body(Body::empty()).unwrap();
        let native_runtime=runtime.router().oneshot(source_runtime("field-invoice-included-billing")).await.unwrap();
        assert_eq!(native_runtime.status(),StatusCode::OK);let observed:serde_json::Value=serde_json::from_slice(&to_bytes(native_runtime.into_body(),65536).await.unwrap()).unwrap();assert_eq!(observed["data"],serde_json::json!({"id":"field-invoice-included-billing","runtime":"rust"}));
        sqlx::query("INSERT INTO finance_invoices SELECT (jsonb_populate_record(NULL::finance_invoices,to_jsonb(i)||jsonb_build_object('id','standalone-routing-fixture','invoice_number','STANDALONE-ROUTING','source_type','standalone','source_id',NULL))).* FROM finance_invoices i WHERE id='field-invoice-included-billing'").execute(db).await.unwrap();
        let legacy_runtime=runtime.router().oneshot(source_runtime("standalone-routing-fixture")).await.unwrap();assert_eq!(legacy_runtime.status(),StatusCode::OK);let observed:serde_json::Value=serde_json::from_slice(&to_bytes(legacy_runtime.into_body(),65536).await.unwrap()).unwrap();assert_eq!(observed["data"],serde_json::json!({"id":"standalone-routing-fixture","runtime":"legacy"}));
        let absent=runtime.router().oneshot(source_runtime("missing-invoice")).await.unwrap();assert_eq!(absent.status(),StatusCode::NOT_FOUND);
        let response=runtime.router().oneshot(http_post("https://acme.example.test","company","1",input.as_str())).await.unwrap();
        let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        let http_body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        let posted_result=post().await.unwrap();
        assert_eq!(http_body["data"],serde_json::from_str::<serde_json::Value>(posted_result.as_str()).unwrap());
        let replay=post().await.unwrap();assert_eq!(replay,posted_result);
        let mut changed_input:serde_json::Value=serde_json::from_str(input.as_str()).unwrap();changed_input["expectedVersion"]=serde_json::json!(2);
        let selected=selection.clone();let conflict=erpx_service::finance_service_invoice_post::execute_service_invoice_post(&p.review.authorizer,"acme.example.test",proof.clone(),move||Ok(selected),post_config.clone(),clock.clone(),move||Ok(erpx_server_workflow::public::commands::WorkflowRequest{key:"native-field-invoice-post".into(),operation:"finance.invoice.post".into(),record_id:"field-invoice-included-billing".into(),input:erpx_server_workflow::public::commands::CommandJson::from_serializable(&changed_input).unwrap()})).await;
        assert!(matches!(conflict,Err(PgTransactionError::Business(e)) if e.code=="IDEMPOTENCY_CONFLICT"));
        let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='native-field-invoice-post'),(SELECT count(*) FROM audit_events WHERE command_id='native-field-invoice-post'),(SELECT count(*) FROM outbox_events WHERE command_id='native-field-invoice-post')").fetch_one(db).await.unwrap();assert_eq!(effects,(1,1,1));
        let posted:(String,i64,String,String)=sqlx::query_as("SELECT i.status,i.version::bigint,j.status,s.carried_source_type FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total' WHERE i.id='field-invoice-included-billing'").fetch_one(db).await.unwrap();
        assert_eq!(posted,("posted".into(),2,"posted".into(),"finance_invoice".into()));
        let claim=sqlx::query("UPDATE outbox_events SET attempts=99 WHERE command_id='native-field-invoice-post'").execute(db).await.unwrap_err();assert_eq!(claim.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));
        let write=db.clone();
        crate::projection_fixture::with_database(2,move|read|async move{
            use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
            let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&files).await.unwrap();
            let mut attempts=0;while erpx_service::field_dependency_worker::run_once(&write,read.raw(),"native-invoice-worker",now).await.unwrap(){attempts+=1;assert!(attempts<20);}
            let events:Vec<String>=sqlx::query_scalar("SELECT id FROM outbox_events WHERE command_id IN('native-field-invoice-post','native-invoice-flow','native-invoice-flow-read') ORDER BY id").fetch_all(&write).await.unwrap();assert_eq!(events.len(),if actual_files{3}else{1});
            for event in &events {
                let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=$1").bind(event).fetch_one(read.raw()).await.unwrap();assert_eq!(receipts,1,"native invoice post/Flow must project before acknowledgement: {event}");
            }
            let acknowledged:i64=sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE id=ANY($1) AND processed_at IS NOT NULL").bind(&events).fetch_one(&write).await.unwrap();assert_eq!(acknowledged,events.len() as i64);
            let work:i64=sqlx::query_scalar("SELECT count(*) FROM work_items WHERE source_id='field-invoice-included-billing'").fetch_one(read.raw()).await.unwrap();assert_eq!(work,0);
        }).await;
        let acknowledged:bool=sqlx::query_scalar("SELECT processed_at IS NOT NULL FROM outbox_events WHERE command_id='native-field-invoice-post'").fetch_one(db).await.unwrap();assert!(acknowledged);
        sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('receipt-bank','company','chart','130','Receipt bank','asset','THB',1,'bank',0,0)").execute(db).await.unwrap();
        let receipt_input=erpx_server_finance::public::service_receipt::ServiceReceiptCreateInput{kind:"receipt".into(),party_id:"customer".into(),payment_number:"FS-RECEIPT-NATIVE".into(),payment_date:"2026-11-01".into(),currency:currency.into(),amount:"100.00".into(),bank_account:"receipt-bank".into(),settlement_invoice_id:"field-invoice-included-billing".into()};
        let receipt_create=|input:erpx_server_finance::public::service_receipt::ServiceReceiptCreateInput|{let selected=selection.clone();erpx_service::finance_service_receipt_create::execute_service_receipt_create(&p.review.authorizer,"acme.example.test",proof.clone(),move||Ok(selected),post_config.clone(),clock.clone(),move||Ok(("native-field-receipt-create".into(),erpx_service::finance_service_receipt_create::normalize_service_receipt_create(&serde_json::to_vec(&input).unwrap())?)))};
        sqlx::raw_sql("CREATE FUNCTION native_receipt_create_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_accounts SET status='inactive' WHERE id='receipt-bank'; RETURN NULL; END $$;CREATE CONSTRAINT TRIGGER native_receipt_create_fault AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='native-field-receipt-create') EXECUTE FUNCTION native_receipt_create_fault()").execute(db).await.unwrap();
        assert!(receipt_create(receipt_input.clone()).await.is_err());
        let rollback:(i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_payments WHERE payment_number='FS-RECEIPT-NATIVE'),(SELECT count(*) FROM command_receipts WHERE command_id='native-field-receipt-create'),(SELECT status FROM finance_accounts WHERE id='receipt-bank')").fetch_one(db).await.unwrap();assert_eq!(rollback,(0,0,"active".into()));
        sqlx::raw_sql("DROP TRIGGER native_receipt_create_fault ON command_receipts;DROP FUNCTION native_receipt_create_fault()").execute(db).await.unwrap();
        let created_http=runtime.router().oneshot(Request::builder().method("POST").uri("/api/v1/holdings/acme/finance/payments").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&poster_cookie).header("idempotency-key","native-field-receipt-create").header("x-erpx-context-company","company").header("x-erpx-context-revision","1").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-assignment-version","3").body(Body::from(serde_json::to_vec(&receipt_input).unwrap())).unwrap()).await.unwrap();let status=created_http.status();let bytes=to_bytes(created_http.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&bytes));let created_http:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        let created=receipt_create(receipt_input.clone()).await.unwrap();assert_eq!(created_http["data"],serde_json::from_str::<serde_json::Value>(created.as_str()).unwrap());assert_eq!(receipt_create(receipt_input.clone()).await.unwrap(),created);
        let receipt_body:serde_json::Value=serde_json::from_str(created.as_str()).unwrap();let receipt_id=receipt_body["id"].as_str().unwrap();assert!(receipt_id.starts_with("pay-"));assert_eq!(receipt_body["status"],"draft");assert_eq!(receipt_body["settlementInvoiceId"],"field-invoice-included-billing");
        let saved:(String,i64,i64,i64)=sqlx::query_as("SELECT p.status,p.amount_minor,(SELECT count(*) FROM money_transaction_snapshots m WHERE m.source_type='finance_payment' AND m.source_id=p.id AND m.amount_kind='requested'),(SELECT count(*) FROM outbox_events WHERE command_id='native-field-receipt-create') FROM finance_payments p WHERE p.id=$1").bind(receipt_id).fetch_one(db).await.unwrap();assert_eq!(saved,("draft".into(),10000,1,1));
        let mut changed=receipt_input.clone();changed.amount="99.00".into();assert!(matches!(receipt_create(changed).await,Err(PgTransactionError::Business(e)) if e.code=="IDEMPOTENCY_CONFLICT"));
        use erpx_service::finance_service_receipt_review::{execute_service_receipt_review,ServiceReceiptDecisionInput,normalize_service_receipt_decision};
        let approve_receipt=|as_reviewer:bool,key:&str,version:i64|{let selected=if as_reviewer{p.review.reviewer_selection.clone()}else{selection.clone()};let device=if as_reviewer{p.review.reviewer.clone()}else{proof.clone()};let id=receipt_id.to_owned();let key=key.to_owned();execute_service_receipt_review(&p.review.authorizer,"acme.example.test",device,move||Ok(selected),post_config.clone(),clock.clone(),true,move||Ok((key.clone(),id.clone(),normalize_service_receipt_decision(&serde_json::to_vec(&ServiceReceiptDecisionInput{expected_version:version,proof:"Actual independent receipt review".into()}).unwrap(),true)?)))};
        sqlx::query("INSERT INTO finance_parties SELECT (jsonb_populate_record(NULL::finance_parties,to_jsonb(p)||jsonb_build_object('id','receipt-other-customer','code','RECEIPT-OTHER-CUSTOMER'))).* FROM finance_parties p WHERE id='customer'").execute(db).await.unwrap();
        inject_receipt_fixture_change(&p,receipt_id,"UPDATE finance_payments SET party_id='receipt-other-customer' WHERE id=$1").await;
        assert!(matches!(approve_receipt(true,"native-receipt-wrong-party",1).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PAYMENT_SOURCE_CHANGED"));
        inject_receipt_fixture_change(&p,receipt_id,"UPDATE finance_payments SET party_id='customer' WHERE id=$1").await;
        assert!(matches!(approve_receipt(false,"native-receipt-self-review",1).await,Err(PgTransactionError::Business(e)) if e.code=="SELF_REVIEW_FORBIDDEN"));
        sqlx::raw_sql("CREATE FUNCTION native_receipt_review_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='native-receipt-approve' THEN NEW.input_digest:=repeat('0',64); END IF; RETURN NEW; END $$;CREATE TRIGGER native_receipt_review_fault BEFORE INSERT ON decision_evidence FOR EACH ROW EXECUTE FUNCTION native_receipt_review_fault()").execute(db).await.unwrap();
        assert!(approve_receipt(true,"native-receipt-approve",1).await.is_err());
        let rollback:(String,i64,i64,i64)=sqlx::query_as("SELECT p.status,p.version,(SELECT count(*) FROM command_receipts WHERE command_id='native-receipt-approve'),(SELECT count(*) FROM decision_evidence WHERE command_id='native-receipt-approve') FROM finance_payments p WHERE id=$1").bind(receipt_id).fetch_one(db).await.unwrap();assert_eq!(rollback,("draft".into(),1,0,0));
        sqlx::raw_sql("DROP TRIGGER native_receipt_review_fault ON decision_evidence;DROP FUNCTION native_receipt_review_fault()").execute(db).await.unwrap();
        let receipt_http=|cookie:&str,selected:&erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,action:&str,key:&str,body:&str| {
            let expected=selected.expected.as_ref().unwrap();
            Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/finance/payments/{receipt_id}/{action}")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",cookie).header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-revision",expected.revision.to_string()).header("x-erpx-context-assignment",expected.assignment_id.as_str()).header("x-erpx-context-assignment-version",expected.assignment_version.to_string()).body(Body::from(body.to_owned())).unwrap()
        };
        let approval_body=r#"{"proof":"Actual independent receipt review","expectedVersion":1.0,"ignored":"stripped"}"#;
        let anonymous=receipt_http("",&p.review.reviewer_selection,"approve","native-receipt-approve","not-json");
        assert_eq!(runtime.router().oneshot(anonymous).await.unwrap().status(),StatusCode::UNAUTHORIZED);
        let mut bad_origin=receipt_http(&finance,&p.review.reviewer_selection,"approve","native-receipt-approve",approval_body);bad_origin.headers_mut().insert("origin","https://northstar.example.test".parse().unwrap());
        assert_eq!(runtime.router().oneshot(bad_origin).await.unwrap().status(),StatusCode::FORBIDDEN);
        let http_approval=runtime.router().oneshot(receipt_http(&finance,&p.review.reviewer_selection,"approve","native-receipt-approve",approval_body)).await.unwrap();let status=http_approval.status();let bytes=to_bytes(http_approval.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let http_approval:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        let approved=approve_receipt(true,"native-receipt-approve",1).await.unwrap();assert_eq!(approve_receipt(true,"native-receipt-approve",1).await.unwrap(),approved);assert_eq!(http_approval["data"],serde_json::from_str::<serde_json::Value>(approved.as_str()).unwrap());
        assert!(matches!(approve_receipt(true,"native-receipt-stale-approval",1).await,Err(PgTransactionError::Business(e)) if e.code=="VERSION_CONFLICT"));
        // Explicit synthetic third account, using real native password login,
        // Holding SSO and native role selection; no injected session/context.
        sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\")VALUES('fixture-c','Synthetic Treasury C','c@example.test',1,$1,$1)").bind(now).execute(system.control.raw()).await.unwrap();
        sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") SELECT 'fixture-c','fixture-c','credential','local:credential','fixture-c',password,$1,$1 FROM account WHERE id='fixture-a'").bind(now).execute(system.control.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','native_receipt_treasury','Synthetic independent Treasury',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','native_receipt_treasury',1,'[\"finance.read\",\"finance.write\",\"finance.pay\",\"module.finance.read\"]','[]','[\"holding\"]','synthetic',0)").execute(db).await.unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('finance-fixture-c','fixture-c','native_receipt_treasury','[\"finance\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
        let central=account::login(system,"c@example.test",None).await;let treasury=account::child(system,&central,account::ACME).await;let cookie_c=format!("__Host-erpx-product={}",treasury.token.expose());
        let role=runtime.router().oneshot(Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie_c).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"finance-fixture-c","assignmentVersion":3,"revision":0}"#)).unwrap()).await.unwrap();let status=role.status();let bytes=to_bytes(role.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        let device_c=erpx_server_identity_access::public::DeviceProof{active:Some(SystemAuthenticationSecrets.digest(SecretPurpose::Session,&treasury.token)),..Default::default()};let mut selection_c=selection.clone();selection_c.expected.as_mut().unwrap().assignment_id=erpx_server_shared::public::AssignmentId::new("finance-fixture-c").unwrap();
        let release=||{let selected=selection_c.clone();let id=receipt_id.to_owned();execute_service_receipt_review(&p.review.authorizer,"acme.example.test",device_c.clone(),move||Ok(selected),post_config.clone(),clock.clone(),false,move||Ok(("native-receipt-release".into(),id.clone(),normalize_service_receipt_decision(&serde_json::to_vec(&ServiceReceiptDecisionInput{expected_version:2,proof:"Actual third-person bank release".into()}).unwrap(),false)?)))};
        let http_release=runtime.router().oneshot(receipt_http(&cookie_c,&selection_c,"release","native-receipt-release",r#"{"expectedVersion":2,"proof":"Actual third-person bank release"}"#)).await.unwrap();let status=http_release.status();let bytes=to_bytes(http_release.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let http_release:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        let released=release().await.unwrap();assert_eq!(release().await.unwrap(),released);assert_eq!(http_release["data"],serde_json::from_str::<serde_json::Value>(released.as_str()).unwrap());
        let readback:(String,i64,String,String,String,i64,i64)=sqlx::query_as("SELECT p.status,p.version,p.created_by,p.approved_by,p.released_by,(SELECT count(*) FROM decision_evidence d WHERE d.record_id=p.id),(SELECT count(*) FROM money_transaction_snapshots m WHERE m.source_id=p.id AND m.source_type='finance_payment') FROM finance_payments p WHERE id=$1").bind(receipt_id).fetch_one(db).await.unwrap();assert_eq!(readback,("released".into(),3,"fixture-a".into(),"fixture-b".into(),"fixture-c".into(),2,1));

        let legacy_write=sqlx::query("UPDATE finance_payments SET updated_at=updated_at+1 WHERE id=$1").bind(receipt_id).execute(db).await.unwrap_err();assert_eq!(legacy_write.as_database_error().and_then(|e|e.constraint()),Some("native_service_payment_write_required"));
        let legacy_invoice=sqlx::query("UPDATE finance_invoices SET updated_at=updated_at+1 WHERE id='field-invoice-included-billing'").execute(db).await.unwrap_err();assert_eq!(legacy_invoice.as_database_error().and_then(|e|e.constraint()),Some("native_service_invoice_effect_required"));
        use erpx_server_finance::public::service_receipt::{ServiceReceiptBankInput,ServiceReceiptBankRow};
        let bank_input=ServiceReceiptBankInput{settlement_payment_id:receipt_id.into(),bank_account:"receipt-bank".into(),statement_number:"NATIVE-FIELD-RECEIPT-STATEMENT".into(),statement_date:"2026-11-01".into(),opening:"0".into(),closing:"100.00".into(),closing_fx_rate:None,source_hash:"explicit-synthetic-source".into(),rows:vec![ServiceReceiptBankRow{external_id:"NATIVE-FIELD-RECEIPT-ROW".into(),transaction_date:"2026-11-01".into(),description:Some("Actual controlled receipt evidence".into()),amount:"100.00".into(),currency:currency.into()}]};
        let bank_import=|key:&str,input:ServiceReceiptBankInput|{let selected=selection_c.clone();let key=key.to_owned();erpx_service::finance_service_receipt_bank::execute_service_receipt_bank(&p.review.authorizer,"acme.example.test",device_c.clone(),move||Ok(selected),post_config.clone(),clock.clone(),move||Ok((key.clone(),erpx_service::finance_service_receipt_bank::normalize_service_receipt_bank(&serde_json::to_vec(&input).unwrap())?)))};
        let mut mismatch=bank_input.clone();mismatch.rows[0].amount="99.00".into();assert!(matches!(bank_import("native-receipt-wrong-bank",mismatch).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PAYMENT_BANK_SOURCE_INVALID"));
        sqlx::raw_sql("CREATE FUNCTION native_receipt_bank_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_accounts SET name='Deferred bank drift' WHERE id=NEW.bank_account; RETURN NULL; END $$;CREATE CONSTRAINT TRIGGER native_receipt_bank_fault AFTER INSERT ON finance_bank_statements DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.id='statement-native-receipt-bank') EXECUTE FUNCTION native_receipt_bank_fault()").execute(db).await.unwrap();
        assert!(bank_import("native-receipt-bank",bank_input.clone()).await.is_err());
        let rollback:(i64,i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_bank_statements),(SELECT count(*) FROM finance_bank_transactions),(SELECT count(*) FROM command_receipts WHERE command_id='native-receipt-bank'),(SELECT name FROM finance_accounts WHERE id='receipt-bank')").fetch_one(db).await.unwrap();assert_eq!(rollback,(0,0,0,"Receipt bank".into()));
        sqlx::raw_sql("DROP TRIGGER native_receipt_bank_fault ON finance_bank_statements;DROP FUNCTION native_receipt_bank_fault()").execute(db).await.unwrap();
        let mut bank_request=receipt_http(&cookie_c,&selection_c,"approve","native-receipt-bank",&serde_json::to_string(&bank_input).unwrap());*bank_request.uri_mut()="/api/v1/holdings/acme/finance/bank/statements/import".parse().unwrap();let http_bank=runtime.router().oneshot(bank_request).await.unwrap();let status=http_bank.status();let bytes=to_bytes(http_bank.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&bytes));let http_bank:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        let imported=bank_import("native-receipt-bank",bank_input.clone()).await.unwrap();assert_eq!(http_bank["data"],serde_json::from_str::<serde_json::Value>(imported.as_str()).unwrap());assert_eq!(bank_import("native-receipt-bank",bank_input.clone()).await.unwrap(),imported);

        let legacy_bank=sqlx::query("UPDATE finance_bank_transactions SET match_status='proposed',version=version+1 WHERE id='bankrow-native-receipt-bank-0'").execute(db).await.unwrap_err();assert_eq!(legacy_bank.as_database_error().and_then(|e|e.constraint()),Some("native_service_bank_write_required"));
        let bank:(String,i64,String,i64)=sqlx::query_as("SELECT t.match_status,t.amount_minor,t.currency,s.closing_fx_rate_micros FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id='bankrow-native-receipt-bank-0'").fetch_one(db).await.unwrap();assert_eq!(bank,("unmatched".into(),10000,currency.into(),1000000));
        assert!(matches!(bank_import("native-receipt-duplicate-bank",bank_input.clone()).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PAYMENT_SOURCE_CHANGED"));
        let read_receipt=||{
            let mut selected=selection_c.clone();selected.mutation=false;let config=post_config.clone();let clock=clock.clone();let id=receipt_id.to_owned();
            p.review.authorizer.execute_with_sources_deferred("acme.example.test",device_c.clone(),move||Ok(selected),erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},erpx_service::finance_service_receipt_files::FinancePaymentIntakeSources,move|c,current|{let(config,clock,id)=(config.clone(),clock.clone(),id.clone());Box::pin(async move{erpx_service::finance_service_receipt_files::read_finance_service_receipt_workspace(c,current,&config,clock.as_ref(),&id).await})})
        };
        let(workspace,source_only,purchases)=read_receipt().await.expect("native released receipt workspace");assert_eq!(workspace["payment"]["id"],receipt_id);assert_eq!(workspace["payment"]["status"],"released");assert_eq!(workspace["payment"]["version"],3);assert_eq!(workspace["payment"]["settlement_invoice_id"],"field-invoice-included-billing");assert!(workspace["payment"]["party_name"].is_string());assert_eq!(workspace["allocations"],serde_json::json!([]));assert_eq!(workspace["bankTransactions"][0]["id"],"bankrow-native-receipt-bank-0");assert_eq!(workspace["bankTransactions"].as_array().unwrap().len(),1);assert_eq!(workspace["invoices"][0]["id"],"field-invoice-included-billing");assert!(workspace["invoices"][0].get("due_date").is_none());assert_eq!(workspace["sourcePurchase"]["capability"],"finance.service-invoice");assert_eq!(workspace["sourcePurchase"]["settleExisting"],true);assert_eq!(workspace["sourcePurchase"]["history"],true);
        let get_receipt=||{
            let e=selection_c.expected.as_ref().unwrap();
            Request::builder().uri(format!("/api/v1/holdings/acme/finance/payments/{receipt_id}/workspace")).header("host","acme.example.test").header("cookie",&cookie_c).header("x-erpx-context-company","company").header("x-erpx-context-assignment",e.assignment_id.as_str()).header("x-erpx-context-assignment-version",e.assignment_version.to_string()).header("x-erpx-context-revision",e.revision.to_string()).body(Body::empty()).unwrap()
        };
        let mut anonymous=get_receipt();anonymous.headers_mut().remove("cookie");assert_eq!(runtime.router().oneshot(anonymous).await.unwrap().status(),StatusCode::UNAUTHORIZED);
        let mut stale=get_receipt();stale.headers_mut().insert("x-erpx-context-revision","999".parse().unwrap());assert_eq!(runtime.router().oneshot(stale).await.unwrap().status(),StatusCode::CONFLICT);
        let mut wrong_origin=get_receipt();wrong_origin.headers_mut().insert("origin","https://northstar.example.test".parse().unwrap());assert_eq!(runtime.router().oneshot(wrong_origin).await.unwrap().status(),StatusCode::FORBIDDEN);
        let r=runtime.router().oneshot(get_receipt()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let readback:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(readback["data"],workspace);assert_eq!(readback["sourceOnly"],source_only);assert_eq!(readback["purchaseControls"],serde_json::to_value(purchases).unwrap());assert_eq!(readback["context"]["userId"],"fixture-c");assert_eq!(readback["context"]["companyId"],"company");assert_eq!(readback["filesMode"],if actual_files{"enabled"}else{"disabled"});
        let mut route=get_receipt();*route.uri_mut()=format!("/api/v1/holdings/acme/finance/payments/{receipt_id}/runtime").parse().unwrap();let r=runtime.router().oneshot(route).await.unwrap();assert_eq!(r.status(),StatusCode::OK);let data:serde_json::Value=serde_json::from_slice(&to_bytes(r.into_body(),65536).await.unwrap()).unwrap();assert_eq!(data["data"],serde_json::json!({"id":receipt_id,"runtime":"rust"}));
        sqlx::raw_sql("INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,created_by,created_at,updated_at)VALUES('legacy-routing-payment','company','payment','LEGACY-ROUTING-PAYMENT','2026-11-01','THB',100,100,'fixture-c',1,1),('unknown-routing-receipt','company','receipt','UNKNOWN-ROUTING-RECEIPT','2026-11-01','THB',100,100,'fixture-c',1,1)").execute(db).await.unwrap();
        for (id,status,expected) in [("legacy-routing-payment",StatusCode::OK,"legacy"),("unknown-routing-receipt",StatusCode::SERVICE_UNAVAILABLE,"PAYMENT_OWNER_UNKNOWN"),("missing-routing-payment",StatusCode::NOT_FOUND,"PAYMENT_NOT_FOUND")] {
            let mut route=get_receipt();*route.uri_mut()=format!("/api/v1/holdings/acme/finance/payments/{id}/runtime").parse().unwrap();let r=runtime.router().oneshot(route).await.unwrap();let actual=r.status();let bytes=to_bytes(r.into_body(),65536).await.unwrap();assert_eq!(actual,status,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();if status==StatusCode::OK{assert_eq!(data["data"]["runtime"],expected);}else{assert_eq!(data["error"]["code"],expected);}
        }



        inject_receipt_fixture_change(&p,receipt_id,&format!("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at)VALUES('receipt-mixed-fixture','company',$1,'standalone-routing-fixture',100,'fixture-c',{now})")).await;
        assert!(matches!(bank_import("native-receipt-bank",bank_input.clone()).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PAYMENT_SOURCE_CHANGED"));
        inject_receipt_fixture_change(&p,receipt_id,"DELETE FROM finance_allocations WHERE id='receipt-mixed-fixture' AND payment_id=$1").await;
        assert_eq!(bank_import("native-receipt-bank",bank_input.clone()).await.unwrap(),imported);
        use erpx_server_finance::public::service_receipt::{ServiceReceiptConfirmInput,ServiceReceiptAllocationInput};
        if secondary_book {
            // Actual approved book source, then genuine native settlement; no synthetic posted journal.
            sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('receipt-secondary','company','Receipt secondary','THB','fixture',0,0);INSERT INTO finance_accounts SELECT (jsonb_populate_record(NULL::finance_accounts,to_jsonb(a)||jsonb_build_object('id','receipt-secondary-'||a.id,'chart_id','receipt-secondary'))).* FROM finance_accounts a WHERE a.company_id='company' AND a.chart_id='chart';INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,created_at) VALUES('receipt-book-rule','company','chart','receipt-secondary','2020-01-01','author','Synthetic reviewed rule','{}','synthetic-hash',0);INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) SELECT 'receipt-book-rule',a.id,'receipt-secondary-'||a.id FROM finance_accounts a WHERE a.company_id='company' AND a.chart_id='chart';UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer' WHERE id='receipt-book-rule'").execute(db).await.unwrap();
        }
        #[cfg(feature="files-scan-worker")]
        let mut actual_file=if actual_files {Some(actual_payment_proof(&p,device_c.clone(),selection_c.clone(),post_config.clone(),&cookie_c,runtime.router(),clock.clone(),receipt_id,native_directory.take().unwrap()).await)}else{None};
        #[cfg(not(feature="files-scan-worker"))]
        assert!(!actual_files,"actual payment Files requires provider feature");
        let proof_set_id=if actual_files {
            #[cfg(feature="files-scan-worker")]
            {actual_file.as_ref().unwrap().0.clone()}
            #[cfg(not(feature="files-scan-worker"))]
            {unreachable!()}
        }else{
        // Explicit available-source fixture exercises binding/atomicity only;
        // intake, content upload and scanner are separately pending native Finance integration.
        sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type) VALUES('receipt-proof-upload','finance_payment',$1,'payment_proof','receipt.pdf','application/pdf',10,10,repeat('a',64),'synthetic/receipt-staging','synthetic/receipt-proof','ready','fixture-c',$2+3600000,$2,$2,3,'company','application/pdf')").bind(receipt_id).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES('receipt-proof-source','receipt-proof-upload','finance_payment',$1,'payment_proof','synthetic/receipt-proof',1,'receipt.pdf','application/pdf',10,repeat('a',64),'fixture-c',$2,'company','application/pdf','available',$2,$2)").bind(receipt_id).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('receipt-proof-set','company','finance_payment',$1,'action','bank-confirmation','payment_proof',3,1,'confidential','{}','fixture-c',$2,$2)").bind(receipt_id).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('receipt-proof-link','receipt-proof-set','receipt-proof-source','draft','fixture-c',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();
            "receipt-proof-set".to_owned()
        };
        let confirm_input=ServiceReceiptConfirmInput{expected_version:3,period_id:"period".into(),bank_transaction_id:"bankrow-native-receipt-bank-0".into(),bank_version:1,bank_reference:"Actual bank receipt confirmation".into(),proof:"Independent Treasury confirmed bank statement".into(),fx_rate:None,fx_gain_account_id:None,fx_loss_account_id:None,allocations:vec![ServiceReceiptAllocationInput{invoice_id:"field-invoice-included-billing".into(),amount:"100.00".into()}],attachment_set_ids:vec![proof_set_id.clone()]};
        let confirm=|input:ServiceReceiptConfirmInput|{let selected=selection_c.clone();let id=receipt_id.to_owned();erpx_service::finance_service_receipt_confirm::execute_service_receipt_confirm(&p.review.authorizer,"acme.example.test",device_c.clone(),move||Ok(selected),post_config.clone(),clock.clone(),erpx_server_files::public::payment_proof_binding::PaymentProofBindingFeatures{enabled:true,read_only:false,uploads_frozen:false},move||Ok(("native-receipt-confirm".into(),id.clone(),erpx_service::finance_service_receipt_confirm::normalize_service_receipt_confirm(&serde_json::to_vec(&input).unwrap())?)))};
        // This profile deliberately has no Files providers. HTTP must not
        // enable attachment binding from the body or fall back to TS.
        if !actual_files {
        let unavailable=runtime.router().oneshot(receipt_http(&cookie_c,&selection_c,"confirm","native-receipt-confirm",&serde_json::to_string(&confirm_input).unwrap())).await.unwrap();let status=unavailable.status();let bytes=to_bytes(unavailable.into_body(),65536).await.unwrap();let error:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(status,StatusCode::NOT_FOUND,"{error}");assert_eq!(error["error"]["code"],"ATTACHMENTS_DISABLED");
        }
        let mut stale=confirm_input.clone();stale.expected_version=2;assert!(matches!(confirm(stale).await,Err(PgTransactionError::Business(e)) if e.code=="PAYMENT_NOT_CONFIRMABLE"));
        inject_receipt_fixture_change(&p,receipt_id,"UPDATE finance_payments SET released_by='fixture-a' WHERE id=$1").await;
        assert!(matches!(confirm(confirm_input.clone()).await,Err(PgTransactionError::Business(e)) if e.code=="INDEPENDENT_PAYER_REQUIRED"));
        inject_receipt_fixture_change(&p,receipt_id,"UPDATE finance_payments SET released_by='fixture-c' WHERE id=$1").await;
        // Exact target, purpose and retained policy version must precede writes.
        for (column,value,code) in [("purpose_key","technician_report","ATTACHMENT_BINDING_CONTEXT"),("target_id","another-payment","ATTACHMENT_SET_TARGET"),("component_ref","another-action","ATTACHMENT_BINDING_CONTEXT")] {
            let original:String=sqlx::query_scalar("SELECT CASE $1 WHEN 'purpose_key' THEN purpose_key WHEN 'target_id' THEN target_id ELSE component_ref END FROM attachment_sets WHERE id=$2").bind(column).bind(&proof_set_id).fetch_one(db).await.unwrap();
            sqlx::query("UPDATE attachment_sets SET purpose_key=CASE WHEN $1='purpose_key' THEN $2 ELSE purpose_key END,target_id=CASE WHEN $1='target_id' THEN $2 ELSE target_id END,component_ref=CASE WHEN $1='component_ref' THEN $2 ELSE component_ref END WHERE id=$3").bind(column).bind(value).bind(&proof_set_id).execute(db).await.unwrap();
            let denied=confirm(confirm_input.clone()).await;assert!(matches!(&denied,Err(PgTransactionError::Business(e)) if e.code==code),"{denied:?}");
            sqlx::query("UPDATE attachment_sets SET purpose_key=CASE WHEN $1='purpose_key' THEN $2 ELSE purpose_key END,target_id=CASE WHEN $1='target_id' THEN $2 ELSE target_id END,component_ref=CASE WHEN $1='component_ref' THEN $2 ELSE component_ref END WHERE id=$3").bind(column).bind(original).bind(&proof_set_id).execute(db).await.unwrap();
        }
        sqlx::query("UPDATE attachment_sets SET policy_version=2 WHERE id=$1").bind(&proof_set_id).execute(db).await.unwrap();
        assert!(matches!(confirm(confirm_input.clone()).await,Err(PgTransactionError::Business(e)) if e.code=="ATTACHMENT_SET_CHANGED"));
        sqlx::query("UPDATE attachment_sets SET policy_version=1 WHERE id=$1").bind(&proof_set_id).execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION native_receipt_issue_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.export_sha256:=repeat('0',64); RETURN NEW; END $$;CREATE TRIGGER native_receipt_issue_fault BEFORE INSERT ON finance_customer_receipts FOR EACH ROW EXECUTE FUNCTION native_receipt_issue_fault()").execute(db).await.unwrap();
        let failed=confirm(confirm_input.clone()).await;assert!(matches!(&failed,Err(PgTransactionError::Business(e)) if e.code=="CUSTOMER_RECEIPT_SOURCE_CHANGED"),"{failed:?}");
        let rolled:(String,i64,i64,i64,i64)=sqlx::query_as("SELECT p.status,(SELECT count(*) FROM finance_journals WHERE id='journal-native-receipt-confirm'),(SELECT count(*) FROM finance_allocations WHERE payment_id=p.id),(SELECT count(*) FROM finance_customer_receipts WHERE payment_id=p.id),(SELECT count(*) FROM command_receipts WHERE command_id='native-receipt-confirm') FROM finance_payments p WHERE p.id=$1").bind(receipt_id).fetch_one(db).await.unwrap();assert_eq!(rolled,("released".into(),0,0,0,0));
        sqlx::raw_sql("DROP TRIGGER native_receipt_issue_fault ON finance_customer_receipts;DROP FUNCTION native_receipt_issue_fault()").execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION native_receipt_event_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.journal_id='journal-native-receipt-confirm' THEN NEW.evidence_json:='{}'; END IF; RETURN NEW; END $$;CREATE TRIGGER native_receipt_event_fault BEFORE INSERT ON finance_financial_events FOR EACH ROW EXECUTE FUNCTION native_receipt_event_fault()").execute(db).await.unwrap();
        let failed_event=confirm(confirm_input.clone()).await;assert!(matches!(&failed_event,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PAYMENT_SOURCE_CHANGED"),"{failed_event:?}");
        let rolled:(String,i64,i64,i64,i64)=sqlx::query_as("SELECT p.status,(SELECT count(*) FROM money_transaction_snapshots m WHERE m.source_id=p.id AND m.source_type='finance_payment'),(SELECT count(*) FROM finance_customer_receipts WHERE payment_id=p.id),(SELECT count(*) FROM decision_evidence WHERE command_id='native-receipt-confirm'),(SELECT count(*) FROM finance_sales_number_sequences WHERE company_id='company' AND prefix='RC') FROM finance_payments p WHERE p.id=$1").bind(receipt_id).fetch_one(db).await.unwrap();assert_eq!(rolled,("released".into(),1,0,0,0));
        let files_rolled:(String,String,i64)=sqlx::query_as("SELECT s.state,l.state,(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='set.bound') FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1").bind(&proof_set_id).fetch_one(db).await.unwrap();assert_eq!(files_rolled,("draft".into(),"draft".into(),0));
        sqlx::raw_sql("DROP TRIGGER native_receipt_event_fault ON finance_financial_events;DROP FUNCTION native_receipt_event_fault()").execute(db).await.unwrap();
        let mut successful_input=confirm_input.clone();
        if http_confirmation {
            // Optional evidence is a distinct original contract path. Keep the
            // two positive bound-evidence variants and all their assertions.
            successful_input.attachment_set_ids.clear();
            let http=runtime.router().oneshot(receipt_http(&cookie_c,&selection_c,"confirm","native-receipt-confirm",&serde_json::to_string(&successful_input).unwrap())).await.unwrap();let status=http.status();let bytes=to_bytes(http.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let result:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let replay=confirm(successful_input.clone()).await.unwrap();assert_eq!(result["data"],serde_json::from_str::<serde_json::Value>(replay.as_str()).unwrap());
        }
        if actual_files {
            let http=runtime.router().oneshot(receipt_http(&cookie_c,&selection_c,"confirm","native-receipt-confirm",&serde_json::to_string(&successful_input).unwrap())).await.unwrap();let status=http.status();let bytes=to_bytes(http.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let result:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let replay=confirm(successful_input.clone()).await.unwrap();assert_eq!(result["data"],serde_json::from_str::<serde_json::Value>(replay.as_str()).unwrap());
        }
        let confirmed=confirm(successful_input.clone()).await.expect("fresh confirmation or retained HTTP result");assert_eq!(confirm(successful_input.clone()).await.expect("confirmation replay"),confirmed);
        if http_confirmation {
            let unbound:(String,String,i64)=sqlx::query_as("SELECT s.state,l.state,(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='set.bound') FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1").bind(&proof_set_id).fetch_one(db).await.unwrap();assert_eq!(unbound,("draft".into(),"draft".into(),0));
        }else{
        let files_bound:(String,String,String,String,i64)=sqlx::query_as("SELECT s.state,l.state,l.binding_type,l.binding_id,(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='set.bound') FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1").bind(&proof_set_id).fetch_one(db).await.unwrap();assert_eq!(files_bound,("bound".into(),"bound".into(),"finance_payment_confirmation".into(),format!("{receipt_id}:4"),1));
        }
        let settled:(String,i64,String,i64,String,i64,i64,i64)=sqlx::query_as("SELECT p.status,p.version,i.status,i.outstanding_minor,t.match_status,(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='finance_payment' AND source_id=p.id),(SELECT count(*) FROM finance_customer_receipts WHERE payment_id=p.id),(SELECT count(*) FROM decision_evidence WHERE record_id=p.id) FROM finance_payments p JOIN finance_allocations a ON a.payment_id=p.id JOIN finance_invoices i ON i.id=a.invoice_id JOIN finance_bank_transactions t ON t.matched_id=p.id WHERE p.id=$1").bind(receipt_id).fetch_one(db).await.unwrap();assert_eq!(settled,("settled".into(),4,"paid".into(),0,"matched".into(),3,1,3));
        let(workspace,source_only,purchases)=read_receipt().await.expect("native settled receipt workspace");assert_eq!(workspace["payment"]["status"],"settled");assert_eq!(workspace["payment"]["version"],4);assert_eq!(workspace["allocations"].as_array().unwrap().len(),1);assert_eq!(workspace["allocations"][0]["invoice_id"],"field-invoice-included-billing");assert_eq!(workspace["allocations"][0]["amount_minor"],10000);assert_eq!(workspace["invoices"],serde_json::json!([]));assert_eq!(workspace["bankTransactions"],serde_json::json!([]));
        let r=runtime.router().oneshot(get_receipt()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let readback:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(readback["data"],workspace);assert_eq!(readback["sourceOnly"],source_only);assert_eq!(readback["purchaseControls"],serde_json::to_value(purchases).unwrap());


        #[cfg(feature="files-scan-worker")]
        if let Some((_,link,_,_,_))=actual_file.as_ref() {
            // Complete the original controlled-cash History assertions on
            // native business writes and actual scanned private evidence.
            let grants:Vec<String>=sqlx::query_scalar("SELECT package_id FROM company_package_grants WHERE company_id='company' ORDER BY package_id").fetch_all(db).await.unwrap();assert_eq!(grants,vec!["service"]);
            {
                use erpx_service::field_service_package_settlement::*;
                let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_package_obligation_sources(&mut tx).await.unwrap();
                let company=erpx_server_shared::public::CompanyId::new("company").unwrap();let field=Box::pin(read_field_package_obligations(&mut tx,&company,"service",1,clock.now().get())).await.unwrap();assert!(field.observation().outstanding>0);
                let actor=erpx_server_finance::public::FinanceActor{holding_id:erpx_server_shared::public::HoldingId::new("h001").unwrap(),company_id:company,user_id:erpx_server_shared::public::UserId::new("fixture-c").unwrap(),assignment_id:erpx_server_shared::public::AssignmentId::new("finance-fixture-c").unwrap()};
                let finance=Box::pin(read_service_finance_package_obligations(&mut tx,&actor,&field)).await.unwrap();assert_eq!(finance.observation().outstanding,0);finance.require_current(&mut tx,&field).await.unwrap();field.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
            }
            sqlx::query("UPDATE attachments SET legal_hold=1 WHERE id=(SELECT attachment_id FROM attachment_links WHERE id=$1)").bind(link).execute(db).await.unwrap();
            let content=||{let mut request=get_receipt();*request.uri_mut()=format!("/api/v1/holdings/acme/finance/payment-files/attachment-links/{link}/content?variant=original").parse().unwrap();request};
            let response=runtime.router().oneshot(content()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let original=to_bytes(response.into_body(),10485760).await.unwrap();
            for(key,body)in[("receipt-close-exception",json!({"id":"issue-order","action":"exception","expectedVersion":3,"reason":"Unused fixture visit cannot proceed"})),("receipt-close-cancel",json!({"id":"issue-order","action":"cancel","expectedVersion":4,"reason":"End the remaining contractual fixture visit"}))]{let result=send(&runtime.router(),&cookie,key,&body.to_string()).await;assert_eq!(result.0,StatusCode::OK,"{key}: {}",result.1);}
            for id in ["review-agreement","review-reject-agreement"] {let result=send_as(&runtime.router(),&reviewer,"agreement-reviewer-b",&format!("receipt-reject-{id}"),&json!({"entity":"agreement","id":id,"action":"reject","expectedVersion":1,"reason":"Independently decline unused proposals"}).to_string()).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);}
            let result=send_as(&runtime.router(),&reviewer,"agreement-reviewer-b","receipt-retire-agreement",r#"{"entity":"agreement","id":"agreement","action":"retire","expectedVersion":2,"reason":"Customer invoice and bank receipt fully settled"}"#).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);
            Box::pin(native_service_commercial_withdrawal(&p,&runtime)).await;
            // Retention timestamps are committed by PostgreSQL. As in the
            // existing No-Charge cohort, advance the frozen test clock to an
            // observed database instant; future History rights remain denied.
            let current_time:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(db).await.unwrap();
            let retained_at:i64=sqlx::query_scalar("SELECT max(retained_at) FROM company_package_history_rights WHERE company_id='company'").fetch_one(db).await.unwrap();assert!(current_time>=retained_at);clock.0.store(current_time,Ordering::SeqCst);
            assert_eq!(confirm(successful_input.clone()).await.expect("exact confirmation replay after actual Service withdrawal"),confirmed);
            let response=runtime.router().oneshot(get_receipt()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let history:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(history["data"]["sourcePurchase"]["history"],true);assert_eq!(history["data"]["sourcePurchase"]["settleExisting"],false);assert_eq!(history["data"]["payment"]["status"],"settled");
            let response=runtime.router().oneshot(content()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(to_bytes(response.into_body(),10485760).await.unwrap(),original);
            let held:i64=sqlx::query_scalar("SELECT legal_hold FROM attachments WHERE id=(SELECT attachment_id FROM attachment_links WHERE id=$1)").bind(link).fetch_one(db).await.unwrap();assert_eq!(held,1);
        }

        #[cfg(feature="files-scan-worker")]
        if let Some((set,link,_store,_directory,mut cleanup))=actual_file.take() {
            let expires:i64=sqlx::query_scalar("SELECT max(u.expires_at) FROM upload_intents u JOIN attachment_sets s ON s.id=u.attachment_set_id WHERE s.target_type='finance_payment' AND s.target_id=$1").bind(receipt_id).fetch_one(db).await.unwrap();
            let before_cleanup=clock.now().get();clock.0.store(expires+1,Ordering::SeqCst);
            for _ in 0..3 {cleanup.run_once().await.expect("actual native payment Files scheduled cleanup");}
            clock.0.store(before_cleanup,Ordering::SeqCst);
            let retained:(String,String,String)=sqlx::query_as("SELECT s.state,l.state,a.source_state FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id JOIN attachments a ON a.id=l.attachment_id WHERE s.id=$1 AND l.id=$2").bind(&set).bind(&link).fetch_one(db).await.unwrap();assert_eq!(retained,("bound".into(),"bound".into(),"available".into()));
            let withdrawn:(String,String)=sqlx::query_as("SELECT l.state,u.status FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id JOIN upload_intents u ON u.id=l.upload_intent_id WHERE s.id=$1 AND l.state='withdrawn'").bind(&set).fetch_one(db).await.unwrap();assert_eq!(withdrawn,("withdrawn".into(),"expired".into()));
            let claims:i64=sqlx::query_scalar("SELECT count(*) FROM field_upload_object_cleanup_claims WHERE company_id='company'").fetch_one(db).await.unwrap();assert_eq!(claims,0);
            cleanup.transactions.close().await;
        }

        // Finance receipt/Files and invoice Flow successors use the same native projection
        // checkpoint and never enter the legacy consumer partition.
        let payment_events:Vec<(String,bool)>=sqlx::query_as("SELECT e.id,erpx_native_outbox_owned(e) FROM outbox_events e WHERE e.aggregate_id=$1 OR e.command_id='native-receipt-bank' ORDER BY e.id").bind(receipt_id).fetch_all(db).await.unwrap();
        assert!(payment_events.len()>=5);assert!(payment_events.iter().all(|(_,owned)|*owned));
        let flow_events:Vec<(String,bool)>=sqlx::query_as("SELECT e.id,erpx_native_outbox_owned(e) FROM outbox_events e WHERE e.command_id IN ('native-invoice-flow','native-invoice-flow-read') ORDER BY e.id").fetch_all(db).await.unwrap();
        if actual_files {assert_eq!(flow_events.len(),2);assert!(flow_events.iter().all(|(_,owned)|*owned));}
        for (id,_) in &flow_events {
            let mut predicate=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT ((");
            predicate.push(erpx_server_finance::public::field_outbox::ELIGIBLE).push(") OR ((").push(erpx_server_collaboration::public::field_outbox::ELIGIBLE).push(") AND (").push(erpx_server_workflow::public::field_outbox::COMMAND_EVIDENCE).push("))) FROM outbox_events e WHERE e.id=").push_bind(id);
            let eligible:bool=predicate.build_query_scalar().fetch_one(db).await.unwrap();
            assert!(eligible,"native invoice Flow event must retain complete projection evidence: {id}");
        }
        // The earlier invoice worker already committed the two Flow receipts
        // to its retained read database; this separate database covers successors.
        let event_ids:Vec<String>=payment_events.iter().map(|(id,_)|id.clone()).collect();
        for event in &event_ids {
            let error=sqlx::query("UPDATE outbox_events SET attempts=99 WHERE id=$1").bind(event).execute(db).await.expect_err("legacy cannot claim native receipt/evidence event");
            assert_eq!(error.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));
        }
        let write=db.clone();let expected_events=event_ids.clone();let record=receipt_id.to_owned();
        crate::projection_fixture::with_database(2,move|read|async move{
            use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
            let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&files).await.unwrap();
            let mut attempts=0;while erpx_service::field_dependency_worker::run_once(&write,read.raw(),"native-receipt-worker",now).await.unwrap(){attempts+=1;assert!(attempts<50);}
            let pending:Vec<(String,String,i64,i64,Option<String>)>=sqlx::query_as("SELECT command_id,event_type,next_attempt_at,attempts,last_error FROM outbox_events WHERE id=ANY($1) AND processed_at IS NULL").bind(&expected_events).fetch_all(&write).await.unwrap();
            assert!(pending.is_empty(),"native invoice/receipt projection pending at {now}: {pending:?}");
            let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=ANY($1)").bind(&expected_events).fetch_one(read.raw()).await.unwrap();assert_eq!(receipts,expected_events.len() as i64);
            let work:i64=sqlx::query_scalar("SELECT count(*) FROM work_items WHERE source_id=$1").bind(&record).fetch_one(read.raw()).await.unwrap();assert_eq!(work,0);
        }).await;
        let acknowledged:i64=sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE id=ANY($1) AND processed_at IS NOT NULL").bind(&event_ids).fetch_one(db).await.unwrap();assert_eq!(acknowledged,event_ids.len() as i64);


        if secondary_book {
            let books:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_book_postings WHERE primary_journal_id='journal-native-receipt-confirm'),(SELECT count(*) FROM finance_financial_events WHERE journal_id IN('journal-native-receipt-confirm','book-receipt-book-rule-journal-native-receipt-confirm')),(SELECT sum(debit_minor)::bigint FROM finance_journal_lines WHERE journal_id='book-receipt-book-rule-journal-native-receipt-confirm'),(SELECT sum(credit_minor)::bigint FROM finance_journal_lines WHERE journal_id='book-receipt-book-rule-journal-native-receipt-confirm')").fetch_one(db).await.unwrap();assert_eq!(books,(1,2,10000,10000));
        }
        assert_eq!(approve_receipt(true,"native-receipt-approve",1).await.unwrap(),approved);
        assert_eq!(release().await.unwrap(),released);
        assert_eq!(bank_import("native-receipt-bank",bank_input.clone()).await.unwrap(),imported);
        // Non-Field writers remain independent even after Field allocations
        // exist; a quarantining classifier must not expand to every payment.
        assert_eq!(sqlx::query("UPDATE finance_payments SET updated_at=updated_at+1 WHERE id='legacy-routing-payment'").execute(db).await.unwrap().rows_affected(),1);
        assert_eq!(sqlx::query("UPDATE finance_invoices SET updated_at=updated_at+1 WHERE id='standalone-routing-fixture'").execute(db).await.unwrap().rows_affected(),1);
        let receipt_json:String=sqlx::query_scalar("SELECT snapshot_json FROM finance_customer_receipts WHERE payment_id=$1").bind(receipt_id).fetch_one(db).await.unwrap();let snapshot:serde_json::Value=serde_json::from_str(&receipt_json).unwrap();assert_eq!(snapshot["schema"],"finance.customer-receipt.v1");assert_eq!(snapshot["paymentVersion"],4);assert_eq!(snapshot["allocations"][0]["amountMinor"],"10000");

        // Actual immutable issued-copy endpoint, on the same native cohort and
        // three-person settlement above. No synthetic receipt or source label.
        let issued_id=format!("customer-receipt-{receipt_id}");
        let issued_request=|suffix:&str|{
            let mut request=get_receipt();
            *request.uri_mut()=format!("/api/v1/holdings/acme/finance/sales/receipts/{issued_id}/{suffix}").parse().unwrap();
            request
        };
        let response=runtime.router().oneshot(issued_request("runtime")).await.unwrap();
        let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();
        assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["data"],serde_json::json!({"id":issued_id,"runtime":"rust"}));
        let response=runtime.router().oneshot(issued_request("workspace")).await.unwrap();
        let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();
        assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        let issued:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        assert_eq!(issued["data"]["snapshot"],snapshot);
        assert_eq!(issued["data"]["status"],"issued");
        assert_eq!(issued["data"]["receipt"]["payment_status"],"settled");
        assert_eq!(issued["data"]["receipt"]["payment_version"],4);
        assert!(issued["data"]["receipt"].get("export_html").is_none());
        assert_eq!(issued["context"]["userId"],"fixture-c");
        let original:(String,String)=sqlx::query_as("SELECT export_html,export_sha256 FROM finance_customer_receipts WHERE id=$1").bind(&issued_id).fetch_one(db).await.unwrap();
        assert_eq!(issued["export"]["content"],original.0);
        assert_eq!(issued["export"]["sha256"],original.1);
        assert_eq!(issued["data"]["sha256"],original.1);
        let mut anonymous=issued_request("workspace");anonymous.headers_mut().remove("cookie");
        assert_eq!(runtime.router().oneshot(anonymous).await.unwrap().status(),StatusCode::UNAUTHORIZED);
        let mut stale=issued_request("workspace");stale.headers_mut().insert("x-erpx-context-revision","999".parse().unwrap());
        assert_eq!(runtime.router().oneshot(stale).await.unwrap().status(),StatusCode::CONFLICT);
        let mut cross_origin=issued_request("workspace");cross_origin.headers_mut().insert("origin","https://northstar.example.test".parse().unwrap());
        assert_eq!(runtime.router().oneshot(cross_origin).await.unwrap().status(),StatusCode::FORBIDDEN);
        let mut missing=issued_request("runtime");*missing.uri_mut()="/api/v1/holdings/acme/finance/sales/receipts/missing/runtime".parse().unwrap();
        assert_eq!(runtime.router().oneshot(missing).await.unwrap().status(),StatusCode::NOT_FOUND);
        // This valid persisted later status is a fixture, not a claimed native posting.
        inject_receipt_fixture_change(&p,receipt_id,"UPDATE finance_invoices SET status='voided',version=version+1 WHERE id='field-invoice-included-billing' AND EXISTS(SELECT 1 FROM finance_allocations a WHERE a.invoice_id=finance_invoices.id AND a.payment_id=$1)").await;let r=runtime.router().oneshot(get()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(body["data"]["invoice"]["status"],"voided");assert_eq!(body["data"]["customerReceipts"].as_array().unwrap().len(),1);assert_eq!(body["data"]["customerReceipts"][0]["amount_minor"],10000);assert_eq!(body["data"]["customerReceipts"][0]["payment_status"],"settled");
        assert_eq!(post().await.unwrap(),posted_result); // Historical replay precedes fresh postable-state checks.
        let until:i64=sqlx::query_scalar("SELECT valid_until FROM assignments WHERE id='finance-fixture-a'").fetch_one(db).await.unwrap();
        sqlx::query("UPDATE assignments SET valid_until=$1 WHERE id='finance-fixture-a'").bind(now-1).execute(db).await.unwrap();
        let denied=post().await;assert!(matches!(denied,Err(PgTransactionError::Business(e)) if matches!(e.kind,erpx_server_shared::public::FailureKind::Forbidden|erpx_server_shared::public::FailureKind::Conflict|erpx_server_shared::public::FailureKind::Unauthenticated)));
        sqlx::query("UPDATE assignments SET valid_until=$1 WHERE id='finance-fixture-a'").bind(until).execute(db).await.unwrap();
        assert_eq!(post().await.unwrap(),posted_result);
        let final_effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='native-field-invoice-post'),(SELECT count(*) FROM audit_events WHERE command_id='native-field-invoice-post'),(SELECT count(*) FROM outbox_events WHERE command_id='native-field-invoice-post')").fetch_one(db).await.unwrap();assert_eq!(final_effects,(1,1,1));
        if actual_files {
            let image="SELECT jsonb_build_array((SELECT to_jsonb(p) FROM finance_payments p WHERE id=$1),(SELECT count(*) FROM command_receipts WHERE command_id='native-receipt-confirm'),(SELECT count(*) FROM audit_events WHERE command_id='native-receipt-confirm'),(SELECT count(*) FROM outbox_events WHERE command_id='native-receipt-confirm'))";
            let before:serde_json::Value=sqlx::query_scalar(image).bind(receipt_id).fetch_one(db).await.unwrap();
            sqlx::query("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='finance-fixture-c'").execute(db).await.unwrap();
            let denied=confirm(successful_input.clone()).await;
            let denied_code=match &denied {Err(PgTransactionError::Business(e))=>e.code,Err(PgTransactionError::Persistence(_))=>"UNEXPECTED_PERSISTENCE_ERROR",Ok(_)=>"UNEXPECTED_REPLAY_ALLOWED"};
            assert_eq!(denied_code,"FINANCE_ACCESS_CHANGED","revoked Treasury must not replay a settled Field receipt");
            let after:serde_json::Value=sqlx::query_scalar(image).bind(receipt_id).fetch_one(db).await.unwrap();assert_eq!(after,before);
        }
        runtime.close().await;
    })).await;
}

}

#[test]
fn native_service_invoice_flow_wire_preserves_original_schema_and_utf16() {
    use erpx_service::finance_service_invoice_flow_command::normalize_service_invoice_flow_assignment as normalize;
    let input=br#"{"kind":"stock_count","stepKey":"draft","userId":"\ud800","expectedVersion":1e0,"recordVersion":1,"reason":"\ufeff  Keep original \u00a0","ignored":true}"#;
    let normalized=normalize(input).unwrap();
    assert_eq!(normalized.property("kind").unwrap().unwrap().string_utf16_units().unwrap(),Some("invoice".encode_utf16().collect()));
    assert_eq!(normalized.property("userId").unwrap().unwrap().string_utf16_units().unwrap(),Some(vec![0xd800]));
    assert_eq!(normalized.property("reason").unwrap().unwrap().string_utf16_units().unwrap(),Some("Keep original".encode_utf16().collect()));
    assert_eq!(normalized.property("expectedVersion").unwrap().unwrap().as_str(),"1");
    assert_eq!(normalized.property("saveDefault").unwrap().unwrap().as_str(),"false");
    assert!(normalized.property("ignored").unwrap().is_none());
    for input in [
        r#"{"stepKey":"draft","userId":"person","expectedVersion":9007199254740992,"recordVersion":1,"reason":"Valid reason"}"#,
        r#"{"stepKey":"draft","userId":"person","expectedVersion":1.5,"recordVersion":1,"reason":"Valid reason"}"#,
        r#"{"stepKey":"draft","userId":"person","expectedVersion":0,"recordVersion":0,"reason":"Valid reason"}"#,
        r#"{"stepKey":"draft","userId":"person","expectedVersion":0,"recordVersion":1,"reason":"  no\ufeff "}"#,
        r#"{"stepKey":"draft","userId":"person","expectedVersion":0,"recordVersion":1,"reason":"Valid reason","saveDefault":null}"#,
    ] {assert!(normalize(input.as_bytes()).is_err(),"{input}");}
}

#[tokio::test]
async fn service_prerequisite_commands_apply_replay_and_rollback_actual_owners() {
    use crate::facts::account;
    use crate::service_posting_fixture::service_posting_finance_fixture;
    use erpx_server_runtime::public::postgres::PgTransactionError;
    use erpx_server_shared::public::FailureKind;
    use erpx_server_workflow::public::commands::CommandJson;
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    use erpx_service::service_parts_posting_setup::ServicePostingEnvironment;
    use erpx_service::service_parts_prerequisite_setup::{
        ServicePrerequisiteSources, ServicePrerequisiteWriteSources, execute_service_prerequisite,
        normalize_service_prerequisite, read_service_prerequisites,
    };
    account::with_system(|system|async move {
        let p=service_posting_finance_fixture(system).await;let clock=p.system.scenario.clock.clone();
        let access=||WorkAuthorizationRequest{permission:"finance.read".into(),module:None,destination:"services/field-service".into(),api:true};
        let read=||{let c=clock.clone();p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePrerequisiteSources,move|db,current|{let c=c.clone();Box::pin(async move{read_service_prerequisites(db,current,c.as_ref(),ServicePostingEnvironment::Development).await})})};
        let view=read().await.unwrap();assert!(view.can_create_chart);
        let chart_body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"name":"New synthetic chart","baseCurrency":"thb"}}}}"#,view.context_key.unwrap());
        let chart=normalize_service_prerequisite(&CommandJson::parse(&chart_body).unwrap(),"chart").unwrap();
        let run=|key:&str,input:erpx_service::service_parts_prerequisite_setup::NormalizedPrerequisiteCommand|{let c=clock.clone();let key=key.to_owned();let variant=input.input.variant();p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePrerequisiteWriteSources(variant),move|db,current|{let c=c.clone();let key=key.clone();let input=input.clone();Box::pin(async move{execute_service_prerequisite(db,current,c.as_ref(),&key,&input).await})})};
        let created=run("prereq-chart",chart.clone()).await.unwrap();assert!(!created.replayed);let chart_value:serde_json::Value=serde_json::from_str(created.result.as_str()).unwrap();let chart_id=chart_value["id"].as_str().unwrap().to_owned();assert_eq!(chart_value["baseCurrency"],"THB");assert_eq!(chart_value["accountingBasis"],"IFRS");assert!(run("prereq-chart",chart.clone()).await.unwrap().replayed);
        let changed=normalize_service_prerequisite(&CommandJson::parse(&chart_body.replace("New synthetic chart","Changed chart")).unwrap(),"chart").unwrap();assert!(matches!(run("prereq-chart",changed).await,Err(PgTransactionError::Business(e)) if e.code=="IDEMPOTENCY_CONFLICT"));
        let view=read().await.unwrap();let account_body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"chartId":"{}","code":"121","name":"Synthetic bank","category":"asset","controlKind":"bank"}}}}"#,view.context_key.unwrap(),chart_id);
        let account=normalize_service_prerequisite(&CommandJson::parse(&account_body).unwrap(),"account").unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_prereq_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_charts SET name='changed by trigger' WHERE id='chart'; RETURN NEW; END $$; CREATE TRIGGER fixture_prereq_drift AFTER INSERT ON finance_accounts FOR EACH ROW EXECUTE FUNCTION fixture_prereq_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        let failed=run("prereq-rollback",account.clone()).await;assert!(matches!(failed,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));
        let counts:(i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='prereq-rollback'),(SELECT count(*) FROM finance_accounts WHERE name='Synthetic bank'),(SELECT name FROM finance_charts WHERE id='chart')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(0,0,"Primary".into()));
        sqlx::raw_sql("DROP TRIGGER fixture_prereq_drift ON finance_accounts; DROP FUNCTION fixture_prereq_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_prereq_deferred_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_accounts SET updated_at=updated_at+1 WHERE id=NEW.id; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER fixture_prereq_deferred_drift AFTER INSERT ON finance_accounts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fixture_prereq_deferred_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        assert!(matches!(run("prereq-deferred-drift",account.clone()).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='prereq-deferred-drift'),(SELECT count(*) FROM finance_accounts WHERE name='Synthetic bank')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(0,0));
        sqlx::raw_sql("DROP TRIGGER fixture_prereq_deferred_drift ON finance_accounts; DROP FUNCTION fixture_prereq_deferred_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        let result=run("prereq-account",account.clone()).await.unwrap();let value:serde_json::Value=serde_json::from_str(result.result.as_str()).unwrap();assert_eq!(value["currency"],"THB");assert_eq!(value["isControl"],false);assert!(run("prereq-account",account).await.unwrap().replayed);
        let view=read().await.unwrap();let context=view.context_key.unwrap();let period_body=format!(r#"{{"expectedVersion":0,"contextKey":"{context}","input":{{"name":"Future period","startDate":"2031-01-01","endDate":"2031-12-31"}}}}"#);let period=normalize_service_prerequisite(&CommandJson::parse(&period_body).unwrap(),"period").unwrap();
        let stale=normalize_service_prerequisite(&CommandJson::parse(&period_body.replace(&context,&"0".repeat(64))).unwrap(),"period").unwrap();assert!(matches!(run("prereq-stale",stale).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_CONTEXT_CHANGED"));
        sqlx::raw_sql("CREATE FUNCTION fixture_prereq_iam_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='finance-fixture-a'; RETURN NEW; END $$; CREATE TRIGGER fixture_prereq_iam_drift AFTER INSERT ON finance_periods FOR EACH ROW EXECUTE FUNCTION fixture_prereq_iam_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        assert!(matches!(run("prereq-iam-drift",period.clone()).await,Err(PgTransactionError::Business(_))));let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('prereq-stale','prereq-iam-drift')),(SELECT count(*) FROM finance_periods WHERE name='Future period'),(SELECT version FROM assignments WHERE id='finance-fixture-a')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(0,0,3));
        sqlx::raw_sql("DROP TRIGGER fixture_prereq_iam_drift ON finance_periods; DROP FUNCTION fixture_prereq_iam_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        let result=run("prereq-period",period.clone()).await.unwrap();let value:serde_json::Value=serde_json::from_str(result.result.as_str()).unwrap();assert_eq!(value["version"],0);assert_eq!(value["state"],"open");assert!(run("prereq-period",period).await.unwrap().replayed);
        let view=read().await.unwrap();let rate_body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"currency":"THB","effectiveDate":"2027-01-01","currencyPerUsd":"35.125"}}}}"#,view.context_key.unwrap());let rate=normalize_service_prerequisite(&CommandJson::parse(&rate_body).unwrap(),"rate").unwrap();let result=run("prereq-rate",rate.clone()).await.unwrap();let value:serde_json::Value=serde_json::from_str(result.result.as_str()).unwrap();assert_eq!(value["currencyPerUsd"],"35.125");assert!(run("prereq-rate",rate).await.unwrap().replayed);
        assert!(run("prereq-chart",chart.clone()).await.unwrap().replayed);
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('prereq-chart','prereq-account','prereq-period','prereq-rate')),(SELECT count(*) FROM audit_events WHERE command_id IN ('prereq-chart','prereq-account','prereq-period','prereq-rate')),(SELECT count(*) FROM outbox_events WHERE command_id IN ('prereq-chart','prereq-account','prereq-period','prereq-rate'))").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(4,4,4));
        let replay=run("prereq-chart",chart.clone()).await.unwrap();
        assert_eq!(replay.result,created.result);
        assert!(created.result.property("_servicePartsPrerequisite").unwrap().is_none());
        assert!(value.get("_servicePartsPrerequisite").is_none());
        let events:Vec<(String,bool)>=sqlx::query_as("SELECT e.id,erpx_native_outbox_owned(e) FROM outbox_events e WHERE e.command_id IN('prereq-chart','prereq-account','prereq-period','prereq-rate') ORDER BY e.id").fetch_all(p.system.holdings[0].raw()).await.unwrap();
        assert_eq!(events.len(),4);assert!(events.iter().all(|(_,owned)|*owned));
        let markers:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id IN('prereq-chart','prereq-account','prereq-period','prereq-rate') AND result_json::jsonb->'_servicePartsPrerequisite'->>'kind'='native-service-prerequisite.v1'").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(markers,4);
        for (id,_) in &events {
            let denied=sqlx::query("UPDATE outbox_events SET attempts=99 WHERE id=$1").bind(id).execute(p.system.holdings[0].raw()).await.unwrap_err();
            assert_eq!(denied.as_database_error().and_then(|e|e.constraint()),Some("native_outbox_runtime_required"));
        }
        // A payload label cannot replace server-retained source provenance.
        sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)VALUES('unmarked-prerequisite-event','unmarked-prerequisite','finance.chart.create','finance_chart','unmarked-chart',1,'{\"id\":\"unmarked-chart\",\"version\":1,\"companyId\":\"company\",\"_servicePartsPrerequisite\":true}',0)").execute(p.system.holdings[0].raw()).await.unwrap();
        let owned:bool=sqlx::query_scalar("SELECT erpx_native_outbox_owned(e) FROM outbox_events e WHERE id='unmarked-prerequisite-event'").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert!(!owned);
        let write=p.system.holdings[0].raw().clone();let ids:Vec<String>=events.into_iter().map(|(id,_)|id).collect();let now=clock.now().get();
        crate::projection_fixture::with_database(2,move|read|async move{
            use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
            let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&files).await.unwrap();
            let mut turns=0;while erpx_service::field_dependency_worker::run_once(&write,read.raw(),"native-prerequisite-worker",now).await.unwrap(){turns+=1;assert!(turns<20);}
            for id in &ids {
                let receipt:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=$1").bind(id).fetch_one(read.raw()).await.unwrap();assert_eq!(receipt,1,"native prerequisite projection: {id}");
            }
            let acknowledged:i64=sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE id=ANY($1) AND processed_at IS NOT NULL").bind(&ids).fetch_one(&write).await.unwrap();assert_eq!(acknowledged,4);
            let work:i64=sqlx::query_scalar("SELECT count(*) FROM work_items").fetch_one(read.raw()).await.unwrap();assert_eq!(work,0);
            let untouched:(i64,Option<i64>)=sqlx::query_as("SELECT attempts,processed_at FROM outbox_events WHERE id='unmarked-prerequisite-event'").fetch_one(&write).await.unwrap();assert_eq!(untouched,(0,None));
        }).await;
        let mut draining=p.system.holdings[0].raw().begin().await.unwrap();let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id='company' AND package_id='service'").fetch_one(&mut *draining).await.unwrap();
        sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='service'").execute(&mut *draining).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'prereq-draining','company',package_id,'fixture-a','synthetic-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(before).bind(clock.now().get()).execute(&mut *draining).await.unwrap();draining.commit().await.unwrap();
        assert!(run("prereq-chart",chart).await.unwrap().replayed);let view=read().await.unwrap();assert!(!view.can_create_chart&&!view.can_confirm_rate);
        let new_rate=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"currency":"THB","effectiveDate":"2028-01-01","currencyPerUsd":"36"}}}}"#,view.context_key.unwrap());let new_rate=normalize_service_prerequisite(&CommandJson::parse(&new_rate).unwrap(),"rate").unwrap();assert!(matches!(run("prereq-draining-new",new_rate).await,Err(PgTransactionError::Business(e)) if e.kind==FailureKind::Forbidden));
    }).await;
}


#[tokio::test]
async fn retired_generic_field_records_preserve_history_and_block_all_writers() {
    projection_fixture::with_database(2,|pool|async move{
        use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files=load_migrations(&root,MigrationTrack::HoldingWrite).await.unwrap();
        let current=files.iter().position(|file|file.name()=="1095_native_service_prerequisite_jobs.sql").unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files[..current]).await.unwrap();
        // Deliberate pre-cutover images; no current Field workflow is fabricated.
        sqlx::query("INSERT INTO workspace_records(module_id,id,section_id,title,fields_json,company_id,created_by,updated_by,created_at,updated_at)VALUES('field_service','historical-field-document','orders','Retained old Field document','[]','company','historical','historical',1,2),('operations','retained-other-document','exceptions','Other owner document','[]','company','historical','historical',1,2)").execute(pool.raw()).await.unwrap();
        let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(w) ORDER BY module_id,id) FROM workspace_records w").fetch_one(pool.raw()).await.unwrap();
        let history:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(m) ORDER BY name) FROM erpx_migrations m").fetch_one(pool.raw()).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(w) ORDER BY module_id,id) FROM workspace_records w").fetch_one(pool.raw()).await.unwrap();assert_eq!(before,after);
        let retained:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(m) ORDER BY name) FROM erpx_migrations m WHERE name<$1").bind(files[current].name()).fetch_one(pool.raw()).await.unwrap();assert_eq!(retained,history);
        for sql in [
            "INSERT INTO workspace_records(module_id,id,section_id,title,fields_json,company_id,created_by,updated_by,created_at,updated_at)VALUES('field_service','new-legacy-field-document','orders','Rejected legacy creation','[]','company','historical','historical',1,2)",
            "UPDATE workspace_records SET title='Rejected legacy update' WHERE module_id='field_service'",
            "UPDATE workspace_records SET module_id='operations' WHERE module_id='field_service'",
            "UPDATE workspace_records SET module_id='field_service' WHERE id='retained-other-document'",
            "DELETE FROM workspace_records WHERE module_id='field_service'",
        ] {
            let denied=sqlx::query(sql).execute(pool.raw()).await.unwrap_err();
            assert_eq!(denied.as_database_error().and_then(|e|e.constraint()),Some("native_field_records_required"));
        }
        let unchanged:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(w) ORDER BY module_id,id) FROM workspace_records w").fetch_one(pool.raw()).await.unwrap();assert_eq!(before,unchanged);
        assert_eq!(sqlx::query("UPDATE workspace_records SET title='Other owner still writable' WHERE id='retained-other-document'").execute(pool.raw()).await.unwrap().rows_affected(),1);
        sqlx::query("INSERT INTO workspace_records(module_id,id,section_id,title,fields_json,company_id,created_by,updated_by,created_at,updated_at)VALUES('operations','other-owner-new-document','exceptions','Other owner creation','[]','company','historical','historical',1,2)").execute(pool.raw()).await.unwrap();
        assert_eq!(sqlx::query("DELETE FROM workspace_records WHERE id='other-owner-new-document'").execute(pool.raw()).await.unwrap().rows_affected(),1);
    }).await;
}

/// Historical data/schema preservation, not an authenticated workflow fixture.
/// Current selected-authority/Flow/confirm/replay assertions remain in
/// fully_covered_field_invoice_uses_native_independent_confirmation above.
#[tokio::test]
async fn populated_1054_no_charge_upgrade_keeps_original_money_flow_and_receipts() {
    use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack,PgTransactionError};
    use erpx_server_finance::public::service_no_charge::*;
    projection_fixture::with_database(2,|pool|async move{
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let mut files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();files.retain(|file|file.name()<"1056");
        let split=files.iter().position(|file|file.name().starts_with("1055_")).unwrap();assert_eq!(files.len(),split+1);
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files[..split]).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        // Explicit historical rows: no current IAM/session is fabricated, no
        // command is claimed to have run, and none of this is owner acceptance.
        sqlx::raw_sql(r#"
INSERT INTO organization_nodes(id,kind,slug,display_name,company_id)VALUES('company','company','company','Historical Company','company');
INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id)VALUES('branch','branch','branch','Historical branch','company','company','branch');
INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('customer','company','customer','HIST','Original old-schema customer','THB',1,1);
INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('warehouse','company','branch','HIST','Historical warehouse');
INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('sale','company','branch','sales','HIST','customer','warehouse','THB','2026-10-06','2026-11-05',10000,0,'creator',1,1);
INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at)VALUES('ticket','company','branch','sale','customer','Retained original case','Original issue','technician','2026-10-06','creator',1,1);
INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,cost_minor,acquired_date,warranty_end,created_by,created_at,updated_at)VALUES('asset','company','branch','HIST','Populated original covered equipment','equipment','THB',8000,'2026-01-01','2099-12-31','creator',1,1);
INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,asset_id,title,start_date,end_date,currency,visit_rate_minor,warranty_discount_bps,status,created_by,reviewed_by,reviewed_at,version,created_at,updated_at)VALUES('ordinary','company','branch','customer',NULL,'Original non-warranty retained agreement','2026-01-01','2027-12-31','THB',10000,0,'active','creator','reviewer',1,2,1,1),('covered','company','branch','customer','asset','Retained original fully covered agreement','2026-01-01','2027-12-31','THB',10000,10000,'active','creator','reviewer',1,2,1,1);
INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at)SELECT id||'-create',id,1,'create','creator','Declared historical draft','draft',1 FROM field_service_agreements UNION ALL SELECT id||'-activate',id,2,'activate','reviewer','Declared historical independent review','active',1 FROM field_service_agreements;
INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,asset_id,issue,site,technician_id,scheduled_at,status,created_by,version,visit_started_at,visit_completed_at,labor_minutes,completion_evidence,customer_confirmation,created_at,updated_at)VALUES('ordinary-order','company','branch','ticket',1,'ordinary',2,NULL,'Original issue','Original customer site','technician',60001,'scheduled','creator',1,NULL,NULL,NULL,NULL,NULL,1,1),('covered-order','company','branch','ticket',1,'covered',2,'asset','Original issue','Original customer site','technician',60001,'billed','creator',5,2,3,20,'Original pre1055 completed inspection','Original customer accepted covered visit',1,4);
INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES('ordinary-create','ordinary-order',1,'create','creator','Declared historical request','scheduled','technician',60001,'{}',1),('covered-create','covered-order',1,'create','creator','Declared historical request','scheduled','technician',60001,'{}',1),('covered-dispatch','covered-order',2,'dispatch','technician','Original covered dispatch','dispatched','technician',60001,'{}',2),('covered-start','covered-order',3,'start','technician','Original customer site access','in_visit','technician',60001,'{}',2),('service-order-event-covered-complete','covered-order',4,'complete','technician','Actual completed pre1055 covered source','completed','technician',60001,'{"laborMinutes":20,"evidence":"Original pre1055 completed inspection","customerConfirmation":"Original customer accepted covered visit"}',3),('service-order-event-covered-release','covered-order',5,'release-billing','reviewer','Original independent pre1055 full warranty release','billed','technician',60001,'{}',4);
INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)VALUES('covered-complete','field.order.complete','technician',repeat('a',64),'covered-order','{"id":"covered-order","status":"completed","version":4}',3,'{"companyId":"company"}'),('covered-release','field.order.release-billing','reviewer',repeat('b',64),'covered-order','{"id":"covered-order","status":"billed","version":5}',4,'{"companyId":"company"}'),('original-flow','flow.assign','creator',repeat('c',64),'field-invoice-covered-release','{"id":"field-invoice-covered-release","version":1}',5,'{"companyId":"company"}');
INSERT INTO field_service_billing_releases(id,order_id,agreement_id,customer_id,currency,amount_minor,source_order_version,released_by,reason,created_at,warranty_applied,warranty_discount_bps,warranty_end_snapshot,pricing_date)VALUES('field-billing-covered-release','covered-order','covered','customer','THB',0,5,'reviewer','Original independent pre1055 full warranty release',4,true,10000,'2099-12-31','2026-10-06');
INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Original chart','THB','creator',1,1);
INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at)VALUES('ar','chart','company','1200','Original trade receivables','asset','THB',1,'receivable',1,1),('revenue','chart','company','4100','Original revenue','revenue','THB',0,'none',1,1);
INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,created_by,created_at,updated_at)VALUES('field-invoice-covered-release','company','customer','receivable','FS-HIST','2026-10-06','2026-11-05','THB',0,0,0,0,'ar','revenue','field_service','covered-order','draft','reviewer',4,4);
INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)VALUES('company','field_service_agreement','covered','visit_rate','2026-01-01','THB',2,10000,10000,NULL,true,1),('company','field_service_billing_release','field-billing-covered-release','invoice_total','2026-10-06','THB',2,0,0,NULL,true,4),('company','finance_invoice','field-invoice-covered-release','draft:v1','2026-10-06','THB',2,0,0,NULL,true,4);
INSERT INTO flow_routes(company_id,kind,record_id,version,defaults_revision,steps_json,created_at)VALUES('company','invoice','field-invoice-covered-release',1,0,'{"draft":{"primaryUserId":"manager","teamName":"Finance","defaultRevision":0},"posted":{"primaryUserId":null,"teamName":"Finance","defaultRevision":0}}',5);
INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at)VALUES('original-flow-event','original-flow','company','invoice','field-invoice-covered-release','draft','assign','creator','creator','Original independent Finance responsibility',1,5);
"#).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        let mut missing=pool.raw().begin().await.unwrap();
        let error=lock_service_no_charge_sources(&mut missing).await.unwrap_err();assert!(matches!(error,PgTransactionError::Business(e)if e.code=="SERVICE_NO_CHARGE_SCHEMA_REQUIRED"));missing.rollback().await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM command_receipts WHERE command_id='unavailable-new-operation'").fetch_one(pool.raw()).await.unwrap(),0);
        let shapes:Vec<(String,Vec<String>)>=sqlx::query_as("SELECT t.tablename::text,array_agg(c.column_name::text ORDER BY c.ordinal_position) FROM pg_tables t JOIN information_schema.columns c ON c.table_schema=t.schemaname AND c.table_name=t.tablename WHERE t.schemaname='public' AND t.tablename<>'erpx_migrations' GROUP BY t.tablename ORDER BY t.tablename").fetch_all(pool.raw()).await.unwrap();
        async fn snapshot(pool:&erpx_server_runtime::public::postgres::DatabasePool,shapes:&[(String,Vec<String>)])->Vec<(String,i64,String)>{let mut result=Vec::new();for(table,columns)in shapes{assert!(std::iter::once(table).chain(columns).all(|v|!v.is_empty()&&v.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'_')));let columns=columns.iter().map(|v|format!("\"{v}\"")).collect::<Vec<_>>().join(",");let query=format!("SELECT count(*)::bigint,md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) FROM(SELECT {columns} FROM \"{table}\")t");let(count,digest):(i64,String)=sqlx::query_as(sqlx::AssertSqlSafe(query)).fetch_one(pool.raw()).await.unwrap();result.push((table.clone(),count,digest));}result}
        let before=snapshot(&pool,&shapes).await;
        let baselines:Vec<serde_json::Value>=sqlx::query_scalar("SELECT to_jsonb(b) FROM work_cache_revision_baselines b ORDER BY table_name").fetch_all(pool.raw()).await.unwrap();
        let receipts:Vec<(String,String)>=sqlx::query_as("SELECT name,checksum FROM erpx_migrations ORDER BY name").fetch_all(pool.raw()).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        assert_eq!(snapshot(&pool,&shapes).await.into_iter().filter(|r|r.0!="work_cache_revision_baselines").collect::<Vec<_>>(),before.into_iter().filter(|r|r.0!="work_cache_revision_baselines").collect::<Vec<_>>());
        let retained:Vec<serde_json::Value>=sqlx::query_scalar("SELECT to_jsonb(b) FROM work_cache_revision_baselines b WHERE table_name<>'finance_service_no_charge_confirmations' ORDER BY table_name").fetch_all(pool.raw()).await.unwrap();assert_eq!(retained,baselines);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM work_cache_revision_baselines WHERE table_name='finance_service_no_charge_confirmations'").fetch_one(pool.raw()).await.unwrap(),2);
        let retained:Vec<(String,String)>=sqlx::query_as("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1055_%' ORDER BY name").fetch_all(pool.raw()).await.unwrap();assert_eq!(retained,receipts);
        let counts:(i64,i64)=sqlx::query_as("SELECT(SELECT count(*) FROM finance_service_no_charge_confirmations),(SELECT count(*) FROM finance_service_no_charge_completed)").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0));
        let source:(String,i64,String)=sqlx::query_as("SELECT invoice_id,invoice_version,invoice_status FROM finance_service_no_charge_candidates WHERE invoice_id='field-invoice-covered-release'").fetch_one(pool.raw()).await.unwrap();assert_eq!(source,("field-invoice-covered-release".into(),1,"draft".into()));
        // Native Finance consumes the preserved original Money/invoice, not a
        // TS source read. This owner witness is not an authorization grant.
        let mut read=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *read).await.unwrap();
        let actor=erpx_server_finance::public::FinanceActor{user_id:erpx_server_shared::public::UserId::new("manager").unwrap(),holding_id:HoldingId::new("h001").unwrap(),company_id:CompanyId::new("company").unwrap(),assignment_id:erpx_server_shared::public::AssignmentId::new("finance-manager").unwrap()};
        let original=read_optional_service_no_charge(&mut read,&actor,"field-invoice-covered-release").await.unwrap().expect("historical zero source remains readable");assert_eq!(original.version().unwrap(),1);assert!(!original.confirmed());
        let command_actor=erpx_server_workflow::public::commands::CommandActor{user_id:actor.user_id.clone(),assignment_id:actor.assignment_id.clone(),company_id:actor.company_id.clone(),role:"finance_approver".into(),branch_id:erpx_server_workflow::public::commands::CommandField::Null,department_id:erpx_server_workflow::public::commands::CommandField::Null,authority:erpx_server_workflow::public::commands::CommandField::Omitted,iam:erpx_server_workflow::public::commands::CommandField::Omitted,assignment_scope_kind:"holding".into(),assignment_scope_node_id:None};
        // Owner review policy only; actual selected IAM/Flow authority is
        // exercised by the existing normal-authentication confirmation cases.
        let key=original.context_key(&command_actor).unwrap();original.require_decision(1,&key,&command_actor,true).unwrap();
        assert_eq!(original.require_decision(1,&key,&command_actor,false).unwrap_err().code,"INDEPENDENT_APPROVER_REQUIRED");
        original.require_current(&mut read).await.unwrap();read.rollback().await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files[..split]).await.unwrap();
        // Historical SQL shapes remain usable after selecting the older source
        // inventory. This is not a restored TS command or an authority bypass.
        let mut shapes=pool.raw().begin().await.unwrap();
        sqlx::raw_sql(r#"
UPDATE field_service_orders SET status='dispatched',version=2,updated_at=6 WHERE id='ordinary-order' AND version=1;
INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES('ordinary-dispatch','ordinary-order',2,'dispatch','technician','Historical command shape after additive upgrade','dispatched','technician',60001,'{}',6);
INSERT INTO field_service_orders SELECT(jsonb_populate_record(NULL::field_service_orders,to_jsonb(o)||jsonb_build_object('id','new-ordinary-order','status','scheduled','version',1,'created_at',7,'updated_at',7))).* FROM field_service_orders o WHERE o.id='ordinary-order';
INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)VALUES('new-ordinary-create','new-ordinary-order',1,'create','creator','Historical fresh request shape after additive upgrade','scheduled','technician',60001,'{}',7);
"#).execute(&mut *shapes).await.unwrap();shapes.commit().await.unwrap();
        let states:Vec<(String,String,i64)>=sqlx::query_as("SELECT id,status,version FROM field_service_orders WHERE id IN('ordinary-order','new-ordinary-order') ORDER BY id").fetch_all(pool.raw()).await.unwrap();assert_eq!(states,vec![("new-ordinary-order".into(),"scheduled".into(),1),("ordinary-order".into(),"dispatched".into(),2)]);
        let retained_after:Vec<(String,String)>=sqlx::query_as("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1055_%' ORDER BY name").fetch_all(pool.raw()).await.unwrap();assert_eq!(retained_after,receipts);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grants").fetch_one(pool.raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM erpx_migrations WHERE name LIKE '1055_%'").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}

#[tokio::test]
async fn native_field_executive_evidence_keeps_cursor_audit_and_source_rollback() {
    use service_invoice_posting_cases::no_charge_response;
    use axum::{body::Body,http::{Request,StatusCode}};
    use erpx_service::private_development_api::{assemble_development_api,DevelopmentApiConfiguration};
    use erpx_server_identity_access::public::SystemAuthenticationSecrets;
    facts::account::with_system(|system|async move{
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,std::sync::atomic::Ordering::SeqCst);
        let p=facts::setup_workspace(system).await;let system=&p.review.system;let db=p.pool().raw();
        sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
        let mut config=runtime_configuration::document(&system.control,&system.holdings).await;
        config["fieldServiceReads"]=json!([{"holdingId":"h001","companyId":"company"}]);config["fieldServiceSourceRecordReads"]=json!([{"holdingId":"h001","mode":"enforced"}]);
        for flag in ["fieldServiceRequests","fieldServiceVisits","fieldServiceVisitCompletions","fieldServiceOrderCancellations"]{config[flag]=json!(["h001"]);}
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let central=facts::account::login(system,"a@example.test",None).await;let child=facts::account::child(system,&central,facts::account::ACME).await;let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let select=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/work-roles").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").body(Body::from(r#"{"assignmentId":"field-fixture-a","assignmentVersion":3,"revision":0}"#)).unwrap();no_charge_response(runtime.router(),select,StatusCode::OK).await;
        let call=|path:&str,key:&str,body:serde_json::Value|{let mut r=Request::builder().method("POST").uri(path).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","1");if !key.is_empty(){r=r.header("idempotency-key",key);}r.body(Body::from(body.to_string())).unwrap()};
        let scheduled=(now/86_400_000+2)*86_400_000+43_200_000;
        let mut orders=Vec::new();
        for(label,at)in[("completed",scheduled+1000),("pending",scheduled),("cancelled",scheduled),("outside",scheduled+86_400_000)]{
            let created=no_charge_response(runtime.router(),call("/api/v1/holdings/acme/field-service",&format!("evidence-request-{label}"),json!({"entity":"request","agreementId":"agreement","agreementVersion":2,"title":format!("Native evidence {label}"),"issue":"Actual native source inspection","site":"Customer site","technicianId":"fixture-a","scheduledAt":at})),StatusCode::OK).await;
            orders.push(created["data"]["id"].as_str().unwrap().to_owned());
        }
        let completed=orders[0].as_str();let pending=orders[1].as_str();let cancelled=orders[2].as_str();let outside=orders[3].as_str();
        for(action,version)in[("dispatch",1),("start",2),("complete",3)]{
            let mut body=json!({"id":completed,"action":action,"expectedVersion":version,"reason":"Actual native evidence visit"});if action=="complete"{body["laborMinutes"]=json!(45);body["evidence"]=json!("Original completed inspection");body["customerConfirmation"]=json!("Customer accepted the visit");}
            no_charge_response(runtime.router(),call("/api/v1/holdings/acme/field-service",&format!("evidence-{action}"),body),StatusCode::OK).await;
        }
        no_charge_response(runtime.router(),call("/api/v1/holdings/acme/field-service","evidence-cancel",json!({"id":cancelled,"action":"cancel","expectedVersion":1,"reason":"Customer cancelled this visit"})),StatusCode::OK).await;
        let date=time::OffsetDateTime::from_unix_timestamp(scheduled/1000).unwrap().date().to_string();let month=&date[..7];let midnight=scheduled-43_200_000;let filters=json!({"from":date,"to":date,"currency":"THB"});
        for(breakdown,expected)in[("default",json!([{"label":"Field visits","numerator":"1","denominator":"2"}])),("month",json!([{"label":month,"numerator":"1","denominator":"2"}])),("status",json!([{"label":"completed","numerator":"1","denominator":"1"},{"label":"scheduled","numerator":"0","denominator":"1"}]))]{
            let mut report=call(&format!("/api/v1/holdings/acme/field-service/executive?metric=field-visit-completion&fromAt={midnight}&toAt={midnight}&breakdown={breakdown}"),"",json!({}));*report.method_mut()=axum::http::Method::GET;*report.body_mut()=Body::empty();
            let response=no_charge_response(runtime.router(),report,StatusCode::OK).await;assert_eq!(response["data"]["raw"],expected);assert_eq!(response["data"]["sources"].as_array().unwrap().len(),2);
        }
        let body=json!({"metric":"field-visit-completion","filters":filters,"query":"","limit":1});let path="/api/v1/holdings/acme/field-service/executive-evidence/search";
        let mut anonymous=call(path,"",body.clone());anonymous.headers_mut().remove("cookie");no_charge_response(runtime.router(),anonymous,StatusCode::UNAUTHORIZED).await;
        let first=no_charge_response(runtime.router(),call(path,"",body.clone()),StatusCode::OK).await;assert_eq!(first["data"]["provider"],"field-service-orders");assert_eq!(first["data"]["items"].as_array().unwrap().len(),1);assert!(!first["data"]["nextCursor"].as_str().unwrap().is_empty());assert_eq!(first["data"]["hasMore"],true);assert_eq!(first["data"]["items"][0]["id"],completed);assert_eq!(first["data"]["items"][0]["version"],4);
        let original=format!(r#"{{"providerId":"field-service-orders","metric":"field-visit-completion","filters":{{"from":"{date}","to":"{date}","currency":"THB"}},"query":"","cursor":null}}"#);use sha2::{Digest,Sha256};assert_eq!(first["data"]["queryHash"],format!("{:x}",Sha256::digest(original.as_bytes())));
        let mut second_body=body.clone();second_body["cursor"]=first["data"]["nextCursor"].clone();let second=no_charge_response(runtime.router(),call(path,"",second_body),StatusCode::OK).await;assert_eq!(second["data"]["hasMore"],false);assert_eq!(second["data"]["items"].as_array().unwrap().len(),1);assert_eq!(second["data"]["items"][0]["id"],pending);
        let mut matched=body.clone();matched["query"]=json!(completed);let result=no_charge_response(runtime.router(),call(path,"",matched),StatusCode::OK).await;assert_eq!(result["data"]["items"].as_array().unwrap().len(),1);assert_eq!(result["data"]["items"][0]["id"],completed);
        let open=|id:&str|json!({"metric":"field-visit-completion","filters":filters,"recordId":id});
        let detail=no_charge_response(runtime.router(),call("/api/v1/holdings/acme/field-service/executive-evidence/open","",open(completed)),StatusCode::OK).await;assert_eq!(detail["data"]["record"]["id"],completed);assert_eq!(detail["data"]["record"]["href"],format!("/field-service/{completed}"));assert_eq!(detail["data"]["record"]["status"],"completed");assert_eq!(detail["data"]["record"]["version"],4);assert!(detail["data"]["record"]["visit_completed_at"].as_i64().unwrap()>0);assert_eq!(detail["data"]["fields"],json!(["scheduled_at","visit_completed_at","status","version"]));
        no_charge_response(runtime.router(),call("/api/v1/holdings/acme/field-service/executive-evidence/open","",open(cancelled)),StatusCode::NOT_FOUND).await;
        no_charge_response(runtime.router(),call("/api/v1/holdings/acme/field-service/executive-evidence/open","",open(outside)),StatusCode::NOT_FOUND).await;
        let audits:Vec<(String,String,i64,serde_json::Value)>=sqlx::query_as("SELECT action,record_id,record_version,field_ids FROM executive_evidence_access_events WHERE provider_id='field-service-orders' AND metric_id='field-visit-completion' ORDER BY created_at,id").fetch_all(db).await.unwrap();assert_eq!(audits.len(),4);assert_eq!(audits.iter().filter(|r|r.0=="search").count(),3);assert!(audits.iter().any(|r|r.0=="open"&&r.1==completed&&r.2==4));assert!(audits.iter().all(|r|r.3==json!(["scheduled_at","visit_completed_at","status","version"])));
        let legacy=sqlx::query("INSERT INTO executive_evidence_access_events(id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)VALUES('legacy-attempt','company','fixture-a','field-fixture-a','field-service-orders','field-visit-completion','open','reserve-order',4,'[]',repeat('a',64),0)").execute(db).await.unwrap_err();assert_eq!(legacy.as_database_error().and_then(|e|e.constraint()),Some("native_field_evidence_required"));
        let mut foreign=call(path,"",body.clone());foreign.headers_mut().insert("origin","https://northstar.example.test".parse().unwrap());no_charge_response(runtime.router(),foreign,StatusCode::FORBIDDEN).await;
        let mut stale=call(path,"",body.clone());stale.headers_mut().insert("x-erpx-context-revision","0".parse().unwrap());no_charge_response(runtime.router(),stale,StatusCode::CONFLICT).await;
        let mut foreign_company=call(path,"",body.clone());foreign_company.headers_mut().insert("x-erpx-context-company","another-company".parse().unwrap());no_charge_response(runtime.router(),foreign_company,StatusCode::CONFLICT).await;
        sqlx::raw_sql("CREATE FUNCTION evidence_deferred_source_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_parties SET name='Deferred evidence source drift' WHERE id='customer'; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER evidence_deferred_source_drift AFTER INSERT ON executive_evidence_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION evidence_deferred_source_drift()").execute(db).await.unwrap();
        use tower::ServiceExt;let response=runtime.router().oneshot(call(path,"",body.clone())).await.unwrap();assert_ne!(response.status(),StatusCode::OK,"late source drift must roll back the access audit");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM executive_evidence_access_events WHERE provider_id='field-service-orders'").fetch_one(db).await.unwrap(),4);let name:String=sqlx::query_scalar("SELECT name FROM finance_parties WHERE id='customer'").fetch_one(db).await.unwrap();assert_ne!(name,"Deferred evidence source drift");
        // A real published permission withdrawal must not be simulated by
        // changing the actor supplied by a caller; the same session is denied.
        sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_field_parts',2,'[\"module.field_service.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0)").execute(db).await.unwrap();
        sqlx::query("UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_field_parts'").execute(db).await.unwrap();
        let denied=no_charge_response(runtime.router(),call(path,"",body),StatusCode::FORBIDDEN).await;assert_eq!(denied["error"]["code"],"EVIDENCE_FORBIDDEN");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM executive_evidence_access_events WHERE provider_id='field-service-orders'").fetch_one(db).await.unwrap(),4);
        runtime.close().await;
    }).await;
}
