import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SupportCommandPorts } from "@erpx/server-support/application/command";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { supportAlertWorkItems, type SupportAlert } from "@erpx/server-support/domain/support-sla-alerts";
export function createSupportAlerts(ports: Pick<SupportCommandPorts, "authorize"> & {
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext) => { ports.authorize(a, 'support.read'); return ports.alerts(a); }; }
export function createGetSupportAlert(ports: Pick<SupportCommandPorts, "error"> & {
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext, id: string) => { const row = (await ports.alerts(a)).find(r => r.id === id); if (!row)
    throw ports.error(404, 'SUPPORT_ALERT_NOT_FOUND', 'Target escalation is not available in this scope.'); return row; }; }
export function createSupportAlertWork(ports: {
    alerts(a: WorkContext): Promise<SupportAlert[]>;
}) { return async (a: WorkContext) => !hasPermission(a, 'support.read') ? [] : supportAlertWorkItems(a, await ports.alerts(a)); }
