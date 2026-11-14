import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { createSupportAlerts, createGetSupportAlert, createSupportAlertWork } from "@erpx/server-support/application/support-sla-alerts";
import type { SupportAlert as Alert } from "@erpx/server-support/domain/support-sla-alerts";
const recipient = `m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${supportRecipientEligibility().replaceAll('t.owner_id', 'm.user_id')}`;
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
export async function supportAlerts(a: WorkContext): Promise<Alert[]> {
    const runtime = nativeSupportRuntime(a), access = await runtime.requirePurchase("support.sla");
    const rows = await createSupportAlerts({ ...runtime, alerts: async (a) => {
            const write = await runtime.database();
            return (await write.prepare(`SELECT s.*,t.title,t.company_id,t.branch_id,t.department_id,
  CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${recipient} AND m.user_id=?) THEN 1 ELSE 0 END can_ack,
  (SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${recipient}) recipients
  FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id WHERE ${scope} ORDER BY s.breached_at,s.id`).bind(a.userId, ...args(a)).all<Alert>()).results;
        } })(a); return rows.map(row => ({ ...row, can_ack: access.settleExisting ? row.can_ack : 0 }));
}
export async function getSupportAlert(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a); return createGetSupportAlert({ ...runtime, alerts: supportAlerts })(a, id); }
export async function supportAlertWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.sla"); if (!access.history) return []; const items = await createSupportAlertWork({ alerts: supportAlerts })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }
