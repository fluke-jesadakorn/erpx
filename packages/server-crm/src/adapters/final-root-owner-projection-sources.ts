import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  crm_case_events: "SELECT \"id\",\"lead_id\",\"opportunity_id\",\"version\",\"action\",\"actor_id\",\"reason\",\"snapshot_json\",\"created_at\" FROM crm_case_events",
  crm_lead_activities: "SELECT \"id\",\"company_id\",\"lead_id\",\"activity_type\",\"subject\",\"details\",\"owner_id\",\"due_date\",\"status\",\"outcome\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM crm_lead_activities",
  crm_leads: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"contact_name\",\"contact_detail\",\"source\",\"brief\",\"owner_id\",\"due_date\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"account_id\",\"contact_id\" FROM crm_leads",
  crm_opportunities: "SELECT \"id\",\"lead_id\",\"customer_id\",\"stage\",\"expected_minor\",\"currency\",\"close_date\",\"quote_id\",\"quote_version\",\"sales_id\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"reservation_id\",\"win_source\",\"won_at\" FROM crm_opportunities",
} as const;
