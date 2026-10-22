import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commerceFinalOwnerProjectionSources = {
  employee_requests: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"category\",\"title\",\"description\",\"location\",\"impact\",\"needed_date\",\"items_json\",\"currency\",\"requester_user_id\",\"created_by\",\"parent_request_id\",\"state\",\"team_id\",\"assignee_user_id\",\"routing_json\",\"approval_json\",\"blocked_reason\",\"resolution\",\"purchase_order_id\",\"handover_json\",\"version\",\"evidence_version\",\"created_at\",\"updated_at\",\"engagement_id\" FROM employee_requests",
} as const;
