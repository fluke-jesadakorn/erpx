//! Native migration lane 9 for people_payroll.
//!
//! Owner-local migration of the live `projectWorkerPersonImage` read: the axum
//! router executes the Company-scoped statement with sqlx, validates and shapes
//! the response in Rust, and a strict TS transport replaces the deleted TS SQL.

mod application;
mod domain;
pub mod flow_routing;
mod http;
pub mod person_route;
mod postgres;

pub use application::{PersonImageFuture, PersonImageRead};
pub use domain::{PersonImageFacts, PersonImageRequest};
pub use http::{
    LaneTarget, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgPersonImageRead, SQL_PERSON_IMAGE};

#[cfg(test)]
mod tests {
    use super::application::{PersonImageFuture, PersonImageRead, project};
    use super::domain::{IMAGE_KEYS, PersonImageFacts, PersonImageRequest, validate_image, valid_id};
    use super::http::{LaneTarget, router_with_reader, router_with_readers};
    use erpx_server_runtime::public::http_support::serde_json::{self, json};
    use erpx_server_shared::public::{BusinessError, FailureKind};
    use sha2::Digest;
    use std::sync::{Arc, Mutex};

    const IMAGE: &str = r#"{"person":{"id":"person-1"},"engagements":[],"employees":[],"cases":[],"tasks":[],"compensation":[],"leave":[]}"#;

    struct Probe {
        image: Option<String>,
        fail: bool,
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl PersonImageRead for Probe {
        fn image<'a>(
            &'a self,
            company: &'a str,
            person: &'a str,
        ) -> PersonImageFuture<'a, Option<PersonImageFacts>> {
            let seen = self.seen.clone();
            let image = self.image.clone();
            let fail = self.fail;
            Box::pin(async move {
                seen.lock().unwrap().push((company.to_owned(), person.to_owned()));
                if fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PEOPLE_PAYROLL_LANE9_TEST_UNAVAILABLE",
                        "Synthetic native failure.",
                    ));
                }
                Ok(image.map(|image| PersonImageFacts { image }))
            })
        }
    }

    #[test]
    fn identity_and_request_are_bounded() {
        assert!(valid_id("person-1"));
        assert!(valid_id("comp_1.acme:worker"));
        assert!(!valid_id(""));
        assert!(!valid_id("a b"));
        assert!(!valid_id("x'; DROP TABLE hr_people --"));
        assert!(PersonImageRequest::new("comp_1".into(), "person-1".into())
            .validate()
            .is_ok());
        assert!(PersonImageRequest::new("bad id".into(), "person-1".into())
            .validate()
            .is_err());
    }

    #[test]
    fn projection_pins_response_shape_and_fails_closed() {
        let facts = project(Some(IMAGE.to_owned())).unwrap().unwrap();
        assert_eq!(facts.image, IMAGE);
        let value: serde_json::Value = serde_json::from_str(&facts.image).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), IMAGE_KEYS.len());
        for key in IMAGE_KEYS {
            assert!(object.contains_key(key));
        }
        assert!(project(None).unwrap().is_none());
        assert!(validate_image("not-json").is_err());
        assert!(validate_image(&json!({"person": null}).to_string()).is_err());
        let mut bad = serde_json::from_str::<serde_json::Value>(IMAGE).unwrap();
        bad.as_object_mut().unwrap().remove("leave");
        assert!(validate_image(&bad.to_string()).is_err());
    }

    #[test]
    fn response_payload_matches_the_client_contract_oracle() {
        let body = serde_json::to_string(&json!({ "personImage": IMAGE })).expect("serialize");
        assert_eq!(body, "{\"personImage\":\"{\\\"person\\\":{\\\"id\\\":\\\"person-1\\\"},\\\"engagements\\\":[],\\\"employees\\\":[],\\\"cases\\\":[],\\\"tasks\\\":[],\\\"compensation\\\":[],\\\"leave\\\":[]}\"}");
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "5ef5ac117da385b5f86d249033449b7fff1483f9317ef0378804a993b595b81c",
            "update this oracle in Rust and in tests/unit/people_payroll/native-lane-9.test.ts"
        );
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let reader: Arc<dyn PersonImageRead> = Arc::new(Probe {
            image: Some(IMAGE.to_owned()),
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
            vec![LaneTarget {
                holding_slug: "Bad_Slug".into(),
                ..target()
            }]
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
    async fn per_holding_readers_are_distinct() {
        // Reader injection is the testable equivalent of `router_for_holdings`:
        // each target/pool pair becomes one independent reader in the state map.
        let image_a = IMAGE.to_owned();
        let image_b = IMAGE.replace("person-1", "person-2");
        assert_ne!(image_a, image_b);
        let seen_a = Arc::new(Mutex::new(Vec::new()));
        let seen_b = Arc::new(Mutex::new(Vec::new()));
        let reader_a: Arc<dyn PersonImageRead> = Arc::new(Probe {
            image: Some(image_a.clone()),
            fail: false,
            seen: seen_a.clone(),
        });
        let reader_b: Arc<dyn PersonImageRead> = Arc::new(Probe {
            image: Some(image_b.clone()),
            fail: false,
            seen: seen_b.clone(),
        });
        let router = router_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                },
                reader_a,
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                },
                reader_b,
            ),
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
                 x-erpx-context-company: comp_1\r\nx-erpx-context-person: person-1\r\n\r\n"
            )
        };
        let (status, body) = http(
            addr,
            get(
                "acme.example.test",
                "/api/v1/holdings/acme/people-payroll/person-image",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["personImage"], json!(image_a));
        let (status, body) = http(
            addr,
            get(
                "northstar.example.test",
                "/api/v1/holdings/northstar/people-payroll/person-image",
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["personImage"], json!(image_b));
        let (status, _) = http(
            addr,
            get(
                "acme.example.test",
                "/api/v1/holdings/other/people-payroll/person-image",
            ),
        )
        .await;
        assert_eq!(status, 404);
        server.abort();
        assert_eq!(
            seen_a.lock().unwrap().clone(),
            vec![("comp_1".to_owned(), "person-1".to_owned())]
        );
        assert_eq!(
            seen_b.lock().unwrap().clone(),
            vec![("comp_1".to_owned(), "person-1".to_owned())]
        );
        // A request to an unregistered Holding under a registered Host never
        // reaches either reader.
        assert_eq!(seen_a.lock().unwrap().len(), 1);
        assert_eq!(seen_b.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn router_scopes_company_and_person_and_maps_errors() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe: Arc<dyn PersonImageRead> = Arc::new(Probe {
            image: Some(IMAGE.to_owned()),
            fail: false,
            seen: seen.clone(),
        });
        let failing: Arc<dyn PersonImageRead> = Arc::new(Probe {
            image: None,
            fail: true,
            seen: seen.clone(),
        });
        let router = router_with_reader(
            probe,
            vec![LaneTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
            }],
        )
        .unwrap();
        let failing_router = router_with_reader(
            failing,
            vec![LaneTarget {
                host: "fail.example.test".into(),
                holding_slug: "fail".into(),
            }],
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let get = |path: &str| {
            format!(
                "GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\
                 x-erpx-context-company: comp_1\r\nx-erpx-context-person: person-1\r\n\r\n"
            )
        };
        let (status, body) = http(
            addr,
            get("/api/v1/holdings/acme/people-payroll/person-image"),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["personImage"], json!(IMAGE));
        let (status, _) = http(addr, get("/api/v1/holdings/other/people-payroll/person-image")).await;
        assert_eq!(status, 404);
        let (status, body) = http(
            addr,
            "GET /api/v1/holdings/acme/people-payroll/person-image HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\nx-erpx-context-company: bad id\r\nx-erpx-context-person: person-1\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE9_REQUEST_INVALID");
        assert_eq!(
            seen.lock().unwrap().last().cloned(),
            Some(("comp_1".to_owned(), "person-1".to_owned()))
        );
        server.abort();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, failing_router).await;
        });
        let (status, body) = http(
            addr,
            "GET /api/v1/holdings/fail/people-payroll/person-image HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\nx-erpx-context-company: comp_1\r\nx-erpx-context-person: person-1\r\n\r\n".into(),
        )
        .await;
        assert_eq!(status, 503);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE9_TEST_UNAVAILABLE");
        server.abort();
    }
}
