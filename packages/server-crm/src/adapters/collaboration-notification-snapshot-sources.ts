import 'server-only';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. */
export function collaborationNotificationSnapshotSources(){return {
  "crm_notifications_crm_leads_from": "FROM (SELECT \"id\", \"company_id\", \"branch_id\", \"department_id\", \"title\", \"contact_name\", \"contact_detail\", \"source\", \"brief\", \"owner_id\", \"due_date\", \"status\", \"version\", \"created_by\", \"created_at\", \"updated_at\", \"account_id\", \"contact_id\" FROM crm_leads) crm_leads",
  "crm_notifications_crm_lead_activities_from": "FROM (SELECT \"id\", \"company_id\", \"lead_id\", \"activity_type\", \"subject\", \"details\", \"owner_id\", \"due_date\", \"status\", \"outcome\", \"version\", \"created_by\", \"created_at\", \"updated_at\" FROM crm_lead_activities) crm_lead_activities"
} as const;}
