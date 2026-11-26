//! Verified-customer portal support-thread projection.
//!
//! Pure domain types and invariants for the customer-facing support thread read
//! that backs the Customer Portal ticket list. No SQL, no HTTP, no I/O.

use erpx_server_shared::public::{BusinessError, FailureKind};

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PORTAL_SUPPORT_THREADS_INVALID",
        "Review the portal support thread request.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PORTAL_SUPPORT_REQUEST_NOT_FOUND",
        "The actual customer support request is unavailable.",
    )
}

pub fn identity_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_PORTAL_IDENTITY_REQUIRED",
        "Use the verified account that made the original request.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PORTAL_SUPPORT_SOURCE_CHANGED",
        "The original customer binding changed during this read.",
    )
}

/// Scope facts already resolved from the verified portal session. The binding
/// id is the exact customer access binding the session was enrolled against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadsQuery {
    pub binding_id: String,
    pub user_id: String,
    pub email: String,
}

impl ThreadsQuery {
    pub fn validate(self) -> Result<Self, BusinessError> {
        let email = self.email.trim().to_ascii_lowercase();
        if self.binding_id.is_empty()
            || self.binding_id.chars().count() > 200
            || self.user_id.is_empty()
            || self.user_id.chars().count() > 200
            || email.is_empty()
            || email.chars().count() > 254
            || !email.contains('@')
        {
            return Err(invalid());
        }
        Ok(Self {
            email,
            ..self
        })
    }
}

/// A published response exactly as the retained TypeScript projection returns
/// it: `id`, `version`, `message`, `publishedAt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadResponse {
    pub id: String,
    pub version: i64,
    pub message: String,
    pub published_at: i64,
}

impl ThreadResponse {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "version": self.version,
            "message": self.message,
            "publishedAt": self.published_at,
        })
    }
}

/// One verified-customer support thread. Field names and order match
/// `PortalSupportThread` from the retained TypeScript owner exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    pub request_id: String,
    pub requested_at: i64,
    pub note: String,
    pub response: Option<ThreadResponse>,
    pub acknowledged_at: Option<i64>,
    pub closed_at: Option<i64>,
}

impl Thread {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "requestId": self.request_id,
            "requestedAt": self.requested_at,
            "note": self.note,
            "response": self.response.as_ref().map(ThreadResponse::wire),
            "acknowledgedAt": self.acknowledged_at,
            "closedAt": self.closed_at,
        })
    }
}

/// Build a thread from the exact columns of the registered
/// `portal-support.threads` read. A response is present only when every
/// published-response column is present, matching the TypeScript null guard.
#[allow(clippy::too_many_arguments)]
pub fn thread_from_columns(
    request_id: String,
    requested_at: i64,
    note: String,
    response_id: Option<String>,
    response_version: Option<i64>,
    message: Option<String>,
    published_at: Option<i64>,
    acknowledged_at: Option<i64>,
    closed_at: Option<i64>,
) -> Thread {
    let response = match (response_id, response_version, message, published_at) {
        (Some(id), Some(version), Some(message), Some(published_at)) if !id.is_empty() => {
            Some(ThreadResponse {
                id,
                version,
                message,
                published_at,
            })
        }
        _ => None,
    };
    Thread {
        request_id,
        requested_at,
        note,
        response,
        acknowledged_at,
        closed_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_normalizes_and_guards_scope() {
        let q = ThreadsQuery {
            binding_id: "binding-1".into(),
            user_id: "customer".into(),
            email: " Customer@Example.Invalid ".into(),
        }
        .validate()
        .unwrap();
        assert_eq!(q.email, "customer@example.invalid");
        assert_eq!(q.binding_id, "binding-1");
        assert_eq!(
            ThreadsQuery {
                email: "not-an-email".into(),
                ..q
            }
            .validate()
            .unwrap_err()
            .code,
            "PORTAL_SUPPORT_THREADS_INVALID"
        );
    }

    #[test]
    fn wire_shape_matches_retained_portal_thread() {
        let thread = thread_from_columns(
            "request-1".into(),
            1000,
            "Please review".into(),
            Some("response-1".into()),
            Some(2),
            Some("Reviewed".into()),
            Some(2000),
            Some(3000),
            None,
        );
        let wire = thread.wire();
        let object = wire.as_object().unwrap();
        assert_eq!(object.len(), 6);
        for key in [
            "requestId",
            "requestedAt",
            "note",
            "response",
            "acknowledgedAt",
            "closedAt",
        ] {
            assert!(object.contains_key(key), "{key}");
        }
        let response = object["response"].as_object().unwrap();
        assert_eq!(response.len(), 4);
        assert_eq!(response["id"], "response-1");
        assert_eq!(response["version"], 2);
        assert_eq!(response["publishedAt"], 2000);
        assert!(object["acknowledgedAt"].is_i64());
        assert!(object["closedAt"].is_null());
    }

    #[test]
    fn partial_response_columns_collapse_to_null() {
        let thread = thread_from_columns(
            "request-1".into(),
            1000,
            "note".into(),
            Some("response-1".into()),
            None,
            Some("message".into()),
            None,
            None,
            None,
        );
        assert_eq!(thread.response, None);
        assert_eq!(thread.wire()["response"], serde_json::Value::Null);
    }
}
