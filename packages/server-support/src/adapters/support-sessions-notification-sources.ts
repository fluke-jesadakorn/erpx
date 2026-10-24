

import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

import "server-only";

export function sessionReviewersNotificationSource(){return ` SELECT DISTINCT ?||m.user_id,s.company_id,m.user_id,'support_session',s.id,s.version,'Review a temporary customer support session',s.updated_at FROM support_sessions s JOIN support_tickets t ON t.id=s.ticket_id JOIN (${identity_accessProjectionSources.assignments}) m ON m.company_id=s.company_id AND m.user_id<>s.created_by AND m.user_id<>s.requested_for AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) WHERE s.id=?`;}
export function sessionUpdatedNotificationSource(){return " SELECT ?,company_id,requested_for,'support_session',id,version,'Temporary support access changed: review current grants',updated_at FROM support_sessions WHERE id=?";}
