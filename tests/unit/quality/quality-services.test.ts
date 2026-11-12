import {describe,expect,test} from 'vitest';
import {createQualityApplication,type QualityRepository} from "@erpx/server-quality/application/quality";
import {createQualityAlertApplication,sweepQualityAlerts,type QualityAlertRepository,type QualityAlertSweepRepository} from "@erpx/server-quality/application/alerts";
import type {QualityRow} from "@erpx/server-quality/domain/quality";
import type {QualityAlert} from "@erpx/server-quality/domain/alerts";

const baseHold:QualityRow={id:'hold',status:'held',created_by:'creator',last_inspector:'inspector',last_result:null,held_quantity_milli:2000,warehouse_id:'warehouse',product_id:'product',branch_id:'branch',version:1,spec_id:null,updated_at:42,due_date:'2026-10-06',eligible_count:1};
function fixture(overrides:Partial<QualityRepository<string[]>>={}){
 const calls:string[]=[];
 const repository:QualityRepository<string[]>={
  async holds(){return [baseHold];},async activeSpec(){return {id:'spec',version:2};},evidenceVersionGuard:()=>['version-fence'],receiptHold(input,spec,now){return [JSON.stringify({input,spec,now})];},...overrides,
 };
 const ports={departmentScope:false,companyScope:true,userId:'manager',authorize(permission:string){calls.push(permission);},can:(permission:string)=>Boolean(permission),samePerson:(id:string|number|null)=>id==='manager',clock:{now:()=>42},hash:async()=> '1234567890abcdef',async repository(){return repository;}};
 return {ports,repository,calls,app:createQualityApplication(ports)};
}
describe('Quality receipt-sourced quarantine',()=>{
 test('receipt-created partial quarantine stages on the supplied repository without a second receipt',async()=>{const f=fixture(),input={id:'hold',key:'key',branchId:'branch',warehouseId:'warehouse',productId:'product',quantityMilli:1000,reason:'Received product needs inspection',dueDate:'2026-10-07',sourceId:'receipt',sourceType:'customer_return' as const};expect(JSON.parse((await f.app.receiptQualityHoldSteps(input))[0])).toEqual({input,spec:{id:'spec',version:2},now:42});expect(f.calls).toEqual([]);});
});
describe('Quality retained reads and evidence guards',()=>{
 test('the hold creator cannot stage inspection evidence',async()=>{const f=fixture({async holds(){return [{...baseHold,created_by:'manager'}];}});await expect(f.app.qualityEvidenceGuard('hold',1,'key')).rejects.toMatchObject({code:'INDEPENDENT_INSPECTOR_REQUIRED'});expect(f.calls).toEqual(['quality.read','quality.inspect']);});
 test('evidence staging requires an active hold and reports a missing hold',async()=>{const f=fixture({async holds(){return [{...baseHold,status:'released'}];}});await expect(f.app.qualityEvidenceGuard('hold',1,'key')).rejects.toMatchObject({code:'QUALITY_INSPECTION_LOCKED'});f.repository.holds=async()=>[];await expect(f.app.getQualityHold('hold')).rejects.toMatchObject({code:'QUALITY_HOLD_NOT_FOUND'});});
 test('revoked independent recipients leave work waiting with an actionable blocker',async()=>{const f=fixture({async holds(){return [{...baseHold,eligible_count:0}];}});f.ports.can=()=>false;expect(await f.app.qualityWork()).toEqual([]);f.ports.can=permission=>permission==='quality.read';expect(await f.app.qualityWork()).toEqual([expect.objectContaining({queue:'waiting',canAct:false,blocker:'No eligible independent recipient; Company administrator must assign quality authority'})]);});
});
const alert:QualityAlert={id:'alert',hold_id:'hold',phase:'overdue',source_version:1,hold_version:2,due_date:'2026-10-06',detected_at:42,version:1,acknowledged_by:null,acknowledged_at:null,reason:null,company_id:'company',branch_id:'branch',status:'held',product_name:'Product',warehouse_name:'Warehouse',can_ack:1,recipients:1};
describe('Quality alert recovery',()=>{
 test('sweep ordering and UTC dates remain deterministic across recovery',async()=>{const calls:unknown[][]=[],repository:QualityAlertSweepRepository={async dueSoon(...args){calls.push(['due',...args]);return 2;},async overdue(...args){calls.push(['overdue',...args]);return 3;},async notify(...args){calls.push(['route',...args]);}};const now=Date.UTC(2026,9,6,23,30);expect(await sweepQualityAlerts(repository,now,86400000)).toEqual({dueSoon:2,overdue:3});expect(calls).toEqual([['due',now,'2026-10-06','2026-10-07'],['overdue',now,'2026-10-06'],['route',now]]);});
 test('a partially failed sweep stops and a restarted sweep continues all three stages',async()=>{const calls:string[]=[],repository:QualityAlertSweepRepository={async dueSoon(){calls.push('due');return 0;},async overdue(){calls.push('overdue');throw new Error('database interruption');},async notify(){calls.push('route');}};await expect(sweepQualityAlerts(repository,42,86400000)).rejects.toThrow('database interruption');expect(calls).toEqual(['due','overdue']);repository.overdue=async()=>{calls.push('overdue');return 1;};expect(await sweepQualityAlerts(repository,42,86400000)).toEqual({dueSoon:0,overdue:1});expect(calls.slice(2)).toEqual(['due','overdue','route']);});
 test('revoked acknowledgement authority is checked before replay',async()=>{const repository:QualityAlertRepository={async alerts(){return [alert];},async replay(){throw new Error('must not replay');},async acknowledge(){throw new Error('must not mutate');}};const app=createQualityAlertApplication({departmentScope:false,userId:'user',authorizeRead(){},can:()=>false,clock:{now:()=>42},async repository(){return repository;}});await expect(app.acknowledgeQualityAlert('alert',{expectedVersion:1,reason:'Take responsibility'},'key')).rejects.toMatchObject({code:'QUALITY_ALERT_PERMISSION_REQUIRED'});});
 test('completed and recipient-revoked escalations retain their source but cannot act',async()=>{const repository:QualityAlertRepository={async alerts(){return [{...alert,can_ack:0,recipients:0},{...alert,id:'released',status:'released'}];},async replay(){return null;},async acknowledge(){throw new Error('must not mutate');}};const app=createQualityAlertApplication({departmentScope:false,userId:'user',authorizeRead(){},can:()=>true,clock:{now:()=>42},async repository(){return repository;}});expect(await app.qualityAlertWork()).toEqual([expect.objectContaining({queue:'waiting',canAct:false,blocker:expect.stringContaining('No eligible Quality recipient'),source:expect.objectContaining({id:'hold',version:2})}),expect.objectContaining({queue:'completed',canAct:false,status:'acknowledged'})]);});
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
