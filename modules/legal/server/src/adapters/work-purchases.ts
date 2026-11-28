import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkItem} from "@erpx/work-projection-ui/lib/work-item";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {legalPackageEnforcementMode} from "@erpx/commercial-server/public/purchase-command-boundary";
import {featureSpecs} from "@erpx/legal-ui/lib/legal/features";
import {visibility} from "@erpx/legal-server/adapters/legal/access";
import {legalReadScope} from "@erpx/legal-server/adapters/purchases";
import {legalKindCapability} from "@erpx/legal-server/domain/purchases";
import type {LegalKind} from "@erpx/legal-server/domain/model";
import {legalMemberEditExistsSql} from "./legal-read-tail-r25-sources";
import contract from './work-purchases-sources.json' with {type:'json'};
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_WORK_PURCHASES_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
/** Re-resolve native identifiers after shared Flow enrichment. An enrichment
 * cannot restore missing-source work or elevate a history-only decision. */
export async function filterLegalPurchasedWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await legalPackageEnforcementMode()!=="enforced"||!items.some(item=>item.type==="legal"))return items;
 if(!hasPermission(actor,"legal.read"))return items.filter(item=>item.type!=="legal");
 const purchase=await legalReadScope(database,actor),scope=visibility(actor),ids=items.filter(item=>item.type==="legal").map(item=>item.id);
 const sources=[contract.recordSource,contract.approvalSource,Object.values(featureSpecs).map(spec=>fill(contract.featureSource,{table:spec.table})).join(' UNION ALL ')].join(' UNION ALL ');
 const rows=(await database.prepare(fill(contract.rows,{memberEdit:legalMemberEditExistsSql("editor"),sources,inList:ids.map(()=>'?').join(','),scope:scope.sql,purchase:purchase.where})).bind(actor.userId,...ids,...scope.values).all<{work_id:string;source:string;kind:LegalKind;can_edit:boolean}>()).results,map=new Map(rows.map(row=>[row.work_id,row]));
 return items.flatMap(item=>{if(item.type!=="legal")return [item];const source=map.get(item.id);if(!source)return [];const flags=purchase.purchases?.[legalKindCapability(source.kind)],canAct=Boolean(flags&&(source.source==='record'?flags.newWork:flags.settleExisting)&&(source.source!=='approval'||purchase.purchases?.['legal.documents']?.settleExisting)&&(source.source!=='task'||purchase.purchases?.['legal.obligations']?.settleExisting)&&(source.source==='approval'?hasPermission(actor,'legal.approve'):source.can_edit&&hasPermission(actor,'legal.edit')));return canAct?[item]:[{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review retained Legal source",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))}];});
}
