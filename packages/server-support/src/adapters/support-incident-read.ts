import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type {Row} from '@erpx/server-commerce/public/commerce';

import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { supportEligibleFlagSql, supportReviewerCountSql, supportAssignmentAccessSql } from "./eligibility-projection-sources";

import { createGetSupportIncident } from "@erpx/server-support/application/support-incidents";
import{nativeSupportReadRuntime}from '@erpx/server-support/adapters/read-runtime';
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
const eligible = supportRecipientEligibility();
const reviewer = `m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${eligible.replaceAll('t.owner_id', 'm.user_id')}`;
/** Canonical owner-local eligibility projection for Support incident reads.
 * One fixed text serves the by-id reader and the work-list reader so the two
 * call sites cannot drift; the native incident reader remains the authority. */
export const incidentEligibilitySelect = (ownerEligible: string, reviewerPredicate: string) =>
    `SELECT t.*,CASE WHEN ${ownerEligible} THEN 1 ELSE 0 END owner_eligible,${supportEligibleFlagSql(reviewerPredicate, 'reviewer_eligible')},${supportReviewerCountSql(reviewerPredicate)} reviewer_count`;
export function supportIncidentReadPredicate(a: WorkContext, alias = 't') {
    return { sql: supportAssignmentAccessSql(alias, '  ', `  AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id)`), bindings: [a.assignmentId, a.userId, a.role] };
}
export async function getSupportIncident(a: WorkContext, id: string, provided?:SqlDatabase) {
    const runtime = nativeSupportReadRuntime(a,provided);
    await runtime.requirePurchase("support.incidents");
    return createGetSupportIncident({ ...runtime, incident: async (a, id) => {
            const write = await runtime.database(), permission = supportIncidentReadPredicate(a);
            return await write.prepare(`${incidentEligibilitySelect(eligible, reviewer)}
  FROM support_incidents t WHERE t.id=? AND ${scope} AND ${permission.sql}`).bind(a.userId, id, ...args(a), ...permission.bindings).first<Row>();
        } })(a, id);
}
