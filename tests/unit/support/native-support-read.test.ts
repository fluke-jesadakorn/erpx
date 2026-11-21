import {describe,expect,it} from 'vitest';
import {ApiError} from '@erpx/server-runtime/public/http';
import {createNativeSupportReader,type SupportReadTransport} from '../../../packages/feature-support/src/adapters/server/native-support-read';

const configuration=JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:3105',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}],
});
const incoming=new Headers({
  host:'acme.example.test',
  cookie:'__Host-erpx-product=session',
  'x-erpx-context-company':'company',
  'x-erpx-context-assignment':'support-assignment',
  'x-erpx-context-revision':'7',
  'x-erpx-context-assignment-version':'3',
  'x-erpx-workspace-branch':'bangkok',
});
function transport(body:unknown,status=200){
  const calls:{url?:URL;headers?:Headers;method?:string}[]=[];
  const handler:SupportReadTransport=async(url,headers)=>{
    calls.push({url,headers,method:'GET'});
    return new Response(typeof body==='string'?body:JSON.stringify(body),{status,headers:{'content-type':'application/json'}});
  };
  return {calls,handler};
}
describe('native Support read transport',()=>{
  it('reads the workspace through the exact registered Holding host and forwards the selected context',async()=>{
    const fake=transport({data:{items:[],sources:[],canCreate:true,canCreateOrder:false,ownReportsOnly:false}});
    const reader=createNativeSupportReader(configuration,fake.handler);
    const data=await reader.workspace(incoming,'acme');
    expect(data).toMatchObject({canCreate:true,items:[]});
    expect(fake.calls).toHaveLength(1);
    expect(fake.calls[0].method).toBe('GET');
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/support');
    expect(fake.calls[0].headers?.get('cookie')).toBe('__Host-erpx-product=session');
    expect(fake.calls[0].headers?.get('x-erpx-context-assignment')).toBe('support-assignment');
    expect(fake.calls[0].headers?.get('x-erpx-workspace-branch')).toBe('bangkok');
    expect(fake.calls[0].headers?.get('idempotency-key')).toBeNull();
  });
  it('encodes detail and recipient selectors without inventing cursor or search state',async()=>{
    const fake=transport({data:{row:{id:'ticket-1'}}});
    const reader=createNativeSupportReader(configuration,fake.handler);
    await reader.detail(incoming,'acme','ticket-1');
    expect(fake.calls[0].url?.search).toBe('?id=ticket-1');
    await reader.recipients(incoming,'acme','order-1');
    expect(fake.calls[1].url?.search).toBe('?orderId=order-1');
  });
  it('reads incidents, SLA, sessions and evidence through their exact Support GET paths',async()=>{
    const fake=transport({data:{items:[]}});
    const reader=createNativeSupportReader(configuration,fake.handler);
    await reader.incidents(incoming,'acme');
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/support/incidents');
    await reader.incident(incoming,'acme','incident-1');
    expect(fake.calls[1].url?.search).toBe('?id=incident-1');
    await reader.sla(incoming,'acme');
    expect(fake.calls[2].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/support/sla');
    await reader.sessions(incoming,'acme');
    expect(fake.calls[3].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/support/sessions');
    await reader.session(incoming,'acme','session-1');
    expect(fake.calls[4].url?.search).toBe('?id=session-1');
    await reader.evidence(incoming,'acme','ticket-1');
    expect(fake.calls[5].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/support/evidence?id=ticket-1');
    expect(fake.calls[5].headers?.get('x-erpx-workspace-department')).toBeNull();
    expect(fake.calls[5].headers?.get('cookie')).toBe('__Host-erpx-product=session');
  });
  it('keeps owner denial codes on the remaining Support read surfaces',async()=>{
    const denied=createNativeSupportReader(configuration,transport({error:{code:'SUPPORT_SESSION_NOT_FOUND',message:'Support session not found in this scope.'}},404).handler);
    const error=await denied.session(incoming,'acme','session-absent').catch(value=>value);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({status:404,code:'SUPPORT_SESSION_NOT_FOUND'});
  });
  it('hides the target unless the incoming host and Holding slug match a configured origin',async()=>{
    const fake=transport({data:{}});
    const reader=createNativeSupportReader(configuration,fake.handler);
    await expect(reader.workspace(new Headers({host:'northstar.example.test'}),'acme')).rejects.toMatchObject({status:503,code:'NATIVE_SUPPORT_READ_AUTHORITY_INVALID'});
    await expect(reader.workspace(incoming,'other')).rejects.toMatchObject({status:503,code:'NATIVE_SUPPORT_READ_AUTHORITY_INVALID'});
    expect(fake.calls).toHaveLength(0);
  });
  it('preserves owner denial codes and rejects malformed response envelopes',async()=>{
    const denied=createNativeSupportReader(configuration,transport({error:{code:'SUPPORT_TICKET_NOT_FOUND',message:'Ticket not found in this scope.'}},404).handler);
    const error=await denied.detail(incoming,'acme','ticket-absent').catch(value=>value);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({status:404,code:'SUPPORT_TICKET_NOT_FOUND'});
    const malformed=createNativeSupportReader(configuration,transport({items:[]}).handler);
    await expect(malformed.workspace(incoming,'acme')).rejects.toMatchObject({status:503,code:'NATIVE_SUPPORT_READ_RESPONSE_INVALID'});
  });
  it('rejects reserved, production and malformed private origins before transport',()=>{
    expect(()=>createNativeSupportReader(JSON.stringify({privateBaseUrl:'http://127.0.0.1:3100',timeoutMs:1000,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}]}))).toThrow();
    expect(()=>createNativeSupportReader(JSON.stringify({privateBaseUrl:'http://127.0.0.1:3101',timeoutMs:1000,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}]}))).toThrow();
    expect(()=>createNativeSupportReader('{}')).toThrow();
  });
});
