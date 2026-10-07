import type { SupportAlert } from "../domain/support-sla-alerts";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { SupportCommandPorts, SupportCommand, SupportRow, SupportMutation } from "./command";
import { supportPolicyCreateSchema, supportPolicyChangeSchema, supportClockAttachSchema, supportClockValue, supportPolicyWorkItems, supportPolicyClock, type SupportPolicyCreate, type SupportPolicyChange, type SupportClockAttach } from "../domain/support-sla";
import { supportSlaPolicySchema, type SupportClock } from "../domain/support-clock";
function manager(ports: Pick<SupportCommandPorts, "authorize" | "error">, a: WorkContext) { ports.authorize(a, 'support.manage'); if (a.branchId || a.departmentId)
    throw ports.error(403, 'SUPPORT_COMPANY_POLICY_REQUIRED', 'Use a Company Support manager assignment to manage SLA policies.'); }
export function createGetSupportPolicy(ports: Pick<SupportCommandPorts, "authorize" | "error"> & {
    policy(a: WorkContext, id: string): Promise<SupportRow | null>;
}) { return async (a: WorkContext, id: string) => { ports.authorize(a, 'support.read'); const row = await ports.policy(a, id); if (!row)
    throw ports.error(404, 'SUPPORT_POLICY_NOT_FOUND', 'Policy not found in this Company.'); return row; }; }
export function createSupportSlaWorkspace(ports: Pick<SupportCommandPorts, "authorize" | "now"> & {
    policies(a: WorkContext): Promise<SupportRow[]>;
    tickets(a: WorkContext): Promise<SupportRow[]>;
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext) => { ports.authorize(a, 'support.read'); const policies = await ports.policies(a), items = await ports.tickets(a); return { alerts: await ports.alerts(a), policies: policies.map(p => ({ id: String(p.id), name: String(p.name), version: Number(p.version), status: String(p.status), createdBy: String(p.created_by), policy: supportSlaPolicySchema.parse(JSON.parse(String(p.policy_json))), canApprove: hasPermission(a, 'support.manage') && !a.branchId && !a.departmentId && !samePerson(a, p.created_by) && p.status === 'draft' && Number(p.actor_eligible) > 0 })), items: items.map(r => ({ id: String(r.id), title: String(r.title), status: String(r.status), ownerId: String(r.owner_id), policyId: String(r.sla_policy_id), policyVersion: Number(r.sla_policy_version), clock: supportClockValue(r)! })), canManage: hasPermission(a, 'support.manage') && !a.branchId && !a.departmentId, now: ports.now() }; }; }
export function createSupportPolicy(ports: SupportCommandPorts & {
    prepare(command: SupportCommand, input: SupportPolicyCreate): Promise<SupportMutation>;
}) { return async (a: WorkContext, raw: unknown, key: string) => { manager(ports, a); const input = supportPolicyCreateSchema.parse(raw), id = `support-policy-${key}`, op = 'support.sla.create', prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; const now = ports.now(); supportPolicyClock(input.policy, now, ports.error); const command: SupportCommand = { actor: a, key, operation: op, id, input, version: 1, result: { id, version: 1 }, now }; return ports.commit(command, await ports.prepare(command, input)); }; }
export function createChangeSupportPolicy(ports: SupportCommandPorts & {
    policy(a: WorkContext, id: string): Promise<SupportRow>;
    prepare(command: SupportCommand, input: SupportPolicyChange, row: SupportRow): Promise<SupportMutation>;
}) { return async (a: WorkContext, id: string, raw: unknown, key: string) => { manager(ports, a); const input = supportPolicyChangeSchema.parse(raw), op = `support.sla.${input.action}`, prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; const r = await ports.policy(a, id); if (input.action === 'approve' && samePerson(a, r.created_by))
    throw ports.error(403, 'INDEPENDENT_APPROVER_REQUIRED', 'Another Company Support manager must approve this policy.'); if (input.action === 'approve' ? r.status !== 'draft' : r.status === 'retired')
    throw ports.error(409, 'SUPPORT_POLICY_STATE', 'Review the current policy state.'); const command: SupportCommand = { actor: a, key, operation: op, id, input, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1 }, now: ports.now() }; return ports.commit(command, await ports.prepare(command, input, r)); }; }
export function createAttachSupportClock(ports: SupportCommandPorts & {
    ticket(a: WorkContext, id: string): Promise<SupportRow>;
    policy(a: WorkContext, id: string): Promise<SupportRow>;
    prepare(command: SupportCommand, input: SupportClockAttach, value: SupportClock): Promise<SupportMutation>;
}) { return async (a: WorkContext, id: string, raw: unknown, key: string) => { ports.authorize(a, 'support.manage'); const input = supportClockAttachSchema.parse(raw), op = 'support.sla.attach', prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; const r = await ports.ticket(a, id), policy = await ports.policy(a, input.policyId), now = ports.now(), value = supportPolicyClock(supportSlaPolicySchema.parse(JSON.parse(String(policy.policy_json))), now, ports.error); const command: SupportCommand = { actor: a, key, operation: op, id, input, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1 }, now, scope: { companyId: a.companyId, branchId: String(r.branch_id), departmentId: r.department_id ? String(r.department_id) : null } }; return ports.commit(command, await ports.prepare(command, input, value)); }; }
export function createSupportPolicyWork(ports: {
    policies(a: WorkContext): Promise<SupportRow[]>;
}) { return async (a: WorkContext) => !hasPermission(a, 'support.read') ? [] : supportPolicyWorkItems(a, await ports.policies(a)); }
