//! Composition seam for the final-root witness in the Customer 360 timeline.
//! Proves a caller's uncommitted Field writes are visible through the injected
//! fixed source statements, and that a witness from another transaction fails
//! closed instead of silently reading a changed source snapshot.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_customer_360::public::customer_timeline::{
    CustomerTimelineActor, CustomerTimelineView, FinalRootEventSources, PgCustomerTimelineRead,
};
use erpx_server_field_service::public::final_root_consumer::final_root_event_sql;
use erpx_server_field_service::public::final_root_projection::{
    inspect_final_root_projection_sources, lock_final_root_projection_sources,
};
use erpx_server_runtime::public::postgres::{MigrationTrack, load_migrations, migrate_database};
use erpx_server_shared::public::{CompanyId, RecordId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> CustomerTimelineActor {
    CustomerTimelineActor {
        user_id: "user".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        role: "field_tech".into(),
        permissions: vec!["commerce.read".into()],
        modules: vec!["field_service".into()],
        assignment_id: "assignment-field".into(),
        expires_at: NOW.get() + 60_000,
    }
}

#[tokio::test]
async fn final_root_witness_sees_caller_writes_and_rejects_a_stale_unit() {
    fixture::with_database(5, |pool| async move {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::HoldingWrite)
            .await
            .unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES \
             ('customer','company','customer','C-001','Original','THB','active',1,10,10);\
             INSERT INTO field_service_agreements(id,company_id,branch_id,department_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at) VALUES \
             ('agreement','company','branch',NULL,'customer','Synthetic agreement','2026-09-01','2027-09-01','THB',2500,'draft','owner',1,106,106);\
             INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES \
             ('agreement-event','agreement',1,'create','owner','Synthetic','draft',106);",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        // The exact owner utility fence Customer360Sources::prepare acquires.
        lock_final_root_projection_sources(&mut tx).await.unwrap();
        let witness = inspect_final_root_projection_sources(&mut tx).await.unwrap();
        let (agreement_events, order_events) = final_root_event_sql(&witness);
        let sources = FinalRootEventSources {
            agreement_events,
            order_events,
        };

        // This write is still uncommitted: it is visible only inside this
        // transaction's snapshot, which the witness is bound to.
        sqlx::query(
            "INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) \
             VALUES('late-uncommitted-event','agreement',2,'activate','owner','Late caller write','draft',107)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        let items = {
            let mut reader = PgCustomerTimelineRead::with_sources(&mut tx, sources);
            reader
                .events(
                    &actor(),
                    "customer",
                    CustomerTimelineView {
                        support: false,
                        field_service: true,
                        finance: false,
                        legal: false,
                    },
                    None,
                    50,
                )
                .await
                .unwrap()
        };
        assert!(
            items
                .iter()
                .any(|item| item.id == "service-agreement:late-uncommitted-event"),
            "the caller's uncommitted Field event must be in the composed timeline"
        );
        // A write inside the witnessed unit does not invalidate it.
        witness.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();

        // A fresh unit (new backend pid/xid) must reject the old witness rather
        // than treat a different snapshot as current.
        let mut next = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        lock_final_root_projection_sources(&mut next).await.unwrap();
        let error = witness.require_current(&mut next).await.unwrap_err();
        assert!(
            format!("{error:?}").contains("FIELD_ROOT_PROJECTION_SOURCE_CHANGED"),
            "stale witness must fail closed, got {error:?}"
        );
        next.rollback().await.unwrap();
    })
    .await;
}
