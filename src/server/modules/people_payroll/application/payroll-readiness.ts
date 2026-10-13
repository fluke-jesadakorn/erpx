import {z} from 'zod';import type {WorkContext} from '../../identity_access/public/authorization';import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';import type {PayrollStatutoryCommands} from './payroll-statutory';import {payrollPeriodForDate,type PayrollFrequency} from "@/lib/people_payroll/payroll-schedule";import type {thaiEvidenceForEmployee,thaiEvidenceGuards,ThaiPayrollEvidence} from '../public/payroll-statutory';import type {payrollAdjustmentInputs,payrollAdjustmentSourceFresh,payrollAdjustmentGuards} from '../public/payroll-adjustments';import type {calculatePayrollItemForSystem} from '../public/people-payroll';import {payrollScheduleSchema,payrollSubmitReviewSchema,type Schedule,type Run,type PayrollBlocker,localDate,equal} from '../domain/payroll-readiness';export interface PayrollReadinessRepository<Effect>{readCostEvidenceBlockersEngagements(company_id:string,pay_group_id:string,currency:string,period_end:string,period_start:string,scope_engagement_id:string | null,scope_engagement_id2:string | null):Promise<{results:({id:string;start_date:string;end_date:string|null;person_id:string;legal_name:string})[]}>;
readCostEvidenceBlockersAssignments(id:string,company_id:string,period_end:string,period_start:string):Promise<{results:({id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string})[]}>;
readCostEvidenceBlockersTimeEntries(id:string,period_start:string,period_end:string):Promise<{results:({work_date:string;cost_assignment_id:string|null;cost_branch_id:string|null;cost_department_id:string|null})[]}>;
readSourceBlockersRows(id:string):Promise<{results:({engagement_id:string;input_snapshot_json:string})[]}>;
readSourceBlockersCurrent(company_id:string,pay_group_id:string,currency:string,period_end:string,period_start:string,scope_engagement_id:string | null,scope_engagement_id2:string | null):Promise<{results:({id:string})[]}>;
readSourceBlockersGroup(pay_group_id:string,company_id:string):Promise<{overtime_multiplier_bps:number;proration_method:string}|null>;
readSourceBlockersEngagement(engagementId:string,company_id:string):Promise<Record<string,unknown>|null>;
readSourceBlockersPayee(value:string,company_id:string):Promise<Record<string,unknown>|null>;
readSourceBlockersCompensation(engagementId:string,period_end:string,period_start:string):Promise<Record<string,unknown>|null>;
readSourceBlockersEntries(engagementId:string,period_start:string,period_end:string):Promise<{results:(Record<string,unknown>)[]}>;
readSourceBlockersCostAssignments(engagementId:string,company_id:string,activeEnd:string,activeStart:string):Promise<{results:(Record<string,unknown>)[]}>;
readSourceBlockersHandoffs(engagementId:string,period_start:string,period_end:string):Promise<{results:(Record<string,unknown>)[]}>;
readSourceBlockersBenefits(engagementId:string,currency:string,period_end:string,period_start:string):Promise<{results:(Record<string,unknown>)[]}>;
readSourceBlockersAdvances(company_id:string,engagementId:string,period_end:string,period_start:string,id:string):Promise<{results:(Record<string,unknown>)[]}>;
readAssertPayrollSourceFreshRun(runId:string):Promise<Run|null>;
readPayrollSourceGuardsRun(runId:string):Promise<{id:string;company_id:string;pay_group_id:string;country_pack_id:string|null;pay_date:string;mode:string;period_start:string;period_end:string;currency:string;scope_engagement_id:string|null;adjustment_of_run_id:string|null}|null>;
readPayrollSourceGuardsRows(runId:string):Promise<{results:({engagement_id:string;input_snapshot_json:string})[]}>;
guard18(key:string,value:"source-employee-count",company_id:string,pay_group_id:string,currency:string,period_end:string,period_start:string,scope_engagement_id:string | null,scope_engagement_id2:string | null,length:number):Effect;
guard19(key:string,value:string,pay_group_id:string,company_id:string,overtimeMultiplierBps:number,prorationMethod:string):Effect;
guard20(key:string,value:string,engagement_id:string,company_id:string,pay_group_id:string,currency:string,version:number):Effect;
guard21(key:string,value:string,engagement_id:string,id:string,company_id:string,version:number,legal_name:string,preferred_name:string | null):Effect;
guard22(key:string,value:string,id:string,engagement_id:string,version:number,period_end:string,period_start:string):Effect;
guard23(key:string,value:string,engagement_id:string,period_start:string,period_end:string,length:number):Effect;
guard24(key:string,value:string,id:string,engagement_id:string,version:number,work_date:string,regular_hours:number,overtime_hours:number,cost_assignment_id:string | null,cost_branch_id:string | null,cost_department_id:string | null):Effect;
guard25(key:string,value:string,engagement_id:string,company_id:string,period_end:string,period_start:string,length:number):Effect;
guard26(key:string,value:string,id:string,engagement_id:string,company_id:string,version:number,effective_from:string,effective_to:string | null,branch_id:string | null,department_id:string | null):Effect;
guard27(key:string,value:string,engagement_id:string,period_start:string,period_end:string,length:number):Effect;
guard28(key:string,value:string,id:string,engagement_id:string,request_version:number,minutes:number,work_date:string):Effect;
guard29(key:string,value:string,engagement_id:string,currency:string,period_end:string,period_start:string,length:number):Effect;
guard30(key:string,value:string,id:string,engagement_id:string,employee_deduction_minor:number,employer_cost_minor:number):Effect;
guard31(key:string,value:string,company_id:string,engagement_id:string,period_end:string,period_start:string,runId:string,length:number):Effect;
guard32(key:string,value:string,id:string,version:number):Effect;
readPayrollReadinessRun(runId:string,companyId:string):Promise<Run|null>;
readPayrollReadinessToday(pay_group_id:string,companyId:string):Promise<{timezone:string}|null>;
readPayrollReadinessEmployees(companyId:string,pay_group_id:string,currency:string,period_end:string,period_start:string,scope_engagement_id:string | null,scope_engagement_id2:string | null):Promise<{count:number}|null>;
readPayrollReadinessContractors(companyId:string,pay_group_id:string,period_end:string,period_start:string):Promise<{count:number}|null>;
readPayrollReadinessMissing(companyId:string,pay_group_id:string,currency:string,period_end:string,period_start:string,scope_engagement_id:string | null,scope_engagement_id2:string | null,currency2:string,period_start2:string,period_end2:string):Promise<{results:({id:string;legal_name:string;person_id:string})[]}>;
readPayrollReadinessMissingOvertimeRates(currency:string,period_start:string,period_end:string,companyId:string,pay_group_id:string,period_end2:string,period_start2:string,scope_engagement_id:string | null,scope_engagement_id2:string | null,period_start3:string,period_end3:string,period_start4:string,period_end4:string):Promise<{results:({person_id:string;legal_name:string})[]}>;
readPayrollReadinessPendingTime(companyId:string,pay_group_id:string,period_start:string,period_end:string):Promise<{count:number}|null>;
readSubmitPayrollReviewRun(runId:string,companyId:string):Promise<{id:string;status:string;version:number;input_digest:string|null}|null>;
guard41(key:string,value:"preview",runId:string,companyId:string,expectedVersion:number,inputDigest:string):Effect;
stageSubmitPayrollReviewEffect(userId:string,inputDigest:string,now:number,now2:number,runId:string,companyId:string,expectedVersion:number):Effect;
readGetPayrollScheduleGroup(groupId:string,companyId:string):Promise<Schedule|null>;
readSavePayrollScheduleGroup(groupId:string,companyId:string):Promise<{id:string;frequency:string;schedule_version:number}|null>;
guard45(key:string,value:"pay-group",groupId:string,companyId:string,expectedVersion:number):Effect;
stageSavePayrollScheduleEffect(enabled:boolean,payDelayDays:number,value:string | null,now:number,groupId:string,companyId:string,expectedVersion:number):Effect;
readDrainPayrollSchedulesGroups():Promise<{results:(Schedule)[]}>;
readDrainPayrollSchedulesExisting(company_id:string,id:string,periodStart:string,periodEnd:string):Promise<Record<string,unknown>|null>;
stageDrainPayrollSchedulesResult(id:string,company_id:string,id2:string,periodStart:string,periodEnd:string,payDate:string,currency:string,timestamp:number,timestamp2:number,company_id2:string,id3:string,periodStart2:string,periodEnd2:string):Promise<{success:boolean;meta:{changes:number;duration?:number;last_row_id?:number|string}}>;
readDrainPayrollScheduledPreviewsDue(value:number):Promise<{results:({id:string;timezone:string})[]}>;
readDrainPayrollScheduledPreviewsRun(id:string):Promise<{
          id:string;company_id:string;pay_group_id:string;period_start:string;period_end:string;pay_date:string;currency:string;
          status:string;version:number;revision:number;timezone:string;overtime_multiplier_bps:number;proration_method:"calendar_days"|"none";
        }|null>;
readDrainPayrollScheduledPreviewsPending(company_id:string,pay_group_id:string,period_start:string,period_end:string):Promise<{count:number}|null>;
readDrainPayrollScheduledPreviewsContractors(company_id:string,pay_group_id:string,period_end:string,period_start:string):Promise<{count:number}|null>;
readDrainPayrollScheduledPreviewsEngagements(company_id:string,pay_group_id:string,currency:string,period_end:string,period_start:string):Promise<{results:({
          id:string;version:number;person_id:string;worker_type:string;employment_type:string;start_date:string;end_date:string|null;
          job_title:string;branch_id:string|null;department_id:string|null;
        })[]}>;
readDrainPayrollScheduledPreviewsIncomplete(company_id:string,pay_group_id:string,currency:string,period_end:string,period_start:string,currency2:string,period_start2:string,period_end2:string):Promise<{count:number}|null>;
stageDrainPayrollScheduledPreviewsCompleted(value:string,digest:string,timestamp:number,id:string):Promise<{success:boolean;meta:{changes:number;duration?:number;last_row_id?:number|string}}>;
stageDrainPayrollScheduledPreviewsCompleted2(value:string,id:string,engagementId:string,workerType:string,value2:string,value3:string,gross:number,deductions:number,employer:number,net:number,currency:string,timestamp:number):Promise<{success:boolean;meta:{changes:number;duration?:number;last_row_id?:number|string}}>;
stageDrainPayrollScheduledPreviewsCompleted3(value:string,commandId:string,id:string,value2:number,timestamp:number):Promise<{success:boolean;meta:{changes:number;duration?:number;last_row_id?:number|string}}>;
stageDrainPayrollScheduledPreviewsCompleted4(value:string,commandId:string,id:string,value2:number,value3:string,timestamp:number):Promise<{success:boolean;meta:{changes:number;duration?:number;last_row_id?:number|string}}>;
stageDrainPayrollScheduledPreviewsEffect(value:string,value2:number,id:string):Promise<{success:boolean;meta:{changes:number;duration?:number;last_row_id?:number|string}}>;}export interface PayrollReadinessPorts<Effect>{repository:PayrollReadinessRepository<Effect>;clock:Clock;identifiers:IdentifierSource;hash(value:string):Promise<string>;authorize(actor:WorkContext,permission:string):WorkContext;prepare(holdingId:string):Promise<void>;commands:PayrollStatutoryCommands<Effect>;thaiEvidence(run:Parameters<typeof thaiEvidenceForEmployee>[1],id:string):ReturnType<typeof thaiEvidenceForEmployee>;thaiGuards(key:string,evidence:ThaiPayrollEvidence,run:Parameters<typeof thaiEvidenceGuards>[3],id:string):Effect[];adjustmentInputs(run:Parameters<typeof payrollAdjustmentInputs>[1]):ReturnType<typeof payrollAdjustmentInputs>;adjustmentFresh(run:Parameters<typeof payrollAdjustmentSourceFresh>[1]):Promise<void>;adjustmentGuards(key:string,run:Parameters<typeof payrollAdjustmentGuards>[2],items:Parameters<typeof payrollAdjustmentGuards>[3]):Promise<Effect[]>;calculate(run:Parameters<typeof calculatePayrollItemForSystem>[1],engagement:Parameters<typeof calculatePayrollItemForSystem>[2],policy:Parameters<typeof calculatePayrollItemForSystem>[3],production:boolean):ReturnType<typeof calculatePayrollItemForSystem>;transactional:boolean;transaction<Result>(run:(ports:PayrollReadinessPorts<Effect>)=>Promise<Result>):Promise<Result>;deferred(runId:string,code:string):void;}export function createPayrollReadinessApplication<Effect>(ports:PayrollReadinessPorts<Effect>){const repository=ports.repository;async function costEvidenceBlockers(run:Pick<Run,"company_id"|"pay_group_id"|"currency"|"period_start"|"period_end"|"scope_engagement_id">):Promise<PayrollBlocker[]> {
  const engagements=(await repository.readCostEvidenceBlockersEngagements(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id)).results;
  const blockers:PayrollBlocker[]=[];
  for(const engagement of engagements){
    const assignments=(await repository.readCostEvidenceBlockersAssignments(engagement.id,run.company_id,run.period_end,run.period_start)).results;
    const start=engagement.start_date>run.period_start?engagement.start_date:run.period_start;
    const end=engagement.end_date&&engagement.end_date<run.period_end?engagement.end_date:run.period_end;
    const assignmentOn=(date:string)=>assignments.filter(row=>row.effective_from<=date&&(row.effective_to===null||row.effective_to>=date));
    let complete=true;
    for(let time=Date.parse(start);time<=Date.parse(end);time+=86400000){
      const matches=assignmentOn(new Date(time).toISOString().slice(0,10));
      if(matches.length!==1||matches[0].evidence_status!=="verified"){complete=false;break;}
    }
    if(!complete){blockers.push({code:"COST_ASSIGNMENT_UNVERIFIED",message:`Verify dated branch and department assignment for ${engagement.legal_name}.`,href:`/people/${encodeURIComponent(engagement.person_id)}`});continue;}
    const timeEntries=(await repository.readCostEvidenceBlockersTimeEntries(engagement.id,run.period_start,run.period_end)).results;
    if(timeEntries.some(entry=>{const matches=assignmentOn(entry.work_date);return matches.length!==1||entry.cost_assignment_id!==matches[0].id||entry.cost_branch_id!==matches[0].branch_id||entry.cost_department_id!==matches[0].department_id;}))
      blockers.push({code:"TIME_COST_EVIDENCE_MISSING",message:`Approved time needs a dated cost unit for ${engagement.legal_name}.`,href:"/people/time"});
  }
  return blockers;
}
async function sourceBlockers(run:Run):Promise<PayrollBlocker[]> {
  if(!run.input_digest)return [{code:"PREVIEW_REQUIRED",message:"Calculate a payroll preview first."}];
  const rows=(await repository.readSourceBlockersRows(run.id)).results;
  if(!rows.length)return [{code:"PAYROLL_EMPTY",message:"Add employees to this pay group before review."}];
  const snapshots=rows.map(row=>JSON.parse(row.input_snapshot_json) as Record<string,unknown>);
  if(await ports.hash(JSON.stringify(snapshots))!==run.input_digest)return [{code:"PREVIEW_DIGEST_CHANGED",message:"Payroll review evidence changed. Recalculate the preview."}];
  if(run.adjustment_of_run_id){try{await ports.adjustmentFresh(run);return [];}catch(error){if(error instanceof BusinessRuleError)return [{code:error.code,message:error.message}];throw error;}}
  const current=(await repository.readSourceBlockersCurrent(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id)).results;
  if(!equal(current.map(row=>row.id),rows.map(row=>row.engagement_id)))return [{code:"PAYROLL_EMPLOYEES_CHANGED",message:"Employees in this pay group changed. Recalculate the preview."}];
  const group=await repository.readSourceBlockersGroup(run.pay_group_id,run.company_id);
  if(!group)return [{code:"PAY_GROUP_CHANGED",message:"The pay group is no longer active."}];
  const blockers:PayrollBlocker[]=[];
  for(let index=0;index<rows.length;index++){
    const snapshot=snapshots[index],engagementId=rows[index].engagement_id;
    if(snapshot.thaiStatutory){
      try {if(!equal(snapshot.thaiStatutory,await ports.thaiEvidence(run,engagementId)))blockers.push({code:"PAYROLL_STATUTORY_SOURCE_CHANGED",message:"Statutory rules, allowances or prior payroll changed. Recalculate this payroll."});}
      catch(error){if(error instanceof BusinessRuleError)blockers.push({code:error.code,message:error.message});else throw error;}
      if(blockers.length)break;
    }else if(run.mode==="production"){
      blockers.push({code:"PAYROLL_STATUTORY_ADAPTER_REQUIRED",message:"Calculate this run with its active Thai statutory adapter."});break;
    }
    const engagement=await repository.readSourceBlockersEngagement(engagementId,run.company_id);
    const payee=await repository.readSourceBlockersPayee((engagement as {person_id?:string}|null)?.person_id??"",run.company_id);
    const compensation=await repository.readSourceBlockersCompensation(engagementId,run.period_end,run.period_start);
    const entries=(await repository.readSourceBlockersEntries(engagementId,run.period_start,run.period_end)).results;
    const activeStart=String((engagement as {start_date:string}|null)?.start_date??run.period_start)>run.period_start?String((engagement as {start_date:string}).start_date):run.period_start;
    const endDate=(engagement as {end_date?:string|null}|null)?.end_date;
    const activeEnd=endDate&&endDate<run.period_end?endDate:run.period_end;
    const costAssignments=(await repository.readSourceBlockersCostAssignments(engagementId,run.company_id,activeEnd,activeStart)).results;
    const handoffs=(await repository.readSourceBlockersHandoffs(engagementId,run.period_start,run.period_end)).results;
    const benefits=(await repository.readSourceBlockersBenefits(engagementId,run.currency,run.period_end,run.period_start)).results;
    const advances=(await repository.readSourceBlockersAdvances(run.company_id,engagementId,run.period_end,run.period_start,run.id)).results;
    const hours=snapshot.hours as {entries:unknown;handoffs:unknown}|undefined;
    const policy=snapshot.policy as {overtimeMultiplierBps:number;prorationMethod:string}|undefined;
    if(!equal(snapshot.engagement,engagement)||!equal(snapshot.payee,payee)||!equal(snapshot.compensation,compensation)||!equal(hours?.entries,entries)||!equal(hours?.handoffs,handoffs)||!equal(snapshot.costAssignments,costAssignments)||!equal(snapshot.benefitInputs,benefits)||!equal(snapshot.advanceRepayments,advances)||!policy||Number(policy.overtimeMultiplierBps)!==Number(group.overtime_multiplier_bps)||policy.prorationMethod!==group.proration_method){
      blockers.push({code:"PAYROLL_SOURCE_CHANGED",message:"An employee's pay source changed. Recalculate the preview.",href:`/people/${encodeURIComponent(String((engagement as {person_id?:string}|null)?.person_id??""))}`});
      break;
    }
  }
  return blockers;
}
async function assertPayrollSourceFresh(runId:string):Promise<void> {
  const run=await repository.readAssertPayrollSourceFreshRun(runId);
  if(!run)throw new BusinessRuleError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if((await sourceBlockers(run)).length)throw new BusinessRuleError(409,"PAYROLL_SOURCE_CHANGED","Payroll inputs changed. Recalculate and submit this preview again.");
}
async function payrollSourceGuards(key:string,runId:string):Promise<Effect[]> {
  const run=await repository.readPayrollSourceGuardsRun(runId);
  if(!run)throw new BusinessRuleError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if(run.adjustment_of_run_id)return ports.adjustmentGuards(key,run,await ports.adjustmentInputs(run));
  const rows=(await repository.readPayrollSourceGuardsRows(runId)).results;
  const guards:Effect[]=[repository.guard18(key,"source-employee-count",run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id,rows.length)];
  for(let index=0;index<rows.length;index++){
    const row=rows[index],snapshot=JSON.parse(row.input_snapshot_json) as {
      engagement:{version:number};compensation:{id:string;version:number}|null;
      payee:{id:string;version:number;legal_name:string;preferred_name:string|null};
      policy:{overtimeMultiplierBps:number;prorationMethod:string};
      hours:{entries:Array<{id:string;version:number;work_date:string;regular_hours:number;overtime_hours:number;cost_assignment_id:string|null;cost_branch_id:string|null;cost_department_id:string|null}>;handoffs:Array<{id:string;request_version:number;minutes:number;work_date:string}>};
      costAssignments:Array<{id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string;version:number}>;
      benefitInputs:Array<{id:string;employee_deduction_minor:number;employer_cost_minor:number}>;
      advanceRepayments:Array<{id:string;version:number}>;
      thaiStatutory?:ThaiPayrollEvidence;
    };
    if(snapshot.thaiStatutory)guards.push(...ports.thaiGuards(key,snapshot.thaiStatutory,run,row.engagement_id));
    guards.push(repository.guard19(key,`source-policy-${index}`,run.pay_group_id,run.company_id,snapshot.policy.overtimeMultiplierBps,snapshot.policy.prorationMethod));
    guards.push(repository.guard20(key,`source-engagement-${index}`,row.engagement_id,run.company_id,run.pay_group_id,run.currency,snapshot.engagement.version));
    if(!snapshot.payee?.id)throw new BusinessRuleError(409,"PAYROLL_PAYEE_REVIEW_REQUIRED","Recalculate and review employee payment identities.");
    guards.push(repository.guard21(key,`source-payee-${index}`,row.engagement_id,snapshot.payee.id,run.company_id,snapshot.payee.version,snapshot.payee.legal_name,snapshot.payee.preferred_name));
    if(snapshot.compensation)guards.push(repository.guard22(key,`source-compensation-${index}`,snapshot.compensation.id,row.engagement_id,snapshot.compensation.version,run.period_end,run.period_start));
    guards.push(repository.guard23(key,`source-time-count-${index}`,row.engagement_id,run.period_start,run.period_end,snapshot.hours.entries.length));
    for(const entry of snapshot.hours.entries)guards.push(repository.guard24(key,`source-time-${index}-${entry.id}`,entry.id,row.engagement_id,entry.version,entry.work_date,entry.regular_hours,entry.overtime_hours,entry.cost_assignment_id,entry.cost_branch_id,entry.cost_department_id));
    if(!Array.isArray(snapshot.costAssignments))throw new BusinessRuleError(409,"PAYROLL_COST_ALLOCATION_INCOMPLETE","Recalculate payroll with dated cost assignments.");
    guards.push(repository.guard25(key,`source-cost-count-${index}`,row.engagement_id,run.company_id,run.period_end,run.period_start,snapshot.costAssignments.length));
    for(const assignment of snapshot.costAssignments)guards.push(repository.guard26(key,`source-cost-${index}-${assignment.id}`,assignment.id,row.engagement_id,run.company_id,assignment.version,assignment.effective_from,assignment.effective_to,assignment.branch_id,assignment.department_id));
    guards.push(repository.guard27(key,`source-handoff-count-${index}`,row.engagement_id,run.period_start,run.period_end,snapshot.hours.handoffs.length));
    for(const handoff of snapshot.hours.handoffs)guards.push(repository.guard28(key,`source-handoff-${index}-${handoff.id}`,handoff.id,row.engagement_id,handoff.request_version,handoff.minutes,handoff.work_date));
    guards.push(repository.guard29(key,`source-benefits-count-${index}`,row.engagement_id,run.currency,run.period_end,run.period_start,snapshot.benefitInputs.length));
    for(const benefit of snapshot.benefitInputs)guards.push(repository.guard30(key,`source-benefit-${index}-${benefit.id}`,benefit.id,row.engagement_id,benefit.employee_deduction_minor,benefit.employer_cost_minor));
    guards.push(repository.guard31(key,`source-advances-count-${index}`,run.company_id,row.engagement_id,run.period_end,run.period_start,runId,snapshot.advanceRepayments.length));
    for(const advance of snapshot.advanceRepayments)guards.push(repository.guard32(key,`source-advance-${index}-${advance.id}`,advance.id,advance.version));
  }
  return guards;
}
async function payrollReadiness(actor:WorkContext,runId:string) {
  actor=ports.authorize(actor,"payroll.read");
  if(actor.branchId||actor.departmentId)throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  await ports.prepare(actor.holdingId);
  const run=await repository.readPayrollReadinessRun(runId,actor.companyId);
  if(!run)throw new BusinessRuleError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if(run.adjustment_of_run_id){
    const blockers=await sourceBlockers(run),sourceFresh=blockers.length===0;
    let readyToCalculate=true;try{await ports.adjustmentInputs(run);}catch(error){readyToCalculate=false;if(error instanceof BusinessRuleError)blockers.push({code:error.code,message:error.message});else throw error;}
    if(new Date(ports.clock.now()).toISOString().slice(0,10)<=run.period_end){readyToCalculate=false;blockers.push({code:"PERIOD_OPEN",message:"The original work period must be closed before correcting payroll."});}
    return {runId,status:run.status,version:run.version,inputDigest:run.input_digest,readyToCalculate,readyToSubmit:readyToCalculate&&sourceFresh&&run.status==='preview',readyToApprove:readyToCalculate&&sourceFresh&&run.status==='in_review'&&!!run.submitted_by&&run.submitted_digest===run.input_digest,sourceFresh,blockers};
  }
  const today=localDate(new Date(ports.clock.now()),(await repository.readPayrollReadinessToday(run.pay_group_id,actor.companyId))?.timezone??"UTC");
  const blockers:PayrollBlocker[]=[];
  if(today<=run.period_end)blockers.push({code:"PERIOD_OPEN",message:"Payroll preview is available after the work period closes."});
  if(run.pay_date<run.period_end||(run.pay_date===run.period_end&&run.mode!=="final_pay"))blockers.push({code:"PAY_DATE_INVALID",message:run.mode==="final_pay"?"Final pay must be on or after the last work day.":"Pay date must follow the work period."});
  const employees=await repository.readPayrollReadinessEmployees(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id);
  if(Number(employees?.count??0)===0)blockers.push({code:"PAYROLL_EMPTY",message:"Assign employees to this pay group before calculating."});
  if(run.mode!=="final_pay"){
    const contractors=await repository.readPayrollReadinessContractors(actor.companyId,run.pay_group_id,run.period_end,run.period_start);
    if(Number(contractors?.count??0)>0)blockers.push({code:"CONTRACTOR_PAYABLE_ROUTE",message:"Move contractors to the Finance payable flow before submitting this payroll."});
  }
  const missing=(await repository.readPayrollReadinessMissing(actor.companyId,run.pay_group_id,run.currency,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id,run.currency,run.period_start,run.period_end)).results;
  for(const row of missing)blockers.push({code:"COMPENSATION_MISSING",message:`Approved compensation is missing for ${row.legal_name}.`,href:`/people/${encodeURIComponent(row.person_id)}`});
  const missingOvertimeRates=(await repository.readPayrollReadinessMissingOvertimeRates(run.currency,run.period_start,run.period_end,actor.companyId,run.pay_group_id,run.period_end,run.period_start,run.scope_engagement_id,run.scope_engagement_id,run.period_start,run.period_end,run.period_start,run.period_end)).results;
  for(const row of missingOvertimeRates)blockers.push({code:"OVERTIME_RATE_MISSING",message:`Approved hourly rate is missing for ${row.legal_name}'s overtime.`,href:`/people/${encodeURIComponent(row.person_id)}`});
  const pendingTime=await repository.readPayrollReadinessPendingTime(actor.companyId,run.pay_group_id,run.period_start,run.period_end);
  if(Number(pendingTime?.count??0)>0)blockers.push({code:"TIME_PENDING",message:"Approve or reject time entries in this period before payroll review.",href:"/people/time"});
  blockers.push(...await costEvidenceBlockers(run));
  const source=await sourceBlockers(run);
  const sourceFresh=source.length===0;
  if(run.input_digest)blockers.push(...source);
  const readyToCalculate=!blockers.some(item=>["PERIOD_OPEN","PAY_DATE_INVALID","PAYROLL_EMPTY","COMPENSATION_MISSING","OVERTIME_RATE_MISSING","TIME_PENDING","CONTRACTOR_PAYABLE_ROUTE","COST_ASSIGNMENT_UNVERIFIED","TIME_COST_EVIDENCE_MISSING"].includes(item.code));
  const readyToSubmit=readyToCalculate&&sourceFresh&&run.status==="preview";
  const readyToApprove=readyToCalculate&&sourceFresh&&run.status==="in_review"&&!!run.submitted_by&&run.submitted_digest===run.input_digest;
  return {runId,status:run.status,version:run.version,inputDigest:run.input_digest,readyToCalculate,readyToSubmit,readyToApprove,sourceFresh,blockers};
}
async function submitPayrollReview(actor:WorkContext,runId:string,input:z.infer<typeof payrollSubmitReviewSchema>,key:string) {
  actor=ports.authorize(actor,"payroll.prepare");
  if(actor.branchId||actor.departmentId)throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  await ports.prepare(actor.holdingId);
  const prior=await ports.commands.replay<{id:string;status:string;version:number;submittedBy:string;inputDigest:string}>(actor,key,"people.payroll.submit-review",runId,input);if(prior)return prior;
  const run=await repository.readSubmitPayrollReviewRun(runId,actor.companyId);
  if(!run)throw new BusinessRuleError(404,"PAYROLL_RUN_NOT_FOUND","Payroll run not found.");
  if(run.status!=="preview"||run.version!==input.expectedVersion||run.input_digest!==input.inputDigest)throw new BusinessRuleError(409,"PAYROLL_REVIEW_CHANGED","Refresh the payroll preview before submitting review.");
  const readiness=await payrollReadiness(actor,runId);
  if(!readiness.readyToSubmit)throw new BusinessRuleError(409,"PAYROLL_NOT_READY",readiness.blockers[0]?.message??"Resolve payroll readiness issues first.");
  const now=ports.clock.now(),result={id:runId,status:"in_review",version:run.version+1,submittedBy:actor.userId,inputDigest:input.inputDigest};
  return ports.commands.commit(actor,key,"people.payroll.submit-review",input,runId,run.version+1,result,[
    repository.guard41(key,"preview",runId,actor.companyId,input.expectedVersion,input.inputDigest),
    ...await payrollSourceGuards(key,runId),
    repository.stageSubmitPayrollReviewEffect(actor.userId,input.inputDigest,now,now,runId,actor.companyId,input.expectedVersion),
  ]);
}
async function getPayrollSchedule(actor:WorkContext,groupId:string){
  actor=ports.authorize(actor,"payroll.read");if(actor.branchId||actor.departmentId)throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  await ports.prepare(actor.holdingId);
  const group=await repository.readGetPayrollScheduleGroup(groupId,actor.companyId);
  if(!group)throw new BusinessRuleError(404,"PAY_GROUP_NOT_FOUND","Pay group not found.");
  return {id:group.id,frequency:group.frequency,timezone:group.timezone,currency:group.currency,enabled:group.schedule_enabled,payDelayDays:group.pay_delay_days,anchorDate:group.schedule_anchor_date,version:group.schedule_version};
}
async function savePayrollSchedule(actor:WorkContext,groupId:string,input:z.infer<typeof payrollScheduleSchema>,key:string){
  actor=ports.authorize(actor,"payroll.prepare");if(actor.branchId||actor.departmentId)throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
  await ports.prepare(actor.holdingId);const prior=await ports.commands.replay(actor,key,"people.payroll.schedule",groupId,input);if(prior)return prior;
  const group=await repository.readSavePayrollScheduleGroup(groupId,actor.companyId);
  if(!group)throw new BusinessRuleError(404,"PAY_GROUP_NOT_FOUND","Active pay group not found.");
  if(group.schedule_version!==input.expectedVersion)throw new BusinessRuleError(409,"VERSION_CONFLICT","Pay group schedule changed. Refresh before saving.");
  if(input.enabled&&["weekly","fortnightly"].includes(group.frequency)&&!input.anchorDate)throw new BusinessRuleError(422,"PAYROLL_ANCHOR_REQUIRED","Set an anchor date for weekly or fortnightly payroll.");
  const result={id:groupId,enabled:input.enabled,payDelayDays:input.payDelayDays,anchorDate:input.anchorDate??null,version:group.schedule_version+1};
  return ports.commands.commit(actor,key,"people.payroll.schedule",input,groupId,result.version,result,[
    repository.guard45(key,"pay-group",groupId,actor.companyId,input.expectedVersion),
    repository.stageSavePayrollScheduleEffect(input.enabled,input.payDelayDays,input.anchorDate??null,ports.clock.now(),groupId,actor.companyId,input.expectedVersion),
  ]);
}
async function drainPayrollSchedules(now:Date):Promise<number>{
  const groups=(await repository.readDrainPayrollSchedulesGroups()).results;
  let created=0;
  for(const group of groups){
    const today=localDate(now,group.timezone),period=payrollPeriodForDate({...group,frequency:group.frequency as PayrollFrequency},today);
    const existing=await repository.readDrainPayrollSchedulesExisting(group.company_id,group.id,period.periodStart,period.periodEnd);
    if(existing)continue;
    const timestamp=now.getTime();
    const id=`payrun-scheduled-${ports.identifiers.next()}`;
    const result=await repository.stageDrainPayrollSchedulesResult(id,group.company_id,group.id,period.periodStart,period.periodEnd,period.payDate,group.currency,timestamp,timestamp,group.company_id,group.id,period.periodStart,period.periodEnd);
    if(result.meta.changes>0)created++;
  }
  return created;
}
async function drainPayrollScheduledPreviews(now:Date):Promise<number>{
  if(!ports.transactional)throw new Error("PAYROLL_TRANSACTION_REQUIRED");
  const due=(await repository.readDrainPayrollScheduledPreviewsDue(now.getTime()-600_000)).results;
  let calculated=0;
  for(const candidate of due){
    try{
      const completed=await ports.transaction(transactionPorts=>createPayrollReadinessApplication(transactionPorts).previewCandidate(candidate,now));
      if(completed)calculated++;
    }catch(error){
      const code=error instanceof BusinessRuleError?error.code:error instanceof Error?error.name:"PayrollPreviewError";
      ports.deferred(candidate.id,code);
      await repository.stageDrainPayrollScheduledPreviewsEffect(String(code).slice(0,80),now.getTime(),candidate.id);
    }
  }
  return calculated;
}
async function previewCandidate(candidate:{id:string;timezone:string},now:Date){
        const run=await repository.readDrainPayrollScheduledPreviewsRun(candidate.id);
        if(!run||localDate(now,run.timezone)<=run.period_end||run.pay_date<=run.period_end)return false;
        const pending=await repository.readDrainPayrollScheduledPreviewsPending(run.company_id,run.pay_group_id,run.period_start,run.period_end);
        if(Number(pending?.count??0)>0)return false;
        const contractors=await repository.readDrainPayrollScheduledPreviewsContractors(run.company_id,run.pay_group_id,run.period_end,run.period_start);
        if(Number(contractors?.count??0)>0)return false;
        const engagements=(await repository.readDrainPayrollScheduledPreviewsEngagements(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start)).results;
        if(!engagements.length)return false;
        const incomplete=await repository.readDrainPayrollScheduledPreviewsIncomplete(run.company_id,run.pay_group_id,run.currency,run.period_end,run.period_start,run.currency,run.period_start,run.period_end);
        if(Number(incomplete?.count??0)>0)return false;
        if((await costEvidenceBlockers({...run,scope_engagement_id:null})).length)return false;
        
        const policy={overtimeMultiplierBps:Number(run.overtime_multiplier_bps),prorationMethod:run.proration_method};
        const items=await Promise.all(engagements.map(engagement=>ports.calculate(run,engagement,policy,false)));
        if(items.some(item=>!item.inputSnapshot.compensation||item.inputSnapshot.compensation.currency!==run.currency||item.net<0))return false;
        const totals=items.reduce((sum,item)=>({gross:sum.gross+item.gross,deductions:sum.deductions+item.deductions,employer:sum.employer+item.employer,net:sum.net+item.net}),{gross:0,deductions:0,employer:0,net:0});
        if(Object.values(totals).some(value=>!Number.isSafeInteger(value)))throw new Error("PAYROLL_MONEY_OVERFLOW");
        const digest=await ports.hash(JSON.stringify(items.map(item=>item.inputSnapshot))),timestamp=now.getTime();
        const commandId=`system-payroll-preview-${run.id}-${run.revision+1}`;
        await repository.stageDrainPayrollScheduledPreviewsCompleted(JSON.stringify({...totals,headcount:items.length}),digest,timestamp,run.id);
        for(let index=0;index<items.length;index++){
          const item=items[index];
          await repository.stageDrainPayrollScheduledPreviewsCompleted2(`payitem-system-${run.id}-${run.revision+1}-${index}`,run.id,item.engagementId,item.workerType,JSON.stringify(item.inputSnapshot),JSON.stringify(item.result),item.gross,item.deductions,item.employer,item.net,run.currency,timestamp);
        }
        await repository.stageDrainPayrollScheduledPreviewsCompleted3(ports.identifiers.next(),commandId,run.id,run.version+1,timestamp);
        await repository.stageDrainPayrollScheduledPreviewsCompleted4(ports.identifiers.next(),commandId,run.id,run.version+1,JSON.stringify({id:run.id,companyId:run.company_id,status:"preview",system:true}),timestamp);
        return true;
      }return {assertPayrollSourceFresh,payrollSourceGuards,payrollReadiness,submitPayrollReview,getPayrollSchedule,savePayrollSchedule,drainPayrollSchedules,drainPayrollScheduledPreviews,previewCandidate};}
