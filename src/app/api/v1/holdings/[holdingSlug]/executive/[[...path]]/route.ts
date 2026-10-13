import {appExecutiveCacheRevisions} from "@/server/modules/work_projection/public/app-executive-cache-source";
import { after } from "next/server";
import { z } from "zod";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { filtersSchema, generateSchema, metricIds, shareSchema, sharePreviewSchema, transferOwnershipSchema } from "@/lib/shared/executive/canvas";
import { createReport, reportLibrary, reportView, reportData, reportAccess, saveReport, saveSchema, saveShare, revokeShare, recipients, restoreReport, recipientPreviewContexts, previewShare, ownershipCandidates, transferReportOwnership } from "@/server/modules/runtime/public/executive/reports";
import { queueGeneration, generationHistory, cancelGeneration, drainExecutiveGeneration } from "@/server/modules/runtime/public/executive/generation";
import { executiveAiContext } from "@/server/modules/runtime/public/executive/context";
import { requireAuthor } from "@/server/modules/runtime/public/executive/access";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { readExecutiveEvidence, searchExecutiveEvidence } from "@/server/modules/runtime/public/executive/evidence";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={"Cache-Control":"private, no-store"};
export async function GET(request:Request,context:Context) {
  try {
    const {holdingSlug,path=[]}=await context.params,a=await requireWorkContext(holdingSlug),{write}=await holdingResources(a.holdingId),q=new URL(request.url).searchParams;
    const [id,action,recordId]=path;let data:unknown;
    if(!id)data=await reportLibrary(write,a);
    else if(id==="evidence"&&!action)data=await searchExecutiveEvidence(write,a,{
      metric:q.get("metric"),
      filters:filtersSchema.parse({from:q.get("from"),to:q.get("to"),currency:q.get("currency")}),
      query:q.get("query")??"",
      cursor:q.get("cursor")??undefined,
      limit:q.get("limit")??25,
    });
    else if(id==="evidence"&&action&&recordId)data=await readExecutiveEvidence(write,a,z.enum(metricIds).parse(action),recordId,
      filtersSchema.parse({from:q.get("from"),to:q.get("to"),currency:q.get("currency")}));
    else if(!action)data=await reportView(write,a,id);
    else if(action==="data")data=await reportData(write,a,id,q.has("from")?filtersSchema.parse(Object.fromEntries(q)):undefined);
    else if(action==="context"){await requireAuthor(write,a);const report=await reportAccess(write,a,id);if(!report.canEdit)throw new ApiError(403,"REPORT_READ_ONLY","Only report authors can inspect AI context.");data=await executiveAiContext(write,a,report.filters);}
    else if(action==="jobs")data=await generationHistory(write,a,id);
    else if(action==="recipients")data=await recipients(write,a,id,(q.get("q")??"").slice(0,100));
    else if(action==="preview-contexts")data=await recipientPreviewContexts(write,a,id,q.get("recipientId")??"");
    else if(action==="transfer-candidates")data=await ownershipCandidates(write,a,id);
    else if(action==="events") {
      await reportAccess(write,a,id);
      let timer:ReturnType<typeof setTimeout>|undefined,closed=false;
      const encoder=new TextEncoder(),started=Date.now();
      const stream=new ReadableStream({async start(controller) {
        const stop=()=>{if(closed)return;closed=true;if(timer)clearTimeout(timer);controller.close();};
        request.signal.addEventListener("abort",stop,{once:true});
        let last=request.headers.get("Last-Event-ID")??"";
        const tick=async()=>{if(closed)return;try{
          const access=await reportAccess(write,a,id);
          if(access.share?.mode==="distribution")await requireAuthor(write,JSON.parse(access.share.publisher_json) as WorkContext,"executive.data.distribute");
          // Transactional table watermarks cover commands, jobs and direct SQL. Reconnect always reconciles the full snapshot.
          const revisions=await appExecutiveCacheRevisions(write);
          const revision=await sha256(JSON.stringify([revisions,access.r.version,access.share?.version]));
          if(!closed){controller.enqueue(encoder.encode(revision!==last?`id: ${revision}\nevent: revision\ndata: {}\n\n`:`: heartbeat\n\n`));last=revision;}
          if(Date.now()-started>50000){stop();return;}
          if(!closed)timer=setTimeout(()=>void tick(),3000);
        }catch(error){if(!closed){const denied=error instanceof ApiError&&[401,403,404].includes(error.status);controller.enqueue(encoder.encode(denied?'event: revoked\ndata: {}\n\n':'event: reconnect\ndata: {}\n\n'));stop();}}};
        await tick();
      },cancel(){closed=true;if(timer)clearTimeout(timer);}});
      return new Response(stream,{headers:{...headers,"Content-Type":"text/event-stream","X-Accel-Buffering":"no","Connection":"keep-alive"}});
    }else throw new ApiError(404,"NOT_FOUND","This Executive resource does not exist.");
    return Response.json({data},{headers});
  }catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:Context) {
  try {
    if(request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key=request.headers.get("Idempotency-Key");if(!key||!/^[a-zA-Z0-9_-]{8,100}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","Use a valid Idempotency-Key.");
    const reader=request.body?.getReader();if(!reader)throw new ApiError(422,"INVALID_JSON","A request body is required.");
    let body="",bytes=0;const decoder=new TextDecoder();
    while(true){const p=await reader.read();if(p.done)break;bytes+=p.value.byteLength;if(bytes>256000){await reader.cancel();throw new ApiError(413,"BODY_TOO_LARGE","This report request is too large.");}body+=decoder.decode(p.value,{stream:true});}
    let raw:unknown;try{raw=JSON.parse(body+decoder.decode());}catch{throw new ApiError(422,"INVALID_JSON","Use valid JSON.");}
    const {holdingSlug,path=[]}=await context.params,a=await requireWorkContext(holdingSlug),{write}=await holdingResources(a.holdingId),[id,action]=path;
    let data:unknown;
    if(!id)data=await createReport(write,a,key,z.object({title:z.string().max(120).optional()}).parse(raw).title);
    else if(action==="save")data=await saveReport(write,a,id,saveSchema.parse(raw),key);
    else if(action==="generate") {
      const env=await runtimeEnv();data=await queueGeneration(write,a,env,id,generateSchema.parse(raw),key);
      after(async()=>{await drainExecutiveGeneration(write,env);});
    }else if(action==="cancel")data=await cancelGeneration(write,a,id,z.object({jobId:z.string()}).parse(raw).jobId,key);
    else if(action==="share")data=await saveShare(write,a,id,shareSchema.parse(raw),key);
    else if(action==="preview")data=await previewShare(write,a,id,sharePreviewSchema.parse(raw));
    else if(action==="transfer")data=await transferReportOwnership(write,a,id,transferOwnershipSchema.parse(raw),key);
    else if(action==="revoke")data=await revokeShare(write,a,id,z.object({shareId:z.string()}).parse(raw).shareId,key);
    else if(action==="restore"){const input=z.object({version:z.number().int().positive(),expectedVersion:z.number().int().positive()}).parse(raw);data=await restoreReport(write,a,id,input.version,input.expectedVersion,key);}
    else throw new ApiError(404,"NOT_FOUND","This Executive action does not exist.");
    return Response.json({data},{headers});
  }catch(e){return errorResponse(e);}
}
