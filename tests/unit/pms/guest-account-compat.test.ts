import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { canonicalGuestAccount, legacyGuestAccount, guestAccountOperation, guestAccountUrl, parseGuestAccountJson } from "../../../src/lib/identity_access/guest-account-compat";
import { builtInPermissions, hasPermission } from "../../../src/lib/identity_access/permissions";
import type { PermissionActor } from "../../../src/lib/identity_access/iam";

it("reads historical references without changing IDs, money or the frozen payload",()=>{
  const current={kind:"guest_account",operation:"guest_account.close",input:{guestAccountId:"historical-account-id",toGuestAccountId:"other-account",amount:10700},guestAccount:{due:10700,deposit:3000},guestAccounts:["historical-account-id"]};
  const bytes=JSON.stringify(legacyGuestAccount(current));
  expect(parseGuestAccountJson(bytes)).toEqual(current);
  expect(JSON.stringify(legacyGuestAccount(current))).toBe(bytes);
  expect(guestAccountOperation(legacyGuestAccount({operation:current.operation}).operation)).toBe(current.operation);
  const oldKey=Object.keys(legacyGuestAccount({guestAccountId:"old"}))[0];
  expect(()=>canonicalGuestAccount({[oldKey]:"old",guestAccountId:"different"})).toThrow("GUEST_ACCOUNT_REFERENCE_CONFLICT");
});
it("redirects historical URLs with the original selectors and canonical response names",()=>{
  const section=legacyGuestAccount({section:"guest-accounts"}).section;
  const key=Object.keys(legacyGuestAccount({guestAccount:"account-id"}))[0];
  expect(guestAccountUrl(`/holdings/acme/pms/${section}?property=hotel&${key}=account-id`)).toBe("/holdings/acme/pms/guest-accounts?property=hotel&guestAccount=account-id");
  expect(guestAccountUrl(`/acme/acme-co/holding/${section}?${key}=account-id`,true)).toBe("/acme/acme-co/holding/guest-accounts?guestAccount=account-id");
});
it("separates built-in operational and accounting assignments and preserves approved custom grants",()=>{
  for(const role of ["finance_preparer","finance_approver","treasury","auditor"]){
    const permissions=builtInPermissions(role);
    for(const forbidden of ["pms.read","pms.cashier","pms.approve","pos.read","pos.configure","pos.approve"])expect(permissions).not.toContain(forbidden);
  }
  expect(builtInPermissions("pos_manager")).toContain("pos.configure");
  expect(builtInPermissions("pos_manager")).not.toContain("pos.accounting.prepare");
  for(const role of ["pos_cashier","pos_server"])expect(builtInPermissions(role)).not.toContain("pos.kitchen");
  const actor={userId:"one-person",role:"custom",permissions:["pms.cashier","pos.configure"],assignmentScope:"holding"} as PermissionActor;
  expect(hasPermission(actor,"pms.cashier")).toBe(true); expect(hasPermission(actor,"pos.configure")).toBe(true);
  expect(hasPermission({...actor,permissions:["pos.configure"]},"pms.cashier")).toBe(false);
  expect(hasPermission({...actor,assignmentScope:"branch",permissions:["pos.accounting.prepare"]},"pos.accounting.prepare")).toBe(false);
});

it("replays a historical receipt after the migration changes context revision, with current authority still required",async()=>{
  const {createCommandApplication}=await import("../../../src/server/modules/workflow/application/commands");
  const actor={userId:"operator",assignmentId:"hotel-cashier",role:"pms_cashier",companyId:"company",branchId:null,departmentId:null,authority:{sessionId:"session",revision:2,assignmentVersion:1}};
  const previous={...actor,authority:{...actor.authority,revision:1}},input={guestAccountId:"original-id"},operation="pms.guest_account.close";
  const historical=legacyGuestAccount({operation,input,result:{guestAccountId:"original-id",version:9}});
  const request_hash=JSON.stringify({actor:actor.userId,assignment:actor.assignmentId,role:actor.role,authority:previous.authority,company:actor.companyId,branch:actor.branchId,department:actor.departmentId,operation:historical.operation,recordId:"original-id",input:historical.input});
  let current=true,writes=0;
  const ports={repository:{receipt:async()=>({request_hash,result_json:JSON.stringify(historical.result),operation:historical.operation,record_id:"original-id",actor_id:actor.userId,authority_json:JSON.stringify({role:actor.role,assignmentId:actor.assignmentId,...previous.authority,companyId:actor.companyId,branchId:null,departmentId:null})}),clearAssertions:()=>"cleanup",insertReceipt:()=>{writes++;return "receipt";},captureAssignmentAuthority:()=>"capture",audit:()=>"audit",outbox:()=>"event"},clock:{now:()=>100},identifiers:{next:()=>"authority-check"},digest:async(value:unknown)=>JSON.stringify(value),workContextGuards:()=>["context"],iamMutationGuards:()=>["grants"],assignmentGuard:()=>"assignment",prepareOwnerStatements:async()=>[],batch:async()=>{if(!current)throw Object.assign(new Error("expired"),{code:"23514"});},activeContext:async()=>null,activeGrant:async()=>null};
  const app=createCommandApplication(ports);
  expect(await app.replayCommand(actor,"historical-key",operation,"original-id",input)).toEqual({guestAccountId:"original-id",version:9});
  expect(writes).toBe(0);
  await expect(app.replayCommand({...actor,companyId:"other-company"},"historical-key",operation,"original-id",input)).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
  await expect(app.replayCommand(actor,"historical-key",operation,"original-id",{guestAccountId:"different-id"})).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
  current=false;
  await expect(app.replayCommand(actor,"historical-key",operation,"original-id",input)).rejects.toMatchObject({code:"WORK_CONTEXT_CHANGED"});
});


// Actual owner pages retain their original local link semantics. Only native
// owner readers are fixture ports; React, Next Link, redirect and page bodies run.
import {createElement,isValidElement,type ComponentProps,type ReactNode} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import NextLink from 'next/link';
import BookingDirectory from '../../../src/app/book/[holdingSlug]/page';
import FrontCompanyPicker from '../../../src/app/front-access/page';
import {FrontError} from '../../../src/components/front/front-error';
import {MainContent} from '../../../src/components/ui/main';
import {ProductLinkEnvironment} from '../../../src/components/workplace/product-link-context';
import {BookingFlow,type HotelCard} from '../../../src/components/pms/booking-flow';
import type {FrontCompanyChoice} from '../../../src/server/modules/work_projection/public/front-quick-access';
const localOwners=vi.hoisted(()=>({links:[] as ComponentProps<typeof NextLink>[],booking:vi.fn(),catalog:vi.fn(),session:vi.fn(),choices:vi.fn(),preferences:vi.fn(),workspaces:vi.fn(),remembered:vi.fn(),refresh:vi.fn(),push:vi.fn(),replace:vi.fn(),buttons:[] as ComponentProps<typeof import('../../../src/components/ui/button').Button>[],localizedTrees:[] as ReactNode[]}));
vi.mock('next/link',async importOriginal=>{const actual=await importOriginal<typeof import('next/link')>();const {createElement}=await import('react');return {...actual,default:function ObservedNextLink(props:ComponentProps<typeof actual.default>){localOwners.links.push(props);return createElement(actual.default,props);}};});
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'pms-dev-erpx.jesadakorn.com'}),cookies:async()=>({get:()=>undefined})}));
vi.mock('next/navigation',async importOriginal=>({...await importOriginal<typeof import('next/navigation')>(),useRouter:()=>({push:localOwners.push,replace:localOwners.replace,refresh:localOwners.refresh}),usePathname:()=>'/front-access',useSearchParams:()=>new URLSearchParams()}));
vi.mock('../../../src/server/modules/pms/public/presentation-context-queries',()=>({bookingPresentationQueries:localOwners.booking}));
vi.mock('../../../src/server/modules/identity_access/public/authorization',()=>({currentSession:localOwners.session}));
vi.mock('../../../src/server/modules/work_projection/public/front-app',()=>({frontCompanyApp:localOwners.remembered}));
vi.mock('../../../src/server/modules/work_projection/public/front-quick-access',async()=>{const {frontCompanyItems}=await import('../../../src/server/modules/work_projection/domain/front-quick-access');return {frontCompanyItems,frontCompanyChoices:localOwners.choices};});
vi.mock('../../../src/server/modules/work_projection/public/quick-access-preferences',()=>({readQuickAccessPreferences:localOwners.preferences}));
vi.mock('../../../src/server/modules/organization/public/workspace-choices',()=>({workspaceChoices:localOwners.workspaces}));
const hotels:HotelCard[]=['north','south'].map(id=>({id:`hotel-${id}`,name:`Original ${id} hotel`,description:`Published ${id} description`,address:`${id} address`,contactEmail:`${id}@example.test`,contactPhone:'000',currency:'THB',checkIn:'14:00',checkOut:'12:00',timezone:'Asia/Bangkok',images:[],roomContent:[]}));
const companies:FrontCompanyChoice[]=[{id:'company:acme:north',label:'Authorized north Company',holding:'acme',slug:'north'},{id:'company:other:south',label:'Authorized south Company',holding:'other',slug:'south'}];
beforeEach(()=>{
 vi.clearAllMocks();localOwners.links.length=0;localOwners.buttons.length=0;localOwners.localizedTrees.length=0;
 localOwners.catalog.mockResolvedValue(hotels);localOwners.booking.mockResolvedValue({bookingCatalog:localOwners.catalog});
 localOwners.session.mockResolvedValue({session:{user:{id:'original-account'}}});localOwners.choices.mockResolvedValue(companies);
 localOwners.preferences.mockResolvedValue({pinnedIds:[],version:0});localOwners.workspaces.mockResolvedValue([]);
});
function localProductSurface(audience:'booking'|'pms',children:ReactNode){const props:ComponentProps<typeof ProductLinkEnvironment>={audience,children};return createElement(ProductLinkEnvironment,props);}
function observeLocalLinks(owner:string,paths:string[]){
 const links=localOwners.links.filter(link=>typeof link.href==='string'&&paths.includes(link.href));
 expect(links.length).toBeGreaterThanOrEqual(paths.length);
 for(const path of paths)expect(links.some(link=>link.href===path)).toBe(true);
 expect(links.every(link=>!Reflect.get(link,'data-product-switch'))).toBe(true);
 // Observe false (the temporary wrapper default) versus undefined (the exact
 // original Next default). Every observed URL is local; neither is an SSO GET.
 console.info('LOCAL_OWNER_LINK_DEFAULTS',JSON.stringify({owner,links:links.map(link=>({href:link.href,prefetch:link.prefetch??'Next default'}))}));
}
it('renders the actual Booking directory from one original catalog read with two published local hotel destinations',async()=>{
 const node=await BookingDirectory({params:Promise.resolve({holdingSlug:'acme'}),searchParams:Promise.resolve({})}),html=renderToStaticMarkup(localProductSurface('booking',node));
 expect(html).toContain('Original north hotel');expect(html).toContain('Original south hotel');
 expect(html).toContain('href="/book/acme/hotel-north"');expect(html).toContain('href="/book/acme/hotel-south"');
 expect(localOwners.booking.mock.calls).toEqual([['acme']]);expect(localOwners.catalog.mock.calls).toEqual([[]]);
 expect(localOwners.session).not.toHaveBeenCalled();expect(localOwners.choices).not.toHaveBeenCalled();
 observeLocalLinks('Booking directory',['/book/acme/hotel-north','/book/acme/hotel-south']);
});
it('renders actual Front Company choices from the current account and original scoped choice reader without auto-selecting a role',async()=>{
 const node=await FrontCompanyPicker({searchParams:Promise.resolve({choose:'1'})}),html=renderToStaticMarkup(localProductSurface('pms',node));
 expect(html).toContain('Authorized north Company');expect(html).toContain('Authorized south Company');
 expect(html).toContain('href="/acme/north"');expect(html).toContain('href="/other/south"');
 expect(localOwners.session).toHaveBeenCalledTimes(1);expect(localOwners.choices.mock.calls).toEqual([['pms']]);
 expect(localOwners.preferences.mock.calls).toEqual([['original-account','front-picker','pms']]);
 expect(localOwners.workspaces.mock.calls).toEqual([[]]);expect(localOwners.remembered).not.toHaveBeenCalled();
 observeLocalLinks('Front Company picker',['/acme/north','/other/south']);
});
it('retains actual Front unauthenticated redirect precedence before Company choices and preferences',async()=>{
 localOwners.session.mockRejectedValueOnce(Error('UNAUTHENTICATED'));
 await expect(FrontCompanyPicker({searchParams:Promise.resolve({choose:'1'})})).rejects.toThrow('NEXT_REDIRECT');
 expect(localOwners.choices).not.toHaveBeenCalled();expect(localOwners.preferences).not.toHaveBeenCalled();
});
it('retains actual Front current-owner refusal without replacing it with visible Company links',async()=>{
 const denied=Object.assign(Error('Current Company authority refused'),{status:403});localOwners.choices.mockRejectedValueOnce(denied);
 await expect(FrontCompanyPicker({searchParams:Promise.resolve({choose:'1'})})).rejects.toBe(denied);
 expect(localOwners.preferences).not.toHaveBeenCalled();expect(localOwners.links).toEqual([]);
});
it.each(['not-found','unavailable'] as const)('keeps the real Front %s recovery local and preserves its original main/section landmark and retry button',kind=>{
 const retry=vi.fn(),element=createElement(FrontError,{kind,reference:'fixture-reference',retry}),standalone=renderToStaticMarkup(localProductSurface('pms',element));
 expect(standalone).toContain('href="/"');expect(standalone).toContain('fixture-reference');expect(standalone).toContain('role="alert"');expect(standalone.match(/<main\b/g)).toHaveLength(1);
 const embedded=renderToStaticMarkup(createElement(MainContent,null,element));expect(embedded.match(/<main\b/g)).toHaveLength(1);expect(embedded).toContain('href="/"');expect(retry).not.toHaveBeenCalled();
 expect(localOwners.session).not.toHaveBeenCalled();expect(localOwners.choices).not.toHaveBeenCalled();observeLocalLinks(`Front ${kind}`,['/']);
});
it('keeps repeated query values and the original fragment on an actual local Next link without a cross-host launch',()=>{
 const href='/book/acme/hotel-north?tag=one&tag=two&arrival=2026-11-01#rooms',html=renderToStaticMarkup(createElement(NextLink,{href,prefetch:false},'Original hotel query'));
 expect(html).toContain('href="/book/acme/hotel-north?tag=one&amp;tag=two&amp;arrival=2026-11-01#rooms"');expect(html).not.toContain('/sso/');
 const observed=localOwners.links.at(-1);expect(observed?.href).toBe(href);expect(observed?.prefetch).toBe(false);
});


import {BookingRefunds} from '../../../src/components/pms/booking-refunds';
import {CounterRefunds} from '../../../src/components/pms/counter-refunds';
import {PmsPromptPay} from '../../../src/components/pms/promptpay-counter';
vi.mock('../../../src/components/ui/button',async importOriginal=>{const actual=await importOriginal<typeof import('../../../src/components/ui/button')>();const {createElement}=await import('react');return {...actual,Button:function ObservedButton(props:ComponentProps<typeof actual.Button>){localOwners.buttons.push(props);return createElement(actual.Button,props);}};});
vi.mock('../../../src/components/front/front-language',async importOriginal=>{const actual=await importOriginal<typeof import('../../../src/components/front/front-language')>();const {createElement}=await import('react');return {...actual,FrontLocalizedTree:function ObservedLocalizedTree(props:ComponentProps<typeof actual.FrontLocalizedTree>){localOwners.localizedTrees.push(props.children);return createElement(actual.FrontLocalizedTree,props);}};});
afterEach(()=>vi.unstubAllGlobals());
it.each(['booking','counter'] as const)('retains the actual %s refund approval body/current refusal and refresh callback without navigating',async kind=>{
 const base='/holdings/acme',booking:ComponentProps<typeof BookingRefunds>={base,rows:[{id:'original-refund',amount_minor:10000,status:'paid',refund_state:'requested',last_error:null,created_at:1}],canApprove:true},counter:ComponentProps<typeof CounterRefunds>={base,rows:[{id:'original-refund',property_id:'original-property',guest_account_id:'original-account',amount_minor:10000,refund_amount_minor:10000,refund_state:'requested',last_error:null,refund_reason:'Original independently reviewed reason',payer_email:null}],canApprove:true};
 const render=(allowed:boolean)=>kind==='booking'?renderToStaticMarkup(createElement(BookingRefunds,{...booking,canApprove:allowed})):renderToStaticMarkup(createElement(CounterRefunds,{...counter,canApprove:allowed}));
 expect(render(true)).toContain('Approve Stripe refund');
 const button=localOwners.buttons.find(button=>String(button.children).trim()==='Approve Stripe refund');if(!button?.onClick)throw Error('The real original approval callback is required.');
 let approved=true;const fetcher=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>approved?Response.json({data:{id:'original-refund'}}):Response.json({error:{message:'Original current authority refused'}},{status:403}));vi.stubGlobal('fetch',fetcher);
 await Reflect.apply(button.onClick,undefined,[]);
 expect(fetcher.mock.calls[0][0]).toBe(`/api/v1${base}/pms/${kind==='booking'?'booking-refunds':'stripe-refunds'}`);
 const request=fetcher.mock.calls[0][1];expect(request?.method).toBe('POST');expect(JSON.parse(String(request?.body))).toEqual(kind==='booking'?{id:'original-refund'}:{action:'approve',id:'original-refund'});
 expect(localOwners.refresh).toHaveBeenCalledTimes(1);expect(localOwners.push).not.toHaveBeenCalled();expect(localOwners.replace).not.toHaveBeenCalled();
 approved=false;await Reflect.apply(button.onClick,undefined,[]);expect(localOwners.refresh).toHaveBeenCalledTimes(1);
 expect(render(false)).not.toContain('Approve Stripe refund');
});
it('keeps actual PromptPay SSR/create serialization and waits for original confirmed polling before refreshing',async()=>{
 const html=renderToStaticMarkup(createElement(PmsPromptPay,{base:'/holdings/acme',propertyId:'original-property',guestAccountId:'original-guest-account',dueMinor:10000,history:[]}));
 expect(html).toContain('PromptPay');expect(html).toContain('name="amount"');expect(html).toContain('value="100.00"');
 function findForm(node:ReactNode):ComponentProps<'form'>|null{if(Array.isArray(node)){for(const child of node){const form=findForm(child);if(form)return form;}return null;}if(!isValidElement<ComponentProps<'form'>>(node))return null;if(node.type==='form')return node.props;return findForm(node.props.children);}
 const form=localOwners.localizedTrees.map(findForm).find(form=>form?.onSubmit);if(!form?.onSubmit)throw Error('The actual original PromptPay form callback is required.');
 const OriginalFormData=globalThis.FormData;class SubmittedBrowserFields extends OriginalFormData{constructor(){super();this.set('kind','receipt');this.set('amount','100.00');this.set('email','guest@example.test');}}vi.stubGlobal('FormData',SubmittedBrowserFields);
 const fetcher=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>Response.json({data:{id:'original-pending-payment',state:'pending'}}));vi.stubGlobal('fetch',fetcher);const preventDefault=vi.fn();
 await Reflect.apply(form.onSubmit,undefined,[{preventDefault,currentTarget:{}}]);expect(preventDefault).toHaveBeenCalledOnce();
 expect(fetcher.mock.calls[0][0]).toBe('/api/v1/holdings/acme/pms/stripe-payments');const request=fetcher.mock.calls[0][1];expect(request?.method).toBe('POST');expect(JSON.parse(String(request?.body))).toEqual({propertyId:'original-property',guestAccountId:'original-guest-account',kind:'receipt',amountMinor:10000,payerEmail:'guest@example.test'});
 expect(new Headers(request?.headers).get('Idempotency-Key')).toMatch(/^[0-9a-f-]{36}$/i);
 expect(localOwners.refresh).not.toHaveBeenCalled();expect(localOwners.push).not.toHaveBeenCalled();expect(localOwners.replace).not.toHaveBeenCalled();
});


import {canonicalUiHref} from '../../../src/lib/work_projection/destination-registry';
it.each(['public','staff'] as const)('preserves the actual original %s Booking availability form URL/body and makes no mount request',async channel=>{
 const staff:NonNullable<ComponentProps<typeof BookingFlow>['staff']>={base:'/holdings/acme',propertyId:'hotel-north',guests:[],blocks:[],save:async()=>true};
 const fetcher=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>Response.json([]));vi.stubGlobal('fetch',fetcher);
 const html=renderToStaticMarkup(createElement(BookingFlow,{slug:'fictional-holding',hotels,today:'2026-11-01',...(channel==='staff'?{staff}:{})}));
 expect(html).toContain('name="arrival"');expect(html).toContain('name="departure"');expect(fetcher).not.toHaveBeenCalled();
 function findForm(node:ReactNode):ComponentProps<'form'>|null{if(Array.isArray(node)){for(const child of node){const form=findForm(child);if(form)return form;}return null;}if(!isValidElement<ComponentProps<'form'>>(node))return null;if(node.type==='form')return node.props;return findForm(node.props.children);}
 const form=localOwners.localizedTrees.map(findForm).find(form=>form?.onSubmit);if(!form?.onSubmit)throw Error('The actual original Booking availability callback is required.');
 const OriginalFormData=globalThis.FormData;class SubmittedAvailabilityFields extends OriginalFormData{constructor(){super();this.set('arrival','2026-11-01');this.set('departure','2026-11-03');this.set('adults','2');this.set('children','7, 11');this.set('quantity','1');this.set('hotel','hotel-north');}}vi.stubGlobal('FormData',SubmittedAvailabilityFields);
 const preventDefault=vi.fn();Reflect.apply(form.onSubmit,undefined,[{preventDefault,currentTarget:{}}]);
 await vi.waitFor(()=>expect(fetcher).toHaveBeenCalledOnce());expect(preventDefault).toHaveBeenCalledOnce();
 const [url,request]=fetcher.mock.calls[0],search={arrival:'2026-11-01',departure:'2026-11-03',adults:2,childAges:[7,11],quantity:1,blockId:''};
 expect(url).toBe(channel==='public'?'/api/public/booking/fictional-holding/availability':'/api/v1/holdings/acme/pms/availability');
 expect(request?.method).toBe('POST');expect(JSON.parse(String(request?.body))).toEqual(channel==='public'?{search,propertyId:'hotel-north'}:{propertyId:'hotel-north',input:search});
 expect(localOwners.refresh).not.toHaveBeenCalled();expect(localOwners.push).not.toHaveBeenCalled();expect(localOwners.replace).not.toHaveBeenCalled();
});
it.each(['availability','quote','reservations','reservations?attempt=fictional%20%3F%26token'])('keeps the actual Booking public API %s URL byte-identical through the UI normalizer',suffix=>{
 for(const slug of ['fictional-holding','fictional / ?& holding']){
  const api=`/api/public/booking/${encodeURIComponent(slug)}/${suffix}`;
  expect(canonicalUiHref(api)).toBe(api);
 }
});

import {StandardPmsImport as PmsImport} from '../../../src/components/pms/standard-pms-import';
import {createPmsImportCommands,type PmsImportSource} from '../../../src/lib/pms/import-command';
import {setContextSwitching} from '../../../src/lib/work_projection/context-switch';
import {pmsImportSchema,type PmsImportReport} from '../../../src/server/modules/pms/public/rules/pms-import';
import {createHash as importFixtureHash} from 'node:crypto';
let importFixtureNonce=0;
const importFixtureKey=(n:number)=>`10000000-0000-4000-8000-${String(n).padStart(12,'0')}`;
const importFixtureId=()=> 'pms-import-'+importFixtureHash('sha256').update(JSON.stringify(['company',importFixtureKey(Math.max(1,importFixtureNonce))])).digest('hex').slice(0,40);
const importReport:PmsImportReport={errors:[],counts:{roomTypes:0,rooms:0,rates:0,guests:0,reservations:0},totals:{due:0,advance:0,advanceTax:0,deposit:0},canApprove:true};
const importBatch={get id(){return importFixtureId();},state:'preview' as const,version:1,created_by:'preparer',approved_by:null,opening_journal_id:null,report:importReport};
const importDefinition=pmsImportSchema.parse({propertyId:'hotel-north',expectedVersion:3,cutoverDate:'2026-10-16',currency:'THB',source:'fictional-cutover',roomTypes:[],rooms:[],rates:[],guests:[],reservations:[]});
const importApi='/api/v1/holdings/acme/pms/imports';
type ImportAction='preview'|'approve'|'apply';
function importInput(action:ImportAction){return action==='preview'?importDefinition:action==='approve'?{id:importBatch.id,expectedVersion:1,openingJournalId:null}:{id:importBatch.id,expectedVersion:2};}
function importReceipt(action:ImportAction){return action==='preview'?{id:importBatch.id,version:1,state:'preview',report:importReport}:action==='approve'?{id:importBatch.id,version:2,state:'approved'}:{id:importBatch.id,version:3,state:'applied',propertyId:'hotel-north',propertyVersion:4};}
async function actualPmsImportController(){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),sourceText=await readFile(new URL('../../../src/components/pms/standard-pms-import.tsx',import.meta.url),'utf8'),file=ts.createSourceFile('actual-pms-import.tsx',sourceText,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),functions:import('typescript').FunctionDeclaration[]=[];
 const visit=(node:import('typescript').Node)=>{if(ts.isFunctionDeclaration(node)&&node.name?.text==='run')functions.push(node);ts.forEachChild(node,visit);};visit(file);if(functions.length!==1)throw Error('Expected the original PMS Import run controller.');
 const emitted=ts.transpileModule(functions[0].getText(file),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const names=['busy','flight','isCurrentOwner','draft','setBusy','setError','attempts','setBatches','router','setUncertain','action','input'],execute=new Function(...names,`${emitted}\nreturn run(action,input);`) as (...ports:unknown[])=>Promise<void>;
 const source={current:{api:importApi,propertyId:'hotel-north',propertyVersion:3} as PmsImportSource|null},attempts={current:createPmsImportCommands(()=>source.current)},setBusy=vi.fn(),setError=vi.fn(),setBatches=vi.fn(),setUncertain=vi.fn(),router={refresh:vi.fn()};
 const run=(action?:ImportAction,input?:unknown)=>execute(false,{current:false},true,{capture:()=>undefined},setBusy,setError,attempts,setBatches,router,setUncertain,action,input);
 return {attempts,source,setBusy,setError,setBatches,setUncertain,router,run,retry:()=>run()};
}
function importBrowser(){
 const originalCrypto=globalThis.crypto;importFixtureNonce=0;vi.stubGlobal('crypto',{subtle:originalCrypto.subtle,randomUUID:()=>importFixtureKey(++importFixtureNonce)});
 const node={isConnected:true,dataset:{workUser:'operator',workRevision:'7',workCompany:'company',workAssignment:'assignment',workStale:'false'}},browser=Object.assign(new EventTarget(),{location:{origin:'https://pms-dev-erpx.jesadakorn.com'}}),events:{type:string;detail?:unknown}[]=[];
 for(const type of ['erpx:record-command-saved','erpx:work-changed'])browser.addEventListener(type,event=>events.push({type,detail:(event as CustomEvent).detail}));
 // The actual standalone Import screen has text inputs but no enclosing form.
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'&&node.isConnected?node:null,activeElement:{closest:()=>null},documentElement:{dataset:{}}});
 return {node,events};
}

it.each([
 ['preview','revision'],['preview','stale boundary'],['preview','removed boundary'],['preview','switch generation'],['approve','revision'],['apply','revision'],
] as const)('actual PMS Import %s keeps its key and withholds effects after late %s in POST body',async(action,changed)=>{
 const {node,events}=importBrowser(),refreshedBatch={...importBatch,state:importReceipt(action).state,version:importReceipt(action).version,approved_by:action==='preview'?null:'finance-reviewer'};let bodyStarted!:()=>void,releaseBody!:()=>void,posts=0;
 const started=new Promise<void>(resolve=>{bodyStarted=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class OriginalBody extends Response{override async json(){bodyStarted();await released;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?(++posts===1?new OriginalBody(JSON.stringify({data:importReceipt(action)}),{headers:{'content-type':'application/json'}}):Response.json({data:importReceipt(action)})):Response.json({data:[refreshedBatch]}));vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController(),input=importInput(action),capturedBody=JSON.stringify({action,input});
 try{
  const pending=controller.run(action,input);await started;const eventsBeforeBody=[...events];
  if(changed==='revision')node.dataset.workRevision='8';else if(changed==='stale boundary')node.dataset.workStale='true';else if(changed==='removed boundary')node.isConnected=false;else setContextSwitching(true);
  releaseBody();await pending;
  const refused={events:[...events],batches:controller.setBatches.mock.calls.length,reads:network.mock.calls.filter(call=>call[1]?.method==='GET').length,refresh:controller.router.refresh.mock.calls.length,retained:Number(controller.attempts.current.hasPending()),error:controller.setError.mock.calls.at(-1)?.[0]};
  node.dataset.workRevision='7';node.dataset.workStale='false';node.isConnected=true;setContextSwitching(false);await controller.retry();
  const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=mutations.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));
  expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(mutations.map(call=>call[1]?.body)).toEqual([capturedBody,capturedBody]);
  expect(eventsBeforeBody).toEqual([]);expect(refused).toMatchObject({events:[],batches:0,reads:0,refresh:0,retained:1});expect(refused.error).toContain('Return to the original work context');
  expect(Number(controller.attempts.current.hasPending())).toBe(0);expect(controller.setBatches).toHaveBeenCalledExactlyOnceWith([refreshedBatch]);expect(controller.router.refresh).toHaveBeenCalledOnce();expect(controller.setBusy.mock.calls.at(-1)?.[0]).toBe(false);
 }finally{releaseBody();setContextSwitching(false);vi.unstubAllGlobals();}
});

it('actual PMS Import keeps its exact submitted command pending until the follow-up list can be acknowledged in the original context',async()=>{
 const {node,events}=importBrowser();let bodyStarted!:()=>void,releaseBody!:()=>void;
 const started=new Promise<void>(resolve=>{bodyStarted=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class OriginalListBody extends Response{override async json(){bodyStarted();await released;return super.json();}}
 let reads=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:importReceipt('preview')}):++reads===1?new OriginalListBody(JSON.stringify({data:[importBatch]}),{headers:{'content-type':'application/json'}}):Response.json({data:[importBatch]}));vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController(),pending=controller.run('preview',importInput('preview'));await started;node.dataset.workRevision='8';releaseBody();await pending;
 expect(Number(controller.attempts.current.hasPending())).toBe(1);expect(network.mock.calls.filter(call=>call[1]?.method==='POST')).toHaveLength(1);expect(events).toEqual([]);
 expect(controller.setBatches).not.toHaveBeenCalled();expect(controller.router.refresh).not.toHaveBeenCalled();expect(controller.setError.mock.calls.at(-1)?.[0]).toContain('Return to the original work context');
 node.dataset.workRevision='7';await controller.retry();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=mutations.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[1]).toBe(keys[0]);expect(mutations[1][1]?.body).toBe(mutations[0][1]?.body);expect(controller.setBatches).toHaveBeenCalledExactlyOnceWith([importBatch]);expect(controller.router.refresh).toHaveBeenCalledOnce();expect(Number(controller.attempts.current.hasPending())).toBe(0);
});

it.each([null,{...importReceipt('preview'),version:0},{...importReceipt('preview'),state:'applied'}])('actual PMS Import retains an incomplete owner receipt %j without list reads or completion',async malformed=>{
 const {events}=importBrowser();let posts=0;
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:++posts===1?malformed:importReceipt('preview')}):Response.json({data:[importBatch]}));vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController(),input=importInput('preview');await controller.run('preview',input);
 const unconfirmed={retained:Number(controller.attempts.current.hasPending()),events:[...events],batches:controller.setBatches.mock.calls.length,reads:network.mock.calls.filter(call=>call[1]?.method==='GET').length,refresh:controller.router.refresh.mock.calls.length};
 await controller.retry();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=mutations.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));
 expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(mutations.map(call=>call[1]?.body)).toEqual([JSON.stringify({action:'preview',input}),JSON.stringify({action:'preview',input})]);expect(unconfirmed).toEqual({retained:1,events:[],batches:0,reads:0,refresh:0});expect(controller.setBatches).toHaveBeenCalledExactlyOnceWith([importBatch]);expect(controller.router.refresh).toHaveBeenCalledOnce();
});

it('actual PMS Import keeps its original payload/key after unreadable JSON without premature saved events',async()=>{
 const {events}=importBrowser();let posts=0;
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?(++posts===1?new Response('{"data":',{headers:{'content-type':'application/json'}}):Response.json({data:importReceipt('preview')})):Response.json({data:[importBatch]}));vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController(),input=importInput('preview');await controller.run('preview',input);const premature=[...events];expect(Number(controller.attempts.current.hasPending())).toBe(1);expect(controller.setBatches).not.toHaveBeenCalled();expect(controller.router.refresh).not.toHaveBeenCalled();
 await controller.retry();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=mutations.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(mutations.map(call=>call[1]?.body)).toEqual([JSON.stringify({action:'preview',input}),JSON.stringify({action:'preview',input})]);expect(premature).toEqual([]);
});

it('actual PMS Import retains its displayed batches when a confirmed command refresh has malformed data',async()=>{
 const {events}=importBrowser();let reads=0;
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?importReceipt('preview'):++reads===1?{unexpected:'not an import batch list'}:[importBatch]}));vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController();await controller.run('preview',importInput('preview'));expect(Number(controller.attempts.current.hasPending())).toBe(1);expect(events).toEqual([]);expect(controller.setBatches).not.toHaveBeenCalled();expect(controller.router.refresh).not.toHaveBeenCalled();expect(controller.setError.mock.calls.at(-1)?.[0]).toBeTruthy();
 await controller.retry();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=mutations.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[1]).toBe(keys[0]);expect(mutations[1][1]?.body).toBe(mutations[0][1]?.body);expect(Number(controller.attempts.current.hasPending())).toBe(0);expect(controller.setBatches).toHaveBeenCalledExactlyOnceWith([importBatch]);expect(controller.router.refresh).toHaveBeenCalledOnce();
});

it('renders the original standalone PMS Import JSON/journal fields and review controls without an enclosing form or mount request',()=>{
 const network=vi.fn();vi.stubGlobal('fetch',network);
 const html=renderToStaticMarkup(createElement(PmsImport,{api:importApi,base:'/holdings/acme',propertyId:'hotel-north',propertyVersion:3,businessDate:'2026-10-16',initial:[importBatch],userId:'finance-reviewer',canPrepare:true,canApprove:true}));
 expect(html).toContain('aria-label="Import JSON"');expect(html).toContain('Finance: approve this preview');expect(html).toContain('Posted opening journal ID');expect(html).toContain('Preview only');expect(html).not.toMatch(/<form[\s>]/);expect(network).not.toHaveBeenCalled();
});

it.each(['another hotel','disposed'] as const)('actual PMS Import retains the submitted source when %s replaces it during a body pause',async changed=>{
 const {events}=importBrowser();let bodyStarted!:()=>void,releaseBody!:()=>void,posts=0;
 const started=new Promise<void>(resolve=>{bodyStarted=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class OriginalBody extends Response{override async json(){bodyStarted();await released;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?(++posts===1?new OriginalBody(JSON.stringify({data:importReceipt('preview')}),{headers:{'content-type':'application/json'}}):Response.json({data:importReceipt('preview')})):Response.json({data:[importBatch]}));vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController(),pending=controller.run('preview',importInput('preview'));await started;
 controller.source.current=changed==='disposed'?null:{api:importApi,propertyId:'another-hotel'};releaseBody();await pending;expect(controller.attempts.current.hasPending()).toBe(true);expect(controller.setBatches).not.toHaveBeenCalled();expect(controller.router.refresh).not.toHaveBeenCalled();expect(events).toEqual([]);expect(network).toHaveBeenCalledOnce();
 await controller.retry();expect(network).toHaveBeenCalledOnce();expect(controller.setError.mock.calls.at(-1)?.[0]).toContain('Return to the original work context');
 controller.source.current={api:importApi,propertyId:'hotel-north'};await controller.retry();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(mutations[1][1]?.body).toBe(mutations[0][1]?.body);expect(new Headers(mutations[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));expect(controller.setBatches).toHaveBeenCalledExactlyOnceWith([importBatch]);expect(controller.router.refresh).toHaveBeenCalledOnce();
});

it('actual PMS Import blocks another pending action, changed JSON and a different role while retaining the original serialized request',async()=>{
 const {node}=importBrowser();let posts=0;
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>{if(init?.method==='POST'){if(++posts===1)throw Error('Original response lost');return Response.json({data:importReceipt('preview')});}return Response.json({data:[importBatch]});});vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController(),input={...importDefinition,source:'Original captured cutover'},original=JSON.stringify({action:'preview',input});await controller.run('preview',input);expect(controller.setUncertain.mock.calls.at(-1)?.[0]).toBe(true);
 input.source='Newer local draft';await controller.run('preview',input);await controller.run('approve',importInput('approve'));expect(network).toHaveBeenCalledOnce();expect(controller.setError.mock.calls.at(-1)?.[0]).toContain('Retry the submitted import action first');
 node.dataset.workAssignment='other-role';await controller.retry();expect(network).toHaveBeenCalledOnce();expect(controller.setError.mock.calls.at(-1)?.[0]).toContain('Return to the original work context');
 node.dataset.workAssignment='assignment';await controller.retry();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(mutations.map(call=>call[1]?.body)).toEqual([original,original]);expect(new Headers(mutations[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));expect(input.source).toBe('Newer local draft');expect(controller.attempts.current.hasPending()).toBe(false);expect(controller.setUncertain.mock.calls.at(-1)?.[0]).toBe(false);expect(controller.router.refresh).toHaveBeenCalledOnce();
});

it.each([false,true])('actual PMS Import preserves refusal certainty with prior unknown=%s',async previouslyUnknown=>{
 importBrowser();let posts=0;
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>{if(init?.method!=='POST')return Response.json({data:[importBatch]});++posts;if(previouslyUnknown&&posts===1)throw Error('Original response lost');if(posts===(previouslyUnknown?2:1))return Response.json({error:{code:'PMS_IMPORT_STALE',message:'Preview a fresh batch.'}},{status:409,headers:{'X-Request-ID':'fictional-request'}});return Response.json({data:importReceipt('preview')});});vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController();await controller.run('preview',importInput('preview'));if(previouslyUnknown)await controller.retry();expect(controller.setError.mock.calls.at(-1)?.[0]).toBe('Preview a fresh batch. (Reference: fictional-request)');expect(controller.attempts.current.hasPending()).toBe(previouslyUnknown);
 if(previouslyUnknown){await controller.run('preview',{...importDefinition,source:'Changed after unknown'});expect(posts).toBe(2);await controller.retry();}else await controller.run('preview',{...importDefinition,source:'Reviewed fresh cutover'});
 expect(controller.attempts.current.hasPending()).toBe(false);expect(controller.router.refresh).toHaveBeenCalledOnce();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=mutations.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));if(previouslyUnknown)expect(new Set(keys).size).toBe(1);else expect(keys[1]).not.toBe(keys[0]);
});

it.each([
 ['preview',{...importReceipt('preview'),version:2}],
 ['approve',{...importReceipt('approve'),id:'another-batch'}],
 ['apply',{...importReceipt('apply'),propertyId:'another-hotel'}],
] as const)('actual PMS Import %s keeps receipt lineage before acknowledging %j',async(action,foreign)=>{
 const {events}=importBrowser();let posts=0;
 const nativeBatch={...importBatch,state:importReceipt(action).state,version:importReceipt(action).version};
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?(++posts===1?foreign:importReceipt(action)):[nativeBatch]}));vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController();await controller.run(action,importInput(action));expect(controller.attempts.current.hasPending()).toBe(true);expect(controller.setBatches).not.toHaveBeenCalled();expect(events).toEqual([]);expect(network).toHaveBeenCalledOnce();await controller.retry();
 expect(controller.attempts.current.hasPending()).toBe(false);expect(controller.setBatches).toHaveBeenCalledExactlyOnceWith([nativeBatch]);expect(controller.router.refresh).toHaveBeenCalledOnce();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(new Headers(mutations[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));
});

it('PMS Import validates the final acknowledgement again and rejects retired completions without clearing a new pending request',async()=>{
 const {node,events}=importBrowser(),network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?importReceipt('preview'):[importBatch]}));vi.stubGlobal('fetch',network);
 const commands=createPmsImportCommands(()=>({api:importApi,propertyId:'hotel-north'})),first=await commands.run('preview',importInput('preview'));expect(commands.hasPending()).toBe(true);expect(events).toEqual([]);
 node.dataset.workRevision='8';expect(()=>first.acknowledge()).toThrow('Return to the original work context');expect(commands.hasPending()).toBe(true);expect(events).toEqual([]);
 node.dataset.workRevision='7';const recovered=await commands.retry();if(!recovered)throw Error('Expected the original pending request.');expect(recovered.acknowledge()).toBe(true);expect(recovered.acknowledge()).toBe(false);const next=await commands.run('preview',importInput('preview'));expect(first.acknowledge()).toBe(false);expect(commands.hasPending()).toBe(true);expect(next.acknowledge()).toBe(true);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed','erpx:record-command-saved','erpx:work-changed']);
 const keys=network.mock.calls.filter(call=>call[1]?.method==='POST').map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[1]).toBe(keys[0]);expect(keys[2]).not.toBe(keys[0]);
});

it('PMS Import keeps an originally absent form completion absent when another form becomes focused',async()=>{
 const {events}=importBrowser();let posts=0;
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>{if(init?.method==='POST'){if(++posts===1)throw Error('Original response lost');return Response.json({data:importReceipt('preview')});}return Response.json({data:[importBatch]});});vi.stubGlobal('fetch',network);
 const controller=await actualPmsImportController();await controller.run('preview',importInput('preview'));const newerForm={isConnected:true,getAttribute:()=> 'newer-edit'};Object.defineProperty(document,'activeElement',{configurable:true,value:{closest:()=>newerForm}});await controller.retry();
 expect(events).toEqual([{type:'erpx:record-command-saved',detail:{form:null,formEditVersion:null}},{type:'erpx:work-changed',detail:undefined}]);expect(controller.attempts.current.hasPending()).toBe(false);
});

import {betaFetch} from '../../../src/lib/shared/beta-client';
import {capturePmsHomeContext,sendPmsHomeCommand,type PmsHomeAttempt} from '../../../src/lib/pms/home-command';
import {bangkokToday} from '../../../src/lib/shared/handover';
import type {HomePost} from '../../../src/server/modules/collaboration/public/social-home';
const homeApi='/api/v1/front/acme/acme-co/pms/home';
const homePost:HomePost={id:'original-post',kind:'team',company_id:'company',branch_id:'branch-original',department_id:null,author_id:'operator',author_name:'Original operator',body:'Original native post',mentions_json:'[]',content_json:null,rich_content_json:null,audience_kind:'branch',audience_id:'branch-original',purpose:'update',edited_at:null,retracted_at:null,version:1,state:'published',linked_module:null,created_at:1,audience_name:'Original branch',audience_count:0,audienceLabel:'Original branch',aiName:'Assistant',content:null,richContent:null,mentions:[],linkedSources:[],comments:[],files:[],agentReplies:[],appreciationCount:0,appreciated:0,followed:0,commentCount:0,task:null,trackingState:null,hasTask:false};
function homeBrowser(){
 const {node,events}=importBrowser(),front={isConnected:true,dataset:{frontApp:'pms',frontLocation:'holding'}};
 vi.stubGlobal('navigator',{onLine:true});vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'&&node.isConnected?node:selector==='[data-front-app]'?front:null,activeElement:{closest:()=>null},documentElement:{dataset:{}}});
 return {node,front,events};
}
async function actualPmsHomeControllers(){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),source=await readFile(new URL(process.env.ERPX_PMS_HOME_BASELINE==='1'?'../../../src/components/pms/home.tsx':'../../../src/components/pms/standard-pms-today.tsx',import.meta.url),'utf8'),file=ts.createSourceFile('actual-pms-home.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),functions=new Map<string,import('typescript').FunctionDeclaration>(),sends:import('typescript').VariableDeclaration[]=[],commentCallbacks:import('typescript').ArrowFunction[]=[],globalRetries:import('typescript').ArrowFunction[]=[];
 const visit=(node:import('typescript').Node)=>{
  if(ts.isFunctionDeclaration(node)&&node.name&&['run','publish','publishHandover','act','retrySubmitted'].includes(node.name.text))functions.set(node.name.text,node);
  if(ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text==='send'&&node.initializer&&ts.isArrowFunction(node.initializer))sends.push(node);
  if(ts.isCallExpression(node)&&ts.isPropertyAccessExpression(node.expression)&&node.expression.name.text==='then'&&node.arguments[0]&&ts.isArrowFunction(node.arguments[0])&&node.arguments[0].getText(file).includes('setBody("")'))commentCallbacks.push(node.arguments[0]);
  if(ts.isArrowFunction(node)&&node.modifiers?.some(modifier=>modifier.kind===ts.SyntaxKind.AsyncKeyword)&&node.getText(file).includes('await send(attempt.endpoint, attempt.input)'))globalRetries.push(node);
  ts.forEachChild(node,visit);
 };visit(file);if(functions.size!==5||sends.length!==1||commentCallbacks.length!==1||globalRetries.length!==1)throw Error('Expected the exact PMS Home send/publish/handover/PostCard/retry controller bodies.');
 const statements=`const ${sends[0].getText(file)};\n${[...functions.values()].map(node=>node.getText(file)).join('\n')}\nconst afterComment=${commentCallbacks[0].getText(file)};`;
 const emitted=ts.transpileModule(statements,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const draft={body:'Captured composer text',mentions:[] as unknown[],doc:null as unknown,composer:true,handoverBody:'Captured source handover',selected:[{id:'stay-original',sourceVersion:3}],recipients:['recipient-original'],review:true};
 const ui={api:homeApi,property:'hotel-original',selectedBranch:'branch-original'},attempts={current:new Map<string,PmsHomeAttempt>()},setError=vi.fn(),setBusy=vi.fn(),setRetryPending=vi.fn(),onPending=vi.fn(),refresh=vi.fn(async()=>{}),onChanged=vi.fn(async()=>{}),setCurrent=vi.fn(),clears:string[]=[];
 const set=(key:keyof typeof draft)=>(value:unknown)=>{clears.push(key);Reflect.set(draft,key,value);};
 const homeSource={get current(){return {api:ui.api,property:ui.property,selectedBranch:ui.selectedBranch};}},composerDraft={get current(){return draft;}},postSource={current:{api:homeApi,property:homePost.id,selectedBranch:homePost.branch_id}},commentDraft={get current(){return {body:draft.body,mentions:draft.mentions};}};
 const names=['sendPmsHomeCommand','capturePmsHomeContext','homeSource','composerDraft','postSource','commentDraft','attempts','betaFetch','busy','setBusy','setPendingAttempts','setError','api','body','mentions','purpose','selectedBranch','doc','setBody','setMentions','setDoc','setComposer','refresh','bangkokToday','handoverBody','recipients','selected','setSelected','setRecipients','setReview','setHandoverBody','post','setCurrent','onChanged','retainedAttempt','setRetryPending','onPending'];
 const globalRetry=ts.transpileModule(`const retryGlobal=(attempt)=>run(${globalRetries[0].getText(file)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const build=new Function(...names,`${emitted}\n${globalRetry}\nreturn {publish,publishHandover,act,retrySubmitted,retryGlobal,comment:()=>act({action:'comment',body,mentions}).then(afterComment)};`) as (...ports:unknown[])=>{publish():Promise<void>;publishHandover():Promise<void>;act(input:unknown):Promise<boolean>;retrySubmitted():Promise<void>;retryGlobal(attempt:PmsHomeAttempt):Promise<void>;comment():Promise<void>};
 const handlers=build(sendPmsHomeCommand,capturePmsHomeContext,homeSource,composerDraft,postSource,commentDraft,attempts,betaFetch,false,setBusy,vi.fn(),setError,homeApi,draft.body,draft.mentions,'update',ui.selectedBranch,draft.doc,set('body'),set('mentions'),set('doc'),set('composer'),refresh,bangkokToday,draft.handoverBody,draft.recipients,draft.selected,set('selected'),set('recipients'),set('review'),set('handoverBody'),homePost,setCurrent,onChanged,(endpoint:string)=>[...attempts.current.values()].find(attempt=>attempt.endpoint===endpoint),setRetryPending,onPending);
 return {...handlers,draft,ui,attempts,setError,setBusy,setRetryPending,onPending,refresh,onChanged,setCurrent,clears};
}

it.each([
 ['publish','revision'],['publish','stale boundary'],['publish','removed boundary'],['publish','switch generation'],['publish','hotel/audience'],['publish','front location'],
 ['handover','revision'],['handover','hotel/audience'],['handover','front location'],
] as const)('actual PMS Home %s retains its captured command and draft after late %s in POST JSON',async(operation,changed)=>{
 const {node,front,events}=homeBrowser();let signalBody!:()=>void,releaseBody!:()=>void,calls=0;
 const started=new Promise<void>(resolve=>{signalBody=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class OriginalBody extends Response{override async json(){signalBody();await released;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>++calls===1?new OriginalBody(JSON.stringify({data:{id:'published-post',version:1}}),{headers:{'content-type':'application/json'}}):Response.json({data:{id:'published-post',version:1}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers(),submit=operation==='publish'?owner.publish:owner.publishHandover;
 try{
  const pending=submit();await started;const beforeBody=[...events];
  if(changed==='revision')node.dataset.workRevision='8';else if(changed==='stale boundary')node.dataset.workStale='true';else if(changed==='removed boundary')node.isConnected=false;else if(changed==='switch generation')setContextSwitching(true);else if(changed==='front location')front.dataset.frontLocation='branch:another';else {owner.ui.property='hotel-other';owner.ui.selectedBranch='branch-other';}
  owner.draft.body='Newer composer draft';owner.draft.handoverBody='Newer handover draft';owner.draft.doc={type:'doc',content:[]};const newer=structuredClone(owner.draft);
  releaseBody();await pending;const refused={clears:[...owner.clears],draft:structuredClone(owner.draft),refresh:owner.refresh.mock.calls.length,events:[...events],pending:owner.attempts.current.size};
  node.dataset.workRevision='7';node.dataset.workStale='false';node.isConnected=true;setContextSwitching(false);front.dataset.frontLocation='holding';owner.ui.property='hotel-original';owner.ui.selectedBranch='branch-original';await submit();
  const keys=network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(network.mock.calls[1][1]?.body).toBe(network.mock.calls[0][1]?.body);
  expect(beforeBody).toEqual([]);expect(refused).toEqual({clears:[],draft:newer,refresh:0,events:[],pending:1});expect(new Headers(network.mock.calls[0][1]?.headers).get('x-erpx-front-location')).toBe('holding');
 }finally{releaseBody();setContextSwitching(false);vi.unstubAllGlobals();}
});

it.each(['publish','handover'] as const)('actual PMS Home %s does not clear a newer composer input after a valid late acknowledgement',async operation=>{
 homeBrowser();let signalBody!:()=>void,releaseBody!:()=>void;
 const started=new Promise<void>(resolve=>{signalBody=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class OriginalBody extends Response{override async json(){signalBody();await released;return super.json();}}
 const network=vi.fn(async()=>new OriginalBody(JSON.stringify({data:{id:'published-post',version:1}}),{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers(),pending=operation==='publish'?owner.publish():owner.publishHandover();await started;
 owner.draft.body='Newer composer draft';owner.draft.handoverBody='Newer handover draft';owner.draft.doc={type:'doc',content:[]};owner.draft.recipients=['recipient-newer'];const newer=structuredClone(owner.draft);releaseBody();await pending;
 expect(owner.draft).toEqual(newer);expect(owner.clears).toEqual([]);expect(owner.attempts.current.size).toBe(0);
});

it.each(['publish','handover'] as const)('actual PMS Home %s retains an incomplete native publication receipt without clearing its draft',async operation=>{
 const {events}=homeBrowser();let calls=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>Response.json({data:++calls===1?{id:'published-post',version:0}:{id:'published-post',version:1}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers(),submit=operation==='publish'?owner.publish:owner.publishHandover;await submit();const refused={clears:[...owner.clears],refresh:owner.refresh.mock.calls.length,pending:owner.attempts.current.size,events:[...events]};await submit();
 const keys=network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(network.mock.calls[1][1]?.body).toBe(network.mock.calls[0][1]?.body);expect(refused).toEqual({clears:[],refresh:0,pending:1,events:[]});
});

it.each(['revision','front location'] as const)('actual PMS PostCard withholds its follow-up post, changed callback and comment clearing after late %s in GET JSON',async changed=>{
 const {node,front}=homeBrowser();let signalBody!:()=>void,releaseBody!:()=>void;
 const started=new Promise<void>(resolve=>{signalBody=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class OriginalBody extends Response{override async json(){signalBody();await released;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:{id:homePost.id}}):new OriginalBody(JSON.stringify({data:homePost}),{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers(),pending=owner.comment();await started;if(changed==='revision')node.dataset.workRevision='8';else front.dataset.frontLocation='branch:another';owner.draft.body='Newer comment draft';releaseBody();await pending;
 expect(owner.setCurrent).not.toHaveBeenCalled();expect(owner.onChanged).not.toHaveBeenCalled();expect(owner.draft.body).toBe('Newer comment draft');expect(owner.clears).toEqual([]);
});

it('actual PMS PostCard retains a newer comment after a same-context valid receipt and post read',async()=>{
 homeBrowser();let signalBody!:()=>void,releaseBody!:()=>void;
 const started=new Promise<void>(resolve=>{signalBody=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class OriginalBody extends Response{override async json(){signalBody();await released;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:{id:homePost.id}}):new OriginalBody(JSON.stringify({data:homePost}),{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers(),pending=owner.comment();await started;owner.draft.body='Newer comment draft';releaseBody();await pending;expect(owner.draft.body).toBe('Newer comment draft');expect(owner.clears).toEqual([]);
});

it('actual PMS PostCard rejects a follow-up post from another native id before changing its displayed source',async()=>{
 homeBrowser();const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?{id:homePost.id}:{...homePost,id:'another-post'}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers();expect(await owner.act({action:'appreciate',active:true})).toBe(false);expect(owner.setCurrent).not.toHaveBeenCalled();expect(owner.onChanged).not.toHaveBeenCalled();
});

it.each(['revision','front location'] as const)('actual PMS Home keeps the original comment key and completion pending through late %s in the required GET body',async changed=>{
 const {node,front,events}=homeBrowser();let startBody!:()=>void,releaseBody!:()=>void,reads=0;
 const started=new Promise<void>(resolve=>{startBody=resolve;}),released=new Promise<void>(resolve=>{releaseBody=resolve;});
 class NativePostBody extends Response{override async json(){startBody();await released;return super.json();}}
 const updated={...homePost,version:2};
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:{id:homePost.id}}):++reads===1?new NativePostBody(JSON.stringify({data:updated}),{headers:{'content-type':'application/json'}}):Response.json({data:updated}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers(),first=owner.comment();await started;
 const beforeGet={pending:owner.attempts.current.size,events:[...events]};
 if(changed==='revision')node.dataset.workRevision='8';else front.dataset.frontLocation='branch:another';owner.draft.body='Newer retained comment';releaseBody();await first;
 const refused={pending:owner.attempts.current.size,events:[...events]};node.dataset.workRevision='7';front.dataset.frontLocation='holding';await owner.act({action:'comment',body:'Captured composer text',mentions:[]});
 const posts=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=posts.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));
 expect(beforeGet).toEqual({pending:1,events:[]});expect(refused).toEqual({pending:1,events:[]});expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);
 expect(owner.draft.body).toBe('Newer retained comment');expect(owner.attempts.current.size).toBe(0);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);
});

it.each(['unreadable','foreign source','unavailable'] as const)('actual PMS Home retains its submitted comment after a committed POST and %s follow-up GET, then reuses the exact request',async failure=>{
 const {events}=homeBrowser();let reads=0;
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:{id:homePost.id}}):++reads===1?failure==='unreadable'?new Response('{"data":',{headers:{'content-type':'application/json'}}):failure==='unavailable'?Response.json({error:{message:'Original post read unavailable'}},{status:500}):Response.json({data:{...homePost,id:'another-source',version:2}}):Response.json({data:{...homePost,version:2}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers();await owner.comment();const refused={pending:owner.attempts.current.size,events:[...events],clears:[...owner.clears]};await owner.comment();
 const posts=network.mock.calls.filter(call=>call[1]?.method==='POST'),keys=posts.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));
 expect(refused).toEqual({pending:1,events:[],clears:[]});expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(owner.attempts.current.size).toBe(0);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);
});

it.each([['publish',2],['publish',99],['handover',2],['handover',99]] as const)('actual PMS Home %s requires native version1 instead of positive version%i',async(operation,version)=>{
 const {events}=homeBrowser();let posts=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>Response.json({data:{id:'published-post',version:++posts===1?version:1}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers(),submit=operation==='publish'?owner.publish:owner.publishHandover;await submit();const refused={pending:owner.attempts.current.size,events:[...events],clears:[...owner.clears]};await submit();
 const keys=network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(refused).toEqual({pending:1,events:[],clears:[]});expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(network.mock.calls[1][1]?.body).toBe(network.mock.calls[0][1]?.body);
});

it.each([undefined,4,6])('actual PMS Home edit receipt requires captured CAS+1 instead of native version%j',async version=>{
 const {events}=homeBrowser(),attempts=new Map<string,PmsHomeAttempt>(),input={action:'edit',expectedVersion:4,body:'Captured edited post'},source=()=>({api:homeApi,property:'hotel-original',selectedBranch:'branch-original'});
 vi.stubGlobal('fetch',vi.fn(async()=>Response.json({data:{id:homePost.id,...(version===undefined?{}:{version})}})));
 await expect(sendPmsHomeCommand(attempts,source,`${homeApi}/posts/${homePost.id}`,input)).rejects.toThrow('acknowledgement');expect(attempts.size).toBe(1);expect(events).toEqual([]);
});

it.each([false,true])('actual PMS Home reachable submitted-comment retry preserves a newer draft=%j and reuses only the original body/key',async newer=>{
 const {events}=homeBrowser();let reads=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:{id:homePost.id}}):++reads===1?new Response('{"data":',{headers:{'content-type':'application/json'}}):Response.json({data:{...homePost,version:2}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers();await owner.comment();expect(owner.setRetryPending).toHaveBeenLastCalledWith(true);expect(owner.onPending).toHaveBeenCalledOnce();if(newer)owner.draft.body='Newer visible comment';await owner.retrySubmitted();
 const posts=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(JSON.parse(String(posts[1][1]?.body)).body).toBe('Captured composer text');expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));
 expect(owner.draft.body).toBe(newer?'Newer visible comment':'');expect(owner.setRetryPending).toHaveBeenLastCalledWith(false);expect(owner.attempts.current.size).toBe(0);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);
});

it('actual PMS Home global original-action retry validates the post read before its completion and retains the current draft',async()=>{
 const {events}=homeBrowser();let reads=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?Response.json({data:{id:homePost.id}}):++reads===1?Response.json({data:{...homePost,id:'foreign-post'}}):Response.json({data:{...homePost,version:2}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers();await owner.comment();owner.draft.body='Current newer comment';const attempt=[...owner.attempts.current.values()][0];if(!attempt)throw Error('Expected the original retained comment');await owner.retryGlobal(attempt);
 expect(reads).toBe(2);expect(owner.draft.body).toBe('Current newer comment');expect(owner.attempts.current.size).toBe(0);expect(owner.refresh).toHaveBeenCalledOnce();expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);const posts=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));
});

it('actual PMS Home withholds post publication and completion when context changes in its required onChanged body',async()=>{
 const {node,events}=homeBrowser();let start!:()=>void,release!:()=>void;const started=new Promise<void>(resolve=>{start=resolve;}),released=new Promise<void>(resolve=>{release=resolve;});
 class NativeAggregateBody extends Response{override async json(){start();await released;return super.json();}}
 const aggregate=new NativeAggregateBody(JSON.stringify({data:{updated:true}}),{headers:{'content-type':'application/json'}});
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?{id:homePost.id}:{...homePost,version:2}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers();owner.onChanged.mockImplementationOnce(async()=>{await aggregate.json();});const pending=owner.comment();await started;expect(owner.attempts.current.size).toBe(1);expect(events).toEqual([]);expect(owner.setCurrent).not.toHaveBeenCalled();node.dataset.workRevision='8';release();await pending;
 expect(owner.attempts.current.size).toBe(1);expect(events).toEqual([]);expect(owner.setCurrent).not.toHaveBeenCalled();expect(owner.draft.body).toBe('Captured composer text');node.dataset.workRevision='7';await owner.retrySubmitted();const posts=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));
});

it.each([null,{...homePost,version:0},{...homePost,version:'2'},{...homePost,company_id:'another-company'},{...homePost,branch_id:'another-branch'}])('actual PMS Home rejects malformed or foreign post projection %j before consuming the original comment',async malformed=>{
 const {events}=homeBrowser();vi.stubGlobal('fetch',vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?{id:homePost.id}:malformed})));
 const owner=await actualPmsHomeControllers();await owner.comment();expect(owner.attempts.current.size).toBe(1);expect(events).toEqual([]);expect(owner.setCurrent).not.toHaveBeenCalled();expect(owner.onChanged).not.toHaveBeenCalled();expect(owner.clears).toEqual([]);
});

it.each(['appreciate','follow'] as const)('actual PMS Home preserves native id-only %s receipts and unchanged positive post versions',async action=>{
 const {events}=homeBrowser();vi.stubGlobal('fetch',vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?{id:homePost.id}:homePost})));
 const owner=await actualPmsHomeControllers();expect(await owner.act({action,active:true})).toBe(true);expect(owner.setCurrent).toHaveBeenCalledExactlyOnceWith(homePost);expect(owner.attempts.current.size).toBe(0);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);
});

it('actual PMS Home accepts only captured edit CAS+1 and each receipt closure consumes the original attempt once',async()=>{
 const {events}=homeBrowser(),attempts=new Map<string,PmsHomeAttempt>(),input={action:'edit',expectedVersion:4,body:'Captured edited post'},source=()=>({api:homeApi,property:'hotel-original',selectedBranch:'branch-original'});
 const network=vi.fn(async()=>Response.json({data:{id:homePost.id,version:5}}));vi.stubGlobal('fetch',network);
 const first=await sendPmsHomeCommand(attempts,source,`${homeApi}/posts/${homePost.id}`,input),replay=await sendPmsHomeCommand(attempts,source,`${homeApi}/posts/${homePost.id}`,input);expect(first.acknowledge()).toBe(false);expect(attempts.size).toBe(1);expect(replay.data).toEqual({id:homePost.id,version:5});expect(replay.acknowledge()).toBe(true);expect(replay.acknowledge()).toBe(false);expect(attempts.size).toBe(0);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);
 const newer=await sendPmsHomeCommand(attempts,source,`${homeApi}/posts/${homePost.id}`,input);expect(replay.acknowledge()).toBe(false);expect(attempts.size).toBe(1);expect(newer.acknowledge()).toBe(true);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed','erpx:record-command-saved','erpx:work-changed']);
});

it('actual PMS Handover can correct a refreshed source after a first authoritative prewrite source rejection without losing its draft',async()=>{
 const {events}=homeBrowser();let calls=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>++calls===1?Response.json({error:{code:'HANDOVER_SOURCE_CHANGED',message:'Selected hotel work changed. Refresh before publishing.'}},{status:409}):Response.json({data:{id:'published-post',version:1}}));vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers();await owner.publishHandover();const rejected={pending:owner.attempts.current.size,events:[...events],clears:[...owner.clears],body:owner.draft.handoverBody};owner.draft.selected[0].sourceVersion=4;await owner.publishHandover();
 expect(rejected).toEqual({pending:0,events:[],clears:[],body:'Captured source handover'});expect(network).toHaveBeenCalledTimes(2);const keys=network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[0]).toBeTruthy();expect(keys[1]).toBeTruthy();expect(keys[1]).not.toBe(keys[0]);expect(JSON.parse(String(network.mock.calls[0][1]?.body)).sources[0].sourceVersion).toBe(3);expect(JSON.parse(String(network.mock.calls[1][1]?.body)).sources[0].sourceVersion).toBe(4);expect(owner.attempts.current.size).toBe(0);expect(events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);
});

it('actual PMS Handover retains the original key after a lost response followed by the same source rejection and blocks a different draft',async()=>{
 const {events}=homeBrowser();let calls=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>{if(++calls===1)throw Error('Original response lost');return Response.json({error:{code:'HANDOVER_SOURCE_CHANGED',message:'Selected hotel work changed. Refresh before publishing.'}},{status:409});});vi.stubGlobal('fetch',network);
 const owner=await actualPmsHomeControllers();await owner.publishHandover();await owner.publishHandover();expect(owner.attempts.current.size).toBe(1);owner.draft.selected[0].sourceVersion=4;await owner.publishHandover();
 expect(network).toHaveBeenCalledTimes(2);const keys=network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'));expect(keys[0]).toBeTruthy();expect(keys[1]).toBe(keys[0]);expect(network.mock.calls[1][1]?.body).toBe(network.mock.calls[0][1]?.body);const pending=[...owner.attempts.current.values()][0];expect(JSON.parse(pending.body).sources[0].sourceVersion).toBe(3);expect(owner.draft.selected[0].sourceVersion).toBe(4);expect(owner.draft.handoverBody).toBe('Captured source handover');expect(owner.clears).toEqual([]);expect(events).toEqual([]);expect(owner.setError).toHaveBeenLastCalledWith(expect.stringContaining('Retry the submitted action first'));
});

// Public-only reservation recovery. Original guest/staff cases above stay exact.
import * as reservationReact from 'react';
import {createRequire as reservationRequire} from 'node:module';
import {readFileSync as reservationRead} from 'node:fs';
import {resolve as reservationResolve} from 'node:path';
import {createPublicReservationClient,publicBookingCheckoutUrl,type PublicReservationInput,type PublicReservationScope,type PublicReservationReceipt} from '../../../src/lib/pms/public-booking-reservation-command';
import {money as reservationMoney,plusDays as reservationPlusDays} from '../../../src/server/modules/pms/public/rules/pms-model';
import type {BookingQuote as ReservationQuote} from '../../../src/server/modules/pms/public/rules/pms-booking';
const reservationInput=():PublicReservationInput=>({propertyId:'hotel-north',quoteToken:'original-native-quote',guest:{name:'Original guest',email:'guest@example.test',phone:'000'},notes:'Original notes'});
const reservationReceipt=(online=false):PublicReservationReceipt=>({id:'native-reservation',number:'BK-original',version:1,state:online?'payment_pending':'confirmed',hotel:{id:'hotel-north',name:'Original north hotel',address:'north address',contactEmail:'north@example.test',contactPhone:'000'},guest:{name:'Original guest',email:'guest@example.test'},currency:'THB',total:10000,paymentState:'pay_at_hotel',managementToken:'original-management-token',...(online?{attemptToken:'native-payment.signature',expiresAt:1792296000000,checkoutUrl:'https://checkout.stripe.com/c/pay/original'}:{})});
const reservationScope=():PublicReservationScope=>({base:'/api/public/booking/acme',propertyId:'hotel-north',owner:'original-page',draft:1});
const reservationReply=(value:unknown)=>Response.json({data:value});
function reservationDeferred(){let resolve!:()=>void;const promise=new Promise<void>(done=>{resolve=done;});return {promise,resolve};}
it('retains immutable original reservation body/key after an unknown commit, rejects another draft and recovers the original receipt',async()=>{
 const source=reservationScope(),input=reservationInput(),network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>().mockRejectedValueOnce(Error('Original native response lost')).mockResolvedValueOnce(reservationReply(reservationReceipt())),commands=createPublicReservationClient(()=>source,network,()=> '11111111-1111-4111-8111-111111111111');
 await expect(commands.submit(input)).rejects.toThrow('Original native response lost');input.guest.name='Newer guest';input.notes='Newer notes';source.draft++;
 expect(()=>commands.submit(input)).toThrow('Recover the original reservation');expect(network).toHaveBeenCalledTimes(1);
 const recovered=await commands.recover();expect(recovered.unchanged()).toBe(false);expect(recovered.acknowledge()).toBe(true);expect(recovered.acknowledge()).toBe(false);expect(commands.hasPending()).toBe(false);
 expect(network.mock.calls[1]).toEqual(network.mock.calls[0]);expect(JSON.parse(String(network.mock.calls[1][1]?.body))).toEqual(reservationInput());expect(new Headers(network.mock.calls[1][1]?.headers).get('Idempotency-Key')).toBe('11111111-1111-4111-8111-111111111111');
});
it.each([403,409,422,503])('retains the original reservation after a first potentially post-commit HTTP%s checkout failure and a later quote rejection',async status=>{
 const input=reservationInput(),network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>().mockResolvedValueOnce(Response.json({error:{code:'PMS_GUEST_EMAIL_REQUIRED',message:'Checkout source unavailable'}},{status})).mockResolvedValueOnce(Response.json({error:{code:'PMS_QUOTE_EXPIRED',message:'Later quote rejection'}},{status:409})).mockResolvedValueOnce(reservationReply(reservationReceipt(true))),commands=createPublicReservationClient(reservationScope,network);
 await expect(commands.submit(input)).rejects.toThrow('Checkout source unavailable');expect(commands.hasPending()).toBe(true);await expect(commands.recover()).rejects.toThrow('Later quote rejection');expect(commands.hasPending()).toBe(true);const result=await commands.recover();expect(result.acknowledge()).toBe(true);expect(new Set(network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).size).toBe(1);expect(network.mock.calls.every(call=>call[1]?.body===JSON.stringify(input))).toBe(true);
});
it.each([{status:409,code:'PMS_QUOTE_EXPIRED'},{status:409,code:'PMS_QUOTE_CHANGED'},{status:422,code:'PMS_QUOTE_INVALID'}])('permits a fresh quote only after the first native pre-effect $code rejection',async denial=>{
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>().mockResolvedValueOnce(Response.json({error:{code:denial.code,message:'Review a fresh quote'}},{status:denial.status})).mockResolvedValueOnce(reservationReply(reservationReceipt())),commands=createPublicReservationClient(reservationScope,network);
 await expect(commands.submit(reservationInput())).rejects.toThrow('Review a fresh quote');expect(commands.hasPending()).toBe(false);const result=await commands.submit({...reservationInput(),quoteToken:'fresh-native-quote'});expect(result.acknowledge()).toBe(true);expect(new Headers(network.mock.calls[1][1]?.headers).get('Idempotency-Key')).not.toBe(new Headers(network.mock.calls[0][1]?.headers).get('Idempotency-Key'));
});
it.each(['tenant','property','owner','unmount'] as const)('guards the original reservation %s after delayed ORIGINAL JSON and retains the first attempt',async changed=>{
 let current:PublicReservationScope|null=reservationScope();const original=current,started=reservationDeferred(),held=reservationDeferred();class OriginalReservationJSON extends Response{override async json(){started.resolve();await held.promise;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>().mockResolvedValueOnce(new OriginalReservationJSON(JSON.stringify({data:reservationReceipt(true)}))).mockResolvedValueOnce(reservationReply(reservationReceipt(true))),commands=createPublicReservationClient(()=>current,network),pending=commands.submit(reservationInput());await started.promise;
 if(changed==='unmount')current=null;else current={...original,...(changed==='tenant'?{base:'/api/public/booking/other'}:changed==='property'?{propertyId:'other-property'}:{owner:'different-page'})};held.resolve();await expect(pending).rejects.toMatchObject({name:'AbortError'});expect(commands.hasPending()).toBe(true);current=original;const outcome=await commands.recover();current={...original,owner:'changed-before-ack'};expect(()=>outcome.acknowledge()).toThrow('booking page changed');expect(commands.hasPending()).toBe(true);current=original;expect(outcome.acknowledge()).toBe(true);expect(network.mock.calls[1]).toEqual(network.mock.calls[0]);
});
it.each(['identity','version','hotel','guest','checkout','state'] as const)('retains malformed native reservation %s without treating it as a confirmed booking',async kind=>{
 const receipt=reservationReceipt(true),bad=kind==='identity'?{...receipt,id:''}:kind==='version'?{...receipt,version:0}:kind==='hotel'?{...receipt,hotel:{...receipt.hotel,id:'foreign-hotel'}}:kind==='guest'?{...receipt,guest:{...receipt.guest,email:'foreign@example.test'}}:kind==='checkout'?{...receipt,checkoutUrl:'https://another.example/checkout'}:{...receipt,state:'invented-status'};
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>().mockResolvedValueOnce(reservationReply(bad)).mockResolvedValueOnce(reservationReply(receipt)),commands=createPublicReservationClient(reservationScope,network);await expect(commands.submit(reservationInput())).rejects.toThrow('confirmation is incomplete');expect(commands.hasPending()).toBe(true);expect((await commands.recover()).acknowledge()).toBe(true);expect(network.mock.calls[1]).toEqual(network.mock.calls[0]);
});
const reservationQuote=(online=false):ReservationQuote=>({propertyId:'hotel-north',currency:'THB',total:10000,dueNow:online?10000:0,dueAtHotel:online?0:10000,lines:[{typeId:'type-original',rateId:'rate-original',arrival:'2026-11-01',departure:'2026-11-02',adults:2,childAges:[],quantity:1,roomName:'Original room',rateName:'Original public rate',inclusions:'Breakfast',policy:undefined,nights:[{date:'2026-11-01',net:10000,service:0,tax:0,total:10000}],total:10000,dueNow:online?10000:0,dueAtHotel:online?0:10000,collection:{method:online?'full':'pay_at_hotel',depositPercent:30,changesAllowed:true}}]});
async function reservationFlush(){for(let index=0;index<12;index++)await Promise.resolve();}
function mountPublicReservation(network:(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>){
 const require=reservationRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),root=process.env.ERPX_PUBLIC_RESERVATION_SOURCE_ROOT??'.',baseline=process.env.ERPX_PUBLIC_RESERVATION_BASELINE==='1',file=reservationResolve(baseline?process.env.ERPX_PUBLIC_RESERVATION_BASELINE_ROOT??'.scratch/dev/public-booking-reservation-standard-next/before':root,`src/components/pms/${baseline?'booking-flow':'public-booking-reservation'}.tsx`),source=reservationRead(file,'utf8'),emitted=ts.transpileModule(source,{compilerOptions:{jsx:ts.JsxEmit.ReactJSX,target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 type Hook={value?:unknown;current?:unknown;deps?:readonly unknown[];cleanup?:()=>void};const hooks:Hook[]=[],effects:(()=>void)[]=[];let cursor=0;
 const state=(initial:unknown)=>{const index=cursor++;if(!hooks[index])hooks[index]={value:typeof initial==='function'?initial():initial};return [hooks[index].value,(next:unknown)=>{hooks[index].value=typeof next==='function'?next(hooks[index].value):next;}];},ref=(initial:unknown)=>{const index=cursor++;return hooks[index]??(hooks[index]={current:initial});},effect=(run:()=>void|(()=>void),deps?:readonly unknown[])=>{const index=cursor++,old=hooks[index];if(!old||!deps||!old.deps||deps.length!==old.deps.length||deps.some((value,part)=>!Object.is(value,old.deps?.[part]))){effects.push(()=>{old?.cleanup?.();const cleanup=run();hooks[index]={deps,cleanup:typeof cleanup==='function'?cleanup:undefined};});}};
 const callback=(value:unknown,deps:readonly unknown[])=>{const index=cursor++,old=hooks[index];if(!old||!old.deps||deps.length!==old.deps.length||deps.some((part,key)=>!Object.is(part,old.deps?.[key])))hooks[index]={value,deps};return hooks[index].value;};
 const location={href:'https://booking-dev-erpx.jesadakorn.com/book/acme/hotel-north',origin:'https://booking-dev-erpx.jesadakorn.com',pathname:'/book/acme/hotel-north',assign:vi.fn()},browser=Object.assign(new EventTarget(),{location,setInterval:vi.fn(),clearInterval:vi.fn()}),document=Object.assign(new EventTarget(),{documentElement:{dataset:{}},querySelector:()=>null});vi.stubGlobal('window',browser);vi.stubGlobal('document',document);vi.stubGlobal('fetch',network);
 const primitives=new Proxy<Record<string,unknown>>({}, {get:(_target,key)=>Object.assign((props:Record<string,unknown>)=>reservationReact.createElement(key==='Button'?'button':key==='FormLayout'?'form':key==='Field'?'label':key==='TextInput'?'input':key==='Select'?'select':key==='TextArea'?'textarea':key==='ActionAnchor'?'a':'section',props,props.children as ReactNode),{displayName:String(key)})}),evaluatedModule={exports:{} as Record<string,unknown>},localRequire=(name:string)=>{if(name==='react')return {...reservationReact,useState:state,useRef:ref,useEffect:effect,useLayoutEffect:effect,useCallback:callback};if(name==='react/jsx-runtime')return require(name);if(name.endsWith('/foundation'))return primitives;if(name.endsWith('/dialog'))return {Dialog:(props:Record<string,unknown>)=>props.open?reservationReact.createElement('aside',props,props.children as ReactNode):null};if(name.endsWith('/front-language'))return {FrontLocalizedTree:(props:{children:ReactNode})=>reservationReact.createElement(reservationReact.Fragment,null,props.children),useFrontLanguage:()=> 'en'};if(name.endsWith('/context-switch'))return {registerUnsavedWork:()=>()=>{}};if(name.endsWith('/pms-model'))return {money:reservationMoney,plusDays:reservationPlusDays};if(name.endsWith('/public-booking-reservation-command'))return {createPublicReservationClient,publicBookingCheckoutUrl};if(name.endsWith('/booking-gallery'))return {BookingGallery:()=>null};if(name.endsWith('/booking-quote-details-standard'))return {BookingQuoteDetailsStandard:()=>null};if(name.endsWith('/booking-payment-return'))return {BookingPaymentReturn:()=>null};if(name.endsWith('/booking-policy-summary'))return {BookingPolicySummary:()=>null};if(name.endsWith('/choice-select'))return {PmsChoiceSelect:()=>null};if(name.endsWith('/work-fetch'))return {workFetch:network};if(name.endsWith('.module.css'))return {default:new Proxy({}, {get:(_target,key)=>String(key)})};throw Error('Unexpected public reservation import '+name);};new Function('require','module','exports',emitted)(localRequire,evaluatedModule,evaluatedModule.exports);
 type Node=reservationReact.ReactElement<Record<string,unknown>>;const collect=(value:ReactNode,result:Node[]=[]):Node[]=>{reservationReact.Children.forEach(value,child=>{if(!reservationReact.isValidElement<Record<string,unknown>>(child))return;result.push(child);if(typeof child.type==='function')collect((child.type as (props:Record<string,unknown>)=>ReactNode)(child.props),result);else collect(child.props.children as ReactNode,result);});return result;};const Owner=evaluatedModule.exports[baseline?'BookingFlow':'PublicBookingReservation'] as (props:Record<string,unknown>)=>ReactNode;let props:Record<string,unknown>={slug:'acme',hotels,propertyId:'hotel-north',today:'2026-11-01'};
 const version={value:'original'},fields=new Map<string,string>([['arrival','2026-11-01'],['departure','2026-11-02'],['adults','2'],['children',''],['quantity','1']]),form={isConnected:true,getAttribute:()=>version.value,setAttribute:(_name:string,value:string)=>{version.value=value;}} as unknown as HTMLFormElement;
 vi.stubGlobal('FormData',class {get(key:string){return fields.get(key)??null;}getAll(key:string){return fields.has(key)?[fields.get(key)!]:[];}});
 const render=()=>{cursor=0;const tree=Owner(props),nodes=collect(tree);while(effects.length)effects.shift()!();return nodes;},find=(label:string)=>{const node=render().find(item=>item.props.children===label||item.props['aria-label']===label);if(!node)throw Error('Actual reservation control missing: '+label);return node;},click=(label:string)=>Reflect.apply(find(label).props.onClick as (...args:unknown[])=>unknown,undefined,[]),submit=async(label?:string)=>{const nodes=render(),node=label?nodes.find(item=>item.type==='form'&&(item.props['aria-label']===label||collect(item.props.children as ReactNode).some(child=>child.props.children===label)))??(label==='Find rooms'?nodes.find(item=>item.type==='form'):undefined):nodes.find(item=>item.type==='form');if(!node)throw Error('Actual reservation form missing');await Reflect.apply(node.props.onSubmit as (...args:unknown[])=>unknown,undefined,[{preventDefault(){},currentTarget:form}]);await reservationFlush();},type=(field:string,value:string)=>{const nodes=render(),input=nodes.find(node=>field==='notes'?node.type==='textarea':node.type==='input'&&node.props.autoComplete===field);if(!input)throw Error('Actual guest field missing '+field);const capture=nodes.find(node=>typeof node.props.onChangeCapture==='function');if(capture)Reflect.apply(capture.props.onChangeCapture as (...args:unknown[])=>unknown,undefined,[{target:{closest:()=>form}}]);Reflect.apply(input.props.onChange as (...args:unknown[])=>unknown,undefined,[{target:{value}}]);};
 return {render,find,click,submit,type,form,version,location,props:(patch:Record<string,unknown>)=>{props={...props,...patch};render();},close(){for(const hook of hooks)hook.cleanup?.();}};
}
async function preparePublicReservation(component:ReturnType<typeof mountPublicReservation>){component.render();await component.submit();component.click('Add rooms');component.click('Continue to guest details');component.type('name','Original guest');component.type('email','guest@example.test');component.type('tel','000');component.type('notes','Original notes');await component.submit('Guest details');}
it('renders the actual public reservation standard fields from SSR hotels without a mount request and preserves native availability input',async()=>{
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>reservationReply([])),component=mountPublicReservation(network);try{const nodes=component.render();expect(network).not.toHaveBeenCalled();expect(nodes.some(node=>node.type==='input'&&node.props.name==='arrival')).toBe(true);expect(nodes.filter(node=>['button','input','select'].includes(String(node.type))).every(node=>node.props.appearance==='standard')).toBe(true);await component.submit('Find rooms');expect(network).toHaveBeenCalledExactlyOnceWith('/api/public/booking/acme/availability',expect.objectContaining({method:'POST',body:JSON.stringify({search:{arrival:'2026-11-01',departure:'2026-11-02',adults:2,childAges:[],quantity:1,blockId:''},propertyId:'hotel-north'})}));}finally{component.close();}
});
it('actual public reservation handler blocks a fresh quote after unknown commit, recovers exact body/key and keeps newer guest details',async()=>{
 let creates=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async url=>String(url).endsWith('/availability')?reservationReply([{hotel:hotels[0],offers:[reservationQuote()]}]):String(url).endsWith('/quote')?reservationReply({quote:reservationQuote(),token:'original-native-quote'}):++creates===1?Promise.reject(Error('Original native response lost')):reservationReply(reservationReceipt())),component=mountPublicReservation(network);
 try{await preparePublicReservation(component);await component.submit('Review your stay');expect(creates).toBe(1);expect(component.find('Recover original reservation')).toBeDefined();component.click('Back / refresh quote');component.type('name','Newer guest');component.type('notes','Newer notes');const before=network.mock.calls.length;await component.submit('Guest details');expect(network.mock.calls).toHaveLength(before);expect(component.find('Review price & policy').props.disabled).toBe(true);component.click('Recover original reservation');await reservationFlush();
 const posts=network.mock.calls.filter(call=>String(call[0]).endsWith('/reservations'));expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(JSON.parse(String(posts[1][1]?.body))).toEqual(reservationInput());expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));expect(component.location.assign).not.toHaveBeenCalled();expect(component.find('Your original reservation is confirmed. Your newer details are retained.')).toBeDefined();
 component.click('Book another stay');await component.submit('Find rooms');component.click('Add rooms');component.click('Continue to guest details');const nodes=component.render();expect(nodes.find(node=>node.type==='input'&&node.props.autoComplete==='name')?.props.value).toBe('Newer guest');expect(nodes.find(node=>node.type==='textarea')?.props.value).toBe('Newer notes');expect(creates).toBe(2);
 }finally{component.close();}
});
it.each(['unmount','pathname'] as const)('actual mounted public reservation waits for original native JSON before Checkout and never redirects after owner %s',async changed=>{
 const started=reservationDeferred(),held=reservationDeferred();class CheckoutJSON extends Response{override async json(){started.resolve();await held.promise;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async url=>String(url).endsWith('/availability')?reservationReply([{hotel:hotels[0],offers:[reservationQuote(true)]}]):String(url).endsWith('/quote')?reservationReply({quote:reservationQuote(true),token:'original-native-quote'}):new CheckoutJSON(JSON.stringify({data:reservationReceipt(true)}))),component=mountPublicReservation(network);
 try{await preparePublicReservation(component);const submit=component.submit('Review your stay');await started.promise;expect(component.location.assign).not.toHaveBeenCalled();if(changed==='unmount')component.close();else{component.location.pathname='/book/another-holding/another-hotel';component.location.href=component.location.origin+component.location.pathname;}held.resolve();await submit;await reservationFlush();expect(component.location.assign).not.toHaveBeenCalled();}finally{held.resolve();component.close();}
});
it('actual public reservation routes only a validated native Checkout confirmation once',async()=>{
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async url=>String(url).endsWith('/availability')?reservationReply([{hotel:hotels[0],offers:[reservationQuote(true)]}]):String(url).endsWith('/quote')?reservationReply({quote:reservationQuote(true),token:'original-native-quote'}):reservationReply(reservationReceipt(true))),component=mountPublicReservation(network);
 try{await preparePublicReservation(component);await component.submit('Review your stay');expect(component.location.assign).toHaveBeenCalledExactlyOnceWith('https://checkout.stripe.com/c/pay/original');expect(network.mock.calls.filter(call=>String(call[0]).endsWith('/reservations'))).toHaveLength(1);expect(component.find('Manage booking').props.href).toBe('/book/acme/manage#token=original-management-token');}finally{component.close();}
});

// This suite exercises the explicit development product hosts. Declare its
// routing fixture instead of inheriting the workspace's ignored .env files.
beforeEach(() => {
  vi.stubEnv('ERPX_ENVIRONMENT', 'local');
  vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT', 'development');
  vi.stubEnv('ERPX_PRODUCT_ROUTING', 'enabled');
  vi.stubEnv('ERPX_PRODUCT_SSO', 'enabled');
});
afterEach(() => vi.unstubAllEnvs());

it('server-renders all public reservation step labels in the selected Booking language without a guest fetch', async () => {
  const {FrontLanguageContext} = await import('../../../src/components/front/front-language');
  const {PublicBookingReservation} = await import('../../../src/components/pms/public-booking-reservation');
  const network = vi.fn(); vi.stubGlobal('fetch', network);
  const html = renderToStaticMarkup(createElement(FrontLanguageContext.Provider, {value: 'th'}, createElement(PublicBookingReservation, {slug: 'acme', hotels: hotels.map(hotel => ({...hotel, images: hotel.images ?? [], roomContent: hotel.roomContent.map(room => ({...room, images: room.images ?? []}))})), today: '2026-11-01'})));
  expect(html).toContain('aria-label="ขั้นตอนการจอง"');
  expect(html).toContain('1 · ค้นหาห้องพัก'); expect(html).toContain('2 · ข้อมูลผู้เข้าพัก'); expect(html).toContain('3 · ตรวจสอบและยืนยัน');
  expect(html).not.toContain('2 · Guest details'); expect(network).not.toHaveBeenCalled();
});
// Standard setting owners retain the native forms and callbacks. Only React
// hooks, media children and the browser FormData boundary are fixture ports.
import {newProperty as standardSettingsProperty} from '../../../src/lib/pms/pms-engine';
import {records as standardSettingsRecords} from '../../../src/server/modules/pms/public/rules/pms-model';
import {decimalToMinor as standardSettingsMinor,minorToDecimal as standardSettingsDecimal} from '../../../src/lib/finance/finance-money';
type StandardBookingProps=ComponentProps<typeof import('../../../src/components/pms/standard-booking-settings').StandardBookingSettings>;
function standardBookingSnapshot(){
 const snapshot=standardSettingsProperty('hotel-north','company','branch','Original north hotel','2026-10-16');snapshot.property.version=3;snapshot.property.state='open';
 snapshot.records.push({id:'type-original',kind:'room_type',version:2,updatedAt:1,data:{name:'Original room',capacity:3,amenities:'Original amenities'}});
 return snapshot;
}
async function mountStandardBookingSettings(props:StandardBookingProps){
 const {createRequire}=await import('node:module'),{readFile}=await import('node:fs/promises'),ts=await import('typescript'),require=createRequire(import.meta.url),source=await readFile(new URL('../../../src/components/pms/standard-booking-settings.tsx',import.meta.url),'utf8'),emitted=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
 const react=require('react') as typeof import('react'),slots:{value?:unknown;current?:unknown}[]=[];let cursor=0;
 const state=(initial:unknown)=>{const at=cursor++;if(!slots[at])slots[at]={value:typeof initial==='function'?initial():initial};return [slots[at].value,(value:unknown)=>{slots[at].value=typeof value==='function'?value(slots[at].value):value;}];},ref=(initial:unknown)=>{const at=cursor++;return slots[at]??(slots[at]={current:initial});};
 class SettingsFormData{constructor(private readonly values:Map<string,string[]>){ }get(name:string){return this.values.get(name)?.[0]??null;}getAll(name:string){return this.values.get(name)??[];}}
 const ports=(name:string):unknown=>{if(name==='react')return {...react,useState:state,useRef:ref};if(name==='react/jsx-runtime')return require(name);if(name.endsWith('/workspace-link'))return {default:'WorkspaceLink'};if(name.endsWith('/destination-registry'))return {canonicalUiHref:(value:string)=>value};if(name.endsWith('/front-language'))return {FrontLocalizedTree:'FrontLocalizedTree'};if(name.endsWith('/form-disclosure'))return {FormDisclosure:'FormDisclosure'};if(name.endsWith('/booking-policy-settings'))return {BookingRatePolicy:'BookingRatePolicy',BookingMediaSetup:'BookingMediaSetup',BookingMediaPicker:'BookingMediaPicker'};if(name.endsWith('/foundation'))return {Button:'Button',Field:'Field',TextArea:'TextArea',TextInput:'TextInput'};if(name.endsWith('/booking-gallery'))return {BookingGallery:'BookingGallery'};if(name.endsWith('/pms-model'))return {records:standardSettingsRecords};if(name.endsWith('/finance-money'))return {decimalToMinor:standardSettingsMinor,minorToDecimal:standardSettingsDecimal};if(name.endsWith('.module.css'))return {default:new Proxy({}, {get:(_target,key)=>String(key)})};throw Error('Unexpected standard Booking settings import '+name);};
 const evaluated={exports:{} as Record<string,unknown>};new Function('require','module','exports','FormData',emitted)(ports,evaluated,evaluated.exports,SettingsFormData);
 const Component=evaluated.exports.StandardBookingSettings as (value:StandardBookingProps)=>ReactNode;
 type Node=import('react').ReactElement<Record<string,unknown>>;
 const collect=(value:ReactNode,out:Node[]=[]):Node[]=>{react.Children.forEach(value,child=>{if(!react.isValidElement<Record<string,unknown>>(child))return;out.push(child);collect(child.props.children as ReactNode,out);});return out;};
 return {render:()=>{cursor=0;return collect(Component(props));},props};
}
it('actual standard Booking settings keeps native field constraints, source media and right-drawer controls without a mount request',async()=>{
 const network=vi.fn();vi.stubGlobal('fetch',network);const snapshot=standardBookingSnapshot(),save=vi.fn(async()=>true),mounted=await mountStandardBookingSettings({snapshot,save,slug:'acme',canReview:true});
 try{const nodes=mounted.render(),drawer=nodes.find(node=>node.type==='FormDisclosure');expect(drawer?.props.appearance).toBe('standard');expect(drawer?.props.dismissDisabled).toBe(false);expect(nodes.filter(node=>['Field','TextInput','TextArea','Button'].includes(String(node.type))).every(node=>node.props.appearance==='standard')).toBe(true);expect(nodes.find(node=>node.type==='TextArea'&&node.props.name==='description')?.props).toMatchObject({required:true,maxLength:3000});expect(nodes.find(node=>node.type==='TextInput'&&node.props.name==='days')?.props).toMatchObject({type:'number',min:0,max:365,defaultValue:1});expect(nodes.find(node=>node.type==='TextInput'&&node.props.name==='time')?.props.defaultValue).toBe('18:00');expect(nodes.filter(node=>node.type==='BookingMediaSetup').map(node=>node.props)).toEqual(expect.arrayContaining([expect.objectContaining({base:'/holdings/acme',targetType:'pms_property',targetId:'hotel-north',version:3,canReview:true}),expect.objectContaining({targetType:'pms_room_type',targetId:'type-original',version:2})]));expect(save).not.toHaveBeenCalled();expect(network).not.toHaveBeenCalled();}finally{vi.unstubAllGlobals();}
});
it('actual standard Booking publication requires its current preview and retains the captured editing version and newer local fields while saving',async()=>{
 let release!:(value:boolean)=>void;const save=vi.fn<(op:string,input:unknown,version?:number)=>Promise<boolean>>(()=>new Promise(resolve=>{release=resolve;})),snapshot=standardBookingSnapshot(),mounted=await mountStandardBookingSettings({snapshot,save,slug:'acme'}),values=new Map<string,string[]>([['description',['Original description']],['address',['Original address']],['email',['guest@example.test']],['phone',['000']],['days',['2']],['time',['17:30']],['published',['on']]]);
 let nodes=mounted.render(),form=nodes.find(node=>node.type==='form')!;Reflect.apply(form.props.onChange as ()=>void,undefined,[]);snapshot.property.version=4;
 const submit=()=>Reflect.apply(form.props.onSubmit as (event:unknown)=>Promise<void>,undefined,[{preventDefault(){},currentTarget:values}]);await submit();expect(save).not.toHaveBeenCalled();nodes=mounted.render();expect(nodes.find(node=>node.props.role==='status')?.props.children).toBe('Preview the latest content and readiness before publishing');form=nodes.find(node=>node.type==='form')!;(form.props.ref as {current:unknown}).current=values;
 const preview=nodes.find(node=>node.type==='Button'&&node.props.children==='Preview and check readiness')!;Reflect.apply(preview.props.onClick as ()=>void,undefined,[]);nodes=mounted.render();form=nodes.find(node=>node.type==='form')!;const pending=submit();expect(save).toHaveBeenCalledExactlyOnceWith('booking.save',{published:true,description:'Original description',address:'Original address',contactEmail:'guest@example.test',contactPhone:'000',cancellationDays:2,cancellationTime:'17:30',rateIds:[],images:[],roomContent:[]},3);expect(mounted.render().find(node=>node.type==='FormDisclosure')?.props.dismissDisabled).toBe(true);values.set('description',['Newer unsaved description']);release(false);await pending;expect(values.get('description')).toEqual(['Newer unsaved description']);expect(mounted.render().find(node=>node.props.role==='status')?.props.children).toBe('Not saved. Check the message above');expect(save).toHaveBeenCalledOnce();
});
it.each([true,false])('actual standard Import presents original independent Finance review only when native canApprove=%s',canApprove=>{
 const network=vi.fn();vi.stubGlobal('fetch',network);try{const html=renderToStaticMarkup(createElement(PmsImport,{api:importApi,base:'/holdings/acme',propertyId:'hotel-north',propertyVersion:3,businessDate:'2026-10-16',initial:[importBatch],userId:'finance-reviewer',canPrepare:false,canApprove}));expect(html).toContain('data-slot="card"');expect(html).toContain('data-appearance="standard"');expect(html.includes('Finance: approve this preview')).toBe(canApprove);expect(html).not.toContain('Preview only');expect(html).not.toMatch(/<form[\s>]/);expect(localOwners.buttons.every(button=>button.appearance==='standard')).toBe(true);expect(network).not.toHaveBeenCalled();}finally{vi.unstubAllGlobals();}
});
import {createPmsImportDraft} from '../../../src/lib/pms/import-draft';
import {installPmsPendingNavigation} from '../../../src/lib/pms/pending-navigation';
import {hasUnsavedWork,confirmContextSwitch} from '../../../src/lib/work_projection/context-switch';

function importLifetimeBrowser(withNavigation=true){
 const {node,events}=importBrowser(),url=new URL('https://pms-dev-erpx.jesadakorn.com/holdings/acme/pms/import?property=hotel-north'),confirm=vi.fn(()=>false),navigation=new EventTarget(),doc=Object.assign(new EventTarget(),{querySelector:(selector:string)=>selector==='[data-work-revision]'&&node.isConnected?node:null,activeElement:{closest:()=>null},documentElement:{dataset:{}}}),history={pushState:vi.fn(),replaceState:vi.fn(),go:vi.fn()};
 Object.assign(window,{location:url,confirm,history,...(withNavigation?{navigation}:{})});vi.stubGlobal('document',doc);
 class ImportAnchor{constructor(readonly href:string,readonly attributes:Partial<Record<string,string>>={}){}get target(){return this.attributes.target??'';}hasAttribute(name:string){return Object.hasOwn(this.attributes,name);}closest(){return this;}}
 vi.stubGlobal('Element',ImportAnchor);
 const click=(href:string,attributes:Partial<Record<string,string>>={},modifiers:Record<string,unknown>={})=>{const event=new Event('click',{cancelable:true});Object.defineProperties(event,Object.fromEntries(Object.entries({target:new ImportAnchor(new URL(href,url).href,attributes),button:0,...modifiers}).map(([key,value])=>[key,{value}])));doc.dispatchEvent(event);return event;};
 const traverse=(href:string,cancelable=true,sameDocument=true)=>{const event=Object.assign(new Event('navigate',{cancelable}),{navigationType:'traverse',destination:{url:new URL(href,url).href,sameDocument}});navigation.dispatchEvent(event);return event;};
 return {node,events,url,confirm,doc,history,click,traverse};
}
async function mountImportLifetime(props:ComponentProps<typeof PmsImport>){
 const {createRequire}=await import('node:module'),{readFile}=await import('node:fs/promises'),ts=await import('typescript'),require=createRequire(import.meta.url),react=require('react') as typeof import('react'),source=await readFile(new URL('../../../src/components/pms/standard-pms-import.tsx',import.meta.url),'utf8'),emitted=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
 type Slot={value?:unknown;current?:unknown;deps?:readonly unknown[];cleanup?:()=>void};const slots:Slot[]=[],effects:(()=>void)[]=[];let cursor=0;
 const state=(initial:unknown)=>{const at=cursor++;if(!slots[at])slots[at]={value:typeof initial==='function'?initial():initial};return [slots[at].value,(value:unknown)=>{slots[at].value=typeof value==='function'?value(slots[at].value):value;}];},ref=(initial:unknown)=>{const at=cursor++;return slots[at]??(slots[at]={current:initial});},callback=(value:unknown,deps:readonly unknown[])=>{const at=cursor++,old=slots[at];if(!old||deps.some((value,index)=>!Object.is(value,old.deps?.[index])))slots[at]={value,deps};return slots[at].value;},effect=(run:()=>void|(()=>void),deps:readonly unknown[])=>{const at=cursor++,old=slots[at];if(!old||deps.some((value,index)=>!Object.is(value,old.deps?.[index])))effects.push(()=>{old?.cleanup?.();const cleanup=run();slots[at]={deps,cleanup:typeof cleanup==='function'?cleanup:undefined};});};
 const router={refresh:vi.fn()},ports=(name:string):unknown=>{if(name==='react')return {...react,useState:state,useRef:ref,useLayoutEffect:effect,useCallback:callback};if(name==='react/jsx-runtime')return require(name);if(name==='next/navigation')return {usePathname:()=>window.location.pathname};if(name.endsWith('/destination-search'))return {useSearchParams:()=>new URL(window.location.href).searchParams};if(name.endsWith('/destination-registry'))return {canonicalUiHref:(value:string)=>value};if(name.endsWith('/front-language'))return {FrontLocalizedTree:'FrontLocalizedTree',useFrontLanguage:()=> 'en'};if(name.endsWith('/foundation'))return {Button:'Button',Card:'Card',Field:'Field',InlineNotice:'InlineNotice',TextArea:'TextArea',TextInput:'TextInput'};if(name.endsWith('/workspace-link'))return {default:'WorkspaceLink'};if(name.endsWith('/destination-router'))return {useRouter:()=>router};if(name.endsWith('/import-command'))return {createPmsImportCommands};if(name.endsWith('/import-draft'))return {createPmsImportDraft};if(name.endsWith('/pending-navigation'))return {installPmsPendingNavigation};if(name.endsWith('.module.css'))return {default:new Proxy({}, {get:(_target,key)=>String(key)})};throw Error('Unexpected actual Import lifetime import '+name);};
 const evaluated={exports:{} as Record<string,unknown>};new Function('require','module','exports',emitted)(ports,evaluated,evaluated.exports);const Component=evaluated.exports.StandardPmsImport as (props:ComponentProps<typeof PmsImport>)=>ReactNode;
 type Node=import('react').ReactElement<Record<string,unknown>>;const collect=(value:ReactNode,out:Node[]=[]):Node[]=>{react.Children.forEach(value,child=>{if(!react.isValidElement<Record<string,unknown>>(child))return;out.push(child);collect(child.props.children as ReactNode,out);});return out;};
 const render=()=>{cursor=0;const nodes=collect(Component(props));while(effects.length)effects.shift()!();return nodes;};
 const edit=(label:'Import JSON'|'journal',value:string)=>{const node=render().find(node=>label==='Import JSON'?node.type==='TextArea'&&node.props['aria-label']===label:node.type==='TextInput');if(!node)throw Error('Actual Import draft field missing '+label);Reflect.apply(node.props.onChange as (event:unknown)=>void,undefined,[{target:{value}}]);};
 const click=(text:string)=>{const node=render().find(node=>node.type==='Button'&&node.props.children===text);if(!node)throw Error('Actual Import action missing '+text);Reflect.apply(node.props.onClick as ()=>void,undefined,[]);};
 return {props,render,edit,click,router,close(){for(const slot of slots)slot.cleanup?.();}};
}
function importLifetimeProps():ComponentProps<typeof PmsImport>{return {api:importApi,base:'/holdings/acme',propertyId:'hotel-north',propertyVersion:3,businessDate:'2026-10-16',initial:[importBatch],userId:'finance-reviewer',companyId:'company',canPrepare:true,canApprove:true};}
it('mounted standard Import registers actual JSON and journal edits, preserves native modified links and record overlays, and cleans its lifetime on unmount',async()=>{
 const browser=importLifetimeBrowser(),network=vi.fn();vi.stubGlobal('fetch',network);const mounted=await mountImportLifetime(importLifetimeProps());
 try{mounted.render();expect(hasUnsavedWork()).toBe(false);mounted.edit('Import JSON',JSON.stringify(importDefinition));mounted.edit('journal','Original independent journal');expect(hasUnsavedWork()).toBe(true);expect(confirmContextSwitch()).toBe(false);expect(browser.click('?property=another-hotel').defaultPrevented).toBe(true);expect(browser.click('?property=hotel-north&detailKind=pms_entry&detailId=entry-original').defaultPrevented).toBe(false);for(const modifiers of [{ctrlKey:true},{metaKey:true},{shiftKey:true},{altKey:true},{button:1}])expect(browser.click('?property=another-hotel',{},modifiers).defaultPrevented).toBe(false);for(const attrs of [{target:'_blank'},{download:''},{'data-product-switch':'true'}])expect(browser.click('?property=another-hotel',attrs).defaultPrevented).toBe(false);expect(browser.traverse('?property=another-hotel').defaultPrevented).toBe(true);const unload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unload);expect(unload.defaultPrevented).toBe(true);browser.confirm.mockReturnValue(true);expect(browser.click('?property=another-hotel').defaultPrevented).toBe(false);expect(mounted.render().find(node=>node.type==='TextArea')?.props.value).toBe(JSON.stringify(importDefinition));expect(network).not.toHaveBeenCalled();expect(browser.history.pushState).not.toHaveBeenCalled();expect(browser.history.replaceState).not.toHaveBeenCalled();expect(browser.history.go).not.toHaveBeenCalled();}finally{mounted.close();}expect(hasUnsavedWork()).toBe(false);
});
it('mounted standard Import zero-edit apply protects original JSON, retains first403 and recovers only with the same body/key after command permission returns',async()=>{
 const browser=importLifetimeBrowser();let release!:()=>void,started!:()=>void,posts=0;const held=new Promise<void>(resolve=>{release=resolve;}),reading=new Promise<void>(resolve=>{started=resolve;});class OriginalJson extends Response{override async json(){started();await held;return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>init?.method==='POST'?(++posts===1?new OriginalJson(JSON.stringify({data:importReceipt('apply')})):posts===2?Response.json({error:{code:'FORBIDDEN',message:'Original current permission refused.'}},{status:403}):Response.json({data:importReceipt('apply')})):Response.json({data:[{...importBatch,state:'applied',version:3}]}));vi.stubGlobal('fetch',network);const props={...importLifetimeProps(),initial:[{...importBatch,state:'approved' as const,version:2}]},mounted=await mountImportLifetime(props);
 try{mounted.render();mounted.click('Apply approved batch / นำเข้าชุดที่อนุมัติ');await reading;expect(hasUnsavedWork()).toBe(true);expect(browser.click('?property=another-hotel').defaultPrevented).toBe(true);browser.node.dataset.workRevision='8';release();await vi.waitFor(()=>expect(mounted.render().some(node=>node.props.role==='alert')).toBe(true));props.canPrepare=false;props.canApprove=false;expect(mounted.render().find(node=>node.type==='Button'&&node.props.children==='Retry submitted import action')?.props.disabled).toBe(false);browser.node.dataset.workRevision='7';mounted.click('Retry submitted import action');await vi.waitFor(()=>expect(network.mock.calls.filter(call=>call[1]?.method==='POST')).toHaveLength(2));await vi.waitFor(()=>expect(mounted.render().find(node=>node.props.role==='alert')?.props.children).toBe('Original current permission refused.'));expect(hasUnsavedWork()).toBe(true);props.canPrepare=true;mounted.click('Retry submitted import action');await vi.waitFor(()=>expect(mounted.render().some(node=>node.type==='Button'&&node.props.children==='Retry submitted import action')).toBe(false));const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(mutations).toHaveLength(3);expect(new Set(mutations.map(call=>call[1]?.body)).size).toBe(1);expect(new Set(mutations.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).size).toBe(1);expect(mounted.router.refresh).toHaveBeenCalledOnce();expect(browser.events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);expect(hasUnsavedWork()).toBe(false);}finally{release();mounted.close();}
});
it('mounted standard Import keeps newer JSON and journal fields dirty after original preview recovery, and quarantines a forced different native owner without replacing its request',async()=>{
 const browser=importLifetimeBrowser();let first=true;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>{if(init?.method==='POST'){if(first){first=false;throw Error('Native response lost after send.');}return Response.json({data:importReceipt('preview')});}return Response.json({data:[importBatch]});});vi.stubGlobal('fetch',network);const props=importLifetimeProps(),mounted=await mountImportLifetime(props);
 try{mounted.render();mounted.edit('Import JSON',JSON.stringify(importDefinition));mounted.click('Preview only');await vi.waitFor(()=>expect(mounted.render().find(node=>node.props.role==='alert')?.props.children).toBe('Native response lost after send.'));const newer=JSON.stringify({...importDefinition,source:'Newer unsent JSON'});mounted.edit('Import JSON',newer);mounted.edit('journal','Newer unsent journal');props.propertyId='another-hotel';browser.url.search='?property=another-hotel';let nodes=mounted.render();expect(nodes.some(node=>node.type==='TextArea')).toBe(false);expect(nodes.find(node=>node.type==='Button'&&node.props.children==='Retry submitted import action')?.props.disabled).toBe(true);expect(nodes.some(node=>node.type==='Button'&&node.props.children==='Use currently selected hotel')).toBe(false);expect(network).toHaveBeenCalledOnce();props.propertyId='hotel-north';browser.url.search='?property=hotel-north';mounted.render();mounted.click('Retry submitted import action');await vi.waitFor(()=>expect(mounted.router.refresh).toHaveBeenCalledOnce());nodes=mounted.render();expect(nodes.find(node=>node.type==='TextArea')?.props.value).toBe(newer);expect(nodes.find(node=>node.type==='TextInput')?.props.value).toBe('Newer unsent journal');expect(hasUnsavedWork()).toBe(true);expect(network.mock.calls.filter(call=>call[1]?.method==='POST').map(call=>call[1]?.body)).toEqual([JSON.stringify({action:'preview',input:importDefinition}),JSON.stringify({action:'preview',input:importDefinition})]);}finally{mounted.close();}
});
it('Import without Navigation API protects full-document departures but never invents cancellable popstate or durable reload recovery',async()=>{
 const browser=importLifetimeBrowser(false),mounted=await mountImportLifetime(importLifetimeProps());try{mounted.render();mounted.edit('journal','Unsaved original journal');const pop=new Event('popstate');window.dispatchEvent(pop);expect(pop.defaultPrevented).toBe(false);const unload=new Event('beforeunload',{cancelable:true});window.dispatchEvent(unload);expect(unload.defaultPrevented).toBe(true);expect(browser.history.go).not.toHaveBeenCalled();expect(browser.history.pushState).not.toHaveBeenCalled();expect(browser.history.replaceState).not.toHaveBeenCalled();}finally{mounted.close();}
});
it.each([
 ['foreign batch id',(data:ReturnType<typeof importReceipt>)=>({...data,id:'unrelated-native-batch'})],
 ['wrong original count',(data:ReturnType<typeof importReceipt>)=>({...data,report:{...importReport,counts:{...importReport.counts,rooms:1}}})],
 ['wrong opening total',(data:ReturnType<typeof importReceipt>)=>({...data,report:{...importReport,totals:{...importReport.totals,due:1}}})],
 ['wrong approval readiness',(data:ReturnType<typeof importReceipt>)=>({...data,report:{...importReport,canApprove:false}})],
] as const)('Import preview retains %s until the exact native Company/key report recovers',async(_label,change)=>{
 const {events}=importBrowser();let posts=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?(++posts===1?change(importReceipt('preview')):importReceipt('preview')):[importBatch]}));vi.stubGlobal('fetch',network);const commands=createPmsImportCommands(()=>({api:importApi,propertyId:'hotel-north',propertyVersion:3}));await expect(commands.run('preview',importDefinition)).rejects.toThrow('acknowledgement is incomplete');expect(commands.hasPending()).toBe(true);expect(events).toEqual([]);expect(network).toHaveBeenCalledOnce();const result=await commands.retry();expect(result?.acknowledge()).toBe(true);const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(mutations[1][1]?.body).toBe(mutations[0][1]?.body);expect(new Headers(mutations[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));
});
it('Import refuses a different selected hotel before POST and rechecks Work after asynchronous native preview ID hashing',async()=>{
 const {node}=importBrowser(),subtle=crypto.subtle;let release!:()=>void,started!:()=>void;const held=new Promise<void>(resolve=>{release=resolve;}),hashing=new Promise<void>(resolve=>{started=resolve;});vi.stubGlobal('crypto',{...crypto,subtle:{digest:async(...args:Parameters<SubtleCrypto['digest']>)=>{started();await held;return subtle.digest(...args);}}});const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>Response.json({data:importReceipt('preview')}));vi.stubGlobal('fetch',network);const commands=createPmsImportCommands(()=>({api:importApi,propertyId:'hotel-north',propertyVersion:3}));expect(()=>commands.run('preview',{...importDefinition,propertyId:'another-hotel'})).toThrow('selected hotel');expect(network).not.toHaveBeenCalled();const pending=commands.run('preview',importDefinition);await hashing;node.dataset.workRevision='8';release();await expect(pending).rejects.toThrow('Return to the original work context');expect(commands.hasPending()).toBe(true);expect(network).toHaveBeenCalledOnce();
});
it.each([
 ['preview',409,'PMS_IMPORT_JOURNAL'],['approve',409,'UNRECOGNIZED_REJECTION'],['preview',422,'PMS_IMPORT_STALE'],['apply',403,'WORK_ROLE_UNAVAILABLE'],
] as const)('Import retains first %s status %s/%s outside exact native rollback proof',async(action,status,code)=>{
 importBrowser();vi.stubGlobal('fetch',async()=>Response.json({error:{code,message:'Original refusal.'}},{status}));const commands=createPmsImportCommands(()=>({api:importApi,propertyId:'hotel-north',propertyVersion:3}));await expect(commands.run(action,importInput(action))).rejects.toThrow('Original refusal.');expect(commands.hasPending()).toBe(true);
});
it.each([false,true])('Import exact opening-journal rollback allows a corrected fresh key only when priorUnknown=%s is false',async priorUnknown=>{
 importBrowser();let posts=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>{if(init?.method!=='POST')return Response.json({data:[importBatch]});if(++posts===1&&priorUnknown)throw Error('Unknown original response.');return Response.json({error:{code:'PMS_IMPORT_JOURNAL',message:'Review the independently posted opening journal.'}},{status:409});});vi.stubGlobal('fetch',network);const commands=createPmsImportCommands(()=>({api:importApi,propertyId:'hotel-north',propertyVersion:3}));await expect(commands.run('approve',importInput('approve'))).rejects.toThrow();if(priorUnknown)await expect(commands.retry()).rejects.toThrow();expect(commands.hasPending()).toBe(priorUnknown);if(priorUnknown)expect(()=>commands.run('approve',{...importInput('approve'),openingJournalId:'corrected-journal'})).toThrow('Retry the submitted import action first');else{await expect(commands.run('approve',{...importInput('approve'),openingJournalId:'corrected-journal'})).rejects.toThrow();const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(new Headers(mutations[1][1]?.headers).get('Idempotency-Key')).not.toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));}
});
it('Import retains its first owner completion and leaves newer inline JSON or an unrelated journal dirty after exact native recovery',async()=>{
 importBrowser();const draft=createPmsImportDraft();draft.editBody(JSON.stringify(importDefinition));let first=true;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>{if(init?.method==='POST'){if(first){first=false;throw Error('Original response lost.');}return Response.json({data:importReceipt('preview')});}return Response.json({data:[importBatch]});});vi.stubGlobal('fetch',network);const commands=createPmsImportCommands(()=>({api:importApi,propertyId:'hotel-north',propertyVersion:3}));await expect(commands.run('preview',importDefinition,draft.capture('preview'))).rejects.toThrow();draft.editBody('Newer local JSON');const recovered=await commands.run('preview',importDefinition,draft.capture('preview'));expect(recovered.acknowledge()).toBe(true);expect(recovered.acknowledge()).toBe(false);expect(draft.isDirty()).toBe(true);draft.editJournal('Unrelated independent opening journal');const next={...importDefinition,source:'Current reviewed JSON'};draft.editBody(JSON.stringify(next));const saved=await commands.run('preview',next,draft.capture('preview'));expect(saved.acknowledge()).toBe(true);expect(draft.isDirty()).toBe(true);const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(mutations[1][1]?.body).toBe(mutations[0][1]?.body);expect(new Headers(mutations[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));expect(new Headers(mutations[2][1]?.headers).get('Idempotency-Key')).not.toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));draft.discard();expect(draft.isDirty()).toBe(false);
});
it('Import apply validates the original property version while accepting an exact historical receipt after later legitimate owner versions',async()=>{
 importBrowser();const source={api:importApi,propertyId:'hotel-north',propertyVersion:3};let posts=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>Response.json({data:init?.method==='POST'?{...importReceipt('apply'),propertyVersion:++posts===1?6:4}:[{...importBatch,state:'applied',version:3}]}));vi.stubGlobal('fetch',network);const commands=createPmsImportCommands(()=>source),input=importInput('apply');await expect(commands.run('apply',input)).rejects.toThrow('acknowledgement is incomplete');expect(commands.hasPending()).toBe(true);source.propertyVersion=5;const result=await commands.retry();expect(result?.acknowledge()).toBe(true);const mutations=network.mock.calls.filter(call=>call[1]?.method==='POST');expect(mutations[1][1]?.body).toBe(mutations[0][1]?.body);expect(new Headers(mutations[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(mutations[0][1]?.headers).get('Idempotency-Key'));
});
it('Import does not release even a first proven rollback code when the owner query changes during ORIGINAL error JSON',async()=>{
 importBrowser();let release!:()=>void,started!:()=>void;const held=new Promise<void>(resolve=>{release=resolve;}),reading=new Promise<void>(resolve=>{started=resolve;});class OriginalError extends Response{override async json(){started();await held;return super.json();}}let posts=0;const source={api:importApi,propertyId:'hotel-north',href:'https://pms-dev-erpx.jesadakorn.com/holdings/acme/pms/import?property=hotel-north'};const network=vi.fn(async()=>++posts===1?new OriginalError(JSON.stringify({error:{code:'PMS_IMPORT_STALE',message:'Preview a fresh batch.'}}),{status:409}):Response.json({error:{code:'PMS_IMPORT_STALE',message:'Preview a fresh batch.'}},{status:409}));vi.stubGlobal('fetch',network);const commands=createPmsImportCommands(()=>source),pending=commands.run('preview',importDefinition);await reading;source.href+='&page=2';release();await expect(pending).rejects.toThrow('Return to the original work context');expect(commands.hasPending()).toBe(true);source.href=source.href.replace('&page=2','');await expect(commands.retry()).rejects.toThrow('Preview a fresh batch.');expect(commands.hasPending()).toBe(true);
});

// Actual Front entry pages keep native readers and destinations. Presentation
// opts into the shared standard controls; no auth or preference command is run.
import FrontLogin from '../../../src/app/front-access/login/page';
import {LoginForm} from '../../../src/app/login/login-form';
import {StandardFrontSignIn,StandardFrontCompanyPicker} from '../../../src/components/front/standard-front-access';
import {QuickAccessShelf} from '../../../src/components/work_projection/quick-access/shelf';
import type {ReactElement} from 'react';
function standardFrontChild<Props>(node:ReactNode,type:unknown):ReactElement<Props>|undefined{
 if(Array.isArray(node)){for(const child of node){const found=standardFrontChild<Props>(child,type);if(found)return found;}return;}
 if(!isValidElement<Props&{children?:ReactNode}>(node))return;
 if(node.type===type)return node;
 return standardFrontChild<Props>(node.props.children,type);
}
it('actual Front sign-in uses standard native controls while retaining the exact safe return query and expired-session notice',async()=>{
 const next='/acme/north/holding/rooms?tag=one&tag=two',node=await FrontLogin({searchParams:Promise.resolve({next,reason:'expired'})});
 if(!isValidElement<ComponentProps<typeof StandardFrontSignIn>>(node))throw Error('Expected actual Front sign-in presentation.');
 expect(node.type).toBe(StandardFrontSignIn);expect(node.props.next).toBe(next);expect(node.props.isPos).toBe(false);
 const form=standardFrontChild<ComponentProps<typeof LoginForm>>(StandardFrontSignIn(node.props),LoginForm);
 expect(form?.props).toEqual({appearance:'standard',next});
 const html=renderToStaticMarkup(node);expect(html).toContain('data-slot="form-layout"');expect(html).toContain('data-appearance="standard"');expect(html).toContain('name="email"');expect(html).toContain('autoComplete="username"');expect(html).toContain('name="password"');expect(html).toContain('autoComplete="current-password"');expect(html).toContain('method="post"');expect(html).toContain('Your session has expired. Please sign in again.');expect(html).toContain('href="/forgot-password"');expect(html).toContain('the next guest');
 expect(localOwners.session.mock.calls).toEqual([[]]);expect(localOwners.workspaces.mock.calls).toEqual([[]]);expect(localOwners.choices).not.toHaveBeenCalled();expect(localOwners.preferences).not.toHaveBeenCalled();
});
it.each(['https://other.example/account','//other.example/account','/holdings/acme/finance','/acme\\north'])('actual Front sign-in retains native refusal of unsafe return %s',async next=>{
 const node=await FrontLogin({searchParams:Promise.resolve({next})});
 if(!isValidElement<ComponentProps<typeof StandardFrontSignIn>>(node))throw Error('Expected actual Front sign-in presentation.');
 expect(node.props.next).toBe('/');expect(node.props.erpHref).toBeNull();
 const html=renderToStaticMarkup(node);expect(html).not.toContain('Your session has expired');expect(html).not.toContain('Back to ERP');expect(html).toContain('Staff sign in');
});
it('actual Front sign-in keeps its optional-session failure fallback without reading Company choices or preferences',async()=>{
 localOwners.session.mockRejectedValueOnce(Error('No optional current session'));
 const node=await FrontLogin({searchParams:Promise.resolve({next:'/acme/north/holding/sale?receipt=original'})});
 if(!isValidElement<ComponentProps<typeof StandardFrontSignIn>>(node))throw Error('Expected actual Front sign-in presentation.');
 expect(node.props.next).toBe('/acme/north/holding/sale?receipt=original');expect(node.props.erpHref).toBeNull();expect(localOwners.workspaces).not.toHaveBeenCalled();expect(localOwners.choices).not.toHaveBeenCalled();expect(localOwners.preferences).not.toHaveBeenCalled();
});
it('actual standard Front Company picker retains the original preference object, plain endpoint, default IDs and local Next links',async()=>{
 const preference={pinnedIds:[companies[1].id],version:7};localOwners.preferences.mockResolvedValueOnce(preference);
 const node=await FrontCompanyPicker({searchParams:Promise.resolve({choose:'1'})});
 if(!isValidElement<ComponentProps<typeof StandardFrontCompanyPicker>>(node))throw Error('Expected actual Front Company presentation.');
 expect(node.type).toBe(StandardFrontCompanyPicker);expect(node.props.companies).toBe(companies);expect(node.props.preference).toBe(preference);
 const shelf=standardFrontChild<ComponentProps<typeof QuickAccessShelf>>(StandardFrontCompanyPicker(node.props),QuickAccessShelf);
 expect(shelf?.props).toEqual({appearance:'standard',items:node.props.items,initialPreferences:preference,defaultIds:companies.map(company=>company.id),endpoint:'/api/v1/front-access/quick-access',title:'Quick access',requestMode:'plain',locale:'en',placement:'rail',embedded:true});
 const html=renderToStaticMarkup(localProductSurface('pms',node));expect(html).toContain('data-slot="card"');expect(html).toContain('data-ui-appearance="standard"');expect(html).toContain('href="#front-company-list"');expect(html).toContain('id="front-company-list"');expect(html).toContain('tabindex="-1"');expect(html).toContain('aria-label="Available companies"');expect(html).toContain('href="/acme/north"');expect(html).toContain('href="/other/south"');
 expect(localOwners.session).toHaveBeenCalledTimes(1);expect(localOwners.choices.mock.calls).toEqual([['pms']]);expect(localOwners.preferences.mock.calls).toEqual([['original-account','front-picker','pms']]);expect(localOwners.workspaces.mock.calls).toEqual([[]]);observeLocalLinks('Standard Front Company picker',['/acme/north','/other/south']);
});
it('actual standard Front Company picker keeps the empty owner result without inventing a Company or role',async()=>{
 localOwners.choices.mockResolvedValueOnce([]);const node=await FrontCompanyPicker({searchParams:Promise.resolve({choose:'1'})}),html=renderToStaticMarkup(localProductSurface('pms',node));
 expect(html).toContain('No available locations for this account. Contact your company administrator.');expect(html).toContain('aria-label="Available companies"');expect(localOwners.remembered).not.toHaveBeenCalled();expect(localOwners.links).toEqual([]);expect(localOwners.preferences).toHaveBeenCalledTimes(1);
});
it('actual standard Front Company entry still redirects its sole native choice before loading preferences',async()=>{
 localOwners.choices.mockResolvedValueOnce([companies[0]]);await expect(FrontCompanyPicker({searchParams:Promise.resolve({choose:'1'})})).rejects.toThrow('NEXT_REDIRECT');expect(localOwners.preferences).not.toHaveBeenCalled();expect(localOwners.workspaces).not.toHaveBeenCalled();expect(localOwners.links).toEqual([]);
});
it('standard POS sign-in retains the original owner identity and exact optional ERP handoff instead of constructing an SSO destination',()=>{
 const href='https://account.example/sso/apps?state=original&next=%2Fapps',node=StandardFrontSignIn({isPos:true,next:'/acme/north/holding/sale?receipt=original',erpHref:href}),html=renderToStaticMarkup(node);
 expect(html).toContain('the next sale');expect(html).toContain('POINT OF SALE');expect(html).toContain('href="https://account.example/sso/apps?state=original&amp;next=%2Fapps"');expect(html).toContain('Back to ERP');const form=standardFrontChild<ComponentProps<typeof LoginForm>>(node,LoginForm);expect(form?.props.next).toBe('/acme/north/holding/sale?receipt=original');expect(form?.props.frontMode).toBeUndefined();expect(localOwners.session).not.toHaveBeenCalled();expect(localOwners.workspaces).not.toHaveBeenCalled();
});
