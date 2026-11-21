import {readFileSync} from 'node:fs';
import {afterEach,describe,expect,it,vi} from 'vitest';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {
  createNativeHandbookClient,
  type HandbookTransport,
} from '../../../packages/feature-collaboration/src/adapters/server/native-handbook';
import {
  createNativeHandbookReads,
  handbookReads,
  nativeHandbookAvailable,
  nativeHandbookReads,
} from '../../../packages/feature-collaboration/src/adapters/server/native-handbook-page';

const headerState=vi.hoisted(()=>{const holder={current:new Headers()};return {holder,headers:vi.fn(async()=>holder.current)};});
vi.mock('next/headers',()=>({headers:headerState.headers}));
const retained=vi.hoisted(()=>({collaborationPresentationQueries:vi.fn()}));
vi.mock('@erpx/server-collaboration/public/presentation-queries',()=>retained);

const configuration=JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:3105',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}],
});
const objectEnvelope=(value:unknown)=>!!value&&typeof value==='object'&&!Array.isArray(value)&&('error' in value||'data' in value);
const incoming=new Headers({
  host:'acme.example.test',
  cookie:'__Host-erpx-product=session',
  'x-erpx-context-company':'company',
  'x-erpx-context-assignment':'handbook-assignment',
  'x-erpx-context-revision':'7',
  'x-erpx-context-assignment-version':'3',
  'x-erpx-workspace-branch':'bangkok',
  origin:'https://acme.example.test',
});
function transport(body:unknown,status=200){
  const calls:{url?:URL;method?:string;headers?:Headers;body?:string}[]=[];
  const handler:HandbookTransport=async(url,init)=>{
    calls.push({url,method:init.method,headers:init.headers,body:init.body});
    return new Response(typeof body==='string'?body:JSON.stringify(objectEnvelope(body)?body:{data:body}),{
      status,
      headers:{'content-type':'application/json'},
    });
  };
  return {calls,handler};
}
describe('native collaboration handbook transport',()=>{
  it('fails closed in the Next transport instead of calling the legacy owner service',()=>{
    const source=readFileSync(new URL('../../../packages/feature-apps/src/routes/api/v1/holdings/[holdingSlug]/handbook/[[...path]]/route.ts',import.meta.url),'utf8');
    expect(source).toContain('ERPX_NATIVE_HANDBOOK_CONFIG');
    expect(source).toContain('NATIVE_HANDBOOK_UNAVAILABLE');
    expect(source).not.toContain('saveHandbookArticle');
    expect(source).not.toContain('actOnHandbook');
  });
  it('reads the list through the exact registered Host and forwards the selected context',async()=>{
    const fake=transport({access:{editor:true},articles:[],hasMore:false});
    const client=createNativeHandbookClient(configuration,fake.handler);
    const data=await client.list(incoming,'acme',{q:'policy',edit:true,cursor:'1:article',limit:20});
    expect(data).toMatchObject({access:{editor:true}});
    expect(fake.calls).toHaveLength(1);
    expect(fake.calls[0].method).toBe('GET');
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook?q=policy&edit=1&cursor=1%3Aarticle&limit=20');
    expect(fake.calls[0].headers?.get('cookie')).toBe('__Host-erpx-product=session');
    expect(fake.calls[0].headers?.get('x-erpx-context-assignment')).toBe('handbook-assignment');
    expect(fake.calls[0].headers?.get('origin')).toBe('https://acme.example.test');
    expect(fake.calls[0].headers?.get('idempotency-key')).toBeNull();
  });
  it('keeps article, revision and editor selectors on their exact owner paths',async()=>{
    const fake=transport({article:{id:'article-1'}});
    const client=createNativeHandbookClient(configuration,fake.handler);
    await client.article(incoming,'acme','article-1',{edit:true,revision:2});
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook/article-1?edit=1&revision=2');
    await client.editors(incoming,'acme');
    expect(fake.calls[1].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook/editors');
  });
  it('sends the exact retained body and idempotency key for a command',async()=>{
    const fake=transport({id:'article-key',version:1,revision:1,status:'draft'});
    const client=createNativeHandbookClient(configuration,fake.handler);
    const body=JSON.stringify({title:'Policy',category:'policy',body:'Body',expectedVersion:1});
    const data=await client.command(incoming,'acme','',body,'handbook-key-0001');
    expect(data).toMatchObject({id:'article-key',version:1});
    expect(fake.calls[0].method).toBe('POST');
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook');
    expect(fake.calls[0].body).toBe(body);
    expect(fake.calls[0].headers?.get('idempotency-key')).toBe('handbook-key-0001');
    expect(fake.calls[0].headers?.get('content-type')).toBe('application/json');
  });
  it('preserves owner denial and conflict codes across the private boundary',async()=>{
    for(const [status,code] of [[403,'HANDBOOK_EDITOR_REQUIRED'],[409,'VERSION_CONFLICT']] as const){
      const denied=createNativeHandbookClient(configuration,transport({error:{code,message:'denied'}},status).handler);
      const error=await denied.command(incoming,'acme','/article-1','{}', 'handbook-key-0002').catch(value=>value);
      expect(error).toBeInstanceOf(ApiError);
      expect(error).toMatchObject({status,code});
    }
  });
  it('hides the target unless the incoming host and Holding slug match a configured origin',async()=>{
    const fake=transport({});
    const client=createNativeHandbookClient(configuration,fake.handler);
    await expect(client.list(new Headers({host:'northstar.example.test'}),'acme')).rejects.toMatchObject({status:503,code:'NATIVE_HANDBOOK_AUTHORITY_INVALID'});
    await expect(client.list(incoming,'other')).rejects.toMatchObject({status:503,code:'NATIVE_HANDBOOK_AUTHORITY_INVALID'});
    expect(fake.calls).toHaveLength(0);
  });
  it('rejects malformed envelopes and missing keys before transport',async()=>{
    const malformed=createNativeHandbookClient(configuration,transport('{"data":5}').handler);
    await expect(malformed.list(incoming,'acme')).rejects.toMatchObject({status:503,code:'NATIVE_HANDBOOK_RESPONSE_INVALID'});
    const fake=transport({});
    const client=createNativeHandbookClient(configuration,fake.handler);
    await expect(client.command(incoming,'acme','','{}','')).rejects.toMatchObject({status:422,code:'COMMAND_KEY_REQUIRED'});
    expect(fake.calls).toHaveLength(0);
  });
  it('rejects reserved, production and malformed private origins before transport',()=>{
    for(const privateBaseUrl of ['http://127.0.0.1:3100','http://127.0.0.1:3101','http://127.0.0.1:3103','http://127.0.0.1:8080','http://127.0.0.1:3200','https://127.0.0.1:3105']){
      expect(()=>createNativeHandbookClient(JSON.stringify({privateBaseUrl,timeoutMs:1000,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}]}))).toThrow();
    }
    expect(()=>createNativeHandbookClient('{}')).toThrow();
    expect(()=>createNativeHandbookClient(JSON.stringify({privateBaseUrl:'http://127.0.0.1:3105',timeoutMs:1000,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'},{origin:'https://acme.example.test',holdingSlug:'other',holdingId:'h002'}]}))).toThrow();
  });
});

const pageActor={userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company'} as WorkContext;
const pageAccess={owner:false,editor:true};
const pageRow={id:'article-1',page_key:'Policy',version:2,archived:0,revision:1,title:'Policy',category:'policy',excerpt:'Body',published_at:10};
const pageRevision={revision:1,title:'Policy',category:'policy',body:'Body',created_by:'user',created_at:10,published_by:'user',published_at:10};
function pageTransport(body:unknown,status=200){
  const calls:{url?:URL;method?:string;headers?:Headers;body?:string}[]=[];
  const handler:HandbookTransport=async(url,init)=>{
    calls.push({url,method:init.method,headers:init.headers,body:init.body});
    return new Response(JSON.stringify(body),{status,headers:{'content-type':'application/json'}});
  };
  return {calls,client:createNativeHandbookClient(configuration,handler)};
}
function nativePage(body:unknown,status=200){
  const fake=pageTransport(status===200?{data:body}:body,status);
  return {...fake,reads:createNativeHandbookReads(fake.client,incoming,'acme','acme')};
}
afterEach(()=>{
  delete process.env.ERPX_NATIVE_HANDBOOK_CONFIG;
  headerState.headers.mockClear();
  retained.collaborationPresentationQueries.mockReset();
});
describe('native collaboration handbook page reads',()=>{
  it('resolves access through the bounded native list read',async()=>{
    const fake=nativePage({access:pageAccess,articles:[],previousCursor:null,nextCursor:null,hasMore:false});
    await expect(fake.reads.handbookAccess(pageActor)).resolves.toEqual(pageAccess);
    expect(fake.calls).toHaveLength(1);
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook?limit=1');
    expect(fake.calls[0].headers?.get('cookie')).toBe('__Host-erpx-product=session');
  });
  it('forwards the exact list selectors and validates every row',async()=>{
    const fake=nativePage({access:pageAccess,articles:[pageRow],previousCursor:null,nextCursor:'1:article-1',hasMore:true});
    await expect(fake.reads.handbookList(pageActor,'policy',true,'1:article-0')).resolves.toEqual({access:pageAccess,articles:[pageRow],previousCursor:null,nextCursor:'1:article-1',hasMore:true});
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook?q=policy&edit=1&cursor=1%3Aarticle-0');
  });
  it('keeps the article revision selector and history shape',async()=>{
    const fake=nativePage({access:pageAccess,article:{id:'article-1',version:2,draft_revision:3,published_revision:1,archived:0},revision:pageRevision,history:[pageRevision]});
    const result=await fake.reads.handbookArticle(pageActor,'article-1',true,2);
    expect(result.article).toMatchObject({id:'article-1',version:2});
    expect(result.history).toHaveLength(1);
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook/article-1?edit=1&revision=2');
  });
  it('accepts only the exact editor settings shape',async()=>{
    const fake=nativePage({version:4,editors:[{user_id:'editor',granted_at:10}]});
    await expect(fake.reads.handbookEditorOptions(pageActor)).resolves.toEqual({version:4,editors:[{user_id:'editor',granted_at:10}]});
    expect(fake.calls[0].url?.href).toBe('http://127.0.0.1:3105/api/v1/holdings/acme/handbook/editors');
  });
  it('rejects a malformed row instead of returning partial owner data',async()=>{
    const fake=nativePage({access:pageAccess,articles:[{...pageRow,version:'2'}],previousCursor:null,nextCursor:null,hasMore:false});
    await expect(fake.reads.handbookList(pageActor)).rejects.toMatchObject({status:503,code:'NATIVE_HANDBOOK_RESPONSE_INVALID'});
  });
  it('rejects malformed access and purchase facts',async()=>{
    const fake=nativePage({access:{owner:false,editor:'yes'},articles:[],previousCursor:null,nextCursor:null,hasMore:false});
    await expect(fake.reads.handbookAccess(pageActor)).rejects.toMatchObject({status:503,code:'NATIVE_HANDBOOK_RESPONSE_INVALID'});
    const purchase=nativePage({access:{owner:false,editor:true,purchase:{newWork:true,settleExisting:false,history:true}},articles:[],previousCursor:null,nextCursor:null,hasMore:false});
    await expect(purchase.reads.handbookAccess(pageActor)).rejects.toMatchObject({status:503,code:'NATIVE_HANDBOOK_RESPONSE_INVALID'});
  });
  it('fails before transport when the Holding context disagrees',async()=>{
    const fake=pageTransport({data:{}});
    const reads=createNativeHandbookReads(fake.client,incoming,'acme','northstar');
    await expect(reads.handbookList(pageActor)).rejects.toMatchObject({status:409,code:'NATIVE_HANDBOOK_CONTEXT_CHANGED'});
    await expect(reads.handbookList({...pageActor,holdingSlug:'northstar'})).rejects.toMatchObject({status:409,code:'NATIVE_HANDBOOK_CONTEXT_CHANGED'});
    expect(fake.calls).toHaveLength(0);
  });
  it('preserves owner denial and hides a foreign host',async()=>{
    const denied=nativePage({error:{code:'HANDBOOK_EDITOR_REQUIRED',message:'denied'}},403);
    await expect(denied.reads.handbookList(pageActor,'',true)).rejects.toMatchObject({status:403,code:'HANDBOOK_EDITOR_REQUIRED'});
    const foreign=createNativeHandbookReads(pageTransport({data:{}}).client,new Headers({host:'northstar.example.test'}),'acme','acme');
    await expect(foreign.handbookList(pageActor)).rejects.toMatchObject({status:503,code:'NATIVE_HANDBOOK_AUTHORITY_INVALID'});
  });
  it('routes every Handbook SSR reader through the strict selector',()=>{
    for(const path of [
      'packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/handbook/[id]/screen.tsx',
      'packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/knowledge/screen.tsx',
      'packages/workplace/src/routes/holdings/[holdingSlug]/knowledge/handbook/drafts/_screen.tsx',
      'packages/workplace/src/routes/holdings/[holdingSlug]/knowledge/handbook/editors/_screen.tsx',
      'packages/workplace/src/routes/holdings/[holdingSlug]/knowledge/handbook/published/_screen.tsx',
    ]){
      const source=readFileSync(new URL(`../../../${path}`,import.meta.url),'utf8');
      expect(source).toContain('@erpx/feature-collaboration/lib/native-handbook-page');
      expect(source).toMatch(/handbookReads\(\s*actor\s*,\s*holdingSlug\s*\)/);
      expect(source).not.toContain('collaborationPresentationQueries');
    }
  });
});
describe('handbook read selection',()=>{
  it('keeps the retained reader only when the native configuration is unset',async()=>{
    expect(nativeHandbookAvailable()).toBe(false);
    const reader={handbookList:vi.fn()};
    retained.collaborationPresentationQueries.mockResolvedValue(reader);
    await expect(handbookReads(pageActor,'acme')).resolves.toBe(reader);
    expect(retained.collaborationPresentationQueries).toHaveBeenCalledWith('h001');
    expect(headerState.headers).not.toHaveBeenCalled();
  });
  it('never falls back to retained TypeScript after a configured native failure',async()=>{
    process.env.ERPX_NATIVE_HANDBOOK_CONFIG='{';
    expect(nativeHandbookAvailable()).toBe(true);
    await expect(handbookReads(pageActor,'acme')).rejects.toBeInstanceOf(ApiError);
    expect(retained.collaborationPresentationQueries).not.toHaveBeenCalled();
  });
  it('rejects a mismatched actor before selecting a transport',async()=>{
    process.env.ERPX_NATIVE_HANDBOOK_CONFIG=configuration;
    await expect(nativeHandbookReads({...pageActor,holdingSlug:'northstar'},'acme')).rejects.toMatchObject({status:409,code:'NATIVE_HANDBOOK_CONTEXT_CHANGED'});
    expect(headerState.headers).not.toHaveBeenCalled();
    expect(retained.collaborationPresentationQueries).not.toHaveBeenCalled();
  });
});
