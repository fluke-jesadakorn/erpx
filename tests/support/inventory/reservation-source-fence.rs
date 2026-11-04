//! Proves the Inventory read fences cover every reservation source behind
//! inventory_active_reservations. A blocked UPDATE is used instead of a data
//! assertion because relation-level lock waiting is the actual guard.
use erpx_server_runtime::public::postgres::DatabasePool;
use sqlx::AssertSqlSafe;

const RESERVATION_SOURCES: [&str; 4] = [
    "pos_reservations",
    "inventory_order_reservations",
    "field_service_part_reservations",
    "manufacturing_material_reservations",
];

pub async fn assert_reservation_source_writes_wait(pool: &DatabasePool) {
    // A non-fenced write must succeed under the same lock timeout, proving the
    // observed wait comes from the Inventory fence rather than a test-wide
    // transaction or lock_timeout that is already aborted.
    let mut control = pool.raw().begin().await.unwrap();
    sqlx::query("SET LOCAL lock_timeout='150ms'")
        .execute(&mut *control)
        .await
        .unwrap();
    sqlx::query("UPDATE support_tickets SET title=title WHERE false")
        .execute(&mut *control)
        .await
        .expect("a non-fenced source must remain writable");
    control.rollback().await.unwrap();

    for table in RESERVATION_SOURCES {
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("SET LOCAL lock_timeout='150ms'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let update = format!("UPDATE {table} SET quantity_milli=quantity_milli");
        let error = sqlx::query(AssertSqlSafe(update))
            .execute(&mut *writer)
            .await
            .expect_err("the reservation source writer must wait behind the Inventory fence");
        assert_eq!(
            error
                .as_database_error()
                .and_then(|value| value.code())
                .as_deref(),
            Some("55P03"),
            "{table} was not protected by the Inventory reservation-source fence"
        );
        writer.rollback().await.unwrap();
    }
}
