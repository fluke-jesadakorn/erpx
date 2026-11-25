import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
vi.mock('server-only',()=>({}));
import {INVENTORY_RD_COUNT_SOURCE_ROW,INVENTORY_RD_SERVICE_INITIAL_COUNT_ROW,nativeInventoryReadFirst} from '../../../packages/server-inventory/src/adapters/native-inventory-read';
import {serviceInitialCountExists} from '../../../packages/server-inventory/src/adapters/native-service-count-source';

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

const SERVICE_SQL='SELECT 1 FROM inventory_service_initial_physical_sources WHERE count_id=? AND company_id=?';

describe('native Inventory service physical-count source probe',()=>{
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
  const fakeDb=(rows:unknown[])=>{
    const bound:unknown[][]=[];
    const db={
      prepare(_sql:string){return {bind(...values:unknown[]){bound.push(values);return this;}};},
      async batch(){return [{results:rows,success:true,meta:{changes:0,duration:0}}];},
      async exec(){return {count:0,duration:0};},
    };
    return {db,bound};
  };
  const stubRead=(rows:unknown[])=>{
    const bodies:{kind:string;values:unknown[]}[]=[];
    vi.stubGlobal('fetch',vi.fn(async (_url:URL,init:RequestInit)=>{
      const body=JSON.parse(String(init.body)) as {kind:string;values:unknown[]};
      bodies.push(body);
      return new Response(JSON.stringify({data:{statements:[{sql:SERVICE_SQL,values:body.values}]}}),{status:200,headers:{'content-type':'application/json'}});
    }));
    return bodies;
  };

  it('sends only the owner probe kind and scoped binds, returning row presence',async()=>{
    const bodies=stubRead([{1:1}]);
    const {db,bound}=fakeDb([{1:1}]);
    expect(await serviceInitialCountExists(db as never,'count-1','company')).toBe(true);
    expect(bodies.map(x=>x.kind)).toEqual(['inventory_rd_service_initial_count_row']);
    expect(bodies.map(x=>x.values)).toEqual([['count-1','company']]);
    expect(bound).toEqual([['count-1','company']]);
    expect(bodies.every(x=>!Object.values(x).some(v=>String(v).match(/SELECT|FROM inventory_/i)))).toBe(true);
    const empty=stubRead([]),{db:emptyDb}=fakeDb([]);
    expect(await serviceInitialCountExists(emptyDb as never,'count-2','company')).toBe(false);
    expect(empty.map(x=>x.kind)).toEqual([INVENTORY_RD_SERVICE_INITIAL_COUNT_ROW]);
  });

  it('fails closed when the native read service is unavailable',async()=>{
    vi.stubGlobal('fetch',vi.fn(async ()=>new Response('down',{status:503})));
    const {db}=fakeDb([]);
    await expect(serviceInitialCountExists(db as never,'count-1','company')).rejects.toMatchObject({status:503});
  });

  it('deleted the TS probe literal from both consumers and pinned the Rust plan',()=>{
    const tails=readFileSync(`${root}/packages/server-inventory/src/adapters/read-tail-sql.ts`,'utf8');
    expect(tails).not.toContain(SERVICE_SQL);
    for(const file of ['service-initial-physical-stock-count-source.ts','count-attachment-purchases.ts']){
      const source=readFileSync(`${root}/packages/server-inventory/src/adapters/${file}`,'utf8');
      expect(source).not.toContain('serviceInitialPhysicalNativeRowSql');
      expect(source).toContain('serviceInitialCountExists');
    }
    const rust=readFileSync(`${root}/packages/server-inventory/src/adapters/native_command_sql.rs`,'utf8');
    expect(rust).toContain(`"${SERVICE_SQL}"`);
    expect(rust).toContain('pub const RD_SERVICE_INITIAL_COUNT_ROW: &str = "inventory_rd_service_initial_count_row";');
    expect(rust).toMatch(/const READ_SERVICE_INITIAL_COUNT_ROW: NativeStatement = NativeStatement \{[\s\S]*?bind_count: 2,/);
  });
});
