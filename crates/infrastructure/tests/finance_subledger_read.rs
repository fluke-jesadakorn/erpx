//! Disposable two-database proof for the Finance AP/AR read boundary.

use chrono::NaiveDate;
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::finance_subledgers::{
    Invoice, InvoiceKind, InvoiceLine, PartyKind, PartyReference,
};
use erpx_infrastructure::{
    FinanceSubledgerAggregate, FinanceSubledgerReadCursor, SqlxFinanceSubledgerStore,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "finance-subledger-read-test".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "finance.subledger.read.test",
    )
    .expect("context")
}

fn invoice(
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Uuid,
    period_id: Uuid,
    id: Uuid,
    key: &str,
) -> Invoice {
    let scope = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let party_scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let party = PartyReference::new(
        PartyKind::Vendor,
        Uuid::new_v4(),
        party_scope,
        holding_id,
        company_id,
    )
    .expect("party");
    Invoice::new(
        InvoiceKind::Payable,
        id,
        scope.clone(),
        holding_id,
        company_id,
        period_id,
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
        NaiveDate::from_ymd_opt(2026, 7, 10).expect("date"),
        NaiveDate::from_ymd_opt(2026, 7, 31).expect("date"),
        party,
        "THB",
        vec![InvoiceLine {
            line_id: Uuid::new_v4(),
            description: "read-side fixture".into(),
            quantity: 1,
            unit_price_minor: 100,
            tax_minor: 0,
        }],
        &context(scope, key),
    )
    .expect("invoice")
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_finance_subledger_read_is_projected_scoped_paginated_and_rls_forced() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding-write migrations");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding-read migrations");

    let data_profile_key = format!("holding-finance-subledger-read-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let first_company = Uuid::new_v4();
    let first_branch = Uuid::new_v4();
    let second_company = Uuid::new_v4();
    let second_branch = Uuid::new_v4();
    let first = invoice(
        holding_id,
        first_company,
        first_branch,
        Uuid::new_v4(),
        Uuid::new_v4(),
        "subledger-read-first",
    );
    let second = invoice(
        holding_id,
        second_company,
        second_branch,
        Uuid::new_v4(),
        Uuid::new_v4(),
        "subledger-read-second",
    );
    let source =
        SqlxFinanceSubledgerStore::new(write_pool.clone(), read_pool.clone(), &data_profile_key)
            .expect("source store");
    source
        .save(&FinanceSubledgerAggregate::Invoice(first), None)
        .await
        .expect("first invoice");
    source
        .save(&FinanceSubledgerAggregate::Invoice(second), None)
        .await
        .expect("second invoice");

    let projection =
        SqlxFinanceSubledgerStore::new(write_pool.clone(), read_pool.clone(), &data_profile_key)
            .expect("projection store");
    let source_page = projection
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(source_page.events.len(), 2);
    assert_eq!(
        projection
            .source_event_high_watermark()
            .await
            .expect("source watermark"),
        2
    );
    for event in &source_page.events {
        assert!(
            projection
                .project_event(
                    event.event_id,
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await
                .expect("project event")
                .applied
        );
    }

    let first_event = &source_page.events[0];
    let exact_replay = projection
        .project_event(
            first_event.event_id,
            &first_event.aggregate_type,
            first_event.aggregate_id,
            first_event.aggregate_version,
            &first_event.payload,
        )
        .await
        .expect("exact Finance subledger event replay");
    assert!(!exact_replay.applied);

    let conflicting_event = projection
        .project_event(
            Uuid::new_v4(),
            &first_event.aggregate_type,
            first_event.aggregate_id,
            first_event.aggregate_version,
            &first_event.payload,
        )
        .await;
    assert!(matches!(
        conflicting_event,
        Err(erpx_infrastructure::PersistenceError::InvalidData(message))
            if message == "Finance projection event is not present in the authoritative outbox"
    ));

    let mut conflicting_payload = first_event.payload.clone();
    conflicting_payload["aggregate"]["lines"][0]["description"] =
        serde_json::Value::String("tampered Finance projection payload".into());
    let conflicting_payload = projection
        .project_event(
            first_event.event_id,
            &first_event.aggregate_type,
            first_event.aggregate_id,
            first_event.aggregate_version,
            &conflicting_payload,
        )
        .await;
    assert!(matches!(
        conflicting_payload,
        Err(erpx_infrastructure::PersistenceError::InvalidData(message))
            if message == "Finance projection event does not match its authoritative outbox row"
    ));

    let mut inspection = read_pool.begin().await.expect("checkpoint inspection");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *inspection)
        .await
        .expect("checkpoint inspection scope");
    let checkpoint: (Uuid, i64, Value) = sqlx::query_as(
        "SELECT c.last_event_id, c.source_version, r.payload
         FROM erpx_holding.finance_subledger_projection_checkpoints c
         JOIN erpx_holding.finance_subledger_read_rows r
           ON r.data_profile_key = c.data_profile_key
          AND r.aggregate_type = c.aggregate_type
          AND r.aggregate_id = c.aggregate_id
         WHERE c.data_profile_key=$1 AND c.aggregate_type=$2 AND c.aggregate_id=$3",
    )
    .bind(&data_profile_key)
    .bind(&first_event.aggregate_type)
    .bind(first_event.aggregate_id)
    .fetch_one(&mut *inspection)
    .await
    .expect("unchanged Finance subledger checkpoint");
    inspection
        .commit()
        .await
        .expect("checkpoint inspection commit");
    assert_eq!(checkpoint.0, first_event.event_id);
    assert_eq!(
        checkpoint.1,
        i64::try_from(first_event.aggregate_version).unwrap()
    );
    assert_eq!(checkpoint.2, first_event.payload["aggregate"]);

    let holding_scope = AuthorityScope::Holding { holding_id };
    let first_scope = AuthorityScope::Company {
        holding_id,
        company_id: first_company,
    };
    let first_branch_scope = AuthorityScope::Branch {
        holding_id,
        company_id: first_company,
        branch_id: first_branch,
    };
    let page = projection
        .list_read_rows(&holding_scope, Some("payable_invoice"), None, None, 1)
        .await
        .expect("first page");
    assert_eq!(page.rows.len(), 1);
    assert!(page.has_more);
    let cursor = page.next_cursor.expect("cursor");
    assert_eq!(
        cursor,
        FinanceSubledgerReadCursor::new("payable_invoice", page.rows[0].aggregate_id)
            .expect("cursor")
    );
    let second_page = projection
        .list_read_rows(
            &holding_scope,
            Some("payable_invoice"),
            None,
            Some(&cursor),
            1,
        )
        .await
        .expect("second page");
    assert_eq!(second_page.rows.len(), 1);
    assert!(!second_page.has_more);
    assert_ne!(page.rows[0].aggregate_id, second_page.rows[0].aggregate_id);

    let company_page = projection
        .list_read_rows(&first_scope, None, Some(first_company), None, 10)
        .await
        .expect("company page");
    assert_eq!(company_page.rows.len(), 1);
    assert_eq!(company_page.rows[0].company_id, first_company);
    let branch_page = projection
        .list_read_rows(&first_branch_scope, None, None, None, 10)
        .await
        .expect("branch page");
    assert_eq!(branch_page.rows.len(), 1);
    assert_eq!(branch_page.rows[0].branch_id, Some(first_branch));
    assert!(
        projection
            .list_read_rows(&first_scope, None, Some(second_company), None, 10)
            .await
            .is_err()
    );

    let (_, events_replayed) = projection
        .rebuild_projection_with_source_fence(None, 10)
        .await
        .expect("atomic Finance subledger rebuild");
    assert!(events_replayed >= 2);

    let forced: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class
         WHERE oid = 'erpx_holding.finance_subledger_read_rows'::regclass",
    )
    .fetch_one(&read_pool)
    .await
    .expect("forced RLS metadata");
    assert!(forced);
}
