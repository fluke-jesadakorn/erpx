import { afterEach, beforeEach, expect, it, vi } from "vitest";
const guarded = vi.hoisted(() => ({ session: vi.fn(async () => { throw Error("UNAUTHENTICATED"); }), holding: vi.fn(), page:vi.fn(), host:vi.fn() }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ currentSession: guarded.session, requirePlatformContext: async () => { throw Error("UNAUTHENTICATED"); } }));
vi.mock("@erpx/server-runtime/public/runtime", () => ({ runtimeEnv: async () => ({ ERPX_ENVIRONMENT: "local" }), activeHoldingBindings: async () => [], holdingResources: guarded.holding }));
import { providerCommercialApi } from "@erpx/server-commercial/public/provider-commercial";
vi.mock("@erpx/server-commercial/public/provider-page-context",()=>({providerCommercialPageContext:guarded.page}));
vi.mock("@erpx/server-identity_access/public/account-preference-snapshot",()=>({preferredAccountLanguage:async()=> 'en'}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:guarded.host()}),cookies:async()=>({get:()=>undefined})}));
vi.mock('next/navigation',()=>({forbidden:()=>{throw Error('FORBIDDEN');},notFound:()=>{throw Error('NOT_FOUND');},redirect:(href:string)=>{throw Error('REDIRECT:'+href);},usePathname:()=>'/provider/commercial',useSearchParams:()=>new URLSearchParams(),useRouter:()=>({refresh:vi.fn(),push:vi.fn(),replace:vi.fn()})}));
const origin = "https://admin-dev-erpx.jesadakorn.com", prefix = "/api/v1/platform/commercial/";
beforeEach(() => {
  vi.stubEnv("ERPX_ENVIRONMENT", "local"); vi.stubEnv("ERPX_PRODUCT_ENVIRONMENT", "development");
  vi.stubEnv("ERPX_PRODUCT_ROUTING", "enabled"); vi.stubEnv("ERPX_PRODUCT_SSO", "enabled"); vi.clearAllMocks();guarded.host.mockReturnValue("admin-dev-erpx.jesadakorn.com");
});
afterEach(() => vi.unstubAllEnvs());
function request(path: string, method = "POST", extra: { host?: string; origin?: string; mime?: string; body?: string } = {}) {
  return new Request(origin + prefix + path, { method, headers: { host: extra.host ?? new URL(origin).host,
    origin: extra.origin ?? origin, "content-type": extra.mime ?? "application/json" }, ...(method === "GET" ? {} : { body: extra.body ?? "{}" }) });
}
async function denial(response: Response, status: number, code: string) {
  expect(response.status).toBe(status); expect((await response.json()).error.code).toBe(code);
  expect(response.headers.get("cache-control")).toBe("private, no-store"); expect(response.headers.get("referrer-policy")).toBe("no-referrer");
  expect(guarded.holding).not.toHaveBeenCalled();
}
it("keeps ordinary package snapshots and mutations on the exact Administration host", async () => {
  for (const host of ["console-dev-erpx.jesadakorn.com", "finance-dev-erpx.jesadakorn.com", "admin-dev-erpx.jesadakorn.com.evil.test"]) {
    await denial(await providerCommercialApi(request("packages/" + "apply", "POST", { host }), ["packages", "apply"]), 404, "NOT_FOUND");
    await denial(await providerCommercialApi(request("packages?holdingId=h1&companyId=c1", "GET", { host }), ["packages"]), 404, "NOT_FOUND");
  }
  expect(guarded.session).not.toHaveBeenCalled();
});
it("rejects a foreign Origin before reading a provider session or request payload", async () => {
  await denial(await providerCommercialApi(request("packages/apply", "POST", { origin: "https://apps-dev-erpx.jesadakorn.com", body: "invalid" }), ["packages", "apply"]), 403, "ORIGIN_FORBIDDEN");
  expect(guarded.session).not.toHaveBeenCalled();
});
it("requires an actual JSON media type rather than an application/json prefix", async () => {
  for (const mime of ["application/jsonp", "application/json-patch+json", "text/plain"])
    await denial(await providerCommercialApi(request("packages/preview", "POST", { mime }), ["packages", "preview"]), 415, "JSON_REQUIRED");
  expect(guarded.session).not.toHaveBeenCalled();
  await denial(await providerCommercialApi(request("packages/preview", "POST", { mime: "Application/JSON; charset=utf-8" }), ["packages", "preview"]), 401, "UNAUTHENTICATED");
});
it("bounds and validates the JSON body without acquiring a Holding connection", async () => {
  await denial(await providerCommercialApi(request("packages/preview", "POST", { body: "{" }), ["packages", "preview"]), 422, "INVALID_JSON");
  await denial(await providerCommercialApi(request("packages/preview", "POST", { body: " ".repeat(200_001) }), ["packages", "preview"]), 413, "REQUEST_TOO_LARGE");
  expect(guarded.session).not.toHaveBeenCalled();
});
it("refuses ambiguous Company selectors and unknown snapshot query fields", async () => {
  for (const query of ["companyId=c1", "holdingId=h1&companyId=c1&companyId=c2", "holdingId=h1&companyId=c1&role=owner"])
    expect((await providerCommercialApi(request("packages?" + query, "GET"), ["packages"])).status).toBe(422);
  expect(guarded.session).not.toHaveBeenCalled(); expect(guarded.holding).not.toHaveBeenCalled();
});
it("requires the normal current provider session for both snapshot and Apply", async () => {
  await denial(await providerCommercialApi(request("packages?holdingId=h1&companyId=c1", "GET"), ["packages"]), 401, "UNAUTHENTICATED");
  await denial(await providerCommercialApi(request("packages/apply"), ["packages", "apply"]), 401, "UNAUTHENTICATED");
  expect(guarded.session).toHaveBeenCalledTimes(2);
});

import {providerPackageReview,captureProviderPackageCommand,acknowledgeProviderPackageCommand,providerLegacyPackageReview,captureProviderLegacyPackageCommand} from '@erpx/feature-commercial/provider-package-command';
import {createProviderCommercialPageContext} from '@erpx/server-commercial/application/provider-page-context';
import type {PackageGrantBatch} from '@erpx/server-commercial/public/package-grants';
const packageRequest:PackageGrantBatch={companyId:'owned-company',expectedRevision:'a'.repeat(64),idempotencyKey:'captured-request-key',changes:[{expectedVersion:3,grant:{companyId:'owned-company',packageId:'finance',packageVersion:1,state:'draining',startsAt:0,endsAt:null,version:4,reason:'Reviewed outstanding Finance work',evidence:'test:original-reviewed-source'}}]};
const packageComparison={companyId:packageRequest.companyId,expectedRevision:packageRequest.expectedRevision,before:[],after:[packageRequest.changes[0].grant],changes:packageRequest.changes},packageReview={comparison:packageComparison,request:packageRequest,review:{token:'opaque-reviewed-owner-token',expiresAt:1234}};
it('provider package acknowledgement validates the exact reviewed Company, revision, versions, periods, reason and source evidence',()=>{
 const review=providerPackageReview(packageReview,packageRequest),capture=captureProviderPackageCommand(review,'original-holding','original-context',7,'draft-at-capture');expect(acknowledgeProviderPackageCommand({receipt:packageComparison},capture,{contextKey:'original-context',editVersion:7,draftImage:'draft-at-capture'}).clearDraft).toBe(true);
 for(const receipt of [{...packageComparison,companyId:'foreign-company'},{...packageComparison,expectedRevision:'b'.repeat(64)},{...packageComparison,after:[]},{...packageComparison,changes:[{...packageRequest.changes[0],expectedVersion:4}]},{...packageComparison,after:[{...packageRequest.changes[0].grant,reason:'Different supplied reason'}]},{...packageComparison,after:[{...packageRequest.changes[0].grant,evidence:'Different source evidence'}]}])expect(()=>acknowledgeProviderPackageCommand({receipt},capture,{contextKey:'original-context',editVersion:7,draftImage:'draft-at-capture'})).toThrow('acknowledgement');
});
it('provider package unknown responses preserve the original immutable body/key and never acknowledge another Provider context',()=>{
 const source=structuredClone(packageReview),capture=captureProviderPackageCommand(source,'original-holding','original-context',7,'original-draft'),body=capture.body;source.request.changes[0].grant.reason='Later source object edit';expect(capture.body).toBe(body);expect(capture.key).toBe(packageRequest.idempotencyKey);expect(JSON.parse(body)).toEqual({holdingId:'original-holding',request:packageReview.request,token:packageReview.review.token});for(const result of [undefined,{}, {receipt:{}},{receipt:{companyId:'foreign'}}])expect(()=>acknowledgeProviderPackageCommand(result,capture,{contextKey:'original-context',editVersion:7,draftImage:'original-draft'})).toThrow();expect(capture.body).toBe(body);expect(()=>acknowledgeProviderPackageCommand({receipt:packageComparison},capture,{contextKey:'different-context',editVersion:7,draftImage:'original-draft'})).toThrow();
});
it('provider package receipt leaves newer local edits intact while acknowledging only the original captured version',()=>{
 const capture=captureProviderPackageCommand(packageReview,'original-holding','original-context',7,'original-draft');for(const current of [{contextKey:'original-context',editVersion:8,draftImage:'original-draft'},{contextKey:'original-context',editVersion:7,draftImage:'later-edited-draft'}])expect(acknowledgeProviderPackageCommand({receipt:packageComparison},capture,current).clearDraft).toBe(false);
});
it('provider package preview must match its captured request and cannot substitute another grant or request key',()=>{
 for(const value of [{...packageReview,request:{...packageRequest,idempotencyKey:'foreign-request-key'}},{...packageReview,comparison:{...packageComparison,changes:[{...packageRequest.changes[0],expectedVersion:5}]}},{...packageReview,review:{token:'',expiresAt:1234}}])expect(()=>providerPackageReview(value,packageRequest)).toThrow();
});
it('provider Commercial SSR snapshots bind original display flags to the same minimal Provider context without mutating a role',async()=>{
 const context={accountKey:'a'.repeat(64),contextKey:'b'.repeat(64),selectedId:'assigned-provider',revision:1,choices:[{id:'assigned-provider',label:'Assigned operator',version:'c'.repeat(64)}]},reader=vi.fn(async()=>context),overview=vi.fn(async()=>({companies:[],canApply:true})),read=createProviderCommercialPageContext({context:reader,overview,changed:()=>Error('PROVIDER_CONTEXT_CHANGED')});expect(await read()).toEqual({context,overview:{companies:[],canApply:true}});expect(reader).toHaveBeenCalledTimes(2);expect(overview).toHaveBeenCalledTimes(1);
 const raced=vi.fn().mockResolvedValueOnce(context).mockResolvedValueOnce({...context,contextKey:'d'.repeat(64),revision:2});await expect(createProviderCommercialPageContext({context:raced,overview,changed:()=>Error('PROVIDER_CONTEXT_CHANGED')})()).rejects.toThrow('PROVIDER_CONTEXT_CHANGED');
});

import {legacyPackagePreview} from '@erpx/server-commercial/public/rules/legacy-packages';
const legacyFacts={companyId:'owned-company',beta:false,observedAt:100,revision:'legacy-source-revision',witnesses:[{assignmentId:'existing-assignment',assignmentVersion:2,roleId:'existing-finance-role',roleRevision:3,scopeKind:'company',scopeNodeId:null,modules:['finance'],validUntil:10000,identityVersion:2,hrAppointmentVersion:null,compatibility:[]}]};
function legacyReview(){const input={companyId:legacyFacts.companyId,reason:'Preserve existing Company access',evidence:'test:original-legacy-source'},comparison=legacyPackagePreview(legacyFacts,[],{...input,now:100,startsAt:100}),request={companyId:input.companyId,expectedRevision:'a'.repeat(64),idempotencyKey:'captured-legacy-request',changes:comparison.changes};return {input,value:{comparison,request,review:{token:'original-legacy-source-token',expiresAt:10000}}};}
it('legacy migration captures the actual multibatch owner comparison and exact response before clearing its original local draft',()=>{
 const {input,value}=legacyReview(),review=providerLegacyPackageReview(value,input),capture=captureProviderLegacyPackageCommand(review,'original-holding','original-provider',4,'captured-legacy-draft'),receipt={companyId:input.companyId,expectedRevision:value.request.expectedRevision,before:value.comparison.before,after:value.comparison.after,changes:[...value.comparison.changes].reverse()};expect(review.comparison.roles[0].modulesBefore).toEqual(['finance']);expect(review.request!.changes.length).toBeGreaterThan(1);expect(acknowledgeProviderPackageCommand({receipt},capture,{contextKey:'original-provider',editVersion:4,draftImage:'captured-legacy-draft'}).clearDraft).toBe(true);expect(acknowledgeProviderPackageCommand({receipt},capture,{contextKey:'original-provider',editVersion:5,draftImage:'later-reason'}).clearDraft).toBe(false);expect(capture.key).toBe(value.request.idempotencyKey);expect(JSON.parse(capture.body).token).toBe(value.review.token);
 for(const bad of [{...value,comparison:{...value.comparison,companyId:'foreign'}},{...value,request:{...value.request,changes:[{...value.request.changes[0],grant:{...value.request.changes[0].grant,reason:'Substituted reason'}}]}},{...value,comparison:{...value.comparison,roles:[{...value.comparison.roles[0],modulesBefore:null}]}},{...value,comparison:{...value.comparison,authorityChanges:['added authority']}}])expect(()=>providerLegacyPackageReview(bad,input)).toThrow('acknowledgement');
});
it('legacy no-change or blocked comparison never fabricates an applicable reviewed request',()=>{
 const {input,value}=legacyReview(),comparison=legacyPackagePreview(legacyFacts,value.comparison.after,{...input,now:100,startsAt:100}),review=providerLegacyPackageReview({comparison,request:null,review:null},input);expect(review.request).toBeNull();expect(review.comparison.changes).toEqual([]);expect(()=>captureProviderLegacyPackageCommand(review,'original-holding','original-provider',1,'draft')).toThrow();expect(()=>providerLegacyPackageReview({comparison,request:value.request,review:value.review},input)).toThrow();
});
it('known-stale or wrong-account Provider observation never acknowledges even an otherwise matching captured receipt',()=>{
 const capture=captureProviderPackageCommand(packageReview,'original-holding','original-context',7,'original-draft');expect(()=>acknowledgeProviderPackageCommand({receipt:packageComparison},capture,{contextKey:'original-context',editVersion:7,draftImage:'original-draft',blocked:true})).toThrow('acknowledgement');
});

import {isValidElement,type ReactElement,type ReactNode} from 'react';
function pageElement(node:ReactNode,name:string):ReactElement<Record<string,unknown>>|undefined{if(Array.isArray(node)){for(const child of node){const found=pageElement(child,name);if(found)return found;}return;}if(!isValidElement<{children?:ReactNode}>(node))return;if(typeof node.type==='function'&&node.type.name===name)return node as ReactElement<Record<string,unknown>>;return pageElement(node.props.children,name);}
it('actual Commercial pages seed the minimal Provider boundary around their original workflows without a Client authority snapshot',async()=>{
 const seed={accountKey:'a'.repeat(64),contextKey:'b'.repeat(64),selectedId:'assigned-provider',revision:1,choices:[{id:'assigned-provider',label:'Assigned operator',version:'c'.repeat(64)}]},overview={companies:[{id:'company',name:'Company',holdingId:'h001',holding:'Holding'}],canApply:true};guarded.page.mockResolvedValue({context:seed,overview});
 for(const [path,workflow] of [["@erpx/feature-admin/routes/provider/commercial/page",'LegacyGrantReview'],["@erpx/feature-admin/routes/provider/commercial/packages/page",'CompanyPackageManager']]){const {default:Page}=await(path.endsWith('packages/page')?import('@erpx/feature-admin/routes/provider/commercial/packages/page'):import('@erpx/feature-admin/routes/provider/commercial/page')),page=await Page({searchParams:Promise.resolve({})}),boundary=pageElement(page,'ProviderContextBoundary'),manager=pageElement(boundary?.props.children as ReactNode,workflow);expect(boundary?.props.initial).toEqual(seed);expect(pageElement(boundary?.props.children as ReactNode,'ProviderContextSwitcher')).toBeDefined();expect(manager?.props).toEqual({companies:overview.companies,canApply:true,language:'en'});expect(Object.keys(boundary!.props.initial as object).sort()).toEqual(['accountKey','choices','contextKey','revision','selectedId']);}
 expect(guarded.page).toHaveBeenCalledTimes(2);expect(guarded.session).not.toHaveBeenCalled();
});
it('actual Commercial pages preserve original host, login and Commercial denial before exposing a workflow',async()=>{
 const {default:Page}=await import('@erpx/feature-admin/routes/provider/commercial/packages/page');guarded.host.mockReturnValue('finance-dev-erpx.jesadakorn.com');await expect(Page({searchParams:Promise.resolve({})})).rejects.toThrow('NOT_FOUND');expect(guarded.page).not.toHaveBeenCalled();guarded.host.mockReturnValue('admin-dev-erpx.jesadakorn.com');guarded.page.mockRejectedValueOnce(Error('UNAUTHENTICATED'));await expect(Page({searchParams:Promise.resolve({})})).rejects.toThrow('REDIRECT:/sso/start?next=%2Fprovider%2Fcommercial%2Fpackages');guarded.page.mockRejectedValueOnce(Error('FORBIDDEN'));const denied=await Page({searchParams:Promise.resolve({})});expect(pageElement(denied,'CompanyPackageManager')).toBeUndefined();expect(pageElement(denied,'ProviderContextBoundary')).toBeUndefined();
});

it('forwards Service lifecycle through exact native Holding transport without authority claims or fallback',async()=>{
 const {createNativeServiceCommercialTransport}=await import('../../../packages/server-commercial/src/adapters/native-service-commercial');
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingId:'h001',holdingSlug:'acme'}]};
 const send=vi.fn(async(_input:Parameters<typeof import('@erpx/native-sdk/field-service-parts').nativeServicePackageRequest>[0])=>{void _input;return {status:200,body:{data:{receipt:{companyId:'company'}}}};});
 const run=createNativeServiceCommercialTransport(JSON.stringify(config),send),body='{"holdingId":"h001","request":{"changes":[]},"token":"captured"}';
 const h=new Headers({host:'acme.example.test',origin:'https://acme.example.test',cookie:'__Host-erpx-product=synthetic','content-type':'application/json','idempotency-key':'captured-service-key','x-erpx-platform-assignment':'hint','x-forwarded-host':'forged','authorization':'never-forward'});
 expect(await run(h,'apply',{body})).toEqual({receipt:{companyId:'company'}});
 expect(send).toHaveBeenCalledOnce();const call=send.mock.calls[0]![0];expect(call).toMatchObject({baseUrl:config.privateBaseUrl,operation:'apply',body,slug:'acme'});
 expect([...call.headers.keys()].sort()).toEqual(['accept','content-type','cookie','host','idempotency-key','origin','x-erpx-platform-assignment']);
 h.set('origin','https://foreign.example.test');await expect(run(h,'apply',{body})).rejects.toMatchObject({status:403,code:'ORIGIN_FORBIDDEN'});expect(send).toHaveBeenCalledOnce();
 h.set('origin',config.holdings[0]!.origin);send.mockRejectedValueOnce(Error('offline'));
 await expect(run(h,'apply',{body})).rejects.toMatchObject({status:503,code:'NATIVE_SERVICE_COMMERCIAL_UNAVAILABLE'});expect(send).toHaveBeenCalledTimes(2);expect(guarded.holding).not.toHaveBeenCalled();
});
it('fences native Service grants below legacy HTTP, receipts and internal transaction entrypoints',async()=>{
 const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');
 const database={transaction:vi.fn(),assertActiveTransaction:vi.fn(),prepare:vi.fn()};
 const authorize=vi.fn(),obligations=vi.fn(),store=packageGrantStore(database as never,{authorize,obligations});
 for(const packageId of ['service','service-parts-assets']){
  const grant={companyId:'company',packageId,packageVersion:1,state:'inactive' as const,startsAt:0,endsAt:null,version:2,reason:'Reviewed',evidence:'Actual source'};
  const single={grant,expectedVersion:1,idempotencyKey:'captured-service-key'},batch={companyId:'company',expectedRevision:'a'.repeat(64),idempotencyKey:single.idempotencyKey,changes:[single]};
  for(const call of [()=>store.preview(single),()=>store.apply(single),()=>store.batch.preview(batch),()=>store.batch.apply(batch,async()=>{}),()=>store.batch.previewIn(database as never,batch),()=>store.batch.applyIn(database as never,batch,async()=>{})])expect(call).toThrow('NATIVE_SERVICE_PACKAGE_REQUIRED');
 }
 expect(authorize).not.toHaveBeenCalled();expect(obligations).not.toHaveBeenCalled();expect(database.prepare).not.toHaveBeenCalled();expect(database.transaction).not.toHaveBeenCalled();expect(database.assertActiveTransaction).not.toHaveBeenCalled();
});
