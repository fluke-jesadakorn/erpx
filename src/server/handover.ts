import "server-only";

import { z } from "zod";
import { bangkokToday,handoverKind,type HandoverItem } from "@/lib/handover";
import type { WorkContext } from "./authorization";
import { readDailyPlan } from "./daily-plan";
import { handoverTaskAccessSql } from "./handover-policy";
import { assertion,executeCommand } from "./commands";
import { ApiError } from "./http";
import { publishPost,savePostDraft } from "./social-home";
import { readablePost,sourceGuard } from "./social-source";
import { staffPeople } from "./staff-policy";
import { getTask } from "./staff-tasks";
import { readablePmsSource, pmsSourceRecipients } from "./pms/source";
import { pmsSourcesSql, pmsSourceReadableSql } from "./pms/source-policy";
import type { SqlDatabase } from "./runtime/types";
import type { WorkSnapshot } from "./work-items";

export type HandoverView={date:string;ownerKey:string;items:HandoverItem[];defaultIds:string[];
  people:{id:string;name:string}[];unavailable:boolean;reason:string|null};
export const handoverPublishSchema=z.object({
  date:z.iso.date(),body:z.string().trim().min(1).max(3000),
  sources:z.array(z.object({id:z.string().min(1).max(160),sourceVersion:z.number().int().positive(),
    kind:z.enum(["staff_task","pms_record"]).default("staff_task")})).min(1).max(5),
  recipientIds:z.array(z.string().min(1).max(160)).min(1).max(20),
}).strict().superRefine((input,context)=>{
  if(new Set(input.sources.map(item=>item.id)).size!==input.sources.length)
    context.addIssue({code:"custom",path:["sources"],message:"Choose each task once."});
  if(new Set(input.recipientIds).size!==input.recipientIds.length)
    context.addIssue({code:"custom",path:["recipientIds"],message:"Choose each recipient once."});
});

function peopleInScope(actor:WorkContext,people:Awaited<ReturnType<typeof staffPeople>>) {
  return [...new Map(people.filter(person=>person.user_id!==actor.userId &&
    (!actor.departmentId||person.department_id===actor.departmentId) &&
    (!actor.branchId||person.branch_id===actor.branchId))
    .map(person=>[person.user_id,person] as const)).values()];
}

export async function handoverView(db:SqlDatabase,actor:WorkContext,snapshot:WorkSnapshot,
  now=Date.now()):Promise<HandoverView>{
  const date=bangkokToday(now);
  const tasks=snapshot.items.filter(item=>["staff_task","pms"].includes(item.type)&&item.status!=="cancelled"&&item.status!=="draft");
  const [plan,people,pmsPeople]=await Promise.all([
    readDailyPlan(db,actor,date,snapshot),staffPeople(db,actor.companyId),pmsSourceRecipients(db,actor),
  ]);
  const planned=new Set(plan.items.filter(item=>item.type==="staff_task").map(item=>item.id));
  const ids=tasks.filter(item=>item.type==="staff_task").map(item=>item.id),versions=ids.length?(await db.prepare(`SELECT t.id,t.version FROM staff_tasks t
    WHERE t.company_id=? AND t.id IN (${ids.map(()=>"?").join(",")})
    AND NOT EXISTS(SELECT 1 FROM social_task_origins o WHERE o.task_id=t.id)`)
    .bind(actor.companyId,...ids).all<{id:string;version:number}>()).results:[];
  const byId=new Map(versions.map(item=>[item.id,Number(item.version)]));
  for(const item of tasks.filter(item=>item.type==="pms")){
    try{const source=await readablePmsSource(db,actor,item.id);byId.set(item.id,Number(source.version));}
    catch(cause){if(!(cause instanceof ApiError&&cause.status===404))throw cause;}
  }
  const midnight=Date.parse(`${date}T00:00:00Z`)-7*60*60_000;
  const items: HandoverItem[]=tasks.flatMap(item=>{
    const sourceVersion=byId.get(item.id);if(!sourceVersion)return [];
    const kind=handoverKind(item);
    if(kind==="completed"&&item.updatedAt<midnight&&!planned.has(item.id))return [];
    return [{id:item.id,title:item.title,status:item.status,updatedAt:item.updatedAt,href:item.href,
      nextAction:item.nextAction,blocker:item.blocker,kind,planned:planned.has(item.id),sourceVersion,
      sourceKind:item.type==="pms"?"pms_record" as const:"staff_task" as const}];
  }).sort((a,b)=>Number(b.planned)-Number(a.planned)||
    Number(b.kind==="blocked")-Number(a.kind==="blocked")||b.updatedAt-a.updatedAt).slice(0,30);
  const unavailable=items.length===0;
  const suggested=items.filter(item=>item.planned||item.kind==="blocked"||item.kind==="completed");
  return {date,ownerKey:JSON.stringify([actor.companyId,actor.userId]),items,defaultIds:(suggested.length?suggested:items.slice(0,3))
    .slice(0,5).map(item=>item.id),people:[...new Map([...peopleInScope(actor,people).map(person=>({id:person.user_id,name:person.legal_name})),...pmsPeople].map(person=>[person.id,person])).values()],
    unavailable,reason:unavailable&&snapshot.unavailableSources.includes("Tasks")?"Tasks are temporarily unavailable. Try again when they return."
      :items.length===0?"No readable task work is available for a handover yet.":null};
}

export async function publishHandover(db:SqlDatabase,actor:WorkContext,snapshot:WorkSnapshot,
  raw:unknown,key:string){
  const input=handoverPublishSchema.parse(raw);
  if(input.date!==bangkokToday())throw new ApiError(409,"HANDOVER_DATE_CHANGED","Refresh your handover for today.");
  const existing=await db.prepare(`SELECT body,participants_json,sources_json,published_post_id FROM social_post_drafts
    WHERE id=? AND company_id=? AND author_id=?`).bind(`post-draft-${key}`,actor.companyId,actor.userId)
    .first<{body:string;participants_json:string;sources_json:string;published_post_id:string|null}>();
  const sources=input.sources.map(item=>({kind:item.kind,id:item.id,...(item.kind==="pms_record"?{version:item.sourceVersion}:{})}));
  const recipients=[...input.recipientIds].sort();
  if(existing?.published_post_id){
    await readablePost(db,actor,existing.published_post_id);
    const storedRecipients=(JSON.parse(existing.participants_json) as string[]).sort();
    const storedSources=JSON.parse(existing.sources_json) as typeof sources;
    if(existing.body!==input.body||JSON.stringify(storedRecipients)!==JSON.stringify(recipients)||
      JSON.stringify(storedSources)!==JSON.stringify(sources))
      throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This publish key was already used for another handover.");
    return {id:existing.published_post_id,version:1};
  }
  if(input.sources.some(source=>source.kind==="staff_task")&&snapshot.unavailableSources.includes("Tasks"))throw new ApiError(503,"HANDOVER_SOURCE_UNAVAILABLE","Tasks are temporarily unavailable.");
  const visible=new Set(snapshot.items.filter(item=>item.type==="staff_task").map(item=>item.id));
  const pmsOnly=input.sources.every(source=>source.kind==="pms_record");
  const eligible=new Set(pmsOnly?(await pmsSourceRecipients(db,actor)).map(person=>person.id)
    :[...peopleInScope(actor,await staffPeople(db,actor.companyId)).map(person=>person.user_id),...(await pmsSourceRecipients(db,actor)).map(person=>person.id)]);
  if(recipients.some(id=>!eligible.has(id)))throw new ApiError(403,"HANDOVER_RECIPIENT_UNAVAILABLE","Choose current colleagues within your work scope.");
  for(const source of input.sources){
    if(source.kind==="pms_record") {
      const item=await readablePmsSource(db,actor,source.id);
      if(Number(item.version)!==source.sourceVersion)throw new ApiError(409,"HANDOVER_SOURCE_CHANGED","Selected hotel work changed. Refresh before publishing.");
      for(const recipientId of recipients) {
        const accessible=await db.prepare(`SELECT 1 FROM (${pmsSourcesSql}) pr CROSS JOIN (SELECT ?::text user_id) recipient
          WHERE pr.id=? AND pr.company_id=? AND ${pmsSourceReadableSql("pr","recipient.user_id")}`)
          .bind(recipientId,source.id,actor.companyId).first();
        if(!accessible)throw new ApiError(403,"HANDOVER_RECIPIENT_ACCESS","A colleague cannot open every linked hotel item. Choose another recipient or fewer items.");
      }
      continue;
    }
    if(!visible.has(source.id))throw new ApiError(403,"HANDOVER_SOURCE_UNAVAILABLE","A selected task is no longer available to you.");
    const task=await getTask(db,actor,source.id);
    if(Number(task.version)!==source.sourceVersion)throw new ApiError(409,"HANDOVER_SOURCE_CHANGED","A selected task changed. Refresh the handover before publishing.");
    if(task.source_post_id)throw new ApiError(403,"HANDOVER_SOURCE_PRIVATE","This task comes from a conversation and cannot be included in a handover.");
    for(const recipientId of recipients){
      const accessible=await db.prepare(`SELECT 1 FROM staff_tasks ht CROSS JOIN (SELECT ?::text user_id) recipient
        WHERE ht.id=? AND ht.company_id=? AND ${handoverTaskAccessSql("ht","recipient.user_id")}`)
        .bind(recipientId,source.id,actor.companyId).first();
      if(!accessible)throw new ApiError(403,"HANDOVER_RECIPIENT_ACCESS","A selected colleague cannot open every linked task. Choose another recipient or fewer tasks.");
    }
  }
  const draft=await savePostDraft(db,actor,null,{body:input.body,content:[{type:"text",text:input.body}],
    mentions:[],audience:{kind:"participants",userIds:recipients},sources,purpose:"handover",expectedVersion:1},key);
  return publishPost(db,actor,draft.id,draft.version,`${key}-publish`);
}

export type HandoverAcknowledgement={postId:string;postVersion:number;canAcknowledge:boolean;
  acknowledged:boolean;recipientCount:number;acknowledgedCount:number};
export async function handoverAcknowledgement(db:SqlDatabase,actor:WorkContext,postId:string):Promise<HandoverAcknowledgement>{
  const post=await readablePost(db,actor,postId);
  if(post.purpose!=="handover"||post.audience_kind!=="participants")
    throw new ApiError(404,"HANDOVER_NOT_FOUND","This handover is unavailable.");
  const recipients=(await db.prepare(`SELECT user_id FROM social_post_participants WHERE post_id=? AND user_id<>?`)
    .bind(postId,post.author_id).all<{user_id:string}>()).results;
  const ids=recipients.map(person=>person.user_id);
  const acknowledgements=ids.length?(await db.prepare(`SELECT user_id FROM social_handover_acknowledgements
    WHERE post_id=? AND post_version=? AND user_id IN (${ids.map(()=>"?").join(",")})`)
    .bind(postId,post.version,...ids).all<{user_id:string}>()).results:[];
  const canAcknowledge=ids.includes(actor.userId);
  return {postId,postVersion:post.version,canAcknowledge,acknowledged:canAcknowledge&&acknowledgements.some(item=>item.user_id===actor.userId),
    recipientCount:ids.length,acknowledgedCount:acknowledgements.length};
}

export async function acknowledgeHandover(db:SqlDatabase,actor:WorkContext,postId:string,key:string){
  const post=await readablePost(db,actor,postId);
  const state=await handoverAcknowledgement(db,actor,postId);
  if(!state.canAcknowledge)throw new ApiError(403,"HANDOVER_RECIPIENT_REQUIRED","Only an intended recipient can confirm this handover.");
  if(state.acknowledged)return state;
  const now=Date.now();
  await executeCommand(db,actor,key,"social.handover.acknowledge",{postId,postVersion:post.version},
    postId,post.version,{postId,postVersion:post.version,acknowledged:true},[
      sourceGuard(db,key,post,actor.userId,"handover-readable"),
      assertion(db,key,"handover-recipient",`EXISTS(SELECT 1 FROM social_post_participants WHERE post_id=? AND user_id=?)
        AND NOT EXISTS(SELECT 1 FROM social_handover_acknowledgements WHERE post_id=? AND user_id=? AND post_version=?)`,
        [postId,actor.userId,postId,actor.userId,post.version]),
      db.prepare(`INSERT INTO social_handover_acknowledgements(post_id,user_id,post_version,acknowledged_at,command_id)
        VALUES(?,?,?,?,?)`).bind(postId,actor.userId,post.version,now,key),
    ],{companyId:actor.companyId,branchId:post.branch_id,departmentId:post.department_id});
  return handoverAcknowledgement(db,actor,postId);
}
