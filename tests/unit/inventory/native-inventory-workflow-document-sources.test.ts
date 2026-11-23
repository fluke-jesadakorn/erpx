import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
vi.mock('server-only',()=>({}));
import {workflowDocument,workflowDocumentPage} from '../../../packages/server-inventory/src/adapters/workflow-document-sources';
import {
  INVENTORY_RD_WORKFLOW_DOCUMENT_STOCK_COUNT,
  INVENTORY_RD_WORKFLOW_DOCUMENT_TRANSFER,
  INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_STOCK_COUNT,
  INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_TRANSFER,
} from '../../../packages/server-inventory/src/adapters/native-inventory-read';

describe('native Inventory workflow-document-sources caller-unit reads',()=>{
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

  const stubPlan=()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    return bodies;
  };

  it('switches both single-row document reads to native kinds with caller-unit binds',async()=>{
    const bodies=stubPlan();
    const {db,bound}=fakeDb();
    expect(await workflowDocument(db as never,'stock_count','company','count-1')).toEqual({row:1});
    expect(await workflowDocument(db as never,'transfer','company','transfer-1')).toEqual({row:1});
    expect(bodies.map(x=>x.kind)).toEqual([INVENTORY_RD_WORKFLOW_DOCUMENT_STOCK_COUNT,INVENTORY_RD_WORKFLOW_DOCUMENT_TRANSFER]);
    expect(bodies.map(x=>x.values)).toEqual([['company','count-1'],['company','transfer-1']]);
    expect(bound).toEqual([['company','count-1'],['company','transfer-1']]);
  });

  it('switches both paginated document reads to native kinds with caller-unit binds',async()=>{
    const bodies=stubPlan();
    const {db,bound}=fakeDb();
    expect(await workflowDocumentPage(db as never,'stock_count','company','after-1')).toMatchObject({results:[{row:1}],success:true});
    expect(await workflowDocumentPage(db as never,'transfer','company','after-2')).toMatchObject({results:[{row:1}],success:true});
    expect(bodies.map(x=>x.kind)).toEqual([INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_STOCK_COUNT,INVENTORY_RD_WORKFLOW_DOCUMENT_PAGE_TRANSFER]);
    expect(bodies.map(x=>x.values)).toEqual([['company','after-1'],['company','after-2']]);
    expect(bound).toEqual([['company','after-1'],['company','after-2']]);
  });

  it('fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    await expect(workflowDocument(db as never,'stock_count','company','count-1')).rejects.toMatchObject({status:503});
    await expect(workflowDocumentPage(db as never,'transfer','company','after-1')).rejects.toMatchObject({status:503});
  });
});
