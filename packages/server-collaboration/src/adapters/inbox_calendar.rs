//! Native calendar visibility delegate for the Inbox destination port.
//!
//! This mirrors the retained TypeScript `calendarAppointment` /
//! `calendarRecipient` reads in
//! `packages/server-work_projection/src/adapters/collaboration-inbox-sources.ts`:
//! a `calendar_appointment` item is visible only when the appointment row
//! exists and the reader is an attendee (or recorded the appointment via
//! `created_by`); a `calendar_removed` item is visible when the appointment
//! row still exists and never carries a link. Every other source type defers
//! to the existing fail-closed behavior instead of inventing a link.
//!
//! Nothing here opens a pool, commits, or retries. It runs serializable-safe
//! SELECTs on the caller's connection.
use crate::adapters::inbox_pg::{InboxTarget, InboxTargetFuture, InboxTargetPort};
use crate::adapters::inbox_targets::{
    encode_uri_component, retained_inbox_destination, source_record_label,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::json;
use sqlx::{PgConnection, Row};

/// Source types this delegate can prove against calendar rows.
pub fn is_calendar_inbox_target(source_type: &str) -> bool {
    matches!(source_type, "calendar_appointment" | "calendar_removed")
}

/// Retained destination address for one appointment, byte-identical to the
/// TypeScript `` `/calendar?event=appointment:${encodeURIComponent(id)}` ``.
pub fn calendar_appointment_href(source_id: &str) -> String {
    format!(
        "/calendar?event=appointment:{}",
        encode_uri_component(source_id)
    )
}

fn unsupported() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_SOURCE_UNSUPPORTED",
        "This Inbox source is not part of the retained projection.",
    )
}

fn owner_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_PORT_UNAVAILABLE",
        "This Inbox destination requires its owning workspace.",
    )
}

fn storage() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_TARGET_QUERY_FAILED",
        "The Inbox destination is unavailable.",
    )
}

/// Native calendar visibility delegate, composed via
/// `PgInboxTargets::with_delegate`. Calendar-only: every other source type
/// defers to the existing fail-closed behavior instead of inventing a link.
pub struct CalendarInboxTargets;

impl CalendarInboxTargets {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CalendarInboxTargets {
    fn default() -> Self {
        Self::new()
    }
}

/// Retained `calendarAppointment` read: `(id, version)` for one Company row.
async fn appointment_identity(
    c: &mut PgConnection,
    company_id: &str,
    source_id: &str,
) -> Result<Option<(String, i32)>, BusinessError> {
    let row = sqlx::query(
        "SELECT id,version FROM calendar_appointments WHERE id=$1 AND company_id=$2",
    )
    .bind(source_id)
    .bind(company_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    row.map(|row| {
        let id: String = row.try_get("id").map_err(|_| storage())?;
        let version: i32 = row.try_get("version").map_err(|_| storage())?;
        Ok((id, version))
    })
    .transpose()
}

/// Retained `calendarRecipient` read: attendee row or the recording creator.
async fn invited(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    source_id: &str,
) -> Result<bool, BusinessError> {
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM calendar_attendees \
          WHERE appointment_id=$1 AND company_id=$2 AND user_id=$3 \
          UNION ALL SELECT 1 FROM calendar_appointments \
          WHERE id=$4 AND company_id=$5 AND created_by=$6 LIMIT 1",
    )
    .bind(source_id)
    .bind(company_id)
    .bind(user_id)
    .bind(source_id)
    .bind(company_id)
    .bind(user_id)
    .fetch_optional(&mut *c)
    .await
    .map_err(|_| storage())?;
    Ok(hit.is_some())
}

impl InboxTargetPort for CalendarInboxTargets {
    fn target<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        company_id: &'a str,
        user_id: &'a str,
        source_type: &'a str,
        source_id: &'a str,
        _source_version: i64,
    ) -> InboxTargetFuture<'a> {
        Box::pin(async move {
            match source_type {
                "calendar_appointment" => {
                    let Some((id, version)) =
                        appointment_identity(connection, company_id, source_id).await?
                    else {
                        return Ok(None);
                    };
                    if !invited(connection, company_id, user_id, source_id).await? {
                        return Ok(None);
                    }
                    let href = calendar_appointment_href(source_id);
                    Ok(Some(InboxTarget {
                        href: Some(href.clone()),
                        source: Some(json!({
                            "kind": "calendar_appointment",
                            "id": id,
                            "version": version,
                            "href": href,
                            "label": source_record_label("calendar_appointment"),
                        })),
                    }))
                }
                "calendar_removed" => {
                    let exists: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM calendar_appointments \
                          WHERE id=$1 AND company_id=$2)",
                    )
                    .bind(source_id)
                    .bind(company_id)
                    .fetch_one(&mut *connection)
                    .await
                    .map_err(|_| storage())?;
                    // The retained projection sets neither href nor source for
                    // a removed appointment.
                    Ok(exists.then(InboxTarget::default))
                }
                _ if retained_inbox_destination(source_type, source_id).is_some() => {
                    Err(owner_unavailable())
                }
                _ => Err(unsupported()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_covers_only_the_calendar_slice() {
        assert!(is_calendar_inbox_target("calendar_appointment"));
        assert!(is_calendar_inbox_target("calendar_removed"));
        for other in [
            "notice",
            "collaboration_action",
            "support_ticket",
            "calendar",
            "future_source",
            "",
        ] {
            assert!(!is_calendar_inbox_target(other), "{other}");
        }
    }

    #[test]
    fn href_encodes_exactly_like_the_typescript_projection() {
        assert_eq!(
            calendar_appointment_href("appt-1"),
            "/calendar?event=appointment:appt-1"
        );
        assert_eq!(
            calendar_appointment_href("seg/one two"),
            "/calendar?event=appointment:seg%2Fone%20two"
        );
        assert_eq!(
            calendar_appointment_href("😀"),
            "/calendar?event=appointment:%F0%9F%98%80"
        );
    }

    #[test]
    fn calendar_kinds_keep_their_retained_labels() {
        assert_eq!(source_record_label("calendar_appointment"), "Appointment");
    }
}
