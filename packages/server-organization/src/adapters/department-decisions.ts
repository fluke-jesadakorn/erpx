import "server-only";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {approvalView} from '@erpx/server-workflow/public/approval-planning';
import {departmentOverrideReceiptOperation,departmentOverridePendingApprovals} from '@erpx/server-workflow/public/department-override-facts';
import {departmentOverrideRecordVersion,departmentOverrideApproverName,departmentOverrideOvertimeState} from '@erpx/server-people_payroll/public/department-override-facts';
import {departmentOverrideRequest,departmentOverridePendingRequests} from '@erpx/server-records/public/department-override-facts';
import {transitionExpense} from '@erpx/server-people_payroll/public/expenses';
import {approveLeave,transitionLeave} from '@erpx/server-people_payroll/public/leave';
import {commandOvertime} from '@erpx/server-people_payroll/public/overtime';
import {actOnEmployeeRequest,requestActionSchema} from '@erpx/server-records/public/employee-requests';
import {canOverrideEmployeeRequest} from '@erpx/server-records/public/request-override-read';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createDepartmentOverrideApplication,type DepartmentOverridePorts} from '@erpx/server-organization/application/department-decisions';
import type {DepartmentOverrideKind} from '@erpx/server-organization/domain/department-decisions';
function application(actor:WorkContext){let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);const ports:DepartmentOverridePorts<unknown>={request:async(id)=>departmentOverrideRequest(await database(),actor.companyId,id),canOverrideRequest:async(row)=>canOverrideEmployeeRequest(await database(),actor,row),approverName:async(id)=>departmentOverrideApproverName(await database(),actor.companyId,id),approval:async(kind,id)=>approvalView(await database(),kind,id,actor),recordVersion:async(kind,id)=>departmentOverrideRecordVersion(await database(),actor.companyId,kind,id),receiptOperation:async(key)=>departmentOverrideReceiptOperation(await database(),key),overtimeState:async(id)=>departmentOverrideOvertimeState(await database(),actor.companyId,id),
 decideRequest:async(input,key)=>actOnEmployeeRequest(await database(),actor,input.id,requestActionSchema.parse({action:input.outcome==='approve'?'approve':'reject',expectedVersion:input.expectedVersion,note:input.reason,override:true}),key),
 decideExpense:async(input,key)=>transitionExpense(actor,input.id,input.outcome,{expectedVersion:input.expectedVersion,reason:input.reason,override:true},key),
 decideLeave:async(input,key)=>input.outcome==='approve'?approveLeave(actor.holdingId,input.id,{expectedVersion:input.expectedVersion,note:input.reason,override:true},key,actor):transitionLeave(actor,input.id,{action:'reject',expectedVersion:input.expectedVersion,note:input.reason,override:true},key),
 decideOvertime:async(input,action,key)=>commandOvertime(actor,input.id,{action,expectedVersion:input.expectedVersion,note:input.reason,override:true,attachmentSetIds:[]},key),pending:async()=>departmentOverridePendingApprovals(await database(),actor.companyId),pendingRequests:async()=>departmentOverridePendingRequests(await database(),actor.companyId)};return createDepartmentOverrideApplication(ports);}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const departmentOverrideDetail=(a:WorkContext,kind:DepartmentOverrideKind,id:string)=>invoke(()=>application(a).detail(kind,id));
export const decideDepartmentOverride=(a:WorkContext,raw:unknown,key:string)=>invoke(()=>application(a).decide(raw,key));
export const departmentOverrideWork=(a:WorkContext)=>invoke(()=>application(a).work());
