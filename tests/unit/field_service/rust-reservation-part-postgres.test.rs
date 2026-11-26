//! Real Postgres execution proof for the native Field part-reservation read.
//!
//! BUILD FREEZE: the lead owns `packages/server-field_service/Cargo.toml`, so
//! this test is ready to wire but has no `[[test]]` entry yet. Add:
//!   [[test]]
//!   name = "reservation_part_postgres"
//!   path = "../../tests/unit/field_service/rust-reservation-part-postgres.test.rs"
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_field_service::public::reservation_part::{
    FieldPartReservationQuery, FieldPartReservationStore, PgFieldPartReservationStore, project,
};

#[tokio::test]
async fn owner_adapter_executes_real_sql_and_returns_exact_payload() {
    fixture::with_database(1, |pool| async move {
        // Temporary relations shadow the real names on this connection's search
        // path, so the adapter's own unqualified SELECT runs against actual
        // Postgres rows without committing business data.
        sqlx::raw_sql(
            "CREATE TEMP TABLE field_service_orders(id text, company_id text); \
             CREATE TEMP TABLE field_service_part_reservations(id text, order_id text, warehouse_id text, product_id text, quantity_milli bigint, consumed_milli bigint, released_milli bigint, expires_at bigint); \
             INSERT INTO field_service_orders VALUES('order','company'); \
             INSERT INTO field_service_part_reservations VALUES('reservation','order','warehouse','product',1000,400,100,1800000010100);",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let store = PgFieldPartReservationStore::new(pool.raw().clone());
        let query = FieldPartReservationQuery {
            holding_id: "holding".into(),
            company_id: "company".into(),
            order_id: "order".into(),
            reservation_id: "reservation".into(),
        };
        let row = store.read(&query).await.unwrap().expect("row from real SQL");
        let value = project(&row, &query).unwrap();
        assert_eq!(value["id"], "reservation");
        assert_eq!(value["orderId"], "order");
        assert_eq!(value["warehouseId"], "warehouse");
        assert_eq!(value["productId"], "product");
        assert_eq!(value["quantityMilli"], 1000);
        assert_eq!(value["consumedMilli"], 400);
        assert_eq!(value["releasedMilli"], 100);
        assert_eq!(value["expiresAt"], 1_800_000_010_100i64);
        assert_eq!(value.as_object().unwrap().len(), 8);

        let wrong_company = FieldPartReservationQuery {
            company_id: "other".into(),
            ..query
        };
        assert!(store.read(&wrong_company).await.unwrap().is_none());
    })
    .await;
}
