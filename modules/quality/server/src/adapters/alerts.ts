import {qualitySourceAccess,requireQualityPurchasedRead} from "@erpx/quality-server/adapters/purchases";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/commerce-server/public/commercial-owner-projection-sources";
import { qualityAlertRecipientSql } from './quality-alert-recipient-sql';
import { evidenceHead } from './inspection-evidence-source-sql';
import "server-only";

import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { ApiError } from "@erpx/platform-runtime-server/public/http";


import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createQualityAlertApplication,type QualityAlertRepository} from '@erpx/quality-server/application/alerts';
import type {QualityAlert as Alert} from '@erpx/quality-server/domain/alerts';
// Assignment scope and the required quality authority are evaluated from the
// current hold state on every sweep and read. No recipient routing is copied
// into the alert, so revocation and role changes take effect immediately.
// The shared recipient read is owned by ./quality-alert-recipient-sql.

const scope = "h.company_id=? AND (?::text IS NULL OR h.branch_id=?)";
const scopeArgs = (actor: WorkContext) => [
  actor.companyId,
  actor.branchId,
  actor.branchId,
];


function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;return createQualityAlertApplication({departmentScope:Boolean(actor.departmentId),authorizeRead:()=>requirePermission(actor,'quality.read'),can:permission=>hasPermission(actor,permission),repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):QualityAlertRepository{return {async alerts(){return (
    await write
      .prepare(
        evidenceHead(`a.*,h.company_id,h.branch_id,h.status,h.version hold_version,p.name product_name,w.name warehouse_name,
          CASE WHEN EXISTS(${qualityAlertRecipientSql('1', `
             AND m.user_id=?`)}) THEN 1 ELSE 0 END can_ack,
          (${qualityAlertRecipientSql('COUNT(DISTINCT m.user_id)')}) recipients
        `,` FROM inventory_quality_alerts a
         JOIN inventory_quality_holds h ON h.id=a.hold_id
         JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id
         JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id
         WHERE ${scope}
         ORDER BY a.due_date,CASE WHEN a.phase='overdue' THEN 0 ELSE 1 END,a.id`,''),
      )
      .bind(actor.userId, ...scopeArgs(actor))
      .all<Alert>()
  ).results;
}};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function getQualityAlert(actor:WorkContext,id:string){const {write}=await holdingResources(actor.holdingId),row=await translated(()=>application(actor,write).getQualityAlert(id));await requireQualityPurchasedRead(write,actor,row.hold_id);return row;}
export async function qualityAlertWork(actor:WorkContext){if(!hasPermission(actor,"quality.read")||actor.departmentId)return [];const {write}=await holdingResources(actor.holdingId),items=await translated(()=>application(actor,write).qualityAlertWork()),visible:typeof items=[];for(const item of items){const source=item.source?.id;if(!source)continue;const access=await qualitySourceAccess(write,actor,source);if(access&&!access.history)continue;visible.push(!access||access.settleExisting?item:{...item,canAct:false,queue:item.queue==="completed"?"completed":"waiting",relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))});}return visible;}
