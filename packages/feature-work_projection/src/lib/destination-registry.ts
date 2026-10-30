import {supportTicketPortalEntry} from '@erpx/contracts/ticket-portal-entry';
import {holdingHostForHost,parseHoldingRoute} from '@erpx/contracts/holding-hosts';
import { recordRoutes,recordBasePath,defaultRecordSection,type RecordKind } from "@erpx/feature-records/lib/record-route-definitions";
import {talentHireSourceEntry,talentCandidatePrivacySourceEntry} from "@erpx/contracts/talent-product-entry";
/** UI routes only. API resources and command contracts keep their owner paths. */
export type Destination = {
  path: string;
  label: string;
  source: string;
  selection: Record<string, string>;
};
const rows: Destination[] = [];
function add(source: string, path: string, label: string, selection: Record<string, string> = {}) {
  rows.push({ source, path, label, selection });
}
function views(source: string, parent: string, key: string, entries: readonly (readonly [string, string])[], defaults: Record<string, string> = {}) {
  for (const [id, label] of entries) add(source, `${parent}/${id}`, label, { ...defaults, [key]: id });
}
views('work', 'work', 'view', [['actions','My actions'],['approvals','Approvals'],['tracking','Tracking'],['team','Team'],['for-me','For me'],['completed','Completed'],['waiting','Waiting'],['requests','My requests'],['all','All work']]);
add('work/routing','work/routing','Routing attention');
add('work/reports','work/reports','Work reports');
views('calendar','calendar','mode',[['mine','My calendar'],['team','Team calendar']]);
add('calendar','calendar/coverage','Coverage',{mode:'team',preset:'coverage'});
for (const [id, tab, label] of [['messages','chats','Messages'],['teams','teams','Teams'],['notifications','updates','Notifications']]) add('inbox',`inbox/${id}`,label,{tab});
const people='people-organization/people';
add('people',`${people}/directory/current`,'Current',{view:'internal',status:'current'});
add('people',`${people}/directory/archived`,'Archived',{view:'internal',status:'archived'});
add('people/organization',`${people}/organization`,'Organization');
add('people/roster',`${people}/roster`,'Roster');
views('people',`${people}/external`,'section',[['users','Collaborators'],['organizations','External organizations'],['teams','External teams']],{view:'external'});
add('people',`${people}/unclassified`,'Unclassified',{view:'unclassified'});
views('people/settings',`${people}/hr-settings`,'section',[['levels','Staff levels'],['leave','Leave & overtime'],['calendars','Holiday calendars'],['pay-groups','Pay groups'],['templates','Organization templates'],['lifecycle','Lifecycle handoff']]);
views('people/evaluation',`${people}/evaluation`,'view',[['table','Compare table'],['graph','Quadrants'],['individual','Individuals'],['cycles','Annual cycles'],['criteria','Criteria & pay bands']]);
add('people/payroll',`${people}/payroll/dashboard`,'Dashboard');
for(const id of ['history','advances','recoveries','settings']) add(`people/payroll/${id}`,`${people}/payroll/${id}`,id[0].toUpperCase()+id.slice(1));
add('people/payroll/settings/posting',`${people}/payroll/settings/posting`,'Payroll posting setup');
add('people/payslips',`${people}/payslips`,'My payslips');
views('benefits','people-organization/benefits','view',[['plans','Benefit plans'],['enrollments','Enrollments'],['mine','My benefits']]);
const sales='customers-sales/sales',customers='customers-sales/customers';
views('sales',sales,'view',[['today','Today'],['pipeline','Pipeline'],['reports','CRM reports']]);
add('sales',`${sales}/documents/quote`,'Quotations',{view:'documents',type:'quote'});
add('sales',`${sales}/documents/sales`,'Sales orders',{view:'documents',type:'sales'});
add('sales',`${sales}/settings/posting`,'Sales posting setup',{view:'settings',setup:'posting'});
add('sales',`${sales}/followups`,'Follow-ups',{view:'today',kind:'followups'});
add('commerce',`${sales}/pricing`,'Price lists',{view:'pricing'});
views('customers',customers,'view',[['directory','Customer directory'],['contacts','Prospects & contacts']]);
add('customers',`${customers}/prospects`,'Prospects',{view:'contacts',kind:'prospect'});
for(const review of ['identity','duplicates','access']) add('customers',`${customers}/reviews/${review}`,review==='identity'?'Identity reviews':review==='duplicates'?'Duplicate reviews':'Access reviews',{view:'reviews',review});
add('customers',`${customers}/service/cases`,'Support cases',{view:'support'});
add('support/incidents',`${customers}/service/incidents`,'Shared incidents');
add('support/sla',`${customers}/service/sla`,'Resolution-time watch');
add('support/sessions',`${customers}/service/sessions`,'Temporary support access');
add('support/ticket-portal',`${customers}/service/portal`,'Customer ticket access');
for(const [view,label,path] of [['purchase','Purchase orders','purchasing/orders'],['parties','Suppliers','purchasing/suppliers'],['stock','Stock & movements','inventory-quality/stock'],['products','Products','inventory-quality/products'],['warehouses','Warehouses','inventory-quality/warehouses']]) add('commerce',path,label,{view,...(view==='parties'?{kind:'supplier'}:{})});
add('commerce','purchasing/setup/posting','Posting setup',{view:'purchase',setup:'posting'});
for(const [source,path,label] of [['procurement-rfqs','purchasing/rfqs','Supplier quotation requests'],['supplier-bills','purchasing/supplier-bills','Supplier bills'],['stock-counts','inventory-quality/counts','Stock counts'],['stock-transfers','inventory-quality/transfers','Warehouse transfers'],['stock-transit','inventory-quality/transit','Stock in transit'],['quality','inventory-quality/quality','Quality holds'],['projects','projects-routines/projects','All projects'],['staff-tasks','projects-routines/tasks','Staff tasks'],['routine-templates','projects-routines/routines','Routine templates'],['operations','projects-routines/operations','Operations'],['field-service','services/field-service','Field service'],['requests','services/desk','Service desk']]) add(source,path,label);
add('projects','projects-routines/projects/settings/workers','Project worker preparation',{view:'worker-preparation'});
add('field-service','services/field-service/settings/parts','Service parts preparation',{view:'parts-setup'});
add('field-service','services/field-service/settings/initial-stock','Service initial physical stock',{view:'initial-stock'});
views('assets','services/assets','view',[['register','Asset register'],['accounting','Asset accounting'],['maintenance','Maintenance work'],['plans','Preventive plans']]);
const legalKinds=[['request','Requests & advice'],['matter','Cases & disputes'],['contract','Contracts'],['obligation','Obligations'],['compliance','Compliance'],['license','Licenses'],['corporate','Corporate records'],['ip','Intellectual property'],['privacy','Privacy'],['counsel','Outside counsel'],['template','Templates & clauses'],['rule','Jurisdiction rules']] as const;
views('legal','services/legal','kind',legalKinds);
views('legal','services/legal','workView',[['mine','My legal work'],['deadlines','Deadlines']]);
views('legal','services/legal','tool',[['reports','Reports'],['knowledge','Knowledge search'],['thailand','Thailand starter pack'],['laws','Official law sources'],['release','Company readiness']]);
add('learn','knowledge/guides','Guides');
add('settings','knowledge/help','Help using ERPX',{section:'feedback'});
add('future-features','knowledge/roadmap','Product roadmap');
views('knowledge','knowledge/handbook','section',[['published','Published handbook'],['drafts','Article drafts'],['editors','Editorial access']],{view:'handbook'});
add('knowledge','knowledge/learning/settings/learners','Learning worker preparation',{view:'worker-preparation'});
views('knowledge','knowledge/learning','section',[['courses','Courses'],['assignments','Learning assignments'],['mine','My learning']],{view:'learning'});
add('executive','insights/executive','AI Live Canvas');
add('technology','administration/technology/organization','Organization & Access',{view:'organization'});
for(const [view,sections] of Object.entries({resources:[['packages','Packages'],['accounts','Accounts'],['groups','Groups'],['policies','ERPX policies'],['tasks','Tasks']],directory:[['objects','Objects & OUs'],['connections','Connections'],['mappings','Field mappings'],['work','Work & verification']],settings:[['attributes','Custom attributes'],['roles','Roles & permissions'],['matrix','Access matrix'],['groups','Group mappings'],['reviews','Reviews & audit']]})) views('technology',`administration/technology/${view}`,'section',sections as [string,string][],{view});
views('settings','administration/approvals','tab',[['templates','Routes'],['issues','Routing issues'],['history','Routing history']],{section:'approvals'});
views('settings','administration/brand','tab',[['identity','Brand identity'],['media','Brand media'],['history','Brand version history']],{section:'brand'});
add('settings','administration/ai','AI providers',{section:'ai'});
add('settings','administration/usage','Usage & billing',{section:'usage'});
for(const [source,path,label] of [['settings/business-readiness','administration/business-readiness','Business readiness'],['agents','administration/agents','Agent governance'],['settings/pilot-features','administration/pilot-features','Pilot feature rollout'],['settings/notifications','administration/notifications','Notification settings'],['settings/stripe','administration/stripe','Company Stripe']]) add(source,path,label);
views('finance','finance/today','view',[['actions','My actions'],['waiting','Waiting'],['team','Team']]);
for(const [id,label] of [['quote','Quotations'],['sales','Sales orders'],['invoices','Invoices'],['receipts','Receipts']]) add('finance/sales',`finance/documents/sales/${id}`,label,{tab:id==='quote'?'quotations':id==='sales'?'orders':id});
for(const [id,label] of [['receivable','Receivables'],['payable','Payables']]) add('finance/invoices',`finance/documents/${id}`,label,{kind:id});
for(const [source,path,label] of [['expenses','finance/documents/expenses','Employee expenses'],['finance/contracts','finance/documents/contracts','Business contracts'],['finance/due-drafts','finance/documents/due-drafts','Scheduled drafts'],['finance/sales/settings','finance/documents/sales/settings','Sales settings'],['finance/payments','finance/cash/payments','Payments'],['finance/bank','finance/cash/bank','Bank reconciliation'],['finance/supplier-batches','finance/cash/supplier-batches','Supplier batches'],['finance/refunds','finance/cash/refunds','Refunds'],['finance/collections','finance/cash/collections','Collections & cash forecast'],['finance/accounting','finance/accounting/journals','Journals'],['finance/tax','finance/accounting/tax','Tax'],['finance/close','finance/accounting/close','Period close'],['finance/hotels','finance/accounting/hotels','Hotel source reconciliation'],['finance/restaurants','finance/accounting/restaurants','Restaurant accounting'],['finance/engines','finance/accounting/engines','Calculations & schedules'],['finance/assets','finance/accounting/assets','Fixed assets'],['finance/instruments','finance/accounting/instruments','Leases & loans'],['finance/book-adjustments','finance/accounting/book-adjustments','Secondary book adjustments'],['finance/reports','finance/reports/company','Company reports'],['finance/group','finance/reports/group','Group reports'],['finance/group-editions','finance/reports/group-editions','Retained group editions'],['finance/hospitality-settings','finance/setup/hospitality-settings','Hotel accounting settings'],['finance/hotel-import','finance/setup/hotel-import','Hotel import'],['finance/asset-policies','finance/setup/asset-policies','Asset depreciation policies'],['finance/thai-tax','finance/setup/thai-tax','Thai statutory identities/documents']]) add(source,path,label);
add('finance/setup/expenses','finance/setup/expenses','Expense posting setup');
views('finance/setup','finance/setup','step',[['checklist','Readiness'],['chart','Charts & accounts'],['profile','Company accounting policy'],['opening','Opening balances'],['books','Books & dimensions'],['tools','Company tools'],['policy','Go-live review'],['ai','AI preparation']]);
add('pos','pos/approvals','Approvals',{tab:'history',queue:'approval'});
add('pos','pos/settings/startup','POS startup',{tab:'settings',setup:'startup'});
add('pos','pos/settings/initial-stock','POS initial physical stock',{tab:'settings',setup:'initial-stock'});
views('pos','pos','tab',[['reports','Reports'],['settings','Settings']]);
add('pms/guest-accounts','pms/approvals','Approvals',{queue:'approval'});
const platformRoot='erpx-management';
add('manage-erpx',platformRoot,'Overview');
views('manage-erpx/billing',`${platformRoot}/billing`,'tab',[['target','Measured usage & target'],['overview','Legacy Support'],['plans','Legacy plans'],['invoices','Costs & billing periods'],['payment','Payment'],['history','History & changes']]);
add('manage-erpx/billing/usage',`${platformRoot}/usage`,'Customer usage');
for(const [id,label] of [['overview','Overview'],['users','Users'],['roles','Roles & permissions'],['groups','Groups & mappings'],['matrix','Access matrix'],['connections','Directory connections'],['reviews','Reviews & audit'],['my-access','My access']]) add(`manage-erpx/iam/${id}`,`${platformRoot}/iam/${id}`,label);
add('manage-erpx/executive',`${platformRoot}/executive`,'Platform Executive');

for(const [source,path,label] of [['home','home','Today'],['talent','people-organization/talent','Talent'],['marketing','customers-sales/marketing','Marketing'],['ecommerce','customers-sales/ecommerce','E-commerce / Channels'],['planning','production/planning','Planning'],['manufacturing','production/manufacturing','Manufacturing'],['hospitality/setup','pms/setup','Hospitality setup'],['pms/import','pms/import','Import']])add(source,path,label);
add('manufacturing','production/manufacturing/settings/posting','Manufacturing posting setup',{setup:'posting'});
add('manufacturing','production/manufacturing/settings/initial-stock','Initial physical material count',{setup:'initial-stock'});
for(const [id,label] of [['dashboard','Today'],['reservations','Reservations'],['front-desk','Front desk'],['rooms','Rooms'],['housekeeping','Housekeeping'],['guest-accounts','Guest accounts'],['cashier','Cashier'],['night-audit','Night audit'],['minibar','Minibar & linen'],['guests','Guests'],['reports','Reports'],['settings','Settings']])add('pms/'+id,'pms/'+id,label);
export const destinations: readonly Destination[] = rows;
/** Collection/detail prefixes. Longest source wins; no HTTP redirects exist. */
export const routePrefixes: Readonly<Record<string,string>> = {
 'payroll':'people-organization/people/payroll','people':'people-organization/people/directory','people/settings':'people-organization/people/hr-settings','people/payroll':'people-organization/people/payroll','people/payslips':'people-organization/people/payslips','people/organization':'people-organization/people/organization','people/roster':'people-organization/people/roster','people/evaluation':'people-organization/people/evaluation','people/access':'people-organization/people/users-access','people/clearance':'people-organization/people/clearance','people/hire-handoffs':'people-organization/people/hire-handoffs','people/move':'people-organization/people/move',
 'talent':'people-organization/talent','benefits':'people-organization/benefits','marketing':'customers-sales/marketing','ecommerce':'customers-sales/ecommerce','customers':'customers-sales/customers/directory','crm':'customers-sales/sales/pipeline','customer-followups':'customers-sales/sales/followups','customer-identity':'customers-sales/customers/reviews/identity','customer-duplicates':'customers-sales/customers/reviews/duplicates','customer-access':'customers-sales/customers/reviews/access','support':'customers-sales/customers/service/cases','support/incidents':'customers-sales/customers/service/incidents','support/sessions':'customers-sales/customers/service/sessions','support/sla':'customers-sales/customers/service/sla','support/ticket-portal':'customers-sales/customers/service/portal',
 'commerce/orders':'customers-sales/sales/documents/sales','commerce':'customers-sales/sales/documents/sales','procurement-rfqs':'purchasing/rfqs','supplier-bills':'purchasing/supplier-bills','landed-costs':'purchasing/landed-costs','stock-counts':'inventory-quality/counts','stock-transfers':'inventory-quality/transfers','stock-transit':'inventory-quality/transit','quality':'inventory-quality/quality',
 'projects':'projects-routines/projects','staff-tasks':'projects-routines/tasks','routine-templates':'projects-routines/routines','operations':'projects-routines/operations','field-service':'services/field-service','requests':'services/desk','assets':'services/assets/register','maintenance':'services/assets/maintenance','maintenance/plans':'services/assets/plans','legal':'services/legal','planning':'production/planning','manufacturing':'production/manufacturing',
 'handbook':'knowledge/handbook/published','learning':'knowledge/learning/assignments','learn':'knowledge/guides','future-features':'knowledge/roadmap','executive':'insights/executive','technology':'administration/technology','settings':'administration','agents':'administration/agents','usage':'administration/usage',
 'finance':'finance/today','finance/sales':'finance/documents/sales','finance/invoices':'finance/documents/invoices','expenses':'finance/documents/expenses','finance/contracts':'finance/documents/contracts','finance/due-drafts':'finance/documents/due-drafts','finance/payments':'finance/cash/payments','finance/bank':'finance/cash/bank','finance/supplier-batches':'finance/cash/supplier-batches','finance/refunds':'finance/cash/refunds','finance/collections':'finance/cash/collections','finance/accounting':'finance/accounting/journals','finance/journals':'finance/accounting/journals','finance/tax':'finance/accounting/tax','finance/close':'finance/accounting/close','finance/hotels':'finance/accounting/hotels','finance/hotel-accounts':'finance/accounting/hotels/accounts','finance/hotel-documents':'finance/accounting/hotels/documents','finance/restaurants':'finance/accounting/restaurants','finance/engines':'finance/accounting/engines','finance/assets':'finance/accounting/assets','finance/instruments':'finance/accounting/instruments','finance/book-adjustments':'finance/accounting/book-adjustments','finance/reports':'finance/reports/company','finance/group':'finance/reports/group','finance/group-editions':'finance/reports/group-editions','finance/hospitality-settings':'finance/setup/hospitality-settings','finance/hotel-import':'finance/setup/hotel-import','finance/asset-policies':'finance/setup/asset-policies','finance/thai-tax':'finance/setup/thai-tax','finance/thai-documents':'finance/setup/thai-documents','finance/statement-packages':'finance/reports/statement-packages',
 'hospitality/setup':'pms/setup','feed':'home','manage-erpx':'erpx-management','manage-erpx/billing/usage':'erpx-management/usage','manage-erpx/iam':'erpx-management/iam',
};
const prefixes=Object.entries(routePrefixes).sort((a,b)=>b[0].length-a[0].length);
export function routePath(source:string) {
 const match=prefixes.find(([old])=>source===old||source.startsWith(old+'/'));
 return match?match[1]+source.slice(match[0].length):source;
}
export function destinationForPath(path:string) { return destinations.find(item=>item.path===path); }
export function sourcePath(path:string) {
 if(path==='payroll'||path.startsWith('payroll/'))path='people/'+path;
 const order=path.match(/^(?:purchasing\/orders|customers-sales\/sales\/documents\/quote)(\/.*)$/);if(order)return 'commerce/orders'+order[1];
 const exact=destinationForPath(path); if(exact)return exact.source;
 if(Object.hasOwn(parentLabels,path)){const children=destinations.filter(item=>item.path.startsWith(path+'/'));if(children.length&&children.every(item=>item.source===children[0].source))return children[0].source;}
 const match=Object.entries(routePrefixes).sort((a,b)=>b[1].length-a[1].length).find(([,next])=>path===next||path.startsWith(next+'/'));
 return match?match[0]+path.slice(match[1].length):path;
}
export function workspacePathParts(pathname:string,holdingSlug?:string|null) {
 if(!pathname.startsWith('/holdings/')&&holdingSlug!==null){
  const holding=holdingSlug??(typeof window==='undefined'?undefined:holdingHostForHost(window.location.host)?.holdingSlug);
  const route=holding?parseHoldingRoute(holding,pathname):null;
  if(route?.internalPath.startsWith('/holdings/'))return workspacePathParts(route.internalPath,null);
 }
 const match=pathname.match(/^(\/holdings\/[^/]+(?:\/branches\/[^/]+)?(?:\/departments\/[^/]+)?)(?:\/(.*))?$/);
 return match?{base:match[1],path:match[2]??''}:null;
}
export function selectionForPath(pathname:string):Record<string,string> {
 const receipt=pathname.match(/\/(sale|history|shifts)\/receipts\/([^/]+)\/overview$/);
 if(receipt)return {tab:receipt[1],detailKind:'pos_sale',detailId:decodeURIComponent(receipt[2]),sale:decodeURIComponent(receipt[2])};
 const front=pathname.match(/^(?:\/front\/[^/]+\/[^/]+\/(?:pos|pms)|\/[^/]+\/[^/]+)\/(?:holding|branches\/[^/]+(?:\/departments\/[^/]+)?|departments\/[^/]+)\/(sale|kitchen|shifts|history)$/);
 if(front)return {tab:front[1]};
 const path=workspacePathParts(pathname)?.path??'';
 if(path.startsWith('people-organization/people/move'))return {panel:'move'};
 const explorer=path.match(/^people-organization\/people\/directory\/(person|position|department|team|branch|company|level|unassigned)\/([^/]+)\/([^/]+)$/);
 if(explorer)return {[explorer[1]]:decodeURIComponent(explorer[2]),tab:explorer[3],view:'internal',status:'current'};
 const technology=path.match(/^administration\/technology\/organization\/(person|position|department|team|branch|company|level|unassigned)\/([^/]+)\/([^/]+)$/);
 if(technology)return {[technology[1]]:decodeURIComponent(technology[2]),view:'organization',detail:'1',section:technology[3]==='delivery'?'operations':technology[3]};
 const local=path.match(/^(.*)\/records\/([^/]+)\/([^/]+)\/overview$/);
 if(local)return {...destinationForPath(local[1])?.selection,detailKind:decodeURIComponent(local[2]),detailId:decodeURIComponent(local[3])};
 return destinationForPath(path)?.selection??{};
}
/** Normalize newly generated links, including scoped workspaces and retained filters. */
export function canonicalUiHref(href:string):string {
 if(!href||href.startsWith('/api/')||href.startsWith('#'))return href;
 const absolute=/^https?:\/\//.test(href);
 let url:URL; try{url=new URL(href,'https://routes.invalid');}catch{return href;}
 if(absolute&&!/(^|\.)jesadakorn\.com$/.test(url.hostname)&&!/(^|\.)localhost$/.test(url.hostname)&&url.hostname!=='127.0.0.1')return href;
 const front=url.pathname.match(/^(\/front\/[^/]+\/[^/]+\/(pos|pms)\/(?:holding|branches\/[^/]+(?:\/departments\/[^/]+)?|departments\/[^/]+))\/?(?:sale|kitchen|shifts|history|today|dashboard)?$/);
 const standalone=url.pathname.match(/^(\/[^/]+\/[^/]+\/(?:holding|branches\/[^/]+(?:\/departments\/[^/]+)?|departments\/[^/]+))\/?(?:sale|kitchen|shifts|history)?$/);
 const frontTab=url.searchParams.get('tab');
 if((front||standalone)&&frontTab&&['sale','kitchen','shifts','history'].includes(frontTab)) {url.pathname=(front??standalone)![1]+'/'+frontTab;url.searchParams.delete('tab');return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;}
 if(url.pathname==='/platform'||url.pathname.startsWith('/platform/')) {const path=url.pathname.slice('/platform'.length).replace(/^\//,'');const platformPath=path==='home'||!path?'':path==='billing/usage'?'usage':path.startsWith('billing')?'billing'+path.slice('billing'.length):path;return canonicalUiHref(`${absolute?url.origin:''}/holdings/erpx/erpx-management${platformPath?'/'+platformPath:''}${url.search}${url.hash}`);}
 const parts=workspacePathParts(url.pathname,null); if(!parts||!parts.path)return href;
 // This typed handoff has its own exact owner page, rather than a Person
 // record whose ID happens to be the hire-handoffs path segment.
 if(talentHireSourceEntry(parts.path)||talentCandidatePrivacySourceEntry(parts.path))return href;
 const portalPath=routePath(parts.path);
 if(supportTicketPortalEntry(portalPath)){url.pathname=parts.base+'/'+portalPath;return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;}
 const legacyPeopleAccess=parts.path.match(/^people-organization\/people\/users-access(?:\/(users|organizations|teams|roles))?$/);
 if(legacyPeopleAccess){const requestedSection=legacyPeopleAccess[1]??url.searchParams.get('section');const section=requestedSection&&['users','roles','organizations','teams'].includes(requestedSection)?requestedSection:'users';url.pathname=parts.base+'/'+(section==='users'?'administration/technology/resources/accounts':section==='roles'?'administration/technology/settings/roles':`people-organization/people/external/${section}`);for(const key of ['view','section'])url.searchParams.delete(key);return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;}
 const exact=destinationForPath(parts.path),source=exact?.source??sourcePath(parts.path);
 // A canonical order path already selects its owner kind. Link, drawer and
 // product routing may normalize it again after legacy selectors were removed.
 const orderPath=parts.path.match(/^(purchasing\/orders|customers-sales\/sales\/documents\/(?:quote|sales))\/[^/]+(?:\/[^/]+)?$/);
 const orderView=orderPath?(orderPath[1]==='purchasing/orders'?'purchase':orderPath[1].endsWith('/quote')?'quote':'sales'):null;
 const query:Record<string,string>={...exact?.selection,...Object.fromEntries(url.searchParams),...(orderView?{view:orderView}:{})};
 // The People organization workspace retains its legacy structure editor for
 // owners reviewing branch and department settings. Its query-driven company
 // selection must stay on that owner page when the editor updates the URL.
 if(source==='people'&&query.view==='legacy'&&(query.company||query.branch)){
  url.pathname=`${parts.base}/people-organization/people/organization`;
  // The server selects this tab for legacy organization settings, so serialize
  // that selection for client components that read the URL after hydration.
  if(!url.searchParams.has('tab'))url.searchParams.set('tab','settings');
  return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;
 }
 if(source==='work'&&query.new&&['task','expense','purchase_request'].includes(query.new)){const route=query.new==='task'?'projects-routines/tasks/new':query.new==='expense'?'finance/documents/expenses/new':'services/desk/new';url.pathname=parts.base+'/'+route;if(query.new==='purchase_request')url.searchParams.set('kind','purchase');for(const key of ['new','view','queue','type','responsibility'])url.searchParams.delete(key);return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;}
 if(source==='people'&&query.new){query.view='internal';query.status??='current';}
 if(source==='legal'&&query.new)query.kind??='request';
 if(source==='sales'&&query.view==='documents'&&query.new)query.type??='sales';
 if(source==='finance/invoices'&&query.new)query.kind??='receivable';
 if(source==='finance/sales'&&query.new==='invoice')query.tab='invoices';

 if(source==='people'&&query.view==='users'){const section=['users','roles','organizations','teams'].includes(query.section)?query.section:'users';url.pathname=parts.base+'/'+(section==='users'?'administration/technology/resources/accounts':section==='roles'?'administration/technology/settings/roles':`people-organization/people/external/${section}`);for(const key of ['view','section'])url.searchParams.delete(key);return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;}
 const iamSection=parts.path.startsWith('iam/')?parts.path.slice(4):parts.path==='iam'?'overview':source==='people/settings'&&['attributes','roles','matrix','groups','reviews','connections'].includes(query.section)?query.section:null;
 if(iamSection){if(iamSection==='my-access')return '/my-access';const normalizedIam=iamSection==='mappings'?'groups':iamSection;const route=['overview','users'].includes(normalizedIam)?'administration/technology/resources/accounts':`administration/technology/${normalizedIam==='connections'?'directory':'settings'}/${normalizedIam}`;url.pathname=parts.base+'/'+route;for(const key of ['section','view','tab'])url.searchParams.delete(key);return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;}
 if(source==='people'&&(query.panel==='schedule'||query.panel==='move'||query.panel==='settings'||query.view==='payroll')){const route=query.panel==='schedule'?'people-organization/people/roster':query.panel==='move'?'people-organization/people/move':query.panel==='settings'?'people-organization/people/hr-settings':'people-organization/people/payroll/dashboard';url.pathname=parts.base+'/'+route;url.searchParams.delete('panel');url.searchParams.delete('view');return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;}

 if(source==='work'&&!query.view&&query.queue)query.view=query.queue==='all'?'all':query.queue==='completed'?'completed':query.queue==='waiting'?'waiting':'actions';
 if(Object.hasOwn(parentLabels,parts.path)&&![...url.searchParams.keys()].some(key=>['view','section','tab','step','mode','preset','record','sale','periodId','detailKind','queue'].includes(key)))return href;
 const options=destinations.filter(item=>item.source===source && (source!=="people" || query.view || item.selection.view!=="unclassified"));
 let selected=options.filter(item=>Object.entries(item.selection).every(([key,value])=>query[key]===value)).sort((a,b)=>Object.keys(b.selection).length-Object.keys(a.selection).length)[0];
 let selectedParent:string|undefined;
 if(!selected&&options.length){const partial=options.filter(item=>Object.entries(item.selection).every(([key,value])=>query[key]===undefined||query[key]===value));const missing=(item:Destination)=>Object.keys(item.selection).filter(key=>query[key]===undefined).length;partial.sort((a,b)=>missing(a)-missing(b));const winners=partial.filter(item=>missing(item)===missing(partial[0]));if(winners.length===1)selected=winners[0];else if(winners.length){const segments=winners[0].path.split('/');while(segments.length&&!winners.every(item=>item.path.startsWith(segments.join('/')+'/')))segments.pop();selectedParent=segments.join('/')||undefined;}}
 let path=selected?.path??selectedParent??routePath(source);
 // Existing owner link inputs become record paths before reaching the browser.
 const recordKind = source==='legal'&&query.record?'legal':source==='pos'&&query.sale?'pos_sale':source==='finance/close'&&query.periodId?'finance_period':null;
 if(recordKind){const id=query.record??query.sale??query.periodId;path=recordBasePath(recordKind,query,routePath).replace(':id',encodeURIComponent(id))+'/'+(query.section??'overview');for(const key of ['record','sale','periodId','section','kind','tab'])url.searchParams.delete(key);}
 else if(!exact&&!options.length&&!selectedParent&&!Object.hasOwn(routePrefixes,source)){for(const kind of Object.keys(recordRoutes) as RecordKind[]){const definition=recordRoutes[kind].route;if(definition.includes('?')||kind==='work_flow')continue;const oldPrefix=definition.split(':id')[0],newPrefix=recordBasePath(kind,query,routePath).split(':id')[0];const rest=source.startsWith(oldPrefix)?source.slice(oldPrefix.length):parts.path.startsWith(newPrefix)?parts.path.slice(newPrefix.length):'';const [id,section,...tail]=rest.split('/');if(id&&!tail.length&&!['new','current','archived','settings','history','runs','drafts'].includes(id)){const next=kind==='project'?query.view??section??'overview':kind==='person'?query.tab??section??'profile':section??defaultRecordSection(kind);path=newPrefix+id+'/'+next;if(kind==='order'){url.searchParams.delete('view');url.searchParams.delete('kind');url.searchParams.delete('type');}if(kind==='project'){url.searchParams.delete('view');url.searchParams.delete('section');}if(kind==='person'){url.searchParams.delete('tab');url.searchParams.delete('section');}break;}}}
 const explorerKinds=['person','position','department','team','branch','company','level','unassigned'];
 const explorerKind=explorerKinds.find(kind=>typeof query[kind]==='string'&&query[kind]);
 if(source==='people'&&!['external','unclassified','users'].includes(query.view)&&explorerKind&&!query.new&&!query.panel){path='people-organization/people/directory/'+explorerKind+'/'+encodeURIComponent(query[explorerKind])+'/'+(query.tab??query.section??(explorerKind==='person'?'profile':'overview')).toLowerCase().replaceAll(' ','-');for(const key of [...explorerKinds,'tab','section'])url.searchParams.delete(key);}
 if(source==='technology'&&query.view==='organization'&&explorerKind){path='administration/technology/organization/'+explorerKind+'/'+encodeURIComponent(query[explorerKind])+'/'+(query.section==='operations'?'delivery':query.section??'policies');for(const key of [...explorerKinds,'section','detail'])url.searchParams.delete(key);}
 if(query.detailKind&&query.detailId&&!recordKind){const collection=exact?.path??selected?.path??path;const nativeKind=Object.hasOwn(recordRoutes,query.detailKind)?query.detailKind as RecordKind:null;path=nativeKind?recordBasePath(nativeKind,query,routePath).replace(':id',encodeURIComponent(query.detailId))+'/'+(query.detailSection??defaultRecordSection(nativeKind)):collection+'/records/'+encodeURIComponent(query.detailKind)+'/'+encodeURIComponent(query.detailId)+'/overview';for(const key of ['detailKind','detailId','detailArgs','detailMode','detailPage'])url.searchParams.delete(key);}

 for(const key of new Set(options.flatMap(item=>Object.keys(item.selection)))){
  if(source==='people'&&['external','unclassified'].includes(query.view)&&key==='status')continue;
  url.searchParams.delete(key);
 }
 url.pathname=`${parts.base}/${path}`;
 return `${absolute?url.origin:''}${url.pathname}${url.search}${url.hash}`;
}
export function destinationTrail(base:string,path:string) {
 const segments=path.split('/').filter(Boolean);
 return segments.map((segment,index)=>{const prefix=segments.slice(0,index+1).join('/');return {href:`${base}/${prefix}`,label:destinationForPath(prefix)?.label??parentLabels[prefix]??segment.replaceAll('-',' ').replace(/^./,c=>c.toUpperCase())};});
}
export const parentLabels:Readonly<Record<string,string>>={
 'people-organization':'People & organization','people-organization/people':'People','people-organization/people/directory':'Employees','people-organization/people/external':'External collaborators','people-organization/people/users-access':'Users & Access','people-organization/people/hr-settings':'HR settings','people-organization/people/evaluation':'Skills & performance','people-organization/people/payroll':'Payroll',
 'customers-sales':'Customers & sales','customers-sales/sales':'Sales','customers-sales/sales/documents':'Documents','customers-sales/customers':'Customers','customers-sales/customers/reviews':'Reviews','customers-sales/customers/service':'Service',
 'purchasing':'Purchasing','inventory-quality':'Inventory & quality','projects-routines':'Projects & routines','services':'Services','services/assets':'Assets & maintenance','services/legal':'Legal','production':'Production','knowledge':'Knowledge','knowledge/handbook':'Handbook','knowledge/learning':'Learning','insights':'Insights','administration':'Administration','administration/technology':'Technology','administration/technology/resources':'Resources','administration/technology/directory':'Directory','administration/technology/settings':'Access settings','administration/approvals':'Approval routing','administration/brand':'Brand',
 'work':'My work','calendar':'Calendar','inbox':'Messages','finance':'Finance','finance/today':'Today','finance/documents':'Documents','finance/documents/sales':'Sales & collections','finance/cash':'Cash','finance/accounting':'Accounting','finance/reports':'Reports','finance/setup':'Setup','pos':'POS','pms':'Hotel PMS','erpx-management':'ERPX Management','erpx-management/billing':'Customer billing','erpx-management/iam':'Platform IAM',
};
