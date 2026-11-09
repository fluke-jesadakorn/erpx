//! Native generic Records create through the Workflow command ledger on a
//! disposable Holding database with current Account/IAM authority.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod review;

use erpx_server_records::public::record_write::{CreateRecordInput, RecordFieldInput};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, CompanyId, HoldingId};
use erpx_server_workflow::public::commands::CommandJson;
use erpx_service::records::RecordsConfiguration;
use erpx_service::records_command::RecordsCommandService;
use serde_json::json;
use std::sync::Arc;

#[derive(Clone)]
struct TestClock(Arc<review::account::fixture::MutableClock>);

impl Clock for TestClock {
    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        self.0.now()
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn input(title: &str) -> CreateRecordInput {
    CreateRecordInput {
        section_id: "tickets".into(),
        title: title.into(),
        description: "  Native support record  ".into(),
        fields: vec![RecordFieldInput {
            key: "priority".into(),
            label: "Priority".into(),
            field_type: "text".into(),
            value: json!("high"),
            options: None,
        }],
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn records_setup(
    system: review::account::System,
) -> (review::Prepared, RecordsCommandService<TestClock>) {
    let p = review::setup_without_finance_references(system).await;
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    let expires = now + 3_600_000;
    sqlx::query("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','custom_records_writer','Synthetic Records writer',false,1,'active') ON CONFLICT DO NOTHING").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_records_writer',1,'[\"records.read\",\"records.write\",\"module.support.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0) ON CONFLICT DO NOTHING").execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE assignments SET role='custom_records_writer',modules_json='[\"support\"]',valid_until=$1 WHERE user_id='fixture-a' AND company_id='company'").bind(expires).execute(pool.raw()).await.unwrap();
    let clock = Arc::new(TestClock(p.system.scenario.clock.clone()));
    let service = RecordsCommandService::new(
        p.authorizer.clone(),
        RecordsConfiguration::new(
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
        ),
        clock,
    );
    (p, service)
}

#[tokio::test]
async fn creates_one_record_with_receipt_audit_outbox_and_replay() {
    review::account::with_system(|system| async move {
        let (p, service) = records_setup(system).await;
        let pool = &p.system.holdings[0];
        let create = input("  Native support record  ");
        let result = service
            .create(
                "acme.example.test",
                p.maker.clone(),
                { let selection = p.maker_selection.clone(); move || Ok(selection) },
                "support".into(),
                create.clone(),
                key("records:create-1"),
            )
            .await
            .unwrap();
        let id = format!("rec-support-records:create-1");
        assert_eq!(result.as_str(), CommandJson::from_serializable(&json!({"id": id})).unwrap().as_str());
        let row: (String, String, String, String, String, String, i64, String) = sqlx::query_as(
            "SELECT section_id,title,description,fields_json,company_id,created_by,version,status \
             FROM workspace_records WHERE module_id='support' AND id=$1",
        )
        .bind(&id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.0, "tickets");
        assert_eq!(row.1, "Native support record");
        assert_eq!(row.2, "Native support record");
        assert_eq!(row.3, json!([{"key":"priority","label":"Priority","type":"text","value":"high"}]).to_string());
        assert_eq!(row.4, "company");
        assert_eq!(row.5, "fixture-a");
        assert_eq!(row.6, 1);
        assert_eq!(row.7, "draft");
        let receipt: (String, String) = sqlx::query_as(
            "SELECT operation,result_json FROM command_receipts WHERE command_id='records:create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt.0, "record.create");
        assert_eq!(receipt.1, json!({"id":id}).to_string());
        let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE command_id='records:create-1' AND record_type='record_create' AND record_id=$1").bind(&id).fetch_one(pool.raw()).await.unwrap();
        let outbox: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE command_id='records:create-1' AND event_type='record.create' AND aggregate_id=$1").bind(&id).fetch_one(pool.raw()).await.unwrap();
        assert_eq!(audit, 1);
        assert_eq!(outbox, 1);
        // Exact replay returns the same receipt and writes no second row/effect.
        let replay = service
            .create(
                "acme.example.test",
                p.maker.clone(),
                { let selection = p.maker_selection.clone(); move || Ok(selection) },
                "support".into(),
                create,
                key("records:create-1"),
            )
            .await
            .unwrap();
        assert_eq!(replay.as_str(), result.as_str());
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_records WHERE module_id='support' AND id=$1").bind(&id).fetch_one(pool.raw()).await.unwrap();
        assert_eq!(rows, 1);
        let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE command_id='records:create-1'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(audits, 1);
    })
    .await;
}

#[tokio::test]
async fn rejects_changed_replay_unauthorized_scope_and_invalid_module() {
    review::account::with_system(|system| async move {
        let (p, service) = records_setup(system).await;
        let pool = &p.system.holdings[0];
        let first = service
            .create(
                "acme.example.test",
                p.maker.clone(),
                { let selection = p.maker_selection.clone(); move || Ok(selection) },
                "support".into(),
                input("First native record"),
                key("records:create-2"),
            )
            .await
            .unwrap();
        assert!(first.as_str().contains("rec-support-records:create-2"));
        // A changed payload under the same key conflicts before a second row.
        let conflict = service
            .create(
                "acme.example.test",
                p.maker.clone(),
                { let selection = p.maker_selection.clone(); move || Ok(selection) },
                "support".into(),
                input("Changed native record"),
                key("records:create-2"),
            )
            .await
            .unwrap_err();
        assert_eq!(business(conflict).code, "IDEMPOTENCY_CONFLICT");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM workspace_records WHERE module_id='support' AND id='rec-support-records:create-2'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(count, 1);
        // The reviewer fixture has no records.write permission and no support module.
        let denied = service
            .create(
                "acme.example.test",
                p.reviewer.clone(),
                { let selection = p.reviewer_selection.clone(); move || Ok(selection) },
                "support".into(),
                input("Denied native record"),
                key("records:create-3"),
            )
            .await
            .unwrap_err();
        assert_eq!(business(denied).kind, erpx_server_shared::public::FailureKind::Forbidden);
        // Only the bounded support module is accepted by this first slice.
        let unsupported = service
            .create(
                "acme.example.test",
                p.maker.clone(),
                { let selection = p.maker_selection.clone(); move || Ok(selection) },
                "crm".into(),
                input("Unsupported module"),
                key("records:create-4"),
            )
            .await
            .unwrap_err();
        assert_eq!(business(unsupported).code, "RECORD_MODULE_UNSUPPORTED");
        let invalid_section = service
            .create(
                "acme.example.test",
                p.maker.clone(),
                { let selection = p.maker_selection.clone(); move || Ok(selection) },
                "support".into(),
                CreateRecordInput {
                    section_id: "not-a-section".into(),
                    title: "Invalid section".into(),
                    description: String::new(),
                    fields: vec![RecordFieldInput {
                        key: "priority".into(),
                        label: "Priority".into(),
                        field_type: "text".into(),
                        value: json!("high"),
                        options: None,
                    }],
                },
                key("records:create-5"),
            )
            .await
            .unwrap_err();
        assert_eq!(business(invalid_section).code, "INVALID_SECTION");
    })
    .await;
}
