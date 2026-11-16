import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

import "server-only";

import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";

const policyRecipient = supportRecipientEligibility().replaceAll('t.company_id', 'p.company_id').replaceAll('t.owner_id', 'm.user_id').replaceAll('t.branch_id', 'NULL::text').replaceAll('t.department_id', 'NULL::text');
const reviewerSql = `m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND ${policyRecipient}`;
export function policyReviewersNotificationSource(){return ` SELECT DISTINCT ?||m.user_id,p.company_id,m.user_id,'support_policy',p.id,p.version,'Review a proposed working-time resolution policy',p.updated_at FROM support_sla_policies p JOIN (${identity_accessProjectionSources.assignments}) m ON ${reviewerSql} WHERE p.id=?`;}
export function policyUpdatedNotificationSource(){return " SELECT ?,company_id,created_by,'support_policy',id,version,'SLA policy reviewed: check its current availability',updated_at FROM support_sla_policies WHERE id=?";}
export function ticketClockAttachedNotificationSource(){return " SELECT ?,company_id,owner_id,'support_ticket',id,version,'Approved working-time target attached: review the deadline',updated_at FROM support_tickets WHERE id=?";}
