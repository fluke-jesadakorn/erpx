//! Retained PMS social event table, business day, identity and body facts.
use erpx_server_workflow::public::job_projector::{
    PMS_SOCIAL_EVENTS, PmsSocialSource, pms_social_body, pms_social_day, pms_social_event,
    pms_social_notice_id, pms_social_post_id,
};
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
fn event_table_has_every_retained_label_and_group() {
    assert_eq!(PMS_SOCIAL_EVENTS.len(), 18);
    let expected = [
        (
            "pms.reservation.create",
            "ยืนยันการจอง / Reservation confirmed",
            "reservation",
        ),
        (
            "pms.reservation.cancel",
            "ยกเลิกการจอง / Reservation cancelled",
            "reservation",
        ),
        ("pms.stay.check_in", "เช็กอิน / Check-in", "stay"),
        ("pms.stay.check_out", "เช็กเอาต์ / Check-out", "stay"),
        (
            "pms.task.complete",
            "ทำความสะอาดเสร็จ / Room prepared",
            "room",
        ),
        ("pms.task.inspect", "ตรวจห้องแล้ว / Room inspected", "room"),
        (
            "pms.payment.request_transfer",
            "เงินโอนรอตรวจ / Bank transfer awaiting Finance",
            "transfer",
        ),
        (
            "pms.payment.verify_transfer",
            "Finance ยืนยันเงินโอน / Bank transfer verified",
            "transfer",
        ),
        (
            "pms.payment.cancel_transfer",
            "ยกเลิกการตรวจเงินโอน / Transfer request cancelled",
            "transfer",
        ),
        (
            "pms.charge.correction.request",
            "ขอตรวจแก้ค่าบริการ / Charge correction requested",
            "correction",
        ),
        (
            "pms.charge.correction.review",
            "ตรวจคำขอแก้ค่าบริการ / Charge correction reviewed",
            "correction",
        ),
        ("pms.refund.request", "ขอคืนเงิน / Refund requested", "refund"),
        (
            "pms.refund.approve",
            "อนุมัติคืนเงิน / Refund approved",
            "refund",
        ),
        (
            "pms.refund.reject",
            "ปฏิเสธคืนเงิน / Refund rejected",
            "refund",
        ),
        ("pms.refund.pay", "จ่ายเงินคืนแล้ว / Refund paid", "refund"),
        (
            "pms.shift.close",
            "ปิดกะแคชเชียร์ / Cashier shift closed",
            "shift",
        ),
        (
            "pms.shift.review",
            "ตรวจส่วนต่างกะแล้ว / Cash variance reviewed",
            "shift",
        ),
        ("pms.night.step", "ผลปิดวัน / Night Audit progress", "night"),
    ];
    for (event_type, label, group) in expected {
        let event = pms_social_event(event_type).expect(event_type);
        assert_eq!(event.label, label);
        assert_eq!(event.group, group);
    }
}

#[test]
fn only_the_social_table_is_owned() {
    assert!(pms_social_event("pms.reservation.create").is_some());
    assert!(pms_social_event("pms.unknown").is_none());
    assert!(pms_social_event("pms").is_none());
    assert!(pms_social_event("task.start").is_none());
    assert!(pms_social_event("PMS.RESERVATION.CREATE").is_none());
}

#[test]
fn business_day_is_utc_plus_seven_hours() {
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
fn body_uses_the_exact_retained_fallback_order() {
    assert_eq!(
        pms_social_body(
            &source(Some("confirmed"), Some("2026-10-06")),
            "ยืนยันการจอง / Reservation confirmed"
        ),
        "Hotel · ยืนยันการจอง / Reservation confirmed\nconfirmed · วันที่ธุรกิจ / Business date 2026-10-06"
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
