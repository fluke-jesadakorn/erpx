import "server-only";
/** Canonical owner-local fixed SQL for the Records employee-request projection.
 * Admitted once and shared by every Records projection consumer; the native
 * owner states the same fixed selection. No duplicate TS literal remains. */
export const employeeRequestsProjectionSql = "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"category\",\"title\",\"description\",\"location\",\"impact\",\"needed_date\",\"items_json\",\"currency\",\"requester_user_id\",\"created_by\",\"parent_request_id\",\"state\",\"team_id\",\"assignee_user_id\",\"routing_json\",\"approval_json\",\"blocked_reason\",\"resolution\",\"purchase_order_id\",\"handover_json\",\"version\",\"evidence_version\",\"created_at\",\"updated_at\",\"engagement_id\" FROM employee_requests";
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  employee_requests: employeeRequestsProjectionSql,
} as const);
