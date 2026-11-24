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

import {existsSync,readFileSync} from 'node:fs';
import {resolve} from 'node:path';
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
  expect(text.match(/SELECT n\.display_name/g)??[]).toHaveLength(1);
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
 test('one fixed CAS SELECT is composed by both attachment owner facts',async()=>{
  const {qualityAttachmentVersionGuardSql}=await import('@erpx/server-quality/adapters/attachments-service-facts');
  const sql=qualityAttachmentVersionGuardSql('inventory_quality_holds');
  expect(sql.match(/SELECT /g)).toHaveLength(1);
  expect(sql).toBe('EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=?)');
  expect(qualityAttachmentVersionGuardSql('inventory_quality_holds','h.status=?'))
   .toBe('EXISTS(SELECT 1 FROM inventory_quality_holds WHERE id=? AND company_id=? AND version=? AND h.status=?)');
  const inline=/EXISTS\(SELECT 1 FROM \$\{[^}]+\} WHERE id=\? AND company_id=\? AND version=\?/g;
  const service=readFileSync(source('packages/server-quality/src/adapters/attachments-service-facts.ts'),'utf8');
  const operational=readFileSync(source('packages/server-quality/src/adapters/attachments-operational-facts.ts'),'utf8');
  expect(service.match(inline)??[]).toHaveLength(1);
  expect(operational.match(inline)??[]).toHaveLength(0);
  expect(service).toContain('qualityAttachmentVersionGuardSql');
  expect(operational).toContain('qualityAttachmentVersionGuardSql');
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
describe('quality attachment target-scope read dedupe',()=>{
 const source=(path:string)=>resolve(process.cwd(),path);
 test('one fixed target-scope SELECT backs all three admitted variants',async()=>{
  const {qualityAttachmentScopeReadSql}=await import('../../../packages/server-quality/src/adapters/quality-attachment-scope-read-sql');
  expect(qualityAttachmentScopeReadSql('legacy_hold'))
   .toBe('SELECT company_id,branch_id branch_id,NULL department_id,version FROM inventory_quality_holds WHERE id=?');
  expect(qualityAttachmentScopeReadSql('hold'))
   .toBe('SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM inventory_quality_holds WHERE id=? AND company_id=?');
  expect(qualityAttachmentScopeReadSql('spec'))
   .toBe('SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM inventory_quality_specs WHERE id=? AND company_id=?');
  for(const variant of ['legacy_hold','hold','spec'] as const)expect(qualityAttachmentScopeReadSql(variant).match(/SELECT /g)).toHaveLength(1);
  const helper=readFileSync(source('packages/server-quality/src/adapters/quality-attachment-scope-read-sql.ts'),'utf8');
  expect(helper.match(/SELECT /g)).toHaveLength(1);
 });
 test('owner facts compose the shared SELECT instead of inlining it',()=>{
  const migration='packages/server-quality/src/adapters/attachment-migration-owner.ts';
  const policy='packages/server-quality/src/adapters/attachments-policy-facts.ts';
  const operational='packages/server-quality/src/adapters/attachments-operational-facts.ts';
  for(const path of [migration,policy,operational])expect(readFileSync(source(path),'utf8')).toContain('qualityAttachmentScopeReadSql');
  expect(readFileSync(source(migration),'utf8')).not.toContain('SELECT ');
  expect(readFileSync(source(operational),'utf8')).not.toContain('SELECT ');
  const policyText=readFileSync(source(policy),'utf8');
  expect(policyText).not.toContain('SELECT ');
  expect(policyText).toContain('readQualityHold');
  expect(policyText.match(/qualityAttachmentScopeReadSql/g)).toHaveLength(3);
  expect(readFileSync(source(operational),'utf8')).toContain('qualityAttachmentVersionGuardSql');
 });
});
