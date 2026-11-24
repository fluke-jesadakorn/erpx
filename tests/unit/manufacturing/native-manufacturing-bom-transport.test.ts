import {describe,it,expect,vi} from 'vitest';
import * as nodeFs from 'node:fs';
import * as nodePath from 'node:path';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers()}));
import {
  createNativeManufacturingBomClient,
  nativeManufacturingAvailable,
  nativeManufacturingConfiguration,
  NativeManufacturingError,
  type NativeManufacturingPageContext,
} from '../../../packages/feature-manufacturing/src/public/native-manufacturing-bom';
import { nativeManufacturingFailure } from '../../../packages/feature-manufacturing/src/public/native-manufacturing-bom-page';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeManufacturingPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['manufacturing'],iam:{permissions:['records.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['manufacturing'],permissions:['records.read'],expiresAt:9_007_199_254_740_991};
const line={id:'line-a',specification_id:'spec',product_id:'material-a',
  quantity_milli:1500,substitution_group:null,ordinal:0};
const operation={id:'operation-a',specification_id:'spec',resource_id:'resource',
  resource_name:'Assembly line',name:'Assemble',setup_minutes:5,
  run_minutes_per_batch:30,ordinal:0};
const draft={id:'spec',company_id:'company',name:'Bounded assembly',mode:'bom',
  output_product_id:'output',batch_quantity_milli:10000,expected_yield_bps:9800,
  status:'draft',created_by:'maker',reviewed_by:null,version:1,created_at:100,
  updated_at:100,lines:[line],operations:[operation]};
const data={specifications:[draft]};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Manufacturing BOM register transport',()=>{
 it('forwards the configured Holding, session, CAS and workspace to the owner read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data,context}));
  const client=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),transport);
  const result=await client.bom(incoming(),actor,input);
  expect(result).toEqual(data);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/manufacturing/specifications');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace slug as context CAS input',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data,context:branchContext}));
  const client=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),transport);
  await client.bom(incoming(),branchActor,{...input,branchSlug:'north'});
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/manufacturing/specifications');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'MANUFACTURING_BOM_SOURCE_INVALID',message:'Reload the current BOM.'}}),
    {status:503,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),transport);
  await expect(client.bom(incoming(),actor,input)).rejects.toMatchObject({
    code:'MANUFACTURING_BOM_SOURCE_INVALID',status:503,
    message:'Reload the current BOM.',
  });
 });

 it('rejects an unconfigured host, changed context and malformed responses before returning data',async()=>{
  const client=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn());
  await expect(client.bom(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_AUTHORITY_INVALID'});
  const changed=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data,context:{...context,revision:8}}),
  ));
  await expect(changed.bom(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_CONTEXT_CHANGED'});
  const malformed=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{specifications:[{...draft,extra:'field'}]},context}),
  ));
  await expect(malformed.bom(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_RESPONSE_INVALID'});
  const foreign=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{specifications:[{...draft,company_id:'foreign'}]},context}),
  ));
  await expect(foreign.bom(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_RESPONSE_INVALID'});
  const duplicateLine=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{specifications:[{...draft,lines:[line,{...line,product_id:'material-b',ordinal:1}]}]},context}),
  ));
  await expect(duplicateLine.bom(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_RESPONSE_INVALID'});
  const orphanLine=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{specifications:[{...draft,lines:[{...line,specification_id:'other'}]}]},context}),
  ));
  await expect(orphanLine.bom(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_RESPONSE_INVALID'});
  const selfReviewed=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{specifications:[{...draft,status:'approved',reviewed_by:'maker',version:2}]},context}),
  ));
  await expect(selfReviewed.bom(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_RESPONSE_INVALID'});
  const materialIsOutput=createNativeManufacturingBomClient(nativeManufacturingConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{specifications:[{...draft,lines:[{...line,product_id:'output'}]}]},context}),
  ));
  await expect(materialIsOutput.bom(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MANUFACTURING_RESPONSE_INVALID'});
 });

 it('fails closed when the native Manufacturing configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_MANUFACTURING_CONFIG;
  delete process.env.ERPX_NATIVE_MANUFACTURING_CONFIG;
  expect(nativeManufacturingAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_MANUFACTURING_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeManufacturingBomClient(nativeManufacturingConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeManufacturingBomClient(nativeManufacturingConfiguration(config({privateBaseUrl:'http://10.0.0.1:44993'})),vi.fn())).toThrow();
  expect(()=>createNativeManufacturingBomClient(nativeManufacturingConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeManufacturingConfiguration('{not-json')).toThrow();
  expect(()=>createNativeManufacturingBomClient(nativeManufacturingConfiguration(config({
    holdings:[{origin:'https://acme.example.com:3443',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).not.toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativeManufacturingFailure(new NativeManufacturingError('NATIVE_MANUFACTURING_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeManufacturingFailure(new NativeManufacturingError('NATIVE_MANUFACTURING_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'MANUFACTURING_REQUEST_INVALID'});
  expect(nativeManufacturingFailure(new NativeManufacturingError('NATIVE_MANUFACTURING_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_MANUFACTURING_UNAVAILABLE'});
  expect(nativeManufacturingFailure(new NativeManufacturingError('MANUFACTURING_BOM_SOURCE_INVALID',503,'Reload current BOM.')))
    .toMatchObject({status:503,code:'NATIVE_MANUFACTURING_UNAVAILABLE'});
  expect(nativeManufacturingFailure(new Error('other'))).toBeUndefined();
 });
});
describe('Manufacturing finance-close read retirement',()=>{
  const src=new URL('../../../packages/server-manufacturing/src/',import.meta.url).pathname;
  const files=(dir:string):string[]=>nodeFs.readdirSync(dir).flatMap(name=>{const path=nodePath.join(dir,name);return nodeFs.statSync(path).isDirectory()?files(path):path.endsWith('.ts')?[path]:[];});
  it('removed the four close-source SELECT modules and keeps the owner SELECT KPI strictly below 148',()=>{
    const paths=files(src);
    expect(paths.some(path=>path.includes('finance-close-source'))).toBe(false);
    const count=paths.reduce((count,path)=>count+(nodeFs.readFileSync(path,'utf8').match(/\bSELECT\b/g)?.length??0),0);
    expect(count).toBeLessThan(148);
    expect(count).toBe(129);
  });
});
describe('Manufacturing owner-local report-actor read dedupe',()=>{
  const source=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/adapters/production-command-sources.ts',import.meta.url),'utf8');
  const registry=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/public/native_command.rs',import.meta.url),'utf8');
  const sql="SELECT actor_id FROM manufacturing_order_events WHERE order_id=? AND action='report' ORDER BY version DESC LIMIT 1";
  it('keeps one fixed-SQL definition and switches both QC callers to it',()=>{
    expect(source).toContain('const latestReportActorSelect');
    expect(source).toContain('const reportActorSelect=(suffix:string)=>');
    expect(source).toContain('reportActorSelect(" ORDER BY version DESC LIMIT 1")');
    expect(source).toContain('reportActorSelect(" AND version=?")');
    for(const method of ['lastReportActor','lastOutputReportActor']){
      const at=source.indexOf(`async ${method}`);
      const body=source.slice(at,at+260);
      expect(body).toContain('.prepare(latestReportActorSelect)');
      expect(body).toContain('.bind(id)');
      expect(body).toContain('.first<string>("actor_id")');
    }
  });
  it('stays owner-local, outside the RD_* registry and free of fail-closed markers',()=>{
    expect(sql).toContain('FROM manufacturing_order_events');
    expect(sql).not.toContain('JOIN');
    expect(registry).not.toContain(sql);
    expect(registry).not.toContain('report-actor');
    for(const method of ['lastReportActor','lastOutputReportActor']){
      const at=source.indexOf(`async ${method}`);
      expect(source.slice(at,at+260)).not.toMatch(/503|UNAVAILABLE|fail closed/i);
    }
  });
  it('runs the identical live statement and binds on both QC callers',async()=>{
    const {productionCommandSources}=await import('../../../packages/server-manufacturing/src/adapters/production-command-sources');
    const prepares:string[]=[],binds:unknown[][]=[];
    const db={prepare(statement:string){prepares.push(statement);return {bind(...values:unknown[]){binds.push(values);return {first:async()=>'reporter'};}};}} as never;
    const sources=productionCommandSources(db);
    expect(await sources.lastReportActor('order-1')).toBe('reporter');
    expect(await sources.lastOutputReportActor('order-1')).toBe('reporter');
    expect(prepares).toEqual([sql,sql]);
    expect(binds).toEqual([['order-1'],['order-1']]);
  });
});
describe('Manufacturing command-receipt replay read dedupe',()=>{
  const purchases=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/adapters/purchases.ts',import.meta.url),'utf8');
  const posting=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/adapters/posting-profiles.ts',import.meta.url),'utf8');
  const registry=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/public/native_command.rs',import.meta.url),'utf8');
  const sql="SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
  it('keeps one fixed-SQL definition and switches both command paths to it',()=>{
    expect(purchases).toContain('export const commandReceiptSelect');
    expect(purchases.split(sql)).toHaveLength(2);
    expect(posting.split(sql)).toHaveLength(1);
    expect(posting).toContain("commandReceiptSelect} from '@erpx/server-manufacturing/adapters/purchases'");
    for(const source of [purchases,posting]){
      expect(source).toContain('.prepare(commandReceiptSelect)');
      expect(source).not.toContain('.prepare("SELECT 1 FROM command_receipts');
      expect(source).not.toContain(".prepare('SELECT 1 FROM command_receipts");
    }
  });
  it('stays owner-local, outside the RD_* registry and free of fail-closed markers',()=>{
    expect(sql).toContain('FROM command_receipts');
    expect(sql).not.toContain('JOIN');
    expect(registry).not.toContain(sql);
    for(const [source,method] of [[purchases,'withManufacturingPurchasedCommand'],[posting,'withManufacturingPostingProfileCommand']] as const){
      const at=source.indexOf(method);
      expect(source.slice(at,at+900)).not.toMatch(/fail closed/i);
    }
  });
  it('keeps the identical live statement and binds on both command paths',async()=>{
    const {commandReceiptSelect}=await import('../../../packages/server-manufacturing/src/adapters/purchases');
    expect(commandReceiptSelect).toBe(sql);
    expect(purchases).toContain('.bind(command.key,command.operation,command.target.id,actor.userId,actor.companyId)');
    expect(posting).toContain('.bind(key,operation,target,actor.userId,actor.companyId)');
  });
});
describe('Manufacturing executive-metric branch-label read dedupe',()=>{
  const source=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/adapters/executive-metric-projections.ts',import.meta.url),'utf8');
  const registry=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/public/native_command.rs',import.meta.url),'utf8');
  const fragment="COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Branch')";
  it('keeps one fixed-SQL definition and switches both branch breakdowns to it',()=>{
    expect(source).toContain('const branchDisplayLabel');
    expect(source.split(fragment)).toHaveLength(2);
    expect(source.split('breakdown === "branch" ? branchDisplayLabel')).toHaveLength(3);
  });
  it('stays owner-local, outside the RD_* registry and free of fail-closed markers',()=>{
    expect(fragment).toContain('FROM (${organizationProjectionSources.organization_nodes})');
    expect(registry).not.toContain(fragment);
    expect(source).not.toMatch(/503|UNAVAILABLE|fail closed/i);
  });
  it('keeps the live fragment interpolated into both caller queries on the caller snapshot',()=>{
    for(const needle of ['SELECT ${label} label,count(*)','SELECT ${label} label,COALESCE(sum(m.amount_minor']){
      expect(source).toContain(needle);
    }
    expect(source).toContain('sourceDb.prepare(sql).bind(...values)');
  });
});
describe('Manufacturing specification-source guard dedupe',()=>{
  const source=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/adapters/production-command-sources.ts',import.meta.url),'utf8');
  const registry=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/public/native_command.rs',import.meta.url),'utf8');
  const sql="EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved')";
  it('keeps one fixed-SQL definition and switches both specification-source guards to it',()=>{
    expect(source).toContain('const approvedSpecGuardSql');
    expect(source.split(sql)).toHaveLength(2);
    for(const [method,args] of [['guardScenarioSpecification','[specificationId, companyId, specificationVersion]'],['guardRecommendationSpecification','[value2, companyId, value3]']] as const){
      const at=source.indexOf(method);
      const body=source.slice(at,at+320);
      expect(body).toContain('assertion(db, key, value, approvedSpecGuardSql,');
      expect(body).toContain(args);
    }
  });
  it('stays owner-local, outside the RD_* registry and free of fail-closed markers',()=>{
    expect(sql).toContain("FROM manufacturing_specifications WHERE id=? AND company_id=? AND version=? AND status='approved'");
    expect(sql).not.toContain('JOIN');
    expect(registry).not.toContain(sql);
    expect(registry).not.toContain('approvedSpecGuardSql');
    for(const method of ['guardScenarioSpecification','guardRecommendationSpecification']){
      const at=source.indexOf(method);
      expect(source.slice(at,at+320)).not.toMatch(/503|UNAVAILABLE|fail closed/i);
    }
  });
  it('keeps the identical live statement on both guards with their own binds',()=>{
    expect(source).not.toMatch(/\.prepare\(("|')EXISTS\(SELECT 1 FROM manufacturing_specifications/);
    expect(source.split('assertion(db, key, value, approvedSpecGuardSql,')).toHaveLength(3);
  });
});
describe('Manufacturing independent-QC guard dedupe',()=>{
  const source=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/adapters/production-command-sources.ts',import.meta.url),'utf8');
  const registry=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/public/native_command.rs',import.meta.url),'utf8');
  const sql="EXISTS(SELECT 1 FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?)";
  it('keeps one fixed-SQL definition and switches both independent-QC guards to it',()=>{
    expect(source).toContain('const independentQcGuardSql');
    expect(source.split(sql)).toHaveLength(2);
    for(const method of ['guardIndependentPass','guardIndependentFail']){
      const at=source.indexOf(method);
      const body=source.slice(at,at+280);
      expect(body).toContain('assertion(db, key, value, independentQcGuardSql,');
      expect(body).toContain('[id, expectedVersion, userId]');
    }
  });
  it('stays owner-local, outside the RD_* registry and free of fail-closed markers',()=>{
    expect(sql).toContain("FROM manufacturing_order_events WHERE order_id=? AND action='report' AND version=? AND actor_id<>?");
    expect(sql).not.toContain('JOIN');
    expect(registry).not.toContain(sql);
    expect(registry).not.toContain('independentQcGuardSql');
    for(const method of ['guardIndependentPass','guardIndependentFail']){
      const at=source.indexOf(method);
      expect(source.slice(at,at+280)).not.toMatch(/503|UNAVAILABLE|fail closed/i);
    }
  });
  it('keeps the identical live statement on both guards with their own binds',()=>{
    expect(source).not.toMatch(/\.prepare\(("|')EXISTS\(SELECT 1 FROM manufacturing_order_events/);
    expect(source.split('assertion(db, key, value, independentQcGuardSql,')).toHaveLength(3);
  });
});
describe('Manufacturing operation-event done guard dedupe',()=>{
  const source=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/adapters/production-command-sources.ts',import.meta.url),'utf8');
  const registry=nodeFs.readFileSync(new URL('../../../packages/server-manufacturing/src/public/native_command.rs',import.meta.url),'utf8');
  const sql="AND NOT EXISTS(SELECT 1 FROM manufacturing_operation_events e WHERE e.order_id=? AND e.operation_id=op.id))";
  it('keeps one fixed-SQL definition and interpolates it into both routing guards',()=>{
    expect(source).toContain('const operationEventDoneGuardSql =');
    expect(source.split(sql)).toHaveLength(2);
    expect(source.split('${operationEventDoneGuardSql}')).toHaveLength(3);
    for(const method of ['guardCompleteRouting','guardOperationSequence']){
      const at=source.indexOf(method);
      const body=source.slice(at,at+360);
      expect(body).toContain('${operationEventDoneGuardSql}');
    }
  });
  it('stays owner-local, outside the RD_* registry and free of fail-closed markers',()=>{
    expect(sql).toContain('FROM manufacturing_operation_events');
    expect(sql).not.toContain('JOIN');
    expect(registry).not.toContain(sql);
    expect(registry).not.toContain('operationEventDoneGuardSql');
    for(const method of ['guardCompleteRouting','guardOperationSequence']){
      const at=source.indexOf(method);
      expect(source.slice(at,at+360)).not.toMatch(/503|UNAVAILABLE|fail closed/i);
    }
  });
  it('keeps the live statement on both guards with their own binds',()=>{
    expect(source.split('${operationEventDoneGuardSql}')).toHaveLength(3);
    expect(source.slice(source.indexOf('guardCompleteRouting'),source.indexOf('guardCompleteRouting')+360)).toContain('[value2, id]');
    expect(source.slice(source.indexOf('guardOperationSequence'),source.indexOf('guardOperationSequence')+360)).toContain('[value2, value3, id]');
  });
});
describe('Manufacturing read-tail text-only fragment dedupe', () => {
  const adapter = (name: string) => nodeFs.readFileSync(new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url), 'utf8');
  const opSpec = '(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_specification_operations operation WHERE operation.specification_id=specification.id)';
  const opEvents = '(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_operation_events operation WHERE operation.order_id=record.id)';
  const opProd = '(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_specification_operations operation WHERE operation.specification_id=s.id)';
  const asFile = 'EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(production_file_scope.module_id)))';
  const asSource = 'EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(production_source.module_id)))';
  const lineSpec = '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM manufacturing_specification_lines line WHERE line.specification_id=specification.id)';
  const lineS = '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM manufacturing_specification_lines line WHERE line.specification_id=s.id)';
  const src = 'MANUFACTURING_PRODUCT_SOURCES';
  const prodSpec = `(SELECT jsonb_agg(to_jsonb(product) ORDER BY product.id) FROM (${src}) product WHERE product.company_id=specification.company_id AND (product.id=specification.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=specification.id)))`;
  const prodS = `(SELECT jsonb_agg(to_jsonb(product) ORDER BY product.id) FROM (${src}) product WHERE product.company_id=s.company_id AND (product.id=s.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=s.id)))`;
  const cteOrders = 'orders AS MATERIALIZED(SELECT o.* FROM manufacturing_orders o,scope WHERE o.company_id=scope.company_id)';
  const ctePostings = 'postings AS MATERIALIZED(SELECT p.* FROM manufacturing_order_postings p,scope WHERE p.company_id=scope.company_id)';
  const cteMovements = 'movements AS MATERIALIZED(SELECT m.* FROM manufacturing_cost_movements m,scope WHERE m.company_id=scope.company_id)';
  it('emits the former inline projection and guard text byte for byte', async () => {
    const f = await import('../../../packages/server-manufacturing/src/adapters/manufacturing-read-sql-fragments');
    expect(f.manufacturingOperationAggSql('manufacturing_specification_operations operation', 'WHERE operation.specification_id=specification.id')).toBe(opSpec);
    expect(f.manufacturingOperationAggSql('manufacturing_operation_events operation', 'WHERE operation.order_id=record.id')).toBe(opEvents);
    expect(f.manufacturingOperationAggSql('manufacturing_specification_operations operation', 'WHERE operation.specification_id=s.id')).toBe(opProd);
    expect(f.manufacturingSelectedAssignmentExists('production_file_scope.module_id')).toBe(asFile);
    expect(f.manufacturingSelectedAssignmentExists('production_source.module_id')).toBe(asSource);
    expect(f.manufacturingSpecificationLineAggSql('specification')).toBe(lineSpec);
    expect(f.manufacturingSpecificationLineAggSql('s')).toBe(lineS);
    expect(f.manufacturingSpecificationProductsAggSql('specification', src)).toBe(prodSpec);
    expect(f.manufacturingSpecificationProductsAggSql('s', src)).toBe(prodS);
    expect(f.manufacturingScopedCte('orders', 'manufacturing_orders', 'o')).toBe(cteOrders);
    expect(f.manufacturingScopedCte('postings', 'manufacturing_order_postings', 'p')).toBe(ctePostings);
    expect(f.manufacturingScopedCte('movements', 'manufacturing_cost_movements', 'm')).toBe(cteMovements);
  });
  it('keeps one text-only definition and routes every caller through it', () => {
    const fragment = adapter('manufacturing-read-sql-fragments.ts');
    expect(fragment.match(/\bSELECT\b/g)).toHaveLength(6);
    expect(fragment).not.toMatch(/^import /m);
    expect(fragment).not.toMatch(/compositionRoot|registerRead|readPlanRegistry/);
    for (const [name, old, helper] of [
      ['attachment-read-sources.ts', [opSpec, opEvents, lineSpec, prodSpec.replaceAll(src, '${manufacturingProductSources}')], 'manufacturingOperationAggSql'],
      ['purchases.ts', [opProd, lineS, prodS.replaceAll(src, '${manufacturingProductSources}')], 'manufacturingOperationAggSql'],
      ['attachment-purchases.ts', [asFile], 'manufacturingSelectedAssignmentExists'],
      ['record-purchases.ts', [asSource], 'manufacturingSelectedAssignmentExists'],
      ['posting-setup-sources.ts', [cteOrders, ctePostings, cteMovements], 'manufacturingScopedCte'],
    ] as const) {
      const text = adapter(name);
      for (const literal of old) expect(text).not.toContain(literal);
      expect(text).toContain(helper);
    }
    for (const helper of ['manufacturingSpecificationLineAggSql', 'manufacturingSpecificationProductsAggSql']) {
      expect(adapter('attachment-read-sources.ts')).toContain(helper);
      expect(adapter('purchases.ts')).toContain(helper);
    }
  });
});
