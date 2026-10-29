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
        "usage: native-field-service-development ABSOLUTE_PRIVATE_CASE_DIR LOOPBACK_LISTEN [post|request|order-create|visit|complete|recovery|cancel|agreement-review|agreement-create]"
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
            "agreement-create"
        ]
        .contains(&scenario),
        "invalid private fixture scenario"
    );
    let agreement_review_scenario = scenario == "agreement-review";
    let agreement_create_scenario = scenario == "agreement-create";
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
        let prepared = field_service::setup(system).await;
        let system = &prepared.review.system;
        sqlx::raw_sql("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(system.holdings[0].raw()).await.expect("synthetic field service source unavailable");
        // A genuine reserve command creates the immutable Service/Workflow provenance.
        // The visit is synthetic historical data, not an implemented transition.
        if agreement_review_scenario { field_service::setup_agreement_review_facts(&prepared, now).await; }
        if !visit_scenario && !agreement_review_scenario && !agreement_create_scenario {
        let reserve = format!(r#"{{"action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":{},"expiresAt":{},"reason":"Synthetic issue reservation"}}"#, if completion_scenario || recovery_scenario || cancel_scenario {2000} else {1000}, now + 3_600_000);
        prepared.execute("reserve-order", "browser-issue-reserve", &reserve).await.expect("synthetic reserve command failed");
        let mut historical = system.holdings[0].raw().begin().await.expect("historical visit transaction unavailable");
        sqlx::query("UPDATE field_service_orders SET status='in_visit',version=3 WHERE id='reserve-order'").execute(&mut *historical).await.expect("synthetic historical visit unavailable");
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES('synthetic-start-issue','reserve-order',3,'start','fixture-a','Synthetic historical visit','in_visit','fixture-a',$1+60000,'{}',$1)").bind(now).execute(&mut *historical).await.expect("synthetic historical event unavailable");
        historical.commit().await.expect("synthetic historical visit commit failed");
        }
        let journal_id = if post_scenario || completion_scenario || recovery_scenario || cancel_scenario {
            let issue = r#"{"action":"issue-part","expectedVersion":3,"reservationId":"field-part-browser-issue-reserve","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Synthetic post source"}"#;
            prepared.execute("reserve-order", "browser-post-issue", issue).await.expect("synthetic issue command failed");
            let id: String = sqlx::query_scalar("SELECT id FROM finance_journals WHERE company_id='company' AND source_id='operational-part-browser-post-issue'").fetch_one(system.holdings[0].raw()).await.expect("generated cost journal unavailable");
        // Independent Finance staff is backed by current HR and a published
        // scoped IAM role; Workflow selects its actual primary during posting.
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','journal_flow_reviewer','Journal workflow reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','journal_flow_reviewer',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','fixture',0);UPDATE assignments SET role='journal_flow_reviewer' WHERE id='finance-fixture-b';INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('person-reviewer','fixture-b','company','Reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('reviewer-engagement','person-reviewer','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0)").execute(system.holdings[0].raw()).await.expect("journal reviewer fixture unavailable");
            if completion_scenario || recovery_scenario || cancel_scenario {
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
                let dispatcher=Arc::new(erpx_service::field_service_parts::VisitDispatcher{clock:system.scenario.clock.clone(),allow_visits:false,allow_completion:false,allow_recovery:true,allow_cancel:false});
                let router=erpx_service::field_service_http::field_service_http_router(Arc::new(field_service::account::fixture::account::policy()),field_service::account::fixture::account::holding_registry(),Arc::new(erpx_service::field_service_reservation_read_http::FieldServiceReadScopeHeaders),vec![erpx_service::field_service_http::FieldServiceHttpTarget::new_private_creates("acme.example.test".into(),"acme".into(),service,None,None,(Some(dispatcher),None,None),(false,false))]).expect("recovery router unavailable");
                let request=axum::http::Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).header("idempotency-key","browser-cancel-exception").header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").body(axum::body::Body::from(r#"{"id":"reserve-order","action":"exception","expectedVersion":4,"reason":"Synthetic access interruption"}"#)).unwrap();
                let response=router.oneshot(request).await.expect("real recovery request failed");assert_eq!(response.status(),axum::http::StatusCode::OK,"real recovery prerequisite rejected");
            }
            Some(id)
        } else { None };
        if order_create_scenario {
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
        if agreement_create_scenario { doc["fieldServiceAgreementCreates"] = serde_json::json!(["h001"]); }
        if agreement_review_scenario { doc["fieldServiceAgreementReviews"] = serde_json::json!(["h001"]); }
        if cancel_scenario { doc["fieldServiceOrderCancellations"] = serde_json::json!(["h001"]); }
        if recovery_scenario { doc["fieldServiceVisitRecoveries"] = serde_json::json!(["h001"]); }
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
        let mut credentials = serde_json::json!({"email":if agreement_review_scenario {"b@example.test"} else {"a@example.test"},"password":field_service::account::fixture::PASSWORD,"orderId":"reserve-order","reservationId":"field-part-browser-issue-reserve","expectedVersion":if visit_scenario {1} else if cancel_scenario {5} else if completion_scenario || recovery_scenario {4} else {3},"initialStatus":if visit_scenario {"scheduled"} else if cancel_scenario {"exception"} else {"in_visit"},"entryDate":"2026-01-10","financeEmail":"b@example.test","financePassword":field_service::account::fixture::PASSWORD,"journalId":journal_id,"journalVersion":if completion_scenario || recovery_scenario || cancel_scenario {2} else {1},"agreementId":if agreement_review_scenario {"review-agreement"} else {"agreement"},"agreementVersion":if agreement_review_scenario {1} else {2},"agreementTitle":"Synthetic Agreement for activation","rejectAgreementId":"review-reject-agreement","rejectAgreementTitle":"Synthetic Agreement for rejection","rejectAgreementVersion":1,"creatorId":"fixture-a","technicianId":"fixture-a","assignmentId":if agreement_review_scenario {"agreement-reviewer-b"} else {"field-fixture-a"},"assignmentVersion":3,"ticketId":"existing-ticket","ticketVersion":3});
        if agreement_create_scenario { credentials["agreementCreate"] = serde_json::json!({"branchId":"branch","branchName":"Synthetic North","customerId":"customer","customerName":"PRIVATE_CUSTOMER","currency":"THB","title":"Synthetic Agreement draft","startDate":"2026-01-10","endDate":"2027-12-31","visitRate":"123.45","visitRateMinor":12345,"warrantyDiscountPercent":"5","warrantyDiscountBps":500,"usdUnits":"6173","fxRateId":"rate","usedLatestFallback":false}); }
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
