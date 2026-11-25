//! Native migration lane 9, family 2: payroll payment-batch flow-routing reads.
//!
//! The axum owner executes the two Company-scoped statements with sqlx, validates
//! and shapes the response in Rust, and a strict multi-holding TS transport
//! replaces the deleted TypeScript SQL.

mod application;
mod domain;
mod http;
mod postgres;

pub use application::{BatchFuture, PaymentBatchRead};
pub use domain::{PaymentBatch, PaymentBatchDetail, valid_id};
pub use http::{
    router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgPaymentBatchRead, SQL_PAYMENT_BATCHES, SQL_PAYMENT_BATCH_BY_SOURCE};

#[cfg(test)]
mod tests {
    use super::application::{BatchFuture, PaymentBatchRead};
    use super::domain::{PaymentBatch, PaymentBatchDetail, valid_id};
    use super::http::{router_with_reader, router_with_readers};
    use crate::adapters::native_lane_9::LaneTarget;
    use erpx_server_runtime::public::http_support::serde_json::{self, json};
    use erpx_server_shared::public::{BusinessError, FailureKind};
    use sha2::Digest;
    use std::sync::{Arc, Mutex};

    const LIST: &str = r#"[{"source_id":"payroll-1","id":"batch-1","status":"exported","created_by":"user-1"}]"#;
    const DETAIL: &str = r#"{"id":"batch-1","status":"exported","created_by":"user-1"}"#;

    struct Probe {
        fail: bool,
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl PaymentBatchRead for Probe {
        fn batches<'a>(
            &'a self,
            company: &'a str,
        ) -> BatchFuture<'a, Vec<PaymentBatch>> {
            let seen = self.seen.clone();
            let fail = self.fail;
            Box::pin(async move {
                seen.lock().unwrap().push((company.to_owned(), String::new()));
                if fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PEOPLE_PAYROLL_LANE9_FLOW_TEST_UNAVAILABLE",
                        "Synthetic native failure.",
                    ));
                }
                Ok(vec![PaymentBatch {
                    source_id: "payroll-1".into(),
                    id: "batch-1".into(),
                    status: "exported".into(),
                    created_by: "user-1".into(),
                }])
            })
        }

        fn by_source<'a>(
            &'a self,
            company: &'a str,
            source: &'a str,
        ) -> BatchFuture<'a, Option<PaymentBatchDetail>> {
            let seen = self.seen.clone();
            let fail = self.fail;
            Box::pin(async move {
                seen.lock().unwrap().push((company.to_owned(), source.to_owned()));
                if fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PEOPLE_PAYROLL_LANE9_FLOW_TEST_UNAVAILABLE",
                        "Synthetic native failure.",
                    ));
                }
                Ok((source == "payroll-1").then(|| PaymentBatchDetail {
                    id: "batch-1".into(),
                    status: "exported".into(),
                    created_by: "user-1".into(),
                }))
            })
        }
    }

    #[test]
    fn identities_are_bounded() {
        assert!(valid_id("acme-co"));
        assert!(valid_id("payroll-1"));
        assert!(!valid_id(""));
        assert!(!valid_id("bad id"));
        assert!(!valid_id("x'; DROP TABLE hr_payment_batches --"));
    }

    #[test]
    fn response_shapes_match_the_client_contract_oracle() {
        let list = serde_json::to_string(&json!({"batches": serde_json::from_str::<serde_json::Value>(LIST).unwrap()})).unwrap();
        assert_eq!(
            sha2::Sha256::digest(list.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "0b8fcb3eb183bb535b183dcd6022793405fee2f790d497f2aada8797da45f1f5"
        );
        let detail = serde_json::to_string(&json!({"batch": serde_json::from_str::<serde_json::Value>(DETAIL).unwrap()})).unwrap();
        assert_eq!(
            sha2::Sha256::digest(detail.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "46c9321269d025f43d575b0436f1bc4101166064b6761b9bd17b1c627f8a9583"
        );
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let reader: Arc<dyn PaymentBatchRead> = Arc::new(Probe {
            fail: false,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let target = || LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(
            reader,
            vec![LaneTarget { holding_slug: "Bad_Slug".into(), ..target() }]
        )
        .is_err());
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let seen_a = Arc::new(Mutex::new(Vec::new()));
        let seen_b = Arc::new(Mutex::new(Vec::new()));
        let reader_a: Arc<dyn PaymentBatchRead> = Arc::new(Probe { fail: false, seen: seen_a.clone() });
        let reader_b: Arc<dyn PaymentBatchRead> = Arc::new(Probe { fail: true, seen: seen_b.clone() });
        let router = router_with_readers(vec![
            (LaneTarget { host: "acme.example.test".into(), holding_slug: "acme".into() }, reader_a),
            (LaneTarget { host: "northstar.example.test".into(), holding_slug: "northstar".into() }, reader_b),
        ])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, path: &str| {
            format!(
                "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
                 x-erpx-context-company: acme-co\r\nx-erpx-context-source: payroll-1\r\n\r\n"
            )
        };
        let (status, body) = http(
            addr,
            get("acme.example.test", "/api/v1/holdings/acme/people-payroll/payment-batches"),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["batches"][0]["source_id"], "payroll-1");
        let (status, body) = http(
            addr,
            get("northstar.example.test", "/api/v1/holdings/northstar/people-payroll/payment-batch"),
        )
        .await;
        assert_eq!(status, 503);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE9_FLOW_TEST_UNAVAILABLE");
        assert_eq!(seen_a.lock().unwrap().len(), 1);
        assert_eq!(seen_b.lock().unwrap().len(), 1);
        server.abort();
    }
}
