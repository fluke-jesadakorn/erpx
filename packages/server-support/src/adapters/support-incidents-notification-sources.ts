import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

import "server-only";

import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";

const eligible = supportRecipientEligibility();
const reviewer = `m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${eligible.replaceAll('t.owner_id', 'm.user_id')}`;
export function incidentUpdatedNotificationSource(){return `
  SELECT DISTINCT ?||u.user_id,t.company_id,u.user_id,'support_incident',t.id,t.version,'Incident updated: review impact and follow up each linked customer case',t.updated_at
  FROM support_incidents t JOIN LATERAL(SELECT t.owner_id user_id UNION SELECT s.owner_id FROM support_incident_tickets l JOIN support_tickets s ON s.id=l.ticket_id WHERE l.incident_id=t.id AND l.active UNION SELECT m.user_id FROM (${identity_accessProjectionSources.assignments}) m WHERE t.status='mitigated' AND ${reviewer}) u ON true
  WHERE t.id=?`;}
