import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
vi.mock('server-only',()=>({}));
import {commerceAttachmentStockSourceImage} from '../../../modules/inventory/server/src/adapters/commerce-attachment-sources';
import {INVENTORY_RD_COMMERCE_ATTACHMENT_STOCK_IMAGE} from '../../../modules/inventory/server/src/adapters/native-inventory-read';

describe('native Inventory commerce attachment stock image caller-unit read',()=>{
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
  const image='{"reservations":[],"allocations":[],"lots":[],"balances":[]}';
  const fakeDb=(rows:unknown[]=[{image}])=>{
    const bound:unknown[][]=[];
    const db={
      prepare(_sql:string){return {bind(...values:unknown[]){bound.push(values);return this;},async all(){return {results:rows,success:true,meta:{changes:0,duration:0}};},async first(){return rows[0]??null;}};},
      async batch(){return [{results:rows,success:true,meta:{changes:0,duration:0}}];},
      async exec(){return {count:0,duration:0};},
    };
    return {db,bound};
  };
  const input={companyId:'company',orderLineIds:['o1','o2'],fulfillmentLineIds:['f1'],lotIds:['l1']};

  it('switches the stock image read to the native kind with caller-unit binds',async()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:'SELECT 1 AS image',values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db,bound}=fakeDb();
    expect(await commerceAttachmentStockSourceImage(db as never,input)).toBe(image);
    expect(bodies.map(x=>x.kind)).toEqual([INVENTORY_RD_COMMERCE_ATTACHMENT_STOCK_IMAGE]);
    expect(bodies.map(x=>x.values)).toEqual([['["f1"]','company','["l1"]','["o1","o2"]']]);
    expect(bound).toEqual([['["f1"]','company','["l1"]','["o1","o2"]']]);
  });

  it('returns null when the native plan yields no row',async()=>{
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {values:unknown[]};
      return new Response(JSON.stringify({data:{statements:[{sql:'SELECT 1 AS image',values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db}=fakeDb([]);
    expect(await commerceAttachmentStockSourceImage(db as never,input)).toBeNull();
  });

  it('fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    await expect(commerceAttachmentStockSourceImage(db as never,input)).rejects.toMatchObject({status:503});
  });
});
