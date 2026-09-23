import { beforeAll,beforeEach,describe,it,expect,vi } from 'vitest';
import { env,applyMigrations } from './runtime-env';
import type { WorkContext } from '@/server/authorization';
import { createSelfServiceLeave,editSelfServiceLeave,previewLeave,runLeaveAccruals,leaveSummary,normalizeLegacyLeave,administerLeave,leaveAdminSchema } from '@/server/leave-self-service';
import { approveLeave,transitionLeave,decideLeaveCancellation,getLeave } from '@/server/leave';
import { createOvertime,commandOvertime,getOvertime,previewOvertime,overtimeWork,setOvertimeEligibility } from '@/server/overtime';
import { enrichFlowWork } from '@/server/flow-work';
import { listStaffNotifications } from '@/server/staff-notifications';
import { createPayrollRun,calculatePayrollRun,payrollRunSchema } from '@/server/people-payroll';
import { leaveItems } from '@/server/calendar/leave';
import { calendarAccess } from '@/server/calendar/access';
import { calendarQuerySchema } from '@/lib/calendar';
import { overtimeDetailsSchema } from '@/lib/overtime';
vi.mock('@/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES})}));
const db=env.H001_DB,key=()=>crypto.randomUUID();
let company:string,employee:WorkContext,manager:WorkContext,hr:WorkContext,other:WorkContext,type:string,policy:string;
async function person(name:string,role:string):Promise<WorkContext>{
 const userId=`${company}-${name}`,personId=`p-${userId}`,engagementId=`e-${userId}`,assignmentId=`a-${userId}`;
 await db.batch([
  db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,'[]',?,0,9999999999999,'holding')").bind(assignmentId,userId,role,company),
  db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(personId,userId,company,name),
  db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,'employee','salaried','active','2026-01-01','Staff','THB','fixture',1,1)").bind(engagementId,personId,company),
 ]);
 return {userId,assignmentId,role,companyId:company,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'test',holdingName:'Test',name,email:`${name}@example.test`,scope:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company},assignmentScope:'holding',inherited:true,modules:[],assignments:[],workspaces:[]} as WorkContext;
}
beforeAll(()=>applyMigrations(db,env.STAFF_TEST_MIGRATIONS));
beforeEach(async()=>{
 company=`self-${key()}`;await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Self service',?)").bind(company,company,company).run();
 employee=await person('Employee','employee');manager=await person('Manager','people_manager');hr=await person('HR','hr_admin');other=await person('Other','employee');
 await db.prepare('UPDATE hr_engagements SET manager_person_id=? WHERE id=?').bind(`p-${manager.userId}`,`e-${employee.userId}`).run();
 type=`type-${company}`;policy=`policy-${company}`;
 await db.batch([
  db.prepare("INSERT INTO leave_types(id,company_id,code,name,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?,'annual','Annual leave',true,'2026-01-01','fixture',1,1)").bind(type,company),
  db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,created_by,created_at) VALUES(?,?,'annual',1,'Company leave','active','2026-01-01','Asia/Bangkok','fixture',1)").bind(policy,company),
  db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units) VALUES(?,?,'annual',10)").bind(policy,type),
  db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,'2026-01-01','fixture',1)").bind(`assignment-${company}`,company,`e-${employee.userId}`,policy),
 ]);
 await runLeaveAccruals(db,'2026-09-17');
});
const leaveInput=()=>({leaveTypeId:type,startDate:'2026-10-05',endDate:'2026-10-05',startSegment:'full' as const,endSegment:'full' as const,handoverNote:'Private handover',saveOnly:false});
async function sums(id:string){return (await db.prepare('SELECT kind,sum(units)::float units FROM leave_balance_entries WHERE request_id=? GROUP BY kind ORDER BY kind').bind(id).all<{kind:string;units:number}>()).results;}
const actual=()=>overtimeDetailsSchema.parse({mode:'retrospective',startLocal:'2026-09-10T22:00',endLocal:'2026-09-11T02:00',timezone:'Asia/Bangkok',breakMinutes:30,reason:'Inventory work',lateExplanation:'Submitted after the overnight count.'});
it('holds scheduled leave accrual until People is enabled for the company in production',async()=>{
 vi.stubEnv('NODE_ENV','production');
 try {
  expect(await runLeaveAccruals(db,'2027-01-01')).toBe(0);
  await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'people_payroll','pilot',1,'Test leave accrual','test',?)").bind(company,Date.now()).run();
  expect(await runLeaveAccruals(db,'2027-01-01')).toBeGreaterThan(0);
 } finally {vi.unstubAllEnvs();}
});
async function otAction(actor:WorkContext,id:string,action:'submit'|'authorize'|'actuals'|'verify'|'approve'|'return'|'withdraw',details?:ReturnType<typeof actual>,commandKey=key()){
 const row=await getOvertime(actor,id);return commandOvertime(actor,id,{action,expectedVersion:row.version,note:'Reviewed independently',details},commandKey);
}
describe('Staff self-service transactional journeys',()=>{
 it('reserves once under retries; rejection releases without usage and resubmission re-previews',async()=>{
  const input=leaveInput(),k=key(),created=await createSelfServiceLeave(employee,input,k),id=String(created.id);
  expect(await createSelfServiceLeave(employee,input,k)).toEqual(created);
  expect(await sums(id)).toEqual([{kind:'reservation',units:-1}]);
  await transitionLeave(manager,id,{action:'reject',expectedVersion:1,note:'Choose another date'},key());
  expect(await sums(id)).toEqual([{kind:'reservation',units:-1},{kind:'reservation_release',units:1}]);
  await editSelfServiceLeave(employee,id,{...input,startSegment:'first_half',endSegment:'first_half',expectedVersion:2},key());
  expect((await getLeave('h001',id,employee)).calculated_units).toBe(.5);
 });
 it('serializes competing entitlement requests and supports non-overlapping halves',async()=>{
  await db.prepare("INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,created_by,created_at) VALUES(?,?,?,?,'adjustment',-9,'2026-01-01',?,'fixture',1)").bind(key(),company,`e-${employee.userId}`,type,key()).run();
  const input=leaveInput();const outcomes=await Promise.allSettled([createSelfServiceLeave(employee,input,key()),createSelfServiceLeave(employee,{...input,startDate:'2026-10-06',endDate:'2026-10-06'},key())]);
  expect(outcomes.filter(o=>o.status==='fulfilled')).toHaveLength(1);
  expect((await leaveSummary(employee)).balances[0].available).toBe(0);
 });
 it('keeps approved absence until both cancellation reviews, with exactly one reversal',async()=>{
  const id=String((await createSelfServiceLeave(employee,leaveInput(),key())).id);
  await approveLeave('h001',id,{expectedVersion:1,note:''},key(),manager);
  await approveLeave('h001',id,{expectedVersion:2,note:''},key(),hr);
  const k=key(),command={action:'cancel' as const,expectedVersion:3,note:'Plans changed'};
  const cancellation=await transitionLeave(employee,id,command,k);expect(await transitionLeave(employee,id,command,k)).toEqual(cancellation);
  expect((await getLeave('h001',id,employee)).state).toBe('approved');
  await decideLeaveCancellation(manager,id,{expectedVersion:4,note:''},key(),true);
  expect((await getLeave('h001',id,employee)).state).toBe('approved');
  const finalKey=key(),input={expectedVersion:5,note:''};await decideLeaveCancellation(hr,id,input,finalKey,true);await decideLeaveCancellation(hr,id,input,finalKey,true);
  expect((await getLeave('h001',id,employee)).state).toBe('cancelled');expect((await sums(id)).find(e=>e.kind==='usage_reversal')?.units).toBe(1);
 });
 it('freezes policy snapshots and requires evidence before submission',async()=>{
  await db.prepare('UPDATE leave_types SET evidence_after_units=.5 WHERE id=?').bind(type).run();
  await expect(createSelfServiceLeave(employee,leaveInput(),key())).rejects.toMatchObject({code:'LEAVE_EVIDENCE_REQUIRED'});
  const draft=await createSelfServiceLeave(employee,{...leaveInput(),saveOnly:true},key());
  await expect(editSelfServiceLeave(employee,String(draft.id),{...leaveInput(),expectedVersion:9},key())).rejects.toMatchObject({code:'STALE_VERSION'});
  await expect(getLeave('h001',String(draft.id),other)).rejects.toMatchObject({status:404});
 });
 it('catches up missed accrual periods once and isolates balances by year',async()=>{
  await db.prepare("UPDATE leave_policy_rules SET grant_kind='accrual',accrual_units=1,accrual_period='monthly' WHERE policy_version_id=?").bind(policy).run();
  expect(await runLeaveAccruals(db,'2026-09-17')).toBe(9);expect(await runLeaveAccruals(db,'2026-09-18')).toBe(0);
  const p=await previewLeave(employee,{...leaveInput(),startDate:'2027-01-04',endDate:'2027-01-04'});expect(p.accounts[0].available).toBe(0);expect(p.canSubmit).toBe(false);
 });
 it('reviews retrospective overnight actuals and creates exactly one immutable payroll handoff',async()=>{
  const id=String((await createOvertime(employee,actual(),key())).id);await otAction(employee,id,'submit');
  await expect(otAction(hr,id,'approve')).rejects.toMatchObject({status:403});
  await otAction(manager,id,'verify');const current=await getOvertime(hr,id),k=key(),input={action:'approve' as const,expectedVersion:current.version,note:'Checked'};
  await commandOvertime(hr,id,input,k);await commandOvertime(hr,id,input,k);
  const row=await getOvertime(employee,id);expect(row.minutes).toBe(210);expect(row.state).toBe('approved');expect(row.permittedActions).toEqual([]);
  expect(await db.prepare('SELECT count(*)::int n FROM overtime_payroll_handoffs WHERE request_id=?').bind(id).first()).toEqual({n:1});
  expect((await enrichFlowWork(employee,await overtimeWork(employee))).find(r=>r.id===id)).toMatchObject({status:'approved',queue:'completed',href:`/overtime/${id}`});
  expect((await listStaffNotifications(db,employee)).filter(n=>n.record_id===id).every(n=>n.href===`/overtime/${id}`)).toBe(true);
  const payroll=await person('Payroll','payroll_preparer'),group=`paygroup-${company}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES(?,?,'Test payroll','TH','THB','monthly','Asia/Bangkok','fixture',1,1)").bind(group,company).run();
  await db.prepare('UPDATE hr_engagements SET pay_group_id=? WHERE id=?').bind(group,row.engagement_id).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-01-01','monthly',10000,0,160,'THB','[]','approved',?,'fixture',1,1)").bind(`comp-${company}`,row.engagement_id,hr.userId).run();
  await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','calculate',?,'fixture','Test payroll preparer',1)").bind(company,payroll.userId).run();
  const run=await createPayrollRun(payroll,payrollRunSchema.parse({payGroupId:group,periodStart:'2026-09-01',periodEnd:'2026-09-30',payDate:'2026-09-30',currency:'THB'}),key());
  await calculatePayrollRun(payroll,String(run.id),key());
  const item=await db.prepare('SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=?').bind(run.id).first<{input_snapshot_json:string}>();
  expect(JSON.parse(item!.input_snapshot_json).hours).toMatchObject({overtime:3.5,handoffs:[{request_id:id,minutes:210}]});

  await expect(db.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,note,status,submitted_by,created_at,updated_at) VALUES(?,?,?,'2026-09-10',0,3.5,'duplicate','submitted',?,1,1)").bind(key(),row.engagement_id,company,employee.userId).run()).rejects.toThrow('OVERTIME_RECONCILIATION_REQUIRED');
 });
 it('separates planned authorization, actuals corrections and fresh independent reviews',async()=>{
  const planned=overtimeDetailsSchema.parse({...actual(),mode:'advance',startLocal:'2027-01-04T18:00',endLocal:'2027-01-04T20:00',breakMinutes:0});
  const id=String((await createOvertime(employee,planned,key())).id);await otAction(employee,id,'submit');await otAction(manager,id,'authorize');
  expect((await getOvertime(employee,id)).handoff).toBeNull();
  await otAction(employee,id,'actuals',{...actual(),mode:'advance'});await otAction(manager,id,'verify');await otAction(hr,id,'return');
  expect((await getOvertime(employee,id)).state).toBe('returned');
  await otAction(employee,id,'actuals',{...actual(),mode:'advance',breakMinutes:60});expect((await getOvertime(employee,id)).state).toBe('pending_verification');
 });
 it('blocks future actuals, overlaps, contractor defaults and cross-company access',async()=>{
  expect((await previewOvertime(employee,{...actual(),endLocal:'2027-09-11T02:00',startLocal:'2027-09-10T22:00'})).blockers.join(' ')).toContain('future');
  const id=String((await createOvertime(employee,actual(),key())).id);await otAction(employee,id,'submit');expect((await previewOvertime(employee,actual())).blockers.join(' ')).toContain('overlaps');
  await expect(getOvertime({...employee,companyId:'other-company'},id)).rejects.toMatchObject({status:404});
  await db.prepare("UPDATE hr_engagements SET worker_type='contractor',employment_type='contractor' WHERE id=?").bind(`e-${employee.userId}`).run();
  expect((await previewOvertime(employee,actual())).blockers.join(' ')).toContain('eligibility');
 });
 it('blocks handoff when manual overtime requires explicit reconciliation',async()=>{
  const id=String((await createOvertime(employee,actual(),key())).id);await otAction(employee,id,'submit');await otAction(manager,id,'verify');
  await db.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,note,status,submitted_by,created_at,updated_at) VALUES(?,?,?,'2026-09-10',0,2,'manual','submitted',?,1,1)").bind(key(),`e-${employee.userId}`,company,employee.userId).run();
  await expect(otAction(hr,id,'approve')).rejects.toMatchObject({code:'OVERTIME_RECONCILIATION'});expect((await getOvertime(employee,id)).state).toBe('pending_hr');
 });
 it('permits separate half days, excludes nonworking gaps and freezes policy details',async()=>{
  const input=leaveInput();
  expect((await normalizeLegacyLeave(employee,{startDate:'2026-10-09',days:2,handoverNote:''})).endDate).toBe('2026-10-12');
  await createSelfServiceLeave(employee,{...input,startSegment:'first_half',endSegment:'first_half'},key());
  const second=await createSelfServiceLeave(employee,{...input,startSegment:'second_half',endSegment:'second_half'},key());
  expect((await getLeave('h001',String(second.id),employee)).calculated_units).toBe(.5);
  const span=await createSelfServiceLeave(employee,{...input,startDate:'2026-10-09',endDate:'2026-10-12'},key()),id=String(span.id);
  await approveLeave('h001',id,{expectedVersion:1,note:''},key(),manager);await approveLeave('h001',id,{expectedVersion:2,note:''},key(),hr);
  const snapshot=JSON.parse((await getLeave('h001',id,employee)).calculation_snapshot!);
  expect(snapshot.policy.workweek).toEqual([1,2,3,4,5]);
  const items=await leaveItems(db,await calendarAccess(db,employee),calendarQuerySchema.parse({date:'2026-10-09',mode:'mine'}),'2026-10-09','2026-10-13',100);
  expect(items.filter(i=>i.sourceId===id).map(i=>i.startDate)).toEqual(['2026-10-09','2026-10-12']);
 });

 it('requires explicit contractor eligibility and rejects stale configuration writes',async()=>{
  const engagementId=`e-${employee.userId}`,command={engagementId,eligible:true,topLevel:false,reason:'Test contractor agreement'},k=key();
  await db.prepare("UPDATE hr_engagements SET worker_type='contractor',employment_type='contractor' WHERE id=?").bind(engagementId).run();
  expect((await previewOvertime(employee,actual())).canSubmit).toBe(false);
  const saved=await setOvertimeEligibility(hr,command,k);expect(await setOvertimeEligibility(hr,command,k)).toEqual(saved);
  expect((await previewOvertime(employee,actual())).canSubmit).toBe(true);
  await expect(setOvertimeEligibility(hr,{...command,eligible:false},key())).rejects.toMatchObject({code:'STALE_VERSION'});
  await expect(setOvertimeEligibility(other,{...command,expectedVersion:1},key())).rejects.toMatchObject({status:403});
 });
 it('administers multiple leave rules, assignments and adjustments with recoverable receipts',async()=>{
  const admin=(raw:unknown,k=key())=>administerLeave(hr,leaveAdminSchema.parse(raw),k);
  const second=await admin({resource:'type',code:'test-personal',name:'Personal time',tracked:false});
  const configured=await admin({resource:'policy',policyKey:'test-multi',name:'Multiple types',version:1,effectiveFrom:'2026-09-17',timezone:'Asia/Bangkok',workweek:[1,2,3,4,5],rules:[{leaveTypeId:type,grantKind:'annual',grantUnits:10},{leaveTypeId:second.id,grantKind:'untracked'}]});
  const assignment=await admin({resource:'assignment',engagementId:`e-${other.userId}`,policyVersionId:configured.id,effectiveFrom:'2026-09-17'});
  const command={resource:'adjustment',engagementId:`e-${other.userId}`,leaveTypeId:type,units:-.5,effectiveDate:'2026-09-17',note:'Audited adjustment'},k=key();
  expect(await admin(command,k)).toEqual(await admin(command,k));
  expect((await leaveSummary(other)).balances.find(b=>b.type.id===type)?.available).toBe(9.5);
  await admin({resource:'assignment_end',id:assignment.id,expectedVersion:1,effectiveTo:'2026-09-30'});
  await expect(admin({resource:'assignment_end',id:assignment.id,expectedVersion:1,effectiveTo:'2026-10-01'})).rejects.toMatchObject({code:'STALE_VERSION'});
 });

});
