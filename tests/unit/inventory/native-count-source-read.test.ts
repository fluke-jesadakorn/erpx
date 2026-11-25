import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
vi.mock('server-only',()=>({}));
import {INVENTORY_RD_COUNT_SOURCE_ROW,nativeInventoryReadFirst} from '../../../packages/server-inventory/src/adapters/native-inventory-read';

const RETIRED_SQL='SELECT id,company_id,branch_id,warehouse_id FROM inventory_counts WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)';
const root=resolve(new URL('.',import.meta.url).pathname,'../../..');

describe('native Inventory count-purchase source read',()=>{
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
  const planned={id:'count-1',company_id:'company',branch_id:null,warehouse_id:'warehouse-1'};
  const fakeDb=()=>{
    const bound:unknown[][]=[];
    const db={
      prepare(_sql:string){return {bind(...values:unknown[]){bound.push(values);return this;}};},
      async batch(){return [{results:[planned],success:true,meta:{changes:0,duration:0}}];},
      async exec(){return {count:0,duration:0};},
    };
    return {db,bound};
  };

  it('sends only the owner read kind and scoped values, never caller SQL',async()=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:RETIRED_SQL,values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    const {db,bound}=fakeDb();
    const row=await nativeInventoryReadFirst<typeof planned>(INVENTORY_RD_COUNT_SOURCE_ROW,['count-1','company',null,null],db as never);
    expect(row).toEqual(planned);
    expect(bodies.map(x=>x.kind)).toEqual(['inventory_rd_count_source_row']);
    expect(bodies.map(x=>x.values)).toEqual([['count-1','company',null,null]]);
    expect(String(JSON.stringify(bodies))).not.toMatch(/SELECT|FROM inventory_counts/i);
    expect(bound).toEqual([['count-1','company',null,null]]);
  });

  it('fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb();
    await expect(nativeInventoryReadFirst(INVENTORY_RD_COUNT_SOURCE_ROW,['count-1','company',null,null],db as never)).rejects.toMatchObject({status:503});
  });

  it('deleted the replaced TypeScript SQL and pinned the byte-identical owner plan',()=>{
    const source=readFileSync(`${root}/packages/server-inventory/src/adapters/count-purchases.ts`,'utf8');
    expect(source).not.toContain(RETIRED_SQL);
    expect(source).toContain('INVENTORY_RD_COUNT_SOURCE_ROW');
    expect(source).toContain('nativeInventoryReadFirst');
    const rust=readFileSync(`${root}/packages/server-inventory/src/adapters/native_command_sql.rs`,'utf8');
    expect(rust).toContain(`"${RETIRED_SQL}"`);
    expect(rust).toContain('pub const RD_COUNT_SOURCE_ROW: &str = "inventory_rd_count_source_row";');
    expect(rust).toMatch(/const READ_COUNT_SOURCE_ROW: NativeStatement = NativeStatement \{[\s\S]*?bind_count: 4,/);
  });
});
