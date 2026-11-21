//! Unit proof of the retained Inbox `source_type` -> destination table.
use erpx_server_collaboration::public::inbox::{
    InboxTargetVisibility, encode_uri_component, is_owned_inbox_target, retained_inbox_destination,
    source_record_label,
};

const ID: &str = "seg/one two";
const ENCODED: &str = "seg%2Fone%20two";

fn href(source_type: &str) -> Option<String> {
    retained_inbox_destination(source_type, ID)
        .unwrap_or_else(|| panic!("missing retained destination for {source_type}"))
        .href
}

#[test]
fn every_retained_staff_source_has_a_destination() {
    // Retained `staffNotificationSourceTypes` plus the notice/calendar family.
    for source_type in [
        "invoice",
        "journal",
        "organization_change",
        "company_business_acceptance",
        "customer_identity",
        "customer_duplicate",
        "customer_access",
        "support_sla_alert",
        "support_incident",
        "support_policy",
        "support_ticket",
        "support_session",
        "iam_access_review",
        "payroll_correction",
        "payroll_advance",
        "staff_task_alert",
        "maintenance_work",
        "maintenance_plan",
        "project_handoff",
        "field_service_order",
        "commerce_order",
        "collaboration_action",
        "operational_exception",
        "learning_assignment",
        "benefit_election",
        "talent_application",
        "marketing_candidate",
        "manufacturing_order",
        "crm_lead",
        "crm_activity",
        "landed_cost",
        "stock_count",
        "customer_followup",
        "supplier_bill",
        "procurement_rfq",
        "supplier_promise",
        "inventory_transit",
        "quality_hold",
        "quality_alert",
        "supplier_batch",
        "refund",
        "legal",
        "retirement_clearance",
        "roster_week",
        "roster_request",
        "roster_coverage",
        "overtime",
        "leave",
        "leave_cancellation",
        "expense",
        "request",
        "task",
        "post",
        "handbook",
        "handbook_article",
        "notice",
        "calendar_appointment",
        "calendar_removed",
    ] {
        assert!(
            retained_inbox_destination(source_type, ID).is_some(),
            "missing {source_type}"
        );
    }
    assert!(retained_inbox_destination("future_source", ID).is_none());
    assert!(retained_inbox_destination("social_comment", ID).is_none());
}

#[test]
fn static_hrefs_match_the_typescript_projection() {
    let expected = [
        ("customer_identity", format!("/customer-identity/{ENCODED}")),
        (
            "customer_duplicate",
            format!("/customer-duplicates/{ENCODED}"),
        ),
        ("customer_access", format!("/customer-access/{ENCODED}")),
        ("support_sla_alert", "/support/sla".into()),
        ("support_policy", "/support/sla".into()),
        ("support_incident", format!("/support/incidents/{ID}")),
        ("support_ticket", format!("/support/{ENCODED}")),
        ("operational_exception", format!("/operations/{ENCODED}")),
        ("support_session", format!("/support/sessions/{ENCODED}")),
        (
            "iam_access_review",
            format!("/people/settings?section=reviews#{ENCODED}"),
        ),
        ("staff_task_alert", format!("/staff-tasks/{ENCODED}")),
        ("maintenance_work", format!("/maintenance/{ENCODED}")),
        ("maintenance_plan", format!("/maintenance/plans/{ENCODED}")),
        ("field_service_order", format!("/field-service/{ENCODED}")),
        ("commerce_order", format!("/commerce/orders/{ENCODED}")),
        ("collaboration_action", "/collaboration".into()),
        ("learning_assignment", format!("/learning/{ENCODED}")),
        ("benefit_election", format!("/benefits/{ENCODED}")),
        ("talent_application", format!("/talent/{ENCODED}")),
        ("marketing_candidate", format!("/marketing/{ENCODED}")),
        ("manufacturing_order", format!("/manufacturing/{ENCODED}")),
        ("crm_lead", format!("/crm/{ENCODED}")),
        ("stock_count", format!("/stock-counts/{ENCODED}")),
        (
            "customer_followup",
            format!("/customer-followups/{ENCODED}"),
        ),
        ("supplier_bill", format!("/supplier-bills/{ENCODED}")),
        ("procurement_rfq", format!("/procurement-rfqs/{ENCODED}")),
        (
            "supplier_promise",
            format!("/commerce/orders/{ENCODED}#supplier-promises"),
        ),
        ("inventory_transit", format!("/stock-transit/{ENCODED}")),
        ("quality_hold", format!("/quality/{ENCODED}")),
        (
            "supplier_batch",
            format!("/finance/supplier-batches/{ENCODED}"),
        ),
        ("refund", format!("/finance/refunds/{ENCODED}")),
        ("legal", format!("/legal?record={ENCODED}")),
        ("overtime", format!("/overtime/{ID}")),
        ("leave", format!("/tasks/people_payroll/{ID}")),
        ("leave_cancellation", format!("/tasks/people_payroll/{ID}")),
        ("expense", format!("/expenses/{ID}")),
        ("request", format!("/requests/{ID}")),
        ("task", format!("/staff-tasks/{ID}")),
        ("post", format!("/posts/{ID}")),
        ("handbook", format!("/handbook/{ENCODED}")),
        ("handbook_article", format!("/handbook/{ENCODED}")),
        (
            "calendar_appointment",
            format!("/calendar?event=appointment:{ENCODED}"),
        ),
        ("invoice", format!("/finance/invoices/{ENCODED}")),
        ("journal", format!("/finance/journals/{ENCODED}")),
    ];
    for (source_type, expected) in expected {
        assert_eq!(
            href(source_type).as_deref(),
            Some(expected.as_str()),
            "{source_type}"
        );
    }
    // Owner-resolved destinations must not be invented by this table.
    for source_type in [
        "organization_change",
        "company_business_acceptance",
        "payroll_correction",
        "payroll_advance",
        "project_handoff",
        "crm_activity",
        "landed_cost",
        "retirement_clearance",
        "roster_week",
        "roster_request",
        "roster_coverage",
        "quality_alert",
        "calendar_removed",
        "notice",
    ] {
        assert_eq!(href(source_type), None, "{source_type} must delegate");
    }
}

#[test]
fn source_kinds_labels_and_visibility_are_retained() {
    let task = retained_inbox_destination("task", ID).unwrap();
    assert_eq!(task.source_kind, "staff_task");
    assert_eq!(task.label, "Task");
    assert_eq!(task.visibility, InboxTargetVisibility::Owner);

    let leave = retained_inbox_destination("leave_cancellation", ID).unwrap();
    assert_eq!(leave.source_kind, "leave");
    assert_eq!(leave.label, "Leave");

    let handbook = retained_inbox_destination("handbook", ID).unwrap();
    assert_eq!(handbook.source_kind, "handbook_article");
    assert_eq!(handbook.label, "Handbook article");

    let alert = retained_inbox_destination("quality_alert", ID).unwrap();
    assert_eq!(alert.source_kind, "quality_hold");
    assert_eq!(alert.label, "Quality hold");

    let notice = retained_inbox_destination("notice", ID).unwrap();
    assert_eq!(notice.visibility, InboxTargetVisibility::Delivery);
    assert_eq!(notice.source_kind, "notice");
    assert_eq!(notice.label, "Notice");
}

#[test]
fn encoding_and_labels_match_javascript() {
    assert_eq!(encode_uri_component(ID), ENCODED);
    assert_eq!(encode_uri_component("A-z_0.9!~*'()"), "A-z_0.9!~*'()");
    assert_eq!(encode_uri_component("😀"), "%F0%9F%98%80");
    assert_eq!(source_record_label("staff_task"), "Task");
    assert_eq!(
        source_record_label("maintenance_plan"),
        "Preventive maintenance"
    );
    assert_eq!(
        source_record_label("collaboration_action"),
        "Collaboration action"
    );
    assert_eq!(source_record_label("future_record"), "Future record");
}

#[test]
fn only_collaboration_owned_sources_are_resolved_without_a_delegate() {
    assert!(is_owned_inbox_target("notice"));
    assert!(is_owned_inbox_target("collaboration_action"));
    for other in ["support_ticket", "post", "handbook", "invoice", "task"] {
        assert!(!is_owned_inbox_target(other), "{other}");
    }
}
