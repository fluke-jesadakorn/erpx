import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import { supportAlerts } from "@erpx/server-support/adapters/support-sla-alerts";
import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { supportEligibleFlagSql, supportReviewerCountSql } from "./eligibility-projection-sources";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { nativeSupportReadAll, SUPPORT_RD_SLA_WORKSPACE_TICKETS } from "@erpx/server-support/adapters/native-support-read";
import { createGetSupportPolicy, createSupportSlaWorkspace, createSupportPolicyWork } from "@erpx/server-support/application/support-sla";
export { supportPolicyCreateSchema, supportPolicyChangeSchema, supportClockAttachSchema, supportClockValue } from "@erpx/server-support/domain/support-sla";
const policyRecipient = supportRecipientEligibility().replaceAll('t.company_id', 'p.company_id').replaceAll('t.owner_id', 'm.user_id').replaceAll('t.branch_id', 'NULL::text').replaceAll('t.department_id', 'NULL::text');
const reviewerSql = `m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND ${policyRecipient}`;
/** Owner-local fixed projection for Support SLA policy eligibility. The native
 * `/support/sla` read is the authority and already returns this policy shape;
 * keeping one projection for both the workspace and the work-item source
 * removes the duplicated query text and stops the two call sites from
 * drifting apart. */
const policyEligibilitySelect = (withReviewers: boolean) =>
    `SELECT p.*,${supportEligibleFlagSql(reviewerSql, 'actor_eligible')}${withReviewers ? `,${supportReviewerCountSql(reviewerSql)} reviewers` : ""}`;
export async function getSupportPolicy(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a); await runtime.requirePurchase("support.sla"); return createGetSupportPolicy({ ...runtime, policy: async (a, id) => { const write = await runtime.database(); return await write.prepare('SELECT * FROM support_sla_policies WHERE id=? AND company_id=?').bind(id, a.companyId).first<Row>(); } })(a, id); }
export async function supportSlaWorkspace(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.sla"); const data = await createSupportSlaWorkspace({ ...runtime, alerts: supportAlerts, policies: async (a) => { const write = await runtime.database(); return (await write.prepare(`${policyEligibilitySelect(false)} FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.created_at DESC,p.id`).bind(a.userId, a.companyId).all<Row>()).results; }, tickets: async (a) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_SLA_WORKSPACE_TICKETS, [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId], write)).results; } })(a); return { ...data, canCreate: data.canManage && access.newWork, canManage: data.canManage && access.settleExisting, policies: data.policies.map(policy => ({ ...policy, canApprove: policy.canApprove && access.newWork })) }; }
export async function supportPolicyWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.sla"); if (!access.history) return []; const items = await createSupportPolicyWork({ policies: async (a) => { const write = await runtime.database(); return (await write.prepare(`${policyEligibilitySelect(true)} FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.updated_at DESC`).bind(a.userId, a.companyId).all<Row>()).results; } })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.newWork, queue: item.queue === "todo" && !access.newWork ? "waiting" as const : item.queue })); }
