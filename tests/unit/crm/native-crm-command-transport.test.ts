import {afterEach, describe, expect, it, vi} from 'vitest';
import {createCRMActivityApplication,type CRMActivityPorts} from '../../../packages/server-crm/src/application/activities';
import {createRequirePermission} from '../../../packages/server-identity_access/src/application/permissions';
import {ApiError,errorResponse} from '../../../packages/server-runtime/src/adapters/http';
vi.mock('server-only',()=>({}));

it('retains CRM activity schema, permission and record-gate order',async()=>{
 const permissions:string[]=[];const actor={userId:'owner',role:'custom',scope:{kind:'holding'},permissions};
 const requirePermission=createRequirePermission({forbidden:()=>{throw new Error('page only');},error:(status,code,message)=>new ApiError(status,code,message)});
 const read=vi.fn(async()=>({id:'activity',owner_id:'other',status:'completed',version:2}));
 const lead=vi.fn(async()=>({id:'lead',status:'disqualified',stage:null,version:2}));
 const replay=vi.fn(async(_key:string,_operation:string,_id:string,_input:unknown)=>null);
 const ports={actorId:'owner',hasRead:true,sales:true,manager:false,clock:{now:()=>0},isSamePerson:(id:string)=>id==='owner',authorize:(permission:string)=>{requirePermission(actor,permission);},replay,lead,activities:async()=>[],history:async()=>[],recipients:async()=>[],activity:read,workRows:async()=>[],commit:async()=>{throw new Error('unexpected write');}} as unknown as CRMActivityPorts;
 const application=createCRMActivityApplication(ports);
 const change={action:'complete',expectedVersion:1,reason:'Done'};
 let error=await application.change('activity',{...change,reason:'no'},'activity-key').then(()=>null,e=>e);expect(errorResponse(error).status).toBe(422);expect((await errorResponse(error).json()).error.code).toBe('VALIDATION_FAILED');expect(read).not.toHaveBeenCalled();
 error=await application.change('activity',change,'activity-key').then(()=>null,e=>e);expect(error).toMatchObject({status:403,code:'PERMISSION_REQUIRED'});expect(read).not.toHaveBeenCalled();
 permissions.push('commerce.sales','commerce.read');
 error=await application.change('activity',change,'activity-key').then(()=>null,e=>e);expect(error).toMatchObject({status:403,code:'CRM_ACTIVITY_OWNER_REQUIRED'});
 read.mockResolvedValueOnce({id:'activity',owner_id:'owner',status:'completed',version:2});
 error=await application.change('activity',change,'activity-key').then(()=>null,e=>e);expect(error).toMatchObject({status:409,code:'CRM_ACTIVITY_STATE_CHANGED'});
 const create={leadId:'lead',expectedVersion:1.0,activityType:'call',subject:' Call ',details:' Discuss ',ownerId:'owner',dueDate:'2026-11-20',ignored:true};
 error=await application.create(create,'create-key').then(()=>null,e=>e);expect(error).toMatchObject({status:409,code:'CRM_ACTIVITY_SOURCE_CLOSED'});
 expect(replay.mock.calls.at(-1)?.[3]).toEqual({leadId:'lead',expectedVersion:1,activityType:'call',subject:'Call',details:'Discuss',ownerId:'owner',dueDate:'2026-11-20'});
});
import {
  NATIVE_CRM_LEAD_ACTIONS,
  NATIVE_CRM_OPPORTUNITY_ACTIONS,
  nativeCrmCommand,
  nativeCrmCommandAvailable,
  nativeCrmCommandFailure,
  NativeCrmCommandError,
} from '../../../packages/feature-sales/src/public/native-crm-command';
import type {WorkContext} from '../../../packages/server-identity_access/src/public/authorization';

const config=()=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44995',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
});
const actor={
  userId:'user',name:'User',email:'user@example.com',assignmentId:'assignment',
  role:'sales_manager',companyId:'company',branchId:null,departmentId:null,
  holdingId:'h001',holdingSlug:'acme',holdingName:'Acme',
  scope:{kind:'holding',holdingSlug:'acme'},assignmentScope:'holding',inherited:false,
  modules:['crm'],assignments:[],workspaces:[],
  authority:{revision:7,assignmentVersion:3},
  iam:{permissions:['commerce.read','commerce.sales','commerce.approve']},
} as unknown as WorkContext;
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

afterEach(()=>{delete process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG;vi.restoreAllMocks();});

describe('native CRM command transport',()=>{
 it('keeps exactly the owner-complete action slice',()=>{
  expect([...NATIVE_CRM_LEAD_ACTIONS]).toEqual(['reassign','disqualify','reopen']);
  expect([...NATIVE_CRM_OPPORTUNITY_ACTIONS]).toEqual([
    'link-quote',
    'negotiate',
    'lose',
    'reopen',
    'win',
  ]);
  expect(nativeCrmCommandAvailable()).toBe(false);
 });

 it('forwards a lead disqualify with exact path, body, CAS and idempotency headers',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn().mockResolvedValue(ok({id:'lead-1',version:2,opportunityId:null}));
  const result=await nativeCrmCommand(
    actor,'acme',
    {id:'lead-1',action:'disqualify',expectedVersion:1,reason:'No response'},
    'key-1',incoming(),transport,
  );
  expect(result).toEqual({id:'lead-1',version:2,opportunityId:null});
  const [method,url,headers,body]=transport.mock.calls[0]!;
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/crm/leads/lead-1/changes');
  expect(headers.get('idempotency-key')).toBe('key-1');
  expect(headers.get('content-type')).toBe('application/json');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(JSON.parse(body as string)).toEqual({
    action:'disqualify',expectedVersion:1,reason:'No response',
  });
 });

 it('forwards reassign and opportunity lose with their exact owner fields',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({id:'lead-1',version:2,opportunityId:null}))
    .mockResolvedValueOnce(ok({id:'lead-1',version:2,leadVersion:3,stage:'lost'}));
  await nativeCrmCommand(actor,'acme',{
    id:'lead-1',action:'reassign',expectedVersion:1,ownerId:'owner-2',
    dueDate:'2026-12-01',reason:'Territory change',
  },'key-2',incoming(),transport);
  await nativeCrmCommand(actor,'acme',{
    id:'lead-1',domain:'opportunity',action:'lose',expectedVersion:1,leadVersion:2,
    lossReason:'price',reason:'Too expensive',
  },'key-3',incoming(),transport);
  const [,leadUrl,,leadBody]=transport.mock.calls[0]!;
  expect(leadUrl.toString()).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/crm/leads/lead-1/changes');
  expect(JSON.parse(leadBody as string)).toEqual({
    action:'reassign',expectedVersion:1,ownerId:'owner-2',dueDate:'2026-12-01',
    reason:'Territory change',
  });
  const [,opportunityUrl,,opportunityBody]=transport.mock.calls[1]!;
  expect(opportunityUrl.toString()).toBe(
    'http://127.0.0.1:44995/api/v1/holdings/acme/crm/leads/lead-1/opportunity/changes',
  );
  expect(JSON.parse(opportunityBody as string)).toEqual({
    action:'lose',expectedVersion:1,leadVersion:2,lossReason:'price',reason:'Too expensive',
  });
 });

 it('forwards link-quote, negotiate and win with exact owner references',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({id:'lead-1',version:2,leadVersion:3,stage:'proposal'}))
    .mockResolvedValueOnce(ok({id:'lead-1',version:3,leadVersion:4,stage:'negotiation'}))
    .mockResolvedValueOnce(ok({id:'lead-1',version:4,leadVersion:5,stage:'won'}));
  await nativeCrmCommand(actor,'acme',{
    id:'lead-1',domain:'opportunity',action:'link-quote',expectedVersion:1,leadVersion:2,
    quoteId:'quote-1',quoteVersion:2,reason:'Approved quotation',
  },'key-4',incoming(),transport);
  await nativeCrmCommand(actor,'acme',{
    id:'lead-1',domain:'opportunity',action:'negotiate',expectedVersion:2,leadVersion:3,
    reason:'Negotiate terms',
  },'key-5',incoming(),transport);
  await nativeCrmCommand(actor,'acme',{
    id:'lead-1',domain:'opportunity',action:'win',expectedVersion:3,leadVersion:4,
    salesId:'sales-1',salesVersion:2,reason:'Customer signed',
  },'key-6',incoming(),transport);
  expect(JSON.parse(transport.mock.calls[0]![3] as string)).toEqual({
    action:'link-quote',expectedVersion:1,leadVersion:2,quoteId:'quote-1',quoteVersion:2,
    reason:'Approved quotation',
  });
  expect(JSON.parse(transport.mock.calls[1]![3] as string)).toEqual({
    action:'negotiate',expectedVersion:2,leadVersion:3,reason:'Negotiate terms',
  });
  expect(JSON.parse(transport.mock.calls[2]![3] as string)).toEqual({
    action:'win',expectedVersion:3,leadVersion:4,salesId:'sales-1',salesVersion:2,
    reason:'Customer signed',
  });
 });

 it('leaves create, qualify, PMS and identity commands on the retained path',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn();
  for (const input of [
    {branchId:'branch',title:'New lead'},
    {id:'lead-1',action:'qualify',expectedVersion:1,reason:'Qualified'},
    {id:'lead-1',domain:'opportunity',action:'win-pms',expectedVersion:1,leadVersion:2,reason:'Won'},
    {id:'lead-1',domain:'identity',action:'create-account',kind:'business',name:'Acme'},
  ]) {
    await expect(nativeCrmCommand(actor,'acme',input,'key',incoming(),transport)).resolves.toBeUndefined();
  }
  expect(transport).not.toHaveBeenCalled();
 });

 it('does not fall back when a configured native command fails',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'CRM_LEAD_VERSION_CHANGED',message:'The lead changed.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  await expect(nativeCrmCommand(
    actor,'acme',{id:'lead-1',action:'disqualify',expectedVersion:1,reason:'No response'},
    'key',incoming(),transport,
  )).rejects.toMatchObject({code:'CRM_LEAD_VERSION_CHANGED',status:409});
 });

 it('rejects a malformed or out-of-slice acknowledgement',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  for (const value of [
    {id:'lead-1',version:2},
    {id:'lead-1',version:2,opportunityId:'opportunity-1'},
    {id:'lead-2',version:2,opportunityId:null},
    {id:'lead-1',version:0,opportunityId:null},
  ]) {
    const transport=vi.fn().mockResolvedValue(ok(value));
    await expect(nativeCrmCommand(
      actor,'acme',{id:'lead-1',action:'disqualify',expectedVersion:1,reason:'No response'},
      'key',incoming(),transport,
    )).rejects.toMatchObject({code:'NATIVE_CRM_COMMAND_RESPONSE_INVALID'});
  }
  const transport=vi.fn().mockResolvedValue(ok({
    id:'lead-1',version:2,leadVersion:3,stage:'discovery',
  }));
  await expect(nativeCrmCommand(
    actor,'acme',{id:'lead-1',domain:'opportunity',action:'lose',expectedVersion:1,leadVersion:2,reason:'Deferred'},
    'key',incoming(),transport,
  )).rejects.toMatchObject({code:'NATIVE_CRM_COMMAND_RESPONSE_INVALID'});
 });

 it('rejects a host or payload outside the configured authority and bounds',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn();
  await expect(nativeCrmCommand(
    actor,'acme',{id:'lead-1',action:'disqualify',expectedVersion:1,reason:'No response'},
    'key',incoming({host:'evil.example.com'}),transport,
  )).rejects.toMatchObject({code:'NATIVE_CRM_COMMAND_AUTHORITY_INVALID'});
  await expect(nativeCrmCommand(
    actor,'acme',{id:'lead-1',action:'reassign',expectedVersion:1,ownerId:'owner-2',reason:'Change'},
    'key',incoming(),transport,
  )).rejects.toMatchObject({code:'NATIVE_CRM_COMMAND_REQUEST_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });

 it('maps native failures to the retained route status policy',()=>{
  expect(nativeCrmCommandFailure(new NativeCrmCommandError('NATIVE_CRM_COMMAND_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeCrmCommandFailure(new NativeCrmCommandError('CRM_LEAD_INPUT_INVALID',422,'Invalid.')))
    .toMatchObject({status:422,code:'CRM_LEAD_INPUT_INVALID'});
  expect(nativeCrmCommandFailure(new NativeCrmCommandError('NATIVE_CRM_COMMAND_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_CRM_COMMAND_UNAVAILABLE'});
  expect(nativeCrmCommandFailure(new Error('other'))).toBeUndefined();
 });
});
