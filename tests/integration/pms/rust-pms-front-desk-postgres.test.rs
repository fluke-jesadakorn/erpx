//! Native PMS front-desk reads over real migrated PostgreSQL. These are owner
//! read projections, not commands, source purchase or browser evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_pms::public::front_desk::*;
use erpx_server_pms::public::room_board::{
    PmsBoardActor, PmsBoardEntitlement, PmsBoardEntitlementPort, PmsBoardFuture, PmsBoardVisibility,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
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
    sqlx::query(
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES($1,$2,$3,$4,1,1)",
    )
    .bind(id)
    .bind(property_id)
    .bind(kind)
    .bind(data.to_string())
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn document(pool: &DatabasePool, property_id: &str, id: &str, account_id: &str) {
    sqlx::query(
        "INSERT INTO pms_documents(id,property_id,guest_account_id,kind,snapshot_json,created_at) VALUES($1,$2,$3,'receipt','{}',1)",
    )
    .bind(id)
    .bind(property_id)
    .bind(account_id)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn guest(pool: &DatabasePool, property_id: &str, id: &str, name: &str) {
    record(
        pool,
        property_id,
        id,
        "guest",
        serde_json::json!({"name":name,"email":"","phone":"","nationality":"","documentRef":"","notes":""}),
    )
    .await;
}

async fn reservation(
    pool: &DatabasePool,
    property_id: &str,
    id: &str,
    number: &str,
    guest_id: &str,
) {
    record(
        pool,
        property_id,
        id,
        "reservation",
        serde_json::json!({"number":number,"guestId":guest_id,"source":"direct","externalRef":"","groupName":"","notes":"","state":"confirmed","createdBy":"maker"}),
    )
    .await;
}

async fn account(
    pool: &DatabasePool,
    property_id: &str,
    id: &str,
    guest_id: &str,
    reservation_id: &str,
    due: i64,
) {
    record(
        pool,
        property_id,
        id,
        "guest_account",
        serde_json::json!({"name":"Synthetic Guest","guestId":guest_id,"reservationId":reservation_id,"partyId":"","state":"open","due":due,"advance":0,"advanceTax":0,"deposit":0}),
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn stay(
    pool: &DatabasePool,
    property_id: &str,
    id: &str,
    guest_id: &str,
    reservation_id: &str,
    account_id: &str,
    room_id: &str,
    arrival: &str,
    departure: &str,
    state: &str,
) {
    record(
        pool,
        property_id,
        id,
        "stay",
        serde_json::json!({
            "reservationId": reservation_id, "guestId": guest_id, "typeId": "type-1",
            "rateId": "rate-1", "roomId": room_id, "arrival": arrival, "departure": departure,
            "adults": 2, "state": state, "guestAccountId": account_id,
            "prices": [{"date":arrival,"net":100,"service":10,"tax":7,"total":117}],
            "segments": [{"roomId":room_id,"from":arrival,"to":departure}]
        }),
    )
    .await;
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
    for (id, number) in [("room-1", "101"), ("room-2", "102"), ("room-3", "103")] {
        record(
            pool,
            "property-1",
            id,
            "room",
            serde_json::json!({"number":number,"typeId":"type-1","building":"Main","floor":"1","active":true,"cleanliness":"clean","dnd":false}),
        )
        .await;
    }
    guest(pool, "property-1", "guest-1", "Arrival Guest").await;
    guest(pool, "property-1", "guest-2", "In House Guest").await;
    guest(pool, "property-1", "guest-3", "Departure Guest").await;
    reservation(pool, "property-1", "reservation-1", "R-1", "guest-1").await;
    reservation(pool, "property-1", "reservation-2", "R-2", "guest-2").await;
    reservation(pool, "property-1", "reservation-3", "R-3", "guest-3").await;
    account(
        pool,
        "property-1",
        "account-1",
        "guest-1",
        "reservation-1",
        117,
    )
    .await;
    account(
        pool,
        "property-1",
        "account-2",
        "guest-2",
        "reservation-2",
        200,
    )
    .await;
    account(
        pool,
        "property-1",
        "account-3",
        "guest-3",
        "reservation-3",
        300,
    )
    .await;
    stay(
        pool,
        "property-1",
        "stay-arrival",
        "guest-1",
        "reservation-1",
        "account-1",
        "room-1",
        "2026-11-04",
        "2026-11-06",
        "reserved",
    )
    .await;
    stay(
        pool,
        "property-1",
        "stay-inhouse",
        "guest-2",
        "reservation-2",
        "account-2",
        "room-2",
        "2026-11-02",
        "2026-11-04",
        "in_house",
    )
    .await;
    stay(
        pool,
        "property-1",
        "stay-overdue",
        "guest-3",
        "reservation-3",
        "account-3",
        "room-3",
        "2026-11-01",
        "2026-11-04",
        "in_house",
    )
    .await;
    record(
        pool,
        "property-1",
        "stay-outside",
        "stay",
        serde_json::json!({
            "reservationId": "reservation-1", "guestId": "guest-1", "typeId": "type-1",
            "rateId": "rate-1", "roomId": "room-1", "arrival": "2026-01-01",
            "departure": "2026-01-02", "adults": 1, "state": "checked_out",
            "guestAccountId": "account-1", "prices": [], "segments": []
        }),
    )
    .await;
    record(
        pool,
        "property-2",
        "stay-south",
        "stay",
        serde_json::json!({
            "reservationId": "", "guestId": "", "typeId": "", "rateId": "", "roomId": "",
            "arrival": "2026-11-04", "departure": "2026-11-05", "adults": 1,
            "state": "reserved", "guestAccountId": "", "prices": [], "segments": []
        }),
    )
    .await;
    document(pool, "property-1", "document-1", "account-1").await;
    document(pool, "property-1", "document-2", "account-2").await;
    record(
        pool,
        "property-x",
        "foreign-stay",
        "stay",
        serde_json::json!({
            "reservationId": "", "guestId": "", "typeId": "", "rateId": "", "roomId": "",
            "arrival": "2026-11-04", "departure": "2026-11-05", "adults": 1,
            "state": "reserved", "guestAccountId": "", "prices": [], "segments": []
        }),
    )
    .await;
}

fn actor(permissions: &[&str]) -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("north".into()),
        department_id: None,
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: vec!["pms".into()],
        expires_at: 9_007_199_254_740_991,
    }
}

fn selection(status: &str) -> PmsFrontDeskSelection {
    PmsFrontDeskSelection {
        property_id: Some("property-1".into()),
        status: status.into(),
        ..PmsFrontDeskSelection::default()
    }
}

struct Packages(PmsBoardEntitlement);
impl Packages {
    fn allowed() -> Self {
        Self(PmsBoardEntitlement {
            enforced: true,
            history_allowed: true,
        })
    }
}
impl PmsBoardEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsBoardEntitlement, Self::Error> {
        let value = self.0;
        Box::pin(async move { Ok(value) })
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence error: {error}"),
    }
}

#[tokio::test]
async fn arrivals_and_departures_preserve_owner_rows_and_company_branch_scope() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_front_desk_sources(&mut tx).await.unwrap();
        let mut repo = PgPmsFrontDeskRead::new(&mut tx);
        let mut packages = Packages::allowed();

        let workspace = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk", "pms.cashier"]),
            &selection("arrivals"),
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        let snapshot = workspace.snapshot.as_ref().unwrap();
        let meta = snapshot.list_meta.as_ref().unwrap();
        assert_eq!(meta.section, "front-desk");
        assert_eq!(meta.kind, "stay");
        assert_eq!(meta.ids, ["stay-arrival"]);
        assert_eq!(meta.from, "2026-10-05");
        assert_eq!(meta.to, "2027-01-03");
        assert!(!meta.has_next);
        assert!(meta.room_summary.is_none());
        let kinds: std::collections::BTreeSet<&str> = snapshot
            .records
            .iter()
            .map(|row| row.kind.as_str())
            .collect();
        for kind in [
            "stay",
            "guest",
            "reservation",
            "guest_account",
            "room",
            "room_type",
            "rate",
        ] {
            assert!(kinds.contains(kind), "missing {kind}");
        }
        assert!(!snapshot.records.iter().any(|row| row.id == "stay-south"));
        assert!(!snapshot.records.iter().any(|row| row.id == "foreign-stay"));
        let stay = snapshot
            .records
            .iter()
            .find(|row| row.id == "stay-arrival")
            .unwrap();
        assert_ne!(stay.data["prices"], serde_json::json!([]));
        let account = snapshot
            .records
            .iter()
            .find(|row| row.id == "account-1")
            .unwrap();
        assert_eq!(account.data["due"], 117);
        assert_eq!(workspace.documents.len(), 1);
        assert_eq!(workspace.documents[0]["id"], "document-1");
        assert_eq!(workspace.front_ready, Some(true));

        let departures = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk", "pms.cashier"]),
            &selection("departures"),
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(
            departures
                .snapshot
                .as_ref()
                .unwrap()
                .list_meta
                .as_ref()
                .unwrap()
                .ids,
            ["stay-overdue", "stay-inhouse"]
        );

        let all = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk", "pms.cashier"]),
            &selection(""),
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(
            all.snapshot
                .as_ref()
                .unwrap()
                .list_meta
                .as_ref()
                .unwrap()
                .ids,
            ["stay-overdue", "stay-inhouse", "stay-arrival"]
        );
        assert!(
            !all.snapshot
                .as_ref()
                .unwrap()
                .records
                .iter()
                .any(|row| row.id == "stay-outside")
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn frontdesk_without_cashier_redacts_financial_facts_and_documents() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_front_desk_sources(&mut tx).await.unwrap();
        let mut repo = PgPmsFrontDeskRead::new(&mut tx);
        let mut packages = Packages::allowed();
        let workspace = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk"]),
            &selection("arrivals"),
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        let snapshot = workspace.snapshot.as_ref().unwrap();
        let stay = snapshot
            .records
            .iter()
            .find(|row| row.id == "stay-arrival")
            .unwrap();
        assert_eq!(stay.data["prices"], serde_json::json!([]));
        let account = snapshot
            .records
            .iter()
            .find(|row| row.id == "account-1")
            .unwrap();
        assert_eq!(account.data["due"], 0);
        assert!(workspace.documents.is_empty());
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn queue_search_selected_id_and_paging_are_bounded() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        for index in 0..52 {
            let id = format!("stay-extra-{index:02}");
            record(
                &holding,
                "property-1",
                &id,
                "stay",
                serde_json::json!({
                    "reservationId": "", "guestId": "", "typeId": "", "rateId": "", "roomId": "",
                    "arrival": "2026-11-03", "departure": "2026-11-05", "adults": 1,
                    "state": "in_house", "guestAccountId": "", "prices": [], "segments": []
                }),
            )
            .await;
        }
        let mut tx = serializable(&holding).await;
        lock_front_desk_sources(&mut tx).await.unwrap();
        let mut repo = PgPmsFrontDeskRead::new(&mut tx);
        let mut packages = Packages::allowed();

        let mut first = selection("in_house");
        first.page = 0;
        let workspace = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk"]),
            &first,
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        let meta = workspace
            .snapshot
            .as_ref()
            .unwrap()
            .list_meta
            .as_ref()
            .unwrap();
        assert_eq!(meta.ids.len(), 50);
        assert!(meta.has_next);

        let mut second = first.clone();
        second.page = 1;
        let workspace = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk"]),
            &second,
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        let meta = workspace
            .snapshot
            .as_ref()
            .unwrap()
            .list_meta
            .as_ref()
            .unwrap();
        assert_eq!(meta.ids.len(), 4);
        assert!(!meta.has_next);

        let mut search = selection("arrivals");
        search.q = "R-1".into();
        let workspace = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk"]),
            &search,
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(
            workspace
                .snapshot
                .as_ref()
                .unwrap()
                .list_meta
                .as_ref()
                .unwrap()
                .ids,
            ["stay-arrival"]
        );

        let mut selected = selection("arrivals");
        selected.selected_id = Some("stay-inhouse".into());
        let workspace = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk"]),
            &selected,
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        let snapshot = workspace.snapshot.as_ref().unwrap();
        assert!(snapshot.records.iter().any(|row| row.id == "stay-inhouse"));
        assert_eq!(
            snapshot.list_meta.as_ref().unwrap().selected_id.as_deref(),
            Some("stay-inhouse")
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn foreign_scope_and_missing_property_fail_closed() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_front_desk_sources(&mut tx).await.unwrap();
        let mut repo = PgPmsFrontDeskRead::new(&mut tx);
        let mut packages = Packages::allowed();

        let mut south = actor(&["pms.read", "pms.frontdesk"]);
        south.branch_id = Some("south".into());
        let error = pms_front_desk(
            &south,
            &selection("arrivals"),
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap_err();
        let error = business(error);
        assert_eq!(error.kind, FailureKind::NotFound);
        assert_eq!(error.code, "PMS_NOT_FOUND");

        let mut south_selection = selection("arrivals");
        south_selection.property_id = Some("property-2".into());
        let workspace = pms_front_desk(&south, &south_selection, &mut repo, &mut packages, 1_000)
            .await
            .unwrap();
        assert_eq!(
            workspace
                .snapshot
                .as_ref()
                .unwrap()
                .list_meta
                .as_ref()
                .unwrap()
                .ids,
            ["stay-south"]
        );

        let mut other = actor(&["pms.read", "pms.frontdesk"]);
        other.company_id = "other".into();
        other.branch_id = Some("other".into());
        let error = pms_front_desk(
            &other,
            &selection("arrivals"),
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");

        let mut missing = selection("arrivals");
        missing.property_id = Some("missing".into());
        let error = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk"]),
            &missing,
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn role_and_package_guards_fail_before_reading() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_front_desk_sources(&mut tx).await.unwrap();
        let mut repo = PgPmsFrontDeskRead::new(&mut tx);
        let mut packages = Packages::allowed();
        let error = pms_front_desk(
            &actor(&["pms.read", "pms.reservations"]),
            &selection("arrivals"),
            &mut repo,
            &mut packages,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_FRONTDESK_REQUIRED");

        let mut denied = Packages(PmsBoardEntitlement {
            enforced: true,
            history_allowed: false,
        });
        let error = pms_front_desk(
            &actor(&["pms.read", "pms.frontdesk"]),
            &selection("arrivals"),
            &mut repo,
            &mut denied,
            1_000,
        )
        .await
        .unwrap_err();
        let error = business(error);
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(error.code, "PACKAGE_REQUIRED");
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn reads_refuse_missing_and_read_committed_transactions() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();
        let error = read_front_desk(
            &mut connection,
            "property-1",
            &selection("arrivals"),
            PmsBoardVisibility {
                guest_details: true,
                pricing: true,
                financial: true,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "PMS_BOARD_TRANSACTION_REQUIRED"
        ));
        drop(connection);

        let mut tx = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await
            .unwrap();
        let error = read_front_desk(
            &mut tx,
            "property-1",
            &selection("arrivals"),
            PmsBoardVisibility {
                guest_details: true,
                pricing: true,
                financial: true,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "PMS_BOARD_SERIALIZABLE_REQUIRED"
        ));
        tx.rollback().await.unwrap();
    })
    .await;
}
