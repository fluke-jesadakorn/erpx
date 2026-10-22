import {afterEach,describe,expect,it,vi} from 'vitest';
import {createHash} from 'node:crypto';
import {EventEmitter} from 'node:events';
import type {ChildProcess} from 'node:child_process';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,realpathSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {assertProductHostPlanWorkflow,companyAdminOwnerReadsProductHostCase,financeSalesInvoiceProductHostCase,fullAcceptanceProductHostPlanPathsFromEnvironment,permitsProductHostBusinessWrite,permitsProductHostFinanceInvoice,permitsProductHostReadOnlyPost,permitsProductHostRequestPreview,permitsProductHostUiWrite,permitsProductHostRequestOrigin,productHostBusinessWorkflowForCase,productHostPlanPathForWorkflow,productHostPlanPathsFromEnvironment,productHostReadOnlyWorkflowForCase,productHostPlan,productHostProducts,productHostBrowserFile,productHostWorkflowIsArmed,waitForProductHostEdgeMessage,workRequestDraftProductHostCase} from '../../../scripts/testing/product-host-cohort';
import {executiveWorkGetKind,executiveWorkPostKind,executiveWorkBackgroundTrafficIsCurrent,isKnownExecutiveWorkChromiumBackgroundHost} from '../../../scripts/testing/executive-work-host-cohort';
import {productIds} from '@erpx/contracts/product-registry';
import {formsHostOrigin,formsHostWorkflow,formsHostRequestKind,permitsFormsHostRequest,formsHostRedirectAllowed,assertFormsHostHistoricalBindings,type FormsHostPolicyInput,type FormsHostFixture} from '../../../scripts/testing/forms-host-cohort';

const roots:string[]=[];
afterEach(()=>{for(const root of roots.splice(0))rmSync(root,{recursive:true,force:true});});

describe('isolated Executive Work role-selection edge policy',()=>{
 const expected={run:'a'.repeat(12),companyId:'company-fixture',assignmentId:'assignment-fixture',assignmentVersion:1,email:'owner@example.test',password:'fixture-password-that-is-not-used-here',token:'erpx_exec_'+'a'.repeat(43)};
 const selection=(headers:Record<string,string|undefined>={})=>({method:'POST',origin:'https://work-dev-erpx.jesadakorn.com',path:'/launch',headers:{'content-type':'application/x-www-form-urlencoded',origin:'null','sec-fetch-site':'same-origin','sec-fetch-mode':'navigate','sec-fetch-dest':'document',...headers},body:new URLSearchParams({companyId:expected.companyId,next:'/holdings/acme/insights/executive',assignmentId:expected.assignmentId,assignmentVersion:String(expected.assignmentVersion),revision:'0'}).toString()});
 it('allows the exact null-origin same-origin document navigation used by the browser role form',()=>{
  expect(executiveWorkPostKind(selection(),expected)).toBe('role-selection');
  expect(executiveWorkPostKind(selection({origin:'https://work-dev-erpx.jesadakorn.com'}),expected)).toBe('role-selection');
 });
 it('denies null-origin role posts when any Fetch Metadata value leaves the exact navigation shape',()=>{
  for(const mismatch of [{'sec-fetch-site':'cross-site'},{'sec-fetch-mode':'cors'},{'sec-fetch-dest':'empty'}])expect(executiveWorkPostKind(selection(mismatch),expected)).toBeNull();
 });
 const workspacePreference=(headers:Record<string,string|undefined>={},body:unknown={base:'/holdings/acme'},path='/api/v1/workspace-preference',search='')=>({
  method:'POST',origin:'https://work-dev-erpx.jesadakorn.com',path,search,headers:{
   'content-type':'application/json',origin:'https://work-dev-erpx.jesadakorn.com','sec-fetch-site':'same-origin','sec-fetch-mode':'cors','sec-fetch-dest':'empty',
   'x-erpx-context-company':expected.companyId,'x-erpx-context-assignment':expected.assignmentId,'x-erpx-context-revision':'1',...headers,
  },body:JSON.stringify(body),
 });
 it('allows only the selected fixture role to remember its canonical workspace base',()=>{
  expect(executiveWorkPostKind(workspacePreference(),expected,1)).toBe('workspace-preference');
 });
 it('denies workspace preference posts with an unselected, stale, mismatched, or expanded request',()=>{
  expect(executiveWorkPostKind(workspacePreference(),expected)).toBeNull();
  for(const request of [
   workspacePreference({}, {base:'/holdings/acme',extra:true}),
   workspacePreference({}, {base:'/holdings/other'}),
   workspacePreference({}, {base:'/holdings/acme/insights/executive'}),
   workspacePreference({}, {base:'/platform'}),
   workspacePreference({}, {base:'/holdings/acme'}, '/api/v1/workspace-preference/'),
   workspacePreference({}, {base:'/holdings/acme'}, '/api/v1/workspace-preference', '?next=/holdings/acme'),
   workspacePreference({'x-erpx-context-company':'another-company'}),
   workspacePreference({'x-erpx-context-assignment':'another-assignment'}),
   workspacePreference({'x-erpx-context-revision':'0'}),
   workspacePreference({'x-erpx-context-revision':'01'}),
   workspacePreference({'sec-fetch-site':'cross-site'}),
   workspacePreference({'sec-fetch-mode':'navigate'}),
   workspacePreference({'sec-fetch-dest':'document'}),
   workspacePreference({origin:'https://attacker.invalid'}),
   workspacePreference({'content-type':'text/plain'}),
  ]) expect(executiveWorkPostKind(request,expected,1)).toBeNull();
  expect(executiveWorkPostKind({method:'POST',origin:'https://work-dev-erpx.jesadakorn.com',path:'/api/v1/unlisted',headers:{origin:'https://work-dev-erpx.jesadakorn.com'},body:'{}'},expected,1)).toBeNull();
 });
 const mcpRequest=(message:Record<string,unknown>)=>({method:'POST',origin:'https://work-dev-erpx.jesadakorn.com',path:'/api/mcp/executive',headers:{authorization:`Bearer ${expected.token}`,'content-type':'application/json'},body:JSON.stringify(message)});
 it('allows the current MCP client tools/list shape only with omitted or empty parameters',()=>{
  const message={jsonrpc:'2.0',id:1,method:'tools/list'};
  expect(executiveWorkPostKind(mcpRequest(message),expected)).toBe('mcp-read');
  expect(executiveWorkPostKind(mcpRequest({...message,params:{}}),expected)).toBe('mcp-read');
  expect(executiveWorkPostKind(mcpRequest({...message,params:{cursor:'outside-the-exact-read'}}),expected)).toBeNull();
 });
 it('accepts the current MCP initialize version while keeping its request fields exact',()=>{
  const message={jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'executive-work-browser-proof',version:'1.0.0'}}};
  expect(executiveWorkPostKind(mcpRequest(message),expected)).toBe('mcp-read');
  expect(executiveWorkPostKind(mcpRequest({...message,extra:true}),expected)).toBeNull();
 });
 it('allows only the credential-bound no-query Streamable HTTP event-stream read',()=>{
  const eventStream={method:'GET',origin:'https://work-dev-erpx.jesadakorn.com',path:'/api/mcp/executive',search:'',headers:{origin:'https://work-dev-erpx.jesadakorn.com',authorization:`Bearer ${expected.token}`,accept:'text/event-stream'},body:''};
  expect(executiveWorkGetKind(eventStream,expected)).toBe('mcp-event-stream');
  for(const change of [
   {path:'/api/mcp/other'},
   {search:'?resume=1'},
   {origin:'https://account-dev-erpx.jesadakorn.com'},
   {headers:{origin:'https://attacker.invalid',authorization:`Bearer ${expected.token}`,accept:'text/event-stream'}},
   {headers:{origin:'https://work-dev-erpx.jesadakorn.com',authorization:'Bearer wrong',accept:'text/event-stream'}},
   {headers:{origin:'https://work-dev-erpx.jesadakorn.com',authorization:`Bearer ${expected.token}`,accept:'application/json, text/event-stream'}},
   {headers:{origin:'https://work-dev-erpx.jesadakorn.com',authorization:`Bearer ${expected.token}`,accept:'text/event-stream',cookie:'browser-session'}},
   {headers:{origin:'https://work-dev-erpx.jesadakorn.com',authorization:`Bearer ${expected.token}`,accept:'text/event-stream','content-type':'application/json'}},
   {headers:{origin:'https://work-dev-erpx.jesadakorn.com',authorization:`Bearer ${expected.token}`,accept:'text/event-stream','content-length':'1'}},
   {headers:{origin:'https://work-dev-erpx.jesadakorn.com',authorization:`Bearer ${expected.token}`,accept:'text/event-stream','transfer-encoding':'chunked'}},
   {body:'unexpected'},
   {method:'HEAD'},
   {method:'POST'},
  ]) expect(executiveWorkGetKind({...eventStream,...change},expected)).toBeNull();
 });
 it('classifies only exact denied Chromium telemetry hosts as background traffic',()=>{
  expect(isKnownExecutiveWorkChromiumBackgroundHost('www.google.com')).toBe(true);
  expect(isKnownExecutiveWorkChromiumBackgroundHost('content-autofill.googleapis.com')).toBe(true);
  expect(isKnownExecutiveWorkChromiumBackgroundHost('accounts.google.com.attacker.invalid')).toBe(false);
  expect(isKnownExecutiveWorkChromiumBackgroundHost('unknown.example')).toBe(false);
 });
 it('rejects a lease traffic snapshot if a denied background request arrived during verification',()=>{
  const persisted={deniedConnectCount:3,deniedProxyRequestCount:1,connectHosts:{'www.google.com':3},proxyRequestHosts:{'clients2.google.com':1}};
  expect(executiveWorkBackgroundTrafficIsCurrent(persisted,{...persisted})).toBe(true);
  expect(executiveWorkBackgroundTrafficIsCurrent(persisted,{...persisted,deniedConnectCount:4,connectHosts:{'www.google.com':4}})).toBe(false);
 });
});

it('allows a delayed product-host ready handshake while keeping other IPC messages on the short timeout',async()=>{
 vi.useFakeTimers();
 try{
  const readyChild=new EventEmitter() as unknown as ChildProcess;
  const readyOutcome=waitForProductHostEdgeMessage(readyChild,'ready').then(message=>({message}),error=>({error:error instanceof Error?error.message:String(error)}));
  await vi.advanceTimersByTimeAsync(15_000);
  readyChild.emit('message',{kind:'ready'});
  await expect(readyOutcome).resolves.toEqual({message:{kind:'ready'}});
  expect(readyChild.listenerCount('message')).toBe(0);

  const stableChild=new EventEmitter() as unknown as ChildProcess;
  const stableOutcome=waitForProductHostEdgeMessage(stableChild,'stable',7).then(message=>({message}),error=>({error:error instanceof Error?error.message:String(error)}));
  await vi.advanceTimersByTimeAsync(10_000);
  await expect(stableOutcome).resolves.toEqual({error:'PRODUCT_HOST_EDGE_PROTOCOL_TIMEOUT'});
  expect(stableChild.listenerCount('message')).toBe(0);
 }finally{vi.useRealTimers();}
});

function refusedPlan(change:Record<string,unknown>={}){
 const root=realpathSync(mkdtempSync(join(tmpdir(),'erpx-product-host-contract-')));roots.push(root);const directory=join(root,'.scratch/dev'),release=join(root,'.runtime/releases/candidate');mkdirSync(directory,{recursive:true});mkdirSync(release,{recursive:true});
 const path=join(directory,'arm.json');writeFileSync(path,JSON.stringify({version:1,enabled:true,expiresAt:Date.now()+60_000,runtimeFingerprint:'1'.repeat(64),verificationFingerprint:'2'.repeat(64),allowIsolatedNormalMfa:true,allowIsolatedNormalCommercial:true,...change}),{mode:0o600});
 return()=>productHostPlan(root,path,'1'.repeat(64),'2'.repeat(64),release);
}
describe('explicit owned product-host browser mode preflight',()=>{
 it('routes each business case to its own one-workflow plan and unscoped cases to the default arm',()=>{
  const paths=productHostPlanPathsFromEnvironment({ERPX_TEST_PRODUCT_HOST_WORK_PLAN:'/private/work.json',ERPX_TEST_PRODUCT_HOST_FINANCE_PLAN:'/private/finance.json'});
  expect(paths.default).toBe('/private/work.json');
  expect(productHostPlanPathForWorkflow(paths,null)).toBe('/private/work.json');
  expect(productHostPlanPathForWorkflow(paths,'work.request-draft')).toBe('/private/work.json');
  expect(productHostPlanPathForWorkflow(paths,'finance.sales-invoice')).toBe('/private/finance.json');
 });
 it('does not fall back from an explicit missing workflow plan to another plan scope',()=>{
  const paths={default:'/private/work.json',businessWorkflows:{'work.request-draft':'/private/work.json'}};
  expect(productHostPlanPathForWorkflow(paths,'finance.sales-invoice')).toBeUndefined();
  expect(productHostPlanPathForWorkflow(paths,null)).toBe('/private/work.json');
 });
 it('preserves the legacy single-plan path for focused invocations',()=>{
  const paths=productHostPlanPathsFromEnvironment({ERPX_TEST_PRODUCT_HOST_PLAN:'/private/work.json'});
  expect(productHostPlanPathForWorkflow(paths,'work.request-draft')).toBe('/private/work.json');
  expect(productHostPlanPathForWorkflow(paths,'finance.sales-invoice')).toBe('/private/work.json');
  expect(productHostPlanPathForWorkflow(paths,null)).toBe('/private/work.json');
 });
 it('keeps full acceptance scope-specific and never reuses one plan for both business workflows',()=>{
  const oneArm=fullAcceptanceProductHostPlanPathsFromEnvironment({ERPX_TEST_PRODUCT_HOST_PLAN:'/private/work.json'});
  expect(productHostPlanPathForWorkflow(oneArm,'work.request-draft')).toBe('/private/work.json');
  expect(productHostPlanPathForWorkflow(oneArm,'finance.sales-invoice')).toBeUndefined();
  const twoArms=fullAcceptanceProductHostPlanPathsFromEnvironment({ERPX_TEST_PRODUCT_HOST_WORK_PLAN:'/private/work.json',ERPX_TEST_PRODUCT_HOST_FINANCE_PLAN:'/private/finance.json'});
  expect(productHostPlanPathForWorkflow(twoArms,'work.request-draft')).toBe('/private/work.json');
  expect(productHostPlanPathForWorkflow(twoArms,'finance.sales-invoice')).toBe('/private/finance.json');
  expect(twoArms.default).toBe('/private/work.json');
 });
 it('requires the plan arm to match the exact business case workflow',()=>{
  expect(()=>assertProductHostPlanWorkflow({allowIsolatedBusinessWorkflow:'work.request-draft'},'work.request-draft')).not.toThrow();
  expect(()=>assertProductHostPlanWorkflow({allowIsolatedBusinessWorkflow:'finance.sales-invoice'},'work.request-draft')).toThrow('PRODUCT_HOST_BUSINESS_PLAN_SCOPE_MISMATCH');
  expect(()=>assertProductHostPlanWorkflow({},'finance.sales-invoice')).toThrow('PRODUCT_HOST_BUSINESS_PLAN_SCOPE_MISMATCH');
 });
 it('keeps the closed15 names equal to the actual product registry',()=>{expect(productHostProducts).toEqual(productIds);expect(new Set(productHostProducts).size).toBe(15);});
 it('arms a workflow only for its exact registered product-host browser case',()=>{expect(productHostBusinessWorkflowForCase(productHostBrowserFile,workRequestDraftProductHostCase)).toBe('work.request-draft');expect(productHostBusinessWorkflowForCase(productHostBrowserFile,financeSalesInvoiceProductHostCase)).toBe('finance.sales-invoice');expect(productHostBusinessWorkflowForCase('/repo/'+productHostBrowserFile,workRequestDraftProductHostCase)).toBe('work.request-draft');expect(productHostBusinessWorkflowForCase('e2e/identity_access/product-hosts.spec.ts',workRequestDraftProductHostCase)).toBe('work.request-draft');expect(productHostBusinessWorkflowForCase('/repo/e2e/identity_access/product-hosts.spec.ts',workRequestDraftProductHostCase)).toBeNull();expect(productHostBusinessWorkflowForCase(productHostBrowserFile,'a different Finance case')).toBeNull();expect(productHostBusinessWorkflowForCase(productHostBrowserFile,'a different Work case')).toBeNull();expect(productHostBusinessWorkflowForCase('tests/journeys/shared.work/work-journeys.spec.ts',financeSalesInvoiceProductHostCase)).toBeNull();});
 it('classifies the exact Company Admin case as read-only and permits only Account sign-in/out POSTs',()=>{
  const workflow='admin.company-brand-organization-iam' as const;
  expect(productHostReadOnlyWorkflowForCase(productHostBrowserFile,companyAdminOwnerReadsProductHostCase)).toBe(workflow);
  expect(productHostReadOnlyWorkflowForCase('/repo/'+productHostBrowserFile,companyAdminOwnerReadsProductHostCase)).toBe(workflow);
  expect(productHostReadOnlyWorkflowForCase(productHostBrowserFile,'a different Admin case')).toBeNull();
  expect(productHostReadOnlyWorkflowForCase('tests/integration/identity_access/product-workspace.test.ts',companyAdminOwnerReadsProductHostCase)).toBeNull();
  for(const path of ['/api/auth/sign-in/email','/api/auth/sign-out'])expect(permitsProductHostReadOnlyPost(workflow,{method:'POST',origin:'https://account-dev-erpx.jesadakorn.com',path})).toBe(true);
  for(const input of [
   {method:'POST',origin:'https://account-dev-erpx.jesadakorn.com',path:'/api/v1/account/iam/mfa/enroll'},
   {method:'POST',origin:'https://account-dev-erpx.jesadakorn.com',path:'/api/v1/account/iam/mfa/verify'},
   {method:'POST',origin:'https://admin-dev-erpx.jesadakorn.com',path:'/api/v1/platform/commercial/legacy-grants/preview'},
   {method:'POST',origin:'https://admin-dev-erpx.jesadakorn.com',path:'/api/v1/platform/commercial/legacy-grants/apply'},
   {method:'POST',origin:'https://admin-dev-erpx.jesadakorn.com',path:'/admin/launch'},
   {method:'POST',origin:'https://admin-dev-erpx.jesadakorn.com',path:'/api/v1/holdings/simulation/iam/assignments'},
   {method:'GET',origin:'https://account-dev-erpx.jesadakorn.com',path:'/api/auth/sign-in/email'},
   {method:'POST',origin:'https://attacker.invalid',path:'/api/auth/sign-in/email'},
  ])expect(permitsProductHostReadOnlyPost(workflow,input)).toBe(false);
  expect(permitsProductHostReadOnlyPost(null,{method:'POST',origin:'https://account-dev-erpx.jesadakorn.com',path:'/api/auth/sign-in/email'})).toBe(false);
 });
 it('requires the private plan to authorize the exact isolated workflow scope',()=>{expect(productHostWorkflowIsArmed({},null)).toBe(true);expect(productHostWorkflowIsArmed({},'work.request-draft')).toBe(false);expect(productHostWorkflowIsArmed({},'finance.sales-invoice')).toBe(false);expect(productHostWorkflowIsArmed({allowIsolatedBusinessWorkflow:'work.request-draft'},'work.request-draft')).toBe(true);expect(productHostWorkflowIsArmed({allowIsolatedBusinessWorkflow:'finance.sales-invoice'},'finance.sales-invoice')).toBe(true);expect(productHostWorkflowIsArmed({allowIsolatedBusinessWorkflow:'finance.sales-invoice'},'work.request-draft')).toBe(false);});
 it('allows only a service-request draft create on Work and its same-run cancellation across the owner handoff',()=>{
  const scope='work.request-draft' as const,idempotencyKey='migration-case-1234',createdDraftIds=new Set<string>(),base={method:'POST',origin:'https://work-dev-erpx.jesadakorn.com',workflowHeader:scope,idempotencyKey,createdDraftIds},create={...base,path:'/api/v1/holdings/simulation/requests',body:JSON.stringify({kind:'service',category:'it',title:'Migration Work draft case',description:'A fictional isolated owner workflow.'})};
  expect(permitsProductHostBusinessWrite(scope,create)).toBe(true);expect(permitsProductHostBusinessWrite(scope,{...create,origin:'https://support-dev-erpx.jesadakorn.com'})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...create,origin:'https://finance-dev-erpx.jesadakorn.com'})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...create,workflowHeader:'work'})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...create,body:JSON.stringify({...JSON.parse(create.body),kind:'purchase'})})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...create,body:JSON.stringify({...JSON.parse(create.body),title:'Unscoped request'})})).toBe(false);
  const ownId=`request-${idempotencyKey}`,cancel={...base,createdDraftIds:new Set([ownId]),path:`/api/v1/holdings/simulation/requests/${ownId}/actions`,body:JSON.stringify({action:'cancel',expectedVersion:2})};expect(permitsProductHostBusinessWrite(scope,cancel)).toBe(true);expect(permitsProductHostBusinessWrite(scope,{...cancel,origin:'https://support-dev-erpx.jesadakorn.com'})).toBe(true);expect(permitsProductHostBusinessWrite(scope,{...cancel,origin:'https://finance-dev-erpx.jesadakorn.com'})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...cancel,path:'/api/v1/holdings/simulation/requests/request_other/actions'})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...cancel,body:JSON.stringify({action:'approve',expectedVersion:2})})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...cancel,body:JSON.stringify({action:'cancel',expectedVersion:2,note:'outside the exact UI command'})})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...cancel,method:'GET'})).toBe(false);expect(permitsProductHostBusinessWrite(scope,{...cancel,path:'/api/v1/holdings/other/requests/request_123/actions'})).toBe(false);expect(permitsProductHostBusinessWrite(null,create)).toBe(false);
 });
 it('allows one exact disposable Finance invoice body and only its original idempotent replay',()=>{
  const run='abcdef123456',scope='finance.sales-invoice' as const,idempotencyKey='aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee',expectedPartyId='fixture-customer',payload={partyId:expectedPartyId,invoiceDate:'2026-10-21',dueDate:'2026-10-21',currency:'THB',completionConfirmed:true,completionNote:`Fictional Finance Product Host completion evidence ${run}`,lines:[{description:`Fictional Finance Product Host completed service ${run}`,quantity:'2',unitPrice:'100',taxBps:700}]},body=JSON.stringify(payload),base={method:'POST',origin:'https://finance-dev-erpx.jesadakorn.com',path:'/api/v1/holdings/simulation/finance/sales/invoices',search:'',contentType:'application/json',workflowHeader:scope,idempotencyKey,expectedPartyId,run,body};
  expect(permitsProductHostFinanceInvoice(scope,base)).toBe(true);
  expect(permitsProductHostFinanceInvoice(scope,{...base,recordedRequest:{idempotencyKey,body}})).toBe(true);
  for(const request of [
   {...base,method:'GET'},
   {...base,origin:'https://sales-dev-erpx.jesadakorn.com'},
   {...base,path:'/api/v1/holdings/simulation/finance/sales/defaults'},
   {...base,path:'/api/v1/holdings/other/finance/sales/invoices'},
   {...base,search:'?outside=1'},
   {...base,contentType:'text/plain'},
   {...base,workflowHeader:'work.request-draft'},
   {...base,idempotencyKey:'not-a-uuid'},
   {...base,expectedPartyId:'another-customer'},
   {...base,body:JSON.stringify({...payload,currency:'USD'})},
   {...base,body:JSON.stringify({...payload,completionConfirmed:false})},
   {...base,body:JSON.stringify({...payload,invoiceDate:'2026-02-30',dueDate:'2026-02-30'})},
   {...base,body:JSON.stringify({...payload,dueDate:'2026-10-22'})},
   {...base,body:JSON.stringify({...payload,extra:'unbounded input'})},
   {...base,body:JSON.stringify({...payload,lines:[...payload.lines,{...payload.lines[0]}]})},
   {...base,body:JSON.stringify({...payload,lines:[{...payload.lines[0],taxBps:0}]})},
   {...base,recordedRequest:{idempotencyKey:'bbbbbbbb-bbbb-4ccc-8ddd-eeeeeeeeeeee',body}},
   {...base,recordedRequest:{idempotencyKey,body:JSON.stringify({...payload,completionNote:'changed after first request'})}},
  ]) expect(permitsProductHostFinanceInvoice(scope,request)).toBe(false);
  expect(permitsProductHostFinanceInvoice('work.request-draft',base)).toBe(false);
  expect(permitsProductHostFinanceInvoice(null,base)).toBe(false);
 });
 it('permits only the exact read-only service preview on Work and Support for this workflow',()=>{
  const scope='work.request-draft' as const,workOrigin='https://work-dev-erpx.jesadakorn.com',supportOrigin='https://support-dev-erpx.jesadakorn.com',path='/api/v1/holdings/simulation/requests/preview',createdDraftIds=new Set(['request-migration-case-1234']),base={method:'POST',origin:workOrigin,path,workflowHeader:scope,createdDraftIds},body={kind:'service',category:'it',branchId:null,departmentId:null,parentRequestId:null};
  expect(permitsProductHostRequestPreview(scope,{...base,body:JSON.stringify(body)})).toBe(true);expect(permitsProductHostRequestPreview(scope,{...base,origin:supportOrigin,body:JSON.stringify({...body,id:'request-migration-case-1234'})})).toBe(true);expect(permitsProductHostRequestPreview(scope,{...base,origin:supportOrigin,body:JSON.stringify({...body,id:'request-from-another-run'})})).toBe(false);expect(permitsProductHostRequestPreview(scope,{...base,body:JSON.stringify({...body,kind:'purchase'})})).toBe(false);expect(permitsProductHostRequestPreview(scope,{...base,body:JSON.stringify({...body,category:'other'})})).toBe(false);expect(permitsProductHostRequestPreview(scope,{...base,body:JSON.stringify({...body,extra:true})})).toBe(false);expect(permitsProductHostRequestPreview(scope,{...base,path:'/api/v1/holdings/simulation/requests',body:JSON.stringify(body)})).toBe(false);expect(permitsProductHostRequestPreview(scope,{...base,workflowHeader:'',body:JSON.stringify(body)})).toBe(false);expect(permitsProductHostRequestPreview(null,{...base,body:JSON.stringify(body)})).toBe(false);
 });
 it('allows only exact isolated workspace and home-read preference writes',()=>{
  const origin='https://work-dev-erpx.jesadakorn.com',workspace={method:'POST',origin,path:'/api/v1/workspace-preference',body:JSON.stringify({base:'/holdings/simulation'})};
  expect(permitsProductHostUiWrite(workspace)).toBe(true);expect(permitsProductHostUiWrite({...workspace,body:JSON.stringify({base:'/holdings/riverside-lantern'})})).toBe(true);expect(permitsProductHostUiWrite({...workspace,body:JSON.stringify({base:'/holdings/other'})})).toBe(false);expect(permitsProductHostUiWrite({...workspace,body:JSON.stringify({base:'/holdings/simulation',role:'owner'})})).toBe(false);expect(permitsProductHostUiWrite({...workspace,origin:'https://account-dev-erpx.jesadakorn.com'})).toBe(false);expect(permitsProductHostUiWrite({...workspace,method:'GET'})).toBe(false);
  const readHome={method:'POST',origin,path:'/api/v1/holdings/simulation/navigation',body:JSON.stringify({action:'read-home'})};expect(permitsProductHostUiWrite(readHome)).toBe(true);expect(permitsProductHostUiWrite({...readHome,path:'/api/v1/holdings/other/navigation'})).toBe(false);expect(permitsProductHostUiWrite({...readHome,body:JSON.stringify({action:'assign-role'})})).toBe(false);expect(permitsProductHostUiWrite({...readHome,body:JSON.stringify({action:'read-home',companyId:'other'})})).toBe(false);
 });
 it('allows opaque Origin only for the exact native same-origin /launch document POST',()=>{
  const valid={origin:'null',targetOrigin:'https://work-dev-erpx.jesadakorn.com',method:'POST',path:'/launch',fetchSite:'same-origin',fetchMode:'navigate',fetchDest:'document'};
  expect(permitsProductHostRequestOrigin(valid)).toBe(true);
  expect(permitsProductHostRequestOrigin({...valid,origin:undefined})).toBe(false);
  expect(permitsProductHostRequestOrigin({...valid,origin:'https://evil.test'})).toBe(false);
  expect(permitsProductHostRequestOrigin({...valid,path:'/api/v1/holdings/simulation/requests'})).toBe(false);
  expect(permitsProductHostRequestOrigin({...valid,method:'GET'})).toBe(false);
  expect(permitsProductHostRequestOrigin({...valid,fetchSite:'cross-site'})).toBe(false);
  expect(permitsProductHostRequestOrigin({...valid,fetchMode:'cors'})).toBe(false);
  expect(permitsProductHostRequestOrigin({...valid,fetchDest:'empty'})).toBe(false);
  expect(permitsProductHostRequestOrigin({...valid,origin:'https://work-dev-erpx.jesadakorn.com'})).toBe(true);
 });
 it('refuses the disabled arm before reaching an artifact, socket or fixture',async()=>{await expect(refusedPlan({enabled:false})()).rejects.toThrow('CURRENT_EXPLICIT_PRODUCT_HOST_ARM_REQUIRED');});
 it('refuses stale source or verification identities and expired arms',async()=>{for(const change of [{runtimeFingerprint:'f'.repeat(64)},{verificationFingerprint:'f'.repeat(64)},{expiresAt:Date.now()-1}])await expect(refusedPlan(change)()).rejects.toThrow('CURRENT_EXPLICIT_PRODUCT_HOST_ARM_REQUIRED');});
 it('does not treat permission to open private TLS as permission to enroll MFA or apply purchases',async()=>{for(const change of [{allowIsolatedNormalMfa:false},{allowIsolatedNormalCommercial:false}])await expect(refusedPlan(change)()).rejects.toThrow('CURRENT_EXPLICIT_PRODUCT_HOST_ARM_REQUIRED');});
 it('refuses unknown business workflow permissions in the private arm',async()=>{await expect(refusedPlan({allowIsolatedBusinessWorkflow:'work.anything'} )()).rejects.toThrow('CURRENT_EXPLICIT_PRODUCT_HOST_ARM_REQUIRED');});
});


import {verifyProductHostUiLease} from "../../../scripts/testing/product-host-ui-evidence";
/** Real private-file refusal; no TLS listener, native owner or browser starts. */
it("refuses replaced original lease bytes before reading any plan, certificate, tool or credential",async()=>{
 const root=realpathSync(mkdtempSync(join(tmpdir(),"erpx-product-ui-proof-")));roots.push(root);const run="a".repeat(12),directory=join(root,".scratch/dev/test-suites/browser/case",run);mkdirSync(directory,{recursive:true,mode:0o700});const path=join(directory,"product-host-lease.json");writeFileSync(path,"changed original lease",{mode:0o600});
 await expect(verifyProductHostUiLease({root,leaseBinding:{path,sha256:"0".repeat(64)},state:{run,origin:"http://127.0.0.1:3104",sourceFingerprint:"1".repeat(64),release:{id:"original",fileManifestSha256:"2".repeat(64)}},expected:{run,sourceFingerprint:"1".repeat(64),verificationFingerprint:"3".repeat(64),release:{id:"original",fileManifestSha256:"2".repeat(64)},stateBinding:{path:join(directory,"state.json"),sha256:"4".repeat(64),bytes:1},chromiumPath:join(root,"never-read-chromium")}})).rejects.toThrow("UI_PRODUCT_HOST_BOUND_BYTES_CHANGED");
});

describe('isolated Forms-only host request policy',()=>{
 const run='a'.repeat(12),fixture:FormsHostFixture={slug:'simulation',formId:'form-fixture',formVersion:2,mediaPublicationId:'publication-fixture'},key='forms-run-12345678';
 const body=JSON.stringify({formVersion:2,contactName:`Fictional Forms visitor ${run}`,contactDetail:`forms-visitor-${run}@simulation.test`,brief:`Fictional Forms-host inquiry for isolated run ${run}; human review only.`,consentAccepted:true,source:`Declared fictional Forms visitor ${run}`});
 const post=(changes:Partial<FormsHostPolicyInput>={})=>{
  const {headers:headerChanges,...otherChanges}=changes,currentBody=otherChanges.body??body;
  const headers={host:new URL(formsHostOrigin).host,origin:formsHostOrigin,'content-type':'application/json','sec-fetch-site':'same-origin','sec-fetch-mode':'cors','sec-fetch-dest':'empty','idempotency-key':key,...headerChanges,'content-length':headerChanges?.['content-length']??String(Buffer.byteLength(currentBody))};
  return {method:'POST',origin:formsHostOrigin,path:`/api/public/marketing/simulation/${fixture.formId}`,search:'',headers,body:currentBody,fixture,run,prior:{acceptedPosts:0},...otherChanges};
 };
 const get=(path:string,search='')=>({method:'GET',origin:formsHostOrigin,path,search,headers:{host:new URL(formsHostOrigin).host},body:'',fixture,run,prior:{acceptedPosts:0}});
 it('allows only the exact run-bound form, public reads, approved media, and bounded existing assets',()=>{
  expect(formsHostRequestKind(formsHostWorkflow,post())).toBe('submission');
  expect(permitsFormsHostRequest(formsHostWorkflow,post())).toBe(true);
  expect(formsHostRequestKind(formsHostWorkflow,get(`/forms/simulation/${fixture.formId}`,`?utm_source=${encodeURIComponent(`Declared fictional Forms visitor ${run}`)}`))).toBe('forms-page');
  expect(formsHostRequestKind(formsHostWorkflow,get(`/api/public/marketing/simulation/${fixture.formId}`))).toBe('public-form-read');
  expect(formsHostRequestKind(formsHostWorkflow,get(`/api/public/holdings/simulation/media-publications/${fixture.mediaPublicationId}/content`))).toBe('approved-media-read');
  expect(formsHostRequestKind(formsHostWorkflow,get('/fonts/NotoSansThai.woff2'))).toBe('static-read');
  expect(formsHostRequestKind(formsHostWorkflow,get('/_next/static/chunks/app/forms/simulation/page.js'))).toBe('static-read');
 });
 it('allows exactly one byte-identical same-key replay and no later or modified submission',()=>{
  const first=post(),prior={acceptedPosts:1,firstBody:body,firstIdempotencyKey:key};
  expect(formsHostRequestKind(formsHostWorkflow,{...first,prior})).toBe('replay');
  expect(formsHostRequestKind(formsHostWorkflow,{...first,headers:{...first.headers,'idempotency-key':'forms-run-other-1234'},prior})).toBeNull();
  const modifiedBody=body.replace('human review only.','review only.');
  expect(formsHostRequestKind(formsHostWorkflow,{...first,body:modifiedBody,headers:{...first.headers,'content-length':String(Buffer.byteLength(modifiedBody))},prior})).toBeNull();
  expect(formsHostRequestKind(formsHostWorkflow,{...first,prior:{...prior,acceptedPosts:2}})).toBeNull();
 });
 it('fails closed for wrong hosts, paths, queries, write schemas, and credential or cross-site headers',()=>{
  const badConsent=JSON.stringify({...JSON.parse(body),consentAccepted:false});
  const badSource=JSON.stringify({...JSON.parse(body),source:`Other visitor ${run}`});
  const mutations:FormsHostPolicyInput[]=[
   {...post(),origin:'https://attacker.invalid'},
   {...post(),path:'/api/public/marketing/simulation/another-form'},
   {...post(),search:'?campaignId=outside'},
   {...post({body:badConsent})},
   {...post({body:badSource})},
   {...post(),headers:{...post().headers,cookie:'session=secret'}},
   {...post(),headers:{...post().headers,authorization:'Bearer token'}},
   {...post(),headers:{...post().headers,'sec-fetch-site':'cross-site'}},
   {...post(),headers:{...post().headers,'content-length':'1e2'}},
   {...post(),headers:{...post().headers,host:'forms-dev-erpx.jesadakorn.com.attacker.invalid'}},
   {...post(),method:'DELETE'},
  ];
  expect(formsHostRequestKind('forms.other-workflow',post())).toBeNull();
  for(const request of mutations)expect(formsHostRequestKind(formsHostWorkflow,request)).toBeNull();
  expect(formsHostRequestKind(formsHostWorkflow,get(`/forms/simulation/${fixture.formId}`))).toBeNull();
  expect(formsHostRequestKind(formsHostWorkflow,get(`/forms/simulation/${fixture.formId}`,`?utm_source=${encodeURIComponent(`Declared fictional Forms visitor ${run}`)}&extra=1`))).toBeNull();
  expect(formsHostRequestKind(formsHostWorkflow,get('/fonts/other.woff2'))).toBeNull();
  expect(formsHostRequestKind(formsHostWorkflow,get('/_next/static/%2e%2e/secret.js'))).toBeNull();
  expect(formsHostRequestKind(formsHostWorkflow,{...get('/fonts/NotoSansThai.woff2'),headers:{host:new URL(formsHostOrigin).host,'content-length':'1'}})).toBeNull();
 });
 it('constrains redirects to run-bound Forms, public, approved-media, or static requests on the one origin',()=>{
  expect(formsHostRedirectAllowed(`/forms/simulation/${fixture.formId}?utm_source=${encodeURIComponent(`Declared fictional Forms visitor ${run}`)}`,fixture,run)).toBe(true);
  expect(formsHostRedirectAllowed('https://attacker.invalid/forms',fixture,run)).toBe(false);
  expect(formsHostRedirectAllowed(`/forms/simulation/${fixture.formId}?utm_source=Other`,fixture,run)).toBe(false);
  expect(formsHostRedirectAllowed('/api/private/marketing',fixture,run)).toBe(false);
 });
});

describe('offline Forms host lease arm and TLS binding',()=>{
 const root='/offline-forms-fixture',run='a'.repeat(12),sourceFingerprint='1'.repeat(64),verificationFingerprint='2'.repeat(64),spki='3'.repeat(64);
 const observedAt=Date.now()-24*60*60*1000,observedAtText=new Date(observedAt).toISOString(),expiresAt=observedAt+60*60*1000;
 const releaseDirectory=join(root,'.runtime/releases/release-fixture'),certificate={path:join(root,'.scratch/dev/forms/certificate.pem'),sha256:'4'.repeat(64)},
  key={path:join(root,'.scratch/dev/forms/key.pem'),sha256:'5'.repeat(64)},ca={path:join(root,'.scratch/dev/forms/ca.pem'),sha256:'6'.repeat(64)};
 const releaseManifest=Buffer.from('{"server.js":"fixture"}'),releaseManifestSha256=createHash('sha256').update(releaseManifest).digest('hex');
 const release={id:'release-fixture',sourceFingerprint,node:'26.11.1',fileManifestSha256:releaseManifestSha256};
 const arm={schema:'erpx-forms-host-arm/v1',version:1,enabled:true,expiresAt,workflow:formsHostWorkflow,runtimeFingerprint:sourceFingerprint,verificationFingerprint,
  release:{path:join(releaseDirectory,'release.json'),sha256:'7'.repeat(64)},releaseFiles:{path:join(releaseDirectory,'release-files.json'),sha256:releaseManifestSha256},
  tls:{certificate,key,ca,spki},tools:{node:{path:join(root,'.runtime/node/node-v26.11.1-darwin-arm64/bin/node'),sha256:'8'.repeat(64)},chromium:{path:join(root,'.runtime/chromium'),sha256:'9'.repeat(64)}}};
 const lease={sourceFingerprint,verificationFingerprint,release:{id:release.id,fileManifestSha256:release.fileManifestSha256},workflow:formsHostWorkflow,certificate,key,ca,spki,
  ui:{observedAt:observedAtText}} as unknown as Parameters<typeof assertFormsHostHistoricalBindings>[0]['lease'];
 const tls={certificateHostMatches:true,certificateIssuerMatchesCa:true,certificateValidFrom:observedAt-1000,certificateValidTo:observedAt+1000,
  certificateSpki:spki,certificateSignatureValid:true,caIsAuthority:true,privateKeySpki:spki};
 const verify=(changes:Partial<Parameters<typeof assertFormsHostHistoricalBindings>[0]>={})=>assertFormsHostHistoricalBindings({root,lease,arm,release,releaseManifest,tls,...changes});
 it('accepts the bound arm and TLS evidence at observation time even after the arm has since expired',()=>{
  expect(Date.now()).toBeGreaterThan(expiresAt);
  expect(()=>verify()).not.toThrow();
 });
 it('rejects altered arm, release, certificate validity, hostname, chain, or private-key bindings',()=>{
  expect(()=>verify({arm:{...arm,workflow:'forms.other-workflow'}})).toThrow('FORMS_HOST_HISTORICAL_ARM_IDENTITY_REQUIRED');
  expect(()=>verify({arm:{...arm,verificationFingerprint:'a'.repeat(64)}})).toThrow('FORMS_HOST_HISTORICAL_ARM_IDENTITY_REQUIRED');
  expect(()=>verify({release:{...release,id:'other-release'}})).toThrow('FORMS_HOST_HISTORICAL_RELEASE_BINDING_REQUIRED');
  expect(()=>verify({arm:{...arm,tls:{...arm.tls,key:{...key,sha256:'a'.repeat(64)}}}})).toThrow('FORMS_HOST_HISTORICAL_TLS_BINDING_REQUIRED');
  expect(()=>verify({arm:{...arm,tls:{...arm.tls,certificate:{...certificate,sha256:'a'.repeat(64)}}}})).toThrow('FORMS_HOST_HISTORICAL_TLS_BINDING_REQUIRED');
  expect(()=>verify({arm:{...arm,tls:{...arm.tls,ca:{...ca,sha256:'a'.repeat(64)}}}})).toThrow('FORMS_HOST_HISTORICAL_TLS_BINDING_REQUIRED');
  expect(()=>verify({arm:{...arm,tls:{...arm.tls,spki:'a'.repeat(64)}}})).toThrow('FORMS_HOST_HISTORICAL_TLS_BINDING_REQUIRED');
  expect(()=>verify({tls:{...tls,certificateHostMatches:false}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({tls:{...tls,certificateIssuerMatchesCa:false}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({tls:{...tls,certificateValidTo:observedAt-1}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({tls:{...tls,certificateValidFrom:observedAt+1}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({tls:{...tls,certificateSignatureValid:false}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({tls:{...tls,caIsAuthority:false}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({tls:{...tls,certificateSpki:'a'.repeat(64)}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({tls:{...tls,privateKeySpki:'a'.repeat(64)}})).toThrow('FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED');
  expect(()=>verify({arm:{...arm,expiresAt:observedAt-1}})).toThrow('FORMS_HOST_HISTORICAL_ARM_IDENTITY_REQUIRED');
 });
});
