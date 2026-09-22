import "server-only";

import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { WorkItem } from "@/lib/work-item";
import { hasPermission } from "@/lib/permissions";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";

const terminal = "('released','cancelled','disposed')";
// Assignment scope and the required quality authority are evaluated from the
// current hold state on every sweep and read. No recipient routing is copied
// into the alert, so revocation and role changes take effect immediately.
const recipient = `m.company_id=h.company_id
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END)
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=h.branch_id))
 AND NOT iam_same_person(m.user_id,h.created_by,h.company_id)
 AND (h.status<>'cancel_pending' OR NOT iam_same_person(m.user_id,h.cancel_requested_by,h.company_id))
 AND (h.status<>'awaiting_release' OR NOT iam_same_person(m.user_id,h.last_inspector,h.company_id))`;

const scope = "h.company_id=? AND (?::text IS NULL OR h.branch_id=?)";
const scopeArgs = (actor: WorkContext) => [
  actor.companyId,
  actor.branchId,
  actor.branchId,
];

type Alert = {
  id: string;
  hold_id: string;
  phase: "due_soon" | "overdue";
  source_version: number;
  due_date: string;
  detected_at: number;
  version: number;
  acknowledged_by: string | null;
  acknowledged_at: number | null;
  reason: string | null;
  company_id: string;
  branch_id: string;
  status: string;
  product_name: string;
  warehouse_name: string;
  can_ack: number;
  recipients: number;
};

type QualityAlertResult = { dueSoon: number; overdue: number };

function utcDate(now: number) {
  return new Date(now).toISOString().slice(0, 10);
}

/**
 * Creates recoverable due-soon and overdue alert records, then routes them to
 * the currently eligible quality recipients. Re-running after downtime or a
 * partial batch is safe because both records and notifications are unique.
 */
export async function drainQualityAlerts(
  db: SqlDatabase,
  now = Date.now(),
  warningWindowMs = 86_400_000,
): Promise<QualityAlertResult> {
  const today = utcDate(now);
  const horizon = utcDate(now + warningWindowMs);
  const dueSoon = await db
    .prepare(
      `INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)
       SELECT 'quality-alert-due-soon-'||h.id,h.id,'due_soon',h.version,h.due_date::date,?
       FROM inventory_quality_holds h
       WHERE h.status NOT IN ${terminal} AND h.due_date>=? AND h.due_date<=?
       ON CONFLICT DO NOTHING`,
    )
    .bind(now, today, horizon)
    .run();
  const overdue = await db
    .prepare(
      `INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)
       SELECT 'quality-alert-overdue-'||h.id,h.id,'overdue',h.version,h.due_date::date,?
       FROM inventory_quality_holds h
       WHERE h.status NOT IN ${terminal} AND h.due_date<?
       ON CONFLICT DO NOTHING`,
    )
    .bind(now, today)
    .run();

  await db
    .prepare(
      `INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
       SELECT DISTINCT a.id||':'||m.user_id,h.company_id,m.user_id,'quality_alert',a.id,a.version,
         CASE WHEN a.phase='overdue'
           THEN 'Quality hold is overdue: take responsibility for inspection or release follow-up'
           ELSE 'Quality hold is due within 24 hours: prepare the next inspection or release action'
         END,?::bigint
       FROM inventory_quality_alerts a
       JOIN inventory_quality_holds h ON h.id=a.hold_id
       JOIN assignments m ON ${recipient.replaceAll("clock_timestamp()", "to_timestamp(?::double precision / 1000.0)")}
       WHERE a.acknowledged_by IS NULL AND h.status NOT IN ${terminal}
       ON CONFLICT DO NOTHING`,
    )
    // The recipient expression contains the sweep timestamp once. PostgreSQL
    // still evaluates the rest of the eligibility predicates against current
    // assignment rows, while the testable timestamp makes a replay deterministic.
    .bind(now, now)
    .run();
  return {
    dueSoon: Number(dueSoon.meta.changes ?? 0),
    overdue: Number(overdue.meta.changes ?? 0),
  };
}

function alertAccess(actor: WorkContext) {
  requirePermission(actor, "quality.read");
  if (actor.departmentId)
    throw new ApiError(
      403,
      "QUALITY_SCOPE_REQUIRED",
      "Use a Company or Branch quality assignment.",
    );
}

export async function qualityAlerts(actor: WorkContext): Promise<Alert[]> {
  alertAccess(actor);
  const { write } = await holdingResources(actor.holdingId);
  return (
    await write
      .prepare(
        `SELECT a.*,h.company_id,h.branch_id,h.status,p.name product_name,w.name warehouse_name,
          CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE ${recipient.replaceAll("clock_timestamp()", "clock_timestamp()")}
             AND m.user_id=?) THEN 1 ELSE 0 END can_ack,
          (SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE ${recipient}) recipients
         FROM inventory_quality_alerts a
         JOIN inventory_quality_holds h ON h.id=a.hold_id
         JOIN commerce_products p ON p.id=h.product_id
         JOIN commerce_warehouses w ON w.id=h.warehouse_id
         WHERE ${scope}
         ORDER BY a.due_date,CASE WHEN a.phase='overdue' THEN 0 ELSE 1 END,a.id`,
      )
      .bind(actor.userId, ...scopeArgs(actor))
      .all<Alert>()
  ).results;
}

export async function getQualityAlert(actor: WorkContext, id: string) {
  const row = (await qualityAlerts(actor)).find((alert) => alert.id === id);
  if (!row)
    throw new ApiError(
      404,
      "QUALITY_ALERT_NOT_FOUND",
      "Quality escalation is not available in this scope.",
    );
  return row;
}

const acknowledgeSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});

export async function acknowledgeQualityAlert(
  actor: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  const input = acknowledgeSchema.parse(raw);
  if (!hasPermission(actor, "quality.inspect") && !hasPermission(actor, "quality.release"))
    throw new ApiError(403, "QUALITY_ALERT_PERMISSION_REQUIRED", "A quality inspector or manager must acknowledge this escalation.");
  const { write } = await holdingResources(actor.holdingId);
  const operation = "quality.alert.acknowledge";
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const alert = await getQualityAlert(actor, id);
  if (alert.acknowledged_by) {
    throw new ApiError(409, "QUALITY_ALERT_ACKNOWLEDGED", "This escalation has already been acknowledged.");
  }
  const now = Date.now();
  return executeCommand(
    write,
    actor,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1, acknowledgedBy: actor.userId },
    [
      write.prepare("SELECT id FROM inventory_quality_alerts WHERE id=? FOR UPDATE").bind(id),
      assertion(
        write,
        key,
        "alert",
        `EXISTS(
          SELECT 1 FROM inventory_quality_alerts a
          JOIN inventory_quality_holds h ON h.id=a.hold_id
          WHERE a.id=? AND a.version=? AND a.acknowledged_by IS NULL
            AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)
            AND h.status NOT IN ${terminal}
            AND EXISTS(SELECT 1 FROM assignments m WHERE ${recipient} AND m.user_id=?))`,
        [id, input.expectedVersion, actor.companyId, actor.branchId, actor.branchId, actor.userId],
      ),
      write
        .prepare(
          "UPDATE inventory_quality_alerts SET acknowledged_by=?,acknowledged_at=?,reason=?,version=version+1 WHERE id=? AND version=? AND acknowledged_by IS NULL",
        )
        .bind(actor.userId, now, input.reason, id, input.expectedVersion),
    ],
    { companyId: alert.company_id, branchId: alert.branch_id, departmentId: null },
  );
}

export async function qualityAlertWork(actor: WorkContext): Promise<WorkItem[]> {
  if (!hasPermission(actor, "quality.read")) return [];
  const alerts = await qualityAlerts(actor);
  return alerts.map((alert) => {
    const completed = Boolean(alert.acknowledged_by) || terminal.includes(`'${alert.status}'`);
    const canAct = !completed && Boolean(alert.can_ack) &&
      (hasPermission(actor, "quality.inspect") || hasPermission(actor, "quality.release"));
    const overdue = alert.phase === "overdue";
    return {
      id: String(alert.id),
      type: "quality_alert" as const,
      title: `${overdue ? "Overdue quality hold" : "Quality hold due soon"} · ${alert.product_name}`,
      detail: `${alert.warehouse_name} · due ${alert.due_date}`,
      href: `/quality/${encodeURIComponent(alert.hold_id)}`,
      status: completed ? "acknowledged" : alert.phase,
      updatedAt: Number(alert.acknowledged_at ?? alert.detected_at),
      queue: completed ? "completed" : canAct ? "todo" : "waiting",
      canAct,
      dueAt: Date.parse(`${alert.due_date}T00:00:00Z`),
      responsibleQueue: "Eligible Quality inspector or manager",
      nextAction: completed
        ? "Follow the recorded quality recovery plan"
        : overdue
          ? "Take responsibility and record the overdue follow-up plan"
          : "Prepare the next inspection or release action",
      blocker:
        !completed && !Number(alert.recipients)
          ? "No eligible Quality recipient; Company administrator must assign quality authority"
          : null,
      relationships: [canAct ? "approval" : "team"],
    };
  });
}
