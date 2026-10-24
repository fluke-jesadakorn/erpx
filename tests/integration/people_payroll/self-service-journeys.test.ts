import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { afterEach,beforeAll,beforeEach,describe,it,expect,vi } from 'vitest';
import { env,applyMigrations } from "../../support/runtime-env";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { createSelfServiceLeave,editSelfServiceLeave,previewLeave,runLeaveAccruals,leaveSummary,normalizeLegacyLeave,administerLeave,leaveAdminSchema } from "@erpx/server-people_payroll/public/leave-self-service";
import { approveLeave,transitionLeave,decideLeaveCancellation,getLeave } from "@erpx/server-people_payroll/public/leave";
import { createOvertime,commandOvertime,getOvertime,previewOvertime,overtimeWork,setOvertimeEligibility } from "@erpx/server-people_payroll/public/overtime";
import { enrichFlowWork } from "@erpx/server-workflow/public/flow-work";
import { listStaffNotifications } from "@erpx/server-collaboration/public/staff-notifications";
import { createPayrollRun,calculatePayrollRun,payrollRunSchema } from "@erpx/server-people_payroll/public/people-payroll";
import { leaveItems } from "@erpx/server-work_projection/public/calendar/leave";
import { calendarAccess } from "@erpx/server-work_projection/public/calendar/access";
import { calendarQuerySchema } from "@erpx/feature-work_projection/lib/calendar";
import { overtimeDetailsSchema } from "@erpx/feature-people_payroll/lib/overtime";
import { assertLeaveAttachmentAccess,availableLeaveEvidence,leaveEvidenceManifest,pendingLeaveEvidenceCount } from "@erpx/server-people_payroll/public/leave-evidence";
import { withdrawAttachmentLink } from "@erpx/server-files/public/attachments/service";
import { getLeaveEvidence } from "@erpx/server-work_projection/public/work";
import { prepareLocalBrowserHrReviewer,prepareLocalBrowserLeaveRecords } from "../../../fixtures/people_payroll/builders/local-browser-leave";
import {peopleRequestRoute} from '@erpx/server-organization/public/department-authority-reads';
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:workforceDatabase??env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const db=env.H001_DB,key=()=>crypto.randomUUID();
let workforceDatabase:SqlDatabase|undefined;
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
 const department=`department-${company}`;
 await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id) VALUES(?,'department',?,'Self service department',?,?)").bind(department,department,company,company).run();
 employee=await person('Employee','employee');manager=await person('Manager','people_manager');hr=await person('HR','hr_admin');other=await person('Other','employee');
 await db.prepare('UPDATE hr_engagements SET manager_person_id=?,department_id=? WHERE id=?').bind(`p-${manager.userId}`,department,`e-${employee.userId}`).run();
 await db.prepare("INSERT INTO department_heads(department_id,company_id,person_id,updated_by,updated_at) VALUES(?,?,?,?,?)").bind(department,company,`p-${manager.userId}`,'fixture',1).run();
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
it('prepares missing browser leave prerequisites without approving the request, changing other policies or duplicating grants',async()=>{
 const existing=await db.prepare('SELECT id,policy_version_id,version FROM leave_policy_assignments WHERE company_id=? AND engagement_id=?').bind(company,`e-${employee.userId}`).first();
 await db.prepare('UPDATE hr_engagements SET department_id=? WHERE id=?').bind(`department-${company}`,`e-${other.userId}`).run();
 expect((await leaveSummary(other)).setupMessage).toContain('a leave policy has not been assigned');
 const input={companyId:company,userId:other.userId,requestId:`browser-request-${company}`,requestNumber:`LV-${company}`,now:Date.UTC(2026,9,6)};
 await prepareLocalBrowserLeaveRecords(db,input);
 const summary=await leaveSummary(other);
 expect(summary.setupMessage).toBeNull();
 expect(summary.balances).toHaveLength(1);expect(summary.balances[0].available).toBe(30);
 const request=await getLeave('h001',input.requestId,other);
 expect(request.state).toBe('draft');expect(request.permittedActions).toContain('submit');
 expect(request.approval.blockedReason).toBeNull();
 await db.prepare("UPDATE leave_requests SET version=4,handover_note='Retained case correction' WHERE id=?").bind(input.requestId).run();
 await prepareLocalBrowserLeaveRecords(db,{...input,now:input.now+1});
 expect(await db.prepare('SELECT state,version,handover_note FROM leave_requests WHERE id=?').bind(input.requestId).first()).toEqual({state:'draft',version:4,handover_note:'Retained case correction'});
 expect((await leaveSummary(other)).balances[0].available).toBe(30);
 expect(await db.prepare('SELECT id,policy_version_id,version FROM leave_policy_assignments WHERE company_id=? AND engagement_id=?').bind(company,`e-${employee.userId}`).first()).toEqual(existing);
 expect(Number(await db.prepare('SELECT count(*) count FROM approval_instances WHERE record_id=?').bind(input.requestId).first('count'))).toBe(0);
});
it('declares the independent browser HR reviewer without depending on role sort order or replacing another appointment',async()=>{
 const owner=await person('Owner','holding_owner'),extraHr=await person('AAA-HR','hr_admin');
 expect((await peopleRequestRoute(db,company,employee.userId)).steps.map(step=>step.userId)).toEqual([manager.userId,extraHr.userId]);
 await expect(prepareLocalBrowserHrReviewer(db,employee,hr.userId)).rejects.toMatchObject({status:403});
 expect(await db.prepare("SELECT person_id FROM authority_seats WHERE company_id=? AND kind='hr'").bind(company).first('person_id')).toBeNull();
 const first=await prepareLocalBrowserHrReviewer(db,owner,hr.userId);
 expect(first).toEqual({personId:`p-${hr.userId}`,version:1});
 expect((await peopleRequestRoute(db,company,employee.userId)).steps.map(step=>step.userId)).toEqual([manager.userId,hr.userId]);
 expect((await loadIamAuthority(db,company,hr.userId,'hr_admin')).permissions).toContain('people.manage');
 expect((await loadIamAuthority(db,company,extraHr.userId,'hr_admin')).permissions).not.toContain('people.manage');
 const snapshot=await db.prepare("SELECT * FROM authority_seats WHERE company_id=? AND kind='hr'").bind(company).first();
 expect(await prepareLocalBrowserHrReviewer(db,owner,hr.userId)).toEqual(first);
 await expect(prepareLocalBrowserHrReviewer(db,owner,extraHr.userId)).rejects.toThrow('BROWSER_HR_APPOINTMENT_ALREADY_OWNED');
 expect(await db.prepare("SELECT * FROM authority_seats WHERE company_id=? AND kind='hr'").bind(company).first()).toEqual(snapshot);
 expect(Number(await db.prepare("SELECT count(*) count FROM authority_appointments WHERE company_id=? AND kind='hr'").bind(company).first('count'))).toBe(1);
 expect(Number(await db.prepare('SELECT count(*) count FROM approval_instances WHERE company_id=?').bind(company).first('count'))).toBe(0);
});
async function sums(id:string){return (await db.prepare('SELECT kind,sum(units)::float units FROM leave_balance_entries WHERE request_id=? GROUP BY kind ORDER BY kind').bind(id).all<{kind:string;units:number}>()).results;}
function parseJson(value:unknown){return typeof value==='string'?JSON.parse(value):value as Record<string,unknown>;}
async function stageLeaveEvidence(
 leaveRequestId:string,
 expectedVersion:number,
 purpose:'medical_certificate'|'leave_supporting_photo',
 mediaType:'application/pdf'|'image/jpeg',
 checksum:string,
 compatibilityAdapter=false,
){
 const upload=key(),source=key(),setId=key(),linkId=key(),filename=mediaType==='application/pdf'?'certificate.pdf':'support.jpg';
 await db.batch([
  db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?,4,4,?,?,?,?,?,9999999999999,1,1)")
   .bind(upload,'leave_request',leaveRequestId,purpose,filename,mediaType,checksum,`staging-${upload}`,`final-${upload}`,'ready',employee.userId),
  db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state) VALUES(?,?,?,?,?,?,1,?,?,4,?,?,1,?,?, 'available')")
   .bind(source,upload,'leave_request',leaveRequestId,purpose,`final-${upload}`,filename,mediaType,checksum,employee.userId,company,mediaType),
  db.prepare(`INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,
    policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at)
    VALUES(?,?,'leave_request',?,'record',?,?,?,1,?,'{}',0,10,'draft',?,1,1)`)
   .bind(setId,company,leaveRequestId,`leave-evidence:${purpose}`,purpose,expectedVersion,purpose==='medical_certificate'?'restricted':'confidential',employee.userId),
  db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,1,9999999999999)")
   .bind(linkId,setId,source,employee.userId),
  ...(compatibilityAdapter?[db.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,'{\"compatibilityAdapter\":true}',1)")
   .bind(`event-${key()}`,company,setId,employee.userId,'set.created')]:[]),
 ]);
 return {upload,source,setId,linkId};
}
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
 const row=await getOvertime(actor,id);return commandOvertime(actor,id,{action,override:false,attachmentSetIds:[],expectedVersion:row.version,note:'Reviewed independently',details},commandKey);
}
describe('Staff self-service transactional journeys',()=>{
 it('reserves once under retries; rejection releases without usage and resubmission re-previews',async()=>{
  const input=leaveInput(),k=key(),created=await createSelfServiceLeave(employee,input,k),id=String(created.id);
  expect(await createSelfServiceLeave(employee,input,k)).toEqual(created);
  expect(await sums(id)).toEqual([{kind:'reservation',units:-1}]);
  await transitionLeave(manager,id,{action:'reject',override:false,expectedVersion:1,note:'Choose another date'},key());
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
  await approveLeave('h001',id,{override:false,expectedVersion:1,note:''},key(),manager);
  await approveLeave('h001',id,{override:false,expectedVersion:2,note:''},key(),hr);
  const k=key(),command={override:false,action:'cancel' as const,expectedVersion:3,note:'Plans changed'};
  const cancellation=await transitionLeave(employee,id,command,k);expect(await transitionLeave(employee,id,command,k)).toEqual(cancellation);
  expect((await getLeave('h001',id,employee)).state).toBe('approved');
  await decideLeaveCancellation(manager,id,{override:false,expectedVersion:4,note:''},key(),true);
  expect((await getLeave('h001',id,employee)).state).toBe('approved');
  const finalKey=key(),input={override:false,expectedVersion:5,note:''};await decideLeaveCancellation(hr,id,input,finalKey,true);await decideLeaveCancellation(hr,id,input,finalKey,true);
  expect((await getLeave('h001',id,employee)).state).toBe('cancelled');expect((await sums(id)).find(e=>e.kind==='usage_reversal')?.units).toBe(1);
 });
 it('freezes versioned medical policy, permits submission first, and gates final approval on a clean certificate',async()=>{
  vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
  try {
  await db.prepare(`UPDATE leave_policy_rules SET evidence_min_units=.5,evidence_purpose='medical_certificate',evidence_min_count=1,
    evidence_due_stage='before_final_approval',evidence_grace_days=2,evidence_note='Attach the medical certificate.'
    WHERE policy_version_id=? AND leave_type_id=?`).bind(policy,type).run();
  const submitted=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(submitted.id);
  const frozen=parseJson((await getLeave('h001',id,employee)).evidence_policy_snapshot);
  expect(frozen).toMatchObject({required:true,purposeKey:'medical_certificate',minimumCount:1,dueStage:'before_final_approval',graceDays:2});
  await expect(assertLeaveAttachmentAccess(db,other,{leaveRequestId:id,purposeKey:'medical_certificate',operation:'read'})).rejects.toMatchObject({status:403});
  await expect(assertLeaveAttachmentAccess(db,manager,{leaveRequestId:id,purposeKey:'medical_certificate',operation:'read'})).resolves.toMatchObject({version:1});
  const supporting=await stageLeaveEvidence(id,1,'leave_supporting_photo','image/jpeg','support-checksum');
  await approveLeave('h001',id,{override:false,expectedVersion:1,note:''},key(),manager);
  expect(await db.prepare('SELECT state,binding_type FROM attachment_links WHERE id=?').bind(supporting.linkId).first()).toEqual({state:'bound',binding_type:'leave_approval'});
  expect((await getLeave('h001',id,employee)).evidence_version).toBe(1);
  await expect(withdrawAttachmentLink(db,employee,supporting.linkId,'Replace the supporting photo',key())).rejects.toMatchObject({code:'BOUND_ATTACHMENT_IMMUTABLE'});
  await expect(approveLeave('h001',id,{override:false,expectedVersion:2,note:''},key(),hr)).rejects.toMatchObject({code:'LEAVE_EVIDENCE_REQUIRED'});
  const certificate=await stageLeaveEvidence(id,2,'medical_certificate','application/pdf','certificate-checksum');
  await approveLeave('h001',id,{override:false,expectedVersion:2,note:''},key(),hr);
  const approved=await getLeave('h001',id,employee);
  expect(approved).toMatchObject({state:'approved',evidence_version:2});
  await expect(assertLeaveAttachmentAccess(db,manager,{leaveRequestId:id,purposeKey:'medical_certificate',operation:'read'}))
    .resolves.toMatchObject({version:3});
  expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(certificate.linkId).first()).toEqual({state:'bound'});
  const manifests=(await db.prepare('SELECT request_version,manifest_json FROM leave_evidence_manifests WHERE request_id=? ORDER BY request_version').bind(id).all<{request_version:number;manifest_json:string}>()).results;
  expect(parseJson(manifests[0].manifest_json).files).toEqual([
   expect.objectContaining({sourceId:supporting.source,purposeKey:'leave_supporting_photo',checksumSha256:'support-checksum'}),
  ]);
  expect(parseJson(manifests[1].manifest_json).files).toEqual(expect.arrayContaining([
   expect.objectContaining({sourceId:supporting.source,purposeKey:'leave_supporting_photo',checksumSha256:'support-checksum'}),
   expect.objectContaining({sourceId:certificate.source,purposeKey:'medical_certificate',checksumSha256:'certificate-checksum'}),
  ]));

  const draft=await createSelfServiceLeave(employee,{...leaveInput(),startDate:'2026-10-06',endDate:'2026-10-06',saveOnly:true},key());
  await expect(editSelfServiceLeave(employee,String(draft.id),{...leaveInput(),startDate:'2026-10-06',endDate:'2026-10-06',expectedVersion:9},key())).rejects.toMatchObject({code:'STALE_VERSION'});
  await expect(getLeave('h001',String(draft.id),other)).rejects.toMatchObject({status:404});
 } finally { vi.unstubAllEnvs(); }
 });
 it('binds compatibility Leave evidence and pins the exact manifest while the global gate is off',async()=>{
  vi.stubEnv('ERPX_ATTACHMENTS_V2','false');
  vi.stubEnv('ERPX_ATTACHMENTS_READ_ONLY','false');
  try {
   await db.prepare(`UPDATE leave_policy_rules SET evidence_min_units=.5,evidence_purpose='medical_certificate',evidence_min_count=1,
     evidence_due_stage='before_final_approval',evidence_grace_days=0,evidence_note='Attach the medical certificate.'
     WHERE policy_version_id=? AND leave_type_id=?`).bind(policy,type).run();
   const request=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(request.id);
   const certificate=await stageLeaveEvidence(id,1,'medical_certificate','application/pdf','gate-off-certificate',true);

   await approveLeave('h001',id,{override:false,expectedVersion:1,note:'Reviewed the compatibility attachment'},key(),manager);
   expect(await db.prepare('SELECT state,binding_type FROM attachment_links WHERE id=?').bind(certificate.linkId).first())
     .toEqual({state:'bound',binding_type:'leave_approval'});
   expect(await db.prepare('SELECT state FROM attachment_sets WHERE id=?').bind(certificate.setId).first())
     .toEqual({state:'bound'});

   await approveLeave('h001',id,{override:false,expectedVersion:2,note:'Final medical evidence review'},key(),hr);
   expect((await getLeave('h001',id,employee)).state).toBe('approved');
   const manifests=(await db.prepare('SELECT manifest_json FROM leave_evidence_manifests WHERE request_id=? ORDER BY request_version').bind(id).all<{manifest_json:string}>()).results;
   expect(manifests).toHaveLength(2);
   for(const manifest of manifests)expect(parseJson(manifest.manifest_json).files).toEqual([
    expect.objectContaining({linkId:certificate.linkId,sourceId:certificate.source,purposeKey:'medical_certificate',checksumSha256:'gate-off-certificate'}),
   ]);
  } finally { vi.unstubAllEnvs(); }
 });
 it('ignores foreign-Company canonical and legacy evidence even when it points at the Leave id',async()=>{
  vi.stubEnv('ERPX_ATTACHMENTS_V2','false');
  vi.stubEnv('ERPX_ATTACHMENTS_READ_ONLY','false');
  try {
   await db.prepare(`UPDATE leave_policy_rules SET evidence_min_units=.5,evidence_purpose='medical_certificate',evidence_min_count=1,
     evidence_due_stage='before_final_approval',evidence_grace_days=0,evidence_note='Attach the medical certificate.'
     WHERE policy_version_id=? AND leave_type_id=?`).bind(policy,type).run();
   const request=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(request.id),foreignCompany=`foreign-${key()}`;
   await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Foreign company',?)")
     .bind(foreignCompany,foreignCompany,foreignCompany).run();
   const availableUpload=key(),availableSource=key(),pendingUpload=key(),pendingSource=key(),setId=key(),availableLink=key(),pendingLink=key(),localSetId=key(),crossCompanyLink=key(),legacyUpload=key(),legacySource=key();
   await db.batch([
    db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES(?,'leave_request',?,'medical_certificate','foreign.pdf','application/pdf',4,4,'foreign-clean',?,?,'ready',?,9999999999999,1,1,?)")
      .bind(availableUpload,id,`staging-${availableUpload}`,`final-${availableUpload}`,employee.userId,foreignCompany),
    db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state) VALUES(?,?,'leave_request',?,'medical_certificate',?,1,'foreign.pdf','application/pdf',4,'foreign-clean',?,1,?,'application/pdf','available')")
      .bind(availableSource,availableUpload,id,`final-${availableUpload}`,employee.userId,foreignCompany),
    db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES(?,'leave_request',?,'medical_certificate','foreign-pending.pdf','application/pdf',4,4,'foreign-pending',?,?,'uploaded',?,9999999999999,1,1,?)")
      .bind(pendingUpload,id,`staging-${pendingUpload}`,`final-${pendingUpload}`,employee.userId,foreignCompany),
    db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state) VALUES(?,?,'leave_request',?,'medical_certificate',?,1,'foreign-pending.pdf','application/pdf',4,'foreign-pending',?,1,?,'application/pdf','quarantined')")
      .bind(pendingSource,pendingUpload,id,`final-${pendingUpload}`,employee.userId,foreignCompany),
    db.prepare(`INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,
      policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at)
      VALUES(?,?,'leave_request',?,'record','leave-evidence:medical_certificate','medical_certificate',1,1,'restricted','{}',0,10,'draft',?,1,1)`)
      .bind(setId,foreignCompany,id,employee.userId),
    db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,1,9999999999999)")
      .bind(availableLink,setId,availableSource,employee.userId),
    db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,1,9999999999999)")
      .bind(pendingLink,setId,pendingSource,employee.userId),
    db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES(?,'leave_request',?,'medical_certificate','foreign-legacy.pdf','application/pdf',4,4,'foreign-legacy',?,?,'ready',?,9999999999999,1,1,?)")
      .bind(legacyUpload,id,`staging-${legacyUpload}`,`final-${legacyUpload}`,employee.userId,foreignCompany),
    db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state) VALUES(?,?,'leave_request',?,'medical_certificate',?,1,'foreign-legacy.pdf','application/pdf',4,'foreign-legacy',?,1,?,'application/pdf','available')")
      .bind(legacySource,legacyUpload,id,`final-${legacyUpload}`,employee.userId,foreignCompany),
   ]);

   vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
   await expect(getLeaveEvidence('h001',id,employee)).resolves.toEqual([]);
   vi.stubEnv('ERPX_ATTACHMENTS_V2','false');
   await expect(getLeaveEvidence('h001',id,employee)).resolves.toEqual([]);
   await expect(availableLeaveEvidence(db,id,{purposeKey:'medical_certificate',minimumCount:1})).resolves.toEqual({count:0,setIds:[]});
   await expect(leaveEvidenceManifest(db,id)).resolves.toMatchObject({manifest:[],canonicalManifest:[],legacyManifest:[],draftSetIds:[]});
   await expect(pendingLeaveEvidenceCount(db,id)).resolves.toBe(0);
   await approveLeave('h001',id,{override:false,expectedVersion:1,note:'No in-scope evidence was submitted'},key(),manager);
   await expect(approveLeave('h001',id,{override:false,expectedVersion:2,note:'Final review'},key(),hr))
     .rejects.toMatchObject({code:'LEAVE_EVIDENCE_REQUIRED'});
   await db.batch([
    db.prepare(`INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,
      policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at)
      VALUES(?,?,'leave_request',?,'record','leave-evidence:cross-company-source','medical_certificate',2,1,'restricted','{}',0,10,'draft',?,2,2)`)
      .bind(localSetId,company,id,employee.userId),
    db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES(?,?,?,?,'draft',?,2,9999999999999)")
      .bind(crossCompanyLink,localSetId,availableSource,availableUpload,employee.userId),
   ]);
   vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
   await expect(getLeaveEvidence('h001',id,employee)).resolves.toEqual([]);
  } finally { vi.unstubAllEnvs(); }
 });
 it('revokes medical-evidence access immediately when the current approver assignment is removed',async()=>{
  await db.prepare(`UPDATE leave_policy_rules SET evidence_min_units=.5,evidence_purpose='medical_certificate',evidence_min_count=1,
    evidence_due_stage='before_final_approval',evidence_grace_days=2,evidence_note='Attach the medical certificate.'
    WHERE policy_version_id=? AND leave_type_id=?`).bind(policy,type).run();
  const request=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(request.id);

  await expect(assertLeaveAttachmentAccess(db,manager,{leaveRequestId:id,purposeKey:'medical_certificate',operation:'read'}))
    .resolves.toMatchObject({version:1});
  await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),manager.assignmentId).run();
  await expect(assertLeaveAttachmentAccess(db,manager,{leaveRequestId:id,purposeKey:'medical_certificate',operation:'read'}))
    .rejects.toMatchObject({status:403,code:'LEAVE_EVIDENCE_FORBIDDEN'});
  await expect(assertLeaveAttachmentAccess(db,employee,{leaveRequestId:id,purposeKey:'medical_certificate',operation:'read'}))
    .resolves.toMatchObject({version:1});
  await expect(assertLeaveAttachmentAccess(db,hr,{leaveRequestId:id,purposeKey:'medical_certificate',operation:'read'}))
    .resolves.toMatchObject({version:1});
 });
 it('counts distinct physical medical sources instead of duplicate links',async()=>{
  vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
  try {
   await db.prepare(`UPDATE leave_policy_rules SET evidence_min_units=.5,evidence_purpose='medical_certificate',evidence_min_count=2,
     evidence_due_stage='before_final_approval',evidence_grace_days=0,evidence_note='Attach two independent medical documents.'
     WHERE policy_version_id=? AND leave_type_id=?`).bind(policy,type).run();
   const request=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(request.id);
   const first=await stageLeaveEvidence(id,1,'medical_certificate','application/pdf','first-certificate');
   await approveLeave('h001',id,{override:false,expectedVersion:1,note:'Reviewed the submitted documents'},key(),manager);
   const duplicateSet=key(),duplicateLink=key();
   // A later canonical reuse of the same physical source must not satisfy a
   // two-document policy twice.
   await db.batch([
    db.prepare(`INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,
      policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at)
      VALUES(?,?,'leave_request',?,'record','leave-evidence:medical_certificate','medical_certificate',2,1,'restricted','{}',0,10,'draft',?,2,2)`)
      .bind(duplicateSet,company,id,employee.userId),
    db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,2,9999999999999)")
      .bind(duplicateLink,duplicateSet,first.source,employee.userId),
   ]);
   await expect(approveLeave('h001',id,{override:false,expectedVersion:2,note:'Final review'},key(),hr))
     .rejects.toMatchObject({code:'LEAVE_EVIDENCE_REQUIRED'});
   const upload=key(),source=key(),link=key();
   await db.batch([
    db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at) VALUES(?,'leave_request',?,'medical_certificate','certificate-2.pdf','application/pdf',4,4,'second-certificate',?,?,'ready',?,9999999999999,2,2)")
      .bind(upload,id,`staging-${upload}`,`final-${upload}`,employee.userId),
    db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state) VALUES(?,?,'leave_request',?,'medical_certificate',?,1,'certificate-2.pdf','application/pdf',4,'second-certificate',?,2,?,'application/pdf','available')")
      .bind(source,upload,id,`final-${upload}`,employee.userId,company),
    db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,3,9999999999999)")
      .bind(link,duplicateSet,source,employee.userId),
   ]);
   await approveLeave('h001',id,{override:false,expectedVersion:2,note:'Final review'},key(),hr);
   expect((await getLeave('h001',id,employee)).state).toBe('approved');
  } finally { vi.unstubAllEnvs(); }
 });
 it('catches up missed accrual periods once and isolates balances by year',async()=>{
  await db.prepare("UPDATE leave_policy_rules SET grant_kind='accrual',accrual_units=1,accrual_period='monthly' WHERE policy_version_id=?").bind(policy).run();
  expect(await runLeaveAccruals(db,'2026-09-17')).toBe(9);expect(await runLeaveAccruals(db,'2026-09-18')).toBe(0);
  const p=await previewLeave(employee,{...leaveInput(),startDate:'2027-01-04',endDate:'2027-01-04'});expect(p.accounts[0].available).toBe(0);expect(p.canSubmit).toBe(false);
 });
 it('reviews retrospective overnight actuals and creates exactly one immutable payroll handoff',async()=>{
  const id=String((await createOvertime(employee,actual(),key())).id);await otAction(employee,id,'submit');
  await expect(otAction(hr,id,'approve')).rejects.toMatchObject({status:403});
  await otAction(manager,id,'verify');const current=await getOvertime(hr,id),k=key(),input={override:false,attachmentSetIds:[],action:'approve' as const,expectedVersion:current.version,note:'Checked'};
  await commandOvertime(hr,id,input,k);await commandOvertime(hr,id,input,k);
  const row=await getOvertime(employee,id);expect(row.minutes).toBe(210);expect(row.state).toBe('approved');expect(row.permittedActions).toEqual([]);
  expect(await db.prepare('SELECT count(*)::int n FROM overtime_payroll_handoffs WHERE request_id=?').bind(id).first()).toEqual({n:1});
  expect((await enrichFlowWork(employee,await overtimeWork(employee))).find(r=>r.id===id)).toMatchObject({status:'approved',queue:'completed',href:`/overtime/${id}`});
  expect((await listStaffNotifications(db,employee)).filter(n=>n.record_id===id).every(n=>n.href===`/overtime/${id}`)).toBe(true);
  const payroll=await person('Payroll','payroll_preparer'),group=`paygroup-${company}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES(?,?,'Test payroll','TH','THB','monthly','Asia/Bangkok','fixture',1,1)").bind(group,company).run();
  await db.prepare('UPDATE hr_engagements SET pay_group_id=? WHERE id=?').bind(group,row.engagement_id).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-01-01','monthly',1000000,6250,160,'THB','[]','approved',?,'fixture',1,1)").bind(`comp-${company}`,row.engagement_id,hr.userId).run();
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
  await approveLeave('h001',id,{override:false,expectedVersion:1,note:''},key(),manager);await approveLeave('h001',id,{override:false,expectedVersion:2,note:''},key(),hr);
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

afterEach(()=>{workforceDatabase=undefined;vi.unstubAllEnvs();});
async function workforcePurchase(state:"active"|"draining"="active"){const {packageGrantStore,readCompanyPackageGrants}=await import("@erpx/server-commercial/public/package-grants"),store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-workforce-source-provider"}),obligations:async()=>{throw Error("No workforce inactive witness is invented before all owner readers are registered");}}),grants=await readCompanyPackageGrants(db,company);for(const packageId of ["people","people-workforce"]){const prior=grants.find(row=>row.packageId===packageId);if(packageId==="people"&&prior)continue;await store.apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId,packageVersion:1,state:packageId==="people"?"active":state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:"Actual Workforce self-service and retained owner source workflow",evidence:"test:workforce-time-leave-owner"}});}}
it("enforces actual Workforce Time Leave and Overtime for employee self-service without a full People module and settles existing duties while draining",async()=>{
 const {createTimeEntry,approveTimeEntry,timeEntrySchema}=await import("@erpx/server-people_payroll/public/people-payroll"),time=timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:"2026-09-25",regularHours:8,note:"Original employee time source"}),timeKey=key(),ot=overtimeDetailsSchema.parse({mode:"advance",startLocal:"2026-10-06T18:00",endLocal:"2026-10-06T20:00",timezone:"Asia/Bangkok",reason:"Original planned customer delivery"});vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");expect(employee.modules).toEqual([]);await expect(leaveSummary(employee)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(createSelfServiceLeave(employee,leaveInput(),key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(createTimeEntry(employee,time,timeKey)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(createOvertime(employee,ot,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await workforcePurchase();const logged=await createTimeEntry(employee,time,timeKey),leave=await createSelfServiceLeave(employee,leaveInput(),key()),overtime=await createOvertime(employee,ot,key());await workforcePurchase("draining");await expect(createTimeEntry(employee,{...time,workDate:"2026-09-26"},key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});await expect(editSelfServiceLeave(employee,String(leave.id),{...leaveInput(),expectedVersion:1},key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});await expect(createOvertime(employee,ot,key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});await approveTimeEntry(manager,String(logged.id),1,key());await transitionLeave(employee,String(leave.id),{action:"cancel",expectedVersion:1,note:"Cancel the existing original leave reservation",override:false},key());await commandOvertime(employee,String(overtime.id),{action:"withdraw",expectedVersion:1,note:"Withdraw original unsubmitted overtime",override:false,attachmentSetIds:[]},key());expect(await createTimeEntry(employee,time,timeKey)).toEqual(logged);expect((await getLeave("h001",String(leave.id),employee)).state).toBe("cancelled");expect((await getOvertime(employee,String(overtime.id))).state).toBe("withdrawn");expect(await db.prepare("SELECT status FROM hr_time_entries WHERE id=?").bind(logged.id).first("status")).toBe("approved");expect((await leaveSummary(employee)).history.map(row=>row.id)).toContain(leave.id);await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(employee.assignmentId).run();await expect(createTimeEntry(employee,time,timeKey)).rejects.toMatchObject({code:"PEOPLE_ACCESS_CHANGED"});await expect(leaveSummary(employee)).rejects.toMatchObject({code:"PEOPLE_ACCESS_CHANGED"});
});
it("rolls actual Time source and selected IAM late effects back before receipt commit and retries the exact employee command",async()=>{
 const {createTimeEntry,timeEntrySchema}=await import("@erpx/server-people_payroll/public/people-payroll");await workforcePurchase();vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");for(const fault of ["source","authority"] as const){const command=key(),id=`time-${command}`,input=timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:fault==="source"?"2026-09-22":"2026-09-23",regularHours:8,note:"Original source must remain current"}),name=`workforce_final_${key().replaceAll('-','')}`,tables=["hr_engagements","hr_time_entries","assignments","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot(),effect=fault==="source"?`UPDATE hr_engagements SET status='suspended' WHERE id='e-${employee.userId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${employee.assignmentId}'`;await db.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`);try{await expect(createTimeEntry(employee,input,command)).rejects.toMatchObject(fault==="source"?{code:"WORKFORCE_SOURCE_CHANGED"}:{code:"PEOPLE_ACCESS_CHANGED"});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${name} ON command_receipts;DROP FUNCTION ${name}()`);}const saved=await createTimeEntry(employee,input,command);expect(await createTimeEntry(employee,input,command)).toEqual(saved);expect(await db.prepare("SELECT count(*)::int count FROM command_receipts WHERE command_id=?").bind(command).first("count")).toBe(1);expect(await db.prepare("SELECT company_id,submitted_by,status FROM hr_time_entries WHERE id=?").bind(id).first()).toEqual({company_id:company,submitted_by:employee.userId,status:"submitted"});}
});
it("checks the actual Company purchase for scheduled Leave accrual while keeping approved entitlements and original accrual keys intact",async()=>{
 vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");const ledger=()=>db.prepare("SELECT count(*)::int count FROM leave_balance_entries WHERE company_id=?").bind(company).first<number>("count"),before=await ledger();expect(await runLeaveAccruals(db,"2027-01-02",company)).toBe(0);expect(await ledger()).toBe(before);await workforcePurchase();expect(await runLeaveAccruals(db,"2027-01-02",company)).toBe(1);expect(await runLeaveAccruals(db,"2027-01-02",company)).toBe(0);await workforcePurchase("draining");expect(await runLeaveAccruals(db,"2028-01-02",company)).toBe(1);expect(await db.prepare("SELECT sum(units)::int units FROM leave_balance_entries WHERE company_id=? AND kind='grant'").bind(company).first("units")).toBe(30);expect(await db.prepare("SELECT status FROM hr_engagements WHERE id=?").bind(`e-${employee.userId}`).first("status")).toBe("active");
});

it("rechecks the original due Leave policy after deferred accrual effects and rolls both entitlement and source changes back",async()=>{
 await workforcePurchase();vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");const name=`workforce_accrual_${key().replaceAll('-','')}`,tables=["leave_policy_rules","leave_balance_entries"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();await db.exec(`CREATE SEQUENCE ${name}_attempt;CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}_attempt');UPDATE leave_policy_rules SET grant_units=grant_units+1 WHERE policy_version_id='${policy}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON leave_balance_entries DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.company_id='${company}' AND NEW.effective_date='2027-01-01') EXECUTE FUNCTION ${name}()`);try{await expect(runLeaveAccruals(db,"2027-01-02",company)).rejects.toMatchObject({code:"WORKFORCE_SOURCE_CHANGED"});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${name}_attempt`).first("is_called")).toBe(true);}finally{await db.exec(`DROP TRIGGER ${name} ON leave_balance_entries;DROP FUNCTION ${name}();DROP SEQUENCE ${name}_attempt`);}expect(await runLeaveAccruals(db,"2027-01-02",company)).toBe(1);expect(await runLeaveAccruals(db,"2027-01-02",company)).toBe(0);
});
it("keeps a normal Company Leave policy command from accruing another Company's approved entitlements",async()=>{
 const foreign=`foreign-accrual-${key()}`,personId=`p-${foreign}`,engagementId=`e-${foreign}`,foreignType=`type-${foreign}`,foreignPolicy=`policy-${foreign}`;await db.batch([db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Independent Leave Company',?)").bind(foreign,foreign,foreign),db.prepare("INSERT INTO hr_people(id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,'Independent worker','TH','fixture',1,1)").bind(personId,foreign),db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,'employee','salaried','active','2026-01-01','Worker','THB','fixture',1,1)").bind(engagementId,personId,foreign),db.prepare("INSERT INTO leave_types(id,company_id,code,name,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?,'annual','Independent leave',true,'2026-01-01','fixture',1,1)").bind(foreignType,foreign),db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,created_by,created_at) VALUES(?,?,'annual',1,'Independent entitlement','active','2026-01-01','Asia/Bangkok','fixture',1)").bind(foreignPolicy,foreign),db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units) VALUES(?,?,'annual',12)").bind(foreignPolicy,foreignType),db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,'2026-01-01','fixture',1)").bind(`assignment-${foreign}`,foreign,engagementId,foreignPolicy)]);const snapshot=()=>db.prepare("SELECT row_to_json(entry)::text row FROM leave_balance_entries entry WHERE entry.company_id=? ORDER BY entry.id").bind(foreign).all(),before=(await snapshot()).results;await workforcePurchase();vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");await administerLeave(hr,leaveAdminSchema.parse({resource:"type",code:`source-${key().slice(0,20)}`,name:"Own Company supplemental policy type",tracked:false}),key());expect((await snapshot()).results).toEqual(before);expect(await runLeaveAccruals(db,"2026-10-01",foreign)).toBe(0);expect((await snapshot()).results).toEqual(before);
});

it("filters actual Leave calendar and Work sources before shared list limits and keeps retained history scoped to the selected current employee",async()=>{
 const leave=await createSelfServiceLeave(employee,leaveInput(),key()),{getWork}=await import("@erpx/server-work_projection/public/work"),{workReadPeoplePayrollFacts}=await import("@erpx/server-people_payroll/public/work-read-facts"),calendar=()=>leaveItems(db,calendarContext,calendarQuerySchema.parse({date:"2026-10-05",mode:"mine"}),"2026-10-01","2026-11-01",1),calendarContext=await calendarAccess(db,employee);vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");expect((await getWork("h001",employee)).rows).toEqual([]);expect(await calendar()).toEqual([]);await workforcePurchase();expect((await getWork("h001",employee)).rows.map(row=>row.id)).toContain(leave.id);expect((await calendar()).map(row=>row.sourceId)).toContain(leave.id);await workforcePurchase("draining");expect((await getLeave("h001",String(leave.id),employee)).purchase).toMatchObject({newWork:false,settleExisting:true,history:true});expect((await leaveSummary(employee)).purchase).toMatchObject({newWork:false,settleExisting:true,history:true});expect((await getWork("h001",employee)).rows.map(row=>row.id)).toContain(leave.id);await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(employee.assignmentId).run();await expect(getLeave("h001",String(leave.id),employee)).rejects.toMatchObject({code:"PEOPLE_ACCESS_CHANGED"});expect((await workReadPeoplePayrollFacts(db).pendingLeave({companyId:`foreign-${company}`})).results).toEqual([]);
});

it("acquires actual Workforce source leases before its first snapshot and reads an original concurrently committed Time command",async()=>{
 const {withWorkforceAccrualTransaction}=await import("@erpx/server-people_payroll/public/workforce-purchases"),{createTimeEntry,timeEntrySchema}=await import("@erpx/server-people_payroll/public/people-payroll");await workforcePurchase();const command=key(),input=timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:"2026-09-21",regularHours:8,note:"Actual original Time source before the waiting scheduled snapshot"});let ready:()=>void=()=>{},failed:(cause:unknown)=>void=()=>{},release:()=>void=()=>{};const sourceReady=new Promise<void>((resolve,reject)=>{ready=resolve;failed=reject;}),finish=new Promise<void>(resolve=>{release=resolve;});const writer=db.transaction(async tx=>{workforceDatabase=tx;try{await createTimeEntry(employee,input,command);}finally{workforceDatabase=undefined;}ready();await finish;}).catch(cause=>{failed(cause);throw cause;});try{await sourceReady;vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");let complete=false;const reader=withWorkforceAccrualTransaction(db,async(tx,permits)=>{expect(await permits(company)).toBe(true);return tx.prepare("SELECT count(*)::int count FROM hr_time_entries WHERE id=? AND company_id=?").bind(`time-${command}`,company).first<number>("count");}).finally(()=>{complete=true;});await new Promise(resolve=>setTimeout(resolve,100));expect(complete).toBe(false);release();await writer;expect(await reader).toBe(1);}finally{release();workforceDatabase=undefined;await writer.catch(()=>{});}expect(await db.prepare("SELECT count(*)::int count FROM command_receipts WHERE command_id=? AND operation='people.time.submit'").bind(command).first("count")).toBe(1);
});

const workforceSettlementInput=()=>({companyId:company,packageId:'people-workforce',packageVersion:1});
async function workforceSettlement(now=Date.now()){
 const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');
 return withOwnerPackageObligations(db,workforceSettlementInput(),tx=>readOwnerPackageObligations(db,tx,{...workforceSettlementInput(),now}));
}
async function workforceWithdrawal(){
 const {readCompanyPackageGrants,packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,company),current=grants.find(row=>row.packageId==='people-workforce')!;
 return {companyId:company,expectedRevision:packageGrantRevision(company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual Workforce source duties settled with original employment retained',evidence:'test:people-workforce-owner-settlement'}}]};
}
async function workforceWithdrawalReview(request:Awaited<ReturnType<typeof workforceWithdrawal>>,apply=false){
 const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-workforce-lifecycle-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});
 return withOwnerPackageObligations(db,workforceSettlementInput(),tx=>apply?store.batch.applyIn(tx,request,async()=>{}):store.batch.previewIn(tx,request));
}
async function endWorkforceAccrual(){return administerLeave(hr,leaveAdminSchema.parse({resource:'assignment_end',id:`assignment-${company}`,expectedVersion:1,effectiveTo:'2026-01-01'}),key());}
it('withdraws actual reviewed Workforce source duties through normal Commercial Preview Apply without terminating existing employment',async()=>{
 const {createTimeEntry,approveTimeEntry,timeEntrySchema}=await import('@erpx/server-people_payroll/public/people-payroll');
 await workforcePurchase();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const timeKey=key(),timeInput=timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:'2026-09-29',regularHours:8,note:'Actual employee Time retained after software withdrawal'}),time=await createTimeEntry(employee,timeInput,timeKey),leave=await createSelfServiceLeave(employee,leaveInput(),key()),ot=await createOvertime(employee,actual(),key());
 await workforcePurchase('draining');expect((await workforceSettlement()).total).toBeGreaterThan(3);await expect(workforceWithdrawalReview(await workforceWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await approveTimeEntry(manager,String(time.id),1,key());await transitionLeave(employee,String(leave.id),{action:'cancel',expectedVersion:1,note:'Cancel original reserved absence only',override:false},key());await commandOvertime(employee,String(ot.id),{action:'withdraw',expectedVersion:1,note:'Withdraw original unsubmitted hours',override:false,attachmentSetIds:[]},key());await endWorkforceAccrual();
 const before=await db.prepare('SELECT id,status,version FROM hr_engagements WHERE company_id=? ORDER BY id').bind(company).all();expect((await workforceSettlement()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['benefits',0],['finance',0]]);
 const request=await workforceWithdrawal(),preview=await workforceWithdrawalReview(request),applied=await workforceWithdrawalReview(request,true);expect(applied).toEqual(preview);expect(await workforceWithdrawalReview(request,true)).toEqual(applied);expect((await db.prepare('SELECT id,status,version FROM hr_engagements WHERE company_id=? ORDER BY id').bind(company).all()).results).toEqual(before.results);
 expect(await createTimeEntry(employee,timeInput,timeKey)).toEqual(time);expect((await getLeave('h001',String(leave.id),employee)).purchase).toMatchObject({newWork:false,settleExisting:false,history:true});expect((await getOvertime(employee,String(ot.id))).state).toBe('withdrawn');await expect(createTimeEntry(employee,{...timeInput,workDate:'2026-09-30'},key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(employee.assignmentId).run();await expect(createTimeEntry(employee,timeInput,timeKey)).rejects.toMatchObject({code:'PEOPLE_ACCESS_CHANGED'});
});
it('keeps future approved Leave pending through its actual policy date while retaining naturally ended approved absence and delivered overtime',async()=>{
 await workforcePurchase();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const leave=await createSelfServiceLeave(employee,{...leaveInput(),startDate:'2026-12-07',endDate:'2026-12-07'},key());await approveLeave('h001',String(leave.id),{override:false,expectedVersion:1,note:'Original manager review'},key(),manager);await approveLeave('h001',String(leave.id),{override:false,expectedVersion:2,note:'Original HR review'},key(),hr);await endWorkforceAccrual();
 expect((await workforceSettlement(Date.UTC(2026,11,7))).total).toBe(1);expect((await workforceSettlement(Date.UTC(2026,11,8))).total).toBe(0);expect(await db.prepare('SELECT state FROM leave_requests WHERE id=?').bind(leave.id).first('state')).toBe('approved');
 const created=await createOvertime(employee,actual(),key()),id=String(created.id);await commandOvertime(employee,id,{action:'submit',expectedVersion:1,note:'Original actual hours',override:false,attachmentSetIds:[]},key());await commandOvertime(manager,id,{action:'verify',expectedVersion:2,note:'Independent actual work reviewed',override:false,attachmentSetIds:[]},key());await commandOvertime(hr,id,{action:'approve',expectedVersion:3,note:'Original HR immutable Payroll handoff',override:false,attachmentSetIds:[]},key());
 expect((await workforceSettlement(Date.UTC(2026,11,8))).total).toBe(0);expect(await db.prepare('SELECT count(*)::int count FROM overtime_payroll_handoffs WHERE request_id=?').bind(id).first('count')).toBe(1);expect(await db.prepare('SELECT count(*)::int count FROM hr_payroll_runs WHERE company_id=?').bind(company).first('count')).toBe(0);
});
it('rejects unregistered Workforce snapshots, holds actual raw owner sources before reads and observes unrelated Benefits without blocking the add-on',async()=>{
 const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{peoplePackageObligations}=await import('@erpx/server-people_payroll/public/package-obligations');await workforcePurchase();await endWorkforceAccrual();const input={...workforceSettlementInput(),moduleId:'people_payroll',now:Date.now()};await expect(db.transaction(tx=>peoplePackageObligations(db,tx,input))).rejects.toThrow('PEOPLE_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...workforceSettlementInput(),packageVersion:2},tx=>peoplePackageObligations(db,tx,{...input,packageVersion:2}))).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');
 await withOwnerPackageObligations(db,workforceSettlementInput(),async tx=>{await expect(peoplePackageObligations(db,tx,{...input,companyId:`foreign-${company}`})).rejects.toThrow('PEOPLE_PACKAGE_OBLIGATION_SOURCE_REQUIRED');await expect(peoplePackageObligations(db,tx,{...input,moduleId:'finance'})).rejects.toThrow('PEOPLE_PACKAGE_OBLIGATION_SCOPE_REQUIRED');for(const table of ['hr_people','hr_engagements','people_lifecycle_cases','people_preparation_tasks','people_lifecycle_grants','iam_reviews','people_roster_shifts','people_roster_requests','hr_time_entries','leave_requests','leave_balance_entries','leave_policy_assignments','overtime_requests','overtime_payroll_handoffs','benefit_elections','benefit_payroll_inputs','hr_payroll_runs','hr_payment_batches','finance_journals','finance_journal_lines','money_transaction_snapshots'])await expect(db.transaction(async competing=>{await competing.prepare("SET LOCAL lock_timeout='50ms'").run();await competing.prepare(`LOCK TABLE ${table} IN ROW EXCLUSIVE MODE`).run();})).rejects.toMatchObject({code:'55P03'});expect((await readOwnerPackageObligations(db,tx,{...workforceSettlementInput(),now:input.now})).total).toBe(0);});
 const before=await workforceSettlement(),plan=`benefit-${key()}`;await db.prepare("INSERT INTO benefit_plans(id,company_id,name,provider,coverage_type,currency,employee_deduction_minor,employer_cost_minor,effective_from,created_by,created_at,updated_at) VALUES(?,?,'Independent pending Benefits plan','Original provider','employee','THB',100,100,'2026-01-01','fixture',1,1)").bind(plan,company).run();const after=await workforceSettlement();expect(after.total).toBe(0);expect(after.observations.find(row=>row.moduleId==='benefits')?.revision).not.toBe(before.observations.find(row=>row.moduleId==='benefits')?.revision);const imported=`legacy-approved-${key()}`;await db.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,status,submitted_by,approved_by,created_at,updated_at) VALUES(?,?,?,'2026-09-26',8,'approved',?,?,1,1)").bind(imported,`e-${employee.userId}`,company,employee.userId,manager.userId).run();expect((await workforceSettlement()).total).toBe(1);
});
it('rolls normal Workforce inactivation back with a late deferred Time source and reads concurrently committed original work before its first settlement snapshot',async()=>{
 const {createTimeEntry,timeEntrySchema}=await import('@erpx/server-people_payroll/public/people-payroll');await workforcePurchase();await endWorkforceAccrual();await workforcePurchase('draining');const request=await workforceWithdrawal(),name=`workforce_settlement_${key().replaceAll('-','')}`,id=`late-${key()}`;await db.exec(`CREATE SEQUENCE ${name}_attempt;CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}_attempt');INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,status,created_at,updated_at) VALUES('${id}','e-${employee.userId}','${company}','2026-09-28','submitted',1,1);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.company_id='${company}') EXECUTE FUNCTION ${name}()`);try{await expect(workforceWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await db.prepare(`SELECT is_called FROM ${name}_attempt`).first('is_called')).toBe(true);expect(await db.prepare('SELECT id FROM hr_time_entries WHERE id=?').bind(id).first()).toBeNull();expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='people-workforce'").bind(company).first()).toEqual({state:'draining',version:2});expect(await db.prepare('SELECT id FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${name} ON company_package_change_sets;DROP FUNCTION ${name}();DROP SEQUENCE ${name}_attempt`);}
 let ready:()=>void=()=>{},failed:(cause:unknown)=>void=()=>{},release:()=>void=()=>{};const sourceReady=new Promise<void>((resolve,reject)=>{ready=resolve;failed=reject;}),finish=new Promise<void>(resolve=>{release=resolve;});const command=key(),writer=db.transaction(async tx=>{workforceDatabase=tx;try{await createTimeEntry(employee,timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:'2026-09-27',regularHours:8,note:'Original command before waiting withdrawal snapshot'}),command);}finally{workforceDatabase=undefined;}ready();await finish;}).catch(cause=>{failed(cause);throw cause;});try{await sourceReady;let complete=false;const reader=workforceSettlement().finally(()=>{complete=true;});await new Promise(resolve=>setTimeout(resolve,100));expect(complete).toBe(false);release();await writer;expect((await reader).total).toBe(1);}finally{release();workforceDatabase=undefined;await writer.catch(()=>{});}
});

it('keeps actual missed Leave accruals pending after a legacy assignment end until the original scheduled owner delivers its immutable entitlement keys',async()=>{
 await workforcePurchase();await endWorkforceAccrual();const legacyPolicy=`missed-policy-${key()}`,assignment=`missed-assignment-${key()}`;await db.batch([db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,created_by,created_at) VALUES(?,?,?,1,'Actual closed legacy accrual','active','2026-01-01','Asia/Bangkok','legacy',1)").bind(legacyPolicy,company,legacyPolicy),db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,accrual_units,accrual_period) VALUES(?,?,'accrual',1,'monthly')").bind(legacyPolicy,type),db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,effective_to,created_by,created_at) VALUES(?,?,?,?,'2026-01-01','2026-02-01','legacy',1)").bind(assignment,company,`e-${other.userId}`,legacyPolicy)]);await workforcePurchase('draining');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect((await workforceSettlement()).total).toBe(2);await expect(workforceWithdrawalReview(await workforceWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await runLeaveAccruals(db,'2026-10-01',company)).toBe(2);expect((await workforceSettlement()).total).toBe(0);expect(await runLeaveAccruals(db,'2026-10-01',company)).toBe(0);expect((await db.prepare("SELECT command_key,units FROM leave_balance_entries WHERE company_id=? AND engagement_id=? ORDER BY effective_date").bind(company,`e-${other.userId}`).all()).results).toEqual([{command_key:`leave:e-${other.userId}:accrual:2026-01-01`,units:1},{command_key:`leave:e-${other.userId}:accrual:2026-02-01`,units:1}]);
});

it('requires the actual Workforce purchase for registered Time Leave and Overtime private evidence while preserving employee self-service and source-specific draft versus settlement rules',async()=>{
 const {createTimeEntry,timeEntrySchema}=await import('@erpx/server-people_payroll/public/people-payroll'),{seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),{createAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');const time=await createTimeEntry(employee,timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:'2026-09-24',regularHours:8,note:'Actual evidence source'}),key()),leave=await createSelfServiceLeave(employee,leaveInput(),key()),ot=await createOvertime(employee,actual(),key()),targets=[{targetType:'time_entry',targetId:String(time.id),purposeKey:'corrected_entry_proof',componentKind:'action' as const},{targetType:'leave_request',targetId:String(leave.id),purposeKey:'medical_certificate',componentKind:'record' as const},{targetType:'overtime_request',targetId:String(ot.id),purposeKey:'actual_evidence',componentKind:'action' as const}],documents=[];for(const target of targets){const file=await seedAttachmentSet(db,employee,{...target,expectedTargetVersion:1}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes=`Private actual ${target.targetType} evidence`;await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();documents.push({file,bytes});}
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(employee.modules).toEqual([]);for(const [index,target] of targets.entries()){await expect(attachmentContent(db,env.H001_FILES,employee,documents[index].file.linkId,'original')).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(createAttachmentSet(db,employee,{...target,componentRef:'Actual source evidence',expectedTargetVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});}await workforcePurchase();for(const document of documents)expect(await(await attachmentContent(db,env.H001_FILES,employee,document.file.linkId,'original')).object.text()).toBe(document.bytes);await workforcePurchase('draining');for(const target of targets.slice(0,2))expect(await createAttachmentSet(db,employee,{...target,componentRef:'Existing submitted source evidence',expectedTargetVersion:1},key())).toMatchObject({targetType:target.targetType,targetId:target.targetId});await expect(createAttachmentSet(db,employee,{...targets[2],componentRef:'Unsubmitted new source evidence',expectedTargetVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});await expect(attachmentContent(db,env.H001_FILES,other,documents[0].file.linkId,'original')).rejects.toBeInstanceOf(Error);
});
it('rolls new Workforce file sets and their deferred actual source or selected-role changes back together before exact retry',async()=>{
 const {createTimeEntry,timeEntrySchema}=await import('@erpx/server-people_payroll/public/people-payroll'),{createAttachmentSet}=await import('@erpx/server-files/public/attachments/service');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');await workforcePurchase();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const time=await createTimeEntry(employee,timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:'2026-09-24',regularHours:8,note:'Actual owner before deferred File source test'}),key());for(const fault of ['source','authority'] as const){const command=key(),fn=`workforce_files_${key().replaceAll('-','')}`,input={targetType:'time_entry',targetId:String(time.id),componentKind:'action' as const,componentRef:`Actual ${fault} evidence source`,purposeKey:'corrected_entry_proof',expectedTargetVersion:1},tables=['attachment_sets','attachment_events','hr_engagements','assignments','command_receipts','audit_events','outbox_events'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(row)::text image FROM ${table} row ORDER BY row_to_json(row)::text`).all()).results)),before=await snapshot(),effect=fault==='source'?`UPDATE hr_engagements SET status='suspended' WHERE id='e-${employee.userId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${employee.assignmentId}'`;await db.exec(`CREATE SEQUENCE ${fn}_attempt;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON attachment_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.company_id='${company}' AND NEW.target_id='${time.id}') EXECUTE FUNCTION ${fn}()`);try{await expect(createAttachmentSet(db,employee,input,command)).rejects.toMatchObject({code:'WORKFORCE_ATTACHMENT_CHANGED'});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_attempt`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON attachment_sets;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_attempt`);}const saved=await createAttachmentSet(db,employee,input,command);expect(await createAttachmentSet(db,employee,input,command)).toEqual(saved);expect(await db.prepare('SELECT count(*)::int count FROM attachment_sets WHERE id=?').bind(saved.id).first('count')).toBe(1);}
});
it('retains actual bound Leave certificate bytes and Legal hold after normal Workforce inactivation while preserving current reviewer and employee authority',async()=>{
 const {attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{seedAttachmentSet}=await import('../../support/helpers/attachment-v2');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');await workforcePurchase();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const leave=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(leave.id),file=await seedAttachmentSet(db,employee,{targetType:'leave_request',targetId:id,componentKind:'record',componentRef:'leave-evidence:medical_certificate',purposeKey:'medical_certificate',expectedTargetVersion:1}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),bytes='%PDF-1.7\nActual original approved medical certificate';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();await workforcePurchase('draining');await approveLeave('h001',id,{override:false,expectedVersion:1,note:'Original independent department evidence review'},key(),manager);await approveLeave('h001',id,{override:false,expectedVersion:2,note:'Original independent HR evidence review'},key(),hr);await endWorkforceAccrual();expect((await workforceSettlement()).total).toBe(0);expect(await db.prepare('SELECT state FROM attachment_links WHERE id=?').bind(file.linkId).first('state')).toBe('bound');await workforceWithdrawalReview(await workforceWithdrawal(),true);for(const actor of [employee,manager,hr])expect(await(await attachmentContent(db,env.H001_FILES,actor,file.linkId,'original')).object.text()).toBe(bytes);await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(employee.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,employee,file.linkId,'original')).rejects.toBeInstanceOf(Error);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect(await(await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);
});

it('binds actual Leave evidence in two Companies without hidden source-lease upgrade deadlocks or crossed sensitive receipts',async()=>{
 const {seedAttachmentSet,attachmentBindingState}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');const first=await createSelfServiceLeave(employee,leaveInput(),key()),foreign=`separate-files-${key()}`,department=`department-${foreign}`,foreignType=`type-${foreign}`,foreignPolicy=`policy-${foreign}`;await db.batch([db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Separate actual evidence Company',?)").bind(foreign,foreign,foreign),db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id) VALUES(?,'department',?,'Separate source department',?,?)").bind(department,department,foreign,foreign)]);
 const member=async(name:string,role:string):Promise<WorkContext>=>{const userId=`${foreign}-${name}`,personId=`p-${userId}`,assignmentId=`a-${userId}`;await db.batch([db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,'[]',?,0,9999999999999,'holding')").bind(assignmentId,userId,role,foreign),db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(personId,userId,foreign,name),db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,'employee','salaried','active','2026-01-01','Staff','THB','fixture',1,1)").bind(`e-${userId}`,personId,foreign)]);return {...employee,userId,assignmentId,role,companyId:foreign,name,email:`${userId}@example.test`,scope:{...employee.scope,companyId:foreign}};},requester=await member('Employee','employee'),reviewer=await member('Manager','people_manager');await member('HR','hr_admin');await db.batch([db.prepare('UPDATE hr_engagements SET manager_person_id=?,department_id=? WHERE id=?').bind(`p-${reviewer.userId}`,department,`e-${requester.userId}`),db.prepare('INSERT INTO department_heads(department_id,company_id,person_id,updated_by,updated_at) VALUES(?,?,?,?,1)').bind(department,foreign,`p-${reviewer.userId}`,'fixture'),db.prepare("INSERT INTO leave_types(id,company_id,code,name,tracked,effective_from,created_by,created_at,updated_at) VALUES(?,?,'annual','Actual separate Company leave',true,'2026-01-01','fixture',1,1)").bind(foreignType,foreign),db.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,created_by,created_at) VALUES(?,?,'annual',1,'Actual separate entitlement','active','2026-01-01','Asia/Bangkok','fixture',1)").bind(foreignPolicy,foreign),db.prepare("INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units) VALUES(?,?,'annual',10)").bind(foreignPolicy,foreignType),db.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES(?,?,?,?,'2026-01-01','fixture',1)").bind(`assignment-${foreign}`,foreign,`e-${requester.userId}`,foreignPolicy)]);await runLeaveAccruals(db,'2026-09-17',foreign);const second=await createSelfServiceLeave(requester,{...leaveInput(),leaveTypeId:foreignType},key()),firstFile=await seedAttachmentSet(db,employee,{targetType:'leave_request',targetId:String(first.id),componentKind:'record',purposeKey:'medical_certificate',expectedTargetVersion:1}),secondFile=await seedAttachmentSet(db,requester,{targetType:'leave_request',targetId:String(second.id),componentKind:'record',purposeKey:'medical_certificate',expectedTargetVersion:1});await workforcePurchase();const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-separate-workforce-provider'}),obligations:async()=>{throw Error('No unrelated withdrawal witness');}});for(const packageId of ['people','people-workforce'])await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:foreign,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual independent Company evidence purchase',evidence:'test:separate-workforce-evidence'}});vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const fn=`workforce_parallel_files_${key().replaceAll('-','')}`,firstCommand=key(),secondCommand=key();await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);RETURN NEW;END $$;CREATE TRIGGER ${fn} BEFORE UPDATE ON leave_requests FOR EACH ROW WHEN(NEW.evidence_version>OLD.evidence_version AND NEW.company_id IN ('${company}','${foreign}')) EXECUTE FUNCTION ${fn}()`);try{const measured=await measureReads(()=>Promise.all([bindRecordAttachmentSet(db,manager,firstFile.setId,1,firstCommand),bindRecordAttachmentSet(db,reviewer,secondFile.setId,1,secondCommand)]));expect(measured.result).toHaveLength(2);expect(measured.metrics.deadlockFailures??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fn} ON leave_requests;DROP FUNCTION ${fn}()`);}for(const [actor,leave,file,command] of [[manager,first,firstFile,firstCommand],[reviewer,second,secondFile,secondCommand]] as const){expect(await db.prepare('SELECT company_id,version,evidence_version FROM leave_requests WHERE id=?').bind(leave.id).first()).toEqual({company_id:actor.companyId,version:2,evidence_version:1});expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'record_document'});expect(await db.prepare("SELECT count(*)::int count FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command,actor.userId,actor.companyId).first('count')).toBe(1);}
});

async function workforceGenericRecord(id:string,title:string,at:number){await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,version,created_by,updated_by,created_at,updated_at) VALUES('people_payroll',?,'payroll-summaries',?,'','draft','{}',?,1,?,?,?,?)").bind(id,title,company,employee.userId,employee.userId,at,at).run();}
it('Workforce projections filter actual private Time Leave and Overtime Records before page/search limits without gating included generic references',async()=>{
 const {createTimeEntry,timeEntrySchema}=await import('@erpx/server-people_payroll/public/people-payroll'),{recordsPage,searchReferences,getRecord}=await import('@erpx/server-records/public/records');await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
 const leave=await createSelfServiceLeave(employee,leaveInput(),key()),ot=await createOvertime(employee,actual(),key()),time=await createTimeEntry(employee,timeEntrySchema.parse({engagementId:`e-${employee.userId}`,workDate:'2026-09-24',regularHours:8,note:'Actual employee time'}),key()),own=[String(leave.id),String(ot.id),String(time.id)],included=`included-worker-${key()}`,now=Date.now();
 await workforceGenericRecord(included,'Workforce projection included reference',now);for(const id of own)await workforceGenericRecord(id,'Workforce projection own private source',now+1);
 for(let index=0;index<16;index++){const privateTime=await createTimeEntry(other,timeEntrySchema.parse({engagementId:`e-${other.userId}`,workDate:`2026-09-${String(index+1).padStart(2,'0')}`,regularHours:8,note:'Another employee private time'}),key());await workforceGenericRecord(String(privateTime.id),'Workforce projection other private source',now+index+2);}
 // Generic Records retain their original module entry requirement; the normal
 // employee self-service actor below still has modules [].
 await db.prepare("UPDATE assignments SET modules_json='[\"people_payroll\"]' WHERE id=?").bind(employee.assignmentId).run();const recordActor={...employee,modules:['people_payroll']};vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 expect((await recordsPage(recordActor,'people_payroll','payroll-summaries',{q:'Workforce projection'})).items.map(row=>row.id)).toEqual([included]);
 await expect(getRecord(recordActor,'people_payroll',own[0],db)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await workforcePurchase();
 expect((await recordsPage(recordActor,'people_payroll','payroll-summaries',{q:'Workforce projection'})).items.map(row=>row.id).sort()).toEqual([included,...own].sort());const search=await searchReferences(recordActor,'Workforce projection');expect(search.filter(row=>row.kind==='record')).toHaveLength(4);for(const id of [included,...own])expect(search).toContainEqual(expect.objectContaining({kind:'record',id}));
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(employee.assignmentId).run();expect((await recordsPage(recordActor,'people_payroll','payroll-summaries',{q:'Workforce projection'})).items.map(row=>row.id)).toEqual([included]);await expect(getRecord(recordActor,'people_payroll',own[0],db)).rejects.toMatchObject({code:'WORKFORCE_SOURCE_NOT_FOUND'});
});
it('Workforce projections hide 105 invalid notices before the valid original modules-empty employee source and never resurrect private work after Flow',async()=>{
 const {publishInboxEvent,inboxPage}=await import('@erpx/server-collaboration/public/inbox'),{notificationsPage}=await import('@erpx/server-collaboration/public/staff-notifications');const leave=await createSelfServiceLeave(employee,leaveInput(),key()),ot=await createOvertime(employee,actual(),key()),now=Date.now();
 const notice=async(id:string,at:number)=>{await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'leave',?,1,'Actual employee private handover',?)").bind(key(),company,employee.userId,id,at).run();await publishInboxEvent(db,{key:key(),companyId:company,sourceType:'leave',sourceId:id,sourceVersion:1,category:'work',preview:'Actual employee private handover',recipientIds:[employee.userId]});};
 await notice(String(leave.id),now);for(let index=0;index<105;index++)await notice(`missing-workforce-${key()}`,now+index+1);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(employee.modules).toEqual([]);expect(await listStaffNotifications(db,employee)).toEqual([]);expect((await inboxPage(db,employee,{view:'all'})).items).toEqual([]);await workforcePurchase();
 expect((await listStaffNotifications(db,employee)).map(row=>row.record_id)).toEqual([leave.id]);expect((await notificationsPage(db,employee,{view:'all'})).items.map(row=>row.record_id)).toEqual([leave.id]);const inbox=(await inboxPage(db,employee,{view:'all'})).items;expect(inbox).toHaveLength(2);expect(inbox.every(row=>row.source?.id===leave.id)).toBe(true);
 const work=(await overtimeWork(employee)).find(row=>row.id===ot.id)!;expect(work.canAct).toBe(true);await workforcePurchase('draining');const retained=await enrichFlowWork(employee,[{...work,canAct:true,actionKind:'approval',queue:'todo'}]);expect(retained).toHaveLength(1);expect(retained[0]).toMatchObject({id:ot.id,canAct:false,queue:'waiting',actionKind:'work'});expect(await enrichFlowWork(other,[work])).toEqual([]);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(employee.assignmentId).run();expect(await listStaffNotifications(db,employee)).toEqual([]);expect((await inboxPage(db,employee,{view:'all'})).items).toEqual([]);expect(await enrichFlowWork(employee,[work])).toEqual([]);
});
it('Workforce direct Flow reads and assignments retain actual Leave history while new routing defaults and late source/IAM changes cannot commit',async()=>{
 const {resolveFlow,assignFlowPrimary}=await import('@erpx/server-workflow/public/flow-routing'),owner=await person('RouteOwner','holding_owner'),leave=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(leave.id);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(resolveFlow(db,owner,'leave',id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await workforcePurchase();await expect(resolveFlow(db,other,'leave',id)).rejects.toMatchObject({code:'WORKFORCE_SOURCE_NOT_FOUND'});
 const flow=await resolveFlow(db,owner,'leave',id),step=flow.current!;expect(step?.primary?.userId).toBe(manager.userId);const input={stepKey:step.key,userId:manager.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:'Actual original independent Leave routing',saveDefault:false},command=key(),saved=await assignFlowPrimary(db,owner,'leave',id,input,command);expect(await assignFlowPrimary(db,owner,'leave',id,input,command)).toEqual(saved);await workforcePurchase('draining');expect(await assignFlowPrimary(db,owner,'leave',id,input,command)).toEqual(saved);await expect(assignFlowPrimary(db,owner,'leave',id,{...input,saveDefault:true},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 for(const fault of ['source','authority'] as const){const current=await resolveFlow(db,owner,'leave',id),request={...input,expectedVersion:current.version,recordVersion:current.recordVersion},keyValue=key(),name=`workforce_flow_${key().replaceAll('-','')}`,tables=['flow_routes','flow_events','flow_defaults','leave_requests','assignments','command_receipts','audit_events','outbox_events'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(row)::text image FROM ${table} row ORDER BY row_to_json(row)::text`).all()).results)),before=await snapshot(),effect=fault==='source'?`UPDATE leave_requests SET engagement_id='e-${other.userId}' WHERE id='${id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`;
  await db.exec(`CREATE SEQUENCE ${name}_attempt;CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}_attempt');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${keyValue}') EXECUTE FUNCTION ${name}()`);
  try{await expect(assignFlowPrimary(db,owner,'leave',id,request,keyValue)).rejects.toMatchObject({code:'WORKFORCE_SOURCE_CHANGED'});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${name}_attempt`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${name} ON command_receipts;DROP FUNCTION ${name}();DROP SEQUENCE ${name}_attempt`);}
  const accepted=await assignFlowPrimary(db,owner,'leave',id,request,keyValue);expect(await assignFlowPrimary(db,owner,'leave',id,request,keyValue)).toEqual(accepted);
 }
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();await expect(assignFlowPrimary(db,owner,'leave',id,input,command)).rejects.toMatchObject({code:'WORKFORCE_SOURCE_NOT_FOUND'});
});
it('Workforce projections use the captured current assignment and Work-context revision inside each source statement',async()=>{
 const {resolveFlow}=await import('@erpx/server-workflow/public/flow-routing'),{publishInboxEvent,inboxPage}=await import('@erpx/server-collaboration/public/inbox'),leave=await createSelfServiceLeave(employee,leaveInput(),key()),id=String(leave.id),sessionId=key(),now=Date.now();
 await db.prepare('INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES(?,?,?,?,1,1,?,?)').bind(sessionId,employee.userId,company,employee.assignmentId,now+60000,now).run();
 await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'leave',?,1,'Captured employee source',?)").bind(key(),company,employee.userId,id,now).run();await publishInboxEvent(db,{key:key(),companyId:company,sourceType:'leave',sourceId:id,sourceVersion:1,category:'work',preview:'Captured employee source',recipientIds:[employee.userId]});
 await workforcePurchase();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const captured:WorkContext={...employee,iam:await loadIamAuthority(db,company,employee.userId,employee.role,sessionId),authority:{sessionId,revision:1,assignmentVersion:1}};
 expect((await listStaffNotifications(db,captured)).map(row=>row.record_id)).toEqual([id]);expect((await resolveFlow(db,captured,'leave',id)).recordId).toBe(id);
 await db.prepare('UPDATE active_work_contexts SET revision=2 WHERE session_id=? AND company_id=?').bind(sessionId,company).run();expect(await listStaffNotifications(db,captured)).toEqual([]);expect((await inboxPage(db,captured,{view:'all'})).items).toEqual([]);await expect(resolveFlow(db,captured,'leave',id)).rejects.toMatchObject({code:'WORKFORCE_SOURCE_NOT_FOUND'});
 const switched={...captured,authority:{...captured.authority!,revision:2}};expect((await listStaffNotifications(db,switched)).map(row=>row.record_id)).toEqual([id]);
 await db.prepare('UPDATE assignments SET version=2 WHERE id=?').bind(employee.assignmentId).run();await db.prepare('UPDATE active_work_contexts SET assignment_version=2 WHERE session_id=? AND company_id=?').bind(sessionId,company).run();expect(await listStaffNotifications(db,switched)).toEqual([]);const current={...switched,authority:{...switched.authority,assignmentVersion:2}};expect((await listStaffNotifications(db,current)).map(row=>row.record_id)).toEqual([id]);
});
