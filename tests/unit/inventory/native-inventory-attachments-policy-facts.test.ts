import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
vi.mock('server-only',()=>({}));
import {policyInventoryFacts} from '../../../packages/server-inventory/src/adapters/attachments-policy-facts';
import {INVENTORY_RD_TRANSIT_ATTACHMENT_DESCRIPTOR} from '../../../packages/server-inventory/src/adapters/native-inventory-read';

describe('native Inventory attachments-policy-facts caller-unit read',()=>{
  const envKey='ERPX_NATIVE_INVENTORY_CONFIG';
  const original=process.env[envKey];
  beforeEach(()=>{
    process.env[envKey]=JSON.stringify({privateBaseUrl:'http://127.0.0.1:44991',timeoutMs:1000,
      holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001',companyId:'company'}]});
  });
  afterEach(()=>{
    if(original===undefined)delete process.env[envKey]; else process.env[envKey]=original;
    vi.unstubAllGlobals();
  });
  const plannedSql='SELECT 1 AS row';
  const fakeDb=()=>{
    const bound:unknown[][]=[];
    const db={
      prepare(_sql:string){return {bind(...values:unknown[]){bound.push(values);return this;},async all(){return {results:[{row:1}],success:true,meta:{changes:0,duration:0}};},async first(){return {row:1};}};},
      async batch(){return [{results:[{row:1}],success:true,meta:{changes:0,duration:0}}];},
      async exec(){return {count:0,duration:0};},
    };
    return {db,bound};
  };

  it('switches the transit descriptor read to the native kind with caller-unit binds',async()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db,bound}=fakeDb();
    const facts=policyInventoryFacts(db as never);
    const transit=await facts.readTransit({targetId:'t1',companyId:'company'});
    expect(transit).toEqual({row:1});
    expect(bodies.map(x=>x.kind)).toEqual([INVENTORY_RD_TRANSIT_ATTACHMENT_DESCRIPTOR]);
    expect(bodies.map(x=>x.values)).toEqual([['t1','company']]);
    expect(bound).toEqual([['t1','company']]);
  });

  it('fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    const facts=policyInventoryFacts(db as never);
    await expect(facts.readTransit({targetId:'t1',companyId:'company'})).rejects.toMatchObject({status:503});
  });
});
