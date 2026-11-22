//! Unit proof of the retained Inbox `source_type` -> destination table.
use erpx_server_collaboration::public::inbox::{
    CalendarInboxTargets, CommerceInboxTargets, CrmInboxTargets, FinanceInboxTargets,
    InboxTargetPort, InboxTargetVisibility, PayrollInboxTargets, PeopleInboxTargets,
    ProcurementInboxTargets, SupportInboxTargets, bangkok_coverage_weeks, calendar_appointment_href,
    commerce_order_capability, commerce_order_href, crm_activity_href, crm_lead_href,
    encode_uri_component, expense_href, finance_document_capability, finance_invoice_href,
    finance_journal_href, finance_refund_href, is_calendar_inbox_target, is_commerce_inbox_target,
    is_crm_inbox_target, is_finance_inbox_target, is_owned_inbox_target, is_payroll_inbox_target,
    is_people_inbox_target, is_procurement_inbox_target, is_roster_monday, is_support_inbox_target,
    leave_href, overtime_href, parse_roster_coverage_id, payroll_advance_href, payroll_correction_href,
    procurement_history_capability, procurement_rfq_href, retirement_case_href,
    retained_inbox_destination, roster_request_href, roster_week_href, source_record_label,
    support_history_capability, support_incident_href, support_session_href, support_sla_href,
    support_ticket_href, supplier_bill_href, supplier_promise_href, uses_enforced_layer,
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;

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

fn assert_inbox_target_port<T: InboxTargetPort>(_: &T) {}

#[test]
fn calendar_slice_routes_to_the_native_delegate() {
    assert!(is_calendar_inbox_target("calendar_appointment"));
    assert!(is_calendar_inbox_target("calendar_removed"));
    for other in [
        "notice",
        "collaboration_action",
        "calendar",
        "support_ticket",
        "future_source",
        "",
    ] {
        assert!(!is_calendar_inbox_target(other), "{other}");
    }
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&CalendarInboxTargets::new());
    assert_inbox_target_port(&CalendarInboxTargets::default());
}

#[test]
fn calendar_hrefs_encode_exactly_like_the_typescript_projection() {
    assert_eq!(
        calendar_appointment_href(ID),
        "/calendar?event=appointment:seg%2Fone%20two"
    );
    assert_eq!(
        calendar_appointment_href("appt-1"),
        "/calendar?event=appointment:appt-1"
    );
    assert_eq!(
        calendar_appointment_href("A-z_0.9!~*'()"),
        "/calendar?event=appointment:A-z_0.9!~*'()"
    );
    assert_eq!(
        calendar_appointment_href("😀"),
        "/calendar?event=appointment:%F0%9F%98%80"
    );
    assert_eq!(source_record_label("calendar_appointment"), "Appointment");
}

#[test]
fn crm_slice_routes_to_the_native_delegate() {
    assert!(is_crm_inbox_target("crm_lead"));
    assert!(is_crm_inbox_target("crm_activity"));
    for other in [
        "notice",
        "collaboration_action",
        "calendar_appointment",
        "calendar_removed",
        "support_ticket",
        "crm",
        "future_source",
        "",
    ] {
        assert!(!is_crm_inbox_target(other), "{other}");
    }
    // The CRM delegate owns neither calendar source.
    assert!(!is_calendar_inbox_target("crm_lead"));
    assert!(!is_calendar_inbox_target("crm_activity"));
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&CrmInboxTargets::new(None, None));
    assert_inbox_target_port(&CrmInboxTargets::default());
    assert_inbox_target_port(
        &CrmInboxTargets::new(Some("branch"), Some("department")).assignment(Some("grant")),
    );
}

#[test]
fn crm_hrefs_encode_exactly_like_the_typescript_projection() {
    assert_eq!(crm_lead_href(ID), "/crm/seg%2Fone%20two");
    assert_eq!(crm_lead_href("lead-1"), "/crm/lead-1");
    assert_eq!(
        crm_activity_href(ID),
        "/crm/seg%2Fone%20two#crm-activities"
    );
    assert_eq!(crm_activity_href("lead-1"), "/crm/lead-1#crm-activities");
    assert_eq!(
        crm_activity_href("😀"),
        "/crm/%F0%9F%98%80#crm-activities"
    );
    assert_eq!(source_record_label("crm_lead"), "Crm lead");
    assert_eq!(source_record_label("crm_activity"), "Crm activity");
}

#[test]
fn commerce_slice_routes_to_the_native_delegate() {
    assert!(is_commerce_inbox_target("commerce_order"));
    for other in [
        "notice",
        "collaboration_action",
        "calendar_appointment",
        "crm_lead",
        "crm_activity",
        "support_ticket",
        "commerce",
        "future_source",
        "",
    ] {
        assert!(!is_commerce_inbox_target(other), "{other}");
    }
    // No other delegate owns the commerce source.
    assert!(!is_calendar_inbox_target("commerce_order"));
    assert!(!is_crm_inbox_target("commerce_order"));
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&CommerceInboxTargets::new(None, None));
    assert_inbox_target_port(&CommerceInboxTargets::default());
    assert_inbox_target_port(
        &CommerceInboxTargets::new(Some("branch"), Some("department")).assignment(Some("grant")),
    );
}

#[test]
fn commerce_hrefs_encode_exactly_like_the_typescript_projection() {
    assert_eq!(commerce_order_href(ID), "/commerce/orders/seg%2Fone%20two");
    assert_eq!(commerce_order_href("ord-1"), "/commerce/orders/ord-1");
    assert_eq!(commerce_order_href("😀"), "/commerce/orders/%F0%9F%98%80");
    assert_eq!(source_record_label("commerce_order"), "Commerce order");
}

#[test]
fn commerce_kind_capability_table_matches_the_typescript_oracle() {
    assert_eq!(commerce_order_capability("quote"), Some("sales.quotes"));
    assert_eq!(commerce_order_capability("sales"), Some("sales.orders"));
    assert_eq!(
        commerce_order_capability("purchase"),
        Some("supply.purchasing")
    );
    assert_eq!(commerce_order_capability("ecommerce"), None);
    assert_eq!(commerce_order_capability(""), None);
}

#[test]
fn finance_slice_routes_to_the_native_delegate() {
    assert!(is_finance_inbox_target("invoice"));
    assert!(is_finance_inbox_target("journal"));
    assert!(is_finance_inbox_target("refund"));
    for other in [
        "notice",
        "collaboration_action",
        "calendar_appointment",
        "crm_lead",
        "crm_activity",
        "commerce_order",
        "supplier_batch",
        "support_ticket",
        "finance",
        "future_source",
        "",
    ] {
        assert!(!is_finance_inbox_target(other), "{other}");
    }
    // No other delegate owns the finance sources.
    assert!(!is_calendar_inbox_target("invoice"));
    assert!(!is_crm_inbox_target("journal"));
    assert!(!is_commerce_inbox_target("refund"));
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&FinanceInboxTargets::new(None, None));
    assert_inbox_target_port(&FinanceInboxTargets::default());
    assert_inbox_target_port(
        &FinanceInboxTargets::new(Some("branch"), Some("department")).assignment(Some("grant")),
    );
}

#[test]
fn finance_hrefs_encode_exactly_like_the_typescript_projection() {
    assert_eq!(finance_invoice_href(ID), "/finance/invoices/seg%2Fone%20two");
    assert_eq!(finance_invoice_href("inv-1"), "/finance/invoices/inv-1");
    assert_eq!(finance_journal_href(ID), "/finance/journals/seg%2Fone%20two");
    assert_eq!(finance_journal_href("jou-1"), "/finance/journals/jou-1");
    assert_eq!(finance_refund_href(ID), "/finance/refunds/seg%2Fone%20two");
    assert_eq!(finance_refund_href("ref-1"), "/finance/refunds/ref-1");
    assert_eq!(finance_journal_href("😀"), "/finance/journals/%F0%9F%98%80");
    assert_eq!(source_record_label("invoice"), "Invoice");
    assert_eq!(source_record_label("journal"), "Journal");
    assert_eq!(source_record_label("refund"), "Refund");
}

#[test]
fn finance_document_capability_table_matches_the_retained_bases() {
    assert_eq!(
        finance_document_capability("invoice"),
        Some("finance.accounting")
    );
    assert_eq!(
        finance_document_capability("journal"),
        Some("finance.accounting")
    );
    assert_eq!(
        finance_document_capability("refund"),
        Some("finance.banking")
    );
    assert_eq!(finance_document_capability("supplier_batch"), None);
    assert_eq!(finance_document_capability("payment"), None);
    assert_eq!(finance_document_capability(""), None);
}

#[test]
fn payroll_slice_routes_to_the_native_delegate() {
    assert!(is_payroll_inbox_target("payroll_correction"));
    assert!(is_payroll_inbox_target("payroll_advance"));
    for other in [
        "notice",
        "collaboration_action",
        "calendar_appointment",
        "crm_lead",
        "crm_activity",
        "commerce_order",
        "invoice",
        "refund",
        "supplier_batch",
        "support_ticket",
        "payroll",
        "future_source",
        "",
    ] {
        assert!(!is_payroll_inbox_target(other), "{other}");
    }
    // No other delegate owns the payroll sources.
    assert!(!is_calendar_inbox_target("payroll_correction"));
    assert!(!is_crm_inbox_target("payroll_advance"));
    assert!(!is_commerce_inbox_target("payroll_correction"));
    assert!(!is_finance_inbox_target("payroll_advance"));
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&PayrollInboxTargets::new(None, None));
    assert_inbox_target_port(&PayrollInboxTargets::default());
    assert_inbox_target_port(
        &PayrollInboxTargets::new(Some("branch"), Some("department")).assignment(Some("grant")),
    );
}

#[test]
fn payroll_hrefs_encode_both_segments_exactly_like_the_typescript_projection() {
    assert_eq!(
        payroll_correction_href(ID, "cor-1"),
        "/people/payroll/runs/seg%2Fone%20two#correction-cor-1"
    );
    assert_eq!(
        payroll_correction_href("run-1", ID),
        "/people/payroll/runs/run-1#correction-seg%2Fone%20two"
    );
    assert_eq!(
        payroll_advance_href(ID),
        "/people/payroll/advances#payroll-advance-seg%2Fone%20two"
    );
    assert_eq!(
        payroll_advance_href("adv-1"),
        "/people/payroll/advances#payroll-advance-adv-1"
    );
    assert_eq!(source_record_label("payroll_correction"), "Payroll correction");
    assert_eq!(source_record_label("payroll_advance"), "Payroll advance");
}

#[test]
fn payroll_layer_selection_follows_the_enforced_mode_check() {
    assert!(uses_enforced_layer(PackageEnforcementMode::Enforced));
    assert!(!uses_enforced_layer(PackageEnforcementMode::Disabled));
    assert!(!uses_enforced_layer(PackageEnforcementMode::Shadow));
}

#[test]
fn people_slice_routes_to_the_native_delegate() {
    for source_type in [
        "roster_week",
        "roster_request",
        "roster_coverage",
        "overtime",
        "leave",
        "leave_cancellation",
        "expense",
        "retirement_clearance",
    ] {
        assert!(is_people_inbox_target(source_type), "{source_type}");
    }
    for other in [
        "notice",
        "collaboration_action",
        "calendar_appointment",
        "crm_lead",
        "commerce_order",
        "invoice",
        "payroll_correction",
        "request",
        "task",
        "post",
        "support_ticket",
        "people",
        "future_source",
        "",
    ] {
        assert!(!is_people_inbox_target(other), "{other}");
    }
    // No other delegate owns the people sources.
    assert!(!is_calendar_inbox_target("overtime"));
    assert!(!is_crm_inbox_target("leave"));
    assert!(!is_commerce_inbox_target("expense"));
    assert!(!is_finance_inbox_target("retirement_clearance"));
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&PeopleInboxTargets::new(None, None));
    assert_inbox_target_port(&PeopleInboxTargets::default());
    assert_inbox_target_port(
        &PeopleInboxTargets::new(Some("branch"), Some("department")).assignment(Some("grant")),
    );
}

#[test]
fn people_hrefs_encode_exactly_like_the_typescript_projection() {
    assert_eq!(
        roster_week_href("dep-1", "2026-11-16"),
        "/people/roster?department=dep-1&week=2026-11-16"
    );
    assert_eq!(
        roster_week_href("a/b c", "2026-11-16"),
        "/people/roster?department=a%2Fb%20c&week=2026-11-16"
    );
    assert_eq!(
        roster_request_href("dep-1", "2026-11-16", "req-1"),
        "/people/roster?department=dep-1&week=2026-11-16&view=requests&request=req-1"
    );
    assert_eq!(
        roster_request_href("dep-1", "2026-11-16", "a/b"),
        "/people/roster?department=dep-1&week=2026-11-16&view=requests&request=a%2Fb"
    );
    // Overtime/leave/expense hrefs carry the raw record id, no encoding.
    assert_eq!(overtime_href("ovt-1/2"), "/overtime/ovt-1/2");
    assert_eq!(leave_href("lea-1/2"), "/tasks/people_payroll/lea-1/2");
    assert_eq!(expense_href("exp-1/2"), "/expenses/exp-1/2");
    assert_eq!(
        retirement_case_href("case-1"),
        "/people/clearance/case-1"
    );
    assert_eq!(source_record_label("leave"), "Leave");
    assert_eq!(source_record_label("expense"), "Expense");
    assert_eq!(source_record_label("overtime"), "Overtime");
    assert_eq!(source_record_label("retirement_clearance"), "Retirement clearance");
}

#[test]
fn people_coverage_ids_parse_byte_identically_to_the_typescript_regex() {
    assert_eq!(
        parse_roster_coverage_id("dep-1:2026-11-16:person-1"),
        Some(("dep-1", "2026-11-16", "person-1"))
    );
    assert_eq!(
        parse_roster_coverage_id("a:b:2026-11-16:p1"),
        Some(("a:b", "2026-11-16", "p1"))
    );
    assert_eq!(
        parse_roster_coverage_id(":2026-11-16:p1"),
        Some(("", "2026-11-16", "p1"))
    );
    for invalid in [
        "dep-1:2026-11-16:",
        "dep-1:2026-11-16",
        "dep-1:not-a-date:p1",
        "dep-1:2026-11-16:two:parts:extra",
        "",
    ] {
        assert_eq!(parse_roster_coverage_id(invalid), None, "{invalid}");
    }
}

#[test]
fn people_mondays_and_bangkok_weeks_match_the_retained_calendar() {
    assert!(is_roster_monday("2026-11-16"));
    assert!(is_roster_monday("2026-11-23"));
    for not_monday in ["2026-11-17", "2026-11-22", "2026-02-29", "not-a-date", ""] {
        assert!(!is_roster_monday(not_monday), "{not_monday}");
    }
    // 2026-11-22T12:00:00+07:00 is a Sunday: week starts 2026-11-16.
    assert_eq!(
        bangkok_coverage_weeks(1_795_323_600_000),
        Some(("2026-11-16".into(), "2026-11-23".into()))
    );
}

#[test]
fn support_slice_routes_to_the_native_delegate() {
    for source_type in [
        "support_sla_alert",
        "support_incident",
        "support_policy",
        "support_ticket",
        "support_session",
    ] {
        assert!(is_support_inbox_target(source_type), "{source_type}");
    }
    for other in [
        "notice",
        "collaboration_action",
        "calendar_appointment",
        "crm_lead",
        "commerce_order",
        "invoice",
        "payroll_correction",
        "roster_week",
        "overtime",
        "leave",
        "support",
        "future_source",
        "",
    ] {
        assert!(!is_support_inbox_target(other), "{other}");
    }
    // No other delegate owns the support sources.
    assert!(!is_calendar_inbox_target("support_ticket"));
    assert!(!is_crm_inbox_target("support_ticket"));
    assert!(!is_commerce_inbox_target("support_ticket"));
    assert!(!is_finance_inbox_target("support_session"));
    assert!(!is_payroll_inbox_target("support_policy"));
    assert!(!is_people_inbox_target("support_sla_alert"));
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&SupportInboxTargets::new(None, None));
    assert_inbox_target_port(&SupportInboxTargets::default());
    assert_inbox_target_port(
        &SupportInboxTargets::new(Some("branch"), Some("department")).assignment(Some("grant")),
    );
}

#[test]
fn support_hrefs_encode_exactly_like_the_typescript_projection() {
    assert_eq!(support_sla_href(), "/support/sla");
    // Incident hrefs carry the raw id, no encoding.
    assert_eq!(support_incident_href("inc-1/2"), "/support/incidents/inc-1/2");
    assert_eq!(support_ticket_href("t-1"), "/support/t-1");
    assert_eq!(support_ticket_href(ID), format!("/support/{ENCODED}"));
    assert_eq!(support_session_href("ses-1"), "/support/sessions/ses-1");
    assert_eq!(
        support_session_href(ID),
        format!("/support/sessions/{ENCODED}")
    );
    assert_eq!(source_record_label("support_ticket"), "Support ticket");
    assert_eq!(source_record_label("support_session"), "Support session");
    assert_eq!(source_record_label("support_sla_alert"), "Support sla alert");
}

#[test]
fn support_kind_capability_table_matches_the_retained_gates() {
    assert_eq!(
        support_history_capability("support_sla_alert"),
        Some("support.sla")
    );
    assert_eq!(support_history_capability("support_policy"), Some("support.sla"));
    assert_eq!(
        support_history_capability("support_ticket"),
        Some("support.tickets")
    );
    assert_eq!(
        support_history_capability("support_session"),
        Some("support.tickets")
    );
    assert_eq!(
        support_history_capability("support_incident"),
        Some("support.incidents")
    );
    assert_eq!(support_history_capability("support"), None);
    assert_eq!(support_history_capability(""), None);
}

#[test]
fn procurement_slice_routes_to_the_native_delegate() {
    for source_type in ["supplier_bill", "procurement_rfq", "supplier_promise"] {
        assert!(is_procurement_inbox_target(source_type), "{source_type}");
    }
    for other in [
        "notice",
        "collaboration_action",
        "calendar_appointment",
        "crm_lead",
        "commerce_order",
        "invoice",
        "payroll_correction",
        "roster_week",
        "support_ticket",
        "inventory_transit",
        "procurement",
        "future_source",
        "",
    ] {
        assert!(!is_procurement_inbox_target(other), "{other}");
    }
    // No other delegate owns the procurement sources; inventory transit
    // stays inventory-owned.
    assert!(!is_calendar_inbox_target("supplier_bill"));
    assert!(!is_crm_inbox_target("procurement_rfq"));
    assert!(!is_commerce_inbox_target("supplier_promise"));
    assert!(!is_finance_inbox_target("supplier_bill"));
    assert!(!is_support_inbox_target("procurement_rfq"));
    // The native delegate satisfies the composition port bound by
    // `PgInboxTargets::with_delegate` at compile time.
    assert_inbox_target_port(&ProcurementInboxTargets::new(None, None));
    assert_inbox_target_port(&ProcurementInboxTargets::default());
    assert_inbox_target_port(
        &ProcurementInboxTargets::new(Some("branch"), Some("department")).assignment(Some("grant")),
    );
}

#[test]
fn procurement_hrefs_encode_exactly_like_the_typescript_projection() {
    assert_eq!(supplier_bill_href("bill-1"), "/supplier-bills/bill-1");
    assert_eq!(supplier_bill_href(ID), format!("/supplier-bills/{ENCODED}"));
    assert_eq!(procurement_rfq_href("rfq-1"), "/procurement-rfqs/rfq-1");
    assert_eq!(procurement_rfq_href(ID), format!("/procurement-rfqs/{ENCODED}"));
    assert_eq!(
        supplier_promise_href("ord-1"),
        "/commerce/orders/ord-1#supplier-promises"
    );
    assert_eq!(
        supplier_promise_href(ID),
        format!("/commerce/orders/{ENCODED}#supplier-promises")
    );
    assert_eq!(source_record_label("supplier_bill"), "Supplier bill");
    assert_eq!(source_record_label("procurement_rfq"), "Procurement rfq");
    assert_eq!(source_record_label("supplier_promise"), "Supplier promise");
}

#[test]
fn procurement_kind_capability_table_matches_the_retained_gates() {
    assert_eq!(
        procurement_history_capability("supplier_bill"),
        Some("supply.purchasing")
    );
    assert_eq!(
        procurement_history_capability("procurement_rfq"),
        Some("supply.purchasing")
    );
    assert_eq!(
        procurement_history_capability("supplier_promise"),
        Some("supply.purchasing")
    );
    assert_eq!(procurement_history_capability("inventory_transit"), None);
    assert_eq!(procurement_history_capability(""), None);
}
