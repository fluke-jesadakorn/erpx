import {afterEach, describe, expect, it, vi} from 'vitest';
import {
  NATIVE_CRM_ACTIVITY_ACTIONS,
  NATIVE_CRM_LEAD_ACTIONS,
  NATIVE_CRM_OPPORTUNITY_ACTIONS,
  nativeCrmActivityCommand,
  nativeCrmActivityCommandAvailable,
  nativeCrmCommand,
  nativeCrmCommandAvailable,
  nativeCrmCommandFailure,
  nativeCrmCommandRequired,
  NativeCrmCommandError,
} from '../../../packages/feature-sales/src/public/native-crm-command';
vi.mock('server-only',()=>({}));
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
  expect([...NATIVE_CRM_LEAD_ACTIONS]).toEqual(['reassign','disqualify','qualify','reopen']);
  expect([...NATIVE_CRM_OPPORTUNITY_ACTIONS]).toEqual([
    'link-quote',
    'negotiate',
    'lose',
    'reopen',
    'win',
    'win-pms',
  ]);
  expect(nativeCrmCommandAvailable()).toBe(false);
  expect([...NATIVE_CRM_ACTIVITY_ACTIONS]).toEqual(['complete','cancel','reassign']);
  expect(nativeCrmActivityCommandAvailable()).toBe(false);
 });

 it('forwards a native activity create with origin, idempotency and context headers',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn().mockResolvedValue(ok({
    data:{id:'crm-activity-key-1',version:1,status:'open'},
    context:{userId:'user',companyId:'company',holdingId:'h001'},
  }));
  const result=await nativeCrmActivityCommand(actor,'acme',{
    leadId:'lead-1',expectedVersion:1,activityType:'call',
    subject:'Call the customer',details:'Discuss the delivery window',
    ownerId:'owner-1',dueDate:'2026-11-20',
  },'key-1',incoming(),transport);
  expect(result).toEqual({id:'crm-activity-key-1',version:1,status:'open'});
  const [method,url,headers,body]=transport.mock.calls[0]!;
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/crm/activities');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('idempotency-key')).toBe('key-1');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(JSON.parse(body as string)).toEqual({
    leadId:'lead-1',expectedVersion:1,activityType:'call',
    subject:'Call the customer',details:'Discuss the delivery window',
    ownerId:'owner-1',dueDate:'2026-11-20',
  });
 });

 it('forwards native activity complete and reassign with their exact owner fields',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({
      data:{id:'activity-1',version:2,status:'completed'},
      context:{userId:'user',companyId:'company',holdingId:'h001'},
    }))
    .mockResolvedValueOnce(ok({
      data:{id:'activity-1',version:2,status:'open'},
      context:{userId:'user',companyId:'company',holdingId:'h001'},
    }));
  await nativeCrmActivityCommand(actor,'acme',{
    id:'activity-1',action:'complete',expectedVersion:1,reason:'Customer confirmed',
  },'key-2',incoming(),transport);
  await nativeCrmActivityCommand(actor,'acme',{
    id:'activity-1',action:'reassign',expectedVersion:1,ownerId:'owner-2',
    dueDate:'2026-12-01',reason:'Territory change',
  },'key-3',incoming(),transport);
  const [,completeUrl,,completeBody]=transport.mock.calls[0]!;
  expect(completeUrl.toString()).toBe(
    'http://127.0.0.1:44995/api/v1/holdings/acme/crm/activities/activity-1/changes',
  );
  expect(JSON.parse(completeBody as string)).toEqual({
    action:'complete',expectedVersion:1,reason:'Customer confirmed',
  });
  const [,reassignUrl,,reassignBody]=transport.mock.calls[1]!;
  expect(reassignUrl.toString()).toBe(
    'http://127.0.0.1:44995/api/v1/holdings/acme/crm/activities/activity-1/changes',
  );
  expect(JSON.parse(reassignBody as string)).toEqual({
    action:'reassign',expectedVersion:1,ownerId:'owner-2',dueDate:'2026-12-01',
    reason:'Territory change',
  });
 });

 it('fails closed when the native activity configuration is absent',async()=>{
  const transport=vi.fn();
  await expect(nativeCrmActivityCommand(actor,'acme',{
    leadId:'lead-1',expectedVersion:1,activityType:'call',
    subject:'Call the customer',details:'Discuss the delivery window',
    ownerId:'owner-1',dueDate:'2026-11-20',
  },'key-1',incoming(),transport)).rejects.toMatchObject({
    code:'NATIVE_CRM_COMMAND_UNAVAILABLE',status:503,
  });
  expect(transport).not.toHaveBeenCalled();
 });

 it('rejects an out-of-slice activity action without transport',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn();
  await expect(nativeCrmActivityCommand(actor,'acme',{
    id:'activity-1',action:'outcome',expectedVersion:1,reason:'Unsupported',
  },'key-1',incoming(),transport)).rejects.toMatchObject({
    code:'NATIVE_CRM_COMMAND_REQUEST_INVALID',status:422,
  });
  expect(transport).not.toHaveBeenCalled();
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

 it('leaves create and identity commands on the retained path',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn();
  for (const input of [
    {branchId:'branch',title:'New lead'},
    {id:'lead-1',domain:'identity',action:'create-account',kind:'business',name:'Acme'},
  ]) {
    await expect(nativeCrmCommand(actor,'acme',input,'key',incoming(),transport)).resolves.toBeUndefined();
  }
  expect(transport).not.toHaveBeenCalled();
 });

 it('forwards native qualification with the deterministic opportunity receipt',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const input={id:'lead-1',action:'qualify',expectedVersion:1,customerId:'customer-1',
    expectedValue:'1200.50',closeDate:'2026-11-30',reason:'Customer needs are confirmed'};
  const transport=vi.fn().mockResolvedValue(ok({
    id:'lead-1',version:2,opportunityId:'opportunity-key-7',
  }));
  const result=await nativeCrmCommand(actor,'acme',input,'key-7',incoming(),transport);
  expect(result).toEqual({id:'lead-1',version:2,opportunityId:'opportunity-key-7'});
  const [method,url,,body]=transport.mock.calls[0]!;
  expect(method).toBe('POST');
  expect(url.toString()).toBe(
    'http://127.0.0.1:44995/api/v1/holdings/acme/crm/leads/lead-1/changes',
  );
  expect(JSON.parse(body as string)).toEqual({
    action:'qualify',expectedVersion:1,customerId:'customer-1',
    expectedValue:'1200.50',closeDate:'2026-11-30',
    reason:'Customer needs are confirmed',
  });
  const wrong=vi.fn().mockResolvedValue(ok({
    id:'lead-1',version:2,opportunityId:'opportunity-other',
  }));
  await expect(nativeCrmCommand(actor,'acme',input,'key-7',incoming(),wrong))
    .rejects.toMatchObject({code:'NATIVE_CRM_COMMAND_RESPONSE_INVALID'});
 });

 it('forwards native PMS win with exact reservation fields',async()=>{
  process.env.ERPX_NATIVE_CRM_COMMAND_CONFIG=config();
  const transport=vi.fn().mockResolvedValue(ok({
    id:'lead-1',version:2,leadVersion:3,stage:'won',
  }));
  const result=await nativeCrmCommand(actor,'acme',{
    id:'lead-1',domain:'opportunity',action:'win-pms',expectedVersion:1,leadVersion:2,
    reservationId:'reservation-1',propertyId:'property-1',reason:'Confirmed reservation',
  },'key-8',incoming(),transport);
  expect(result).toEqual({id:'lead-1',version:2,leadVersion:3,stage:'won'});
  const [,url,,body]=transport.mock.calls[0]!;
  expect(url.toString()).toBe(
    'http://127.0.0.1:44995/api/v1/holdings/acme/crm/leads/lead-1/opportunity/changes',
  );
  expect(JSON.parse(body as string)).toEqual({
    action:'win-pms',expectedVersion:1,leadVersion:2,reservationId:'reservation-1',
    propertyId:'property-1',reason:'Confirmed reservation',
  });
 });

 it('marks the deleted writers as native-only so the route fails closed',async()=>{
  const qualify={id:'lead-1',action:'qualify',expectedVersion:1,customerId:'customer-1',
    expectedValue:'100',closeDate:'2026-11-30',reason:'Qualified'};
  const winPms={id:'lead-1',domain:'opportunity',action:'win-pms',expectedVersion:1,
    leadVersion:2,reservationId:'reservation-1',propertyId:'property-1',reason:'Won'};
  expect(nativeCrmCommandRequired(qualify)).toBe(true);
  expect(nativeCrmCommandRequired(winPms)).toBe(true);
  expect(nativeCrmCommandRequired({id:'lead-1',action:'disqualify'})).toBe(false);
  expect(nativeCrmCommandRequired({branchId:'branch',title:'New lead'})).toBe(false);
  expect(nativeCrmCommandRequired({id:'lead-1',domain:'identity',action:'create-account'}))
    .toBe(false);
  const transport=vi.fn();
  await expect(nativeCrmCommand(actor,'acme',qualify,'key-9',incoming(),transport))
    .resolves.toBeUndefined();
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
