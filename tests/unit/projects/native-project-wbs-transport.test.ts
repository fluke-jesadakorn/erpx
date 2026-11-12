import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeProjectsClient,
  nativeProjectsAvailable,
  nativeProjectsConfiguration,
  NativeProjectsError,
  type NativeProjectsPageContext,
} from '../../../packages/feature-projects/src/public/native-project-wbs';
import { nativeProjectsFailure } from '../../../packages/feature-projects/src/public/native-project-wbs-page';
import { createNativeProjectAttachmentClient } from '../../../packages/feature-projects/src/public/native-project-attachment';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44992',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeProjectsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['projects'],iam:{permissions:['records.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['projects'],permissions:['records.read'],
  expiresAt:9_007_199_254_740_991};
const project={id:'project',company_id:'company',branch_id:'branch',department_id:'department',
  title:'Reviewed project',description:'Bounded project read fixture',owner_user_id:'owner',
  lifecycle_status:'active',version:4,updated_at:10};
const milestone={id:'m-child',company_id:'company',project_id:'project',parent_milestone_id:null,
  ordinal_position:0,title:'Delivery package',description:'',due_at:1000,owner_user_id:'owner',
  owner_name:null,state:'planned',is_overdue:true,version:1,created_by:'owner',completed_at:null,
  created_at:10,updated_at:10,depends_on_ids:[],depends_on_titles:[],blocked_by_titles:[],blocked_by_count:0};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Projects WBS transport',()=>{
 it('forwards the configured Holding, session, CAS and workspace to the WBS read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{project,milestones:[milestone]},context}));
  const client=createNativeProjectsClient(nativeProjectsConfiguration(config()),transport);
  const result=await client.wbs(incoming(),actor,'project',input);
  expect(result).toEqual({project,milestones:[milestone]});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/projects/project/wbs');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace slug as context CAS input',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data:{project,milestones:[milestone]},context:branchContext}));
  const client=createNativeProjectsClient(nativeProjectsConfiguration(config()),transport);
  await client.wbs(incoming(),branchActor,'project',{...input,branchSlug:'north'});
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/projects/project/wbs');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'PROJECT_PACKAGE_PARENT_INVALID',message:'Reload the current project structure.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeProjectsClient(nativeProjectsConfiguration(config()),transport);
  await expect(client.wbs(incoming(),actor,'project',input)).rejects.toMatchObject({
    code:'PROJECT_PACKAGE_PARENT_INVALID',status:409,
    message:'Reload the current project structure.',
  });
 });

 it('accepts distinct dependency IDs with repeated display titles',async()=>{
  const successor={...milestone,id:'successor',title:'Successor',depends_on_ids:['a','b'],
    depends_on_titles:['Delivery','Delivery'],blocked_by_titles:['Delivery','Delivery'],blocked_by_count:2};
  const transport=vi.fn().mockResolvedValue(ok({data:{project,milestones:[
    {...milestone,id:'a',title:'Delivery'},{...milestone,id:'b',title:'Delivery'},successor,
  ]},context}));
  const client=createNativeProjectsClient(nativeProjectsConfiguration(config()),transport);
  const result=await client.wbs(incoming(),actor,'project',input);
  expect(result.milestones).toHaveLength(3);
  expect(result.milestones[2].blocked_by_count).toBe(2);
 });

 it('rejects an unconfigured host, changed context and malformed responses before returning data',async()=>{
  const client=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn());
  await expect(client.wbs(incoming({host:'evil.example.com'}),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_AUTHORITY_INVALID'});
  const changed=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{project,milestones:[milestone]},context:{...context,revision:8}}),
  ));
  await expect(changed.wbs(incoming(),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_CONTEXT_CHANGED'});
  const malformed=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{project:{...project,extra:'field'},milestones:[milestone]},context}),
  ));
  await expect(malformed.wbs(incoming(),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});
  const duplicate=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{project,milestones:[milestone,milestone]},context}),
  ));
  await expect(duplicate.wbs(incoming(),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});
  const missingParent=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{project,milestones:[{...milestone,parent_milestone_id:'missing'}]},context}),
  ));
  await expect(missingParent.wbs(incoming(),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});
  const missingDependency=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{project,milestones:[{...milestone,depends_on_ids:['missing'],depends_on_titles:['Missing']}]},context}),
  ));
  await expect(missingDependency.wbs(incoming(),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});
  const invalidCompletion=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{project,milestones:[{...milestone,state:'completed'}]},context}),
  ));
  await expect(invalidCompletion.wbs(incoming(),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});
  const foreignProject=createNativeProjectsClient(nativeProjectsConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{project:{...project,company_id:'foreign'},milestones:[milestone]},context}),
  ));
  await expect(foreignProject.wbs(incoming(),actor,'project',input))
    .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});
 });

 it('fails closed when the native Projects configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  delete process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  expect(nativeProjectsAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_PROJECTS_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeProjectsClient(nativeProjectsConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeProjectsClient(nativeProjectsConfiguration(config({privateBaseUrl:'http://10.0.0.1:44992'})),vi.fn())).toThrow();
  expect(()=>createNativeProjectsClient(nativeProjectsConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeProjectsConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativeProjectsFailure(new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeProjectsFailure(new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'PROJECTS_REQUEST_INVALID'});
  expect(nativeProjectsFailure(new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_PROJECTS_UNAVAILABLE'});
  expect(nativeProjectsFailure(new NativeProjectsError('PROJECT_PACKAGE_CYCLE',409,'Reload current project.')))
    .toMatchObject({status:409,code:'PROJECT_PACKAGE_CYCLE'});
  expect(nativeProjectsFailure(new Error('other'))).toBeUndefined();
 });
});

describe('native Projects attachment bind transport',()=>{
 const manifest=[{linkId:'link-1',sourceId:'attachment-1',purposeKey:'project_document',checksumSha256:'a'.repeat(64),mediaType:'application/pdf',bytes:10}];
 const data={id:'attachment-1',version:5,status:'bound',ownerVersion:5,attachmentSetId:'set-1',attachmentLinkId:'link-1',setId:'set-1',manifest};
 const headers=()=>new Headers({
  host:'acme.example.com',
  origin:'https://acme.example.com',
  cookie:'__Host-erpx-product=session',
  'x-erpx-context-company':'company',
  'x-erpx-context-assignment':'assignment',
  'x-erpx-context-revision':'7',
  'x-erpx-context-assignment-version':'3',
 });
 const client=(transport:Parameters<typeof createNativeProjectAttachmentClient>[1])=>createNativeProjectAttachmentClient(nativeProjectsConfiguration(config()),transport);

 it('forwards the exact loopback path, key, origin and current context',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data,context}));
  const result=await client(transport).bind(headers(),actor,'set-1',{key:'bind-key-0001',expectedVersion:4});
  expect(result).toEqual({setId:'set-1',version:5,manifest});
  const [method,url,outgoing,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/project-attachments/set-1/bind');
  expect(outgoing.get('idempotency-key')).toBe('bind-key-0001');
  expect(outgoing.get('origin')).toBe('https://acme.example.com');
  expect(outgoing.get('cookie')).toBe('__Host-erpx-product=session');
  expect(outgoing.get('x-erpx-context-company')).toBe('company');
  expect(outgoing.get('content-type')).toBe('application/json');
  expect(new TextDecoder().decode(body)).toBe('{"expectedTargetVersion":4}');
 });

 it('fails closed with the owner status and code and never falls back',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
   JSON.stringify({error:{code:'ATTACHMENT_SET_CHANGED',message:'Refresh before retrying.'}}),
   {status:409,headers:{'content-type':'application/json'}},
  ));
  await expect(client(transport).bind(headers(),actor,'set-1',{key:'bind-key-0001',expectedVersion:4}))
   .rejects.toMatchObject({code:'ATTACHMENT_SET_CHANGED',status:409});
 });

 it('rejects a receipt for another set or version before returning to the route',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{...data,ownerVersion:6},context}));
  await expect(client(transport).bind(headers(),actor,'set-1',{key:'bind-key-0001',expectedVersion:4}))
   .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});
 });

 it('requires the captured context headers and exact configured origin',async()=>{
  const missing=headers();missing.delete('x-erpx-context-company');
  await expect(client(vi.fn()).bind(missing,actor,'set-1',{key:'bind-key-0001',expectedVersion:4}))
   .rejects.toMatchObject({code:'NATIVE_PROJECTS_CONTEXT_CHANGED'});
  const wrongOrigin=headers();wrongOrigin.set('origin','https://other.example.com');
  await expect(client(vi.fn()).bind(wrongOrigin,actor,'set-1',{key:'bind-key-0001',expectedVersion:4}))
   .rejects.toMatchObject({code:'NATIVE_PROJECTS_AUTHORITY_INVALID'});
 });
});

describe('native Projects attachment intake transport',()=>{
 const headers=()=>new Headers({
  host:'acme.example.com',
  origin:'https://acme.example.com',
  cookie:'__Host-erpx-product=session',
  'x-erpx-context-company':'company',
  'x-erpx-context-assignment':'assignment',
  'x-erpx-context-revision':'7',
  'x-erpx-context-assignment-version':'3',
 });
 const client=(transport:Parameters<typeof createNativeProjectAttachmentClient>[1])=>createNativeProjectAttachmentClient(nativeProjectsConfiguration(config()),transport);
 const set={id:'set-1',targetType:'project',targetId:'project',componentKind:'record',componentRef:null,purposeKey:'project_document',expectedTargetVersion:4,policyVersion:1,classification:'internal',requiredCount:0,maxFiles:50,state:'draft',createdAt:10,updatedAt:10,links:[]};
 const scan={attempts:1,lastResult:'pending',lastError:null,signature:null,definitionUpdatedAt:null,scannedAt:null};
 const completion={uploadIntentId:'upload-1',attachmentLinkId:'link-1',attachmentSetId:'set-1',sourceId:'attachment-upload-1',state:'quarantined',status:'scan_pending',scan,replay:false};
 const uploadStatus={uploadIntentId:'upload-1',attachmentLinkId:'link-1',attachmentSetId:'set-1',sourceId:null,sourceState:null,linkState:'draft',status:'reserved',scan:{...scan,attempts:0,lastResult:null},bindable:false,scannedAt:null,expiresAt:9_000_000_000_000};

 it('forwards create, reserve, upload and status with exact paths and bounds',async()=>{
  const transport=vi.fn()
   .mockResolvedValueOnce(new Response(JSON.stringify({data:set,context}),{status:201,headers:{'content-type':'application/json'}}))
   .mockResolvedValueOnce(new Response(JSON.stringify({data:{uploadIntentId:'upload-1',attachmentLinkId:'link-1',uploadUrl:'/api/v1/holdings/acme/project-attachments/uploads/upload-1/content',expiresAt:9_000_000_000_000},context}),{status:201,headers:{'content-type':'application/json'}}))
   .mockResolvedValueOnce(new Response(JSON.stringify({data:completion,context}),{status:202,headers:{'content-type':'application/json'}}))
   .mockResolvedValueOnce(new Response(JSON.stringify({data:uploadStatus,context}),{status:200,headers:{'content-type':'application/json'}}));
  const native=client(transport);
  await expect(native.createSet(headers(),actor,{key:'create-key-0001',targetId:'project',purposeKey:'project_document',expectedTargetVersion:4}))
   .resolves.toMatchObject({id:'set-1',state:'draft'});
  const [createMethod,createUrl,createHeaders,createBody]=transport.mock.calls[0];
  expect(createMethod).toBe('POST');
  expect(createUrl.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/project-attachments/attachment-sets');
  expect(createHeaders.get('idempotency-key')).toBe('create-key-0001');
  expect(createHeaders.get('origin')).toBe('https://acme.example.com');
  expect(createHeaders.get('content-type')).toBe('application/json');
  expect(createHeaders.get('cookie')).toBe('__Host-erpx-product=session');
  expect(JSON.parse(new TextDecoder().decode(createBody))).toEqual({targetType:'project',targetId:'project',componentKind:'record',purposeKey:'project_document',expectedTargetVersion:4});

  await expect(native.reserveFile(headers(),actor,'set-1',{key:'reserve-key-0001',filename:'plan.pdf',mediaType:'application/pdf',bytes:5}))
   .resolves.toMatchObject({uploadIntentId:'upload-1',uploadUrl:'/api/v1/holdings/acme/attachment-sets/set-1/uploads/upload-1/content'});
  const [reserveMethod,reserveUrl,reserveHeaders,reserveBody]=transport.mock.calls[1];
  expect(reserveMethod).toBe('POST');
  expect(reserveUrl.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/project-attachments/attachment-sets/set-1/files');
  expect(reserveHeaders.get('idempotency-key')).toBe('reserve-key-0001');
  expect(JSON.parse(new TextDecoder().decode(reserveBody))).toEqual({filename:'plan.pdf',mediaType:'application/pdf',bytes:5});

  const bytes=new TextEncoder().encode('%PDF-');
  await expect(native.completeUpload(headers(),actor,'set-1','upload-1','application/pdf',bytes))
   .resolves.toMatchObject({status:'scan_pending',attachmentSetId:'set-1'});
  const [uploadMethod,uploadUrl,uploadHeaders,uploadBody]=transport.mock.calls[2];
  expect(uploadMethod).toBe('PUT');
  expect(uploadUrl.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/project-attachments/uploads/upload-1/content');
  expect(uploadHeaders.get('content-type')).toBe('application/pdf');
  expect(uploadHeaders.get('idempotency-key')).toBeNull();
  expect(new TextDecoder().decode(uploadBody)).toBe('%PDF-');

  await expect(native.uploadStatus(headers(),actor,'upload-1')).resolves.toMatchObject({status:'reserved',bindable:false});
  const [statusMethod,statusUrl,,statusBody]=transport.mock.calls[3];
  expect(statusMethod).toBe('GET');
  expect(statusUrl.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/project-attachments/uploads/upload-1/status');
  expect(statusBody.byteLength).toBe(0);
 });

 it('surfaces owner errors and rejects a mismatched receipt without fallback',async()=>{
  const conflict=vi.fn().mockResolvedValue(new Response(
   JSON.stringify({error:{code:'ATTACHMENT_TARGET_CHANGED',message:'The record changed. Refresh before adding files.'}}),
   {status:409,headers:{'content-type':'application/json'}},
  ));
  await expect(client(conflict).createSet(headers(),actor,{key:'create-key-0001',targetId:'project',purposeKey:'project_document',expectedTargetVersion:4}))
   .rejects.toMatchObject({status:409,code:'ATTACHMENT_TARGET_CHANGED'});

  const wrongSet=vi.fn().mockResolvedValue(new Response(
   JSON.stringify({data:{...completion,attachmentSetId:'set-2'},context}),
   {status:202,headers:{'content-type':'application/json'}},
  ));
  await expect(client(wrongSet).completeUpload(headers(),actor,'set-1','upload-1','application/pdf',new TextEncoder().encode('%PDF-')))
   .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});

  const wrongStatus=vi.fn().mockResolvedValue(new Response(
   JSON.stringify({data:{...uploadStatus,uploadIntentId:'upload-2'},context}),
   {status:200,headers:{'content-type':'application/json'}},
  ));
  await expect(client(wrongStatus).uploadStatus(headers(),actor,'upload-1'))
   .rejects.toMatchObject({code:'NATIVE_PROJECTS_RESPONSE_INVALID'});

  const changed=vi.fn().mockResolvedValue(new Response(
   JSON.stringify({data:set,context:{...context,revision:8}}),
   {status:201,headers:{'content-type':'application/json'}},
  ));
  await expect(client(changed).createSet(headers(),actor,{key:'create-key-0001',targetId:'project',purposeKey:'project_document',expectedTargetVersion:4}))
   .rejects.toMatchObject({code:'NATIVE_PROJECTS_CONTEXT_CHANGED'});

  const missing=headers();missing.delete('x-erpx-context-company');
  await expect(client(vi.fn()).createSet(missing,actor,{key:'create-key-0001',targetId:'project',purposeKey:'project_document',expectedTargetVersion:4}))
   .rejects.toMatchObject({code:'NATIVE_PROJECTS_CONTEXT_CHANGED'});
 });
});
