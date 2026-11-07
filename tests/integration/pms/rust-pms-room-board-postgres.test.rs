//! Native PMS room-board reads over real migrated PostgreSQL. These are owner
//! read projections, not commands, source purchase or browser evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_pms::public::room_board::*;
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
    for (id, number, cleanliness, active, dnd) in [
        ("room-1", "101", "dirty", true, true),
        ("room-2", "102", "clean", true, false),
        ("room-3", "103", "inspected", false, false),
    ] {
        record(
            pool,
            "property-1",
            id,
            "room",
            serde_json::json!({
                "number": number,
                "typeId": "type-1",
                "building": "Main",
                "floor": "1",
                "active": active,
                "cleanliness": cleanliness,
                "dnd": dnd,
                "observedOccupied": null
            }),
        )
        .await;
    }
    record(
        pool,
        "property-1",
        "stay-1",
        "stay",
        serde_json::json!({
            "reservationId": "reservation-1", "guestId": "guest-1", "typeId": "type-1",
            "rateId": "rate-1", "roomId": "room-1", "arrival": "2026-11-04",
            "departure": "2026-11-06", "adults": 2, "state": "in_house",
            "guestAccountId": "account-1",
            "prices": [{"date":"2026-11-04","net":100,"service":10,"tax":7,"total":117}],
            "segments": [{"roomId":"room-1","from":"2026-11-04","to":"2026-11-06"}]
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "task-1",
        "task",
        serde_json::json!({
            "roomId": "room-1", "stayId": "stay-1", "type": "departure",
            "date": "2026-11-04", "assigneeId": "staff-1", "priority": "urgent",
            "state": "pending", "checklist": ["Clean"], "checked": [], "notes": "",
            "completedBy": "", "inspectedBy": ""
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "task-done",
        "task",
        serde_json::json!({
            "roomId": "room-2", "type": "stayover", "date": "2026-11-03",
            "assigneeId": "staff-1", "priority": "normal", "state": "done",
            "checklist": [], "checked": [], "notes": "", "completedBy": "staff-1",
            "inspectedBy": "staff-2"
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "block-1",
        "block",
        serde_json::json!({
            "name": "Maintenance", "typeId": "type-1", "roomId": "room-1",
            "from": "2026-11-04", "to": "2026-11-05", "quantity": 1,
            "releaseDate": "", "state": "active", "kind": "maintenance",
            "pickedStayIds": []
        }),
    )
    .await;
    record(
        pool,
        "property-1",
        "guest-1",
        "guest",
        serde_json::json!({"name":"Synthetic Guest","email":"","phone":"","nationality":"","documentRef":"","notes":""}),
    )
    .await;
    record(
        pool,
        "property-1",
        "reservation-1",
        "reservation",
        serde_json::json!({"number":"R-1","guestId":"guest-1","source":"direct","externalRef":"","groupName":"","notes":"","state":"confirmed","createdBy":"maker"}),
    )
    .await;
    record(
        pool,
        "property-1",
        "account-1",
        "guest_account",
        serde_json::json!({"name":"Synthetic Guest","guestId":"guest-1","reservationId":"reservation-1","partyId":"","state":"open","due":117,"advance":0,"advanceTax":0,"deposit":0}),
    )
    .await;
    record(
        pool,
        "property-2",
        "foreign-room",
        "room",
        serde_json::json!({"number":"201","typeId":"type-1","building":"South","floor":"1","active":true,"cleanliness":"dirty","dnd":false}),
    )
    .await;
}

fn actor() -> PmsBoardActor {
    PmsBoardActor {
        user_id: "user".into(),
        company_id: "company".into(),
        branch_id: Some("north".into()),
        department_id: None,
        permissions: vec!["pms.read".into(), "pms.housekeeping".into()],
        modules: vec!["pms".into()],
        expires_at: 9_007_199_254_740_991,
    }
}

fn selection() -> PmsBoardSelection {
    PmsBoardSelection {
        section: PmsBoardSection::Rooms,
        property_id: Some("property-1".into()),
        ..PmsBoardSelection::default()
    }
}

struct Packages;
impl PmsBoardEntitlementPort for Packages {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> PmsBoardFuture<'a, PmsBoardEntitlement, Self::Error> {
        Box::pin(async {
            Ok(PmsBoardEntitlement {
                enforced: true,
                history_allowed: true,
            })
        })
    }
}

struct Staff;
impl PmsStaffDirectory for Staff {
    type Error = BusinessError;
    fn staff<'a>(
        &'a mut self,
        _company_id: &'a str,
        _branch_id: Option<&'a str>,
    ) -> PmsBoardFuture<'a, Vec<PmsStaff>, Self::Error> {
        Box::pin(async {
            Ok(vec![PmsStaff {
                id: "staff-1".into(),
                name: "Synthetic Worker".into(),
            }])
        })
    }
}

#[tokio::test]
async fn board_read_is_company_branch_scoped_and_preserves_owner_rows() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_room_board_sources(&mut tx).await.unwrap();
        let properties = read_room_properties(&mut tx, "company", Some("north"))
            .await
            .unwrap();
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].id, "property-1");

        let mut repo = PgPmsBoardRead::new(&mut tx);
        let mut staff = Staff;
        let mut packages = Packages;
        let workspace = pms_room_board(
            &actor(),
            &selection(),
            &mut repo,
            &mut staff,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        let snapshot = workspace.snapshot.as_ref().unwrap();
        assert_eq!(snapshot.property.id, "property-1");
        assert_eq!(snapshot.property.config["chartId"], "");
        assert!(snapshot.list_meta.is_some());
        let kinds: std::collections::BTreeSet<&str> = snapshot
            .records
            .iter()
            .map(|row| row.kind.as_str())
            .collect();
        assert!(kinds.contains("room"));
        assert!(kinds.contains("room_type"));
        assert!(kinds.contains("stay"));
        assert!(kinds.contains("task"));
        assert!(kinds.contains("block"));
        assert!(!kinds.contains("rate"));
        assert!(!kinds.contains("guest"));
        let summary = snapshot
            .list_meta
            .as_ref()
            .unwrap()
            .room_summary
            .as_ref()
            .unwrap();
        assert_eq!(
            (
                summary.total,
                summary.active,
                summary.dirty,
                summary.clean,
                summary.inspected,
                summary.dnd,
                summary.occupied
            ),
            (3, 2, 1, 1, 1, 1, 1)
        );
        let stay = snapshot
            .records
            .iter()
            .find(|row| row.kind == "stay")
            .unwrap();
        assert_eq!(stay.data["prices"], serde_json::json!([]));
        assert_eq!(workspace.staff[0].id, "staff-1");
        assert!(workspace.readiness.is_empty(), "{:?}", workspace.readiness);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn filters_pagination_selected_id_and_default_range_are_bounded() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        for index in 200..252 {
            record(
                &holding,
                "property-1",
                &format!("room-extra-{index}"),
                "room",
                serde_json::json!({"number":index.to_string(),"typeId":"type-1","building":"Main","floor":"2","active":true,"cleanliness":"clean","dnd":false}),
            )
            .await;
        }
        let mut tx = serializable(&holding).await;
        lock_room_board_sources(&mut tx).await.unwrap();
        let mut repo = PgPmsBoardRead::new(&mut tx);
        let mut staff = Staff;
        let mut packages = Packages;

        let mut dirty = selection();
        dirty.status = "dirty".into();
        dirty.page = 0;
        let workspace = pms_room_board(&actor(), &dirty, &mut repo, &mut staff, &mut packages, 1_000)
            .await
            .unwrap();
        let meta = workspace.snapshot.as_ref().unwrap().list_meta.as_ref().unwrap();
        assert_eq!(meta.ids, ["room-1"]);
        assert_eq!(meta.from, "2026-10-05");
        assert_eq!(meta.to, "2027-01-03");
        assert!(!meta.has_next);

        let mut query = selection();
        query.q = "102".into();
        let workspace = pms_room_board(&actor(), &query, &mut repo, &mut staff, &mut packages, 1_000)
            .await
            .unwrap();
        assert_eq!(
            workspace.snapshot.as_ref().unwrap().list_meta.as_ref().unwrap().ids,
            ["room-2"]
        );

        let mut selected = selection();
        selected.selected_id = Some("room-2".into());
        let workspace = pms_room_board(&actor(), &selected, &mut repo, &mut staff, &mut packages, 1_000)
            .await
            .unwrap();
        assert_eq!(
            workspace.snapshot.as_ref().unwrap().list_meta.as_ref().unwrap().selected_id.as_deref(),
            Some("room-2")
        );

        let first = pms_room_board(&actor(), &selection(), &mut repo, &mut staff, &mut packages, 1_000)
            .await
            .unwrap();
        let meta = first.snapshot.as_ref().unwrap().list_meta.as_ref().unwrap();
        assert_eq!(meta.ids.len(), 50);
        assert!(meta.has_next);
        let mut second = selection();
        second.page = 1;
        let second = pms_room_board(&actor(), &second, &mut repo, &mut staff, &mut packages, 1_000)
            .await
            .unwrap();
        let meta = second.snapshot.as_ref().unwrap().list_meta.as_ref().unwrap();
        assert_eq!(meta.ids.len(), 5);
        assert!(!meta.has_next);
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn foreign_company_branch_and_missing_property_fail_closed() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_room_board_sources(&mut tx).await.unwrap();
        let mut repo = PgPmsBoardRead::new(&mut tx);
        let mut staff = Staff;
        let mut packages = Packages;

        let mut south = actor();
        south.branch_id = Some("south".into());
        let mut north_property = selection();
        let error = pms_room_board(
            &south,
            &north_property,
            &mut repo,
            &mut staff,
            &mut packages,
            1_000,
        )
        .await
        .unwrap_err();
        let error = business(error);
        assert_eq!(error.kind, FailureKind::NotFound);
        assert_eq!(error.code, "PMS_NOT_FOUND");

        let workspace = pms_room_board(
            &south,
            &south_property(&south),
            &mut repo,
            &mut staff,
            &mut packages,
            1_000,
        )
        .await
        .unwrap();
        assert_eq!(
            workspace.snapshot.as_ref().unwrap().property.id,
            "property-2"
        );
        assert_eq!(
            workspace
                .snapshot
                .as_ref()
                .unwrap()
                .list_meta
                .as_ref()
                .unwrap()
                .ids,
            ["foreign-room"]
        );

        let mut other = actor();
        other.company_id = "other".into();
        other.branch_id = Some("other".into());
        let mut other_property = selection();
        other_property.property_id = Some("property-1".into());
        let error = pms_room_board(
            &other,
            &other_property,
            &mut repo,
            &mut staff,
            &mut packages,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(business(error).code, "PMS_NOT_FOUND");

        north_property.property_id = Some("missing".into());
        let error = pms_room_board(
            &actor(),
            &north_property,
            &mut repo,
            &mut staff,
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

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence error: {error}"),
    }
}

fn south_property(actor: &PmsBoardActor) -> PmsBoardSelection {
    PmsBoardSelection {
        section: PmsBoardSection::Rooms,
        property_id: actor.branch_id.as_ref().map(|_| "property-2".to_owned()),
        ..PmsBoardSelection::default()
    }
}

#[tokio::test]
async fn reads_refuse_missing_and_read_committed_transactions() {
    fixture::with_database(2, |holding| async move {
        seed(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();
        let error = read_room_properties(&mut connection, "company", Some("north"))
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
        let error = read_room_properties(&mut tx, "company", Some("north"))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Persistence(ref value)
                if value.code == "PMS_BOARD_SERIALIZABLE_REQUIRED"
        ));
        tx.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        let error = read_room_properties(&mut tx, "company", Some("north")).await;
        assert!(error.is_ok());
        tx.rollback().await.unwrap();
    })
    .await;
}
