import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type {Row} from '@erpx/commerce-server/public/commerce';

import { supportAssignmentAccessSql } from "./eligibility-projection-sources";

import { createGetSupportIncident } from "@erpx/support-server/application/support-incidents";
import{nativeSupportReadRuntime}from '@erpx/support-server/adapters/read-runtime';
import{nativeSupportReadFirst,SUPPORT_RD_INCIDENT_BY_ID}from '@erpx/support-server/adapters/native-support-read';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
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
