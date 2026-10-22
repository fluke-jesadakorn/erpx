import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  support_tickets: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"order_id\",\"customer_id\",\"title\",\"issue\",\"owner_id\",\"due_date\",\"status\",\"blocked_reason\",\"resolution\",\"resolved_by\",\"resolved_at\",\"confirmation_note_id\",\"confirmation_note_version\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"sla_policy_id\",\"sla_policy_version\",\"sla_clock_json\",\"source_kind\",\"confirmed_by\",\"confirmed_at\",\"confirmation_reason\",\"source_record_json\" FROM support_tickets",
} as const;
