import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ATTACHMENT_MAX_BYTES,
  attachmentApiBase,
  attachmentMediaType,
  formatAttachmentBytes,
  publicMediaPublicationUrl,
  uploadAttachmentBytes,
  validateGenericAttachment,
} from "@erpx/feature-files/lib/attachment-client";

afterEach(()=>vi.unstubAllGlobals());

describe("attachment client policy", () => {
  it("maps holding workspace paths to their authenticated API scope", () => {
    expect(attachmentApiBase("/holdings/acme")).toBe("/api/v1/holdings/acme");
    expect(attachmentApiBase("/holdings/acme/branches/bkk/")).toBe(
      "/api/v1/holdings/acme/branches/bkk",
    );
    expect(attachmentApiBase("/api/v1/holdings/acme")).toBe("/api/v1/holdings/acme");
  });

  it("accepts only generic image and PDF formats within ten MiB", () => {
    expect(validateGenericAttachment({ name: "receipt.heic", type: "", size: 1234 })).toBeNull();
    expect(attachmentMediaType({ name: "receipt.heic", type: "" })).toBe("image/heic");
    expect(validateGenericAttachment({ name: "scan.pdf", type: "application/pdf", size: ATTACHMENT_MAX_BYTES })).toBeNull();
    expect(validateGenericAttachment({ name: "sheet.csv", type: "text/csv", size: 1234 })).toContain("JPEG");
    expect(validateGenericAttachment({ name: "large.png", type: "image/png", size: ATTACHMENT_MAX_BYTES + 1 })).toContain("10 MB");
    expect(validateGenericAttachment({ name: "empty.pdf", type: "application/pdf", size: 0 })).toContain("empty");
  });

  it("formats file sizes for accessible status text", () => {
    expect(formatAttachmentBytes(12)).toBe("12 B");
    expect(formatAttachmentBytes(2048)).toBe("2 KB");
    expect(formatAttachmentBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  });

  it("keeps approved derivatives on the explicit public publication route", () => {
    expect(publicMediaPublicationUrl("/holdings/acme", "publication-1")).toBe(
      "/api/public/holdings/acme/media-publications/publication-1/content",
    );
    expect(() => publicMediaPublicationUrl("/customer-portal/acme", "publication-1")).toThrow(
      "holding workspace",
    );
  });

  it("preserves the explicit scan-pending acceptance status from canonical PUT",async()=>{
    vi.stubGlobal("fetch",vi.fn().mockResolvedValue(Response.json({data:{uploadIntentId:"upload-1",attachmentLinkId:"link-1",sourceId:"source-1",state:"quarantined",status:"scan_pending"}},{status:202})));
    const result=await uploadAttachmentBytes("/api/v1/holdings/acme/uploads/upload-1/content",new File([new Uint8Array([1])],"evidence.png",{type:"image/png"}),"image/png");
    expect(result).toEqual({uploadIntentId:"upload-1",attachmentLinkId:"link-1",sourceId:"source-1",state:"quarantined",status:"scan_pending"});
  });
});

it('retries the exact selected Field withdrawal after a lost response and remount',async()=>{
 const {fieldWithdrawalScope,registerFieldWithdrawalOwner,submitFieldWithdrawal,retryFieldWithdrawal,pendingFieldWithdrawal}=await import('@erpx/feature-files/lib/native-field-withdraw');
 const node={isConnected:true,dataset:{workUser:'worker',workRevision:'7',workCompany:'company',workAssignment:'grant',workAssignmentVersion:'3'}};const emitted:string[]=[];
 vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'?node:null,documentElement:{dataset:{}},activeElement:null});vi.stubGlobal('window',{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/service'},dispatchEvent:(event:Event)=>{emitted.push(event.type);}});vi.stubGlobal('CustomEvent',class extends Event{detail:unknown;constructor(type:string,init:{detail:unknown}){super(type);this.detail=init.detail;}});
 const calls:{body:unknown;key:string|null}[]=[];vi.stubGlobal('fetch',vi.fn(async(_url:unknown,init:RequestInit)=>{calls.push({body:init.body,key:new Headers(init.headers).get('idempotency-key')});if(calls.length===1)throw Error('lost committed response');return Response.json({data:{linkId:'link',setId:'set',sourceId:'new-source',sourceState:'available',linkState:'withdrawn',scan:{attempts:0,lastResult:null,lastError:null,signature:null,definitionUpdatedAt:null,scannedAt:null},previewAvailable:false,bindable:false,expiresAt:100,boundAt:null,withdrawnAt:10}});}));
 const scope=fieldWithdrawalScope('/holdings/acme','field_service_visit','order','record',null,'site_photo');let unregister=registerFieldWithdrawalOwner(scope,()=>({setId:'set'}));const input={scope,base:'/holdings/acme',linkId:'link',setId:'set',reason:'Original reason',draftReason:'Original reason'};
 await expect(submitFieldWithdrawal(input)).rejects.toThrow('lost committed response');expect(pendingFieldWithdrawal(scope)).toBeDefined();node.dataset.workUser='other';expect(fieldWithdrawalScope('/holdings/acme','field_service_visit','order','record',null,'site_photo')).not.toBe(scope);expect(pendingFieldWithdrawal(scope)).toBeUndefined();await expect(retryFieldWithdrawal(scope)).rejects.toThrow('role changed');node.dataset.workUser='worker';await expect(submitFieldWithdrawal({...input,reason:'Newer reason'})).rejects.toThrow('original withdrawal');unregister();unregister=registerFieldWithdrawalOwner(scope,()=>({setId:'different-newer-set'}));
 const status=await retryFieldWithdrawal(scope);expect(status.linkState).toBe('withdrawn');expect(calls).toHaveLength(2);expect(calls[1]).toEqual(calls[0]);expect(pendingFieldWithdrawal(scope)).toBeUndefined();expect(emitted).toContain('erpx:record-command-saved');unregister();
});

it('keeps a late withdrawal response pending after a Work account change',async()=>{
 const {fieldWithdrawalScope,registerFieldWithdrawalOwner,submitFieldWithdrawal,pendingFieldWithdrawal}=await import('@erpx/feature-files/lib/native-field-withdraw');const node={isConnected:true,dataset:{workUser:'worker',workRevision:'7',workCompany:'company',workAssignment:'grant',workAssignmentVersion:'3'}};
 vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'?node:null,documentElement:{dataset:{}},activeElement:null});vi.stubGlobal('window',{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/service'},dispatchEvent:()=>{}});let release!:(value:Response)=>void;vi.stubGlobal('fetch',()=>new Promise<Response>(resolve=>{release=resolve;}));
 const scope=fieldWithdrawalScope('/holdings/acme','field_service_visit','other-order','record',null,'site_photo'),unregister=registerFieldWithdrawalOwner(scope,()=>({setId:'set'}));const pending=submitFieldWithdrawal({scope,base:'/holdings/acme',linkId:'link',setId:'set',reason:'Original reason',draftReason:'Original reason'});node.dataset.workUser='other';release(Response.json({data:{linkId:'link',setId:'set',sourceId:null,sourceState:null,linkState:'withdrawn',scan:{attempts:0,lastResult:null,lastError:null,signature:null,definitionUpdatedAt:null,scannedAt:null},previewAvailable:false,bindable:false,expiresAt:100,boundAt:null,withdrawnAt:10}}));await expect(pending).rejects.toThrow();expect(pendingFieldWithdrawal(scope)).toBeUndefined();node.dataset.workUser='worker';expect(pendingFieldWithdrawal(scope)).toBeDefined();unregister();
});

it('keeps an in-flight withdrawal pending when its panel remounts before the old response',async()=>{
 const {fieldWithdrawalScope,registerFieldWithdrawalOwner,submitFieldWithdrawal,retryFieldWithdrawal,pendingFieldWithdrawal}=await import('@erpx/feature-files/lib/native-field-withdraw');const node={isConnected:true,dataset:{workUser:'worker',workRevision:'7',workCompany:'company',workAssignment:'grant',workAssignmentVersion:'3'}};vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'?node:null,documentElement:{dataset:{}},activeElement:null});vi.stubGlobal('window',{location:{origin:'https://acme.example.test',host:'acme.example.test',href:'https://acme.example.test/service'},dispatchEvent:()=>{}});vi.stubGlobal('CustomEvent',class extends Event{detail:unknown;constructor(type:string,init:{detail:unknown}){super(type);this.detail=init.detail;}});const status={linkId:'link',setId:'old-set',sourceId:null,sourceState:null,linkState:'withdrawn',scan:{attempts:0,lastResult:null,lastError:null,signature:null,definitionUpdatedAt:null,scannedAt:null},previewAvailable:false,bindable:false,expiresAt:100,boundAt:null,withdrawnAt:10};let release!:(value:Response)=>void;const calls:string[]=[];vi.stubGlobal('fetch',vi.fn((_url:unknown,init:RequestInit)=>{calls.push(new Headers(init.headers).get('idempotency-key')!);return calls.length===1?new Promise<Response>(resolve=>{release=resolve;}):Promise.resolve(Response.json({data:status}));}));
 const scope=fieldWithdrawalScope('/holdings/acme','field_service_visit','remount-order','record',null,'site_photo');let unregister=registerFieldWithdrawalOwner(scope,()=>({setId:'old-set'}));const original=submitFieldWithdrawal({scope,base:'/holdings/acme',linkId:'link',setId:'old-set',reason:'Original reason',draftReason:'Original reason'});unregister();unregister=registerFieldWithdrawalOwner(scope,()=>({setId:'newer-set'}));release(Response.json({data:status}));await expect(original).rejects.toThrow('retry confirmation');expect(pendingFieldWithdrawal(scope)).toBeDefined();expect((await retryFieldWithdrawal(scope)).setId).toBe('old-set');expect(calls[1]).toBe(calls[0]);expect(pendingFieldWithdrawal(scope)).toBeUndefined();unregister();
});
