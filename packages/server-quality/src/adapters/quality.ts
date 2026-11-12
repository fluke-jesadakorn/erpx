import {qualitySourceAccess,qualityVisibleHolds,requireQualityPurchasedRead} from "@erpx/server-quality/adapters/purchases";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import {prepareQualityNotification as notify} from '@erpx/server-collaboration/public/quality-notifications';
import {BusinessRuleError} from "@erpx/server-shared/index";
import 'server-only';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {sha256} from "@erpx/server-runtime/public/crypto";

import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {createQualityApplication,type QualityRepository} from '@erpx/server-quality/application/quality';
import type {QualityRow as Row,ReceiptQualityHold} from '@erpx/server-quality/domain/quality';
const eligible=`a.company_id=h.company_id AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) AND NOT iam_same_person(a.user_id,h.created_by,h.company_id) AND (h.status<>'cancel_pending' OR NOT iam_same_person(a.user_id,h.cancel_requested_by,h.company_id)) AND (h.status<>'awaiting_release' OR NOT iam_same_person(a.user_id,h.last_inspector,h.company_id))`;

async function rows(db:SqlDatabase,actor:WorkContext,id?:string){return qualityVisibleHolds(db,actor,(await db.prepare(`SELECT h.*,p.name product_name,p.unit product_unit,p.tracking_mode,w.name warehouse_name,l.lot_code,l.serial_number,l.expiry_date,(SELECT count(DISTINCT a.user_id) FROM (${identity_accessProjectionSources.assignments}) a WHERE ${eligible}) eligible_count FROM inventory_quality_holds h JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id LEFT JOIN (${inventoryProjectionSources.inventory_lots}) l ON l.id=h.lot_id WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND (?::text IS NULL OR h.id=?) ORDER BY h.due_date,h.id`).bind(actor.companyId,actor.branchId,actor.branchId,id??null,id??null).all<Row>()).results);}

function application(actor:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;return createQualityApplication({departmentScope:Boolean(actor.departmentId),companyScope:!actor.branchId,userId:actor.userId,authorize:permission=>requirePermission(actor,permission),can:permission=>hasPermission(actor,permission),samePerson:id=>samePerson(actor,id),clock:{now:()=>Date.now()},hash:sha256,repository:async()=>repository(await (resource??=holdingResources(actor.holdingId).then(r=>r.write)),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):QualityRepository<SqlStatement[]>{return {
 holds:id=>rows(write,actor,id),
 activeSpec:productId=>write.prepare("SELECT id,version FROM inventory_quality_specs WHERE company_id=? AND product_id=? AND status='active'").bind(actor.companyId,productId).first<Row>(),
 evidenceVersionGuard:(id,expectedVersion,key)=>[assertion(write,key,'quality-evidence-version',"EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=? AND status IN ('held','awaiting_release')",[id,actor.companyId,expectedVersion])],
 receiptHold:(input,spec,now)=>{const steps=[];if(spec)steps.push(assertion(write,input.key,'quality-spec',"EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND version=? AND status='active')",[spec.id,spec.version]));steps.push(write.prepare('INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,lot_id,held_quantity_milli,source_type,source_id,spec_id,spec_version,quantity_scope) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,\'partial\')').bind(input.id,actor.companyId,input.branchId,input.warehouseId,input.productId,input.reason,input.dueDate,actor.userId,now,now,input.lotId??null,input.quantityMilli,input.sourceType??'goods_receipt',input.sourceId,spec?.id??null,spec?.version??null),notify(write,input.key,input.id));return steps;
 },
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function getQualityHold(actor:WorkContext,id:string,provided?:SqlDatabase){const write=provided??(await holdingResources(actor.holdingId)).write;await requireQualityPurchasedRead(write,actor,id);return translated(()=>application(actor,write).getQualityHold(id));}
export async function qualityEvidenceGuard(db:SqlDatabase,actor:WorkContext,id:string,expectedVersion:number,key:string){await requireQualityPurchasedRead(db,actor,id);return translated(()=>application(actor,db).qualityEvidenceGuard(id,expectedVersion,key));}
/** Original protective receiving/return effects compose under the full named
 * Commerce owner. Shared quarantine facts never acquire a management gate. */
export function receiptQualityHoldSteps(db:SqlDatabase,actor:WorkContext,input:ReceiptQualityHold){return translated(()=>application(actor,db).receiptQualityHoldSteps(input));}
export async function qualityWork(actor:WorkContext){if(!hasPermission(actor,"quality.read")||actor.departmentId)return [];const {write}=await holdingResources(actor.holdingId),items=await translated(()=>application(actor,write).qualityWork());return Promise.all(items.map(async item=>{const access=await qualitySourceAccess(write,actor,item.id);return !access||access.settleExisting?item:{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))};}));}
