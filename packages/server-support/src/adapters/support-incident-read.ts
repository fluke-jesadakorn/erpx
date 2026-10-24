import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type {Row} from '@erpx/server-commerce/public/commerce';

import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";

import { createGetSupportIncident } from "@erpx/server-support/application/support-incidents";
import{nativeSupportReadRuntime}from '@erpx/server-support/adapters/read-runtime';
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
const eligible = supportRecipientEligibility();
const reviewer = `m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${eligible.replaceAll('t.owner_id', 'm.user_id')}`;
export function supportIncidentReadPredicate(a: WorkContext, alias = 't') {
    return { sql: `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
  AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`, bindings: [a.assignmentId, a.userId, a.role] };
}
export async function getSupportIncident(a: WorkContext, id: string, provided?:SqlDatabase) {
    const runtime = nativeSupportReadRuntime(a,provided);
    await runtime.requirePurchase("support.incidents");
    return createGetSupportIncident({ ...runtime, incident: async (a, id) => {
            const write = await runtime.database(), permission = supportIncidentReadPredicate(a);
            return await write.prepare(`SELECT t.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,
  CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewer} AND m.user_id=?) THEN 1 ELSE 0 END reviewer_eligible,
  (SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewer}) reviewer_count
  FROM support_incidents t WHERE t.id=? AND ${scope} AND ${permission.sql}`).bind(a.userId, id, ...args(a), ...permission.bindings).first<Row>();
        } })(a, id);
}
