

import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

import "server-only";

import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";

const recipient = `m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${supportRecipientEligibility().replaceAll('t.owner_id', 'm.user_id')}`;
export function slaBreachedNotificationSource(purchased = "TRUE"){return `
   SELECT DISTINCT s.id||':'||m.user_id,t.company_id,m.user_id,'support_sla_alert',s.id,s.version,
    'Resolution target breached: take responsibility for follow-up',?::bigint
   FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id JOIN (${identity_accessProjectionSources.assignments}) m ON ${recipient}
   WHERE s.acknowledged_by IS NULL AND ${scheduledFeaturePredicate('t.company_id', 'support')} AND ${purchased}`;}
