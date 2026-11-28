//! Query port and handler for the native IAM "sessions by user" read.
//!
//! No SQL lives here. The actor identity is always the private context header
//! supplied by the authenticated gateway; a caller cannot pass a user id in the
//! request body to enumerate another Account's sessions.

use super::domain::{SessionRow, invalid, valid_id};
use erpx_platform_shared_server::public::BusinessError;
use serde::Deserialize;
use std::{future::Future, pin::Pin};

pub type SessionListFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the Control-backed owner reader behind this port. The
/// only implementation executes real sqlx against the Control `session` table.
pub trait SessionListRead: Send + Sync {
    fn list_sessions<'a>(&'a self, user_id: &'a str) -> SessionListFuture<'a, Vec<SessionRow>>;
}

/// Closed wire request. It carries no authority: identity comes from
/// `x-erpx-context-user`, and any body field is rejected.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionListRequest {}

/// Run one validated read for the verified actor only.
pub async fn execute(
    reader: &dyn SessionListRead,
    user_id: &str,
) -> Result<Vec<SessionRow>, BusinessError> {
    if !valid_id(user_id) {
        return Err(invalid());
    }
    reader.list_sessions(user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Probe {
        seen: Mutex<Vec<String>>,
    }

    impl SessionListRead for Probe {
        fn list_sessions<'a>(&'a self, user_id: &'a str) -> SessionListFuture<'a, Vec<SessionRow>> {
            self.seen.lock().unwrap().push(user_id.to_owned());
            Box::pin(async move {
                Ok(vec![SessionRow {
                    id: "session-1".into(),
                    created_at: "2026-01-01T00:00:00.000Z".into(),
                    expires_at: "2026-01-08T00:00:00.000Z".into(),
                    user_agent: None,
                }])
            })
        }
    }

    #[tokio::test]
    async fn read_dispatches_the_verified_actor() {
        let probe = Probe {
            seen: Mutex::new(Vec::new()),
        };
        let rows = execute(&probe, "user-1").await.expect("session read");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "session-1");
        assert_eq!(probe.seen.lock().unwrap().as_slice(), &["user-1".to_owned()]);
    }

    #[tokio::test]
    async fn invalid_actor_never_reaches_the_reader() {
        let probe = Probe {
            seen: Mutex::new(Vec::new()),
        };
        assert_eq!(execute(&probe, "not a user").await, Err(invalid()));
        assert!(probe.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn request_accepts_no_caller_fields() {
        assert!(serde_json::from_str::<SessionListRequest>("{}").is_ok());
        assert!(serde_json::from_str::<SessionListRequest>(
            r#"{"user_id":"someone-else"}"#
        )
        .is_err());
    }
}
