import {sentNoticeRows,sentNoticeRow,sentNoticeDeliveryCount} from '@/server/modules/collaboration/public/people-app-entrypoint-facts';
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { errorResponse, ApiError } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { inboxPage, inboxSummary, sendNotice, changeInboxState } from "@/server/modules/collaboration/public/inbox";
import { messagingPeople } from "@/server/modules/collaboration/public/messaging";

export const dynamic = "force-dynamic";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const response=(data:unknown)=>Response.json({data},{headers:{"Cache-Control":"private, no-store"}});

export async function GET(request:Request,context:Context){
  try{
    const {holdingSlug,path=[]}=await context.params;
    const actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
    const q=new URL(request.url).searchParams;
    if(path.length===1&&path[0]==="notifications")return response(await inboxPage(write,actor,{
      view:q.get("view")??undefined,category:q.get("category")??undefined,
      search:q.get("search")??undefined,cursor:q.get("cursor")??undefined,
      limit:q.has("limit")?Number(q.get("limit")):undefined,
    }));
    if(path.length===1&&path[0]==="summary")return response(await inboxSummary(write,actor));
    if(path.length===1&&path[0]==="people")return response(await messagingPeople(write,actor,q.get("q")??""));
    if(path.length===1&&path[0]==="notices"){
      const notices=(await sentNoticeRows(write,actor.companyId,actor.userId)).results;
      return response(notices);
    }
    if(path.length===2&&path[0]==="notices"){
      const notice=await sentNoticeRow(write,path[1],actor.companyId,actor.userId);
      if(!notice)throw new ApiError(404,"NOTICE_NOT_FOUND","Notice unavailable.");
      const delivery=await sentNoticeDeliveryCount(write,actor.companyId,`notice:${path[1]}`);
      return response({...notice,acceptedCount:notice.recipient_count,completedCount:delivery?.n??0});
    }
    throw new ApiError(404,"INBOX_RESOURCE_NOT_FOUND","Inbox resource not found.");
  }catch(error){return errorResponse(error);}
}

export async function POST(request:Request,context:Context){
  try{
    const origin=request.headers.get("origin");
    if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key=request.headers.get("Idempotency-Key");
    if(!key)throw new ApiError(422,"IDEMPOTENCY_KEY_REQUIRED","Idempotency-Key is required.");
    const {holdingSlug,path=[]}=await context.params;
    const actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
    const input=await request.json();
    if(path.length===1&&path[0]==="notices"){
      if(!isTransactionalSqlDatabase(write))throw new Error("TRANSACTION_REQUIRED");
      return response(await sendNotice(write,actor,input,key));
    }
    if(path.length===3&&path[0]==="notifications"&&path[2]==="state"){
      if(!input||typeof input!=="object"||
        !("read" in input||"archived" in input)||
        ("read" in input&&typeof input.read!=="boolean")||
        ("archived" in input&&typeof input.archived!=="boolean"))
        throw new ApiError(422,"INVALID_INBOX_STATE","Choose a read or archive state.");
      return response(await changeInboxState(write,actor,path[1],input));
    }
    throw new ApiError(404,"INBOX_RESOURCE_NOT_FOUND","Inbox resource not found.");
  }catch(error){return errorResponse(error);}
}
