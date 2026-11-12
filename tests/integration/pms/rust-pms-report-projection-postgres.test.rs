//! Native PMS dashboard/report reads over real migrated PostgreSQL. These are
//! owner read projections: no serializable unit, no write, no receipt and no
//! browser acceptance is inferred.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_pms::public::report::*;
use erpx_server_pms::public::room_board::PmsBoardActor;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

fn config() -> String {
    serde_json::json!({
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

async fn property(pool: &DatabasePool, id: &str, company: &str, branch: &str, name: &str) {
    sqlx::query(
        "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES($1,$2,$3,$4,'2026-11-04','open',$5,1,'maker',1)",
    )
    .bind(id)
    .bind(company)
    .bind(branch)
    .bind(name)
    .bind(config())
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn record(
    pool: &DatabasePool,
    property_id: &str,
    id: &str,
    kind: &str,
    data: serde_json::Value,
) {
    record_at(pool, property_id, id, kind, data, 1).await;
}

async fn record_at(
    pool: &DatabasePool,
    property_id: &str,
    id: &str,
    kind: &str,
    data: serde_json::Value,
    updated_at: i64,
) {
    sqlx::query(
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES($1,$2,$3,$4,1,$5)",
    )
    .bind(id)
    .bind(property_id)
    .bind(kind)
    .bind(data.to_string())
    .bind(updated_at)
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "manager".into(),
        company_id: "company".into(),
        branch_id: Some("north".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.reports".into()],
        modules: vec!["pms".into()],
        expires_at: 10_000,
    }
}

fn range(from: &str, to: &str) -> PmsReportRange {
    PmsReportRange {
        property_id: "property-1".into(),
        from: from.into(),
        to: to.into(),
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence error: {error}"),
    }
}

fn entry(
    id: &str,
    date: &str,
    kind: &str,
    net: i64,
    service: i64,
    tax: i64,
    total: i64,
    source_key: &str,
    original_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "guestAccountId": "ga-1",
        "stayId": "stay-d",
        "date": date,
        "kind": kind,
        "description": id,
        "net": net,
        "service": service,
        "tax": tax,
        "total": total,
        "method": "cash",
        "reference": "",
        "sourceKey": source_key,
        "originalId": original_id,
        "shiftId": "shift-1",
        "actorId": "manager",
        "at": 1
    })
}

async fn seed(pool: &DatabasePool) {
    migrate(pool).await;
    property(pool, "property-1", "company", "north", "Riverside").await;
    property(pool, "property-2", "company", "south", "South Hotel").await;
    property(pool, "property-x", "other", "other", "Foreign Hotel").await;

    record(
        pool,
        "property-1",
        "type-1",
        "room_type",
        serde_json::json!({"name":"Deluxe","capacity":2,"amenities":""}),
    )
    .await;
    record(
        pool,
        "property-1",
        "rate-1",
        "rate",
        serde_json::json!({"name":"Rack","typeId":"type-1","active":true}),
    )
    .await;
    for (id, number) in [("room-1", "101"), ("room-2", "102")] {
        record(
            pool,
            "property-1",
            id,
            "room",
            serde_json::json!({"number":number,"typeId":"type-1","building":"Main","floor":"1","active":true,"cleanliness":"clean","dnd":false}),
        )
        .await;
    }
    // Room-2 is out of order from 2026-11-02, so the fallback capacity of that
    // day is one room unless a completed night supplies its own metrics.
    record(
        pool,
        "property-1",
        "block-1",
        "block",
        serde_json::json!({"kind":"maintenance","state":"active","roomId":"room-2","from":"2026-11-02","to":"2026-11-04","reason":"repair"}),
    )
    .await;
    record(
        pool,
        "property-1",
        "night-2",
        "night",
        serde_json::json!({"date":"2026-11-02","state":"completed","initiatedBy":"auditor","message":"","completedAt":1,"roomMetrics":{"capacity":8,"sold":5}}),
    )
    .await;
    record(
        pool,
        "property-1",
        "night-4",
        "night",
        serde_json::json!({"date":"2026-11-04","state":"pending","initiatedBy":"auditor","message":""}),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-b",
        "stay",
        serde_json::json!({"state":"in_house","arrival":"2026-10-30","departure":"2026-11-01","reservationId":"res-b"}),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-c",
        "stay",
        serde_json::json!({"state":"reserved","arrival":"2026-11-02","departure":"2026-11-03","reservationId":"res-c"}),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-d",
        "stay",
        serde_json::json!({"state":"in_house","arrival":"2026-11-01","departure":"2026-11-05","reservationId":"res-d"}),
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-e",
        "stay",
        serde_json::json!({"state":"no_show","arrival":"2026-11-02","departure":"2026-11-03","reservationId":"res-e"}),
    )
    .await;
    // 2026-11-01T05:00:00Z is 2026-11-01 12:00 in Asia/Bangkok.
    record_at(
        pool,
        "property-1",
        "stay-f2",
        "stay",
        serde_json::json!({"state":"cancelled","arrival":"2026-11-01","departure":"2026-11-02","reservationId":"res-f2"}),
        1_793_509_200_000,
    )
    .await;
    // 2026-11-01T20:00:00Z is 2026-11-02 03:00 in Asia/Bangkok, so the
    // property timezone owner boundary decides the day, not UTC.
    record_at(
        pool,
        "property-1",
        "stay-f3",
        "stay",
        serde_json::json!({"state":"cancelled","arrival":"2026-11-02","departure":"2026-11-03","reservationId":"res-f3"}),
        1_793_563_200_000,
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-g",
        "stay",
        serde_json::json!({"state":"checked_out","arrival":"2026-10-31","departure":"2026-11-02","reservationId":"res-g"}),
    )
    .await;
    record(
        pool,
        "property-1",
        "shift-1",
        "shift",
        serde_json::json!({"name":"Morning","userId":"cashier","date":"2026-11-01","state":"open","opening":0,"cash":0,"counted":0,"variance":0,"handover":"","reviewedBy":"","movements":[]}),
    )
    .await;
    record(
        pool,
        "property-1",
        "shift-2",
        "shift",
        serde_json::json!({"name":"Evening","userId":"cashier","date":"2026-11-01","state":"closed","opening":0,"cash":0,"counted":0,"variance":250,"handover":"","reviewedBy":"","movements":[]}),
    )
    .await;
    record(
        pool,
        "property-1",
        "shift-3",
        "shift",
        serde_json::json!({"name":"Night","userId":"cashier","date":"2026-11-01","state":"closed","opening":0,"cash":0,"counted":0,"variance":0,"handover":"","reviewedBy":"reviewer","movements":[]}),
    )
    .await;

    // Guest accounts: the PMS-owned due/advance/deposit projection.
    record(
        pool,
        "property-1",
        "ga-1",
        "guest_account",
        serde_json::json!({"name":"Somchai","guestId":"guest-1","reservationId":"res-d","partyId":"","state":"open","due":4000,"advance":0,"advanceTax":0,"deposit":0}),
    )
    .await;
    record(
        pool,
        "property-1",
        "ga-2",
        "guest_account",
        serde_json::json!({"name":"Closed","guestId":"guest-2","reservationId":"res-b","partyId":"","state":"closed","due":0,"advance":3000,"advanceTax":0,"deposit":500}),
    )
    .await;
    record(
        pool,
        "property-1",
        "ga-3",
        "guest_account",
        serde_json::json!({"name":"Credit","guestId":"guest-3","reservationId":"res-c","partyId":"party-1","state":"credit","due":0,"advance":0,"advanceTax":0,"deposit":0}),
    )
    .await;

    // Day 1: one direct room entry, one correction whose ancestor (outside the
    // range) carries the room source, one manual charge, one refund.
    record(
        pool,
        "property-1",
        "entry-old",
        "entry",
        entry(
            "entry-old",
            "2026-10-20",
            "charge",
            4000,
            400,
            280,
            4680,
            "night:stay-d:2026-10-20",
            "",
        ),
    )
    .await;
    record(
        pool,
        "property-1",
        "entry-fix",
        "entry",
        entry(
            "entry-fix",
            "2026-11-01",
            "adjustment",
            -1000,
            0,
            0,
            -1000,
            "manual:fix",
            "entry-old",
        ),
    )
    .await;
    record(
        pool,
        "property-1",
        "entry-room",
        "entry",
        entry(
            "entry-room",
            "2026-11-01",
            "charge",
            5000,
            500,
            350,
            5850,
            "night:stay-d:2026-11-01",
            "",
        ),
    )
    .await;
    record(
        pool,
        "property-1",
        "entry-manual",
        "entry",
        entry(
            "entry-manual",
            "2026-11-01",
            "charge",
            1000,
            100,
            70,
            1170,
            "manual:minibar",
            "",
        ),
    )
    .await;
    record(
        pool,
        "property-1",
        "entry-adjust",
        "entry",
        entry(
            "entry-adjust",
            "2026-11-01",
            "adjustment",
            100,
            0,
            0,
            100,
            "manual:adjust",
            "",
        ),
    )
    .await;
    record(
        pool,
        "property-1",
        "entry-refund",
        "entry",
        entry(
            "entry-refund",
            "2026-11-01",
            "refund",
            0,
            0,
            0,
            -500,
            "manual:refund",
            "",
        ),
    )
    .await;
    record(
        pool,
        "property-1",
        "entry-day2",
        "entry",
        entry(
            "entry-day2",
            "2026-11-02",
            "charge",
            7000,
            700,
            490,
            8190,
            "night:stay-d:2026-11-02",
            "",
        ),
    )
    .await;
    // Outside the reported range, so it must not move any day row.
    record(
        pool,
        "property-1",
        "entry-before",
        "entry",
        entry(
            "entry-before",
            "2026-10-31",
            "charge",
            9000,
            900,
            630,
            10530,
            "night:stay-d:2026-10-31",
            "",
        ),
    )
    .await;

    for index in 0..51 {
        record(
            pool,
            "property-1",
            &format!("entry-p{index:02}"),
            "entry",
            entry(
                &format!("entry-p{index:02}"),
                "2026-11-03",
                "charge",
                100,
                0,
                7,
                107,
                "manual:page",
                "",
            ),
        )
        .await;
    }

    sqlx::query(
        "INSERT INTO pms_jobs(id,property_id,business_date,state,actor_json,updated_at) VALUES('job-1','property-1','2026-11-04','pending',$1,1)",
    )
    .bind(serde_json::json!({"userId":"auditor"}).to_string())
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn range_report_retains_scoping_formulas_and_owner_facts() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();
        let mut repository = PgPmsReportRead::new(&mut connection);
        let report = pms_range_report(
            &actor(),
            &range("2026-11-01", "2026-11-02"),
            &mut repository,
            1_000,
        )
        .await
        .unwrap();
        drop(repository);

        assert_eq!(report.property.id, "property-1");
        assert_eq!(report.property.branch_id, "north");
        assert_eq!(report.currency, "THB");
        assert_eq!(report.business_date, "2026-11-04");
        assert_eq!(report.days.len(), 2);

        let first = &report.days[0];
        assert_eq!(first.date, "2026-11-01");
        // No completed night, so the room fallback supplies capacity; room-2 is
        // only out of order from 2026-11-02.
        assert_eq!((first.capacity, first.sold), (2, 2));
        // The correction counts as a room entry through its night-sourced
        // ancestor, and no other day leaks in.
        assert_eq!(first.room_revenue, 4_000);
        assert_eq!(first.net_revenue, 5_700);
        assert_eq!(first.tax, 420);
        assert_eq!(first.refunds, 500);
        assert_eq!(first.cancellations, 1);
        assert_eq!(first.no_shows, 0);

        let second = &report.days[1];
        assert_eq!(second.date, "2026-11-02");
        // The completed night record overrides both fallbacks.
        assert_eq!((second.capacity, second.sold), (8, 5));
        assert_eq!(second.room_revenue, 7_000);
        assert_eq!(second.net_revenue, 7_700);
        assert_eq!(second.tax, 490);
        assert_eq!(second.cancellations, 1);
        assert_eq!(second.no_shows, 1);

        assert_eq!(report.totals.capacity, 10);
        assert_eq!(report.totals.sold, 7);
        assert_eq!(report.totals.room_revenue, 11_000);
        assert_eq!(report.totals.adr, 1_571);
        assert_eq!(report.totals.revpar, 1_100);
        assert!((report.totals.occupancy - 0.7).abs() < f64::EPSILON);

        assert_eq!(report.flow.arrivals, 1);
        assert_eq!(report.flow.departures, 1);
        assert_eq!(report.flow.in_house, 2);

        assert_eq!(report.balances.source, "pms_guest_accounts");
        assert_eq!(
            (
                report.balances.due,
                report.balances.advance,
                report.balances.deposit,
                report.balances.total_accounts,
                report.balances.open_accounts,
                report.balances.credit_accounts,
                report.balances.accounts_with_due,
            ),
            (4_000, 3_000, 500, 3, 2, 1, 1)
        );

        assert_eq!(report.night_audit.business_date, "2026-11-04");
        assert_eq!(report.night_audit.night_state.as_deref(), Some("pending"));
        assert_eq!(report.night_audit.job_state.as_deref(), Some("pending"));
        assert!(!report.night_audit.ready);
        assert_eq!(
            report.night_audit.blockers.len(),
            4,
            "{:?}",
            report.night_audit.blockers
        );
        assert!(
            report
                .night_audit
                .blockers
                .iter()
                .any(|blocker| blocker == "Process stay-b: Checkout due")
        );
        assert!(
            report
                .night_audit
                .blockers
                .iter()
                .any(|blocker| { blocker == "Process stay-c: Not checked in / No-show" })
        );
        assert!(
            report
                .night_audit
                .blockers
                .iter()
                .any(|blocker| blocker == "Close/review shift variance for Morning")
        );
        assert!(
            report
                .night_audit
                .blockers
                .iter()
                .any(|blocker| blocker == "Close/review shift variance for Evening")
        );

        let mut entries_repository = PgPmsReportRead::new(&mut connection);
        let first_page = pms_report_entries(
            &actor(),
            &PmsReportEntries {
                property_id: "property-1".into(),
                date: "2026-11-03".into(),
                page: 1,
            },
            &mut entries_repository,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(first_page.rows.len(), 50);
        assert!(first_page.has_next);
        assert_eq!(first_page.rows[0].id, "entry-p00");
        assert_eq!(first_page.rows[0].data["kind"], "charge");
        drop(entries_repository);
        let mut second_repository = PgPmsReportRead::new(&mut connection);
        let second_page = pms_report_entries(
            &actor(),
            &PmsReportEntries {
                property_id: "property-1".into(),
                date: "2026-11-03".into(),
                page: 2,
            },
            &mut second_repository,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(second_page.rows.len(), 1);
        assert!(!second_page.has_next);
        assert_eq!(second_page.rows[0].id, "entry-p50");
    })
    .await;
}

#[tokio::test]
async fn foreign_company_branch_and_missing_property_are_concealed() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();

        let mut south = actor();
        south.branch_id = Some("south".into());
        let mut repository = PgPmsReportRead::new(&mut connection);
        let error = pms_range_report(
            &south,
            &range("2026-11-01", "2026-11-02"),
            &mut repository,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");
        drop(repository);

        let mut other = actor();
        other.company_id = "other".into();
        other.branch_id = None;
        let mut repository = PgPmsReportRead::new(&mut connection);
        let error = pms_range_report(
            &other,
            &range("2026-11-01", "2026-11-02"),
            &mut repository,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");
        drop(repository);

        let mut missing = PmsReportRange {
            property_id: "property-missing".into(),
            from: "2026-11-01".into(),
            to: "2026-11-02".into(),
        };
        let mut repository = PgPmsReportRead::new(&mut connection);
        let error = pms_range_report(&actor(), &missing, &mut repository, 1_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");
        drop(repository);

        // The south branch property is visible to a south-scoped actor even
        // with no PMS facts yet.
        let empty = pms_range_report(
            &south,
            &PmsReportRange {
                property_id: "property-2".into(),
                from: "2026-11-01".into(),
                to: "2026-11-01".into(),
            },
            &mut PgPmsReportRead::new(&mut connection),
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(empty.days.len(), 1);
        assert_eq!(
            (empty.totals.capacity, empty.totals.sold, empty.totals.adr),
            (0, 0, 0)
        );
        missing.property_id.clear();
        let mut repository = PgPmsReportRead::new(&mut connection);
        let error = pms_range_report(&actor(), &missing, &mut repository, 1_000)
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "PMS_REPORT_REQUEST_INVALID");
    })
    .await;
}

#[tokio::test]
async fn unbounded_ranges_and_pages_are_rejected_before_any_owner_read() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();

        let mut repository = PgPmsReportRead::new(&mut connection);
        let error = pms_range_report(
            &actor(),
            &range("2026-01-01", "2026-11-01"),
            &mut repository,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_REPORT_RANGE");
        drop(repository);

        let mut repository = PgPmsReportRead::new(&mut connection);
        let error = pms_report_entries(
            &actor(),
            &PmsReportEntries {
                property_id: "property-1".into(),
                date: "2026-11-01".into(),
                page: 10_001,
            },
            &mut repository,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_REPORT_REQUEST_INVALID");
        drop(repository);

        let mut repository = PgPmsReportRead::new(&mut connection);
        let error = pms_report_entries(
            &actor(),
            &PmsReportEntries {
                property_id: "property-1".into(),
                date: "2026-11-31".into(),
                page: 1,
            },
            &mut repository,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_REPORT_DATE");
    })
    .await;
}

/// Transport facts: the registered target, exact origin and bounded query are
/// enforced before any owner read; `view=` and `{report}` dispatch to the same
/// two owner use cases.
mod transport {
    use erpx_server_pms::public::report::{
        PmsReportEntriesRequest, PmsReportHttpBackend, PmsReportHttpFuture, PmsReportHttpResponse,
        PmsReportHttpTarget, PmsReportRangeRequest, pms_report_router,
    };
    use erpx_server_runtime::public::http_support::{Router, Value, json, serve};
    use erpx_server_shared::public::{BusinessError, FailureKind};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    struct Backend {
        calls: AtomicUsize,
        seen: Mutex<Vec<String>>,
        fail: Option<BusinessError>,
    }

    impl Backend {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                seen: Mutex::new(Vec::new()),
                fail: None,
            }
        }

        fn reply(&self, line: String) -> PmsReportHttpFuture<'_, PmsReportHttpResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen.lock().unwrap().push(line);
            let fail = self.fail.clone();
            Box::pin(async move {
                if let Some(error) = fail {
                    return Err(error);
                }
                Ok(PmsReportHttpResponse {
                    data: json!({"days": [], "totals": {}}),
                    context: json!({"userId": "manager"}),
                })
            })
        }
    }

    impl PmsReportHttpBackend for Backend {
        fn range<'a>(
            &'a self,
            request: PmsReportRangeRequest<'a>,
        ) -> PmsReportHttpFuture<'a, PmsReportHttpResponse> {
            self.reply(format!(
                "range:{}:{}:{}:{}:{}",
                request.host,
                request.holding_slug,
                request.selection.property_id,
                request.selection.from,
                request.selection.to
            ))
        }

        fn entries<'a>(
            &'a self,
            request: PmsReportEntriesRequest<'a>,
        ) -> PmsReportHttpFuture<'a, PmsReportHttpResponse> {
            self.reply(format!(
                "entries:{}:{}:{}:{}:{}",
                request.host,
                request.holding_slug,
                request.selection.property_id,
                request.selection.date,
                request.selection.page
            ))
        }
    }

    fn router(backend: Arc<Backend>) -> Router {
        pms_report_router(vec![PmsReportHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }])
        .unwrap()
    }

    async fn start(backend: Arc<Backend>) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            serve(listener, router(backend)).await.unwrap();
        });
        (address, task)
    }

    async fn call(
        address: std::net::SocketAddr,
        path: &str,
        origin: Option<&str>,
    ) -> (u16, Value, String) {
        let origin = origin
            .map(|origin| format!("Origin: {origin}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: acme.example.test\r\n{origin}Connection: close\r\n\r\n"
        );
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let response = String::from_utf8_lossy(&bytes);
        let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap();
        (
            status,
            serde_json::from_str(body).unwrap_or(Value::Null),
            head.to_owned(),
        )
    }

    #[tokio::test]
    async fn report_transport_retains_target_origin_query_bounds_and_view_dispatch() {
        let backend = Arc::new(Backend::new());
        let (address, task) = start(backend.clone()).await;

        let (status, value, head) = call(
            address,
            "/api/v1/holdings/acme/pms/reports/range?property=property-1&from=2026-11-01&to=2026-11-02",
            Some("https://acme.example.test"),
        )
        .await;
        assert_eq!(status, 200, "{value}");
        assert_eq!(value["context"]["userId"], "manager");
        assert!(head.contains("cache-control: private, no-store"));
        assert!(head.contains("referrer-policy: no-referrer"));
        assert!(head.contains("x-content-type-options: nosniff"));

        let (status, _, _) = call(
            address,
            "/api/v1/holdings/acme/pms/reports?view=entries&property=property-1&date=2026-11-02&page=2",
            None,
        )
        .await;
        assert_eq!(status, 200);

        let (status, _, _) = call(
            address,
            "/api/v1/holdings/acme/pms/reports?property=property-1&from=2026-11-01&to=2026-11-02",
            None,
        )
        .await;
        assert_eq!(status, 200);

        assert_eq!(
            backend.seen.lock().unwrap().as_slice(),
            [
                "range:acme.example.test:acme:property-1:2026-11-01:2026-11-02",
                "entries:acme.example.test:acme:property-1:2026-11-02:2",
                "range:acme.example.test:acme:property-1:2026-11-01:2026-11-02",
            ]
        );
        assert_eq!(backend.calls.load(Ordering::SeqCst), 3);

        for (path, expected) in [
            (
                "/api/v1/holdings/other/pms/reports/range?property=property-1&from=2026-11-01&to=2026-11-02",
                404,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/unknown?property=property-1&from=2026-11-01&to=2026-11-02",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/range?property=one&property=two&from=2026-11-01&to=2026-11-02",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/range?property=property-1&from=2026-11-01",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/range?property=property-1&from=2026-11-01&to=2026-11-02&asOf=2026-11-02T00:00:00Z",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/range?property=property-1&from=2026-01-01&to=2026-11-01",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/range?property=&from=2026-11-01&to=2026-11-02",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/entries?property=property-1&date=2026-11-01&page=10001",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports?view=entries&property=property-1&date=2026-11-31",
                422,
            ),
            (
                "/api/v1/holdings/acme/pms/reports/range?property=%ZZ&from=2026-11-01&to=2026-11-02",
                422,
            ),
        ] {
            let (status, value, _) = call(address, path, None).await;
            assert_eq!(status, expected, "{path} returned {value}");
        }

        let (status, value, _) = call(
            address,
            "/api/v1/holdings/acme/pms/reports/range?property=property-1&from=2026-11-01&to=2026-11-02",
            Some("https://evil.example.test"),
        )
        .await;
        assert_eq!(status, 403);
        assert_eq!(value["error"]["code"], "ORIGIN_FORBIDDEN");
        assert_eq!(backend.calls.load(Ordering::SeqCst), 3);
        task.abort();
    }

    #[tokio::test]
    async fn report_transport_maps_owner_errors_to_bounded_statuses() {
        let backend = Arc::new(Backend {
            fail: Some(BusinessError::new(
                FailureKind::NotFound,
                "PMS_NOT_FOUND",
                "Hotel not found in this workspace.",
            )),
            ..Backend::new()
        });
        let (address, task) = start(backend).await;
        let (status, value, _) = call(
            address,
            "/api/v1/holdings/acme/pms/reports/range?property=property-1&from=2026-11-01&to=2026-11-01",
            None,
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(value["error"]["code"], "PMS_NOT_FOUND");
        task.abort();
    }
}
