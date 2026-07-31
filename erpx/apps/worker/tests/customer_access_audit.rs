use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use erpx_application::{
    Customer360DetailReadStore, CustomerGlobalReadAuditDetailQuery,
    CustomerGlobalReadAuditDetailService, CustomerGlobalReadAuditQueryContext, CustomerReadView,
    CustomerSecurityAdminScope,
};
use erpx_domain::{
    commercial::BillingState,
    customer::{
        Customer360, CustomerAccount, CustomerHealth, CustomerStatus, GlobalReadActor,
        GlobalReadAuditRecord, GlobalReadScope, HealthState,
    },
};
use erpx_infrastructure::{ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use erpx_worker::CustomerAccessAuditSummaryProjection;
use uuid::Uuid;

fn config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    }
}

fn customer(id: Uuid) -> Customer360 {
    Customer360 {
        account: CustomerAccount {
            id,
            account_code: format!("AUDIT-{}", &id.simple().to_string()[..8]),
            trading_name: "Audit Projection Customer".into(),
            legal_name: Some("Audit Projection Customer Co., Ltd.".into()),
            industry: Some("test".into()),
            country: "TH".into(),
            status: CustomerStatus::Active,
            billing_state: BillingState::Active,
            renewal_at: None,
            lifetime_value_minor: 0,
        },
        health: CustomerHealth {
            customer_account_id: id,
            state: HealthState::Healthy,
            projection_lag_seconds: 0,
            worker_failures: 0,
            backup_status: "verified".into(),
            last_activity_at: None,
        },
        holding_count: 0,
        company_count: 0,
        branch_count: 0,
        active_user_count: 0,
        expiring_role_count: 0,
        open_ticket_count: 0,
        module_count: 0,
    }
}

fn audit(
    customer_id: Uuid,
    occurred_at: DateTime<Utc>,
    correlation_id: Uuid,
) -> GlobalReadAuditRecord {
    GlobalReadAuditRecord::new(
        GlobalReadActor {
            principal_ref: "platform-agent".into(),
            session_ref: Some("platform-session".into()),
        },
        customer_id,
        "customer_360",
        GlobalReadScope {
            label: "platform".into(),
            holding_id: None,
            company_id: None,
            branch_id: None,
        },
        "customer_support_review",
        occurred_at,
        correlation_id,
    )
    .expect("valid global read audit")
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn access_audit_summary_rebuild_is_ordered_idempotent_and_read_side_only() {
    let database_config = config();
    assert_ne!(database_config.write_url, database_config.read_url);
    let pools = ControlPlanePools::connect(&database_config)
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let store = Arc::new(SqlxControlPlane::new(pools));
    let customer_id = Uuid::from_u128(0x9101);
    let unknown_customer_id = Uuid::from_u128(0x9102);
    store
        .upsert_customer_read_model(&customer(customer_id))
        .await
        .expect("customer read projection");

    let first_at = Utc::now() - Duration::minutes(2);
    let second_at = first_at + Duration::minutes(1);
    store
        .record_global_read(&audit(customer_id, first_at, Uuid::from_u128(0xA001)))
        .await
        .expect("first authoritative audit");
    store
        .record_global_read(&audit(customer_id, second_at, Uuid::from_u128(0xA002)))
        .await
        .expect("second authoritative audit");
    store
        .record_global_read(&audit(
            unknown_customer_id,
            second_at,
            Uuid::from_u128(0xA003),
        ))
        .await
        .expect("orphan authoritative audit");

    let projection = CustomerAccessAuditSummaryProjection::new(store.clone());
    let first_rebuild = projection.rebuild().await.expect("first audit rebuild");
    assert_eq!(first_rebuild.summaries_written, 1);
    let first_updated_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT updated_at FROM erpx_control.customer_access_audit_summaries WHERE customer_id = $1",
    )
    .bind(customer_id)
    .fetch_one(&store.pools().read)
    .await
    .expect("summary updated timestamp");

    let summary = store
        .get_customer_access_audit_summary(customer_id)
        .await
        .expect("summary read");
    let CustomerReadView::Present {
        data: summary,
        metadata,
    } = summary
    else {
        panic!("audit summary should be present");
    };
    assert_eq!(summary.read_count, 2);
    assert_eq!(summary.last_access_at, Some(second_at));
    assert!(summary.customer_visible);
    assert!(metadata.read_only);

    let detail_service = CustomerGlobalReadAuditDetailService::new(store.clone(), store.clone());
    let detail_query = CustomerGlobalReadAuditDetailQuery {
        customer_id,
        security_admin_scope: CustomerSecurityAdminScope {
            customer_id,
            platform: true,
            holding_id: None,
            company_id: None,
            branch_id: None,
        },
        audit: CustomerGlobalReadAuditQueryContext {
            requester: GlobalReadActor {
                principal_ref: "customer-security-admin".into(),
                session_ref: Some("security-session".into()),
            },
            purpose: "customer audit review".into(),
            correlation_id: Uuid::from_u128(0xA100),
        },
        before: None,
        limit: 1,
    };
    let detail_page = detail_service
        .list(detail_query)
        .await
        .expect("audited detail query");
    let CustomerReadView::Present {
        data: detail_page,
        metadata,
    } = detail_page
    else {
        panic!("audit detail page should be present");
    };
    assert_eq!(detail_page.entries.len(), 1);
    assert!(detail_page.next_before.is_some());
    assert_eq!(detail_page.entries[0].customer_id, customer_id);
    assert_eq!(detail_page.entries[0].actor.principal_ref, "platform-agent");
    assert!(metadata.read_only);
    let serialized_detail = serde_json::to_value(&detail_page.entries[0]).expect("detail JSON");
    assert!(serialized_detail.get("payload").is_none());

    let next_query = CustomerGlobalReadAuditDetailQuery {
        customer_id,
        security_admin_scope: CustomerSecurityAdminScope {
            customer_id,
            platform: true,
            holding_id: None,
            company_id: None,
            branch_id: None,
        },
        audit: CustomerGlobalReadAuditQueryContext {
            requester: GlobalReadActor {
                principal_ref: "customer-security-admin".into(),
                session_ref: Some("security-session".into()),
            },
            purpose: "customer audit review".into(),
            correlation_id: Uuid::from_u128(0xA101),
        },
        before: Some(detail_page.next_before.expect("next cursor")),
        limit: 1,
    };
    let next_page = detail_service
        .list(next_query)
        .await
        .expect("next audited detail page");
    let CustomerReadView::Present {
        data: next_page, ..
    } = next_page
    else {
        panic!("next audit detail page should be present");
    };
    assert_eq!(next_page.entries.len(), 1);
    assert!(next_page.next_before.is_none());

    store
        .upsert_customer_access_audit_summary_read_model(customer_id, 2, Some(second_at), true)
        .await
        .expect("idempotent summary upsert");
    let idempotent_updated_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT updated_at FROM erpx_control.customer_access_audit_summaries WHERE customer_id = $1",
    )
    .bind(customer_id)
    .fetch_one(&store.pools().read)
    .await
    .expect("idempotent updated timestamp");
    assert_eq!(idempotent_updated_at, first_updated_at);

    let second_rebuild = projection.rebuild().await.expect("second audit rebuild");
    assert_eq!(second_rebuild.summaries_written, 1);
    let row_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM erpx_control.customer_access_audit_summaries")
            .fetch_one(&store.pools().read)
            .await
            .expect("summary row count");
    assert_eq!(row_count, 1);

    projection.clear().await.expect("clear audit summary");
    let cleared = store
        .get_customer_access_audit_summary(customer_id)
        .await
        .expect("cleared summary read");
    let CustomerReadView::Present { data: cleared, .. } = cleared else {
        panic!("existing customer should retain an empty summary view");
    };
    assert_eq!(cleared.read_count, 0);
    assert!(cleared.customer_visible);

    let notes = store
        .get_customer_notes(customer_id)
        .await
        .expect("notes read");
    let CustomerReadView::Present { data: notes, .. } = notes else {
        panic!("notes read model should be present but empty");
    };
    assert!(
        notes.notes.is_empty(),
        "no authoritative note aggregate exists, so the audit projector must not fabricate notes"
    );
    let read_write_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('erpx_control.global_read_audits')::text")
            .fetch_one(&store.pools().read)
            .await
            .expect("read-side audit table isolation");
    assert_eq!(read_write_table, None);
}
