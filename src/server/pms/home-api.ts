import "server-only";
import { z } from "zod";
import type { WorkContext } from "../authorization";
import { holdingResources } from "../runtime";
import { ApiError } from "../http";
import { requirePermission } from "../iam-permissions";
import { pmsWorkspace } from "../pms";
import { pmsHome,pmsHotelSources } from "./home";
import { postAction, postComments, postView, postDraftSchema, publishPost, savePostDraft } from "../social-home";
import { acknowledgeHandover, handoverAcknowledgement, publishHandover } from "../handover";
import { textSuggestions, mentionsSchema } from "../social-text";
import { readablePost } from "../social-source";

async function assertPostScope(actor:WorkContext,id:string) {
  const {write}=await holdingResources(actor.holdingId),post=await readablePost(write,actor,id);
  if(actor.branchId&&post.branch_id&&post.branch_id!==actor.branchId||actor.departmentId&&post.department_id&&post.department_id!==actor.departmentId)
    throw new ApiError(404,"POST_NOT_FOUND","This conversation is unavailable in this workspace.");
}

const homeQuery=z.object({property:z.string().max(160).optional(),view:z.enum(["mine","team","handover","company"]).optional(),
  label:z.enum(["arrivals","departures","housekeeping","inspection","transfers","corrections","refunds","variance","shift","night"]).optional(),
  taskCursor:z.string().max(4096).optional(),before:z.coerce.number().int().positive().optional(),beforeId:z.string().max(160).optional(),post:z.string().max(160).optional()});
export async function pmsHomeRead(actor:WorkContext,path:string[],search:URLSearchParams) {
  requirePermission(actor,"pms.read");
  const {write}=await holdingResources(actor.holdingId);
  if(path.length===2&&path[0]==='hotel-sources')return pmsHotelSources(actor,path[1],z.enum(['occupancy','adr','revpar','room_revenue']).parse(search.get('metric')),z.iso.date().parse(search.get('businessDate')),z.coerce.number().int().min(1).max(100000).default(1).parse(search.get('page')??undefined));
  if(path.length===2&&path[0]==='hotel-record')return pmsWorkspace(actor,z.string().min(1).parse(search.get('property')),false,true,{view:'detail',id:path[1]});
  if(!path.length) {
    const query=homeQuery.parse(Object.fromEntries(search));
    if(query.post)await assertPostScope(actor,query.post);
    let loaded:string[]|undefined;
    if(search.has("loaded")){
      try{loaded=z.array(z.string().max(160)).max(100).parse(JSON.parse(search.get("loaded")!));}
      catch{throw new ApiError(422,"INVALID_FEED_IDS","Refresh the feed to continue.");}
    }
    return pmsHome(actor,{propertyId:query.property,view:query.view,label:query.label,taskCursor:query.taskCursor,before:query.before,beforeId:query.beforeId,postId:query.post,loadedIds:loaded});
  }
  if(path.length>=2&&(path[0]==="posts"||path[0]==="handover"))await assertPostScope(actor,path[1]);
  if(path.length===2&&path[0]==="posts")return postView(write,actor,path[1]);
  if(path.length===3&&path[0]==="posts"&&path[2]==="comments")return postComments(write,actor,path[1],search.get("cursor")??undefined);
  if(path.length===3&&path[0]==="handover"&&path[2]==="acknowledgement")return handoverAcknowledgement(write,actor,path[1]);
  if(path.join("/")==="beta/suggestions")return textSuggestions(write,actor,z.enum(["@","#"]).parse(search.get("trigger")),z.string().max(160).parse(search.get("q")??""));
  throw new ApiError(404,"NOT_FOUND","Not found.");
}

export async function pmsHomeWrite(actor:WorkContext,path:string[],input:unknown,key:string) {
  requirePermission(actor,"pms.read");
  const {write}=await holdingResources(actor.holdingId);
  if(path.length===1&&path[0]==="publish") {
    const draft=postDraftSchema.parse(input);
    if(draft.purpose==="handover")throw new ApiError(422,"HANDOVER_SOURCES_REQUIRED","Use the handover form to choose work and recipients.");
    const saved=await savePostDraft(write,actor,null,draft,key);
    return publishPost(write,actor,saved.id,saved.version,`${key}-publish`);
  }
  if(path.length===1&&path[0]==="handover")return publishHandover(write,actor,{items:[],unavailableSources:[]},input,key);
  if(path.length===3&&path[0]==="handover"&&path[2]==="acknowledgement"){
    await assertPostScope(actor,path[1]);return acknowledgeHandover(write,actor,path[1],key);
  }
  if(path.length===2&&path[0]==="posts") {
    await assertPostScope(actor,path[1]);
    const action=z.object({action:z.enum(["comment","appreciate","follow","edit","retract"]),body:z.string().max(30000).optional(),
      active:z.boolean().optional(),expectedVersion:z.number().int().positive().optional(),mentions:mentionsSchema.optional(),richContent:z.unknown().optional()}).strict().parse(input);
    return postAction(write,actor,path[1],action,key);
  }
  throw new ApiError(404,"NOT_FOUND","Not found.");
}
