//! Focused owner tests for the native POS customer-search read.
//!
//! The tests pin the exact retired wire payload, the authority/package gate and
//! the real owner SQL shape. They exercise the public owner port without a
//! socket; mounting the router into composition is the lead's integration step.
use erpx_server_pos::public::native_customer_search::{
    Bind, CUSTOMER_SEARCH_SQL, CustomerActorDecision, CustomerFuture, CustomerPackageDecision,
    CustomerRow, CustomerSearchQuery, PosCustomerReader, ROW_KEYS, render, search, valid_row,
    validate,
};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::BusinessError;
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

fn row() -> CustomerRow {
    CustomerRow {
        id: "party1".into(),
        name: "Acme".into(),
        phone: Some("0812345678".into()),
        notification_email: None,
        address_json: "{\"line1\":\"1 Main\"}".into(),
        version: 2,
    }
}

fn query() -> CustomerSearchQuery {
    CustomerSearchQuery {
        company: "co".into(),
        q: "acme".into(),
        actor: CustomerActorDecision {
            read: true,
            department: false,
        },
        package: CustomerPackageDecision {
            enforced: false,
            history: false,
        },
    }
}

struct Probe {
    result: Result<Vec<CustomerRow>, BusinessError>,
}

impl PosCustomerReader for Probe {
    fn rows<'a>(&'a self, _: &'a CustomerSearchQuery) -> CustomerFuture<'a, Vec<CustomerRow>> {
        Box::pin(async move { self.result.clone() })
    }
}

#[test]
fn pins_exact_retired_payload() {
    let reader = Probe {
        result: Ok(vec![row()]),
    };
    let rows = block_on(search(&reader, &query())).unwrap();
    assert_eq!(rows.len(), 1);
    let object = rows[0].as_object().unwrap();
    assert_eq!(object.len(), ROW_KEYS.len());
    for key in ROW_KEYS {
        assert!(object.contains_key(key), "missing payload field: {key}");
    }
    assert_eq!(rows[0]["id"], "party1");
    assert_eq!(rows[0]["version"], 2);
    assert_eq!(rows[0]["phone"], "0812345678");
    assert_eq!(rows[0]["notification_email"], Value::Null);
    assert_eq!(rows[0]["address_json"], "{\"line1\":\"1 Main\"}");
}

#[test]
fn denies_authority_and_missing_package_before_read() {
    let reader = Probe {
        result: Ok(vec![row()]),
    };
    let mut department = query();
    department.actor.department = true;
    let error = block_on(search(&reader, &department)).unwrap_err();
    assert_eq!(error.code, "POS_BRANCH_REQUIRED");
    assert_eq!(error.message, "Select a Company or Branch");

    let mut denied = query();
    denied.actor.read = false;
    let error = block_on(search(&reader, &denied)).unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(error.message, "Your selected role does not grant this permission.");

    let gated = CustomerSearchQuery {
        package: CustomerPackageDecision {
            enforced: true,
            history: false,
        },
        ..query()
    };
    let error = block_on(search(&reader, &gated)).unwrap_err();
    assert_eq!(error.code, "PACKAGE_REQUIRED");
    assert_eq!(error.message, "This package does not permit the requested work.");

    let purchased = CustomerSearchQuery {
        package: CustomerPackageDecision {
            enforced: true,
            history: true,
        },
        ..query()
    };
    assert_eq!(block_on(search(&reader, &purchased)).unwrap().len(), 1);
}

#[test]
fn owner_sql_is_read_only_and_render_is_byte_stable() {
    assert!(CUSTOMER_SEARCH_SQL.starts_with("SELECT id,name,phone,notification_email,"));
    assert_eq!(CUSTOMER_SEARCH_SQL.matches('?').count(), 5);
    assert!(CUSTOMER_SEARCH_SQL.contains("kind='customer' AND status='active'"));
    assert!(CUSTOMER_SEARCH_SQL.contains(
        "(?='' OR name ILIKE ? OR phone ILIKE ? OR notification_email ILIKE ?)"
    ));
    assert!(CUSTOMER_SEARCH_SQL.contains("ORDER BY name,id LIMIT 100"));
    for forbidden in [
        "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
        "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "identity_access",
    ] {
        assert!(
            !CUSTOMER_SEARCH_SQL.contains(forbidden),
            "customer-search SQL must not mutate, lock or cross owners: {forbidden}"
        );
    }

    let (sql, binds) = render(&query());
    assert!(sql.starts_with("SELECT id::text AS id,name::text AS name,"));
    assert!(sql.contains(" FROM (SELECT id,name,phone,notification_email,"));
    assert_eq!(binds.len(), 5);
    assert_eq!(binds[0], Bind::Text("co".into()));
    assert_eq!(binds[1], Bind::Text("acme".into()));
    assert_eq!(binds[2], Bind::Text("%acme%".into()));
    assert_eq!(binds[3], binds[2]);
    assert_eq!(binds[4], binds[2]);

    let escaped = render(&CustomerSearchQuery {
        q: " 50%_a\\b ".into(),
        ..query()
    });
    assert_eq!(escaped.1[1], Bind::Text("50%_a\\b".into()));
    assert_eq!(escaped.1[2], Bind::Text("%50\\%\\_a\\\\b%".into()));
}

#[test]
fn validates_scope_and_rows() {
    assert!(validate(&query()).is_ok());
    let mut bad = query();
    bad.company.clear();
    assert!(validate(&bad).is_err());
    assert!(valid_row(&row()));
    let mut bad = row();
    bad.version = 0;
    assert!(!valid_row(&bad));
}
