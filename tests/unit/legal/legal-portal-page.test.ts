import {createHash} from 'node:crypto';
import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import type {ComponentProps} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
vi.mock('server-only',()=>({}));
const owner=vi.hoisted(()=>({binding:vi.fn(),queries:vi.fn(),list:vi.fn(),detail:vi.fn(),preferences:vi.fn(),snapshot:vi.fn()}));
vi.mock('@/server/modules/legal/public/presentation-context-queries',()=>({legalPortalPresentationQueries:owner.queries,legalPortalPresentationSnapshot:owner.snapshot}));
vi.mock('@/server/modules/runtime/public/runtime',()=>({holdingBindingBySlug:owner.binding}));
vi.mock('@/server/modules/work_projection/public/quick-access-preferences',()=>({readQuickAccessPreferences:owner.preferences}));
vi.mock('next/navigation',()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn(),prefetch:vi.fn()}),usePathname:()=>'/legal-portal/tenant',useSearchParams:()=>new URLSearchParams(),redirect:(href:string)=>{throw Error(`REDIRECT:${href}`);}}));
import Page from '@/app/legal-portal/[holdingSlug]/page';
import {LegalPortal} from '@/components/legal/legal-portal';
const privateMarker='PRIVATE_LEGAL_PORTAL_UNUSED_SOURCE';
const share={id:'share-1',name:'Reviewed shared agreement',version:7,expires_at:1890000000000,document_id:privateMarker,user_id:privateMarker,checksum:privateMarker,privateSnapshot:privateMarker};
const detail={shareId:'share-1',kind:'contract',doc:{id:privateMarker,name:'Reviewed shared agreement',body:'Approved shared contract body',version:7,checksum:'native-document-checksum',object_key:privateMarker},comments:[{body:'Approved external comment',created_at:25,created_by:privateMarker}],envelopes:[],billing:[],signingEnabled:false,privateSnapshot:privateMarker};
let actor:{userId:string;email:string;sessionId:string};
beforeEach(()=>{vi.resetAllMocks();actor={userId:'external-verified-one',email:'one@example.invalid',sessionId:privateMarker};owner.binding.mockResolvedValue({id:'holding-one'});owner.queries.mockImplementation(async()=>({actor,queries:{portalList:owner.list,portalDetail:owner.detail}}));owner.list.mockResolvedValue([share]);owner.detail.mockResolvedValue(detail);owner.preferences.mockResolvedValue({pinnedIds:['share:share-1'],version:8,privateAuthority:privateMarker});owner.snapshot.mockImplementation(async(_slug:string,selected?:string)=>({initial:await owner.list(),initialDetail:selected?await owner.detail():undefined,commandScope:createHash('sha256').update(JSON.stringify(['holding-one',actor.userId,actor.sessionId])).digest('hex'),preferences:await owner.preferences()}));vi.stubGlobal('fetch',vi.fn(()=>{throw Error('UNEXPECTED_LEGAL_PAGE_GET');}));});
afterEach(()=>vi.unstubAllGlobals());
const page=(selected?:string)=>Page({params:Promise.resolve({holdingSlug:'tenant'}),searchParams:Promise.resolve(selected?{share:selected}:{})});
const props=async(selected?:string)=>(await page(selected)).props as ComponentProps<typeof LegalPortal>;
it('projects only consumed original share and preference display values at the actual Server Page Client boundary',async()=>{
 const value=await props('share-1');expect(value.initial).toEqual([{id:'share-1',name:'Reviewed shared agreement',version:7,expires_at:1890000000000}]);expect(value.quickAccessPreferences).toEqual({pinnedIds:['share:share-1'],version:8});expect(value.initialDetail).toMatchObject({shareId:'share-1',doc:{version:7,checksum:'native-document-checksum'},comments:[{body:'Approved external comment',created_at:25}]});expect(JSON.stringify(value)).not.toContain(privateMarker);expect(value.quickAccessItems[0].href).toContain('share=share-1');expect(owner.snapshot).toHaveBeenCalledExactlyOnceWith('tenant','share-1');expect(owner.queries).not.toHaveBeenCalled();
});
it('renders actual selected original content and initial share list without a mount discovery request',async()=>{
 const selected=renderToStaticMarkup(await page('share-1'));expect(selected).toContain('Approved shared contract body');expect(selected).toContain('Approved external comment');expect(selected).not.toContain(privateMarker);const list=renderToStaticMarkup(await page());expect(list).toContain('Reviewed shared agreement');expect(list).toContain('v7');expect(list).not.toContain(privateMarker);expect(globalThis.fetch).not.toHaveBeenCalled();
});
it('retains the same account session Holding key on source refresh and isolates a different verified account',async()=>{
 const first=await page();owner.list.mockResolvedValue([{...share,version:8}]);owner.preferences.mockResolvedValue({pinnedIds:null,version:9});expect((await page()).key).toEqual(first.key);actor={...actor,userId:'external-verified-two',email:'two@example.invalid'};const second=await page();expect(second.key).not.toEqual(first.key);expect(String(second.key)).not.toContain(actor.userId);expect(String(second.key)).not.toContain(privateMarker);
});
