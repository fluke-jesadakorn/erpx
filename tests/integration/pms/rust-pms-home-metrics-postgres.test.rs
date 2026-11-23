//! Native PMS home hotel-metrics read over real migrated PostgreSQL. The SQL is
//! the retained `hotelMetrics` query; the expected rows come from the in-memory
//! domain projection over the exact same seeded PMS facts. No HTTP, composition,
//! write, receipt or acceptance claim is inferred here.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_pms::public::home_metrics::{
    PgPmsHotelMetricsRead, PmsHotelMetricsFacts, PmsMetricPropertyRow, PmsMetricRecordRow,
    pms_hotel_metrics, project_hotel_metrics,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use serde_json::json;

const BD: &str = "2026-11-04";
const COMPANY: &str = "company";

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

fn property(id: &str, company: &str, branch: &str, name: &str, currency: &str) -> PmsMetricPropertyRow {
    PmsMetricPropertyRow {
        id: id.into(),
        company_id: company.into(),
        branch_id: branch.into(),
        name: name.into(),
        business_date: BD.into(),
        state: "open".into(),
        config_json: json!({"currency": currency}).to_string(),
    }
}

fn no_currency_property() -> PmsMetricPropertyRow {
    let mut p = property("p3", COMPANY, "south", "charlie", "THB");
    p.config_json = json!({}).to_string();
    p
}

fn record(id: &str, property_id: &str, kind: &str, data: serde_json::Value) -> PmsMetricRecordRow {
    PmsMetricRecordRow {
        id: id.into(),
        property_id: property_id.into(),
        kind: kind.into(),
        data_json: data.to_string(),
    }
}

fn room(id: &str, property_id: &str, active: &str, cleanliness: &str) -> PmsMetricRecordRow {
    record(
        id,
        property_id,
        "room",
        json!({"active": active, "cleanliness": cleanliness}),
    )
}

fn block(
    id: &str,
    property_id: &str,
    state: &str,
    kind: &str,
    from: &str,
    room_id: &str,
) -> PmsMetricRecordRow {
    record(
        id,
        property_id,
        "block",
        json!({"state": state, "kind": kind, "from": from, "roomId": room_id}),
    )
}

fn stay(
    id: &str,
    property_id: &str,
    state: &str,
    arrival: &str,
    departure: &str,
) -> PmsMetricRecordRow {
    record(
        id,
        property_id,
        "stay",
        json!({"state": state, "arrival": arrival, "departure": departure}),
    )
}

async fn insert_property(pool: &DatabasePool, p: &PmsMetricPropertyRow) {
    sqlx::query(
        "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,1,'maker',1)",
    )
    .bind(&p.id)
    .bind(&p.company_id)
    .bind(&p.branch_id)
    .bind(&p.name)
    .bind(&p.business_date)
    .bind(&p.state)
    .bind(&p.config_json)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn insert_record(pool: &DatabasePool, r: &PmsMetricRecordRow) {
    sqlx::query(
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES($1,$2,$3,$4,1,1)",
    )
    .bind(&r.id)
    .bind(&r.property_id)
    .bind(&r.kind)
    .bind(&r.data_json)
    .execute(pool.raw())
    .await
    .unwrap();
}

/// Builds and inserts one consistent fact set, returning the in-memory rows so
/// the parity assertion uses exactly what was seeded.
async fn seed(pool: &DatabasePool) -> (Vec<PmsMetricPropertyRow>, Vec<PmsMetricRecordRow>) {
    migrate(pool).await;
    let properties = vec![
        property("p1", COMPANY, "north", "alpha", "THB"),
        property("p2", COMPANY, "east", "bravo", "USD"),
        no_currency_property(),
        property("p4", "other", "north", "aaa", "EUR"),
    ];
    let records = vec![
        room("r1", "p1", "true", "dirty"),
        room("r2", "p1", "true", "inspected"),
        room("r3", "p1", "true", "dirty"),
        room("r4", "p1", "false", "dirty"),
        block("b1", "p1", "active", "maintenance", "2026-11-01", "r1"),
        block("b2", "p1", "active", "maintenance", "2026-11-05", "r2"),
        block("b3", "p1", "inactive", "maintenance", "2026-11-01", "r3"),
        stay("s1", "p1", "in_house", "2026-11-01", "2026-11-10"),
        stay("s2", "p1", "checked_out", "2026-11-01", "2026-11-05"),
        stay("s3", "p1", "in_house", "2026-11-01", "2026-11-02"),
        stay("s4", "p1", "reserved", "2026-11-03", "2026-11-09"),
        stay("s5", "p1", "reserved", "2026-11-05", "2026-11-09"),
        room("r5", "p2", "true", "dirty"),
    ];
    for p in &properties {
        insert_property(pool, p).await;
    }
    for r in &records {
        insert_record(pool, r).await;
    }
    (properties, records)
}

fn all_facts() -> PmsHotelMetricsFacts {
    PmsHotelMetricsFacts {
        company_id: COMPANY.into(),
        ..PmsHotelMetricsFacts::default()
    }
}

#[tokio::test]
async fn native_hotel_metrics_match_the_domain_projection() {
    fixture::with_database(2, |pool| async move {
        let (properties, records) = seed(&pool).await;
        let facts = all_facts();
        let expected = project_hotel_metrics(&properties, &records, &facts);
        let mut connection = pool.raw().acquire().await.unwrap();
        let actual = pms_hotel_metrics(&facts, &mut PgPmsHotelMetricsRead::new(&mut connection))
            .await
            .unwrap();
        assert_eq!(actual, expected);

        assert_eq!(
            actual.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["p1", "p2", "p3"],
            "retained ORDER BY p.name,p.id and company scope"
        );
        let alpha = &actual[0];
        assert_eq!(alpha.currency.as_deref(), Some("THB"));
        assert_eq!(
            (
                alpha.room_count,
                alpha.inspected,
                alpha.capacity,
                alpha.sold,
                alpha.arrivals,
                alpha.departures
            ),
            (3, 1, 2, 2, 1, 1),
            "maintenance-block capacity rule and stay/date guards"
        );
        assert_eq!(actual[2].currency, None);
    })
    .await;
}

#[tokio::test]
async fn branch_and_property_scope_match_the_domain_projection() {
    fixture::with_database(2, |pool| async move {
        let (properties, records) = seed(&pool).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        for facts in [
            PmsHotelMetricsFacts {
                branch_id: Some("south".into()),
                ..all_facts()
            },
            PmsHotelMetricsFacts {
                property_id: Some("p1".into()),
                ..all_facts()
            },
            PmsHotelMetricsFacts {
                branch_id: Some("east".into()),
                property_id: Some("p1".into()),
                ..all_facts()
            },
            PmsHotelMetricsFacts {
                company_id: "other".into(),
                ..all_facts()
            },
        ] {
            let expected = project_hotel_metrics(&properties, &records, &facts);
            let actual =
                pms_hotel_metrics(&facts, &mut PgPmsHotelMetricsRead::new(&mut connection))
                    .await
                    .unwrap();
            assert_eq!(actual, expected, "facts: {facts:?}");
        }
    })
    .await;
}
