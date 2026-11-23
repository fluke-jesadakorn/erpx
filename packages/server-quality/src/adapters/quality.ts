import {qualitySourceAccess,qualityVisibleHolds,requireQualityPurchasedRead} from "@erpx/server-quality/adapters/purchases";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import {nativeCustomerReturnHoldStatements} from './native-customer-return-hold';
import {BusinessRuleError} from "@erpx/server-shared/index";
import 'server-only';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {sha256} from "@erpx/server-runtime/public/crypto";

import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {createQualityApplication,type QualityRepository} from '@erpx/server-quality/application/quality';
import type {QualityRow as Row,ReceiptQualityHold} from '@erpx/server-quality/domain/quality';
const eligible=`a.company_id=h.company_id AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) AND NOT iam_same_person(a.user_id,h.created_by,h.company_id) AND (h.status<>'cancel_pending' OR NOT iam_same_person(a.user_id,h.cancel_requested_by,h.company_id)) AND (h.status<>'awaiting_release' OR NOT iam_same_person(a.user_id,h.last_inspector,h.company_id))`;

/* Removal note: rows(), getQualityHold() and qualityWork() stay only until
 * retire.quality.workspace-read routes their consumers to the native quality
 * read/detail_core ports; the dead active-specification / evidence-version
 * read branch is retired here and its policy is covered by native owner tests. */
async function rows(db:SqlDatabase,actor:WorkContext,id?:string){return qualityVisibleHolds(db,actor,(await db.prepare(`SELECT h.*,p.name product_name,p.unit product_unit,p.tracking_mode,w.name warehouse_name,l.lot_code,l.serial_number,l.expiry_date,(SELECT count(DISTINCT a.user_id) FROM (${identity_accessProjectionSources.assignments}) a WHERE ${eligible}) eligible_count FROM inventory_quality_holds h JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id LEFT JOIN (${inventoryProjectionSources.inventory_lots}) l ON l.id=h.lot_id WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND (?::text IS NULL OR h.id=?) ORDER BY h.due_date,h.id`).bind(actor.companyId,actor.branchId,actor.branchId,id??null,id??null).all<Row>()).results);}

function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;return createQualityApplication({departmentScope:Boolean(actor.departmentId),companyScope:!actor.branchId,userId:actor.userId,authorize:permission=>requirePermission(actor,permission),can:permission=>hasPermission(actor,permission),samePerson:id=>samePerson(actor,id),clock:{now:()=>Date.now()},hash:sha256,repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):QualityRepository<SqlStatement[]>{return {
 holds:id=>rows(write,actor,id),
 receiptHold:async(input,now)=>{
   const statements=await nativeCustomerReturnHoldStatements(actor,input,actor.userId,now);
   const steps:SqlStatement[]=[];
   if(statements.specGuard)steps.push(write.prepare(statements.specGuard.sql).bind(...statements.specGuard.values));
   steps.push(write.prepare(statements.hold.sql).bind(...statements.hold.values),write.prepare(statements.notification.sql).bind(...statements.notification.values));
   return steps;
 }
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function getQualityHold(actor:WorkContext,id:string,provided?:SqlDatabase){const write=provided??(await holdingResources(actor.holdingId)).write;await requireQualityPurchasedRead(write,actor,id);return translated(()=>application(actor,write).getQualityHold(id));}
/** Original protective receiving/return effects compose under the full named
 * Commerce owner. Shared quarantine facts never acquire a management gate. */
export function receiptQualityHoldSteps(db:SqlDatabase,actor:WorkContext,input:ReceiptQualityHold){return translated(()=>application(actor,db).receiptQualityHoldSteps(input));}
export async function qualityWork(actor:WorkContext){if(!hasPermission(actor,"quality.read")||actor.departmentId)return [];const {write}=await holdingResources(actor.holdingId),items=await translated(()=>application(actor,write).qualityWork());return Promise.all(items.map(async item=>{const access=await qualitySourceAccess(write,actor,item.id);return !access||access.settleExisting?item:{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))};}));}
