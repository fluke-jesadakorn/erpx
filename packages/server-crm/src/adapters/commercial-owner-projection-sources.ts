import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  crm_leads: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"contact_name\",\"contact_detail\",\"source\",\"brief\",\"owner_id\",\"due_date\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"account_id\",\"contact_id\" FROM crm_leads",
} as const;
