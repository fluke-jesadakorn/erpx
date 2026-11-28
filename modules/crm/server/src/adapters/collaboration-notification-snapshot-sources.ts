import 'server-only';
import {crmProjectionSql,crmLeadColumns,crmLeadActivityColumns} from './commercial-owner-projection-sources';
/** Closed, named source clauses for the supplied notification statement snapshot.
 * No table, SQL, predicate or alias is accepted from a caller. Both clauses reuse
 * the CRM column-list renderer so their SQL text stays single-sourced. */
export function collaborationNotificationSnapshotSources(){return {
  "crm_notifications_crm_leads_from": `FROM (${crmProjectionSql('crm_leads',crmLeadColumns,', ')}) crm_leads`,
  "crm_notifications_crm_lead_activities_from": `FROM (${crmProjectionSql('crm_lead_activities',crmLeadActivityColumns,', ')}) crm_lead_activities`
} as const;}
