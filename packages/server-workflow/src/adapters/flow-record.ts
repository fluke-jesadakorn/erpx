import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import type{FlowKind,FlowStep}from '@erpx/feature-shared/lib/flow';
import type{PermissionActor}from '@erpx/feature-identity_access/lib/iam';
import{flowRoleAllowed as pureflowRoleAllowed,flowCommandTarget as pureflowCommandTarget,type Row}from '@erpx/server-workflow/domain/flow-routing';
import{createFlowRecordReader}from '@erpx/server-workflow/application/flow-routing';
import{workflowRecord}from '@erpx/server-workflow/adapters/document-sources';
import{policyRead}from '@erpx/server-identity_access/public/iam-read-snapshot';
import{BusinessRuleError}from '@erpx/server-shared/index';
import{ApiError as NativeApiError}from '@erpx/server-runtime/public/http';
function application(database:SqlDatabase){return{flowRecord:createFlowRecordReader({repository:{readFlowRecordRow:async(companyId,id,kind)=>{const row=await workflowRecord(database,kind,companyId,id);if(row&&kind==='order')return {...row,has_service_lines:await commerceFlowRouting(database).readServiceOrder(companyId,id)?1:0};return row;}},policyRead:(key,read)=>policyRead(database,key,read)})};}
import{createFlowRoutingSourceRepository as commerceFlowRouting}from '@erpx/server-commerce/public/workflow-flow-routing-facts';
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
function invokeSync<Result>(run: () => Result) { try {
    return run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function flowRecord(db: SqlDatabase, companyId: string, kind: FlowKind, id: string): Promise<Row> { return invoke(() => application(db).flowRecord(companyId, kind, id)); }
export function flowRoleAllowed(authority: string | PermissionActor, step: FlowStep | null) { return invokeSync(() => pureflowRoleAllowed(authority, step)); }
export function flowCommandTarget(operation: string, id: string, input: unknown): {
    kind: FlowKind;
    id: string;
    action: string;
} | null { return invokeSync(() => pureflowCommandTarget(operation, id, input)); }
