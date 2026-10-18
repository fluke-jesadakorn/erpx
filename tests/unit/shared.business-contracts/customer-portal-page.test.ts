import {beforeEach,afterEach,expect,it,vi} from 'vitest';
import {isValidElement,type ReactNode,type ReactElement,type ComponentProps} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
vi.mock('server-only',()=>({}));
const owner=vi.hoisted(()=>({binding:vi.fn(),queries:vi.fn(),projection:vi.fn(),preferences:vi.fn()}));
vi.mock('@/server/modules/customer_360/public/presentation-context-queries',()=>({customerPortalPresentationQueries:owner.queries}));
vi.mock('@/server/modules/runtime/public/runtime',()=>({holdingBindingBySlug:owner.binding}));
vi.mock('@/server/modules/work_projection/public/quick-access-preferences',()=>({readQuickAccessPreferences:owner.preferences}));
vi.mock('@/server/modules/work_projection/public/page-error',()=>({pageError:(error:unknown)=>{throw error;}}));
vi.mock('next/navigation',()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn(),prefetch:vi.fn()}),usePathname:()=>'/customer-portal/tenant',useSearchParams:()=>new URLSearchParams(),redirect:(href:string)=>{throw Error(`REDIRECT:${href}`);}}));

import CustomerPortalPage from '@/app/customer-portal/[holdingSlug]/page';
import {CustomerPortalActions} from '@/components/customer_authority/customer-portal-actions';
import {PortalSupportAcknowledgement} from '@/components/customer_authority/portal-support-acknowledgement';
import {QuickAccessShelf} from '@/components/work_projection/quick-access/shelf';

const privateMarker='PRIVATE_PORTAL_SOURCE_SENTINEL';
const purchase={settleExisting:true,history:true,revision:'opaque-purchase',identityRevision:'opaque-identity',nativeGrant:{secret:privateMarker}};
const thread={requestId:'request',requestedAt:10,note:'Original customer delivery question',response:{id:'response',version:3,message:'Independently published company response',publishedAt:20,privateSource:privateMarker},acknowledgedAt:null,closedAt:null,internalAudit:privateMarker};
const record={binding_id:'binding',binding_version:2,order_id:'order',number:'SO-ORIGINAL',kind:'sales',state:'fulfilled',due_date:'2026-10-20',currency:'THB',updated_at:25,fulfillments:[{number:'DELIVERY-1',createdAt:12,privateSource:privateMarker}],actions:[{action:'acknowledge_delivery',createdAt:15,privateSource:privateMarker}],sharedFiles:[{linkId:'shared-link',label:'Reviewed delivery preview',previewAvailable:true,objectKey:privateMarker}],purchase,supportThreads:[thread],email:privateMarker,sessionToken:privateMarker};
let account:{userId:string;email:string;authority:{secret:string}};
beforeEach(()=>{vi.resetAllMocks();account={userId:'verified-customer-one',email:'first@fixture.invalid',authority:{secret:privateMarker}};owner.binding.mockResolvedValue({id:'holding-one'});owner.queries.mockImplementation(async()=>({actor:account,queries:{customerPortalProjection:owner.projection}}));owner.projection.mockResolvedValue({projectionVersion:'customer-order-v1',items:[record],privateSnapshot:privateMarker});owner.preferences.mockResolvedValue({pinnedIds:['order:binding'],version:9,sessionToken:privateMarker});vi.stubGlobal('fetch',vi.fn(()=>{throw Error('UNEXPECTED_PORTAL_SSR_GET');}));});
afterEach(()=>vi.unstubAllGlobals());
function find<Props>(node:ReactNode,type:ReactElement['type']):ReactElement<Props>|undefined{if(Array.isArray(node)){for(const child of node){const result=find<Props>(child,type);if(result)return result;}return;}if(!isValidElement<{children?:ReactNode}>(node))return;if(node.type===type)return node as ReactElement<Props>;return find<Props>(node.props.children,type);}
const page=(holdingSlug='tenant')=>CustomerPortalPage({params:Promise.resolve({holdingSlug})});

it('passes only consumed public order actions preview labels flags and exact response fields from the actual async Portal page',async()=>{
 const tree=await page(),actions=find<ComponentProps<typeof CustomerPortalActions>>(tree,CustomerPortalActions),ack=find<ComponentProps<typeof PortalSupportAcknowledgement>>(tree,PortalSupportAcknowledgement),shelf=find<ComponentProps<typeof QuickAccessShelf>>(tree,QuickAccessShelf);
 expect(actions).toBeDefined();expect(ack).toBeDefined();expect(shelf).toBeDefined();expect(actions!.props).toEqual({holdingSlug:'tenant',bindingId:'binding',bindingVersion:2,purchase:{settleExisting:true,history:true,revision:'opaque-purchase',identityRevision:'opaque-identity'},actions:[{action:'acknowledge_delivery'}],sharedFiles:[{linkId:'shared-link',label:'Reviewed delivery preview',previewAvailable:true}]});
 expect(ack!.props.thread).toEqual({requestId:'request',note:'Original customer delivery question',response:{id:'response',version:3,message:'Independently published company response',publishedAt:20},acknowledgedAt:null,closedAt:null});expect(shelf!.props.initialPreferences).toEqual({pinnedIds:['order:binding'],version:9});expect(JSON.stringify([actions!.props,ack!.props,shelf!.props])).not.toContain(privateMarker);expect(owner.projection).toHaveBeenCalledExactlyOnceWith(account);expect(owner.preferences).toHaveBeenCalledExactlyOnceWith(account.userId,'customer_portal','holding-one');
});

it('resets account-owned Client state when another verified account has a lower authoritative shortcut version at the same tenant',async()=>{
 const first=await page(),firstShelf=find(first,QuickAccessShelf),firstActions=find(first,CustomerPortalActions),firstAck=find(first,PortalSupportAcknowledgement);
 account={userId:'verified-customer-two',email:'second@fixture.invalid',authority:{secret:privateMarker}};owner.preferences.mockResolvedValue({pinnedIds:null,version:0});owner.projection.mockResolvedValue({projectionVersion:'customer-order-v1',items:[{...record,binding_id:'second-binding',order_id:'second-order',purchase:{...purchase,identityRevision:'second-opaque-identity'},supportThreads:[{...thread,requestId:'second-request',response:{...thread.response,id:'second-response'}}]}]});
 const second=await page(),secondShelf=find<ComponentProps<typeof QuickAccessShelf>>(second,QuickAccessShelf),secondActions=find(second,CustomerPortalActions),secondAck=find(second,PortalSupportAcknowledgement);
 expect(secondShelf!.props.initialPreferences).toEqual({pinnedIds:null,version:0});for(const [left,right] of [[firstShelf,secondShelf],[firstActions,secondActions],[firstAck,secondAck]]){expect(left!.key).not.toBeNull();expect(right!.key).not.toEqual(left!.key);expect(String(right!.key)).not.toContain(account.userId);expect(String(right!.key)).not.toContain(account.email);}
});

it('keeps stable account Holding keys across ordinary source and shortcut revisions but changes them for another Holding',async()=>{
 const first=await page();owner.preferences.mockResolvedValue({pinnedIds:null,version:10});owner.projection.mockResolvedValue({projectionVersion:'customer-order-v1',items:[{...record,binding_version:3,purchase:{...purchase,revision:'updated-purchase'},supportThreads:[{...thread,response:{...thread.response,version:4}}]}]});const revised=await page();for(const type of [QuickAccessShelf,CustomerPortalActions,PortalSupportAcknowledgement])expect(find(revised,type)!.key).toEqual(find(first,type)!.key);owner.binding.mockResolvedValue({id:'holding-two'});expect(find(await page('another-tenant'),QuickAccessShelf)!.key).not.toEqual(find(first,QuickAccessShelf)!.key);
});

it('renders the actual initial document delivery public response shortcuts and permitted acknowledgement without a client GET',async()=>{
 const tree=await page(),html=renderToStaticMarkup(tree);for(const label of ['Customer documents','SO-ORIGINAL','DELIVERY-1','Original customer delivery question','Independently published company response','Acknowledge this response','Delivery acknowledged','Reviewed delivery preview'])expect(html).toContain(label);expect(html).not.toContain(privateMarker);expect(globalThis.fetch).not.toHaveBeenCalled();expect(owner.projection).toHaveBeenCalledTimes(1);
});

it('preserves legacy controls and retained read-only source revisions without serializing an authority snapshot',async()=>{
 owner.projection.mockResolvedValue({projectionVersion:'customer-order-v1',items:[{...record,purchase:undefined}]});expect(find<ComponentProps<typeof CustomerPortalActions>>(await page(),CustomerPortalActions)!.props.purchase).toBeUndefined();
 owner.projection.mockResolvedValue({projectionVersion:'customer-order-v1',items:[{...record,purchase:{...purchase,settleExisting:false}}]});const tree=await page(),actions=find<ComponentProps<typeof CustomerPortalActions>>(tree,CustomerPortalActions),ack=find<ComponentProps<typeof PortalSupportAcknowledgement>>(tree,PortalSupportAcknowledgement);expect(actions!.props.purchase).toEqual({settleExisting:false,history:true,revision:'opaque-purchase',identityRevision:'opaque-identity'});expect(ack!.props.bindingVersion).toBe(2);expect(ack!.props.thread.response?.version).toBe(3);expect(renderToStaticMarkup(ack!)).toContain('disabled=""');expect(JSON.stringify([actions!.props,ack!.props])).not.toContain(privateMarker);
});
