import { afterEach, expect, it, vi } from 'vitest';
import { childSelections, hiddenDraftKeys, peopleExplorerTabs, selectPanel, trayKindsForParents } from "../../../src/lib/people_payroll/people-explorer";
import { emptyDraft } from "../../../src/lib/organization/organization-draft";
import type { HoldingPeopleDirectory } from "../../../src/server/modules/organization/public/holding-people-directory";
import {createPerformanceCommandFetch, performanceCommandPurpose, performanceFormUnchanged, validPerformanceAcknowledgement} from "../../../src/lib/people_payroll/performance-command-fetch";
import {EvaluationConsole} from "../../../src/components/people_payroll/evaluation-console";
import {DefinitionEditor} from "../../../src/components/people_payroll/evaluation-editors";
import {EmbeddedRecord} from "../../../src/components/records/record-context";
import type {EvaluationWorkspace} from "../../../src/server/modules/people_payroll/public/people-evaluation";
import {defaultEvaluationPolicy} from "../../../src/lib/people_payroll/people-evaluation";
import {createElement,type ComponentProps} from "react";
import {renderToStaticMarkup} from "react-dom/server";
import {InstallLevels} from "../../../src/components/organization/organization-workspace";
import {useStaffCommand} from "../../../src/components/work_projection/use-staff-command";
import {CompensationEditor} from "../../../src/components/people_payroll/payroll-controls";
import {PayrollFormDrawer} from "../../../src/components/people_payroll/payroll-form-drawer";
import type {DraftFormDrawer} from "../../../src/components/ui/draft-form-drawer";
const evaluationNavigation=vi.hoisted(()=>({search:"view=criteria",routineModuleImports:0,payrollBoundaryImports:0,drawers:[] as ComponentProps<typeof DraftFormDrawer>[]}));
vi.mock("../../../src/components/people_payroll/payroll-draft-boundary", async importOriginal => {
 const actual=await importOriginal<typeof import("../../../src/components/people_payroll/payroll-draft-boundary")>();
 evaluationNavigation.payrollBoundaryImports++;
 return actual;
});
vi.mock("../../../src/components/ui/draft-form-drawer", async importOriginal => {
 const actual=await importOriginal<typeof import("../../../src/components/ui/draft-form-drawer")>();
 const {createElement}=await import("react");
 return {...actual,DraftFormDrawer:function ObservedDraftDrawer(props:ComponentProps<typeof actual.DraftFormDrawer>){evaluationNavigation.drawers.push(props);return createElement(actual.DraftFormDrawer,props);}};
});
vi.mock("../../../src/components/operations/routine-schedule-controls", async importOriginal => {
 const actual=await importOriginal<typeof import("../../../src/components/operations/routine-schedule-controls")>();
 evaluationNavigation.routineModuleImports++;
 return actual;
});
vi.mock("next/navigation", () => ({usePathname: () => "/holdings/acme/people-organization/people/evaluation", useSearchParams: () => new URLSearchParams(evaluationNavigation.search), useRouter: () => ({push: vi.fn(), replace: vi.fn(), refresh: vi.fn()})}));
afterEach(() => {vi.unstubAllGlobals();evaluationNavigation.search="view=criteria";evaluationNavigation.drawers.length=0;});
const data:HoldingPeopleDirectory={companies:[{id:'c',name:'Company',slug:'c'}],nodes:[{id:'b',name:'Branch',slug:'b',kind:'branch',companyId:'c',branchId:null,parentId:'c'},{id:'d',name:'Department',slug:'d',kind:'department',companyId:'c',branchId:'b',parentId:'b'},{id:'e',name:'Other department',slug:'e',kind:'department',companyId:'c',branchId:'b',parentId:'b'}],positions:[{id:'p',companyId:'c',branchId:'b',departmentId:'d',title:'Position',levelId:'l',reportsToPositionId:null,isCeo:false}],levels:[{id:'l',companyId:'c',name:'Level',number:2,code:'L2'}],people:[{id:'eng',engagementId:'eng',personId:'person',companyId:'c',branchId:'b',departmentId:'d',positionId:'p',legalName:'Employee',jobTitle:'Position',workEmail:null,workPhone:null}]};

it('retains the exact Evaluation command key until a parsed owner acknowledgement, including truncated successful responses', async () => {
 const keys:string[]=[],request=createPerformanceCommandFetch(async(_url,init)=>{keys.push(new Headers(init?.headers).get('Idempotency-Key')!);if(keys.length===1)throw Error('Lost response');if(keys.length===2)return new Response('incomplete',{status:200});if(keys.length===3)return Response.json({data:{version:2}});return Response.json({data:{id:'assessment',version:2}});});
 for(let attempt=0;attempt<3;attempt++)await expect(request('/evaluation/assessments',{engagementId:'one'})).rejects.toMatchObject({uncertain:true});
 expect(request.hasAttempt('/evaluation/assessments',{engagementId:'one'})).toBe(true);await expect(request('/evaluation/assessments',{engagementId:'one'})).resolves.toEqual({id:'assessment',version:2});expect(new Set(keys).size).toBe(1);expect(request.hasAttempt('/evaluation/assessments',{engagementId:'one'})).toBe(false);
 await request('/evaluation/assessments',{engagementId:'one'});expect(keys[4]).not.toBe(keys[0]);
});
it('separates changed Evaluation bodies and rendered authority while preserving current-role retry identity', async () => {
 const dataset={workUser:'employee',workCompany:'company',workAssignment:'hr',workRevision:'1'};vi.stubGlobal('document',{querySelector:()=>({dataset})});const keys:string[]=[],request=createPerformanceCommandFetch(async(_url,init)=>{keys.push(new Headers(init?.headers).get('Idempotency-Key')!);return Response.json({error:{message:'Current permission required'}},{status:403});});
 for(const input of [{reason:'Original'},{reason:'Original'},{reason:'Changed'}])await expect(request('/evaluation/criteria',input)).rejects.toMatchObject({uncertain:false});dataset.workAssignment='reviewer';dataset.workRevision='2';expect(request.hasAttempt('/evaluation/criteria',{reason:'Original'})).toBe(false);await expect(request('/evaluation/criteria',{reason:'Original'})).rejects.toThrow('Current permission required');expect(keys[1]).toBe(keys[0]);expect(keys[2]).not.toBe(keys[0]);expect(keys[3]).not.toBe(keys[0]);
});
it('acknowledges only the captured Evaluation form version and retains a newer local draft', async () => {
 let edit='submitted';const form:Pick<HTMLFormElement,'isConnected'|'getAttribute'>={isConnected:true,getAttribute:()=>edit},completion={form,formEditVersion:edit},events:Event[]=[];vi.stubGlobal('window',{dispatchEvent:(event:Event)=>{events.push(event);return true;}});vi.stubGlobal('document',{activeElement:{closest:()=>form},querySelector:()=>null});expect(performanceFormUnchanged(completion)).toBe(true);const request=createPerformanceCommandFetch(async()=>{edit='newer';return Response.json({data:{id:'definition',version:2}});});await request('/evaluation/criteria',{reason:'Original'});const event=events.find(event=>event.type==='erpx:record-command-saved');expect(event).toBeInstanceOf(CustomEvent);if(!(event instanceof CustomEvent))throw Error('Owner acknowledgement absent');expect(event.detail.formEditVersion).toBe('submitted');expect(event.detail.performanceAcknowledged).toBe(true);expect(performanceFormUnchanged(event.detail)).toBe(false);for(const invalid of [null,[],{},1,{id:''},{version:2}])expect(validPerformanceAcknowledgement(invalid)).toBe(false);
});
it('classifies native Performance preparation separately from retained independent settlement',()=>{
 for(const path of ['criteria','market-import','assessments','assessments/a/save','basis/e','assessments/a/compensation','ai'])expect(performanceCommandPurpose(path,{})).toBe('newWork');expect(performanceCommandPurpose('assessments/a/transition',{action:'reopen'})).toBe('newWork');expect(performanceCommandPurpose('criteria',{id:'stored-cycle',kind:'cycle',data:{status:'closed'}})).toBe('settleExisting');expect(performanceCommandPurpose('criteria',{kind:'cycle',data:{status:'closed'}})).toBe('newWork');for(const [path,input] of [['assessments/a/evidence',{}],['assessments/a/transition',{action:'confirm'}],['compensation/d/approve',{}],['compensation/d/cancel',{}]] as const)expect(performanceCommandPurpose(path,input)).toBe('settleExisting');
});
it('renders original Performance criteria history while disabling new actions during draining without exposing purchase internals',()=>{
 const initial:EvaluationWorkspace={rows:[],skillVersions:[],definitions:[{id:'skill',kind:'skill',version:1,data:{name:'Actual retained skill',confirmed:true,category:'professional'},updatedAt:1}],positions:[],levels:[],query:{year:2026,date:'2026-12-31'},cycleId:null,policy:defaultEvaluationPolicy,actorId:'hr',canWrite:true,canConfigure:true,canConfirm:true,canCompensation:true,canEvidenceFiles:true,canApproveCompensation:true,purchase:{newWork:false,settleExisting:true,history:true,revision:'opaque-performance-revision'}};
 const html=renderToStaticMarkup(createElement(EvaluationConsole,{initial,base:'/holdings/acme',apiBase:'/api/v1/holdings/acme/people/evaluation'}));expect(html).toContain('Actual retained skill');expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Add Skill/);expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Review \/ revise/);expect(html).not.toContain('opaque-performance-revision');expect(html).not.toContain('company_package_grants');
});
it('offers unchanged closed-cycle reaffirmation during draining through a reason-only editor while retaining inactive history',()=>{
 evaluationNavigation.search='view=cycles';const cycle={id:'closed-cycle',kind:'cycle' as const,version:2,data:{name:'Original closed annual review',year:2026,policy:{id:'original-policy',version:1},status:'closed',confirmed:true},updatedAt:1},initial:EvaluationWorkspace={rows:[],skillVersions:[],definitions:[cycle],positions:[],levels:[],query:{year:2026,date:'2026-12-31'},cycleId:cycle.id,policy:defaultEvaluationPolicy,actorId:'hr',canWrite:true,canConfigure:true,canConfirm:true,canCompensation:true,canEvidenceFiles:true,canApproveCompensation:true,purchase:{newWork:false,settleExisting:true,history:true,revision:'opaque-closed-revision'}};
 const html=renderToStaticMarkup(createElement(EvaluationConsole,{initial,base:'/holdings/acme',apiBase:'/api/v1/holdings/acme/people/evaluation'}));expect(html).toMatch(/<button(?![^>]*\sdisabled=)[^>]*>Reaffirm cycle closure/);expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Add Annual cycle/);const renderEditor=(workspace:EvaluationWorkspace)=>renderToStaticMarkup(createElement(EmbeddedRecord.Provider,{value:true},createElement(DefinitionEditor,{workspace,kind:'cycle',current:cycle,command:async()=>({id:cycle.id,version:3}),onClose:()=>{}}))),editor=renderEditor(initial);expect(editor).toContain('Reaffirm annual cycle closure');expect(editor).toContain('Incomplete reviews remain unresolved');expect(editor).toContain('Cycle closure review reason');expect(editor).toMatch(/<button(?![^>]*\sdisabled=)[^>]*>Reaffirm closure/);expect(editor).not.toContain('Cycle status');expect(editor).not.toContain('January–December year');expect(editor).not.toContain('opaque-closed-revision');const inactive={...initial,purchase:{newWork:false,settleExisting:false,history:true,revision:'opaque-inactive-revision'}};expect(renderEditor(inactive)).toMatch(/<button[^>]*disabled=""[^>]*>Reaffirm closure/);expect(renderToStaticMarkup(createElement(EvaluationConsole,{initial:inactive,base:'/holdings/acme',apiBase:'/api/v1/holdings/acme/people/evaluation'}))).toMatch(/<button[^>]*disabled=""[^>]*>Reaffirm cycle closure/);
});
it('opens three rows, replaces only the clicked path and never opens a child for an employee',()=>{
 const initial=[{selection:{kind:'company' as const,id:'c'},tab:''}];
 const source=selectPanel(initial,0,{kind:'department',id:'d'},data);
 const destination=selectPanel(initial,0,{kind:'branch',id:'b'},data);
 expect(source).toHaveLength(2);expect(destination).toHaveLength(2);
 const deeper=selectPanel(source,1,{kind:'position',id:'p'},data);expect(deeper).toHaveLength(3);
 const employee=selectPanel(deeper,2,{kind:'person',id:'eng'},data);expect(employee).toHaveLength(3);
 const changed=selectPanel(employee,0,{kind:'department',id:'e'},data);expect(changed).toHaveLength(1);
 expect(destination[0].selection).toEqual({kind:'branch',id:'b'});
 expect(childSelections(data,{kind:'position',id:'p'})).toEqual([{kind:'person',id:'eng'}]);
});
it('hides children carried by a staged group while keeping the single draft untouched',()=>{
 const draft={...emptyDraft(),items:[{kind:'department' as const,id:'d',parked:true}]};
 expect([...hiddenDraftKeys(data,draft,true)]).toEqual(['department:d','position:p','person:eng']);
 expect([...hiddenDraftKeys(data,draft,false)]).toEqual(['department:d','position:p','person:eng']);
 expect(draft.items).toHaveLength(1);
});
it('limits HR and compensation tabs separately',()=>{
 expect(peopleExplorerTabs('person',false,false)).toEqual(['Profile']);
 expect(peopleExplorerTabs('person',false,true)).toEqual(['Profile','Compensation']);
 expect(peopleExplorerTabs('person',true,false)).toEqual(['Profile','Employment','History']);
});
it('hides carried teams and their descendants even when children precede parents in the directory',()=>{
 const nested={...data,nodes:[{...data.nodes[1],id:'child',parentId:'team'},{...data.nodes[1],id:'team',kind:'team' as const,parentId:'d'},...data.nodes],positions:[{...data.positions[0],departmentId:'child'}],people:[{...data.people[0],departmentId:'child'}]};
 const draft={...emptyDraft(),items:[{kind:'department' as const,id:'d',parked:true}]};
 expect([...hiddenDraftKeys(nested,draft,true)]).toEqual(expect.arrayContaining(['department:d','team:team','department:child','position:p','person:eng']));
});

it('keeps row indices when opening a Tray detail beneath the other path',()=>{
 const path=selectPanel([{selection:null,tab:''}],2,{kind:'person',id:'eng'},data);
 expect(path).toHaveLength(3);expect(path[1]).toEqual({selection:null,tab:''});expect(path[2].selection).toEqual({kind:'person',id:'eng'});
});
it('keeps trays available when the two sides drill through different item types',()=>{
 expect(trayKindsForParents([{kind:'department',id:'d'},{kind:'position',id:'p'}])).toEqual(['position','person']);
 expect(trayKindsForParents([])).toEqual(['department']);
});


it('loads actual People organization controls without importing unrelated routine editors',()=>{
 expect(evaluationNavigation.routineModuleImports).toBe(0);
});
it.each(['legacy','standard'] as const)('keeps actual Organization staff-level controls in %s without a mount request',appearance=>{
 const fetcher=vi.spyOn(globalThis,'fetch');
 try {
  const html=renderToStaticMarkup(createElement(InstallLevels,{base:'/holdings/acme',appearance}));
  expect(html).toContain('Set up six staff levels');
  expect(html).toContain('Install staff levels</button>');
  expect(html).toContain(`data-appearance="${appearance}"`);
  expect(html.includes('ui-standard-control')).toBe(appearance==='standard');
  expect(html).not.toContain('Run routine now');
  expect(fetcher).not.toHaveBeenCalled();
 } finally {fetcher.mockRestore();}
});
it('preserves the original Staff command body/key through failed transport and changes the key only for changed input',async()=>{
 let command:ReturnType<typeof useStaffCommand>|undefined;
 function Capture(){command=useStaffCommand('/holdings/acme');return createElement('span',null,'Original command owner');}
 expect(renderToStaticMarkup(createElement(Capture))).toBe('<span>Original command owner</span>');
 if(!command)throw Error('The actual React hook must supply the original command.');
 const calls:RequestInit[]=[],fetcher=vi.fn(async(_url:RequestInfo|URL,init?:RequestInit)=>{calls.push(init!);if(calls.length===1)throw Error('Lost response');return Response.json({error:{message:'Current original owner refused'}},{status:403});});
 vi.stubGlobal('fetch',fetcher);
 expect(await command.send('people/organization/levels',{original:true})).toBeNull();
 expect(await command.send('people/organization/levels',{original:true})).toBeNull();
 expect(await command.send('people/organization/levels',{original:false})).toBeNull();
 expect(fetcher.mock.calls.map(call=>call[0])).toEqual(Array(3).fill('/api/v1/holdings/acme/people/organization/levels'));
 expect(calls[0].body).toBe(calls[1].body);
 expect(JSON.parse(String(calls[0].body))).toEqual({original:true});
 expect(new Headers(calls[0].headers).get('Idempotency-Key')).toBe(new Headers(calls[1].headers).get('Idempotency-Key'));
 expect(new Headers(calls[2].headers).get('Idempotency-Key')).not.toBe(new Headers(calls[0].headers).get('Idempotency-Key'));
});


it('loads the actual Compensation editor without importing the unrelated page-wide Payroll navigation boundary',()=>{
 expect(evaluationNavigation.payrollBoundaryImports).toBe(0);
});
it.each(['legacy','standard'] as const)('keeps actual Compensation %s controls and owner form values without a mount request',appearance=>{
 const fetcher=vi.spyOn(globalThis,'fetch');
 try {
  const html=renderToStaticMarkup(createElement(CompensationEditor,{
   ...(appearance==='standard'?{appearance}:{}),base:'/holdings/acme',person:{engagement_id:'original-engagement',engagement_version:3,currency:'THB',pay_group_id:'original-group'},groups:[{id:'original-group',currency:'THB',name:'Original group'}],revisions:[],canEdit:true,canApprove:false,userId:'original-HR',
  }));
  expect(html).toContain('Compensation and pay group');
  expect(html).toContain('Original group');
  expect(html).toContain('value="original-group"');
  expect(html).toContain('Update pay group</button>');
  expect(html).toContain(`data-appearance="${appearance}"`);
  expect(html.includes('ui-standard-control')).toBe(appearance==='standard');
  expect(html).not.toContain('Leave unsaved Payroll work?');
  expect(fetcher).not.toHaveBeenCalled();
 } finally {fetcher.mockRestore();}
});
it.each(['legacy','standard'] as const)('keeps the actual Payroll form drawer %s prop and original form ancestry',appearance=>{
 const child=createElement('form',{className:'original-payroll-form'},createElement('input',{name:'payAmount',defaultValue:'123.45'}));
 const props:ComponentProps<typeof PayrollFormDrawer>={appearance,open:true,onClose:()=>{},title:'Original compensation revision',children:child};
 renderToStaticMarkup(createElement(PayrollFormDrawer,props));
 expect(evaluationNavigation.drawers).toHaveLength(1);
 const drawer=evaluationNavigation.drawers[0];
 expect(drawer.appearance).toBe(appearance);expect(drawer.dirty).toBe(false);
 expect(drawer.title).toBe('Original compensation revision');
 const html=renderToStaticMarkup(drawer.children);
 expect(html.match(/<form\b/g)).toHaveLength(1);
 expect(html).toContain('class="original-payroll-form"');
 expect(html).toContain('value="123.45"');
});
