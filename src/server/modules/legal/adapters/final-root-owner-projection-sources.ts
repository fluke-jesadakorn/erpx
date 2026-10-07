import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  legal_events: "SELECT \"id\",\"item_id\",\"action\",\"actor_id\",\"note\",\"version\",\"created_at\",\"snapshot\" FROM legal_events",
  legal_items: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"title\",\"description\",\"classification\",\"country\",\"jurisdiction\",\"language\",\"currency\",\"timezone\",\"priority\",\"due_date\",\"owner_id\",\"created_by\",\"state\",\"version\",\"created_at\",\"updated_at\",\"escalation_owner\",\"sla_started_at\",\"sla_due_at\",\"intake_form_id\",\"intake_answers\" FROM legal_items",
  legal_native_references: "SELECT \"item_id\",\"target_module\",\"target_id\",\"ordinal\",\"created_by\",\"created_at\" FROM legal_native_references",
} as const;
