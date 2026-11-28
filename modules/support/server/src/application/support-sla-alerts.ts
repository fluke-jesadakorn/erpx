import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SupportCommandPorts } from "@erpx/support-server/application/command";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { supportAlertWorkItems, type SupportAlert } from "@erpx/support-server/domain/support-sla-alerts";
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
