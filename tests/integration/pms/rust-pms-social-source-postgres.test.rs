//! PMS-owned source union and payout-source reads over migrated PostgreSQL.
//!
//! Proves the exact `pmsSourcesSql` union for records, transfer requests and
//! charge corrections, plus the refund payout `originalId` lookup.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_pms::public::pms_social_event::{
    PMS_SOURCES_SQL, pms_social_payout_source, pms_social_source,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,\
         version,created_by,updated_at) VALUES\
         ('property','company','branch','Grand Hotel','2026-10-05','open','{}',1,'creator',100),\
         ('other-property','other','branch','Other Hotel','2026-10-05','open','{}',1,'creator',100);\
         INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES\
         ('reservation','property','reservation','{\"state\":\"confirmed\",\"guestAccountId\":\"guest\"}',4,100),\
         ('folio','property','guest_account','{}',1,100),\
         ('shift','property','shift','{\"userId\":\"cashier\"}',1,100),\
         ('entry','property','entry','{\"originalId\":\"refund\"}',1,100),\
         ('refund','property','refund','{\"state\":\"pending\",\"requestedBy\":\"cashier\"}',2,100),\
         ('empty-entry','property','entry','{}',1,100);\
         INSERT INTO pms_transfer_requests(id,company_id,property_id,guest_account_id,shift_id,kind,\
         business_date,currency,amount_minor,reference,state,version,recorded_by,created_at,updated_at) VALUES\
         ('transfer','company','property','folio','shift','receipt','2026-10-05','THB',1000,'ref-1',\
          'pending',5,'cashier',100,100);\
         INSERT INTO pms_charge_corrections(id,company_id,property_id,entry_id,guest_account_id,status,\
         version,reason,requested_by,created_at,updated_at) VALUES\
         ('correction','company','property','entry','folio','pending',3,'Wrong charge','cashier',100,100);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn source_union_reads_records_transfers_and_corrections() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();

        let reservation =
            pms_social_source(&mut connection, Some("reservation"), "company", "property")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(reservation.id, "reservation");
        assert_eq!(reservation.kind, "reservation");
        assert_eq!(reservation.company_id, "company");
        assert_eq!(reservation.branch_id, "branch");
        assert_eq!(reservation.property_name, "Grand Hotel");
        assert_eq!(reservation.business_date, "2026-10-05");
        assert_eq!(reservation.version, 4);
        assert_eq!(reservation.data_json["state"], "confirmed");
        assert_eq!(reservation.data_json["guestAccountId"], "guest");

        let transfer = pms_social_source(&mut connection, Some("transfer"), "company", "property")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(transfer.kind, "transfer");
        assert_eq!(transfer.version, 5);
        assert_eq!(transfer.data_json["state"], "pending");
        assert_eq!(transfer.data_json["recordedBy"], "cashier");
        assert_eq!(transfer.data_json["guestAccountId"], "folio");

        let correction =
            pms_social_source(&mut connection, Some("correction"), "company", "property")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(correction.kind, "correction");
        assert_eq!(correction.version, 3);
        assert_eq!(correction.data_json["state"], "pending");
        assert_eq!(correction.data_json["requestedBy"], "cashier");

        // Company/property filters and a null source id fail closed.
        assert!(
            pms_social_source(&mut connection, Some("reservation"), "other", "property")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            pms_social_source(
                &mut connection,
                Some("reservation"),
                "company",
                "other-property"
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(
            pms_social_source(&mut connection, None, "company", "property")
                .await
                .unwrap()
                .is_none()
        );
    })
    .await;
}

#[tokio::test]
async fn payout_source_reads_the_refund_original_id() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        assert_eq!(
            pms_social_payout_source(&mut connection, "entry", "property")
                .await
                .unwrap(),
            Some("refund".into())
        );
        assert_eq!(
            pms_social_payout_source(&mut connection, "empty-entry", "property")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            pms_social_payout_source(&mut connection, "entry", "other-property")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            pms_social_payout_source(&mut connection, "reservation", "property")
                .await
                .unwrap(),
            None,
            "only kind='entry' is a payout source"
        );
    })
    .await;
}

#[test]
fn retained_union_keeps_three_branches() {
    assert_eq!(PMS_SOURCES_SQL.matches("UNION ALL").count(), 2);
    assert!(PMS_SOURCES_SQL.contains("FROM pms_records r JOIN pms_properties p"));
    assert!(PMS_SOURCES_SQL.contains("FROM pms_transfer_requests t"));
    assert!(PMS_SOURCES_SQL.contains("FROM pms_charge_corrections c"));
}
