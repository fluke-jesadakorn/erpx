import {qualitySourceAccess,requireQualityPurchasedRead,withQualityPurchasedCommand,withQualityPurchasedSweep} from "@erpx/server-quality/adapters/purchases";
import {acknowledgeSchema} from "@erpx/server-quality/domain/alerts";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import {routeQualityAlertNotifications} from '@erpx/server-collaboration/public/quality-notifications';
import "server-only";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/hospitality-rollout";


import {BusinessRuleError} from '@erpx/server-shared/index';
import {createQualityAlertApplication,sweepQualityAlerts,type QualityAlertRepository,type QualityAlertSweepRepository} from '@erpx/server-quality/application/alerts';
import type {QualityAlert as Alert} from '@erpx/server-quality/domain/alerts';
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


function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;return createQualityAlertApplication({departmentScope:Boolean(actor.departmentId),userId:actor.userId,authorizeRead:()=>requirePermission(actor,'quality.read'),can:permission=>hasPermission(actor,permission),clock:{now:()=>Date.now()},repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
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
function sweepRepository(db:SqlDatabase,selected?:{companyId:string;holdIds:string[]}):QualityAlertSweepRepository{const filter=selected?" AND h.company_id=? AND h.id IN (SELECT jsonb_array_elements_text(?::jsonb))":"",args=selected?[selected.companyId,JSON.stringify(selected.holdIds)]:[];return {async dueSoon(now,today,horizon){const result = await db
    .prepare(
      `INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)
       SELECT 'quality-alert-due-soon-'||h.id,h.id,'due_soon',h.version,h.due_date::date,?
       FROM inventory_quality_holds h
       WHERE h.status NOT IN ${terminal} AND h.due_date>=? AND h.due_date<=?
         AND ${scheduledFeaturePredicate("h.company_id", "quality")}${filter}
       ON CONFLICT DO NOTHING`,
    )
    .bind(now, today, horizon,...args)
    .run();
  return Number(result.meta.changes??0);},async overdue(now,today){const result = await db
    .prepare(
      `INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at)
       SELECT 'quality-alert-overdue-'||h.id,h.id,'overdue',h.version,h.due_date::date,?
       FROM inventory_quality_holds h
       WHERE h.status NOT IN ${terminal} AND h.due_date<?
         AND ${scheduledFeaturePredicate("h.company_id", "quality")}${filter}
       ON CONFLICT DO NOTHING`,
    )
    .bind(now, today,...args)
    .run();
return Number(result.meta.changes??0);},async notify(now){await routeQualityAlertNotifications(db,now,selected);}};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function drainQualityAlerts(db:SqlDatabase,now=Date.now(),warningWindowMs=86_400_000){return withQualityPurchasedSweep(db,(database,scope)=>sweepQualityAlerts(sweepRepository(database,scope),now,warningWindowMs),results=>results.reduce((sum,result)=>({dueSoon:sum.dueSoon+result.dueSoon,overdue:sum.overdue+result.overdue}),{dueSoon:0,overdue:0}));}
export async function qualityAlerts(actor:WorkContext){const {write}=await holdingResources(actor.holdingId),rows=await translated(()=>application(actor,write).qualityAlerts()),visible:Alert[]=[];for(const row of rows){const access=await qualitySourceAccess(write,actor,row.hold_id);if(!access||access.history)visible.push(row);}return visible;}
export async function getQualityAlert(actor:WorkContext,id:string){const {write}=await holdingResources(actor.holdingId),row=await translated(()=>application(actor,write).getQualityAlert(id));await requireQualityPurchasedRead(write,actor,row.hold_id);return row;}
export async function acknowledgeQualityAlert(actor:WorkContext,id:string,raw:unknown,key:string){const {write}=await holdingResources(actor.holdingId),input=acknowledgeSchema.parse(raw);return withQualityPurchasedCommand(write,actor,{key,operation:"quality.alert.acknowledge",target:{kind:"alert",id},purpose:"settle-existing",permissions:[hasPermission(actor,"quality.release")?"quality.release":"quality.inspect"],existing:true},database=>translated(()=>application(actor,database).acknowledgeQualityAlert(id,input,key)));}
export async function qualityAlertWork(actor:WorkContext){if(!hasPermission(actor,"quality.read")||actor.departmentId)return [];const {write}=await holdingResources(actor.holdingId),items=await translated(()=>application(actor,write).qualityAlertWork()),visible:typeof items=[];for(const item of items){const source=item.source?.id;if(!source)continue;const access=await qualitySourceAccess(write,actor,source);if(access&&!access.history)continue;visible.push(!access||access.settleExisting?item:{...item,canAct:false,queue:item.queue==="completed"?"completed":"waiting",relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))});}return visible;}
