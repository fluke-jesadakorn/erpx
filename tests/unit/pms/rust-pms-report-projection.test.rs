//! Native PMS dashboard/report read domain and application facts. No database,
//! HTTP, browser or receipt acceptance is inferred here.
mod domain {
    #[path = "../../../../packages/server-pms/src/domain/night_audit.rs"]
    pub mod night_audit;
    #[path = "../../../../packages/server-pms/src/domain/report_projection.rs"]
    pub mod report_projection;
    #[path = "../../../../packages/server-pms/src/domain/room_board.rs"]
    pub mod room_board;
}
mod application {
    #[path = "../../../../packages/server-pms/src/application/report_projection.rs"]
    pub mod report_projection;
}

use application::report_projection::{
    PmsReportNightRead, PmsReportRepository, pms_range_report, pms_report_entries,
};
use domain::night_audit::PmsNightAuditFacts;
use domain::report_projection::{
    PAGE_SIZE, PmsReportBalances, PmsReportDay, PmsReportEntries, PmsReportEntry, PmsReportFlow,
    PmsReportRange, js_round, summarize,
};
use domain::room_board::{PmsBoardActor, PmsPropertyRow};
use serde_json::json;

const NOW: i64 = 1_700_000_000_000;

fn actor() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "manager".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.reports".into()],
        modules: vec!["pms".into()],
        expires_at: NOW + 60_000,
    }
}

fn config() -> String {
    json!({
        "country": "TH",
        "currency": "THB",
        "minorUnit": 2,
        "timezone": "Asia/Bangkok",
        "checkIn": "14:00",
        "checkOut": "12:00",
        "cancellationPolicy": "",
        "chartId": "chart",
        "accounts": {
            "cash": "cash", "bank": "bank", "edc": "edc", "receivable": "receivable",
            "advance": "advance", "deposit": "deposit", "revenue": "revenue",
            "service": "service", "tax": "tax", "prepaidTax": "prepaidTax",
            "inventory": "inventory", "consumption": "consumption",
            "commission": "commission", "cashDifference": "cashDifference"
        },
        "taxBps": 700,
        "serviceBps": 1000,
        "taxInclusive": false,
        "advanceTax": true,
        "preparedBy": "maker",
        "approvedBy": "reviewer",
        "revision": 1
    })
    .to_string()
}

fn property_row(id: &str, company: &str, branch: &str, business_date: &str) -> PmsPropertyRow {
    PmsPropertyRow {
        id: id.into(),
        company_id: company.into(),
        branch_id: branch.into(),
        name: "Riverside".into(),
        business_date: business_date.into(),
        state: "open".into(),
        config_json: config(),
        version: 1,
    }
}

fn day(date: &str, capacity: i64, sold: i64, room_revenue: i64) -> PmsReportDay {
    PmsReportDay {
        date: date.into(),
        capacity,
        sold,
        room_revenue,
        net_revenue: room_revenue + 700,
        tax: 700,
        refunds: 0,
        cancellations: 0,
        no_shows: 0,
    }
}

fn balances() -> PmsReportBalances {
    PmsReportBalances {
        source: "pms_guest_accounts".into(),
        due: 4_000,
        advance: 3_000,
        deposit: 500,
        total_accounts: 3,
        open_accounts: 2,
        credit_accounts: 1,
        accounts_with_due: 1,
    }
}

/// Read-only fake repository: one property in scope, deterministic facts.
struct Fixture {
    property: Option<PmsPropertyRow>,
    days: Vec<PmsReportDay>,
    flow: PmsReportFlow,
    balances: PmsReportBalances,
    stays: Vec<domain::room_board::PmsRecord>,
    shifts: Vec<domain::room_board::PmsRecord>,
    night_state: Option<String>,
    job_state: Option<String>,
    entries: Vec<PmsReportEntry>,
    calls: usize,
}

impl Fixture {
    fn new() -> Self {
        Self {
            property: Some(property_row("property", "company", "branch", "2026-11-04")),
            days: Vec::new(),
            flow: PmsReportFlow {
                arrivals: 2,
                departures: 3,
                in_house: 4,
            },
            balances: balances(),
            stays: Vec::new(),
            shifts: Vec::new(),
            night_state: Some("pending".into()),
            job_state: Some("pending".into()),
            entries: Vec::new(),
            calls: 0,
        }
    }
    fn with_days(mut self, days: Vec<PmsReportDay>) -> Self {
        self.days = days;
        self
    }
}

type Error = erpx_server_shared::public::BusinessError;

impl PmsReportRepository for Fixture {
    type Error = Error;

    fn property<'a>(
        &'a mut self,
        company_id: &'a str,
        property_id: &'a str,
    ) -> application::report_projection::PmsReportFuture<'a, Option<PmsPropertyRow>, Error> {
        self.calls += 1;
        let property = self
            .property
            .clone()
            .filter(|row| row.company_id == company_id && row.id == property_id);
        Box::pin(async move { Ok(property) })
    }

    fn days<'a>(
        &'a mut self,
        _property: &'a PmsPropertyRow,
        _from: &'a str,
        _to: &'a str,
    ) -> application::report_projection::PmsReportFuture<'a, Vec<PmsReportDay>, Error> {
        self.calls += 1;
        let days = self.days.clone();
        Box::pin(async move { Ok(days) })
    }

    fn flow<'a>(
        &'a mut self,
        _property_id: &'a str,
        _business_date: &'a str,
    ) -> application::report_projection::PmsReportFuture<'a, PmsReportFlow, Error> {
        self.calls += 1;
        let flow = self.flow;
        Box::pin(async move { Ok(flow) })
    }

    fn balances<'a>(
        &'a mut self,
        _property_id: &'a str,
    ) -> application::report_projection::PmsReportFuture<'a, PmsReportBalances, Error> {
        self.calls += 1;
        let balances = self.balances.clone();
        Box::pin(async move { Ok(balances) })
    }

    fn night_read<'a>(
        &'a mut self,
        property: &'a PmsPropertyRow,
    ) -> application::report_projection::PmsReportFuture<'a, PmsReportNightRead, Error> {
        self.calls += 1;
        let mut facts = PmsNightAuditFacts::empty(property.clone());
        facts.stays = self.stays.clone();
        facts.shifts = self.shifts.clone();
        let nights = self.night_state.clone();
        let jobs = self.job_state.clone();
        Box::pin(async move {
            Ok(PmsReportNightRead {
                night_state: nights,
                job_state: jobs,
                facts,
            })
        })
    }

    fn entries<'a>(
        &'a mut self,
        _property_id: &'a str,
        _date: &'a str,
        _offset: i64,
    ) -> application::report_projection::PmsReportFuture<'a, Vec<PmsReportEntry>, Error> {
        self.calls += 1;
        let entries = self.entries.clone();
        Box::pin(async move { Ok(entries) })
    }
}

fn record(id: &str, kind: &str, data: serde_json::Value) -> domain::room_board::PmsRecord {
    domain::room_board::PmsRecord {
        id: id.into(),
        kind: kind.into(),
        version: 1,
        updated_at: 1,
        data,
    }
}

fn range() -> PmsReportRange {
    PmsReportRange {
        property_id: "property".into(),
        from: "2026-11-01".into(),
        to: "2026-11-02".into(),
    }
}

fn entries() -> PmsReportEntries {
    PmsReportEntries {
        property_id: "property".into(),
        date: "2026-11-01".into(),
        page: 1,
    }
}

#[tokio::test]
async fn range_report_retains_formulas_flow_balances_and_context() {
    let mut fixture = Fixture::new().with_days(vec![
        day("2026-11-01", 4, 2, 5_000),
        day("2026-11-02", 6, 3, 7_000),
    ]);
    let report = pms_range_report(&actor(), &range(), &mut fixture, NOW)
        .await
        .unwrap();
    assert_eq!(report.property.id, "property");
    assert_eq!(report.property.branch_id, "branch");
    assert_eq!(report.currency, "THB");
    assert_eq!(report.business_date, "2026-11-04");
    assert_eq!(report.from, "2026-11-01");
    assert_eq!(report.to, "2026-11-02");
    assert_eq!(report.totals.capacity, 10);
    assert_eq!(report.totals.sold, 5);
    assert_eq!(report.totals.room_revenue, 12_000);
    assert_eq!(report.totals.net_revenue, 13_400);
    assert!((report.totals.occupancy - 0.5).abs() < f64::EPSILON);
    assert_eq!(report.totals.adr, 2_400);
    assert_eq!(report.totals.revpar, 1_200);
    assert_eq!(report.flow.arrivals, 2);
    assert_eq!(report.flow.departures, 3);
    assert_eq!(report.flow.in_house, 4);
    assert_eq!(report.balances.source, "pms_guest_accounts");
    assert_eq!(report.balances.due, 4_000);
    assert_eq!(report.balances.advance, 3_000);
    assert_eq!(report.balances.deposit, 500);
    assert_eq!(report.night_audit.night_state.as_deref(), Some("pending"));
    assert_eq!(report.night_audit.job_state.as_deref(), Some("pending"));
    assert!(!report.night_audit.ready);
    assert!(
        report
            .night_audit
            .blockers
            .iter()
            .any(|blocker| blocker == "Add a room type")
    );
}

#[tokio::test]
async fn range_report_rounds_half_up_and_counts_overdue_departures_and_shifts() {
    let mut fixture = Fixture::new().with_days(vec![
        day("2026-11-01", 3, 1, 1_000),
        day("2026-11-02", 3, 0, 0),
    ]);
    fixture.stays = vec![
        record(
            "stay-a",
            "stay",
            json!({"state":"in_house","arrival":"2026-10-30","departure":"2026-11-01"}),
        ),
        record(
            "stay-b",
            "stay",
            json!({"state":"reserved","arrival":"2026-11-09","departure":"2026-11-10"}),
        ),
    ];
    fixture.shifts = vec![record(
        "shift-1",
        "shift",
        json!({"name":"Morning","date":"2026-11-01","state":"open","variance":0}),
    )];
    let report = pms_range_report(&actor(), &range(), &mut fixture, NOW)
        .await
        .unwrap();
    assert_eq!(report.totals.adr, 1_000);
    assert_eq!(report.totals.revpar, js_round(1_000.0 / 6.0));
    assert!(
        report
            .night_audit
            .blockers
            .iter()
            .any(|blocker| blocker.contains("Checkout due"))
    );
    assert!(
        report
            .night_audit
            .blockers
            .iter()
            .any(|blocker| blocker == "Close/review shift variance for Morning")
    );
}

#[tokio::test]
async fn another_branch_and_another_company_are_concealed_as_not_found() {
    let mut foreign_branch = Fixture::new();
    foreign_branch.property = Some(property_row("property", "company", "south", "2026-11-04"));
    assert_eq!(
        pms_range_report(&actor(), &range(), &mut foreign_branch, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_NOT_FOUND"
    );
    let mut foreign_company = Fixture::new();
    foreign_company.property = Some(property_row("property", "other", "branch", "2026-11-04"));
    assert_eq!(
        pms_range_report(&actor(), &range(), &mut foreign_company, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_NOT_FOUND"
    );
}

#[tokio::test]
async fn reports_require_pms_read_and_pms_reports_before_any_read() {
    let mut fixture = Fixture::new();
    let mut no_reports = actor();
    no_reports.permissions = vec!["pms.read".into()];
    assert_eq!(
        pms_range_report(&no_reports, &range(), &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PERMISSION_REQUIRED"
    );
    assert_eq!(fixture.calls, 0);

    let mut no_read = actor();
    no_read.permissions = vec!["pms.reports".into()];
    assert_eq!(
        pms_range_report(&no_read, &range(), &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PERMISSION_REQUIRED"
    );
    assert_eq!(fixture.calls, 0);

    let mut department = actor();
    department.department_id = Some("department".into());
    assert_eq!(
        pms_range_report(&department, &range(), &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_BRANCH_REQUIRED"
    );
    assert_eq!(fixture.calls, 0);

    let mut expired = actor();
    expired.expires_at = NOW;
    assert_eq!(
        pms_range_report(&expired, &range(), &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "WORK_CONTEXT_CHANGED"
    );
    assert_eq!(fixture.calls, 0);
}

#[tokio::test]
async fn unbounded_ranges_and_dates_are_rejected_before_any_read() {
    let mut fixture = Fixture::new();
    let wide = PmsReportRange {
        property_id: "property".into(),
        from: "2026-01-01".into(),
        to: "2026-11-01".into(),
    };
    assert_eq!(
        pms_range_report(&actor(), &wide, &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_REPORT_RANGE"
    );
    let reversed = PmsReportRange {
        property_id: "property".into(),
        from: "2026-11-02".into(),
        to: "2026-11-01".into(),
    };
    assert_eq!(
        pms_range_report(&actor(), &reversed, &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_REPORT_RANGE"
    );
    let invalid_date = PmsReportRange {
        property_id: "property".into(),
        from: "2026-11-01".into(),
        to: "2026-11-31".into(),
    };
    assert_eq!(
        pms_range_report(&actor(), &invalid_date, &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_REPORT_DATE"
    );
    assert_eq!(fixture.calls, 0);
}

#[tokio::test]
async fn a_range_read_must_cover_every_day_or_fail_closed() {
    let mut fixture = Fixture::new().with_days(vec![day("2026-11-01", 4, 2, 5_000)]);
    assert_eq!(
        pms_range_report(&actor(), &range(), &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_REPORT_READ_INVALID"
    );
}

#[tokio::test]
async fn entry_pages_are_bounded_and_ordered_by_the_owner_read() {
    let mut fixture = Fixture::new();
    fixture.entries = (0..=PAGE_SIZE)
        .map(|index| PmsReportEntry {
            id: format!("entry-{index:02}"),
            data: json!({"kind":"charge","date":"2026-11-01","total":1_000}),
        })
        .collect();
    let page = pms_report_entries(&actor(), &entries(), &mut fixture, NOW)
        .await
        .unwrap();
    assert_eq!(page.page, 1);
    assert_eq!(page.date, "2026-11-01");
    assert_eq!(page.rows.len(), PAGE_SIZE);
    assert!(page.has_next);
    assert_eq!(page.rows[0].id, "entry-00");

    let too_large = PmsReportEntries {
        property_id: "property".into(),
        date: "2026-11-01".into(),
        page: 10_001,
    };
    assert_eq!(
        pms_report_entries(&actor(), &too_large, &mut fixture, NOW)
            .await
            .unwrap_err()
            .code,
        "PMS_REPORT_REQUEST_INVALID"
    );
}

#[tokio::test]
async fn totals_sum_every_day_row() {
    let totals = summarize(&[
        day("2026-11-01", 4, 2, 5_000),
        day("2026-11-02", 6, 3, 7_000),
    ]);
    assert_eq!(
        (
            totals.capacity,
            totals.sold,
            totals.tax,
            totals.net_revenue,
            totals.refunds
        ),
        (10, 5, 1_400, 13_400, 0)
    );
}
