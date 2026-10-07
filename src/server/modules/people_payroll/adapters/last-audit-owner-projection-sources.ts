import "server-only";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  leave_requests: "SELECT \"id\",\"number\",\"employee_id\",\"company_id\",\"branch_id\",\"start_date\",\"days\",\"handover_note\",\"state\",\"version\",\"created_by\",\"prepared_by\",\"decided_by\",\"decision_note\",\"last_command_id\",\"created_at\",\"updated_at\",\"department_id\",\"person_id\",\"engagement_id\",\"leave_type_id\",\"end_date\",\"start_segment\",\"end_segment\",\"policy_version_id\",\"calculated_units\",\"calculation_snapshot\",\"cancellation_state\",\"evidence_policy_snapshot\",\"evidence_due_at\",\"evidence_version\",\"submitted_at\" FROM leave_requests",
} as const;
