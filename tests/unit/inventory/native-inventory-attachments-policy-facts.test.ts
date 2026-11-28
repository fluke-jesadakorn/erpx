import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
vi.mock('server-only',()=>({}));
import {policyInventoryFacts} from '../../../modules/inventory/server/src/adapters/attachments-policy-facts';
import {
  INVENTORY_RD_TRANSIT_ATTACHMENT_DESCRIPTOR,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LOT,
} from '../../../modules/inventory/server/src/adapters/native-inventory-read';

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

  it('maps every attachment target type to a fixed owner descriptor kind',async()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db,bound}=fakeDb();
    const facts=policyInventoryFacts(db as never);
    const cases:[string,string][]=[
      ['landed_cost',INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LANDED_COST],
      ['stock_count',INVENTORY_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT],
      ['inventory_stock_count',INVENTORY_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT],
      ['stock_transfer',INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSFER],
      ['inventory_transfer',INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSFER],
      ['inventory_transit',INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSIT],
      ['inventory_lot',INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LOT],
    ];
    for(const [targetType,kind] of cases){
      expect(await facts.readDescriptor({targetId:'t1',companyId:'company',targetType})).toEqual({row:1});
    }
    expect(bodies.map(x=>x.kind)).toEqual(cases.map(x=>x[1]));
    expect(bodies.every(x=>JSON.stringify(x.values)===JSON.stringify(['t1','company']))).toBe(true);
    expect(bound).toHaveLength(cases.length);
    expect(await facts.readDescriptor({targetId:'t1',companyId:'company',targetType:'unknown'})).toBeNull();
    expect(bodies).toHaveLength(cases.length);
  });

  it('descriptor read fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    const facts=policyInventoryFacts(db as never);
    await expect(facts.readDescriptor({targetId:'t1',companyId:'company',targetType:'inventory_lot'})).rejects.toMatchObject({status:503});
  });
});
