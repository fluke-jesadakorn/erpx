//! Focused owner test for the native Commerce employee-request order read.
//!
//! The lead registers this target in `modules/commerce/server/Cargo.toml`
//! (`autotests = false`); the owner unit tests live beside the implementation.

use erpx_commerce_server::public::native_employee_request_order::{
    CommerceEmployeeOrderRead, EmployeeOrder, EmployeeOrderFuture, SQL_EMPLOYEE_ORDER_TS, project,
    read_order, statement,
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Probe {
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl CommerceEmployeeOrderRead for Probe {
    fn read<'a>(
        &'a self,
        company: &'a str,
        order_id: &'a str,
    ) -> EmployeeOrderFuture<'a, Option<EmployeeOrder>> {
        let seen = self.seen.clone();
        let company = company.to_owned();
        let order_id = order_id.to_owned();
        Box::pin(async move {
            seen.lock().expect("lock").push((company, order_id));
            Ok(Some(EmployeeOrder {
                kind: "purchase".to_owned(),
                company_id: "co-1".to_owned(),
                branch_id: Some("br-1".to_owned()),
                department_id: None,
                state: "approved".to_owned(),
                version: 3,
            }))
        })
    }
}

#[tokio::test]
async fn native_read_forwards_owner_scope_and_returns_the_complete_payload() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Probe { seen: seen.clone() };
    let order = read_order(&probe, "co-1", "order-1")
        .await
        .expect("read")
        .expect("order");
    assert_eq!(
        *seen.lock().expect("lock"),
        vec![("co-1".to_owned(), "order-1".to_owned())]
    );
    let value = project(Some(order));
    assert_eq!(value.as_object().expect("object").len(), 1);
    assert_eq!(value["data"]["kind"], "purchase");
    assert_eq!(value["data"]["company_id"], "co-1");
    assert_eq!(value["data"]["branch_id"], "br-1");
    assert!(value["data"]["department_id"].is_null());
    assert_eq!(value["data"]["state"], "approved");
    assert_eq!(value["data"]["version"], 3);
    assert_eq!(value["data"].as_object().expect("row").len(), 6);
    assert_eq!(project(None), serde_json::json!({"data": null}));
    assert!(read_order(&probe, "bad company!", "order-1").await.is_err());
    assert!(read_order(&probe, "co-1", "bad id!").await.is_err());
}

#[test]
fn sql_execution_contract_is_owner_scoped_and_parity_exact() {
    assert!(SQL_EMPLOYEE_ORDER_TS.contains('?'));
    assert!(!statement().contains('?'));
    assert_eq!(
        statement().replace("$1", "?").replace("$2", "?"),
        SQL_EMPLOYEE_ORDER_TS
    );
    assert!(statement().starts_with("SELECT kind,company_id"));
    assert!(statement().contains("id=$1 AND company_id=$2"));
    for column in [
        "kind",
        "company_id",
        "branch_id",
        "department_id",
        "state",
        "version",
    ] {
        assert!(statement().contains(column), "missing {column}");
    }
    assert!(!statement().contains("FOR UPDATE"));
    assert!(!statement().contains("LOCK TABLE"));
}
