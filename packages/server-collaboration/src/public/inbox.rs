//! Native Inbox read port. Delivery/notice rows are owner-local; every other
//! source destination is a composition port and fails closed when absent.
pub const CONTRACT_VERSION: &str = "collaboration.inbox.v1";

pub use crate::adapters::inbox_http::{
    InboxCategoryCounts, InboxHttpBackend, InboxHttpFuture, InboxHttpReadReply,
    InboxHttpReadRequest, InboxHttpState, InboxHttpTarget, InboxItemData, InboxPageData,
    InboxQuery, InboxReadKind, InboxSummaryData, inbox_router,
};
pub use crate::adapters::inbox_pg::{
    InboxCursor, InboxNotificationQuery, InboxScope, InboxTarget, InboxTargetFuture,
    InboxTargetPort, PgInboxUnit, UnavailableInboxTargets, unavailable_targets,
};
pub use crate::adapters::inbox_targets::{
    ChainedInboxTargets, InboxTargetVisibility, PgInboxTargets, RetainedInboxDestination,
    encode_uri_component, is_owned_inbox_target, retained_inbox_destination, source_record_label,
};
pub use crate::adapters::inbox_calendar::{
    CalendarInboxTargets, calendar_appointment_href, is_calendar_inbox_target,
};
pub use crate::adapters::inbox_crm::{
    CrmInboxTargets, crm_activity_href, crm_lead_href, is_crm_inbox_target, require_purchase_history,
};
pub use crate::adapters::inbox_commerce::{
    CommerceInboxTargets, commerce_order_capability, commerce_order_href, is_commerce_inbox_target,
};
pub use crate::adapters::inbox_finance::{
    FinanceInboxTargets, finance_document_capability, finance_invoice_href, finance_journal_href,
    finance_refund_href, is_finance_inbox_target,
};
pub use crate::adapters::inbox_payroll::{
    PayrollInboxTargets, is_payroll_inbox_target, payroll_advance_href, payroll_correction_href,
    uses_enforced_layer,
};
pub use crate::adapters::inbox_people::{
    PeopleInboxTargets, bangkok_coverage_weeks, expense_href, is_people_inbox_target,
    is_roster_monday, leave_href, overtime_href, parse_roster_coverage_id, retirement_case_href,
    roster_request_href, roster_week_href,
};
pub use crate::adapters::inbox_support::{
    SupportInboxTargets, is_support_inbox_target, support_history_capability, support_incident_href,
    support_session_href, support_sla_href, support_ticket_href,
};
pub use crate::adapters::inbox_procurement::{
    ProcurementInboxTargets, is_procurement_inbox_target, procurement_history_capability,
    procurement_rfq_href, supplier_bill_href, supplier_promise_href,
};
pub use crate::adapters::inbox_inventory::{
    InventoryInboxTargets, customer_followup_href, inventory_history_capabilities,
    inventory_transit_href, is_inventory_inbox_target, landed_cost_href, stock_count_href,
};
pub use crate::adapters::inbox_quality::{
    QualityInboxTargets, is_quality_inbox_target, legal_kind_capability, legal_record_href,
    quality_alert_href, quality_history_capabilities, quality_hold_href,
};
