import type { SupportAlert } from "@erpx/support-server/domain/support-sla-alerts";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { samePerson } from "@erpx/identity-access-ui/lib/iam";
import type { SupportCommandPorts, SupportRow } from "@erpx/support-server/application/command";
import { supportClockValue } from "@erpx/support-server/domain/support-sla";
import { supportSlaPolicySchema } from "@erpx/support-server/domain/support-clock";
export function createSupportSlaWorkspace(ports: Pick<SupportCommandPorts, "authorize" | "now"> & {
    policies(a: WorkContext): Promise<SupportRow[]>;
    tickets(a: WorkContext): Promise<SupportRow[]>;
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext) => { ports.authorize(a, 'support.read'); const policies = await ports.policies(a), items = await ports.tickets(a); return { alerts: await ports.alerts(a), policies: policies.map(p => ({ id: String(p.id), name: String(p.name), version: Number(p.version), status: String(p.status), createdBy: String(p.created_by), policy: supportSlaPolicySchema.parse(JSON.parse(String(p.policy_json))), canApprove: hasPermission(a, 'support.manage') && !a.branchId && !a.departmentId && !samePerson(a, p.created_by) && p.status === 'draft' && Number(p.actor_eligible) > 0 })), items: items.map(r => ({ id: String(r.id), title: String(r.title), status: String(r.status), ownerId: String(r.owner_id), policyId: String(r.sla_policy_id), policyVersion: Number(r.sla_policy_version), clock: supportClockValue(r)! })), canManage: hasPermission(a, 'support.manage') && !a.branchId && !a.departmentId, now: ports.now() }; }; }
