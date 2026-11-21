import type { SupportAlert } from "@erpx/server-support/domain/support-sla-alerts";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import type { SupportCommandPorts, SupportRow } from "@erpx/server-support/application/command";
import { supportClockValue, supportPolicyWorkItems } from "@erpx/server-support/domain/support-sla";
import { supportSlaPolicySchema, type SupportClock } from "@erpx/server-support/domain/support-clock";
export function createGetSupportPolicy(ports: Pick<SupportCommandPorts, "authorize" | "error"> & {
    policy(a: WorkContext, id: string): Promise<SupportRow | null>;
}) { return async (a: WorkContext, id: string) => { ports.authorize(a, 'support.read'); const row = await ports.policy(a, id); if (!row)
    throw ports.error(404, 'SUPPORT_POLICY_NOT_FOUND', 'Policy not found in this Company.'); return row; }; }
export function createSupportSlaWorkspace(ports: Pick<SupportCommandPorts, "authorize" | "now"> & {
    policies(a: WorkContext): Promise<SupportRow[]>;
    tickets(a: WorkContext): Promise<SupportRow[]>;
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext) => { ports.authorize(a, 'support.read'); const policies = await ports.policies(a), items = await ports.tickets(a); return { alerts: await ports.alerts(a), policies: policies.map(p => ({ id: String(p.id), name: String(p.name), version: Number(p.version), status: String(p.status), createdBy: String(p.created_by), policy: supportSlaPolicySchema.parse(JSON.parse(String(p.policy_json))), canApprove: hasPermission(a, 'support.manage') && !a.branchId && !a.departmentId && !samePerson(a, p.created_by) && p.status === 'draft' && Number(p.actor_eligible) > 0 })), items: items.map(r => ({ id: String(r.id), title: String(r.title), status: String(r.status), ownerId: String(r.owner_id), policyId: String(r.sla_policy_id), policyVersion: Number(r.sla_policy_version), clock: supportClockValue(r)! })), canManage: hasPermission(a, 'support.manage') && !a.branchId && !a.departmentId, now: ports.now() }; }; }
export function createSupportPolicyWork(ports: {
    policies(a: WorkContext): Promise<SupportRow[]>;
}) { return async (a: WorkContext) => !hasPermission(a, 'support.read') ? [] : supportPolicyWorkItems(a, await ports.policies(a)); }
