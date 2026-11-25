import {describe,expect,test} from 'vitest';
import {createQualityApplication,type QualityRepository} from "@erpx/server-quality/application/quality";
import {createQualityAlertApplication,type QualityAlertRepository} from "@erpx/server-quality/application/alerts";
import type {QualityRow} from "@erpx/server-quality/domain/quality";
import type {QualityAlert} from "@erpx/server-quality/domain/alerts";

function fixture(overrides:Partial<QualityRepository<string[]>>={}){
 const calls:string[]=[];
 const repository:QualityRepository<string[]>={
  async receiptHold(input,now){return [JSON.stringify({input,now})];},...overrides,
 };
 const ports={departmentScope:false,companyScope:true,userId:'manager',authorize(permission:string){calls.push(permission);},can:(permission:string)=>Boolean(permission),samePerson:(id:string|number|null)=>id==='manager',clock:{now:()=>42},hash:async()=> '1234567890abcdef',async repository(){return repository;}};
 return {ports,repository,calls,app:createQualityApplication(ports)};
}
describe('Quality receipt-sourced quarantine',()=>{
 test('receipt-created partial quarantine stages on the supplied repository without a second receipt',async()=>{const f=fixture(),input={id:'hold',key:'key',branchId:'branch',warehouseId:'warehouse',productId:'product',quantityMilli:1000,reason:'Received product needs inspection',dueDate:'2026-10-07',sourceId:'receipt',sourceType:'customer_return' as const};expect(JSON.parse((await f.app.receiptQualityHoldSteps(input))[0])).toEqual({input,now:42});expect(f.calls).toEqual([]);});
});
// The independent-inspector and active-status evidence assertions formerly in
// this block are covered by the native Quality owner decisions
// (tests/unit/quality/rust-quality-decisions.test.rs), not by a TS read branch.
const alert:QualityAlert={id:'alert',hold_id:'hold',phase:'overdue',source_version:1,hold_version:2,due_date:'2026-10-06',detected_at:42,version:1,acknowledged_by:null,acknowledged_at:null,reason:null,company_id:'company',branch_id:'branch',status:'held',product_name:'Product',warehouse_name:'Warehouse',can_ack:1,recipients:1};
describe('Quality alert recovery',()=>{
 test('completed and recipient-revoked escalations retain their source but cannot act',async()=>{const repository:QualityAlertRepository={async alerts(){return [{...alert,can_ack:0,recipients:0},{...alert,id:'released',status:'released'}];}};const app=createQualityAlertApplication({departmentScope:false,authorizeRead(){},can:()=>true,async repository(){return repository;}});expect(await app.qualityAlertWork()).toEqual([expect.objectContaining({queue:'waiting',canAct:false,blocker:expect.stringContaining('No eligible Quality recipient'),source:expect.objectContaining({id:'hold',version:2})}),expect.objectContaining({queue:'completed',canAct:false,status:'acknowledged'})]);});
});

import {afterEach,vi} from 'vitest';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {qualityCommandAllowed,type QualityPurchaseControls} from '@erpx/feature-quality/lib/quality-purchase-controls';
import {createQualityCommandFetch,qualityFormUnchanged,validQualityAcknowledgement} from '@erpx/feature-quality/lib/quality-command-fetch';
import {createQualityCommandAttempt,type QualityCommandAttempt} from '@erpx/feature-quality/lib/quality-command-attempt';
import {QualityPurchaseProvider} from '@erpx/feature-quality/components/quality-purchase-context';
import {QualityConsole} from '@erpx/feature-quality/components/quality-console';
vi.mock('next/navigation',()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn()}),usePathname:()=>'/holdings/acme/quality',useSearchParams:()=>new URLSearchParams()}));
afterEach(()=>vi.unstubAllGlobals());
const activeQuality:QualityPurchaseControls={newWork:true,settleExisting:true,history:true,revision:'opaque'},drainingQuality={...activeQuality,newWork:false},historyQuality={...drainingQuality,settleExisting:false};
test('Quality keeps manual/spec work active while existing source inspection, independent decisions and cancellation settle',()=>{
 for(const path of ['', '/specifications','/specifications/spec'])expect(qualityCommandAllowed(drainingQuality,path)).toBe(false);
 expect(qualityCommandAllowed(drainingQuality,'/specifications/spec',{action:'activate'})).toBe(false);expect(qualityCommandAllowed(drainingQuality,'/specifications/spec',{action:'cancel'})).toBe(true);
 for(const path of ['/hold/inspect','/hold/release','/hold/disposition','/hold/cancel','/alert/acknowledge']){expect(qualityCommandAllowed(drainingQuality,path)).toBe(true);expect(qualityCommandAllowed(historyQuality,path)).toBe(false);}
 expect(qualityCommandAllowed(activeQuality,'/hold/foreign-action')).toBe(false);expect(qualityCommandAllowed(undefined,'')).toBe(true);
});
test('Quality uncertain acknowledgement keeps exact key until a parsed actual id and positive version',async()=>{
 const keys:string[]=[],request=createQualityCommandFetch(async(_url,init)=>{keys.push(new Headers(init?.headers).get('Idempotency-Key')!);if(keys.length===1)throw Error('Lost inspection response');if(keys.length===2)return new Response('truncated');if(keys.length===3)return Response.json({data:{id:'hold'}});if(keys.length===4)return Response.json({data:{id:'hold',version:0}});return Response.json({data:{id:'hold',version:2}});}),input={method:'POST',body:'{"expectedVersion":1,"result":"fail"}'};
 await expect(request('/quality/hold/inspect',input)).rejects.toThrow('Lost inspection response');for(let i=0;i<3;i++)await request('/quality/hold/inspect',input);expect(request.hasAttempt('/quality/hold/inspect',input.body)).toBe(true);await request('/quality/hold/inspect',input);expect(request.hasAttempt('/quality/hold/inspect',input.body)).toBe(false);await request('/quality/hold/inspect',input);expect(new Set(keys.slice(0,5)).size).toBe(1);expect(keys[5]).not.toBe(keys[0]);
});
test('Quality command identities separate Company, role assignment and changed physical evidence',async()=>{
 const dataset={workUser:'inspector',workRevision:'1',workCompany:'A',workAssignment:'inspection-A'};vi.stubGlobal('document',{querySelector:()=>({dataset})});const keys:string[]=[],request=createQualityCommandFetch(async(_url,init)=>{keys.push(new Headers(init?.headers).get('Idempotency-Key')!);return new Response('unavailable',{status:503});});await request('/inspect',{body:'submitted'});await request('/inspect',{body:'submitted'});await request('/inspect',{body:'newer evidence'});dataset.workAssignment='manager-A';await request('/inspect',{body:'submitted'});dataset.workCompany='B';await request('/inspect',{body:'submitted'});expect(keys[1]).toBe(keys[0]);expect(new Set([keys[0],...keys.slice(2)]).size).toBe(4);
});
test('Quality unknown physical outcome blocks newer drafts and reuses the original captured inspection revision',()=>{
 const commands=createQualityCommandAttempt(),completion={form:{} as HTMLFormElement,formEditVersion:'original'},original:QualityCommandAttempt={path:'/hold/inspect',body:'submitted original measurements',context:'company-inspector',create:false,completion};commands.unresolved(original);expect(()=>commands.select({...original,body:'changed measurement'},original.context)).toThrow('Retry the submitted quality change first');expect(()=>commands.select(original,'company-manager')).toThrow('workspace changed');expect(commands.select({...original,completion:{...completion,formEditVersion:'later'}},original.context)).toBe(original);expect(commands.retry()?.completion?.formEditVersion).toBe('original');commands.resolved(original);expect(commands.select({...original,body:'changed measurement'},original.context).body).toBe('changed measurement');
});
test('Quality parsed acknowledgement leaves newer measurements or evidence dirty',async()=>{
 const form={isConnected:true,getAttribute:()=> 'later-measurement'} as unknown as HTMLFormElement,completion={form,formEditVersion:'submitted'},events:Event[]=[];vi.stubGlobal('window',{dispatchEvent:(event:Event)=>{events.push(event);return true;}});await createQualityCommandFetch(async()=>Response.json({data:{id:'hold',version:2}}))('/inspect',{body:'original'},completion);const saved=events.find(event=>event.type==='erpx:record-command-saved') as CustomEvent;expect(saved.detail.qualityAcknowledged).toBe(true);expect(saved.detail.formEditVersion).toBe('submitted');expect(qualityFormUnchanged(saved.detail)).toBe(false);for(const value of [null,[],{id:''},{id:'hold'},{id:'hold',version:0},{id:'hold',version:1.5}])expect(validQualityAcknowledgement(value)).toBe(false);
});
const uiDecision={canInspect:true,canRelease:false,canCancel:true,canReviewCancellation:false,canDisposition:false};
const uiHold={id:'hold',companyId:'company',branchId:'branch',warehouseId:'warehouse',productId:'product',lotId:null,
 reason:'Independent review of the received stock',dueDate:'2026-10-06',status:'held',createdBy:'creator',
 lastInspector:null,lastResult:null,releasedBy:null,releaseReason:null,releasedAt:null,version:1,createdAt:1,
 updatedAt:1,specId:null,specVersion:null,quantityScope:'partial',heldQuantityMilli:2000,sourceType:'manual',
 sourceId:null,cancelRequestedBy:null,cancelReason:null,cancelledBy:null,cancelledAt:null,dispositionKind:null,
 dispositionReason:null,dispositionRequestedBy:null,dispositionCountId:null,dispositionQuantityMilli:null,
 disposedBy:null,disposedAt:null,inspectionCount:0,productName:'Product',productUnit:'unit',trackingMode:'none',
 warehouseName:'Warehouse',lotCode:null,serialNumber:null,expiryDate:null,eligibleCount:1,decision:uiDecision,
 nextAction:'Record an independent inspection'};
const uiWorkspace={holds:[],stock:[],lots:[],products:[],specifications:[],specMetrics:[],canHold:true,canManageSpecs:true};
const uiDetail={hold:uiHold,inspections:[],dispositions:[],specMetrics:[],measurements:[],alerts:[],cancellationHistory:'paged' as const};
function renderQuality(props:Parameters<typeof QualityConsole>[0],purchase?:QualityPurchaseControls){return renderToStaticMarkup(createElement(QualityPurchaseProvider,{purchase},createElement(QualityConsole,props)));}
test('Quality current-page long manual/spec/inspection editors disclose guarded drawers with no initial inline fields',()=>{
 const workspace=renderQuality({base:'/holdings/acme',data:uiWorkspace},activeQuality);expect(workspace).toContain('Quarantine stock');expect(workspace).toContain('Prepare product inspection specification');expect(workspace).not.toContain('name="metrics"');expect(workspace).not.toContain('name="dueDate"');const source=renderQuality({base:'/holdings/acme',detail:uiDetail},drainingQuality);expect(source).toContain('Record inspection');expect(source).toContain('Request cancellation');expect(source).not.toContain('name="evidence"');
});
test('Quality retained history suppresses mutation tools while original independent role flags still decide eligible source actions',()=>{
 expect(renderQuality({base:'/holdings/acme',data:uiWorkspace},drainingQuality)).not.toContain('Quarantine stock');const history=renderQuality({base:'/holdings/acme',detail:uiDetail},historyQuality);expect(history).not.toContain('Record inspection');expect(history).not.toContain('Request cancellation');expect(renderQuality({base:'/holdings/acme',detail:{...uiDetail,hold:{...uiHold,decision:{...uiDecision,canInspect:false,canCancel:false}}}},activeQuality)).not.toContain('Record inspection');for(const secret of ['company_package_grants','assignment_permissions','source_witness'])expect(history).not.toContain(secret);
});

import {qualitySourceEntry} from "@erpx/server-quality/public/rules/product-entry";
describe('existing Quality/Count product source destinations',()=>{
 test('parses only canonical exact source detail identities',()=>{
  expect(qualitySourceEntry('inventory-quality/quality/quality-return-123-0')).toEqual({kind:'hold',id:'quality-return-123-0'});
  expect(qualitySourceEntry('inventory-quality/counts/quality-scrap-123/overview')).toEqual({kind:'count',id:'quality-scrap-123'});
  for(const path of ['inventory-quality/quality','inventory-quality/counts','inventory-quality/counts/new','inventory-quality/quality/..','inventory-quality/quality/id/edit','inventory-quality/quality/id?companyId=other','inventory-quality/quality/id#extra','inventory-quality/quality/id\\escape','inventory-quality/quality/id\u0000','quality/id','inventory-quality/transfers/id'])expect(qualitySourceEntry(path)).toBeNull();
 });
});

import {existsSync,readFileSync,readdirSync,statSync} from 'node:fs';
import {resolve,join} from 'node:path';
describe('retired Quality stock new-work read branch',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('the quality-stock adapter, barrel and Commerce package exports are absent',()=>{
  expect(existsSync(source('packages/server-commerce/src/adapters/quality-stock.ts'))).toBe(false);
  expect(existsSync(source('packages/server-commerce/src/public/quality-stock.ts'))).toBe(false);
  expect(readFileSync(source('packages/server-commerce/package.json'),'utf8')).not.toContain('quality-stock');
 });
 test('the unreachable new-work branch and its Commerce read imports are gone from Quality purchases',()=>{
  const purchases=readFileSync(source('packages/server-quality/src/adapters/purchases.ts'),'utf8');
  expect(purchases).not.toContain('qualityStockFacts');
  expect(purchases).not.toContain('qualityProductFacts');
  expect(purchases).not.toContain('new-work');
  expect(purchases).not.toContain('current warehouse policy changed');
 });
});
describe('quality metric branch-label dedupe',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('one fixed Organization node label SELECT guards both metric breakdowns',async()=>{
  const {qualityBranchDisplayLabelSql,readExecutiveMetricProjection}=await import('@erpx/server-quality/adapters/executive-metric-projections');
  expect(qualityBranchDisplayLabelSql.match(/SELECT n\.display_name/g)).toHaveLength(1);
  expect(qualityBranchDisplayLabelSql).toContain('organization_nodes');
  const text=readFileSync(source('packages/server-quality/src/adapters/executive-metric-projections.ts'),'utf8');
  expect(text.match(/SELECT n\.display_name/g)??[]).toHaveLength(0);
  expect(text).toContain('import { scopeSql } from "./quality-scope-sql";');
  expect(text).toContain('COALESCE(${scopeSql(`n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=h.branch_id AND n.company_id=h.company_id`)},\'Company-wide\')');
  expect(text.match(/qualityBranchDisplayLabelSql/g)??[]).toHaveLength(3);
  const sql:string[]=[],db={prepare(statement:string){sql.push(statement);return {bind(){return {all:()=>({results:[]})};}};}};
  const actor={companyId:'company',branchId:'branch'} as never,filters={from:'2026-01-01',to:'2026-01-31'} as never;
  await readExecutiveMetricProjection(db as never,actor,'quality-holds',filters,'branch');
  await readExecutiveMetricProjection(db as never,actor,'inspection-pass-rate',filters,'branch');
  expect(sql).toHaveLength(2);
  for(const statement of sql){expect(statement).toContain(qualityBranchDisplayLabelSql);expect(statement.match(/SELECT n\.display_name/g)?.length).toBe(1);}
 });
});
describe('quality attachment owner version-guard dedupe',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('both CAS guards are closed native kinds with byte-exact three-bind SQL',async()=>{
  const plans=readFileSync(source('packages/server-quality/src/adapters/native_quality_read_sql.rs'),'utf8');
  for(const [kind,sql] of [
   ['quality_rd_attach_hold_cas','EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)'],
   ['quality_rd_attach_spec_cas','EXISTS(SELECT 1 FROM inventory_quality_specs WHERE id=? AND company_id=? AND version=?)'],
  ] as const){
   expect(plans).toContain(`"${kind}"`);
   expect(plans).toContain(`"${sql}"`);
   expect(sql.match(/\?/g)).toHaveLength(3);
   expect(sql.startsWith('EXISTS(SELECT 1 FROM ')).toBe(true);
  }
  expect(plans).toContain('RD_ATTACH_HOLD_CAS => Some(&READ_ATTACH_HOLD_CAS)');
  expect(plans).toContain('RD_ATTACH_SPEC_CAS => Some(&READ_ATTACH_SPEC_CAS)');
 });
 test('both attachment owner facts delegate to the native guard bridge',()=>{
  const service=readFileSync(source('packages/server-quality/src/adapters/attachments-service-facts.ts'),'utf8');
  const operational=readFileSync(source('packages/server-quality/src/adapters/attachments-operational-facts.ts'),'utf8');
  const bridge=readFileSync(source('packages/server-quality/src/adapters/native-quality-read.ts'),'utf8');
  const inline=/EXISTS\(SELECT 1 FROM \$\{[^}]+\} WHERE id=\? AND company_id=\? AND version=\?/g;
  expect(service.match(inline)??[]).toHaveLength(0);
  expect(operational.match(inline)??[]).toHaveLength(0);
  expect(service).not.toContain('qualityAttachmentVersionGuardSql');
  expect(operational).not.toContain('qualityAttachmentVersionGuardSql');
  expect(service).toContain('nativeQualityGuardAssertion(db, commandId, `attachment-owner-${index}`, RD_ATTACH_HOLD_CAS, [targetId, companyId, version])');
  expect(operational).toContain('RD_ATTACH_SPEC_CAS');
  expect(operational).toContain('nativeQualityGuardAssertion(');
  expect(bridge).toContain('export async function nativeQualityReadSql(');
  expect(bridge).toContain('export function nativeQualityGuardAssertion(');
  expect(bridge).toContain('const guard=assertion(db,commandId,step,statement.sql,statement.values as (string|number|null)[]);');
  expect(bridge).toContain('return (context?guard.withContext(context):guard).run();');
 });
});
describe('retired Quality dead read branch',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('the active-spec and evidence-version reads are gone from the Quality application, adapter and public layers',()=>{
  for(const path of ['packages/server-quality/src/application/quality.ts','packages/server-quality/src/adapters/quality.ts','packages/server-quality/src/public/quality.ts']){
   const text=readFileSync(source(path),'utf8');
   expect(text).not.toContain('activeSpec');
   expect(text).not.toContain('evidenceVersionGuard');
   expect(text).not.toContain('qualityEvidenceGuard');
  }
  const adapter=readFileSync(source('packages/server-quality/src/adapters/quality.ts'),'utf8');
  expect(adapter).not.toContain('inventory_quality_specs');
  expect(adapter).not.toContain('quality-evidence-version');
  expect(adapter).not.toContain('SELECT');
  expect(adapter).toContain('nativeQualityWork');
  expect(adapter).toContain('nativeQualityHoldDetail');
  expect(adapter).toContain('getQualityHold');
  expect(adapter).toContain('qualityWork');
 });
 test('native Quality owner decisions own the retired guard assertions',()=>{
  const decisions=readFileSync(source('tests/unit/quality/rust-quality-decisions.test.rs'),'utf8');
  expect(decisions).toContain('identity_aliases_control_all_independent_quality_decisions');
  expect(decisions).toContain('missing_detail_is_a_scoped_not_found_after_the_owner_read');
  expect(decisions).toContain('QUALITY_HOLD_NOT_FOUND');
 });
});
describe('quality attachment owner-scope reads are fully native',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 const text=(path:string)=>readFileSync(source(path),'utf8');
 test('both retained scope statements are closed native read-plan kinds',()=>{
  const registry=text('packages/server-quality/src/adapters/native_quality_read_sql.rs');
  expect(registry).toContain('pub const RD_LEGACY_ATTACHMENT_SCOPE: &str = "quality_rd_legacy_attachment_scope"');
  expect(registry).toContain('pub const RD_ATTACHMENT_SPEC_SCOPE: &str = "quality_rd_attachment_spec_scope"');
  expect(registry).toContain('SELECT company_id,branch_id branch_id,NULL department_id,version FROM inventory_quality_holds WHERE id=?');
  expect(registry).toContain('SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM inventory_quality_specs WHERE id=? AND company_id=?');
  expect(()=>text('packages/server-quality/src/adapters/quality-attachment-scope-read-sql.ts')).toThrow();
 });
 test('owner adapters emit no SQL and delegate to the native scope transport',()=>{
  const migration=text('packages/server-quality/src/adapters/attachment-migration-owner.ts');
  const policy=text('packages/server-quality/src/adapters/attachments-policy-facts.ts');
  const operational=text('packages/server-quality/src/adapters/attachments-operational-facts.ts');
  const scope=text('packages/server-quality/src/adapters/native-quality-scope.ts');
  for(const value of [migration,policy,operational,scope]){
   expect(value).not.toContain('SELECT ');
   expect(value).not.toContain('qualityAttachmentScopeReadSql');
  }
  expect(migration).toContain('RD_LEGACY_ATTACHMENT_SCOPE');
  expect(operational).toContain('RD_ATTACHMENT_SPEC_SCOPE');
  expect(scope).toContain('readNativeQualityScope');
  expect(policy).toContain('readDescriptor');
  expect(policy).toContain('readQualityHold');
  expect(policy).toContain('RD_ATTACHMENT_DESCRIPTOR');
  expect(operational).toContain('nativeQualityGuardAssertion');
 });
});
describe('quality package-obligation scope and clock CTE dedupe',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 const files=(dir:string):string[]=>readdirSync(dir).flatMap(name=>{const path=join(dir,name);return statSync(path).isDirectory()?files(path):path.endsWith('.ts')?[path]:[];});
 test('one fixed scope CTE and one fixed clock CTE back the obligation and purchase reads',async()=>{
  const {qualityPackageScopeCte,qualityClockCte}=await import('../../../packages/server-quality/src/adapters/quality-scope-sql');
  const {packageClockSql}=await import('@erpx/server-commercial/public/purchased-capability-sql');
  expect(qualityPackageScopeCte).toBe('(SELECT ?::text company_id,?::boolean full)');
  expect(qualityPackageScopeCte.match(/SELECT /g)).toHaveLength(1);
  for(const name of ['quality_source_clock','quality_entry_clock','quality_calendar_clock'])expect(qualityClockCte(name)).toBe(`WITH ${name} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`);
  const supply='packages/server-quality/src/adapters/supply-package-obligations.ts';
  const manuf='packages/server-quality/src/adapters/manufacturing-package-obligations.ts';
  for(const path of [supply,manuf]){const text=readFileSync(source(path),'utf8');expect(text).toContain('WITH scope AS${qualityPackageScopeCte}');expect(text).not.toContain('WITH scope AS(SELECT');}
  const clockSites=[['packages/server-quality/src/adapters/record-purchases.ts','quality_source_clock'],['packages/server-quality/src/adapters/product-entry.ts','quality_entry_clock'],['packages/server-quality/src/adapters/calendar-quality-facts.ts','quality_calendar_clock']] as const;
  for(const [path,name] of clockSites){const text=readFileSync(source(path),'utf8');expect(text).toContain(`qualityClockCte("${name}")`);expect(text).not.toContain(`WITH ${name} AS MATERIALIZED (SELECT`);}
 });
 test('the owner TS statement KPI stays strictly below the r17 BEFORE=75 baseline',()=>{
  const count=files(source('packages/server-quality/src')).reduce((total,path)=>total+(readFileSync(path,'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g)?.length??0),0);
  expect(count).toBeLessThan(75);
  expect(count).toBe(9);
 });
});
describe('quality command-receipt guard dedupe',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('one parameterized helper reproduces the four receipt authority/result guards',async()=>{
  const {qualityReceiptGuardSql}=await import('../../../packages/server-quality/src/adapters/quality-receipt-guard-sql');
  const cases=[
   ['hold.id','quality.hold.release','hold.company_id','version','hold.version::text','hold.released_by'],
   ['alert.id','quality.alert.acknowledge','hold.company_id','version','alert.version::text','alert.acknowledged_by'],
   ['spec.id','quality.spec.activate','spec.company_id','status',"'active'",'spec.approved_by'],
   ['spec.id','quality.spec.cancel','spec.company_id','status',"'cancelled'",undefined],
  ] as const;
  const expected=[
   "SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=hold.id AND receipt.operation='quality.hold.release' AND receipt.actor_id=hold.released_by AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=hold.version::text",
   "SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=alert.id AND receipt.operation='quality.alert.acknowledge' AND receipt.actor_id=alert.acknowledged_by AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=alert.version::text",
   "SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=spec.id AND receipt.operation='quality.spec.activate' AND receipt.actor_id=spec.approved_by AND receipt.authority_json::jsonb->>'companyId'=spec.company_id AND receipt.result_json::jsonb->>'status'='active'",
   "SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=spec.id AND receipt.operation='quality.spec.cancel' AND receipt.authority_json::jsonb->>'companyId'=spec.company_id AND receipt.result_json::jsonb->>'status'='cancelled'",
  ] as const;
  for(const [record,operation,company,field,value,actor] of cases){
   const sql=qualityReceiptGuardSql(record,operation,company,field,value,actor);
   const i=cases.findIndex(item=>item[0]===record&&item[1]===operation);
   expect(sql).toBe(expected[i]);
   expect(sql.match(/SELECT /g)).toHaveLength(1);
  }
  const adapter=readFileSync(source('packages/server-quality/src/adapters/supply-package-obligations.ts'),'utf8');
  expect(adapter.match(/qualityReceiptGuardSql\(/g)).toHaveLength(4);
  expect(adapter).not.toContain('SELECT 1 FROM command_receipts receipt');
  const guardHelper=readFileSync(source('packages/server-quality/src/adapters/quality-receipt-guard-sql.ts'),'utf8');
  expect(guardHelper.match(/SELECT /g) ?? []).toHaveLength(0);
  expect(guardHelper).toContain('import {evidenceHead} from "./inspection-evidence-source-sql";');
  expect(guardHelper).toContain("evidenceHead('1', ` FROM command_receipts receipt WHERE");
 });
});
describe('quality hold child aggregate dedupe',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('one parameterized helper reproduces the six hold-keyed child aggregates byte-for-byte',async()=>{
  const {qualityChildAggSql}=await import('../../../packages/server-quality/src/adapters/quality-child-agg-sql');
  const cases=[
   ['inventory_quality_inspections','i','hold_id','h.id'],
   ['inventory_quality_cancellations','c','hold_id','h.id'],
   ['inventory_quality_disposition_events','d','hold_id','h.id'],
   ['inventory_quality_inspections','inspection','hold_id','hold.id'],
   ['inventory_quality_cancellations','event','hold_id','hold.id'],
   ['inventory_quality_disposition_events','event','hold_id','hold.id'],
  ] as const;
  for(const [table,alias,key,parent] of cases){
   const sql=qualityChildAggSql(table,alias,key,parent);
   expect(sql).toBe(`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${alias}.${key}=${parent})`);
   expect(sql.match(/SELECT /g)).toHaveLength(1);
  }
  for(const path of ['packages/server-quality/src/adapters/attachment-read-sources.ts','packages/server-quality/src/adapters/supply-package-obligations.ts']){
   const text=readFileSync(source(path),'utf8');
   expect(text).toContain('qualityChildAggSql(');
   expect(text).not.toMatch(/jsonb_agg\(to_jsonb\((i|c|d|inspection|event)\) ORDER BY \1\.id\) FROM inventory_quality_/);
  }
 });
});
describe('quality r15 child aggregate family dedupe',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('spec-metric and inspection-joined child aggregates stay byte-identical through the shared helpers',async()=>{
  const {qualityChildAggSql,qualityJoinedChildAggSql}=await import('../../../packages/server-quality/src/adapters/quality-child-agg-sql');
  const spec=[
   ['inventory_quality_spec_metrics','m','spec_id','s.id'],
   ['inventory_quality_spec_metrics','metric','spec_id','spec.id'],
  ] as const;
  for(const [table,alias,key,parent] of spec){
   const sql=qualityChildAggSql(table,alias,key,parent);
   expect(sql).toBe(`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${alias}.${key}=${parent})`);
   expect(sql.match(/SELECT /g)).toHaveLength(1);
  }
  const joined=[
   ['inventory_quality_measurements','m','m.id','inspection_id','h.id','(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_quality_measurements m JOIN inventory_quality_inspections i ON i.id=m.inspection_id WHERE i.hold_id=h.id)'],
   ['inventory_quality_inspection_attachments','a','a.inspection_id,a.attachment_id','inspection_id','h.id','(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.inspection_id,a.attachment_id) FROM inventory_quality_inspection_attachments a JOIN inventory_quality_inspections i ON i.id=a.inspection_id WHERE i.hold_id=h.id)'],
  ] as const;
  for(const [table,alias,order,key,parent,before] of joined){
   const sql=qualityJoinedChildAggSql(table,alias,order,key,parent);
   expect(sql).toBe(before);
   expect(sql.match(/SELECT /g)).toHaveLength(1);
  }
  const attach=readFileSync(source('packages/server-quality/src/adapters/attachment-read-sources.ts'),'utf8');
  const supply=readFileSync(source('packages/server-quality/src/adapters/supply-package-obligations.ts'),'utf8');
  expect(attach.match(/qualityChildAggSql\(/g)).toHaveLength(4);
  expect(attach.match(/qualityJoinedChildAggSql\(/g)).toHaveLength(2);
  expect(supply.match(/qualityChildAggSql\(/g)).toHaveLength(4);
  expect(attach).not.toContain('jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_quality_spec_metrics');
  expect(supply).not.toContain('jsonb_agg(to_jsonb(metric) ORDER BY metric.id) FROM inventory_quality_spec_metrics');
  expect(attach).not.toContain('jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_quality_measurements');
  expect(attach).not.toContain('jsonb_agg(to_jsonb(a) ORDER BY a.inspection_id,a.attachment_id) FROM inventory_quality_inspection_attachments');
  const aggHelper=readFileSync(source('packages/server-quality/src/adapters/quality-child-agg-sql.ts'),'utf8');
  expect(aggHelper.match(/SELECT /g) ?? []).toHaveLength(0);
  expect(aggHelper).toContain('import {scopeSql} from "./quality-scope-sql";');
  expect(aggHelper).toContain('scopeSql(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${table} ${alias} WHERE ${alias}.${key}=${parent}`)');
 });
});
describe('quality source-rows family dedupe (r12)',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 const pre={
  holds:'SELECT * FROM inventory_quality_holds',
  inspections:'SELECT * FROM inventory_quality_inspections',
  dispositions:'SELECT * FROM inventory_quality_disposition_events',
  cancellations:'SELECT * FROM inventory_quality_cancellations',
 } as const;
 test('one parameterized helper reproduces the four full-row source reads byte-for-byte',async()=>{
  const {qualitySourceRows}=await import('../../../packages/server-quality/src/adapters/quality-source-rows-sql');
  for(const literal of Object.values(pre)){
   const table=literal.slice('SELECT * FROM '.length);
   expect(qualitySourceRows(table)).toBe(literal);
   expect(qualitySourceRows(table).match(/SELECT /g)).toHaveLength(1);
  }
  const {salesQualitySettlementSources}=await import('../../../packages/server-quality/src/adapters/sales-package-sources');
  for(const [key,literal] of Object.entries(pre))expect(salesQualitySettlementSources[key as keyof typeof pre]).toBe(literal);
 });
 test('the four callers share one helper literal with no inline copies',()=>{
  const adapter=readFileSync(source('packages/server-quality/src/adapters/sales-package-sources.ts'),'utf8');
  expect(adapter.match(/qualitySourceRows\(/g)).toHaveLength(4);
  for(const literal of Object.values(pre))expect(adapter).not.toContain(literal);
  const helper=readFileSync(source('packages/server-quality/src/adapters/quality-source-rows-sql.ts'),'utf8');
  expect(helper.match(/SELECT /g) ?? []).toHaveLength(0);
 });
 test('source locks and r10/r11 helpers stay independent and untouched',()=>{
  const adapter=readFileSync(source('packages/server-quality/src/adapters/sales-package-sources.ts'),'utf8');
  expect(adapter).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations,inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE');
  expect(adapter).toContain('export function salesQualitySettlementSourceReadLocks');
  const helper=readFileSync(source('packages/server-quality/src/adapters/quality-source-rows-sql.ts'),'utf8');
  expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
 });
});
