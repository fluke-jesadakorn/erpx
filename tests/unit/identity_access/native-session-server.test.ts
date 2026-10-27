import {describe,it,expect,vi} from 'vitest';
const {requestHeaders}=vi.hoisted(()=>({requestHeaders:vi.fn()}));
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:requestHeaders}));
import {createNativeSessionClient,createNextNativeSessionReader,NativeSessionReadError} from '@erpx/feature-runtime/public/server';
import type {NativeSessionFetch} from '@erpx/feature-runtime/public/server';
import type {AccountSessionResponse} from '@erpx/contracts/account-auth';
const configuration={privateBaseUrl:'http://127.0.0.1:43117',accountOrigin:'https://account.example.test',holdingOrigins:['https://acme.example.test','https://northstar.example.test'],timeoutMs:250} as const;
function session(id='alpha'):AccountSessionResponse{return {session:{id:'session-'+id,userId:id,expiresAt:'2026-10-27T12:00:00Z',createdAt:'2026-10-27T11:00:00Z',updatedAt:'2026-10-27T11:00:00Z'},user:{id,name:'Synthetic '+id,email:id+'@example.test',emailVerified:true,image:null}};}
function json(value:unknown,status=200){return new Response(JSON.stringify(value),{status,headers:{'content-type':'application/json','cache-control':'private, no-store'}});}
function incoming(host='acme.example.test',cookie='__Host-erpx-product=opaque-test-only'){return new Headers({host,cookie,authorization:'never-forward-this','x-forwarded-host':'never-trust-this'});}
describe('typed server-only native session transport',()=>{
 it('forwards only exact Host and opaque Cookie to explicit loopback and never caches or follows redirects',async()=>{
  const calls:{url:URL;init:RequestInit}[]=[];const transport:NativeSessionFetch=async(url,init)=>{calls.push({url,init});return json(session());};
  const read=createNativeSessionClient(configuration,transport);expect(await read(incoming())).toEqual(session());expect(calls).toHaveLength(1);
  expect(calls[0]!.url.href).toBe('http://127.0.0.1:43117/api/auth/get-session');expect(calls[0]!.init).toMatchObject({method:'GET',cache:'no-store',redirect:'error',credentials:'omit'});
  const headers=new Headers(calls[0]!.init.headers);expect([...headers.keys()].sort()).toEqual(['accept','cache-control','cookie','host']);expect(headers.get('host')).toBe('acme.example.test');expect(headers.get('cookie')).toBe('__Host-erpx-product=opaque-test-only');
 });
 it('rejects normalized/foreign/protected transport authorities before any IO',async()=>{
  const transport=vi.fn<NativeSessionFetch>(async()=>json(null));
  for(const privateBaseUrl of ['https://127.0.0.1:43117','http://localhost:43117','http://127.1:43117','http://127.0.0.1:3100','http://127.0.0.1:3101','http://127.0.0.1:3103','http://127.0.0.1:3204','http://127.0.0.1:5432','http://127.0.0.1:8080','http://user:password@127.0.0.1:43117','http://127.0.0.1:43117/','http://127.0.0.1:43117?'])expect(()=>createNativeSessionClient({...configuration,privateBaseUrl},transport)).toThrow(NativeSessionReadError);
  for(const accountOrigin of ['https://ACCOUNT.example.test','https://account.example.test/','https://account.example.test:443','http://account.example.test','https://account.example.test#'])expect(()=>createNativeSessionClient({...configuration,accountOrigin},transport)).toThrow(NativeSessionReadError);
  const read=createNativeSessionClient(configuration,transport);await expect(read(incoming('retired.example.test'))).rejects.toMatchObject({code:'NATIVE_SESSION_AUTHORITY_INVALID'});await expect(read(incoming('acme.example.test','x'.repeat(8193)))).rejects.toMatchObject({code:'NATIVE_SESSION_COOKIE_LIMIT'});expect(transport).not.toHaveBeenCalled();
 });
 it('keeps anonymous200 null distinct from unavailable/invalid/non200 responses',async()=>{
  expect(await createNativeSessionClient(configuration,async()=>json(null))(incoming())).toBeNull();
  for(const status of [201,403,404,500,503])await expect(createNativeSessionClient(configuration,async()=>json(null,status))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_UNAVAILABLE',status});
  await expect(createNativeSessionClient(configuration,async()=>new Response('{}',{headers:{'content-type':'text/plain'}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
  await expect(createNativeSessionClient(configuration,async()=>json({session:session().session,user:{...session().user,id:'other'}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
  await expect(createNativeSessionClient(configuration,async()=>json({...session(),actor:{permissions:['*']}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
  await expect(createNativeSessionClient(configuration,async()=>json({...session(),session:{...session().session,id:''}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
 });
 it('reads actual async Next request snapshots separately for each invocation instead of memoizing a prior user',async()=>{
  requestHeaders.mockResolvedValueOnce(incoming('account.example.test','first')).mockResolvedValueOnce(incoming('northstar.example.test','second'));
  const seen:string[]=[];const read=createNativeSessionClient(configuration,async(_url,init)=>{const cookie=new Headers(init.headers).get('cookie')!;seen.push(cookie);return json(session(cookie));});
  const current=createNextNativeSessionReader(read);expect((await current())?.user.id).toBe('first');expect((await current())?.user.id).toBe('second');expect(seen).toEqual(['first','second']);expect(requestHeaders).toHaveBeenCalledTimes(2);
 });
 it('caps streamed bytes before JSON and closes the owned body on overflow',async()=>{
  let cancelled=0;const response=new Response(new ReadableStream<Uint8Array>({start(controller){controller.enqueue(new Uint8Array(32769));},cancel(){cancelled++;}}),{headers:{'content-type':'application/json'}});
  await expect(createNativeSessionClient(configuration,async()=>response)(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_LIMIT'});expect(cancelled).toBe(1);
 });
 it('propagates caller cancellation and the absolute deadline through the response body',async()=>{
  const controller=new AbortController();let cancelled=0;const response=()=>new Response(new ReadableStream<Uint8Array>({cancel(){cancelled++;}}),{headers:{'content-type':'application/json'}});
  const reading=createNativeSessionClient(configuration,async()=>response())(incoming(),controller.signal);controller.abort();await expect(reading).rejects.toMatchObject({code:'NATIVE_SESSION_ABORTED'});
  await expect(createNativeSessionClient(configuration,async()=>response())(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_ABORTED'});expect(cancelled).toBe(2);
 });
});
