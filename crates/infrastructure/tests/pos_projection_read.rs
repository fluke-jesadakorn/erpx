//! Disposable end-to-end evidence for the typed POS read gateway.

use chrono::{Duration, Utc};
use erpx_infrastructure::{MAX_POS_READ_CHILD_ROWS_PER_SALE, PosReadScope, SqlxPosProjectionStore};
use sqlx::PgPool;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires one disposable holding PostgreSQL read database"]
async fn live_pos_read_projection_is_scope_bound_bounded_and_cursor_stable() {
    let url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("read pool");
    erpx_migration::migrate_holding_read(&pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-pos-read-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let first_sale = Uuid::new_v4();
    let second_sale = Uuid::new_v4();
    let first_updated_at = Utc::now() - Duration::minutes(1);
    let second_updated_at = Utc::now();

    for (sale_id, updated_at, reference, event_id) in [
        (first_sale, first_updated_at, "POS-001", event_id),
        (second_sale, second_updated_at, "POS-002", Uuid::new_v4()),
    ] {
        sqlx::query(
            "INSERT INTO erpx_holding.pos_sale_read_rows
                (data_profile_key, sale_id, holding_id, company_id, reference, currency,
                 subtotal_minor, tax_minor, total_minor, paid_minor, status,
                 command_id, idempotency_key, created_at, updated_at,
                 source_version, source_event_id)
             VALUES ($1,$2,$3,$4,$5,'THB',100,7,107,107,'completed',
                     $6,$7,$8,$9,1,$10)",
        )
        .bind(&data_profile_key)
        .bind(sale_id)
        .bind(holding_id)
        .bind(company_id)
        .bind(reference)
        .bind(Uuid::new_v4())
        .bind(format!("idempotency-{reference}"))
        .bind(updated_at - Duration::minutes(1))
        .bind(updated_at)
        .bind(event_id)
        .execute(&pool)
        .await
        .expect("sale read row");
    }

    let line_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_holding.pos_sale_line_read_rows
            (data_profile_key, sale_id, line_id, line_no, item_reference, description,
             quantity, unit_price_minor, tax_minor, subtotal_minor, total_minor,
             source_version, source_event_id)
         VALUES ($1,$2,$3,0,'SKU-001','Anonymized item',1,100,7,100,107,1,$4)",
    )
    .bind(&data_profile_key)
    .bind(first_sale)
    .bind(line_id)
    .bind(event_id)
    .execute(&pool)
    .await
    .expect("sale line read row");
    sqlx::query(
        "INSERT INTO erpx_holding.pos_sale_payment_read_rows
            (data_profile_key, sale_id, payment_id, amount_minor, method, received_at,
             source_version, source_event_id)
         VALUES ($1,$2,$3,107,'cash',$4,1,$5)",
    )
    .bind(&data_profile_key)
    .bind(first_sale)
    .bind(Uuid::new_v4())
    .bind(first_updated_at)
    .bind(event_id)
    .execute(&pool)
    .await
    .expect("sale payment read row");

    let store = SqlxPosProjectionStore::from_pools(&pool, &pool, &data_profile_key)
        .expect("POS read store");
    let scope = PosReadScope {
        holding_id,
        company_id,
        branch_id: None,
    };
    let first_page = store.list_sales(scope, 1, None).await.expect("first page");
    assert_eq!(first_page.rows.len(), 1);
    assert!(first_page.has_more);
    assert_eq!(first_page.rows[0].sale_id, second_sale);
    assert!(first_page.rows[0].lines.is_empty());
    assert!(first_page.rows[0].payments.is_empty());
    let cursor = first_page.next_cursor.as_ref().expect("next cursor");

    let second_page = store
        .list_sales(scope, 1, Some(cursor))
        .await
        .expect("second page");
    assert!(!second_page.has_more);
    assert_eq!(second_page.rows[0].sale_id, first_sale);
    assert_eq!(second_page.rows[0].lines.len(), 1);
    assert_eq!(second_page.rows[0].payments.len(), 1);
    assert!(!second_page.rows[0].lines_truncated);
    assert!(!second_page.rows[0].payments_truncated);
    assert!(second_page.rows[0].lines.len() <= MAX_POS_READ_CHILD_ROWS_PER_SALE as usize);
}
