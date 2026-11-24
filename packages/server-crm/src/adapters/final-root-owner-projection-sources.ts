import { commercialOwnerProjectionSources, crmProjectionSql, crmCaseEventColumns, crmLeadActivityColumns, crmOpportunityColumns } from "./commercial-owner-projection-sources";
import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller.
 *
 * The crm_leads projection is CRM-owned fixed SQL also served by the
 * commercial-owner source. Both public surfaces reuse one statement so the two
 * callers cannot drift into separate query text. All four projections render
 * through the one CRM column-list renderer so their SQL text stays single-sourced. */
export const finalRootOwnerProjectionSources = {
  crm_case_events: crmProjectionSql('crm_case_events',crmCaseEventColumns),
  crm_lead_activities: crmProjectionSql('crm_lead_activities',crmLeadActivityColumns),
  crm_leads: commercialOwnerProjectionSources.crm_leads,
  crm_opportunities: crmProjectionSql('crm_opportunities',crmOpportunityColumns),
} as const;
