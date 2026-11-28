//! Focused owner tests for the native POS room-charge reversal queue read.
//!
//! The tests pin the exact retired wire payload, the authority/package gate and
//! the real owner SQL shape. They exercise the public owner port without a
//! socket; mounting the router into composition is the lead's integration step.
use erpx_pos_server::public::native_room_charges::{
    ActorDecision, Bind, PackageDecision, PosRoomChargeReader, ROOM_CHARGE_SQL, ROW_KEYS,
    RoomChargeFuture, RoomChargeQuery, RoomChargeRow, read, render, valid_row, validate,
};
use erpx_platform_runtime_server::public::http_support::serde_json::Value;
use erpx_platform_shared_server::public::BusinessError;
use std::task::{Context, Poll, Waker};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn row() -> RoomChargeRow {
    RoomChargeRow {
        id: "rc1".into(),
        company_id: "co".into(),
        branch_id: "br".into(),
        sale_id: "sale".into(),
        property_id: "prop".into(),
        sale_version: 2,
        property_version: 3,
        restock_line_ids_json: Some("[\"l1\"]".into()),
        number: "POS-0001".into(),
        state: "requested".into(),
        version: 1,
        reason: "Wrong room".into(),
        amount_minor: 10_000,
        requested_by: "requester".into(),
        pos_approved_by: None,
        created_by: "cashier".into(),
        charged_by: None,
        updated_at: 1_700_000_000_000,
        source_current: true,
    }
}

fn query() -> RoomChargeQuery {
    RoomChargeQuery {
        company: "co".into(),
        branch: Some("br".into()),
        property: Some("prop".into()),
        actor: ActorDecision {
            pos_approve: true,
            pms_approve: false,
            department: false,
            self_ids: Vec::new(),
        },
        package: PackageDecision {
            enforced: false,
            history: false,
            settle_existing: false,
        },
    }
}

struct Probe {
    rows: Vec<RoomChargeRow>,
}

impl PosRoomChargeReader for Probe {
    fn rows<'a>(&'a self, _: &'a RoomChargeQuery) -> RoomChargeFuture<'a, Vec<RoomChargeRow>> {
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
}

#[test]
fn native_payload_matches_the_retired_row_field_set() {
    let reader = Probe {
        rows: vec![row()],
    };
    let rows = block_on(read(&reader, &query())).unwrap();
    assert_eq!(rows.len(), 1);
    let object = rows[0].as_object().unwrap();
    assert_eq!(object.len(), ROW_KEYS.len());
    for key in ROW_KEYS {
        assert!(object.contains_key(key), "missing field: {key}");
    }
    assert_eq!(rows[0]["number"], "POS-0001");
    assert_eq!(rows[0]["amount_minor"], 10_000);
    assert_eq!(rows[0]["sale_version"], 2);
    assert_eq!(rows[0]["restock_line_ids_json"], "[\"l1\"]");
    assert_eq!(rows[0]["pos_approved_by"], Value::Null);
    assert_eq!(rows[0]["source_current"], true);
    assert_eq!(rows[0]["stage"], "pos");
    assert_eq!(rows[0]["canAct"], true);
    assert_eq!(rows[0]["canApprove"], true);
    assert_eq!(rows[0]["reasonDisabled"], Value::Null);
}

#[test]
fn same_person_and_stale_source_gates_match_the_retained_rules() {
    let mut same = row();
    same.charged_by = Some("cashier".into());
    let reader = Probe { rows: vec![same] };
    let mut q = query();
    q.actor.self_ids = vec!["cashier".into()];
    let rows = block_on(read(&reader, &q)).unwrap();
    assert_eq!(rows[0]["canAct"], false);
    assert_eq!(
        rows[0]["reasonDisabled"],
        "A different person must review this request."
    );

    let mut stale = row();
    stale.source_current = false;
    let reader = Probe { rows: vec![stale] };
    let rows = block_on(read(&reader, &query())).unwrap();
    assert_eq!(rows[0]["canAct"], true);
    assert_eq!(rows[0]["canApprove"], false);
    assert_eq!(
        rows[0]["reasonDisabled"],
        "The source changed. Reject this request and request again."
    );
}

#[test]
fn denied_authority_and_missing_purchase_history_fail_before_the_reader() {
    let reader = Probe { rows: vec![row()] };
    let mut denied = query();
    denied.actor.department = true;
    assert_eq!(
        block_on(read(&reader, &denied)).unwrap_err().code,
        "POS_BRANCH_REQUIRED"
    );

    let mut denied = query();
    denied.actor.pos_approve = false;
    assert_eq!(
        block_on(read(&reader, &denied)).unwrap_err().code,
        "PERMISSION_REQUIRED"
    );

    let gated = RoomChargeQuery {
        package: PackageDecision {
            enforced: true,
            history: false,
            settle_existing: true,
        },
        ..query()
    };
    assert!(block_on(read(&reader, &gated)).unwrap().is_empty());
}

#[test]
fn owner_sql_is_lock_free_and_binds_in_the_retired_order() {
    assert!(ROOM_CHARGE_SQL.starts_with("SELECT q.*,s.number,s.created_by,"));
    assert_eq!(ROOM_CHARGE_SQL.matches('?').count(), 5);
    for forbidden in [
        "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
        "LOCK TABLE", "pg_advisory", "finance_", "inventory_", "identity_access",
    ] {
        assert!(!ROOM_CHARGE_SQL.contains(forbidden), "owner SQL leaked: {forbidden}");
    }
    let (sql, binds) = render(&query());
    assert!(sql.contains(" FROM (SELECT q.*,s.number,s.created_by,"));
    assert_eq!(
        binds,
        vec![
            Bind::Text("co".into()),
            Bind::Opt(Some("br".into())),
            Bind::Opt(Some("br".into())),
            Bind::Opt(Some("prop".into())),
            Bind::Opt(Some("prop".into())),
        ]
    );
    assert!(validate(&query()).is_ok());
    assert!(valid_row(&row()));
    let mut bad = query();
    bad.company.clear();
    assert!(validate(&bad).is_err());
}

// Keep the public BusinessError type referenced so the failure path is typed
// by the owner contract rather than by an internal error enum.
#[allow(dead_code)]
fn _error_type(error: BusinessError) -> String {
    error.code.to_string()
}
