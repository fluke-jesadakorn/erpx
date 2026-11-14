//! Retained PMS social event projection facts and identifiers.
//!
//! Exact port of `packages/server-pms/src/domain/social-events.ts`, the
//! `PmsSocialPost`/`PmsSocialNotice` write shapes and the deterministic
//! business-day post identity used by
//! `packages/server-pms/src/application/social-events.ts`. This module contains
//! no SQL; owner adapters retain every statement.
use crate::domain::command_json::CommandJson;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// One outbox event joined to its command receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PmsSocialOriginal {
    pub command_id: String,
    pub created_at: i64,
    pub actor_id: String,
    pub result_json: String,
}

/// One PMS source from the retained `pmsSourcesSql` union.
#[derive(Clone, Debug, PartialEq)]
pub struct PmsSocialSource {
    pub id: String,
    pub kind: String,
    pub company_id: String,
    pub branch_id: String,
    pub property_name: String,
    pub business_date: String,
    pub data_json: Value,
    pub version: i64,
}

/// The retained `PmsSocialPost` row body. `source_id`, `event_id` and
/// `received_at` are explicit publish arguments in the native owner port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PmsSocialPost {
    pub post_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub actor_id: String,
    pub body: String,
    pub command_id: String,
    pub at: i64,
}

/// The retained `PmsSocialNotice` row body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PmsSocialNotice {
    pub id: String,
    pub company_id: String,
    pub user_id: String,
    pub post_id: String,
    pub version: Option<i64>,
    pub body: String,
    pub at: i64,
}

/// One retained PMS social event label and deduplication group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmsSocialEvent {
    pub label: &'static str,
    pub group: &'static str,
}

/// The exact `pmsSocialEvents` table. The outbox projector only owns events in
/// this table; every other `pms.%` event remains unhandled.
pub const PMS_SOCIAL_EVENTS: &[(&str, PmsSocialEvent)] = &[
    (
        "pms.reservation.create",
        PmsSocialEvent {
            label: "ยืนยันการจอง / Reservation confirmed",
            group: "reservation",
        },
    ),
    (
        "pms.reservation.cancel",
        PmsSocialEvent {
            label: "ยกเลิกการจอง / Reservation cancelled",
            group: "reservation",
        },
    ),
    (
        "pms.stay.check_in",
        PmsSocialEvent {
            label: "เช็กอิน / Check-in",
            group: "stay",
        },
    ),
    (
        "pms.stay.check_out",
        PmsSocialEvent {
            label: "เช็กเอาต์ / Check-out",
            group: "stay",
        },
    ),
    (
        "pms.task.complete",
        PmsSocialEvent {
            label: "ทำความสะอาดเสร็จ / Room prepared",
            group: "room",
        },
    ),
    (
        "pms.task.inspect",
        PmsSocialEvent {
            label: "ตรวจห้องแล้ว / Room inspected",
            group: "room",
        },
    ),
    (
        "pms.payment.request_transfer",
        PmsSocialEvent {
            label: "เงินโอนรอตรวจ / Bank transfer awaiting Finance",
            group: "transfer",
        },
    ),
    (
        "pms.payment.verify_transfer",
        PmsSocialEvent {
            label: "Finance ยืนยันเงินโอน / Bank transfer verified",
            group: "transfer",
        },
    ),
    (
        "pms.payment.cancel_transfer",
        PmsSocialEvent {
            label: "ยกเลิกการตรวจเงินโอน / Transfer request cancelled",
            group: "transfer",
        },
    ),
    (
        "pms.charge.correction.request",
        PmsSocialEvent {
            label: "ขอตรวจแก้ค่าบริการ / Charge correction requested",
            group: "correction",
        },
    ),
    (
        "pms.charge.correction.review",
        PmsSocialEvent {
            label: "ตรวจคำขอแก้ค่าบริการ / Charge correction reviewed",
            group: "correction",
        },
    ),
    (
        "pms.refund.request",
        PmsSocialEvent {
            label: "ขอคืนเงิน / Refund requested",
            group: "refund",
        },
    ),
    (
        "pms.refund.approve",
        PmsSocialEvent {
            label: "อนุมัติคืนเงิน / Refund approved",
            group: "refund",
        },
    ),
    (
        "pms.refund.reject",
        PmsSocialEvent {
            label: "ปฏิเสธคืนเงิน / Refund rejected",
            group: "refund",
        },
    ),
    (
        "pms.refund.pay",
        PmsSocialEvent {
            label: "จ่ายเงินคืนแล้ว / Refund paid",
            group: "refund",
        },
    ),
    (
        "pms.shift.close",
        PmsSocialEvent {
            label: "ปิดกะแคชเชียร์ / Cashier shift closed",
            group: "shift",
        },
    ),
    (
        "pms.shift.review",
        PmsSocialEvent {
            label: "ตรวจส่วนต่างกะแล้ว / Cash variance reviewed",
            group: "shift",
        },
    ),
    (
        "pms.night.step",
        PmsSocialEvent {
            label: "ผลปิดวัน / Night Audit progress",
            group: "night",
        },
    ),
];

/// The exact `pmsSocialEvents[eventType]` lookup.
pub fn pms_social_event(event_type: &str) -> Option<PmsSocialEvent> {
    PMS_SOCIAL_EVENTS
        .iter()
        .find(|(key, _)| *key == event_type)
        .map(|(_, event)| *event)
}

/// Exact retained business-day derivation:
/// `new Date(created_at + 7h).toISOString().slice(0, 10)`.
pub fn pms_social_day(created_at: i64) -> String {
    let shifted = created_at.saturating_add(7 * 60 * 60 * 1000);
    let days = shifted.div_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Exact retained post identity:
/// `pms-post-` + sha256(JSON.stringify([company, source, day, group])).slice(0,40).
pub fn pms_social_post_id(
    company_id: &str,
    source_id: &str,
    created_at: i64,
    group: &str,
) -> String {
    let day = pms_social_day(created_at);
    let payload = CommandJson::from_serializable(&[company_id, source_id, day.as_str(), group])
        .expect("a static string array is a bounded JSON value");
    let digest = hex::encode(Sha256::digest(payload.as_str().as_bytes()));
    format!("pms-post-{}", &digest[..40])
}

/// Exact retained notice identity:
/// `pms-notice-` + sha256(JSON.stringify([postId, userId])).slice(0,40).
pub fn pms_social_notice_id(post_id: &str, user_id: &str) -> String {
    let payload = CommandJson::from_serializable(&[post_id, user_id])
        .expect("a static string array is a bounded JSON value");
    let digest = hex::encode(Sha256::digest(payload.as_str().as_bytes()));
    format!("pms-notice-{}", &digest[..40])
}

/// Exact retained post body.
pub fn pms_social_body(source: &PmsSocialSource, label: &str) -> String {
    let state = source
        .data_json
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("updated");
    let date = source
        .data_json
        .get("date")
        .and_then(Value::as_str)
        .unwrap_or(&source.business_date);
    format!(
        "{} · {label}\n{state} · วันที่ธุรกิจ / Business date {date}",
        source.property_name
    )
}

/// The retained `new Date(...).toISOString().slice(0, 10)` civil conversion.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source(state: Option<&str>, date: Option<&str>) -> PmsSocialSource {
        PmsSocialSource {
            id: "reservation".into(),
            kind: "reservation".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            property_name: "Hotel".into(),
            business_date: "2026-10-05".into(),
            data_json: json!({ "state": state, "date": date }),
            version: 3,
        }
    }

    #[test]
    fn event_table_matches_the_retained_labels_and_groups() {
        assert_eq!(PMS_SOCIAL_EVENTS.len(), 18);
        let event = pms_social_event("pms.reservation.create").unwrap();
        assert_eq!(event.label, "ยืนยันการจอง / Reservation confirmed");
        assert_eq!(event.group, "reservation");
        let refund = pms_social_event("pms.refund.pay").unwrap();
        assert_eq!(refund.label, "จ่ายเงินคืนแล้ว / Refund paid");
        assert_eq!(refund.group, "refund");
        assert!(pms_social_event("pms.unknown").is_none());
        assert!(pms_social_event("task.start").is_none());
    }

    #[test]
    fn business_day_uses_utc_plus_seven_hours() {
        assert_eq!(pms_social_day(0), "1970-01-01");
        assert_eq!(pms_social_day(86_399_999), "1970-01-02");
        assert_eq!(pms_social_day(1_790_000_000_000), "2026-09-21");
    }

    #[test]
    fn post_and_notice_ids_match_the_retained_bun_hash() {
        assert_eq!(
            pms_social_post_id("company", "reservation", 1_790_000_000_000, "reservation"),
            "pms-post-83b92a95fc4e224578285f30e46e124571b5820b"
        );
        assert_eq!(
            pms_social_notice_id("pms-post-x", "user-1"),
            "pms-notice-5ea9587a4f00295427d6b93c434f382f73bb7432"
        );
    }

    #[test]
    fn body_defaults_state_and_date_exactly() {
        assert_eq!(
            pms_social_body(&source(Some("confirmed"), Some("2026-10-06")), "Label"),
            "Hotel · Label\nconfirmed · วันที่ธุรกิจ / Business date 2026-10-06"
        );
        assert_eq!(
            pms_social_body(&source(None, None), "Label"),
            "Hotel · Label\nupdated · วันที่ธุรกิจ / Business date 2026-10-05"
        );
        assert_eq!(
            pms_social_body(&source(Some(""), Some("")), "Label"),
            "Hotel · Label\n · วันที่ธุรกิจ / Business date "
        );
    }
}
