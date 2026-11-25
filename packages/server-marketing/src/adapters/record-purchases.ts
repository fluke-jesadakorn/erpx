import 'server-only';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkItem} from '@erpx/feature-work_projection/lib/work-item';
import {commercePackageEnforcementMode,readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {marketingClockedReportFrameSql} from './marketing-clocked-report-frame';
import {marketingPurchaseUnionSql} from './marketing-purchase-union-sql';
import {marketingPurchaseAssetRows,marketingPurchaseCandidateRows,requireMarketingPurchasedRead,type MarketingPurchaseTarget} from '@erpx/server-marketing/adapters/purchases';
import {marketingModuleMemberSql} from './module-membership';
import {marketingRecordScopeAuthoritySql} from './record-scope-authority';
import {marketingSourceIdsSql} from './marketing-source-id-sql';
import {marketingOwnerPointReadSql} from './owner-point-read-sql';
import {marketingPointExistsSql} from './marketing-point-exists-sql';
import {marketingSourceRowGuardSql} from './attachment-owner-version-guard';
import {marketingSelectHeadSql} from './select-head-sql';
const nativeIds=marketingSourceIdsSql();
const rows=marketingPurchaseUnionSql([
  ["asset.id,'asset' kind,asset.company_id,asset.branch_id,asset.department_id",`(${marketingPurchaseAssetRows}) asset`],
  ["candidate.id,'candidate',candidate.company_id,candidate.branch_id,candidate.department_id",`(${marketingPurchaseCandidateRows}) candidate`],
  ["id,'campaign',company_id,NULL,NULL",'marketing_campaigns'],
  ["cost.id,'campaign-cost',cost.company_id,NULL,NULL",'marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id'],
]);
function scope(actor:WorkContext,id:string,company:string){
 const history=purchasedCapabilityPredicate('marketing_source.company_id','sales.marketing','history','marketing_clock.now_ms');
 return {where:`EXISTS(${marketingClockedReportFrameSql('marketing_clock','', '1', rows, 'marketing_source')}marketing_source.id=${id} AND marketing_source.company_id=${company} AND ${hasPermission(actor,'records.read')&&actor.modules.includes('marketing')?'TRUE':'FALSE'} AND (marketing_source.branch_id IS NULL OR ?::text IS NULL OR marketing_source.branch_id=?) AND (marketing_source.department_id IS NULL OR ?::text IS NULL OR marketing_source.department_id=?) AND ${history}
 AND ${marketingRecordScopeAuthoritySql('marketing_source','marketing_authority')} AND EXISTS(${marketingModuleMemberSql('selected')}))`,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId]};
}
export async function marketingSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await commercePackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccesses(database,mode,actor.companyId,['sales.marketing']);return {where:'TRUE',args:[] as unknown[]};}
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id',type=surface==='staff'?'staff_notifications.record_type':'e.source_type',actual=`${marketingSourceRowGuardSql('('+nativeIds+') native_marketing','native_marketing.id='+id)}`,relevant=surface==='records'?actual:`(${type} IN ('marketing_asset','marketing_candidate','marketing_campaign','marketing_campaign_cost') OR ${actual})`,source=scope(actor,id,company);
 return {where:`(NOT ${relevant} OR ${source.where})`,args:source.args};
}
export async function requireMarketingWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,_moduleId:string,id:string){
 if(await commercePackageEnforcementMode()==='disabled')return;const row=await database.prepare(marketingOwnerPointReadSql('kind',`(${rows}) actual`)).bind(id,actor.companyId).first<{kind:MarketingPurchaseTarget['kind']}>();
 if(row)await requireMarketingPurchasedRead(database,actor,{kind:row.kind,id});else if(await database.prepare(marketingPointExistsSql(`(${nativeIds}) actual`,'id=?')).bind(id).first())throw new ApiError(404,'MARKETING_SOURCE_NOT_FOUND','The original Marketing source is unavailable in this workspace.');
}
export async function filterMarketingPurchasedWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 const selected=items.filter(item=>item.type==='marketing_candidate');if(!selected.length||await commercePackageEnforcementMode()!=='enforced')return items;
 const ids=[...new Set(selected.map(item=>item.id))],visible=scope(actor,'marketing_work.id','marketing_work.company_id'),access=await readPurchasedAccesses(database,'enforced',actor.companyId,['sales.crm','sales.marketing']);
 const rows=(await database.prepare(marketingSelectHeadSql(`marketing_work.id,${hasPermission(actor,'records.write')&&hasPermission(actor,'commerce.read')&&hasPermission(actor,'commerce.sales')?'TRUE':'FALSE'} AND ${assignmentReadSql('marketing_work','records.write')} AND ${assignmentReadSql('marketing_work','commerce.read')} AND ${assignmentReadSql('marketing_work','commerce.sales')} AND ${marketingSourceRowGuardSql('marketing_lead_candidates native',"native.id=marketing_work.id AND native.company_id=marketing_work.company_id AND native.status='pending' AND native.created_by<>?")} can_act FROM (${marketingPurchaseCandidateRows}) marketing_work WHERE marketing_work.company_id=? AND marketing_work.id IN (${ids.map(()=>'?').join(',')}) AND ${visible.where}`)).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.userId,actor.companyId,...ids,...visible.args).all<{id:string;can_act:boolean}>()).results,map=new Map(rows.map(row=>[row.id,row]));
 return items.flatMap(item=>{if(item.type!=='marketing_candidate')return [item];const row=map.get(item.id);if(!row)return [];if(row.can_act&&access['sales.crm'].settleExisting&&access['sales.marketing'].settleExisting)return [item];return [{...item,canAct:false,actionKind:'work' as const,queue:item.queue==='completed'?'completed' as const:'waiting' as const,nextAction:'Review retained Marketing source',flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value))}];});
}
