import 'server-only';
import {createHash} from 'node:crypto';
import type {CRMAnalyticsAccess} from '@/lib/crm/analytics-access';
import {hasPermission} from '@/lib/identity_access/permissions';
import type {WorkContext} from '../../identity_access/public/authorization';
import {assignmentReadSql} from '../../identity_access/public/record-source-authority';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {commercePackageEnforcementMode,readPurchasedAccesses} from '../../commercial/public/purchase-command-boundary';
import {ApiError} from '../../runtime/public/http';
export async function crmAnalyticsAccess(database:SqlDatabase,actor:WorkContext):Promise<CRMAnalyticsAccess|undefined>{
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;const capabilities=['sales.crm','sales.marketing','sales.orders','pms.reservations','pms.guest-accounts','finance.sales-posting','finance.hotel-posting','finance.accounting'],access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities),current=async(permission:string,module?:string)=>hasPermission(actor,permission)&&(!module||actor.modules.includes(module))&&!!await database.prepare(`SELECT ${assignmentReadSql('report_scope',permission)} ${module?"AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(?::text)))":""} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) report_scope`).bind(actor.assignmentId,actor.userId,actor.role,...(module?[actor.assignmentId,module]:[]),actor.companyId,actor.branchId,actor.departmentId).first<boolean>('allowed');
 const [sales,marketing,hotel,finance]=await Promise.all([current('commerce.read'),current('records.read','marketing'),current('pms.read'),current('finance.read')]),crm=sales&&access['sales.crm'].history,marketingHistory=marketing&&access['sales.marketing'].history,hotelHistory=hotel&&access['pms.reservations'].history;
 return {crm,marketing:marketingHistory,hotel:hotelHistory,salesRevenue:crm&&access['sales.orders'].history&&access['finance.sales-posting'].history,hotelRevenue:crm&&hotelHistory&&access['pms.guest-accounts'].history&&access['finance.hotel-posting'].history,costs:marketingHistory&&finance&&access['finance.accounting'].history,revision:createHash('sha256').update(capabilities.map(id=>access[id].revision).join(':')).digest('hex')};
}
export function requireCRMReportAccess(access:CRMAnalyticsAccess|undefined,kind:'crm'|'marketing'|'options',raw?:{propertyId?:string;campaignId?:string;ownerId?:string}){
 if(!access)return;if(kind==='crm'&&!access.crm||kind==='marketing'&&!access.marketing||kind==='options'&&!access.crm&&!access.marketing)throw new ApiError(403,'REPORT_ACCESS_REQUIRED','The original current report authority and purchased history are required.');
 if(raw?.propertyId&&!access.hotel||raw?.campaignId&&!access.marketing||kind==='marketing'&&raw?.ownerId&&!access.crm)throw new ApiError(403,'REPORT_FILTER_UNAVAILABLE','That report filter is unavailable under current source access.');
}
