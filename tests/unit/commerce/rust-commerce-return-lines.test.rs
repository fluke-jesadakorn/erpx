//! Focused owner test for the native Commerce return-line read.
//!
//! The lead registers this target in `modules/commerce/server/Cargo.toml`
//! (`autotests = false`); the owner unit tests live beside the implementation.

use erpx_commerce_server::public::native_return_lines::{
    CommerceReturnLineRead, ReturnLine, ReturnLineFuture, SQL_RETURN_LINES_TS, project, read_lines,
    statement,
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Probe {
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl CommerceReturnLineRead for Probe {
    fn read<'a>(
        &'a self,
        company: &'a str,
        order_id: &'a str,
    ) -> ReturnLineFuture<'a, Vec<ReturnLine>> {
        let seen = self.seen.clone();
        let company = company.to_owned();
        let order_id = order_id.to_owned();
        Box::pin(async move {
            seen.lock().expect("lock").push((company, order_id));
            Ok(Vec::new())
        })
    }
}

#[tokio::test]
async fn native_read_forwards_owner_scope_and_returns_the_envelope() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Probe { seen: seen.clone() };
    let lines = read_lines(&probe, "co-1", "order-1").await.expect("read");
    assert!(lines.is_empty());
    assert_eq!(
        *seen.lock().expect("lock"),
        vec![("co-1".to_owned(), "order-1".to_owned())]
    );
    assert_eq!(project(lines), serde_json::json!({"data": []}));
    assert!(read_lines(&probe, "bad company!", "order-1").await.is_err());
}

#[test]
fn sql_execution_contract_is_owner_scoped() {
    assert!(SQL_RETURN_LINES_TS.contains('?'));
    assert!(!statement().contains('?'));
    assert!(statement().contains("f.order_id=$1 AND o.company_id=$2"));
    assert!(statement().contains("r.status!='rejected'"));
    assert!(!statement().contains("fl.*"));
}
