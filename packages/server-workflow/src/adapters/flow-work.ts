import {filterWorkTaskWork} from "@erpx/server-operations/public/record-purchases";
import {filterServiceWork} from "@erpx/server-field_service/public/record-purchases";
import {filterBenefitWork} from "@erpx/server-benefits/public/record-purchases";
import {filterWorkforceWork} from "@erpx/server-people_payroll/public/workforce-record-purchases";
import {filterFollowupPurchasedWork} from "@erpx/server-sales/public/followup-purchases";
import {filterCRMPurchasedWork} from "@erpx/server-crm/public/record-purchases";
import {filterMarketingPurchasedWork} from "@erpx/server-marketing/public/record-purchases";
import {filterLegalPurchasedWork} from "@erpx/server-legal/public/work-purchases";
import {filterLearningWork} from "@erpx/server-learning/public/record-purchases";
import {filterProjectWork} from "@erpx/server-projects/public/record-purchases";
import {filterPeopleLifecycleWork} from "@erpx/server-people_payroll/public/record-purchases";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { requireFlowAccess } from '@erpx/server-workflow/public/flow-access';
import { resolveFlow } from '@erpx/server-workflow/public/flow-routing';
import { policyRead } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { createFlowWorkApplication } from '@erpx/server-workflow/application/flow-work';
import { flowWorkActionKind, workFlowKind } from '@erpx/server-workflow/domain/flow-work';
import { workflowReferenceRows as commerceReferences } from '@erpx/server-commerce/public/workflow-read-sources';
import { workflowReferenceRows as financeReferences } from '@erpx/server-finance/public/workflow-document-sources';
import { workflowReferenceRows as peopleReferences } from '@erpx/server-people_payroll/public/workflow-document-sources';
import {financeFlowAccess,requireFinanceDocumentHistory} from '@erpx/server-finance/public/document-purchases';
import { payrollPurchasedAccess, withPayrollPurchasedRead } from '@erpx/server-people_payroll/public/payroll-purchases';
import {commerceWorkAccess,commerceWorkReferenceRows} from '@erpx/server-commerce/public/record-purchases';
import {countSourceAccess} from '@erpx/server-inventory/public/count-purchases';
import {transferSourceAccess} from '@erpx/server-inventory/public/transfer-purchases';
export { flowWorkActionKind, workFlowKind };
function application(provided?: SqlDatabase) {
    let write = provided!;
    return createFlowWorkApplication({ prepare: async (holding) => { if (!provided)
            write = (await holdingResources(holding)).write; }, policyRead: (key, read) => policyRead(write, key, read), resolveFlow: (actor, kind, id) => fromNative(() => resolveFlow(write, actor, kind, id)), requireFlowAccess: (actor, kind, id) => fromNative(async () => { await requireFlowAccess(actor, kind, id, write); }), workRows: (actor, kind, reference) => fromNative(async () => {
            if (reference && kind === 'payroll') return withPayrollPurchasedRead(actor, async () => (await peopleReferences(write, kind, actor.companyId)).results, write);
            if (reference && kind === 'order') return commerceWorkReferenceRows(write,actor);
            if (reference)
                return (await (kind === 'order' ? commerceReferences(write, kind, actor.companyId) : kind === 'payroll' ? peopleReferences(write, kind, actor.companyId) : financeReferences(write, kind, actor.companyId))).results;
            switch (kind) {
                case 'order': return (await import('@erpx/server-commerce/public/commerce')).workOrderRows(actor, write);
                case 'invoice': return (await import('@erpx/server-finance/public/finance')).workInvoiceRows(actor, write);
                case 'journal': return (await import('@erpx/server-finance/public/ledger')).workJournalRows(actor, write);
                case 'payroll': return (await import('@erpx/server-people_payroll/public/people-payroll')).workPayrollRows(actor, write);
            }
        }) });
}
async function fromNative<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
async function currentFinancialItems(actor:WorkContext,items:WorkItem[]):Promise<WorkItem[]>{
 if(!items.some(item=>item.type==="invoice"||item.type==="journal"))return items;
 const database=(await holdingResources(actor.holdingId)).write,access=await financeFlowAccess(database,actor);
 if(access.history&&access.settleExisting)return items;
 const visible:WorkItem[]=[];
 for(const item of items){
  if(item.type!=="invoice"&&item.type!=="journal"){visible.push(item);continue;}
  if(!access.history){try{await requireFinanceDocumentHistory(database,actor,item.type,item.id);}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))continue;throw error;}}
  visible.push({...item,canAct:false,actionKind:"work",queue:item.queue==="completed"?"completed":"waiting",nextAction:"Review retained Finance source",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))});
 }
 return visible;
}
async function currentPayrollItems(actor: WorkContext, items: WorkItem[]): Promise<WorkItem[]> {
  if (!items.some(item => item.type === "payroll")) return items;
  const access = await payrollPurchasedAccess((await holdingResources(actor.holdingId)).write, actor);
  if (!access || access.history && access.settleExisting) return items;
  return items.flatMap(item => item.type !== "payroll" ? [item] : !access.history ? [] : [{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review retained payroll source",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))}]);
}
async function currentCommerceItems(actor:WorkContext,items:WorkItem[]):Promise<WorkItem[]>{
 const ids=items.filter(item=>item.type==="order").map(item=>item.id);if(!ids.length)return items;
 const access=await commerceWorkAccess((await holdingResources(actor.holdingId)).write,actor,ids);if(access===null)return items;
 return items.flatMap(item=>{if(item.type!=="order")return [item];const source=access.get(item.id);if(!source?.history)return [];if(source.canAct)return [item];
  return [{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review retained Commerce source",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))}];});
}
async function currentCountItems(actor:WorkContext,items:WorkItem[]):Promise<WorkItem[]>{
 if(!items.some(item=>item.type==="stock_count"))return items;
 const database=(await holdingResources(actor.holdingId)).write,output:WorkItem[]=[];
 for(const item of items){if(item.type!=="stock_count"){output.push(item);continue;}
  try{const access=await countSourceAccess(database,actor,item.id);if(!access){output.push(item);continue;}if(!access.history)continue;
   if(["draft","rejected"].includes(item.status)?access.newWork:access.settleExisting){output.push(item);continue;}
   output.push({...item,canAct:false,actionKind:"work",queue:item.queue==="completed"?"completed":"waiting",nextAction:"Review retained stock count",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))});
  }catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))continue;throw error;}
 }return output;
}
async function currentTransferItems(actor:WorkContext,items:WorkItem[]):Promise<WorkItem[]>{
 if(!items.some(item=>item.type==="transfer"))return items;
 const access=await transferSourceAccess((await holdingResources(actor.holdingId)).write,actor);if(!access)return items;
 return items.flatMap(item=>{if(item.type!=="transfer")return [item];if(!access.history)return [];
  if(["draft","rejected"].includes(item.status)?access.newWork:access.settleExisting)return [item];
  return [{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review retained warehouse transfer",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))}];});
}
async function currentPeopleItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="people_lifecycle")?filterPeopleLifecycleWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentProjectItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==='staff_task'||item.type==='project_handoff')?filterProjectWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentLearningItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==='learning_assignment')?filterLearningWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentLegalItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="legal")?filterLegalPurchasedWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentCRMItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="crm_lead"||item.type==="crm_activity")?filterCRMPurchasedWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentMarketingItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="marketing_candidate")?filterMarketingPurchasedWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentFollowupItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="customer_followup")?filterFollowupPurchasedWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentWorkforceItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="leave"||item.type==="overtime")?filterWorkforceWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentBenefitItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="benefit_enrollment")?filterBenefitWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentServiceItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="field_service")?filterServiceWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentWorkTaskItems(actor:WorkContext,items:WorkItem[]){return items.some(item=>item.type==="staff_task")?filterWorkTaskWork((await holdingResources(actor.holdingId)).write,actor,items):items;}
async function currentOwnerItems(actor:WorkContext,items:WorkItem[]){
 for(const filter of [currentFinancialItems,currentPayrollItems,currentCommerceItems,currentCountItems,currentTransferItems,currentPeopleItems,currentProjectItems,currentLearningItems,currentLegalItems,currentCRMItems,currentMarketingItems,currentFollowupItems,currentWorkforceItems,currentBenefitItems,currentServiceItems,currentWorkTaskItems])items=await filter(actor,items);
 return items;
}
export async function enrichFlowWork(actor: WorkContext, items: WorkItem[]) { const authorized=await currentOwnerItems(actor,items);return currentOwnerItems(actor,await invoke(() => application().enrichFlowWork(actor, authorized))); }
export async function otherFlowWork(actor: WorkContext, reference = false): Promise<WorkItem[]> { return currentOwnerItems(actor,await invoke(() => application().otherFlowWork(actor, reference))); }
