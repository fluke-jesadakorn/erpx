import { beforeEach, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  calls: [] as string[],
  withdraw: vi.fn(),
  status: vi.fn(),
  native: vi.fn(),
  write: { name: "holding-write" },
}));

vi.mock("@erpx/server-files/public/attachments/native-field-attachment",()=>({nativeFieldAttachment:state.native}));

vi.mock("@erpx/server-files/public/attachments/service", () => ({
  withdrawAttachmentLink: state.withdraw,
  attachmentLinkStatus: state.status,
}));

vi.mock("@erpx/server-identity_access/public/authorization", () => ({
  requireWorkContext: async () => ({ holdingId: "h001", companyId: "acme-co", userId: "requester" }),
}));

vi.mock("@erpx/server-manage_erpx/public/public-origin", () => ({
  requestPublicOrigin: () => "https://dev-erpx.example.test",
}));

vi.mock("@erpx/server-runtime/public/runtime", () => ({
  holdingResources: async () => ({ write: state.write }),
}));

import { POST } from "@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/withdraw/route";
import { ApiError } from "@erpx/server-runtime/public/http";

beforeEach(() => {
  state.calls = [];
  state.native.mockReset().mockResolvedValue(undefined);
  state.withdraw.mockReset().mockImplementation(async () => {
    state.calls.push("withdraw");
    return { id: "link-1", state: "withdrawn" };
  });
  state.status.mockReset().mockImplementation(async () => {
    state.calls.push("status");
    return {
      linkId: "link-1",
      setId: "set-1",
      sourceId: "source-1",
      sourceState: "available",
      linkState: "withdrawn",
      scan: { attempts: 1, lastResult: "clean", lastError: null, scannedAt: 10 },
      previewAvailable: true,
      bindable: false,
      expiresAt: 20,
      boundAt: null,
      withdrawnAt: 30,
    };
  });
});

it("returns the post-withdraw status contract consumed by AttachmentPanel", async () => {
  const response = await POST(
    new Request("https://dev-erpx.example.test/api/v1/holdings/acme/attachment-links/link-1/withdraw", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Idempotency-Key": "withdraw-001",
        Origin: "https://dev-erpx.example.test",
      },
      body: JSON.stringify({ reason: "Replace the incorrect photo" }),
    }),
    { params: Promise.resolve({ holdingSlug: "acme", linkId: "link-1" }) },
  );

  expect(response.status).toBe(200);
  expect(response.headers.get("cache-control")).toBe("private, no-store");
  expect(state.calls).toEqual(["withdraw", "status"]);
  expect(state.withdraw).toHaveBeenCalledWith(
    state.write,
    expect.objectContaining({ holdingId: "h001", companyId: "acme-co", userId: "requester" }),
    "link-1",
    "Replace the incorrect photo",
    "withdraw-001",
  );
  expect(state.status).toHaveBeenCalledWith(
    state.write,
    expect.objectContaining({ holdingId: "h001" }),
    "link-1",
  );
  await expect(response.json()).resolves.toMatchObject({
    data: { linkId: "link-1", sourceState: "available", linkState: "withdrawn", withdrawnAt: 30 },
  });
});

it('selects only a configured link and validates native owner context and full post-withdraw status',async()=>{
 const {createNativeFieldAttachmentBridge}=await import('../../../packages/server-files/src/adapters/attachments/native-field-attachment');
 const actor={userId:'requester',holdingId:'h001',holdingSlug:'acme',companyId:'acme-co',branchId:null,departmentId:null,assignmentId:'grant',role:'worker',assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},modules:['field_service'],iam:{permissions:['records.read','records.write']}} as unknown as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const context={userId:'requester',holdingId:'h001',companyId:'acme-co',branchId:null,departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'grant',assignmentVersion:3,revision:7,role:'worker',assignmentScopeKind:'holding',modules:['field_service'],permissions:['records.read','records.write'],expiresAt:1800000000000};const status={linkId:'link-1',setId:'set-1',sourceId:'source-1',sourceState:'quarantined',linkState:'withdrawn',scan:{attempts:0,lastResult:null,lastError:null,signature:null,definitionUpdatedAt:null,scannedAt:null},previewAvailable:false,bindable:false,expiresAt:100,boundAt:null,withdrawnAt:30};
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://dev-erpx.example.test',holdingSlug:'acme',holdingId:'h001',linkIds:['link-1']}]};const calls:{path:string;method:string;key:string|null;body:string|undefined}[]=[];const bridge=createNativeFieldAttachmentBridge(JSON.stringify(config),async(url,method,headers,body)=>{calls.push({path:url.pathname,method,key:headers.get('idempotency-key'),body});return Response.json({data:status,context});});const incoming=new Headers({host:'dev-erpx.example.test',cookie:'opaque'});
 expect(await bridge(incoming,actor,'other',{key:'withdraw-001',body:'{"reason":"Original"}'})).toBeUndefined();expect(calls).toHaveLength(0);expect(await bridge(incoming,actor,'link-1',{key:'withdraw-001',body:'{"reason":"Original"}'})).toEqual(status);expect(calls[0]).toEqual({path:'/api/v1/holdings/acme/attachment-links/link-1/withdraw',method:'POST',key:'withdraw-001',body:'{"reason":"Original"}'});
 await bridge(incoming,actor,'link-1');expect(calls[1]?.path).toBe('/api/v1/holdings/acme/attachment-links/link-1/status');const stale=createNativeFieldAttachmentBridge(JSON.stringify(config),async()=>Response.json({data:status,context:{...context,userId:'other'}}));await expect(stale(incoming,actor,'link-1')).rejects.toMatchObject({status:409});
});


it('dispatches a selected native withdrawal only after Origin, key, actor and body validation',async()=>{
 state.native.mockResolvedValue({linkId:'link-1',setId:'set-1',linkState:'withdrawn'});
 const request=(origin:string,key:string,body:string)=>new Request('https://dev-erpx.example.test/api/v1/holdings/acme/attachment-links/link-1/withdraw',{method:'POST',headers:{origin,'Idempotency-Key':key,'content-type':'application/json'},body});const ctx={params:Promise.resolve({holdingSlug:'acme',linkId:'link-1'})};
 expect((await POST(request('https://foreign.example.test','withdraw-001','{"reason":"Original"}'),ctx)).status).toBe(403);expect((await POST(request('https://dev-erpx.example.test','bad','{"reason":"Original"}'),ctx)).status).toBe(422);expect((await POST(request('https://dev-erpx.example.test','withdraw-001','{"reason":"x"}'),ctx)).status).toBe(422);expect(state.native).not.toHaveBeenCalled();
 const body='{"reason":"Original reason"}';const response=await POST(request('https://dev-erpx.example.test','withdraw-001',body),ctx);expect(response.status).toBe(200);expect(state.native).toHaveBeenCalledWith(expect.any(Headers),expect.objectContaining({holdingId:'h001'}),'link-1',{key:'withdraw-001',body});expect(state.withdraw).not.toHaveBeenCalled();expect(state.status).not.toHaveBeenCalled();
});

it('accepts a representable physical link ID longer than 200 bytes in the native selector',async()=>{
 const {createNativeFieldAttachmentBridge}=await import('../../../packages/server-files/src/adapters/attachments/native-field-attachment');const id='link-'+ 'x'.repeat(210);const actor={userId:'requester',holdingId:'h001',holdingSlug:'acme',companyId:'acme-co',branchId:null,departmentId:null,assignmentId:'grant',role:'worker',assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},modules:['field_service'],iam:{permissions:['records.read']}} as unknown as import('@erpx/server-identity_access/public/authorization').WorkContext;let path='';const bridge=createNativeFieldAttachmentBridge(JSON.stringify({privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://dev-erpx.example.test',holdingSlug:'acme',holdingId:'h001',linkIds:[id]}]}),async url=>{path=url.pathname;return new Response(null,{status:404});});await expect(bridge(new Headers({host:'dev-erpx.example.test'}),actor,id)).rejects.toMatchObject({status:404});expect(path).toBe(`/api/v1/holdings/acme/attachment-links/${id}/status`);
});

it("does not fall back to the retained owner when the selected native withdrawal fails", async () => {
  state.native.mockRejectedValue(new ApiError(503, "NATIVE_ATTACHMENT_UNAVAILABLE", "Unavailable"));
  const response = await POST(new Request("https://dev-erpx.example.test/api/v1/holdings/acme/attachment-links/link-1/withdraw", {
    method: "POST",
    headers: { Origin: "https://dev-erpx.example.test", "Idempotency-Key": "withdraw-001", "Content-Type": "application/json" },
    body: JSON.stringify({ reason: "Original reason" }),
  }), { params: Promise.resolve({ holdingSlug: "acme", linkId: "link-1" }) });
  expect(response.status).toBe(503);
  expect(state.withdraw).not.toHaveBeenCalled();
  expect(state.status).not.toHaveBeenCalled();
});
