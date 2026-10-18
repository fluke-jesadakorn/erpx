import {samePerson} from '@/lib/identity_access/iam';
import type {WorkContext} from '../../identity_access/public/authorization';
import {BusinessRuleError,type Clock} from '../../shared';
import {finalPayCancellationSchema,type FinalPayCancellationInput,type FinalPayCancellationSource,type FinalPayCancellationResult,type FinalPayCancellationView} from '../domain/final-pay-cancellation';
export type FinalPayCancellationMutation={actor:WorkContext;source:FinalPayCancellationSource;input:FinalPayCancellationInput;key:string;now:number;result:FinalPayCancellationResult};
export interface FinalPayCancellationPorts{clock:Clock;authorize(actor:WorkContext,permission:string):WorkContext;currentAuthority(actor:WorkContext):Promise<void>;schemaAvailable():Promise<boolean>;source(actor:WorkContext,runId:string):Promise<FinalPayCancellationSource|null>;replay(actor:WorkContext,key:string,id:string,input:FinalPayCancellationInput):Promise<FinalPayCancellationResult|null>;commit(mutation:FinalPayCancellationMutation):Promise<FinalPayCancellationResult>;}
const unposted=new Set(['draft','calculating','failed','preview']);
function scope(actor:WorkContext){if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new BusinessRuleError(403,'COMPANY_PAYROLL_SCOPE_REQUIRED','Use a current Company Payroll preparation assignment.');}
function owner(actor:WorkContext,s:FinalPayCancellationSource){return samePerson(actor,s.preparedBy)||s.assigneeId===actor.userId;}
export function finalPayCancellationBlocker(s:FinalPayCancellationSource):FinalPayCancellationView['unavailableReason']{
 if(s.status==='cancelled')return 'already-cancelled';
 if(s.caseStatus!=='cancelled'||s.taskStatus!=='cancelled')return 'parent-active';
 if(!unposted.has(s.runStatus)||s.reviewed||!s.noMoneyEffects)return 'run-reviewed';
 if(s.status!=='prepared'||s.mode!=='final_pay'||s.clearanceType!=='payroll'||!s.originalPreparation||!s.originalParentCancellation)return 'source-unavailable';
 return null;
}
export function createFinalPayCancellationApplication(ports:FinalPayCancellationPorts){
 async function current(actor:WorkContext){actor=ports.authorize(actor,'payroll.prepare');scope(actor);await ports.currentAuthority(actor);return actor;}
 async function read(actor:WorkContext,runId:string){actor=await current(actor);const s=await ports.source(actor,runId);if(!s||s.companyId!==actor.companyId||s.runId!==runId||!owner(actor,s))throw new BusinessRuleError(404,'FINAL_PAY_PREPARATION_NOT_FOUND','The assigned final-pay preparation was not found.');const schemaAvailable=await ports.schemaAvailable(),blocker=finalPayCancellationBlocker(s);return {companyId:s.companyId,preparationId:s.id,runId:s.runId,taskId:s.taskId,caseId:s.caseId,version:s.version,runVersion:s.runVersion,taskVersion:s.taskVersion,caseVersion:s.caseVersion,status:s.status,runStatus:s.runStatus,schemaAvailable,canCancel:schemaAvailable&&blocker===null,unavailableReason:schemaAvailable?blocker:'schema-required'} satisfies FinalPayCancellationView;}
 async function cancel(actor:WorkContext,runId:string,raw:unknown,key:string){
  const input=finalPayCancellationSchema.parse(raw);actor=await current(actor);
  if(!await ports.schemaAvailable())throw new BusinessRuleError(503,'FINAL_PAY_CANCELLATION_SCHEMA_REQUIRED','The reviewed final-pay cancellation schema is required.');
  const s=await ports.source(actor,runId);if(!s||s.companyId!==actor.companyId||s.runId!==runId||!owner(actor,s))throw new BusinessRuleError(404,'FINAL_PAY_PREPARATION_NOT_FOUND','The assigned final-pay preparation was not found.');
  const replay=await ports.replay(actor,key,s.id,input);if(replay)return replay;
  if(s.version!==input.expectedVersion||s.runVersion!==input.expectedRunVersion||s.taskVersion!==input.expectedTaskVersion||s.caseVersion!==input.expectedCaseVersion)throw new BusinessRuleError(409,'FINAL_PAY_CANCELLATION_SOURCE_CHANGED','Review the current preparation, run, clearance task and lifecycle case versions.');
  const blocker=finalPayCancellationBlocker(s);if(blocker)throw new BusinessRuleError(409,'FINAL_PAY_CANCELLATION_UNAVAILABLE',blocker==='parent-active'?'Only a final-pay preparation whose original lifecycle case and clearance task were normally cancelled can be closed.':'Reviewed, posted or otherwise unsettled final pay cannot use this cancellation.');
  const now=ports.clock.now(),result:FinalPayCancellationResult={id:s.id,runId:s.runId,taskId:s.taskId,caseId:s.caseId,status:'cancelled',version:s.version+1,runVersion:s.runVersion+1,taskVersion:s.taskVersion,caseVersion:s.caseVersion,cancellationId:`final-pay-cancel-event-${key}`};
  return ports.commit({actor,source:s,input,key,now,result});
 }
 return {getFinalPayCancellation:read,cancelFinalPayPreparation:cancel};
}
