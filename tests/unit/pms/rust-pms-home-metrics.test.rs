//! Native PMS home hotel-metrics domain and application facts. No database,
//! HTTP, composition or acceptance claim is inferred here.
mod domain {
    #[path = "../../../../packages/server-pms/src/domain/home_metrics.rs"]
    pub mod home_metrics;
}
mod application {
    #[path = "../../../../packages/server-pms/src/application/home_metrics.rs"]
    pub mod home_metrics;
}

use application::home_metrics::{
    PmsHotelMetricsFuture, PmsHotelMetricsRepository, pms_hotel_metrics,
};
use domain::home_metrics::{
    PmsHotelMetric, PmsHotelMetricsFacts, PmsMetricPropertyRow, PmsMetricRecordRow, ensure_ordered,
    project_hotel_metrics,
};
use serde_json::json;

const BD: &str = "2026-11-04";
const COMPANY: &str = "company";

fn facts() -> PmsHotelMetricsFacts {
    PmsHotelMetricsFacts {
        company_id: COMPANY.into(),
        ..PmsHotelMetricsFacts::default()
    }
}

fn property(id: &str, company: &str, branch: &str, name: &str) -> PmsMetricPropertyRow {
    PmsMetricPropertyRow {
        id: id.into(),
        company_id: company.into(),
        branch_id: branch.into(),
        name: name.into(),
        business_date: BD.into(),
        state: "open".into(),
        config_json: json!({"currency": "THB"}).to_string(),
    }
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

#[test]
fn maintenance_blocks_reduce_capacity_but_not_room_count() {
    let properties = [property("p1", COMPANY, "north", "alpha")];
    let records = vec![
        room("r1", "p1", "true", "dirty"),
        room("r2", "p1", "true", "inspected"),
        room("r3", "p1", "true", "dirty"),
        room("r4", "p1", "false", "dirty"),
        block("b1", "p1", "active", "maintenance", "2026-11-01", "r1"),
        block("b2", "p1", "active", "maintenance", "2026-11-05", "r2"),
        block("b3", "p1", "inactive", "maintenance", "2026-11-01", "r3"),
        block("b4", "p1", "active", "cleaning", "2026-11-01", "r3"),
        block("b5", "p2", "active", "maintenance", "2026-11-01", "r3"),
    ];
    let rows = project_hotel_metrics(&properties, &records, &facts());
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.currency.as_deref(), Some("THB"));
    assert_eq!(
        (
            row.room_count,
            row.inspected,
            row.capacity,
            row.sold,
            row.arrivals,
            row.departures
        ),
        (3, 1, 2, 0, 0, 0)
    );
}

#[test]
fn stay_counts_keep_the_retained_state_and_date_guards() {
    let properties = [property("p1", COMPANY, "north", "alpha")];
    let records = vec![
        stay("s1", "p1", "in_house", "2026-11-01", "2026-11-10"),
        stay("s2", "p1", "checked_out", "2026-11-01", "2026-11-05"),
        stay("s3", "p1", "in_house", "2026-11-01", "2026-11-02"),
        stay("s4", "p1", "reserved", "2026-11-03", "2026-11-09"),
        stay("s5", "p1", "reserved", "2026-11-05", "2026-11-09"),
        stay("s6", "p1", "checked_out", "2026-11-01", "2026-11-03"),
        stay("s7", "p1", "in_house", "2026-11-05", "2026-11-10"),
    ];
    let rows = project_hotel_metrics(&properties, &records, &facts());
    assert_eq!((rows[0].sold, rows[0].arrivals, rows[0].departures), (2, 1, 1));
}

#[test]
fn scope_filters_and_retained_name_id_ordering() {
    let properties = [
        property("p2", COMPANY, "north", "bravo"),
        property("p1", COMPANY, "north", "alpha"),
        property("p3", COMPANY, "south", "charlie"),
        property("p4", "other", "north", "aaa"),
    ];
    let all = project_hotel_metrics(&properties, &[], &facts());
    assert_eq!(
        all.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["p1", "p2", "p3"]
    );

    let branch = PmsHotelMetricsFacts {
        branch_id: Some("south".into()),
        ..facts()
    };
    let branch_rows = project_hotel_metrics(&properties, &[], &branch);
    assert_eq!(branch_rows.len(), 1);
    assert_eq!(branch_rows[0].id, "p3");

    let single = PmsHotelMetricsFacts {
        property_id: Some("p2".into()),
        ..facts()
    };
    let single_rows = project_hotel_metrics(&properties, &[], &single);
    assert_eq!(single_rows.len(), 1);
    assert_eq!(single_rows[0].id, "p2");

    let other = PmsHotelMetricsFacts {
        company_id: "other".into(),
        ..facts()
    };
    let other_rows = project_hotel_metrics(&properties, &[], &other);
    assert_eq!(other_rows.len(), 1);
    assert_eq!(other_rows[0].id, "p4");
}

#[test]
fn equal_names_break_the_order_by_id() {
    let properties = [
        property("p2", COMPANY, "north", "alpha"),
        property("p1", COMPANY, "north", "alpha"),
    ];
    let rows = project_hotel_metrics(&properties, &[], &facts());
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["p1", "p2"]
    );
    assert!(ensure_ordered(&rows).is_ok());
}

#[test]
fn missing_currency_and_empty_reads_stay_safe() {
    let mut p = property("p1", COMPANY, "north", "alpha");
    p.config_json = json!({}).to_string();
    let rows = project_hotel_metrics(&[p], &[], &facts());
    assert_eq!(rows[0].currency, None);
    assert!(project_hotel_metrics(&[], &[], &facts()).is_empty());
}

#[derive(Default)]
struct FakeRepository {
    rows: Vec<PmsHotelMetric>,
    seen: Option<PmsHotelMetricsFacts>,
}

impl PmsHotelMetricsRepository for FakeRepository {
    type Error = erpx_server_shared::public::BusinessError;

    fn hotel_metrics<'a>(
        &'a mut self,
        facts: &'a PmsHotelMetricsFacts,
    ) -> PmsHotelMetricsFuture<'a, Vec<PmsHotelMetric>, Self::Error> {
        self.seen = Some(facts.clone());
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
}

fn metric(id: &str, name: &str) -> PmsHotelMetric {
    PmsHotelMetric {
        id: id.into(),
        name: name.into(),
        branch_id: "north".into(),
        business_date: BD.into(),
        state: "open".into(),
        currency: Some("THB".into()),
        room_count: 0,
        inspected: 0,
        capacity: 0,
        sold: 0,
        arrivals: 0,
        departures: 0,
    }
}

#[tokio::test]
async fn application_delegates_facts_and_fails_closed_on_unordered_rows() {
    let selection = PmsHotelMetricsFacts {
        company_id: COMPANY.into(),
        branch_id: Some("north".into()),
        property_id: None,
    };
    let mut repository = FakeRepository {
        rows: vec![metric("p2", "alpha"), metric("p1", "alpha")],
        seen: None,
    };
    let error = pms_hotel_metrics(&selection, &mut repository)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PMS_HOTEL_METRICS_READ_INVALID");
    assert_eq!(repository.seen, Some(selection.clone()));

    let mut repository = FakeRepository {
        rows: vec![metric("p1", "alpha"), metric("p2", "bravo")],
        seen: None,
    };
    let rows = pms_hotel_metrics(&selection, &mut repository).await.unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn application_returns_the_complete_owner_payload_from_bounded_facts() {
    // Behavioral application test: bounded Company/branch facts go in, and the
    // full retained twelve-field owner payload comes back with every count,
    // currency and scope value. This exercises the use case and the in-memory
    // owner projection; it does not restate a SQL string.
    let properties = [
        property("p2", COMPANY, "north", "bravo"),
        property("p1", COMPANY, "north", "alpha"),
    ];
    let records = vec![
        room("r1", "p1", "true", "inspected"),
        room("r2", "p1", "true", "dirty"),
        block("b1", "p1", "active", "maintenance", "2026-11-01", "r1"),
        stay("s1", "p1", "in_house", "2026-11-01", "2026-11-10"),
        stay("s2", "p1", "reserved", "2026-11-03", "2026-11-09"),
        stay("s3", "p1", "in_house", "2026-11-01", "2026-11-02"),
    ];
    let expected = project_hotel_metrics(&properties, &records, &facts());
    assert_eq!(expected.len(), 2);

    let selection = PmsHotelMetricsFacts {
        company_id: COMPANY.into(),
        branch_id: Some("north".into()),
        property_id: None,
    };
    let mut repository = FakeRepository {
        rows: expected.clone(),
        seen: None,
    };
    let rows = pms_hotel_metrics(&selection, &mut repository)
        .await
        .expect("read");
    // The use case forwarded the bounded scope unchanged and returned every row.
    assert_eq!(repository.seen, Some(selection));
    assert_eq!(rows, expected);

    // Full payload parity: retained name/id order, currency, all six counts,
    // and no cross-branch or cross-company leakage.
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["p1", "p2"]
    );
    let alpha = &rows[0];
    assert_eq!(
        (
            alpha.name.as_str(),
            alpha.branch_id.as_str(),
            alpha.business_date.as_str(),
            alpha.state.as_str(),
            alpha.currency.as_deref()
        ),
        ("alpha", "north", BD, "open", Some("THB"))
    );
    assert_eq!(
        (
            alpha.room_count,
            alpha.inspected,
            alpha.capacity,
            alpha.sold,
            alpha.arrivals,
            alpha.departures
        ),
        (2, 1, 1, 1, 1, 1)
    );
    assert_eq!((rows[1].room_count, rows[1].sold, rows[1].capacity), (0, 0, 0));
    assert!(rows.iter().all(|row| row.branch_id == "north"));
}
