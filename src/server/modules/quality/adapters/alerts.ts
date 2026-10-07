import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/commercial-owner-projection-sources";
import {routeQualityAlertNotifications} from '../../collaboration/public/quality-notifications';
import "server-only";

import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { hasPermission } from "@/lib/permissions";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { ApiError } from "@/server/modules/runtime/public/http";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { scheduledFeaturePredicate } from "@/server/modules/pms/public/hospitality-rollout";


import {BusinessRuleError} from '../../shared';
import {createQualityAlertApplication,sweepQualityAlerts,type QualityAlertRepository,type QualityAlertSweepRepository} from '../application/alerts';
import type {QualityAlert as Alert} from '../domain/alerts';
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


function application(actor:WorkContext){let resource:Promise<SqlDatabase>|null=null;return createQualityAlertApplication({departmentScope:Boolean(actor.departmentId),userId:actor.userId,authorizeRead:()=>requirePermission(actor,'quality.read'),can:permission=>hasPermission(actor,permission),clock:{now:()=>Date.now()},repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):QualityAlertRepository{return {async alerts(){return (
    await write
      .prepare(
        `SELECT a.*,h.company_id,h.branch_id,h.status,h.version hold_version,p.name product_name,w.name warehouse_name,
          CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${recipient.replaceAll("clock_timestamp()", "clock_timestamp()")}
             AND m.user_id=?) THEN 1 ELSE 0 END can_ack,
          (SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${recipient}) recipients
         FROM inventory_quality_alerts a
         JOIN inventory_quality_holds h ON h.id=a.hold_id
         JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id
         JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id
         WHERE ${scope}
         ORDER BY a.due_date,CASE WHEN a.phase='overdue' THEN 0 ELSE 1 END,a.id`,
      )
      .bind(actor.userId, ...scopeArgs(actor))
      .all<Alert>()
  ).results;
},replay:(key,operation,id,input)=>replayCommand(write,actor,key,operation,id,input),async acknowledge(id,key,input,alert,now){const operation='quality.alert.acknowledge';return executeCommand(
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
            AND EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${recipient} AND m.user_id=?))`,
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
}};}
function sweepRepository(db:SqlDatabase):QualityAlertSweepRepository{return {async dueSoon(now,today,horizon){const result = await db
    .prepare(
      `INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)
       SELECT 'quality-alert-due-soon-'||h.id,h.id,'due_soon',h.version,h.due_date::date,?
       FROM inventory_quality_holds h
       WHERE h.status NOT IN ${terminal} AND h.due_date>=? AND h.due_date<=?
         AND ${scheduledFeaturePredicate("h.company_id", "quality")}
       ON CONFLICT DO NOTHING`,
    )
    .bind(now, today, horizon)
    .run();
  return Number(result.meta.changes??0);},async overdue(now,today){const result = await db
    .prepare(
      `INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)
       SELECT 'quality-alert-overdue-'||h.id,h.id,'overdue',h.version,h.due_date::date,?
       FROM inventory_quality_holds h
       WHERE h.status NOT IN ${terminal} AND h.due_date<?
         AND ${scheduledFeaturePredicate("h.company_id", "quality")}
       ON CONFLICT DO NOTHING`,
    )
    .bind(now, today)
    .run();
return Number(result.meta.changes??0);},async notify(now){await routeQualityAlertNotifications(db,now);}};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function drainQualityAlerts(db:SqlDatabase,now=Date.now(),warningWindowMs=86_400_000){return sweepQualityAlerts(sweepRepository(db),now,warningWindowMs);}
export function qualityAlerts(actor:WorkContext){return translated(()=>application(actor).qualityAlerts());}
export function getQualityAlert(actor:WorkContext,id:string){return translated(()=>application(actor).getQualityAlert(id));}
export function acknowledgeQualityAlert(actor:WorkContext,id:string,raw:unknown,key:string){return translated(()=>application(actor).acknowledgeQualityAlert(id,raw,key));}
export function qualityAlertWork(actor:WorkContext){return translated(()=>application(actor).qualityAlertWork());}


