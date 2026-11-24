import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { nativeSupportReadAll, SUPPORT_RD_SLA_ALERTS } from "@erpx/server-support/adapters/native-support-read";
import { createSupportAlerts, createGetSupportAlert, createSupportAlertWork } from "@erpx/server-support/application/support-sla-alerts";
import type { SupportAlert as Alert } from "@erpx/server-support/domain/support-sla-alerts";
export async function supportAlerts(a: WorkContext): Promise<Alert[]> {
    const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.sla");
    const rows = await createSupportAlerts({ ...runtime, alerts: async (a) => {
            const write = await runtime.database();
            return (await nativeSupportReadAll<Alert>(SUPPORT_RD_SLA_ALERTS, [a.userId, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId], write)).results;
        } })(a); return rows.map(row => ({ ...row, can_ack: access.settleExisting ? row.can_ack : 0 }));
}
export async function getSupportAlert(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a); return createGetSupportAlert({ ...runtime, alerts: supportAlerts })(a, id); }
export async function supportAlertWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.sla"); if (!access.history) return []; const items = await createSupportAlertWork({ alerts: supportAlerts })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }
