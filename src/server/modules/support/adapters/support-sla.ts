import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/final-root-owner-projection-sources";
import { supportNotificationEffects } from "../../collaboration/public/support-notification-effects";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Row } from "@/server/modules/commerce/public/commerce";
import { assertion } from "@/server/modules/workflow/public/commands";
import { getSupportTicket } from "./support";
import { supportAlerts } from "./support-sla-alerts";
import { supportRecipientEligibility } from "./support-recipient";
import { nativeSupportRuntime } from "./command-runtime";
import { createGetSupportPolicy, createSupportSlaWorkspace, createSupportPolicy as createPolicy, createChangeSupportPolicy, createAttachSupportClock, createSupportPolicyWork } from "../application/support-sla";
export { supportPolicyCreateSchema, supportPolicyChangeSchema, supportClockAttachSchema, supportClockValue } from "../domain/support-sla";
const policyRecipient = supportRecipientEligibility().replaceAll('t.company_id', 'p.company_id').replaceAll('t.owner_id', 'm.user_id').replaceAll('t.branch_id', 'NULL::text').replaceAll('t.department_id', 'NULL::text');
const reviewerSql = `m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND ${policyRecipient}`;
export async function getSupportPolicy(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a); return createGetSupportPolicy({ ...runtime, policy: async (a, id) => { const write = await runtime.database(); return await write.prepare('SELECT * FROM support_sla_policies WHERE id=? AND company_id=?').bind(id, a.companyId).first<Row>(); } })(a, id); }
export async function supportSlaWorkspace(a: WorkContext) { const runtime = nativeSupportRuntime(a); return createSupportSlaWorkspace({ ...runtime, alerts: supportAlerts, policies: async (a) => { const write = await runtime.database(); return (await write.prepare(`SELECT p.*,CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewerSql} AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.created_at DESC,p.id`).bind(a.userId, a.companyId).all<Row>()).results; }, tickets: async (a) => { const write = await runtime.database(); return (await write.prepare("SELECT id,title,status,owner_id,sla_policy_id,sla_policy_version,sla_clock_json FROM support_tickets WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND sla_clock_json IS NOT NULL AND status NOT IN ('closed','cancelled') ORDER BY updated_at DESC,id").bind(a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId).all<Row>()).results; } })(a); }
export async function createSupportPolicy(a: WorkContext, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createPolicy({ ...runtime, prepare: async (command, input) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            return { native: [
                    write.prepare('INSERT INTO support_sla_policies(id,company_id,name,policy_json,created_by,created_at,updated_at) VALUES(?,?,?,?::jsonb,?,?,?)').bind(id, a.companyId, input.name, JSON.stringify(input.policy), a.userId, now, now),
                    write.prepare("INSERT INTO support_sla_policy_events VALUES(?,?,1,'create',?,?,?)").bind(`support-policy-event-${key}`, id, a.userId, input.reason, now),
                    supportNotificationEffects(write).policyReviewers().bind(`support-policy-notice-${key}-`, id)
                ] };
        } })(a, raw, key);
}
export async function changeSupportPolicy(a: WorkContext, id: string, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createChangeSupportPolicy({ ...runtime, policy: getSupportPolicy, prepare: async (command, input, r) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            return { native: [
                    ...(input.action === 'approve' ? [assertion(write, key, 'eligible-reviewer', `EXISTS(SELECT 1 FROM support_sla_policies p JOIN (${identity_accessProjectionSources.assignments}) m ON ${reviewerSql} WHERE p.id=? AND m.user_id=?)`, [id, a.userId])] : []),
                    assertion(write, key, 'policy', "EXISTS(SELECT 1 FROM support_sla_policies WHERE id=? AND company_id=? AND version=? AND status=?)", [id, a.companyId, input.expectedVersion, r.status]),
                    write.prepare('UPDATE support_sla_policies SET status=?,approved_by=?,approved_at=?,version=version+1,updated_at=? WHERE id=?').bind(input.action === 'approve' ? 'approved' : 'retired', input.action === 'approve' ? a.userId : r.approved_by, input.action === 'approve' ? now : r.approved_at, now, id),
                    write.prepare('INSERT INTO support_sla_policy_events VALUES(?,?,?,?,?,?,?)').bind(`support-policy-event-${key}`, id, input.expectedVersion + 1, input.action, a.userId, input.reason, now),
                    supportNotificationEffects(write).policyUpdated().bind(`support-policy-notice-${key}`, id)
                ] };
        } })(a, id, raw, key);
}
export async function attachSupportClock(a: WorkContext, id: string, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createAttachSupportClock({ ...runtime, ticket: getSupportTicket, policy: getSupportPolicy, prepare: async (command, input, value) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            return { native: [
                    assertion(write, key, 'ticket', "EXISTS(SELECT 1 FROM support_tickets WHERE id=? AND version=? AND status='open' AND sla_policy_id IS NULL)", [id, input.expectedVersion]),
                    assertion(write, key, 'approved-policy', "EXISTS(SELECT 1 FROM support_sla_policies WHERE id=? AND company_id=? AND version=? AND status='approved')", [input.policyId, a.companyId, input.policyVersion]),
                    write.prepare('UPDATE support_tickets SET sla_policy_id=?,sla_policy_version=?,sla_clock_json=?::jsonb,version=version+1,updated_at=? WHERE id=?').bind(input.policyId, input.policyVersion, JSON.stringify(value), now, id),
                    write.prepare("INSERT INTO support_ticket_events SELECT ?,id,version,'sla-attach',?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?").bind(`support-sla-event-${key}`, a.userId, input.reason, JSON.stringify({ policyId: input.policyId, policyVersion: input.policyVersion, clock: value, reason: input.reason }), id),
                    supportNotificationEffects(write).ticketClockAttached().bind(`support-sla-notice-${key}`, id)
                ] };
        } })(a, id, raw, key);
}
export async function supportPolicyWork(a: WorkContext) { const runtime = nativeSupportRuntime(a); return createSupportPolicyWork({ policies: async (a) => { const write = await runtime.database(); return (await write.prepare(`SELECT p.*,CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewerSql} AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewerSql}) reviewers FROM support_sla_policies p WHERE p.company_id=? ORDER BY p.updated_at DESC`).bind(a.userId, a.companyId).all<Row>()).results; } })(a); }

export function policyReviewersNotificationSource(){return ` SELECT DISTINCT ?||m.user_id,p.company_id,m.user_id,'support_policy',p.id,p.version,'Review a proposed working-time resolution policy',p.updated_at FROM support_sla_policies p JOIN (${identity_accessProjectionSources.assignments}) m ON ${reviewerSql} WHERE p.id=?`;}
export function policyUpdatedNotificationSource(){return " SELECT ?,company_id,created_by,'support_policy',id,version,'SLA policy reviewed: check its current availability',updated_at FROM support_sla_policies WHERE id=?";}
export function ticketClockAttachedNotificationSource(){return " SELECT ?,company_id,owner_id,'support_ticket',id,version,'Approved working-time target attached: review the deadline',updated_at FROM support_tickets WHERE id=?";}
