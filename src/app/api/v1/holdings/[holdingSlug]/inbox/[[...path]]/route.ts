import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { errorResponse, ApiError } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { isTransactionalSqlDatabase } from "@/server/runtime/types";
import { inboxPage, inboxSummary, sendNotice, changeInboxState } from "@/server/inbox";
import { messagingPeople } from "@/server/messaging";

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
      const notices=(await write.prepare(`SELECT n.id,n.title,n.recipient_count accepted_count,n.created_at,
        (SELECT count(*)::int FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
         WHERE e.company_id=n.company_id AND e.event_key='notice:'||n.id) delivered_count
        FROM inbox_notices n WHERE n.company_id=? AND n.sender_user_id=? ORDER BY n.created_at DESC,n.id DESC LIMIT 50`)
        .bind(actor.companyId,actor.userId).all()).results;
      return response(notices);
    }
    if(path.length===2&&path[0]==="notices"){
      const notice=await write.prepare("SELECT id,title,body,recipient_count,created_at FROM inbox_notices WHERE id=? AND company_id=? AND sender_user_id=?")
        .bind(path[1],actor.companyId,actor.userId).first<Record<string,unknown>>();
      if(!notice)throw new ApiError(404,"NOTICE_NOT_FOUND","Notice unavailable.");
      const delivery=await write.prepare(`SELECT count(*)::int n FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
        WHERE e.company_id=? AND e.event_key=?`).bind(actor.companyId,`notice:${path[1]}`).first<{n:number}>();
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
