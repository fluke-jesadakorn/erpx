import {describe,it,expect,vi} from 'vitest';
import {createDirectorySourceAdminClient,DirectorySourceClientError,type DirectorySourceFetch} from '@erpx/contracts/directory-source-admin-client';
const target={holdingSlug:'acme',connectionId:'source-é'};
const pins={companyId:'company',revision:7,assignmentId:'source-admin-a',assignmentVersion:3};
const input={keyId:'reviewed_key',expectedConnectionVersion:1,reason:'  Reviewed exact source  '};
const pending={id:'directory-source-fixture',status:'pending',version:1,sourceRevision:1};
const response=(data:object)=>new Response(JSON.stringify({data}),{status:200,headers:{'content-type':'application/json'}});
describe('native Directory source contract client',()=>{
 it('sends only bounded intent and the captured complete context through an explicit uncached credential transport',async()=>{
  const fetcher=vi.fn<DirectorySourceFetch>(async()=>response(pending));const client=createDirectorySourceAdminClient(fetcher);
  await expect(client.preview(target,pins,'source-preview-key',input)).resolves.toEqual(pending);
  const [url,request]=fetcher.mock.calls[0] as unknown as [string,RequestInit];expect(url).toBe('/api/v1/holdings/acme/iam/connections/source-%C3%A9/source/preview');expect(request).toMatchObject({method:'POST',credentials:'include',cache:'no-store',redirect:'error'});
  expect(new Headers(request.headers).get('x-erpx-context-assignment-version')).toBe('3');expect(new Headers(request.headers).get('x-erpx-context-revision')).toBe('7');expect(JSON.parse(request.body as string)).toEqual({...input,reason:'Reviewed exact source'});
 });
 it('preserves the independent reason and never caches an approval receipt locally',async()=>{
  const approved={...pending,status:'approved',version:2,decisionReason:'Independently verified source'};const fetcher=vi.fn<DirectorySourceFetch>(async()=>response(approved));const client=createDirectorySourceAdminClient(fetcher);
  for(let i=0;i<2;i++)await expect(client.approve(target,pending.id,pins,'source-approve-key',{expectedVersion:1,reason:'\ufeff Independently verified source '})).resolves.toEqual(approved);
  expect(fetcher).toHaveBeenCalledTimes(2);expect(JSON.parse(fetcher.mock.calls[0][1]!.body as string).reason).toBe(approved.decisionReason);
 });
 it('rejects unsafe versions and incomplete or unsafe render pins before dispatch',async()=>{
  const fetcher=vi.fn<DirectorySourceFetch>(async()=>response(pending));const client=createDirectorySourceAdminClient(fetcher);
  await expect(Promise.resolve().then(()=>client.preview(target,pins,'source-version-key',{...input,expectedConnectionVersion:Number.MAX_SAFE_INTEGER+1}))).rejects.toBeInstanceOf(DirectorySourceClientError);
  await expect(client.preview(target,{...pins,assignmentVersion:0},'source-pins-key',input)).rejects.toMatchObject({code:'DIRECTORY_SOURCE_CONTRACT_INVALID'});expect(fetcher).not.toHaveBeenCalled();
 });
 it('rejects unknown credential fields, missing independent reasons and unsafe output numbers',async()=>{
  for(const data of [{...pending,sessionToken:'synthetic-never-a-response-field'},{...pending,status:'approved',version:2},{...pending,sourceRevision:Number.MAX_SAFE_INTEGER+1}]){
   const client=createDirectorySourceAdminClient(async()=>response(data));await expect(client.preview(target,pins,'source-output-key',input)).rejects.toMatchObject({code:'DIRECTORY_SOURCE_CONTRACT_INVALID'});
  }
 });
 it('retains closed current-policy failures and generic unavailable targets',async()=>{
  const client=createDirectorySourceAdminClient(async()=>new Response(JSON.stringify({error:{code:'IDEMPOTENCY_CONFLICT',message:'This key belongs to another decision.'}}),{status:409}));
  await expect(client.approve(target,pending.id,pins,'source-conflict-key',{expectedVersion:1,reason:'Different exact reason'})).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT',status:409});
  const missing=createDirectorySourceAdminClient(async()=>new Response('Not found.',{status:404}));await expect(missing.preview(target,pins,'source-missing-key',input)).rejects.toMatchObject({code:'NOT_FOUND',status:404});
 });
 it('bounds streamed responses and cancels before retaining an oversized body',async()=>{
  const cancelled=vi.fn();const stream=new ReadableStream<Uint8Array>({start(c){c.enqueue(new Uint8Array(16*1024+1));},cancel:cancelled});const client=createDirectorySourceAdminClient(async()=>new Response(stream));
  await expect(client.preview(target,pins,'source-stream-key',input)).rejects.toMatchObject({code:'DIRECTORY_SOURCE_CONTRACT_INVALID'});expect(cancelled).toHaveBeenCalledOnce();
 });
 it('uses pinned Zod code-point reason bounds and rejects malformed UTF-8 server data',async()=>{
  const fetcher=vi.fn<DirectorySourceFetch>(async()=>response(pending));const client=createDirectorySourceAdminClient(fetcher);await client.preview(target,pins,'source-unicode-key',{...input,reason:'\ufeff🙃ab '});expect(JSON.parse(fetcher.mock.calls[0][1]!.body as string).reason).toBe('🙃ab');
  await expect(Promise.resolve().then(()=>client.preview(target,pins,'source-long-reason',{...input,reason:'🙃'.repeat(1001)}))).rejects.toBeInstanceOf(DirectorySourceClientError);
  const malformed=createDirectorySourceAdminClient(async()=>new Response(new Uint8Array([0xff,0xfe])));await expect(malformed.preview(target,pins,'source-utf8-key',input)).rejects.toMatchObject({code:'DIRECTORY_SOURCE_CONTRACT_INVALID'});
 });
 it('never serializes extra Actor or grant properties supplied beside typed intent',async()=>{
  const fetcher=vi.fn<DirectorySourceFetch>(async()=>response(pending));const client=createDirectorySourceAdminClient(fetcher);const extra={...input,actor:{userId:'different-user',permissions:['iam.review']},database:'untrusted'};await client.preview(target,pins,'source-closed-intent',extra);
  const body=JSON.parse(fetcher.mock.calls[0][1]!.body as string);expect(Object.keys(body).sort()).toEqual(['expectedConnectionVersion','keyId','reason']);expect(body.actor).toBeUndefined();expect(new Headers(fetcher.mock.calls[0][1]!.headers).has('cookie')).toBe(false);
 });
});
