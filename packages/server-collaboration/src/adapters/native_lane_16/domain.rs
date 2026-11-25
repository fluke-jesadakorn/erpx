//! Pure response types and validation for the collaboration sent-notice reads.
//! No SQL, no pool and no transport live in this module.

use serde::{Deserialize, Serialize};

pub const KIND_NOTICES: &str = "notices";
pub const KIND_NOTICE: &str = "notice";
pub const KIND_DELIVERY_COUNT: &str = "delivery-count";
pub const KIND_TASK_ORIGIN: &str = "task-origin";
pub const KIND_PUSH_PREFERENCES: &str = "push-preferences";
pub const KIND_PUSH_SESSION_COUNT: &str = "push-session-count";
pub const KIND_PUSH_ENDPOINT_OWNER: &str = "push-endpoint-owner";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushReadKind {
    Preferences,
    SessionCount,
    EndpointOwner,
}

impl PushReadKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            KIND_PUSH_PREFERENCES => Some(Self::Preferences),
            KIND_PUSH_SESSION_COUNT => Some(Self::SessionCount),
            KIND_PUSH_ENDPOINT_OWNER => Some(Self::EndpointOwner),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preferences => KIND_PUSH_PREFERENCES,
            Self::SessionCount => KIND_PUSH_SESSION_COUNT,
            Self::EndpointOwner => KIND_PUSH_ENDPOINT_OWNER,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadKind {
    Notices,
    Notice,
    DeliveryCount,
    TaskOrigin,
}

impl ReadKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            KIND_NOTICES => Some(Self::Notices),
            KIND_NOTICE => Some(Self::Notice),
            KIND_DELIVERY_COUNT => Some(Self::DeliveryCount),
            KIND_TASK_ORIGIN => Some(Self::TaskOrigin),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Notices => KIND_NOTICES,
            Self::Notice => KIND_NOTICE,
            Self::DeliveryCount => KIND_DELIVERY_COUNT,
            Self::TaskOrigin => KIND_TASK_ORIGIN,
        }
    }

    /// The registered shape boundary for each kind. `notice_id`, `event_key`
    /// and `task_id` are never accepted outside their owning kind.
    pub fn shape(self) -> (bool, bool, bool) {
        match self {
            Self::Notices => (false, false, false),
            Self::Notice => (true, false, false),
            Self::DeliveryCount => (false, true, false),
            Self::TaskOrigin => (false, false, true),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SentNoticeRow {
    pub id: String,
    pub title: String,
    pub accepted_count: i64,
    pub created_at: i64,
    pub delivered_count: i64,
}

impl SentNoticeRow {
    pub fn into_value(self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "title": self.title,
            "accepted_count": self.accepted_count,
            "created_at": self.created_at,
            "delivered_count": self.delivered_count,
        })
    }
}

/// The merged notice detail the retained inbox route returns: it keeps the
/// original snake_case `recipient_count` and adds the native counts.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[allow(non_snake_case)] // exact wire keys the retained inbox route consumes
pub struct SentNoticeDetail {
    pub id: String,
    pub title: String,
    pub body: String,
    pub recipient_count: i64,
    pub created_at: i64,
    pub acceptedCount: i64,
    pub completedCount: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct TaskOrigin {
    pub post_id: String,
}

pub fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'@'))
}

pub fn valid_event_key(value: &str) -> bool {
    value.len() <= 400
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_bounded_and_closed() {
        assert!(valid_identifier("company-1"));
        assert!(valid_identifier("notice:1"));
        assert!(!valid_identifier(""));
        assert!(!valid_identifier("x/y"));
        assert!(!valid_identifier(&"x".repeat(201)));
        assert!(valid_event_key("notice:notice-1"));
        assert!(!valid_event_key("notice:1 and 1=1"));
    }

    #[test]
    fn push_read_kinds_are_closed() {
        for kind in [
            PushReadKind::Preferences,
            PushReadKind::SessionCount,
            PushReadKind::EndpointOwner,
        ] {
            assert_eq!(PushReadKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(PushReadKind::parse("SELECT 1"), None);
    }

    #[test]
    fn every_kind_has_one_exact_shape() {
        for kind in [
            ReadKind::Notices,
            ReadKind::Notice,
            ReadKind::DeliveryCount,
            ReadKind::TaskOrigin,
        ] {
            assert_eq!(ReadKind::parse(kind.as_str()), Some(kind));
            let (notice, event, task) = kind.shape();
            assert_eq!(
                usize::from(notice) + usize::from(event) + usize::from(task),
                usize::from(kind != ReadKind::Notices)
            );
        }
        assert_eq!(ReadKind::parse("SELECT 1"), None);
    }
}
