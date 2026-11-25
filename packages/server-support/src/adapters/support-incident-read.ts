import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type {Row} from '@erpx/server-commerce/public/commerce';

import { eligibilitySelect, supportEligibleFlagSql, supportReviewerCountSql, supportAssignmentAccessSql } from "./eligibility-projection-sources";

import { createGetSupportIncident } from "@erpx/server-support/application/support-incidents";
import{nativeSupportReadRuntime}from '@erpx/server-support/adapters/read-runtime';
import{nativeSupportReadFirst,SUPPORT_RD_INCIDENT_BY_ID}from '@erpx/server-support/adapters/native-support-read';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
/** Canonical owner-local eligibility projection for the Support incident work list.
 * The by-id reader is native; this prefix is shared so the rendered SQL stays
 * byte-identical for the remaining work-list caller. */
export const incidentEligibilitySelect = (ownerEligible: string, reviewerPredicate: string) =>
    eligibilitySelect(`CASE WHEN ${ownerEligible} THEN 1 ELSE 0 END owner_eligible,${supportEligibleFlagSql(reviewerPredicate, 'reviewer_eligible')},${supportReviewerCountSql(reviewerPredicate)} reviewer_count`);
export function supportIncidentReadPredicate(a: WorkContext, alias = 't') {
    return { sql: supportAssignmentAccessSql(alias, '  ', `  AND iam_role_allows(g.role,g.company_id,'support.read') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id)`), bindings: [a.assignmentId, a.userId, a.role] };
}
export async function getSupportIncident(a: WorkContext, id: string, provided?:SqlDatabase) {
    const runtime = nativeSupportReadRuntime(a,provided);
    await runtime.requirePurchase("support.incidents");
    return createGetSupportIncident({ ...runtime, incident: async (a, id) => {
            const write = await runtime.database();
            return nativeSupportReadFirst<Row>(SUPPORT_RD_INCIDENT_BY_ID, [a.userId, id, ...args(a), ...supportIncidentReadPredicate(a).bindings], write);
        } })(a, id);
}
