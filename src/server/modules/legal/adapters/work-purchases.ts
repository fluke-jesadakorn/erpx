import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkItem} from "@/lib/work_projection/work-item";
import {hasPermission} from "@/lib/identity_access/permissions";
import {legalPackageEnforcementMode} from "../../commercial/public/purchase-command-boundary";
import {featureSpecs} from "@/lib/legal/legal/features";
import {visibility} from "./legal/access";
import {legalReadScope} from "./purchases";
import {legalKindCapability} from "../domain/purchases";
import type {LegalKind} from "../domain/model";
/** Re-resolve native identifiers after shared Flow enrichment. An enrichment
 * cannot restore missing-source work or elevate a history-only decision. */
export async function filterLegalPurchasedWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await legalPackageEnforcementMode()!=="enforced"||!items.some(item=>item.type==="legal"))return items;
 if(!hasPermission(actor,"legal.read"))return items.filter(item=>item.type!=="legal");
 const purchase=await legalReadScope(database,actor),scope=visibility(actor),ids=items.filter(item=>item.type==="legal").map(item=>item.id);
 const sources=`SELECT id work_id,id item_id,'record' source FROM legal_items UNION ALL SELECT 'legal-approval:'||id,id,'approval' FROM legal_items UNION ALL ${Object.values(featureSpecs).map(spec=>`SELECT 'legal-task:'||id,item_id,'task' FROM ${spec.table}`).join(' UNION ALL ')}`;
 const rows=(await database.prepare(`SELECT source.work_id,source.source,i.kind,EXISTS(SELECT 1 FROM legal_members editor WHERE editor.item_id=i.id AND editor.user_id=? AND editor.access='edit') can_edit FROM (${sources}) source JOIN legal_items i ON i.id=source.item_id WHERE source.work_id IN (${ids.map(()=>'?').join(',')}) AND ${scope.sql} AND ${purchase.where}`).bind(actor.userId,...ids,...scope.values).all<{work_id:string;source:string;kind:LegalKind;can_edit:boolean}>()).results,map=new Map(rows.map(row=>[row.work_id,row]));
 return items.flatMap(item=>{if(item.type!=="legal")return [item];const source=map.get(item.id);if(!source)return [];const flags=purchase.purchases?.[legalKindCapability(source.kind)],canAct=Boolean(flags&&(source.source==='record'?flags.newWork:flags.settleExisting)&&(source.source!=='approval'||purchase.purchases?.['legal.documents']?.settleExisting)&&(source.source!=='task'||purchase.purchases?.['legal.obligations']?.settleExisting)&&(source.source==='approval'?hasPermission(actor,'legal.approve'):source.can_edit&&hasPermission(actor,'legal.edit')));return canAct?[item]:[{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review retained Legal source",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))}];});
}
