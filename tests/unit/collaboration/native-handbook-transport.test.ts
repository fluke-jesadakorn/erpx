import {readFileSync} from 'node:fs';
import {describe,expect,it} from 'vitest';
import {ApiError} from '@erpx/server-runtime/public/http';
import {
  createNativeHandbookClient,
  type HandbookTransport,
} from '../../../packages/feature-collaboration/src/adapters/server/native-handbook';

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
