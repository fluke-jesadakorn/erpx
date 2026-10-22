import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createElement, type ComponentProps } from 'react';
import { renderToReadableStream } from 'react-dom/server.browser';
import Loadable from 'next/dist/shared/lib/loadable.shared-runtime';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FrontCompanyApp } from '@erpx/server-work_projection/public/front-app';
import type { PmsHome } from '@erpx/server-pms/public/pms/home';
import type { PosWorkspace } from '@erpx/server-pos/public/pos';
import type { CurrencyContextData } from '@erpx/feature-finance/components/money/currency-provider';
import type { StandardFrontAppShell as FrontAppShell, StandardFrontLocationPicker as FrontLocationPicker } from '@erpx/front/components/standard-front-app-shell';
import type { StandardPmsToday as PmsToday } from '@erpx/feature-pms/components/standard-pms-today';
import type { PosConsole } from '@erpx/feature-pos/components/pos-console';

// Owner and HTTP request adapters are controlled. React, Account preference
// projection, the specialized page bodies, Front shell, contexts and widgets run.
const owner = vi.hoisted(() => ({
  app: 'pms' as 'pms' | 'pos', path: '', query: '', cookies: new Map<string, string>(),
  session: vi.fn(), profile: vi.fn(), company: vi.fn(), scope: vi.fn(), role: vi.fn(), work: vi.fn(),
  home: vi.fn(), pos: vi.fn(), pms: vi.fn(), catalogue: vi.fn(), money: vi.fn(), quick: vi.fn(),
  push: vi.fn(), replace: vi.fn(), refresh: vi.fn(),
  actions: vi.fn(), frontHref: vi.fn(),
  shells: [] as ComponentProps<typeof FrontAppShell>[],
  pickers: [] as ComponentProps<typeof FrontLocationPicker>[],
  homes: [] as ComponentProps<typeof PmsToday>[],
  consoles: [] as ComponentProps<typeof PosConsole>[],
  sourceDetail: null as import("@erpx/server-pms/public/pms").PmsWorkspace|null,
  sourceId: '',
}));
vi.mock('server-only', () => ({}));
vi.mock('next/headers', () => ({
  headers: async () => new Headers({ host: `${owner.app}-dev-erpx.jesadakorn.com` }),
  cookies: async () => ({ get: (name: string) => owner.cookies.has(name) ? { name, value: owner.cookies.get(name) } : undefined }),
}));
vi.mock('next/navigation', async importOriginal => ({
  ...await importOriginal<typeof import('next/navigation')>(),
  usePathname: () => owner.path,
  useSearchParams: () => new URLSearchParams(owner.query),
  useRouter: () => ({ push: owner.push, replace: owner.replace, refresh: owner.refresh }),
}));
vi.mock("@erpx/server-identity_access/public/authorization", async () => ({
  currentSession: owner.session, requireWorkContext: owner.work, requestedHoldingScope:async(holdingSlug:string)=>({kind:'holding',holdingSlug}),
  publicScope: (await import('@erpx/server-identity_access/domain/authorization')).publicScope,
}));
vi.mock("@erpx/server-identity_access/adapters/authorization", () => ({ currentSession: owner.session }));
vi.mock("@erpx/server-identity_access/adapters/account-profile", () => ({ currentAccountProfile: owner.profile }));
vi.mock("@erpx/server-work_projection/public/front-app", async () => ({
  ...await import('@erpx/server-work_projection/domain/front-app'),
  frontCompanyApp: owner.company, frontCompanyBrand: owner.company,
  frontScopeFromLocation: owner.scope, ensureFrontRole: owner.role, frontWorkspaceHref: owner.frontHref,
  ensureFrontRoleSnapshot: async (...args: [string, string, 'pos' | 'pms', string]) => {
    const checkedAt = Date.now();
    return {...await owner.role(...args), checkedAt};
  },
}));
vi.mock("@erpx/server-work_projection/public/front-quick-access", () => ({ frontActionItems: owner.actions }));
vi.mock("@erpx/server-work_projection/public/quick-access-preferences", () => ({ readQuickAccessPreferences: owner.quick }));
vi.mock("@erpx/server-pms/public/pms/home", () => ({ pmsHome: owner.home }));
vi.mock("@erpx/server-pms/public/pms", () => ({ pmsWorkspace: owner.pms }));
vi.mock("@erpx/server-pms/public/hospitality-rollout", () => ({ companyFeatureCatalogue: owner.catalogue }));
vi.mock("@erpx/server-pos/public/pos", () => ({ posWorkspace: owner.pos }));
vi.mock("@erpx/server-pos/public/movement-presentation-query", () => ({ posMovementSaleForPresentation: vi.fn() }));
vi.mock("@erpx/server-finance/public/company-money", () => ({ moneyContext: owner.money }));
vi.mock("@erpx/front/components/standard-front-app-shell", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/front/components/standard-front-app-shell")>();
  return { ...actual,
    StandardFrontAppShell: (props: ComponentProps<typeof actual.StandardFrontAppShell>) => { owner.shells.push(props); return createElement(actual.StandardFrontAppShell, props); },
    StandardFrontLocationPicker: (props: ComponentProps<typeof actual.StandardFrontLocationPicker>) => { owner.pickers.push(props); return createElement(actual.StandardFrontLocationPicker, props); },
  };
});
vi.mock("@erpx/feature-pms/components/standard-pms-today", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/feature-pms/components/standard-pms-today")>();
  return { ...actual, StandardPmsToday: (props: ComponentProps<typeof actual.StandardPmsToday>) => { owner.homes.push(props); return createElement(actual.StandardPmsToday, props); } };
});
vi.mock("@erpx/feature-pos/components/pos-console", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/feature-pos/components/pos-console")>();
  return { ...actual, PosConsole: (props: ComponentProps<typeof actual.PosConsole>) => { owner.consoles.push(props); return createElement(actual.PosConsole, props); } };
});
vi.mock("@erpx/feature-pos/components/standard-pos-front-console", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/feature-pos/components/standard-pos-front-console")>();
  return { ...actual, StandardFrontPosConsole: (props: ComponentProps<typeof actual.StandardFrontPosConsole>) => { owner.consoles.push(props); return createElement(actual.StandardFrontPosConsole, props); } };
});
vi.mock("@erpx/feature-records/components/record-collection", async importOriginal => {
  const actual=await importOriginal<typeof import("@erpx/feature-records/components/record-collection")>();
  return {...actual,RecordCollectionDrawer:function LoadedOwnerDrawer<T>(props:Parameters<typeof actual.RecordCollectionDrawer<T>>[0]){
    if(owner.sourceDetail&&props.kind==='pms_hotel_record'){
      // This exact drawer's original owner response is supplied, not a caller
      // authority grant. Original selection/render callbacks and React run.
      const snapshot=owner.sourceDetail as T,record=props.select(snapshot,owner.sourceId);
      if(!record)throw Error('The original owner snapshot must contain the selected source.');
      return props.render?.(record,snapshot)??null;
    }
    return createElement(actual.RecordCollectionDrawer<T>,props);
  }};
});

import PmsTodayScreen from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pms-daily-entry';
import PosSaleScreen from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pos-entry';
import PosHistoryScreen from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pos-entry';
import FrontEntryScreen from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-front-base-entry';
import { ProductLinkEnvironment } from '@erpx/workplace/components/product-link-context';
import { ContextSwitchProvider } from '@erpx/workplace/components/context-switch-provider';

const actor: WorkContext = {
  userId: 'current-account', name: 'Current operator', email: 'operator@example.test', assignmentId: 'current-assignment',
  role: 'holding_owner', companyId: 'original-company', branchId: null, departmentId: null, holdingId: 'isolated',
  holdingSlug: 'fictional-holding', holdingName: 'Fictional Holding', assignmentScope: 'holding', inherited: false,
  scope: { kind: 'holding', holdingId: 'isolated', holdingSlug: 'fictional-holding', holdingName: 'Fictional Holding', companyId: 'original-company' },
  modules: ['pms', 'pos'], assignments: [], workspaces: [], authority: { sessionId: 'current-session', revision: 7, assignmentVersion: 2 },
};
const company: FrontCompanyApp = {
  holdingId: actor.holdingId, holdingSlug: actor.holdingSlug, companyId: actor.companyId, companySlug: 'original-company',
  companyName: 'Original Company', shortName: 'Original', accentColor: '#345678', logoAttachmentId: null, logoPublicationId: null,
  userId: actor.userId, locations: [{ key: 'original-company', label: 'Original Company location', path: 'holding' }],
};
const money: CurrencyContextData = { companyId: actor.companyId, defaultCurrency: 'THB', displayCurrency: 'THB', scales: { THB: 2 }, rates: [], currencies: [], canConfirmRate: false };
const home: PmsHome = {
  updatedAt: Date.parse('2026-10-16T08:00:00Z'), hotels: [{ id: 'original-hotel', name: 'Original seeded hotel', branchId: 'original-branch', businessDate: '2026-10-16', state: 'open', currency: 'THB', capacity: 10, sold: 2, inspected: 8, roomCount: 10, arrivals: 1, departures: 0, occupancy: 0.2, roomRevenue: null, adr: null, revpar: null, receivable: null, advance: null, deposit: null }],
  tasks: { items: [], nextCursor: null, previousCursor: null, hasMore: false }, counts: [{ label: 'arrivals', count: 1, urgent: 0 }],
  posts: [], readablePostIds: [], canFinance: false, feedHasMore: false, people: [], userId: actor.userId, view: 'team',
};
const pos: PosWorkspace = {
  hotelLinks: [], currency: 'THB', salesPage: null, permissions: { 'pos.read': true, 'pos.sell': true, 'pos.kitchen': false, 'pos.approve': false },
  userId: actor.userId, settings: { enabled: true, currency: 'THB', timezone: 'Asia/Bangkok' }, commerce: null,
  availableTenderMethods: ['cash'], roomChargeEnabled: false, registers: [{ id: 'original-register', name: 'Original seeded register', active: true }],
  products: [], warehouses: [], shifts: [], tables: [], sales: [{ id: 'original-receipt', number: 'ORIGINAL-RECEIPT-1', state: 'completed', currency: 'THB', total_minor: 10000, business_date: '2026-10-16' }],
  sale: null, taxDocumentInvoiceId: null, lines: [], payments: [], transferEvidence: [], roomCharge: null, roomChargeStays: [], kitchen: [], refunds: [], cashMovements: [], stock: [], jobs: [], reports: [], paymentMix: [], companyName: company.companyName, bankTransactions: [], customers: [], receipt: null,
  companyId: actor.companyId, branchId: null, branches: [], dayCloses: [], accounts: [], charts: [], bankAccounts: [], settlementConnection: null, connection: null, settlements: [],
  frontSetup: { hasProducts: true, blockedRegisters: [] }, frontReady: true,
};
const fetcher = vi.fn<(url: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () => { throw Error('SSR must use its original owner seed, without HTTP reads or commands.'); });
beforeEach(() => {
  vi.clearAllMocks(); owner.cookies.clear(); owner.shells.length = 0; owner.pickers.length = 0; owner.homes.length = 0; owner.consoles.length = 0;
  owner.sourceDetail=null;owner.sourceId='';
  owner.actions.mockReturnValue([]);
  owner.query = ''; owner.cookies.set(`erpx_front_language_${actor.userId}`, 'en'); owner.cookies.set('erpx_appearance', 'light');
  owner.session.mockResolvedValue({ session: { user: { id: actor.userId } } });
  owner.profile.mockResolvedValue({ profile: { preferences: { language: 'th', appearance: 'dark', version: 3 } } });
  owner.company.mockResolvedValue(company); owner.scope.mockResolvedValue({ kind: 'holding', holdingSlug: actor.holdingSlug });
  owner.role.mockResolvedValue({ options: [{ id: actor.assignmentId, role: actor.role, label: 'Original role' }] });
  owner.work.mockResolvedValue(actor); owner.home.mockResolvedValue(home); owner.pos.mockResolvedValue(pos); owner.money.mockResolvedValue(money);
  owner.quick.mockResolvedValue({ pinnedIds: [], version: 0 });
  vi.stubGlobal('fetch', fetcher);
});
afterEach(() => vi.unstubAllGlobals());
type Screen = 'today' | 'sale' | 'history';
async function renderScreen(screen: Screen, picker = false) {
  owner.app = screen === 'today' ? 'pms' : 'pos';
  owner.path = `/${actor.holdingSlug}/${company.companySlug}${picker ? '' : `/holding/${screen}`}`;
  owner.query = picker ? 'locations=1' : '';
  const component = screen === 'today' ? PmsTodayScreen : screen === 'sale' ? PosSaleScreen : PosHistoryScreen;
  const node = await component({ params: Promise.resolve({ holdingSlug: actor.holdingSlug, companySlug: company.companySlug, app: owner.app, path: picker ? [] : ['holding', screen] }), searchParams: Promise.resolve(picker ? { locations: '1' } : {}) });
  // Native unit resolution uses Next's normal Loadable wrapper. Perform the
  // original server preload phase so real deferred widget bodies render too.
  await Loadable.preloadAll();
  const context: ComponentProps<typeof ContextSwitchProvider> = { children: node };
  const product: ComponentProps<typeof ProductLinkEnvironment> = { audience: owner.app, children: createElement(ContextSwitchProvider, context) };
  const stream = await renderToReadableStream(createElement(ProductLinkEnvironment, product));
  await stream.allReady;
  return new Response(stream).text();
}
function assertPreference(language: 'en' | 'th', appearance: 'light' | 'dark', html: string) {
  expect(owner.shells).toHaveLength(1);
  expect(owner.shells[0]).toMatchObject({ initialLanguage: language, initialTheme: appearance, companyId: actor.companyId, userId: actor.userId, standalone: true });
  expect(html).toContain(language === 'th' ? 'ข้ามไปยังเนื้อหา' : 'Skip to page content');
  expect(html).toContain(`<option value="${appearance}" selected=""`);
  expect(fetcher).not.toHaveBeenCalled(); expect(owner.push).not.toHaveBeenCalled(); expect(owner.replace).not.toHaveBeenCalled();
}
it.each(['today', 'sale', 'history'] as const)('renders actual %s caller/widgets with current Account language/theme over stale Front cookies', async screen => {
  const html = await renderScreen(screen); assertPreference('th', 'dark', html);
  expect(owner.company.mock.calls).toEqual([[actor.holdingSlug, company.companySlug, owner.app]]);
  expect(owner.work.mock.calls).toEqual([[{ kind: 'holding', holdingSlug: actor.holdingSlug }]]);
  if (screen === 'today') {
    expect(owner.home.mock.calls).toEqual([[actor]]); expect(owner.homes).toHaveLength(1); expect(owner.homes[0].initial).toBe(home);
    expect(html).toContain('Original seeded hotel'); expect(html).toContain('วันนี้ งานเดินหน้าไปด้วยกัน'); expect(owner.pos).not.toHaveBeenCalled();
  } else {
    expect(owner.pos.mock.calls).toEqual([[actor, undefined, '', screen, undefined, true]]);
    expect(owner.consoles).toHaveLength(1); expect(owner.consoles[0]).toMatchObject({ initial: pos, initialTab: screen, frontMode: true, commandScopeKey: 'holding', initialRead: { q: '', tab: screen } });
    expect(html).toContain(screen === 'history' ? 'ORIGINAL-RECEIPT-1' : 'Original seeded register'); expect(owner.home).not.toHaveBeenCalled();
  }
  expect(html).not.toContain('Loading the point of sale'); expect(owner.pms).not.toHaveBeenCalled(); expect(owner.catalogue).not.toHaveBeenCalled();
});
it.each(['today', 'sale', 'history'] as const)('retains the original %s Front cookie fallback when Account preferences have never been saved', async screen => {
  owner.profile.mockResolvedValue({ profile: { preferences: { language: 'th', appearance: 'dark', version: 0 } } });
  assertPreference('en', 'light', await renderScreen(screen));
});
it.each(['today', 'sale'] as const)('renders the actual %s location picker from current scoped locations with Account preferences and no business reader', async screen => {
  const html = await renderScreen(screen, true); assertPreference('th', 'dark', html);
  expect(html).toContain('เลือกสาขาเพื่อเริ่มงาน'); expect(html).toContain('ทั้งบริษัท');
  expect(owner.pickers).toHaveLength(1); expect(owner.pickers[0]).toMatchObject({ base: `/${actor.holdingSlug}/${company.companySlug}`, locations: company.locations, standalone: true });
  expect(owner.scope).not.toHaveBeenCalled(); expect(owner.role).not.toHaveBeenCalled(); expect(owner.work).not.toHaveBeenCalled();
  expect(owner.home).not.toHaveBeenCalled(); expect(owner.pos).not.toHaveBeenCalled(); expect(owner.money).not.toHaveBeenCalled(); expect(owner.quick).not.toHaveBeenCalled();
});
it('keeps the actual current-context refusal inside the original location picker without reading PMS business data', async () => {
  owner.work.mockRejectedValueOnce(Object.assign(Error('Original current authority refused'), { status: 403 }));
  const html = await renderScreen('today'); assertPreference('th', 'dark', html);
  expect(html).toContain('สิทธิ์ของคุณเปลี่ยนไปแล้ว'); expect(owner.pickers).toHaveLength(1);
  expect(owner.home).not.toHaveBeenCalled(); expect(owner.pos).not.toHaveBeenCalled(); expect(owner.money).not.toHaveBeenCalled();
});

it('opens an explicit Front location on the current role workflow with original filters instead of a generic route list',async()=>{
  owner.actions.mockImplementation((await import('@erpx/server-work_projection/domain/front-quick-access')).frontActionItems);
  for(const [app,permission,screen] of [['pos','pos.read','history'],['pos','pos.kitchen','kitchen'],['pms','pms.housekeeping','rooms']] as const){
    owner.app=app;owner.path=`/${actor.holdingSlug}/${company.companySlug}/holding`;owner.query='q=open&tag=one&tag=two';
    owner.work.mockResolvedValue({...actor,iam:{permissions:[permission]}});
    let caught:unknown;
    try{await FrontEntryScreen({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app,path:['holding']}),searchParams:Promise.resolve({q:'open',tag:['one','two']})});}catch(error){caught=error;}
    expect(caught).toMatchObject({digest:`NEXT_REDIRECT;replace;/${actor.holdingSlug}/${company.companySlug}/holding/${screen}?q=open&tag=one&tag=two;307;`});
  }
  expect(owner.pos).not.toHaveBeenCalled();expect(owner.home).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
});


function canonicalPmsRedirect(error:unknown){
 expect(error).toHaveProperty('digest');const digest=(error as {digest:string}).digest;expect(digest).toMatch(/^NEXT_REDIRECT;replace;.*;307;$/);return new URL(digest.slice('NEXT_REDIRECT;replace;'.length,-';307;'.length));
}
function originalPmsWorkspace():import("@erpx/server-pms/public/pms").PmsWorkspace{
 return {bookingDelivery:[],transferRequests:[],chargeCorrections:[],stripeConnectionReady:false,stripePayments:[],counterRefunds:[],bookingRefunds:[],transferBankEvidence:[],defaultDate:'2026-10-16',properties:[],snapshot:null,readiness:[],frontReady:false,canReviewBookingMedia:false,bookingReadiness:{email:false,accounting:false,onlineAccounting:false,fees:false,connect:false},canManageSetup:false,permissions:['pms.read','pms.housekeeping','pms.cashier','pms.reservations'],availablePaymentMethods:['cash'],userId:actor.userId,branches:[],staff:[],accounts:[],charts:[],parties:[],products:[],warehouses:[],documents:[],ledger:null,controlBalances:null,dayControls:null};
}
it.each(['legacy','product'] as const)('keeps actual canonical Housekeeping record selection through the %s daily redirect and typed current owner read',async kind=>{
 const {default:CanonicalRecordPage}=await import('@erpx/feature-pms/routes/holdings/[holdingSlug]/pms/housekeeping/records/pms_record/[id]/[recordSection]/page'),{default:FrontHousekeepingPage}=await import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pms-daily-entry'),{StandardPmsFrontWorkspace:PmsWorkspaceView}=await import('@erpx/feature-pms/components/standard-pms-front-workspace'),{isValidElement}=await import('react');
 const origin=kind==='legacy'?'https://dev-pms.jesadakorn.com':'https://pms-dev-erpx.jesadakorn.com',root=`${origin}/${actor.holdingSlug}/${company.companySlug}/holding/`,id='original-task',query={property:'original-hotel',record:'earlier-task',task:'original-task',q:'review',floor:['2','3'],detailKind:'foreign-client-kind',detailId:'wrong-client-id',detailMode:'page',returnTo:`/${actor.holdingSlug}/${company.companySlug}/holding/housekeeping?property=original-hotel`,token:'not-forwarded',return:'view=actions'};
 owner.app='pms';owner.frontHref.mockResolvedValue(root);let caught:unknown;try{await CanonicalRecordPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,id,recordSection:'overview'}),searchParams:Promise.resolve(query)});}catch(error){caught=error;}
 const target=canonicalPmsRedirect(caught);expect(target.origin).toBe(origin);expect(target.pathname).toBe(`/${actor.holdingSlug}/${company.companySlug}/holding/housekeeping`);expect(target.searchParams.get('detailKind')).toBe('pms_record');expect(target.searchParams.get('detailId')).toBe(id);expect(target.searchParams.get('detailMode')).toBe('page');expect(target.searchParams.get('returnTo')).toBe(query.returnTo);expect(target.searchParams.getAll('floor')).toEqual(['2','3']);expect(target.searchParams.get('property')).toBe('original-hotel');expect(target.searchParams.has('token')).toBe(false);expect(target.searchParams.has('return')).toBe(false);expect(owner.work.mock.calls).toEqual([[actor.holdingSlug]]);expect(owner.frontHref.mock.calls).toEqual([[actor,'pms']]);expect(owner.pms).not.toHaveBeenCalled();
 const values:Record<string,string|string[]>={};for(const name of new Set(target.searchParams.keys())){const all=target.searchParams.getAll(name);values[name]=all.length===1?all[0]:all;}
 owner.query=target.searchParams.toString();owner.path=target.pathname;const workspace=originalPmsWorkspace();owner.pms.mockResolvedValue(workspace);const node=await FrontHousekeepingPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app:'pms',path:['holding','housekeeping']}),searchParams:Promise.resolve(values)});
 expect(owner.pms.mock.calls).toEqual([[actor,'original-hotel',false,true,{view:'list',section:'housekeeping',page:0,q:'review',status:'',building:'',floor:'',typeId:'',selectedId:id}]]);
 const found:ComponentProps<typeof PmsWorkspaceView>[]=[];const walk=(value:import('react').ReactNode):void=>{if(Array.isArray(value)){value.forEach(walk);return;}if(!isValidElement<{children?:import('react').ReactNode}>(value))return;if(value.type===PmsWorkspaceView)found.push(value.props as ComponentProps<typeof PmsWorkspaceView>);walk(value.props.children);};walk(node);expect(found).toHaveLength(1);expect(found[0]).toMatchObject({initial:workspace,section:'housekeeping',selectedRecord:id,frontMode:true});expect(owner.role.mock.calls).toEqual([[actor.holdingSlug,actor.companyId,'pms','holding']]);expect(fetcher).not.toHaveBeenCalled();
});
it.each(['legacy','product'] as const)('retains the canonical Guest Account entry drawer identity and current close target on the %s daily redirect',async kind=>{
 const {default:CanonicalEntryPage}=await import('@erpx/feature-pms/routes/holdings/[holdingSlug]/pms/guest-accounts/records/pms_entry/[id]/[recordSection]/page'),origin=kind==='legacy'?'https://dev-pms.jesadakorn.com':'https://pms-dev-erpx.jesadakorn.com',root=`${origin}/${actor.holdingSlug}/${company.companySlug}/holding/`,close=`/${actor.holdingSlug}/${company.companySlug}/holding/guest-accounts?property=original-hotel&guestAccount=original-account`;
 owner.frontHref.mockResolvedValue(root);let caught:unknown;try{await CanonicalEntryPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,id:'original-entry',recordSection:'overview'}),searchParams:Promise.resolve({property:'original-hotel',guestAccount:'original-account',detailMode:'page',returnTo:close})});}catch(error){caught=error;}
 const target=canonicalPmsRedirect(caught);expect(target.pathname).toBe(`/${actor.holdingSlug}/${company.companySlug}/holding/guest-accounts`);expect(target.searchParams.get('detailKind')).toBe('pms_entry');expect(target.searchParams.get('detailId')).toBe('original-entry');expect(target.searchParams.get('detailMode')).toBe('page');expect(target.searchParams.get('returnTo')).toBe(close);expect(target.searchParams.get('guestAccount')).toBe('original-account');expect(owner.pms).not.toHaveBeenCalled();
 const {withoutRecordDrawer}=await import('@erpx/feature-records/lib/record-drawer');expect(withoutRecordDrawer(target.href)).toBe(close);
 for(const foreign of ['https://unrelated.example/holding/guest-accounts',`/${actor.holdingSlug}/foreign-company/holding/guest-accounts`,`/${actor.holdingSlug}/${company.companySlug}/branches/foreign-branch/guest-accounts`,`/${actor.holdingSlug}/${company.companySlug}/holding/api/auth/sign-out`]){let refused:unknown;try{await CanonicalEntryPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,id:'original-entry',recordSection:'overview'}),searchParams:Promise.resolve({property:'original-hotel',returnTo:foreign})});}catch(error){refused=error;}expect(canonicalPmsRedirect(refused).searchParams.has('returnTo')).toBe(false);}
 let sanitized:unknown;try{await CanonicalEntryPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,id:'original-entry',recordSection:'overview'}),searchParams:Promise.resolve({property:'original-hotel',returnTo:close+'&token=not-forwarded'})});}catch(error){sanitized=error;}expect(canonicalPmsRedirect(sanitized).searchParams.get('returnTo')).toBe(close);
});
it('requires the actual canonical PMS work context before generating a daily redirect',async()=>{
 const {default:CanonicalRecordPage}=await import('@erpx/feature-pms/routes/holdings/[holdingSlug]/pms/housekeeping/records/pms_record/[id]/[recordSection]/page'),denied=Object.assign(Error('Original current PMS authority denied'),{status:403});owner.work.mockRejectedValue(denied);owner.frontHref.mockResolvedValue('https://dev-pms.jesadakorn.com/unused/');
 await expect(CanonicalRecordPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,id:'private-task',recordSection:'overview'}),searchParams:Promise.resolve({property:'private-hotel'})})).rejects.toBe(denied);expect(owner.frontHref).not.toHaveBeenCalled();expect(owner.pms).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
});
it.each([['reservations','groups'],['guest-accounts','approval'],['guest-accounts','corrections'],['cashier','approval']] as const)('keeps the existing %s %s review on its original owner presenter',async(section,queue)=>{
 const {default:PmsPage}=await import('@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/pms/standard-entry');owner.frontHref.mockImplementation(async(_actor:WorkContext,app:'pms'|'pos')=>app==='pms'?`https://dev-pms.jesadakorn.com/${actor.holdingSlug}/${company.companySlug}/holding/`:null);owner.pms.mockResolvedValue(originalPmsWorkspace());
 await expect(PmsPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,section:[section]}),searchParams:Promise.resolve({property:'original-hotel',queue})})).resolves.toBeDefined();expect(owner.pms).toHaveBeenCalledWith(actor,'original-hotel',false,false,expect.objectContaining({view:'list',section}));expect(fetcher).not.toHaveBeenCalled();
});

it.each([['room','housekeeping'],['stay','front-desk'],['entry','guest-accounts']] as const)('opens the actual %s metric source in its registered %s owner section with exact current selection',async(kind,section)=>{
 const {PmsHotelSources}=await import('@erpx/feature-records/components/pms-hotel-sources'),{newProperty}=await import('@erpx/feature-pms/lib/pms-engine'),{frontProductLaunchTarget}=await import('@erpx/feature-work_projection/lib/product-navigation'),{StandardPmsFrontWorkspace:PmsWorkspaceView}=await import('@erpx/feature-pms/components/standard-pms-front-workspace'),{isValidElement}=await import('react');
 const rows:{room:import("@erpx/server-pms/public/rules/pms-model").PmsRecord<'room'>;stay:import("@erpx/server-pms/public/rules/pms-model").PmsRecord<'stay'>;entry:import("@erpx/server-pms/public/rules/pms-model").PmsRecord<'entry'>}={
  room:{id:'original-room',kind:'room',version:3,updatedAt:100,data:{number:'101',typeId:'original-type',building:'A',floor:'1',active:true,cleanliness:'inspected',dnd:false,observedOccupied:null}},
  stay:{id:'original-stay',kind:'stay',version:4,updatedAt:100,data:{reservationId:'original-reservation',guestId:'original-guest',typeId:'original-type',rateId:'original-rate',roomId:'original-room',arrival:'2026-10-15',departure:'2026-10-17',adults:1,state:'in_house',guestAccountId:'original-guest-account',prices:[],segments:[]}},
  entry:{id:'original-entry',kind:'entry',version:5,updatedAt:100,data:{guestAccountId:'original-guest-account',stayId:'original-stay',date:'2026-10-16',kind:'charge',description:'Original room night',net:10000,service:0,tax:700,total:10700,method:'night',reference:'Original night',sourceKey:'night:original-night',originalId:'',shiftId:'',actorId:actor.userId,at:100}},
 },row=rows[kind],snapshot=newProperty('original-hotel',actor.companyId,'original-branch','Original source hotel','2026-10-16');
 snapshot.records.push(row);const workspace=originalPmsWorkspace();workspace.snapshot=snapshot;workspace.permissions.push('pms.frontdesk');
 owner.sourceDetail=workspace;owner.sourceId=row.id;owner.path=`/holdings/${actor.holdingSlug}/pms/dashboard/records/pms_hotel_record/${row.id}/overview`;owner.query='property=original-hotel';
 const stream=await renderToReadableStream(createElement(PmsHotelSources,{api:`/api/v1/holdings/${actor.holdingSlug}/pms-home`,frontHref:`/${actor.holdingSlug}/${company.companySlug}/holding/`}));await stream.allReady;const html=await new Response(stream).text(),match=/href="([^"]+)"[^>]*>Open full hotel record<\/a>/.exec(html);
 expect(match,'The actual authorized source drawer must render its full-record link.').not.toBeNull();const url=new URL(match![1].replaceAll('&amp;','&'),'https://pms-dev-erpx.jesadakorn.com'),target=frontProductLaunchTarget('pms',url.pathname+url.search);
 expect(target,'The real rendered link must be a closed registered Front destination.').not.toBeNull();expect(url.pathname).toBe(`/${actor.holdingSlug}/${company.companySlug}/holding/${section}`);expect(url.searchParams.get('property')).toBe(snapshot.property.id);expect(url.searchParams.get('record')).toBe(row.id);
 expect(url.searchParams.get('guestAccount')).toBe(kind==='room'?null:'original-guest-account');
 owner.sourceDetail=null;owner.path=url.pathname;owner.query=url.searchParams.toString();owner.pms.mockResolvedValue(workspace);
 const {default:FrontOwnerPage}=await import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pms-daily-entry');
 const node=await FrontOwnerPage({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app:'pms',path:['holding',section]}),searchParams:Promise.resolve(Object.fromEntries(url.searchParams))});
 expect(owner.pms.mock.calls).toEqual([[actor,snapshot.property.id,false,true,{view:'list',section,page:0,q:'',status:'',building:'',floor:'',typeId:'',selectedId:row.id}]]);
 const views:ComponentProps<typeof PmsWorkspaceView>[]=[];const walk=(value:import('react').ReactNode):void=>{if(Array.isArray(value)){value.forEach(walk);return;}if(!isValidElement<{children?:import('react').ReactNode}>(value))return;if(value.type===PmsWorkspaceView)views.push(value.props as ComponentProps<typeof PmsWorkspaceView>);walk(value.props.children);};walk(node);
 expect(views).toHaveLength(1);expect(views[0]).toMatchObject({initial:workspace,section,selectedRecord:row.id,frontMode:true,...(kind==='room'?{}:{selectedGuestAccount:'original-guest-account'})});expect(owner.role).toHaveBeenCalledExactlyOnceWith(actor.holdingSlug,actor.companyId,'pms','holding');expect(fetcher).not.toHaveBeenCalled();
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


import StandardPmsDailyEntry from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-pms-daily-entry';
const nativePmsDailyRecordRoutes=[
 ['reservations',()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/reservations/records/[recordKind]/[id]/[recordSection]/page')],
 ['front-desk',()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/front-desk/records/[recordKind]/[id]/[recordSection]/page')],
 ['rooms',()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/rooms/records/[recordKind]/[id]/[recordSection]/page')],
 ['housekeeping',()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/housekeeping/records/[recordKind]/[id]/[recordSection]/page')],
 ['guest-accounts',()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/guest-accounts/records/[recordKind]/[id]/[recordSection]/page')],
 ['cashier',()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/cashier/records/[recordKind]/[id]/[recordSection]/page')],
] as const;
it.each(nativePmsDailyRecordRoutes)('retains the actual %s record route, native selection and current owner read through the common PMS entry',async(section,load)=>{
 const {default:Page}=await load(),{StandardPmsFrontWorkspace:PmsWorkspaceView}=await import('@erpx/feature-pms/components/standard-pms-front-workspace'),{isValidElement}=await import('react');
 const id='selected-native-source',query={property:'original-hotel',record:'superseded-source',guestAccount:'original-guest-account',queue:'arrivals',q:'review',page:'2',floor:'3',detailMode:'page',returnTo:'/'+actor.holdingSlug+'/'+company.companySlug+'/holding/'+section};
 owner.app='pms';owner.path='/'+actor.holdingSlug+'/'+company.companySlug+'/holding/'+section+'/records/pms_record/'+id+'/overview';owner.query=new URLSearchParams(query).toString();
 const workspace=originalPmsWorkspace();workspace.permissions.push('pms.frontdesk');owner.pms.mockResolvedValue(workspace);
 const routed=await Page({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app:'pms',recordKind:'pms_record',id,recordSection:'overview'}),searchParams:Promise.resolve(query)});
 if(!isValidElement<ComponentProps<typeof StandardPmsDailyEntry>>(routed))throw Error('The actual route must return its native server entry.');
 expect(routed.type).toBe(StandardPmsDailyEntry);
 const node=await StandardPmsDailyEntry(routed.props);
 expect(owner.pms.mock.calls).toEqual([[actor,'original-hotel',false,true,{view:'list',section,page:2,q:'review',status:section==='front-desk'?'arrivals':'',building:'',floor:'3',typeId:'',selectedId:id}]]);
 expect(owner.role.mock.calls).toEqual([[actor.holdingSlug,actor.companyId,'pms','holding']]);expect(owner.work.mock.calls).toEqual([[{kind:'holding',holdingSlug:actor.holdingSlug}]]);
 const views:ComponentProps<typeof PmsWorkspaceView>[]=[];const walk=(value:import('react').ReactNode):void=>{if(Array.isArray(value)){value.forEach(walk);return;}if(!isValidElement<{children?:import('react').ReactNode}>(value))return;if(value.type===PmsWorkspaceView)views.push(value.props as ComponentProps<typeof PmsWorkspaceView>);walk(value.props.children);};walk(node);
 expect(views).toHaveLength(1);expect(views[0]).toMatchObject({initial:workspace,section,selectedRecord:id,selectedGuestAccount:'original-guest-account',initialQueue:'arrivals',frontMode:true});
 expect(owner.home).not.toHaveBeenCalled();expect(owner.pos).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
});


it('retains the actual Today property dashboard reader without substituting the home projection or financial mode',async()=>{
 const {default:Page}=await import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/today/page'),{StandardPmsFrontWorkspace:PmsWorkspaceView}=await import('@erpx/feature-pms/components/standard-pms-front-workspace'),{isValidElement}=await import('react');
 const query={property:'original-hotel',record:'dashboard-native-source',q:'retained-filter'};
 owner.app='pms';owner.path='/'+actor.holdingSlug+'/'+company.companySlug+'/holding/today';owner.query=new URLSearchParams(query).toString();
 const workspace=originalPmsWorkspace();owner.pms.mockResolvedValue(workspace);
 const routed=await Page({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app:'pms'}),searchParams:Promise.resolve(query)});
 if(!isValidElement<ComponentProps<typeof StandardPmsDailyEntry>>(routed))throw Error('The actual Today route must return its native server entry.');
 expect(routed.type).toBe(StandardPmsDailyEntry);
 const node=await StandardPmsDailyEntry(routed.props);
 expect(owner.pms.mock.calls).toEqual([[actor,'original-hotel',false,true,{view:'dashboard'}]]);expect(owner.home).not.toHaveBeenCalled();
 const views:ComponentProps<typeof PmsWorkspaceView>[]=[];const walk=(value:import('react').ReactNode):void=>{if(Array.isArray(value)){value.forEach(walk);return;}if(!isValidElement<{children?:import('react').ReactNode}>(value))return;if(value.type===PmsWorkspaceView)views.push(value.props as ComponentProps<typeof PmsWorkspaceView>);walk(value.props.children);};walk(node);
 expect(views).toHaveLength(1);expect(views[0]).toMatchObject({initial:workspace,section:'dashboard',selectedRecord:'dashboard-native-source',frontMode:true});
 expect(owner.role).toHaveBeenCalledExactlyOnceWith(actor.holdingSlug,actor.companyId,'pms','holding');expect(fetcher).not.toHaveBeenCalled();
});
it('retains the actual Night audit record reader financial mode and exact native selection instead of the Today home branch',async()=>{
 const {default:Page}=await import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/night-audit/records/[recordKind]/[id]/[recordSection]/page'),{StandardPmsFrontWorkspace:PmsWorkspaceView}=await import('@erpx/feature-pms/components/standard-pms-front-workspace'),{isValidElement}=await import('react');
 const id='original-night-source',query={property:'original-hotel',night:'superseded-night',page:'1',status:'open',q:'review'};
 owner.app='pms';owner.path='/'+actor.holdingSlug+'/'+company.companySlug+'/holding/night-audit/records/pms_record/'+id+'/overview';owner.query=new URLSearchParams(query).toString();
 const workspace=originalPmsWorkspace();workspace.permissions.push('pms.audit');owner.pms.mockResolvedValue(workspace);
 const routed=await Page({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app:'pms',recordKind:'pms_record',id,recordSection:'overview'}),searchParams:Promise.resolve(query)});
 if(!isValidElement<ComponentProps<typeof StandardPmsDailyEntry>>(routed))throw Error('The actual Night audit route must return its native server entry.');
 expect(routed.type).toBe(StandardPmsDailyEntry);
 const node=await StandardPmsDailyEntry(routed.props);
 expect(owner.pms.mock.calls).toEqual([[actor,'original-hotel',true,true,{view:'list',section:'night-audit',page:1,q:'review',status:'open',building:'',floor:'',typeId:'',selectedId:id}]]);expect(owner.home).not.toHaveBeenCalled();
 const views:ComponentProps<typeof PmsWorkspaceView>[]=[];const walk=(value:import('react').ReactNode):void=>{if(Array.isArray(value)){value.forEach(walk);return;}if(!isValidElement<{children?:import('react').ReactNode}>(value))return;if(value.type===PmsWorkspaceView)views.push(value.props as ComponentProps<typeof PmsWorkspaceView>);walk(value.props.children);};walk(node);
 expect(views).toHaveLength(1);expect(views[0]).toMatchObject({initial:workspace,section:'night-audit',selectedRecord:id,frontMode:true});
 expect(owner.role).toHaveBeenCalledExactlyOnceWith(actor.holdingSlug,actor.companyId,'pms','holding');expect(fetcher).not.toHaveBeenCalled();
});


it('standard PMS navigation preserves native permission destinations, selected Property and current section without an HTTP read',async()=>{
 const {StandardPmsNavigation}=await import('@erpx/feature-pms/components/standard-pms-navigation');
 const stream=await renderToReadableStream(createElement(StandardPmsNavigation,{href:'/fictional-holding/original-company/branches/original-branch/',section:'housekeeping',permissions:['pms.read','pms.inspect'],propertyId:'original hotel / ก'}));await stream.allReady;const html=await new Response(stream).text();
 expect(html).toContain('aria-label="Hotel tasks"');expect(html).toContain('/fictional-holding/original-company/branches/original-branch/today?property=original%20hotel%20%2F%20%E0%B8%81');expect(html).toContain('/fictional-holding/original-company/branches/original-branch/housekeeping?property=original%20hotel%20%2F%20%E0%B8%81');expect(html).toContain('aria-current="page"');expect(html).not.toContain('/cashier?');expect(html).not.toContain('/reservations?');expect(html).not.toContain('/settings?');expect(html).not.toContain('/inventory?');expect(fetcher).not.toHaveBeenCalled();
});
it('maps the native PMS inventory identity to its mounted minibar route',async()=>{
 const {pmsSectionPath}=await import('@erpx/feature-pms/lib/section-route');
 expect(pmsSectionPath('inventory')).toBe('minibar');expect(pmsSectionPath('settings')).toBe('settings');
});
it('actual standard PMS parent renders original empty-owner readiness with standard controls and no mount-time HTTP duplication',async()=>{
 const {StandardPmsFrontWorkspace}=await import('@erpx/feature-pms/components/standard-pms-front-workspace'),{CurrencyProvider}=await import('@erpx/feature-finance/components/money/currency-provider');
 owner.path='/fictional-holding/original-company/holding/rooms';owner.query='';
 const workspace=originalPmsWorkspace();
 const stream=await renderToReadableStream(<CurrencyProvider initial={money} apiBase="/api/v1/front/fictional-holding/original-company/pms"><StandardPmsFrontWorkspace initial={workspace} base="/front/fictional-holding/original-company" erpBase="/holdings/fictional-holding" section="rooms" frontMode={true} crossLinks={{inventory:false,finance:false,pos:false}}/></CurrencyProvider>);await stream.allReady;const html=await new Response(stream).text();
 expect(html).toContain('data-front-daily="pms"');expect(html).toContain('ui-page-header');expect(html).toContain('data-appearance="standard"');expect(html).toContain('Not ready for daily work');expect(html).toContain('No hotel has been set up for this company yet.');expect(html).toContain('aria-label="Hotel"');expect(fetcher).not.toHaveBeenCalled();expect(owner.pms).not.toHaveBeenCalled();
});


it.each(['today','sale'] as const)('actual %s location entry renders standard current-authorized choices with the normal shell and no duplicate read',async screen=>{
 owner.profile.mockResolvedValue({profile:{preferences:{language:'en',appearance:'light',version:4}}});
 const html=await renderScreen(screen,true);
 expect(owner.pickers).toHaveLength(1);expect(owner.pickers[0]).toMatchObject({companyId:actor.companyId,userId:actor.userId,locations:company.locations});
 expect(html).toContain('Choose a location to start');expect(html).toContain('Company-wide');expect(html).toContain('data-appearance="standard"');expect(html).toContain('ui-page-header');expect(html).toContain('Only locations your account can access are listed.');expect(html).toContain('data-front-app="'+(screen==='today'?'pms':'pos')+'"');expect(fetcher).not.toHaveBeenCalled();
});


import StandardFrontNotificationsEntry from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-front-notifications-entry';
const actualNotificationRoutes=[
 [[],()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/notifications/page')],
 [['holding'],()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/holding/notifications/page')],
 [['branches','original-branch'],()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/notifications/page')],
 [['departments','original-department'],()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/departments/[departmentSlug]/notifications/page')],
 [['branches','original-branch','departments','original-department'],()=>import('@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/branches/[branchSlug]/departments/[departmentSlug]/notifications/page')],
] as const;
it.each((['pos','pms'] as const).flatMap(app=>actualNotificationRoutes.map(([path,load])=>[app,path,load] as const)))('actual %s notification route retains its native %j path and exact query at the common owner',async(app,path,load)=>{
 const {default:Page}=await load(),{isValidElement}=await import('react'),query={q:'review',tag:['one','two'],token:'fixture-only-original-token'};
 const node=await Page({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app,branchSlug:'original-branch',departmentSlug:'original-department'}),searchParams:Promise.resolve(query)});
 if(!isValidElement<ComponentProps<typeof StandardFrontNotificationsEntry>>(node))throw Error('The actual notification route must retain its native owner element.');
 expect(node.type).toBe(StandardFrontNotificationsEntry);expect(await node.props.params).toMatchObject({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app,path:[...path,'notifications']});expect(await node.props.searchParams).toBe(query);expect(owner.session).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
});
it.each(['pos','pms'] as const)('actual %s notification owner preserves current role and original Push owner props without exposing query or starting consent/HTTP work in SSR',async app=>{
 const {PushSettings}=await import('@erpx/feature-platform_administration/components/settings/push-settings'),{isValidElement}=await import('react');
 owner.app=app;owner.path='/'+actor.holdingSlug+'/'+company.companySlug+'/holding/notifications';owner.query='q=review&token=fixture-only-original-token';
 owner.profile.mockResolvedValue({profile:{preferences:{language:'en',appearance:'light',version:4}}});
 const node=await StandardFrontNotificationsEntry({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app,path:['holding','notifications']}),searchParams:Promise.resolve({q:'review',token:'fixture-only-original-token'})});
 const widgets:ComponentProps<typeof PushSettings>[]=[];const walk=(value:import('react').ReactNode):void=>{if(Array.isArray(value)){value.forEach(walk);return;}if(!isValidElement<{children?:import('react').ReactNode}>(value))return;if(value.type===PushSettings)widgets.push(value.props as ComponentProps<typeof PushSettings>);walk(value.props.children);};walk(node);
 expect(widgets).toEqual([{appearance:'standard',base:'/holdings/'+actor.holdingSlug,frontCompanySlug:company.companySlug}]);expect(owner.role).toHaveBeenCalledExactlyOnceWith(actor.holdingSlug,actor.companyId,app,'holding');expect(owner.work).toHaveBeenCalledExactlyOnceWith({kind:'holding',holdingSlug:actor.holdingSlug});
 const stream=await renderToReadableStream(<ProductLinkEnvironment audience={app}><ContextSwitchProvider>{node}</ContextSwitchProvider></ProductLinkEnvironment>);await stream.allReady;const html=await new Response(stream).text();
 expect(html).toContain('Notification settings');expect(html).toContain('Loading push settings');expect(html).toContain('data-ui-appearance="standard"');expect(owner.pms).not.toHaveBeenCalled();expect(owner.home).not.toHaveBeenCalled();expect(owner.pos).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
});
it.each(['pos','pms'] as const)('actual %s notification login handoff preserves original repeated query and opaque fixture token without reading current owner data',async app=>{
 owner.app=app;owner.session.mockRejectedValueOnce(Error('UNAUTHENTICATED'));
 let failure:unknown;try{await StandardFrontNotificationsEntry({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app,path:['branches','original-branch','notifications']}),searchParams:Promise.resolve({tag:['one','two'],token:'fixture-only-original-token'})});}catch(error){failure=error;}
 expect(failure).toHaveProperty('digest');const digest=(failure as {digest:string}).digest;expect(digest).toMatch(/^NEXT_REDIRECT;replace;\/login\?next=/);
 const login=new URL(digest.slice('NEXT_REDIRECT;replace;'.length,-';307;'.length),'https://'+app+'-dev-erpx.jesadakorn.com'),next=new URL(login.searchParams.get('next')!,'https://'+app+'-dev-erpx.jesadakorn.com');
 expect(next.pathname).toBe('/'+actor.holdingSlug+'/'+company.companySlug+'/branches/original-branch/notifications');expect(next.searchParams.getAll('tag')).toEqual(['one','two']);expect(next.searchParams.get('token')).toBe('fixture-only-original-token');expect(owner.role).not.toHaveBeenCalled();expect(owner.work).not.toHaveBeenCalled();expect(fetcher).not.toHaveBeenCalled();
});

const actualBackofficeRoutes=[
 ['approvals','guest-accounts',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/approvals/_entry')],
 ['cashier','cashier',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/cashier/_entry')],
 ['dashboard','dashboard',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/dashboard/_entry')],
 ['document section','reports',()=>import('@erpx/feature-pms/routes/holdings/[holdingSlug]/pms/documents/[id]/[recordSection]/page')],
 ['document','reports',()=>import('@erpx/feature-pms/routes/holdings/[holdingSlug]/pms/documents/[id]/page')],
 ['front-desk','front-desk',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/front-desk/_entry')],
 ['guest-accounts','guest-accounts',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/guest-accounts/_entry')],
 ['guests','guests',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/guests/_entry')],
 ['housekeeping','housekeeping',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/housekeeping/_entry')],
 ['minibar','inventory',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/minibar/_entry')],
 ['night-audit','night-audit',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/night-audit/_entry')],
 ['reports','reports',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/reports/_entry')],
 ['reservations','reservations',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/reservations/_entry')],
 ['rooms','rooms',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/rooms/_entry')],
 ['settings','settings',()=>import('@erpx/workplace/routes/holdings/[holdingSlug]/pms/settings/_entry')],
] as const;
it.each(actualBackofficeRoutes)('actual canonical PMS %s entry uses its original native %s reader and real standard Backoffice owner',async(label,section,load)=>{
 const {default:Page}=await load(),{StandardPmsBackofficeWorkspace}=await import('@erpx/feature-pms/components/standard-pms-backoffice-workspace'),{isValidElement}=await import('react');
 owner.frontHref.mockResolvedValue(null);const workspace=originalPmsWorkspace();owner.pms.mockResolvedValue(workspace);
 const node=await Page({params:Promise.resolve({holdingSlug:actor.holdingSlug,id:'original-document',recordSection:'overview'}),searchParams:Promise.resolve({property:'original-hotel',guestAccount:'original-account',q:'Original guest',from:'2026-10-01',to:'2026-10-31',page:'2',status:'open',building:'Original building',floor:'2',typeId:'original-type',detailKind:'pms_record',detailId:'original-record'})});
 if(!isValidElement<ComponentProps<typeof StandardPmsBackofficeWorkspace>>(node))throw Error('The actual canonical route must render its real Backoffice owner.');
 expect(node.type).toBe(StandardPmsBackofficeWorkspace);expect(node.key).toBe(`${actor.companyId}:${actor.branchId}:${actor.assignmentId}:original-hotel`);expect(node.props).toMatchObject({initial:workspace,base:'/holdings/'+actor.holdingSlug,section,selectedGuestAccount:'original-account',selectedRecord:label.startsWith('document')?undefined:'original-record'});
 expect(owner.work).toHaveBeenCalledExactlyOnceWith(actor.holdingSlug);expect(owner.pms).toHaveBeenCalledTimes(1);expect(owner.pms.mock.calls[0].slice(0,4)).toEqual([actor,'original-hotel',section==='night-audit',false]);expect(fetcher).not.toHaveBeenCalled();
});
it('standard PMS Backoffice navigation retains exact native visible-section permissions and scoped Property destinations',async()=>{
 const {StandardPmsBackofficeNavigation}=await import('@erpx/feature-pms/components/standard-pms-backoffice-navigation'),{pmsMenu}=await import('@erpx/feature-shared/components/hospitality/module-menu'),{visiblePmsSections}=await import('@erpx/server-pms/public/rules/pms-model'),{canonicalUiHref}=await import('@erpx/feature-work_projection/lib/destination-registry');
 const permissions=['pms.read','pms.housekeeping'],base='/holdings/'+actor.holdingSlug+'/branches/original-branch',propertyId='original hotel / ก';
 const items=pmsMenu('en').filter(item=>visiblePmsSections(permission=>permissions.includes(permission)).some(([id])=>id===item.id)).map(item=>({...item,href:canonicalUiHref(`${base}/pms/${item.id}?property=${encodeURIComponent(propertyId)}`)}));
 const stream=await renderToReadableStream(<StandardPmsBackofficeNavigation label="Hotel tasks" current="housekeeping" items={items}/>);await stream.allReady;const html=await new Response(stream).text();
 expect(html).toContain('aria-label="Hotel tasks"');expect(html).toContain('aria-current="page"');expect(html).toContain('data-appearance="standard"');expect(html).toContain(base+'/pms/housekeeping?property=original%20hotel%20%2F%20%E0%B8%81');expect(html).toContain(base+'/pms/inventory?');expect(html).toContain(base+'/pms/settings?');expect(html).not.toContain(base+'/pms/cashier?');expect(html).not.toContain(base+'/pms/guest-accounts?');expect(html).not.toContain(base+'/pms/reports?');expect(html).not.toContain(base+'/pms/night-audit?');expect(fetcher).not.toHaveBeenCalled();
});
it('actual standard PMS Backoffice parent keeps honest first-hotel setup and native owner authority without a duplicate mount read',async()=>{
 const {StandardPmsBackofficeWorkspace}=await import('@erpx/feature-pms/components/standard-pms-backoffice-workspace'),{CurrencyProvider}=await import('@erpx/feature-finance/components/money/currency-provider');
 owner.path='/holdings/'+actor.holdingSlug+'/pms/settings';owner.query='';const workspace=originalPmsWorkspace();
 const stream=await renderToReadableStream(<CurrencyProvider initial={money} apiBase={'/api/v1/holdings/'+actor.holdingSlug}><StandardPmsBackofficeWorkspace initial={workspace} base={'/holdings/'+actor.holdingSlug} section="settings" crossLinks={{inventory:false,finance:false,pos:false}}/></CurrencyProvider>);await stream.allReady;const html=await new Response(stream).text();
 expect(html).toContain('ui-page-header');expect(html).toContain('data-appearance="standard"');expect(html).toContain('Hotel PMS');expect(html).toContain('Set up your first hotel');expect(html).toContain('Hotel + Restaurant Setup');expect(html).toContain('/holdings/'+actor.holdingSlug+'/pms/setup');expect(html).toContain('aria-label="Hotel"');expect(html).not.toContain('Create hotel');expect(fetcher).not.toHaveBeenCalled();expect(owner.pms).not.toHaveBeenCalled();
});
it('standard PMS Backoffice readiness retains original Finance/setup handoffs and never substitutes an opening command',async()=>{
 const {StandardPmsBackofficeReadiness}=await import('@erpx/feature-pms/components/standard-pms-backoffice-readiness');
 const items=[{id:'accounting',label:'Accounting review',ready:false,owner:'Finance approver',href:'/holdings/'+actor.holdingSlug+'/pms/settings?property=original-hotel'},{id:'open',label:'Verify readiness and open hotel',ready:false,owner:'Hotel administrator',href:'/holdings/'+actor.holdingSlug+'/pms/settings?property=original-hotel'}];
 const stream=await renderToReadableStream(<StandardPmsBackofficeReadiness title="Hotel readiness" items={items}/>);await stream.allReady;const html=await new Response(stream).text();
 expect(html).toContain('data-appearance="standard"');expect(html).toContain('2 to prepare');expect(html).toContain('Finance approver');expect(html).toContain('Hotel administrator');expect(html).toContain('Verify readiness and open hotel');expect(html).toContain(items[0].href);expect(html).not.toContain('<form');expect(fetcher).not.toHaveBeenCalled();
});


import {isValidElement as isInitialContextElement,type ReactNode as InitialContextNode} from 'react';
import {WorkContextBoundary as FrontInitialContextBoundary} from '@erpx/workplace/components/work-context-boundary';
import FrontNotificationsSnapshotScreen from '@erpx/workplace/routes/front/[holdingSlug]/[companySlug]/[app]/_screens/standard-front-notifications-entry';
it.each(['pms','pos','notifications','base'] as const)('binds the actual %s Front SSR boundary snapshot to current public user/role/location metadata',async screen=>{
 owner.app=screen==='pms'?'pms':'pos';const segment=screen==='pms'?'today':screen==='notifications'?'notifications':screen==='pos'?'sale':undefined;owner.path=`/${actor.holdingSlug}/${company.companySlug}/holding${segment?'/'+segment:''}`;owner.actions.mockReturnValue([]);
 let roleObservedAt=0;owner.role.mockImplementation(async()=>{roleObservedAt=Date.now();return {options:[{id:actor.assignmentId,role:actor.role}]};});
 const component=screen==='pms'?PmsTodayScreen:screen==='pos'?PosSaleScreen:screen==='notifications'?FrontNotificationsSnapshotScreen:FrontEntryScreen,node=await component({params:Promise.resolve({holdingSlug:actor.holdingSlug,companySlug:company.companySlug,app:owner.app,path:['holding',...(segment?[segment]:[])]}),searchParams:Promise.resolve({})}),found:ComponentProps<typeof FrontInitialContextBoundary>[]=[];
 function walk(value:InitialContextNode){if(Array.isArray(value)){value.forEach(walk);return;}if(!isInitialContextElement<{children?:InitialContextNode}>(value))return;if(value.type===FrontInitialContextBoundary)found.push(value.props as ComponentProps<typeof FrontInitialContextBoundary>);walk(value.props.children);}walk(node);expect(found).toHaveLength(1);const props=found[0],initial=props.initialCurrent;expect(props.userId).toBe(actor.userId);expect(initial).toEqual({version:1,base:props.base,userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,revision:actor.authority!.revision,checkedAt:expect.any(Number),frontApp:owner.app,frontLocation:'holding',frontRoleEndpoint:`/api/v1/front/${actor.holdingSlug}/${company.companySlug}/${owner.app}`});expect(initial!.checkedAt).toBeLessThanOrEqual(roleObservedAt);expect(Object.keys(initial!).sort()).toEqual(['assignmentId','base','checkedAt','companyId','frontApp','frontLocation','frontRoleEndpoint','revision','userId','version']);expect(JSON.stringify(initial)).not.toContain('current-session');expect(owner.role).toHaveBeenCalledExactlyOnceWith(actor.holdingSlug,actor.companyId,owner.app,'holding');expect(owner.work).toHaveBeenCalledExactlyOnceWith({kind:'holding',holdingSlug:actor.holdingSlug});expect(fetcher).not.toHaveBeenCalled();
});
