export const staffNotificationSourceTypes = [
    "invoice", "journal", "organization_change", "company_business_acceptance", "customer_identity", "customer_duplicate", "customer_access",
    "support_sla_alert", "support_incident", "support_policy", "support_ticket", "support_session",
    "iam_access_review", "payroll_correction", "payroll_advance", "staff_task_alert", "maintenance_work", "maintenance_plan", "project_handoff",
    "field_service_order", "commerce_order", "collaboration_action", "operational_exception", "learning_assignment", "benefit_election",
    "talent_application", "marketing_candidate", "manufacturing_order", "crm_lead", "crm_activity", "landed_cost",
    "stock_count", "customer_followup", "supplier_bill", "procurement_rfq", "supplier_promise", "inventory_transit",
    "quality_hold", "quality_alert", "supplier_batch", "refund", "legal", "retirement_clearance",
    "roster_week", "roster_request", "roster_coverage", "overtime", "leave", "leave_cancellation", "expense", "request", "task", "post", "handbook", "handbook_article",
] as const;
export type StaffNotification = {
    id: string;
    record_type: string;
    record_id: string;
    record_version: number;
    message: string;
    created_at: number;
    read_at: number | null;
    href: string;
    source?: import("@/lib/records/source-record").SourceRecordReference;
};
