import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {createNativeSupportEvidenceWriter} from '../../../packages/feature-support/src/adapters/server/native-support-evidence';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44991',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const headers=()=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  origin:'https://acme.example.com',
  'x-erpx-context-company':'company',
  'x-erpx-context-assignment':'assignment',
  'x-erpx-context-revision':'7',
  'x-erpx-context-assignment-version':'3',
});
const ok=(data:unknown)=>new Response(JSON.stringify({data}),{status:200,headers:{'content-type':'application/json'}});

describe('native Support evidence transport',()=>{
 it('forwards the configured Holding with the original session, CAS and key',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({id:'ticket-1',version:2}));
  const write=createNativeSupportEvidenceWriter(config(),transport);
  const body='{"action":"bind-evidence","expectedVersion":1,"attachmentSetIds":["set-1"],"reason":"Internal case evidence"}';
  await expect(write(headers(),'acme',body,'key')).resolves.toEqual({id:'ticket-1',version:2});
  const [url,out,sent]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/support/evidence');
  expect(out.get('cookie')).toBe('__Host-erpx-product=session');
  expect(out.get('x-erpx-context-company')).toBe('company');
  expect(out.get('x-erpx-context-assignment')).toBe('assignment');
  expect(out.get('x-erpx-context-revision')).toBe('7');
  expect(out.get('x-erpx-context-assignment-version')).toBe('3');
  expect(out.get('origin')).toBe('https://acme.example.com');
  expect(out.get('idempotency-key')).toBe('key');
  expect(sent).toBe(body);
 });

 it('rejects an unknown Holding or host before any request',async()=>{
  const transport=vi.fn();
  const write=createNativeSupportEvidenceWriter(config(),transport);
  await expect(write(headers(),'other','{}','key')).rejects.toMatchObject({status:503,code:'NATIVE_SUPPORT_EVIDENCE_AUTHORITY_INVALID'});
  const foreign=headers();foreign.set('host','evil.example.com');
  await expect(write(foreign,'acme','{}','key')).rejects.toMatchObject({code:'NATIVE_SUPPORT_EVIDENCE_AUTHORITY_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });

 it('admits loopback 3101 only with the explicit production opt-in',()=>{
  expect(()=>createNativeSupportEvidenceWriter(config({privateBaseUrl:'http://127.0.0.1:3101'}),vi.fn())).toThrow();
  expect(()=>createNativeSupportEvidenceWriter(config({privateBaseUrl:'http://127.0.0.1:3101',production:true}),vi.fn())).not.toThrow();
 });

 it('maps a native error payload to the original status and code',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(JSON.stringify({error:{code:'SUPPORT_EVIDENCE_CHANGED',message:'Review the current incident state.'}}),{status:409,headers:{'content-type':'application/json'}}));
  const write=createNativeSupportEvidenceWriter(config(),transport);
  await expect(write(headers(),'acme','{}','key')).rejects.toMatchObject({status:409,code:'SUPPORT_EVIDENCE_CHANGED'});
 });
});
