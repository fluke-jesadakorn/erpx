use std::sync::Arc;

use chrono::Utc;
use erpx_application::{AddCustomerNoteCommand, Customer360NoteService, CustomerReadStore};
use erpx_domain::{
    access::Scope,
    commercial::BillingState,
    cqrs::OutboxEvent,
    customer::{
        Customer360, Customer360Projection as DomainCustomer360Projection, CustomerAccount,
        CustomerHealth, CustomerNote, CustomerStatus, HealthState, OrganizationUnit,
    },
};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePools, EventMetadata, OutboxEnvelope, OutboxWorker,
    SqlxControlPlane,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use erpx_worker::{
    Customer360Projection as WorkerCustomer360Projection, CustomerAccessAuditSummaryProjection,
};
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
            account_code: "C360-001".into(),
            trading_name: "Customer 360 Test".into(),
            legal_name: Some("Customer 360 Test Co., Ltd.".into()),
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
        holding_count: 1,
        company_count: 1,
        branch_count: 1,
        active_user_count: 1,
        expiring_role_count: 0,
        open_ticket_count: 0,
        module_count: 1,
    }
}

async fn reset_disposable_customer_fixture(store: &SqlxControlPlane) {
    sqlx::query(
        "DELETE FROM erpx_control.outbox_events
         WHERE aggregate_type IN ('customer', 'customer_note')",
    )
    .execute(&store.pools().write)
    .await
    .expect("clear disposable customer outbox fixtures");
    sqlx::query(
        "DELETE FROM erpx_control.aggregate_snapshots
         WHERE aggregate_type IN ('customer', 'customer_note')",
    )
    .execute(&store.pools().write)
    .await
    .expect("clear disposable customer snapshots");
    sqlx::query("DELETE FROM erpx_control.global_read_audits")
        .execute(&store.pools().write)
        .await
        .expect("clear disposable customer access audits");
    sqlx::query(
        "DELETE FROM erpx_control.projection_event_receipts
         WHERE projection_name = 'customer_360'",
    )
    .execute(&store.pools().write)
    .await
    .expect("clear disposable customer projection receipts");
    sqlx::query(
        "DELETE FROM erpx_control.projection_checkpoints
         WHERE projection_name = 'customer_360'",
    )
    .execute(&store.pools().write)
    .await
    .expect("clear disposable customer projection checkpoints");
    sqlx::query("DELETE FROM erpx_control.customer_scope_bindings")
        .execute(&store.pools().write)
        .await
        .expect("clear disposable customer scope bindings");
    sqlx::query("DELETE FROM erpx_control.customers")
        .execute(&store.pools().read)
        .await
        .expect("clear disposable customer read rows");
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn customer_360_rebuild_projects_and_claims_outbox_events() {
    let pools = ControlPlanePools::connect(&config()).await.expect("pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("read migration");
    let store = Arc::new(SqlxControlPlane::new(pools));
    reset_disposable_customer_fixture(&store).await;
    let customer = customer(Uuid::new_v4());
    let holding_id = Uuid::from_u128(0x1001);
    let company_id = Uuid::from_u128(0x2001);
    let branch_id = Uuid::from_u128(0x4001);
    let foreign_holding_id = Uuid::from_u128(0x1002);
    let foreign_company_id = Uuid::from_u128(0x2002);
    let foreign_branch_id = Uuid::from_u128(0x4002);
    let mut customer_projection = DomainCustomer360Projection::from_legacy(&customer);
    customer_projection.organization.holdings = vec![OrganizationUnit {
        id: holding_id,
        kind: "holding".into(),
        active: true,
        ..Default::default()
    }];
    customer_projection.organization.companies = vec![OrganizationUnit {
        id: company_id,
        kind: "company".into(),
        parent_id: Some(holding_id),
        active: true,
        ..Default::default()
    }];
    // These descendants reference a holding that is not present in this
    // customer's authoritative organization projection. They must not become
    // customer scope authority merely because their IDs are well formed.
    customer_projection
        .organization
        .companies
        .push(OrganizationUnit {
            id: foreign_company_id,
            kind: "company".into(),
            parent_id: Some(foreign_holding_id),
            active: true,
            ..Default::default()
        });
    customer_projection.organization.branches = vec![OrganizationUnit {
        id: branch_id,
        kind: "branch".into(),
        parent_id: Some(company_id),
        active: true,
        ..Default::default()
    }];
    customer_projection
        .organization
        .branches
        .push(OrganizationUnit {
            id: foreign_branch_id,
            kind: "branch".into(),
            parent_id: Some(foreign_company_id),
            active: true,
            ..Default::default()
        });
    customer_projection.commercial.stripe_customer_ref = Some("cus_fixture".into());
    customer_projection.canonicalize();
    let event = OutboxEvent {
        id: Uuid::new_v4(),
        aggregate_type: "customer".into(),
        aggregate_id: customer.account.id,
        aggregate_version: 1,
        event_type: "customer.created".into(),
        payload: serde_json::to_value(&customer_projection).expect("customer event payload"),
        occurred_at: Utc::now(),
    };
    let mut transaction = store.begin_write().await.expect("write transaction");
    transaction
        .upsert_aggregate_snapshot("customer", customer.account.id, event.payload.clone(), None)
        .await
        .expect("aggregate snapshot");
    transaction
        .append_outbox_event(&event, &EventMetadata::for_event(&event))
        .await
        .expect("outbox event");
    transaction.commit().await.expect("commit");

    let worker = OutboxWorker::new(store.clone(), Uuid::new_v4());
    let projection = WorkerCustomer360Projection::new(store.clone());
    let rebuild = worker
        .rebuild_projection("customer_360", &projection)
        .await
        .expect("customer 360 rebuild");
    assert_eq!(rebuild.events_replayed, 1);
    let projected = store
        .get_customer(customer.account.id)
        .await
        .expect("read model query")
        .expect("projected customer");
    assert_eq!(projected.account.account_code, "C360-001");
    let projected_payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM erpx_control.customers WHERE id = $1")
            .bind(customer.account.id)
            .fetch_one(&store.pools().read)
            .await
            .expect("enriched read payload");
    let persisted_projection: DomainCustomer360Projection =
        serde_json::from_value(projected_payload.clone()).expect("enriched projection");
    assert_eq!(
        persisted_projection.scope_bindings(),
        customer_projection.scope_bindings()
    );
    let summary = serde_json::to_value(persisted_projection.summary()).expect("summary");
    assert!(summary.get("stripe_customer_ref").is_none());
    assert!(summary.get("tax_identifier").is_none());

    let binding_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
    )
    .bind(customer.account.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("binding count");
    assert_eq!(binding_count, 3);
    let foreign_binding_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.customer_scope_bindings
         WHERE customer_id = $1
           AND (holding_id = $2 OR company_id = $3 OR branch_id = $4)",
    )
    .bind(customer.account.id)
    .bind(foreign_holding_id)
    .bind(foreign_company_id)
    .bind(foreign_branch_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("foreign binding count");
    assert_eq!(
        foreign_binding_count, 0,
        "foreign-parent organization rows must not cross customer ownership"
    );
    assert!(
        store
            .customer_scope_contains(
                customer.account.id,
                &Scope::Branch {
                    holding_id,
                    company_id,
                    branch_id,
                },
            )
            .await
            .expect("branch binding")
    );
    assert!(
        !store
            .customer_scope_contains(
                customer.account.id,
                &Scope::Branch {
                    holding_id: Uuid::from_u128(0x1002),
                    company_id,
                    branch_id: Uuid::from_u128(0x4002),
                },
            )
            .await
            .expect("sibling binding denial")
    );
    let read_binding_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('erpx_control.customer_scope_bindings')::text")
            .fetch_one(&store.pools().read)
            .await
            .expect("read-side binding isolation");
    assert_eq!(read_binding_table, None);

    let second_rebuild = worker
        .rebuild_projection("customer_360", &projection)
        .await
        .expect("deterministic second rebuild");
    assert_eq!(second_rebuild.events_replayed, 1);
    let second_payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM erpx_control.customers WHERE id = $1")
            .bind(customer.account.id)
            .fetch_one(&store.pools().read)
            .await
            .expect("second enriched read payload");
    assert_eq!(projected_payload, second_payload);
    let binding_count_after_rebuild: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
    )
    .bind(customer.account.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("binding count after rebuild");
    assert_eq!(binding_count_after_rebuild, 3);

    let mut revoked_projection = customer_projection.clone();
    revoked_projection.organization = Default::default();
    revoked_projection.canonicalize();
    let revoked_event = OutboxEvent {
        id: Uuid::new_v4(),
        aggregate_type: "customer".into(),
        aggregate_id: customer.account.id,
        aggregate_version: 2,
        event_type: "customer.organization_scopes_revoked".into(),
        payload: serde_json::to_value(&revoked_projection).expect("revocation payload"),
        occurred_at: Utc::now(),
    };
    let mut transaction = store.begin_write().await.expect("revocation transaction");
    transaction
        .upsert_aggregate_snapshot(
            "customer",
            customer.account.id,
            revoked_event.payload.clone(),
            Some(1),
        )
        .await
        .expect("revocation aggregate snapshot");
    transaction
        .append_outbox_event(&revoked_event, &EventMetadata::for_event(&revoked_event))
        .await
        .expect("revocation outbox event");
    transaction.commit().await.expect("revocation commit");

    let revocation_rebuild = worker
        .rebuild_projection("customer_360", &projection)
        .await
        .expect("customer 360 revocation rebuild");
    assert_eq!(revocation_rebuild.events_replayed, 2);
    let binding_count_after_revocation: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
    )
    .bind(customer.account.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("binding count after revocation");
    assert_eq!(binding_count_after_revocation, 0);
    assert!(
        !store
            .customer_scope_contains(customer.account.id, &Scope::Holding { holding_id },)
            .await
            .expect("revoked holding binding")
    );

    // A malformed next event must release its projection receipt before any
    // read-model or authoritative binding mutation. This is the retry/rollback
    // fence for a bad customer event after a successful revocation.
    let payload_before_failure: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM erpx_control.customers WHERE id = $1")
            .bind(customer.account.id)
            .fetch_one(&store.pools().read)
            .await
            .expect("payload before malformed event");
    let malformed_event = OutboxEnvelope {
        id: Uuid::new_v4(),
        event_type: "customer.updated".into(),
        event_version: 1,
        aggregate_type: "customer".into(),
        aggregate_id: customer.account.id,
        aggregate_version: 3,
        tenant_scope: None,
        actor: None,
        correlation_id: Uuid::new_v4(),
        causation_id: None,
        idempotency_key: None,
        payload: serde_json::json!({"account": {"id": customer.account.id}}),
        occurred_at: Utc::now(),
        available_at: Utc::now(),
        attempts: 0,
        max_attempts: 10,
        claimed_by: None,
        claimed_at: None,
        published_at: None,
        dead_lettered_at: None,
        last_error: None,
        last_error_context: None,
    };
    let projection_error = worker
        .project_once("customer_360", &malformed_event, &projection)
        .await
        .expect_err("malformed customer event must fail closed");
    assert!(
        projection_error
            .to_string()
            .contains("invalid customer event")
    );

    let payload_after_failure: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM erpx_control.customers WHERE id = $1")
            .bind(customer.account.id)
            .fetch_one(&store.pools().read)
            .await
            .expect("payload after malformed event");
    assert_eq!(payload_after_failure, payload_before_failure);
    let checkpoint_version: i64 = sqlx::query_scalar(
        "SELECT last_aggregate_version
         FROM erpx_control.projection_checkpoints
         WHERE projection_name = 'customer_360'
           AND aggregate_type = 'customer' AND aggregate_id = $1",
    )
    .bind(customer.account.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("checkpoint after malformed event");
    assert_eq!(checkpoint_version, 2);
    let receipt_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.projection_event_receipts
         WHERE projection_name = 'customer_360' AND event_id = $1",
    )
    .bind(malformed_event.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("projection receipt after malformed event");
    assert_eq!(receipt_count, 0, "failed event receipt must be released");
    let binding_count_after_failure: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.customer_scope_bindings WHERE customer_id = $1",
    )
    .bind(customer.account.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("bindings after malformed event");
    assert_eq!(binding_count_after_failure, 0);

    let note = CustomerNote::new(
        Uuid::new_v4(),
        customer.account.id,
        "customer-success",
        Scope::Platform,
        Utc::now(),
        "Authorization: Bearer note-secret",
    )
    .expect("customer note");
    let note_service = Customer360NoteService::new(store.clone());
    let note_command = AddCustomerNoteCommand::new(
        Uuid::new_v4(),
        "customer-note-command-1",
        note.clone(),
        "customer_360_note",
        Uuid::new_v4(),
    )
    .expect("note command");
    let first_note = note_service
        .append(note_command.clone())
        .await
        .expect("append note");
    assert!(!first_note.replayed);
    let persisted_note_event: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.outbox_events
         WHERE event_type = 'customer.note_added' AND aggregate_id = $1",
    )
    .bind(note.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("note outbox event");
    assert_eq!(persisted_note_event, 1);
    let persisted_note_snapshot: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.aggregate_snapshots
         WHERE aggregate_type = 'customer_note' AND aggregate_id = $1",
    )
    .bind(note.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("note aggregate snapshot");
    assert_eq!(persisted_note_snapshot, 1);
    let persisted_note_audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.audit_events
         WHERE action = 'customer_note_added' AND customer_account_id = $1",
    )
    .bind(customer.account.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("note audit event");
    assert_eq!(persisted_note_audit, 1);
    let replayed_note = note_service
        .append(note_command)
        .await
        .expect("replay note");
    assert!(replayed_note.replayed);

    let note_rebuild = worker
        .rebuild_projection("customer_360", &projection)
        .await
        .expect("customer note rebuild");
    assert_eq!(note_rebuild.events_replayed, 3);
    let note_page = store
        .get_customer_notes_page(customer.account.id, None, 10)
        .await
        .expect("customer notes page");
    let erpx_domain::customer::CustomerReadView::Present {
        data: note_page, ..
    } = note_page
    else {
        panic!("customer notes page should be present");
    };
    assert_eq!(note_page.notes.len(), 1);
    assert_eq!(note_page.notes[0].body, "Authorization: [REDACTED]");

    let access_audit_projection = CustomerAccessAuditSummaryProjection::new(store.clone());
    access_audit_projection
        .rebuild()
        .await
        .expect("customer access-audit summary rebuild");

    let reconciliation = projection
        .reconcile()
        .await
        .expect("customer 360 reconciliation");
    assert!(
        reconciliation.healthy(),
        "unexpected G3 reconciliation: {reconciliation:?}"
    );

    let unbound_note = CustomerNote::new(
        Uuid::new_v4(),
        customer.account.id,
        "customer-success",
        Scope::Holding {
            holding_id: Uuid::from_u128(0x9999),
        },
        Utc::now(),
        "unbound note",
    )
    .expect("unbound note shape");
    let denied = note_service
        .append(
            AddCustomerNoteCommand::new(
                Uuid::new_v4(),
                "customer-note-command-denied",
                unbound_note,
                "customer_360_note",
                Uuid::new_v4(),
            )
            .expect("unbound note command"),
        )
        .await
        .expect_err("unbound note must fail closed");
    assert!(denied.to_string().contains("not bound"));
}
