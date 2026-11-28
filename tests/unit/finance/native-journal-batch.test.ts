import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {createServer} from 'node:http';
import {nativeJournalBatch} from '@erpx/finance-ui/public/server';

const base = {holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}],timeoutMs:250} as const;
const key='batch-review-key-0001';
const body=' { "journals":[{"id":"journal-one","expectedVersion":1}],"reason":"Reviewed the original balanced draft" } ';
function incoming(extra:Record<string,string>={}){return new Request('https://acme.example.test/api/v1/holdings/acme/finance/journals/batch-post',{method:'POST',headers:{host:'acme.example.test',cookie:'opaque-original',origin:'https://acme.example.test','content-type':'application/json','idempotency-key':key,'x-erpx-context-company':'company','x-erpx-context-revision':'7','x-erpx-context-assignment':'grant','x-erpx-context-assignment-version':'3','x-erpx-workspace-branch':'bangkok',authorization:'never-forward','x-forwarded-host':'never-trust',...extra},body});}
function reply(value:unknown,status=200){return JSON.stringify({data:value});}
const result={id:`journal-batch-${key}`,status:'posted',journals:[{id:'journal-one',version:2}],lineCount:2};
type Capture={method?:string;url?:string;headers:Record<string,string|string[]|undefined>;body:string};
async function withNative(handler:(capture:Capture)=>string|[number,string],run:()=>Promise<void>){
  let capture:Capture={headers:{},body:''};
  const server=createServer((req,res)=>{
    let text='';req.setEncoding('utf8');req.on('data',chunk=>{text+=chunk;});
    req.on('end',()=>{capture={method:req.method,url:req.url,headers:req.headers,body:text};const out=handler(capture);const status=Array.isArray(out)?out[0]:200;const payload=Array.isArray(out)?out[1]:out;res.writeHead(status,{'content-type':'application/json'});res.end(payload);});
  });
  await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
  const address=server.address();if(!address||typeof address==='string')throw Error('missing port');
  const previous=process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG,previousPost=process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG;
  process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG=JSON.stringify({...base,privateBaseUrl:`http://127.0.0.1:${address.port}`});
  try{await run();}finally{
    if(previous===undefined)delete process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG;else process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG=previous;
    if(previousPost===undefined)delete process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG;else process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG=previousPost;
    await new Promise<void>((resolve,reject)=>server.close(error=>error?reject(error):resolve()));
  }
  return capture;
}

describe('native Finance journal-batch transport',()=>{
  it('forwards one exact batch POST and returns the owner receipt with its full payload',async()=>{
    const capture=await withNative(()=>reply(result),async()=>{
      const response=await nativeJournalBatch(incoming(),'acme');
      expect(response.status).toBe(200);
      expect(await response.json()).toEqual({data:result});
      expect(response.headers.get('cache-control')).toBe('private, no-store');
    });
    expect(capture.method).toBe('POST');
    expect(capture.url).toBe('/api/v1/holdings/acme/finance/journals/batch-post');
    expect(capture.body).toBe(body);
    expect(capture.headers['idempotency-key']).toBe(key);
    expect(capture.headers.cookie).toBe('opaque-original');
    expect(capture.headers['x-erpx-context-company']).toBe('company');
    expect(capture.headers['x-erpx-context-revision']).toBe('7');
    expect(capture.headers['x-erpx-context-assignment']).toBe('grant');
    expect(capture.headers['x-erpx-context-assignment-version']).toBe('3');
    expect(capture.headers['x-erpx-workspace-branch']).toBe('bangkok');
    expect(capture.headers.authorization).toBeUndefined();
    expect(capture.headers['x-forwarded-host']).toBeUndefined();
  });
  it('fails closed without configuration and never selects a TypeScript writer',async()=>{
    const previous=process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG,previousPost=process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG;
    try{
      delete process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG;delete process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG;
      const absent=await nativeJournalBatch(incoming(),'acme');expect(absent.status).toBe(503);expect(await absent.json()).toMatchObject({error:{code:'NATIVE_FINANCE_UNAVAILABLE'}});
      process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG='{broken';
      const broken=await nativeJournalBatch(incoming(),'acme');expect(broken.status).toBe(503);expect(await broken.json()).toMatchObject({error:{code:'NATIVE_FINANCE_UNAVAILABLE'}});
    }finally{
      if(previous===undefined)delete process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG;else process.env.ERPX_NATIVE_FINANCE_JOURNAL_BATCH_CONFIG=previous;
      if(previousPost===undefined)delete process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG;else process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG=previousPost;
    }
  });
  it('rejects a receipt that changes identity, status, line count or journal versions',async()=>{
    const malformed=[
      {...result,id:'journal-batch-other'},
      {...result,status:'draft'},
      {...result,lineCount:9007199254740992},
      {...result,journals:[{id:'journal-one',version:2,extra:true}]},
      {...result,journals:[{id:'journal-one',version:2},{id:'journal-two',version:9007199254740992}]},
      {...result,journals:[]},
      {...result,unexpected:true},
    ];
    for(const value of malformed)await withNative(()=>reply(value),async()=>{
      const response=await nativeJournalBatch(incoming(),'acme');
      expect(response.status).toBe(502);
      expect(await response.json()).toMatchObject({error:{code:'NATIVE_FINANCE_UNAVAILABLE'}});
    });
  });
  it('propagates strict owner errors and closes malformed failures',async()=>{
    await withNative(()=>[409,JSON.stringify({error:{code:'VERSION_CONFLICT',message:'A selected draft changed.'}})],async()=>{
      const response=await nativeJournalBatch(incoming(),'acme');
      expect(response.status).toBe(409);
      expect(await response.json()).toEqual({error:{code:'VERSION_CONFLICT',message:'A selected draft changed.'}});
    });
    await withNative(()=>[409,JSON.stringify({error:{code:'x'.repeat(101),message:'bad'}})],async()=>{
      const response=await nativeJournalBatch(incoming(),'acme');
      expect(response.status).toBe(502);
    });
  });
  it('rejects a foreign Holding, origin, missing key and oversized cookie before any IO',async()=>{
    await withNative(()=>reply(result),async()=>{
      for(const request of [
        incoming({host:'foreign.example.test'}),
        incoming({origin:'https://foreign.example.test'}),
        new Request('https://acme.example.test/api/v1/holdings/acme/finance/journals/batch-post',{method:'POST',headers:{host:'acme.example.test','content-type':'application/json'},body}),
        incoming({cookie:'x'.repeat(8193)}),
      ]){
        const response=await nativeJournalBatch(request,'acme');
        expect(response.status).toBeGreaterThanOrEqual(400);
      }
    });
  });
});
