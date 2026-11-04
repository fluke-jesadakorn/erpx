import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
const {controls}=vi.hoisted(()=>({controls:vi.fn()}));
vi.mock('server-only',()=>({}));
vi.mock('@erpx/server-runtime/public/runtime',()=>({holdingResources:async()=>({write:{}})}));
vi.mock('@erpx/server-learning/public/purchases',()=>({learningPurchaseControls:controls,requireLearningPurchasedRead:vi.fn()}));

import {
  createNativeLearningClient,
  nativeLearningAvailable,
  nativeLearningConfiguration,
  nativeLearningFailure,
  NativeLearningError,
} from '../../../packages/server-learning/src/public/native-learning';
import {nativeLearningCommandRoute} from '../../../packages/feature-learn/src/lib/native-learning-command';
import type {NativeLearningPageContext} from '../../../packages/server-learning/src/public/native-learning';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeLearningPageContext={
  userId:'reviewer',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment-reviewer',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['learning'],iam:{permissions:['records.read','records.write','records.review']},
};
const context={userId:'reviewer',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment-reviewer',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['learning'],permissions:['records.read','records.write','records.review'],
  expiresAt:9_007_199_254_740_991};
const course={id:'course-1',company_id:'company',title:'Native course',
  description:'Native course description',provider:'Academy',duration_minutes:30,
  required_for:null,status:'published',created_by:'author',reviewed_by:'reviewer',
  reviewed_at:1_799_999_000_000,version:2,created_at:1_799_999_700_000,updated_at:1_799_999_900_000};
const assignment={id:'learning-assignment-1',company_id:'company',branch_id:'branch',
  department_id:'department',course_id:'course-1',course_version:2,learner_id:'learner',
  due_date:'2026-11-30',status:'in_progress',attendance:null,score_bps:null,
  result_evidence:null,completion_reason:null,created_by:'reviewer',version:2,
  created_at:1_799_999_700_000,updated_at:1_799_999_900_000,course_title:'Native course',
  required_for:'equipment_operator',learner_name:'Learner',learner_eligible:1};
const person={id:'learner',name:'Learner',branch_id:'branch',department_id:'department'};
const event={id:'learning-assignment-event-1',assignment_id:'learning-assignment-1',version:1,
  action:'assign',actor_id:'reviewer',reason:'Required before operating equipment',
  status:'assigned',created_at:1_799_999_700_000};
const handoff={id:'learning-eligibility-1',assignment_id:'learning-assignment-1',
  company_id:'company',person_id:'person-learner',eligibility_code:'equipment_operator',
  source_assignment_version:2,status:'open',created_at:1_799_999_900_000};
const certificate={id:'learning-certificate-1',handoff_id:'learning-eligibility-1',
  company_id:'company',person_id:'person-learner',assignment_id:'learning-assignment-1',
  source_assignment_version:2,eligibility_code:'equipment_operator',valid_from:'2026-11-04',
  expires_on:'2028-11-04',completion_kind:'completed',
  evidence_ref:'Provider result and practical check',issued_by:'learner',
  created_at:1_799_999_900_000,revocation_id:null};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment-reviewer',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

beforeEach(()=>{controls.mockReset();controls.mockResolvedValue(undefined);});
afterEach(()=>{delete process.env.ERPX_NATIVE_LEARNING_CONFIG;vi.unstubAllEnvs();});

describe('native Learning transport',()=>{
 it('forwards the exact workspace Host, session, personal selector and context CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{courses:[course],assignments:[{...assignment,learner_id:'reviewer'}],people:[person],canWrite:true,canReview:true,actorId:'reviewer'},context}));
  const learning=createNativeLearningClient(nativeLearningConfiguration(config()),transport);
  const result=await learning.workspace(incoming(),actor,{...input,personal:true});
  expect(result.courses).toEqual([course]);
  expect(result.assignments).toEqual([{...assignment,learner_id:'reviewer'}]);
  expect(result.people).toEqual([person]);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/learning/workspace?personal=true');
  expect(headers.get('host')).toBe('acme.example.com');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment-reviewer');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the exact scoped detail path and complete detail evidence',async()=>{
  const branchActor={...actor,branchId:'branch',departmentId:'department',scope:{kind:'department' as const,branchSlug:'north',departmentSlug:'team'}};
  const branchContext={...context,branchId:'branch',departmentId:'department',branchSlug:'north',departmentSlug:'team'};
  const transport=vi.fn().mockResolvedValue(ok({data:{courses:[course],assignments:[{...assignment,version:2}],people:[person],canWrite:true,canReview:true,actorId:'reviewer',assignment,history:[event],handoff,certificate},context:branchContext}));
  const learning=createNativeLearningClient(nativeLearningConfiguration(config()),transport);
  const result=await learning.detail(incoming(),branchActor,'learning-assignment-1',{...input,branchSlug:'north',departmentSlug:'team'});
  expect(result.assignment.status).toBe('in_progress');
  expect(result.certificate?.id).toBe('learning-certificate-1');
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/learning/assignments/learning-assignment-1');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(headers.get('x-erpx-workspace-department')).toBe('team');
 });

 it('forwards exact course, assignment and certificate command bodies with keys',async()=>{
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({data:{id:'learning-course-key-1',status:'draft',version:1},context}))
    .mockResolvedValueOnce(ok({data:{id:'course-1',status:'published',version:2},context}))
    .mockResolvedValueOnce(ok({data:{id:'learning-assignment-key-1',status:'assigned',version:1},context}))
    .mockResolvedValueOnce(ok({data:{id:'learning-assignment-1',status:'completed',version:3},context}))
    .mockResolvedValueOnce(ok({data:{id:'learning-certificate-1',status:'revoked',version:2},context}));
  const learning=createNativeLearningClient(nativeLearningConfiguration(config()),transport);
  const createCourse={title:'Native course',description:'Native course description',provider:'Academy',durationMinutes:30,requiredFor:'equipment_operator'};
  expect(await learning.createCourse(incoming(),actor,JSON.stringify(createCourse),'learning-course-command-1',input))
    .toEqual({id:'learning-course-key-1',status:'draft',version:1});
  expect(await learning.changeCourse(incoming(),actor,'course-1',JSON.stringify({action:'publish',expectedVersion:1,reason:'Independent review completed'}),'learning-course-command-2',input))
    .toEqual({id:'course-1',status:'published',version:2});
  const createAssignment={courseId:'course-1',courseVersion:2,learnerId:'learner',dueDate:'2026-11-30',reason:'Required before operating equipment'};
  expect(await learning.createAssignment(incoming(),actor,JSON.stringify(createAssignment),'learning-assignment-command-1',input))
    .toEqual({id:'learning-assignment-key-1',status:'assigned',version:1});
  const complete={action:'complete',expectedVersion:2,reason:'Course and assessment completed',attendance:'attended',scoreBps:9500,evidence:'Provider result and practical check',certificateValidFrom:'2026-11-04',certificateExpiresOn:'2028-11-04',attachmentSetIds:[]};
  expect(await learning.changeAssignment(incoming(),actor,'learning-assignment-1',JSON.stringify(complete),'learning-assignment-command-2',input))
    .toEqual({id:'learning-assignment-1',status:'completed',version:3});
  expect(await learning.revokeCertificate(incoming(),actor,'learning-certificate-1',JSON.stringify({expectedAssignmentVersion:2,reason:'Independent owner retires this certificate'}),'learning-certificate-command-1',input))
    .toEqual({id:'learning-certificate-1',status:'revoked',version:2});
  const calls=transport.mock.calls;
  expect(calls[0][1].toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/learning/courses');
  expect(JSON.parse(calls[0][3]!)).toEqual(createCourse);
  expect(calls[1][1].toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/learning/courses/course-1');
  expect(calls[2][1].toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/learning/assignments');
  expect(calls[3][1].toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/learning/assignments/learning-assignment-1');
  expect(JSON.parse(calls[3][3]!)).toEqual(complete);
  expect(calls[4][1].toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/learning/certificates/learning-certificate-1/revoke');
  expect(calls.every(call=>call[2].get('content-type')==='application/json')).toBe(true);
  expect(calls.map(call=>call[2].get('idempotency-key'))).toEqual([
    'learning-course-command-1','learning-course-command-2','learning-assignment-command-1',
    'learning-assignment-command-2','learning-certificate-command-1',
  ]);
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'LEARNING_STATE_CHANGED',message:'Reload the current assignment.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const learning=createNativeLearningClient(nativeLearningConfiguration(config()),transport);
  await expect(learning.changeAssignment(incoming(),actor,'learning-assignment-1',JSON.stringify({action:'start',expectedVersion:1,reason:'Learner starts the course'}),'learning-assignment-command-3',input))
    .rejects.toMatchObject({code:'LEARNING_STATE_CHANGED',status:409,message:'Reload the current assignment.'});
 });

 it('rejects foreign scope, duplicate rows, extra keys and changed context',async()=>{
  const foreign=createNativeLearningClient(nativeLearningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{courses:[{...course,company_id:'foreign'}],assignments:[],people:[],canWrite:false,canReview:false,actorId:'reviewer'},context}),
  ));
  await expect(foreign.workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_LEARNING_RESPONSE_INVALID'});
  const duplicate=createNativeLearningClient(nativeLearningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{courses:[course],assignments:[assignment,assignment],people:[],canWrite:false,canReview:false,actorId:'reviewer'},context}),
  ));
  await expect(duplicate.workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_LEARNING_RESPONSE_INVALID'});
  const extra=createNativeLearningClient(nativeLearningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{courses:[course],assignments:[],people:[],canWrite:false,canReview:false,actorId:'reviewer',extra:'field'},context}),
  ));
  await expect(extra.workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_LEARNING_RESPONSE_INVALID'});
  const changed=createNativeLearningClient(nativeLearningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{courses:[],assignments:[],people:[],canWrite:false,canReview:false,actorId:'reviewer'},context:{...context,revision:8}}),
  ));
  await expect(changed.workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_LEARNING_CONTEXT_CHANGED'});
  const unknownHost=createNativeLearningClient(nativeLearningConfiguration(config()),vi.fn());
  await expect(unknownHost.workspace(incoming({host:'evil.example.com'}),actor,input)).rejects.toMatchObject({code:'NATIVE_LEARNING_AUTHORITY_INVALID'});
 });

 it('fails closed when native Learning configuration is absent and retains attached completion',async()=>{
  delete process.env.ERPX_NATIVE_LEARNING_CONFIG;
  expect(nativeLearningAvailable()).toBe(false);
  expect(await nativeLearningCommandRoute(actor as never,{entity:'course',title:'Native course'},'learning-course-command-9',incoming())).toBeUndefined();
  process.env.ERPX_NATIVE_LEARNING_CONFIG=config();
  expect(await nativeLearningCommandRoute(actor as never,{entity:'assignment',id:'learning-assignment-1',action:'complete',attachmentSetIds:['set-1']},'learning-assignment-command-9',incoming())).toBeUndefined();
 });

 it('enforces the retained package preflight before a configured native command',async()=>{
  process.env.ERPX_NATIVE_LEARNING_CONFIG=config();
  controls.mockResolvedValue({'learn.courses':{newWork:false,settleExisting:false,history:true,revision:'1'}});
  await expect(nativeLearningCommandRoute(actor as never,{entity:'course',title:'Native course',description:'Native course description',provider:'Academy',durationMinutes:30},'learning-course-command-10',incoming()))
    .rejects.toMatchObject({status:403,code:'LEARNING_PURCHASE_REQUIRED'});
 });

 it('maps a malformed configured native selector to a bounded 503 without fallback',async()=>{
  process.env.ERPX_NATIVE_LEARNING_CONFIG='{not-json';
  await expect(nativeLearningCommandRoute(actor as never,{entity:'course'},'learning-course-command-11',incoming()))
    .rejects.toMatchObject({status:503,code:'NATIVE_LEARNING_UNAVAILABLE'});
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeLearningClient(nativeLearningConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeLearningClient(nativeLearningConfiguration(config({privateBaseUrl:'http://10.0.0.1:44993'})),vi.fn())).toThrow();
  expect(()=>createNativeLearningClient(nativeLearningConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeLearningConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativeLearningFailure(new NativeLearningError('NATIVE_LEARNING_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeLearningFailure(new NativeLearningError('NATIVE_LEARNING_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'LEARNING_REQUEST_INVALID'});
  expect(nativeLearningFailure(new NativeLearningError('NATIVE_LEARNING_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_LEARNING_UNAVAILABLE'});
  expect(nativeLearningFailure(new NativeLearningError('NATIVE_LEARNING_CONFIGURATION_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_LEARNING_UNAVAILABLE'});
  expect(nativeLearningFailure(new NativeLearningError('LEARNING_STATE_CHANGED',409,'Reload.')))
    .toMatchObject({status:409,code:'LEARNING_STATE_CHANGED'});
  expect(nativeLearningFailure(new Error('other'))).toBeUndefined();
 });
});
