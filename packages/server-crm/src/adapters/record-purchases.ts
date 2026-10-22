import 'server-only';
import {salesRecipientEligibility} from '@erpx/server-sales/public/sales-recipient';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkItem} from '@erpx/feature-work_projection/lib/work-item';
import {commercePackageEnforcementMode,readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {crmPurchaseLeadRows,crmPurchaseActivityRows,crmPurchaseOpportunityRows,crmPurchaseAccountRows,crmPurchaseContactRows,crmPurchaseGuestLinkRows,requireCRMPurchasedRead,crmReferenceHistory,type CRMPurchaseTarget} from '@erpx/server-crm/adapters/purchases';
const nativeIds='SELECT id FROM crm_leads UNION ALL SELECT id FROM crm_lead_activities UNION ALL SELECT id FROM crm_opportunities UNION ALL SELECT id FROM crm_accounts UNION ALL SELECT id FROM crm_contacts UNION ALL SELECT id FROM crm_guest_links';
const validLeads=`SELECT lead.* FROM (${crmPurchaseLeadRows}) lead WHERE NOT EXISTS(SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id=lead.id) OR EXISTS(SELECT 1 FROM (${crmPurchaseOpportunityRows}) opportunity WHERE opportunity.id=lead.id AND opportunity.company_id=lead.company_id)`;
/** Native record identity is classified independently of generic module labels.
 * Opportunity IDs are mapped to the original lead-ID command/read interface. */
const recordRows=`SELECT lead.id,'lead' kind,lead.id target_id,lead.company_id,lead.branch_id,lead.department_id,NULL::text created_by,NULL::text account_id,NULL::text contact_id FROM (${validLeads}) lead
 UNION ALL SELECT activity.id,'activity',activity.id,activity.company_id,activity.branch_id,activity.department_id,NULL,NULL,NULL FROM (${crmPurchaseActivityRows}) activity JOIN (${validLeads}) lead ON lead.id=(SELECT lead_id FROM crm_lead_activities WHERE id=activity.id) AND lead.company_id=activity.company_id
 UNION ALL SELECT opportunity.id,'lead',lead.id,lead.company_id,lead.branch_id,lead.department_id,NULL,NULL,NULL FROM crm_opportunities opportunity JOIN (${validLeads}) lead ON lead.id=opportunity.lead_id
 UNION ALL SELECT account.id,'account',account.id,account.company_id,NULL,NULL,account.created_by,account.id,NULL FROM (${crmPurchaseAccountRows}) account
 UNION ALL SELECT contact.id,'contact',contact.id,contact.company_id,NULL,NULL,contact.created_by,NULL,contact.id FROM (${crmPurchaseContactRows}) contact
 UNION ALL SELECT link.id,'guest-link',link.id,link.company_id,link.branch_id,NULL,link.created_by,NULL,native.contact_id FROM (${crmPurchaseGuestLinkRows}) link JOIN crm_guest_links native ON native.id=link.id`;
function scope(actor:WorkContext,id:string,company:string){
 const history=purchasedCapabilityPredicate('crm_source.company_id','sales.crm','history','crm_clock.now_ms'),hotel=purchasedCapabilityPredicate('crm_source.company_id','pms.reservations','history','crm_clock.now_ms');
 return {where:`EXISTS(WITH crm_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${recordRows}) crm_source CROSS JOIN crm_clock WHERE crm_source.id=${id} AND crm_source.company_id=${company} AND ${hasPermission(actor,'commerce.read')?'TRUE':'FALSE'} AND ${history}
 AND CASE WHEN crm_source.kind IN ('lead','activity') THEN (?::text IS NULL OR crm_source.branch_id=?) AND (?::text IS NULL OR crm_source.department_id=?) ELSE (?::text IS NULL OR crm_source.created_by=? OR EXISTS(SELECT 1 FROM (${validLeads}) scoped_lead JOIN crm_leads native ON native.id=scoped_lead.id WHERE scoped_lead.company_id=crm_source.company_id AND (native.account_id=crm_source.account_id OR native.contact_id=crm_source.contact_id) AND scoped_lead.branch_id=? AND (?::text IS NULL OR scoped_lead.department_id=?))) END
 AND EXISTS(SELECT 1 FROM (SELECT crm_source.company_id,COALESCE(crm_source.branch_id,?::text) branch_id,COALESCE(crm_source.department_id,?::text) department_id) crm_authority WHERE ${assignmentReadSql('crm_authority','commerce.read')})
 AND (crm_source.kind<>'guest-link' OR (${hasPermission(actor,'pms.read')?'TRUE':'FALSE'} AND (?::text IS NULL OR crm_source.branch_id=?) AND ${hotel} AND ${assignmentReadSql('crm_source','pms.read')})))`,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.branchId,actor.userId,actor.branchId,actor.departmentId,actor.departmentId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]};
}
export async function crmSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await commercePackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccesses(database,mode,actor.companyId,['sales.crm']);return {where:'TRUE',args:[] as unknown[]};}
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id',type=surface==='staff'?'staff_notifications.record_type':'e.source_type',actual=`EXISTS(SELECT 1 FROM (${nativeIds}) native_crm WHERE native_crm.id=${id})`,relevant=surface==='records'?actual:`(${type} IN ('crm_lead','crm_activity','crm_account','crm_contact','crm_opportunity','crm_guest_link') OR ${actual})`,source=scope(actor,id,company);
 return {where:`(NOT ${relevant} OR ${source.where})`,args:source.args};
}
export async function requireCRMWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,_moduleId:string,id:string){
 if(await commercePackageEnforcementMode()==='disabled')return;
 const row=await database.prepare(`SELECT kind,target_id FROM (${recordRows}) actual WHERE id=? AND company_id=?`).bind(id,actor.companyId).first<{kind:CRMPurchaseTarget['kind'];target_id:string}>();
 if(row){await requireCRMPurchasedRead(database,actor,{kind:row.kind,id:row.target_id});if(row.kind==='guest-link'&&!await crmReferenceHistory(database,actor,'pms.reservations'))throw new ApiError(404,'CRM_SOURCE_NOT_FOUND','The original Hotel identity source is unavailable.');}
 else if(await database.prepare(`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`).bind(id).first())throw new ApiError(404,'CRM_SOURCE_NOT_FOUND','The original CRM source is unavailable in this workspace.');
}
/** Calendar and Sales customer readers retain their original scopes and apply
 * current CRM source/authority/purchase before grouping or limiting rows. */
export async function crmLeadProjectionScope(database:SqlDatabase,actor:WorkContext,alias:'l'){
 const mode=await commercePackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccesses(database,mode,actor.companyId,['sales.crm']);return {where:'TRUE',args:[] as unknown[]};}
 const history=purchasedCapabilityPredicate('crm_projection.company_id','sales.crm','history','crm_projection_clock.now_ms');
 return {where:`EXISTS(WITH crm_projection_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${validLeads}) crm_projection CROSS JOIN crm_projection_clock WHERE crm_projection.id=${alias}.id AND crm_projection.company_id=${alias}.company_id AND ${hasPermission(actor,'commerce.read')?'TRUE':'FALSE'} AND ${assignmentReadSql('crm_projection','commerce.read')} AND ${history})`,args:[actor.assignmentId,actor.userId,actor.role]};
}
export async function filterCRMPurchasedWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 const selected=items.filter(item=>item.type==='crm_lead'||item.type==='crm_activity');if(!selected.length||await commercePackageEnforcementMode()!=='enforced')return items;
 const ids=[...new Set(selected.map(item=>item.id))],visible=scope(actor,'crm_work.id','crm_work.company_id'),purchase=(await readPurchasedAccesses(database,'enforced',actor.companyId,['sales.crm']))['sales.crm'];
 const rows=(await database.prepare(`SELECT crm_work.id,crm_work.kind,${hasPermission(actor,'commerce.sales')?'TRUE':'FALSE'} AND ${assignmentReadSql('crm_work','commerce.sales')} AND CASE WHEN crm_work.kind='lead' THEN EXISTS(SELECT 1 FROM crm_leads l LEFT JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id WHERE l.id=crm_work.target_id AND l.company_id=crm_work.company_id AND l.owner_id=? AND l.status<>'disqualified' AND COALESCE(opportunity.stage,'discovery') NOT IN ('won','lost') AND ${salesRecipientEligibility('l','l.owner_id')}) ELSE EXISTS(SELECT 1 FROM crm_lead_activities c JOIN crm_leads l ON l.id=c.lead_id AND l.company_id=c.company_id WHERE c.id=crm_work.id AND c.owner_id=? AND c.status='open' AND ${salesRecipientEligibility('l','c.owner_id')}) END can_act FROM (${recordRows}) crm_work WHERE crm_work.company_id=? AND crm_work.id IN (${ids.map(()=>'?').join(',')}) AND crm_work.kind IN ('lead','activity') AND crm_work.target_id=crm_work.id AND ${visible.where}`).bind(actor.assignmentId,actor.userId,actor.role,actor.userId,actor.userId,actor.companyId,...ids,...visible.args).all<{id:string;kind:string;can_act:boolean}>()).results,map=new Map(rows.map(row=>[`${row.kind}:${row.id}`,row]));
 return items.flatMap(item=>{if(item.type!=='crm_lead'&&item.type!=='crm_activity')return [item];const row=map.get(`${item.type==='crm_lead'?'lead':'activity'}:${item.id}`);if(!row)return [];if(purchase.settleExisting&&row.can_act)return [item];return [{...item,canAct:false,actionKind:'work' as const,queue:item.queue==='completed'?'completed' as const:'waiting' as const,nextAction:'Review retained CRM source',flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value))}];});
}
