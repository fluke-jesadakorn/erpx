import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
vi.mock('server-only',()=>({}));
import {posStartupInventoryState} from '../../../packages/server-inventory/src/adapters/pos-startup-sources';
import {INVENTORY_RD_POS_STARTUP_INVENTORY_STATE} from '../../../packages/server-inventory/src/adapters/native-inventory-read';

describe('native Inventory pos-startup posting-state caller-unit read',()=>{
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
  const plannedSql='SELECT 1 AS economics,\'[]\'::text image';
  const fakeDb=()=>{
    const bound:unknown[][]=[];
    const db={
      prepare(_sql:string){return {bind(...values:unknown[]){bound.push(values);return this;},async all(){return {results:[],success:true,meta:{changes:0,duration:0}};},async first(){return {economics:true,image:'[]'};}};},
      async batch(){return [{results:[{economics:true,image:'[]'}],success:true,meta:{changes:0,duration:0}}];},
      async exec(){return {count:0,duration:0};},
    };
    return {db,bound};
  };

  it('switches the posting-state read to a native kind with caller-unit binds',async()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db,bound}=fakeDb();
    const state=await posStartupInventoryState(db as never,'company-1');
    expect(state).toEqual({economics:true,image:'[]'});
    expect(bodies.map(x=>x.kind)).toEqual([INVENTORY_RD_POS_STARTUP_INVENTORY_STATE]);
    expect(bodies.map(x=>x.values)).toEqual([['company-1','company-1']]);
    expect(bound).toEqual([['company-1','company-1']]);
  });

  it('fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    await expect(posStartupInventoryState(db as never,'company-1')).rejects.toMatchObject({status:503});
  });
});
