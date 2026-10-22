import {afterEach,expect,it,vi} from 'vitest';
import {build} from 'esbuild';
import {chromium,expect as browserExpect} from '@playwright/test';
import {existsSync,readFileSync} from 'node:fs';
import {createServer} from 'node:http';
import {recordTarget,recordDrawerHref,recordCanonicalHref,withoutRecordDrawer} from '@erpx/feature-records/lib/record-drawer';
import {canonicalUiHref,destinations,destinationTrail} from '@erpx/feature-work_projection/lib/destination-registry';
import {canonicalDestinationLinks} from '@erpx/feature-work_projection/lib/destination-links';
import {destinationProduct} from '@erpx/contracts/product-registry';
import {productPageAllowed} from '@erpx/feature-runtime/lib/product-route-policy';
import {productEntryCapability} from '@erpx/server-commercial/public/rules/product-entry-capability';
const recordDrawerHarness=vi.hoisted(()=>({active:false,states:[] as unknown[],stateCursor:0,refs:[] as {current:unknown}[],refCursor:0,effects:[] as (()=>void|(()=>void))[],contentNode:undefined as unknown}));
vi.mock('react',async importOriginal=>{
 const actual=await importOriginal<typeof import('react')>();
 return {...actual,
  useState:(initial:unknown)=>{
   if(!recordDrawerHarness.active)return actual.useState(initial);
   const index=recordDrawerHarness.stateCursor++;
   if(!(index in recordDrawerHarness.states))recordDrawerHarness.states[index]=index===0&&recordDrawerHarness.contentNode?recordDrawerHarness.contentNode:initial;
   return [recordDrawerHarness.states[index],(next:unknown)=>{recordDrawerHarness.states[index]=typeof next==='function'?(next as (previous:unknown)=>unknown)(recordDrawerHarness.states[index]):next;}];
  },
  useRef:(initial:unknown)=>{
   if(!recordDrawerHarness.active)return actual.useRef(initial);
   const index=recordDrawerHarness.refCursor++;
   return recordDrawerHarness.refs[index]??(recordDrawerHarness.refs[index]={current:initial});
  },
  useEffect:(effect:()=>void|(()=>void),dependencies:readonly unknown[])=>{
   if(!recordDrawerHarness.active)return actual.useEffect(effect,dependencies);
   recordDrawerHarness.effects.push(effect);
  }
 };
});
vi.mock('react-dom',()=>({createPortal:(children:unknown)=>children}));
vi.mock("@erpx/ui/components/workspace",()=>({WorkspaceDetail:(props:{children?:unknown})=>props.children}));
vi.mock("@erpx/ui/components/workspace-overlay",()=>({useWorkspaceOverlay:()=>({editors:0})}));
vi.mock("@erpx/feature-records/components/record-workspace",()=>({useRecordWorkspace:()=>({host:null})}));
vi.mock("@erpx/feature-shared/lib/destination-search",()=>({useSearchParams:()=>({get:()=>null})}));
vi.mock("@erpx/feature-shared/lib/destination-router",()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn(),back:vi.fn()})}));
vi.mock("@erpx/ui/components/dialog",()=>({Dialog:(props:{children?:unknown})=>props.children}));
vi.mock("@erpx/ui/components/button",()=>({Button:(props:{children?:unknown})=>props.children}));
vi.mock("@erpx/ui/components/record-context",()=>({EmbeddedRecord:(props:{children?:unknown})=>props.children}));
vi.mock("@erpx/feature-work_projection/lib/context-switch",()=>({registerUnsavedWork:()=>()=>{}}));
import {RecordDrawer} from '@erpx/feature-records/components/record-drawer';
const base='/holdings/acme';
it('keeps repeated collection filters, pagination and focus fragment when closing a path drawer',()=>{
 const current=`${base}/projects-routines/projects?q=desk&filter=a&filter=b&cursor=page2#projects`;
 const selected=recordDrawerHref(current,{kind:'project',id:'project:line / encoded',args:{section:'documents'}});
 expect(new URL(selected,'https://test.invalid').pathname).toBe(`${base}/projects-routines/projects/project%3Aline%20%2F%20encoded/documents`);
 expect(withoutRecordDrawer(selected)).toBe(current);
 expect(new URL(selected,'https://test.invalid').searchParams.has('detailId')).toBe(false);
});
it('preserves exact in-workspace drawer return selectors without allowing scope escapes',()=>{
 const list=`${base}/finance/documents/sales/invoices?tab=invoices&q=issued&tag=one&tag=two#invoices`,detail=new URL(`${base}/finance/documents/invoices/invoice-1/overview`,'https://erpx.invalid');
 detail.searchParams.set('returnTo',list);
 expect(withoutRecordDrawer(detail.pathname+detail.search)).toBe(list);
 detail.searchParams.set('returnTo',`${base}/../other/finance/documents/sales/invoices?tab=invoices`);
 expect(withoutRecordDrawer(detail.pathname+detail.search)).not.toContain('/holdings/other/');
});
it('opens the same record and section from a direct or modified-click link',()=>{
 const target={kind:'stock_position' as const,id:'product:1',args:{warehouseId:'warehouse:2'}};
 expect(recordTarget(recordCanonicalHref(base,target),base)).toEqual({...target,args:{...target.args,section:'overview'}});
 expect(recordTarget(`${base}/services/legal/contract/a%3Ab/documents`,base)).toEqual({kind:'legal',id:'a:b',args:{kind:'contract',section:'documents'}});
 expect(recordTarget(`${base}/people-organization/people/directory/person-id/pay`,base)?.args?.section).toBe('pay');
});
it('keeps print, external origins and other work scopes outside the drawer',()=>{
 expect(recordTarget(`${base}/finance/documents/invoices/1/overview?print=1`,base)).toBeNull();
 expect(recordTarget('https://example.com/holdings/acme/finance/documents/invoices/1/overview',base)).toBeNull();
 expect(recordTarget('/holdings/another/finance/documents/invoices/1/overview',base)).toBeNull();
});
it('does not mistake submenu and creation routes for record IDs',()=>{
 for(const path of ['people/organization','people/payroll','support/incidents','support/sla','people/move','maintenance/plans','requests/new','people-organization/people/directory/current','administration/technology/directory/connections'])expect(recordTarget(`${base}/${path}`,base)).toBeNull();
});
it('normalizes generated owner links and keeps APIs, filters and workspace scope',()=>{
 expect(canonicalUiHref(`${base}/branches/bangkok/departments/sales/technology?view=directory&section=connections&q=Thai&tag=a&tag=b`)).toBe(`${base}/branches/bangkok/departments/sales/administration/technology/directory/connections?q=Thai&tag=a&tag=b`);
 expect(canonicalUiHref(`${base}/finance/setup?step=books&chart=chart-1`)).toBe(`${base}/finance/setup/books?chart=chart-1`);
 expect(canonicalUiHref(`/api/v1${base}/finance/invoices?kind=payable`)).toBe(`/api/v1${base}/finance/invoices?kind=payable`);
 expect(canonicalDestinationLinks({data:{href:'/finance/payments/payment-1',version:7},api:`/api/v1${base}/finance/payments`,input:{path:'finance/payments'}})).toEqual({data:{href:'/finance/cash/payments/payment-1/overview',version:7},api:`/api/v1${base}/finance/payments`,input:{path:'finance/payments'}});
});
it('keeps legacy organization settings navigation on the organization workspace',()=>{
 const legacy=`${base}/people?view=legacy&company=company-1`,expected=`${base}/people-organization/people/organization?view=legacy&company=company-1&tab=settings`;
 expect(canonicalUiHref(legacy)).toBe(expected);
 expect(canonicalUiHref(expected)).toBe(expected);
 expect(canonicalUiHref(canonicalUiHref(legacy))).toBe(expected);
});
it('provides distinct Server Component pages and clickable breadcrumb parents for every destination',()=>{
 const paths=destinations.map(destination=>destination.path);
 expect(new Set(paths).size).toBe(paths.length);
 for(const path of paths){const file=`apps/compat/src/app/holdings/[holdingSlug]/${path}/page.tsx`;expect(existsSync(file),path).toBe(true);expect(readFileSync(file,'utf8')).not.toMatch(/^['"]use client['"]/);for(const crumb of destinationTrail(base,path).slice(0,-1))expect(existsSync(`apps/compat/src/app/holdings/[holdingSlug]/${crumb.href!.slice(base.length+1)}/page.tsx`),crumb.href).toBe(true);}
 expect(existsSync('src/app/holdings/[holdingSlug]/technology/page.tsx')).toBe(false);
 expect(existsSync('src/app/platform/billing/page.tsx')).toBe(false);
});

it('preserves canonical Purchase and Quotation record identity across repeated normalization and drawer navigation',()=>{
 for(const scope of [base,base+'/branches/bangkok/departments/purchasing'])for(const [view,path] of [['purchase','purchasing/orders'],['quote','customers-sales/sales/documents/quote'],['sales','customers-sales/sales/documents/sales']] as const){
  const legacy=`${scope}/commerce/orders/original-order?view=${view}&tag=one&tag=two&cursor=page2#evidence`,expected=`${scope}/${path}/original-order/overview?tag=one&tag=two&cursor=page2#evidence`;
  expect(canonicalUiHref(legacy)).toBe(expected);
  expect(canonicalUiHref(expected)).toBe(expected);
  expect(canonicalUiHref(canonicalUiHref(expected))).toBe(expected);
  const drawer=recordDrawerHref(`${scope}/${path}?tag=one&tag=two&cursor=page2#records`,{kind:'order',id:'original-order',args:{view}});
  expect(canonicalUiHref(drawer)).toBe(drawer);
  expect(new URL(canonicalUiHref(drawer),'https://test.invalid').pathname).toBe(`${scope}/${path}/original-order/overview`);
  expect(withoutRecordDrawer(drawer)).toBe(`${scope}/${path}?tag=one&tag=two&cursor=page2#records`);
 }
});

it('keeps exact accepted hire handoffs on their owner page instead of treating the path as a Person ID',()=>{
 for(const scope of [base,base+'/branches/bangkok'])for(const section of ['', '/overview']){
  const href=`${scope}/people-organization/people/hire-handoffs/original-handoff${section}?tag=one&tag=two#receipt`;
  expect(canonicalUiHref(href)).toBe(href);expect(canonicalUiHref(canonicalUiHref(href))).toBe(href);
 }
});

it('keeps the accepted hire collection separate from an ordinary Person record across normalization',()=>{
 for(const scope of [base,base+'/branches/bangkok/departments/people']){
  const query='?tag=one&tag=two&cursor=page2#handoffs',expected=`${scope}/people-organization/people/hire-handoffs${query}`;
  expect(canonicalUiHref(`${scope}/people/hire-handoffs${query}`)).toBe(expected);
  expect(canonicalUiHref(expected)).toBe(expected);
  expect(canonicalUiHref(canonicalUiHref(expected))).toBe(expected);
  expect(recordTarget(expected,scope)).toBeNull();
 }
});

it('keeps the exact candidate privacy page and its query separate from ordinary Talent records',()=>{
 for(const suffix of ['', '/overview']){const href=`${base}/people-organization/talent/candidates/original-candidate/privacy${suffix}?tag=one&tag=two#history`;expect(canonicalUiHref(href)).toBe(href);expect(canonicalUiHref(canonicalUiHref(href))).toBe(href);}
});
it('keeps original price-list identity and drawer return filters on Sales instead of requiring the Finance product',()=>{
 for(const scope of [base,base+'/branches/main']){const current=`${scope}/customers-sales/sales/pricing?tag=one&tag=two#prices`,target={kind:'price_list' as const,id:'list:original /version'},href=recordDrawerHref(current,target),url=new URL(href,'https://test.invalid');expect(url.pathname).toBe(`${scope}/customers-sales/sales/pricing/records/price_list/list%3Aoriginal%20%2Fversion/overview`);expect(destinationProduct(url.pathname.slice(scope.length+1))).toBe('sales');expect(canonicalUiHref(href)).toBe(href);expect(canonicalUiHref(canonicalUiHref(href))).toBe(href);expect(recordTarget(href,scope)).toEqual({...target,args:{returnTo:current,section:'overview'}});expect(withoutRecordDrawer(href)).toBe(current);expect(canonicalUiHref(`${scope}/record-details/price_list/original?tag=one&tag=two`)).toBe(`${scope}/customers-sales/sales/pricing/records/price_list/original/overview?tag=one&tag=two`);}
 for(const suffix of ['[id]','[id]/[recordSection]'])for(const slot of ['', '@record/'])expect(existsSync(`apps/compat/src/app/holdings/[holdingSlug]/${slot}customers-sales/sales/pricing/records/price_list/${suffix}/page.tsx`)).toBe(true);
 const path='customers-sales/sales/pricing/records/price_list/original/overview';expect(productPageAllowed('sales',base+'/'+path)).toBe(true);expect(productPageAllowed('finance',base+'/'+path)).toBe(false);expect(productEntryCapability('sales',path)).toBe('sales.orders');
});

class DrawerTarget {
 listeners=new Map<string,(event:unknown)=>void>();
 addEventListener(type:string,listener:(event:unknown)=>void){this.listeners.set(type,listener);}
 removeEventListener(type:string){this.listeners.delete(type);}
 emit(type:string,event:unknown){this.listeners.get(type)?.(event);}
}
class DrawerElement extends DrawerTarget {
 isConnected=true;
 private attributes=new Map<string,string>();
 constructor(private readonly options:{form?:boolean;parentForm?:DrawerElement}={}){super();}
 matches(selector:string){return selector==='form'&&this.options.form===true;}
 closest(selector:string){return selector==='form, [contenteditable="true"]'?this.options.parentForm??null:null;}
 setAttribute(name:string,value:string){this.attributes.set(name,value);}
 getAttribute(name:string){return this.attributes.get(name)??null;}
 querySelector(){return null;}
 removeAttribute(name:string){this.attributes.delete(name);}
 contains(node:unknown){return node===this;}
}
class DrawerMutationObserver {observe(){} disconnect(){}}
afterEach(()=>{recordDrawerHarness.active=false;recordDrawerHarness.states=[];recordDrawerHarness.stateCursor=0;recordDrawerHarness.refs=[];recordDrawerHarness.refCursor=0;recordDrawerHarness.effects=[];recordDrawerHarness.contentNode=undefined;vi.unstubAllGlobals();});
it('keeps a newer RecordDrawer edit dirty when the original command acknowledgement mutates its form stamp',()=>{
 const contentNode=new DrawerElement(),windowTarget=Object.assign(new DrawerTarget(),{location:{href:'https://erpx.example.test/holdings/acme/finance/documents/invoices/1/overview'},history:{state:null}});
 vi.stubGlobal('Element',DrawerElement);vi.stubGlobal('MutationObserver',DrawerMutationObserver);vi.stubGlobal('document',{querySelector:()=>null,querySelectorAll:()=>[]});vi.stubGlobal('window',windowTarget);vi.stubGlobal('crypto',{randomUUID:(()=>{let version=0;return()=>`drawer-edit-${++version}`;})()});
 recordDrawerHarness.active=true;recordDrawerHarness.contentNode=contentNode;
 RecordDrawer({title:'Invoice',fullHref:'/holdings/acme/finance/documents/invoices/1',children:null});
 for(const effect of recordDrawerHarness.effects)effect();
 const form=new DrawerElement({form:true}),field=new DrawerElement({parentForm:form}),inputEvent={target:field};
 const edit=()=>{contentNode.emit('input',inputEvent);windowTarget.emit('input',inputEvent);};
 edit();const original=form.getAttribute('data-erpx-edit-version');expect(original).toBe('drawer-edit-1');expect(recordDrawerHarness.states[2]).toBe(true);
 windowTarget.emit('erpx:record-command-saved',{type:'erpx:record-command-saved',detail:{form,formEditVersion:original}});expect(recordDrawerHarness.states[2]).toBe(false);
 edit();const newer=form.getAttribute('data-erpx-edit-version');expect(newer).toBe('drawer-edit-2');expect(recordDrawerHarness.states[2]).toBe(true);
 // An owner can update the DOM stamp while acknowledging its earlier request.
 // The drawer's retained local stamp must still identify the newer edit.
 form.setAttribute('data-erpx-edit-version',original!);
 windowTarget.emit('erpx:record-command-saved',{type:'erpx:record-command-saved',detail:{form,formEditVersion:original}});expect(recordDrawerHarness.states[2]).toBe(true);
 windowTarget.emit('erpx:record-command-saved',{type:'erpx:record-command-saved',detail:{form,formEditVersion:newer}});expect(recordDrawerHarness.states[2]).toBe(false);
});

it('keeps a newer edit dirty after a late acknowledgement in a mounted React RecordDrawer',async()=>{
 const stubs:Record<string,string>={
  "@erpx/ui/components/workspace":'export function WorkspaceDetail({children}){return children;}',
  "@erpx/ui/components/workspace-overlay":'export function useWorkspaceOverlay(){return {editors:0};}',
  '@erpx/feature-records/components/record-workspace':'export function useRecordWorkspace(){return {host:null};}',
  "@erpx/feature-shared/lib/destination-search":'export function useSearchParams(){return {get:()=>null};}',
  "@erpx/feature-shared/lib/destination-router":'export function useRouter(){return {push(){},pushPreservingQuery(){},replace(){},refresh(){},back(){}};}',
  "@erpx/ui/components/dialog":'export function Dialog({children,title}){return <div role="dialog" aria-label={title}>{children}</div>;}',
  "@erpx/ui/components/button":'export function Button({children,onClick,type="button"}){return <button type={type} onClick={onClick}>{children}</button>;}',
  '@erpx/ui/components/record-context':'export function EmbeddedRecord({children}){return children;}',
  "@erpx/feature-work_projection/lib/context-switch":'export function registerUnsavedWork(){return ()=>{};}',
  "@erpx/feature-records/lib/record-drawer":'export function recordDrawerHref(){return "/record";} export function withoutRecordDrawer(){return "/list";}',
 };
 const bundle=await build({
  stdin:{sourcefile:'record-drawer-mounted-harness.tsx',loader:'tsx',resolveDir:process.cwd(),contents:`
   import {createElement as h,useEffect,useState} from 'react';
   import {createRoot} from 'react-dom/client';
   import {RecordDrawer} from './packages/feature-records/src/components/record-drawer';
   function RecordDrawerHarness(){
    const [generation,setGeneration]=useState(0);
    useEffect(()=>{const replace=()=>setGeneration(value=>value+1);window.addEventListener('test:replace-record-form',replace);return()=>window.removeEventListener('test:replace-record-form',replace);},[]);
    const input=h('input',{name:'note',defaultValue:'Saved value'});
    const form=h('form',{key:generation,'aria-label':'Record fields'},h('label',null,'Record note',input));
    return h(RecordDrawer,{title:'Record',fullHref:'/record/full'},form);
   }
   const root=createRoot(document.querySelector('#root')!);
   root.render(h(RecordDrawerHarness));
   Object.assign(window,{unmountRecordDrawer:()=>root.unmount()});
  `},
  bundle:true,write:false,platform:'browser',format:'iife',globalName:'RecordDrawerMountedHarness',target:'es2022',jsx:'automatic',loader:{'.css':'empty'},
  plugins:[{name:'record-drawer-mounted-boundary-stubs',setup(builder){
   builder.onResolve({filter:/^@(?:\/|erpx\/)/},args=>{if(args.path.endsWith('.css'))return undefined;if(!Object.hasOwn(stubs,args.path))throw new Error(`Missing RecordDrawer test boundary stub: ${args.path}`);return {path:args.path,namespace:'record-drawer-stub'};});
   builder.onResolve({filter:/^\.\//},args=>args.importer.endsWith('/packages/feature-records/src/components/record-drawer.tsx')&&Object.hasOwn(stubs,args.path)?{path:args.path,namespace:'record-drawer-stub'}:undefined);
   builder.onLoad({filter:/.*/,namespace:'record-drawer-stub'},args=>({contents:stubs[args.path]!,loader:'tsx',resolveDir:process.cwd()}));
  }}]
 });
 const server=createServer((_request,response)=>{response.writeHead(200,{'content-type':'text/html'});response.end('<!doctype html><html><body><div id="root"></div></body></html>');});
 await new Promise<void>((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
 const address=server.address();if(!address||typeof address==='string')throw new Error('RecordDrawer browser harness did not acquire a loopback port.');
 let browser:Awaited<ReturnType<typeof chromium.launch>>|undefined;
 try{
  browser=await chromium.launch({headless:true});const page=await browser.newPage();await page.goto(`http://127.0.0.1:${address.port}`);await page.addScriptTag({content:bundle.outputFiles[0]!.text});
  const input=page.locator('input[name="note"]'),content=page.locator('[data-record-drawer-content]'),form=content.locator('form'),status=page.locator('.record-drawer-toolbar [role="status"]');
  await input.fill('Submitted value');await browserExpect(status).toHaveText('Unsaved changes');
  const submittedVersion=await form.getAttribute('data-erpx-edit-version');expect(submittedVersion).toBeTruthy();
  await input.fill('Newer local value');await browserExpect(status).toHaveText('Unsaved changes');
  const newerVersion=await form.getAttribute('data-erpx-edit-version');expect(newerVersion).toBeTruthy();expect(newerVersion).not.toBe(submittedVersion);
  await page.evaluate(version=>{const form=document.querySelector('[data-record-drawer-content] form')!;form.setAttribute('data-erpx-edit-version',String(version));window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form,formEditVersion:version}}));},submittedVersion);
  await browserExpect(status).toHaveText('Unsaved changes');await browserExpect(content).toHaveAttribute('data-record-dirty','true');await browserExpect(input).toHaveValue('Newer local value');
  const replaceForm=()=>page.evaluate(()=>window.dispatchEvent(new Event('test:replace-record-form')));
  await replaceForm();
  const retained=page.locator('[data-record-drawer-retained-draft]');await browserExpect(retained).toBeVisible();await browserExpect(retained.locator('output')).toHaveText('Newer local value');
  await input.fill('Newer local value');const replacementVersion=await form.getAttribute('data-erpx-edit-version');expect(replacementVersion).toBeTruthy();
  await page.evaluate(()=>document.querySelector('[data-record-drawer-content] form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true})));
  await page.evaluate(version=>{const form=document.querySelector('[data-record-drawer-content] form')!;window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form,formEditVersion:version}}));},replacementVersion);
  await browserExpect(status).toHaveText('Unsaved changes');await browserExpect(content).toHaveAttribute('data-record-dirty','true');await browserExpect(retained.locator('output')).toHaveText('Newer local value');

  await page.reload();await page.addScriptTag({content:bundle.outputFiles[0]!.text});
  const reloadedInput=page.locator('input[name="note"]'),reloadedContent=page.locator('[data-record-drawer-content]'),reloadedForm=reloadedContent.locator('form'),reloadedStatus=page.locator('.record-drawer-toolbar [role="status"]');
  await reloadedInput.fill('A later retained value');await replaceForm();const reloadedRetained=page.locator('[data-record-drawer-retained-draft]');await browserExpect(reloadedRetained).toBeVisible();
  await reloadedInput.fill('A different submitted value');const differentVersion=await reloadedForm.getAttribute('data-erpx-edit-version');expect(differentVersion).toBeTruthy();
  await page.evaluate(()=>document.querySelector('[data-record-drawer-content] form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true})));
  await page.evaluate(version=>{const form=document.querySelector('[data-record-drawer-content] form')!;window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form,formEditVersion:version}}));},differentVersion);
  await browserExpect(reloadedStatus).toHaveText('Unsaved changes');await browserExpect(reloadedContent).toHaveAttribute('data-record-dirty','true');await browserExpect(reloadedRetained.locator('output')).toHaveText('A later retained value');
 }finally{await browser?.close();await new Promise<void>((resolve,reject)=>server.close(error=>error?reject(error):resolve()));}
});
