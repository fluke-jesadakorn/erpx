

import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

import 'server-only';

import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";

const eligible = supportRecipientEligibility();
export function ticketReopenedNotificationSource(){return `
    SELECT DISTINCT ?||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'New issue needs Support triage',t.updated_at
    FROM support_tickets t JOIN (${identity_accessProjectionSources.assignments}) m ON m.company_id=t.company_id
    WHERE t.id=? AND m.user_id<>t.created_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(m.role,m.company_id,'support.manage')
      AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
      AND ${eligible.replaceAll('t.owner_id', 'm.user_id')}`;}
export function ticketCreatedNotificationSource(){return " SELECT ?,company_id,owner_id,'support_ticket',id,version,'New customer support case: accept and review the issue',updated_at FROM support_tickets WHERE id=?";}
export function ticketUpdatedNotificationSource(){return " SELECT ?,company_id,owner_id,'support_ticket',id,version,'Support case updated: review current status and next action',updated_at FROM support_tickets WHERE id=? AND owner_id IS NOT NULL";}
export function ticketReviewersNotificationSource(){return ` SELECT DISTINCT ?||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'Review proposed issue resolution',t.updated_at FROM support_tickets t JOIN (${identity_accessProjectionSources.assignments}) m ON m.company_id=t.company_id WHERE t.id=? AND m.user_id<>t.owner_id AND m.user_id<>t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND ${eligible.replaceAll('t.owner_id', 'm.user_id')}`;}
