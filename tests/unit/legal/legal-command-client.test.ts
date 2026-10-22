import { afterEach, beforeEach, expect, it, vi } from "vitest";
const { workFetch } = vi.hoisted(() => ({ workFetch: vi.fn() }));
vi.mock("@erpx/feature-work_projection/lib/work-fetch", () => ({ workFetch }));
import { createLegalCommandClient } from "@erpx/feature-shared/lib/legal/command-client";

const path = "/api/v1/holdings/acme/legal";
const input = { action: "create", title: "Contract question", metadata: { subject: "Thai text ไทย" } };
let storage: Map<string, string>;
beforeEach(() => {
  storage = new Map();
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  });
  workFetch.mockReset();
});
afterEach(() => vi.unstubAllGlobals());

it("recovers a committed command whose response was lost using the exact saved body and key after refresh", async () => {
  workFetch.mockRejectedValueOnce(new TypeError("connection lost after commit"));
  const first = createLegalCommandClient("company-a:command");
  await expect(first(path, input)).rejects.toThrow("connection lost");
  const original = workFetch.mock.calls[0][1];
  const refreshed = createLegalCommandClient("company-a:command");
  expect(refreshed.hasPending()).toBe(true);
  workFetch.mockResolvedValueOnce(Response.json({ data: { id: "committed-item", replayed: true } }));
  await expect(refreshed.recover()).resolves.toEqual({ input, data: { id: "committed-item", replayed: true } });
  expect(workFetch.mock.calls[1]).toEqual([path, original]);
  expect(refreshed.hasPending()).toBe(false);
  expect(storage.size).toBe(0);
});

it("blocks changed details or another action while the result is uncertain", async () => {
  workFetch.mockRejectedValueOnce(new TypeError("disconnected"));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("disconnected");
  await expect(send(path, { ...input, title: "Changed" })).rejects.toThrow("uncertain result");
  await expect(send(`${path}/release-acceptance`, input)).rejects.toThrow("uncertain result");
  expect(workFetch).toHaveBeenCalledTimes(1);
});

it.each([408, 429, 500, 503])("retains the original command for recovery after HTTP %s", async status => {
  workFetch.mockResolvedValueOnce(Response.json({ error: { message: "Try later" } }, { status }));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("Try later");
  expect(send.hasPending()).toBe(true);
  expect(storage.size).toBe(1);
});

it.each([401, 403, 409, 422])("clears a definitively rejected HTTP %s even if a proxy returns HTML", async status => {
  workFetch.mockResolvedValueOnce(new Response("<html>Rejected</html>", { status }));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("request was rejected");
  expect(send.hasPending()).toBe(false);
  workFetch.mockResolvedValueOnce(Response.json({ data: { id: "new-item" } }));
  await expect(send(path, { ...input, title: "Reviewed details" })).resolves.toEqual({ id: "new-item" });
});

it("retains an unreadable successful response because the command may have committed", async () => {
  workFetch.mockResolvedValueOnce(new Response("truncated", { status: 200 }));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("recover this action");
  expect(send.hasPending()).toBe(true);
});

it("prevents a second in-flight request from reaching the server", async () => {
  let finish!: (response: Response) => void;
  workFetch.mockReturnValueOnce(new Promise<Response>(resolve => { finish = resolve; }));
  const send = createLegalCommandClient("company-a:command");
  const pending = send(path, input);
  await expect(send(path, input)).rejects.toThrow("Wait for the current");
  expect(workFetch).toHaveBeenCalledTimes(1);
  finish(Response.json({ data: { id: "one-item" } }));
  await expect(pending).resolves.toEqual({ id: "one-item" });
});

it("keeps recovery isolated to the saved work context", async () => {
  workFetch.mockRejectedValueOnce(new TypeError("disconnected"));
  await expect(createLegalCommandClient("company-a:command")(path, input)).rejects.toThrow();
  const other = createLegalCommandClient("company-b:command");
  expect(other.hasPending()).toBe(false);
  await expect(other.recover()).resolves.toBeNull();
  expect(workFetch).toHaveBeenCalledTimes(1);
});

import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {LegalPortal} from '@erpx/feature-legal/components/legal-portal';
import {legalPortalSnapshot,legalPortalSnapshotMatches} from '@erpx/feature-legal/lib/portal-snapshot';
const sharedSource={shareId:'actual-share',kind:'contract',doc:{name:'Actual authorized shared version',body:'Current expressly shared document content.',version:3,checksum:'pinned-sha',object_key:'private-storage-secret'},comments:[{body:'External discussion only',created_at:1}],billing:[{id:'entry',title:'Own submitted hours',state:'submitted',data:{minutes:30,amount:0,evidence:'Private source field unused by these controls'}}],envelopes:[{id:'envelope',state:'sent',consent:'Original recorded consent',signer_state:'pending',expires_at:1}],signingEnabled:false,purchase:{settleExisting:true,history:true,revision:'minimal-revision'},sessionId:'private-session-secret',grants:['private-grant']};
it('renders the authorized shared version as initial server content and matches only its exact snapshot source',()=>{const initial=legalPortalSnapshot(sharedSource);const html=renderToStaticMarkup(createElement(LegalPortal,{slug:'h001',commandScope:'isolated-portal-account-session',initial:[],initialShareId:'actual-share',initialDetail:initial,quickAccessItems:[],quickAccessPreferences:{pinnedIds:[],version:0}}));expect(html).toContain('Current expressly shared document content.');expect(html).not.toContain('Loading shared document');expect(legalPortalSnapshotMatches(initial,'actual-share')).toBe(true);expect(legalPortalSnapshotMatches(initial,'another-share')).toBe(false);expect(legalPortalSnapshotMatches(undefined,'actual-share')).toBe(false);});
it('retains shared document history while disabling unavailable settlement and keeping unready signing actions absent',()=>{const initial=legalPortalSnapshot({...sharedSource,purchase:{...sharedSource.purchase,settleExisting:false}}),html=renderToStaticMarkup(createElement(LegalPortal,{slug:'h001',commandScope:'isolated-portal-account-session',initial:[],initialShareId:'actual-share',initialDetail:initial,quickAccessItems:[],quickAccessPreferences:{pinnedIds:[],version:0}})),accept=Array.from(html.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)).find(match=>match[0].includes('Accept invitation'))?.[0];expect(accept).toContain('disabled=""');expect(html).toContain('Download original');expect(html).not.toContain('Accept and sign');expect(html).not.toContain('>Decline<');expect(html).toContain('Original recorded consent');});
it('serializes only the exact Portal presentation fields without private session, grant or unused file metadata',()=>{const snapshot=legalPortalSnapshot(sharedSource);expect(JSON.stringify(snapshot)).not.toMatch(/private-storage-secret|private-session-secret|private-grant|Private source field unused/);expect(Object.keys(snapshot.doc).sort()).toEqual(['body','checksum','name','version']);expect(snapshot.billing[0].data).toEqual({minutes:'30',amount:'0'});expect(snapshot.purchase).toEqual({settleExisting:true,history:true,revision:'minimal-revision'});});

import {legalActionPurchased} from '@erpx/server-legal/public/rules/purchases';
it('separates active Legal amendment from retained comment, hold and original document workflow controls',()=>{
 const draining={newWork:false,settleExisting:true,history:true,revision:'actual-current'},purchases={'legal.contracts':draining,'legal.documents':draining,'legal.access':draining,'legal.obligations':draining};
 for(const action of ['create','update','document','share','revision.create','finance.prepare'])expect(legalActionPurchased(purchases,'contract',action)).toBe(false);
 for(const action of ['comment','transition','hold.acknowledge','hold.release','approval.decide','share.revoke'])expect(legalActionPurchased(purchases,'contract',action)).toBe(true);
 expect(legalActionPurchased({...purchases,'legal.documents':{...draining,settleExisting:false}},'contract','approval.decide')).toBe(false);
 expect(legalActionPurchased({...purchases,'legal.access':{...draining,settleExisting:false}},'contract','share.revoke')).toBe(false);
 expect(legalActionPurchased(purchases,'matter','comment')).toBe(false);
 expect(legalActionPurchased(undefined,'contract','create')).toBe(true);
});

import {legalWorkspaceSnapshot} from '@erpx/feature-legal/lib/workspace-snapshot';
it('keeps owner download content and checksums while omitting private Legal File pointers from browser snapshots',()=>{
 const source={item:{id:'actual-contract'},documents:[{id:'actual-original',item_id:'actual-contract',version:2,name:'Approved original.pdf',media_type:'application/pdf',checksum:'current-checksum',bytes:100,body:'',created_by:'authorized-owner',created_at:1,locked:true,object_key:'private/canonical/original',attachment_link_id:'private-link-id',attachment_source_id:'private-source-id'}],events:[],members:[],holds:[],links:[],comments:[],envelopes:[],shares:[],submissions:[]} as unknown as Parameters<typeof legalWorkspaceSnapshot>[0],snapshot=legalWorkspaceSnapshot(source);
 expect(snapshot.documents).toEqual([expect.objectContaining({id:'actual-original',checksum:'current-checksum',version:2,object_key:null})]);expect(JSON.stringify(snapshot)).not.toMatch(/private\/canonical|private-link-id|private-source-id|attachment_link_id|attachment_source_id/);expect(source.documents[0].object_key).toBe('private/canonical/original');
});

it('retains exact Legal hold and existing retention duties after inactive without admitting new policy or business work',()=>{
 const inactive={newWork:false,settleExisting:false,history:true,revision:'actual-inactive-history'},purchases={'legal.contracts':inactive,'legal.obligations':inactive,'legal.documents':inactive,'legal.access':inactive};
 for(const action of ['hold.create','hold.acknowledge','hold.request-release','hold.release','retention.assign','retention.request','retention.approve','retention.retry','retention.cancel','retention.approve-policy','retention.retire-policy'])expect(legalActionPurchased(purchases,'contract',action)).toBe(true);
 for(const action of ['retention.policy','create','update','document','comment','share','approval.open'])expect(legalActionPurchased(purchases,'contract',action)).toBe(false);
});

import {createLegalPortalCommandClient} from '@erpx/feature-legal/lib/portal-command-client';
import {legalPortalUploadFingerprintInput} from '@erpx/server-legal/public/rules/portal-upload-recovery';
const portalBase='/api/legal-portal/h001',portalSource={shareId:'actual-share',version:3,checksum:'immutable-shared-revision'},portalInput={action:'comment',shareId:'actual-share',body:'Original submitted discussion'};
function portalTransport(){const transport=vi.fn();vi.stubGlobal('fetch',transport);return transport;}
it('recovers a lost public Portal response after refresh with the exact saved body and key under its original account session',async()=>{
 const transport=portalTransport();transport.mockRejectedValueOnce(new TypeError('lost after actual commit'));const send=createLegalPortalCommandClient(portalBase,'account-session-a');await expect(send.command(portalSource,portalInput)).rejects.toThrow('lost');const original=transport.mock.calls[0];expect(send.pending()).toMatchObject({kind:'command',shareId:'actual-share'});expect(createLegalPortalCommandClient(portalBase,'account-session-b').pending()).toBeNull();
 const refreshed=createLegalPortalCommandClient(portalBase,'account-session-a');transport.mockResolvedValueOnce(Response.json({data:{id:'actual-share'}}));expect(await refreshed.recover()).toMatchObject({attempt:{source:portalSource,body:JSON.stringify(portalInput)},data:{id:'actual-share'}});expect(transport.mock.calls[1]).toEqual(original);expect(refreshed.pending()).toBeNull();expect(storage.size).toBe(0);
});
it.each([null,{}, {id:'another-share'}, {id:123}])('keeps uncertain Portal work when a successful response has no exact owner acknowledgement (%j)',async data=>{
 const transport=portalTransport();transport.mockResolvedValueOnce(Response.json({data}));const send=createLegalPortalCommandClient(portalBase,'account-session-a');await expect(send.command(portalSource,portalInput)).rejects.toThrow('exact shared-document action');expect(send.pending()).toBeTruthy();await expect(send.command(portalSource,{...portalInput,body:'Later unsent draft'})).rejects.toThrow('uncertain result');expect(transport).toHaveBeenCalledTimes(1);
});
it('retains the public command through an unreadable acknowledgement and later auth denial without treating a lost committed receipt as a new request',async()=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a');transport.mockResolvedValueOnce(new Response('truncated',{status:200}));await expect(send.command(portalSource,portalInput)).rejects.toThrow('could not be read');const original=transport.mock.calls[0];transport.mockResolvedValueOnce(Response.json({error:{message:'Current session revoked'}},{status:403}));await expect(send.recover()).rejects.toThrow('Current session revoked');expect(send.pending()).toBeTruthy();transport.mockResolvedValueOnce(Response.json({data:{id:'actual-share'}}));await send.recover();expect(transport.mock.calls[1]).toEqual(original);expect(transport.mock.calls[2]).toEqual(original);expect(send.pending()).toBeNull();
});
it('allows a corrected Portal draft after an initial definitive validation rejection while retaining unknown server failures for exact retry',async()=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a');transport.mockResolvedValueOnce(Response.json({error:{message:'Invalid comment'}},{status:422}));await expect(send.command(portalSource,portalInput)).rejects.toThrow('Invalid comment');expect(send.pending()).toBeNull();const corrected={...portalInput,body:'Corrected submitted discussion'};transport.mockResolvedValueOnce(Response.json({error:{message:'Gateway disconnected'}},{status:503}));await expect(send.command(portalSource,corrected)).rejects.toThrow('Gateway disconnected');expect(send.pending()).toBeTruthy();expect(transport.mock.calls[1][1].headers['Idempotency-Key']).not.toBe(transport.mock.calls[0][1].headers['Idempotency-Key']);
});
it('blocks a changed share revision or duplicate in-flight public command while retaining the original submitted body',async()=>{
 let finish!:(response:Response)=>void;const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a');transport.mockReturnValueOnce(new Promise<Response>(resolve=>{finish=resolve;}));const pending=send.command(portalSource,portalInput);await expect(send.command(portalSource,portalInput)).rejects.toThrow('Wait for the current');finish(Response.json({data:{id:'wrong-share'}}));await expect(pending).rejects.toThrow('exact shared-document action');await expect(send.command({...portalSource,version:4},portalInput)).rejects.toThrow('uncertain result');expect(transport).toHaveBeenCalledTimes(1);expect(JSON.parse(transport.mock.calls[0][1].body).body).toBe('Original submitted discussion');
});
it('recovers a public upload only with the same original bytes, name and media type after refresh, without persisting document bytes',async()=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a'),file=new File(['%PDF-original private selected bytes'],'Retained original.pdf',{type:'application/pdf'});transport.mockRejectedValueOnce(new TypeError('lost upload response'));await expect(send.upload(portalSource,file)).rejects.toThrow('lost upload');const original=transport.mock.calls[0];expect([...storage.values()].join('')).not.toContain('private selected bytes');const refreshed=createLegalPortalCommandClient(portalBase,'account-session-a');expect(refreshed.pending()).toMatchObject({kind:'upload',needsFile:true,name:file.name});await expect(refreshed.recover()).rejects.toThrow('Reselect the same');const changed=new File([(await file.text()).replace('original','changed!')],file.name,{type:file.type});expect(changed.size).toBe(file.size);await expect(refreshed.recover(changed)).rejects.toThrow('uncertain result');expect(transport).toHaveBeenCalledTimes(1);
 transport.mockImplementationOnce(async(_path,options)=>Response.json({data:{id:options.headers['Idempotency-Key']}}));expect(await refreshed.recover(file)).toMatchObject({data:{id:original[1].headers['Idempotency-Key']}});expect(transport.mock.calls[1]).toEqual(original);expect(refreshed.pending()).toBeNull();
});
it('keeps an upload with an invalid receipt and refuses a different command until the original file key is acknowledged',async()=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a'),file=new File(['%PDF-current original'],'Revision.pdf',{type:'application/pdf'});transport.mockResolvedValueOnce(Response.json({data:{id:'actual-share'}}));await expect(send.upload(portalSource,file)).rejects.toThrow('exact shared-document action');await expect(send.command(portalSource,portalInput)).rejects.toThrow('uncertain result');transport.mockImplementationOnce(async(_path,options)=>Response.json({data:{id:options.headers['Idempotency-Key']}}));const result=await send.recover();expect(result?.file).toBe(file);expect(transport.mock.calls[1][1]).toEqual(transport.mock.calls[0][1]);expect(send.pending()).toBeNull();
});
it('refuses persisted cross-origin or other-share upload paths before public retry can send private draft data',async()=>{
 const transport=portalTransport();storage.set('erpx:legal-portal:account-session-a',JSON.stringify({kind:'command',path:'https://untrusted.invalid/collect',key:crypto.randomUUID(),source:portalSource,body:JSON.stringify(portalInput),uncertain:true}));const send=createLegalPortalCommandClient(portalBase,'account-session-a');expect(send.pending()).toBeNull();expect(await send.recover()).toBeNull();expect(transport).not.toHaveBeenCalled();
});

async function uploadOutcome(options:RequestInit){
 if(!(options.body instanceof File))throw Error('The exact retained upload File is required');
 const file=options.body,key=new Headers(options.headers).get('Idempotency-Key');if(!key)throw Error('The original upload key is required');
 const hex=(bytes:ArrayBuffer)=>Array.from(new Uint8Array(bytes),byte=>byte.toString(16).padStart(2,'0')).join(''),checksum=hex(await crypto.subtle.digest('SHA-256',await file.arrayBuffer())),requestFingerprint=hex(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(legalPortalUploadFingerprintInput({shareId:portalSource.shareId,name:file.name,mediaType:file.type,bytes:file.size,checksum}))));
 return {id:key,requestFingerprint,outcome:'not-applied'};
}
it('releases only a parsed exact owner not-applied outcome while retaining the selected file for an explicit new request',async()=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a'),file=new File(['%PDF-retained original'],'Review.pdf',{type:'application/pdf'});transport.mockRejectedValueOnce(new TypeError('lost response'));await expect(send.upload(portalSource,file)).rejects.toThrow('lost response');const first=transport.mock.calls[0];transport.mockImplementationOnce(async(_path:string,options:RequestInit)=>Response.json({data:{uploadOutcome:await uploadOutcome(options)}}));await expect(send.recover()).rejects.toThrow('not applied');expect(send.pending()).toBeNull();expect(storage.size).toBe(0);expect(await file.text()).toBe('%PDF-retained original');expect(transport.mock.calls[1]).toEqual(first);
 transport.mockImplementationOnce(async(_path:string,options:RequestInit)=>Response.json({data:{id:new Headers(options.headers).get('Idempotency-Key')}}));expect((await send.upload(portalSource,file)).file).toBe(file);expect(transport.mock.calls[2][1].headers['Idempotency-Key']).not.toBe(first[1].headers['Idempotency-Key']);
});
it.each(['key','fingerprint','outcome','missing','extra'] as const)('retains unknown upload work for a mismatched or malformed not-applied proof (%s)',async mismatch=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a'),file=new File(['%PDF-original'],'Retained.pdf',{type:'application/pdf'});transport.mockRejectedValueOnce(new TypeError('lost response'));await expect(send.upload(portalSource,file)).rejects.toThrow('lost response');transport.mockImplementationOnce(async(_path:string,options:RequestInit)=>{const proof:Record<string,unknown>={...await uploadOutcome(options)};if(mismatch==='key')proof.id='another-request';if(mismatch==='fingerprint')proof.requestFingerprint='another-body';if(mismatch==='outcome')proof.outcome='applied';if(mismatch==='missing')delete proof.requestFingerprint;if(mismatch==='extra')proof.privateSource={object_key:'must-never-be-a-proof'};return Response.json({data:{uploadOutcome:proof}});});await expect(send.recover()).rejects.toThrow('could not be confirmed');expect(send.pending()).toMatchObject({kind:'upload',shareId:portalSource.shareId});await expect(send.command(portalSource,portalInput)).rejects.toThrow('uncertain result');expect(transport).toHaveBeenCalledTimes(2);
});
it('keeps an ordinary unknown409 even if its error body resembles an upload outcome',async()=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a'),file=new File(['%PDF-original'],'Retained.pdf',{type:'application/pdf'});transport.mockRejectedValueOnce(new TypeError('lost response'));await expect(send.upload(portalSource,file)).rejects.toThrow('lost response');transport.mockImplementationOnce(async(_path:string,options:RequestInit)=>Response.json({data:{uploadOutcome:await uploadOutcome(options)}},{status:409}));await expect(send.recover()).rejects.toThrow('not acknowledged');expect(send.pending()).toBeTruthy();
});
it('does not accept an upload outcome as confirmation of an unknown comment',async()=>{
 const transport=portalTransport(),send=createLegalPortalCommandClient(portalBase,'account-session-a');transport.mockRejectedValueOnce(new TypeError('lost response'));await expect(send.command(portalSource,portalInput)).rejects.toThrow('lost response');transport.mockResolvedValueOnce(Response.json({data:{uploadOutcome:{id:'unrelated',requestFingerprint:'unrelated',outcome:'not-applied'}}}));await expect(send.recover()).rejects.toThrow('could not be confirmed');expect(send.pending()).toMatchObject({kind:'command',shareId:portalSource.shareId});
});

import {createLegalDocumentUploadClient,legalDocumentUploadAcknowledged} from '@erpx/feature-legal/lib/document-upload-client';
import {legalCommandAcknowledged} from '@erpx/feature-shared/lib/legal/command-acknowledgement';
it.each([{id:'another-record',version:8},{id:'original-record',version:7},{id:'original-record'},{},null])('retains an internal Legal mutation until its exact record/version is acknowledged (%j)',async data=>{
 workFetch.mockResolvedValueOnce(Response.json({data}));const send=createLegalCommandClient('actual-work-context',{strictOwnerReceipt:true}),body={action:'update',id:'original-record',expectedVersion:7,item:{title:'Submitted original'}};await expect(send(path,body)).rejects.toThrow('did not acknowledge');expect(send.hasPending()).toBe(true);expect(workFetch.mock.calls[0][3]).toBe('caller');workFetch.mockResolvedValueOnce(Response.json({data:{id:'original-record',version:8}}));expect(await send.recover()).toMatchObject({input:body,data:{id:'original-record',version:8}});expect(workFetch.mock.calls[1][1]).toEqual(workFetch.mock.calls[0][1]);
});
it('keeps native Legal preview errors, batch item receipts and queued inspection contracts distinct from record mutation receipts',()=>{
 expect(legalCommandAcknowledged(path,{action:'import.preview',rows:[{},{}]},{rows:[{}],errors:[{row:2,message:'Original invalid row'}],count:2,digest:'actual-preview-digest'},true)).toBe(true);expect(legalCommandAcknowledged(path,{action:'import.commit',rows:[{},{}]},{items:[{id:'only-one',version:1}]},true)).toBe(false);expect(legalCommandAcknowledged(path,{action:'import.commit',rows:[{},{}]},{items:[{id:'one',version:1},{id:'two',version:1}]},true)).toBe(true);expect(legalCommandAcknowledged(path+'/documents/original/verify',{}, {id:'record',jobId:'queued-original',state:'queued'},true)).toBe(true);expect(legalCommandAcknowledged(path+'/documents/original/verify',{}, {id:'record',state:'queued'},true)).toBe(false);expect(legalCommandAcknowledged(path,{action:'create'},{id:'record',version:7},true)).toBe(false);
});
it('publishes no saved-form event before parsed owner acknowledgement and retains the original submitted edit revision through uncertain recovery',async()=>{
 let revision='submitted',finish!:(response:Response)=>void;const form={getAttribute:()=>revision},dispatchEvent=vi.fn();vi.stubGlobal('window',{dispatchEvent});vi.stubGlobal('document',{activeElement:{closest:()=>form}});const send=createLegalCommandClient('actual-work-context',{strictOwnerReceipt:true}),body={action:'update',id:'original-record',expectedVersion:7};workFetch.mockReturnValueOnce(new Promise<Response>(resolve=>{finish=resolve;}));const pending=send(path,body);revision='later-unsent-edit';finish(Response.json({data:{id:'wrong-record',version:8}}));await expect(pending).rejects.toThrow('did not acknowledge');expect(dispatchEvent).not.toHaveBeenCalled();workFetch.mockResolvedValueOnce(Response.json({data:{id:'original-record',version:8}}));await send.recover();const saved=dispatchEvent.mock.calls.map(([event])=>event as CustomEvent).find(event=>event.type==='erpx:record-command-saved');expect(saved?.detail).toEqual({form,formEditVersion:'submitted'});expect(revision).toBe('later-unsent-edit');
});
it('recovers an internal upload after refresh using its original record version/key and exact same bytes without persisting file content',async()=>{
 const file=new File(['%PDF-original private file'],'Original.pdf',{type:'application/pdf'}),send=createLegalDocumentUploadClient(path,'actual-work-context');workFetch.mockRejectedValueOnce(new TypeError('lost after file commit'));await expect(send.upload({id:'original-record',version:7},file)).rejects.toThrow('lost');const original=workFetch.mock.calls[0];expect(original[3]).toBe('caller');expect([...storage.values()].join('')).not.toContain('private file');const refreshed=createLegalDocumentUploadClient(path,'actual-work-context');expect(refreshed.pending()).toMatchObject({id:'original-record',version:7,needsFile:true});await expect(refreshed.recover()).rejects.toThrow('Reselect the same');const changed=new File(['%PDF-changed! private file'],file.name,{type:file.type});expect(changed.size).toBe(file.size);await expect(refreshed.recover(changed)).rejects.toThrow('original file');expect(workFetch).toHaveBeenCalledTimes(1);workFetch.mockResolvedValueOnce(Response.json({data:{id:'original-record',version:8,documentId:'immutable-document'}}));expect(await refreshed.recover(file)).toEqual({id:'original-record',version:8,documentId:'immutable-document'});expect(workFetch.mock.calls[1]).toEqual(original);expect(refreshed.pending()).toBeNull();expect(createLegalDocumentUploadClient(path,'other-work-context').pending()).toBeNull();
});
it.each([{id:'other',version:8,documentId:'doc'},{id:'original-record',version:7,documentId:'doc'},{id:'original-record',version:8}])('keeps the original internal file and key when an upload receipt is incomplete or foreign (%j)',async data=>{
 const file=new File(['%PDF-current original'],'Original.pdf',{type:'application/pdf'}),send=createLegalDocumentUploadClient(path,'actual-work-context');workFetch.mockResolvedValueOnce(Response.json({data}));await expect(send.upload({id:'original-record',version:7},file)).rejects.toThrow('exact Legal document upload');expect(send.pending()).toBeTruthy();await expect(send.upload({id:'original-record',version:8},file)).rejects.toThrow('original file');workFetch.mockResolvedValueOnce(Response.json({data:{id:'original-record',version:8,documentId:'immutable-document'}}));await send.recover();expect(workFetch.mock.calls[1][1]).toEqual(workFetch.mock.calls[0][1]);
});
it('acknowledges an executed import only with the original item, next version and both immutable document/copy identities',()=>{
 const source={id:'original-contract',version:7},receipt={id:source.id,version:8,documentId:'signed-original',executedCopyId:'independently-reviewable-copy'};expect(legalDocumentUploadAcknowledged(receipt,source,true)).toBe(true);for(const data of [{...receipt,id:'other-contract'},{...receipt,version:9},{...receipt,executedCopyId:undefined},{...receipt,documentId:''},null])expect(legalDocumentUploadAcknowledged(data,source,true)).toBe(false);
});

import * as React from 'react';
import {Button,Field,InlineNotice,Select,TextArea,TextInput} from '@erpx/ui/components/foundation';
import {DraftFormDrawer} from '@erpx/ui/components/draft-form-drawer';
async function actualCounselBillingDrawer(){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),text=await readFile(new URL("../../../packages/feature-legal/src/components/legal-portal.tsx",import.meta.url),'utf8'),tree=ts.createSourceFile('actual-counsel-billing.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),owner=tree.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='LegalPortal');if(!owner||!ts.isFunctionDeclaration(owner)||!owner.body)throw Error('Original LegalPortal required');
 const nodes=owner.body.statements.filter(ts.isFunctionDeclaration).filter(node=>['acknowledged','act','recover'].includes(node.name?.text??''));expect(nodes).toHaveLength(3);let drawerNode:import('typescript').JsxElement|undefined,triggerNode:import('typescript').JsxElement|undefined;const visit=(node:import('typescript').Node)=>{if(ts.isJsxElement(node)&&node.openingElement.tagName.getText(tree)==='DraftFormDrawer')drawerNode=node;if(ts.isJsxElement(node)&&node.openingElement.tagName.getText(tree)==='Button'&&node.children.some(child=>ts.isJsxText(child)&&child.text.trim()==='Submit time or fees for review'))triggerNode=node;ts.forEachChild(node,visit);};visit(owner);if(!drawerNode)throw Error('Original counsel billing drawer required');
 const current=legalPortalSnapshot({...sharedSource,kind:'counsel'}),send=createLegalPortalCommandClient('/api/legal-portal/h001','counsel-original-session'),state={busy:false,error:'',notice:'',billingOpen:true,pendingAction:null as ReturnType<typeof send.pending>,billing:{entryType:'time',description:'Original reviewed counsel work',minutes:'30',amount:'0',evidence:'Actual original time evidence'}},emptyBilling={entryType:'time',description:'',minutes:'0',amount:'0',evidence:''},selectedRef={current},bodyRef={current:''},billingRef={current:state.billing},fileRef={current:null as File|null},fileInput={current:null as HTMLInputElement|null},alive={current:true},commandInFlight={current:false},open=vi.fn(async(id:string)=>{expect(id).toBe(current.shareId);});
 const compile=(value:string)=>ts.transpileModule(value,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React}}).outputText,
  ports={send,selectedRef,bodyRef,billingRef,fileRef,fileInput,alive,commandInFlight,emptyBilling,open,source:(value:typeof current)=>({shareId:value.shareId,version:value.doc.version,checksum:value.doc.checksum}),setBusy:(value:boolean)=>{state.busy=value;},setError:(value:string)=>{state.error=value;},setNotice:(value:string)=>{state.notice=value;},setPendingAction:(value:typeof state.pendingAction)=>{state.pendingAction=value;},setBillingOpen:(value:boolean)=>{state.billingOpen=value;},editBody:(value:string)=>{bodyRef.current=value;},editFile:(value:File|null)=>{fileRef.current=value;},editBilling:(value:typeof state.billing)=>{state.billing=value;billingRef.current=value;}},handlers=new Function(...Object.keys(ports),compile(nodes.map(node=>node.getText(tree)).join('\n')+'\nreturn {act,recover};'))(...Object.values(ports)) as {act:(action:string,extra:Record<string,string>)=>Promise<boolean>;recover:()=>Promise<void>};
 let completion:Promise<unknown>=Promise.resolve();const runAct=(action:string,extra:Record<string,string>)=>{completion=handlers.act(action,extra);return completion;},runRecover=()=>{completion=handlers.recover();return completion;};
 function drawer(){class OwnerBillingFormData extends FormData{constructor(){super();for(const [key,value]of Object.entries(state.billing))this.append(key,value);}}const renderPorts={React,DraftFormDrawer,Button,Field,Select,TextArea,TextInput,InlineNotice,...state,billingDirty:true,selected:current,file:fileRef.current,busy:state.busy,billing:state.billing,s:{detail:'original-billing-detail',field:'original-billing-field',button:'original-billing-button'},editBilling:ports.editBilling,setBillingOpen:ports.setBillingOpen,emptyBilling,act:runAct,recover:runRecover,FormData:OwnerBillingFormData},result=new Function(...Object.keys(renderPorts),compile('return '+drawerNode!.getText(tree)+';'))(...Object.values(renderPorts)) as React.ReactElement<React.ComponentProps<typeof DraftFormDrawer>>;return {element:result,html:renderToStaticMarkup(React.createElement('section',{},result.props.children))};}
 function find(node:React.ReactNode,predicate:(node:React.ReactElement<Record<string,unknown>>)=>boolean):React.ReactElement<Record<string,unknown>>|undefined{if(Array.isArray(node)){for(const child of node){const hit=find(child,predicate);if(hit)return hit;}return;}if(!React.isValidElement<Record<string,unknown>>(node))return;if(predicate(node))return node;return find(node.props.children as React.ReactNode,predicate);}
 function billingTrigger(){if(!triggerNode)throw Error('Original billing trigger required');const ports={React,Button,pendingAction:state.pendingAction,selected:current,billingDirty:true,setBillingOpen:portsSetter};function portsSetter(value:boolean){state.billingOpen=value;}return new Function(...Object.keys(ports),compile('return '+triggerNode.getText(tree)+';'))(...Object.values(ports)) as React.ReactElement<React.ComponentProps<typeof Button>>;}
 return {state,send,drawer,billingTrigger,open,newer(){ports.editBilling({...state.billing,description:'Newer unsent counsel work',minutes:'45',evidence:'Newer unsent evidence'});},async submit(){const form=find(drawer().element.props.children,node=>node.type==='form');if(!form)throw Error('Original billing form required');(form.props.onSubmit as (event:{preventDefault:()=>void;currentTarget:unknown})=>void)({preventDefault:()=>{},currentTarget:{}});await completion;},async retryInside(){const button=find(drawer().element.props.children,node=>node.type===Button&&node.props.children==='Check previous result');expect(button,'Original retry must be reachable inside the locked billing drawer').toBeDefined();expect(button!.props.disabled).toBe(false);(button!.props.onClick as ()=>void)();await completion;}};
}
it('Legal counsel lost-response billing can retry inside its locked drawer with the original body/key and newer work retained',async()=>{
 const c=await actualCounselBillingDrawer(),requests:{url:string;key:string|null;body:BodyInit|null|undefined}[]=[];vi.stubGlobal('fetch',async(url:RequestInfo|URL,init?:RequestInit)=>{requests.push({url:String(url),key:new Headers(init?.headers).get('Idempotency-Key'),body:init?.body});if(requests.length===1)throw Error('Original counsel response lost');return Response.json({data:{id:sharedSource.shareId}});});
 await c.submit();expect(c.state.pendingAction).toMatchObject({kind:'command',shareId:sharedSource.shareId});expect(c.drawer().element.props.dismissDisabled).toBe(true);expect(c.drawer().html).toContain('Original counsel response lost');c.newer();await c.submit();expect(requests).toHaveLength(1);await c.retryInside();expect(requests[1]).toEqual(requests[0]);expect(JSON.parse(String(requests[1].body))).toMatchObject({action:'billing.submit',shareId:sharedSource.shareId,description:'Original reviewed counsel work',minutes:'30',evidence:'Actual original time evidence'});expect(c.send.pending()).toBeNull();expect(c.state.pendingAction).toBeNull();expect(c.state.billingOpen).toBe(true);expect(c.state.billing.description).toBe('Newer unsent counsel work');expect(c.state.billing.evidence).toBe('Newer unsent evidence');expect(c.drawer().html).toContain('The exact shared-document action was saved.');expect(c.open).toHaveBeenCalledWith(sharedSource.shareId);
});
it('Legal counsel saved acknowledgement remains visible inside the same drawer alongside newer description and evidence',async()=>{const c=await actualCounselBillingDrawer();c.newer();c.state.notice='The exact shared-document action was saved.';const view=c.drawer();expect(view.html).toContain('role="status"');expect(view.html).toContain(c.state.notice);expect(view.html).toContain('Newer unsent counsel work');expect(view.html).toContain('Newer unsent evidence');});
it('Legal Portal cannot open a new billing drawer over an unrelated restored upload that needs original file reselection',async()=>{
 vi.stubGlobal('fetch',vi.fn(async()=>{throw Error('Original upload response lost');}));const first=createLegalPortalCommandClient('/api/legal-portal/h001','counsel-original-session'),file=new File(['%PDF-original counsel upload'],'Original counsel.pdf',{type:'application/pdf'});await expect(first.upload({shareId:sharedSource.shareId,version:sharedSource.doc.version,checksum:sharedSource.doc.checksum},file)).rejects.toThrow('lost');const c=await actualCounselBillingDrawer();c.state.pendingAction=c.send.pending();expect(c.state.pendingAction).toMatchObject({kind:'upload',needsFile:true,name:file.name});const trigger=c.billingTrigger();expect(trigger.props.disabled).toBe(true);expect(renderToStaticMarkup(trigger)).toContain('disabled=""');expect(c.send.pending()).toMatchObject({kind:'upload',needsFile:true});
});

// Exercise the actual mixed-kind queue component and its parent's actual open
// handler. The native current queue reader and React host controls are ports;
// these cases establish handoff/URL behavior, not native permission acceptance.
import {recordDrawerHref} from '@erpx/feature-records/lib/record-drawer';
import {labels as legalQueueLabels,type LegalKind} from '@erpx/server-legal/public/rules/model';
type LegalQueueRow={id:string;kind:LegalKind;title:string;state:string;due_date:string|null;owner_id:string};
type LegalQueueCell={value?:unknown;deps?:readonly unknown[];cleanup?:()=>void};
function legalQueueHooks(){
 const cells:LegalQueueCell[]=[],effects:(()=>void)[]=[];let cursor=0;
 const state=(initial:unknown)=>{const index=cursor++;if(!cells[index])cells[index]={value:initial};return[cells[index].value,(value:unknown)=>{cells[index].value=typeof value==='function'?value(cells[index].value):value;}];};
 const effect=(run:()=>void|(()=>void),deps?:readonly unknown[])=>{const index=cursor++,old=cells[index];if(!old||!deps||!old.deps||deps.length!==old.deps.length||deps.some((part,key)=>!Object.is(part,old.deps?.[key])))effects.push(()=>{old?.cleanup?.();const cleanup=run();cells[index]={deps,cleanup:typeof cleanup==='function'?cleanup:undefined};});};
 return{react:{...React,useState:state,useEffect:effect},render<T>(read:()=>T){cursor=0;const result=read();while(effects.length)effects.shift()!();return result;},close(){for(const cell of cells)cell.cleanup?.();}};
}
function legalQueueNodes(node:React.ReactNode,out:React.ReactElement<Record<string,unknown>>[]=[]){React.Children.forEach(node,child=>{if(!React.isValidElement<Record<string,unknown>>(child))return;out.push(child);legalQueueNodes(child.props.children as React.ReactNode,out);});return out;}
async function actualLegalQueueHandoff(kind:LegalKind,view:'mine'|'deadlines',items:LegalQueueRow[]){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),jsx=await import('react/jsx-runtime'),[workspace,queue]=await Promise.all([readFile(new URL("../../../packages/feature-legal/src/components/legal-workspace.tsx",import.meta.url),'utf8'),readFile(new URL("../../../packages/feature-legal/src/components/legal-work-queue.tsx",import.meta.url),'utf8')]),tree=ts.createSourceFile('actual-legal-workspace.tsx',workspace,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);let openNode:import('typescript').FunctionDeclaration|undefined;
 const visit=(node:import('typescript').Node)=>{if(ts.isFunctionDeclaration(node)&&node.name?.text==='LegalWorkspace'&&node.body){openNode=node.body.statements.find((item):item is import('typescript').FunctionDeclaration=>ts.isFunctionDeclaration(item)&&item.name?.text==='open');}ts.forEachChild(node,visit);};visit(tree);if(!openNode)throw Error('Actual Legal owner open handler required');
 const current=`https://legal.example.test/holdings/acme/services/legal/${view}?q=preserved`,router={push:vi.fn()},ownerPorts={embedded:false,selected:null,kind,router,recordDrawerHref,window:{location:{href:current}}},ownerBody=ts.transpileModule(`${openNode.getText(tree)}\nreturn open;`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,owner=new Function(...Object.keys(ownerPorts),ownerBody)(...Object.values(ownerPorts)) as (id:string,targetKind?:LegalKind)=>Promise<void>,open=vi.fn(owner),call=vi.fn(async()=>({assigned:items,deadlines:items})),hooks=legalQueueHooks(),loaded={exports:{} as Record<string,unknown>},compiled=ts.transpileModule(queue,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}}).outputText;
 const ports=(name:string):unknown=>{if(name==='react')return hooks.react;if(name==='react/jsx-runtime')return jsx;if(name.endsWith("@erpx/ui/components/foundation"))return{Button:'button',InlineNotice:'Notice'};if(name.endsWith("@erpx/server-legal/public/rules/model"))return{labels:legalQueueLabels};if(name==="@erpx/feature-legal/components/legal.module.css")return{default:{detail:'detail',small:'small',event:'event',link:'link'}};throw Error('Unexpected actual Legal work-queue import '+name);};new Function('require','module','exports',compiled)(ports,loaded,loaded.exports);
 const Component=loaded.exports.LegalWorkQueue as (props:{view:'mine'|'deadlines';people:{id:string;name:string}[];call:(path:string)=>Promise<unknown>;open:(id:string,targetKind?:LegalKind)=>Promise<void>})=>React.ReactNode;
 return{router,open,call,current,render:()=>hooks.render(()=>Component({view,people:[],call,open})),close:()=>hooks.close()};
}
it.each(['mine','deadlines'] as const)('actual Legal %s queue preserves each native kind through its parent and canonical record drawer',async view=>{
 const rows:LegalQueueRow[]=[{id:'retained-contract',kind:'contract',title:'Renew supplier agreement',state:'active',due_date:'2026-11-01',owner_id:'owner'},{id:'outside-counsel',kind:'counsel',title:'Review counsel engagement',state:'active',due_date:'2026-11-02',owner_id:'owner'},{id:'retained-obligation',kind:'obligation',title:'Complete regulatory filing',state:'open',due_date:'2026-11-03',owner_id:'owner'}],mounted=await actualLegalQueueHandoff('request',view,rows);
 try{
  mounted.render();await vi.waitFor(()=>expect(legalQueueNodes(mounted.render()).filter(node=>node.type==='button')).toHaveLength(rows.length));expect(mounted.call).toHaveBeenCalledExactlyOnceWith('/workspace');
  for(const [index,row]of rows.entries()){
   const control=legalQueueNodes(mounted.render()).find(node=>node.type==='button'&&node.props.children===row.title);if(!control)throw Error('Actual native queue row control required');const onClick=control.props.onClick;if(typeof onClick!=='function')throw Error('Actual native queue row handler required');Reflect.apply(onClick,undefined,[]);expect(mounted.open).toHaveBeenNthCalledWith(index+1,row.id,row.kind);
   const [href,options]=mounted.router.push.mock.calls[index],url=new URL(String(href),'https://legal.example.test');expect(url.pathname).toBe(`/holdings/acme/services/legal/${row.kind}/${row.id}/overview`);expect(url.searchParams.get('returnTo')).toBe(`/holdings/acme/services/legal/${view}?q=preserved`);expect(options).toEqual({scroll:false});expect(url.searchParams.has('kind')).toBe(false);
  }
 }finally{mounted.close();}
});
it('existing Legal callers that supply only an id keep the current register kind and original return path',async()=>{
 const mounted=await actualLegalQueueHandoff('template','mine',[]);try{await mounted.open('existing-template');expect(mounted.router.push).toHaveBeenCalledOnce();const [href,options]=mounted.router.push.mock.calls[0],url=new URL(String(href),'https://legal.example.test');expect(url.pathname).toBe('/holdings/acme/services/legal/template/existing-template/overview');expect(url.searchParams.get('returnTo')).toBe('/holdings/acme/services/legal/mine?q=preserved');expect(options).toEqual({scroll:false});expect(mounted.call).not.toHaveBeenCalled();}finally{mounted.close();}
});
it('a native current queue denial reports its error without supplying any destination rows',async()=>{
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),text=await readFile(new URL("../../../packages/feature-legal/src/components/legal-work-queue.tsx",import.meta.url),'utf8'),tree=ts.createSourceFile('actual-legal-queue-effect.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),owner=tree.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='LegalWorkQueue');if(!owner||!ts.isFunctionDeclaration(owner)||!owner.body)throw Error('Actual Legal work queue required');const effect=owner.body.statements.filter(ts.isExpressionStatement).map(node=>node.expression).find(node=>ts.isCallExpression(node)&&node.expression.getText(tree)==='useEffect');if(!effect||!ts.isCallExpression(effect)||!effect.arguments[0])throw Error('Actual Legal queue reader effect required');
 const denied=Object.assign(new Error('Current Legal source authority denied'),{status:403,code:'LEGAL_ACCESS_CHANGED'}),call=vi.fn(async()=>{throw denied;}),setData=vi.fn(),setError=vi.fn(),ports={call,setData,setError},emitted=ts.transpileModule(`return (${effect.arguments[0].getText(tree)})();`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,cleanup=new Function(...Object.keys(ports),emitted)(...Object.values(ports)) as ()=>void;
 try{await vi.waitFor(()=>expect(setError).toHaveBeenCalledExactlyOnceWith(denied.message));expect(call).toHaveBeenCalledExactlyOnceWith('/workspace');expect(setData).not.toHaveBeenCalled();}finally{cleanup();}
});
