//! Disposable native Field Service development case. Never installs a service or uses
//! an existing application database; fixture cleanup verifies each database lease.
#[allow(dead_code)]
#[expect(
    unused_imports,
    reason = "Shared real Service fixture reexports helpers used by other targets."
)]
#[path = "../../tests/support/field_service/parts.rs"]
mod field_service;
#[allow(dead_code)]
#[path = "../../tests/support/runtime/private-session-config.rs"]
mod private_fixture;

use erpx_server_identity_access::public::{SystemAccountClock, SystemAuthenticationSecrets};
use erpx_service::private_development_api::{
    DevelopmentApiConfiguration, assemble_development_api,
};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn write_private(dir: &Path, name: &str, value: &serde_json::Value) -> PathBuf {
    let path = dir.join(name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .expect("private case file creation failed");
    serde_json::to_writer(&mut file, value).expect("private case file encoding failed");
    file.write_all(b"\n")
        .expect("private case file write failed");
    path
}
struct CaseFiles(Vec<PathBuf>);
impl Drop for CaseFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        (3..=4).contains(&args.len()),
        "usage: native-field-service-development ABSOLUTE_PRIVATE_CASE_DIR LOOPBACK_LISTEN [post|request|order-create|visit|complete|recovery|cancel|agreement-review|agreement-create|billing-release|finance-invoice-read|workspace|detail|work]"
    );
    let dir = PathBuf::from(&args[1]);
    assert!(
        dir.is_absolute(),
        "absolute private case directory required"
    );
    let meta = fs::symlink_metadata(&dir).expect("private case directory required");
    assert!(
        meta.is_dir() && !meta.file_type().is_symlink() && meta.permissions().mode() & 0o077 == 0,
        "private case directory required"
    );
    let dir = dir.canonicalize().expect("private case directory required");
    let listen = args[2].clone();
    let scenario = args.get(3).map(String::as_str).unwrap_or("issue");
    assert!(
        [
            "issue",
            "post",
            "request",
            "order-create",
            "visit",
            "complete",
            "recovery",
            "cancel",
            "agreement-review",
            "agreement-create",
            "billing-release",
            "finance-invoice-read",
            "workspace",
            "detail",
            "work"
        ]
        .contains(&scenario),
        "invalid private fixture scenario"
    );
    let agreement_review_scenario = scenario == "agreement-review";
    let agreement_create_scenario = scenario == "agreement-create";
    let workspace_scenario = scenario == "workspace";
    let work_scenario = scenario == "work";
    let detail_scenario = scenario == "detail" || work_scenario;
    let invoice_read_scenario = scenario == "finance-invoice-read";
    let billing_scenario = scenario == "billing-release" || invoice_read_scenario;
    let visit_scenario = scenario == "visit";
    let completion_scenario = scenario == "complete";
    let recovery_scenario = scenario == "recovery";
    let cancel_scenario = scenario == "cancel";
    let post_scenario = scenario == "post";
    let request_scenario = scenario == "request";
    let order_create_scenario = scenario == "order-create";
    field_service::account::with_system(move |system| async move {
        let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).expect("wall clock unavailable").as_millis()).expect("wall clock invalid");
        system.scenario.clock.0.store(now, Ordering::SeqCst);
        let prepared = if workspace_scenario || detail_scenario { field_service::setup_workspace(system).await } else { field_service::setup(system).await };
        let system = &prepared.review.system;
        if work_scenario {
            let mut tx = prepared.pool().raw().begin().await.expect("synthetic Work purchase transaction unavailable");
            sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at) VALUES('company','work',1,'active',0,1,'Synthetic Work purchase','public synthetic evidence',$1)").bind(now).execute(&mut *tx).await.expect("synthetic Work purchase unavailable");
            sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'work-fixture-origin',company_id,package_id,'fixture-a','synthetic-work-grant',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='work'").bind(now).execute(&mut *tx).await.expect("synthetic Work purchase event unavailable");
            tx.commit().await.expect("synthetic Work purchase commit unavailable");
        }
        sqlx::raw_sql("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(system.holdings[0].raw()).await.expect("synthetic field service source unavailable");
        // A genuine reserve command creates the immutable Service/Workflow provenance.
        // The visit is synthetic historical data, not an implemented transition.
        if agreement_review_scenario { field_service::setup_agreement_review_facts(&prepared, now).await; }
        if !visit_scenario && !agreement_review_scenario && !agreement_create_scenario && !workspace_scenario && !detail_scenario {
        let reserve = format!(r#"{{"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":{},"expiresAt":{},"reason":"Synthetic issue reservation"}}"#, if completion_scenario || recovery_scenario || cancel_scenario || billing_scenario {2000} else {1000}, now + 3_600_000);
        prepared.execute("reserve-order", "browser-issue-reserve", &reserve).await.expect("synthetic reserve command failed");
        let mut historical = system.holdings[0].raw().begin().await.expect("historical visit transaction unavailable");
        sqlx::query("UPDATE field_service_orders SET status='in_visit',version=3 WHERE id='reserve-order'").execute(&mut *historical).await.expect("synthetic historical visit unavailable");
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES('synthetic-start-issue','reserve-order',3,'start','fixture-a','Synthetic historical visit','in_visit','fixture-a',$1+60000,'{}',$1)").bind(now).execute(&mut *historical).await.expect("synthetic historical event unavailable");
        historical.commit().await.expect("synthetic historical visit commit failed");
        }
        if detail_scenario {prepared.issue("browser-detail-prior-issue").await.expect("native detail prior issue unavailable");}
        let journal_id = if post_scenario || completion_scenario || recovery_scenario || cancel_scenario || billing_scenario {
            let issue = r#"{"action":"issue-part","expectedVersion":3,"reservationId":"field-part-browser-issue-reserve","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Synthetic post source"}"#;
            prepared.execute("reserve-order", "browser-post-issue", issue).await.expect("synthetic issue command failed");
            let id: String = sqlx::query_scalar("SELECT id FROM finance_journals WHERE company_id='company' AND source_id='operational-part-browser-post-issue'").fetch_one(system.holdings[0].raw()).await.expect("generated cost journal unavailable");
        // Independent Finance staff is backed by current HR and a published
        // scoped IAM role; Workflow selects its actual primary during posting.
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','journal_flow_reviewer','Journal workflow reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','journal_flow_reviewer',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','fixture',0);UPDATE assignments SET role='journal_flow_reviewer' WHERE id='finance-fixture-b';INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('person-reviewer','fixture-b','company','Reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('reviewer-engagement','person-reviewer','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0)").execute(system.holdings[0].raw()).await.expect("journal reviewer fixture unavailable");
            if completion_scenario || recovery_scenario || cancel_scenario || billing_scenario {
                let config=erpx_service::finance_journal_review::FinanceJournalConfiguration::new_enforced_post(erpx_service::finance_journal_review::FinanceJournalEnvironment::Development,erpx_server_shared::public::HoldingId::new("h001").unwrap(),erpx_server_shared::public::CompanyId::new("company").unwrap(),Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap())).unwrap().with_field_service_post().expect("completion Finance fixture config unavailable");
                let admin=erpx_service::finance_journal_review::FinanceJournalReviewAdministration::new(prepared.review.authorizer.clone(),config,system.scenario.clock.clone());
                admin.review("acme.example.test",prepared.review.reviewer.clone(),prepared.review.reviewer_selection.clone(),field_service::review::command(&id,"browser-completion-finance-post",field_service::post())).await.expect("actual Finance post unavailable");
            }
            if cancel_scenario {
                use tower::ServiceExt;
                let central=field_service::account::login(system,"a@example.test",None).await;
                let child=field_service::account::child(system,&central,field_service::account::ACME).await;
                sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(system.holdings[0].raw()).await.expect("recovery context unavailable");
                let config=erpx_service::field_service_parts::FieldServicePartsConfiguration::new(erpx_service::field_service_parts::FieldServicePartsEnvironment::Development,erpx_server_shared::public::HoldingId::new("h001").unwrap(),erpx_server_shared::public::CompanyId::new("company").unwrap(),erpx_server_commercial::public::purchases::PackageEnforcementMode::Enforced).unwrap();
                let service=Arc::new(erpx_service::field_service_parts::FieldServicePartsAdministration::new(prepared.review.authorizer.clone(),config,system.scenario.clock.clone(),system.scenario.clock.clone()));
                let dispatcher=Arc::new(erpx_service::field_service_parts::VisitDispatcher{clock:system.scenario.clock.clone(),allow_visits:false,allow_completion:false,allow_recovery:true,allow_cancel:false,allow_billing:false});
                let router=erpx_service::field_service_http::field_service_http_router(Arc::new(field_service::account::fixture::account::policy()),field_service::account::fixture::account::holding_registry(),Arc::new(erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders),vec![erpx_service::field_service_http::FieldServiceHttpTarget::new_private_creates("acme.example.test".into(),"acme".into(),service,None,None,(Some(dispatcher),None,None),(false,false))]).expect("recovery router unavailable");
                let request=axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).header("idempotency-key","browser-cancel-exception").header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").body(axum::body::Body::from(r#"{"id":"reserve-order","action":"exception","expectedVersion":4,"reason":"Synthetic access interruption"}"#)).unwrap();
                let response=router.oneshot(request).await.expect("real recovery request failed");assert_eq!(response.status(),axum::http::StatusCode::OK,"real recovery prerequisite rejected");
            }
            if billing_scenario {
                sqlx::raw_sql("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,is_control,created_at,updated_at) VALUES('billing-ar','company','chart','120','Synthetic receivable','asset','THB','receivable',1,0,0),('billing-revenue','company','chart','410','Synthetic Service revenue','revenue','THB','none',0,0,0)").execute(system.holdings[0].raw()).await.expect("Service billing Finance accounts unavailable");
                use tower::ServiceExt;
                // The prerequisite completion is an actual native command by the
                // assigned technician. The prior Start remains historical fixture data.
                let central=field_service::account::login(system,"a@example.test",None).await;
                let child=field_service::account::child(system,&central,field_service::account::ACME).await;
                sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(system.holdings[0].raw()).await.expect("completion context unavailable");
                let config=erpx_service::field_service_parts::FieldServicePartsConfiguration::new(erpx_service::field_service_parts::FieldServicePartsEnvironment::Development,erpx_server_shared::public::HoldingId::new("h001").unwrap(),erpx_server_shared::public::CompanyId::new("company").unwrap(),erpx_server_commercial::public::purchases::PackageEnforcementMode::Enforced).unwrap();
                let service=Arc::new(erpx_service::field_service_parts::FieldServicePartsAdministration::new(prepared.review.authorizer.clone(),config,system.scenario.clock.clone(),system.scenario.clock.clone()));
                let dispatcher=Arc::new(erpx_service::field_service_parts::VisitDispatcher{clock:system.scenario.clock.clone(),allow_visits:false,allow_completion:true,allow_recovery:false,allow_cancel:false,allow_billing:false});
                let router=erpx_service::field_service_http::field_service_http_router(Arc::new(field_service::account::fixture::account::policy()),field_service::account::fixture::account::holding_registry(),Arc::new(erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders),vec![erpx_service::field_service_http::FieldServiceHttpTarget::new_private_creates("acme.example.test".into(),"acme".into(),service,None,None,(Some(dispatcher),None,None),(false,false))]).expect("completion router unavailable");
                let request=axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).header("idempotency-key","browser-billing-complete").header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").body(axum::body::Body::from(r#"{"id":"reserve-order","action":"complete","expectedVersion":4,"laborMinutes":45,"evidence":"Synthetic completed service evidence","customerConfirmation":"Synthetic customer confirmation","reason":"Synthetic completed visit"}"#)).unwrap();
                let response=router.oneshot(request).await.expect("native completion request failed");assert_eq!(response.status(),axum::http::StatusCode::OK,"native completion prerequisite rejected");
                sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','field_billing_reviewer','Synthetic independent Service billing reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','field_billing_reviewer',1,'[\"records.read\",\"records.write\",\"records.review\",\"module.field_service.read\"]','[]','[\"holding\"]','fixture',0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES('company','fixture-b','local:fixture-b','person-reviewer','active','local',1)").execute(system.holdings[0].raw()).await.expect("Service billing reviewer unavailable");
                sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('field-billing-reviewer','fixture-b','field_billing_reviewer','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(system.holdings[0].raw()).await.expect("Service billing assignment unavailable");
            }
            if invoice_read_scenario {
                use tower::ServiceExt;
                let central=field_service::account::login(system,"b@example.test",None).await;let child=field_service::account::child(system,&central,field_service::account::ACME).await;
                sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','field-billing-reviewer',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(system.holdings[0].raw()).await.expect("billing reviewer context unavailable");
                let config=erpx_service::field_service_parts::FieldServicePartsConfiguration::new(erpx_service::field_service_parts::FieldServicePartsEnvironment::Development,erpx_server_shared::public::HoldingId::new("h001").unwrap(),erpx_server_shared::public::CompanyId::new("company").unwrap(),erpx_server_commercial::public::purchases::PackageEnforcementMode::Enforced).unwrap();
                let service=Arc::new(erpx_service::field_service_parts::FieldServicePartsAdministration::new(prepared.review.authorizer.clone(),config,system.scenario.clock.clone(),system.scenario.clock.clone()));
                let dispatcher=Arc::new(erpx_service::field_service_parts::VisitDispatcher{clock:system.scenario.clock.clone(),allow_visits:false,allow_completion:false,allow_recovery:false,allow_cancel:false,allow_billing:true});
                let router=erpx_service::field_service_http::field_service_http_router(Arc::new(field_service::account::fixture::account::policy()),field_service::account::fixture::account::holding_registry(),Arc::new(erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders),vec![erpx_service::field_service_http::FieldServiceHttpTarget::new_private_creates("acme.example.test".into(),"acme".into(),service,None,None,(Some(dispatcher),None,None),(false,false))]).expect("billing router unavailable");
                let request=axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).header("idempotency-key","browser-invoice-release").header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","field-billing-reviewer").header("x-erpx-context-assignment-version","3").body(axum::body::Body::from(r#"{"id":"reserve-order","action":"release-billing","expectedVersion":5,"reason":"Independent synthetic invoice release"}"#)).unwrap();
                let response=router.oneshot(request).await.expect("native billing request failed");assert_eq!(response.status(),axum::http::StatusCode::OK,"native billing prerequisite rejected");
            }
            Some(id)
        } else if detail_scenario { Some(prepared.journal("browser-detail-prior-issue").await) } else { None };
        if order_create_scenario || workspace_scenario {
            sqlx::query("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('source-sale','company','branch','sales','SOURCE-SALE','customer','warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',$1,$1)").bind(now).execute(system.holdings[0].raw()).await.expect("synthetic sales source unavailable");
            sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,version)VALUES('existing-ticket','company','branch','source-sale','customer','Synthetic source','Synthetic original problem','fixture-a','2026-12-31','fixture-a',$1,$1,3)").bind(now).execute(system.holdings[0].raw()).await.expect("synthetic Support source unavailable");
            for version in 1..=3 {sqlx::query("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) VALUES($1,'existing-ticket',$2,'create','fixture-a','Synthetic retained Support history','fixture-a','2026-12-31','open',$3,'{}')").bind(format!("existing-ticket-history-{version}")).bind(version).bind(now).execute(system.holdings[0].raw()).await.expect("synthetic Support history unavailable");}
        }
        let control = private_fixture::database(&system.control).await;
        let h001 = private_fixture::database(&system.holdings[0]).await;
        let h002 = private_fixture::database(&system.holdings[1]).await;
        let mut doc = serde_json::json!({
            "version":1,"mode":"development","transport":"direct-loopback","listen":listen,
            "accountHost":"account-dev-erpx.jesadakorn.com","requireEmailVerification":true,
            "passwordWorkers":2,"maximumRequests":16,"requestTimeoutMs":5000,"control":control,
            "holdings":[
                {"holdingId":"h001","slug":"acme","host":"acme.jesadakorn.com","writeBinding":"HOLDING_H001_DB","readBinding":"HOLDING_H001_READ_DB","filesBinding":"HOLDING_H001_FILES","database":h001},
                {"holdingId":"h002","slug":"northstar","host":"northstar.jesadakorn.com","writeBinding":"HOLDING_H002_DB","readBinding":"HOLDING_H002_READ_DB","filesBinding":"HOLDING_H002_FILES","database":h002}
            ],"financeJournalReads":[{"holdingId":"h001","companyId":"company"}],"fieldServiceReads":[{"holdingId":"h001","companyId":"company"}],"fieldServiceReservations":["h001"],"fieldServiceIssues":["h001"],"accountBootstrap":true
        });
        if work_scenario { doc["fieldServiceWorkReads"] = serde_json::json!(["h001"]); }
        if detail_scenario { doc["fieldServiceDetailReads"] = serde_json::json!(["h001"]); }
        if workspace_scenario || detail_scenario { doc["fieldServiceWorkspaceReads"] = serde_json::json!(["h001"]); }
        if invoice_read_scenario { doc["financeFieldServiceInvoiceReads"] = serde_json::json!(["h001"]); }
        if billing_scenario && !invoice_read_scenario { doc["fieldServiceBillingReleases"] = serde_json::json!(["h001"]); }
        if agreement_create_scenario || workspace_scenario { doc["fieldServiceAgreementCreates"] = serde_json::json!(["h001"]); }
        if agreement_review_scenario { doc["fieldServiceAgreementReviews"] = serde_json::json!(["h001"]); }
        if cancel_scenario { doc["fieldServiceOrderCancellations"] = serde_json::json!(["h001"]); }
        if recovery_scenario || work_scenario { doc["fieldServiceVisitRecoveries"] = serde_json::json!(["h001"]); }
        if completion_scenario { doc["fieldServiceVisitCompletions"] = serde_json::json!(["h001"]); }
        if visit_scenario { doc["fieldServiceVisits"] = serde_json::json!(["h001"]); }
        if order_create_scenario { doc["fieldServiceOrderCreates"] = serde_json::json!(["h001"]); }
        if request_scenario { doc["fieldServiceRequests"] = serde_json::json!(["h001"]); }
        if post_scenario {
            doc["financeJournalPosts"] = serde_json::json!(["h001"]);
            doc["financeFieldServiceJournalPosts"] = serde_json::json!(["h001"]);
        }
        let configuration = DevelopmentApiConfiguration::parse(&doc.to_string()).expect("invalid explicit native case configuration");
        let manifest = serde_json::json!({"version":1,"databases":{
            "CONTROL_DB":doc["control"]["url"], "CONTROL_READ_DB":doc["control"]["url"],
            "HOLDING_H001_DB":doc["holdings"][0]["database"]["url"],"HOLDING_H001_READ_DB":doc["holdings"][0]["database"]["url"],
            "HOLDING_H002_DB":doc["holdings"][1]["database"]["url"],"HOLDING_H002_READ_DB":doc["holdings"][1]["database"]["url"]
        },"slots":[
            {"id":"h001","write":"HOLDING_H001_DB","read":"HOLDING_H001_READ_DB","files":"HOLDING_H001_FILES"},
            {"id":"h002","write":"HOLDING_H002_DB","read":"HOLDING_H002_READ_DB","files":"HOLDING_H002_FILES"}
        ]});
        let mut files = CaseFiles(Vec::new());
        files.0.push(write_private(&dir, "native-api.json", &doc));
        files.0.push(write_private(&dir, "databases.json", &manifest));
        let mut credentials = serde_json::json!({"email":if agreement_review_scenario || billing_scenario {"b@example.test"} else {"a@example.test"},"password":field_service::account::fixture::PASSWORD,"orderId":if detail_scenario {"issue-order"} else {"reserve-order"},"reservationId":if detail_scenario {"reserved-part"} else {"field-part-browser-issue-reserve"},"expectedVersion":if detail_scenario {4} else if visit_scenario || workspace_scenario {1} else if cancel_scenario || billing_scenario {5} else if completion_scenario || recovery_scenario {4} else {3},"initialStatus":if visit_scenario || workspace_scenario {"scheduled"} else if billing_scenario {"completed"} else if cancel_scenario {"exception"} else {"in_visit"},"entryDate":"2026-01-10","financeEmail":"b@example.test","financePassword":field_service::account::fixture::PASSWORD,"journalId":journal_id,"journalVersion":if completion_scenario || recovery_scenario || cancel_scenario || billing_scenario {2} else {1},"agreementId":if agreement_review_scenario {"review-agreement"} else {"agreement"},"agreementVersion":if agreement_review_scenario {1} else {2},"agreementTitle":"Synthetic Agreement for activation","rejectAgreementId":"review-reject-agreement","rejectAgreementTitle":"Synthetic Agreement for rejection","rejectAgreementVersion":1,"creatorId":"fixture-a","technicianId":"fixture-a","assignmentId":if agreement_review_scenario {"agreement-reviewer-b"} else if billing_scenario {"field-billing-reviewer"} else {"field-fixture-a"},"assignmentVersion":3,"ticketId":"existing-ticket","ticketVersion":3});
        if agreement_create_scenario || workspace_scenario { credentials["agreementCreate"] = serde_json::json!({"branchId":"branch","branchName":"Synthetic North","customerId":"customer","customerName":"PRIVATE_CUSTOMER","currency":"THB","title":"Synthetic Agreement draft","startDate":"2026-01-10","endDate":"2027-12-31","visitRate":"123.45","visitRateMinor":12345,"warrantyDiscountPercent":"5","warrantyDiscountBps":500,"usdUnits":"6173","fxRateId":"rate","usedLatestFallback":false}); }
        if billing_scenario {
            let pricing_date:String=sqlx::query_scalar("SELECT to_char(to_timestamp($1::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD')").bind(now).fetch_one(system.holdings[0].raw()).await.expect("billing pricing date unavailable");
            credentials["billingRelease"] = serde_json::json!({"agreementId":"agreement","customerId":"customer","currency":"THB","amountMinor":10000,"warrantyApplied":false,"warrantyDiscountBps":0,"warrantyEndSnapshot":null,"pricingDate":pricing_date});
        }
        if invoice_read_scenario {
            let money:(String,String,Option<String>,bool)=sqlx::query_as("SELECT amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback FROM money_transaction_snapshots WHERE company_id='company' AND source_type='field_service_billing_release' AND source_id='field-billing-browser-invoice-release' AND amount_kind='invoice_total'").fetch_one(system.holdings[0].raw()).await.expect("invoice source Money unavailable");
            credentials["financeAssignmentId"]=serde_json::json!("finance-fixture-b");credentials["financeAssignmentVersion"]=serde_json::json!(3);
            credentials["invoiceRead"]=serde_json::json!({"invoiceId":"field-invoice-browser-invoice-release","invoiceNumber":"FS-reserve-order","currency":"THB","status":"draft","totalMinor":10000,"outstandingMinor":10000,"source":{"releaseId":"field-billing-browser-invoice-release","orderId":"reserve-order","commandKey":"browser-invoice-release","actorId":"fixture-b","currency":"THB","amountUnits":money.0,"usdUnits":money.1,"fxRateId":money.2,"usedLatestFallback":money.3}});
        }
        if detail_scenario {credentials["detailRead"]=serde_json::json!({"orderTitle":"Synthetic retained intake","status":"in_visit","reservedMilli":3000,"consumedMilli":1000,"releasedMilli":0,"sku":"SYNTHETIC","productName":"Synthetic spare","existingJournalStatus":"draft"});}
        files.0.push(write_private(&dir, "credentials.json", &credentials));
        let runtime = assemble_development_api(configuration, Arc::new(SystemAccountClock), Arc::new(SystemAuthenticationSecrets)).await.expect("native case assembly failed");
        let listener = tokio::net::TcpListener::bind(runtime.listen_address()).await.expect("private case listener unavailable");
        println!("{}", dir.join("databases.json").display());
        println!("READY");
        let (stop, stopped) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || { let _ = std::io::stdin().read(&mut [0u8; 1]); let _ = stop.send(()); });
        let server = axum::serve(listener, runtime.router()).with_graceful_shutdown(async move {
            tokio::select! { _ = stopped => (), _ = tokio::signal::ctrl_c() => () }
        });
        let result = server.await;
        runtime.close().await;
        drop(files);
        result.expect("private case listener failed");
    }).await;
}
