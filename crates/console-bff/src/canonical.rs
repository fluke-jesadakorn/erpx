//! Canonical same-origin HTTP route contracts.
//!
//! The BFF owns HTTP syntax only. Business handlers remain context-owned and
//! are selected after this parser has rejected aliases, malformed identities,
//! unsupported methods, and untyped mutation paths.

use std::fmt;

use erpx_integration_contracts::{catalog::record_type, reference::RecordTypeId};
use erpx_kernel::{IdentifierError, RecordId};
use uuid::Uuid;

pub const API_PREFIX: &str = "/api/v2";
pub const COLLABORATION_STREAM_TEMPLATE: &str = "/api/v2/collaboration/thread/:id/stream";
pub const SSE_CONTENT_TYPE: &str = "text/event-stream";
pub const SSE_CACHE_CONTROL: &str = "no-store";
pub const SSE_RETRY_MILLISECONDS: u32 = 3_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalApiRoute {
    Collection {
        record_type: RecordTypeId,
    },
    Detail {
        record_type: RecordTypeId,
        record_id: RecordId,
    },
    Command {
        record_type: RecordTypeId,
        record_id: Option<RecordId>,
        command: String,
    },
    CollaborationStream {
        thread_id: RecordId,
        after_cursor: u64,
    },
}

impl CanonicalApiRoute {
    pub fn parse(
        method: HttpMethod,
        target: &str,
        last_event_id: Option<&str>,
    ) -> Result<Self, CanonicalRouteError> {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if !path.starts_with("/api/v2/")
            || path.ends_with('/')
            || path.contains('#')
            || path.chars().any(char::is_whitespace)
        {
            return Err(CanonicalRouteError::NotCanonical);
        }
        let segments = path[8..].split('/').collect::<Vec<_>>();
        if let ["collaboration", "thread", raw_id, "stream"] = segments.as_slice() {
            if method != HttpMethod::Get {
                return Err(CanonicalRouteError::MethodNotAllowed);
            }
            return Ok(Self::CollaborationStream {
                thread_id: record_id(raw_id)?,
                after_cursor: resume_cursor(query, last_event_id)?,
            });
        }

        let record_type = match segments.as_slice() {
            [module, record]
            | [module, record, _]
            | [module, record, "commands", _]
            | [module, record, _, "commands", _] => cataloged_record_type(module, record)?,
            _ => return Err(CanonicalRouteError::UnknownRoute),
        };
        match segments.as_slice() {
            [_, _] if method == HttpMethod::Get && query.is_empty() => {
                Ok(Self::Collection { record_type })
            }
            [_, _, raw_id] if method == HttpMethod::Get && query.is_empty() => Ok(Self::Detail {
                record_type,
                record_id: record_id(raw_id)?,
            }),
            [_, _, "commands", command] if method == HttpMethod::Post && query.is_empty() => {
                Ok(Self::Command {
                    record_type,
                    record_id: None,
                    command: command_name(command)?,
                })
            }
            [_, _, raw_id, "commands", command]
                if method == HttpMethod::Post && query.is_empty() =>
            {
                Ok(Self::Command {
                    record_type,
                    record_id: Some(record_id(raw_id)?),
                    command: command_name(command)?,
                })
            }
            [_, _] | [_, _, _] | [_, _, "commands", _] | [_, _, _, "commands", _] => {
                Err(CanonicalRouteError::MethodNotAllowed)
            }
            _ => Err(CanonicalRouteError::UnknownRoute),
        }
    }
}

fn cataloged_record_type(module: &str, record: &str) -> Result<RecordTypeId, CanonicalRouteError> {
    let key = format!("{module}.{record}");
    record_type(&key).ok_or(CanonicalRouteError::UnknownRoute)?;
    RecordTypeId::try_new(key).map_err(|_| CanonicalRouteError::UnknownRoute)
}

fn record_id(value: &str) -> Result<RecordId, CanonicalRouteError> {
    let id = Uuid::parse_str(value).map_err(|_| CanonicalRouteError::InvalidRecordId)?;
    RecordId::try_from_uuid(id).map_err(|IdentifierError::Nil| CanonicalRouteError::InvalidRecordId)
}

fn command_name(value: &str) -> Result<String, CanonicalRouteError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-');
    valid
        .then(|| value.to_owned())
        .ok_or(CanonicalRouteError::InvalidCommand)
}

fn resume_cursor(query: &str, last_event_id: Option<&str>) -> Result<u64, CanonicalRouteError> {
    let query_cursor = if query.is_empty() {
        None
    } else {
        let Some(raw) = query.strip_prefix("cursor=") else {
            return Err(CanonicalRouteError::InvalidCursor);
        };
        Some(parse_cursor(raw)?)
    };
    let header_cursor = last_event_id.map(parse_cursor).transpose()?;
    match (query_cursor, header_cursor) {
        (Some(query), Some(header)) if query != header => Err(CanonicalRouteError::CursorConflict),
        (Some(cursor), _) | (_, Some(cursor)) => Ok(cursor),
        (None, None) => Ok(0),
    }
}

fn parse_cursor(value: &str) -> Result<u64, CanonicalRouteError> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CanonicalRouteError::InvalidCursor);
    }
    value
        .parse()
        .map_err(|_| CanonicalRouteError::InvalidCursor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalRouteError {
    NotCanonical,
    UnknownRoute,
    MethodNotAllowed,
    InvalidRecordId,
    InvalidCommand,
    InvalidCursor,
    CursorConflict,
}

impl fmt::Display for CanonicalRouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotCanonical => "route is not canonical",
            Self::UnknownRoute => "route is not cataloged",
            Self::MethodNotAllowed => "method is not allowed for the canonical route",
            Self::InvalidRecordId => "record identity is invalid",
            Self::InvalidCommand => "command name is invalid",
            Self::InvalidCursor => "stream cursor is invalid",
            Self::CursorConflict => "stream cursor sources conflict",
        })
    }
}

impl std::error::Error for CanonicalRouteError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_detail_and_typed_commands_are_method_bound() {
        let id = Uuid::new_v4();
        assert!(matches!(
            CanonicalApiRoute::parse(HttpMethod::Get, "/api/v2/finance/invoice", None),
            Ok(CanonicalApiRoute::Collection { .. })
        ));
        assert!(matches!(
            CanonicalApiRoute::parse(
                HttpMethod::Get,
                &format!("/api/v2/finance/invoice/{id}"),
                None,
            ),
            Ok(CanonicalApiRoute::Detail { .. })
        ));
        assert!(matches!(
            CanonicalApiRoute::parse(
                HttpMethod::Post,
                &format!("/api/v2/finance/invoice/{id}/commands/post"),
                None,
            ),
            Ok(CanonicalApiRoute::Command { .. })
        ));
        assert_eq!(
            CanonicalApiRoute::parse(
                HttpMethod::Get,
                &format!("/api/v2/finance/invoice/{id}/commands/post"),
                None,
            ),
            Err(CanonicalRouteError::MethodNotAllowed)
        );
    }

    #[test]
    fn aliases_and_untyped_mutations_are_rejected() {
        let id = Uuid::new_v4();
        for target in [
            "/api/bff/v1/finance/invoice",
            "/api/v2/unknown/record",
            &format!("/api/v2/finance/invoice/{id}/approve"),
        ] {
            assert!(CanonicalApiRoute::parse(HttpMethod::Post, target, None).is_err());
        }
    }

    #[test]
    fn collaboration_stream_resume_is_exact_and_conflict_safe() {
        let id = Uuid::new_v4();
        let path = format!("/api/v2/collaboration/thread/{id}/stream?cursor=42");
        assert_eq!(
            CanonicalApiRoute::parse(HttpMethod::Get, &path, Some("42")),
            Ok(CanonicalApiRoute::CollaborationStream {
                thread_id: RecordId::try_from_uuid(id).expect("record ID"),
                after_cursor: 42,
            })
        );
        assert_eq!(
            CanonicalApiRoute::parse(HttpMethod::Get, &path, Some("43")),
            Err(CanonicalRouteError::CursorConflict)
        );
        assert_eq!(SSE_CONTENT_TYPE, "text/event-stream");
        assert_eq!(SSE_CACHE_CONTROL, "no-store");
    }
}
