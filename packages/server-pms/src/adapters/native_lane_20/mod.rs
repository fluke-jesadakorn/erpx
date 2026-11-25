//! Native migration lane 20 for pms: owner-local property-readiness read.
//!
//! The TypeScript `setupFacts` / `hasStays` statements in
//! `adapters/pms/read-repository.ts` are replaced by a thin strict client to
//! this axum router. The owner crate executes the SQL with sqlx, enforces
//! Company scope, projects the bounded boolean facts and owns the response.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{PropertyReadinessFuture, PropertyReadinessRead};
pub use domain::PropertyReadinessFacts;
pub use http::{
    LaneTarget, ProbePropertyReadinessRead, router, router_with_reader, unavailable,
};
pub use postgres::{PgPropertyReadinessRead, PROPERTY_SCOPE_SQL, READINESS_SQL};

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serve;
    use erpx_server_shared::public::BusinessError;
    use sha2::{Digest, Sha256};
    use std::sync::{Arc, Mutex};

    const HOST: &str = "acme.example.test";

    fn sha(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn facts() -> PropertyReadinessFacts {
        PropertyReadinessFacts {
            has_room_types: true,
            has_active_rooms: true,
            has_active_rates: false,
            has_stays: true,
        }
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
        (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    async fn serve_probe(
        loaded: Arc<Mutex<Vec<(String, String)>>>,
        answer: Option<PropertyReadinessFacts>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let reader = Arc::new(ProbePropertyReadinessRead::new(move |company, property| {
            let company = company.to_owned();
            let property = property.to_owned();
            let loaded = loaded.clone();
            Box::pin(async move {
                loaded.lock().unwrap().push((company, property));
                Ok::<Option<PropertyReadinessFacts>, BusinessError>(answer)
            })
        }));
        let router = router_with_reader(
            reader,
            vec![LaneTarget {
                host: HOST.to_owned(),
                holding_slug: "acme".to_owned(),
            }],
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(host: &str, company: &str, property: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/pms/native-lane-20/property-readiness HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\
             x-erpx-context-property: {property}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn readiness_parity_and_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(seen.clone(), Some(facts())).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(HOST, "company", "property-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["facts"]["hasRoomTypes"], true);
        assert_eq!(body["facts"]["hasActiveRooms"], true);
        assert_eq!(body["facts"]["hasActiveRates"], false);
        assert_eq!(body["facts"]["hasStays"], true);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("company".to_owned(), "property-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn foreign_property_is_null() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(seen, None).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(HOST, "company", "foreign"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert!(body["facts"].is_null());
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), Some(facts())).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("attacker.example.test", "company", "property-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn invalid_identity_fails_closed() {
        let (port, server) = serve_probe(Arc::new(Mutex::new(Vec::new())), Some(facts())).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request(HOST, "bad company id!", "property-1"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PMS_NATIVE_LANE_20_REQUEST_INVALID");
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        let reader = Arc::new(ProbePropertyReadinessRead::new(|_, _| {
            Box::pin(async { Ok::<Option<PropertyReadinessFacts>, BusinessError>(None) })
        }));
        let target = || LaneTarget {
            host: HOST.to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(reader, vec![]).is_err());
    }

    #[test]
    fn sql_and_result_oracles() {
        let sql = format!("{PROPERTY_SCOPE_SQL}\n{READINESS_SQL}");
        eprintln!("LANE20_SQL_ORACLE={}", sha(&sql));
        assert_eq!(
            sha(&sql),
            "de5d1aac0cbfb981b79fb75777e19bd00565cdcaaac16b4aed053607bce35565"
        );
        let result = serde_json::json!({
            "hasActiveRates": false,
            "hasActiveRooms": true,
            "hasRoomTypes": true,
            "hasStays": true,
        })
        .to_string();
        eprintln!("LANE20_RESULT_ORACLE={}", sha(&result));
        assert_eq!(
            sha(&result),
            "5c3f6e00b4bd3508d9e301ecdbaaf34971f27d12c6b05592503a49f2a5387a41"
        );
    }
}
