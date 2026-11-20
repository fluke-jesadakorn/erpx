pub mod legal_hold_notifications;
pub mod maintenance_notifications;

pub mod field_service_notifications;
pub mod service_count_flow_notification;

pub mod collaboration_work;

pub mod handbook;

pub mod handbook_http;

pub mod social;

pub mod social_upload_files;

pub mod messaging_native;

pub mod messaging_file_discard;

pub mod messaging_teams;

pub mod messaging_http;

pub mod social_http;

pub mod collaboration_work_http;

pub mod inbox_http;

pub mod inbox;

pub mod operations_notification;

pub mod field_outbox;

pub mod web_push;

pub mod field_reservation_notifications;

pub mod field_inbox;

pub mod service_invoice_flow_notification;

pub mod field_journal_flow_notification;

pub mod crm_activity_notification;
pub mod quality_notification;
pub mod supplier_bill_match_notification;

pub mod staff_facts;

pub mod roster_position_notifications;

pub mod pms_social_event;
pub mod customer_portal_notification;
pub mod rfq_notifications;

/// Collaboration-owned task-origin binding for native Staff Work commands.
pub mod task_source_origin {
    pub use crate::adapters::task_source_origin::{
        bind_task_origin, into_business, TaskOriginBinding,
    };
}

/// Native Collaboration published-post source policy port.
pub mod social_source_policy {
    pub use crate::adapters::social_source_policy::{
        into_business, read_published_post, source_guard, source_hash, PublishedPost, SourceItem,
    };
}

/// Native-owned notification statement port.
pub mod notification_statement {
    pub use crate::adapters::notification_statement::{
        crm_lead_notification_statement, learning_assignment_notification_statement, legal_approval_notification_statement, legal_hold_notification_statement, notification_statement,
        inventory_transit_notification_statement, social_task_origin_statement, workflow_invoicing_notification_statement, workflow_notification_statement, roster_coverage_notification_statement, legal_feature_notice_statement, legal_sla_notice_statement, legal_escalation_notice_statement, legal_deadline_notice_statement, social_mention_statement, social_mention_notice_statement, social_agent_reply_statement, social_topic_statement, social_topic_binding_statement, legal_law_source_notification_statement, company_acceptance_reviewer_statement, company_acceptance_result_statement, customer_portal_notification_statement, explicit_notification_statement, payroll_advance_notification_statement, payroll_correction_notification_statement,
        finance_refund_notification_statement, organization_change_notification_statement,
        rfq_notification_statement, supplier_bill_notification_statement, supplier_promise_notification_statement,
        NotificationStatement,
        NotificationStatementInput,
    };
}

/// Native notification statement HTTP ingress.
pub mod notification_statement_http {
    pub use crate::adapters::notification_statement_http::{
        notification_statement_router, holding_notification_statement_router, system_notification_statement_router, StatementBackend,
        StatementFuture, StatementRequest, StatementTarget,
    };
}
