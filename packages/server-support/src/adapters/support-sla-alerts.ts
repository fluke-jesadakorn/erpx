import { holdingPurchaseReadLocks, observePurchaseResults, purchaseObservationProjection, supportPackageEnforcementMode } from "@erpx/server-commercial/public/purchase-command-boundary";
import { packageClockSql, purchasedCapabilityPredicate } from "@erpx/server-commercial/public/purchased-capability-sql";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { supportNotificationEffects } from "@erpx/server-collaboration/public/support-notification-effects";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { supportRecipientEligibility } from "@erpx/server-support/adapters/support-recipient";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { createDrainSupportSla, createSupportAlerts, createGetSupportAlert, createAcknowledgeSupportAlert, createSupportAlertWork } from "@erpx/server-support/application/support-sla-alerts";
import type { SupportAlert as Alert } from "@erpx/server-support/domain/support-sla-alerts";
const recipient = `m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${supportRecipientEligibility().replaceAll('t.owner_id', 'm.user_id')}`;
const scope = 't.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args = (a: WorkContext) => [a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId];
export async function drainSupportSla(db: SqlDatabase, now?: number) {
    const mode = await supportPackageEnforcementMode();
    const purchased = mode === "enforced" ? `EXISTS(SELECT 1 FROM (SELECT ${packageClockSql} now) purchase_scope WHERE ${purchasedCapabilityPredicate("t.company_id", "support.sla", "settle-existing", "purchase_scope.now")})` : "TRUE";
    return createDrainSupportSla({ now: Date.now, sweep: async (now) => {
            const results = await db.batch([
                ...(mode === "disabled" ? [] : [...holdingPurchaseReadLocks(db), db.prepare("SELECT id FROM support_tickets ORDER BY id FOR SHARE")]),
                db.prepare(`INSERT INTO support_sla_alerts(id,ticket_id,source_version,breached_at,detected_at)
   SELECT 'support-breach-'||t.id,t.id,t.version,
    COALESCE((t.sla_clock_json->>'breachedAt')::bigint,(t.sla_clock_json->>'dueAt')::bigint),?
   FROM support_tickets t WHERE ((t.sla_clock_json->>'breachedAt')::bigint IS NOT NULL
     OR (t.sla_clock_json->>'state'='running' AND (t.sla_clock_json->>'dueAt')::bigint<?))
     AND ${scheduledFeaturePredicate('t.company_id', 'support')} AND ${purchased}
   ON CONFLICT DO NOTHING`).bind(now, now),
                supportNotificationEffects(db).slaBreached(purchased).bind(now),
                ...(mode === "shadow" ? [db.prepare(`WITH observed_clock AS MATERIALIZED (SELECT ${packageClockSql} now) SELECT ${purchaseObservationProjection("source.company_id", "support.sla", "settle-existing", "observed_clock.now")} FROM (SELECT DISTINCT company_id FROM support_tickets) source CROSS JOIN observed_clock`)] : []),
            ]);
            if (mode === "shadow") observePurchaseResults(results);
        } })(now);
}
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
export async function acknowledgeSupportAlert(a: WorkContext, id: string, raw: unknown, key: string) {
    const runtime = nativeSupportRuntime(a);
    return createAcknowledgeSupportAlert({ ...runtime, alert: getSupportAlert, prepare: async (command, input) => {
            const write = await runtime.database(), { actor: a, key, id, now } = command;
            return { native: [
                    assertion(write, key, 'alert', `EXISTS(SELECT 1 FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id WHERE s.id=? AND s.version=? AND s.acknowledged_by IS NULL AND ${scope} AND EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${recipient} AND m.user_id=?))`, [id, input.expectedVersion, ...args(a), a.userId]),
                    write.prepare('UPDATE support_sla_alerts SET acknowledged_by=?,acknowledged_at=?,reason=?,version=version+1 WHERE id=?').bind(a.userId, now, input.reason, id),
                ] };
        } })(a, id, raw, key);
}
export async function supportAlertWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.sla"); if (!access.history) return []; const items = await createSupportAlertWork({ alerts: supportAlerts })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.settleExisting, queue: item.queue === "todo" && !access.settleExisting ? "waiting" as const : item.queue })); }

export function slaBreachedNotificationSource(purchased = "TRUE"){return `
   SELECT DISTINCT s.id||':'||m.user_id,t.company_id,m.user_id,'support_sla_alert',s.id,s.version,
    'Resolution target breached: take responsibility for follow-up',?::bigint
   FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id JOIN (${identity_accessProjectionSources.assignments}) m ON ${recipient}
   WHERE s.acknowledged_by IS NULL AND ${scheduledFeaturePredicate('t.company_id', 'support')} AND ${purchased}`;}
