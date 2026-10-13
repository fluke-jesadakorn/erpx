import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SupportCommandPorts, SupportCommand, SupportMutation } from "./command";
import { hasPermission } from "@/lib/identity_access/permissions";
import { supportAlertAcknowledgeSchema, supportAlertWorkItems, type SupportAlert, type SupportAlertAcknowledge } from "../domain/support-sla-alerts";
export function createDrainSupportSla(ports: {
    now(): number;
    sweep(now: number): Promise<void>;
}) { return async (now = ports.now()) => ports.sweep(now); }
export function createSupportAlerts(ports: Pick<SupportCommandPorts, "authorize"> & {
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext) => { ports.authorize(a, 'support.read'); return ports.alerts(a); }; }
export function createGetSupportAlert(ports: Pick<SupportCommandPorts, "error"> & {
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext, id: string) => { const row = (await ports.alerts(a)).find(r => r.id === id); if (!row)
    throw ports.error(404, 'SUPPORT_ALERT_NOT_FOUND', 'Target escalation is not available in this scope.'); return row; }; }
export function createAcknowledgeSupportAlert(ports: SupportCommandPorts & {
    alert(a: WorkContext, id: string): Promise<SupportAlert>;
    prepare(command: SupportCommand, input: SupportAlertAcknowledge): Promise<SupportMutation>;
}) { return async (a: WorkContext, id: string, raw: unknown, key: string) => { ports.authorize(a, 'support.manage'); const input = supportAlertAcknowledgeSchema.parse(raw), op = 'support.sla.acknowledge', prior = await ports.replay(a, key, op, id, input); if (prior)
    return prior; const r = await ports.alert(a, id), now = ports.now(), command: SupportCommand = { actor: a, key, operation: op, id, input, version: input.expectedVersion + 1, result: { id, version: input.expectedVersion + 1 }, now, scope: { companyId: r.company_id, branchId: r.branch_id, departmentId: r.department_id } }; return ports.commit(command, await ports.prepare(command, input)); }; }
export function createSupportAlertWork(ports: {
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext) => !hasPermission(a, 'support.read') ? [] : supportAlertWorkItems(a, await ports.alerts(a)); }
