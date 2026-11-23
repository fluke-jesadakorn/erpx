import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
vi.mock('server-only',()=>({}));
import {CommerceInventoryFacts} from '../../../packages/server-inventory/src/adapters/commerce-facts';
import {INVENTORY_RD_IN_TRANSIT_CURRENCY_TOTALS,INVENTORY_RD_FULFILLMENT_LOT_ALLOCATION} from '../../../packages/server-inventory/src/adapters/native-inventory-read';

describe('native Inventory commerce-facts caller-unit reads',()=>{
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
  const rows=[{currency:'USD',line_count:2,value_minor:1000}];
  const fakeDb=()=>{
    const bound:unknown[][]=[];
    const db={
      prepare(_sql:string){return {bind(...values:unknown[]){bound.push(values);return this;},async all(){return {results:rows,success:true,meta:{changes:0,duration:0}};},async first(){return rows[0]??null;}};},
      async batch(){return [{results:rows,success:true,meta:{changes:0,duration:0}}];},
      async exec(){return {count:0,duration:0};},
    };
    return {db,bound};
  };
  const facts={companyId:'company',branchId:'b1',branchId2:'b2',branchId3:'b3'};

  it('switches the in-transit totals read to the native kind with caller-unit binds',async()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:'SELECT 1',values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db,bound}=fakeDb();
    const result=await CommerceInventoryFacts(db as never).readInventoryInTransit1(facts);
    expect(result.results).toEqual(rows);
    expect(bodies.map(x=>x.kind)).toEqual([INVENTORY_RD_IN_TRANSIT_CURRENCY_TOTALS]);
    expect(bodies.map(x=>x.values)).toEqual([['company','b1','b2','b3']]);
    expect(bound).toEqual([['company','b1','b2','b3']]);
  });

  it('fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    await expect(CommerceInventoryFacts(db as never).readInventoryInTransit1(facts)).rejects.toMatchObject({status:503});
  });

  it('switches the fulfillment lot allocation read to the native kind with caller-unit binds',async()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:'SELECT 1',values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db,bound}=fakeDb();
    const row=await CommerceInventoryFacts(db as never).readInventoryFulfillmentLotAllocations1({id:'line1',lotId:'lot1'});
    expect(row).toEqual(rows[0]);
    expect(bodies.map(x=>x.kind)).toEqual([INVENTORY_RD_FULFILLMENT_LOT_ALLOCATION]);
    expect(bodies.map(x=>x.values)).toEqual([['line1','lot1']]);
    expect(bound).toEqual([['line1','lot1']]);
  });

  it('fulfillment allocation read fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    await expect(CommerceInventoryFacts(db as never).readInventoryFulfillmentLotAllocations1({id:'line1',lotId:'lot1'})).rejects.toMatchObject({status:503});
  });
});
