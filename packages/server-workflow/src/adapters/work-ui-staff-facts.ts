import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {nativeWorkflowReadAll,nativeWorkflowReadFirst} from "./native-workflow-read";
export function workUiStaffFacts(db:SqlDatabase){return {
async approvalQueue(companyId:string,value:0 | 1){return {results:await nativeWorkflowReadAll<Record<string,unknown>>('wf_native_work_ui_approval_queue',[companyId,value],companyId,db)};},
async betaApproval(value:string,companyId:string){return nativeWorkflowReadFirst<Record<string,unknown>>('wf_native_work_ui_beta_approval',[value,companyId],companyId,db);}
};}
