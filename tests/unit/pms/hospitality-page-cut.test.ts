import {expect,it} from 'vitest';
import {productionPageCut,type PageCutEntry} from "@erpx/feature-shared/lib/hospitality-page-cut";

const base='/holdings/riverside/branches/hotel';
const catalogue:PageCutEntry[]=[
  {id:'crm',name:'CRM',state:'Coming Soon'},
  {id:'pms',name:'Hotel operations',state:'Coming Soon'},
  {id:'pos',name:'Restaurant point of sale',state:'Pilot'},
  {id:'support',name:'Support',state:'Coming Soon'},
  {id:'sales',name:'Sales',state:'Coming Soon'},
  {id:'procurement',name:'Purchasing',state:'Pilot'},
  {id:'inventory',name:'Inventory',state:'Coming Soon'},
  {id:'finance',name:'Finance',state:'Coming Soon'},
  {id:'operations',name:'Work',state:'Coming Soon'},
  {id:'people_payroll',name:'People',state:'Coming Soon'},
  {id:'identity_access',name:'Identity & access',state:'Coming Soon'},
  {id:'organization',name:'Organization',state:'Pilot'},
];
const cut=(path:string,query='')=>productionPageCut(`${base}/${path}`,base,new URLSearchParams(query),catalogue);

it('shows the effective production state on disabled module entry points',()=>{
  expect(cut('crm')).toMatchObject({id:'crm',state:'Coming Soon'});
  expect(cut('pms/front-desk')).toMatchObject({id:'pms',state:'Coming Soon'});
  expect(cut('support/sla')).toMatchObject({id:'support',state:'Coming Soon'});
  expect(cut('commerce','view=sales')).toMatchObject({id:'sales',state:'Coming Soon'});
  expect(cut('commerce')).toMatchObject({id:'sales',state:'Coming Soon'});
  expect(cut('commerce','view=unknown')).toMatchObject({id:'sales',state:'Coming Soon'});
  expect(cut('commerce','view=stock')).toMatchObject({id:'inventory',state:'Coming Soon'});
  expect(cut('commerce','view=parties&kind=customer')).toMatchObject({id:'crm',state:'Coming Soon'});
  expect(cut('commerce','view=parties&kind=supplier')).toBeNull();
  expect(cut('stock-counts')).toMatchObject({id:'inventory',state:'Coming Soon'});
  expect(cut('finance/bank')).toMatchObject({id:'finance',state:'Coming Soon'});
  expect(cut('staff-tasks')).toMatchObject({id:'operations',state:'Coming Soon'});
  expect(cut('people')).toMatchObject({id:'people_payroll',state:'Coming Soon'});
  expect(cut('people/organization')).toBeNull();
  expect(cut('people/settings','section=roles')).toMatchObject({id:'identity_access',state:'Coming Soon'});
  expect(cut('finance/engines')).toMatchObject({id:'advanced-finance',state:'Coming Soon'});
  expect(cut('people','view=payroll')).toBeNull(); // Legacy entry redirects to the read-safe Payroll workspace.
  expect(cut('pos')).toBeNull();
});

it('leaves authorized historical details and unrelated pilot screens readable',()=>{
  expect(cut('crm/lead-123')).toBeNull();
  expect(cut('pms/documents/invoice-123')).toBeNull();
  expect(cut('pms/guest-accounts','guestAccount=guestAccount-123')).toBeNull();
  expect(productionPageCut(`${base}/pos`,base,new URLSearchParams('sale=sale-123'),[{id:'pos',name:'Restaurant point of sale',state:'Coming Soon'}])).toBeNull();
  expect(cut('support/incidents/incident-123')).toBeNull();
  expect(cut('commerce','view=purchase')).toBeNull();
  expect(cut('people/payroll/run-123')).toBeNull();
  expect(cut('finance/invoices/invoice-123')).toBeNull();
  expect(cut('stock-counts/count-123')).toBeNull();
  expect(productionPageCut('/holdings/other/crm',base,new URLSearchParams(),catalogue)).toBeNull();
});

// The remaining live Finance and intercepted PMS documents use the exact
// original native query/authority ports and immutable issued snapshot.
import {vi} from 'vitest';
import * as DocumentReact from 'react';
import {renderToStaticMarkup as documentMarkup} from 'react-dom/server';
import {createRequire as documentRequire} from 'node:module';
import {readFileSync as documentRead} from 'node:fs';
import {resolve as documentResolve} from 'node:path';
import {hasPermission as documentPermission} from '@erpx/feature-identity_access/lib/permissions';
import {parseGuestAccountJson as documentSnapshot,legacyGuestAccountOperation as documentLegacyOperation} from '@erpx/feature-identity_access/lib/guest-account-compat';
import {money as documentMoney} from '@erpx/server-pms/public/rules/pms-model';
import {workspaceBase as documentBase} from '@erpx/feature-work_projection/lib/workspace';
import {canonicalUiHref as documentHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {recordCanonicalHref as documentRecordHref,recordSections as documentSections,defaultRecordSection as documentDefaultSection} from '@erpx/feature-records/lib/record-drawer';
type DocumentKind='invoice'|'receipt'|'refund'|'credit_note'|'opening';
const documentWorkspaceBase='/holdings/acme/branches/north';
const documentSource="packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/pms/documents/[id]/standard-document.tsx";
const documentNodes=(tree:DocumentReact.ReactNode,nodes:DocumentReact.ReactElement<Record<string,unknown>>[]=[]):typeof nodes=>{DocumentReact.Children.forEach(tree,node=>{if(!DocumentReact.isValidElement<Record<string,unknown>>(node))return;nodes.push(node);documentNodes(node.props.children as DocumentReact.ReactNode,nodes);});return nodes;};
function documentFixture(kind:DocumentKind='invoice',permissions=['pms.read','pms.reports','finance.read']){
 const actor={userId:'native-reader',companyId:'native-company',assignmentId:'native-current-assignment',role:'document-role',holdingId:'h001',holdingSlug:'acme',branchId:'north',scope:{kind:'branch',holdingSlug:'acme',branchSlug:'north'},permissions},entry=(id:string,kind:string,total:number)=>({id,kind:'entry',data:{date:'2026-10-18',description:'Issued '+id,kind,net:total-1000,service:300,tax:700,total}}),snapshot={property:{name:'Issued original hotel',currency:'THB'},guest:{name:'Issued guest',email:'issued@example.test',phone:'original-phone'},[documentLegacyOperation('guest_account.').slice(0,-1)]:{name:'Issued original account',due:21400},entries:[entry('native-charge','charge',10700),entry('native-adjustment','adjustment',10700),entry('native-transfer','transfer',10700),entry('native-payment','receipt',-10700)],amount:21400,businessDate:'2026-10-18',openingJournalId:'original-opening-journal'},doc={kind,property_id:'native-hotel',guest_account_id:'native-account',finance_id:'native-finance',snapshot_json:JSON.stringify(snapshot)},image=doc.snapshot_json,missing={value:false},scopeFailure={value:null as Error|null},calls:{name:string;args:unknown[]}[]=[],queries={document:vi.fn(async(id:string)=>{calls.push({name:'document',args:[id]});return missing.value?null:doc;}),relatedPostings:vi.fn(async(property:string,id:string)=>{calls.push({name:'postings',args:[property,id]});return[{journal_id:'actual-posted-source-journal'}];})},presentation={pmsDocumentQueries:()=>queries,loadPms:vi.fn(async(value:unknown,property:string)=>{calls.push({name:'load',args:[value,property]});if(scopeFailure.value)throw scopeFailure.value;return{property:{id:property,branchId:'native-branch'}};}),resolveDocumentBrand:vi.fn(async(value:unknown,input:unknown)=>{calls.push({name:'brand',args:[value,input]});return{companyName:'Current authorized Company'};})};return{actor,doc,image,calls,missing,scopeFailure,queries,presentation};
}
async function documentOwner(f:ReturnType<typeof documentFixture>){
 const require=documentRequire(import.meta.url),ts=require('typescript')as typeof import('typescript'),source=documentRead(documentResolve(process.env.ERPX_PMS_DOCUMENT_SOURCE_ROOT??'.',documentSource),'utf8'),loaded={exports:{}as Record<string,unknown>},code=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
 const h=DocumentReact.createElement,control=(tag:string)=>(props:Record<string,unknown>)=>h(tag,{'data-appearance':props.appearance,...(typeof props.href==='string'?{href:props.href}:{}),...(typeof props.label==='string'?{'aria-label':props.label}:{})},props.children as DocumentReact.ReactNode),ports=(path:string):unknown=>{if(path==='react/jsx-runtime')return require(path);if(path.endsWith("@erpx/feature-work_projection/lib/destination-registry"))return{canonicalUiHref:documentHref};if(path.endsWith("@erpx/feature-identity_access/lib/guest-account-compat"))return{parseGuestAccountJson:documentSnapshot};if(path.endsWith('/permissions'))return{hasPermission:documentPermission};if(path.endsWith('/presentation-queries'))return{pmsPresentationQueries:async(holding:string)=>{f.calls.push({name:'presentation',args:[holding]});return f.presentation;}};if(path.endsWith('/record-drawer'))return{recordCanonicalHref:documentRecordHref};if(path.endsWith("@erpx/ui/components/foundation"))return{ActionLink:control('a'),Card:control('section'),Table:control('table'),TableFrame:control('div')};if(path.endsWith("@erpx/ui/components/primitives"))return{PageHeader:(props:Record<string,unknown>)=>h('header',{'data-appearance':props.appearance},h('p',null,String(props.eyebrow)),h('h1',null,String(props.title)),h('p',null,String(props.description)),props.action as DocumentReact.ReactNode)};if(path.endsWith("@erpx/server-pms/public/rules/pms-model"))return{money:documentMoney};if(path.endsWith('/workspace'))return{workspaceBase:documentBase};if(path.endsWith('/authorization'))return{requireWorkContext:async(slug:string)=>{f.calls.push({name:'work',args:[slug]});return f.actor;},publicScope:()=>f.actor.scope};if(path.endsWith('/iam-permissions'))return{requirePermission:(actor:typeof f.actor,permission:string)=>{f.calls.push({name:'permission',args:[actor,permission]});if(!documentPermission(actor,permission))throw Error('NATIVE_PERMISSION_REQUIRED');return actor;}};if(path.endsWith("@erpx/feature-records/components/record-link"))return{RecordLink:control('a')};if(path==='next/navigation')return{notFound:()=>{throw Error('NATIVE_NOT_FOUND');}};if(path.endsWith('.module.css'))return{default:new Proxy({},{get:(_target,key)=>String(key)})};throw Error('Unexpected actual native document owner import '+path);};new Function('require','module','exports',code)(ports,loaded,loaded.exports);const Owner=loaded.exports.default as(props:{params:Promise<{holdingSlug:string;id:string}>})=>Promise<DocumentReact.ReactNode>;return Owner({params:Promise.resolve({holdingSlug:'acme',id:'native-document'})});
}
it.each(['invoice','receipt','refund','credit_note','opening']as const)('actual standard %s document retains issued Money/guest-account data, native source links and original invoice filtering without changing its immutable snapshot',async kind=>{
 const f=documentFixture(kind),tree=await documentOwner(f),html=documentMarkup(tree),parsed=documentSnapshot<{guestAccount:{name:string;due:number};entries:{id:string;data:{total:number}}[]}>(f.doc.snapshot_json);expect(html).toContain('data-pms-document-id="native-document"');expect(html).toContain('data-appearance="standard"');expect(html).not.toContain('business-table');expect(html).toContain('Issued guest');expect(html).toContain('Issued original account');expect(html).toContain(documentMoney(21400));expect(parsed.guestAccount.due).toBe(21400);expect(f.doc.snapshot_json).toBe(f.image);for(const id of ['native-charge','native-adjustment','native-transfer'])expect(html).toContain('Issued '+id);expect(html.includes('Issued native-payment')).toBe(kind!=='invoice');expect(html).toContain('print command to print or save a PDF');expect(f.presentation.loadPms).toHaveBeenCalledExactlyOnceWith(f.actor,'native-hotel');expect(f.presentation.resolveDocumentBrand).toHaveBeenCalledExactlyOnceWith(f.actor,{branchId:'native-branch'});expect(f.queries.relatedPostings).toHaveBeenCalledExactlyOnceWith('native-hotel','native-document');const links=documentNodes(tree).filter(node=>typeof node.props.href==='string').map(node=>node.props.href);expect(links).toContain(documentHref(`${documentWorkspaceBase}/pms/guest-accounts?property=native-hotel&guestAccount=native-account`));expect(links).toContain(documentHref(`${documentWorkspaceBase}/finance/${['receipt','refund'].includes(kind)?'payments':'invoices'}/native-finance`));expect(links).toContain(documentRecordHref(documentWorkspaceBase,{kind:'pms_snapshot_entry',id:'native-charge',args:{documentId:'native-document'}}));expect(links).toContain(documentHref(`${documentWorkspaceBase}/finance/journals/actual-posted-source-journal`));if(kind==='opening'){expect(html).toContain('no new cash receipt or historical revenue');expect(links).toContain(documentHref(`${documentWorkspaceBase}/finance/journals/original-opening-journal`));}
});
it('actual standard opening receipt chooses its original payment source while a current PMS-only reader cannot see Finance journals or document links',async()=>{
 const f=documentFixture('opening',['pms.read','pms.reports']);const original=JSON.parse(f.doc.snapshot_json);original.entries=[original.entries[3]];f.doc.snapshot_json=JSON.stringify(original);const tree=await documentOwner(f),html=documentMarkup(tree);expect(html).not.toContain('Finance document');expect(html).not.toContain('Related journals');expect(html).not.toContain('Approved opening journal');expect(html).toContain('Guest account');expect(f.doc.snapshot_json).toBe(JSON.stringify(original));
 const financial=documentFixture('opening');financial.doc.snapshot_json=f.doc.snapshot_json;expect(documentNodes(await documentOwner(financial)).some(node=>node.props.href===documentHref(`${documentWorkspaceBase}/finance/payments/native-finance`))).toBe(true);
});
it.each(['finance.read','pms.read'] as const)('actual standard document refuses original missing report/cashier authority for %s without rendering private snapshot',async permission=>{
 const f=documentFixture('invoice',[permission]);await expect(documentOwner(f)).rejects.toThrow('NATIVE_NOT_FOUND');expect(f.queries.document).not.toHaveBeenCalled();expect(f.presentation.resolveDocumentBrand).not.toHaveBeenCalled();
});
it('actual standard document preserves missing native record and original scope failures instead of publishing a partial snapshot',async()=>{
 const missing=documentFixture();missing.missing.value=true;await expect(documentOwner(missing)).rejects.toThrow('NATIVE_NOT_FOUND');expect(missing.presentation.loadPms).not.toHaveBeenCalled();const foreign=documentFixture(),failure=Object.freeze(Error('Native Company/property scope denied'));foreign.scopeFailure.value=failure;await expect(documentOwner(foreign)).rejects.toBe(failure);expect(foreign.queries.relatedPostings).not.toHaveBeenCalled();expect(foreign.presentation.resolveDocumentBrand).not.toHaveBeenCalled();expect(foreign.doc.snapshot_json).toBe(foreign.image);
});
it.each(['finance','drawer','drawer-section']as const)('actual %s document caller binds the standard native owner without changing canonical or record-boundary source selectors',async target=>{
 const root=process.env.ERPX_PMS_DOCUMENT_SOURCE_ROOT??'.',path=target==='finance'?"packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/finance/hotel-documents/[id]/screen.tsx":`packages/feature-pms/src/routes/holdings/[holdingSlug]/@record/pms/documents/[id]/${target==='drawer-section'?'[recordSection]/':''}page.tsx`,require=documentRequire(import.meta.url),ts=require('typescript')as typeof import('typescript'),source=documentRead(documentResolve(root,path),'utf8'),loaded={exports:{}as Record<string,unknown>},owner=vi.fn(async(props:Record<string,unknown>)=>{expect(await props.params).toMatchObject({holdingSlug:'acme',id:'native-document'});return DocumentReact.createElement('p',null,'Actual standard native document owner');}),ports=(name:string):unknown=>{if(name==='react/jsx-runtime')return require(name);if(name.endsWith("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/pms/documents/[id]/standard-document"))return{default:owner};if(name.endsWith("@erpx/feature-records/components/record-detail-boundary"))return{RecordDetailBoundary:'NativeRecordDetailBoundary'};if(name.endsWith('/record-drawer'))return{recordSections:documentSections,defaultRecordSection:documentDefaultSection};if(name==='next/navigation')return{notFound:()=>{throw Error('NATIVE_NOT_FOUND');}};throw Error('Unexpected document caller import '+name);};new Function('require','module','exports',ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText)(ports,loaded,loaded.exports);const Page=loaded.exports.default as(props:Record<string,unknown>)=>Promise<DocumentReact.ReactNode>;if(target==='finance'){expect(Page).toBe(owner);await Page({params:Promise.resolve({holdingSlug:'acme',id:'native-document'})});}else{const tree=await Page({params:Promise.resolve({holdingSlug:'acme',id:'native-document',recordSection:'overview'}),searchParams:Promise.resolve({property:'native-hotel',queue:'pending'})});if(!DocumentReact.isValidElement<Record<string,unknown>>(tree))throw Error('Actual native RecordDetailBoundary missing');expect(tree.type).toBe('NativeRecordDetailBoundary');expect(tree.props.target).toMatchObject({kind:'pms_document',id:'native-document',args:{property:'native-hotel',queue:'pending',section:'overview'}});const child=tree.props.children;if(!DocumentReact.isValidElement<Record<string,unknown>>(child)||typeof child.type!=='function')throw Error('Actual document owner content missing');await Reflect.apply(child.type,undefined,[child.props]);}expect(owner).toHaveBeenCalledOnce();
});

it('the original Records owner map resolves only the standard PMS document owner and preserves its native params',async()=>{
 const root=process.env.ERPX_PMS_DOCUMENT_SOURCE_ROOT??'.',require=documentRequire(import.meta.url),ts=require('typescript')as typeof import('typescript'),source=documentRead(documentResolve(root,'packages/server-records/src/adapters/record-owner-pages.ts'),'utf8'),loaded={exports:{}as Record<string,unknown>},resolved:string[]=[],owner=vi.fn(async(props:{params:Promise<{holdingSlug:string;id:string}>})=>{expect(await props.params).toEqual({holdingSlug:'acme',id:'native-document'});return DocumentReact.createElement('p',null,'Actual standard native document owner');}),ports=(path:string):unknown=>{resolved.push(path);if(path==='server-only')return{};if(path==="@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/pms/documents/[id]/standard-document")return{default:owner};throw Error('Unexpected Records owner resolution '+path);};new Function('require','module','exports',ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText)(ports,loaded,loaded.exports);const pages=loaded.exports.recordOwnerPages as{pms_document:()=>Promise<{default:typeof owner}>;invoice:unknown},selected=await pages.pms_document();expect(typeof pages.invoice).toBe('function');expect(selected.default).toBe(owner);await selected.default({params:Promise.resolve({holdingSlug:'acme',id:'native-document'})});expect(resolved).toEqual(['server-only',"@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/pms/documents/[id]/standard-document"]);expect(owner).toHaveBeenCalledOnce();
});
