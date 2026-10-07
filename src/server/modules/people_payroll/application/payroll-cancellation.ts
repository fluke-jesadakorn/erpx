import {z} from 'zod';import {samePerson} from '@/lib/iam';import type {WorkContext} from '../../identity_access/public/authorization';import {BusinessRuleError,type Clock} from '../../shared';import type {resolveFlow} from '../../workflow/public/flow-routing';import {cancellableStates,payrollCancelSchema,type CancellationRun,type PayrollCancelInput} from '../domain/payroll-cancellation';export type CancellationMutation={actor:WorkContext;runId:string;input:PayrollCancelInput;key:string;run:CancellationRun;evidenceId:string;now:number;step:string};export interface PayrollCancellationPorts{clock:Clock;authorize(actor:WorkContext,permission:string):WorkContext;prepare(actor:WorkContext):Promise<void>;currentGrant(actor:WorkContext):Promise<boolean>;replay(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;run(actor:WorkContext,id:string):Promise<CancellationRun|null>;flow(actor:WorkContext,id:string):ReturnType<typeof resolveFlow>;commit<Result>(mutation:CancellationMutation,result:Result):Promise<Result>;}export function createPayrollCancellationApplication(ports:PayrollCancellationPorts){async function cancelPayrollRun(actor: WorkContext, runId: string, input: z.infer<typeof payrollCancelSchema>, key: string) {
  actor = ports.authorize(actor, "payroll.prepare");
  if (actor.branchId || actor.departmentId) throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company Payroll preparation assignment.");
  await ports.prepare(actor);
  if(!await ports.currentGrant(actor))throw new BusinessRuleError(403,'PAYROLL_CANCELLATION_FORBIDDEN','Your current Company role does not permit payroll preparation.');
  const prior = await ports.replay(actor, key, "people.payroll.cancel", runId, input);
  if (prior) return prior;
  const run = await ports.run(actor,runId);
  if (!run) throw new BusinessRuleError(404, "PAYROLL_RUN_NOT_FOUND", "Payroll run not found in this Company.");
  if (Number(run.version) !== input.expectedVersion) throw new BusinessRuleError(409, "PAYROLL_RUN_CHANGED", "Reload the payroll run before cancelling it.");
  if (!cancellableStates.has(run.status) || run.mode === "final_pay" || run.submitted_by || run.approved_by || run.posted_by)
    throw new BusinessRuleError(409, "PAYROLL_CANCELLATION_UNAVAILABLE", "Only an unsubmitted ordinary payroll draft or preview can be cancelled.");
  const flow = await ports.flow(actor,runId);
  const step = run.status === "preview" ? "preview" : "calculate";
  const primary = flow.steps.find(candidate => candidate.key === step)?.primary?.userId;
  if (!samePerson(actor, run.created_by) && primary !== actor.userId)
    throw new BusinessRuleError(403, "PAYROLL_CANCELLATION_OWNER_REQUIRED", "The creator or named payroll preparer must cancel this run.");
  const evidenceId = `payroll-cancellation-${key}`, now = ports.clock.now();
  return ports.commit({actor,runId,input,key,run,evidenceId,now,step},{id:runId,status:'cancelled',version:input.expectedVersion+1,cancellationId:evidenceId});
}return {cancelPayrollRun};}
