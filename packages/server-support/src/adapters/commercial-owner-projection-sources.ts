import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  support_tickets: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"order_id\",\"customer_id\",\"title\",\"issue\",\"owner_id\",\"due_date\",\"status\",\"blocked_reason\",\"resolution\",\"resolved_by\",\"resolved_at\",\"confirmation_note_id\",\"confirmation_note_version\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"sla_policy_id\",\"sla_policy_version\",\"sla_clock_json\",\"source_kind\",\"confirmed_by\",\"confirmed_at\",\"confirmation_reason\",\"source_record_json\" FROM support_tickets",
} as const;
