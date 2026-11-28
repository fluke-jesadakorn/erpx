//! Query port and handler for the native directory "connection by id" read.
//!
//! No SQL lives here. The Holding target is supplied by the authenticated
//! gateway in the axum layer; a caller carries only the closed Company scope
//! and connection id, both validated before they reach the owner port.

use super::domain::{ConnectionRow, invalid, not_found, valid_id, valid_token};
use erpx_platform_shared_server::public::BusinessError;
use serde::Deserialize;
use std::{future::Future, pin::Pin};

pub type ConnectionReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the Control/Holding-backed owner reader behind this
/// port. The only implementation executes real sqlx against
/// `iam_connections`.
pub trait ConnectionRead: Send + Sync {
    fn connection<'a>(
        &'a self,
        company_id: &'a str,
        id: &'a str,
    ) -> ConnectionReadFuture<'a, Option<ConnectionRow>>;
}

/// Closed wire request: only the two bound columns of the retired statement.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionReadRequest {
    pub company_id: String,
    pub id: String,
}

/// Run one validated read. A missing row becomes the same 404
/// `CONNECTION_NOT_FOUND` the TypeScript `createConnectionLookup` raised.
pub async fn execute(
    reader: &dyn ConnectionRead,
    company_id: &str,
    id: &str,
) -> Result<ConnectionRow, BusinessError> {
    if !valid_token(company_id) || !valid_id(id) {
        return Err(invalid());
    }
    reader
        .connection(company_id, id)
        .await?
        .ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_46::domain::ConnectionFields;
    use erpx_platform_shared_server::public::FailureKind;
    use serde_json::Value;
    use std::sync::Mutex;

    struct Probe {
        seen: Mutex<Vec<(String, String)>>,
        found: bool,
    }

    impl ConnectionRead for Probe {
        fn connection<'a>(
            &'a self,
            company_id: &'a str,
            id: &'a str,
        ) -> ConnectionReadFuture<'a, Option<ConnectionRow>> {
            self.seen
                .lock()
                .unwrap()
                .push((company_id.to_owned(), id.to_owned()));
            let found = self.found;
            Box::pin(async move {
                if !found {
                    return Ok(None);
                }
                Ok(Some(
                    ConnectionFields {
                        id: "connection-1".into(),
                        company_id: "acme-company".into(),
                        name: "Acme AD".into(),
                        provider: "active_directory".into(),
                        config: Value::Object(serde_json::Map::new()),
                        status: "active".into(),
                        version: 1,
                        tested_version: None,
                        last_sync_at: None,
                        health: "healthy".into(),
                        last_error: None,
                        created_by: "actor-1".into(),
                        created_at: 1,
                    }
                    .into_row()
                    .expect("row"),
                ))
            })
        }
    }

    #[tokio::test]
    async fn read_dispatches_the_validated_scope_and_id() {
        let probe = Probe {
            seen: Mutex::new(Vec::new()),
            found: true,
        };
        let row = execute(&probe, "acme-company", "connection-1")
            .await
            .expect("read");
        assert_eq!(row.id, "connection-1");
        assert_eq!(
            probe.seen.lock().unwrap().as_slice(),
            &[("acme-company".to_owned(), "connection-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn missing_row_is_the_retired_not_found_error() {
        let probe = Probe {
            seen: Mutex::new(Vec::new()),
            found: false,
        };
        let error = execute(&probe, "acme-company", "missing").await.expect_err("404");
        assert_eq!(error.code, "CONNECTION_NOT_FOUND");
        assert_eq!(error.kind, FailureKind::NotFound);
    }

    #[tokio::test]
    async fn invalid_inputs_never_reach_the_reader() {
        let probe = Probe {
            seen: Mutex::new(Vec::new()),
            found: true,
        };
        for (company, id) in [("acme company", "connection-1"), ("acme", "bad id")] {
            assert_eq!(execute(&probe, company, id).await, Err(invalid()));
        }
        assert!(probe.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn request_carries_only_the_two_bound_columns() {
        assert!(serde_json::from_str::<ConnectionReadRequest>(
            r#"{"company_id":"acme","id":"connection-1"}"#
        )
        .is_ok());
        assert!(
            serde_json::from_str::<ConnectionReadRequest>(r#"{"company_id":"acme"}"#).is_err()
        );
        assert!(serde_json::from_str::<ConnectionReadRequest>(
            r#"{"company_id":"acme","id":"connection-1","sql":"DROP TABLE x"}"#
        )
        .is_err());
    }
}
